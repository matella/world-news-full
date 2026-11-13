use std::sync::Arc;
use world_news::{config::Config, persistence, web};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();
    let config = Arc::new(Config::from_env().map_err(std::io::Error::other)?);
    let arguments: Vec<_> = std::env::args().skip(1).collect();
    if arguments == ["--check-config"] {
        println!("Configuration valid; no database connection or provider request made.");
        return Ok(());
    }
    let pool = persistence::pool(&config);
    if arguments == ["--migrate"] {
        let result = persistence::migrate(&pool).await;
        pool.close().await;
        result.map_err(|_| {
            std::io::Error::other(
                "Schema migration failed; verify connectivity, privileges and migration checksums.",
            )
        })?;
        println!("App-owned schema migrated successfully.");
        return Ok(());
    }
    if !arguments.is_empty() {
        return Err("Usage: world-news [--check-config | --migrate]".into());
    }
    let listener = tokio::net::TcpListener::bind(config.bind).await?;
    tracing::info!(address = %listener.local_addr()?, "Rust newspaper foundation listening");
    axum::serve(listener, web::router(config, pool.clone()))
        .with_graceful_shutdown(shutdown())
        .await?;
    pool.close().await;
    Ok(())
}

async fn shutdown() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let terminate = async {
        if let Ok(mut signal) =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            signal.recv().await;
        } else {
            std::future::pending::<()>().await;
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! { () = ctrl_c => {}, () = terminate => {} }
}
