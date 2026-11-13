use std::{net::SocketAddr, process::Stdio, time::Duration};

use hoopline::{app_with_pool, db};
use reqwest::{Client, RequestBuilder, StatusCode, header};
use sqlx::SqlitePool;
use tempfile::TempDir;
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    net::{TcpListener, TcpStream},
    process::{Child, Command},
    time::timeout,
};

const DEADLINE: Duration = Duration::from_secs(20);

pub struct Server {
    // Keep kill-on-drop as a fallback if an assertion panics before shutdown.
    process: Child,
    pub pool: SqlitePool,
    pub client: Client,
    pub address: String,
    _directory: TempDir,
}

#[derive(Debug, PartialEq, Eq, sqlx::FromRow)]
pub struct SlotSnapshot {
    id: i64,
    datetime: String,
    venue: String,
    max_players: i64,
    max_waitlist: i64,
    revision: i64,
    schedule_id: Option<i64>,
    occurrence_date: Option<String>,
    is_override: bool,
    map_url: String,
}

impl Server {
    pub async fn start() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let database_url = format!("sqlite://{}", directory.path().join("races.db").display());
        let pool = db::init_pool(&database_url).await.unwrap();
        sqlx::raw_sql(
            "INSERT INTO users (id, name) VALUES
             (1, 'Admin'), (2, 'Player'), (3, 'First waiter'),
             (4, 'Second waiter'), (5, 'New member'), (6, 'Other member');
             INSERT INTO slots (id, datetime, venue, max_players, max_waitlist)
             VALUES (1, '2099-01-05T19:00:00Z', 'Race court', 2, 3);",
        )
        .execute(&pool)
        .await
        .unwrap();
        let mut process = Command::new(std::env::current_exe().unwrap())
            .args(["--ignored", "--exact", "support::serve", "--nocapture"])
            .env("HOOPLINE_RACE_DATABASE", database_url)
            .env("ADMIN_PASSWORD", "race-test-password")
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        let mut output = BufReader::new(process.stdout.take().unwrap()).lines();
        let address = timeout(DEADLINE, async {
            loop {
                let line = output.next_line().await.unwrap().expect("server exited");
                if let Some(address) = line.strip_prefix("RACE_SERVER=") {
                    break address.to_owned();
                }
            }
        })
        .await
        .expect("server startup timed out");
        Self {
            pool,
            client: Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .timeout(DEADLINE)
                .no_proxy()
                .build()
                .unwrap(),
            address,
            process,
            _directory: directory,
        }
    }

    pub async fn shutdown(mut self) {
        self.process.start_kill().unwrap();
        timeout(DEADLINE, self.process.wait())
            .await
            .expect("server exit timed out")
            .expect("server wait failed");
        timeout(DEADLINE, self.pool.close())
            .await
            .expect("pool close timed out");
        // The temporary directory is removed only after the child is reaped
        // and the parent's database connections are closed.
    }

    pub async fn assert_capacities(&self) {
        let capacities: (i64, i64) =
            sqlx::query_as("SELECT max_players, max_waitlist FROM slots WHERE id = 1")
                .fetch_one(&self.pool)
                .await
                .unwrap();
        assert_eq!(capacities, (2, 3));
    }

    pub async fn slot_snapshot(&self) -> SlotSnapshot {
        sqlx::query_as("SELECT * FROM slots WHERE id = 1")
            .fetch_one(&self.pool)
            .await
            .unwrap()
    }

    pub async fn signup_with_lost_response(&self, user: i64) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy_address = listener.local_addr().unwrap();
        let upstream_address = self.address.strip_prefix("http://").unwrap();
        let proxy = timeout(DEADLINE, async {
            let (mut downstream, _) = listener.accept().await.unwrap();
            // This one-shot proxy accepts only an empty-body signup request.
            let mut request = Vec::new();
            while !request.ends_with(b"\r\n\r\n") {
                assert!(request.len() < 16_384, "request headers exceeded limit");
                request.push(downstream.read_u8().await.unwrap());
            }
            let headers = String::from_utf8(request.clone()).unwrap();
            assert!(headers.starts_with("POST /slots/1/signup HTTP/1.1\r\n"));
            let headers = headers.to_ascii_lowercase();
            assert!(headers.contains("\r\nconnection: close\r\n"));
            assert!(!headers.contains("transfer-encoding:"));
            assert!(headers.lines().all(|line| {
                !line.starts_with("content-length:") || line == "content-length: 0"
            }));
            let mut upstream = TcpStream::connect(upstream_address).await.unwrap();
            upstream.write_all(&request).await.unwrap();
            let mut response = Vec::new();
            // Connection: close makes EOF the response boundary. The size limit
            // prevents an unexpected response from allocating without a bound.
            upstream
                .take(1_048_576)
                .read_to_end(&mut response)
                .await
                .unwrap();
            assert!(response.len() < 1_048_576, "response exceeded limit");
            let response = String::from_utf8(response).unwrap();
            assert!(response.starts_with("HTTP/1.1 200 OK\r\n"));
            assert!(response.contains("Booking saved."));
            // The full upstream response proves commit and rendering finished.
            // Deliver no headers or body to the caller.
            downstream.shutdown().await.unwrap();
        });
        let request = self
            .client
            .post(format!("http://{proxy_address}/slots/1/signup"))
            .header(header::COOKIE, format!("user_id={user}"))
            .header("x-hoopline-user", user.to_string())
            .header(header::CONNECTION, "close")
            .body("")
            .send();
        let (response, forwarded) = tokio::join!(request, proxy);
        forwarded.expect("response-dropping proxy timed out");
        let error = response.expect_err("proxy must not deliver a response");
        assert!(error.is_request(), "expected transport failure: {error}");
        assert!(
            !error.is_timeout(),
            "proxy must close before client timeout"
        );
        assert!(!error.is_connect(), "client must reach the proxy");
    }

    pub fn post(&self, path: &str, user: i64) -> RequestBuilder {
        self.client
            .post(format!("{}{path}", self.address))
            .header(header::COOKIE, format!("user_id={user}"))
            .header("x-hoopline-user", user.to_string())
    }

    pub async fn admin_cookie(&self) -> String {
        let response = self
            .post("/admin/login", 1)
            .form(&[("user_id", "1"), ("password", "race-test-password")])
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::SEE_OTHER);
        let session = response.headers()[header::SET_COOKIE]
            .to_str()
            .unwrap()
            .split(';')
            .next()
            .unwrap();
        assert!(session.starts_with("admin_session="));
        format!("user_id=1; {session}")
    }

    pub async fn seed_roster(&self, entries: &[(i64, i64, bool)]) {
        for &(user, position, waitlist) in entries {
            sqlx::query(
                "INSERT INTO bookings (slot_id, user_id, position, is_waitlist)
                 VALUES (1, ?, ?, ?)",
            )
            .bind(user)
            .bind(position)
            .bind(waitlist)
            .execute(&self.pool)
            .await
            .unwrap();
        }
    }

    pub async fn roster(&self) -> Vec<(i64, i64, bool)> {
        sqlx::query_as(
            "SELECT user_id, position, is_waitlist FROM bookings
             WHERE slot_id = 1 ORDER BY is_waitlist, position",
        )
        .fetch_all(&self.pool)
        .await
        .unwrap()
    }

    pub async fn revision(&self) -> i64 {
        sqlx::query_scalar("SELECT revision FROM slots WHERE id = 1")
            .fetch_one(&self.pool)
            .await
            .unwrap()
    }
}

// Each child has isolated configuration, including the real admin password.
#[tokio::test]
#[ignore = "started by the TCP scenario fixture"]
async fn serve() {
    let database_url = std::env::var("HOOPLINE_RACE_DATABASE").unwrap();
    let pool = db::init_pool(&database_url).await.unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    println!("RACE_SERVER=http://{}", listener.local_addr().unwrap());
    axum::serve(
        listener,
        app_with_pool(pool).into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
    .unwrap();
}
