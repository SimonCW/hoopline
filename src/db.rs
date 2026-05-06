use std::str::FromStr;

use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions, SqliteRow};
use sqlx::{Row, SqlitePool};

use crate::models::{Slot, UserIdentity};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SignupOutcome {
    AddedPlayer,
    AddedWaitlist,
    AlreadySignedUp,
    Full,
    SlotNotFound,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CancelOutcome {
    Cancelled,
    NotSignedUp,
    SlotNotFound,
}

/// Initializes a `SQLite` pool, enables foreign keys, and runs migrations.
///
/// # Errors
///
/// Returns an error if the database cannot be opened, PRAGMA execution fails,
/// or migrations cannot be applied.
pub async fn init_pool(database_url: &str) -> Result<SqlitePool, sqlx::Error> {
    let max_connections = if database_url == "sqlite::memory:" {
        1
    } else {
        5
    };

    let mut connect_options = SqliteConnectOptions::from_str(database_url)?;
    if database_url != "sqlite::memory:" {
        connect_options = connect_options.create_if_missing(true);
    }

    let pool = SqlitePoolOptions::new()
        .max_connections(max_connections)
        .connect_with(connect_options)
        .await?;

    sqlx::query("PRAGMA foreign_keys = ON;")
        .execute(&pool)
        .await?;
    sqlx::migrate!("./migrations").run(&pool).await?;

    Ok(pool)
}

/// Returns slots with booking names split into players and waitlist.
///
/// # Errors
///
/// Returns an error when the slot or booking query fails, or row decoding fails.
pub async fn list_slots(pool: &SqlitePool) -> Result<Vec<Slot>, sqlx::Error> {
    let rows = sqlx::query(
        r"
        SELECT
            s.id,
            s.datetime,
            s.venue,
            b.is_waitlist,
            u.name
        FROM slots s
        LEFT JOIN bookings b ON b.slot_id = s.id
        LEFT JOIN users u ON u.id = b.user_id
        ORDER BY s.datetime, s.id, b.is_waitlist, b.position
        ",
    )
    .fetch_all(pool)
    .await?;

    slots_from_rows(rows)
}

/// Returns a single slot with booking names split into players and waitlist.
///
/// # Errors
///
/// Returns an error when the slot query fails, or row decoding fails.
pub async fn find_slot(pool: &SqlitePool, slot_id: i64) -> Result<Option<Slot>, sqlx::Error> {
    let rows = sqlx::query(
        r"
        SELECT
            s.id,
            s.datetime,
            s.venue,
            b.is_waitlist,
            u.name
        FROM slots s
        LEFT JOIN bookings b ON b.slot_id = s.id
        LEFT JOIN users u ON u.id = b.user_id
        WHERE s.id = ?
        ORDER BY s.datetime, s.id, b.is_waitlist, b.position
        ",
    )
    .bind(slot_id)
    .fetch_all(pool)
    .await?;

    let mut slots = slots_from_rows(rows)?;
    Ok(slots.pop())
}

/// Signs up a user to either player list or waitlist for a slot.
///
/// # Errors
///
/// Returns an error when querying or writing booking data fails.
pub async fn signup_for_slot(
    pool: &SqlitePool,
    slot_id: i64,
    user_id: i64,
) -> Result<SignupOutcome, sqlx::Error> {
    let mut tx = pool.begin().await?;
    let slot_limits = sqlx::query("SELECT max_players, max_waitlist FROM slots WHERE id = ?")
        .bind(slot_id)
        .fetch_optional(&mut *tx)
        .await?;
    let Some(slot_limits) = slot_limits else {
        return Ok(SignupOutcome::SlotNotFound);
    };
    let max_players: i64 = slot_limits.try_get("max_players")?;
    let max_waitlist: i64 = slot_limits.try_get("max_waitlist")?;

    let already_signed_up: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM bookings WHERE slot_id = ? AND user_id = ?")
            .bind(slot_id)
            .bind(user_id)
            .fetch_one(&mut *tx)
            .await?;
    if already_signed_up > 0 {
        return Ok(SignupOutcome::AlreadySignedUp);
    }

    let player_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM bookings WHERE slot_id = ? AND is_waitlist = 0")
            .bind(slot_id)
            .fetch_one(&mut *tx)
            .await?;
    if player_count < max_players {
        sqlx::query(
            "INSERT INTO bookings (slot_id, user_id, position, is_waitlist) VALUES (?, ?, ?, 0)",
        )
        .bind(slot_id)
        .bind(user_id)
        .bind(player_count + 1)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        return Ok(SignupOutcome::AddedPlayer);
    }

    let waitlist_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM bookings WHERE slot_id = ? AND is_waitlist = 1")
            .bind(slot_id)
            .fetch_one(&mut *tx)
            .await?;
    if waitlist_count < max_waitlist {
        sqlx::query(
            "INSERT INTO bookings (slot_id, user_id, position, is_waitlist) VALUES (?, ?, ?, 1)",
        )
        .bind(slot_id)
        .bind(user_id)
        .bind(waitlist_count + 1)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        return Ok(SignupOutcome::AddedWaitlist);
    }

    Ok(SignupOutcome::Full)
}

/// Cancels a user's booking and promotes waitlist #1 when needed.
///
/// # Errors
///
/// Returns an error when querying or writing booking data fails.
pub async fn cancel_booking_for_slot(
    pool: &SqlitePool,
    slot_id: i64,
    user_id: i64,
) -> Result<CancelOutcome, sqlx::Error> {
    let mut tx = pool.begin().await?;
    let slot_exists: Option<i64> = sqlx::query_scalar("SELECT id FROM slots WHERE id = ?")
        .bind(slot_id)
        .fetch_optional(&mut *tx)
        .await?;
    if slot_exists.is_none() {
        return Ok(CancelOutcome::SlotNotFound);
    }

    let booking = sqlx::query(
        "SELECT id, position, is_waitlist FROM bookings WHERE slot_id = ? AND user_id = ?",
    )
    .bind(slot_id)
    .bind(user_id)
    .fetch_optional(&mut *tx)
    .await?;
    let Some(booking) = booking else {
        return Ok(CancelOutcome::NotSignedUp);
    };

    let booking_id: i64 = booking.try_get("id")?;
    let booking_position: i64 = booking.try_get("position")?;
    let is_waitlist: i64 = booking.try_get("is_waitlist")?;

    sqlx::query("DELETE FROM bookings WHERE id = ?")
        .bind(booking_id)
        .execute(&mut *tx)
        .await?;

    if is_waitlist == 1 {
        sqlx::query(
            "UPDATE bookings
             SET position = position - 1
             WHERE slot_id = ? AND is_waitlist = 1 AND position > ?",
        )
        .bind(slot_id)
        .bind(booking_position)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        return Ok(CancelOutcome::Cancelled);
    }

    sqlx::query(
        "UPDATE bookings
         SET position = position - 1
         WHERE slot_id = ? AND is_waitlist = 0 AND position > ?",
    )
    .bind(slot_id)
    .bind(booking_position)
    .execute(&mut *tx)
    .await?;

    let waitlist_top = sqlx::query(
        "SELECT id, position
         FROM bookings
         WHERE slot_id = ? AND is_waitlist = 1
         ORDER BY position ASC
         LIMIT 1",
    )
    .bind(slot_id)
    .fetch_optional(&mut *tx)
    .await?;
    if let Some(waitlist_top) = waitlist_top {
        let promoted_booking_id: i64 = waitlist_top.try_get("id")?;
        let promoted_waitlist_position: i64 = waitlist_top.try_get("position")?;

        let next_player_position: i64 = sqlx::query_scalar(
            "SELECT COALESCE(MAX(position), 0) + 1 FROM bookings WHERE slot_id = ? AND is_waitlist = 0",
        )
        .bind(slot_id)
        .fetch_one(&mut *tx)
        .await?;

        sqlx::query(
            "UPDATE bookings
             SET is_waitlist = 0, position = ?
             WHERE id = ?",
        )
        .bind(next_player_position)
        .bind(promoted_booking_id)
        .execute(&mut *tx)
        .await?;

        sqlx::query(
            "UPDATE bookings
             SET position = position - 1
             WHERE slot_id = ? AND is_waitlist = 1 AND position > ?",
        )
        .bind(slot_id)
        .bind(promoted_waitlist_position)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;
    Ok(CancelOutcome::Cancelled)
}

fn slots_from_rows(rows: Vec<SqliteRow>) -> Result<Vec<Slot>, sqlx::Error> {
    let mut slots = Vec::new();
    let mut current_slot: Option<Slot> = None;

    for row in rows {
        let slot_id: i64 = row.get("id");

        if current_slot.as_ref().map(|slot| slot.id) != Some(slot_id) {
            if let Some(finished) = current_slot.take() {
                slots.push(finished);
            }
            current_slot = Some(Slot {
                id: slot_id,
                datetime: row.get("datetime"),
                venue: row.get("venue"),
                players: Vec::new(),
                waitlist: Vec::new(),
            });
        }

        let maybe_name: Option<String> = row.try_get("name")?;
        if let Some(name) = maybe_name {
            let is_waitlist: i64 = row.get("is_waitlist");
            if let Some(slot) = current_slot.as_mut() {
                if is_waitlist == 1 {
                    slot.waitlist.push(name);
                } else {
                    slot.players.push(name);
                }
            }
        }
    }

    if let Some(finished) = current_slot {
        slots.push(finished);
    }

    Ok(slots)
}

/// Returns all users ordered by display name.
///
/// # Errors
///
/// Returns an error when user querying or row decoding fails.
pub async fn list_users(pool: &SqlitePool) -> Result<Vec<UserIdentity>, sqlx::Error> {
    let rows = sqlx::query("SELECT id, name FROM users ORDER BY name ASC")
        .fetch_all(pool)
        .await?;

    rows.into_iter()
        .map(|row| {
            Ok(UserIdentity {
                id: row.try_get("id")?,
                name: row.try_get("name")?,
            })
        })
        .collect()
}

/// Finds a user by id.
///
/// # Errors
///
/// Returns an error when querying fails.
pub async fn find_user_by_id(
    pool: &SqlitePool,
    user_id: i64,
) -> Result<Option<UserIdentity>, sqlx::Error> {
    let row = sqlx::query("SELECT id, name FROM users WHERE id = ?")
        .bind(user_id)
        .fetch_optional(pool)
        .await?;

    row.map(|record| {
        Ok(UserIdentity {
            id: record.try_get("id")?,
            name: record.try_get("name")?,
        })
    })
    .transpose()
}

/// Finds a user by display name.
///
/// # Errors
///
/// Returns an error when querying fails.
pub async fn find_user_by_name(
    pool: &SqlitePool,
    name: &str,
) -> Result<Option<UserIdentity>, sqlx::Error> {
    let row = sqlx::query("SELECT id, name FROM users WHERE name = ?")
        .bind(name)
        .fetch_optional(pool)
        .await?;

    row.map(|record| {
        Ok(UserIdentity {
            id: record.try_get("id")?,
            name: record.try_get("name")?,
        })
    })
    .transpose()
}

/// Creates a user with the provided display name.
///
/// # Errors
///
/// Returns an error when insertion fails.
pub async fn create_user(pool: &SqlitePool, name: &str) -> Result<UserIdentity, sqlx::Error> {
    sqlx::query("INSERT INTO users (name, is_admin) VALUES (?, ?)")
        .bind(name)
        .bind(0_i64)
        .execute(pool)
        .await?;

    let row = sqlx::query("SELECT id, name FROM users WHERE name = ?")
        .bind(name)
        .fetch_one(pool)
        .await?;

    Ok(UserIdentity {
        id: row.try_get("id")?,
        name: row.try_get("name")?,
    })
}

#[cfg(test)]
mod tests {
    use sqlx::Row;

    use super::init_pool;

    #[tokio::test]
    async fn insert_and_query_users_slots_and_bookings() {
        let pool = init_pool("sqlite::memory:")
            .await
            .expect("in-memory db should initialize");

        sqlx::query("INSERT INTO users (name, is_admin) VALUES (?, ?)")
            .bind("Zed")
            .bind(0_i64)
            .execute(&pool)
            .await
            .expect("user insert should succeed");

        sqlx::query(
            "INSERT INTO slots (datetime, venue, max_players, max_waitlist) VALUES (?, ?, ?, ?)",
        )
        .bind("2026-03-20T20:00:00Z")
        .bind("Court D")
        .bind(15_i64)
        .bind(5_i64)
        .execute(&pool)
        .await
        .expect("slot insert should succeed");

        let user_id: i64 = sqlx::query("SELECT id FROM users WHERE name = ?")
            .bind("Zed")
            .fetch_one(&pool)
            .await
            .expect("inserted user should exist")
            .get("id");

        let slot_id: i64 = sqlx::query("SELECT id FROM slots WHERE venue = ?")
            .bind("Court D")
            .fetch_one(&pool)
            .await
            .expect("inserted slot should exist")
            .get("id");

        sqlx::query(
            "INSERT INTO bookings (slot_id, user_id, position, is_waitlist) VALUES (?, ?, ?, ?)",
        )
        .bind(slot_id)
        .bind(user_id)
        .bind(1_i64)
        .bind(0_i64)
        .execute(&pool)
        .await
        .expect("booking insert should succeed");

        let booking_count: i64 =
            sqlx::query("SELECT COUNT(*) as count FROM bookings WHERE slot_id = ?")
                .bind(slot_id)
                .fetch_one(&pool)
                .await
                .expect("booking count query should succeed")
                .get("count");

        assert_eq!(booking_count, 1);
    }
}
