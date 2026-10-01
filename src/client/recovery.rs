use crate::output::Record;
use matrix_sdk::encryption::recovery::RecoveryState;

/// What to tell a user whose device cannot take part in MSC4153 crypto.
pub(crate) const NOT_CROSS_SIGNED: &str = "this device is not cross-signed, so it cannot \
    send encrypted messages and other clients ignore its encrypted messages (MSC4153); \
    cross-sign it with `mn recovery recover` (recovery key) or `mn verify` (from another \
    signed-in device)";

impl super::Client {
    /// Whether this device is cross-signed by its owner. The local copy of
    /// our own device can lag behind, e.g. right after the SDK cross-signed
    /// it, so a "no" is double-checked against the server.
    pub(crate) async fn is_cross_signed(&self) -> anyhow::Result<bool> {
        let encryption = self.inner.encryption();
        let signed = async || -> anyhow::Result<bool> {
            Ok(encryption
                .get_own_device()
                .await?
                .is_some_and(|device| device.is_cross_signed_by_owner()))
        };
        if signed().await? {
            return Ok(true);
        }
        encryption.request_user_identity(&self.user_id).await?;
        signed().await
    }

    /// Whether secret storage holds the cross-signing keys, so that a later
    /// login can be cross-signed with the recovery key.
    pub(crate) fn recovery_enabled(&self) -> bool {
        matches!(
            self.inner.encryption().recovery().state(),
            RecoveryState::Enabled
        )
    }

    /// Turn on the server-side key backup and secret storage for the
    /// cross-signing keys, and return the recovery key. There is no way to
    /// recover it later, so store it somewhere safe. Cross-signing itself is
    /// set up at login; it is never replaced here.
    pub(crate) async fn recovery_enable(&self) -> anyhow::Result<String> {
        let encryption = self.inner.encryption();
        encryption.wait_for_e2ee_initialization_tasks().await;
        anyhow::ensure!(self.is_cross_signed().await?, "{NOT_CROSS_SIGNED}");
        anyhow::ensure!(
            !self.recovery_enabled(),
            "recovery is already enabled; `mn recovery reset` replaces the key"
        );

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

    pub(crate) async fn recovery_status(&self) -> anyhow::Result<Record> {
        let encryption = self.inner.encryption();
        let backups = encryption.backups();
        Ok(Record::new()
            .field("recovery", format!("{:?}", encryption.recovery().state()))
            .field("backup", format!("{:?}", backups.state()))
            .field(
                "backup_on_server",
                backups.fetch_exists_on_server().await.unwrap_or(false),
            )
            .field(
                "cross_signing_complete",
                encryption
                    .cross_signing_status()
                    .await
                    .is_some_and(|s| s.is_complete()),
            )
            .field("device_cross_signed", self.is_cross_signed().await?))
    }
}
