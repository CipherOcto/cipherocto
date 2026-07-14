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

    let (noise_key, identity_key, signed_pre_key, push_name, avp, avs, avt, registration_id) =
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
    // (33 bytes — `PublicKey::serialize()` = 1 type byte + 32 key bytes).
    // We don't know the exact KeyPair serde layout (private||public or
    // public||private), so we hash BOTH halves and report both. To match
    // the daemon's exact computation, also try prepending the type byte.
    let noise_pub_first = &noise_key[..32];
    let noise_pub_last = &noise_key[32..];
    let noise_pub_first_fp = Sha256::digest(noise_pub_first);
    let noise_pub_last_fp = Sha256::digest(noise_pub_last);
    // `PublicKey::serialize()` prepends a 1-byte type code. For Djb
    // (curve25519) keys, that code is `5` per wacore's `key_type` enum.
    // Try both common variants to find what the daemon hashes.
    let mut noise_pub_ser = vec![0x05u8];
    noise_pub_ser.extend_from_slice(noise_pub_last);
    let noise_pub_ser_fp = Sha256::digest(&noise_pub_ser);

    println!("session_path: {}", path.display());
    println!(
        "push_name:    {:?}    app_version: {}.{}.{}    registration_id: {}",
        push_name, avp, avs, avt, registration_id
    );
    println!();
    println!("Full SHA-256 fingerprints (compare to daemon log fields):");
    println!("  noise_key blob (64B)     sha256={}", hex::encode(noise_fp));
    println!("  identity_key blob (64B)  sha256={}", hex::encode(id_fp));
    println!("  signed_pre_key blob(64B) sha256={}", hex::encode(spk_fp));
    println!("  noise pubkey last32 (32B) sha256={}", hex::encode(noise_pub_last_fp));
    println!("  noise pubkey ser(33B,type5) sha256={}", hex::encode(noise_pub_ser_fp));
    println!("  all 3 concat (192B)      sha256={}", hex::encode(concat_fp));
    println!();
    println!("Truncated (16 hex) for eyeballing:");
    println!("  noise_key blob    {}", &hex::encode(&noise_fp)[..16]);
    println!("  noise pubkey last32  {}", &hex::encode(&noise_pub_last_fp)[..16]);
    println!("  noise pubkey ser(33B) {}", &hex::encode(&noise_pub_ser_fp)[..16]);
    Ok(())
}
