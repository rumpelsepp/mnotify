use std::io::{self, BufRead};

use anyhow::Context;
use futures::StreamExt;
use matrix_sdk::authentication::oauth::qrcode::{LoginProgress, QrCodeData, QrProgress};
use matrix_sdk::authentication::oauth::registration::{
    ApplicationType, ClientMetadata, Localized, OAuthGrantType,
};
use matrix_sdk::ruma::serde::Raw;

use crate::CRATE_NAME;

/// OAuth 2.0 client metadata shown to the user on the authorizing device.
fn client_metadata() -> Raw<ClientMetadata> {
    let uri = Localized::new(
        "https://github.com/rumpelsepp/mnotify"
            .parse()
            .expect("static client URI parses"),
        None,
    );
    let metadata = ClientMetadata {
        client_name: Some(Localized::new(CRATE_NAME.to_owned(), [])),
        ..ClientMetadata::new(
            ApplicationType::Native,
            vec![OAuthGrantType::DeviceCode],
            uri,
        )
    };
    Raw::new(&metadata).expect("client metadata serializes")
}

impl super::Client {
    pub(crate) async fn login_password(&self, password: &str) -> anyhow::Result<()> {
        self.inner
            .matrix_auth()
            .login_username(&self.user_id, password)
            .initial_device_display_name(&self.device_name)
            .await?;
        self.persist_session()
    }

    /// Log in via OAuth 2.0 by scanning a QR code shown by an already
    /// logged-in device (MSC4108). The QR image has to be decoded to its
    /// base64 payload and pasted on stdin.
    pub(crate) async fn login_qr(&self) -> anyhow::Result<()> {
        eprintln!("On your other device open \"Link new device\" and let it show the QR code,");
        eprintln!("then decode the image to its base64 payload and paste it here, e.g.:");
        eprintln!("    grim -g \"$(slurp)\" - | zbarimg --oneshot -Sbinary PNG:- | base64 -w0");
        eprint!("QR data: ");

        let mut input = String::new();
        io::stdin().lock().read_line(&mut input)?;
        let data = QrCodeData::from_base64(input.trim())
            .context("could not parse the base64 QR code data")?;

        let registration = client_metadata().into();
        let oauth = self.inner.oauth();
        let login = oauth.login_with_qr_code(Some(&registration)).scan(&data);

        let mut progress = login.subscribe_to_progress();
        let task = tokio::spawn(async move {
            while let Some(state) = progress.next().await {
                match state {
                    LoginProgress::EstablishingSecureChannel(QrProgress { check_code }) => {
                        eprintln!(
                            "Enter this code on the other device: {:02}",
                            check_code.to_digit()
                        );
                    }
                    LoginProgress::WaitingForToken { user_code } => {
                        eprintln!("Confirm the sign-in on the other device: {user_code}");
                    }
                    LoginProgress::Done => break,
                    _ => {}
                }
            }
        });

        let result = login.await;
        task.abort();
        result?;

        self.persist_session()
    }
}
