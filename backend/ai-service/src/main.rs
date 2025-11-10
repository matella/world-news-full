use anyhow::Result;
use axum::{
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use tracing::info;

#[derive(Deserialize)]
struct ProcessRequest {
    text: String,
}

#[derive(Serialize)]
struct ProcessResponse {
    title: String,
    summary: String,
    body: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    // initialize tracing
    tracing_subscriber::fmt::init();

    // build our application with a route
    let app = Router::new()
        // `GET /` goes to `root`
        .route("/", get(root))
        // `POST /users` goes to `create_user`
        .route("/process", post(process_handler));

    // run our app with hyper, listening globally on port 3000
    let addr = "127.0.0.1:9000";
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    info!("AI Service listening on {}", addr);
    axum::serve(listener, app).await.unwrap();
    Ok(())
}

// basic handler that responds with a static string
async fn root() -> &'static str {
    "Hello, World!"
}

async fn process_handler(Json(payload): Json<ProcessRequest>) -> Json<ProcessResponse> {
    // Mock response for now
    let title = format!(
        "Summary: {}...",
        &payload.text.chars().take(20).collect::<String>()
    );
    let summary = format!(
        "This is a mock summary for: {}",
        &payload.text.chars().take(80).collect::<String>()
    );
    let body = payload.text.clone();

    Json(ProcessResponse {
        title,
        summary,
        body,
    })
}
