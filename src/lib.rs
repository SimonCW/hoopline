pub mod error;

use askama::Template;
use axum::{response::Html, routing::get, Router};
use error::AppError;
use tower_http::trace::TraceLayer;

#[derive(Template)]
#[template(path = "index.html")]
struct IndexTemplate {
    title: String,
}

pub fn app() -> Router {
    Router::new()
        .route("/", get(index))
        .route("/robots.txt", get(robots))
        .layer(TraceLayer::new_for_http())
}

async fn index() -> Result<Html<String>, AppError> {
    let template = IndexTemplate {
        title: "Hoopline".to_string(),
    };
    Ok(Html(template.render()?))
}

async fn robots() -> &'static str {
    "User-agent: *\nDisallow: /"
}
