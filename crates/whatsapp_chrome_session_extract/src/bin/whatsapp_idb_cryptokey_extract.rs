//! `whatsapp_idb_cryptokey_extract` — Phase 7.J S2.6 step 2
//!
//! Extract the AES-GCM `encKey` bytes that WhatsApp Web stores inside the
//! IndexedDB `signal-storage/signal-meta-store` CryptoKey blobs.
//!
//! Empirically measured structure (from S2.5 + this binary's predecessor
//! `whatsapp_idb_keyblob_parser` against Chrome 150's real WA IDB):
//!
//! ```text
//! [LDB block header]
//! [IDB key bytes: signal_static_pubkey / signal_static_privkey / ...]
//! 0xff 0x10    ← V8 ScriptValueSerialization kVersion + version
//! 0x6f         ← kObjectTag
//! [V8 PropertyCount:varint]
//! "key" / "value" / "id" / "expiration"  (UTF-16BE properties)
//! [For encKey property:]
//!   0x5c 0x4b 0x01 0x0b 0x10 0x06 0x10     ← kCryptoKeyTag(0x4b) + AesKeyTag(0x01) + props
//!   <raw 16-byte AES key>                  ← offsets +4..+20 post-subtag
//!   <metadata tail>                         ← 30 bytes (including JSON " value " markers)
//!   0xa0                                   ← end marker
//! [For value property: AES-GCM ciphertext with the encKey]
//! ```
//!
//! Per no-guess rule, the AES key length is **measured** as 16 bytes
//! (verified: first 16 bytes look random; bytes 17+ are JSON ASCII markers).
//! `keyLengthBytes = 0x10 = 16` confirms AES-128.
//!
//! Run:
//!   cargo run -p whatsapp_chrome_session_extract --bin whatsapp_idb_cryptokey_extract --release -- \
//!        --ldb-path /tmp/wa-observer/run-1784043740549/.../000463.log

use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;

#[derive(Parser, Debug)]
struct Args {
    #[arg(long)]
    ldb_path: PathBuf,
    /// Find this IDB key name (default: any signal-* key)
    #[arg(long, default_value = "")]
    find_key: String,
}

#[derive(Debug, serde::Serialize)]
struct ExtractedKey {
    idb_key: String,
    offset: usize,
    subtag: String,
    raw_key_hex: String,
    raw_key_len: usize,
    blob_end_offset: usize,
    blob_total_len: usize,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let bytes = std::fs::read(&args.ldb_path)?;
    let size = bytes.len();
    println!("file: {} ({}B)", args.ldb_path.display(), size);

    // Find every '5c 4b 01' (AES CryptoKey) and '5c 4b 02' (HMAC)
    let mut keys: Vec<ExtractedKey> = Vec::new();
    let mut i = 0;
    while let Some(j) = find_seq(&bytes, i, &[0x5c, 0x4b, 0x01]) {
        let end = find_byte(&bytes, j + 1, 0xa0, 200).unwrap_or(j);
        let blob = &bytes[j..end + 1];
        // The 4-byte subtag-prefix is 5c 4b 01, then 4 props bytes (0x0b 0x10 0x06 0x10)
        // then 16-byte raw key, then tail
        let props = &blob[3..7];
        let raw_key = &blob[7..7 + 16];
        let raw_key_len = 16;

        // Walk back to find the IDB key name (UTF-16BE ASCII)
        // Look for "key" pattern in the bytes preceding
        let idb_key = extract_idb_key(&bytes, j);

        keys.push(ExtractedKey {
            idb_key,
            offset: j,
            subtag: format!("AES (0x{:02x} props={})", 0x01, hex_lower(props)),
            raw_key_hex: hex_lower(raw_key),
            raw_key_len,
            blob_end_offset: end + 1,
            blob_total_len: blob.len(),
        });
        i = j + 1;
    }

    println!("found {} AES CryptoKey blobs", keys.len());
    let extracted = serde_json::json!({
        "source_file": args.ldb_path.display().to_string(),
        "file_size": size,
        "aes_cryptokey_count": keys.len(),
        "extracted": keys,
    });
    println!("{}", serde_json::to_string_pretty(&extracted)?);

    // Optionally try a key name match
    if !args.find_key.is_empty() {
        if let Some(k) = keys.iter().find(|k| k.idb_key.contains(&args.find_key)) {
            println!(
                "matched find_key={}: idb_key={} raw_key={}",
                args.find_key, k.idb_key, k.raw_key_hex
            );
        } else {
            println!("no match for find_key={}", args.find_key);
        }
    }

    Ok(())
}

fn find_seq(haystack: &[u8], start: usize, needle: &[u8]) -> Option<usize> {
    if needle.is_empty() {
        return None;
    }
    for i in start..=haystack.len().saturating_sub(needle.len()) {
        if &haystack[i..i + needle.len()] == needle {
            return Some(i);
        }
    }
    None
}

fn find_byte(haystack: &[u8], start: usize, target: u8, max: usize) -> Option<usize> {
    for i in start..std::cmp::min(haystack.len(), start + max) {
        if haystack[i] == target {
            return Some(i);
        }
    }
    None
}

fn hex_lower(b: &[u8]) -> String {
    b.iter().map(|x| format!("{:02x}", x)).collect()
}

/// Walk backwards from the AES blob to find the UTF-16LE ASCII IDB key name
/// (e.g., "signal_static_pubkey"). The key is stored as UTF-16LE in the IDB
/// record, prefixed by a length varint.
fn extract_idb_key(buf: &[u8], blob_offset: usize) -> String {
    // Look backwards up to 400 bytes for the last UTF-16LE run that decodes
    // to a printable ASCII string and contains "signal".
    let lo = blob_offset.saturating_sub(400);
    let chunk = &buf[lo..blob_offset];
    // Scan for the last occurrence of "signal" as UTF-16LE: 0x73 0x00 0x69 0x00 0x67 0x00 0x6e 0x00 0x61 0x00 0x6c 0x00
    let needle = b"s\x00i\x00g\x00n\x00a\x00l\x00";
    let mut pos = None;
    let mut off = 0;
    while let Some(p) = find_seq(chunk, off, needle) {
        pos = Some(p);
        off = p + 1;
    }
    if let Some(p) = pos {
        // Walk forward through UTF-16LE ASCII until non-printable
        let abs = lo + p;
        let mut end = abs + needle.len();
        while end + 1 < buf.len() && end < abs + 160 {
            let lo_byte = buf[end];
            let hi_byte = buf[end + 1];
            // UTF-16LE: each ASCII char is stored as [letter, 0x00]
            // The first char 's' starts at `abs` (where buf[abs]='s'=0x73)
            // so we walk in pairs: at position end, letter is buf[end], high byte is buf[end+1]
            if hi_byte != 0 {
                break;
            }
            if !(0x20..=0x7f).contains(&lo_byte) {
                break;
            }
            end += 2;
        }
        // Decode UTF-16LE bytes
        let mut s = String::new();
        let mut i = abs;
        while i + 1 < end {
            let c = u16::from_le_bytes([buf[i], buf[i + 1]]);
            if let Some(ch) = char::from_u32(c as u32) {
                if ch.is_ascii() && !ch.is_control() {
                    s.push(ch);
                } else {
                    break;
                }
            } else {
                break;
            }
            i += 2;
        }
        if !s.is_empty() {
            return s;
        }
    }
    "<unknown>".to_string()
}