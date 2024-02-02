use anyhow::{self, bail};

impl super::Client {
    pub(crate) fn ensure_login(self) -> anyhow::Result<Self> {
        if !self.logged_in() {
            bail!("client not logged in");
        }
        Ok(self)
    }

    pub(crate) async fn login_password(&self, password: &str) -> anyhow::Result<()> {
        self.inner
            .matrix_auth()
            .login_username(&self.user_id, password)
            .initial_device_display_name(&self.device_name)
            .send()
            .await?;

        self.persist_session()
    }
}
