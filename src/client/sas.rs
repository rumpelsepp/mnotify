use anyhow::Context;
use futures::stream::StreamExt;
use matrix_sdk::Client as MatrixClient;
use matrix_sdk::ruma::DeviceId;
use matrix_sdk::{
    encryption::verification::{
        SasState, SasVerification, Verification, VerificationRequestState, format_emojis,
    },
    ruma::events::{
        key::verification::{
            request::ToDeviceKeyVerificationRequestEvent,
            start::{OriginalSyncKeyVerificationStartEvent, ToDeviceKeyVerificationStartEvent},
        },
        room::message::{MessageType, OriginalSyncRoomMessageEvent},
    },
};

use crate::terminal;

async fn sas_verification_handler(sas: SasVerification) {
    let other_user_id = sas.other_device().user_id();
    let other_device_id = sas.other_device().device_id();

    println!("Starting verification with {other_user_id} {other_device_id}");

    if !sas.we_started() {
        sas.accept().await.unwrap();
    }

    let mut stream = sas.changes();

    while let Some(state) = stream.next().await {
        match state {
            SasState::KeysExchanged {
                emojis,
                decimals: _,
            } => {
                println!("Confirm that the emojis match!");
                println!("{}", format_emojis(emojis.unwrap().emojis));

                let sas = sas.clone();
                tokio::spawn(async move {
                    if terminal::confirm("confirm").await.unwrap() {
                        sas.confirm().await.unwrap();
                    } else {
                        sas.cancel().await.unwrap();
                    }
                });
            }
            SasState::Done { .. } => {
                println!(
                    "successfully verified device {} {}",
                    other_user_id, other_device_id,
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
            SasState::Created { .. }
            | SasState::Started { .. }
            | SasState::Accepted { .. }
            | SasState::Confirmed => (),
        }
    }
}

impl super::Client {
    /// Start an interactive SAS verification of one of our own devices. Requires
    /// a sync to be running concurrently so the other side's replies arrive.
    pub(crate) async fn verify_device(&self, device_id: &DeviceId) -> anyhow::Result<()> {
        let user_id = self.inner.user_id().context("not logged in")?.to_owned();
        let device = self
            .inner
            .encryption()
            .get_device(&user_id, device_id)
            .await?
            .context("no such device")?;

        let request = device.request_verification().await?;
        eprintln!("Verification request sent; accept it on the other device.");

        let mut changes = request.changes();
        while let Some(state) = changes.next().await {
            match state {
                VerificationRequestState::Ready { .. } => {
                    if let Some(sas) = request.start_sas().await? {
                        sas_verification_handler(sas).await;
                    }
                }
                VerificationRequestState::Transitioned {
                    verification: Verification::SasV1(sas),
                } => sas_verification_handler(sas).await,
                VerificationRequestState::Done => break,
                VerificationRequestState::Cancelled(info) => {
                    anyhow::bail!("verification cancelled: {}", info.reason());
                }
                _ => {}
            }
        }
        Ok(())
    }

    pub(crate) async fn set_sas_handlers(&self) -> anyhow::Result<()> {
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
                        .await
                    else {
                        tracing::warn!("creating verification request failed");
                        return;
                    };

                    let Ok(()) = request.accept().await else {
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

        Ok(())
    }
}
