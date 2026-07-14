//! `whatsapp_kdf_dump` — Phase 7.J Session 2.
//!
//! Extracts the WA Web IndexedDB encryption module source by walking
//! the webpack module graph via CDP `Runtime.evaluate`. The IndexedDB
//! values in `signal-storage` and `wawc_db_enc` are encrypted at rest
//! using a key derived from localStorage's `WANoiseInfo` + `WebEncKeySalt`
//! + `WANoiseInfoIv`. To decrypt them in Rust we need the KDF source.
//!
//! Run:
//!     cargo run -p whatsapp_chrome_session_extract --bin whatsapp_kdf_dump --release -- \
//!           --profile-dir /tmp/wa-observer/run-1784043740549/chrome-profile/Default
//!
//! Output: NDJSON of every webpack module whose name matches crypto-related
//! patterns, plus the function source for matched modules.

use anyhow::{bail, Context, Result};
use chrono::Utc;
use clap::Parser;
use futures::{SinkExt, StreamExt};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::process::Stdio;
use tokio::time::{sleep, Duration};
use tracing::{info, warn};

#[derive(Parser, Debug)]
#[command(
    name = "whatsapp_kdf_dump",
    about = "Extract WA Web IndexedDB encryption module source via CDP"
)]
struct Args {
    #[arg(long)]
    chrome: Option<PathBuf>,
    #[arg(long, default_value_t = 9226)]
    port: u16,
    #[arg(long)]
    profile_dir: Option<PathBuf>,
    #[arg(long, default_value = "/tmp/wa-observer")]
    log_dir: PathBuf,
    #[arg(long, default_value_t = 25)]
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
        .context("no Chrome binary: pass --chrome /path")?;

    let profile = if let Some(p) = args.profile_dir.clone() {
        p
    } else {
        find_latest_profile(&args.log_dir).context("no profile_dir")?
    };

    info!(
        "launching chrome: {} --user-data-dir={} --remote-debugging-port={}",
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
        .arg("--window-size=900,800")
        .arg(format!(
            "--user-data-dir={}",
            profile.parent().unwrap_or(&profile).display()
        ))
        .arg("https://web.whatsapp.com")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .with_context(|| format!("spawn chrome at {}", chrome.display()))?;

    let endpoint_url = format!("http://127.0.0.1:{}", args.port);
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

    let http = reqwest::Client::new();
    let raw_list: Vec<Value> = http
        .get(format!("{endpoint_url}/json/list"))
        .send()
        .await?
        .json()
        .await?;
    let mut page_tab: Option<CdpPage> = None;
    for v in raw_list {
        if let Ok(p) = serde_json::from_value::<CdpPage>(v.clone()) {
            if p.page_type == "page" && !p.web_socket_debugger_url.is_empty() {
                page_tab = Some(p);
                break;
            }
        }
    }
    let tab = page_tab.context("no type=page tab found")?;
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
        let msg = json!({"id": id, "method": method, "params": params});
        ws.send(tokio_tungstenite::tungstenite::Message::Text(
            msg.to_string(),
        ))
        .await?;
        Ok(id)
    }

    send_cdp(&mut ws, &mut next_id, "Page.enable", json!({})).await?;

    // Inject the hook BEFORE any page scripts run. This installs a
    // .push() interceptor on webpackChunkwhatsapp_web_client so we
    // capture every module that gets loaded into the chunk.
    let hook_js = r#"
window.__kdfCaptured = [];
window.__kdfNetworkBodies = {};
// Hook Network via xhr/fetch interception — capture every JS response
// body so we can grep for IndexedDB encryption later.
(function() {
  const origFetch = window.fetch;
  if (origFetch) {
    window.fetch = async function(...args) {
      const resp = await origFetch.apply(this, args);
      try {
        const url = args[0]?.url || String(args[0]);
        if (url && (url.endsWith('.js') || url.includes('chunk') || url.includes('wawc'))) {
          const cloned = resp.clone();
          cloned.text().then(t => { window.__kdfNetworkBodies[url] = t.slice(0, 200000); }).catch(() => {});
        }
      } catch (_) {}
      return resp;
    };
  }
  // Hook webpackChunkwhatsapp_web_client.push as soon as it exists.
  let tries = 0;
  const hookInterval = setInterval(() => {
    tries++;
    const chunk = window['webpackChunkwhatsapp_web_client'];
    if (chunk && Array.isArray(chunk) && !chunk.__kdfHooked) {
      clearInterval(hookInterval);
      chunk.__kdfHooked = true;
      const origPush = chunk.push.bind(chunk);
      chunk.push = function(...args) {
        for (const arg of args) {
          try {
            if (Array.isArray(arg) && Array.isArray(arg[1])) {
              for (const m of arg[1]) {
                if (m && typeof m === 'object' && m.factory) {
                  try { window.__kdfCaptured.push({factorySrc: m.factory.toString(), modId: m.id ?? m[0] ?? null}); } catch (_) {}
                } else if (Array.isArray(m) && typeof m[1] === 'function') {
                  try { window.__kdfCaptured.push({factorySrc: m[1].toString(), modId: m[0] ?? null}); } catch (_) {}
                }
              }
            }
          } catch (_) {}
        }
        return origPush(...args);
      };
    }
    if (tries > 100) clearInterval(hookInterval);
  }, 50);
})();
"#;
    send_cdp(
        &mut ws,
        &mut next_id,
        "Page.addScriptToEvaluateOnNewDocument",
        json!({"source": hook_js}),
    )
    .await?;
    send_cdp(&mut ws, &mut next_id, "Network.enable", json!({})).await?;
    send_cdp(
        &mut ws,
        &mut next_id,
        "Page.navigate",
        json!({"url": "https://web.whatsapp.com"}),
    )
    .await?;

    info!(
        "navigated; waiting {}s for WA Web to boot...",
        args.wait_secs
    );
    sleep(Duration::from_secs(args.wait_secs)).await;

    // The webpack-chunk walks: `webpackChunkwhatsapp_web_client` is an array
    // of [chunkId, [moduleId, exportsObject], ...]. For each export we
    // look for functions whose .toString() matches crypto patterns.
    //
    // Strategy: ask JS to enumerate all chunk entries and for each
    // entry, walk its exports + look for `function` or `class` whose
    // name / source mentions IndexedDB encryption. Then dump those.
    let js = r#"
(async () => {
  const out = {modules: [], probe: {}};
  out.probe.chunkType = typeof webpackChunkwhatsapp_web_client;
  out.probe.chunkIsArray = Array.isArray(webpackChunkwhatsapp_web_client);
  out.probe.chunkLen = webpackChunkwhatsapp_web_client?.length;
  if (typeof webpackChunkwhatsapp_web_client === 'undefined') {
    out.error = 'webpackChunkwhatsapp_web_client not found';
    return out;
  }
  const chunks = webpackChunkwhatsapp_web_client;
  // The chunk is actually a flat array of [chunkId, modules] pairs
  // at the top level. The 'modules' for each chunk are themselves
  // [modId, factory, ...] tuples. The current Chrome 150 build uses
  // an inverted structure where modules are loaded into `chunks` via
  // push() as needed.
  //
  // Try a few structural shapes:
  let totalMods = 0;
  // Shape A: chunks is [id, [mods...]] pairs
  for (let ci = 0; ci < chunks.length; ci++) {
    const entry = chunks[ci];
    if (Array.isArray(entry) && Array.isArray(entry[1])) {
      for (const inner of entry[1]) {
        if (inner && typeof inner === 'object' && inner.factory) totalMods++;
      }
    } else if (entry && entry.factory) {
      totalMods++;
    }
  }
  out.probe.totalWithFactory = totalMods;

  // Check known globals for module registries
  out.probe.globals = {
    __webpack_require__: typeof window.__webpack_require__,
    __webpack_modules__: typeof window.__webpack_modules__,
  };

  const patterns = [
    /signal[A-Z]\w*Key/i,
    /decryptSignal/i,
    /encryptSignal/i,
    /IndexedDBCrypt/i,
    /SignalStore/i,
    /Noise[A-Z]\w*/i,
    /WANoise/i,
    /hkdf/i,
    /aesGcm|deriveBits|deriveKey/i,
    /subtle\.crypto/i,
  ];

  // Strategy: hook the webpackChunkwhatsapp_web_client.push to capture
  // modules as they load (between now and a simulated user click).
  if (!window.__kdfHookInstalled) {
    window.__kdfHookInstalled = true;
    window.__kdfCaptured = [];
    const origPush = webpackChunkwhatsapp_web_client.push.bind(webpackChunkwhatsapp_web_client);
    webpackChunkwhatsapp_web_client.push = function(...args) {
      try {
        for (const arg of args) {
          if (Array.isArray(arg) && Array.isArray(arg[1])) {
            for (const m of arg[1]) {
              if (m && typeof m === 'object' && m.factory) {
                try { window.__kdfCaptured.push({factorySrc: m.factory.toString(), modId: m.id ?? m[0] ?? null}); } catch (_) {}
              }
            }
          }
        }
      } catch (_) {}
      return origPush(...args);
    };
  }
  // Simulate user activity to trigger more module loads. Wait for the
  // page to settle + chrome to do its regular activity.
  await new Promise(r => setTimeout(r, 8000));
  out.probe.capturedAfterHook = window.__kdfCaptured.length;
  out.probe.networkBodiesCaptured = Object.keys(window.__kdfNetworkBodies || {}).length;
  out.probe.sampleNetworkUrls = Object.keys(window.__kdfNetworkBodies || {}).slice(0, 10);

  let seen = 0;
  outer:
  for (const cap of window.__kdfCaptured) {
    let matched = null;
    for (const re of patterns) {
      if (re.test(cap.factorySrc)) {
        matched = re.toString();
        break;
      }
    }
    if (!matched) continue;
    const src = cap.factorySrc.length > 8000 ? cap.factorySrc.slice(0, 8000) + '\n/*...TRUNCATED...*/' : cap.factorySrc;
    out.modules.push({modId: cap.modId, matchedPattern: matched,
      factoryLen: cap.factorySrc.length, factorySrc: src});
    seen++;
    if (seen > 20) break outer;
  }

  // Also grep network bodies for crypto patterns.
  for (const [url, body] of Object.entries(window.__kdfNetworkBodies || {})) {
    for (const re of patterns) {
      if (re.test(body)) {
        out.modules.push({source: 'network', url, matchedPattern: re.toString(),
          bodyLen: body.length, body: body.slice(0, 8000) + (body.length > 8000 ? '\n/*...TRUNCATED...*/' : '')});
        break;
      }
    }
    if (out.modules.length > 40) break;
  }

  return out;
})()
"#;
    let eval_id = send_cdp(
        &mut ws,
        &mut next_id,
        "Runtime.evaluate",
        json!({
            "expression": js,
            "returnByValue": true,
            "awaitPromise": true,
        }),
    )
    .await?;
    info!("KDF scan sent, awaiting result (up to 90s)...");

    let deadline = std::time::Instant::now() + Duration::from_secs(90);
    let mut result: Option<Value> = None;
    while std::time::Instant::now() < deadline {
        let remain = deadline.saturating_duration_since(std::time::Instant::now());
        let recv = tokio::time::timeout(remain, ws.next()).await;
        let msg = match recv {
            Ok(Some(Ok(m))) => m,
            Ok(Some(Err(e))) => {
                warn!("ws read err: {e}");
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
        let id = v.get("id").and_then(Value::as_u64).unwrap_or(0);
        if id == eval_id {
            if let Some(err) = v.get("error") {
                bail!("CDP error: {err}");
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
        .join("wa-web-crypto-modules.json");
    match result {
        Some(v) => {
            std::fs::write(
                &out_path,
                serde_json::to_string_pretty(&v).unwrap_or_default(),
            )
            .context("write dump")?;
            let mods = v.get("modules").and_then(Value::as_array);
            println!("== whatsapp_kdf_dump ==");
            println!("profile         : {}", profile.display());
            println!("captured_at     : {}", Utc::now().to_rfc3339());
            println!(
                "total chunk entries scanned: {}",
                v.get("totalChunkEntries").cloned().unwrap_or(json!(null))
            );
            println!(
                "crypto-matched modules     : {}",
                mods.map(|a| a.len()).unwrap_or(0)
            );
            println!("dump written to : {}", out_path.display());
            if let Some(arr) = mods {
                for m in arr {
                    let id = m.get("modId").cloned().unwrap_or(json!(null));
                    let matched = m.get("matchedPattern").cloned().unwrap_or(json!(null));
                    let len = m.get("factoryLen").cloned().unwrap_or(json!(null));
                    println!("  module id={} matched={} factoryLen={}", id, matched, len);
                }
            }
        }
        None => {
            println!("(no result received within 90s)");
        }
    }

    Ok(())
}
