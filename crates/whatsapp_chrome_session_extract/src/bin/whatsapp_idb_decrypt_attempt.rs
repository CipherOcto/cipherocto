//! `whatsapp_idb_decrypt_attempt` — Plan B from Session 1:
//! brute-force the IndexedDB encryption KDF by trying known WA Web
//! key-derivation schemes against the `signal-static-pubkey` ciphertext.
//!
//! The IndexedDB `signal-static-pubkey` value is opaque from JS (CryptoKey).
//! We try to read it as ArrayBuffer via IDB; if that's a CryptoKey struct
//! we can't get raw bytes back from JS. In that case we read the raw
//! IndexedDB LevelDB instead (separate pass).
//!
//! Tries these KDF candidates (in order):
//!   1. SHA-256(WebEncKeySalt)             (174B → 32B)
//!   2. SHA-256(WANoiseInfo.privKey)        (48B → 32B)
//!   3. base64(WebEncKeySalt).slice(0, 32)  (direct truncation)
//!   4. HKDF-SHA256(WANoiseInfo.privKey,    salt=WebEncKeySalt, info="")
//!   5. HKDF-SHA256(WANoiseInfo.privKey,    salt=WebEncKeySalt, info="signal")
//!   6. HKDF-SHA256(WANoiseInfo.privKey,    salt=WebEncKeySalt, info="WhatsApp Signal Storage")
//!   7. HKDF-SHA256(WANoiseInfo.privKey + WebEncKeySalt, salt=empty)
//!   8. AES-GCM key = WebEncKeySalt itself  (try raw, no KDF)
//!
//! For each candidate, derive AES-GCM key, then try decrypt with each
//! of the 4 IVs from WANoiseInfoIv. Plaintext that is exactly 32 bytes
//! AND has high entropy is treated as a candidate X25519 public key.
//!
//! Run:
//!     cargo run -p whatsapp_chrome_session_extract --bin whatsapp_idb_decrypt_attempt --release -- \
//!           --profile-dir /tmp/wa-observer/run-1784043740549/chrome-profile/Default

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
    name = "whatsapp_idb_decrypt_attempt",
    about = "Brute-force WA Web IndexedDB encryption KDF via crypto.subtle"
)]
struct Args {
    #[arg(long)]
    chrome: Option<PathBuf>,
    #[arg(long, default_value_t = 9227)]
    port: u16,
    #[arg(long)]
    profile_dir: Option<PathBuf>,
    #[arg(long, default_value = "/tmp/wa-observer")]
    log_dir: PathBuf,
    #[arg(long, default_value_t = 18)]
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

    // Brute-force KDF candidates. The IndexedDB signal-static-pubkey
    // value is opaque from JS (CryptoKey), but we can still attempt
    // the KDF derivation directly with the localStorage keys we have.
    // If a KDF candidate produces a valid X25519 pubkey-looking plaintext,
    // we'll know the AES key is correct.
    let js = r#"
(async () => {
  const out = {localStorage: {}, signalMeta: {}, attempts: []};
  function b64d(s) {
    try { return Uint8Array.from(atob(s), c => c.charCodeAt(0)); }
    catch (e) { return null; }
  }
  function b64e(b) {
    let s = ''; for (let i=0;i<b.length;i++) s += String.fromCharCode(b[i]);
    return btoa(s);
  }
  function hex(b) {
    return Array.from(b).map(x => x.toString(16).padStart(2,'0')).join('');
  }
  // Pull localStorage
  for (let i=0; i<localStorage.length; i++) {
    const k = localStorage.key(i);
    out.localStorage[k] = localStorage.getItem(k);
  }
  // Parse WANoiseInfo
  let wni = null;
  try {
    const raw = localStorage.getItem('WANoiseInfo');
    if (raw) wni = JSON.parse(raw);
  } catch (_) {}
  out.wni = wni ? {privKey:b64d(wni.privKey), pubKey:b64d(wni.pubKey), recoveryToken:b64d(wni.recoveryToken)} : null;
  out.salt = b64d(JSON.parse(localStorage.getItem('WAWebEncKeySalt')));
  out.ivs = JSON.parse(localStorage.getItem('WANoiseInfoIv')).map(b64d);

  // Pull signal-static-pubkey via IDB. Get back whatever WA Web has stored.
  let stored = null;
  out.allDatabases = [];
  // First: enumerate ALL databases (Chrome 150+ supports this via indexedDB.databases())
  try {
    if (indexedDB.databases) {
      const dbs = await indexedDB.databases();
      out.allDatabases = dbs.map(d => ({name: d.name, version: d.version}));
    }
  } catch (e) {
    out.databasesErr = String(e);
  }
  // For each signal-* and wawc* db, dump all stores + their keys
  out.allStores = {};
  out.allValues = {};
  const interestingDbs = (out.allDatabases || []).map(d => d.name).filter(n => n && /signal|wawc|model/i.test(n));
  async function tryExport(label, key) {
    const out = {label};
    if (!key || typeof key !== 'object' || !key.algorithm) {
      out.notCryptoKey = true;
      return out;
    }
    out.alg = key.algorithm?.name || '?';
    out.extractable = key.extractable;
    out.usages = key.usages;
    let rawHex = null, jwkStr = null;
    const errs = [];
    try {
      const exp = await crypto.subtle.exportKey('raw', key);
      rawHex = hex(new Uint8Array(exp));
    } catch (e) {
      errs.push('raw: ' + String(e).slice(0, 100));
    }
    try {
      const exp = await crypto.subtle.exportKey('jwk', key);
      jwkStr = JSON.stringify(exp);
    } catch (e) {
      errs.push('jwk: ' + String(e).slice(0, 100));
    }
    out.raw = rawHex;
    out.jwk = jwkStr;
    out.errs = errs;
    return out;
  }
  async function walkValue(label, v) {
    const out = {label, ctor: v?.constructor?.name, type: typeof v};
    if (v === null || v === undefined) { out.nil = true; return out; }
    if (v instanceof ArrayBuffer) {
      const u = new Uint8Array(v);
      out.arrayBufferLen = u.length;
      out.head = hex(u.slice(0, 32));
      return out;
    }
    if (ArrayBuffer.isView(v)) {
      const u = new Uint8Array(v.buffer, v.byteOffset, v.byteLength);
      out.typedArrayLen = u.length;
      out.head = hex(u.slice(0, 32));
      return out;
    }
    if (typeof v === 'object') {
      const exports = [];
      if (v.algorithm) {
        // Direct CryptoKey
        exports.push(await tryExport(label, v));
      }
      // CryptoKeyPair shape: {encKey, value} OR {privKey, pubKey} OR {privateKey, publicKey}
      for (const ek of ['encKey', 'privKey', 'privateKey']) {
        if (v[ek] && v[ek].algorithm) {
          exports.push(await tryExport(label + '.' + ek, v[ek]));
        }
      }
      for (const pk of ['pubKey', 'publicKey', 'value']) {
        if (v[pk] && v[pk].algorithm) {
          exports.push(await tryExport(label + '.' + pk, v[pk]));
        }
      }
      // signature is usually ArrayBuffer
      if (v.signature instanceof ArrayBuffer || ArrayBuffer.isView(v.signature)) {
        let sig;
        if (v.signature instanceof ArrayBuffer) sig = new Uint8Array(v.signature);
        else sig = new Uint8Array(v.signature.buffer, v.signature.byteOffset, v.signature.byteLength);
        out.signatureLen = sig.length;
        out.signature = hex(sig);
      }
      if (exports.length) out.cryptoKeyExports = exports;
      out.sampleJson = JSON.stringify(v, (_, val) => (val && val.algorithm) ? '[CryptoKey ' + val.algorithm.name + ']' : val).slice(0, 400);
      return out;
    }
    out.preview = String(v).slice(0, 100);
    return out;
  }
  async function dumpStoreEntries(db, sn) {
    try {
      const tx = db.transaction(sn, 'readonly');
      const store = tx.objectStore(sn);
      const allKeys = await new Promise((res, rej) => {
        const r = store.getAllKeys();
        r.onsuccess = () => res(r.result);
        r.onerror = () => rej(r.error);
      });
      const out = {};
      for (const k of allKeys.slice(0, 20)) {
        try {
          const v = await new Promise((res, rej) => {
            const r = store.get(k);
            r.onsuccess = () => res(r.result);
            r.onerror = () => rej(r.error);
          });
          out[String(k)] = await walkValue('entry', v);
        } catch (e) {
          out[String(k)] = 'get err: ' + String(e).slice(0, 80);
        }
      }
      return out;
    } catch (e) {
      return 'err: ' + String(e);
    }
  }
  for (const dbName of interestingDbs) {
    try {
      const db = await new Promise((res, rej) => {
        const r = indexedDB.open(dbName);
        r.onsuccess = () => res(r.result);
        r.onerror = () => rej(r.error);
      });
      const storeNames = Array.from(db.objectStoreNames);
      const keysOnly = {};
      const fullDump = {};
      for (const sn of storeNames) {
        try {
          const tx = db.transaction(sn, 'readonly');
          const store = tx.objectStore(sn);
          const allKeys = await new Promise((res, rej) => {
            const r = store.getAllKeys();
            r.onsuccess = () => res(r.result);
            r.onerror = () => rej(r.error);
          });
          keysOnly[sn] = allKeys.slice(0, 50);
        } catch (e) { keysOnly[sn] = 'err: ' + String(e); }
        fullDump[sn] = await dumpStoreEntries(db, sn);
      }
      out.allStores[dbName] = {version: db.version, keys: keysOnly};
      out.allValues[dbName] = {version: db.version, values: fullDump};
      db.close();
    } catch (e) {
      out.allStores[dbName] = 'err: ' + String(e);
    }
  }
  // Try the legacy path: signal-storage/signal-meta-store
  try {
    const db = await new Promise((res, rej) => {
      const r = indexedDB.open('signal-storage');
      r.onsuccess = () => res(r.result);
      r.onerror = () => rej(r.error);
    });
    const tx = db.transaction('signal-meta-store', 'readonly');
    const store = tx.objectStore('signal-meta-store');
    const v = await new Promise((res, rej) => {
      const r = store.get('signal-static-pubkey');
      r.onsuccess = () => res(r.result);
      r.onerror = () => rej(r.error);
    });
    stored = v;
    db.close();
  } catch (e) {
    out.idbErr = String(e);
  }
  out.signalMeta = {
    storedType: stored === null ? 'null' : typeof stored,
    storedCtor: stored ? stored.constructor?.name : 'none',
    keys: stored && typeof stored === 'object' ? Object.keys(stored) : [],
  };

  // If stored has encKey/value shape, it might be a CryptoKey. We can
  // try to export it as JWK or raw. But crypto.subtle.exportKey needs
  // the CryptoKey to be extractable, which WA Web's may not be.
  // Try anyway.
  let rawBytes = null;
  if (stored && stored.encKey && stored.value && typeof stored.value === 'object') {
    try {
      // value is likely a CryptoKey
      rawBytes = await crypto.subtle.exportKey('raw', stored.value);
      out.signalMeta.exportable = true;
    } catch (e) {
      out.signalMeta.exportable = false;
      out.signalMeta.exportErr = String(e);
    }
  }
  if (rawBytes) {
    out.signalMeta.ciphertextLen = rawBytes.byteLength;
    out.signalMeta.ciphertextHead = hex(new Uint8Array(rawBytes).slice(0, 32));
    out.signalMeta.ciphertextTail = hex(new Uint8Array(rawBytes).slice(-32));
  }

  // Build key candidates
  if (!out.wni || !out.salt || !out.ivs || out.ivs.length === 0) {
    out.fatal = 'missing inputs';
    return out;
  }
  const privKey = out.wni.privKey;
  const salt = out.salt;
  const ivs = out.ivs;

  async function tryCandidate(name, keyBytes) {
    if (!keyBytes || keyBytes.length !== 32) return null;
    const key = await crypto.subtle.importKey('raw', keyBytes, 'AES-GCM', false, ['decrypt']);
    for (let i = 0; i < ivs.length; i++) {
      try {
        const pt = await crypto.subtle.decrypt({name:'AES-GCM', iv: ivs[i]}, key, rawBytes);
        const ptArr = new Uint8Array(pt);
        out.attempts.push({
          name, ivIdx: i, ok: true, plaintextLen: ptArr.length,
          plaintextHead: hex(ptArr.slice(0, 32)),
          plaintextFull: ptArr.length <= 64 ? hex(ptArr) : null,
        });
      } catch (e) {
        out.attempts.push({name, ivIdx: i, ok: false, err: String(e).slice(0, 80)});
      }
    }
  }

  // Candidate 1: SHA-256(salt)
  const c1 = new Uint8Array(await crypto.subtle.digest('SHA-256', salt));
  await tryCandidate('sha256(salt)', c1);

  // Candidate 2: SHA-256(privKey)
  const c2 = new Uint8Array(await crypto.subtle.digest('SHA-256', privKey));
  await tryCandidate('sha256(privKey)', c2);

  // Candidate 3: salt.slice(0, 32)
  if (salt.length >= 32) {
    await tryCandidate('salt.slice(0,32)', salt.slice(0, 32));
  }

  // Candidate 4: HKDF(privKey, salt, info='')
  const ikm4 = await crypto.subtle.importKey('raw', privKey, 'HKDF', false, ['deriveBits']);
  const dk4 = new Uint8Array(await crypto.subtle.deriveBits(
    {name:'HKDF', hash:'SHA-256', salt, info: new Uint8Array(0)}, ikm4, 256));
  await tryCandidate('hkdf(privKey, salt, info="")', dk4);

  // Candidate 5: HKDF(privKey, salt, info='signal')
  const dk5 = new Uint8Array(await crypto.subtle.deriveBits(
    {name:'HKDF', hash:'SHA-256', salt, info: new TextEncoder().encode('signal')}, ikm4, 256));
  await tryCandidate('hkdf(privKey, salt, info="signal")', dk5);

  // Candidate 6: HKDF(privKey, salt, info='WhatsApp Signal Storage')
  const dk6 = new Uint8Array(await crypto.subtle.deriveBits(
    {name:'HKDF', hash:'SHA-256', salt, info: new TextEncoder().encode('WhatsApp Signal Storage')}, ikm4, 256));
  await tryCandidate('hkdf(privKey, salt, info="WhatsApp Signal Storage")', dk6);

  // Candidate 7: HKDF(privKey+salt, no salt, info='')
  const concat = new Uint8Array(privKey.length + salt.length);
  concat.set(privKey); concat.set(salt, privKey.length);
  const ikm7 = await crypto.subtle.importKey('raw', concat, 'HKDF', false, ['deriveBits']);
  const dk7 = new Uint8Array(await crypto.subtle.deriveBits(
    {name:'HKDF', hash:'SHA-256', salt: new Uint8Array(0), info: new Uint8Array(0)}, ikm7, 256));
  await tryCandidate('hkdf(privKey+salt, empty salt, empty info)', dk7);

  // Candidate 8: HKDF(privKey, no salt, info=salt)
  const dk8 = new Uint8Array(await crypto.subtle.deriveBits(
    {name:'HKDF', hash:'SHA-256', salt: new Uint8Array(0), info: salt}, ikm4, 256));
  await tryCandidate('hkdf(privKey, empty salt, info=salt)', dk8);

  // Candidate 9: HKDF(salt, no salt, info='WhatsApp')
  const ikm9 = await crypto.subtle.importKey('raw', salt, 'HKDF', false, ['deriveBits']);
  const dk9 = new Uint8Array(await crypto.subtle.deriveBits(
    {name:'HKDF', hash:'SHA-256', salt: new Uint8Array(0), info: new TextEncoder().encode('WhatsApp')}, ikm9, 256));
  await tryCandidate('hkdf(salt, empty salt, info="WhatsApp")', dk9);

  // Candidate 10: SHA-256(privKey + salt)
  const c10 = new Uint8Array(await crypto.subtle.digest('SHA-256', concat));
  await tryCandidate('sha256(privKey+salt)', c10);

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
    info!("KDF brute force sent, awaiting result (up to 90s)...");

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
        .join("idb-decrypt-attempts.json");
    match result {
        Some(v) => {
            std::fs::write(
                &out_path,
                serde_json::to_string_pretty(&v).unwrap_or_default(),
            )
            .context("write dump")?;
            println!("== whatsapp_idb_decrypt_attempt ==");
            println!("profile        : {}", profile.display());
            println!("captured_at    : {}", Utc::now().to_rfc3339());
            println!("dump           : {}", out_path.display());
            println!();

            // Summarize
            if let Some(attempts) = v.get("attempts").and_then(Value::as_array) {
                let ok_count = attempts
                    .iter()
                    .filter(|a| a.get("ok") == Some(&json!(true)))
                    .count();
                let fatal = v.get("fatal");
                println!(
                    "attempts       : {} total, {} decrypted cleanly",
                    attempts.len(),
                    ok_count
                );
                if let Some(f) = fatal {
                    println!("fatal          : {f}");
                }
                if let Some(sm) = v.get("signalMeta") {
                    println!(
                        "storedType     : {}",
                        sm.get("storedType").cloned().unwrap_or(json!("?"))
                    );
                    println!(
                        "storedCtor     : {}",
                        sm.get("storedCtor").cloned().unwrap_or(json!("?"))
                    );
                    println!(
                        "exportable     : {}",
                        sm.get("exportable").cloned().unwrap_or(json!("?"))
                    );
                    if let Some(ct) = sm.get("ciphertextLen") {
                        println!("ciphertextLen  : {ct}");
                    }
                }
                println!();
                let oks: Vec<&Value> = attempts
                    .iter()
                    .filter(|a| a.get("ok") == Some(&json!(true)))
                    .collect();
                if !oks.is_empty() {
                    println!("DECRYPTION SUCCEEDED:");
                    for ok in oks {
                        println!(
                            "  {} iv={} len={} ptHead={}",
                            ok.get("name").and_then(Value::as_str).unwrap_or("?"),
                            ok.get("ivIdx").cloned().unwrap_or(json!("?")),
                            ok.get("plaintextLen").cloned().unwrap_or(json!("?")),
                            ok.get("plaintextHead")
                                .and_then(Value::as_str)
                                .unwrap_or("?")
                        );
                    }
                } else {
                    println!("NO KDF CANDIDATE DECRYPTED.");
                    let sample_err = attempts.first().and_then(|a| a.get("err")).cloned();
                    if let Some(e) = sample_err {
                        println!("sample error   : {e}");
                    }
                }
            }
        }
        None => println!("(no result received within 90s)"),
    }

    Ok(())
}
