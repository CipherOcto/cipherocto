//! `whatsapp_decode_frame2_full` — Phase 7.J.6 S4 redux step 1
//!
//! Full protobuf decode of Chrome's frame[2] (363B IK ClientHello with
//! extended fields). Captures the frame from reconnect.jsonl, strips the
//! WA envelope prefix, then walks the HandshakeMessage.ClientHello proto
//! field-by-field printing:
//!   - field 1 (ephemeral, 32B)
//!   - field 2 (static, 32B)
//!   - field 3 (payload, length + content)
//!   - field 4 (useExtended, bool)
//!   - field 5 (extendedCiphertext, length + raw hex)
//!   - field 9 (pqMode, varint)
//!   - field 10 (extendedEphemeral, length + content)
//!
//! These are the measured values to use in the wacore patch (S6.7).
//! No guesswork — every value comes from the captured frame.
//!
//! Run:
//!   cargo run -p octo-adapter-whatsapp --bin whatsapp_decode_frame2_full --release

use anyhow::{Context, Result};
use base64::Engine;
use serde::Deserialize;
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Debug, Deserialize)]
struct CapturedEvent {
    method: String,
    params: serde_json::Value,
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::from(1)
        }
    }
}

fn run() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let trace_dir: PathBuf = if args.len() >= 2 {
        PathBuf::from(&args[1])
    } else {
        PathBuf::from("/tmp/wa-observer")
    };

    let run_dir = std::fs::read_dir(&trace_dir)?
        .filter_map(Result::ok)
        .filter(|e| e.file_name().to_string_lossy().starts_with("run-"))
        .max_by_key(|e| e.file_name())
        .context("no run-* dirs found")?
        .path();

    let candidates: Vec<_> = std::fs::read_dir(&trace_dir)?
        .filter_map(Result::ok)
        .filter(|e| e.file_name().to_string_lossy().starts_with("run-"))
        .map(|e| e.path())
        .collect();
    let mut with_both: Vec<_> = candidates
        .iter()
        .filter(|p| p.join("reconnect.jsonl").exists())
        .collect();
    with_both.sort();
    let chosen = with_both.last().cloned().unwrap_or(&run_dir).clone();
    println!("== whatsapp_decode_frame2_full ==");
    println!("run dir        : {}", chosen.display());

    let frame2 = read_frame_b64(&chosen.join("reconnect.jsonl"), 1, "FrameSent")?;
    println!("frame[2]       : {}B (decoded from b64)", frame2.len());

    // Dump hex
    let hex_str = frame2
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    println!();
    println!("[hex dump] full {} bytes:", hex_str.len() / 2);
    for (i, chunk) in hex_str.as_bytes().chunks(64).enumerate() {
        let s = std::str::from_utf8(chunk).unwrap_or("");
        println!("  {:04x}: {}", i * 32, s);
    }

    // Strip WA envelope prefix
    // Per Phase 7.J evidence: 363B frame starts with 00 01 (=WA frame header),
    // then 68 22 (=BigEndian u16 length 0x2268 = 8808? no, 0x2268=8808, that's
    // bigger than the frame so actually it's not length). Let me just walk the
    // protobuf at the start since Chrome's earlier decoder found the proto
    // at offset +6.
    let payload = &frame2[6..];
    println!();
    println!(
        "[protobuf] HandshakeMessage.ClientHello (full {}B):",
        payload.len()
    );

    let mut pos = 0;
    while pos < payload.len() {
        // Read field tag (varint)
        let (tag, n) = match read_varint(&payload[pos..]) {
            Some(v) => v,
            None => {
                println!("  end of stream at +{pos}");
                break;
            }
        };
        let field_number = tag >> 3;
        let wire_type = tag & 0x7;
        pos += n;
        match wire_type {
            2 => {
                // LEN
                let (len, n2) = read_varint(&payload[pos..]).context("LEN varint")?;
                pos += n2;
                let len = len as usize;
                if pos + len > payload.len() {
                    println!("  field {field_number} (LEN={len}) — truncated");
                    break;
                }
                let slice = &payload[pos..pos + len];
                let preview = slice
                    .iter()
                    .take(32)
                    .map(|b| format!("{b:02x}"))
                    .collect::<String>();
                println!(
                    "  field {field_number:2} (LEN={len:3}B): head={preview}{}",
                    if len > 32 { "..." } else { "" }
                );
                // For field 3 (payload), show the proto-encoded signed cert contents
                if field_number == 3 && len > 0 {
                    // Try to decode the inner protobuf
                    println!("    inner (signed cert payload, {}B):", len);
                    decode_inner_payload(slice, "    ");
                }
                // For field 5 (extendedCiphertext), show full
                if field_number == 5 {
                    let full = slice.iter().map(|b| format!("{b:02x}")).collect::<String>();
                    println!("    full extendedCiphertext ({len}B): {full}");
                }
                pos += len;
            }
            0 => {
                // VARINT
                let (val, n2) = read_varint(&payload[pos..]).context("VARINT varint")?;
                pos += n2;
                println!("  field {field_number:2} (VARINT): {val}");
            }
            1 => {
                // FIXED64
                if pos + 8 > payload.len() {
                    break;
                }
                let _ = &payload[pos..pos + 8];
                pos += 8;
                println!("  field {field_number:2} (FIXED64)");
            }
            5 => {
                // FIXED32
                if pos + 4 > payload.len() {
                    break;
                }
                let _ = &payload[pos..pos + 4];
                pos += 4;
                println!("  field {field_number:2} (FIXED32)");
            }
            _ => {
                println!("  field {field_number:2} (unknown wire_type={wire_type})");
                break;
            }
        }
    }

    Ok(())
}

fn decode_inner_payload(slice: &[u8], indent: &str) {
    // Best-effort sub-decode of the ClientHello.payload (signed cert protobuf)
    let mut pos = 0;
    while pos < slice.len() {
        let (tag, n) = match read_varint(&slice[pos..]) {
            Some(v) => v,
            None => break,
        };
        let fnum = tag >> 3;
        let wt = tag & 0x7;
        pos += n;
        match wt {
            2 => {
                let (len, n2) = match read_varint(&slice[pos..]) {
                    Some(v) => v,
                    None => break,
                };
                pos += n2;
                let len = len as usize;
                if pos + len > slice.len() {
                    break;
                }
                let pre = &slice[pos..pos + len.min(20)];
                let phex = pre.iter().map(|b| format!("{b:02x}")).collect::<String>();
                println!(
                    "{indent}field {fnum:2} (LEN={len:3}B): head={phex}{}",
                    if len > 20 { "..." } else { "" }
                );
                pos += len;
            }
            0 => {
                let (val, n2) = match read_varint(&slice[pos..]) {
                    Some(v) => v,
                    None => break,
                };
                pos += n2;
                println!("{indent}field {fnum:2} (VARINT): {val}");
            }
            _ => break,
        }
    }
}

fn read_varint(buf: &[u8]) -> Option<(u64, usize)> {
    let mut result: u64 = 0;
    let mut shift = 0;
    let mut n = 0;
    for b in buf.iter().take(10) {
        result |= ((b & 0x7f) as u64) << shift;
        n += 1;
        if b & 0x80 == 0 {
            return Some((result, n));
        }
        shift += 7;
    }
    None
}

fn read_frame_b64(path: &PathBuf, idx: usize, suffix: &str) -> Result<Vec<u8>> {
    let file = std::fs::File::open(path).with_context(|| format!("open {}", path.display()))?;
    let reader = std::io::BufReader::new(file);
    let mut counter = 0usize;
    for line in std::io::BufRead::lines(reader) {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let event: CapturedEvent = match serde_json::from_str(&line) {
            Ok(e) => e,
            Err(_) => continue,
        };
        let method = format!("Network.webSocket{suffix}");
        if event.method == method {
            if counter == idx {
                let payload_b64 = event
                    .params
                    .pointer("/response/payloadData")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("");
                let decoded = base64::engine::general_purpose::STANDARD
                    .decode(payload_b64)
                    .with_context(|| format!("decode b64 frame {idx}"))?;
                return Ok(decoded);
            }
            counter += 1;
        }
    }
    anyhow::bail!(
        "could not find Network.webSocket{suffix} #{idx} in {}",
        path.display()
    )
}
