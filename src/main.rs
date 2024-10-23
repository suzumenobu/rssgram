mod config;
use envconfig::Envconfig;
use grammers_client::{Client, Config};
use grammers_session::Session;
use nanodb::nanodb::NanoDB;
use rss::{Channel, ChannelBuilder};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io;
use std::io::BufReader;
use std::path::Path;

#[derive(Deserialize, Serialize, Debug, Default)]
pub struct ChannelInfo {
    pub last_processed_message_id: i32,
    pub rss_feed_file_name: String,
}

pub trait TelegramChannelRepository {
    fn find_channel_info_by_id(
        &self,
        channel_id: &i64,
    ) -> impl std::future::Future<Output = anyhow::Result<Option<ChannelInfo>>> + Send;

    fn update_channel_info(
        &mut self,
        channel_id: &i64,
        channel_info: &ChannelInfo,
    ) -> impl std::future::Future<Output = anyhow::Result<()>> + Send;
}

struct NanoDbTelegramChannelRepository {
    db: NanoDB,
}

impl NanoDbTelegramChannelRepository {
    pub fn new(db: NanoDB) -> Self {
        Self { db }
    }

    pub async fn save(&mut self) -> anyhow::Result<()> {
        self.db.write().await?;
        Ok(())
    }
}

impl TelegramChannelRepository for NanoDbTelegramChannelRepository {
    async fn find_channel_info_by_id(
        &self,
        channel_id: &i64,
    ) -> anyhow::Result<Option<ChannelInfo>> {
        let key = channel_id.to_string();
        match self.db.data().await.get(&key) {
            Ok(value) => Ok(Some(value.into()?)),
            Err(err) => match err {
                nanodb::error::NanoDBError::KeyNotFound(_) => Ok(None),
                any_other_error => Err(any_other_error.into()),
            },
        }
    }

    async fn update_channel_info(
        &mut self,
        channel_id: &i64,
        channel_info: &ChannelInfo,
    ) -> anyhow::Result<()> {
        let key = channel_id.to_string();
        self.db.insert(&key, channel_info).await?;
        Ok(())
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenv::dotenv().ok();
    env_logger::init();
    let config = config::Config::init_from_env()?;

    std::fs::create_dir_all(&config.base_rss_feed_path)?;

    let client = Client::connect(Config {
        session: Session::load_file_or_create(&config.telegram_session_file_path)?,
        api_id: config.telegram_api_id,
        api_hash: config.telegram_api_hash,
        params: Default::default(),
    })
    .await?;

    if !(client.is_authorized().await?) {
        let token = client
            .request_login_code(&config.telegram_account_phone)
            .await?;
        let mut code = String::new();
        io::stdin()
            .read_line(&mut code)
            .expect("Failed to read line");
        client.sign_in(&token, &code).await?;
    }
    client
        .session()
        .save_to_file(&config.telegram_session_file_path)?;

    let db = NanoDB::open("db.json")?;
    let mut repository = NanoDbTelegramChannelRepository::new(db);
    let mut dialogs = client.iter_dialogs();

    while let Some(dialog) = dialogs.next().await? {
        let chat = dialog.chat();

        match chat {
            grammers_client::types::Chat::Channel(channel) => {
                process_channel(
                    &client,
                    &mut repository,
                    channel,
                    &config.base_rss_feed_path,
                )
                .await?;
            }
            _ => continue,
        }
    }

    repository.save().await?;

    Ok(())
}

async fn process_channel(
    client: &Client,
    repository: &mut impl TelegramChannelRepository,
    channel: &grammers_client::types::chat::Channel,
    base_rss_feed_path: &Path,
) -> anyhow::Result<()> {
    log::info!("{}", channel.title());

    let mut channel_info = repository
        .find_channel_info_by_id(&channel.id())
        .await?
        .unwrap_or(ChannelInfo {
            last_processed_message_id: 0,
            rss_feed_file_name: format!("{}.xml", channel.username().unwrap()),
        });

    let rss_feed_path = base_rss_feed_path.join(&channel_info.rss_feed_file_name);
    let mut rss_channel = match File::open(&rss_feed_path) {
        Ok(file) => Channel::read_from(BufReader::new(file))?,
        Err(_) => ChannelBuilder::default()
            .title(channel.title())
            .link(format!("https://t.me/{}", channel.username().unwrap()))
            .description("Not supported yet")
            .build(),
    };

    let mut messages = client.iter_messages(channel);
    let mut last_message_id = None;

    if let Some(message) = messages.next().await? {
        last_message_id = Some(message.id());
    }

    match last_message_id {
        Some(id) => {
            let messages_to_process =
                std::cmp::min(id - channel_info.last_processed_message_id, 10);
            log::info!("{} messages will be processed", messages_to_process);
            messages = messages.limit(messages_to_process as usize);

            let mut items = Vec::with_capacity(messages_to_process as usize);

            while let Some(message) = messages.next().await? {
                let item = rss::ItemBuilder::default()
                    .title(message.id().to_string())
                    .description(message.text().to_string())
                    .link(message.id().to_string())
                    .build();
                items.push(item);
            }

            items.extend_from_slice(rss_channel.items());
            rss_channel.set_items(items);

            let file = File::create(rss_feed_path)?;
            rss_channel.write_to(file)?;
            channel_info.last_processed_message_id = id;
            repository
                .update_channel_info(&channel.id(), &channel_info)
                .await?;
        }
        None => {
            println!("There is no any unprocessed messages");
        }
    }
    Ok(())
}
