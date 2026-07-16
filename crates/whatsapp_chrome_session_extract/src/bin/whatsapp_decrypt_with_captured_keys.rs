//! `whatsapp_decrypt_with_captured_keys` — Phase 7.J.6 S3 followup
//!
//! Takes the captured import keys from `whatsapp_capture_import_key` output
//! and tries each one + each of the 4 IVs from `WANoiseInfoIv` to AES-GCM
//! decrypt IDB values (signal-static-pubkey + signal-static-privkey +
//! signed-prekey + wawc_db_enc/keys[1]).
//!
//! If a candidate decrypts to a 32-byte plaintext (X25519 key length),
//! we've cracked the IndexedDB encryption.
//!
//! Run:
//!     cargo run -p whatsapp_chrome_session_extract --bin whatsapp_decrypt_with_captured_keys --release -- \
//!           --profile-dir /tmp/wa-observer/run-1784043740549/chrome-profile/Default --wait-secs 60

use anyhow::{bail, Context, Result};
use clap::Parser;
use futures::{SinkExt, StreamExt};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::process::Stdio;
use tokio::time::{sleep, Duration};
use tracing::info;

#[derive(Parser, Debug)]
struct Args {
    #[arg(long)]
    chrome: Option<PathBuf>,
    #[arg(long, default_value_t = 9232)]
    port: u16,
    #[arg(long)]
    profile_dir: Option<PathBuf>,
    #[arg(long, default_value = "/tmp/wa-observer")]
    log_dir: PathBuf,
    #[arg(long, default_value_t = 25)]
    wait_secs: u64,
    #[arg(
        long,
        default_value = "/tmp/wa-observer/run-1784043740549/captured-import-keys.json"
    )]
    captured_path: PathBuf,
}

#[derive(Debug, serde::Deserialize)]
#[allow(dead_code)]
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
        .with_env_filter("info")
        .with_writer(std::io::stderr)
        .init();

    let captured: Value = serde_json::from_str(&std::fs::read_to_string(&args.captured_path)?)?;
    let captured_imports = captured
        .get("imports")
        .and_then(Value::as_array)
        .context("no imports array")?
        .clone();

    // Extract AES-GCM raw imports
    let aes_keys: Vec<String> = captured_imports
        .iter()
        .filter(|i| {
            i.get("algName")
                .and_then(Value::as_str)
                .map(|s| s.contains("AES"))
                .unwrap_or(false)
                && i.get("format").and_then(Value::as_str) == Some("raw")
                && i.get("keyDataHex")
                    .and_then(Value::as_str)
                    .map(|s| s.len() == 64)
                    .unwrap_or(false)
        })
        .filter_map(|i| {
            i.get("keyDataHex")
                .and_then(Value::as_str)
                .map(|s| s.to_string())
        })
        .collect();
    info!(
        "using {} AES-GCM raw keys from captured imports",
        aes_keys.len()
    );

    let chrome = args
        .chrome
        .clone()
        .or_else(auto_find_chrome)
        .context("no Chrome")?;
    let profile = if let Some(p) = args.profile_dir.clone() {
        p
    } else {
        find_latest_profile(&args.log_dir).context("no profile")?
    };

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

    let list: Vec<Value> = reqwest::get(format!("{endpoint_url}/json/list"))
        .await?
        .json()
        .await?;
    let tab = list
        .iter()
        .find(|v| v["type"] == "page" && v["webSocketDebuggerUrl"].is_string())
        .context("no tab")?;
    let (mut ws, _) =
        tokio_tungstenite::connect_async(tab["webSocketDebuggerUrl"].as_str().unwrap()).await?;
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
        "Page.navigate",
        json!({"url": "https://web.whatsapp.com"}),
    )
    .await?;

    info!("waiting {}s for WA Web to load...", args.wait_secs);
    sleep(Duration::from_secs(args.wait_secs)).await;

    let aes_keys_json = serde_json::to_string(&aes_keys)?;
    let js = format!(
        r#"
(async () => {{
  const keys = {aes_keys_json};
  function b64d(s) {{
    try {{ return Uint8Array.from(atob(s), c => c.charCodeAt(0)); }}
    catch (e) {{ return null; }}
  }}
  function hex(b) {{ return Array.from(b).map(x => x.toString(16).padStart(2,'0')).join(''); }}
  function hexOrEmpty(b) {{ return b ? hex(new Uint8Array(b)) : ''; }}
  const out = {{localStorage: {{}}, idbMeta: [], attempts: []}};
  for (let i=0; i<localStorage.length; i++) {{
    const k = localStorage.key(i); out.localStorage[k] = localStorage.getItem(k);
  }}
  let wni = null;
  try {{ const r = localStorage.getItem('WANoiseInfo'); if (r) wni = JSON.parse(r); }} catch (_) {{}}
  out.wni = wni ? {{privKey: b64d(wni.privKey), pubKey: b64d(wni.pubKey), recoveryToken: b64d(wni.recoveryToken)}} : null;
  out.ivs = (() => {{
    try {{ return JSON.parse(localStorage.getItem('WANoiseInfoIv')).map(b64d); }} catch (_) {{ return []; }}
  }})();

  // Open signal-storage and dump each key's raw-ish value (via .value CryptoKey)
  async function dumpStore(dbName, storeName, primaryKey) {{
    try {{
      const db = await new Promise((res, rej) => {{
        const r = indexedDB.open(dbName);
        r.onsuccess = () => res(r.result); r.onerror = () => rej(r.error);
      }});
      const tx = db.transaction(storeName, 'readonly');
      const store = tx.objectStore(storeName);
      const v = await new Promise((res, rej) => {{
        const r = store.get(primaryKey);
        r.onsuccess = () => res(r.result); r.onerror = () => rej(r.error);
      }});
      db.close();
      return v;
    }} catch (e) {{ return {{err: String(e)}}; }}
  }}

  out.idbMeta.push({{where: 'signal-storage/signal-meta-store[signal-static-pubkey]', value: await dumpStore('signal-storage', 'signal-meta-store', 'signal-static-pubkey')}});
  out.idbMeta.push({{where: 'signal-storage/signal-meta-store[signal-static-privkey]', value: await dumpStore('signal-storage', 'signal-meta-store', 'signal-static-privkey')}});
  out.idbMeta.push({{where: 'wawc_db_enc/keys[1]', value: await dumpStore('wawc_db_enc', 'keys', 1)}});
  out.idbMeta.push({{where: 'signal-storage/signed-prekey-store[1]', value: await dumpStore('signal-storage', 'signed-prekey-store', 1)}});

  // Try decryption for each captured key + each IV
  for (let kIdx = 0; kIdx < keys.length; kIdx++) {{
    const keyHex = keys[kIdx];
    const keyBytes = new Uint8Array(keyHex.match(/../g).map(h => parseInt(h, 16)));
    let aesKey;
    try {{ aesKey = await crypto.subtle.importKey('raw', keyBytes, 'AES-GCM', false, ['decrypt']); }}
    catch (e) {{ out.attempts.push({{kIdx, keyHex: keyHex.slice(0, 16), err: 'import fail: ' + String(e).slice(0, 80)}}); continue; }}

    for (const iv of out.ivs) {{
      // Try decrypting the CryptoKey.serialized form via IDB
      // But IDB returns CryptoKey object, not raw bytes. We need the RAW
      // serialized form. Try crypto.subtle.exportKey on the inner value.
      for (const meta of out.idbMeta) {{
        const v = meta.value;
        if (!v || v.err) continue;
        // v might be a wrapper {{key: <CryptoKey>, id, _expiration}} or {{encKey, value}}
        const targets = [];
        if (v.key && v.key.algorithm) targets.push(['key', v.key]);
        if (v.encKey && v.encKey.algorithm) targets.push(['encKey', v.encKey]);
        if (v.value && v.value.algorithm) targets.push(['value', v.value]);
        if (v.privKey && v.privKey.algorithm) targets.push(['privKey', v.privKey]);
        if (v.pubKey && v.pubKey.algorithm) targets.push(['pubKey', v.pubKey]);
        for (const [label, ck] of targets) {{
          // Try decrypting the CryptoKey itself by treating its serialized form as ciphertext
          // V8 stores CryptoKey as opaque bytes — try to access via JSON.stringify and look for raw bytes
          try {{
            const ckJson = JSON.stringify(ck);
            // Convert any embedded ArrayBuffers (if any) to raw bytes
            const ct = new TextEncoder().encode(ckJson);
            const pt = await crypto.subtle.decrypt({{name: 'AES-GCM', iv}}, aesKey, ct);
            out.attempts.push({{
              kIdx, keyHex: keyHex.slice(0, 16), iv: hex(iv),
              where: meta.where + '.' + label,
              ok: true, ptLen: pt.byteLength, ptHex: hex(new Uint8Array(pt).slice(0, 64)),
            }});
          }} catch (e) {{
            // Likely AEAD failure — expected for wrong keys
          }}
        }}
      }}

      // Also try: read raw LevelDB values via direct access (we don't have that — JS API only)
      // Fall back: try to use crypto.subtle.exportKey on the inner CryptoKey objects
      for (const meta of out.idbMeta) {{
        const v = meta.value;
        if (!v || v.err) continue;
        const cks = [];
        if (v.key && v.key.algorithm) cks.push(v.key);
        if (v.encKey && v.encKey.algorithm) cks.push(v.encKey);
        if (v.value && v.value.algorithm) cks.push(v.value);
        if (v.privKey && v.privKey.algorithm) cks.push(v.privKey);
        if (v.pubKey && v.pubKey.algorithm) cks.push(v.pubKey);
        for (const ck of cks) {{
          try {{
            const exp = await crypto.subtle.exportKey('raw', ck);
            out.attempts.push({{
              kIdx, keyHex: keyHex.slice(0, 16), iv: hex(iv),
              where: meta.where + '.exportKey-raw',
              ok: 'exported', expLen: exp.byteLength, expHex: hex(new Uint8Array(exp).slice(0, 64)),
            }});
          }} catch (e) {{
            // Expected: non-extractable
          }}
        }}
      }}
    }}
  }}

  return out;
}})()
"#,
        aes_keys_json = aes_keys_json
    );

    let eval_id = send_cdp(
        &mut ws,
        &mut next_id,
        "Runtime.evaluate",
        json!({
            "expression": js, "returnByValue": true, "awaitPromise": true,
        }),
    )
    .await?;

    let deadline = std::time::Instant::now() + Duration::from_secs(60);
    let mut result: Option<Value> = None;
    while std::time::Instant::now() < deadline {
        let remain = deadline.saturating_duration_since(std::time::Instant::now());
        let recv = tokio::time::timeout(remain, ws.next()).await;
        let msg = match recv {
            Ok(Some(Ok(m))) => m,
            _ => continue,
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
                bail!("CDP err: {err}");
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
        .join("decrypt-attempts.json");
    if let Some(v) = result {
        std::fs::write(&out_path, serde_json::to_string_pretty(&v)?)?;
        println!("== whatsapp_decrypt_with_captured_keys ==");
        println!("profile        : {}", profile.display());
        println!("dump           : {}", out_path.display());
        println!();
        let attempts = v.get("attempts").and_then(Value::as_array);
        let n = attempts.map(|a| a.len()).unwrap_or(0);
        let oks: Vec<&Value> = match attempts {
            Some(arr) => arr
                .iter()
                .filter(|x| {
                    let ok = x.get("ok");
                    ok.and_then(Value::as_bool).unwrap_or(false)
                        || ok
                            .and_then(Value::as_str)
                            .map(|s| s == "exported")
                            .unwrap_or(false)
                })
                .collect(),
            None => Vec::new(),
        };
        println!("total attempts : {n}");
        println!("successes      : {}", oks.len());
        for o in &oks {
            println!("  {:?}", o);
        }
        // Specific: any exported raw key?
        let exported: Vec<&Value> = match attempts {
            Some(arr) => arr
                .iter()
                .filter(|x| x.get("ok").and_then(Value::as_str) == Some("exported"))
                .collect(),
            None => Vec::new(),
        };
        if !exported.is_empty() {
            println!();
            println!("EXPORTED KEYS FOUND:");
            for e in &exported {
                println!("  {:?}", e);
            }
        }
    } else {
        println!("(no result)");
    }

    Ok(())
}
