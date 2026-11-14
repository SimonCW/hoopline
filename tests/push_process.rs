//! Check Hostim push configuration in an isolated application process.

use std::{process::Stdio, time::Duration};

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::Command,
    time::timeout,
};

#[tokio::test]
async fn hostim_builtin_domain_starts_push_without_app_origin() {
    let private = "IQ9Ur0ykXoHS9gzfYX0aBjy9lvdrjx_PFUXmie9YRcY";
    let public = URL_SAFE_NO_PAD.encode(
        web_push::VapidSignatureBuilder::from_base64_no_sub(private)
            .unwrap()
            .get_public_key(),
    );
    let directory = tempfile::tempdir().unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_hoopline"))
        .env(
            "DATABASE_URL",
            format!(
                "sqlite://{}",
                directory.path().join("hostim.sqlite").display()
            ),
        )
        .env("PORT", "0")
        .env("PREVIEW_SAMPLE", "0")
        .env_remove("HOOPLINE_TEST_NOW")
        .env("PUSH_ENABLED", "true")
        .env_remove("APP_ORIGIN")
        .env("BUILTIN_DOMAIN", "hoopline-example.hostim.app")
        .env("VAPID_PUBLIC_KEY", public)
        .env("VAPID_PRIVATE_KEY", private)
        .env("VAPID_SUBJECT", "mailto:operator@example.com")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();
    let address = timeout(Duration::from_secs(15), async {
        while let Some(line) = lines.next_line().await.unwrap() {
            if let Some(address) = line.strip_prefix("HOOPLINE_LISTENING=") {
                return address.replace("0.0.0.0", "127.0.0.1");
            }
        }
        panic!("Hostim configuration did not start the application");
    })
    .await
    .expect("startup timed out");
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap();
    let response = client
        .get(format!("http://{address}/push/status"))
        .send()
        .await
        .unwrap();
    let body = response.text().await.unwrap();
    let cookie = format!("push_device={}", URL_SAFE_NO_PAD.encode([42_u8; 32]));
    let cross_origin = client
        .post(format!("http://{address}/push/disable"))
        .header("Origin", "https://other.example")
        .header("Cookie", &cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(cross_origin.status(), 403);
    let same_origin = client
        .post(format!("http://{address}/push/disable"))
        .header("Origin", "https://hoopline-example.hostim.app")
        .header("Cookie", &cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(same_origin.status(), 200);
    child.kill().await.unwrap();
    timeout(Duration::from_secs(5), child.wait())
        .await
        .unwrap()
        .unwrap();
    let status: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(status["available"], true);
}
