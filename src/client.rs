use std::fs;
use std::future::Future;

use anyhow::{anyhow, bail};
use matrix_sdk::config::SyncSettings;
use matrix_sdk::event_handler::{EventHandler, EventHandlerHandle, EventHandlerResult, SyncEvent};
use matrix_sdk::room;
use matrix_sdk::ruma::events::room::message::RoomMessageEventContent;
use matrix_sdk::ruma::{EventId, OwnedDeviceId, OwnedUserId, RoomId};
use matrix_sdk::sync::SyncResponse;
use matrix_sdk::Client as MatrixClient;
use serde::de::DeserializeOwned;
use serde::Serialize;

use futures::stream::StreamExt;
use matrix_sdk::{
    encryption::verification::{format_emojis, SasState, SasVerification, Verification},
    ruma::events::{
        key::verification::{
            request::ToDeviceKeyVerificationRequestEvent,
            start::{OriginalSyncKeyVerificationStartEvent, ToDeviceKeyVerificationStartEvent},
        },
        room::message::{MessageType, OriginalSyncRoomMessageEvent},
    },
};

use crate::config::Config;
use crate::session;
use crate::terminal;
use crate::CRATE_NAME;

// Copy of the ruma Response type; the origninal type does not
// implement Serialize.
// TODO: Can Serialize applied for external types?
#[derive(Debug, Serialize)]
pub(crate) struct WhoamiResponse {
    pub(crate) user_id: OwnedUserId,
    pub(crate) device_id: Option<OwnedDeviceId>,
    pub(crate) is_guest: bool,
}

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

    // pub(crate) fn config(self, config: Config) -> Self {
    //     config.into()
    // }

    pub(crate) fn load_config(self) -> anyhow::Result<Self> {
        let config = Config::load().map_err(|e| anyhow!("could not load config: {}", e))?;
        Ok(Self::from(config))
    }

    pub(crate) async fn build(self) -> anyhow::Result<Client> {
        let Some(user_id) = self.user_id else {
            panic!("no user_id set");
        };
        let Some(device_name) = self.device_name else {
            panic!("no device name set");
        };

        let inner = MatrixClient::builder()
            .server_name(user_id.server_name())
            .sled_store(session::state_db_path(&user_id)?, None)
            .build()
            .await?;

        let client = Client {
            inner,
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

impl From<Config> for ClientBuilder {
    fn from(config: Config) -> Self {
        let device_name = config.device_name.unwrap_or_else(|| CRATE_NAME.to_string());
        Self {
            user_id: Some(config.user_id),
            device_name: Some(device_name),
        }
    }
}

async fn sas_verification_handler(sas: SasVerification) {
    println!(
        "Starting verification with {} {}",
        &sas.other_device().user_id(),
        &sas.other_device().device_id()
    );

    // print_devices(sas.other_device().user_id(), &client).await;
    sas.accept().await.unwrap();

    let mut stream = sas.changes();

    while let Some(state) = stream.next().await {
        match state {
            SasState::KeysExchanged {
                emojis,
                decimals: _,
            } => {
                println!("Confirm that the emojis match!");
                println!("{}", format_emojis(emojis.unwrap().emojis));
                if terminal::confirm("confirm").await.unwrap() {
                    sas.confirm().await.unwrap();
                } else {
                    sas.cancel().await.unwrap();
                }
            }
            SasState::Done { .. } => {
                let device = sas.other_device();

                println!(
                    "successfully verified device {} {}",
                    device.user_id(),
                    device.device_id(),
                );

                break;
            }
            SasState::Cancelled(cancel_info) => {
                println!(
                    "verification has been cancelled, reason: {}",
                    cancel_info.reason()
                );

                break;
            }
            SasState::Started { .. } | SasState::Accepted { .. } | SasState::Confirmed => (),
        }
    }
}

pub(crate) struct Client {
    inner: MatrixClient,
    user_id: OwnedUserId,
    device_name: String,
}

impl Client {
    pub(crate) fn builder() -> ClientBuilder {
        ClientBuilder::default()
    }

    pub(crate) async fn connect(&self) -> anyhow::Result<()> {
        if let Ok(Some(session)) = session::load_session(&self.user_id) {
            self.inner.restore_session(session).await?
        }

        Ok(())
    }

    pub(crate) fn ensure_login(self) -> anyhow::Result<Self> {
        if !self.logged_in() {
            bail!("client not logged in");
        }
        Ok(self)
    }

    pub(crate) async fn logout(&self) -> anyhow::Result<()> {
        self.inner.logout().await?;
        self.delete_session()?;
        self.delete_state_store()
    }

    pub(crate) fn delete_session(&self) -> anyhow::Result<()> {
        session::delete_session(&self.user_id)
    }

    pub(crate) fn delete_state_store(&self) -> anyhow::Result<()> {
        fs::remove_dir_all(session::state_db_path(&self.user_id)?)?;
        Ok(())
    }

    fn get_joined_room(&self, room: impl AsRef<RoomId>) -> anyhow::Result<room::Joined> {
        self.inner
            .get_joined_room(room.as_ref())
            .ok_or_else(|| anyhow!("no such room: {}", room.as_ref()))
    }

    pub(crate) fn logged_in(&self) -> bool {
        self.inner.logged_in()
    }

    pub(crate) async fn login_password(&self, password: &str) -> anyhow::Result<()> {
        self.inner
            .login_username(&self.user_id, password)
            .initial_device_display_name(&self.device_name)
            .send()
            .await?;

        self.persist_session()
    }

    pub(crate) async fn sync_once(
        &self,
        sync_settings: SyncSettings,
    ) -> anyhow::Result<SyncResponse> {
        self.inner
            .sync_once(sync_settings)
            .await
            .map_err(|e| anyhow!(e))
    }

    pub(crate) fn add_event_handler<Ev, Ctx, H>(&self, handler: H) -> EventHandlerHandle
    where
        Ev: SyncEvent + DeserializeOwned + Send + 'static,
        H: EventHandler<Ev, Ctx>,
        <H::Future as Future>::Output: EventHandlerResult,
    {
        self.inner.add_event_handler(handler)
    }

    pub fn add_room_event_handler<Ev, Ctx, H>(
        &self,
        room_id: &RoomId,
        handler: H,
    ) -> EventHandlerHandle
    where
        Ev: SyncEvent + DeserializeOwned + Send + 'static,
        H: EventHandler<Ev, Ctx>,
        <H::Future as Future>::Output: EventHandlerResult,
    {
        self.inner.add_room_event_handler(room_id, handler)
    }

    pub(crate) async fn sync(&self, sync_settings: SyncSettings) -> anyhow::Result<()> {
        self.inner.sync(sync_settings).await.map_err(|e| anyhow!(e))
    }

    pub(crate) async fn redact(
        &self,
        room: impl AsRef<RoomId>,
        event_id: &EventId,
        reason: Option<&str>,
    ) -> anyhow::Result<()> {
        let room = self.get_joined_room(room)?;
        room.redact(event_id, reason, None).await?;
        Ok(())
    }

    pub(crate) async fn send_message_raw(
        &self,
        sync_settings: SyncSettings,
        room: impl AsRef<RoomId>,
        content: RoomMessageEventContent,
    ) -> anyhow::Result<()> {
        self.sync_once(sync_settings).await?;

        let room = self.get_joined_room(room)?;
        room.send(content, None).await?;
        Ok(())
    }

    pub(crate) async fn send_message(
        &self,
        sync_settings: SyncSettings,
        room: impl AsRef<RoomId>,
        msg: &str,
    ) -> anyhow::Result<()> {
        let event = RoomMessageEventContent::text_plain(msg);
        self.send_message_raw(sync_settings, room, event).await
    }

    pub(crate) async fn send_message_md(
        &self,
        sync_settings: SyncSettings,
        room: impl AsRef<RoomId>,
        msg: &str,
    ) -> anyhow::Result<()> {
        let event = RoomMessageEventContent::text_markdown(msg);
        self.send_message_raw(sync_settings, room, event).await
    }

    // async fn devices(&self) -> anyhow::Result<UserDevices> {
    //     Ok(self
    //         .inner
    //         .encryption()
    //         .get_user_devices(&self.user_id)
    //         .await?)
    // }

    fn persist_session(&self) -> anyhow::Result<()> {
        let session = self.inner.session().unwrap();
        session::persist_session(&self.user_id, &session)
    }

    // fn load_session(&self) -> anyhow::Result<Option<Session>> {
    //     persist::load_session(&self.user_id)
    // }

    pub(crate) async fn whoami(&self) -> anyhow::Result<WhoamiResponse> {
        let resp = self.inner.whoami().await?;
        Ok(WhoamiResponse {
            user_id: resp.user_id,
            device_id: resp.device_id,
            is_guest: resp.is_guest,
        })
    }
    pub(crate) async fn sync_sas_verification(&self) -> anyhow::Result<()> {
        self.inner.add_event_handler(
            |ev: ToDeviceKeyVerificationRequestEvent, client: MatrixClient| async move {
                let request = client
                    .encryption()
                    .get_verification_request(&ev.sender, &ev.content.transaction_id)
                    .await
                    .expect("Request object wasn't created");

                request
                    .accept()
                    .await
                    .expect("Can't accept verification request");
            },
        );

        self.inner.add_event_handler(
            |ev: ToDeviceKeyVerificationStartEvent, client: MatrixClient| async move {
                if let Some(Verification::SasV1(sas)) = client
                    .encryption()
                    .get_verification(&ev.sender, ev.content.transaction_id.as_str())
                    .await
                {
                    tokio::spawn(sas_verification_handler(sas));
                }
            },
        );

        self.inner.add_event_handler(
            |ev: OriginalSyncRoomMessageEvent, client: MatrixClient| async move {
                if let MessageType::VerificationRequest(_) = &ev.content.msgtype {
                    let Some(request) = client
                        .encryption()
                        .get_verification_request(&ev.sender, &ev.event_id)
                        .await else {
                        tracing::warn!("creating verification request failed");
                        return;
                    };

                    let Ok(()) = request
                        .accept().await else {
                        tracing::warn!("can't accept verification request");
                        return;
                    };
                }
            },
        );

        self.inner.add_event_handler(
            |ev: OriginalSyncKeyVerificationStartEvent, client: MatrixClient| async move {
                if let Some(Verification::SasV1(sas)) = client
                    .encryption()
                    .get_verification(&ev.sender, ev.content.relates_to.event_id.as_str())
                    .await
                {
                    tokio::spawn(sas_verification_handler(sas));
                }
            },
        );

        self.inner.sync(SyncSettings::new()).await?;

        Ok(())
    }
}
