//! Live integration test: connect to `wss://cable.ua5v.com` using the
//! HandshakeV2 captured from the official WA Android app's "Link a
//! Device" flow. This is the canary test — if `connect_initiator`
//! fails to even open the WebSocket, our URL / header format is wrong;
//! if it opens but the handshake hangs, our Noise framing is wrong.
//!
//! Run with:
//! ```bash
//! cargo run --example live_connect -p octo-cable
//! ```

use octo_cable::HandshakeV2;
use std::time::Duration;

/// Exact URI captured 2026-07-08 from official WA Android's
/// "Link a Device" flow, scanned with a generic QR reader and pasted
/// to chat. See `/tmp/wa-fido-uri-decode.md` for the decode trace.
const CAPTURED_URI: &str = "FIDO:/450667960436000384212746765638726635029113873858466150978817481746737139187585179964034382683425543718266291918030680810069082498271112126385317319279362107096654083076";

#[tokio::main]
async fn main() {
    // rustls 0.23+ requires an explicit CryptoProvider. `ring` is the
    // default; we install it once before any TLS connection.
    let _ = rustls::crypto::ring::default_provider().install_default();

    let h = HandshakeV2::from_fido_uri(CAPTURED_URI).expect("decode captured URI");
    eprintln!(
        "[+] decoded HandshakeV2: peer_identity={}B secret={}B ts={}",
        h.peer_identity.len(),
        h.secret.len(),
        h.timestamp
    );
    eprintln!("[+] request_type = {:?}", h.request_type);

    eprintln!("[*] calling connect_initiator (15s timeout)…");
    let connect =
        tokio::time::timeout(Duration::from_secs(15), octo_cable::connect_initiator(&h)).await;

    match connect {
        Ok(Ok(_tunnel)) => {
            eprintln!("[+] handshake complete — tunnel ready");
            // We don't send a CTAP command here because the relay
            // connection only stays live while the phone is also
            // connected on its side. Without an active phone scan,
            // the handshake either succeeds immediately (relay holds
            // the socket open) or hangs waiting for the responder.
        }
        Ok(Err(e)) => {
            eprintln!("[-] connect error: {e:?}");
            std::process::exit(2);
        }
        Err(_) => {
            eprintln!("[-] timeout: 15s elapsed without a peer response");
            eprintln!("    (expected if no phone is currently scanning)");
            std::process::exit(3);
        }
    }
}
