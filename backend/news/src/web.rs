use crate::{config::Config, persistence};
use askama::Template;
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, State},
    http::StatusCode,
    response::Html,
    routing::get,
};
use serde_json::{Value, json};
use sqlx::PgPool;
use std::sync::Arc;

#[derive(Clone)]
struct AppState {
    config: Arc<Config>,
    pool: PgPool,
}

#[derive(Template)]
#[template(path = "preparation.html")]
struct Preparation<'a> {
    masthead: &'a str,
}

pub fn router(config: Arc<Config>, pool: PgPool) -> Router {
    Router::new()
        .route("/", get(preparation))
        .route("/health/live", get(live))
        .route("/health/ready", get(ready))
        .layer(DefaultBodyLimit::max(64 * 1024))
        .with_state(AppState { config, pool })
}

async fn preparation(State(state): State<AppState>) -> (StatusCode, Html<String>) {
    let template = Preparation {
        masthead: &state.config.masthead,
    };
    match template.render() {
        Ok(html) => (StatusCode::SERVICE_UNAVAILABLE, Html(html)),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Html("Page indisponible.".into()),
        ),
    }
}

async fn live() -> Json<Value> {
    Json(json!({"status": "ok"}))
}

async fn ready(State(state): State<AppState>) -> (StatusCode, Json<Value>) {
    if persistence::database_available(&state.pool).await {
        (
            StatusCode::OK,
            Json(json!({"status": "database_connected"})),
        )
    } else {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({"status": "database_unavailable"})),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::Request};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    fn app() -> Router {
        let config = Config::from_lookup(|key| match key {
            "DATABASE_URL" => Some("postgres://fixture:fixture-secret@127.0.0.1:1/news".into()),
            "NEWS_MASTHEAD" => Some("<script>alert(1)</script>".into()),
            _ => None,
        })
        .unwrap();
        let pool = persistence::pool(&config);
        router(Arc::new(config), pool)
    }

    #[tokio::test]
    async fn liveness_does_not_depend_on_database() {
        let response = app()
            .oneshot(
                Request::builder()
                    .uri("/health/live")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn readiness_returns_real_503_without_connection_details() {
        let response = app()
            .oneshot(
                Request::builder()
                    .uri("/health/ready")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        assert!(!String::from_utf8_lossy(&body).contains("fixture-secret"));
    }

    #[tokio::test]
    async fn preparation_is_escaped_and_does_not_claim_publication_is_ready() {
        let response = app()
            .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let body = String::from_utf8_lossy(&body);
        assert!(!body.contains("<script>"));
        assert!(body.contains("Édition en préparation"));
    }
}
