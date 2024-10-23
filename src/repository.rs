use nanodb::nanodb::NanoDB;

use crate::ChannelInfo;

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

pub struct NanoDbTelegramChannelRepository {
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
