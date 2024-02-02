use std::env;

use anyhow::anyhow;
use matrix_sdk::ruma::OwnedUserId;
use matrix_sdk::Client as MatrixClient;

use super::session::state_db_path;
use super::{session, Client};
use crate::CRATE_NAME;

#[derive(Debug)]
pub(crate) struct ClientBuilder {
    user_id: Option<OwnedUserId>,
    device_name: Option<String>,
}

impl ClientBuilder {
    pub(crate) fn user_id(mut self, user_id: OwnedUserId) -> Self {
        self.user_id = Some(user_id);
        self
    }

    pub(crate) fn device_name(mut self, device_name: String) -> Self {
        self.device_name = Some(device_name);
        self
    }

    pub(crate) fn load_meta(self) -> anyhow::Result<Self> {
        let meta = session::Meta::load().map_err(|e| anyhow!("could not load meta.json: {}", e))?;
        Ok(Self::from(meta))
    }

    pub(crate) async fn build(self) -> anyhow::Result<Client> {
        let Some(user_id) = self.user_id else {
            panic!("no user_id set");
        };
        let Some(device_name) = self.device_name else {
            panic!("no device name set");
        };

        let state_path = state_db_path(user_id.clone())?;

        let mut builder = MatrixClient::builder()
            .server_name(user_id.server_name())
            .sqlite_store(state_path, None);

        if let Ok(proxy) = env::var("HTTPS_PROXY") {
            builder = builder.proxy(proxy);
        }

        if env::var("MN_INSECURE").is_ok() {
            builder = builder.disable_ssl_verification();
        }

        let client = Client {
            inner: builder.build().await?,
            user_id,
            device_name,
        };

        client.connect().await?;

        Ok(client)
    }
}

impl Default for ClientBuilder {
    fn default() -> Self {
        Self {
            user_id: None,
            device_name: Some(CRATE_NAME.to_string()),
        }
    }
}

impl From<session::Meta> for ClientBuilder {
    fn from(config: session::Meta) -> Self {
        let device_name = config.device_name.unwrap_or_else(|| CRATE_NAME.to_string());
        Self {
            user_id: Some(config.user_id),
            device_name: Some(device_name),
        }
    }
}
