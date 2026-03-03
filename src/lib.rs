pub mod error;
pub mod models;

use askama::Template;
use axum::{response::Html, routing::get, Router};
use error::AppError;
use models::Slot;
use tower_http::trace::TraceLayer;

#[derive(Template)]
#[template(path = "slots.html")]
struct SlotsTemplate {
    title: String,
    slots: Vec<Slot>,
}

pub fn app() -> Router {
    Router::new()
        .route("/", get(index))
        .route("/robots.txt", get(robots))
        .layer(TraceLayer::new_for_http())
}

async fn index() -> Result<Html<String>, AppError> {
    let template = SlotsTemplate {
        title: "Hoopline".to_string(),
        slots: Slot::sample_slots(),
    };
    Ok(Html(template.render()?))
}

async fn robots() -> &'static str {
    "User-agent: *\nDisallow: /"
}
