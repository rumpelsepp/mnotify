use matrix_sdk::encryption::verification::{format_emojis, Emoji, SasVerification, Verification};
use matrix_sdk::{
    config::SyncSettings,
    encryption::verification::{format_emojis, Emoji, SasState, SasVerification, Verification},
    ruma::{
        events::{
            key::verification::{
                request::ToDeviceKeyVerificationRequestEvent,
                start::{OriginalSyncKeyVerificationStartEvent, ToDeviceKeyVerificationStartEvent},
            },
            room::message::{MessageType, OriginalSyncRoomMessageEvent},
        },
        UserId,
    },
    Client,
};
use url::Url;

use crate::terminal;

async fn sync(client: Client) -> matrix_sdk::Result<()> {
    client.add_event_handler(
        |ev: ToDeviceKeyVerificationRequestEvent, client: Client| async move {
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

    client.add_event_handler(
        |ev: ToDeviceKeyVerificationStartEvent, client: Client| async move {
            if let Some(Verification::SasV1(sas)) = client
                .encryption()
                .get_verification(&ev.sender, ev.content.transaction_id.as_str())
                .await
            {
                tokio::spawn(sas_verification_handler(client, sas));
            }
        },
    );

    client.add_event_handler(
        |ev: OriginalSyncRoomMessageEvent, client: Client| async move {
            if let MessageType::VerificationRequest(_) = &ev.content.msgtype {
                let request = client
                    .encryption()
                    .get_verification_request(&ev.sender, &ev.event_id)
                    .await
                    .expect("Request object wasn't created");

                request
                    .accept()
                    .await
                    .expect("Can't accept verification request");
            }
        },
    );

    client.add_event_handler(
        |ev: OriginalSyncKeyVerificationStartEvent, client: Client| async move {
            if let Some(Verification::SasV1(sas)) = client
                .encryption()
                .get_verification(&ev.sender, ev.content.relates_to.event_id.as_str())
                .await
            {
                tokio::spawn(sas_verification_handler(client, sas));
            }
        },
    );

    client.sync(SyncSettings::new()).await?;

    Ok(())
}

async fn wait_for_confirmation(sas: SasVerification, emoji: [Emoji; 7]) {
    match terminal::confirm(&format!("Do the emojis match: \n{}", format_emojis(emoji)))
        .await
        .unwrap()
    {
        true => sas.confirm().await.unwrap(),
        false => sas.cancel().await.unwrap(),
    };
}

async fn sas_verification_handler(&self, sas: SasVerification) {
    println!(
        "Starting verification with {} {}",
        &sas.other_device().user_id(),
        &sas.other_device().device_id()
    );
    print_devices(sas.other_device().user_id(), &client).await;
    sas.accept().await.unwrap();

    let mut stream = sas.changes();

    while let Some(state) = stream.next().await {
        match state {
            SasState::KeysExchanged {
                emojis,
                decimals: _,
            } => {
                tokio::spawn(wait_for_confirmation(
                    sas.clone(),
                    emojis
                        .expect("We only support verifications using emojis")
                        .emojis,
                ));
            }
            SasState::Done { .. } => {
                let device = sas.other_device();

                println!(
                    "Successfully verified device {} {} {:?}",
                    device.user_id(),
                    device.device_id(),
                    device.local_trust_state()
                );

                print_devices(sas.other_device().user_id(), &client).await;

                break;
            }
            SasState::Cancelled(cancel_info) => {
                println!(
                    "The verification has been cancelled, reason: {}",
                    cancel_info.reason()
                );

                break;
            }
            SasState::Started { .. } | SasState::Accepted { .. } | SasState::Confirmed => (),
        }
    }
}
