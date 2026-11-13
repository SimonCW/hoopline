use chrono::{DateTime, Utc};
use sqlx::{Row, SqlitePool};

use super::{ActivityKind, BookingActivity, BookingDetails, BookingList, record_booking_at};
use crate::db::{self, CancelOutcome, SignupOutcome};

async fn fixture() -> SqlitePool {
    let pool = db::init_pool("sqlite::memory:").await.unwrap();
    seed(&pool).await;
    pool
}

async fn seed(pool: &SqlitePool) {
    sqlx::raw_sql(
        "INSERT INTO users (id, name) VALUES (1, 'Alex'), (2, 'Ben'), (3, 'Chris');
         INSERT INTO slots (id, datetime, venue, max_players, max_waitlist)
         VALUES (1, '2099-07-01T22:30:00Z', 'Court', 1, 1);",
    )
    .execute(pool)
    .await
    .unwrap();
}

async fn count(pool: &SqlitePool) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM activity_log")
        .fetch_one(pool)
        .await
        .unwrap()
}

#[tokio::test]
async fn signup_and_cancellation_keep_booking_snapshots() {
    let pool = fixture().await;
    let before = Utc::now();
    assert_eq!(
        db::signup_for_slot(&pool, 1, 1).await.unwrap(),
        SignupOutcome::AddedPlayer
    );
    assert_eq!(
        db::cancel_booking_for_slot(&pool, 1, 1).await.unwrap(),
        CancelOutcome::Cancelled
    );
    let rows = sqlx::query("SELECT * FROM activity_log ORDER BY id")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(rows.len(), 2);
    for (row, kind) in rows.iter().zip(["booking.signed_up", "booking.cancelled"]) {
        assert_eq!(row.get::<String, _>("kind"), kind);
        assert_eq!(row.get::<i64, _>("schema_version"), 1);
        assert_eq!(row.get::<i64, _>("actor_user_id"), 1);
        assert_eq!(row.get::<i64, _>("subject_user_id"), 1);
        assert_eq!(row.get::<String, _>("actor_name"), "Alex");
        assert_eq!(row.get::<String, _>("subject_name"), "Alex");
        assert_eq!(row.get::<i64, _>("slot_id"), 1);
        assert_eq!(row.get::<String, _>("slot_local_date"), "2099-07-02");
        assert_eq!(
            row.get::<String, _>("slot_starts_at"),
            "2099-07-01T22:30:00.000000Z"
        );
        let time = DateTime::parse_from_rfc3339(&row.get::<String, _>("occurred_at")).unwrap();
        assert!(time >= before && time <= Utc::now());
    }
    let details: Vec<BookingDetails> = rows
        .iter()
        .map(|row| serde_json::from_str(&row.get::<String, _>("details")).unwrap())
        .collect();
    assert_eq!(details[0], details[1]);
    assert_eq!(details[0].list, BookingList::Players);
    assert_eq!(details[0].position, 1);
    assert_ne!(details[0].booked_at, "");
}

#[tokio::test]
async fn cancellation_links_automatic_promotion_to_the_correct_actor() {
    let pool = fixture().await;
    db::signup_for_slot(&pool, 1, 1).await.unwrap();
    assert_eq!(
        db::signup_for_slot(&pool, 1, 2).await.unwrap(),
        SignupOutcome::AddedWaitlist
    );
    db::cancel_booking_for_slot(&pool, 1, 1).await.unwrap();
    let rows = sqlx::query("SELECT * FROM activity_log ORDER BY id")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(rows.len(), 4);
    let promotion = &rows[3];
    assert_eq!(promotion.get::<String, _>("kind"), "booking.promoted");
    assert_eq!(promotion.get::<i64, _>("actor_user_id"), 1);
    assert_eq!(promotion.get::<i64, _>("subject_user_id"), 2);
    let details: BookingDetails =
        serde_json::from_str(&promotion.get::<String, _>("details")).unwrap();
    assert_eq!(details.list, BookingList::Waitlist);
    assert_eq!(details.caused_by_activity_id, Some(rows[2].get("id")));
    let slot = db::find_slot(&pool, 1).await.unwrap().unwrap();
    assert_eq!(slot.player_user_ids, vec![2]);
}

#[tokio::test]
async fn waitlist_cancellation_is_not_a_player_cancellation() {
    let pool = fixture().await;
    db::signup_for_slot(&pool, 1, 1).await.unwrap();
    db::signup_for_slot(&pool, 1, 2).await.unwrap();
    db::cancel_booking_for_slot(&pool, 1, 2).await.unwrap();
    let details: String =
        sqlx::query_scalar("SELECT details FROM activity_log WHERE kind = 'booking.cancelled'")
            .fetch_one(&pool)
            .await
            .unwrap();
    let details: BookingDetails = serde_json::from_str(&details).unwrap();
    assert_eq!(details.list, BookingList::Waitlist);
    assert_eq!(count(&pool).await, 3);
}

#[tokio::test]
async fn rejected_and_repeated_actions_do_not_create_activity() {
    let pool = fixture().await;
    db::signup_for_slot(&pool, 1, 1).await.unwrap();
    assert_eq!(
        db::signup_for_slot(&pool, 1, 1).await.unwrap(),
        SignupOutcome::AlreadySignedUp
    );
    db::signup_for_slot(&pool, 1, 2).await.unwrap();
    assert_eq!(
        db::signup_for_slot(&pool, 1, 3).await.unwrap(),
        SignupOutcome::Full
    );
    assert_eq!(
        db::signup_for_slot(&pool, 999, 3).await.unwrap(),
        SignupOutcome::SlotNotFound
    );
    assert_eq!(
        db::cancel_booking_for_slot(&pool, 1, 3).await.unwrap(),
        CancelOutcome::NotSignedUp
    );
    assert_eq!(
        db::cancel_booking_for_slot(&pool, 999, 3).await.unwrap(),
        CancelOutcome::SlotNotFound
    );
    sqlx::query("UPDATE slots SET datetime = '2000-01-01T00:00:00Z'")
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        db::signup_for_slot(&pool, 1, 3).await.unwrap(),
        SignupOutcome::Started
    );
    assert_eq!(count(&pool).await, 2);
}

#[tokio::test]
async fn admin_removal_and_its_automatic_promotion_do_not_create_activity() {
    let pool = fixture().await;
    db::signup_for_slot(&pool, 1, 1).await.unwrap();
    db::signup_for_slot(&pool, 1, 2).await.unwrap();
    assert_eq!(
        db::cancel_booking_checked(&pool, 1, 1, Some(0))
            .await
            .unwrap(),
        CancelOutcome::Stale
    );
    assert_eq!(
        db::cancel_booking_checked(&pool, 1, 1, Some(2))
            .await
            .unwrap(),
        CancelOutcome::Cancelled
    );
    assert_eq!(count(&pool).await, 2);
    let slot = db::find_slot(&pool, 1).await.unwrap().unwrap();
    assert_eq!(slot.player_user_ids, vec![2]);
}

#[tokio::test]
async fn activity_failure_rolls_back_signup_and_cancellation() {
    let pool = fixture().await;
    sqlx::raw_sql(
        "CREATE TRIGGER reject_activity BEFORE INSERT ON activity_log
         BEGIN SELECT RAISE(ABORT, 'test failure'); END;",
    )
    .execute(&pool)
    .await
    .unwrap();
    assert!(db::signup_for_slot(&pool, 1, 1).await.is_err());
    let slot = db::find_slot(&pool, 1).await.unwrap().unwrap();
    assert_eq!(slot.players.len(), 0);
    assert_eq!(slot.revision, 0);
    sqlx::query("DROP TRIGGER reject_activity")
        .execute(&pool)
        .await
        .unwrap();
    db::signup_for_slot(&pool, 1, 1).await.unwrap();
    db::signup_for_slot(&pool, 1, 2).await.unwrap();
    // Fail the second event, after the cancellation has already been recorded.
    sqlx::raw_sql(
        "CREATE TRIGGER reject_promotion BEFORE INSERT ON activity_log
         WHEN NEW.kind = 'booking.promoted'
         BEGIN SELECT RAISE(ABORT, 'test failure'); END;",
    )
    .execute(&pool)
    .await
    .unwrap();
    assert!(db::cancel_booking_for_slot(&pool, 1, 1).await.is_err());
    let slot = db::find_slot(&pool, 1).await.unwrap().unwrap();
    assert_eq!(slot.player_user_ids, vec![1]);
    assert_eq!(slot.waitlist_user_ids, vec![2]);
    assert_eq!(slot.revision, 2);
    assert_eq!(count(&pool).await, 2);
}

#[tokio::test]
async fn history_survives_edits_deletion_and_restart() {
    let dir = tempfile::tempdir().unwrap();
    let url = format!("sqlite://{}", dir.path().join("activity.db").display());
    let pool = db::init_pool(&url).await.unwrap();
    seed(&pool).await;
    db::signup_for_slot(&pool, 1, 1).await.unwrap();
    sqlx::raw_sql(
        "UPDATE slots SET datetime = '2099-08-01T10:00:00Z', venue = 'Other court';
         UPDATE users SET name = 'New name' WHERE id = 1;
         DELETE FROM bookings;
         DELETE FROM slots;
         DELETE FROM users;",
    )
    .execute(&pool)
    .await
    .unwrap();
    pool.close().await;
    let pool = db::init_pool(&url).await.unwrap();
    // Reuse the member and game IDs. Old events must not attach to these records.
    seed(&pool).await;
    db::signup_for_slot(&pool, 1, 1).await.unwrap();
    let row = sqlx::query("SELECT * FROM activity_log ORDER BY id LIMIT 1")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(row.get::<String, _>("subject_name"), "Alex");
    assert_eq!(row.get::<String, _>("venue"), "Court");
    assert_eq!(row.get::<String, _>("slot_local_date"), "2099-07-02");
    assert_eq!(row.get::<Option<i64>, _>("subject_user_id"), None);
    assert_eq!(row.get::<Option<i64>, _>("actor_user_id"), None);
    assert_eq!(row.get::<Option<i64>, _>("slot_id"), None);
    let new_subject: i64 =
        sqlx::query_scalar("SELECT subject_user_id FROM activity_log ORDER BY id DESC LIMIT 1")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(new_subject, 1);
}

#[tokio::test]
async fn concurrent_duplicate_signups_create_one_event() {
    let dir = tempfile::tempdir().unwrap();
    let url = format!("sqlite://{}", dir.path().join("concurrent.db").display());
    let pool = db::init_pool(&url).await.unwrap();
    seed(&pool).await;
    let (first, second) = tokio::join!(
        db::signup_for_slot(&pool, 1, 1),
        db::signup_for_slot(&pool, 1, 1),
    );
    let outcomes = [first.unwrap(), second.unwrap()];
    assert!(outcomes.contains(&SignupOutcome::AddedPlayer));
    assert!(outcomes.contains(&SignupOutcome::AlreadySignedUp));
    assert_eq!(count(&pool).await, 1);
}

async fn booking_details(pool: &SqlitePool) -> Vec<BookingDetails> {
    let values: Vec<String> = sqlx::query_scalar("SELECT details FROM activity_log ORDER BY id")
        .fetch_all(pool)
        .await
        .unwrap();
    values
        .iter()
        .map(|value| serde_json::from_str(value).unwrap())
        .collect()
}

#[tokio::test]
async fn same_second_rebooking_has_a_new_lifecycle() {
    let pool = fixture().await;
    // Force both real signups into the same stored second, without a timing race.
    sqlx::raw_sql(
        "CREATE TRIGGER fixed_booking_time AFTER INSERT ON bookings
         BEGIN UPDATE bookings SET created_at = '2099-07-01 12:00:00' WHERE id = NEW.id; END;",
    )
    .execute(&pool)
    .await
    .unwrap();
    db::signup_for_slot(&pool, 1, 1).await.unwrap();
    db::cancel_booking_for_slot(&pool, 1, 1).await.unwrap();
    db::signup_for_slot(&pool, 1, 1).await.unwrap();
    db::cancel_booking_for_slot(&pool, 1, 1).await.unwrap();
    let details = booking_details(&pool).await;
    assert_eq!(details.len(), 4);
    assert_eq!(details[0].booking_id, details[1].booking_id);
    assert_eq!(details[2].booking_id, details[3].booking_id);
    assert_eq!(details[0].booked_at, details[2].booked_at);
    assert_ne!(details[0].booking_id, details[2].booking_id);
}

#[tokio::test]
async fn admin_replacement_starts_a_new_lifecycle_without_logging_admin_actions() {
    let pool = fixture().await;
    db::signup_for_slot(&pool, 1, 1).await.unwrap();
    db::cancel_booking_checked(&pool, 1, 1, Some(1))
        .await
        .unwrap();
    db::admin_add_member(&pool, 1, 1, 2, db::RosterList::Players)
        .await
        .unwrap();
    assert_eq!(count(&pool).await, 1);
    db::cancel_booking_for_slot(&pool, 1, 1).await.unwrap();
    let details = booking_details(&pool).await;
    assert_eq!(details.len(), 2);
    assert_ne!(details[0].booking_id, details[1].booking_id);
}

#[tokio::test]
async fn admin_add_move_promote_and_clear_do_not_create_activity() {
    let pool = fixture().await;
    sqlx::query("UPDATE slots SET max_waitlist = 2")
        .execute(&pool)
        .await
        .unwrap();
    db::admin_add_member(&pool, 1, 1, 0, db::RosterList::Waitlist)
        .await
        .unwrap();
    db::admin_add_member(&pool, 1, 2, 1, db::RosterList::Waitlist)
        .await
        .unwrap();
    db::admin_move_waiter(&pool, 1, 2, 2, db::MoveDirection::Up)
        .await
        .unwrap();
    assert_eq!(
        db::promote_waitlist_checked(&pool, 1, 2, Some(3))
            .await
            .unwrap(),
        db::PromoteOutcome::Promoted
    );
    db::admin_add_member(&pool, 1, 3, 4, db::RosterList::Waitlist)
        .await
        .unwrap();
    db::edit_game(
        &pool,
        &db::GameEdit {
            slot_id: 1,
            revision: 5,
            datetime: "2099-07-02T22:30:00Z".parse().unwrap(),
            venue: "New court".into(),
            map_url: None,
            clear_bookings: Some(true),
        },
    )
    .await
    .unwrap();
    assert_eq!(count(&pool).await, 0);
    let slot = db::find_slot(&pool, 1).await.unwrap().unwrap();
    assert_eq!(slot.players.len(), 0);
    assert_eq!(slot.waitlist.len(), 0);
}

#[tokio::test]
async fn lifecycle_migration_preserves_existing_bookings_and_constraints() {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
    sqlx::raw_sql(include_str!("../../migrations/001_initial.sql"))
        .execute(&pool)
        .await
        .unwrap();
    let select =
        "SELECT id, slot_id, user_id, position, is_waitlist, created_at FROM bookings ORDER BY id";
    let before: Vec<(i64, i64, i64, i64, i64, String)> =
        sqlx::query_as(select).fetch_all(&pool).await.unwrap();
    sqlx::raw_sql(include_str!("../../migrations/011_activity_log.sql"))
        .execute(&pool)
        .await
        .unwrap();
    let after: Vec<(i64, i64, i64, i64, i64, String)> =
        sqlx::query_as(select).fetch_all(&pool).await.unwrap();
    assert_eq!(before, after);
    assert!(
        sqlx::query("INSERT INTO bookings (slot_id, user_id, position) VALUES (1, 1, 1)")
            .execute(&pool)
            .await
            .is_err()
    );
    assert!(
        sqlx::query("INSERT INTO bookings (slot_id, user_id, position) VALUES (999, 1, 1)")
            .execute(&pool)
            .await
            .is_err()
    );
    let old_max = before.last().unwrap().0;
    sqlx::query("DELETE FROM bookings")
        .execute(&pool)
        .await
        .unwrap();
    let new_id: i64 = sqlx::query_scalar(
        "INSERT INTO bookings (slot_id, user_id, position) VALUES (1, 1, 1) RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(new_id > old_max);
}

async fn record_cancellation_at(pool: &SqlitePool, occurred_at: &str) {
    let mut tx = pool.begin().await.unwrap();
    record_booking_at(
        &mut tx,
        BookingActivity {
            kind: ActivityKind::Cancelled,
            actor_user_id: 1,
            subject_user_id: 1,
            slot_id: 1,
            caused_by_activity_id: None,
        },
        occurred_at.parse().unwrap(),
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
}

#[tokio::test]
async fn snapshots_use_berlin_dates_in_winter_summer_and_dst_transitions() {
    for (instant, local_date) in [
        ("2099-01-01T22:30:00Z", "2099-01-01"),
        ("2099-07-01T22:30:00Z", "2099-07-02"),
        ("2030-03-31T00:30:00Z", "2030-03-31"),
        ("2030-03-31T01:30:00Z", "2030-03-31"),
        ("2030-10-27T00:30:00Z", "2030-10-27"),
        ("2030-10-27T01:30:00Z", "2030-10-27"),
    ] {
        let pool = fixture().await;
        sqlx::query("UPDATE slots SET datetime = ?")
            .bind(instant)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO bookings (slot_id, user_id, position) VALUES (1, 1, 1)")
            .execute(&pool)
            .await
            .unwrap();
        record_cancellation_at(&pool, instant).await;
        let dates: (String, String) =
            sqlx::query_as("SELECT occurred_local_date, slot_local_date FROM activity_log")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(dates, (local_date.into(), local_date.into()), "{instant}");
    }
}

#[tokio::test]
async fn documented_query_counts_only_game_day_player_cancellations_within_two_hours() {
    // Execute the documented SQL itself so the example cannot drift from this test.
    let readme = include_str!("../../README.md");
    let query = readme
        .split("```sql\n")
        .nth(1)
        .unwrap()
        .split("```")
        .next()
        .unwrap();
    for (game, cancellation, waitlist, expected) in [
        ("2099-07-02T18:00:00Z", "2099-07-02T16:00:00Z", false, 1),
        ("2099-07-02T18:00:00Z", "2099-07-02T15:59:59Z", false, 0),
        ("2099-07-02T18:00:00Z", "2099-07-02T16:00:01Z", false, 1),
        ("2099-07-02T18:00:00Z", "2099-07-02T18:00:00Z", false, 1),
        ("2099-07-02T18:00:00Z", "2099-07-02T18:00:01Z", false, 0),
        ("2099-07-02T18:00:00Z", "2099-07-02T17:00:00Z", true, 0),
        // Within two hours, but before Berlin midnight on the previous day.
        ("2099-07-01T22:30:00Z", "2099-07-01T21:30:00Z", false, 0),
        // Different UTC dates, but both on the same Berlin calendar day.
        ("2099-07-02T00:00:00Z", "2099-07-01T22:30:00Z", false, 1),
        ("2099-01-02T00:00:00Z", "2099-01-01T23:30:00Z", false, 1),
    ] {
        let pool = fixture().await;
        sqlx::query("UPDATE slots SET datetime = ?")
            .bind(game)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO bookings (slot_id, user_id, position, is_waitlist) VALUES (1, 1, 1, ?)",
        )
        .bind(waitlist)
        .execute(&pool)
        .await
        .unwrap();
        record_cancellation_at(&pool, cancellation).await;
        let results: Vec<(i64, i64)> = sqlx::query_as(query).fetch_all(&pool).await.unwrap();
        let actual = results.first().map_or(0, |(_, count)| *count);
        assert_eq!(
            actual, expected,
            "{game}, {cancellation}, waitlist={waitlist}"
        );
    }
}
