use axum::body::Body;
use axum::http::{Request, StatusCode};
use hoopline::{app_with_pool, db};
use http_body_util::BodyExt;
use tower::ServiceExt;

#[tokio::test]
async fn get_root_returns_ok_and_body() {
    let pool = db::init_pool("sqlite::memory:").await.unwrap();
    let app = app_with_pool(pool);

    let response = app
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body();
    let bytes = body.collect().await.unwrap().to_bytes();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("Hoopline"));
    assert!(body.contains("Court A"));
    assert!(body.contains("Alex"));
    assert_eq!(body.matches("data-testid=\"slot-card\"").count(), 3);
    assert_eq!(body.matches("data-testid=\"player-row\"").count(), 45);
    assert_eq!(body.matches("data-testid=\"waitlist-row\"").count(), 15);
}

#[tokio::test]
async fn get_slots_returns_seeded_data() {
    let pool = db::init_pool("sqlite::memory:").await.unwrap();
    let app = app_with_pool(pool);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/slots")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body();
    let bytes = body.collect().await.unwrap().to_bytes();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("Court B"));
    assert!(body.contains("Jamal"));
}

#[tokio::test]
async fn get_healthz_returns_ok() {
    let pool = db::init_pool("sqlite::memory:").await.unwrap();
    let app = app_with_pool(pool);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/healthz")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body();
    let bytes = body.collect().await.unwrap().to_bytes();
    assert_eq!(bytes.as_ref(), b"ok");
}
