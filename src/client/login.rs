use anyhow::Context;
use futures::StreamExt;
use matrix_sdk::authentication::oauth::qrcode::{GeneratedQrProgress, LoginProgress, QrCodeData};
use matrix_sdk::authentication::oauth::registration::{
    ApplicationType, ClientMetadata, Localized, OAuthGrantType,
};
use matrix_sdk::reqwest::Url;
use matrix_sdk::ruma::serde::Raw;
use qrcode::{EcLevel, QrCode, render::unicode};
use tokio::io::{AsyncBufReadExt, BufReader};

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

/// Render the QR payload as a terminal QR code (half-block characters).
fn render_qr(data: &QrCodeData) -> anyhow::Result<String> {
    let code = QrCode::with_error_correction_level(data.to_bytes(), EcLevel::L)
        .context("could not encode the QR code")?;
    Ok(code.render::<unicode::Dense1x2>().quiet_zone(true).build())
}

async fn read_check_code() -> anyhow::Result<u8> {
    eprint!("Enter the two-digit code shown on the other device: ");
    let mut line = String::new();
    BufReader::new(tokio::io::stdin())
        .read_line(&mut line)
        .await?;
    line.trim().parse().context("that is not a number")
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

    /// Log in via the OAuth 2.0 device grant (MSC4108): `mn` prints a QR code,
    /// you scan it with an already signed-in Element ("Link new device" /
    /// "Sign in with QR code"), then type the check code Element shows back
    /// here. Needs nothing but a phone -- no screenshot tooling.
    pub(crate) async fn login_qr(&self) -> anyhow::Result<()> {
        let registration = client_metadata().into();
        let oauth = self.inner.oauth();
        let login = oauth.login_with_qr_code(Some(&registration)).generate();

        let mut progress = login.subscribe_to_progress();
        let task = tokio::spawn(async move {
            while let Some(state) = progress.next().await {
                match state {
                    LoginProgress::EstablishingSecureChannel(GeneratedQrProgress::QrReady(qr)) => {
                        match render_qr(&qr) {
                            Ok(rendered) => {
                                eprintln!("\nScan this with your other device:\n\n{rendered}")
                            }
                            Err(e) => eprintln!("could not render the QR code: {e}"),
                        }
                    }
                    LoginProgress::EstablishingSecureChannel(GeneratedQrProgress::QrScanned(
                        sender,
                    )) => match read_check_code().await {
                        Ok(code) => {
                            if let Err(e) = sender.send(code).await {
                                eprintln!("could not send the check code: {e}");
                            }
                        }
                        Err(e) => eprintln!("{e}"),
                    },
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

    /// Log in via the legacy SSO flow (`m.login.sso`, e.g. SAML): the SDK
    /// serves the redirect on a random local port; open the printed URL in a
    /// browser. Headless, forward that port with `ssh -L` first.
    pub(crate) async fn login_sso(&self, idp_id: Option<&str>) -> anyhow::Result<()> {
        let mut login = self
            .inner
            .matrix_auth()
            .login_sso(|sso_url| async move {
                eprintln!("Open this URL in a browser and sign in:\n\n    {sso_url}\n");
                if let Some(port) = redirect_port(&sso_url) {
                    eprintln!(
                        "Headless? Forward the port first: ssh -L {port}:localhost:{port} <host>"
                    );
                }
                Ok(())
            })
            .initial_device_display_name(&self.device_name);
        if let Some(idp_id) = idp_id {
            login = login.identity_provider_id(idp_id);
        }
        login.await?;
        self.persist_session()
    }
}

/// The local port of the `redirectUrl` in an SSO login URL.
fn redirect_port(sso_url: &str) -> Option<u16> {
    let url = Url::parse(sso_url).ok()?;
    let (_, redirect) = url.query_pairs().find(|(key, _)| key == "redirectUrl")?;
    Url::parse(&redirect).ok()?.port()
}
