//! Award permanent themes for confirmed roster spots without game-day cancellations.

use chrono::{DateTime, SecondsFormat, Utc};
use chrono_tz::Europe::Berlin;
use sqlx::{Sqlite, SqlitePool, Transaction};

struct Theme {
    id: &'static str,
    name: &'static str,
    milestone: i64,
}

const THEMES: [Theme; 7] = [
    Theme {
        id: "rockets",
        name: "Houston · 2005",
        milestone: 5,
    },
    Theme {
        id: "alba",
        name: "ALBA Berlin · Deutscher Basketball Meister 2026",
        milestone: 10,
    },
    Theme {
        id: "bulls",
        name: "Chicago · 1995–96",
        milestone: 15,
    },
    Theme {
        id: "germany",
        name: "Deutschland · 2025",
        milestone: 15,
    },
    Theme {
        id: "dirk",
        name: "Dirk · 2011",
        milestone: 20,
    },
    Theme {
        id: "bird",
        name: "Larry Bird · Boston",
        milestone: 20,
    },
    Theme {
        id: "knicks",
        name: "New York · 2026",
        milestone: 20,
    },
];

pub(crate) struct Award {
    pub(crate) id: &'static str,
    pub(crate) name: &'static str,
    pub(crate) milestone: i64,
    pub(crate) activity_id: i64,
}

pub(crate) async fn awards(pool: &SqlitePool, user_id: i64) -> Result<Vec<Award>, sqlx::Error> {
    let unlocked: Vec<(String, i64)> =
        sqlx::query_as("SELECT theme, activity_id FROM theme_unlocks WHERE user_id = ?")
            .bind(user_id)
            .fetch_all(pool)
            .await?;
    Ok(THEMES
        .iter()
        .filter_map(|theme| {
            let (_, activity_id) = unlocked.iter().find(|(id, _)| id == theme.id)?;
            Some(Award {
                id: theme.id,
                name: theme.name,
                milestone: theme.milestone,
                activity_id: *activity_id,
            })
        })
        .collect())
}

pub(crate) async fn process_started_games(
    pool: &SqlitePool,
    now: DateTime<Utc>,
) -> Result<(), sqlx::Error> {
    let now_text = now.to_rfc3339_opts(SecondsFormat::Micros, true);
    // Preview the indexed queue without taking a write lock during idle ticks.
    let due: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM slots
         WHERE theme_progress_processed = 0 AND julianday(datetime) <= julianday(?))",
    )
    .bind(&now_text)
    .fetch_one(pool)
    .await?;
    if !due {
        return Ok(());
    }

    loop {
        let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
        let more = process_batch(&mut tx, now).await?;
        tx.commit().await?;
        if !more {
            return Ok(());
        }
    }
}

/// Save a started roster before an admin changes it.
pub(crate) async fn settle_before_roster_edit(
    tx: &mut Transaction<'_, Sqlite>,
    slot_id: i64,
    now: DateTime<Utc>,
) -> Result<(), sqlx::Error> {
    let due: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM slots WHERE id = ?
         AND theme_progress_processed = 0 AND julianday(datetime) <= julianday(?))",
    )
    .bind(slot_id)
    .bind(now.to_rfc3339_opts(SecondsFormat::Micros, true))
    .fetch_one(&mut **tx)
    .await?;
    // Use the same order as the worker so a milestone points to the correct game.
    if due {
        while process_batch(tx, now).await? {}
    }
    Ok(())
}

async fn process_batch(
    tx: &mut Transaction<'_, Sqlite>,
    now: DateTime<Utc>,
) -> Result<bool, sqlx::Error> {
    const BATCH_SIZE: u16 = 50;
    let now_text = now.to_rfc3339_opts(SecondsFormat::Micros, true);
    let slots: Vec<(i64, String)> = sqlx::query_as(
        "SELECT id, datetime FROM slots
         WHERE theme_progress_processed = 0 AND julianday(datetime) <= julianday(?)
         ORDER BY julianday(datetime), id LIMIT ?",
    )
    .bind(&now_text)
    .bind(BATCH_SIZE)
    .fetch_all(&mut **tx)
    .await?;
    let more = slots.len() == usize::from(BATCH_SIZE);
    let occurred_date = now.with_timezone(&Berlin).date_naive().to_string();
    for (slot_id, starts_at) in slots {
        let slot_date = DateTime::parse_from_rfc3339(&starts_at)
            .map_err(|error| sqlx::Error::Decode(Box::new(error)))?
            .with_timezone(&Berlin)
            .date_naive()
            .to_string();
        let progress: Vec<(i64, i64)> = sqlx::query_as(
            "INSERT INTO theme_progress (user_id, qualifying_games)
             SELECT b.user_id, 1 FROM bookings b
             WHERE b.slot_id = ? AND b.is_waitlist = 0
               AND julianday(b.created_at) < julianday(?)
               AND NOT EXISTS (
                   SELECT 1 FROM activity_log a
                   WHERE a.slot_id = b.slot_id AND a.subject_user_id = b.user_id
                     AND a.actor_user_id = b.user_id AND a.kind = 'booking.cancelled'
                     AND a.occurred_local_date = ?
               )
               AND NOT EXISTS (
                   SELECT 1 FROM activity_log a
                   WHERE a.slot_id = b.slot_id AND a.subject_user_id = b.user_id
                     AND a.kind = 'booking.promoted' AND json_extract(a.details, '$.booking_id') = b.id
                     AND julianday(a.occurred_at) >= julianday(?)
               )
             ON CONFLICT(user_id) DO UPDATE SET qualifying_games = qualifying_games + 1
             RETURNING user_id, qualifying_games",
        )
        .bind(slot_id)
        .bind(&starts_at)
        .bind(&slot_date)
        .bind(&starts_at)
        .fetch_all(&mut **tx)
        .await?;

        for (user_id, games) in progress {
            for theme in THEMES.iter().filter(|theme| theme.milestone == games) {
                record_award(
                    tx,
                    theme,
                    user_id,
                    slot_id,
                    [&now_text, &occurred_date, &slot_date],
                )
                .await?;
            }
        }
        sqlx::query("UPDATE slots SET theme_progress_processed = 1 WHERE id = ?")
            .bind(slot_id)
            .execute(&mut **tx)
            .await?;
    }
    Ok(more)
}

async fn record_award(
    tx: &mut Transaction<'_, Sqlite>,
    theme: &Theme,
    user_id: i64,
    slot_id: i64,
    [occurred_at, occurred_date, slot_date]: [&str; 3],
) -> Result<(), sqlx::Error> {
    let activity_id: i64 = sqlx::query_scalar(
        "INSERT INTO activity_log
         (occurred_at, occurred_local_date, kind, actor_name, subject_user_id, subject_name,
          slot_id, slot_starts_at, slot_local_date, venue, details)
         SELECT ?, ?, 'theme.unlocked', 'Hoopline', u.id, u.name,
                s.id, s.datetime, ?, s.venue, json_object('theme', ?, 'qualifying_games', ?)
         FROM users u CROSS JOIN slots s WHERE u.id = ? AND s.id = ?
         RETURNING id",
    )
    .bind(occurred_at)
    .bind(occurred_date)
    .bind(slot_date)
    .bind(theme.id)
    .bind(theme.milestone)
    .bind(user_id)
    .bind(slot_id)
    .fetch_one(&mut **tx)
    .await?;
    sqlx::query("INSERT INTO theme_unlocks (user_id, theme, activity_id) VALUES (?, ?, ?)")
        .bind(user_id)
        .bind(theme.id)
        .bind(activity_id)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests;
