//! Phase 7.J.4 `dump_noise_key` — extract noise_key, identity_key, signed_pre_key
//! blobs from a stoolap-backed WA session DB + print SHA-256 fingerprints.
//!
//! Purpose: rule in/out the "wacore regenerates identical noise keys across
//! `--reset --force` re-pairs" hypothesis. Run this binary on both a `.broken-<ts>`
//! snapshot AND the current `default.session.db`, then compare the printed
//! hex. If the fingerprint of `noise_key` is byte-identical across runs,
//! wacore is re-using keys (bug). If they differ, the keys were fresh but
//! the server-side fingerprint rejection is on something else (UA, IP, etc).
//!
//! Usage:
//!   cargo run -p octo-adapter-whatsapp --bin dump_noise_key -- <session_path>
//!
//! Example:
//!   cargo run -p octo-adapter-whatsapp --bin dump_noise_key -- \
//!     ~/.local/share/octo/whatsapp/default.session.db
//!
//! Exit codes:
//!   0  device row found + fingerprints printed
//!   1  no device row (probably a fresh/empty DB)
//!   2  bad invocation

use std::path::PathBuf;

use octo_adapter_whatsapp::store::StoolapStore;

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 2 {
        eprintln!("usage: dump_noise_key <session_db_path>");
        eprintln!("  e.g. dump_noise_key ~/.local/share/octo/whatsapp/default.session.db");
        std::process::exit(2);
    }
    let path = PathBuf::from(&args[1]);

    if !path.exists() {
        eprintln!("error: session path does not exist: {}", path.display());
        std::process::exit(2);
    }

    let store = StoolapStore::new(&path)?;

    let (noise_key, identity_key, signed_pre_key, push_name, avp, avs, avt) =
        match store.read_device_keys()? {
            Some(t) => t,
            None => {
                eprintln!(
                    "no device row in {} — DB exists but no keys persisted yet",
                    path.display()
                );
                std::process::exit(1);
            }
        };

    use sha2::{Digest, Sha256};
    let noise_fp = Sha256::digest(&noise_key);
    let id_fp = Sha256::digest(&identity_key);
    let spk_fp = Sha256::digest(&signed_pre_key);
    let concat_fp = Sha256::digest([&noise_key[..], &identity_key[..], &signed_pre_key[..]].concat());

    // The daemon captures fingerprints from `device.noise_key.public_key`
    // (32 bytes — the public key half of the KeyPair, NOT the full
    // serialized 64-byte blob stored in the DB). Mirror that here so
    // the diagnostic matches what shows up in the daemon trace.
    //
    // We don't know the exact KeyPair serde layout (private||public or
    // public||private), so we hash BOTH halves and report both.
    let noise_pub_first = &noise_key[..32];
    let noise_pub_last = &noise_key[32..];
    let noise_pub_first_fp = Sha256::digest(noise_pub_first);
    let noise_pub_last_fp = Sha256::digest(noise_pub_last);
    let id_pub_first_fp = Sha256::digest(&identity_key[..32]);
    let id_pub_last_fp = Sha256::digest(&identity_key[32..]);
    let spk_pub_first_fp = Sha256::digest(&signed_pre_key[..32]);
    let spk_pub_last_fp = Sha256::digest(&signed_pre_key[32..]);

    println!("session_path: {}", path.display());
    println!(
        "push_name:    {:?}    app_version: {}.{}.{}",
        push_name, avp, avs, avt
    );
    println!();
    println!("Full serialized blobs (64-byte KeyPair):");
    println!("  noise_key:        sha256={}", &hex::encode(&noise_fp)[..16]);
    println!("  identity_key:     sha256={}", &hex::encode(&id_fp)[..16]);
    println!("  signed_pre_key:   sha256={}", &hex::encode(&spk_fp)[..16]);
    println!();
    println!("Public-key halves (32 bytes — what the daemon hashes):");
    println!("  noise_key  first32  sha256={}", &hex::encode(&noise_pub_first_fp)[..16]);
    println!("  noise_key  last32   sha256={}", &hex::encode(&noise_pub_last_fp)[..16]);
    println!("  identity   first32  sha256={}", &hex::encode(&id_pub_first_fp)[..16]);
    println!("  identity   last32   sha256={}", &hex::encode(&id_pub_last_fp)[..16]);
    println!("  spk        first32  sha256={}", &hex::encode(&spk_pub_first_fp)[..16]);
    println!("  spk        last32   sha256={}", &hex::encode(&spk_pub_last_fp)[..16]);
    println!();
    println!(
        "all 3 concat (192 bytes) sha256={}",
        &hex::encode(&concat_fp)[..16]
    );
    Ok(())
}
