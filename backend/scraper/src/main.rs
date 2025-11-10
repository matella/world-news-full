use std::time::Duration;
use tokio::time::sleep;
use tracing_subscriber;
use tracing::info;
use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    info!("Scraper service starting (mock mode)...");

    loop {
        info!("Scraper: fetching feeds (mock)...");
        sleep(Duration::from_secs(60)).await;
    }
}
