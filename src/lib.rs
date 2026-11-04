pub mod db;
pub mod error;
mod identity;
pub mod models;

use crate::{
    db::{
        CancelOutcome, PromoteOutcome, SignupOutcome, cancel_booking_for_slot,
        create_upcoming_slots, create_user, find_slot, find_user_by_id, find_user_by_name,
        list_slots, list_users, promote_waitlist_user, signup_for_slot,
    },
    identity::{AdminIdentity, BookingIdentity, current_user_from_headers},
    models::{Slot, SlotWeek, UserIdentity, group_slots},
};
use askama::Template;
use axum::{
    Form, Router,
    extract::{Path, State},
    http::{HeaderMap, HeaderName, HeaderValue, header},
    response::{Html, IntoResponse, Redirect, Response},
    routing::{get, post},
};
use chrono::Utc;
use error::AppError;
use serde::Deserialize;
use sqlx::SqlitePool;
use std::{path::Path as FsPath, time::Duration};
use tower_http::trace::TraceLayer;

#[derive(Template)]
#[template(path = "slots.html")]
struct SlotsTemplate {
    user_selector_html: String,
    identity_refresh_html: String,
    title: String,
    current_user_id: Option<i64>,
    current_user_is_admin: bool,
    weeks: Vec<SlotWeek>,
}

#[derive(Template)]
#[template(path = "welcome.html")]
struct WelcomeTemplate {
    title: String,
    user_selector_html: String,
}

#[derive(Template)]
#[template(path = "slots_content.html")]
struct SlotsContentTemplate {
    identity_refresh_html: String,
    current_user_id: Option<i64>,
    current_user_is_admin: bool,
    weeks: Vec<SlotWeek>,
}

#[derive(Template)]
#[template(path = "slot_card.html")]
struct SlotCardTemplate {
    slot: Slot,
    current_user_id: Option<i64>,
    current_user_is_admin: bool,
    feedback: String,
}

#[derive(Template)]
#[template(path = "user_selector.html")]
struct UserSelectorTemplate {
    out_of_band: bool,
    users: Vec<UserIdentity>,
    selected_user_id: Option<i64>,
    current_user_label: String,
    current_user_is_admin: bool,
}

#[derive(Deserialize)]
struct UserSelectionForm {
    selected_user_id: Option<i64>,
    new_user_name: Option<String>,
}

/// Builds the application router with a migrated `SQLite` connection pool.
///
/// # Errors
///
/// Returns an error when pool initialization or migrations fail.
pub async fn app() -> Result<Router, AppError> {
    let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
        if FsPath::new("/data").is_dir() {
            "sqlite:///data/hoopline.db".to_string()
        } else {
            "sqlite://tmp/hoopline.db".to_string()
        }
    });
    if database_url == "sqlite://tmp/hoopline.db" {
        std::fs::create_dir_all("tmp")?;
    }
    let pool = db::init_pool(&database_url).await?;
    create_upcoming_slots(&pool, Utc::now()).await?;
    let renewal_pool = pool.clone();
    tokio::spawn(async move {
        let mut timer = tokio::time::interval(Duration::from_secs(60));
        timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            timer.tick().await;
            if renewal_pool.is_closed() {
                break;
            }
            if let Err(error) = create_upcoming_slots(&renewal_pool, Utc::now()).await {
                eprintln!("slot renewal failed; will retry in one minute: {error}");
            }
        }
    });

    Ok(app_with_pool(pool))
}

pub fn app_with_pool(pool: SqlitePool) -> Router {
    Router::new()
        .route("/", get(index))
        .route("/slots", get(index))
        .route("/slots/fragment", get(slots_fragment))
        .route("/slots/{slot_id}/signup", post(signup_slot))
        .route("/slots/{slot_id}/cancel", post(cancel_slot))
        .route(
            "/admin/slots/{slot_id}/remove/{user_id}",
            post(admin_remove_slot_user),
        )
        .route(
            "/admin/slots/{slot_id}/promote/{user_id}",
            post(admin_promote_waitlist_user),
        )
        .route("/admin/slots/generate", post(admin_generate_slots))
        .route("/users", get(users_fragment))
        .route("/users/select", post(select_user))
        .route("/users/logout", post(logout))
        .route("/healthz", get(healthz))
        .route("/robots.txt", get(robots))
        .route("/assets/app.css", get(app_css))
        .route("/assets/app.js", get(app_js))
        .route("/assets/name-search.js", get(name_search_js))
        .route(
            "/favicon.ico",
            get(|| async { axum::http::StatusCode::NO_CONTENT }),
        )
        .with_state(pool)
        .layer(axum::middleware::map_response(
            |mut response: Response| async move {
                response
                    .headers_mut()
                    .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
                response
            },
        ))
        .layer(TraceLayer::new_for_http())
}

async fn index(
    State(pool): State<SqlitePool>,
    headers: HeaderMap,
) -> Result<Html<String>, AppError> {
    let current_user = current_user_from_headers(&pool, &headers).await?;
    if current_user.is_none() {
        let welcome = WelcomeTemplate {
            title: "Hoopline".into(),
            user_selector_html: render_user_selector(&pool, None, false).await?,
        };
        return Ok(Html(welcome.render()?));
    }
    let current_user_id = current_user.as_ref().map(|user| user.id);
    let current_user_is_admin = current_user.as_ref().is_some_and(|user| user.is_admin);
    let template = SlotsTemplate {
        user_selector_html: render_user_selector(&pool, current_user.as_ref(), false).await?,
        identity_refresh_html: String::new(),
        title: "Hoopline".to_string(),
        current_user_id,
        current_user_is_admin,
        weeks: group_slots(list_slots(&pool).await?),
    };
    Ok(Html(template.render()?))
}

async fn slots_fragment(
    State(pool): State<SqlitePool>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    let current_user = current_user_from_headers(&pool, &headers).await?;
    if current_user.is_none() {
        return Ok(identity_redirect(&headers, "/"));
    }
    let current_user_id = current_user.as_ref().map(|user| user.id);
    let current_user_is_admin = current_user.as_ref().is_some_and(|user| user.is_admin);
    let template = SlotsContentTemplate {
        identity_refresh_html: render_user_selector(&pool, current_user.as_ref(), true).await?,
        current_user_id,
        current_user_is_admin,
        weeks: group_slots(list_slots(&pool).await?),
    };
    Ok(Html(template.render()?).into_response())
}

async fn signup_slot(
    State(pool): State<SqlitePool>,
    Path(slot_id): Path<i64>,
    current_user: BookingIdentity,
) -> Result<Html<String>, AppError> {
    match signup_for_slot(&pool, slot_id, current_user.id()).await? {
        SignupOutcome::AddedPlayer | SignupOutcome::AddedWaitlist => {}
        SignupOutcome::AlreadySignedUp => {
            return Err(AppError::BadRequest(
                "you are already signed up for this slot".to_string(),
            ));
        }
        SignupOutcome::Full => {
            return Err(AppError::BadRequest(
                "slot and waitlist are already full".to_string(),
            ));
        }
        SignupOutcome::SlotNotFound => {
            return Err(AppError::BadRequest("slot does not exist".to_string()));
        }
        SignupOutcome::Started => {
            return Err(AppError::BadRequest(
                "This slot has started. Refresh to see upcoming slots.".into(),
            ));
        }
    }

    let slot = find_slot(&pool, slot_id)
        .await?
        .ok_or_else(|| AppError::BadRequest("slot does not exist".to_string()))?;
    let template = SlotCardTemplate {
        feedback: format!(
            "Booking saved. {}.",
            slot.booking_status(&Some(current_user.id()))
        ),
        slot,
        current_user_id: Some(current_user.id()),
        current_user_is_admin: current_user.is_admin(),
    };
    Ok(Html(template.render()?))
}

async fn cancel_slot(
    State(pool): State<SqlitePool>,
    Path(slot_id): Path<i64>,
    current_user: BookingIdentity,
) -> Result<Html<String>, AppError> {
    match cancel_booking_for_slot(&pool, slot_id, current_user.id()).await? {
        CancelOutcome::Cancelled => {}
        CancelOutcome::NotSignedUp => {
            return Err(AppError::BadRequest(
                "you are not signed up for this slot".to_string(),
            ));
        }
        CancelOutcome::SlotNotFound => {
            return Err(AppError::BadRequest("slot does not exist".to_string()));
        }
    }

    let slot = find_slot(&pool, slot_id)
        .await?
        .ok_or_else(|| AppError::BadRequest("slot does not exist".to_string()))?;
    let template = SlotCardTemplate {
        feedback: "Booking cancelled. You are no longer on this list.".into(),
        slot,
        current_user_id: Some(current_user.id()),
        current_user_is_admin: current_user.is_admin(),
    };
    Ok(Html(template.render()?))
}

async fn admin_remove_slot_user(
    State(pool): State<SqlitePool>,
    Path((slot_id, user_id)): Path<(i64, i64)>,
    current_user: AdminIdentity,
) -> Result<Html<String>, AppError> {
    match cancel_booking_for_slot(&pool, slot_id, user_id).await? {
        CancelOutcome::Cancelled => {}
        CancelOutcome::NotSignedUp => {
            return Err(AppError::BadRequest(
                "target user is not signed up for this slot".to_string(),
            ));
        }
        CancelOutcome::SlotNotFound => {
            return Err(AppError::BadRequest("slot does not exist".to_string()));
        }
    }

    let slot = find_slot(&pool, slot_id)
        .await?
        .ok_or_else(|| AppError::BadRequest("slot does not exist".to_string()))?;
    let template = SlotCardTemplate {
        feedback: "Player removed. The waitlist has been updated.".into(),
        slot,
        current_user_id: Some(current_user.id()),
        current_user_is_admin: true,
    };
    Ok(Html(template.render()?))
}

async fn admin_promote_waitlist_user(
    State(pool): State<SqlitePool>,
    Path((slot_id, user_id)): Path<(i64, i64)>,
    current_user: AdminIdentity,
) -> Result<Html<String>, AppError> {
    match promote_waitlist_user(&pool, slot_id, user_id).await? {
        PromoteOutcome::Promoted => {}
        PromoteOutcome::NotWaitlisted => {
            return Err(AppError::BadRequest(
                "target user is not on this waitlist".to_string(),
            ));
        }
        PromoteOutcome::PlayerListFull => {
            return Err(AppError::BadRequest(
                "player list is full; remove a player first".to_string(),
            ));
        }
        PromoteOutcome::SlotNotFound => {
            return Err(AppError::BadRequest("slot does not exist".to_string()));
        }
    }

    let slot = find_slot(&pool, slot_id)
        .await?
        .ok_or_else(|| AppError::BadRequest("slot does not exist".to_string()))?;
    let template = SlotCardTemplate {
        feedback: "Waitlist player promoted.".into(),
        slot,
        current_user_id: Some(current_user.id()),
        current_user_is_admin: true,
    };
    Ok(Html(template.render()?))
}

async fn admin_generate_slots(
    State(pool): State<SqlitePool>,
    current_user: AdminIdentity,
) -> Result<Html<String>, AppError> {
    create_upcoming_slots(&pool, Utc::now()).await?;
    let template = SlotsContentTemplate {
        identity_refresh_html: String::new(),
        current_user_id: Some(current_user.id()),
        current_user_is_admin: true,
        weeks: group_slots(list_slots(&pool).await?),
    };
    Ok(Html(template.render()?))
}

async fn users_fragment(
    State(pool): State<SqlitePool>,
    headers: HeaderMap,
) -> Result<Html<String>, AppError> {
    let current_user = current_user_from_headers(&pool, &headers).await?;
    Ok(Html(
        render_user_selector(&pool, current_user.as_ref(), false).await?,
    ))
}

async fn render_user_selector(
    pool: &SqlitePool,
    user: Option<&UserIdentity>,
    out_of_band: bool,
) -> Result<String, AppError> {
    let template = UserSelectorTemplate {
        out_of_band,
        users: if user.is_none() {
            list_users(pool).await?
        } else {
            Vec::new()
        },
        selected_user_id: user.map(|user| user.id),
        current_user_label: user
            .map_or_else(|| "Select your name".into(), |user| user.name.clone()),
        current_user_is_admin: user.is_some_and(|user| user.is_admin),
    };
    Ok(template.render()?)
}

async fn select_user(
    State(pool): State<SqlitePool>,
    headers: HeaderMap,
    Form(form): Form<UserSelectionForm>,
) -> Result<Response, AppError> {
    let selected_user = if let Some(selected_user_id) = form.selected_user_id {
        find_user_by_id(&pool, selected_user_id)
            .await?
            .ok_or_else(|| AppError::BadRequest("selected user does not exist".to_string()))?
    } else if let Some(new_user_name) = form.new_user_name {
        let trimmed_name = new_user_name.trim();
        if trimmed_name.is_empty() || trimmed_name.chars().count() > 64 {
            return Err(AppError::BadRequest(
                "Enter a name with 1 to 64 characters.".to_string(),
            ));
        }
        if find_user_by_name(&pool, trimmed_name).await?.is_some() {
            return Err(AppError::BadRequest(
                "This name already exists. Choose your name from the list.".into(),
            ));
        }
        create_user(&pool, trimmed_name).await?
    } else {
        return Err(AppError::BadRequest(
            "Choose your name from the list. If it is missing, use Add a new member.".to_string(),
        ));
    };

    let mut response = identity_redirect(&headers, "/slots");
    response.headers_mut().insert(
        header::SET_COOKIE,
        user_cookie_header_value(selected_user.id)?,
    );
    Ok(response)
}

fn identity_redirect(headers: &HeaderMap, destination: &'static str) -> Response {
    if headers
        .get("hx-request")
        .is_some_and(|value| value == "true")
    {
        (
            [(
                HeaderName::from_static("hx-redirect"),
                HeaderValue::from_static(destination),
            )],
            "",
        )
            .into_response()
    } else {
        Redirect::to(destination).into_response()
    }
}

async fn logout(headers: HeaderMap) -> Result<Response, AppError> {
    let mut response = identity_redirect(&headers, "/");
    response
        .headers_mut()
        .insert(header::SET_COOKIE, identity_cookie("", 0)?);
    Ok(response)
}

async fn robots() -> &'static str {
    "User-agent: *\nDisallow: /"
}

async fn healthz() -> &'static str {
    "ok"
}

async fn app_css() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/css; charset=utf-8")],
        include_str!("../assets/app.css"),
    )
}

async fn app_js() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
        include_str!("../assets/app.js"),
    )
}

async fn name_search_js() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
        include_str!("../assets/name-search.js"),
    )
}

fn user_cookie_header_value(user_id: i64) -> Result<HeaderValue, AppError> {
    identity_cookie(&user_id.to_string(), 31_536_000)
}

fn identity_cookie(value: &str, max_age: u32) -> Result<HeaderValue, AppError> {
    let secure = if std::env::var("COOKIE_SECURE").is_ok_and(|value| value == "true") {
        "; Secure"
    } else {
        ""
    };
    let value =
        format!("user_id={value}; Path=/; Max-Age={max_age}; HttpOnly; SameSite=Lax{secure}");
    Ok(HeaderValue::from_str(&value)?)
}
