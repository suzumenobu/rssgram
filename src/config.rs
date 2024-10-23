use std::path::PathBuf;

use envconfig::Envconfig;

#[derive(Envconfig, Clone)]
pub struct Config {
    pub telegram_api_id: i32,
    pub telegram_api_hash: String,
    pub telegram_account_phone: String,
    pub telegram_session_file_path: PathBuf,
    pub base_rss_feed_path: PathBuf,
    pub rss_feeds_update_interval_secs: u64,
}
