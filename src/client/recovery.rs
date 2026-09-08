use anyhow::Context;
use matrix_sdk::ruma::api::client::uiaa::{self, AuthData};
use serde_json::{Value, json};

impl super::Client {
    /// Bootstrap a cross-signing identity (using `password` for user-interactive
    /// auth if the server asks for it), then turn on the server-side key backup
    /// and secret-storage recovery. Returns the recovery key -- there is no way
    /// to recover it later, so store it somewhere safe.
    pub(crate) async fn recovery_enable(&self, password: &str) -> anyhow::Result<String> {
        let encryption = self.inner.encryption();
        encryption.wait_for_e2ee_initialization_tasks().await;

        if let Err(e) = encryption.bootstrap_cross_signing(None).await {
            let uiaa = e.as_uiaa_response().context(
                "cross-signing bootstrap failed and the server did not fall back to a password",
            )?;
            let mut pw = uiaa::Password::new(self.user_id.clone().into(), password.to_owned());
            pw.session = uiaa.session.clone();
            encryption
                .bootstrap_cross_signing(Some(AuthData::Password(pw)))
                .await?;
        }

        Ok(encryption
            .recovery()
            .enable()
            .wait_for_backups_to_upload()
            .await?)
    }

    /// Restore the cross-signing keys and the key-backup decryption key from a
    /// recovery key, then let matrix-sdk pull the room keys from the backup.
    pub(crate) async fn recovery_recover(&self, recovery_key: &str) -> anyhow::Result<()> {
        let encryption = self.inner.encryption();
        encryption.wait_for_e2ee_initialization_tasks().await;
        encryption.recovery().recover(recovery_key.trim()).await?;
        Ok(())
    }

    /// Replace the recovery key with a fresh one, invalidating the old one.
    pub(crate) async fn recovery_reset(&self) -> anyhow::Result<String> {
        Ok(self.inner.encryption().recovery().reset_key().await?)
    }

    /// Turn off recovery and delete the server-side key backup.
    pub(crate) async fn recovery_disable(&self) -> anyhow::Result<()> {
        self.inner.encryption().recovery().disable().await?;
        Ok(())
    }

    pub(crate) async fn recovery_status(&self) -> anyhow::Result<Value> {
        let encryption = self.inner.encryption();
        let backups = encryption.backups();
        Ok(json!({
            "recovery": format!("{:?}", encryption.recovery().state()),
            "backup": format!("{:?}", backups.state()),
            "backup_on_server": backups.fetch_exists_on_server().await.unwrap_or(false),
            "cross_signing": encryption.cross_signing_status().await.map(|s| format!("{s:?}")),
            "verification": format!("{:?}", encryption.verification_state().get()),
        }))
    }
}
