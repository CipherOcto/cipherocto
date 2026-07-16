//! `whatsapp_noise_local_capture` — generate a Noise HandshakeInit from
//! the on-disk session.db without any network. Lets us compare the EXACT
//! first WS frame our adapter would send against Chrome's captured frame.
//!
//! We can't trivially wrap wacore's noise transport (the read_pump is
//! internal). Instead we replicate the WA frame format + XX HandshakeInit
//! using our existing wacore-noise dependency tree from the fork via
//! /mnt/data/mmacedoeu/work/tools/whatsapp-rust.
//!
//! This binary is best-effort: it constructs the WA envelope (`V\x13A` +
//! length + e_static_pub + tag) by hand from the Device's noise_key and
//! outputs base64. NOT a real Noise handshake — just enough to compare
//! key shape and envelope format with Chrome.

use std::path::PathBuf;
use std::process::ExitCode;

use octo_adapter_whatsapp::store::StoolapStore;
use sha2::{Digest, Sha256};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let path: PathBuf = if args.len() >= 2 {
        PathBuf::from(&args[1])
    } else {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
        PathBuf::from(format!(
            "{home}/.local/share/octo/whatsapp/default.session.db"
        ))
    };
    if !path.exists() {
        eprintln!("error: session path does not exist: {}", path.display());
        return ExitCode::from(2);
    }
    let store = match StoolapStore::new(&path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("open error: {e}");
            return ExitCode::from(1);
        }
    };
    let Some((
        noise_key,
        _identity_key,
        _signed_pre_key,
        push_name,
        avp,
        avs,
        avt,
        registration_id,
    )) = store.read_device_keys().ok().flatten()
    else {
        eprintln!("no device row in {}", path.display());
        return ExitCode::from(1);
    };

    println!("== whatsapp_noise_local_capture ==");
    println!("session path    : {}", path.display());
    println!("registration_id : {registration_id}");
    println!("app_version     : {avp}.{avs}.{avt}");
    println!("push_name       : {push_name:?}    (empty = wacore's <biz>-only path)");

    // We can't easily reproduce wacore's exact HandshakeInit without forking
    // their noise crate into our tree. So instead we:
    //
    //   1) Print the SHA-256 of the on-disk noise_key blob (the canonical
    //      fingerprint our adapter ships when computing IK pattern)
    //   2) Print the FIRST 32 bytes of noise_key as the e_static_pub we'd
    //      publish on the wire (with the 0x05 Djb type-byte prefix)
    //   3) Print a synthetic XX HandshakeInit envelope so byte-level
    //      comparison against Chrome's capture is one diff away
    //
    // The synthetic envelope uses the same V0EGAwAA JIB prefix Chrome
    // produced (V\x13\x41\x03\x02\x00 — the WA WS frame envelope) plus
    // our own e_static_pub. This is a *shape* check, not a real
    // handshake.

    let noise_key_sha256 = hex::encode(Sha256::digest(&noise_key));

    // wacore's noise_key is the KeyPair private||public (32 || 32 bytes).
    // For X25519 the public bytes are the last 32 of the blob.
    let e_pub_bytes = if noise_key.len() >= 64 {
        &noise_key[32..64]
    } else {
        &noise_key[..]
    };
    let e_pub_sha256 = hex::encode(Sha256::digest(e_pub_bytes));

    // Synthetic XX HandshakeInit envelope:
    //   0x56 ("V") + 0x13 + 0x41 ("A") + 0x03 0x02 0x00  (length prefix)
    //   0x24 0x12 0x20 = 36 bytes of payload tag (?) + 32 bytes of e_static_pub
    let mut envelope = vec![0x56, 0x13, 0x41, 0x03, 0x02, 0x00, 0x24, 0x12, 0x20];
    envelope.extend_from_slice(e_pub_bytes);

    let b64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &envelope);

    println!();
    println!("== outputs (compare to chrome_driver capture) ==");
    println!("noise_key blob sha256 : {noise_key_sha256}");
    println!("e_static_pub sha256   : {e_pub_sha256}");
    println!(
        "synthetic XX envelope : ({} bytes)  b64: {b64}",
        envelope.len()
    );
    println!("  hex of envelope     : {}", hex::encode(&envelope));

    ExitCode::SUCCESS
}
