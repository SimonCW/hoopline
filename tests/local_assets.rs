use axum::{body::Body, http::Request};
use hoopline::{app_with_pool, db};
use http_body_util::BodyExt;
use tower::ServiceExt;

#[tokio::test]
async fn page_and_script_use_pinned_local_htmx() {
    let pool = db::init_pool("sqlite::memory:").await.unwrap();
    let app = app_with_pool(pool);
    let page = app
        .clone()
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();
    let body = page.into_body().collect().await.unwrap().to_bytes();
    let html = std::str::from_utf8(&body).unwrap();
    assert!(html.contains("src=\"/assets/htmx.min.js\""));
    assert!(!html.contains("cdn.jsdelivr.net"));
    let script = app
        .oneshot(
            Request::builder()
                .uri("/assets/htmx.min.js")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(script.status(), 200);
    assert_eq!(
        script.headers()["content-type"],
        "text/javascript; charset=utf-8"
    );
    let bytes = script.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(bytes.as_ref(), include_bytes!("../assets/htmx.min.js"));
}
