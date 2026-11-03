use askama::Template;
use axum::http::StatusCode;
use axum::http::header::InvalidHeaderValue;
use axum::response::{Html, IntoResponse, Response};

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("could not create local database directory")]
    Io(#[from] std::io::Error),
    #[error("could not render template")]
    Render(#[from] askama::Error),
    #[error("database error")]
    Database(#[from] sqlx::Error),
    #[error("migration error")]
    Migration(#[from] sqlx::migrate::MigrateError),
    #[error("invalid response header value")]
    InvalidHeaderValue(#[from] InvalidHeaderValue),
    #[error("{0}")]
    BadRequest(String),
    #[error("{0}")]
    Forbidden(String),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        #[derive(Debug, Template)]
        #[template(path = "error.html")]
        struct ErrorTemplate {
            message: String,
        }

        let status = match &self {
            AppError::Database(sqlx::Error::Database(error)) if error.is_unique_violation() => {
                StatusCode::CONFLICT
            }
            AppError::Database(sqlx::Error::Database(error))
                if error
                    .code()
                    .is_some_and(|code| matches!(code.as_ref(), "5" | "6" | "261" | "517")) =>
            {
                StatusCode::SERVICE_UNAVAILABLE
            }
            AppError::Io(_)
            | AppError::Render(_)
            | AppError::Database(_)
            | AppError::Migration(_)
            | AppError::InvalidHeaderValue(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::BadRequest(_) => StatusCode::BAD_REQUEST,
            AppError::Forbidden(_) => StatusCode::FORBIDDEN,
        };
        let message = match status {
            StatusCode::CONFLICT => "The list changed while you were booking. Refresh the slots and try again.".into(),
            StatusCode::SERVICE_UNAVAILABLE => "The booking system is busy. Wait a moment, then refresh and try again.".into(),
            StatusCode::INTERNAL_SERVER_ERROR => "We could not complete this request. Refresh and check your booking before trying again.".into(),
            _ => self.to_string(),
        };
        let tmpl = ErrorTemplate { message };
        if let Ok(body) = tmpl.render() {
            (status, Html(body)).into_response()
        } else {
            (status, "Something went wrong").into_response()
        }
    }
}
