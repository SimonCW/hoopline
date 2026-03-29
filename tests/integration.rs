use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use hoopline::{app_with_pool, db};
use http_body_util::BodyExt;
use tower::ServiceExt;

async fn response_body_string(response: axum::response::Response) -> String {
    let body = response.into_body();
    let bytes = body.collect().await.unwrap().to_bytes();
    String::from_utf8(bytes.to_vec()).unwrap()
}

#[tokio::test]
async fn get_root_returns_ok_and_body() {
    let pool = db::init_pool("sqlite::memory:").await.unwrap();
    let app = app_with_pool(pool);

    let response = app
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_body_string(response).await;
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
    let body = response_body_string(response).await;
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

#[tokio::test]
async fn get_users_returns_selector_fragment() {
    let pool = db::init_pool("sqlite::memory:").await.unwrap();
    let app = app_with_pool(pool);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/users")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_body_string(response).await;
    assert!(body.contains("id=\"user-selector\""));
    assert!(body.contains("Alex"));
    assert!(body.contains("Jamali"));
}

#[tokio::test]
async fn post_users_select_sets_cookie_and_persists_identity() {
    let pool = db::init_pool("sqlite::memory:").await.unwrap();
    let app = app_with_pool(pool.clone());

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/users/select")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from("selected_user_id=2"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let set_cookie = response
        .headers()
        .get(header::SET_COOKIE)
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    assert!(set_cookie.contains("user_id=2"));
    let body = response_body_string(response).await;
    assert!(body.contains("Current user"));
    assert!(body.contains("Ben"));

    let cookie_pair = set_cookie.split(';').next().unwrap().to_string();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/")
                .header(header::COOKIE, cookie_pair)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_body_string(response).await;
    assert!(body.contains("Current user: Ben"));
}

#[tokio::test]
async fn post_users_select_creates_user_when_missing() {
    let pool = db::init_pool("sqlite::memory:").await.unwrap();
    let app = app_with_pool(pool);

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/users/select")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from("new_user_name=Taylor"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let set_cookie = response
        .headers()
        .get(header::SET_COOKIE)
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    let cookie_pair = set_cookie.split(';').next().unwrap().to_string();
    let body = response_body_string(response).await;
    assert!(body.contains("Taylor"));

    let response = app
        .oneshot(
            Request::builder()
                .uri("/users")
                .header(header::COOKIE, cookie_pair)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = response_body_string(response).await;
    assert!(body.contains("Taylor"));
}
