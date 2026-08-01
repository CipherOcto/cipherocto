//! `whatsapp_chrome_driver` — Phase 7.J investigation binary.
//!
//! Drives a real headless Google Chrome via Chrome DevTools Protocol (CDP)
//! against `https://web.whatsapp.com` and captures EVERY network event the
//! browser produces during the load — including the final WebSocket upgrade
//! to `wss://web.whatsapp.com/ws/chat` (or whatever endpoint WA Web actually
//! uses today).
//!
//! Goal: confirm two claims wacore's diagnostic binaries + a Playwright probe
//! previously surfaced:
//!   1. JS `WebSocket(...)` constructor: call site URL (the canonical
//!      multi-device companion WS endpoint + path + query string WA Web
//!      uses today).
//!   2. The Chrome BoringSSL ClientHello fingerprint (cipher suite list,
//!      extension list, GREASE values) when the WS upgrade requests
//!      `wss://*.whatsapp.net:443`. We capture this *only* as a string of
//!      the `Network.requestWillBeSentExtraInfo` headers that Chrome
//!      generated — the raw TLS handshake is below CDP's visibility, so
//!      the absolute JA3 source-of-truth is still the Python loopback
//!      `tls_capture_server.py` used in `/tmp/`.
//!
//! Run:
//!   cargo run -p whatsapp_chrome_driver -- [--chrome PATH] [--port 9223]
//!         [--url https://web.whatsapp.com] [--trace FILE] [--duration 30]
//!
//! Default behaviour:
//!   * uses `/usr/bin/google-chrome` if it exists,
//!   * picks an ephemeral `--user-data-dir` (no profile reuse → fresh QR each run),
//!   * dumps every CDP event as one JSONL line to `--trace`, plus a short
//!     human summary table at the end (UA / Sec-CH-UA / endpoint / frame count).

// Clippy `[disallowed-methods]` allowlist: this binary drives Chrome via
// Chrome DevTools Protocol (CDP) at `http://localhost:9222` — operator
// diagnostic tooling (Phase 7.J), NOT an LLM model provider.
#![allow(clippy::disallowed_methods)]

use anyhow::{bail, Context, Result};
use chrono::Utc;
use clap::Parser;
use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::time::{sleep, Duration};
use tracing::{info, warn};

#[derive(Parser, Debug)]
#[command(
    name = "whatsapp_chrome_driver",
    about = "Headless Chrome → web.whatsapp.com via CDP"
)]
struct Args {
    /// Path to google-chrome / chromium binary (auto-detected if omitted).
    #[arg(long)]
    chrome: Option<PathBuf>,
    /// Remote-debugging port to expose on the launched Chrome.
    #[arg(long, default_value_t = 9223)]
    port: u16,
    /// Initial URL to navigate to.
    #[arg(long, default_value = "https://web.whatsapp.com")]
    url: String,
    /// Trace output file (one JSONL event per line). Empty = stdout.
    #[arg(long, default_value = "/tmp/whatsapp_chrome_driver.jsonl")]
    trace: PathBuf,
    /// Wall-clock duration in seconds before tearing down.
    #[arg(long, default_value_t = 30)]
    duration: u64,
}

#[derive(Debug, Serialize, Deserialize)]
struct CdpPage {
    #[serde(default)]
    id: String,
    #[serde(default, rename = "webSocketDebuggerUrl")]
    web_socket_debugger_url: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    title: String,
    #[serde(default, rename = "type")]
    page_type: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct CdpVersion {
    #[serde(default)]
    web_socket_debugger_url: String,
    #[serde(default)]
    browser: String,
    #[serde(default)]
    #[serde(rename = "Protocol-Version")]
    protocol_version: String,
    #[serde(default)]
    #[serde(rename = "User-Agent")]
    user_agent: String,
    #[serde(default)]
    #[serde(rename = "V8-Version")]
    v8_version: String,
    #[serde(default)]
    #[serde(rename = "WebKit-Version")]
    webkit_version: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct CapturedEvent {
    ts: String,
    method: String,
    params: Value,
    summary: String,
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

    let user_data = std::env::temp_dir().join(format!(
        "wa_chrome_driver_{}",
        Utc::now().timestamp_millis()
    ));
    std::fs::create_dir_all(&user_data).ok();

    info!(
        "launching chrome: {} --headless=new --remote-debugging-port={} --user-data-dir={}",
        chrome.display(),
        args.port,
        user_data.display()
    );

    let mut child = tokio::process::Command::new(&chrome)
        .arg("--headless=new")
        .arg(format!("--remote-debugging-port={}", args.port))
        .arg("--no-sandbox")
        .arg("--no-first-run")
        .arg("--no-default-browser-check")
        .arg("--disable-extensions")
        .arg("--disable-features=Translate,InfiniteSessionRestore")
        .arg(format!("--user-data-dir={}", user_data.display()))
        .arg("about:blank")
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

    let cdp = reqwest::Client::new();

    // /json/version (best-effort; failures are not fatal — we only need the
    // browser tag for the summary line).
    if let Ok(version) = async {
        let r = cdp
            .get(format!("{}/json/version", endpoint_url))
            .send()
            .await?
            .error_for_status()?;
        r.json::<CdpVersion>().await
    }
    .await
    {
        info!(
            "CDP /json/version OK: {} (protocol {}, ua={:?})",
            version.browser, version.protocol_version, version.user_agent
        );
    } else {
        info!("CDP /json/version parse skipped (continuing)");
    }

    // Discover page tabs (this is where webSocketDebuggerUrl lives).
    let raw_list: Vec<Value> = cdp
        .get(format!("{}/json/list", endpoint_url))
        .send()
        .await?
        .json()
        .await?;
    info!(
        "CDP /json/list returned {} tab(s); raw[0]={}",
        raw_list.len(),
        raw_list.first().cloned().unwrap_or(json!(null))
    );
    let pages: Vec<CdpPage> = raw_list
        .into_iter()
        .enumerate()
        .filter_map(
            |(i, v)| match serde_json::from_value::<CdpPage>(v.clone()) {
                Ok(p) => Some(p),
                Err(e) => {
                    tracing::warn!("tab[{i}] decode err: {e}; raw={v}");
                    None
                }
            },
        )
        .collect();

    // Prefer the first type=page tab with a webSocketDebuggerUrl. Background
    // pages (Google Hangouts, etc.) drive a different content area and a
    // Page.navigate on them won't put web.whatsapp.com in the visible tab.
    let mut owned_tab: Option<CdpPage> = pages
        .into_iter()
        .find(|p| p.page_type == "page" && !p.web_socket_debugger_url.is_empty());
    if owned_tab.is_none() {
        info!("no type=page tab yet — creating one via /json/new");
        let target: Value = cdp
            .put(format!("{}/json/new", endpoint_url))
            .send()
            .await?
            .json()
            .await?;
        info!("CDP /json/new -> {target}");
        owned_tab = Some(
            serde_json::from_value(target.clone())
                .with_context(|| format!("decoding CdpPage from /json/new: {target}"))?,
        );
    }
    let tab = owned_tab.context("could not find or create a CDP page tab")?;
    info!("driving tab id={} url={:?}", tab.id, tab.url);

    // Open WS to tab.
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
    send_cdp(
        &mut ws,
        &mut next_id,
        "Page.navigate",
        json!({"url": args.url.clone()}),
    )
    .await?;

    // Trace file.
    let trace_file = if args.trace.to_str() != Some("") {
        Some(Arc::new(Mutex::new(
            tokio::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&args.trace)
                .await?,
        )))
    } else {
        None
    };

    // Run for `args.duration` seconds, reading every CDP message.
    let mut frame_count_sent = 0u32;
    let mut frame_count_received = 0u32;
    let mut ws_endpoint: Option<String> = None;
    let mut ua: Option<String> = None;
    let mut sec_ch_ua: Option<String> = None;
    let mut page_final_url: Option<String> = None;
    let mut page_title: Option<String> = None;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(args.duration);

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
            Err(_) => break, // timeout
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
        let method = v.get("method").and_then(Value::as_str).unwrap_or("");
        if method.is_empty() {
            continue; // response to our `send`
        }
        let params = v.get("method").map(|_| v.clone()).unwrap_or(json!({}));
        let mut summary = String::new();
        match method {
            "Network.requestWillBeSent" => {
                let req = params
                    .pointer("/params/request")
                    .cloned()
                    .unwrap_or(json!({}));
                let url = req.get("url").and_then(Value::as_str).unwrap_or("");
                let method_r = req.get("method").and_then(Value::as_str).unwrap_or("");
                let headers = req
                    .get("headers")
                    .and_then(Value::as_object)
                    .cloned()
                    .unwrap_or_default();
                if url.contains("whatsapp") {
                    if let Some(u) = headers.get("User-Agent").and_then(Value::as_str) {
                        ua = Some(u.to_string());
                    }
                    if let Some(ua_header) = headers
                        .get("sec-ch-ua")
                        .or_else(|| headers.get("Sec-CH-UA"))
                    {
                        sec_ch_ua = Some(ua_header.to_string());
                    }
                    summary = format!("{method_r} {url}");
                }
            }
            "Network.webSocketCreated" => {
                let url = params
                    .pointer("/params/url")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                if url.contains("whatsapp") {
                    ws_endpoint = Some(url.to_string());
                    summary = format!("ws created: {url}");
                }
            }
            "Network.webSocketFrameSent" => {
                let payload_b64 = params
                    .pointer("/params/response/payloadData")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                let decoded_len =
                    base64::Engine::decode(&base64::engine::general_purpose::STANDARD, payload_b64)
                        .map(|v| v.len())
                        .unwrap_or(0);
                frame_count_sent += 1;
                summary = format!(
                    "sent WS frame (b64 {}B -> decoded {decoded_len}B): {payload_b64}",
                    payload_b64.len()
                );
            }
            "Network.webSocketFrameReceived" => {
                let payload_b64 = params
                    .pointer("/params/response/payloadData")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                let decoded_len =
                    base64::Engine::decode(&base64::engine::general_purpose::STANDARD, payload_b64)
                        .map(|v| v.len())
                        .unwrap_or(0);
                frame_count_received += 1;
                summary = format!(
                    "recv WS frame (b64 {}B -> decoded {decoded_len}B): {payload_b64}",
                    payload_b64.len()
                );
            }
            "Network.responseReceived" => {
                let r = params
                    .pointer("/params/response")
                    .cloned()
                    .unwrap_or(json!({}));
                let url = r.get("url").and_then(Value::as_str).unwrap_or("");
                if url.contains("whatsapp") {
                    summary = format!(
                        "response {url} status={}",
                        r.get("status").unwrap_or(&json!(0))
                    );
                }
            }
            "Page.frameNavigated" => {
                page_final_url = params
                    .pointer("/params/frame/url")
                    .and_then(Value::as_str)
                    .map(String::from);
            }
            "Page.frameTitleUpdated" => {
                page_title = params
                    .pointer("/params/title")
                    .and_then(Value::as_str)
                    .map(String::from);
            }
            "Network.loadingFailed" => {
                let url = params
                    .pointer("/params/requestId")
                    .map(|_| "loadingFailed")
                    .unwrap_or("loadingFailed");
                summary = format!("loadingFailed: {url}");
            }
            _ => {}
        }
        if (ws_endpoint.is_some()) || (frame_count_sent + frame_count_received) > 0 {
            // Already have some signal — log every event now.
        }
        let event = CapturedEvent {
            ts: Utc::now().to_rfc3339(),
            method: method.to_string(),
            params,
            summary: summary.clone(),
        };
        if let Some(tf) = &trace_file {
            let mut g = tf.lock().await;
            use tokio::io::AsyncWriteExt;
            let line = format!("{}\n", serde_json::to_string(&event).unwrap_or_default());
            let _ = g.write_all(line.as_bytes()).await;
            let _ = g.flush().await;
        }
        if !summary.is_empty() {
            info!("[cdp] {summary}");
        }
    }

    // Tear down.
    let _ = ws.close(None).await;
    let _ = child.kill().await;

    println!();
    println!("== whatsapp_chrome_driver summary ==");
    println!("  chrome binary            : {}", chrome.display());
    println!("  cdp port                 : {}", args.port);
    println!("  target url               : {}", args.url);
    println!("  duration                 : {}s", args.duration);
    println!("  page final url           : {:?}", page_final_url);
    println!("  page title               : {:?}", page_title);
    println!("  user agent (1st seen)    : {:?}", ua);
    println!("  sec-ch-ua                : {:?}", sec_ch_ua);
    println!(
        "  WS endpoint observed     : {}",
        ws_endpoint.as_deref().unwrap_or("(none in window)")
    );
    println!("  WS frames sent / recv    : {frame_count_sent} / {frame_count_received}");
    println!(
        "  trace file               : {}",
        if trace_file.is_some() {
            args.trace.display().to_string()
        } else {
            "(none)".into()
        }
    );
    Ok(())
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
