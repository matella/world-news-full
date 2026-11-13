use anyhow::Result;
use std::time::Duration;
use tokio::time::sleep;
use tracing::info;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    info!("Scraper service starting (mock mode)...");

    loop {
        info!("Scraper: fetching feeds (mock)...");
        sleep(Duration::from_secs(60)).await;
    }
}
