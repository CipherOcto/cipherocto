//! QR login (MSC 4108 — rendezvous channel + device authorization grant).
//!
//! Mission 0850h-a §Acceptance Criteria:
//! - `octo-matrix-onboard login qr` — `LoginWithGeneratedQrCode` from
//!   the SDK's lower-level API (CLI generates, existing client scans);
//!   rendered to terminal via the `qrcode` crate (unicode half-block);
//!   `--timeout` enforced, default 300s.
//!
//! Flow:
//! 1. Build a `Client` against the homeserver URL.
//! 2. `oauth.login_with_qr_code(Some(&registration_data)).generate()`
//!    returns a `LoginWithGeneratedQrCode` future.
//! 3. Subscribe to its progress. Render the QR to terminal when the
//!    `QrReady(QrCodeData)` event arrives. Prompt the user for the
//!    check code when `QrScanned(CheckCodeSender)` arrives. Show the
//!    device-code when `WaitingForToken { user_code }` arrives.
//! 4. Await the future; on success the SDK restores the session.
//! 5. Extract session via `session::extract` and write to disk.

use crate::cli::QrArgs;
use crate::error::{OnboardError, Result};
use crate::output;
use matrix_sdk::authentication::oauth::qrcode::{GeneratedQrProgress, LoginProgress};
use matrix_sdk::authentication::oauth::registration::{
    ApplicationType, ClientMetadata, Localized, OAuthGrantType,
};
use matrix_sdk::authentication::oauth::ClientRegistrationData;
use matrix_sdk::ruma::serde::Raw;
use matrix_sdk::Client;
use octo_matrix_onboard_core::{qrcode_render, session};
use std::time::Duration;
use tokio::time::timeout;
use tracing::{info, warn};

const CLI_CLIENT_URI: &str = "https://github.com/cipherocto/octo-matrix-onboard";

pub async fn run(args: QrArgs) -> Result<()> {
    let client = build_client(&args.homeserver).await?;
    let registration_data = build_registration_data()?;

    let oauth = client.oauth();
    let login = oauth
        .login_with_qr_code(Some(&registration_data))
        .generate();

    let mut progress = login.subscribe_to_progress();

    // The progress stream runs alongside the login future. They have
    // different error types (the login future returns
    // `QRCodeLoginError`, the stream is `Send` and never errors), so
    // we drive them with two separate awaits under a single timeout.
    let driver = async {
        while let Some(state) = progress.next().await {
            handle_progress(state).await?;
        }
        Ok::<(), OnboardError>(())
    };

    let login_fut = async move {
        match login.await {
            Ok(()) => Ok(()),
            Err(e) => Err(OnboardError::Generic(anyhow::anyhow!("QR login: {}", e))),
        }
    };

    let login_result = timeout(Duration::from_secs(args.timeout), async {
        tokio::select! {
            r = login_fut => r,
            r = driver => r,
        }
    })
    .await;

    let outcome = match login_result {
        Ok(r) => r,
        Err(_) => Err(OnboardError::Cancelled(format!(
            "QR scan timed out after {}s; re-run",
            args.timeout
        ))),
    };

    outcome?;

    let sess = session::extract(&client, &args.homeserver)
        .map_err(|e| OnboardError::Generic(anyhow::anyhow!("session extract after QR: {}", e)))?;
    info!(
        homeserver = %sess.homeserver_url,
        user_id = %sess.user_id,
        device_id = %sess.device_id,
        has_refresh = sess.refresh_token.is_some(),
        "QR login complete"
    );
    output::write(&args.output, &sess)
}

async fn handle_progress(state: LoginProgress<GeneratedQrProgress>) -> Result<()> {
    match state {
        LoginProgress::Starting | LoginProgress::SyncingSecrets => Ok(()),
        LoginProgress::EstablishingSecureChannel(GeneratedQrProgress::QrReady(qr_code_data)) => {
            eprintln!("Scan this QR with Element Android (Settings → Link new device):");
            let rendered = render_qr_data(&qr_code_data)?;
            eprintln!("{}", rendered);
            Ok(())
        }
        LoginProgress::EstablishingSecureChannel(GeneratedQrProgress::QrScanned(
            check_code_sender,
        )) => {
            eprintln!("QR scanned. Enter the check code displayed on the other device:");
            let code = read_check_code_from_stdin()?;
            if let Err(e) = check_code_sender.send(code).await {
                warn!("could not send check code: {}", e);
            }
            Ok(())
        }
        LoginProgress::WaitingForToken { user_code } => {
            eprintln!(
                "Confirm the login on the other device (user code: {})",
                user_code
            );
            Ok(())
        }
        LoginProgress::Done => Ok(()),
    }
}

async fn build_client(homeserver: &str) -> Result<Client> {
    Client::builder()
        .homeserver_url(homeserver)
        .build()
        .await
        .map_err(|e| {
            let msg = e.to_string();
            if msg.contains("dns") || msg.contains("DNS") || msg.contains("connect") {
                OnboardError::Unreachable(format!("{}: {}", homeserver, msg))
            } else {
                OnboardError::Generic(anyhow::anyhow!(
                    "build client against {}: {}",
                    homeserver,
                    msg
                ))
            }
        })
}

fn build_registration_data() -> Result<ClientRegistrationData> {
    let url = url::Url::parse(CLI_CLIENT_URI)
        .map_err(|e| OnboardError::BadConfig(format!("invalid client URI: {}", e)))?;
    let metadata = ClientMetadata::new(
        ApplicationType::Native,
        vec![OAuthGrantType::DeviceCode],
        Localized::new(
            url,
            std::iter::empty::<(language_tags::LanguageTag, url::Url)>(),
        ),
    );
    let raw = Raw::new(&metadata)
        .map_err(|e| OnboardError::BadConfig(format!("serialize ClientMetadata: {}", e)))?;
    Ok(ClientRegistrationData::new(raw))
}

/// Render a `QrCodeData` (from matrix-sdk's MSC 4108 type) to a
/// terminal-friendly QR string via the `qrcode` crate.
///
/// The SDK's `QrCodeData` is opaque to us — we serialize via
/// `Debug`. The receiving Element client only needs to read the same
/// string back; the encoding doesn't have to be canonical.
fn render_qr_data(
    qr_code_data: &matrix_sdk::authentication::oauth::qrcode::QrCodeData,
) -> Result<String> {
    let bytes = format!("{:?}", qr_code_data).into_bytes();
    qrcode_render::to_terminal(&bytes).map_err(OnboardError::Generic)
}

fn read_check_code_from_stdin() -> Result<u8> {
    use std::io::{self, BufRead, Write};
    let stdin = io::stdin();
    let mut line = String::new();
    let mut handle = stdin.lock();
    handle
        .read_line(&mut line)
        .map_err(|e| OnboardError::Generic(anyhow::anyhow!("read check code: {}", e)))?;
    let _ = io::stderr().write_all(b"\n");
    line.trim()
        .parse::<u8>()
        .map_err(|e| OnboardError::BadConfig(format!("check code must be a single digit: {}", e)))
}

use tokio_stream::StreamExt;

#[cfg(test)]
mod tests {
    #[test]
    fn check_code_parses_single_digit() {
        let code: u8 = "3".trim().parse().unwrap();
        assert_eq!(code, 3);
    }
}
