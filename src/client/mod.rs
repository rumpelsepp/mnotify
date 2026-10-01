use std::env;
use std::ops::Deref;

use anyhow::{Context, bail};
use matrix_sdk::Client as MatrixClient;
use matrix_sdk::cross_process_lock::CrossProcessLockConfig;
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
    /// Without a `homeserver` URL it is discovered from the user's server name
    /// (`.well-known`), falling back to `https://<server name>`.
    pub(crate) async fn new(
        user_id: OwnedUserId,
        device_name: String,
        homeserver: Option<&str>,
    ) -> anyhow::Result<Self> {
        let persisted = session::load_or_init(&user_id)?;

        // Several `mn` processes may share the store (e.g. overlapping cron
        // jobs). The SDK's cross-process locks only work if every process
        // has its own holder name; the default one is shared by all of them.
        let lock_holder = format!("{CRATE_NAME}-{}", std::process::id());

        let builder = MatrixClient::builder();
        let mut builder = match homeserver {
            Some(url) => builder.homeserver_url(url),
            None => builder.server_name_or_homeserver_url(user_id.server_name()),
        }
        .handle_refresh_tokens()
        .cross_process_store_config(CrossProcessLockConfig::multi_process(&lock_holder))
        .sqlite_store(
            session::state_db_path(&user_id)?,
            Some(&persisted.store_passphrase),
        );

        // No explicit proxy: reqwest picks up HTTPS_PROXY / ALL_PROXY and
        // honours NO_PROXY by itself. `builder.proxy()` would disable that.
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

        // OAuth refresh tokens are single-use: two processes refreshing at the
        // same time would invalidate the session. Must be set before restore.
        client
            .inner
            .oauth()
            .enable_cross_process_refresh_lock(lock_holder)
            .await?;

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
        if !session::Meta::exists()? {
            bail!("not logged in; run `mn login @user:example.org` first");
        }
        let meta = session::Meta::load().context("could not load meta.json")?;
        let device_name = meta.device_name.unwrap_or_else(|| CRATE_NAME.to_string());
        Self::new(meta.user_id, device_name, meta.homeserver.as_deref()).await
    }

    pub(crate) fn logged_in(&self) -> bool {
        self.inner.session().is_some()
    }

    pub(crate) fn ensure_logged_in(self) -> anyhow::Result<Self> {
        anyhow::ensure!(
            self.logged_in(),
            "no stored session for {}; run `mn login {}` again",
            self.user_id,
            self.user_id,
        );
        Ok(self)
    }
}

impl Deref for Client {
    type Target = MatrixClient;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}
