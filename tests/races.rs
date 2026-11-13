mod support;

use std::{sync::Arc, time::Duration};

use reqwest::{RequestBuilder, StatusCode, header};
use support::Server;
use tokio::{sync::Barrier, time::timeout};

const REPETITIONS: usize = 10;

async fn race(left: RequestBuilder, right: RequestBuilder) -> (StatusCode, StatusCode) {
    let gate = Arc::new(Barrier::new(3));
    let send = |request: RequestBuilder, gate: Arc<Barrier>| {
        tokio::spawn(async move {
            gate.wait().await;
            let response = request.send().await.unwrap();
            let status = response.status();
            // Drain the response to include rendering and the HTTP body in the deadline.
            response.bytes().await.unwrap();
            status
        })
    };
    let left = send(left, gate.clone());
    let right = send(right, gate.clone());
    timeout(Duration::from_secs(25), async {
        gate.wait().await;
        (left.await.unwrap(), right.await.unwrap())
    })
    .await
    .expect("concurrent requests timed out")
}

#[tokio::test]
async fn concurrent_signups_take_only_the_last_waitlist_position() {
    for _ in 0..REPETITIONS {
        let server = Server::start().await;
        server
            .seed_roster(&[(1, 1, false), (2, 2, false), (3, 1, true), (4, 2, true)])
            .await;
        let (left, right) = race(
            server.post("/slots/1/signup", 5),
            server.post("/slots/1/signup", 6),
        )
        .await;
        assert!(
            (left == StatusCode::OK && right == StatusCode::BAD_REQUEST)
                || (right == StatusCode::OK && left == StatusCode::BAD_REQUEST),
            "expected one winner and one full response, got {left}, {right}"
        );
        let winner = if left == StatusCode::OK { 5 } else { 6 };
        assert_eq!(
            server.roster().await,
            vec![
                (1, 1, false),
                (2, 2, false),
                (3, 1, true),
                (4, 2, true),
                (winner, 3, true),
            ]
        );
        assert_eq!(server.revision().await, 1);
        server.assert_capacities().await;
        server.shutdown().await;
    }
}

#[tokio::test]
async fn cancellation_racing_signup_promotes_the_first_existing_waiter() {
    for _ in 0..REPETITIONS {
        let server = Server::start().await;
        server
            .seed_roster(&[(1, 1, false), (2, 2, false), (3, 1, true), (4, 2, true)])
            .await;
        assert_eq!(
            race(
                server.post("/slots/1/cancel", 1),
                server.post("/slots/1/signup", 5),
            )
            .await,
            (StatusCode::OK, StatusCode::OK)
        );
        assert_eq!(
            server.roster().await,
            vec![(2, 1, false), (3, 2, false), (4, 1, true), (5, 2, true)]
        );
        assert_eq!(server.revision().await, 2);
        server.assert_capacities().await;
        server.shutdown().await;
    }
}

#[tokio::test]
async fn retry_after_a_committed_response_is_lost_does_not_duplicate_booking() {
    let server = Server::start().await;
    server
        .seed_roster(&[(1, 1, false), (2, 2, false), (3, 1, true)])
        .await;
    server.signup_with_lost_response(5).await;
    let committed = server.roster().await;
    assert_eq!(
        committed,
        vec![(1, 1, false), (2, 2, false), (3, 1, true), (5, 2, true)]
    );
    for _ in 0..3 {
        let retry = server.post("/slots/1/signup", 5).send().await.unwrap();
        assert_eq!(retry.status(), StatusCode::BAD_REQUEST);
        assert!(retry.text().await.unwrap().contains("already signed up"));
        assert_eq!(server.roster().await, committed);
        assert_eq!(server.revision().await, 1);
        server.assert_capacities().await;
    }
    server.shutdown().await;
}

#[tokio::test]
async fn stale_admin_edits_cannot_clear_or_reorder_the_final_roster() {
    for _ in 0..REPETITIONS {
        let server = Server::start().await;
        server
            .seed_roster(&[(1, 1, false), (2, 2, false), (3, 1, true), (4, 2, true)])
            .await;
        let cookie = server.admin_cookie().await;
        let page = server
            .client
            .get(format!("{}/admin/slots/1/edit", server.address))
            .header(header::COOKIE, &cookie)
            .send()
            .await
            .unwrap();
        assert_eq!(page.status(), StatusCode::OK);
        assert!(
            page.text()
                .await
                .unwrap()
                .contains("name=\"revision\" value=\"0\"")
        );
        assert_eq!(
            race(
                server.post("/slots/1/cancel", 2),
                server.post("/slots/1/signup", 5),
            )
            .await,
            (StatusCode::OK, StatusCode::OK)
        );
        let final_roster = vec![(1, 1, false), (3, 2, false), (4, 1, true), (5, 2, true)];
        assert_eq!(server.roster().await, final_roster);
        server.assert_capacities().await;
        let before_stale_edits = server.slot_snapshot().await;
        let edit = server
            .client
            .post(format!("{}/admin/slots/1/edit", server.address))
            .header(header::COOKIE, &cookie)
            .header("x-hoopline-user", "1")
            .form(&[
                ("date", "2099-01-06"),
                ("time", "20:00"),
                ("venue", "Stale court"),
                ("map_url", "https://example.com/stale-map"),
                ("revision", "0"),
                ("bookings", "clear"),
            ]);
        let remove = server
            .client
            .post(format!("{}/admin/slots/1/remove/3", server.address))
            .header(header::COOKIE, &cookie)
            .header("x-hoopline-user", "1")
            .form(&[("revision", "0")]);
        assert_eq!(
            race(edit, remove).await,
            (StatusCode::CONFLICT, StatusCode::CONFLICT)
        );
        assert_eq!(server.roster().await, final_roster);
        assert_eq!(server.revision().await, 2);
        server.assert_capacities().await;
        assert_eq!(server.slot_snapshot().await, before_stale_edits);
        server.shutdown().await;
    }
}
