use hoopline::app;

#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), String> {
    let app = app()
        .await
        .map_err(|error| format!("failed to initialize app: {error}"))?;
    let port = std::env::var("PORT")
        .ok()
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(5050);

    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{port}"))
        .await
        .map_err(|error| format!("failed to bind listener: {error}"))?;
    axum::serve(listener, app)
        .await
        .map_err(|error| format!("server crashed: {error}"))?;
    Ok(())
}
