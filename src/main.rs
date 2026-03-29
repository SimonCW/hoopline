use hoopline::app;

#[tokio::main]
async fn main() {
    let app = app().await.expect("failed to initialize app");
    let port = std::env::var("PORT")
        .ok()
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(5050);

    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{port}"))
        .await
        .expect("failed to bind listener");
    axum::serve(listener, app).await.unwrap();
}
