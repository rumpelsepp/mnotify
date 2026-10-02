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

/// Drive one SAS verification to its end. Returns an error if it was
/// cancelled, by either side.
async fn sas_verification_handler(sas: SasVerification) -> anyhow::Result<()> {
    let other_user_id = sas.other_device().user_id().to_owned();
    let other_device_id = sas.other_device().device_id().to_owned();

    println!("Starting verification with {other_user_id} {other_device_id}");

    if !sas.we_started() {
        sas.accept()
            .await
            .context("could not accept the verification")?;
    }

    let mut stream = sas.changes();
    // The SDK reports `KeysExchanged` a second time once the other side's MAC
    // arrives (its `MacReceived` state maps to it), i.e. when the user confirms
    // on the other device first. Ask only once: a second prompt would compete
    // with the first one for stdin and print into the raw-mode terminal.
    let mut asked = false;

    while let Some(state) = stream.next().await {
        match state {
            SasState::KeysExchanged {
                emojis,
                decimals: _,
            } => {
                if asked {
                    continue;
                }
                asked = true;

                let Some(emojis) = emojis else {
                    let _ = sas.cancel().await;
                    anyhow::bail!("the other device does not support emoji verification");
                };
                println!("Confirm that the emojis match!");
                println!("{}", format_emojis(emojis.emojis));

                let sas = sas.clone();
                tokio::spawn(async move {
                    // Anything but an explicit "yes" (including a missing
                    // terminal) cancels: never confirm by accident.
                    let answer = tokio::task::spawn_blocking(|| terminal::confirm("confirm"))
                        .await
                        .map_err(anyhow::Error::from)
                        .and_then(|r| r);
                    let result = match answer {
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
                println!("successfully verified device {other_user_id} {other_device_id}");
                return Ok(());
            }
            SasState::Cancelled(cancel_info) => {
                anyhow::bail!("verification cancelled: {}", cancel_info.reason());
            }
            SasState::Created { .. }
            | SasState::Started { .. }
            | SasState::Accepted { .. }
            | SasState::Confirmed => (),
        }
    }
    anyhow::bail!("verification ended unexpectedly")
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
                // Starting SAS moves the request to `Transitioned`, handled
                // below, as is a SAS the other device started itself.
                VerificationRequestState::Ready { .. } => {
                    request.start_sas().await?;
                }
                VerificationRequestState::Transitioned {
                    verification: Verification::SasV1(sas),
                } => return sas_verification_handler(sas).await,
                VerificationRequestState::Done => return Ok(()),
                VerificationRequestState::Cancelled(info) => {
                    anyhow::bail!("verification cancelled: {}", info.reason());
                }
                _ => {}
            }
        }
        anyhow::bail!("verification ended unexpectedly")
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
                    tokio::spawn(async move {
                        if let Err(e) = sas_verification_handler(sas).await {
                            eprintln!("{e:#}");
                        }
                    });
                }
            },
        );

        Ok(())
    }
}
