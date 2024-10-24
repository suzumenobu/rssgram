mod actor;
mod app_state;
mod config;
mod domain;
mod repository;
mod telegram;
mod telegram2rss;

use actor::AppActor;
use app_state::AppState;
use axum::{
    extract::Request,
    http::{header, HeaderValue},
    middleware::{self, Next},
    response::IntoResponse,
    Router,
};
use envconfig::Envconfig;
use nanodb::nanodb::NanoDB;
use tokio::sync::mpsc;
use tower_http::services::ServeDir;

use std::time::Duration;

use crate::actor::AppActorMessage;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenv::dotenv().ok();
    env_logger::init();
    let config = config::Config::init_from_env()?;

    std::fs::create_dir_all(&config.base_rss_feed_path)?;
    log::info!(
        "Serving RSS feeds from {}",
        &config.base_rss_feed_path.display()
    );

    let client = telegram::init_client(&config).await?;

    let db = NanoDB::open("db.json")?;
    let repository = repository::NanoDbTelegramChannelRepository::new(db);

    let state = AppState {
        repository,
        client,
        config: config.clone(),
    };

    let (tx, rx) = mpsc::channel(100);
    let app_actor = AppActor::new(state, rx);

    let update_rss_feeds_tx = tx.clone();
    let update_interval = Duration::from_secs(config.rss_feeds_update_interval_secs);
    tokio::spawn(async move {
        loop {
            log::info!("Sending RSS update msg");
            if let Err(err) = update_rss_feeds_tx
                .send(AppActorMessage::SyncRssFeeds)
                .await
            {
                log::error!("Failed to send update RSS feed with {}", err);
            }

            tokio::time::sleep(update_interval).await;
        }
    });

    tokio::spawn(async move { actor::run(app_actor).await });

    let rss_feeds_service = ServeDir::new(&config.base_rss_feed_path);
    let app = Router::new().nest_service(
        "/rss",
        tower::ServiceBuilder::new()
            .layer(middleware::from_fn(set_rss_feed_headers))
            .service(rss_feeds_service),
    );

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await.unwrap();
    axum::serve(listener, app).await.unwrap();

    Ok(())
}

async fn set_rss_feed_headers(request: Request, next: Next) -> impl IntoResponse {
    let mut response = next.run(request).await;
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/xml"),
    );
    response
}
