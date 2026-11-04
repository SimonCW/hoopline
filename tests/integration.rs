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

async fn test_pool() -> sqlx::SqlitePool {
    let pool = db::init_pool("sqlite::memory:").await.unwrap();
    // Fixed future dates keep booking tests independent of the current weekday.
    sqlx::query("UPDATE slots SET datetime = CASE id WHEN 1 THEN '2099-01-05T19:00:00Z' WHEN 2 THEN '2099-01-06T19:00:00Z' ELSE '2099-01-08T19:00:00Z' END")
        .execute(&pool).await.unwrap();
    pool
}

async fn insert_waitlist_booking(
    pool: &sqlx::SqlitePool,
    slot_id: i64,
    user_id: i64,
    position: i64,
) {
    sqlx::query(
        "INSERT INTO bookings (slot_id, user_id, position, is_waitlist) VALUES (?, ?, ?, 1)",
    )
    .bind(slot_id)
    .bind(user_id)
    .bind(position)
    .execute(pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn booking_mutations_increment_revision_once_and_failures_do_not() {
    let pool = test_pool().await;
    let before = db::find_slot(&pool, 1).await.unwrap().unwrap().revision;
    assert_eq!(
        db::signup_for_slot(&pool, 1, 7).await.unwrap(),
        db::SignupOutcome::AddedPlayer
    );
    assert_eq!(
        db::find_slot(&pool, 1).await.unwrap().unwrap().revision,
        before + 1
    );
    assert_eq!(
        db::signup_for_slot(&pool, 1, 7).await.unwrap(),
        db::SignupOutcome::AlreadySignedUp
    );
    assert_eq!(
        db::find_slot(&pool, 1).await.unwrap().unwrap().revision,
        before + 1
    );
    assert_eq!(
        db::cancel_booking_for_slot(&pool, 1, 7).await.unwrap(),
        db::CancelOutcome::Cancelled
    );
    assert_eq!(
        db::find_slot(&pool, 1).await.unwrap().unwrap().revision,
        before + 2
    );
    assert_eq!(
        db::cancel_booking_for_slot(&pool, 1, 7).await.unwrap(),
        db::CancelOutcome::NotSignedUp
    );
    assert_eq!(
        db::find_slot(&pool, 1).await.unwrap().unwrap().revision,
        before + 2
    );
}

#[tokio::test]
async fn admin_mode_is_explicit_and_requires_admin_identity() {
    let pool = test_pool().await;
    for (cookie, uri, expected) in [
        ("user_id=1", "/slots", false),
        ("user_id=1", "/slots?mode=admin", true),
        ("user_id=1", "/slots/fragment?mode=admin", true),
        ("user_id=2", "/slots?mode=admin", false),
    ] {
        let response = app_with_pool(pool.clone())
            .oneshot(
                Request::builder()
                    .uri(uri)
                    .header(header::COOKIE, cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let body = response_body_string(response).await;
        assert_eq!(body.contains("admin-mode-card"), expected, "{cookie} {uri}");
    }
}

#[tokio::test]
async fn admin_can_append_members_and_reorder_waitlist_with_revision_checks() {
    let pool = test_pool().await;
    let app = app_with_pool(pool.clone());
    let post = |uri: &str, body: &str, cookie: &str| {
        Request::builder()
            .method("POST")
            .uri(uri)
            .header(header::COOKIE, cookie)
            .header(
                "x-hoopline-user",
                if cookie == "user_id=1" { "1" } else { "2" },
            )
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .body(Body::from(body.to_string()))
            .unwrap()
    };
    let add = "/admin/slots/1/add";
    let response = app
        .clone()
        .oneshot(post(add, "user_id=7&revision=0&list=waitlist", "user_id=2"))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let response = app
        .clone()
        .oneshot(post(add, "user_id=7&revision=0&list=waitlist", "user_id=1"))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let response = app
        .clone()
        .oneshot(post(add, "user_id=8&revision=0&list=waitlist", "user_id=1"))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CONFLICT);
    let response = app
        .clone()
        .oneshot(post(add, "user_id=8&revision=1&list=waitlist", "user_id=1"))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let response = app
        .clone()
        .oneshot(post(
            "/admin/slots/1/move/8",
            "revision=2&direction=up",
            "user_id=1",
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let slot = db::find_slot(&pool, 1).await.unwrap().unwrap();
    assert_eq!(slot.waitlist_user_ids, vec![8, 7]);
    assert_eq!(slot.revision, 3);
    let response = app
        .oneshot(post(
            "/admin/slots/1/move/8",
            "revision=3&direction=up",
            "user_id=1",
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(db::find_slot(&pool, 1).await.unwrap().unwrap().revision, 3);
}

#[tokio::test]
async fn stale_admin_remove_and_promote_leave_bookings_unchanged() {
    let pool = test_pool().await;
    insert_waitlist_booking(&pool, 1, 7, 1).await;
    assert_eq!(
        db::signup_for_slot(&pool, 1, 8).await.unwrap(),
        db::SignupOutcome::AddedPlayer
    );
    let app = app_with_pool(pool.clone());
    for uri in ["/admin/slots/1/remove/2", "/admin/slots/1/promote/7"] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(uri)
                    .header(header::COOKIE, "user_id=1")
                    .header("x-hoopline-user", "1")
                    .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                    .body(Body::from("revision=0"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CONFLICT);
    }
    let slot = db::find_slot(&pool, 1).await.unwrap().unwrap();
    assert!(slot.player_user_ids.contains(&2));
    assert_eq!(slot.waitlist_user_ids, vec![7]);
    assert_eq!(slot.revision, 1);
}

#[tokio::test]
async fn get_root_returns_ok_and_body() {
    let pool = test_pool().await;
    let app = app_with_pool(pool);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/")
                .header(header::COOKIE, "user_id=2")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_body_string(response).await;
    assert!(body.contains("Hoopline"));
    assert!(body.contains("Luisenschule"));
    assert!(body.contains("Alex"));
    assert!(body.contains("20:00"));
    assert!(body.contains("Week of"));
    assert!(!body.contains("Berlin local time"));
    assert!(!body.contains(">You<"));
    assert_eq!(body.matches("data-testid=\"slot-card\"").count(), 3);
    assert_eq!(body.matches("data-testid=\"player-row\"").count(), 19);
    assert_eq!(body.matches("data-testid=\"waitlist-row\"").count(), 0);
    assert!(body.contains("slot-grid"));
    assert!(body.contains("roster"));
}

#[tokio::test]
async fn get_slots_returns_seeded_data() {
    let pool = test_pool().await;
    let app = app_with_pool(pool);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/slots")
                .header(header::COOKIE, "user_id=2")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_body_string(response).await;
    assert!(body.contains("Ceci"));
    assert!(body.contains("Jamal"));
    assert!(body.contains("id=\"slots-content\""));
    assert!(body.contains("refresh-slots from:body"));
    assert!(body.contains("roster-list"));
}

#[tokio::test]
async fn get_slots_fragment_reflects_selected_user_from_cookie() {
    let pool = test_pool().await;
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
    assert!(body.contains("identity-name\">Alex"));
    assert!(body.contains("Cancel"));
}

#[tokio::test]
async fn get_healthz_returns_ok() {
    let pool = test_pool().await;
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
    let pool = test_pool().await;
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
    assert!(body.contains("id=\"name-step\""));
    assert!(body.contains("Alex"));
    assert!(body.contains("Jamali"));
    assert!(!body.contains("Admin mode"));
}

#[tokio::test]
async fn post_users_select_sets_cookie_and_persists_identity() {
    let pool = test_pool().await;
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

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(response.headers()[header::LOCATION], "/slots");
    let set_cookie = response
        .headers()
        .get(header::SET_COOKIE)
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    assert!(set_cookie.contains("user_id=2"));
    assert!(set_cookie.contains("Max-Age=31536000"));
    assert!(set_cookie.contains("HttpOnly"));
    assert!(set_cookie.contains("SameSite=Lax"));

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
    assert!(body.contains("identity-name\">Ben"));
}

#[tokio::test]
async fn get_users_shows_admin_tools_for_admin_cookie() {
    let pool = test_pool().await;
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
    assert!(!body.contains("Admin tools"));
}

#[tokio::test]
async fn post_users_select_creates_user_when_missing() {
    let pool = test_pool().await;
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

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    let set_cookie = response
        .headers()
        .get(header::SET_COOKIE)
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    let cookie_pair = set_cookie.split(';').next().unwrap().to_string();
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
    let pool = test_pool().await;
    let app = app_with_pool(pool.clone());

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/slots/3/signup")
                .header(header::COOKIE, "user_id=6")
                .header("x-hoopline-user", "6")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_body_string(response).await;
    assert!(body.contains("id=\"slot-3\""));
    assert!(body.contains("Farid"));
    assert!(body.contains("current-player"));
    assert!(body.contains("Booking saved. Confirmed player."));
    assert!(!body.contains(">You<"));

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
    let pool = test_pool().await;
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
                .header("x-hoopline-user", "8")
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
    assert_eq!(booking.get::<i64, _>("position"), 1);
}

#[tokio::test]
async fn post_slots_signup_rejects_when_slot_and_waitlist_are_full() {
    let pool = test_pool().await;
    sqlx::query("UPDATE slots SET max_players = ?, max_waitlist = ? WHERE id = ?")
        .bind(8_i64)
        .bind(1_i64)
        .bind(2_i64)
        .execute(&pool)
        .await
        .unwrap();
    insert_waitlist_booking(&pool, 2, 7, 1).await;
    let app = app_with_pool(pool.clone());

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/slots/2/signup")
                .header(header::COOKIE, "user_id=8")
                .header("x-hoopline-user", "8")
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
    let pool = test_pool().await;
    let app = app_with_pool(pool);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/slots/1/signup")
                .header(header::COOKIE, "user_id=1")
                .header("x-hoopline-user", "1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn post_slots_cancel_promotes_waitlist_after_player_cancel() {
    let pool = test_pool().await;
    sqlx::query("UPDATE slots SET max_players = ? WHERE id = ?")
        .bind(6_i64)
        .bind(1_i64)
        .execute(&pool)
        .await
        .unwrap();
    insert_waitlist_booking(&pool, 1, 7, 1).await;
    insert_waitlist_booking(&pool, 1, 8, 2).await;
    let app = app_with_pool(pool.clone());

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/slots/1/cancel")
                .header(header::COOKIE, "user_id=2")
                .header("x-hoopline-user", "2")
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
    let pool = test_pool().await;
    sqlx::query("UPDATE slots SET max_players = ? WHERE id = ?")
        .bind(5_i64)
        .bind(3_i64)
        .execute(&pool)
        .await
        .unwrap();
    insert_waitlist_booking(&pool, 3, 3, 1).await;
    insert_waitlist_booking(&pool, 3, 4, 2).await;
    insert_waitlist_booking(&pool, 3, 5, 3).await;
    let app = app_with_pool(pool.clone());

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/slots/3/cancel")
                .header(header::COOKIE, "user_id=4")
                .header("x-hoopline-user", "4")
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
    let pool = test_pool().await;
    let app = app_with_pool(pool);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/admin/slots/1/remove/2")
                .header(header::COOKIE, "user_id=2")
                .header("x-hoopline-user", "2")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn post_admin_remove_triggers_waitlist_promotion() {
    let pool = test_pool().await;
    sqlx::query("UPDATE slots SET max_players = ? WHERE id = ?")
        .bind(6_i64)
        .bind(1_i64)
        .execute(&pool)
        .await
        .unwrap();
    insert_waitlist_booking(&pool, 1, 7, 1).await;
    let app = app_with_pool(pool.clone());

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/admin/slots/1/remove/2")
                .header(header::COOKIE, "user_id=1")
                .header("x-hoopline-user", "1")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from("revision=0"))
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
    let pool = test_pool().await;
    sqlx::query("UPDATE slots SET max_players = ? WHERE id = ?")
        .bind(6_i64)
        .bind(3_i64)
        .execute(&pool)
        .await
        .unwrap();
    insert_waitlist_booking(&pool, 3, 4, 1).await;
    insert_waitlist_booking(&pool, 3, 5, 2).await;
    let app = app_with_pool(pool.clone());

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/admin/slots/3/promote/4")
                .header(header::COOKIE, "user_id=1")
                .header("x-hoopline-user", "1")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from("revision=0"))
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
    assert_eq!(shifted.get::<i64, _>("position"), 1);
}

#[tokio::test]
async fn post_admin_generate_slots_rejects_non_admin() {
    let pool = test_pool().await;
    let app = app_with_pool(pool);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/admin/slots/generate")
                .header(header::COOKIE, "user_id=2")
                .header("x-hoopline-user", "2")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn post_admin_generate_slots_creates_future_slots() {
    let pool = test_pool().await;
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
                .header("x-hoopline-user", "1")
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
    let pool = test_pool().await;
    sqlx::query("UPDATE slots SET max_players = ? WHERE id = ?")
        .bind(7_i64)
        .bind(1_i64)
        .execute(&pool)
        .await
        .unwrap();
    insert_waitlist_booking(&pool, 1, 7, 1).await;
    let app = app_with_pool(pool.clone());

    let signup_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/slots/1/signup")
                .header(header::COOKIE, "user_id=9")
                .header("x-hoopline-user", "9")
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
                .header("x-hoopline-user", "2")
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

#[tokio::test]
async fn past_slots_are_hidden_and_reject_direct_signup() {
    let pool = test_pool().await;
    sqlx::query("UPDATE slots SET datetime = '2000-01-01T20:00:00Z' WHERE id = 1")
        .execute(&pool)
        .await
        .unwrap();
    let app = app_with_pool(pool.clone());
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/")
                .header(header::COOKIE, "user_id=2")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
    let body = response_body_string(response).await;
    assert!(!body.contains("id=\"slot-1\""));
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/slots/1/signup")
                .header(header::COOKIE, "user_id=7")
                .header("x-hoopline-user", "7")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert!(
        response_body_string(response)
            .await
            .contains("This slot has started")
    );
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM bookings WHERE slot_id = 1 AND user_id = 7")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn rendered_capacity_status_and_cancellation_match_database() {
    let pool = test_pool().await;
    sqlx::query("UPDATE slots SET max_players = 6, max_waitlist = 1 WHERE id = 1")
        .execute(&pool)
        .await
        .unwrap();
    let app = app_with_pool(pool);
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/slots/1/signup")
                .header(header::COOKIE, "user_id=7")
                .header("x-hoopline-user", "7")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body = response_body_string(response).await;
    assert!(body.contains("6/6"));
    assert!(body.contains("1/1"));
    assert!(body.contains("Booking saved. Waitlist #1."));
    assert!(body.contains("hx-confirm="));
    assert!(body.contains("hx-sync=\"body:drop\""));
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/slots/1/cancel")
                .header(header::COOKIE, "user_id=7")
                .header("x-hoopline-user", "7")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body = response_body_string(response).await;
    assert!(body.contains("Booking cancelled"));
    assert!(body.contains("Join Waitlist"));
    assert!(body.contains("Not booked"));
}

#[tokio::test]
async fn identity_selection_requires_an_existing_id() {
    let pool = test_pool().await;
    let app = app_with_pool(pool.clone());
    for (id, expected) in [
        (2, StatusCode::SEE_OTHER),
        (999_999, StatusCode::BAD_REQUEST),
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/users/select")
                    .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                    .body(Body::from(format!("selected_user_id={id}")))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), expected);
    }
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM users")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 10);
    let response = app
        .oneshot(
            Request::builder()
                .uri("/users")
                .header(header::COOKIE, "user_id=2")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body = response_body_string(response).await;
    assert!(body.contains("Log out"));
    assert!(!body.contains("Change name"));
    assert!(!body.contains("id=\"existing-user\""));
    assert!(!body.contains("id=\"new-member\""));
}

#[tokio::test]
async fn new_name_validation_and_empty_state_are_clear() {
    let pool = test_pool().await;
    sqlx::query("UPDATE slots SET datetime = '2000-01-01T20:00:00Z'")
        .execute(&pool)
        .await
        .unwrap();
    let app = app_with_pool(pool);
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/")
                .header(header::COOKIE, "user_id=2")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert!(
        response_body_string(response)
            .await
            .contains("No upcoming slots")
    );
    for name in [" ".to_string(), "x".repeat(65)] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/users/select")
                    .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                    .body(Body::from(format!("new_user_name={name}")))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }
}

#[tokio::test]
async fn file_database_serializes_simultaneous_signups_without_overbooking() {
    let directory = tempfile::tempdir().unwrap();
    let database_url = format!(
        "sqlite://{}",
        directory.path().join("bookings.db").display()
    );
    let pool = db::init_pool(&database_url).await.unwrap();
    sqlx::query("DELETE FROM bookings WHERE slot_id = 1")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE slots SET datetime = '2099-01-05T19:00:00Z', max_players = 2, max_waitlist = 3 WHERE id = 1")
        .execute(&pool).await.unwrap();
    let mut users = Vec::new();
    for number in 0..20 {
        users.push(
            db::create_user(&pool, &format!("Concurrent {number}"))
                .await
                .unwrap()
                .id,
        );
    }
    let barrier = std::sync::Arc::new(tokio::sync::Barrier::new(users.len()));
    let mut tasks = tokio::task::JoinSet::new();
    for user_id in users {
        let pool = pool.clone();
        let barrier = barrier.clone();
        tasks.spawn(async move {
            barrier.wait().await;
            db::signup_for_slot(&pool, 1, user_id).await.unwrap()
        });
    }
    let mut players = 0;
    let mut waiting = 0;
    let mut full = 0;
    while let Some(result) = tasks.join_next().await {
        match result.unwrap() {
            db::SignupOutcome::AddedPlayer => players += 1,
            db::SignupOutcome::AddedWaitlist => waiting += 1,
            db::SignupOutcome::Full => full += 1,
            outcome => panic!("unexpected signup result: {outcome:?}"),
        }
    }
    assert_eq!((players, waiting, full), (2, 3, 15));
    let rows = sqlx::query("SELECT is_waitlist, position FROM bookings WHERE slot_id = 1 ORDER BY is_waitlist, position")
        .fetch_all(&pool).await.unwrap();
    let positions: Vec<(i64, i64)> = rows
        .iter()
        .map(|row| (row.get("is_waitlist"), row.get("position")))
        .collect();
    assert_eq!(positions, [(0, 1), (0, 2), (1, 1), (1, 2), (1, 3)]);
    pool.close().await;
    let pool = db::init_pool(&database_url).await.unwrap();
    assert_eq!(
        db::find_slot(&pool, 1)
            .await
            .unwrap()
            .unwrap()
            .players
            .len(),
        2
    );
    pool.close().await;
}

#[tokio::test]
async fn overview_requires_a_valid_remembered_identity() {
    let app = app_with_pool(test_pool().await);
    for uri in ["/", "/slots"] {
        for cookie in ["", "user_id=invalid", "user_id=999999"] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .uri(uri)
                        .header(header::COOKIE, cookie)
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            let body = response_body_string(response).await;
            assert!(body.contains("Choose your name to see the next games"));
            assert!(body.contains("role=\"combobox\""));
            assert!(body.contains("role=\"listbox\""));
            assert!(!body.contains("id=\"slots-content\""));
            assert!(!body.contains("data-testid=\"slot-card\""));
        }
    }
    let response = app
        .oneshot(
            Request::builder()
                .uri("/slots/fragment")
                .header("hx-request", "true")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["hx-redirect"], "/");
    assert_eq!(response_body_string(response).await, "");
}

#[tokio::test]
async fn htmx_name_selection_uses_full_page_redirect_and_persistent_cookie() {
    let app = app_with_pool(test_pool().await);
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/users/select")
                .header("hx-request", "true")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from("selected_user_id=2"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["hx-redirect"], "/slots");
    let cookie = response.headers()[header::SET_COOKIE].to_str().unwrap();
    assert!(cookie.contains("user_id=2; Path=/; Max-Age=31536000; HttpOnly; SameSite=Lax"));
}

#[tokio::test]
async fn logout_clears_identity_without_removing_bookings() {
    let pool = test_pool().await;
    let before: i64 = sqlx::query_scalar("SELECT count(*) FROM bookings")
        .fetch_one(&pool)
        .await
        .unwrap();
    let app = app_with_pool(pool.clone());
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/users/logout")
                .header(header::COOKIE, "user_id=2")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(response.headers()[header::LOCATION], "/");
    let cookie = response.headers()[header::SET_COOKIE].to_str().unwrap();
    assert!(cookie.starts_with("user_id=; Path=/; Max-Age=0;"));
    let after: i64 = sqlx::query_scalar("SELECT count(*) FROM bookings")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(before, after);
    let response = app
        .oneshot(
            Request::builder()
                .uri("/slots")
                .header(header::COOKIE, "user_id=")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert!(
        !response_body_string(response)
            .await
            .contains("id=\"slots-content\"")
    );
}

#[tokio::test]
async fn explicit_selection_uses_id_when_legacy_names_differ_only_in_case() {
    let pool = test_pool().await;
    let id: i64 =
        sqlx::query_scalar("INSERT INTO users (name, is_admin) VALUES ('ben', 0) RETURNING id")
            .fetch_one(&pool)
            .await
            .unwrap();
    let app = app_with_pool(pool);
    for (selected_id, name) in [(2, "Ben"), (id, "ben")] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/users/select")
                    .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                    .body(Body::from(format!(
                        "selected_user_id={selected_id}&existing_user_name={name}"
                    )))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::SEE_OTHER);
        assert!(
            response.headers()[header::SET_COOKIE]
                .to_str()
                .unwrap()
                .starts_with(&format!("user_id={selected_id};"))
        );
    }
}

#[tokio::test]
async fn typed_name_without_an_explicit_selection_cannot_choose_an_account() {
    let pool = test_pool().await;
    sqlx::query("INSERT INTO users (name, is_admin) VALUES ('ben', 0)")
        .execute(&pool)
        .await
        .unwrap();
    let app = app_with_pool(pool);
    for form in [
        "existing_user_name=Ben",
        "existing_user_name=ben",
        "existing_user_name=BEN",
        "existing_user_name=Alex",
        "new_user_name=Ben",
        "new_user_name=ben",
        "new_user_name=BEN",
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/users/select")
                    .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                    .body(Body::from(form))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert!(!response.headers().contains_key(header::SET_COOKIE));
    }
}

async fn booking_state(pool: &sqlx::SqlitePool) -> Vec<(i64, i64, i64, i64)> {
    sqlx::query("SELECT slot_id, user_id, position, is_waitlist FROM bookings ORDER BY id")
        .fetch_all(pool)
        .await
        .unwrap()
        .iter()
        .map(|row| {
            (
                row.get("slot_id"),
                row.get("user_id"),
                row.get("position"),
                row.get("is_waitlist"),
            )
        })
        .collect()
}

#[tokio::test]
async fn stale_displayed_identity_cannot_change_bookings_or_generate_slots() {
    let pool = test_pool().await;
    insert_waitlist_booking(&pool, 3, 4, 1).await;
    let app = app_with_pool(pool.clone());
    let before = booking_state(&pool).await;
    let slots_before: i64 = sqlx::query_scalar("SELECT count(*) FROM slots")
        .fetch_one(&pool)
        .await
        .unwrap();
    for (uri, cookie, displayed_id) in [
        ("/slots/3/signup", "user_id=6", "7"),
        ("/slots/1/cancel", "user_id=2", "1"),
        ("/admin/slots/1/remove/2", "user_id=1", "2"),
        ("/admin/slots/3/promote/4", "user_id=1", "2"),
        ("/admin/slots/generate", "user_id=1", "2"),
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(uri)
                    .header(header::COOKIE, cookie)
                    .header("x-hoopline-user", displayed_id)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CONFLICT, "{uri}");
        assert!(
            response_body_string(response)
                .await
                .contains("Your selected name changed")
        );
        assert_eq!(booking_state(&pool).await, before, "{uri}");
        let slots_after: i64 = sqlx::query_scalar("SELECT count(*) FROM slots")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(slots_after, slots_before);
    }
}

#[tokio::test]
async fn missing_or_invalid_displayed_identity_cannot_modify_bookings() {
    let pool = test_pool().await;
    let app = app_with_pool(pool.clone());
    let before = booking_state(&pool).await;
    for displayed in [None, Some(""), Some("not-an-id"), Some("999999")] {
        for (uri, cookie) in [
            ("/slots/3/signup", "user_id=6"),
            ("/slots/3/cancel", "user_id=6"),
            ("/admin/slots/1/remove/2", "user_id=1"),
            ("/admin/slots/3/promote/4", "user_id=1"),
            ("/admin/slots/generate", "user_id=1"),
        ] {
            let mut request = Request::builder()
                .method("POST")
                .uri(uri)
                .header(header::COOKIE, cookie);
            if let Some(id) = displayed {
                request = request.header("x-hoopline-user", id);
            }
            let response = app
                .clone()
                .oneshot(request.body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::CONFLICT);
            assert_eq!(booking_state(&pool).await, before);
        }
    }
}

#[tokio::test]
async fn logged_out_or_deleted_cookie_cannot_act_on_a_remembered_page() {
    let pool = test_pool().await;
    let app = app_with_pool(pool.clone());
    let before = booking_state(&pool).await;
    for cookie in ["", "user_id=", "user_id=invalid", "user_id=999999"] {
        for uri in ["/slots/3/signup", "/slots/3/cancel"] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri(uri)
                        .header(header::COOKIE, cookie)
                        .header("x-hoopline-user", "6")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::CONFLICT);
            assert_eq!(booking_state(&pool).await, before);
        }
    }
}
