//! Transactional booking history with stable fields and versioned JSON details.

use chrono::{DateTime, SecondsFormat, Utc};
use chrono_tz::Europe::Berlin;
use serde::{Deserialize, Serialize};
use sqlx::{Row, Sqlite, Transaction};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ActivityKind {
    SignedUp,
    Cancelled,
    Promoted,
}

impl ActivityKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::SignedUp => "booking.signed_up",
            Self::Cancelled => "booking.cancelled",
            Self::Promoted => "booking.promoted",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum BookingList {
    Players,
    Waitlist,
}

/// Describes the booking at the time of an action.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct BookingDetails {
    pub booking_id: i64,
    pub booked_at: String,
    pub list: BookingList,
    pub position: i64,
    pub caused_by_activity_id: Option<i64>,
}

pub(crate) struct BookingActivity {
    pub kind: ActivityKind,
    pub actor_user_id: i64,
    pub subject_user_id: i64,
    pub slot_id: i64,
    pub caused_by_activity_id: Option<i64>,
}

// Call before cancellation or promotion, but after signup.
pub(crate) async fn record_booking(
    tx: &mut Transaction<'_, Sqlite>,
    activity: BookingActivity,
) -> Result<i64, sqlx::Error> {
    record_booking_at(tx, activity, Utc::now()).await
}

async fn record_booking_at(
    tx: &mut Transaction<'_, Sqlite>,
    activity: BookingActivity,
    now: DateTime<Utc>,
) -> Result<i64, sqlx::Error> {
    let row = sqlx::query(
        "SELECT b.id, b.created_at, b.is_waitlist, b.position,
                s.datetime, s.venue, actor.name AS actor_name, subject.name AS subject_name
         FROM bookings b
         JOIN slots s ON s.id = b.slot_id
         JOIN users subject ON subject.id = b.user_id
         JOIN users actor ON actor.id = ?
         WHERE b.slot_id = ? AND b.user_id = ?",
    )
    .bind(activity.actor_user_id)
    .bind(activity.slot_id)
    .bind(activity.subject_user_id)
    .fetch_one(&mut **tx)
    .await?;
    let starts_at = DateTime::parse_from_rfc3339(&row.try_get::<String, _>("datetime")?)
        .map_err(|error| sqlx::Error::Decode(Box::new(error)))?;
    let details = BookingDetails {
        booking_id: row.try_get("id")?,
        booked_at: row.try_get("created_at")?,
        list: if row.try_get::<bool, _>("is_waitlist")? {
            BookingList::Waitlist
        } else {
            BookingList::Players
        },
        position: row.try_get("position")?,
        caused_by_activity_id: activity.caused_by_activity_id,
    };
    let details =
        serde_json::to_string(&details).map_err(|error| sqlx::Error::Encode(Box::new(error)))?;
    sqlx::query_scalar(
        "INSERT INTO activity_log
         (occurred_at, occurred_local_date, kind, actor_user_id, actor_name, subject_user_id, subject_name,
          slot_id, slot_starts_at, slot_local_date, venue, details)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) RETURNING id",
    )
    .bind(now.to_rfc3339_opts(SecondsFormat::Micros, true))
    .bind(now.with_timezone(&Berlin).date_naive().to_string())
    .bind(activity.kind.as_str())
    .bind(activity.actor_user_id)
    .bind(row.try_get::<String, _>("actor_name")?)
    .bind(activity.subject_user_id)
    .bind(row.try_get::<String, _>("subject_name")?)
    .bind(activity.slot_id)
    .bind(
        starts_at
            .with_timezone(&Utc)
            .to_rfc3339_opts(SecondsFormat::Micros, true),
    )
    .bind(starts_at.with_timezone(&Berlin).date_naive().to_string())
    .bind(row.try_get::<String, _>("venue")?)
    .bind(details)
    .fetch_one(&mut **tx)
    .await
}

#[cfg(test)]
mod tests;
