mod error;

use askama::Template;
use axum::{Router, response::Html, routing::get};
use error::AppError;

#[derive(Template)]
#[template(path = "hello.html")]
struct HelloTemplate {
    name: String,
}

#[tokio::main]
async fn main() {
    let app = Router::new().route("/", get(hello_world));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn hello_world() -> Result<Html<String>, AppError> {
    let template = HelloTemplate {
        name: "Tt".to_string(),
    };
    Ok(Html(template.render()?))
}
