use super::*;
use axum::{body::Body, http::Request};
use http_body_util::BodyExt;
use tower::ServiceExt;

fn state(timeout: Duration) -> AppState {
    let config = Config::from_lookup(|key| match key {
        "DATABASE_URL" => Some("postgres://fixture:fixture-secret@127.0.0.1:1/news".into()),
        "NEWS_MASTHEAD" => Some("<script>alert(1)</script>".into()),
        "PUBLIC_URL" => Some("https://news.invalid".into()),
        _ => None,
    })
    .unwrap();
    let pool = persistence::pool(&config);
    AppState {
        config: Arc::new(config),
        pool,
        request_timeout: timeout,
        auth: None,
    }
}
async fn app() -> Router {
    let state = state(REQUEST_TIMEOUT);
    state.pool.close().await;
    router(state.config, state.pool)
}
async fn request(app: Router, method: &str, uri: &str, body: Body) -> Response {
    app.oneshot(
        Request::builder()
            .method(method)
            .uri(uri)
            .body(body)
            .unwrap(),
    )
    .await
    .unwrap()
}
async fn text(response: Response) -> String {
    String::from_utf8(
        response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .to_vec(),
    )
    .unwrap()
}

#[tokio::test]
async fn live_and_error_boundaries_are_safe_without_database() {
    let app = app().await;
    let live = request(app.clone(), "GET", "/health/live", Body::empty()).await;
    assert_eq!(live.status(), StatusCode::OK);
    assert_eq!(live.headers()["cache-control"], "no-store");
    assert!(!live.headers().contains_key("access-control-allow-origin"));
    let first_id = live.headers()["x-request-id"].clone();
    let unavailable = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/story/unknown?q=sensitive-query")
                .header("x-request-id", "untrusted-client-id")
                .header("host", "evil.invalid")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(unavailable.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_ne!(unavailable.headers()["x-request-id"], first_id);
    assert_ne!(unavailable.headers()["x-request-id"], "untrusted-client-id");
    assert_eq!(unavailable.headers()["retry-after"], "60");
    assert_eq!(unavailable.headers()["x-robots-tag"], "noindex");
    let body = text(unavailable).await;
    for forbidden in [
        "<script>",
        "fixture-secret",
        "sensitive-query",
        "evil.invalid",
    ] {
        assert!(!body.contains(forbidden));
    }
    assert!(body.contains("&#60;script&#62;"));
    assert_eq!(
        request(app.clone(), "GET", "/health/ready", Body::empty())
            .await
            .status(),
        StatusCode::SERVICE_UNAVAILABLE
    );
    assert_eq!(
        request(app.clone(), "GET", "/", Body::empty())
            .await
            .status(),
        StatusCode::SERVICE_UNAVAILABLE
    );
    assert_eq!(
        request(app, "GET", "/unknown", Body::empty())
            .await
            .status(),
        StatusCode::NOT_FOUND
    );
}
#[tokio::test]
async fn malformed_input_is_rejected_before_any_database_access() {
    let app = app().await;
    for uri in [
        "/edition/2026-02-30",
        "/edition/2026-2-03",
        "/edition/0000-01-01",
        "/story/a%2Fb",
        "/story/a%0Ab",
        "/story/%FF",
        "/story/%ZZ",
        "/story/%2E%2E",
        "/story/a?revision=x%2Fy",
        "/archive?before_date=2026-01-01",
        "/archive?before_id=a&before_date=not-date",
        "/recherche?q=a&q=b",
        "/recherche?q=%ZZ",
        "/recherche?q=%FF",
        "/recherche?q=%0A",
    ] {
        assert_eq!(
            request(app.clone(), "GET", uri, Body::empty())
                .await
                .status(),
            StatusCode::BAD_REQUEST,
            "{uri}"
        );
    }
    let uri = format!("/recherche?q={}", "x".repeat(201));
    assert_eq!(
        request(app.clone(), "GET", &uri, Body::empty())
            .await
            .status(),
        StatusCode::BAD_REQUEST
    );
    let uri = format!("/recherche?q={}", "x".repeat(4097));
    assert_eq!(
        request(app, "GET", &uri, Body::empty()).await.status(),
        StatusCode::URI_TOO_LONG
    );
}
#[tokio::test]
async fn redirects_are_relative_and_preserve_query_without_reflecting_host() {
    let app = app().await;
    for uri in [
        "/archive/?paper=cream",
        "/story/legacy-a/",
        "/edition/2026-01-01/",
        "/sujets/",
    ] {
        let response = request(app.clone(), "GET", uri, Body::empty()).await;
        assert_eq!(response.status(), StatusCode::PERMANENT_REDIRECT);
        let location = response.headers()["location"].to_str().unwrap();
        assert!(location.starts_with('/') && !location.starts_with("//"));
        assert!(!location.split('?').next().unwrap().ends_with('/'));
    }
    assert_eq!(
        request(app.clone(), "GET", "//evil.invalid/", Body::empty())
            .await
            .status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        request(app, "POST", "/archive/", Body::empty())
            .await
            .status(),
        StatusCode::NOT_FOUND
    );
}
#[tokio::test]
async fn unimplemented_features_fail_honestly_and_writes_are_unavailable() {
    let app = app().await;
    for uri in ["/recherche", "/recherche?q=archive", "/sujets"] {
        assert_eq!(
            request(app.clone(), "GET", uri, Body::empty())
                .await
                .status(),
            StatusCode::NOT_IMPLEMENTED
        );
    }
    assert_eq!(
        request(app.clone(), "GET", "/admin", Body::empty())
            .await
            .status(),
        StatusCode::SERVICE_UNAVAILABLE
    );
    let response = request(app.clone(), "POST", "/story/id", Body::empty()).await;
    assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
    let methods: Vec<_> = response.headers()["allow"]
        .to_str()
        .unwrap()
        .split(',')
        .map(str::trim)
        .collect();
    assert_eq!(methods, ["GET", "HEAD"]);
    let logout = request(app.clone(), "GET", "/admin/logout", Body::empty()).await;
    assert_eq!(logout.status(), StatusCode::METHOD_NOT_ALLOWED);
    assert_eq!(logout.headers()["allow"], "POST");
    assert_eq!(
        request(app.clone(), "OPTIONS", "/admin", Body::empty())
            .await
            .status(),
        StatusCode::METHOD_NOT_ALLOWED
    );
    let response = request(app, "HEAD", "/health/live", Body::empty()).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert!(text(response).await.is_empty());
}
#[tokio::test]
async fn bodies_are_actually_limited_even_on_routes_without_body_extractors() {
    let app = app().await;
    let response = request(
        app.clone(),
        "GET",
        "/health/live",
        Body::from(vec![b'x'; MAX_BODY + 1]),
    )
    .await;
    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
    let response = app
        .oneshot(
            Request::builder()
                .uri("/unknown")
                .header("content-length", (MAX_BODY + 1).to_string())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
}
#[tokio::test]
async fn timeout_cancels_the_handler_future_and_keeps_request_metadata() {
    use std::sync::atomic::AtomicBool;
    struct Guard(Arc<AtomicBool>);
    impl Drop for Guard {
        fn drop(&mut self) {
            self.0.store(true, Ordering::SeqCst);
        }
    }
    let cancelled = Arc::new(AtomicBool::new(false));
    let flag = cancelled.clone();
    let routes = Router::new().route(
        "/slow",
        get(move || async move {
            let _guard = Guard(flag);
            std::future::pending::<()>().await;
            "never"
        }),
    );
    let response = request(
        with_boundary(routes, state(Duration::from_millis(20))),
        "GET",
        "/slow",
        Body::empty(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert!(response.headers().contains_key("x-request-id"));
    assert!(cancelled.load(Ordering::SeqCst));
}
#[test]
fn identifiers_are_encoded_as_single_segments_and_queries() {
    let path = path_segment("story", "opaque?#%é");
    assert_eq!(path, "/story/opaque%3F%23%25%C3%A9");
    assert_eq!(
        with_query(&path, &[("revision", "a&b")]),
        format!("{path}?revision=a%26b")
    );
}
