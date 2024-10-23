use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Debug, Default)]
pub struct ChannelInfo {
    pub last_processed_message_id: i32,
    pub rss_feed_file_name: String,
}
