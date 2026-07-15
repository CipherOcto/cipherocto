//! `whatsapp_idb_keyblob_parser` — Phase 7.J S2.6 step 1
//!
//! Parse Blink WebCrypto Structured Clone CryptoKey blobs from Chromium
//! IndexedDB LevelDB `.log` files. The empirical structure (measured via
//! S2.5 against Chrome 150) is:
//!
//! ```text
//! [v8 frame prefix]
//! ...
//! 5c  ← V8 wrapper version byte (constant — emitted by V8ScriptValueSerializer IDB path)
//! 4b  ← kCryptoKeyTag = 'K' = 0x4B
//! subtag:byte   ← algorithm family: 0x01=AesKeyTag, 0x02=HmacKeyTag
//! <algorithm-specific props>   ← 4 bytes observed
//!   AES:    algoId:byte  20 extVarint 20 20   ← algoId, then 3 varints
//!   HMAC:                20 extVarint 20 20   ← no algoId, then 3 varints
//! keyData: 32-bytes raw key material
//! a0   ← end byte
//! ```
//!
//! Strategy (per no-guess rule):
//! 1. Find every `0x4B` byte in the file.
//! 2. For each one, print ±32 bytes of context so we can see what surrounds it.
//! 3. Apply known-key recognition: if any of the 4 experimental-key-byte patterns
//!    (aa*32, bb*32, cc*32, dd*32) is found in the file, log it.
//!
//! Run:
//!   cargo run -p whatsapp_chrome_session_extract --bin whatsapp_idb_keyblob_parser --release -- \
//!        --file /tmp/idb-diff/row-0/.../000003.log

use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;

#[derive(Parser, Debug)]
struct Args {
    /// LDB file to parse
    #[arg(long)]
    file: PathBuf,
    /// Grep length, in bytes, around each 0x4B candidate
    #[arg(long, default_value_t = 96)]
    context: usize,
    /// Try to find these 32-byte needles as raw bytes (hex)
    #[arg(long, value_delimiter = ',')]
    needles: Vec<String>,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let bytes = std::fs::read(&args.file)?;
    println!("file: {} ({}B)", args.file.display(), bytes.len());

    // 1. Locate every 0x4B.
    let mut positions: Vec<usize> = Vec::new();
    for (i, b) in bytes.iter().enumerate() {
        if *b == 0x4B {
            positions.push(i);
        }
    }
    println!("0x4B bytes found at {} position(s)", positions.len());

    for (n, pos) in positions.iter().enumerate() {
        let lo = pos.saturating_sub(args.context);
        let hi = std::cmp::min(bytes.len(), pos + args.context);
        let slice = &bytes[lo..hi];
        let hex = slice
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect::<String>();
        // Pretty print in 32-hex (16 byte) rows
        println!("--- hit #{} offset={} (file range [{}..{})) ---", n, pos, lo, hi);
        for line in hex.as_bytes().chunks(48).map(|c| std::str::from_utf8(c).unwrap_or("")) {
            println!("  {}", line);
        }
    }

    // 2. Grep for known key bytes.
    println!("\n== Needle greps (hex) ==");
    for needle_hex in &args.needles {
        let needle = parse_hex(needle_hex);
        if needle.is_empty() {
            continue;
        }
        let hits = find_all_subslice(&bytes, &needle);
        println!(
            "needle {}... -> {} hits at {:?}",
            needle_hex.chars().take(8).collect::<String>(),
            hits.len(),
            hits
        );
    }

    Ok(())
}

fn parse_hex(s: &str) -> Vec<u8> {
    let s = s.chars().filter(|c| !c.is_whitespace()).collect::<String>();
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap_or(0))
        .collect()
}

fn find_all_subslice(haystack: &[u8], needle: &[u8]) -> Vec<usize> {
    if needle.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    for i in 0..=(haystack.len() - needle.len()) {
        if &haystack[i..i + needle.len()] == needle {
            out.push(i);
        }
    }
    out
}
