use std::env;
use std::ops::Deref;

use anyhow::Context;
use matrix_sdk::Client as MatrixClient;
use matrix_sdk::ruma::OwnedUserId;

use crate::CRATE_NAME;

pub mod room;
pub mod sas;
pub mod session;

pub(crate) use room::TextKind;

pub(crate) struct Client {
    inner: MatrixClient,
    user_id: OwnedUserId,
    device_name: String,
}

impl Client {
    /// Build a client for `user_id`, restoring a persisted session if one exists.
    pub(crate) async fn new(user_id: OwnedUserId, device_name: String) -> anyhow::Result<Self> {
        let mut builder = MatrixClient::builder()
            .server_name(user_id.server_name())
            .sqlite_store(session::state_db_path(&user_id)?, None);

        if let Ok(proxy) = env::var("HTTPS_PROXY") {
            builder = builder.proxy(proxy);
        }
        if env::var_os("MN_INSECURE").is_some() {
            builder = builder.disable_ssl_verification();
        }

        let client = Self {
            inner: builder.build().await?,
            user_id,
            device_name,
        };

        if let Some(session) = client.load_session()? {
            client.inner.restore_session(session).await?;
        }

        Ok(client)
    }

    /// Build a client from the persisted `meta.json`.
    pub(crate) async fn from_meta() -> anyhow::Result<Self> {
        let meta = session::Meta::load().context("could not load meta.json")?;
        let device_name = meta.device_name.unwrap_or_else(|| CRATE_NAME.to_string());
        Self::new(meta.user_id, device_name).await
    }

    pub(crate) fn logged_in(&self) -> bool {
        self.inner.matrix_auth().logged_in()
    }

    pub(crate) fn ensure_logged_in(self) -> anyhow::Result<Self> {
        anyhow::ensure!(self.logged_in(), "client not logged in");
        Ok(self)
    }

    pub(crate) async fn login_password(&self, password: &str) -> anyhow::Result<()> {
        self.inner
            .matrix_auth()
            .login_username(&self.user_id, password)
            .initial_device_display_name(&self.device_name)
            .await?;
        self.persist_session()
    }
}

impl Deref for Client {
    type Target = MatrixClient;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}
