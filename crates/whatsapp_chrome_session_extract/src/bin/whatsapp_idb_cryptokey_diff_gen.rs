//! `whatsapp_idb_cryptokey_diff_gen` — Phase 7.J S2.5 step 1
//!
//! Generate four IDB databases holding CryptoKey objects with a controlled
//! design matrix:
//!
//! | Row | Algorithm   | Length | Extractable | Known key? |
//! |-----|-------------|--------|-------------|------------|
//! |  A  | AES-GCM     | 256    | true        | YES        |
//! |  B  | AES-GCM     | 256    | false       | YES        |
//! |  C  | HMAC        | 256    | true        | YES        |
//! |  D  | HMAC        | 256    | false       | YES        |
//!
//! Each row stores the key under idb key `k-<row>` in object store `keys`
//! inside IndexedDB database `diff-<row>.idb`.
//!
//! The known key bytes are embedded in the row name + the page so we can
//! search the LevelDB dumps for them later.
//!
//! Output: 4 Chrome profile directories under /tmp/idb-diff-<row>/<port>/chrome-profile
//! After the binary exits, the next binary (`whatsapp_idb_leveldb_diff`) reads
//! each profile's IndexedDB LevelDB files.
//!
//! Why: we need to know whether Chrome 150 stores raw key bytes (Case 1 — we
//! can recover them from disk and skip S6 field-value iteration), wrapped
//! ciphertext (Case 2 — need wrapping key), or an opaque handle (Case 3 — IDB
//! path is dead, continue with S6 patch).
//!
//! Run:
//!   cargo run -p whatsapp_chrome_session_extract --bin whatsapp_idb_cryptokey_diff_gen --release

use anyhow::{Context, Result};
use chrono::Utc;
use clap::Parser;
use futures::{SinkExt, StreamExt};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::process::Stdio;
use tokio::time::{sleep, Duration};
use tracing::info;

#[derive(Parser, Debug)]
struct Args {
    #[arg(long, default_value = "/tmp/idb-diff")]
    base_dir: PathBuf,
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

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

/// The HTML page we serve via `data:text/html;base64,...`.
///
/// Generates a CryptoKey with known-raw-bytes input, stores it in IDB, then
/// navigates to `about:blank` (which keeps the page alive until Chrome closes
/// but lets us exit the JS context quickly).
///
/// For each row we use a different algorithm and extractable flag.
/// We pass the raw key bytes as a constant in the JS so we can later grep
/// the LevelDB dump for those exact bytes.
fn page_html(row: &str, algo: &str, extractable: bool, key_bytes_hex: &str) -> String {
    let key_id = format!("k-{row}");
    let db_name = format!("diff-{row}");
    let store_name = "keys";
    let extractable_js = extractable;
    format!(
        r#"<!doctype html>
<html><head><title>diff {row}</title></head><body>
<script>
(async () => {{
  const row = "{row}";
  const keyBytesHex = "{key_hex}";
  const keyBytes = new Uint8Array(keyBytesHex.match(/../g).map(h => parseInt(h, 16)));
  window.__diag = {{row, stage: "init", keyHex: keyBytesHex, algo: "{algo}", extractable: {extractable_js}}};

  // importKey with raw bytes (algorithm-specific) for symmetric keys
  let key;
  try {{
    if ("{algo}" === "AES-GCM") {{
      key = await crypto.subtle.importKey("raw", keyBytes, {{name: "AES-GCM"}}, {extractable_js}, ["encrypt", "decrypt"]);
    }} else if ("{algo}" === "HMAC") {{
      key = await crypto.subtle.importKey("raw", keyBytes, {{name: "HMAC", hash: "SHA-256", length: 256}}, {extractable_js}, ["sign", "verify"]);
    }} else {{
      throw new Error("unknown algo: " + "{algo}");
    }}
    window.__diag.importOk = true;
  }} catch (e) {{
    window.__diag.importErr = String(e).slice(0, 200);
    document.title = "IMPORT-FAIL-" + row;
    return;
  }}

  // Open IDB and store the CryptoKey under "{key_id}"
  const dbName = "{db_name}";
  const storeName = "{store_name}";
  const keyId = "{key_id}";
  try {{
    const db = await new Promise((res, rej) => {{
      const r = indexedDB.open(dbName, 1);
      r.onupgradeneeded = () => {{
        const d = r.result;
        if (!d.objectStoreNames.contains(storeName)) {{
          d.createObjectStore(storeName);
        }}
      }};
      r.onsuccess = () => res(r.result);
      r.onerror = () => rej(r.error);
    }});
    const tx = db.transaction(storeName, "readwrite");
    tx.objectStore(storeName).put(key, keyId);
    await new Promise((res, rej) => {{
      tx.oncomplete = () => res();
      tx.onerror = () => rej(tx.error);
    }});
    db.close();
    window.__diag.idbStored = true;
    document.title = "OK-" + row + "-" + keyBytesHex.slice(0, 8);
  }} catch (e) {{
    window.__diag.idbErr = String(e).slice(0, 200);
    document.title = "IDB-FAIL-" + row;
  }}
}})();
</script>
</body></html>
"#,
        row = row,
        algo = algo,
        extractable_js = extractable_js,
        key_hex = key_bytes_hex,
        db_name = db_name,
        store_name = store_name,
        key_id = key_id,
    )
}

async fn drive_one(
    chrome: &PathBuf,
    port: u16,
    profile_dir: PathBuf,
    html: &str,
) -> Result<Value> {
    let profile_user_dir = profile_dir.join("chrome-user-data");
    std::fs::create_dir_all(&profile_user_dir).ok();

    // Write the HTML to disk so we can navigate via file:// (data: URLs in
    // modern Chrome get an opaque origin with restricted crypto.subtle + IDB
    // persistence semantics).
    let page_path = profile_user_dir.join("page.html");
    std::fs::write(&page_path, html).context("write page.html")?;
    let page_url = format!("file://{}", page_path.display());

    let mut child = tokio::process::Command::new(chrome)
        .arg(format!("--remote-debugging-port={port}"))
        .arg("--no-sandbox")
        .arg("--no-first-run")
        .arg("--no-default-browser-check")
        .arg("--disable-extensions")
        .arg("--disable-features=Translate,InfiniteSessionRestore")
        .arg("--disable-gpu")
        .arg("--window-size=800,600")
        .arg(format!("--user-data-dir={}", profile_user_dir.display()))
        .arg("about:blank")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .context("spawn chrome")?;

    let endpoint_url = format!("http://127.0.0.1:{port}");
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

    let (mut ws, _) =
        tokio_tungstenite::connect_async(&tab.web_socket_debugger_url).await?;
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
    send_cdp(
        &mut ws,
        &mut next_id,
        "Network.setCacheDisabled",
        json!({"cacheDisabled": true}),
    )
    .await?;
    send_cdp(
        &mut ws,
        &mut next_id,
        "Page.navigate",
        json!({"url": page_url}),
    )
    .await?;

    // Poll once per second for up to 12s, reading window.__diag. The eval
    // sends a unique id, then we read WS messages and pull out the matching
    // response. Event messages (no "id" field) are skipped.
    let mut diag: Option<Value> = None;
    for _attempt in 0..24 {
        sleep(Duration::from_millis(500)).await;
        let eval_id = send_cdp(
            &mut ws,
            &mut next_id,
            "Runtime.evaluate",
            json!({
                "expression": "JSON.stringify(window.__diag || {stage: 'no-script'})",
                "returnByValue": true,
            }),
        )
        .await?;
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        while std::time::Instant::now() < deadline {
            let remain = deadline.saturating_duration_since(std::time::Instant::now());
            let recv = tokio::time::timeout(remain, ws.next()).await;
            let msg = match recv {
                Ok(Some(Ok(m))) => m,
                _ => break,
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
                    diag = Some(json!({"eval_error": err}));
                } else {
                    diag = v
                        .get("result")
                        .and_then(|r| r.get("result"))
                        .and_then(|r| r.get("value"))
                        .cloned();
                }
                break;
            }
        }
        if let Some(d) = &diag {
            let s = d.to_string();
            if s.contains("\"idbStored\":true")
                || s.contains("\"idbErr\"")
                || s.contains("\"importErr\"")
            {
                break;
            }
        }
    }

    // Save diag to disk so we can confirm after Chrome is dead.
    let diag_path = profile_dir.join("diag.json");
    std::fs::write(
        &diag_path,
        serde_json::to_string_pretty(&diag.clone().unwrap_or(Value::Null))?,
    )?;
    info!("diag saved to {}", diag_path.display());

    let _ = ws.close(None).await;
    // Give Chrome 1s to flush IDB before SIGKILL.
    sleep(Duration::from_secs(1)).await;
    let _ = child.kill().await;
    let _ = child.wait().await;
    // Sometimes Chrome holds files open for a beat. Wait again so IDB files
    // are durable.
    sleep(Duration::from_secs(1)).await;

    Ok(diag.unwrap_or(Value::Null))
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    tracing_subscriber::fmt()
        .with_env_filter("info")
        .with_writer(std::io::stderr)
        .init();

    let chrome = auto_find_chrome().context("no chrome")?;
    let chrome_ver = std::process::Command::new(&chrome).arg("--version").output()?;
    info!(
        "chrome version: {}",
        String::from_utf8_lossy(&chrome_ver.stdout).trim()
    );

    std::fs::create_dir_all(&args.base_dir).ok();

    // Design matrix. The raw key bytes for AES row (AA..AABB..BB) and HMAC row
    // (CC..CCDD..DD) are designed so a byte-exact grep through the LDB files
    // can detect Case 1 (raw embedded). All 4 keys are distinct.
    let rows = [
        ("aes-ext-true",  "AES-GCM", true,  "aa".repeat(32)),
        ("aes-ext-false", "AES-GCM", false, "bb".repeat(32)),
        ("hmac-ext-true",  "HMAC", true,  "cc".repeat(32)),
        ("hmac-ext-false", "HMAC", false, "dd".repeat(32)),
    ];

    let manifest: serde_json::Value = serde_json::json!({
        "generated_at": Utc::now().to_rfc3339(),
        "chrome_version": String::from_utf8_lossy(&chrome_ver.stdout).trim().to_string(),
        "rows": rows.iter().map(|(name, algo, ext, key_hex)| {
            json!({
                "row": name,
                "algo": algo,
                "extractable": ext,
                "key_hex": key_hex,
                "key_id": format!("k-{name}"),
                "db_name": format!("diff-{name}"),
                "store_name": "keys",
                "port": 0, // filled below
                "profile_dir": "", // filled below
            })
        }).collect::<Vec<_>>()
    });

    let manifest_path = args.base_dir.join("manifest.json");
    let mut manifest_arr: Vec<Value> = manifest["rows"].as_array().cloned().unwrap_or_default();

    for (i, (row, algo, ext, key_hex)) in rows.iter().enumerate() {
        let port = 9300u16 + i as u16;
        let profile_dir = args.base_dir.join(format!("row-{i}"));
        std::fs::create_dir_all(&profile_dir).ok();

        let html = page_html(row, algo, *ext, key_hex);
        info!(
            "row={} algo={} extractable={} key={} port={}",
            row, algo, ext, &hex(key_hex.as_bytes())[..16], port
        );

        let diag = drive_one(&chrome, port, profile_dir.clone(), &html).await?;
        println!("  diag for row={}: {}", row, diag);

        manifest_arr[i]["port"] = json!(port);
        manifest_arr[i]["profile_dir"] = json!(profile_dir.display().to_string());

        // Save the key bytes alongside the profile so the diff binary knows
        // what to grep for.
        std::fs::write(
            profile_dir.join("expected-key.hex"),
            key_hex.as_bytes(),
        )?;
    }

    let final_manifest = json!({
        "generated_at": Utc::now().to_rfc3339(),
        "chrome_version": String::from_utf8_lossy(&chrome_ver.stdout).trim().to_string(),
        "rows": manifest_arr,
    });
    let final_path = manifest_path.clone();
    std::fs::write(
        final_path,
        serde_json::to_string_pretty(&final_manifest)?,
    )?;
    println!("manifest written to {}", manifest_path.display());
    println!("run `whatsapp_idb_leveldb_diff` next");
    Ok(())
}
