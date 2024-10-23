use crate::{config, repository};
use grammers_client::Client;

#[derive(Clone)]
pub struct AppState<T>
where
    T: repository::TelegramChannelRepository,
{
    pub repository: T,
    pub config: config::Config,
    pub client: Client,
}
