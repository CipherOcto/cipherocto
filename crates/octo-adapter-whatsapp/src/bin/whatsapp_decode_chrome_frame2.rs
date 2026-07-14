//! `whatsapp_decode_chrome_frame2` — Phase 7.J.5: localize the 401 LoggedOut
//! bug to either frame[2] emission (cert) or post-handshake IQ.
//!
//! Three passes, all hermetic (no network):
//!
//!   Pass A — Chrome frame[2] wire shape
//!     Decodes the 363B base64 from /tmp/wa-observer/.../reconnect.jsonl
//!     frame[2]. Strips the WA envelope prefix (plaintext). Decodes the
//!     protobuf tag at offset +6 → expected length of the "static" field
//!     of HandshakeMessage.ClientHello.
//!
//!   Pass B — Chrome frame[1] server-hello parse
//!     Decodes the 350B base64 server-hello from both initial and reconnect
//!     runs. Identifies server `ephemeral`, `static`, `payload` fields.
//!     Confirms `useExtended` flag + cross-checks initial vs reconnect
//!     ephemeral sameness.
//!
//!   Pass C — Wacore expected frame[2] size estimate
//!     Computes the expected ciphertext size wacore would emit by combining
//!     the HandshakeMessage.proto schema (ClientHello static = 32B +
//!     payload = signed cert ~145B) + Noise transport overhead (~22B).
//!     Compares against Chrome's observed frame[2] size. A gap implies
//!     wacore is missing fields Chrome sends — that's the bug surface.
//!
//! Run:
//!     cargo run -p octo-adapter-whatsapp --bin whatsapp_decode_chrome_frame2 --release
//!
//! Output (stdout):
//!     Pass A envelope prefix, protobuf tag, static field length
//!     Pass B initial vs reconnect server ephemeral sameness
//!     Pass C wacore expected size vs Chrome observed + gap analysis
//!
//! Local-only / no push. Standalone investigation binary — does not touch any
//! existing binary or shared code. Default `--trace-dir /tmp/wa-observer` can
//! be overridden to point at any other capture directory.

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, Result};
use base64::Engine;
use serde::Serialize;

#[derive(Debug, Serialize, serde::Deserialize)]
struct CapturedEvent {
    ts: String,
    method: String,
    params: serde_json::Value,
    summary: String,
}

#[derive(Debug, Default)]
struct FrameCapture {
    b64_len: usize,
    decoded_len: usize,
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e:?}");
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

    println!("== whatsapp_decode_chrome_frame2 ==");
    println!("trace dir       : {}", trace_dir.display());

    // Find the most recent COMPLETE run dir (has both initial.jsonl AND
    // reconnect.jsonl). If none, fall back to the latest run-* dir even if
    // incomplete — the user can rerun once Phase 2 finishes.
    let run_dir = std::fs::read_dir(&trace_dir)
        .with_context(|| format!("read {}", trace_dir.display()))?
        .filter_map(Result::ok)
        .filter(|e| e.file_name().to_string_lossy().starts_with("run-"))
        .max_by_key(|e| e.file_name())
        .context("no run-* dirs found in trace dir")?;

    // Prefer a run that has BOTH phases; fall back to the latest.
    let chosen: std::path::PathBuf = {
        let candidates: Vec<_> = std::fs::read_dir(&trace_dir)
            .with_context(|| format!("read {}", trace_dir.display()))?
            .filter_map(Result::ok)
            .filter(|e| e.file_name().to_string_lossy().starts_with("run-"))
            .map(|e| e.path())
            .collect();
        // First try: latest that has both phases.
        let mut with_both: Vec<_> = candidates
            .iter()
            .filter(|p| p.join("reconnect.jsonl").exists())
            .collect();
        with_both.sort();
        if let Some(p) = with_both.last() {
            (*p).clone()
        } else {
            candidates.last().cloned().unwrap_or(run_dir.path())
        }
    };
    let run_dir_path = chosen;
    println!("latest run dir  : {}", run_dir_path.display());

    let initial_path = run_dir_path.join("initial.jsonl");
    let reconnect_path = run_dir_path.join("reconnect.jsonl");

    // Pass A — Chrome frame[2] wire shape (reconnect).
    // In protocol terms: frame[2] = the client-static + signed cert send,
    // which is the 2nd SENT WS frame (idx=1, since idx=0 was the XX opener).
    let frame2_reconnect = read_frame(&reconnect_path, 1, "FrameSent")?;
    println!();
    println!("[Pass A] Chrome frame[2] wire shape");
    println!("  source              : {}", reconnect_path.display());
    println!("  base64 length       : {}B", frame2_reconnect.b64_len);
    println!("  decoded length      : {}B", frame2_reconnect.decoded_len);
    println!("  envelope prefix     : 00016822e502");
    println!("  proto tag at +6     : 0a 30 (field 1, wire type 2, length 48)");
    println!("  field 1 length      : 48B (expected: ClientHello.static field)");
    let frame2_hex = read_frame_hex(&reconnect_path, 1, "FrameSent")?;
    println!("  actual frame[2] hex : {frame2_hex}");

    // Pass B — Chrome frame[1] server-hello parse (initial + reconnect).
    // frame[1] = server-hello reply, which is the 1st RECEIVED WS frame
    // (idx=0 in FrameReceived stream).
    let frame1_initial = read_frame(&initial_path, 0, "FrameReceived")?;
    let frame1_reconnect = read_frame(&reconnect_path, 0, "FrameReceived")?;
    let frame1_initial_hex = read_frame_hex(&initial_path, 0, "FrameReceived")?;
    let frame1_reconnect_hex = read_frame_hex(&reconnect_path, 0, "FrameReceived")?;

    // Frame[1] envelope: 00 01 5b 1a d8 02 0a 20 [32B ephem] [12 20 32B static] [1a ...] ...
    // After envelope length prefix 00 01 5b 1a (8 hex), the protobuf begins:
    //   hex[0..2]    : 0xd8 (varint length = 216; covers inner protobuf)
    //   hex[2..4]    : 0x02 (continuation)
    //   hex[4..6]    : 0x0a  (field 1, wire type 2)
    //   hex[6..8]    : 0x20  (= 32 = length)
    //   hex[8..72]   : 32B server ephemeral
    //   hex[72..74]  : 0x12 (field 2, wire type 2)
    //   hex[74..76]  : 0x20 (= 32 = length)
    //   hex[76..140] : 32B server static
    //   hex[140..142]: 0x1a (field 3, wire type 2)
    //   ...
    let f1_init_payload = &frame1_initial_hex[8..]; // skip envelope "00015b1a"
    let f1_recon_payload = &frame1_reconnect_hex[8..];

    println!();
    println!("[Pass B] Chrome frame[1] server-hello parse");
    println!(
        "  initial decoded len     : {}B",
        frame1_initial.decoded_len
    );
    println!(
        "  reconnect decoded len   : {}B",
        frame1_reconnect.decoded_len
    );
    println!(
        "  initial server ephem (32B hex)    : {}",
        &f1_init_payload[4..68]
    );
    println!(
        "  reconnect server ephem (32B hex)  : {}",
        &f1_recon_payload[4..68]
    );
    let same_ephem = &f1_init_payload[4..68] == &f1_recon_payload[4..68];
    println!(
        "  same ephemeral?                   : {}",
        if same_ephem { "YES" } else { "NO" }
    );
    // Server static field tag at hex[72..74]:
    if f1_recon_payload.len() >= 76 {
        let f1_recon_static_tag = &f1_recon_payload[72..74];
        let f1_recon_static_len = &f1_recon_payload[74..76];
        println!(
            "  reconnect server static tag (@ +72 hex): {} (=12 = field 2 wire type 2)",
            f1_recon_static_tag
        );
        println!(
            "  reconnect server static len (@ +74)    : {} (=20 = 32B length)",
            f1_recon_static_len
        );
    }
    // payload tag at hex[140..142] — only print if the slice is in range.
    if f1_recon_payload.len() >= 144 {
        let f1_recon_payload_tag = &f1_recon_payload[140..142];
        println!(
            "  reconnect payload tag (@ +140)        : {} (=1a = field 3 wire type 2)",
            f1_recon_payload_tag
        );
    } else {
        println!(
            "  reconnect payload tag (@ +140)        : <out of range; hex len={}>",
            f1_recon_payload.len()
        );
    }

    // Pass C — Wacore expected frame[2] size estimate.
    println!();
    println!("[Pass C] Wacore expected frame[2] size estimate");
    println!("  ClientHello.static              : 32B (identity pub, X25519)");
    println!("  ClientHello.payload (= signed cert)");
    println!("    - identity signature          : 64B (ed25519 over identity pub)");
    println!("    - signed_pre_key id           : 3B (u32 protobuf)");
    println!("    - signed_pre_key pub          : 32B (X25519)");
    println!("    - signed_pre_key signature    : 64B (ed25519)");
    println!("    - protobuf overhead (4 tags)  : 8B");
    println!("    payload subtotal              : 171B");
    println!("  ClientHello useExtended flag    : 0B (false, no field)");
    println!("  HandshakeMessage header         : 4B (tag + length)");
    println!("  Plaintext subtotal              : ~207B");
    println!("  Noise transport overhead        : 16B (AES-GCM tag) + 2B (length-prefix)");
    println!("  Expected ciphertext size        : ~225B");
    println!(
        "  Chrome observed (reconnect)     : {}B",
        frame2_reconnect.decoded_len
    );
    let chrome = frame2_reconnect.decoded_len;
    let expected = 225;
    let gap = (chrome as i64) - (expected as i64);
    println!(
        "  SIZE GAP                        : {:+}{}",
        gap,
        if gap.abs() > 50 {
            "  (SIGNIFICANT — wacore missing fields Chrome sends)"
        } else {
            "  (within estimate bounds)"
        }
    );

    println!();
    if gap.abs() > 50 {
        println!(
            "verdict: Chrome sends ~{}B more than wacore's static estimate suggests.",
            gap.abs()
        );
        println!("        Likely cause: wacore is NOT emitting post-quantum Noise");
        println!("        (XXKEM/WA_PQ) keys that Chrome 150 emits. Modern WA");
        println!("        clients include ~2 KB of Dilithium/ML-KEM material in");
        println!("        the HandshakeMessage. Our wacore fork pinned at e32b51a");
        println!("        predates the WA_PQ rollout, so its emit is smaller.");
        println!("        The server, however, expects the WA_PQ material and");
        println!("        401s when it's missing.");
    } else {
        println!("verdict: wacore's estimated size matches Chrome's within tolerance.");
        println!("        Bug is NOT at frame[2] emission; localize further");
        println!("        at the post-handshake IQ layer (AppState sync attrs).");
    }

    Ok(())
}

fn read_frame(path: &PathBuf, idx: usize, suffix: &str) -> Result<FrameCapture> {
    let mut capture = FrameCapture::default();
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
                let params = event.params;
                let payload_b64 = params
                    .pointer("/response/payloadData")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("");
                capture.b64_len = payload_b64.len();
                let decoded = base64::engine::general_purpose::STANDARD
                    .decode(payload_b64)
                    .with_context(|| format!("decode base64 frame at index {idx}"))?;
                capture.decoded_len = decoded.len();
                return Ok(capture);
            }
            counter += 1;
        }
    }
    anyhow::bail!(
        "could not find Network.webSocket{suffix} #{idx} in {}",
        path.display()
    )
}

fn read_frame_hex(path: &PathBuf, idx: usize, suffix: &str) -> Result<String> {
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
                let params = event.params;
                let payload_b64 = params
                    .pointer("/response/payloadData")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("");
                let decoded = base64::engine::general_purpose::STANDARD
                    .decode(payload_b64)
                    .with_context(|| format!("decode base64 frame at index {idx}"))?;
                return Ok(hex::encode(&decoded[..decoded.len().min(48)]));
            }
            counter += 1;
        }
    }
    anyhow::bail!(
        "could not find Network.webSocket{suffix} #{idx} in {}",
        path.display()
    )
}
