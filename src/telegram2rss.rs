use std::{fs::File, io::BufReader, path::Path};

use grammers_client::Client;
use rss::{Channel, ChannelBuilder};

use crate::{domain::ChannelInfo, repository};

pub async fn update_rss_feeds(
    client: &Client,
    repository: &mut impl repository::TelegramChannelRepository,
    base_rss_feed_path: &Path,
) -> anyhow::Result<()> {
    log::info!("Starting RSS feeds update");
    let mut dialogs = client.iter_dialogs();
    let mut processed_dialogs = 0;
    let mut total_updates_count = 0;
    while let Some(dialog) = dialogs.next().await? {
        let chat = dialog.chat();

        match chat {
            grammers_client::types::Chat::Channel(channel) => {
                total_updates_count +=
                    process_channel(client, repository, channel, base_rss_feed_path).await?;
                processed_dialogs += 1;
            }
            _ => continue,
        }
    }
    log::info!(
        "RSS feeds update finished. Processed {} channels with {} new messages",
        processed_dialogs,
        total_updates_count
    );
    Ok(())
}

pub async fn add_rss_feed(
    client: &Client,
    base_rss_feed_path: &Path,
    channel_username: &str,
) -> anyhow::Result<()> {
    log::info!("Adding new channel: {}", channel_username);
    let rss_feed_path = base_rss_feed_path.join(format!("{}.xml", channel_username));
    match tokio::fs::metadata(rss_feed_path).await {
        Ok(metadata) => {
            if metadata.is_file() {
                log::info!("Feed already exists for {}", channel_username);
            } else {
                log::warn!("Not a file for feed {}", channel_username);
            }
            return Ok(());
        }
        Err(_) => {
            log::info!("Feed does not exist");
        }
    }

    match client.resolve_username(channel_username).await? {
        Some(chat) => match chat {
            grammers_client::types::Chat::Channel(channel) => {
                client.join_chat(channel).await?;
                log::info!("Successfully joined to {}", channel_username);
            }
            _ => log::error!("Not a channel"),
        },
        None => log::error!("Chat not found"),
    }

    Ok(())
}

async fn process_channel(
    client: &Client,
    repository: &mut impl repository::TelegramChannelRepository,
    channel: &grammers_client::types::chat::Channel,
    base_rss_feed_path: &Path,
) -> anyhow::Result<usize> {
    log::debug!("Starting processing of [{}] ", channel.title());
    let mut updates_count = 0;

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
            log::debug!("{} messages will be processed", messages_to_process);
            messages = messages.limit(messages_to_process as usize);

            let mut items = Vec::with_capacity(messages_to_process as usize);

            while let Some(message) = messages.next().await? {
                let item = rss::ItemBuilder::default()
                    .title(message.id().to_string())
                    .description(message.text().to_string())
                    .link(format!(
                        "https://t.me/{}/{}",
                        channel.username().unwrap(),
                        message.id()
                    ))
                    .build();
                items.push(item);
            }

            updates_count += items.len();
            items.extend_from_slice(rss_channel.items());
            rss_channel.set_items(items);

            let file = File::create(rss_feed_path)?;
            rss_channel.pretty_write_to(file, b' ', 2)?;
            channel_info.last_processed_message_id = id;
            repository
                .update_channel_info(&channel.id(), &channel_info)
                .await?;
        }
        None => {
            log::debug!("There is no any unprocessed messages");
        }
    }
    Ok(updates_count)
}
