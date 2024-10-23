mod config;
mod repository;
mod telegram;
mod telegram2rss;
mod domain;

use envconfig::Envconfig;
use nanodb::nanodb::NanoDB;

use std::time::Duration;


#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenv::dotenv().ok();
    env_logger::init();
    let config = config::Config::init_from_env()?;

    std::fs::create_dir_all(&config.base_rss_feed_path)?;

    let client = telegram::init_client(&config).await?;

    let db = NanoDB::open("db.json")?;
    let mut repository = repository::NanoDbTelegramChannelRepository::new(db);

    telegram2rss::watch_updates(
        &client,
        &mut repository,
        &config.base_rss_feed_path,
        Duration::from_secs(config.rss_feeds_update_interval_secs),
    )
    .await?;

    Ok(())
}

