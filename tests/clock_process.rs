//! Verifies clock configuration without changing the parent process environment.

use std::{process::Stdio, time::Duration};

use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::Command,
    time::timeout,
};

#[tokio::test]
async fn clock_override_is_available_only_in_scenario_builds() {
    let directory = tempfile::tempdir().unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_hoopline"))
        .env(
            "DATABASE_URL",
            format!(
                "sqlite://{}",
                directory.path().join("clock.sqlite").display()
            ),
        )
        .env("PORT", "0")
        .env("PREVIEW_SAMPLE", "0")
        .env("HOOPLINE_TEST_NOW", "invalid-on-purpose")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    if cfg!(feature = "scenario-tests") {
        let output = timeout(Duration::from_secs(15), child.wait_with_output())
            .await
            .expect("invalid clock must stop startup")
            .unwrap();
        assert!(!output.status.success());
        assert!(
            String::from_utf8_lossy(&output.stderr)
                .contains("HOOPLINE_TEST_NOW must be an RFC3339 timestamp")
        );
    } else {
        let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();
        let address = timeout(Duration::from_secs(15), async {
            while let Some(line) = lines.next_line().await.unwrap() {
                if let Some(address) = line.strip_prefix("HOOPLINE_LISTENING=") {
                    return address.replace("0.0.0.0", "127.0.0.1");
                }
            }
            panic!("default build must ignore the test clock");
        })
        .await
        .expect("startup timed out");
        let response = reqwest::Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(5))
            .build()
            .unwrap()
            .get(format!("http://{address}/healthz"))
            .send()
            .await;
        child.kill().await.unwrap();
        timeout(Duration::from_secs(5), child.wait())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(response.unwrap().status(), 200);
    }
}
