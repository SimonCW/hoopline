//! Admin editing for recurring games and their release time.

use askama::Template;
use axum::{
    Form,
    extract::{Path, State},
    http::HeaderMap,
    response::{Html, Response},
};
use chrono::{NaiveTime, Timelike};
use serde::Deserialize;
use sqlx::{Row, SqlitePool};

use crate::{
    error::{AppError, Problem},
    i18n::Language,
    identity::{AdminIdentity, current_user_from_headers},
    identity_redirect, scheduling,
};

struct Schedule {
    id: i64,
    weekday: i64,
    time_local: String,
    venue: String,
    map_url: String,
    max_players: i64,
    max_waitlist: i64,
    is_active: bool,
    revision: i64,
}

#[derive(Template)]
#[template(path = "admin_schedule.html")]
struct SchedulePage {
    lang: Language,
    title: &'static str,
    user_id: i64,
    schedules: Vec<Schedule>,
    release: String,
    release_revision: i64,
    next_release: String,
}

#[derive(Deserialize)]
pub(crate) struct ScheduleForm {
    weekday: i64,
    time_local: String,
    venue: String,
    map_url: Option<String>,
    max_players: i64,
    max_waitlist: i64,
    is_active: Option<String>,
    revision: Option<i64>,
}

#[derive(Deserialize)]
pub(crate) struct ReleaseForm {
    sunday_release_time: String,
    revision: i64,
}

pub(crate) async fn page(
    State(pool): State<SqlitePool>,
    lang: Language,
    headers: HeaderMap,
) -> Result<Html<String>, AppError> {
    let user = current_user_from_headers(&pool, &headers)
        .await?
        .filter(|user| user.is_admin)
        .ok_or(AppError::Forbidden(Problem::AdminRequired))?;
    let rows = sqlx::query("SELECT id, weekday, time_local, venue, map_url, max_players, max_waitlist, is_active, revision FROM slot_schedules ORDER BY weekday, time_local, id")
        .fetch_all(&pool).await?;
    let schedules = rows
        .into_iter()
        .map(|row| {
            Ok(Schedule {
                id: row.try_get("id")?,
                weekday: row.try_get("weekday")?,
                time_local: row.try_get("time_local")?,
                venue: row.try_get("venue")?,
                map_url: row.try_get("map_url")?,
                max_players: row.try_get("max_players")?,
                max_waitlist: row.try_get("max_waitlist")?,
                is_active: row.try_get::<i64, _>("is_active")? != 0,
                revision: row.try_get("revision")?,
            })
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()?;
    let settings =
        sqlx::query("SELECT sunday_release_time, revision FROM schedule_settings WHERE id = 1")
            .fetch_one(&pool)
            .await?;
    let release: String = settings.try_get("sunday_release_time")?;
    let time = NaiveTime::parse_from_str(&release, "%H:%M")
        .map_err(|_| AppError::BadRequest(Problem::InvalidReleaseTime))?;
    let (next, monday) = scheduling::next_release(crate::clock::now(), time)
        .map_err(|_| AppError::BadRequest(Problem::InvalidReleaseTime))?;
    let next_release = lang.locale.release_summary(next, monday);
    Ok(Html(
        SchedulePage {
            title: lang.text.recurring_schedule,
            lang,
            user_id: user.id,
            schedules,
            release,
            release_revision: settings.try_get("revision")?,
            next_release,
        }
        .render()?,
    ))
}

fn validate(form: &ScheduleForm) -> Result<(), AppError> {
    if !(1..=7).contains(&form.weekday)
        || !(1..=100).contains(&form.max_players)
        || !(0..=100).contains(&form.max_waitlist)
        || form.venue.trim().is_empty()
        || form.venue.trim().chars().count() > 120
        || NaiveTime::parse_from_str(&form.time_local, "%H:%M").is_err()
    {
        return Err(AppError::BadRequest(Problem::BadSchedule));
    }
    Ok(())
}

pub(crate) async fn create(
    State(pool): State<SqlitePool>,
    _admin: AdminIdentity,
    headers: HeaderMap,
    Form(form): Form<ScheduleForm>,
) -> Result<Response, AppError> {
    validate(&form)?;
    let map_url = crate::validate_map_url(form.map_url.as_deref())?.unwrap_or_default();
    sqlx::query("INSERT INTO slot_schedules (weekday, time_local, venue, map_url, max_players, max_waitlist, is_active) VALUES (?, ?, ?, ?, ?, ?, ?)")
        .bind(form.weekday).bind(&form.time_local).bind(form.venue.trim()).bind(map_url).bind(form.max_players).bind(form.max_waitlist)
        .bind(i64::from(form.is_active.is_some())).execute(&pool).await?;
    Ok(identity_redirect(&headers, "/admin/schedule"))
}

pub(crate) async fn update(
    State(pool): State<SqlitePool>,
    Path(id): Path<i64>,
    _admin: AdminIdentity,
    headers: HeaderMap,
    Form(form): Form<ScheduleForm>,
) -> Result<Response, AppError> {
    validate(&form)?;
    let map_url = crate::validate_map_url(form.map_url.as_deref())?;
    let revision = form.revision.ok_or(AppError::StaleSlot)?;
    let result = sqlx::query("UPDATE slot_schedules SET weekday=?, time_local=?, venue=?, map_url=COALESCE(?, map_url), max_players=?, max_waitlist=?, is_active=?, revision=revision+1 WHERE id=? AND revision=?")
        .bind(form.weekday).bind(&form.time_local).bind(form.venue.trim()).bind(map_url).bind(form.max_players).bind(form.max_waitlist)
        .bind(i64::from(form.is_active.is_some())).bind(id).bind(revision).execute(&pool).await?;
    if result.rows_affected() != 1 {
        return Err(AppError::StaleSlot);
    }
    Ok(identity_redirect(&headers, "/admin/schedule"))
}

pub(crate) async fn release_time(
    State(pool): State<SqlitePool>,
    _admin: AdminIdentity,
    headers: HeaderMap,
    Form(form): Form<ReleaseForm>,
) -> Result<Response, AppError> {
    let time = NaiveTime::parse_from_str(&form.sunday_release_time, "%H:%M")
        .map_err(|_| AppError::BadRequest(Problem::BadReleaseTime))?;
    if time.hour() < 4 {
        return Err(AppError::BadRequest(Problem::UnsafeReleaseTime));
    }
    let result = sqlx::query("UPDATE schedule_settings SET sunday_release_time=?, revision=revision+1 WHERE id=1 AND revision=?")
        .bind(time.format("%H:%M").to_string()).bind(form.revision).execute(&pool).await?;
    if result.rows_affected() != 1 {
        return Err(AppError::StaleSlot);
    }
    Ok(identity_redirect(&headers, "/admin/schedule"))
}
