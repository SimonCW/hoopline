use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use hoopline::{app_with_pool, db};
use http_body_util::BodyExt;
use sqlx::Row;
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
    assert!(body.contains("id=\"slots-content\""));
    assert!(body.contains("hx-trigger=\"user-changed from:body\""));
}

#[tokio::test]
async fn get_slots_fragment_reflects_selected_user_from_cookie() {
    let pool = db::init_pool("sqlite::memory:").await.unwrap();
    let app = app_with_pool(pool);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/slots/fragment")
                .header(header::COOKIE, "user_id=1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_body_string(response).await;
    assert!(body.contains("Current user: Alex"));
    assert!(body.contains("Cancel"));
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
    assert!(!body.contains("Admin mode"));
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
    let hx_trigger = response
        .headers()
        .get("hx-trigger")
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    assert!(set_cookie.contains("user_id=2"));
    assert_eq!(hx_trigger, "user-changed");
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
async fn get_users_shows_admin_badge_for_admin_cookie() {
    let pool = db::init_pool("sqlite::memory:").await.unwrap();
    let app = app_with_pool(pool);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/users")
                .header(header::COOKIE, "user_id=1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_body_string(response).await;
    assert!(body.contains("Admin mode"));
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

#[tokio::test]
async fn post_slots_signup_adds_player_and_highlights_current_user() {
    let pool = db::init_pool("sqlite::memory:").await.unwrap();
    let app = app_with_pool(pool.clone());

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/slots/3/signup")
                .header(header::COOKIE, "user_id=6")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_body_string(response).await;
    assert!(body.contains("id=\"slot-3\""));
    assert!(body.contains("Farid"));
    assert!(body.contains("You"));

    let booking =
        sqlx::query("SELECT is_waitlist, position FROM bookings WHERE slot_id = ? AND user_id = ?")
            .bind(3_i64)
            .bind(6_i64)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(booking.get::<i64, _>("is_waitlist"), 0);
    assert_eq!(booking.get::<i64, _>("position"), 6);
}

#[tokio::test]
async fn post_slots_signup_routes_to_waitlist_when_players_are_full() {
    let pool = db::init_pool("sqlite::memory:").await.unwrap();
    sqlx::query("UPDATE slots SET max_players = ? WHERE id = ?")
        .bind(8_i64)
        .bind(2_i64)
        .execute(&pool)
        .await
        .unwrap();
    let app = app_with_pool(pool.clone());

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/slots/2/signup")
                .header(header::COOKIE, "user_id=8")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let booking =
        sqlx::query("SELECT is_waitlist, position FROM bookings WHERE slot_id = ? AND user_id = ?")
            .bind(2_i64)
            .bind(8_i64)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(booking.get::<i64, _>("is_waitlist"), 1);
    assert_eq!(booking.get::<i64, _>("position"), 2);
}

#[tokio::test]
async fn post_slots_signup_rejects_when_slot_and_waitlist_are_full() {
    let pool = db::init_pool("sqlite::memory:").await.unwrap();
    sqlx::query("UPDATE slots SET max_players = ?, max_waitlist = ? WHERE id = ?")
        .bind(8_i64)
        .bind(1_i64)
        .bind(2_i64)
        .execute(&pool)
        .await
        .unwrap();
    let app = app_with_pool(pool.clone());

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/slots/2/signup")
                .header(header::COOKIE, "user_id=8")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let booking_count: i64 =
        sqlx::query("SELECT COUNT(*) as count FROM bookings WHERE slot_id = ? AND user_id = ?")
            .bind(2_i64)
            .bind(8_i64)
            .fetch_one(&pool)
            .await
            .unwrap()
            .get("count");
    assert_eq!(booking_count, 0);
}

#[tokio::test]
async fn post_slots_signup_rejects_duplicate_user() {
    let pool = db::init_pool("sqlite::memory:").await.unwrap();
    let app = app_with_pool(pool);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/slots/1/signup")
                .header(header::COOKIE, "user_id=1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn post_slots_cancel_promotes_waitlist_after_player_cancel() {
    let pool = db::init_pool("sqlite::memory:").await.unwrap();
    let app = app_with_pool(pool.clone());

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/slots/1/cancel")
                .header(header::COOKIE, "user_id=2")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let cancelled_count: i64 =
        sqlx::query("SELECT COUNT(*) as count FROM bookings WHERE slot_id = ? AND user_id = ?")
            .bind(1_i64)
            .bind(2_i64)
            .fetch_one(&pool)
            .await
            .unwrap()
            .get("count");
    assert_eq!(cancelled_count, 0);

    let promoted_booking =
        sqlx::query("SELECT is_waitlist, position FROM bookings WHERE slot_id = ? AND user_id = ?")
            .bind(1_i64)
            .bind(7_i64)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(promoted_booking.get::<i64, _>("is_waitlist"), 0);
    assert_eq!(promoted_booking.get::<i64, _>("position"), 6);

    let shifted_waitlist =
        sqlx::query("SELECT is_waitlist, position FROM bookings WHERE slot_id = ? AND user_id = ?")
            .bind(1_i64)
            .bind(8_i64)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(shifted_waitlist.get::<i64, _>("is_waitlist"), 1);
    assert_eq!(shifted_waitlist.get::<i64, _>("position"), 1);
}

#[tokio::test]
async fn post_slots_cancel_shifts_waitlist_after_waitlist_cancel() {
    let pool = db::init_pool("sqlite::memory:").await.unwrap();
    let app = app_with_pool(pool.clone());

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/slots/3/cancel")
                .header(header::COOKIE, "user_id=4")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let cancelled_count: i64 =
        sqlx::query("SELECT COUNT(*) as count FROM bookings WHERE slot_id = ? AND user_id = ?")
            .bind(3_i64)
            .bind(4_i64)
            .fetch_one(&pool)
            .await
            .unwrap()
            .get("count");
    assert_eq!(cancelled_count, 0);

    let shifted_waitlist =
        sqlx::query("SELECT is_waitlist, position FROM bookings WHERE slot_id = ? AND user_id = ?")
            .bind(3_i64)
            .bind(5_i64)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(shifted_waitlist.get::<i64, _>("is_waitlist"), 1);
    assert_eq!(shifted_waitlist.get::<i64, _>("position"), 2);
}

#[tokio::test]
async fn post_admin_remove_rejects_non_admin() {
    let pool = db::init_pool("sqlite::memory:").await.unwrap();
    let app = app_with_pool(pool);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/admin/slots/1/remove/2")
                .header(header::COOKIE, "user_id=2")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn post_admin_remove_triggers_waitlist_promotion() {
    let pool = db::init_pool("sqlite::memory:").await.unwrap();
    let app = app_with_pool(pool.clone());

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/admin/slots/1/remove/2")
                .header(header::COOKIE, "user_id=1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_body_string(response).await;
    assert!(body.contains("Remove"));

    let removed_count: i64 =
        sqlx::query("SELECT COUNT(*) as count FROM bookings WHERE slot_id = ? AND user_id = ?")
            .bind(1_i64)
            .bind(2_i64)
            .fetch_one(&pool)
            .await
            .unwrap()
            .get("count");
    assert_eq!(removed_count, 0);

    let promoted_booking =
        sqlx::query("SELECT is_waitlist, position FROM bookings WHERE slot_id = ? AND user_id = ?")
            .bind(1_i64)
            .bind(7_i64)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(promoted_booking.get::<i64, _>("is_waitlist"), 0);
    assert_eq!(promoted_booking.get::<i64, _>("position"), 6);
}

#[tokio::test]
async fn post_admin_promote_moves_target_from_waitlist_to_players() {
    let pool = db::init_pool("sqlite::memory:").await.unwrap();
    let app = app_with_pool(pool.clone());

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/admin/slots/3/promote/4")
                .header(header::COOKIE, "user_id=1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let promoted =
        sqlx::query("SELECT is_waitlist, position FROM bookings WHERE slot_id = ? AND user_id = ?")
            .bind(3_i64)
            .bind(4_i64)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(promoted.get::<i64, _>("is_waitlist"), 0);
    assert_eq!(promoted.get::<i64, _>("position"), 6);

    let shifted =
        sqlx::query("SELECT is_waitlist, position FROM bookings WHERE slot_id = ? AND user_id = ?")
            .bind(3_i64)
            .bind(5_i64)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(shifted.get::<i64, _>("is_waitlist"), 1);
    assert_eq!(shifted.get::<i64, _>("position"), 2);
}

#[tokio::test]
async fn post_admin_generate_slots_rejects_non_admin() {
    let pool = db::init_pool("sqlite::memory:").await.unwrap();
    let app = app_with_pool(pool);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/admin/slots/generate")
                .header(header::COOKIE, "user_id=2")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn post_admin_generate_slots_creates_future_slots() {
    let pool = db::init_pool("sqlite::memory:").await.unwrap();
    let app = app_with_pool(pool.clone());

    let before_count: i64 = sqlx::query("SELECT COUNT(*) as count FROM slots")
        .fetch_one(&pool)
        .await
        .unwrap()
        .get("count");

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/admin/slots/generate")
                .header(header::COOKIE, "user_id=1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_body_string(response).await;
    assert!(body.contains("id=\"slots-content\""));

    let after_count: i64 = sqlx::query("SELECT COUNT(*) as count FROM slots")
        .fetch_one(&pool)
        .await
        .unwrap()
        .get("count");
    assert!(after_count > before_count);
}

#[tokio::test]
async fn e2e_signup_cancel_and_promotion_flow() {
    let pool = db::init_pool("sqlite::memory:").await.unwrap();
    let app = app_with_pool(pool.clone());

    let signup_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/slots/1/signup")
                .header(header::COOKIE, "user_id=9")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(signup_response.status(), StatusCode::OK);

    let cancel_response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/slots/1/cancel")
                .header(header::COOKIE, "user_id=2")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(cancel_response.status(), StatusCode::OK);

    let signed_up_player =
        sqlx::query("SELECT is_waitlist, position FROM bookings WHERE slot_id = ? AND user_id = ?")
            .bind(1_i64)
            .bind(9_i64)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(signed_up_player.get::<i64, _>("is_waitlist"), 0);
    assert_eq!(signed_up_player.get::<i64, _>("position"), 6);

    let promoted_waitlist_top =
        sqlx::query("SELECT is_waitlist, position FROM bookings WHERE slot_id = ? AND user_id = ?")
            .bind(1_i64)
            .bind(7_i64)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(promoted_waitlist_top.get::<i64, _>("is_waitlist"), 0);
    assert_eq!(promoted_waitlist_top.get::<i64, _>("position"), 7);
}
