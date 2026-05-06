pub mod db;
pub mod error;
pub mod models;

use crate::{
    db::{
        CancelOutcome, PromoteOutcome, SignupOutcome, cancel_booking_for_slot,
        create_upcoming_slots, create_user, find_slot, find_user_by_id, find_user_by_name,
        list_slots, list_users, promote_waitlist_user, signup_for_slot,
    },
    models::{Slot, UserIdentity},
};
use askama::Template;
use axum::{
    Form, Router,
    extract::{Path, State},
    http::{HeaderMap, HeaderName, HeaderValue, header},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
};
use chrono::Utc;
use error::AppError;
use serde::Deserialize;
use sqlx::SqlitePool;
use std::path::Path as FsPath;
use tower_http::trace::TraceLayer;

#[derive(Template)]
#[template(path = "slots.html")]
struct SlotsTemplate {
    title: String,
    current_user_label: String,
    current_user_id: Option<i64>,
    current_user_is_admin: bool,
    slots: Vec<Slot>,
}

#[derive(Template)]
#[template(path = "slots_content.html")]
struct SlotsContentTemplate {
    title: String,
    current_user_label: String,
    current_user_id: Option<i64>,
    current_user_is_admin: bool,
    slots: Vec<Slot>,
}

#[derive(Template)]
#[template(path = "slot_card.html")]
struct SlotCardTemplate {
    slot: Slot,
    current_user_id: Option<i64>,
    current_user_is_admin: bool,
}

#[derive(Template)]
#[template(path = "user_selector.html")]
struct UserSelectorTemplate {
    users: Vec<UserIdentity>,
    selected_user_id: Option<i64>,
    current_user_label: String,
    current_user_is_admin: bool,
}

impl UserSelectorTemplate {
    fn is_selected<T>(&self, user_id: T) -> bool
    where
        T: std::borrow::Borrow<i64>,
    {
        self.selected_user_id == Some(*user_id.borrow())
    }
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
    let pool = db::init_pool(&database_url).await?;
    create_upcoming_slots(&pool, Utc::now()).await?;

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
        .route("/healthz", get(healthz))
        .route("/robots.txt", get(robots))
        .with_state(pool)
        .layer(TraceLayer::new_for_http())
}

async fn index(
    State(pool): State<SqlitePool>,
    headers: HeaderMap,
) -> Result<Html<String>, AppError> {
    let current_user = current_user_from_headers(&pool, &headers).await?;
    let current_user_id = current_user.as_ref().map(|user| user.id);
    let current_user_is_admin = current_user.as_ref().is_some_and(|user| user.is_admin);
    let template = SlotsTemplate {
        title: "Hoopline".to_string(),
        current_user_label: current_user
            .as_ref()
            .map_or_else(|| "Select your name".to_string(), |user| user.name.clone()),
        current_user_id,
        current_user_is_admin,
        slots: list_slots(&pool).await?,
    };
    Ok(Html(template.render()?))
}

async fn slots_fragment(
    State(pool): State<SqlitePool>,
    headers: HeaderMap,
) -> Result<Html<String>, AppError> {
    let current_user = current_user_from_headers(&pool, &headers).await?;
    let current_user_id = current_user.as_ref().map(|user| user.id);
    let current_user_is_admin = current_user.as_ref().is_some_and(|user| user.is_admin);
    let template = SlotsContentTemplate {
        title: "Hoopline".to_string(),
        current_user_label: current_user
            .as_ref()
            .map_or_else(|| "Select your name".to_string(), |user| user.name.clone()),
        current_user_id,
        current_user_is_admin,
        slots: list_slots(&pool).await?,
    };
    Ok(Html(template.render()?))
}

async fn signup_slot(
    State(pool): State<SqlitePool>,
    Path(slot_id): Path<i64>,
    headers: HeaderMap,
) -> Result<Html<String>, AppError> {
    let current_user = current_user_from_headers(&pool, &headers)
        .await?
        .ok_or_else(|| AppError::BadRequest("select your name first".to_string()))?;

    match signup_for_slot(&pool, slot_id, current_user.id).await? {
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
    }

    let slot = find_slot(&pool, slot_id)
        .await?
        .ok_or_else(|| AppError::BadRequest("slot does not exist".to_string()))?;
    let template = SlotCardTemplate {
        slot,
        current_user_id: Some(current_user.id),
        current_user_is_admin: current_user.is_admin,
    };
    Ok(Html(template.render()?))
}

async fn cancel_slot(
    State(pool): State<SqlitePool>,
    Path(slot_id): Path<i64>,
    headers: HeaderMap,
) -> Result<Html<String>, AppError> {
    let current_user = current_user_from_headers(&pool, &headers)
        .await?
        .ok_or_else(|| AppError::BadRequest("select your name first".to_string()))?;

    match cancel_booking_for_slot(&pool, slot_id, current_user.id).await? {
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
        slot,
        current_user_id: Some(current_user.id),
        current_user_is_admin: current_user.is_admin,
    };
    Ok(Html(template.render()?))
}

async fn admin_remove_slot_user(
    State(pool): State<SqlitePool>,
    Path((slot_id, user_id)): Path<(i64, i64)>,
    headers: HeaderMap,
) -> Result<Html<String>, AppError> {
    let current_user = current_admin_from_headers(&pool, &headers).await?;

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
        slot,
        current_user_id: Some(current_user.id),
        current_user_is_admin: true,
    };
    Ok(Html(template.render()?))
}

async fn admin_promote_waitlist_user(
    State(pool): State<SqlitePool>,
    Path((slot_id, user_id)): Path<(i64, i64)>,
    headers: HeaderMap,
) -> Result<Html<String>, AppError> {
    let current_user = current_admin_from_headers(&pool, &headers).await?;

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
        slot,
        current_user_id: Some(current_user.id),
        current_user_is_admin: true,
    };
    Ok(Html(template.render()?))
}

async fn admin_generate_slots(
    State(pool): State<SqlitePool>,
    headers: HeaderMap,
) -> Result<Html<String>, AppError> {
    let current_user = current_admin_from_headers(&pool, &headers).await?;
    create_upcoming_slots(&pool, Utc::now()).await?;
    let template = SlotsContentTemplate {
        title: "Hoopline".to_string(),
        current_user_label: current_user.name.clone(),
        current_user_id: Some(current_user.id),
        current_user_is_admin: true,
        slots: list_slots(&pool).await?,
    };
    Ok(Html(template.render()?))
}

async fn users_fragment(
    State(pool): State<SqlitePool>,
    headers: HeaderMap,
) -> Result<Html<String>, AppError> {
    let current_user = current_user_from_headers(&pool, &headers).await?;
    let selected_user_id = current_user.as_ref().map(|user| user.id);
    let current_user_label = current_user
        .as_ref()
        .map_or_else(|| "Select your name".to_string(), |user| user.name.clone());
    let template = UserSelectorTemplate {
        users: list_users(&pool).await?,
        selected_user_id,
        current_user_label,
        current_user_is_admin: current_user.as_ref().is_some_and(|user| user.is_admin),
    };
    Ok(Html(template.render()?))
}

async fn select_user(
    State(pool): State<SqlitePool>,
    Form(form): Form<UserSelectionForm>,
) -> Result<Response, AppError> {
    let selected_user = if let Some(new_user_name) = form.new_user_name {
        let trimmed_name = new_user_name.trim();
        if trimmed_name.is_empty() {
            return Err(AppError::BadRequest(
                "new user name cannot be empty".to_string(),
            ));
        }
        if let Some(existing_user) = find_user_by_name(&pool, trimmed_name).await? {
            existing_user
        } else {
            create_user(&pool, trimmed_name).await?
        }
    } else if let Some(selected_user_id) = form.selected_user_id {
        find_user_by_id(&pool, selected_user_id)
            .await?
            .ok_or_else(|| AppError::BadRequest("selected user does not exist".to_string()))?
    } else {
        return Err(AppError::BadRequest(
            "select an existing user or create a new one".to_string(),
        ));
    };

    let template = UserSelectorTemplate {
        users: list_users(&pool).await?,
        selected_user_id: Some(selected_user.id),
        current_user_label: selected_user.name.clone(),
        current_user_is_admin: selected_user.is_admin,
    };
    let mut response = Html(template.render()?).into_response();
    response.headers_mut().insert(
        header::SET_COOKIE,
        user_cookie_header_value(selected_user.id)?,
    );
    response.headers_mut().insert(
        HeaderName::from_static("hx-trigger"),
        HeaderValue::from_static("user-changed"),
    );
    Ok(response)
}

async fn robots() -> &'static str {
    "User-agent: *\nDisallow: /"
}

async fn healthz() -> &'static str {
    "ok"
}

async fn current_user_from_headers(
    pool: &SqlitePool,
    headers: &HeaderMap,
) -> Result<Option<UserIdentity>, AppError> {
    if let Some(user_id) = user_id_from_cookie(headers) {
        return find_user_by_id(pool, user_id).await.map_err(AppError::from);
    }
    Ok(None)
}

async fn current_admin_from_headers(
    pool: &SqlitePool,
    headers: &HeaderMap,
) -> Result<UserIdentity, AppError> {
    let current_user = current_user_from_headers(pool, headers)
        .await?
        .ok_or_else(|| AppError::Forbidden("admin access required".to_string()))?;
    if !current_user.is_admin {
        return Err(AppError::Forbidden("admin access required".to_string()));
    }
    Ok(current_user)
}

fn user_id_from_cookie(headers: &HeaderMap) -> Option<i64> {
    let raw_cookie = headers.get(header::COOKIE)?.to_str().ok()?;
    raw_cookie
        .split(';')
        .find_map(|part| {
            let (name, value) = part.trim().split_once('=')?;
            (name == "user_id").then_some(value)
        })
        .and_then(|value| value.parse::<i64>().ok())
}

fn user_cookie_header_value(user_id: i64) -> Result<HeaderValue, AppError> {
    let value = format!("user_id={user_id}; Path=/; Max-Age=31536000; HttpOnly; SameSite=Lax");
    Ok(HeaderValue::from_str(&value)?)
}
