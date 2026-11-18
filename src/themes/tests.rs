use chrono::{DateTime, Utc};
use sqlx::SqlitePool;

use super::{awards, process_started_games};
use crate::db;

fn time(value: &str) -> DateTime<Utc> {
    value.parse().unwrap()
}

async fn data(sql: &str) -> SqlitePool {
    let pool = db::init_pool("sqlite::memory:").await.unwrap();
    sqlx::raw_sql(sql).execute(&pool).await.unwrap();
    pool
}

#[tokio::test]
async fn only_confirmed_players_without_a_berlin_game_day_cancellation_get_credit() {
    let pool = data(
        "INSERT INTO users (id, name) VALUES (1, 'Alex'), (2, 'Ben'), (3, 'Chris'), (4, 'Dani');
         INSERT INTO slots (id, datetime, venue) VALUES (1, '2099-07-01T22:30:00Z', 'Court');
         INSERT INTO bookings (slot_id, user_id, position, is_waitlist)
         VALUES (1, 1, 1, 0), (1, 2, 2, 0), (1, 3, 1, 1), (1, 4, 3, 0);
         INSERT INTO activity_log (occurred_at, occurred_local_date, kind, actor_user_id,
                                   actor_name, subject_user_id, subject_name, slot_id, details)
         VALUES ('2099-07-01T22:10:00Z', '2099-07-02', 'booking.cancelled', 1, 'Alex', 1, 'Alex', 1, '{}'),
                ('2099-07-01T21:50:00Z', '2099-07-01', 'booking.cancelled', 2, 'Ben', 2, 'Ben', 1, '{}'),
                ('2099-07-01T22:15:00Z', '2099-07-02', 'booking.cancelled', 1, 'Alex', 4, 'Dani', 1, '{}');",
    )
    .await;
    process_started_games(&pool, time("2099-07-01T22:29:59Z"))
        .await
        .unwrap();
    let progress = "SELECT user_id, qualifying_games FROM theme_progress ORDER BY user_id";
    let before: Vec<(i64, i64)> = sqlx::query_as(progress).fetch_all(&pool).await.unwrap();
    assert_eq!(before, []);
    process_started_games(&pool, time("2099-07-01T22:30:00Z"))
        .await
        .unwrap();
    let after: Vec<(i64, i64)> = sqlx::query_as(progress).fetch_all(&pool).await.unwrap();
    assert_eq!(after, [(2, 1), (4, 1)]);
}

#[tokio::test]
async fn backfill_ignores_known_late_bookings_and_late_promotions() {
    let pool = data(
        "INSERT INTO users (id, name) VALUES (1, 'Alex'), (2, 'Ben'), (3, 'Chris');
         INSERT INTO slots (id, datetime, venue) VALUES (1, '2099-07-01T22:30:00Z', 'Court');
         INSERT INTO bookings (id, slot_id, user_id, position, created_at) VALUES
             (1, 1, 1, 1, '2099-07-01T22:30:00Z'),
             (2, 1, 2, 2, '2099-07-01T22:00:00Z'),
             (3, 1, 3, 3, '2099-07-01T22:00:00Z');
         INSERT INTO activity_log (occurred_at, occurred_local_date, kind, actor_name,
                                   subject_user_id, subject_name, slot_id, details)
         VALUES ('2099-07-01T22:31:00Z', '2099-07-02', 'booking.promoted', 'Hoopline',
                 2, 'Ben', 1, '{\"booking_id\":2}');",
    )
    .await;
    process_started_games(&pool, time("2099-07-01T23:00:00Z"))
        .await
        .unwrap();
    let users: Vec<i64> = sqlx::query_scalar("SELECT user_id FROM theme_progress ORDER BY user_id")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(users, [3]);
}

#[tokio::test]
async fn credit_uses_tip_off_roster_whether_admin_or_worker_goes_first() {
    for worker_first in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let url = format!("sqlite://{}/themes.db", directory.path().display());
        let pool = db::init_pool(&url).await.unwrap();
        sqlx::raw_sql(
            "INSERT INTO users (id, name) VALUES (1, 'Alex'), (2, 'Ben');
             INSERT INTO slots (id, datetime, venue) VALUES (1, '2000-07-01T22:30:00Z', 'Court');
             INSERT INTO bookings (slot_id, user_id, position, is_waitlist, created_at) VALUES
               (1, 1, 1, 0, '2000-07-01T21:00:00Z'),
               (1, 2, 1, 1, '2000-07-01T21:00:00Z');",
        )
        .execute(&pool)
        .await
        .unwrap();

        assert_eq!(
            db::cancel_booking_for_slot(&pool, 1, 1).await.unwrap(),
            db::CancelOutcome::Started
        );
        if worker_first {
            process_started_games(&pool, Utc::now()).await.unwrap();
        }
        assert_eq!(
            db::cancel_booking_checked(&pool, 1, 1, None).await.unwrap(),
            db::CancelOutcome::Cancelled
        );
        process_started_games(&pool, Utc::now()).await.unwrap();
        let users: Vec<i64> =
            sqlx::query_scalar("SELECT user_id FROM theme_progress ORDER BY user_id")
                .fetch_all(&pool)
                .await
                .unwrap();
        assert_eq!(users, [1]);
        assert_eq!(
            db::find_slot(&pool, 1)
                .await
                .unwrap()
                .unwrap()
                .player_user_ids,
            [2]
        );
    }
}

#[tokio::test]
async fn milestones_award_each_theme_once_and_log_the_game_that_earned_it() {
    let pool = data(
        "INSERT INTO users (id, name) VALUES (1, 'Alex'), (2, 'Ben');
         WITH RECURSIVE numbers(n) AS (VALUES(1) UNION ALL SELECT n+1 FROM numbers WHERE n<21)
         INSERT INTO slots (id, datetime, venue)
         SELECT n, printf('2099-07-%02dT18:00:00Z', n), 'Court' FROM numbers;
         INSERT INTO bookings (slot_id, user_id, position)
         SELECT id, 1, 1 FROM slots WHERE id BETWEEN 1 AND 21;",
    )
    .await;
    for (day, count) in [(4, 0), (5, 1), (10, 2), (15, 4), (20, 7), (21, 7)] {
        let now = time(&format!("2099-07-{day:02}T18:00:00Z"));
        let (first, second) = tokio::join!(
            process_started_games(&pool, now),
            process_started_games(&pool, now),
        );
        first.unwrap();
        second.unwrap();
        assert_eq!(awards(&pool, 1).await.unwrap().len(), count);
    }
    assert_eq!(awards(&pool, 2).await.unwrap().len(), 0);
    let history: Vec<(i64, String, i64)> = sqlx::query_as(
        "SELECT slot_id, json_extract(details, '$.theme'), json_extract(details, '$.qualifying_games')
         FROM activity_log WHERE kind = 'theme.unlocked' AND subject_name = 'Alex' ORDER BY id",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        history
            .iter()
            .map(|(slot, id, games)| (*slot, id.as_str(), *games))
            .collect::<Vec<_>>(),
        [
            (5, "rockets", 5),
            (10, "alba", 10),
            (15, "bulls", 15),
            (15, "germany", 15),
            (20, "dirk", 20),
            (20, "bird", 20),
            (20, "knicks", 20)
        ]
    );
}
