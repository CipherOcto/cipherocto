//! `whatsapp_decode_chrome_frame5` — Phase 7.J.6: localize the 401 at
//! `lla` (post-handshake AppState IQ). After the IK-bypass fix (commit
//! 902a9ff8) the daemon completes the XX Noise handshake successfully,
//! then 401s at the post-handshake AppState layer. This probe parses
//! Chrome's frame[5] (the post-handshake IQ emission) from the captured
//! `reconnect.jsonl` and reports its size + envelope structure.
//!
//! Frame numbering (Chrome's actual observed frames on `reconnect.jsonl`):
//!   idx 0  sent 43B  XX opener
//!   idx 1  recv 350B server-hello
//!   idx 2  sent 363B client-static + signed cert
//!   idx 3  recv 698B server payload (post-handshake init)
//!   idx 4  sent 37B  post-handshake ciphertext (client ack)
//!   idx 5  sent 93B  AppState handshake IQ    <-- THIS ONE
//!   idx 6  recv 66B  IQ handshake response
//!
//! Frame[5] is a Noise-encrypted post-handshake payload. We can't decrypt
//! without Chrome's session keys, but we CAN compare its wire-size vs
//! wacore's expected emit size — a significant gap implies wacore is
//! missing child elements of the AppState `<handshake>` IQ.
//!
//! Run:
//!     cargo run -p octo-adapter-whatsapp --bin whatsapp_decode_chrome_frame5 --release
//!
//! Output: Pass A = Chrome frame[5] envelope + size, Pass B = wacore's
//! expected emit size estimate, Pass C = gap analysis with field-level
//! candidate list.
//!
//! Local-only / no push. Standalone investigation binary.

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

    println!("== whatsapp_decode_chrome_frame5 ==");
    println!("trace dir       : {}", trace_dir.display());

    // Find the most recent run dir with both phases.
    let candidates: Vec<_> = std::fs::read_dir(&trace_dir)
        .with_context(|| format!("read {}", trace_dir.display()))?
        .filter_map(Result::ok)
        .filter(|e| e.file_name().to_string_lossy().starts_with("run-"))
        .map(|e| e.path())
        .collect();
    let mut with_both: Vec<_> = candidates
        .iter()
        .filter(|p| p.join("reconnect.jsonl").exists())
        .collect();
    with_both.sort();
    let run_dir_path = with_both
        .last()
        .map(|p| (*p).clone())
        .unwrap_or_else(|| candidates.last().cloned().unwrap_or_default());
    println!("latest run dir  : {}", run_dir_path.display());

    let reconnect_path = run_dir_path.join("reconnect.jsonl");

    // Pass A — Chrome frame[5] wire shape (the post-handshake IQ emission).
    // WS FrameSent indices: 0=XX opener (43B), 1=client-static (363B),
    // 2=post-handshake ack (37B), 3=AppState IQ (93B) ← this one,
    // 4+=heartbeats (41B each).
    let frame5_reconnect = read_frame(&reconnect_path, 3, "FrameSent")?;
    let frame5_initial = read_frame_initial(trace_dir.as_path(), &run_dir_path, 3, "FrameSent")?;
    println!();
    println!("[Pass A] Chrome frame[5] wire shape (post-handshake AppState IQ)");
    println!("  source (reconnect) : {}", reconnect_path.display());
    println!("  base64 length       : {}B", frame5_reconnect.b64_len);
    println!("  decoded length      : {}B", frame5_reconnect.decoded_len);
    let frame5_hex = read_frame_hex(&reconnect_path, 3, "FrameSent")?;
    println!(
        "  hex head ({}B)       : {}",
        frame5_hex.len() / 2,
        frame5_hex
    );
    if let Some(init) = frame5_initial {
        println!();
        println!("  source (initial)   : (initial.jsonl)");
        println!("  base64 length       : {}B", init.b64_len);
        println!("  decoded length      : {}B", init.decoded_len);
        let init_hex = read_frame_hex(&run_dir_path.join("initial.jsonl"), 3, "FrameSent")?;
        println!("  hex head ({}B)       : {}", init_hex.len() / 2, init_hex);
        if init.decoded_len != frame5_reconnect.decoded_len {
            println!(
                "  size delta vs initial: {:+}",
                frame5_reconnect.decoded_len as i64 - init.decoded_len as i64
            );
        } else {
            println!("  size match initial vs reconnect");
        }
    }

    // Pass B — wacore expected frame[5] size estimate.
    println!();
    println!("[Pass B] Wacore expected frame[5] size estimate");
    println!("  Noise transport ciphertext");
    println!("  Plaintext (AppState handshake IQ)");
    println!("    - <iq id=... to=s.whatsapp.net type=get>");
    println!("      <usync>...</usync>");
    println!("      <handshake>");
    println!("        <proto version=... />");
    println!("      </handshake>");
    println!("      <edge_routing>");
    println!("        <ttl minutes=... />");
    println!("      </edge_routing>");
    println!("      <read_receipts>");
    println!("      <history_sync>");
    println!("    estimated plaintext: 35-50B (typical WA Web XML)");
    let expected = 50;
    println!(
        "    + 16B AES-GCM MAC + 2B length-prefix = ~{}B",
        expected + 18
    );
    println!("  Expected ciphertext size  : ~{}B", expected);
    println!(
        "  Chrome observed (reconnect): {}B",
        frame5_reconnect.decoded_len
    );
    let chrome = frame5_reconnect.decoded_len as i64;
    let gap = chrome - (expected as i64);
    println!(
        "  SIZE GAP                        : {:+}{}",
        gap,
        if gap.abs() > 30 {
            "  (SIGNIFICANT — wacore likely missing IQ children)"
        } else {
            "  (within estimate bounds)"
        }
    );

    // Pass C — gap analysis.
    println!();
    println!("[Pass C] Candidate missing fields (ranked by WA Web's recent emission)");
    println!("  if gap > +20B:  wacore missing one of:");
    println!("    - <history_sync config/height>     (~30B)");
    println!("    - <psk> pre-shared key             (~10B)");
    println!("    - <feature_flags>                  (~5B each)");
    println!("    - <device_props> mirror            (~12B)");
    println!("  if gap < -10B:  Chrome missing fields wacore emits (rare;");
    println!("                    rules out old-version compat issues)");

    // Pass D — server reply shape (frame[6] = IQ response from server).
    let frame6_reconnect = read_frame(&reconnect_path, 1, "FrameReceived")?;
    let frame6_reconnect_2 = read_frame(&reconnect_path, 2, "FrameReceived")?;
    println!();
    println!("[Pass D] Server reply shapes");
    println!(
        "  FrameReceived[1] (server-hello) : {}B",
        frame6_reconnect.decoded_len
    );
    println!(
        "  FrameReceived[2] (server post-handshake): {}B",
        frame6_reconnect_2.decoded_len
    );

    Ok(())
}

#[derive(Debug, Default)]
struct FrameCapture {
    b64_len: usize,
    decoded_len: usize,
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
                let payload_b64 = event
                    .params
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
                let payload_b64 = event
                    .params
                    .pointer("/response/payloadData")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("");
                let decoded = base64::engine::general_purpose::STANDARD
                    .decode(payload_b64)
                    .with_context(|| format!("decode base64 frame at index {idx}"))?;
                return Ok(hex::encode(&decoded[..decoded.len().min(96)]));
            }
            counter += 1;
        }
    }
    anyhow::bail!(
        "could not find Network.webSocket{suffix} #{idx} in {}",
        path.display()
    )
}

fn read_frame_initial(
    _trace_dir: &std::path::Path,
    run_dir: &std::path::Path,
    idx: usize,
    suffix: &str,
) -> Result<Option<FrameCapture>> {
    let initial_path = run_dir.join("initial.jsonl");
    if !initial_path.exists() {
        return Ok(None);
    }
    Ok(Some(read_frame(&initial_path, idx, suffix)?))
}
