//! `whatsapp_chrome_session_extract` — Phase 7.J: extract WA Web's Noise
//! session keys from an existing Chrome profile that has already logged in.
//!
//! Without these keys, the WS frames captured in
//! `/tmp/wa-observer/run-*/reconnect.jsonl` are ciphertext we cannot
//! decrypt. With them, we get:
//!
//!   - frame[2] decrypted = the exact `HandshakeMessage.clientHello`
//!     bytes Chrome sends (proto-payload size, field-by-field)
//!   - frame[5] decrypted = the post-handshake AppState IQ XML with
//!     all child elements
//!
//! That gives us the precise field-level diff vs wacore@551e574's emit.
//!
//! Approach: spawn Chrome pointing at an EXISTING profile dir (the
//! `/tmp/wa-observer/run-*/chrome-profile/` from a prior reconnect
//! observer run), CDP-connect, navigate to `web.whatsapp.com`, run JS
//! to read the `wawc` IndexedDB store, dump JSON.
//!
//! Default: uses the most recent `/tmp/wa-observer/run-*/chrome-profile`.
//!
//! Run:
//!     cargo run -p whatsapp_chrome_session_extract --release -- \
//!           [--chrome PATH] [--port 9225] \
//!           [--profile-dir <explicit path>] \
//!           [--log-dir /tmp/wa-observer]
//!
//! Local-only / no push.

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
    name = "whatsapp_chrome_session_extract",
    about = "Reuse a logged-in Chrome profile + extract WA Web Noise session keys via IndexedDB"
)]
struct Args {
    #[arg(long)]
    chrome: Option<PathBuf>,
    #[arg(long, default_value_t = 9225)]
    port: u16,
    #[arg(long)]
    profile_dir: Option<PathBuf>,
    #[arg(long, default_value = "/tmp/wa-observer")]
    log_dir: PathBuf,
    /// How long to wait for WA Web to boot before reading IndexedDB.
    #[arg(long, default_value_t = 15)]
    wait_secs: u64,
}

#[derive(Debug, serde::Deserialize)]
struct CdpPage {
    #[serde(default)]
    id: String,
    #[serde(default, rename = "webSocketDebuggerUrl")]
    web_socket_debugger_url: String,
    #[serde(default)]
    url: String,
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

/// Find the most recent chrome-profile under `<log-dir>/run-*`.
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
        .context("no Chrome binary: pass --chrome /path or install google-chrome")?;

    let profile = if let Some(p) = args.profile_dir.clone() {
        p
    } else {
        find_latest_profile(&args.log_dir)
            .context("no --profile-dir given and none found under log_dir")?
    };

    if !profile.join("IndexedDB").exists() {
        bail!(
            "profile {} has no IndexedDB dir — has WA Web ever been loaded here?",
            profile.display()
        );
    }

    info!(
        "launching chrome (reuse): {} --user-data-dir={} --remote-debugging-port={}",
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

    // Wait for CDP endpoint.
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

    // Find page tab.
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

    // Enable domains + navigate to web.whatsapp.com.
    send_cdp(&mut ws, &mut next_id, "Network.enable", json!({})).await?;
    send_cdp(&mut ws, &mut next_id, "Page.enable", json!({})).await?;
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

    // Read IndexedDB via Runtime.evaluate.
    let js = r#"
(async () => {
  // Try several known DB / store names that WA Web uses.
  const candidates = [
    {db: 'wawc', stores: ['_s']},
    {db: 'wawc', stores: ['key-version-store', 'prekey-store', 'session-store', 'contact-store']},
    {db: 'wawdb', stores: []},
  ];
  const out = {dbs: [], keys: []};
  try {
    const dbs = await indexedDB.databases();
    out.dbs = dbs.map(d => ({name: d.name, version: d.version}));
  } catch (e) { out.dbsErr = String(e); }
  for (const c of candidates) {
    try {
      const req = indexedDB.open(c.db);
      await new Promise((resolve, reject) => {
        req.onsuccess = resolve;
        req.onerror = () => reject(req.error);
      });
      const db = req.result;
      let storeNames = [];
      try { storeNames = Array.from(db.objectStoreNames); } catch (_) {}
      for (const sn of storeNames.length ? storeNames : c.stores) {
        try {
          const tx = db.transaction(sn, 'readonly');
          const store = tx.objectStore(sn);
          const getAll = store.getAllKeys ? store.getAllKeys() : Promise.resolve([]);
          const getValues = store.getAll ? store.getAll() : Promise.resolve([]);
          const [keys, values] = await Promise.all([
            new Promise(r => { getAll.onsuccess = () => r(getAll.result); getAll.onerror = () => r([]); }),
            new Promise(r => { getValues.onsuccess = () => r(getValues.result); getValues.onerror = () => r([]); }),
          ]);
          out.keys.push({db: c.db, store: sn, count: values.length, sampleKey: keys[0] ?? null, sampleValueLen: JSON.stringify(values[0] ?? null).length});
        } catch (e) {
          out.keys.push({db: c.db, store: sn, err: String(e)});
        }
      }
      db.close();
    } catch (e) {
      out.keys.push({db: c.db, openErr: String(e)});
    }
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
    info!("Runtime.evaluate sent, awaiting result...");

    // Read response (could take a while).
    let deadline = std::time::Instant::now() + Duration::from_secs(30);
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

    println!("== whatsapp_chrome_session_extract ==");
    println!("profile          : {}", profile.display());
    println!("captured_at      : {}", Utc::now().to_rfc3339());
    println!();
    match result {
        Some(v) => {
            // Pretty-print the IndexedDB scan summary.
            println!(
                "{}",
                serde_json::to_string_pretty(&v).unwrap_or_else(|_| v.to_string())
            );

            // Also save the full dump to a file for offline analysis.
            let out_path = profile
                .parent()
                .unwrap()
                .parent()
                .unwrap()
                .join("indexeddb-summary.json");
            std::fs::write(
                &out_path,
                serde_json::to_string_pretty(&v).unwrap_or_default(),
            )
            .context("write summary")?;
            println!();
            println!("summary written to: {}", out_path.display());
        }
        None => {
            println!("(no result received within 30s)");
        }
    }

    Ok(())
}
