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
    extract::{Path, Request, State},
    http::{header, HeaderValue},
    middleware::{self, Next},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use envconfig::Envconfig;
use nanodb::nanodb::NanoDB;
use tokio::sync::mpsc;
use tower_http::services::ServeDir;

use std::time::Duration;

use crate::actor::AppActorMessage;

#[derive(Clone)]
struct ActorSenderState {
    tx: mpsc::Sender<AppActorMessage>,
}

#[derive(Clone)]
struct AppConfigState {
    config: config::Config,
}

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

    let app_config_state = AppConfigState {
        config: config.clone(),
    };
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

    let app_actor_sender_state = ActorSenderState { tx: tx.clone() };

    let rss_feeds_service = ServeDir::new(&config.base_rss_feed_path);
    let rss_api = Router::new()
        .route("/", get(get_available_rss_feeds))
        .nest_service(
            "/feed",
            tower::ServiceBuilder::new()
                .layer(middleware::from_fn(set_rss_feed_headers))
                .service(rss_feeds_service),
        )
        .with_state(app_config_state);

    let app = Router::new()
        .route(
            "/channel/:telegram_channel_username",
            post(post_add_channel),
        )
        .nest("/rss", rss_api)
        .with_state(app_actor_sender_state);

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

async fn post_add_channel(
    State(state): State<ActorSenderState>,
    Path(telegram_channel_username): Path<String>,
) -> impl IntoResponse {
    let channel_username = telegram_channel_username.replace("@", "");
    match state
        .tx
        .send(AppActorMessage::AddTelegramChannel { channel_username })
        .await
    {
        Ok(_) => "OK".to_string(),
        Err(err) => format!("Failed to add telegram channel with {}", err),
    }
}

async fn get_available_rss_feeds(State(state): State<AppConfigState>) -> impl IntoResponse {
    let mut feeds_dir = tokio::fs::read_dir(state.config.base_rss_feed_path)
        .await
        .unwrap();
    let mut feeds = vec![];
    while let Some(feed) = feeds_dir.next_entry().await.unwrap() {
        feeds.push(feed.file_name())
    }
    Json(
        feeds
            .iter()
            .map(|feed| format!("/feed/{}", feed.to_str().unwrap()))
            .collect::<Vec<_>>(),
    )
}
