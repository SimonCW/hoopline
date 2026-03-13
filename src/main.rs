use hoopline::app;

#[tokio::main]
async fn main() {
    let app = app().await.expect("failed to initialize app");

    let listener = tokio::net::TcpListener::bind("0.0.0.0:5050")
        .await
        .expect("failed to bind listener");
    axum::serve(listener, app).await.unwrap();
}
