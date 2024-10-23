use grammers_client::{Client, Config};
use grammers_session::Session;

use crate::config;

pub async fn init_client(config: &config::Config) -> anyhow::Result<Client> {
    let client = Client::connect(Config {
        session: Session::load_file_or_create(&config.telegram_session_file_path)?,
        api_id: config.telegram_api_id,
        api_hash: config.telegram_api_hash.to_string(),
        params: Default::default(),
    })
    .await?;

    if !(client.is_authorized().await?) {
        let token = client
            .request_login_code(&config.telegram_account_phone)
            .await?;
        let mut code = String::new();
        std::io::stdin()
            .read_line(&mut code)
            .expect("Failed to read line");
        client.sign_in(&token, &code).await?;
    }
    client
        .session()
        .save_to_file(&config.telegram_session_file_path)?;
    Ok(client)
}
