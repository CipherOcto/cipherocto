//! Live integration test for the SHORTCAKE_PASSKEY companion-link flow.
//!
//! Connects to `wss://cable.ua5v.com` using the live HandshakeV2 from
//! WA Android, then issues a CTAP2 GetAssertion over the encrypted
//! tunnel using the WebAuthn JSON shape we captured live from WA Web.
//!
//! Run with:
//! ```bash
//! cargo run --example live_assert -p octo-cable
//! ```
//!
//! **Operator action required:** while this binary is running and
//! blocked at the connect step, the phone that produced the captured
//! QR must be on the same FIDO:/ QR display that we re-emit from
//! the decoded HandshakeV2. The phone scans our QR via caBLE → the
//! relay routes the phone to our tunnel → the handshake completes →
//! the phone signs our assertion request → we receive it → done.

use std::time::Duration;

use octo_cable::{assert_via_cable, HandshakeV2};

const CAPTURED_URI: &str = "FIDO:/450667960436000384212746765638726635029113873858466150978817481746737139187585179964034382683425543718266291918030680810069082498271112126385317319279362107096654083076";

/// WebAuthn JSON shape mirroring what `wacore::passkey::parse_request_options`
/// surfaces from `Event::PairPasskeyRequest.request_options_json`. We
/// hard-code it here to mirror the WA Web capture so we can verify
/// the round-trip without going through wacore.
const REQUEST_OPTIONS_JSON: &str = r#"{
    "challenge": "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8",
    "rpId": "whatsapp.com",
    "timeout": 60000,
    "allowCredentials": [],
    "userVerification": "required",
    "extensions": {"uvm": true}
}"#;

#[tokio::main]
async fn main() {
    // rustls 0.23+ needs an explicit crypto provider.
    let _ = rustls::crypto::ring::default_provider().install_default();

    let h = HandshakeV2::from_fido_uri(CAPTURED_URI).expect("decode");
    eprintln!(
        "[+] decoded HandshakeV2: peer_identity={}B secret={}B ts={}",
        h.peer_identity.len(),
        h.secret.len(),
        h.timestamp
    );
    eprintln!("[*] calling assert_via_cable (90s timeout — phone must scan during this window)…");

    match tokio::time::timeout(
        Duration::from_secs(90),
        assert_via_cable(&h, REQUEST_OPTIONS_JSON),
    )
    .await
    {
        Ok(Ok(credential)) => {
            eprintln!("[+] assertion complete");
            eprintln!("[+] PublicKeyCredential JSON:");
            println!("{}", serde_json::to_string_pretty(&credential).unwrap());
        }
        Ok(Err(e)) => {
            eprintln!("[-] assert_via_cable error: {e:?}");
            std::process::exit(2);
        }
        Err(_) => {
            eprintln!("[-] timeout: 90s elapsed without peer response");
            eprintln!("    (expected if no phone is currently scanning)");
            std::process::exit(3);
        }
    }
}
