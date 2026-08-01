//! `whatsapp_capture_import_key` — Phase 7.J.6 S3 retry
//!
//! Hook `crypto.subtle.importKey('raw', ...)` BEFORE WA Web loads.
//! When WA imports its master AES key into a CryptoKey, the raw bytes
//! ARE still in JS scope at that moment — they only become non-extractable
//! AFTER the import completes. Capturing those raw bytes via the hook
//! gives us the AES master key, which we can then use to decrypt
//! IndexedDB values directly from the filesystem LevelDB.
//!
//! Hook strategy:
//!  - install BEFORE page scripts run via Page.addScriptToEvaluateOnNewDocument
//!  - patch window.crypto.subtle.importKey to record every (format, keyData)
//!    pair as hex into window.__capturedKeys
//!  - also patch IDBObjectStore.put / IDBObjectStore.add to capture what's
//!    written (so we know what's stored, even though it's CryptoKey-shaped)
//!
//! Run:
//!     cargo run -p whatsapp_chrome_session_extract --bin whatsapp_capture_import_key --release -- \
//!           --profile-dir /tmp/wa-observer/run-1784043740549/chrome-profile/Default --wait-secs 60
//!
//! Output:
//!   /tmp/wa-observer/run-*/captured-import-keys.json — list of every
//!     (algorithm, extractable, usages, keyDataHex) tuple passed to importKey

// Clippy `[disallowed-methods]` allowlist: these binaries talk to
// Chrome DevTools Protocol (CDP) at `http://localhost:9222` to extract
// WhatsApp Web session keys for Phase 7.J work. CDP is an operator
// diagnostic endpoint (not an LLM model provider). Cipherocto capability
// tokens never reach CDP.
#![allow(clippy::disallowed_methods)]

use anyhow::{Context, Result};
use chrono::Utc;
use clap::Parser;
use futures::{SinkExt, StreamExt};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::process::Stdio;
use tokio::time::{sleep, Duration};
use tracing::{info, warn};

#[derive(Parser, Debug)]
struct Args {
    #[arg(long)]
    chrome: Option<PathBuf>,
    #[arg(long, default_value_t = 9231)]
    port: u16,
    #[arg(long)]
    profile_dir: Option<PathBuf>,
    #[arg(long, default_value = "/tmp/wa-observer")]
    log_dir: PathBuf,
    #[arg(long, default_value_t = 60)]
    wait_secs: u64,
}

#[derive(Debug, serde::Deserialize)]
struct CdpPage {
    #[serde(default)]
    id: String,
    #[serde(default, rename = "webSocketDebuggerUrl")]
    web_socket_debugger_url: String,
    #[serde(default, rename = "type")]
    page_type: String,
}

fn auto_find_chrome() -> Option<PathBuf> {
    for c in [
        "/usr/bin/google-chrome",
        "/usr/bin/google-chrome-stable",
        "/usr/bin/chromium-browser",
        "/usr/bin/chromium",
        "/snap/bin/chromium",
    ] {
        if std::path::Path::new(c).exists() {
            return Some(PathBuf::from(c));
        }
    }
    None
}

fn find_latest_profile(log_dir: &std::path::Path) -> Option<PathBuf> {
    let candidates: Vec<_> = std::fs::read_dir(log_dir)
        .ok()?
        .filter_map(Result::ok)
        .filter(|e| e.file_name().to_string_lossy().starts_with("run-"))
        .map(|e| e.path().join("chrome-profile").join("Default"))
        .filter(|p| p.exists())
        .collect();
    candidates.last().cloned()
}

#[allow(dead_code)]
fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{:02x}", x)).collect()
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .init();

    let chrome = args
        .chrome
        .clone()
        .or_else(auto_find_chrome)
        .context("no Chrome binary")?;
    let profile = if let Some(p) = args.profile_dir.clone() {
        p
    } else {
        find_latest_profile(&args.log_dir).context("no profile_dir")?
    };

    info!(
        "chrome: {} --user-data-dir={} --port={}",
        chrome.display(),
        profile.parent().unwrap_or(&profile).display(),
        args.port
    );

    let mut child = tokio::process::Command::new(&chrome)
        .arg(format!("--remote-debugging-port={}", args.port))
        .arg("--no-sandbox")
        .arg("--no-first-run")
        .arg("--no-default-browser-check")
        .arg("--disable-extensions")
        .arg("--disable-features=Translate,InfiniteSessionRestore")
        .arg("--disable-gpu")
        .arg("--window-size=1200,900")
        .arg(format!(
            "--user-data-dir={}",
            profile.parent().unwrap_or(&profile).display()
        ))
        .arg("https://web.whatsapp.com")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .context("spawn chrome")?;

    let endpoint_url = format!("http://127.0.0.1:{}", args.port);
    for _ in 0..60 {
        if reqwest::get(&endpoint_url).await.is_ok() {
            break;
        }
        sleep(Duration::from_millis(250)).await;
    }

    let http = reqwest::Client::new();
    let list: Vec<Value> = http
        .get(format!("{endpoint_url}/json/list"))
        .send()
        .await?
        .json()
        .await?;
    let mut tab: Option<CdpPage> = None;
    for v in list {
        if let Ok(p) = serde_json::from_value::<CdpPage>(v) {
            if p.page_type == "page" && !p.web_socket_debugger_url.is_empty() {
                tab = Some(p);
                break;
            }
        }
    }
    let tab = tab.context("no page tab")?;
    info!("driving tab id={}", tab.id);

    let (mut ws, _) = tokio_tungstenite::connect_async(&tab.web_socket_debugger_url).await?;
    let mut next_id: u64 = 1;

    async fn send_cdp(
        ws: &mut (impl futures::Sink<
            tokio_tungstenite::tungstenite::Message,
            Error = tokio_tungstenite::tungstenite::Error,
        > + Unpin),
        next_id: &mut u64,
        method: &str,
        params: Value,
    ) -> Result<u64> {
        let id = *next_id;
        *next_id += 1;
        ws.send(tokio_tungstenite::tungstenite::Message::Text(
            json!({"id": id, "method": method, "params": params}).to_string(),
        ))
        .await?;
        Ok(id)
    }

    send_cdp(&mut ws, &mut next_id, "Page.enable", json!({})).await?;

    // The hook — installed BEFORE page scripts run.
    let hook_js = r#"
window.__capturedImports = [];
window.__capturedDerives = [];
const _origImport = window.crypto.subtle.importKey.bind(window.crypto.subtle);
window.crypto.subtle.importKey = async function(format, keyData, algo, extractable, usages) {
  let keyDataHex = null;
  try {
    if (format === 'raw') {
      const u8 = keyData instanceof ArrayBuffer ? new Uint8Array(keyData)
              : ArrayBuffer.isView(keyData) ? new Uint8Array(keyData.buffer, keyData.byteOffset, keyData.byteLength)
              : (keyData && keyData.constructor && keyData.constructor.name === 'ArrayBuffer') ? new Uint8Array(keyData)
              : null;
      if (u8) keyDataHex = Array.from(u8).map(x => x.toString(16).padStart(2,'0')).join('');
    } else if (format === 'jwk' && keyData && keyData.k) {
      keyDataHex = 'jwk.k=' + keyData.k;
    }
  } catch (e) {}
  const algName = (algo && algo.name) || String(algo);
  window.__capturedImports.push({
    ts: Date.now(),
    format, algName, extractable, usages,
    keyDataLen: keyData && (keyData.byteLength || keyData.length || 0),
    keyDataHex,
  });
  return _origImport(format, keyData, algo, extractable, usages);
};

const _origDerive = window.crypto.subtle.deriveKey.bind(window.crypto.subtle);
window.crypto.subtle.deriveKey = async function(algo, baseKey, derivedKeyAlgo, extractable, usages) {
  let algoInfo = null;
  try { algoInfo = JSON.stringify(algo); } catch (_) { algoInfo = String(algo); }
  window.__capturedDerives.push({ts: Date.now(), algoInfo, extractable, usages});
  return _origDerive(algo, baseKey, derivedKeyAlgo, extractable, usages);
};
window.__capturedDerives.origDerive = _origDerive;
"#;

    send_cdp(
        &mut ws,
        &mut next_id,
        "Page.addScriptToEvaluateOnNewDocument",
        json!({"source": hook_js}),
    )
    .await?;
    info!("importKey hook installed for next navigation");

    send_cdp(
        &mut ws,
        &mut next_id,
        "Page.navigate",
        json!({"url": "https://web.whatsapp.com"}),
    )
    .await?;

    info!(
        "waiting {}s for WA Web to load and call importKey...",
        args.wait_secs
    );
    sleep(Duration::from_secs(args.wait_secs)).await;

    // Pull captured imports
    let eval_id = send_cdp(
        &mut ws,
        &mut next_id,
        "Runtime.evaluate",
        json!({
            "expression": "JSON.stringify({imports: window.__capturedImports || [], derives: window.__capturedDerives || []})",
            "returnByValue": true,
        }),
    )
    .await?;

    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    let mut result: Option<Value> = None;
    while std::time::Instant::now() < deadline {
        let remain = deadline.saturating_duration_since(std::time::Instant::now());
        let recv = tokio::time::timeout(remain, ws.next()).await;
        let msg = match recv {
            Ok(Some(Ok(m))) => m,
            Ok(Some(Err(e))) => {
                warn!("ws read err {e}");
                break;
            }
            Ok(None) => break,
            Err(_) => break,
        };
        let text = match msg {
            tokio_tungstenite::tungstenite::Message::Text(t) => t,
            other => {
                if let tokio_tungstenite::tungstenite::Message::Ping(p) = other {
                    ws.send(tokio_tungstenite::tungstenite::Message::Pong(p))
                        .await
                        .ok();
                }
                continue;
            }
        };
        let v: Value = match serde_json::from_str(&text) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if v.get("id").and_then(Value::as_u64) == Some(eval_id) {
            if let Some(err) = v.get("error") {
                warn!("eval err: {err}");
                break;
            }
            result = v
                .get("result")
                .and_then(|r| r.get("result"))
                .and_then(|r| r.get("value"))
                .cloned();
            break;
        }
    }

    let _ = ws.close(None).await;
    let _ = child.kill().await;

    let out_path = profile
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("captured-import-keys.json");
    match result {
        Some(s) => {
            // s is a JSON string
            let parsed: Value = serde_json::from_str(s.as_str().unwrap_or("{}"))?;
            std::fs::write(&out_path, serde_json::to_string_pretty(&parsed)?)?;
            println!("== whatsapp_capture_import_key ==");
            println!("profile        : {}", profile.display());
            println!("captured_at    : {}", Utc::now().to_rfc3339());
            println!("dump           : {}", out_path.display());
            println!();
            let imports = parsed.get("imports").and_then(Value::as_array);
            let derives = parsed.get("derives").and_then(Value::as_array);
            let n_imp = imports.map(|a| a.len()).unwrap_or(0);
            let n_der = derives.map(|a| a.len()).unwrap_or(0);
            println!("importKey calls captured : {n_imp}");
            println!("deriveKey calls captured : {n_der}");
            println!();
            if let Some(imps) = imports {
                // Look for AES-GCM imports (master AES key candidates)
                let aes_gcm: Vec<&Value> = imps
                    .iter()
                    .filter(|i| {
                        i.get("algName")
                            .and_then(Value::as_str)
                            .map(|s| s.contains("AES"))
                            .unwrap_or(false)
                            && i.get("format").and_then(Value::as_str) == Some("raw")
                    })
                    .collect();
                println!(
                    "AES-GCM raw imports (master key candidates): {}",
                    aes_gcm.len()
                );
                for (idx, i) in aes_gcm.iter().enumerate() {
                    let len = i.get("keyDataLen").and_then(Value::as_u64).unwrap_or(0);
                    let hex_v = i.get("keyDataHex").and_then(Value::as_str).unwrap_or("");
                    let ext = i
                        .get("extractable")
                        .and_then(Value::as_bool)
                        .unwrap_or(false);
                    let usage = i
                        .get("usages")
                        .and_then(Value::as_array)
                        .map(|a| {
                            a.iter()
                                .filter_map(|v| v.as_str())
                                .collect::<Vec<_>>()
                                .join(",")
                        })
                        .unwrap_or_default();
                    let head = if hex_v.len() > 64 {
                        format!("{}...", &hex_v[..64])
                    } else {
                        hex_v.to_string()
                    };
                    println!(
                        "  [{}] len={}B extractable={} usage=[{}] hex={}",
                        idx, len, ext, usage, head
                    );
                }
            }
        }
        None => println!("(no result)"),
    }

    Ok(())
}
