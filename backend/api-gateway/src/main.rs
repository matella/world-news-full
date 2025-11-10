use async_graphql::{EmptyMutation, EmptySubscription, Object, Schema};
use async_graphql_axum::{GraphQLRequest, GraphQLResponse};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::middleware::map_response;
use axum::response::{IntoResponse, Response};
use axum::{
    extract::Extension,
    routing::{get, post},
    Json, Router,
};
use serde_json::json;
use std::net::SocketAddr;
use tracing::info;
use tracing_subscriber;

struct QueryRoot;

#[Object]
impl QueryRoot {
    async fn hello(&self) -> &str {
        "Hello from API Gateway"
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let schema = Schema::build(QueryRoot, EmptyMutation, EmptySubscription).finish();

    let graphiql_route = get(graphiql_handler);

    // Provide a simple OPTIONS handler for CORS preflight and add CORS headers
    // to GraphQL responses. This is intentionally permissive for development.
    async fn options_handler() -> impl IntoResponse {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::ACCESS_CONTROL_ALLOW_ORIGIN,
            HeaderValue::from_static("*"),
        );
        headers.insert(
            header::ACCESS_CONTROL_ALLOW_METHODS,
            HeaderValue::from_static("GET, POST, OPTIONS"),
        );
        headers.insert(
            header::ACCESS_CONTROL_ALLOW_HEADERS,
            HeaderValue::from_static("content-type"),
        );
        (headers, StatusCode::NO_CONTENT)
    }

    let graphql_route = post(graphql_handler).options(options_handler);

    // map_response middleware to inject permissive CORS headers on all responses
    let add_cors_layer = map_response(|mut res: Response| async move {
        res.headers_mut().insert(
            header::ACCESS_CONTROL_ALLOW_ORIGIN,
            HeaderValue::from_static("*"),
        );
        res.headers_mut().insert(
            header::ACCESS_CONTROL_ALLOW_HEADERS,
            HeaderValue::from_static("*"),
        );
        res.headers_mut().insert(
            header::ACCESS_CONTROL_ALLOW_METHODS,
            HeaderValue::from_static("GET, POST, OPTIONS"),
        );
        res
    });

    let app = Router::new()
        .route("/", get(root))
        .route("/graphql", graphql_route)
        .route("/graphiql", graphiql_route)
        .layer(Extension(schema))
        .layer(add_cors_layer);

    let addr = SocketAddr::from(([127, 0, 0, 1], 8000));

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    info!("AI Service listening on {}", addr);
    axum::serve(listener, app).await.unwrap();
    Ok(())
}

async fn root() -> Json<serde_json::Value> {
    Json(json!({"status":"ok","message":"World News API Gateway"}))
}

async fn graphql_handler(
    Extension(schema): Extension<Schema<QueryRoot, EmptyMutation, EmptySubscription>>,
    req: GraphQLRequest,
) -> impl IntoResponse {
    let resp = schema.execute(req.into_inner()).await;
    let body = GraphQLResponse::from(resp);

    let mut headers = HeaderMap::new();
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_ORIGIN,
        HeaderValue::from_static("*"),
    );
    (headers, body)
}

async fn graphiql_handler() -> axum::response::Html<String> {
    // Simple GraphiQL HTML that points to /graphql
    let html = r#"<!doctype html>
<html>
  <head>
    <meta charset=utf-8/>
    <title>GraphiQL</title>
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <link href="https://unpkg.com/graphiql/graphiql.min.css" rel="stylesheet" />
  </head>
  <body style="height:100vh;margin:0">
    <div id="graphiql" style="height:100vh;"></div>
    <script crossorigin src="https://unpkg.com/react/umd/react.production.min.js"></script>
    <script crossorigin src="https://unpkg.com/react-dom/umd/react-dom.production.min.js"></script>
    <script src="https://unpkg.com/graphiql/graphiql.min.js"></script>
    <script>
      const fetcher = GraphiQL.createFetcher({ url: '/graphql' });
      ReactDOM.render(React.createElement(GraphiQL, { fetcher }), document.getElementById('graphiql'));
    </script>
  </body>
</html>"#;
    axum::response::Html(html.to_string())
}
