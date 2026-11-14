use std::{str::FromStr, time::Duration};

use chrono::{DateTime, Datelike, Days, NaiveTime, SecondsFormat, TimeZone, Utc};
use chrono_tz::Europe::Berlin;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions, SqliteRow};
use sqlx::{Row, SqlitePool};

use crate::{
    activity::{ActivityKind, BookingActivity, record_booking},
    error::AppError,
    models::{Slot, UserIdentity},
};

#[derive(Clone, Copy, Debug, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RosterList {
    Players,
    Waitlist,
}

#[derive(Clone, Copy, Debug, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum MoveDirection {
    Up,
    Down,
}

async fn checked_admin_slot(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    slot_id: i64,
    revision: i64,
) -> Result<(i64, i64), AppError> {
    let row = sqlx::query("SELECT revision, max_players, max_waitlist FROM slots WHERE id = ?")
        .bind(slot_id)
        .fetch_optional(&mut **tx)
        .await?
        .ok_or_else(|| AppError::BadRequest("Game does not exist.".into()))?;
    if row.try_get::<i64, _>("revision")? != revision {
        return Err(AppError::StaleSlot);
    }
    Ok((row.try_get("max_players")?, row.try_get("max_waitlist")?))
}

pub(crate) async fn admin_add_member(
    pool: &SqlitePool,
    slot_id: i64,
    user_id: i64,
    revision: i64,
    list: RosterList,
) -> Result<(), AppError> {
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    let (max_players, max_waitlist) = checked_admin_slot(&mut tx, slot_id, revision).await?;
    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM users WHERE id = ?)")
        .bind(user_id)
        .fetch_one(&mut *tx)
        .await?;
    if !exists {
        return Err(AppError::BadRequest("Member does not exist.".into()));
    }
    let duplicate: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM bookings WHERE slot_id = ? AND user_id = ?)",
    )
    .bind(slot_id)
    .bind(user_id)
    .fetch_one(&mut *tx)
    .await?;
    if duplicate {
        return Err(AppError::BadRequest(
            "Member is already on this game.".into(),
        ));
    }
    let (is_waitlist, capacity) = match list {
        RosterList::Players => (0_i64, max_players),
        RosterList::Waitlist => (1_i64, max_waitlist),
    };
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM bookings WHERE slot_id = ? AND is_waitlist = ?")
            .bind(slot_id)
            .bind(is_waitlist)
            .fetch_one(&mut *tx)
            .await?;
    if count >= capacity {
        return Err(AppError::BadRequest("The selected list is full.".into()));
    }
    sqlx::query(
        "INSERT INTO bookings (slot_id, user_id, position, is_waitlist, created_at) VALUES (?, ?, ?, ?, ?)",
    )
    .bind(slot_id)
    .bind(user_id)
    .bind(count + 1)
    .bind(is_waitlist)
    .bind(crate::clock::now().to_rfc3339_opts(SecondsFormat::Micros, true))
    .execute(&mut *tx)
    .await?;
    increment_slot_revision(&mut tx, slot_id).await?;
    tx.commit().await?;
    Ok(())
}

pub(crate) async fn admin_move_waiter(
    pool: &SqlitePool,
    slot_id: i64,
    user_id: i64,
    revision: i64,
    direction: MoveDirection,
) -> Result<(), AppError> {
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    checked_admin_slot(&mut tx, slot_id, revision).await?;
    let position: i64 = sqlx::query_scalar(
        "SELECT position FROM bookings WHERE slot_id = ? AND user_id = ? AND is_waitlist = 1",
    )
    .bind(slot_id)
    .bind(user_id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| AppError::BadRequest("Member is not on this waitlist.".into()))?;
    let other = position
        + match direction {
            MoveDirection::Up => -1,
            MoveDirection::Down => 1,
        };
    let other_id: i64 = sqlx::query_scalar(
        "SELECT id FROM bookings WHERE slot_id = ? AND is_waitlist = 1 AND position = ?",
    )
    .bind(slot_id)
    .bind(other)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| AppError::BadRequest("Member cannot move further in that direction.".into()))?;
    sqlx::query("UPDATE bookings SET position = ? WHERE id = ?")
        .bind(position)
        .bind(other_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("UPDATE bookings SET position = ? WHERE slot_id = ? AND user_id = ?")
        .bind(other)
        .bind(slot_id)
        .bind(user_id)
        .execute(&mut *tx)
        .await?;
    increment_slot_revision(&mut tx, slot_id).await?;
    tx.commit().await?;
    Ok(())
}

pub(crate) struct GameEdit {
    pub slot_id: i64,
    pub revision: i64,
    pub datetime: DateTime<Utc>,
    pub venue: String,
    pub map_url: Option<String>,
    pub clear_bookings: Option<bool>,
}

pub(crate) async fn edit_game(pool: &SqlitePool, edit: &GameEdit) -> Result<(), AppError> {
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    checked_admin_slot(&mut tx, edit.slot_id, edit.revision).await?;
    if edit.datetime <= crate::clock::now() {
        return Err(AppError::BadRequest("Choose a future time.".into()));
    }
    let row = sqlx::query("SELECT datetime, venue, map_url, schedule_id FROM slots WHERE id = ?")
        .bind(edit.slot_id)
        .fetch_one(&mut *tx)
        .await?;
    let old_datetime: String = row.try_get("datetime")?;
    let old_venue: String = row.try_get("venue")?;
    let old_map_url: String = row.try_get("map_url")?;
    let map_url = edit.map_url.as_deref().unwrap_or(&old_map_url);
    let old = DateTime::parse_from_rfc3339(&old_datetime)
        .map_err(|_| AppError::BadRequest("The existing game time is invalid.".into()))?;
    if old.with_timezone(&Utc) <= crate::clock::now() {
        return Err(AppError::BadRequest(
            "This game has already started.".into(),
        ));
    }
    if old.with_timezone(&Utc) == edit.datetime && old_venue == edit.venue {
        if old_map_url != map_url {
            sqlx::query("UPDATE slots SET map_url = ?, revision = revision + 1 WHERE id = ?")
                .bind(map_url)
                .bind(edit.slot_id)
                .execute(&mut *tx)
                .await?;
            tx.commit().await?;
        }
        return Ok(());
    }
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM bookings WHERE slot_id = ?")
        .bind(edit.slot_id)
        .fetch_one(&mut *tx)
        .await?;
    if count > 0 && edit.clear_bookings.is_none() {
        return Err(AppError::BadRequest(
            "Choose whether to keep or clear bookings.".into(),
        ));
    }
    let collision: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM slots WHERE id != ? AND venue = ? AND julianday(datetime) = julianday(?))")
        .bind(edit.slot_id).bind(&edit.venue).bind(edit.datetime.to_rfc3339())
        .fetch_one(&mut *tx).await?;
    if collision {
        return Err(AppError::BadRequest(
            "A game already exists at this date, time, and location.".into(),
        ));
    }
    if row.try_get::<Option<i64>, _>("schedule_id")?.is_none() {
        sqlx::query(
            "INSERT OR IGNORE INTO slot_generation_exclusions (venue, local_date) VALUES (?, ?)",
        )
        .bind(&old_venue)
        .bind(old.with_timezone(&Berlin).date_naive().to_string())
        .execute(&mut *tx)
        .await?;
    }
    if edit.clear_bookings == Some(true) {
        sqlx::query("DELETE FROM bookings WHERE slot_id = ?")
            .bind(edit.slot_id)
            .execute(&mut *tx)
            .await?;
    }
    sqlx::query("UPDATE slots SET datetime = ?, venue = ?, map_url = ?, is_override = 1, revision = revision + 1 WHERE id = ?")
        .bind(edit.datetime.to_rfc3339_opts(SecondsFormat::Secs, true))
        .bind(&edit.venue).bind(map_url).bind(edit.slot_id).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SignupOutcome {
    AddedPlayer,
    AddedWaitlist,
    AlreadySignedUp,
    Full,
    SlotNotFound,
    Started,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CancelOutcome {
    Cancelled,
    NotSignedUp,
    SlotNotFound,
    Stale,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PromoteOutcome {
    Promoted,
    NotWaitlisted,
    PlayerListFull,
    SlotNotFound,
    Stale,
}

#[derive(Clone, Debug)]
struct SlotSchedule {
    id: i64,
    weekday: i64,
    time_local: String,
    venue: String,
    map_url: String,
    max_players: i64,
    max_waitlist: i64,
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

    let mut connect_options = SqliteConnectOptions::from_str(database_url)?
        .foreign_keys(true)
        .busy_timeout(Duration::from_secs(5));
    if database_url != "sqlite::memory:" {
        connect_options = connect_options.create_if_missing(true);
    }

    let mut pool_options = SqlitePoolOptions::new().max_connections(max_connections);
    if database_url == "sqlite::memory:" {
        // Closing the only connection destroys the in-memory database.
        pool_options = pool_options.idle_timeout(None).max_lifetime(None);
    }
    let pool = pool_options.connect_with(connect_options).await?;

    sqlx::query("PRAGMA foreign_keys = ON;")
        .execute(&pool)
        .await?;
    let new_database: bool = sqlx::query_scalar(
        "SELECT NOT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = '_sqlx_migrations')",
    )
    .fetch_one(&pool)
    .await?;
    sqlx::migrate!("./migrations").run(&pool).await?;
    if new_database {
        let mut tx = pool.begin().await?;
        sqlx::query("DELETE FROM bookings")
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM slots").execute(&mut *tx).await?;
        sqlx::query("DELETE FROM users").execute(&mut *tx).await?;
        tx.commit().await?;
    }

    Ok(pool)
}

/// Creates slots that have opened under the Sunday release rule.
///
/// # Errors
///
/// Returns an error when schedule or slot write queries fail.
pub async fn create_upcoming_slots(
    pool: &SqlitePool,
    now: DateTime<Utc>,
) -> Result<i64, sqlx::Error> {
    let report = create_upcoming_slots_report(pool, now).await?;
    for warning in &report.warnings {
        eprintln!("slot generation: {warning}");
    }
    Ok(report.created)
}

/// Adds a full example roster once to an empty preview database.
///
/// # Errors
/// Returns an error if the example data cannot be written.
pub(crate) async fn seed_preview_roster(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    let occupied: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM preview_seed) OR EXISTS(SELECT 1 FROM users) OR EXISTS(SELECT 1 FROM bookings)",
    )
    .fetch_one(&mut *tx)
    .await?;
    if occupied {
        return Ok(());
    }
    let slot_id: Option<i64> = sqlx::query_scalar(
        "SELECT id FROM slots WHERE julianday(datetime) > julianday(?) ORDER BY julianday(datetime) DESC, id DESC LIMIT 1",
    )
    .bind(crate::clock::now().to_rfc3339())
    .fetch_optional(&mut *tx)
    .await?;
    let Some(slot_id) = slot_id else {
        return Ok(());
    };
    sqlx::query("UPDATE slots SET max_players = 15, max_waitlist = 5 WHERE id = ?")
        .bind(slot_id)
        .execute(&mut *tx)
        .await?;
    let names = [
        "Amir Hassan",
        "Ben Fischer",
        "Chris Weber",
        "Daniel Müller",
        "Emil Becker",
        "Farid Khan",
        "Gabriel Santos",
        "Hana Kim",
        "Isaac Brown",
        "Jonas Schmidt",
        "Kai Nguyen",
        "Leon Wagner",
        "Maya Johnson",
        "Nina Bauer",
        "Omar Ali",
        "Paul Meyer",
        "Samira Ahmed",
        "Theo Martin",
    ];
    for (offset, name) in names.iter().enumerate() {
        let index = i64::try_from(offset).expect("preview roster index fits in i64") + 1;
        let user_id: i64 = sqlx::query_scalar("INSERT INTO users (name) VALUES (?) RETURNING id")
            .bind(name)
            .fetch_one(&mut *tx)
            .await?;
        sqlx::query(
            "INSERT INTO bookings (slot_id, user_id, position, is_waitlist) VALUES (?, ?, ?, ?)",
        )
        .bind(slot_id)
        .bind(user_id)
        .bind(if index <= 15 { index } else { index - 15 })
        .bind(i64::from(index > 15))
        .execute(&mut *tx)
        .await?;
    }
    sqlx::query("INSERT INTO preview_seed (id) VALUES (1)")
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

pub(crate) struct GenerationReport {
    pub created: i64,
    pub warnings: Vec<String>,
}

pub(crate) async fn create_upcoming_slots_report(
    pool: &SqlitePool,
    now: DateTime<Utc>,
) -> Result<GenerationReport, sqlx::Error> {
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    let schedules = list_active_slot_schedules(&mut tx).await?;
    if schedules.is_empty() {
        return Ok(GenerationReport {
            created: 0,
            warnings: Vec::new(),
        });
    }

    let release: String =
        sqlx::query_scalar("SELECT sunday_release_time FROM schedule_settings WHERE id = 1")
            .fetch_one(&mut *tx)
            .await?;
    let release_time = NaiveTime::parse_from_str(&release, "%H:%M")
        .map_err(|error| sqlx::Error::Protocol(error.to_string()))?;
    let last_date = crate::scheduling::latest_open_date(now, release_time)
        .map_err(|error| sqlx::Error::Protocol(error.into()))?;
    let mut created_count = 0_i64;
    let mut warnings = Vec::new();
    let start_date = now.with_timezone(&Berlin).date_naive();

    for day_offset in 0_u64..21 {
        let Some(target_date) = start_date.checked_add_days(Days::new(day_offset)) else {
            continue;
        };
        if target_date > last_date {
            break;
        }
        let target_weekday = weekday_to_number(target_date.weekday());
        for schedule in &schedules {
            if schedule.weekday != target_weekday {
                continue;
            }
            let time = NaiveTime::parse_from_str(&schedule.time_local, "%H:%M")
                .map_err(|error| sqlx::Error::Protocol(error.to_string()))?;
            let Some(local) = Berlin
                .from_local_datetime(&target_date.and_time(time))
                .single()
            else {
                warnings.push(format!(
                    "Skipped {} on {target_date}: local time is invalid or ambiguous.",
                    schedule.venue
                ));
                continue;
            };
            if local <= now {
                continue;
            }
            let datetime = local
                .with_timezone(&Utc)
                .to_rfc3339_opts(SecondsFormat::Secs, true);
            let collision: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM slots WHERE venue = ? AND julianday(datetime) = julianday(?) AND schedule_id IS NOT NULL AND NOT (schedule_id = ? AND occurrence_date = ?))")
                .bind(&schedule.venue).bind(&datetime).bind(schedule.id).bind(target_date.to_string())
                .fetch_one(&mut *tx).await?;
            if collision {
                warnings.push(format!(
                    "Skipped {} on {target_date}: another game uses this time and location.",
                    schedule.venue
                ));
                continue;
            }
            // Keep existing booked slots, including slots created under the old UTC schedule.
            let day_start = Berlin
                .from_local_datetime(&target_date.and_time(NaiveTime::MIN))
                .single()
                .ok_or_else(|| sqlx::Error::Protocol("invalid local date".into()))?;
            let day_end = Berlin
                .from_local_datetime(&(target_date + Days::new(1)).and_time(NaiveTime::MIN))
                .single()
                .ok_or_else(|| sqlx::Error::Protocol("invalid local date".into()))?;
            let result = sqlx::query(
                "INSERT INTO slots (datetime, venue, map_url, max_players, max_waitlist, schedule_id, occurrence_date)
                 SELECT ?, ?, ?, ?, ?, ?, ? WHERE NOT EXISTS (
                     SELECT 1 FROM slots WHERE schedule_id IS NULL AND venue = ?
                     AND julianday(datetime) >= julianday(?) AND julianday(datetime) < julianday(?)
                 ) AND NOT EXISTS (
                     SELECT 1 FROM slot_generation_exclusions WHERE venue = ? AND local_date = ?
                 ) ON CONFLICT DO NOTHING",
            )
            .bind(datetime)
            .bind(&schedule.venue)
            .bind(&schedule.map_url)
            .bind(schedule.max_players)
            .bind(schedule.max_waitlist)
            .bind(schedule.id)
            .bind(target_date.to_string())
            .bind(&schedule.venue)
            .bind(day_start.to_rfc3339())
            .bind(day_end.to_rfc3339())
            .bind(&schedule.venue)
            .bind(target_date.to_string())
            .execute(&mut *tx)
            .await?;
            created_count += i64::try_from(result.rows_affected()).unwrap_or_default();
        }
    }

    tx.commit().await?;
    Ok(GenerationReport {
        created: created_count,
        warnings,
    })
}

/// Returns slots with booking names split into players and waitlist.
///
/// # Errors
///
/// Returns an error when the slot or booking query fails, or row decoding fails.
pub async fn list_slots(pool: &SqlitePool) -> Result<Vec<Slot>, sqlx::Error> {
    list_slots_at(pool, crate::clock::now()).await
}

/// Returns slots from the current Berlin calendar day onward.
///
/// # Errors
/// Returns an error when querying or decoding slots fails.
pub async fn list_slots_at(
    pool: &SqlitePool,
    now: DateTime<Utc>,
) -> Result<Vec<Slot>, sqlx::Error> {
    let rows = sqlx::query(
        r"
        SELECT
            s.id,
            s.revision,
            s.is_override,
            s.datetime,
            s.venue,
            s.map_url,
            s.max_players,
            s.max_waitlist,
            b.is_waitlist,
            b.user_id,
            u.name
        FROM slots s
        LEFT JOIN bookings b ON b.slot_id = s.id
        LEFT JOIN users u ON u.id = b.user_id
        WHERE julianday(s.datetime) >= julianday(?)
        ORDER BY julianday(s.datetime), s.id, b.is_waitlist, b.position
        ",
    )
    .bind(
        Berlin
            .from_local_datetime(
                &now.with_timezone(&Berlin)
                    .date_naive()
                    .and_time(NaiveTime::MIN),
            )
            .single()
            .ok_or_else(|| sqlx::Error::Protocol("invalid Berlin day start".into()))?
            .to_rfc3339(),
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
            s.revision,
            s.is_override,
            s.datetime,
            s.venue,
            s.map_url,
            s.max_players,
            s.max_waitlist,
            b.is_waitlist,
            b.user_id,
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
    Ok(signup_with_push(pool, slot_id, user_id, None).await?.0)
}

pub(crate) async fn signup_with_push(
    pool: &SqlitePool,
    slot_id: i64,
    user_id: i64,
    binding: Option<&crate::push::Binding>,
) -> Result<(SignupOutcome, Option<crate::push::BindOutcome>), sqlx::Error> {
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    let slot_limits = sqlx::query("SELECT max_players, max_waitlist, julianday(datetime) <= julianday(?) AS started FROM slots WHERE id = ?")
        .bind(crate::clock::now().to_rfc3339())
        .bind(slot_id)
        .fetch_optional(&mut *tx)
        .await?;
    let Some(slot_limits) = slot_limits else {
        return Ok((SignupOutcome::SlotNotFound, None));
    };
    if slot_limits.try_get::<i64, _>("started")? != 0 {
        return Ok((SignupOutcome::Started, None));
    }
    let max_players: i64 = slot_limits.try_get("max_players")?;
    let max_waitlist: i64 = slot_limits.try_get("max_waitlist")?;

    let already_signed_up: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM bookings WHERE slot_id = ? AND user_id = ?")
            .bind(slot_id)
            .bind(user_id)
            .fetch_one(&mut *tx)
            .await?;
    if already_signed_up > 0 {
        return Ok((SignupOutcome::AlreadySignedUp, None));
    }

    let player_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM bookings WHERE slot_id = ? AND is_waitlist = 0")
            .bind(slot_id)
            .fetch_one(&mut *tx)
            .await?;
    if player_count < max_players {
        sqlx::query(
            "INSERT INTO bookings (slot_id, user_id, position, is_waitlist, created_at) VALUES (?, ?, ?, 0, ?)",
        )
        .bind(slot_id)
        .bind(user_id)
        .bind(player_count + 1)
        .bind(crate::clock::now().to_rfc3339_opts(SecondsFormat::Micros, true))
        .execute(&mut *tx)
        .await?;
        increment_slot_revision(&mut tx, slot_id).await?;
        record_booking(
            &mut tx,
            BookingActivity {
                kind: ActivityKind::SignedUp,
                actor_user_id: user_id,
                subject_user_id: user_id,
                slot_id,
                caused_by_activity_id: None,
            },
        )
        .await?;
        tx.commit().await?;
        return Ok((SignupOutcome::AddedPlayer, None));
    }

    let waitlist_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM bookings WHERE slot_id = ? AND is_waitlist = 1")
            .bind(slot_id)
            .fetch_one(&mut *tx)
            .await?;
    if waitlist_count < max_waitlist {
        sqlx::query(
            "INSERT INTO bookings (slot_id, user_id, position, is_waitlist, created_at) VALUES (?, ?, ?, 1, ?)",
        )
        .bind(slot_id)
        .bind(user_id)
        .bind(waitlist_count + 1)
        .bind(crate::clock::now().to_rfc3339_opts(SecondsFormat::Micros, true))
        .execute(&mut *tx)
        .await?;
        increment_slot_revision(&mut tx, slot_id).await?;
        record_booking(
            &mut tx,
            BookingActivity {
                kind: ActivityKind::SignedUp,
                actor_user_id: user_id,
                subject_user_id: user_id,
                slot_id,
                caused_by_activity_id: None,
            },
        )
        .await?;
        let binding_result = if let Some(binding) = binding {
            Some(crate::push::bind(&mut tx, user_id, binding).await?)
        } else {
            None
        };
        tx.commit().await?;
        return Ok((SignupOutcome::AddedWaitlist, binding_result));
    }

    Ok((SignupOutcome::Full, None))
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
    cancel_booking(pool, slot_id, user_id, None, Some(user_id)).await
}

pub(crate) async fn cancel_booking_checked(
    pool: &SqlitePool,
    slot_id: i64,
    user_id: i64,
    revision: Option<i64>,
) -> Result<CancelOutcome, sqlx::Error> {
    // This entry point is for administrative removal, not player cancellation.
    cancel_booking(pool, slot_id, user_id, revision, None).await
}

async fn cancel_booking(
    pool: &SqlitePool,
    slot_id: i64,
    user_id: i64,
    revision: Option<i64>,
    actor_user_id: Option<i64>,
) -> Result<CancelOutcome, sqlx::Error> {
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    if let Some(outcome) = cancellation_slot_status(&mut tx, slot_id, revision).await? {
        return Ok(outcome);
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
    let (booking_id, booking_position, is_waitlist): (i64, i64, i64) = (
        booking.try_get("id")?,
        booking.try_get("position")?,
        booking.try_get("is_waitlist")?,
    );

    let cancellation_id = if let Some(actor_user_id) = actor_user_id {
        Some(
            record_booking(
                &mut tx,
                BookingActivity {
                    kind: ActivityKind::Cancelled,
                    actor_user_id,
                    subject_user_id: user_id,
                    slot_id,
                    caused_by_activity_id: None,
                },
            )
            .await?,
        )
    } else {
        None
    };

    sqlx::query("DELETE FROM bookings WHERE id = ?")
        .bind(booking_id)
        .execute(&mut *tx)
        .await?;

    sqlx::query(
        "UPDATE bookings
         SET position = position - 1
         WHERE slot_id = ? AND is_waitlist = ? AND position > ?",
    )
    .bind(slot_id)
    .bind(is_waitlist)
    .bind(booking_position)
    .execute(&mut *tx)
    .await?;

    if is_waitlist == 0 {
        promote_first_waiter(&mut tx, slot_id, actor_user_id, cancellation_id).await?;
    }
    increment_slot_revision(&mut tx, slot_id).await?;
    tx.commit().await?;
    Ok(CancelOutcome::Cancelled)
}

async fn promote_first_waiter(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    slot_id: i64,
    actor_user_id: Option<i64>,
    cancellation_id: Option<i64>,
) -> Result<(), sqlx::Error> {
    let waitlist_top = sqlx::query(
        "SELECT id, position, user_id
         FROM bookings
         WHERE slot_id = ? AND is_waitlist = 1
         ORDER BY position ASC
         LIMIT 1",
    )
    .bind(slot_id)
    .fetch_optional(&mut **tx)
    .await?;
    if let Some(waitlist_top) = waitlist_top {
        if let Some(actor_user_id) = actor_user_id {
            record_booking(
                tx,
                BookingActivity {
                    kind: ActivityKind::Promoted,
                    actor_user_id,
                    subject_user_id: waitlist_top.try_get("user_id")?,
                    slot_id,
                    caused_by_activity_id: cancellation_id,
                },
            )
            .await?;
        }
        let promoted_booking_id: i64 = waitlist_top.try_get("id")?;
        let promoted_waitlist_position: i64 = waitlist_top.try_get("position")?;

        let next_player_position: i64 = sqlx::query_scalar(
            "SELECT COALESCE(MAX(position), 0) + 1 FROM bookings WHERE slot_id = ? AND is_waitlist = 0",
        )
        .bind(slot_id)
        .fetch_one(&mut **tx)
        .await?;

        sqlx::query(
            "UPDATE bookings
             SET is_waitlist = 0, position = ?
             WHERE id = ?",
        )
        .bind(next_player_position)
        .bind(promoted_booking_id)
        .execute(&mut **tx)
        .await?;

        if actor_user_id.is_some() {
            let now = crate::clock::now().timestamp();
            sqlx::query(
                "INSERT INTO push_jobs
                    (kind, device_id, generation, user_id, booking_id,
                     created_at, next_attempt_at, expires_at)
                 SELECT 'waitlist_promoted', d.id, d.generation, b.user_id, b.id,
                        ?, ?, MIN(unixepoch(s.datetime), ?)
                 FROM bookings b
                 JOIN slots s ON s.id = b.slot_id
                 JOIN push_devices d ON d.user_id = b.user_id
                 WHERE b.id = ? AND d.enabled = 1 AND d.endpoint IS NOT NULL",
            )
            .bind(now)
            .bind(now)
            .bind(now + 30 * 60)
            .bind(promoted_booking_id)
            .execute(&mut **tx)
            .await?;
        }

        sqlx::query(
            "UPDATE bookings
             SET position = position - 1
             WHERE slot_id = ? AND is_waitlist = 1 AND position > ?",
        )
        .bind(slot_id)
        .bind(promoted_waitlist_position)
        .execute(&mut **tx)
        .await?;
    }

    Ok(())
}

/// Promotes a waitlisted user into the player list.
///
/// # Errors
///
/// Returns an error when querying or writing booking data fails.
pub async fn promote_waitlist_user(
    pool: &SqlitePool,
    slot_id: i64,
    user_id: i64,
) -> Result<PromoteOutcome, sqlx::Error> {
    promote_waitlist_checked(pool, slot_id, user_id, None).await
}

pub(crate) async fn promote_waitlist_checked(
    pool: &SqlitePool,
    slot_id: i64,
    user_id: i64,
    revision: Option<i64>,
) -> Result<PromoteOutcome, sqlx::Error> {
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    let slot = sqlx::query("SELECT max_players, revision FROM slots WHERE id = ?")
        .bind(slot_id)
        .fetch_optional(&mut *tx)
        .await?;
    let Some(slot) = slot else {
        return Ok(PromoteOutcome::SlotNotFound);
    };
    if let Some(revision) = revision
        && slot.try_get::<i64, _>("revision")? != revision
    {
        return Ok(PromoteOutcome::Stale);
    }
    let max_players: i64 = slot.try_get("max_players")?;

    let player_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM bookings WHERE slot_id = ? AND is_waitlist = 0")
            .bind(slot_id)
            .fetch_one(&mut *tx)
            .await?;
    if player_count >= max_players {
        return Ok(PromoteOutcome::PlayerListFull);
    }

    let waitlist_booking = sqlx::query(
        "SELECT id, position
         FROM bookings
         WHERE slot_id = ? AND user_id = ? AND is_waitlist = 1",
    )
    .bind(slot_id)
    .bind(user_id)
    .fetch_optional(&mut *tx)
    .await?;
    let Some(waitlist_booking) = waitlist_booking else {
        return Ok(PromoteOutcome::NotWaitlisted);
    };
    let booking_id: i64 = waitlist_booking.try_get("id")?;
    let old_waitlist_position: i64 = waitlist_booking.try_get("position")?;

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
    .bind(booking_id)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        "UPDATE bookings
         SET position = position - 1
         WHERE slot_id = ? AND is_waitlist = 1 AND position > ?",
    )
    .bind(slot_id)
    .bind(old_waitlist_position)
    .execute(&mut *tx)
    .await?;

    increment_slot_revision(&mut tx, slot_id).await?;
    tx.commit().await?;
    Ok(PromoteOutcome::Promoted)
}

async fn increment_slot_revision(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    slot_id: i64,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE slots SET revision = revision + 1 WHERE id = ?")
        .bind(slot_id)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

async fn cancellation_slot_status(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    slot_id: i64,
    revision: Option<i64>,
) -> Result<Option<CancelOutcome>, sqlx::Error> {
    let slot = sqlx::query("SELECT revision FROM slots WHERE id = ?")
        .bind(slot_id)
        .fetch_optional(&mut **tx)
        .await?;
    let Some(slot) = slot else {
        return Ok(Some(CancelOutcome::SlotNotFound));
    };
    if let Some(revision) = revision
        && slot.try_get::<i64, _>("revision")? != revision
    {
        return Ok(Some(CancelOutcome::Stale));
    }
    Ok(None)
}

async fn list_active_slot_schedules(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
) -> Result<Vec<SlotSchedule>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT id, weekday, time_local, venue, map_url, max_players, max_waitlist
         FROM slot_schedules
         WHERE is_active = 1",
    )
    .fetch_all(&mut **tx)
    .await?;

    rows.into_iter()
        .map(|row| {
            Ok(SlotSchedule {
                id: row.try_get("id")?,
                weekday: row.try_get("weekday")?,
                time_local: row.try_get("time_local")?,
                venue: row.try_get("venue")?,
                map_url: row.try_get("map_url")?,
                max_players: row.try_get("max_players")?,
                max_waitlist: row.try_get("max_waitlist")?,
            })
        })
        .collect()
}

fn weekday_to_number(weekday: chrono::Weekday) -> i64 {
    match weekday {
        chrono::Weekday::Mon => 1,
        chrono::Weekday::Tue => 2,
        chrono::Weekday::Wed => 3,
        chrono::Weekday::Thu => 4,
        chrono::Weekday::Fri => 5,
        chrono::Weekday::Sat => 6,
        chrono::Weekday::Sun => 7,
    }
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
                revision: row.get("revision"),
                is_override: row.get::<i64, _>("is_override") != 0,
                datetime: row.get("datetime"),
                venue: row.get("venue"),
                map_url: row.get("map_url"),
                max_players: usize::try_from(row.try_get::<i64, _>("max_players")?)
                    .map_err(|error| sqlx::Error::Decode(Box::new(error)))?,
                max_waitlist: usize::try_from(row.try_get::<i64, _>("max_waitlist")?)
                    .map_err(|error| sqlx::Error::Decode(Box::new(error)))?,
                players: Vec::new(),
                player_user_ids: Vec::new(),
                waitlist: Vec::new(),
                waitlist_user_ids: Vec::new(),
            });
        }

        let maybe_name: Option<String> = row.try_get("name")?;
        if let Some(name) = maybe_name {
            let is_waitlist: i64 = row.get("is_waitlist");
            let user_id: i64 = row.get("user_id");
            if let Some(slot) = current_slot.as_mut() {
                if is_waitlist == 1 {
                    slot.waitlist.push(name);
                    slot.waitlist_user_ids.push(user_id);
                } else {
                    slot.players.push(name);
                    slot.player_user_ids.push(user_id);
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
    let rows = sqlx::query("SELECT id, name, is_admin FROM users ORDER BY name ASC")
        .fetch_all(pool)
        .await?;

    rows.into_iter()
        .map(|row| {
            Ok(UserIdentity {
                id: row.try_get("id")?,
                name: row.try_get("name")?,
                is_admin: row.try_get::<i64, _>("is_admin")? == 1,
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
    let row = sqlx::query("SELECT id, name, is_admin FROM users WHERE id = ?")
        .bind(user_id)
        .fetch_optional(pool)
        .await?;

    row.map(|record| {
        Ok(UserIdentity {
            id: record.try_get("id")?,
            name: record.try_get("name")?,
            is_admin: record.try_get::<i64, _>("is_admin")? == 1,
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
    let row = sqlx::query(
        "SELECT id, name, is_admin FROM users WHERE name = ? COLLATE NOCASE ORDER BY id LIMIT 1",
    )
    .bind(name)
    .fetch_optional(pool)
    .await?;

    row.map(|record| {
        Ok(UserIdentity {
            id: record.try_get("id")?,
            name: record.try_get("name")?,
            is_admin: record.try_get::<i64, _>("is_admin")? == 1,
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
    sqlx::query("INSERT INTO users (name, is_admin) VALUES (?, ?) ON CONFLICT(name) DO NOTHING")
        .bind(name)
        .bind(0_i64)
        .execute(pool)
        .await?;

    let row = sqlx::query("SELECT id, name, is_admin FROM users WHERE name = ?")
        .bind(name)
        .fetch_one(pool)
        .await?;

    Ok(UserIdentity {
        id: row.try_get("id")?,
        name: row.try_get("name")?,
        is_admin: row.try_get::<i64, _>("is_admin")? == 1,
    })
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};
    use sqlx::Row;

    use super::{create_upcoming_slots, init_pool, seed_preview_roster};

    async fn promotion_fixture() -> sqlx::SqlitePool {
        let pool = init_pool("sqlite::memory:").await.unwrap();
        sqlx::raw_sql(
            "INSERT INTO users (id, name) VALUES (1, 'Player'), (2, 'Waiter'), (3, 'Other');
             INSERT INTO slots (id, datetime, venue, max_players, max_waitlist)
                 VALUES (1, '2099-01-01T19:00:00Z', 'Court', 1, 2);
             INSERT INTO bookings (slot_id, user_id, position, is_waitlist)
                 VALUES (1, 1, 1, 0), (1, 2, 1, 1), (1, 3, 2, 1);
             INSERT INTO push_devices (token_hash, user_id, endpoint, p256dh, auth, generation, created_at, updated_at)
                 VALUES (x'01', 2, 'https://one.example', 'key', 'auth', 3, 0, 0),
                        (x'02', 2, 'https://two.example', 'key', 'auth', 4, 0, 0),
                        (x'03', 3, 'https://three.example', 'key', 'auth', 0, 0, 0);
             UPDATE push_devices SET enabled = 0 WHERE token_hash = x'02';",
        )
        .execute(&pool)
        .await
        .unwrap();
        pool
    }

    #[tokio::test]
    async fn member_promotion_enqueues_only_enabled_promoted_devices() {
        let pool = promotion_fixture().await;
        super::cancel_booking_for_slot(&pool, 1, 1).await.unwrap();
        let jobs: Vec<(i64, i64, i64, i64)> =
            sqlx::query_as("SELECT device_id, generation, user_id, booking_id FROM push_jobs")
                .fetch_all(&pool)
                .await
                .unwrap();
        let booking: i64 = sqlx::query_scalar("SELECT id FROM bookings WHERE user_id = 2")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(jobs, vec![(1, 3, 2, booking)]);
        super::cancel_booking_for_slot(&pool, 1, 1).await.unwrap();
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM push_jobs")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn admin_promotion_does_not_enqueue_a_job() {
        let pool = promotion_fixture().await;
        super::cancel_booking_checked(&pool, 1, 1, None)
            .await
            .unwrap();
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM push_jobs")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn waitlist_cancellation_does_not_enqueue_a_job() {
        let pool = promotion_fixture().await;
        super::cancel_booking_for_slot(&pool, 1, 2).await.unwrap();
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM push_jobs")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn promotion_jobs_expire_at_game_start_and_cover_each_enabled_device() {
        let pool = promotion_fixture().await;
        let now = crate::clock::now().timestamp();
        let starts = now + 300;
        sqlx::query("UPDATE slots SET datetime = ? WHERE id = 1")
            .bind(
                chrono::DateTime::from_timestamp(starts, 0)
                    .unwrap()
                    .to_rfc3339(),
            )
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("UPDATE push_devices SET enabled = 1 WHERE id = 2")
            .execute(&pool)
            .await
            .unwrap();
        super::cancel_booking_for_slot(&pool, 1, 1).await.unwrap();
        let jobs: Vec<(i64, i64)> =
            sqlx::query_as("SELECT device_id, expires_at FROM push_jobs ORDER BY device_id")
                .fetch_all(&pool)
                .await
                .unwrap();
        assert_eq!(jobs, vec![(1, starts), (2, starts)]);
    }

    #[tokio::test]
    async fn job_failure_rolls_back_cancellation_promotion_and_activity() {
        let pool = promotion_fixture().await;
        sqlx::raw_sql(
            "CREATE TRIGGER fail_push BEFORE INSERT ON push_jobs
             BEGIN SELECT RAISE(ABORT, 'test failure'); END;",
        )
        .execute(&pool)
        .await
        .unwrap();
        assert!(super::cancel_booking_for_slot(&pool, 1, 1).await.is_err());
        let bookings: Vec<(i64, i64)> =
            sqlx::query_as("SELECT user_id, is_waitlist FROM bookings ORDER BY user_id")
                .fetch_all(&pool)
                .await
                .unwrap();
        assert_eq!(bookings, vec![(1, 0), (2, 1), (3, 1)]);
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM activity_log")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn memory_database_connection_does_not_expire() {
        let pool = init_pool("sqlite::memory:").await.unwrap();
        assert_eq!(pool.options().get_max_connections(), 1);
        assert_eq!(pool.options().get_idle_timeout(), None);
        assert_eq!(pool.options().get_max_lifetime(), None);
        create_upcoming_slots(&pool, Utc::now()).await.unwrap();
    }

    #[tokio::test]
    async fn new_database_has_no_players_or_bookings() {
        let pool = init_pool("sqlite::memory:").await.unwrap();
        create_upcoming_slots(&pool, Utc::now()).await.unwrap();
        for table in ["users", "bookings"] {
            let count: i64 = sqlx::query_scalar(&format!("SELECT count(*) FROM {table}"))
                .fetch_one(&pool)
                .await
                .unwrap();
            assert_eq!(count, 0);
        }
    }

    #[tokio::test]
    async fn preview_has_a_full_game_with_three_waiting() {
        let pool = init_pool("sqlite::memory:").await.unwrap();
        create_upcoming_slots(&pool, chrono::Utc::now())
            .await
            .unwrap();
        seed_preview_roster(&pool).await.unwrap();
        let row: (i64, i64) = sqlx::query_as(
            "SELECT SUM(b.is_waitlist = 0), SUM(b.is_waitlist = 1) FROM bookings b JOIN slots s ON s.id = b.slot_id WHERE s.schedule_id IS NOT NULL GROUP BY s.id HAVING SUM(b.is_waitlist = 0) = 15 AND SUM(b.is_waitlist = 1) = 3",
        )
        .fetch_one(&pool).await.unwrap();
        assert_eq!(row, (15, 3));
    }

    #[tokio::test]
    async fn preview_fills_the_farthest_upcoming_game() {
        let pool = init_pool("sqlite::memory:").await.unwrap();
        create_upcoming_slots(&pool, Utc::now()).await.unwrap();
        let future = (Utc::now() + chrono::Duration::days(365)).to_rfc3339();
        let slot_id: i64 = sqlx::query_scalar(
            "INSERT INTO slots (datetime, venue) VALUES (?, 'Future court') RETURNING id",
        )
        .bind(future)
        .fetch_one(&pool)
        .await
        .unwrap();
        seed_preview_roster(&pool).await.unwrap();
        let booked_slot: i64 = sqlx::query_scalar("SELECT slot_id FROM bookings LIMIT 1")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(booked_slot, slot_id);
    }

    #[tokio::test]
    async fn file_preview_survives_restart_and_is_only_seeded_once() {
        let dir = tempfile::tempdir().unwrap();
        let url = format!("sqlite://{}", dir.path().join("preview.db").display());
        let pool = init_pool(&url).await.unwrap();
        create_upcoming_slots(&pool, Utc::now()).await.unwrap();
        seed_preview_roster(&pool).await.unwrap();
        pool.close().await;

        let pool = init_pool(&url).await.unwrap();
        seed_preview_roster(&pool).await.unwrap();
        let counts: (i64, i64) =
            sqlx::query_as("SELECT (SELECT count(*) FROM users), (SELECT count(*) FROM bookings)")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(counts, (18, 18));
        sqlx::query("DELETE FROM bookings")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM users")
            .execute(&pool)
            .await
            .unwrap();
        seed_preview_roster(&pool).await.unwrap();
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM users")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn preview_does_not_modify_existing_players() {
        let pool = init_pool("sqlite::memory:").await.unwrap();
        create_upcoming_slots(&pool, Utc::now()).await.unwrap();
        sqlx::query("INSERT INTO users (name) VALUES ('Existing player')")
            .execute(&pool)
            .await
            .unwrap();
        seed_preview_roster(&pool).await.unwrap();
        let counts: (i64, i64) =
            sqlx::query_as("SELECT (SELECT count(*) FROM users), (SELECT count(*) FROM bookings)")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(counts, (1, 0));
    }

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

    #[tokio::test]
    async fn create_upcoming_slots_is_idempotent() {
        let pool = init_pool("sqlite::memory:")
            .await
            .expect("in-memory db should initialize");
        let now = Utc
            .with_ymd_and_hms(2030, 1, 1, 0, 0, 0)
            .single()
            .expect("valid test date");

        let first_created = create_upcoming_slots(&pool, now)
            .await
            .expect("first generation should succeed");
        assert!(first_created > 0);

        let second_created = create_upcoming_slots(&pool, now)
            .await
            .expect("second generation should succeed");
        assert_eq!(second_created, 0);
    }

    #[tokio::test]
    async fn generation_keeps_local_time_across_both_dst_changes() {
        for (now, before, after) in [
            (
                "2030-03-24T18:00:00Z",
                "2030-03-25T19:00:00Z",
                "2030-04-01T18:00:00Z",
            ),
            (
                "2030-10-20T17:00:00Z",
                "2030-10-21T18:00:00Z",
                "2030-10-28T19:00:00Z",
            ),
        ] {
            let pool = init_pool("sqlite::memory:").await.unwrap();
            let now = chrono::DateTime::parse_from_rfc3339(now)
                .unwrap()
                .with_timezone(&Utc);
            create_upcoming_slots(&pool, now).await.unwrap();
            let dates: Vec<String> =
                sqlx::query_scalar("SELECT datetime FROM slots WHERE venue = 'Luisenschule'")
                    .fetch_all(&pool)
                    .await
                    .unwrap();
            assert!(dates.contains(&before.to_string()), "{dates:?}");
            assert!(dates.contains(&after.to_string()), "{dates:?}");
        }
    }

    #[tokio::test]
    async fn renewal_extends_window_and_preserves_legacy_booked_slots() {
        let pool = init_pool("sqlite::memory:").await.unwrap();
        sqlx::query("INSERT INTO users (id, name) VALUES (1, 'Alex')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO slots (datetime, venue) VALUES ('2030-03-25T20:00:00Z', 'Luisenschule')",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("INSERT INTO bookings (slot_id, user_id, position) SELECT id, 1, 1 FROM slots WHERE datetime = '2030-03-25T20:00:00Z'")
            .execute(&pool).await.unwrap();
        let now = Utc.with_ymd_and_hms(2030, 3, 25, 12, 0, 0).unwrap();
        create_upcoming_slots(&pool, now).await.unwrap();
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM slots WHERE venue = 'Luisenschule' AND date(datetime) = '2030-03-25'")
            .fetch_one(&pool).await.unwrap();
        assert_eq!(count, 1);
        let bookings: i64 = sqlx::query_scalar("SELECT count(*) FROM bookings b JOIN slots s ON b.slot_id = s.id WHERE s.datetime = '2030-03-25T20:00:00Z'")
            .fetch_one(&pool).await.unwrap();
        assert_eq!(bookings, 1);
        assert!(
            create_upcoming_slots(&pool, now + chrono::Days::new(7))
                .await
                .unwrap()
                > 0
        );
        assert_eq!(
            create_upcoming_slots(&pool, now + chrono::Days::new(7))
                .await
                .unwrap(),
            0
        );
        let slots = super::list_slots_at(&pool, now).await.unwrap();
        assert!(
            slots
                .iter()
                .all(|slot| chrono::DateTime::parse_from_rfc3339(&slot.datetime).unwrap() > now)
        );
    }

    #[tokio::test]
    async fn generation_uses_berlin_date_at_utc_midnight_boundary() {
        let pool = init_pool("sqlite::memory:").await.unwrap();
        let now = Utc.with_ymd_and_hms(2030, 4, 4, 22, 30, 0).unwrap();
        create_upcoming_slots(&pool, now).await.unwrap();
        let last: String =
            sqlx::query_scalar("SELECT max(datetime) FROM slots WHERE datetime LIKE '2030%'")
                .fetch_one(&pool)
                .await
                .unwrap();
        // The latest released week ends Sunday 14 April.
        assert_eq!(last, "2030-04-11T18:00:00Z");
        assert_eq!(create_upcoming_slots(&pool, now).await.unwrap(), 0);
    }

    #[tokio::test]
    async fn generation_does_not_create_slots_that_have_already_started_today() {
        let pool = init_pool("sqlite::memory:").await.unwrap();
        let now = Utc.with_ymd_and_hms(2030, 3, 25, 19, 30, 0).unwrap();
        create_upcoming_slots(&pool, now).await.unwrap();
        let count: i64 =
            sqlx::query_scalar("SELECT count(*) FROM slots WHERE date(datetime) = '2030-03-25'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(count, 0);
    }
}
