pub mod db;
pub mod error;
pub mod models;

use crate::{db::list_slots, models::Slot};
use askama::Template;
use axum::{Router, extract::State, response::Html, routing::get};
use error::AppError;
use sqlx::SqlitePool;
use std::path::Path;
use tower_http::trace::TraceLayer;

#[derive(Template)]
#[template(path = "slots.html")]
struct SlotsTemplate {
    title: String,
    slots: Vec<Slot>,
}

/// Builds the application router with a migrated `SQLite` connection pool.
///
/// # Errors
///
/// Returns an error when pool initialization or migrations fail.
pub async fn app() -> Result<Router, AppError> {
    let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
        if Path::new("/data").is_dir() {
            "sqlite:///data/hoopline.db".to_string()
        } else {
            "sqlite://hoopline.db".to_string()
        }
    });
    let pool = db::init_pool(&database_url).await?;

    Ok(app_with_pool(pool))
}

pub fn app_with_pool(pool: SqlitePool) -> Router {
    Router::new()
        .route("/", get(index))
        .route("/slots", get(index))
        .route("/healthz", get(healthz))
        .route("/robots.txt", get(robots))
        .with_state(pool)
        .layer(TraceLayer::new_for_http())
}

async fn index(State(pool): State<SqlitePool>) -> Result<Html<String>, AppError> {
    let template = SlotsTemplate {
        title: "Hoopline".to_string(),
        slots: list_slots(&pool).await?,
    };
    Ok(Html(template.render()?))
}

async fn robots() -> &'static str {
    "User-agent: *\nDisallow: /"
}

async fn healthz() -> &'static str {
    "ok"
}
