use std::env;
use std::ops::Deref;

use anyhow::Context;
use matrix_sdk::Client as MatrixClient;
use matrix_sdk::ruma::OwnedUserId;

use crate::CRATE_NAME;

pub mod login;
pub mod recovery;
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
        let persisted = session::load_or_init(&user_id)?;

        let mut builder = MatrixClient::builder()
            .server_name(user_id.server_name())
            .handle_refresh_tokens()
            .sqlite_store(
                session::state_db_path(&user_id)?,
                Some(&persisted.store_passphrase),
            );

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

        // Persist a rotated access/refresh token synchronously whenever
        // matrix-sdk refreshes it, so the next `mn` invocation still works.
        let user_id = client.user_id.clone();
        client.inner.set_session_callbacks(
            Box::new({
                let user_id = user_id.clone();
                move |_| session::stored_tokens(&user_id).map_err(Into::into)
            }),
            Box::new(move |c| session::resave_session(&user_id, &c).map_err(Into::into)),
        )?;

        if let Some(session) = persisted.session {
            client
                .inner
                .restore_session(matrix_sdk::AuthSession::from(session))
                .await?;
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
        self.inner.session().is_some()
    }

    pub(crate) fn ensure_logged_in(self) -> anyhow::Result<Self> {
        anyhow::ensure!(self.logged_in(), "client not logged in");
        Ok(self)
    }
}

impl Deref for Client {
    type Target = MatrixClient;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}
