use crate::app_state::AppState;
use crate::{repository, telegram2rss};
use tokio::sync::mpsc;

#[derive(Debug)]
pub enum AppActorMessage {
    SyncRssFeeds,
}

pub async fn run(mut actor: AppActor<impl repository::TelegramChannelRepository>) {
    log::info!("Starting main actor");
    while let Some(msg) = actor.receiver.recv().await {
        log::info!("Got new msg");
        match actor.handle_message(msg).await {
            Err(e) => log::error!("Failed to process with {}", e),
            Ok(_) => log::info!("Msg processed"),
        }
    }
}

pub struct AppActor<T>
where
    T: repository::TelegramChannelRepository,
{
    state: AppState<T>,
    receiver: mpsc::Receiver<AppActorMessage>,
}

impl<T> AppActor<T>
where
    T: repository::TelegramChannelRepository,
{
    pub fn new(state: AppState<T>, receiver: mpsc::Receiver<AppActorMessage>) -> Self {
        Self { state, receiver }
    }

    async fn handle_message(&mut self, msg: AppActorMessage) -> anyhow::Result<()> {
        match msg {
            AppActorMessage::SyncRssFeeds => {
                telegram2rss::update_rss_feeds(
                    &self.state.client,
                    &mut self.state.repository,
                    &self.state.config.base_rss_feed_path,
                )
                .await?
            }
        }
        Ok(())
    }
}
