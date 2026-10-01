use anyhow::Context;
use futures::stream::StreamExt;
use matrix_sdk::Client as MatrixClient;
use matrix_sdk::ruma::DeviceId;
use matrix_sdk::{
    encryption::verification::{
        SasState, SasVerification, Verification, VerificationRequestState, format_emojis,
    },
    ruma::events::key::verification::{
        request::ToDeviceKeyVerificationRequestEvent, start::ToDeviceKeyVerificationStartEvent,
    },
};

use tracing::warn;

use crate::terminal;

async fn sas_verification_handler(sas: SasVerification) {
    let other_user_id = sas.other_device().user_id();
    let other_device_id = sas.other_device().device_id();

    println!("Starting verification with {other_user_id} {other_device_id}");

    if !sas.we_started()
        && let Err(e) = sas.accept().await
    {
        warn!("could not accept the verification: {e}");
        return;
    }

    let mut stream = sas.changes();

    while let Some(state) = stream.next().await {
        match state {
            SasState::KeysExchanged {
                emojis,
                decimals: _,
            } => {
                let Some(emojis) = emojis else {
                    warn!("the other device does not support emoji verification");
                    let _ = sas.cancel().await;
                    break;
                };
                println!("Confirm that the emojis match!");
                println!("{}", format_emojis(emojis.emojis));

                let sas = sas.clone();
                tokio::spawn(async move {
                    // Anything but an explicit "yes" (including a missing
                    // terminal) cancels: never confirm by accident.
                    let result = match terminal::confirm("confirm").await {
                        Ok(true) => sas.confirm().await,
                        Ok(false) => sas.cancel().await,
                        Err(e) => {
                            warn!("{e}");
                            sas.cancel().await
                        }
                    };
                    if let Err(e) = result {
                        warn!("could not answer the verification: {e}");
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

    /// React to verification requests from our own other devices (to-device
    /// SAS). Requests from other users are ignored: under MSC4153 other users
    /// are verified via their cross-signing identity, not device by device.
    pub(crate) async fn set_sas_handlers(&self) -> anyhow::Result<()> {
        self.inner.add_event_handler(
            |ev: ToDeviceKeyVerificationRequestEvent, client: MatrixClient| async move {
                if client.user_id() != Some(&ev.sender) {
                    warn!("ignoring a verification request from {}", ev.sender);
                    return;
                }
                let Some(request) = client
                    .encryption()
                    .get_verification_request(&ev.sender, &ev.content.transaction_id)
                    .await
                else {
                    warn!("unknown verification request from {}", ev.sender);
                    return;
                };

                if let Err(e) = request.accept().await {
                    warn!("can't accept verification request: {e}");
                }
            },
        );

        self.inner.add_event_handler(
            |ev: ToDeviceKeyVerificationStartEvent, client: MatrixClient| async move {
                if client.user_id() != Some(&ev.sender) {
                    return;
                }
                if let Some(Verification::SasV1(sas)) = client
                    .encryption()
                    .get_verification(&ev.sender, ev.content.transaction_id.as_str())
                    .await
                {
                    tokio::spawn(sas_verification_handler(sas));
                }
            },
        );

        Ok(())
    }
}
