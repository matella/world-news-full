use std::{path::Path, process::ExitCode, sync::Arc};
use world_news::{auth, config::Config, legacy, persistence, web};

#[tokio::main]
async fn main() -> Result<ExitCode, Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .init();
    let arguments: Vec<_> = std::env::args().skip(1).collect();
    if arguments
        .first()
        .is_some_and(|arg| arg == "--import-legacy")
    {
        if !(arguments.len() == 2
            || (arguments.len() == 3 && matches!(arguments[2].as_str(), "--apply" | "--dry-run")))
        {
            return Err("Usage: world-news --import-legacy FILE [--dry-run | --apply]".into());
        }
        let policy = Config::editorial_from_env().map_err(std::io::Error::other)?;
        let plan = legacy::plan_file(Path::new(&arguments[1]), &policy.publication_timezone)?;
        let report = if arguments.get(2).is_some_and(|arg| arg == "--apply") {
            let config = Config::from_env().map_err(std::io::Error::other)?;
            let pool = persistence::pool(&config);
            let result = legacy::apply(&pool, &plan).await;
            pool.close().await;
            result?
        } else {
            plan.report
        };
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(if report.quarantined.is_empty() {
            ExitCode::SUCCESS
        } else {
            ExitCode::from(2)
        });
    }
    let config = Arc::new(Config::from_env().map_err(std::io::Error::other)?);
    if arguments == ["--check-config"] {
        println!("Configuration valid; no database connection or provider request made.");
        return Ok(ExitCode::SUCCESS);
    }
    let pool = persistence::pool(&config);
    if arguments.first().is_some_and(|a| a == "--create-admin") {
        if arguments.len() != 3 {
            return Err(
                "Usage: world-news --create-admin USER editor|publisher; password on stdin".into(),
            );
        }
        let role = auth::Role::parse(&arguments[2])?;
        use std::io::Read;
        let mut password = zeroize::Zeroizing::new(String::new());
        std::io::stdin()
            .take(1027)
            .read_to_string(&mut password)
            .map_err(|_| std::io::Error::other("Cannot read password from stdin."))?;
        if password.len() > 1026 {
            return Err("Password input exceeds bootstrap limit.".into());
        }
        if password.ends_with('\n') {
            password.pop();
            if password.ends_with('\r') {
                password.pop();
            }
        }
        let result = auth::bootstrap(&pool, &arguments[1], role, password).await;
        pool.close().await;
        result?;
        println!("Account created; no default credentials or automatic sign-in.");
        return Ok(ExitCode::SUCCESS);
    }
    if arguments.first().is_some_and(|a| a == "--disable-admin") {
        if arguments.len() != 2 {
            return Err("Usage: world-news --disable-admin USER".into());
        }
        let result = auth::disable(&pool, &arguments[1]).await;
        pool.close().await;
        result?;
        println!("Account disabled; all its sessions revoked.");
        return Ok(ExitCode::SUCCESS);
    }
    if arguments == ["--migrate"] {
        let result = persistence::migrate(&pool).await;
        pool.close().await;
        result.map_err(|_| {
            std::io::Error::other(
                "Schema migration failed; verify connectivity, privileges and migration checksums.",
            )
        })?;
        println!("App-owned schema migrated successfully.");
        return Ok(ExitCode::SUCCESS);
    }
    if !arguments.is_empty() {
        return Err(
            "Usage: world-news [--check-config | --migrate | --import-legacy FILE [--dry-run | --apply] | --create-admin USER editor|publisher | --disable-admin USER]"
                .into(),
        );
    }
    let listener = tokio::net::TcpListener::bind(config.bind).await?;
    let editor_pool = persistence::editor_pool(&config);
    tracing::info!(address = %listener.local_addr()?, "Rust newspaper foundation listening");
    axum::serve(
        listener,
        web::router_with_editor(config, pool.clone(), editor_pool.clone()),
    )
    .with_graceful_shutdown(shutdown())
    .await?;
    pool.close().await;
    if let Some(editor_pool) = editor_pool {
        editor_pool.close().await;
    }
    Ok(ExitCode::SUCCESS)
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
