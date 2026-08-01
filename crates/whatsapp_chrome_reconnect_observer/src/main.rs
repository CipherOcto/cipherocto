//! `whatsapp_chrome_reconnect_observer` — Phase 7.J incognito + reconnect drill.
//!
//! Spawns real headless Chrome in incognito mode against `web.whatsapp.com`
//! and observes two phases:
//!
//!   Phase 1 (initial): navigate to https://web.whatsapp.com; capture WA Web's
//!   WS lifecycle — endpoint, Noise XX HandshakeInit payload, server frames,
//!   AppState sync attributes. Operator scans QR with the phone paired to the
//!   account that the daemon's `default.session.db` was registered to.
//!
//!   Phase 2 (reconnect drill): after login settles, close the WA tab via CDP
//!   `Target.closeTarget`, then `Target.createTarget` + `Page.navigate` a
//!   fresh tab to `web.whatsapp.com`. Capture the SAME WS lifecycle for the
//!   reconnect — the daemon's bug surface is here.
//!
//! Goal: produce a side-by-side diff of (initial connect vs reconnect) for:
//!   - WS URL (port 443 vs 5222, query string)
//!   - Noise pattern (XX vs IK)
//!   - First client→server frame bytes (compare against the synthetic XX
//!     envelope `whatsapp_noise_local_capture` already prints)
//!   - First server→client frame bytes (which AppState handshake attribute)
//!   - Frame timing / order / count
//!
//! Run:
//!   cargo run -p whatsapp_chrome_reconnect_observer -- \
//!         [--chrome PATH] [--port 9224] \
//!         [--log-dir /tmp/wa-observer] \
//!         [--login-window 90] [--reconnect-window 60]
//!
//! Outputs:
//!   /tmp/wa-observer-<ts>/initial.jsonl
//!   /tmp/wa-observer-<ts>/reconnect.jsonl
//!   /tmp/wa-observer-<ts>/summary.txt
//!
//! Default behaviour:
//!   * uses `/usr/bin/google-chrome` if it exists,
//!   * picks `--user-data-dir=/tmp/wa-observer-<ts>` (fresh profile, incognito),
//!   * writes human summary at the end (endpoints, frame counts, cookie count,
//!     first-frame hex previews).
//!
//! Why a fresh crate: this binary deliberately does NOT depend on
//! `octo-adapter-whatsapp` or any other workspace crate. It is a clean-room
//! Chrome driver — its only job is to observe what a logged-in Chrome
//! session does during connect + reconnect, so our daemon can mirror it.

// Clippy `[disallowed-methods]` allowlist: these binaries talk to
// Chrome DevTools Protocol (CDP) at `http://localhost:9222` to extract
// WhatsApp Web session keys for Phase 7.J work. CDP is an operator
// diagnostic endpoint (not an LLM model provider). Cipherocto capability
// tokens never reach CDP.
#![allow(clippy::disallowed_methods)]

mod cdp;

use anyhow::{bail, Context, Result};
use chrono::Utc;
use clap::Parser;
use futures::{SinkExt, StreamExt};
use serde::Serialize;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::time::{sleep, Duration};
use tracing::{info, warn};

#[derive(Parser, Debug)]
#[command(
    name = "whatsapp_chrome_reconnect_observer",
    about = "Observe Chrome's reconnect flow against web.whatsapp.com"
)]
struct Args {
    /// Path to google-chrome / chromium binary (auto-detected if omitted).
    #[arg(long)]
    chrome: Option<PathBuf>,
    /// Remote-debugging port to expose on the launched Chrome.
    #[arg(long, default_value_t = 9224)]
    port: u16,
    /// Directory to write NDJSON trace + summary into.
    #[arg(long, default_value = "/tmp/wa-observer")]
    log_dir: PathBuf,
    /// Phase 1 wall-clock window in seconds (initial login observation).
    #[arg(long, default_value_t = 90)]
    login_window: u64,
    /// Phase 2 wall-clock window in seconds (reconnect observation).
    #[arg(long, default_value_t = 60)]
    reconnect_window: u64,
    /// Skip Phase 2 (close+reopen drill). Useful when you only want the
    /// initial-login capture and want to leave Chrome open for manual study.
    #[arg(long, default_value_t = false)]
    skip_reconnect: bool,
}

#[derive(Debug, Serialize)]
struct CapturedEvent {
    ts: String,
    phase: &'static str,
    method: String,
    summary: String,
    /// Full CDP params — recorded for offline analysis.
    params: Value,
    /// First N decoded bytes of the frame, hex-encoded (32 B by default). Empty
    /// for events that have no payload.
    payload_head_hex: String,
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
        .context("no Chrome binary: pass --chrome /path or install google-chrome")?;

    let ts = Utc::now().timestamp_millis();
    let run_dir = args.log_dir.join(format!("run-{ts}"));
    std::fs::create_dir_all(&run_dir).context("create log dir")?;
    let user_data = run_dir.join("chrome-profile");
    std::fs::create_dir_all(&user_data).ok();

    info!(
        "launching chrome (incognito, non-headless for visible QR): {} --remote-debugging-port={} --user-data-dir={}",
        chrome.display(),
        args.port,
        user_data.display()
    );

    let mut child = tokio::process::Command::new(&chrome)
        .arg(format!("--remote-debugging-port={}", args.port))
        .arg("--no-sandbox")
        .arg("--no-first-run")
        .arg("--no-default-browser-check")
        .arg("--disable-extensions")
        .arg("--disable-features=Translate,InfiniteSessionRestore")
        .arg("--disable-gpu")
        .arg("--window-size=900,800")
        .arg(format!("--user-data-dir={}", user_data.display()))
        .arg("--incognito")
        .arg("https://web.whatsapp.com")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .with_context(|| format!("spawn chrome at {}", chrome.display()))?;

    // Wait for CDP endpoint.
    let endpoint_url = format!("http://127.0.0.1:{port}", port = args.port);
    let mut up = false;
    for _ in 0..60 {
        if reqwest::get(&endpoint_url).await.is_ok() {
            up = true;
            break;
        }
        sleep(Duration::from_millis(250)).await;
    }
    if !up {
        let _ = child.kill().await;
        bail!("CDP endpoint {} never came up", endpoint_url);
    }
    info!("CDP endpoint up at {endpoint_url}");

    let http = reqwest::Client::new();

    // Find the first type=page tab.
    let initial_tab = cdp::find_or_create_page(&http, &endpoint_url).await?;
    info!(
        "driving tab id={} url={:?}",
        initial_tab.id, initial_tab.url
    );

    // Phase 1 — initial login.
    let initial_log = run_dir.join("initial.jsonl");
    info!(
        "phase 1: initial login ({}s). scan QR with the phone paired to the daemon's session. log -> {}",
        args.login_window, initial_log.display()
    );

    let summary_phase1 = observe_phase(
        &initial_tab,
        "https://web.whatsapp.com",
        Duration::from_secs(args.login_window),
        &initial_log,
        "initial",
    )
    .await?;

    let summary_phase2 = if !args.skip_reconnect {
        info!("phase 1 complete. sleeping 5s before phase 2 (reconnect drill)…");
        sleep(Duration::from_secs(5)).await;
        info!("phase 2: close original tab + open fresh tab on web.whatsapp.com");
        let reconnect_log = run_dir.join("reconnect.jsonl");

        // Close the original tab (best-effort — failure is fine; we still want
        // a fresh tab).
        let _ = cdp::close_target(&http, &endpoint_url, &initial_tab.id).await;
        sleep(Duration::from_millis(500)).await;

        // Open a fresh tab via /json/new.
        let new_tab = cdp::find_or_create_page(&http, &endpoint_url).await?;
        info!("driving fresh tab id={} url={:?}", new_tab.id, new_tab.url);
        Some(
            observe_phase(
                &new_tab,
                "https://web.whatsapp.com",
                Duration::from_secs(args.reconnect_window),
                &reconnect_log,
                "reconnect",
            )
            .await?,
        )
    } else {
        info!("phase 2 skipped (--skip-reconnect)");
        None
    };

    // Tear down Chrome.
    let _ = child.kill().await;

    // Write summary.
    let summary_path = run_dir.join("summary.txt");
    let mut s = String::new();
    s.push_str("== whatsapp_chrome_reconnect_observer summary ==\n");
    s.push_str(&format!("chrome binary     : {}\n", chrome.display()));
    s.push_str(&format!("cdp port          : {}\n", args.port));
    s.push_str(&format!("run dir           : {}\n", run_dir.display()));
    s.push_str("\n--- phase 1: initial login ---\n");
    s.push_str(&format_summary(&summary_phase1));
    if let Some(p2) = &summary_phase2 {
        s.push_str("\n--- phase 2: reconnect ---\n");
        s.push_str(&format_summary(p2));
    }
    std::fs::write(&summary_path, &s).context("write summary")?;
    println!("{s}");
    Ok(())
}

#[derive(Debug, Default, Serialize, Clone)]
struct PhaseSummary {
    endpoint: Option<String>,
    frames_sent: u32,
    frames_recv: u32,
    cookies_initial: u32,
    cookies_after_login: u32,
    first_sent_frame_head: String,
    first_recv_frame_head: String,
    cookies_observed: Vec<String>,
}

fn format_summary(p: &PhaseSummary) -> String {
    let mut s = String::new();
    s.push_str(&format!("ws endpoint       : {:?}\n", p.endpoint));
    s.push_str(&format!(
        "ws frames s/r     : {} / {}\n",
        p.frames_sent, p.frames_recv
    ));
    s.push_str(&format!(
        "cookies (pre/nav) : {} / {}\n",
        p.cookies_initial, p.cookies_after_login
    ));
    s.push_str(&format!(
        "first sent frame  : {} (hex head)\n",
        p.first_sent_frame_head
    ));
    s.push_str(&format!(
        "first recv frame  : {} (hex head)\n",
        p.first_recv_frame_head
    ));
    if !p.cookies_observed.is_empty() {
        s.push_str("cookies seen:\n");
        for c in &p.cookies_observed {
            s.push_str(&format!("  - {c}\n"));
        }
    }
    s
}

async fn observe_phase(
    tab: &cdp::CdpPage,
    url: &str,
    duration: Duration,
    log_path: &PathBuf,
    phase: &'static str,
) -> Result<PhaseSummary> {
    let trace_file = Arc::new(Mutex::new(
        tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(log_path)
            .await
            .with_context(|| format!("open trace {}", log_path.display()))?,
    ));

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
    ) -> Result<()> {
        let msg = json!({"id": *next_id, "method": method, "params": params});
        *next_id += 1;
        ws.send(tokio_tungstenite::tungstenite::Message::Text(
            msg.to_string(),
        ))
        .await?;
        Ok(())
    }

    // Enable domains.
    send_cdp(&mut ws, &mut next_id, "Network.enable", json!({})).await?;
    send_cdp(&mut ws, &mut next_id, "Page.enable", json!({})).await?;
    send_cdp(&mut ws, &mut next_id, "Storage.enable", json!({})).await?;

    // Pre-navigation cookie snapshot.
    let cookies_initial_json: Value = cdp::call(
        &http(),
        "http://127.0.0.1:9224",
        "Network.getCookies",
        json!({}),
    )
    .await
    .unwrap_or_else(|_| json!({"cookies": []}));
    let cookies_initial = cookies_initial_json
        .get("cookies")
        .and_then(Value::as_array)
        .map(|a| a.len() as u32)
        .unwrap_or(0);
    let cookies_names: Vec<String> = cookies_initial_json
        .get("cookies")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|c| c.get("name").and_then(Value::as_str).map(String::from))
                .collect()
        })
        .unwrap_or_default();

    // Navigate.
    send_cdp(&mut ws, &mut next_id, "Page.navigate", json!({"url": url})).await?;

    let deadline = tokio::time::Instant::now() + duration;
    let mut summary = PhaseSummary {
        cookies_initial,
        cookies_observed: cookies_names,
        ..Default::default()
    };

    loop {
        let now = tokio::time::Instant::now();
        if now >= deadline {
            break;
        }
        let remain = deadline - now;
        let recv = tokio::time::timeout(remain, ws.next()).await;
        let msg = match recv {
            Ok(Some(Ok(m))) => m,
            Ok(Some(Err(e))) => {
                warn!("ws read err: {e}");
                break;
            }
            Ok(None) => break,
            Err(_) => break, // timeout — done
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
        if v.get("method").and_then(Value::as_str).is_none() {
            continue; // response to one of our `send`s
        }
        let method = v["method"].as_str().unwrap_or("").to_string();
        let params = v.get("params").cloned().unwrap_or(json!({}));
        let (summary_line, payload_head_hex) = process_event(&method, &params, &mut summary);
        let event = CapturedEvent {
            ts: Utc::now().to_rfc3339(),
            phase,
            method: method.clone(),
            summary: summary_line.clone(),
            params,
            payload_head_hex: payload_head_hex.clone(),
        };
        let mut g = trace_file.lock().await;
        use tokio::io::AsyncWriteExt;
        let line = format!("{}\n", serde_json::to_string(&event).unwrap_or_default());
        let _ = g.write_all(line.as_bytes()).await;
        let _ = g.flush().await;
        if !summary_line.is_empty() {
            info!("[{phase}] {summary_line}");
        }
        if !payload_head_hex.is_empty() && summary.frames_sent + summary.frames_recv <= 1 {
            info!(
                "[{phase}] first-frame head ({}B): {}",
                payload_head_hex.len() / 2,
                payload_head_hex
            );
        }
    }
    let _ = ws.close(None).await;
    Ok(summary)
}

fn process_event(method: &str, params: &Value, summary: &mut PhaseSummary) -> (String, String) {
    match method {
        "Network.webSocketCreated" => {
            let url = params
                .pointer("/url")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            if url.contains("whatsapp") {
                summary.endpoint = Some(url.clone());
                (format!("ws created: {url}"), String::new())
            } else {
                (String::new(), String::new())
            }
        }
        "Network.webSocketFrameSent" => {
            let payload_b64 = params
                .pointer("/response/payloadData")
                .and_then(Value::as_str)
                .unwrap_or("");
            let decoded =
                base64::Engine::decode(&base64::engine::general_purpose::STANDARD, payload_b64)
                    .unwrap_or_default();
            summary.frames_sent += 1;
            if summary.first_sent_frame_head.is_empty() && !decoded.is_empty() {
                let head = &decoded[..decoded.len().min(48)];
                summary.first_sent_frame_head = hex::encode(head);
            }
            (
                format!(
                    "sent frame b64={}B decoded={}B",
                    payload_b64.len(),
                    decoded.len()
                ),
                hex::encode(decoded.iter().take(48).copied().collect::<Vec<u8>>()),
            )
        }
        "Network.webSocketFrameReceived" => {
            let payload_b64 = params
                .pointer("/response/payloadData")
                .and_then(Value::as_str)
                .unwrap_or("");
            let decoded =
                base64::Engine::decode(&base64::engine::general_purpose::STANDARD, payload_b64)
                    .unwrap_or_default();
            summary.frames_recv += 1;
            if summary.first_recv_frame_head.is_empty() && !decoded.is_empty() {
                let head = &decoded[..decoded.len().min(48)];
                summary.first_recv_frame_head = hex::encode(head);
            }
            (
                format!(
                    "recv frame b64={}B decoded={}B",
                    payload_b64.len(),
                    decoded.len()
                ),
                hex::encode(decoded.iter().take(48).copied().collect::<Vec<u8>>()),
            )
        }
        "Network.requestWillBeSent" => {
            let req = params.pointer("/request").cloned().unwrap_or(json!({}));
            let url = req.get("url").and_then(Value::as_str).unwrap_or("");
            if url.contains("whatsapp") {
                let method_r = req.get("method").and_then(Value::as_str).unwrap_or("");
                (format!("{method_r} {url}"), String::new())
            } else {
                (String::new(), String::new())
            }
        }
        "Network.responseReceived" => {
            let r = params.pointer("/response").cloned().unwrap_or(json!({}));
            let url = r.get("url").and_then(Value::as_str).unwrap_or("");
            if url.contains("whatsapp") {
                (
                    format!(
                        "response {url} status={}",
                        r.get("status").unwrap_or(&json!(0))
                    ),
                    String::new(),
                )
            } else {
                (String::new(), String::new())
            }
        }
        "Network.cookiesAdded" | "Network.cookieChanged" => {
            let c = params.pointer("/cookie").cloned().unwrap_or(json!({}));
            let name = c
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            if !name.is_empty() && !summary.cookies_observed.contains(&name) {
                summary.cookies_observed.push(name.clone());
            }
            (
                format!(
                    "cookie {} domain={:?}",
                    c.get("name").unwrap_or(&json!("?")),
                    c.get("domain").unwrap_or(&json!("?"))
                ),
                String::new(),
            )
        }
        _ => (String::new(), String::new()),
    }
}

/// Tiny wrapper — only used once per phase. Kept inline because the dev test
/// isn't worth pulling cdp::Client into scope from a free fn.
fn http() -> reqwest::Client {
    reqwest::Client::new()
}
