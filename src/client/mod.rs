use std::ops::Deref;

use matrix_sdk::ruma::{OwnedDeviceId, OwnedUserId};
use matrix_sdk::Client as MatrixClient;
use serde::Serialize;

use crate::CRATE_NAME;

pub mod builder;
pub mod login;
pub mod room;
pub mod sas;
pub mod session;

// Copy of the ruma Response type; the origninal type does not
// implement Serialize.
// TODO: Can Serialize applied for external types?
#[derive(Debug, Serialize)]
pub(crate) struct WhoamiResponse {
    pub(crate) user_id: OwnedUserId,
    pub(crate) device_id: Option<OwnedDeviceId>,
    pub(crate) is_guest: bool,
}

pub(crate) struct Client {
    inner: MatrixClient,
    user_id: OwnedUserId,
    device_name: String,
}

impl Client {
    pub(crate) fn builder() -> builder::ClientBuilder {
        builder::ClientBuilder::default()
    }

    pub(crate) async fn connect(&self) -> anyhow::Result<()> {
        if let Ok(Some(session)) = session::load_session(&self.user_id) {
            self.inner.restore_session(session).await?;
        }

        // TODO: Is this actually an error?
        Ok(())
    }

    pub(crate) async fn whoami(&self) -> anyhow::Result<WhoamiResponse> {
        let resp = self.inner.whoami().await?;
        Ok(WhoamiResponse {
            user_id: resp.user_id,
            device_id: resp.device_id,
            is_guest: resp.is_guest,
        })
    }
}

impl Deref for Client {
    type Target = MatrixClient;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}
