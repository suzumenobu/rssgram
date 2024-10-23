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
    while let Some(dialog) = dialogs.next().await? {
        let chat = dialog.chat();

        match chat {
            grammers_client::types::Chat::Channel(channel) => {
                process_channel(client, repository, channel, base_rss_feed_path).await?;
            }
            _ => continue,
        }
    }
    Ok(())
}

async fn process_channel(
    client: &Client,
    repository: &mut impl repository::TelegramChannelRepository,
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
                    .link(format!(
                        "https://t.me/{}/{}",
                        channel.username().unwrap(),
                        message.id()
                    ))
                    .build();
                items.push(item);
            }

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
            println!("There is no any unprocessed messages");
        }
    }
    Ok(())
}
