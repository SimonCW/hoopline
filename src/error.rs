use askama::Template;
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("could not render template")]
    Render(#[from] askama::Error),
    #[error("database error")]
    Database(#[from] sqlx::Error),
    #[error("migration error")]
    Migration(#[from] sqlx::migrate::MigrateError),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        #[derive(Debug, Template)]
        #[template(path = "error.html")]
        struct ErrorTemplate {
            message: String,
        }

        let status = match &self {
            AppError::Render(_) | AppError::Database(_) | AppError::Migration(_) => {
                StatusCode::INTERNAL_SERVER_ERROR
            }
        };
        let tmpl = ErrorTemplate {
            message: self.to_string(),
        };
        if let Ok(body) = tmpl.render() {
            (status, Html(body)).into_response()
        } else {
            (status, "Something went wrong").into_response()
        }
    }
}
