use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::{BodyExt, Empty};
use htmx::app;
use tower::ServiceExt;

#[tokio::test]
async fn get_root_returns_ok_and_body() {
    let app = app();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let mut body = response.into_body();
    let bytes = body.collect().await.unwrap().to_bytes();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("Basketball Booking"));
}
