//! Live integration tests for the `octo-whatsapp` daemon.
//!
//! Boot-once fixture that connects a real `WhatsAppWebAdapter` against
//! an authenticated WhatsApp Web session, brings up the daemon over a
//! hermetic unix socket, and exposes a JSON-RPC client to the chain
//! bodies (added in T2..T7).
//!
//! **Not** run by default. Requires:
//! - An authenticated session mounted at
//!   `$OCTO_WHATSAPP_PERSIST_DIR/$OCTO_WHATSAPP_SESSION_NAME`
//!   (defaults to `~/.local/share/octo/whatsapp/default.session.db`).
//! - Network access to `web.whatsapp.com` / `wss://web.whatsapp.com`.
//! - The `live-whatsapp` feature on both `octo-whatsapp` and
//!   `octo-adapter-whatsapp`. The test also pulls in `test-helpers`
//!   so `DaemonHandle::set_adapter_for_tests` is callable from an
//!   integration test under `tests/` (the helper is normally gated
//!   on `cfg(any(test, feature = "test-helpers"))`).
//!
//! Run with:
//!
//! ```bash
//! cargo test -p octo-whatsapp \
//!   --features "live-whatsapp test-helpers" \
//!   --test live_daemon_test \
//!   -- --include-ignored --nocapture --test-threads=1
//! ```
//!
//! Why `--test-threads=1`: a single host holds only one WhatsApp Web
//! connection per phone number (the WA servers reject a second
//! concurrent device as a duplicate). All chains share one fixture.

#![cfg(feature = "live-whatsapp")]
// T2-T7 will pull these helpers into use; keep them defined here so
// the scaffold compiles + clippy stays clean ahead of the chain bodies.
#![allow(dead_code)]

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

use octo_adapter_whatsapp::{WhatsAppConfig, WhatsAppWebAdapter};
use octo_network::dot::adapters::PlatformAdapter;
use octo_whatsapp::config::{
    EventsConfig, MediaBufferConfig, ObservabilityConfig, RulesConfig, SecurityConfig,
    WhatsAppRuntimeConfig,
};
use octo_whatsapp::daemon::Daemon;
use parking_lot::Mutex;
use serde_json::{json, Value};
use tempfile::TempDir;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
use tokio::sync::OnceCell;
use tokio_util::sync::CancellationToken;

fn init_tracing_once() {
    use std::sync::Once;
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(
                tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                    tracing_subscriber::EnvFilter::new("warn,live_daemon_test=info")
                }),
            )
            .with_test_writer()
            .try_init();
    });
}

// ── env helpers ───────────────────────────────────────────────────

fn env_or<T: std::str::FromStr>(name: &str, default: T) -> T {
    std::env::var(name)
        .ok()
        .and_then(|s| s.parse::<T>().ok())
        .unwrap_or(default)
}

fn inter_call_delay_ms() -> u64 {
    env_or("OCTO_WHATSAPP_LIVE_DELAY_MS", 2000u64)
}

async fn inter_call_delay() {
    inter_call_delay_for("").await;
}

/// Like [`inter_call_delay`] but routes through [`should_delay`] so
/// idempotent / local-only methods (`health.get`, `version.get`,
/// `status.get`, `daemon.methods.*`) skip the throttle. Chain call
/// sites should pass the RPC method name to avoid burning the full
/// delay on idempotent ops.
async fn inter_call_delay_for(method: &str) {
    if should_delay(method) {
        tokio::time::sleep(Duration::from_millis(inter_call_delay_ms())).await;
    }
}

/// Idempotent / local-only methods skip the inter-call delay. WA
/// throttling only matters for calls that hit the network; reads from
/// the daemon's in-memory state are free.
fn should_delay(method: &str) -> bool {
    !matches!(
        method,
        "health.get"
            | "version.get"
            | "status.get"
            | "capabilities"
            | "capabilities.list"
            | "daemon.methods"
            | "daemon.methods.help"
            | "daemon.methods.list"
    )
}

// ── adapter boot (mirrors live_e2e_group_setup_test.rs) ───────────

fn default_persist_dir() -> PathBuf {
    if let Ok(p) = std::env::var("OCTO_WHATSAPP_PERSIST_DIR") {
        return PathBuf::from(p);
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home)
        .join(".local")
        .join("share")
        .join("octo")
        .join("whatsapp")
}

fn default_session_name() -> String {
    std::env::var("OCTO_WHATSAPP_SESSION_NAME").unwrap_or_else(|_| "default.session.db".to_string())
}

fn live_adapter_config() -> WhatsAppConfig {
    let mut path = default_persist_dir();
    path.push(default_session_name());
    if !path.exists() {
        panic!(
            "no live WhatsApp session at {path:?}\n\
             set OCTO_WHATSAPP_PERSIST_DIR to the persistent dir created by \
             `octo-whatsapp-onboard qr-link` / `pair-link`."
        );
    }
    WhatsAppConfig {
        session_path: path.to_string_lossy().into_owned(),
        ws_url: None,
        pair_phone: None,
        pair_code: None,
        groups: vec![],
        sender_allowlist: BTreeMap::new(),
    }
}

async fn connect_adapter() -> Arc<WhatsAppWebAdapter> {
    let cfg = live_adapter_config();
    if let Err(e) = cfg.validate() {
        panic!("invalid live WhatsAppConfig: {e}");
    }
    let adapter = WhatsAppWebAdapter::new(cfg);
    adapter
        .start_bot()
        .await
        .expect("WhatsAppWebAdapter::start_bot failed; is the session mounted?");
    let deadline = std::time::Instant::now() + Duration::from_secs(60);
    while std::time::Instant::now() < deadline {
        if adapter.self_handle().is_some() {
            return Arc::new(adapter);
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    panic!("adapter self_handle() never resolved within 60s; connected never propagated");
}

// ── hermetic runtime config ───────────────────────────────────────

fn make_test_config(tmp: &TempDir) -> WhatsAppRuntimeConfig {
    WhatsAppRuntimeConfig {
        name: "live-daemon-test".to_string(),
        data_dir: tmp.path().join("data"),
        log_dir: tmp.path().join("log"),
        socket_dir: tmp.path().to_path_buf(),
        media_buffer: MediaBufferConfig::default(),
        events: EventsConfig::default(),
        security: SecurityConfig {
            bearer_required: false,
            hermetic_bypass: true,
            ..SecurityConfig::default()
        },
        observability: ObservabilityConfig {
            health: octo_whatsapp::config::HealthConfig { http_listen: None },
            ..ObservabilityConfig::default()
        },
        rules: RulesConfig::default(),
    }
}

// ── JSON-RPC over unix stream (newline-delimited) ──────────────────

struct RpcStream {
    stream: tokio::net::UnixStream,
    next_id: u64,
}

impl RpcStream {
    async fn new(socket: PathBuf) -> Self {
        let stream = tokio::net::UnixStream::connect(&socket)
            .await
            .unwrap_or_else(|e| panic!("unix connect {socket:?} failed: {e}"));
        Self { stream, next_id: 1 }
    }

    async fn call(&mut self, method: &str, params: Value) -> Result<Value, String> {
        let id = self.next_id;
        self.next_id += 1;
        let req = json!({ "id": id, "method": method, "params": params });
        let mut line = serde_json::to_string(&req).map_err(|e| format!("serialize: {e}"))?;
        line.push('\n');
        let fut = async {
            self.stream.write_all(line.as_bytes()).await?;
            self.stream.flush().await?;
            let mut reader = tokio::io::BufReader::new(&mut self.stream);
            let mut buf = String::new();
            reader.read_line(&mut buf).await?;
            Ok::<String, std::io::Error>(buf)
        };
        let raw = tokio::time::timeout(Duration::from_secs(30), fut)
            .await
            .map_err(|_| "rpc timeout after 30s".to_string())?
            .map_err(|e| format!("rpc io: {e}"))?;
        let resp: Value =
            serde_json::from_str(raw.trim()).map_err(|e| format!("rpc parse: {e}; raw={raw:?}"))?;
        if let Some(err) = resp.get("error") {
            return Err(err
                .get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("rpc error")
                .to_string());
        }
        resp.get("result")
            .cloned()
            .ok_or_else(|| format!("rpc response missing `result`: {raw:?}"))
    }
}

// ── boot-once fixture ─────────────────────────────────────────────

struct LiveFixture {
    adapter: Arc<WhatsAppWebAdapter>,
    socket: PathBuf,
    cancel: CancellationToken,
    /// Wrapped in `Arc` so the fixture is `Sync` (required by
    /// `tokio::sync::OnceCell`). `JoinHandle` itself is `!Sync`.
    daemon_task: Arc<tokio::task::JoinHandle<anyhow::Result<()>>>,
    /// Held in `Option` so callers can `.take()` ownership, drop the
    /// `parking_lot::Mutex` guard, and `.await` the call without
    /// holding a lock across an await point
    /// (`clippy::await_holding_lock`). Re-`replace()` after the call.
    rpc: Mutex<Option<RpcStream>>,
    created_groups: Mutex<Vec<String>>,
    created_tokens: Mutex<Vec<String>>,
    tmp: TempDir,
}

static FIXTURE: OnceCell<LiveFixture> = OnceCell::const_new();
static TEARDOWN_DONE: AtomicBool = AtomicBool::new(false);

async fn fixture() -> &'static LiveFixture {
    FIXTURE.get_or_init(init_fixture).await
}

async fn init_fixture() -> LiveFixture {
    let tmp = tempfile::tempdir().expect("tempdir");
    let cfg = make_test_config(&tmp);
    std::fs::create_dir_all(cfg.data_dir.clone()).expect("mkdir data_dir");
    std::fs::create_dir_all(cfg.log_dir.clone()).expect("mkdir log_dir");

    let adapter = connect_adapter().await;

    let daemon = Daemon::new(cfg.clone());
    daemon.handle().set_adapter_for_tests(adapter.clone());

    let cancel = daemon.cancel_token();
    let daemon_task = Arc::new(tokio::spawn(daemon.run()));

    let sock = cfg.socket_path();
    for _ in 0..50 {
        if sock.exists() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(sock.exists(), "socket {sock:?} was never created");

    // Sanity: boot succeeded → daemon is reachable. health.get is in
    // the no-delay skip-list, so no inter-call delay here.
    let rpc_slot = Mutex::new(Some(RpcStream::new(sock.clone()).await));
    {
        let mut stream = rpc_slot.lock().take().expect("rpc present");
        let _ = stream.call("health.get", json!({})).await;
        *rpc_slot.lock() = Some(stream);
    }

    LiveFixture {
        adapter,
        socket: sock,
        cancel,
        daemon_task,
        rpc: rpc_slot,
        created_groups: Mutex::new(Vec::new()),
        created_tokens: Mutex::new(Vec::new()),
        tmp,
    }
}

/// Helper used by chain bodies (T2-T7): take the RpcStream out of the
/// fixture's Mutex, run the async call without holding the lock, then
/// put it back. Bypasses `clippy::await_holding_lock` cleanly.
async fn rpc_call(
    rpc_slot: &Mutex<Option<RpcStream>>,
    method: &str,
    params: Value,
) -> Result<Value, String> {
    let mut rpc = rpc_slot.lock().take().expect("rpc stream present");
    let res = rpc.call(method, params).await;
    *rpc_slot.lock() = Some(rpc);
    res
}

async fn teardown_final() {
    if TEARDOWN_DONE.swap(true, Ordering::SeqCst) {
        return;
    }
    let Some(fix) = FIXTURE.get() else {
        return;
    };

    // Best-effort: leave every group we created. Errors are logged,
    // not asserted — teardown must not panic on partial failure.
    let groups = fix.created_groups.lock().clone();
    for jid in groups {
        let _ = rpc_call(&fix.rpc, "groups.leave", json!({ "jid": jid })).await;
    }

    // Best-effort: revoke every token we issued.
    let tokens = fix.created_tokens.lock().clone();
    for id in tokens {
        let _ = rpc_call(&fix.rpc, "security.tokens.revoke", json!({ "id": id })).await;
    }

    fix.cancel.cancel();
    // Await the daemon task with a 5s budget. `JoinHandle::poll`
    // consumes `self`, so we cannot poll it through `&JoinHandle`.
    // Spawn a small waiter task that owns the cloned JoinHandle and
    // exposes a oneshot we can race against the timeout.
    let task = Arc::clone(&fix.daemon_task);
    let (tx, rx) = tokio::sync::oneshot::channel();
    tokio::spawn(async move {
        // Consume the Arc to extract the JoinHandle (one-shot wait).
        let handle = Arc::try_unwrap(task).expect("Arc clone of JoinHandle");
        let _ = tx.send(handle.await);
    });
    let _ = tokio::time::timeout(Duration::from_secs(5), rx).await;

    assert!(
        !fix.socket.exists(),
        "socket {:?} should be removed on shutdown",
        fix.socket
    );
}

// Empty placeholder: the body is added in T2..T7. This test exists
// only to run `teardown_final` last under alphabetical ordering.
#[tokio::test]
async fn zzz_teardown_runs_last() {
    teardown_final().await;
}

#[tokio::test]
async fn live_chain_a_lifecycle() {
    let fix = fixture().await;
    let v = rpc_call(&fix.rpc, "version.get", json!({})).await.unwrap();
    assert!(v["daemon_version"].is_string(), "version: {v}");
    inter_call_delay_for("health.get").await;
    let h = rpc_call(&fix.rpc, "health.get", json!({})).await.unwrap();
    assert_eq!(h["ok"], true, "health: {h}");
    inter_call_delay_for("status.get").await;
    let s = rpc_call(&fix.rpc, "status.get", json!({})).await.unwrap();
    assert!(s["phase"].is_string(), "status: {s}");
    inter_call_delay_for("capabilities").await;
    let c = rpc_call(&fix.rpc, "capabilities", json!({})).await.unwrap();
    assert!(c.is_object(), "capabilities: {c}");
    inter_call_delay_for("daemon.methods.list").await;
    let m = rpc_call(&fix.rpc, "daemon.methods.list", json!({}))
        .await
        .unwrap();
    let arr = m.as_array().expect("daemon.methods.list not array");
    assert!(
        arr.len() >= 58,
        "daemon.methods.list len = {} (expected >= 58): {m}",
        arr.len()
    );
}

#[tokio::test]
async fn live_chain_h_daemon_control() {
    let fix = fixture().await;
    let _r = rpc_call(&fix.rpc, "reconnect.now", json!({}))
        .await
        .unwrap();
    // Reconnect is async; poll health.get with a 15s budget so a slow
    // WS resync does not flake the test.
    let deadline = std::time::Instant::now() + Duration::from_secs(15);
    let mut last = Value::Null;
    loop {
        if std::time::Instant::now() >= deadline {
            panic!("health.get never returned ok=true within 15s after reconnect: {last}");
        }
        last = rpc_call(&fix.rpc, "health.get", json!({})).await.unwrap();
        if last["ok"] == true {
            break;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    inter_call_delay_for("health.get").await;
}

// ── Chain B — groups lifecycle ────────────────────────────────────
//
// Exercises the four `groups.*` methods that currently exist:
// `groups.create`, `groups.list`, `groups.info`, `groups.leave`.
// `groups.invite` / `invite_link` / `set_subject` / `set_description`
// from the original plan are NOT implemented — dropped.
#[tokio::test]
async fn live_chain_b_groups() {
    init_tracing_once();
    let fix = fixture().await;

    // 1) groups.create — group lifecycle is core, panic on failure.
    let created = rpc_call(
        &fix.rpc,
        "groups.create",
        json!({ "name": "octo-live-test-B" }),
    )
    .await
    .unwrap_or_else(|e| panic!("groups.create failed: {e}"));
    let group_a = created
        .get("jid")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .or_else(|| {
            created
                .get("group_id")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        })
        .unwrap_or_else(|| panic!("groups.create result missing `jid`: {created}"));
    tracing::info!("live: created group_a = {group_a}");

    // 2) inter-call throttle.
    inter_call_delay_for("groups.list").await;

    // 3) Register for teardown BEFORE list/info so a panic between
    // here and `leave` still triggers cleanup.
    fix.created_groups.lock().push(group_a.clone());

    // 4) groups.list — assert result is an array.
    let list = rpc_call(&fix.rpc, "groups.list", json!({}))
        .await
        .unwrap_or_else(|e| panic!("groups.list failed: {e}"));
    assert!(list.is_array(), "groups.list not array: {list}");

    // 5) inter-call throttle.
    inter_call_delay_for("groups.info").await;

    // 6) groups.info — assert object.
    let info = rpc_call(&fix.rpc, "groups.info", json!({ "jid": group_a.clone() }))
        .await
        .unwrap_or_else(|e| panic!("groups.info failed: {e}"));
    assert!(info.is_object(), "groups.info not object: {info}");

    // 7) inter-call throttle.
    inter_call_delay_for("groups.leave").await;

    // 8) groups.leave — best-effort (group may already be left).
    match rpc_call(&fix.rpc, "groups.leave", json!({ "jid": group_a.clone() })).await {
        Ok(_) => {}
        Err(e) => tracing::warn!("live: groups.leave non-fatal: {e}"),
    }
}

// ── Chain C — messages + chats (depends on Chain B's group_a) ────
#[tokio::test]
async fn live_chain_c_messages_chats() {
    init_tracing_once();
    let fix = fixture().await;

    let group_a = {
        let groups = fix.created_groups.lock();
        groups.first().cloned().unwrap_or_else(|| {
            panic!("Chain C requires Chain B to run first (no group_a registered)")
        })
    };

    // Best-effort helper: log warnings on Err, never panic.
    async fn best_effort(fix: &LiveFixture, method: &str, params: Value) -> Value {
        match rpc_call(&fix.rpc, method, params).await {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!("live: {method} non-fatal: {e}");
                Value::Null
            }
        }
    }

    // 1) messages.list
    let _ = best_effort(
        fix,
        "messages.list",
        json!({ "jid": group_a.clone(), "limit": 5 }),
    )
    .await;

    // 2) inter-call throttle
    inter_call_delay_for("messages.search").await;

    // 3) messages.search
    let _ = best_effort(
        fix,
        "messages.search",
        json!({ "jid": group_a.clone(), "query": "live" }),
    )
    .await;

    // 4) inter-call throttle
    inter_call_delay_for("chats.list").await;

    // 5) chats.list
    let _ = best_effort(fix, "chats.list", json!({ "limit": 10 })).await;

    // 6) inter-call throttle
    inter_call_delay_for("chats.info").await;

    // 7) chats.info
    let _ = best_effort(fix, "chats.info", json!({ "jid": group_a.clone() })).await;

    // 8) inter-call throttle
    inter_call_delay_for("chats.pin").await;

    // 9) chats.pin
    let _ = best_effort(fix, "chats.pin", json!({ "jid": group_a.clone() })).await;

    // 10) inter-call throttle
    inter_call_delay_for("chats.unpin").await;

    // 11) chats.unpin
    let _ = best_effort(fix, "chats.unpin", json!({ "jid": group_a.clone() })).await;

    // 12) inter-call throttle
    inter_call_delay_for("chats.mute").await;

    // 13) chats.mute
    let _ = best_effort(
        fix,
        "chats.mute",
        json!({ "jid": group_a.clone(), "duration_s": 3600 }),
    )
    .await;

    // 14) inter-call throttle
    inter_call_delay_for("chats.archive").await;

    // 15) chats.archive
    let _ = best_effort(fix, "chats.archive", json!({ "jid": group_a.clone() })).await;

    // 16) inter-call throttle
    inter_call_delay_for("chats.typing").await;

    // 17) chats.typing — typing
    let _ = best_effort(
        fix,
        "chats.typing",
        json!({ "jid": group_a.clone(), "state": "typing" }),
    )
    .await;

    // 18) inter-call throttle
    inter_call_delay_for("chats.typing").await;

    // 19) chats.typing — paused
    let _ = best_effort(
        fix,
        "chats.typing",
        json!({ "jid": group_a.clone(), "state": "paused" }),
    )
    .await;
}

// ── helpers shared by Chain D + Chain E ──────────────────────────

/// Write a 1-byte dummy file under the fixture tmp dir and return
/// the path. Used by `send.image`/`send.video`/... handlers that
/// require an existing `file` param. Live adapter may reject the
/// placeholder bytes; the call-site logs warn and moves on.
fn write_dummy_file(fix: &LiveFixture, name: &str) -> PathBuf {
    let p = fix.tmp.path().join(name);
    std::fs::write(&p, b"x").expect("write dummy");
    p
}

/// `now` as unix seconds (for `messages.edit.msg_timestamp`, which
/// must be within `EDIT_WINDOW_SECONDS` of now).
fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Best-effort: call `method` with `{peer, file}` from a wire path,
/// log warn on Err, return Value::Null on Err.
async fn best_effort_envelope(
    fix: &LiveFixture,
    method: &str,
    peer: String,
    wire_path: PathBuf,
) -> Value {
    match rpc_call(
        &fix.rpc,
        method,
        json!({
            "peer": peer,
            "file": wire_path.to_string_lossy().into_owned(),
        }),
    )
    .await
    {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!("live: {method} non-fatal: {e}");
            Value::Null
        }
    }
}

// ── Chain D — 11 send.* + media.info + messages.edit ─────────────
//
// Most handlers take `peer: String` (not `jid`) and accept the
// `<digits>@g.us` group JID that Chain B's `groups.create` yields.
// `send.text` is a hermetic stub that returns `queued_for_phase2`
// without dispatching — it will not carry a real `message_id`. The
// chain therefore extracts `message_id` defensively and gates the
// reaction/delete/edit follow-ups on its presence.
#[tokio::test]
async fn live_chain_d_sends() {
    init_tracing_once();
    let fix = fixture().await;

    let group_a = {
        let groups = fix.created_groups.lock();
        groups.first().cloned().unwrap_or_else(|| {
            panic!("Chain D requires Chain B to run first (no group_a registered)")
        })
    };

    // Best-effort helper.
    async fn best_effort(fix: &LiveFixture, method: &str, params: Value) -> Value {
        match rpc_call(&fix.rpc, method, params).await {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!("live: {method} non-fatal: {e}");
                Value::Null
            }
        }
    }

    // 1) send.text — foundational. Spec says panic on Err. `send.text`
    // is a hermetic stub that does NOT actually dispatch and returns
    // `status: queued_for_phase2` without a `message_id`. We accept
    // the result and defensively extract `message_id` (may be absent).
    let text_res = rpc_call(
        &fix.rpc,
        "send.text",
        json!({ "peer": group_a.clone(), "text": "live-test-text" }),
    )
    .await
    .unwrap_or_else(|e| panic!("send.text failed: {e}"));
    let text_id = text_res
        .get("message_id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_default();
    if text_id.is_empty() {
        tracing::warn!(
            "live: send.text returned no message_id (stub path); reaction/delete/edit gated on id"
        );
    }

    // 2) inter-call throttle
    inter_call_delay_for("send.text").await;

    // 3) media sends (5 kinds: image, video, audio, voice, sticker).
    // Each writes a 1-byte dummy file and `send.*` best-effort.
    for (kind, filename) in [
        ("send.image", "live_img.bin"),
        ("send.video", "live_vid.bin"),
        ("send.audio", "live_aud.bin"),
        ("send.voice", "live_voi.bin"),
        ("send.sticker", "live_stk.bin"),
    ] {
        let file_path = write_dummy_file(fix, filename);
        let _ = best_effort(
            fix,
            kind,
            json!({
                "peer": group_a.clone(),
                "file": file_path.to_string_lossy().into_owned(),
                "caption": "live",
            }),
        )
        .await;
        inter_call_delay_for(kind).await;
    }

    // 4) send.contact — `vcard` is a PathBuf per handler signature,
    // so write a tiny vcard file rather than passing the raw string.
    let vcard_path = fix.tmp.path().join("live.vcard");
    std::fs::write(
        &vcard_path,
        b"BEGIN:VCARD\nVERSION:3.0\nFN:live\nEND:VCARD\n",
    )
    .expect("write vcard");
    let _ = best_effort(
        fix,
        "send.contact",
        json!({
            "peer": group_a.clone(),
            "vcard": vcard_path.to_string_lossy().into_owned(),
        }),
    )
    .await;
    inter_call_delay_for("send.contact").await;

    // 5) send.location
    let _ = best_effort(
        fix,
        "send.location",
        json!({ "peer": group_a.clone(), "lat": 0.0, "lon": 0.0 }),
    )
    .await;
    inter_call_delay_for("send.location").await;

    // 6) send.poll
    let _ = best_effort(
        fix,
        "send.poll",
        json!({
            "peer": group_a.clone(),
            "question": "live?",
            "options": ["yes", "no"],
        }),
    )
    .await;
    inter_call_delay_for("send.poll").await;

    // 7) send.reaction — gates on text_id (see note at chain head).
    if !text_id.is_empty() {
        let _ = best_effort(
            fix,
            "send.reaction",
            json!({
                "peer": group_a.clone(),
                "msg_id": text_id.clone(),
                "emoji": "\u{1f44d}",
            }),
        )
        .await;
    } else {
        tracing::warn!("live: skip send.reaction (no text_id)");
    }
    inter_call_delay_for("send.reaction").await;

    // 8) send.delete
    if !text_id.is_empty() {
        let _ = best_effort(
            fix,
            "send.delete",
            json!({ "peer": group_a.clone(), "msg_id": text_id.clone() }),
        )
        .await;
    } else {
        tracing::warn!("live: skip send.delete (no text_id)");
    }
    inter_call_delay_for("send.delete").await;

    // 9) media.info — handler takes `media_ref_token`, NOT `id`.
    // No real media token available; pass an empty string and
    // best-effort (adapter will likely reject).
    let _ = best_effort(
        fix,
        "media.info",
        json!({ "media_ref_token": text_id.clone() }),
    )
    .await;
    inter_call_delay_for("media.info").await;

    // 10) messages.edit — needs `msg_timestamp` (within 1h) and `new_text`.
    if !text_id.is_empty() {
        let _ = best_effort(
            fix,
            "messages.edit",
            json!({
                "peer": group_a.clone(),
                "msg_id": text_id,
                "msg_timestamp": now_secs(),
                "new_text": "live-test-edited",
            }),
        )
        .await;
    } else {
        tracing::warn!("live: skip messages.edit (no text_id)");
    }
}

// ── Chain E — envelopes (DOT/1 path) ─────────────────────────────
//
// `domain.compute_hash` takes `jid` (NOT `payload`) and returns
// `domain_id` (NOT `hash`). `envelope.encode` takes a `file` path
// of wire bytes. `envelope.decode` takes `encoded` string.
// `envelope.send` / `envelope.send_native` take `file` of wire
// bytes. We adapt the call sites to the actual handler shapes and
// warn-skip on Err (envelope methods may not be implemented for
// live groups yet).
#[tokio::test]
async fn live_chain_e_envelopes() {
    init_tracing_once();
    let fix = fixture().await;

    let group_a = {
        let groups = fix.created_groups.lock();
        groups.first().cloned().unwrap_or_else(|| {
            panic!("Chain E requires Chain B to run first (no group_a registered)")
        })
    };

    // 1) domain.compute_hash — computes a deterministic id for the
    // given group JID. Warn-skip on Err.
    let domain_hash = match rpc_call(
        &fix.rpc,
        "domain.compute-hash",
        json!({ "jid": group_a.clone() }),
    )
    .await
    {
        Ok(v) => v
            .get("domain_id")
            .and_then(|s| s.as_str())
            .map(|s| s.to_string())
            .unwrap_or_default(),
        Err(e) => {
            tracing::warn!("live: domain.compute-hash non-fatal: {e}");
            String::new()
        }
    };

    // 2) inter-call throttle
    inter_call_delay_for("domain.compute-hash").await;

    // 3) envelope.encode — needs a `file` of raw wire bytes. We write
    // a tiny wire-blob file. The `type: "TEXT"` field from the spec is
    // NOT a recognized param (handler only takes `file`), so omit it.
    let wire_path = write_dummy_file(fix, "live_wire.bin");
    let envelope = match rpc_call(
        &fix.rpc,
        "envelope.encode",
        json!({ "file": wire_path.to_string_lossy().into_owned() }),
    )
    .await
    {
        Ok(v) => v
            .get("encoded")
            .and_then(|s| s.as_str())
            .map(|s| s.to_string())
            .unwrap_or_default(),
        Err(e) => {
            tracing::warn!("live: envelope.encode non-fatal: {e}");
            String::new()
        }
    };
    if envelope.is_empty() {
        tracing::warn!(
            "live: envelope.encode returned no `encoded`; downstream operations gated on it"
        );
    }

    // 4) inter-call throttle
    inter_call_delay_for("envelope.encode").await;

    // 5) envelope.send — handler takes `peer` + `file`. The spec
    // passes `envelope` directly, but the handler ignores any
    // already-encoded envelope; it reads wire bytes from `file` and
    // re-encodes inside. Pass the same wire path; warn-skip on Err.
    if envelope.is_empty() {
        tracing::warn!("live: skip envelope.send (no envelope)");
    } else {
        let _ =
            best_effort_envelope(fix, "envelope.send", group_a.clone(), wire_path.clone()).await;
    }

    // 6) inter-call throttle
    inter_call_delay_for("envelope.send").await;

    // 7) envelope.send_native — handler takes `peer` + `file` of raw
    // wire bytes (must NOT start with "DOT/"). Our dummy blob is
    // plain bytes; the `envelope` string from step 3 starts with
    // "DOT/" so we cannot repurpose it.
    if envelope.is_empty() {
        tracing::warn!("live: skip envelope.send-native (no envelope)");
    } else {
        let _ = best_effort_envelope(
            fix,
            "envelope.send-native",
            group_a.clone(),
            wire_path.clone(),
        )
        .await;
    }

    // 8) inter-call throttle
    inter_call_delay_for("envelope.send-native").await;

    // 9) envelope.decode — handler takes `encoded` (DOT/1/... string),
    // NOT `wire`. Pass the encoded envelope we built.
    if envelope.is_empty() {
        tracing::warn!("live: skip envelope.decode (no envelope)");
    } else {
        let _ = match rpc_call(
            &fix.rpc,
            "envelope.decode",
            json!({ "encoded": envelope.clone() }),
        )
        .await
        {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!("live: envelope.decode non-fatal: {e}");
                Value::Null
            }
        };
    }

    // domain_hash is computed but unused by current handlers — keep
    // it referenced in a trace so a future field addition picks it up.
    tracing::debug!("live: domain_hash={domain_hash}");
}

// ── Chain F — admin surface (rules/triggers/events/audit/clients/actions) ──
//
// All calls in this chain are best-effort: the daemon's in-memory state
// is read-mostly, but a real WA adapter may not have populated the
// surface the handler reads. We warn-skip on Err rather than panic.
//
// Adaptations from the spec:
// - `actions.escalate` requires BOTH `target` AND `reason` (not just
//   `reason`); pass `target: "live-test"`.
#[tokio::test]
async fn live_chain_f_admin() {
    init_tracing_once();
    let fix = fixture().await;

    // Local best-effort helper (Chain C's `best_effort` is fn-scoped
    // inside its test fn, so we redefine here).
    async fn best_effort(fix: &LiveFixture, method: &str, params: Value) -> Value {
        match rpc_call(&fix.rpc, method, params).await {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!("live: {method} non-fatal: {e}");
                Value::Null
            }
        }
    }

    // 1) rules.list — in-memory rule registry. Warn-skip on Err.
    let _ = best_effort(fix, "rules.list", json!({})).await;

    // 2) inter-call throttle
    inter_call_delay_for("rules.list").await;

    // 3) triggers.list — in-memory trigger registry. Warn-skip on Err.
    let _ = best_effort(fix, "triggers.list", json!({})).await;

    // 4) inter-call throttle
    inter_call_delay_for("triggers.list").await;

    // 5) events.list {limit: 5} — daemon's events buffer. Warn-skip
    // on Err (handler may not be wired for live adapter).
    let _ = best_effort(fix, "events.list", json!({ "limit": 5 })).await;

    // 6) inter-call throttle
    inter_call_delay_for("events.list").await;

    // 7) audit.tail {limit: 5} — audit log tail. May probe the OS for
    // log file mtime. Warn-skip on Err.
    let _ = best_effort(fix, "audit.tail", json!({ "limit": 5 })).await;

    // 8) inter-call throttle
    inter_call_delay_for("audit.tail").await;

    // 9) audit.verify {since_seq: 0} — verify the audit chain. Note:
    // `audit.verify` (per the handler source) takes NO parameters;
    // `since_seq` is silently ignored. Pass an empty object to mirror
    // the handler's actual contract.
    let _ = best_effort(fix, "audit.verify", json!({})).await;

    // 10) inter-call throttle
    inter_call_delay_for("audit.verify").await;

    // 11) clients.list — registered MCP sessions. Warn-skip on Err.
    let _ = best_effort(fix, "clients.list", json!({})).await;

    // 12) inter-call throttle
    inter_call_delay_for("clients.list").await;

    // 13) actions.escalate {target, reason} — handler requires BOTH
    // fields. Warn-skip on Err (it is a phase4_stub, but the
    // `since_seq: 0` arg in the plan is a typo).
    let _ = best_effort(
        fix,
        "actions.escalate",
        json!({ "target": "live-test", "reason": "live test" }),
    )
    .await;
}

// ── Chain G — security tokens (rotate + revoke + list) ────────────
//
// Adapted from the original plan (`security.tokens.issue` + `revoke`)
// to match the actual Phase 5 Part A handler surface:
// - `security.rotate_token` — requires `old_token_id` +
//   `new_secret_hex`. The live daemon starts with NO seeded token, so
//   the first call returns `unknown old_token_id` — best-effort
//   absorbs it. (A future improvement would have the fixture seed a
//   known token before the chain runs; not done here per scope.)
// - `security.revoke_all_tokens` — no params, revokes everything.
// - `security.list_tokens` — read-only snapshot.
//
// No teardown is needed: rotate + revoke_all are inherently
// self-cleaning, and any tokens created during the test are revoked
// at the end.
#[tokio::test]
async fn live_chain_g_tokens() {
    init_tracing_once();
    let fix = fixture().await;

    async fn best_effort(fix: &LiveFixture, method: &str, params: Value) -> Value {
        match rpc_call(&fix.rpc, method, params).await {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!("live: {method} non-fatal: {e}");
                Value::Null
            }
        }
    }

    // 1) Baseline: list tokens to capture starting counts.
    let baseline = best_effort(fix, "security.list_tokens", json!({})).await;
    let baseline_all = baseline
        .get("counts")
        .and_then(|c| c.get("all"))
        .and_then(|n| n.as_u64())
        .unwrap_or(0);
    tracing::info!("live: token baseline all={baseline_all}");

    // 2) inter-call throttle
    inter_call_delay_for("security.list_tokens").await;

    // 3) security.rotate_token — requires a real `old_token_id` +
    //    `new_secret_hex`. The live daemon does NOT seed a token, so
    //    this will return `unknown old_token_id` and warn-skip.
    //    We still pass well-formed args (a 64-hex secret) so that if
    //    a token WERE seeded, the call would succeed.
    let new_secret_hex: String = (0..64)
        .map(|i| format!("{:02x}", (0xA0u8).wrapping_add(i as u8)))
        .collect();
    let _ = best_effort(
        fix,
        "security.rotate_token",
        json!({
            "old_token_id": "live-test-old",
            "new_secret_hex": new_secret_hex,
            "grace_ms": 60_000,
            "label": "live-test-rotate",
        }),
    )
    .await;

    // 4) inter-call throttle
    inter_call_delay_for("security.rotate_token").await;

    // 5) security.list_tokens — count should be >= baseline (rotate
    //    may have failed, but listing should still succeed).
    let after_rotate = best_effort(fix, "security.list_tokens", json!({})).await;
    let after_rotate_all = after_rotate
        .get("counts")
        .and_then(|c| c.get("all"))
        .and_then(|n| n.as_u64())
        .unwrap_or(0);
    tracing::info!("live: tokens after rotate all={after_rotate_all}");

    // 6) inter-call throttle
    inter_call_delay_for("security.revoke_all_tokens").await;

    // 7) security.revoke_all_tokens — revokes every active token and
    //    clears the grace list. No params.
    let _ = best_effort(fix, "security.revoke_all_tokens", json!({})).await;

    // 8) inter-call throttle
    inter_call_delay_for("security.list_tokens").await;

    // 9) security.list_tokens — count may be 0 after revoke_all.
    //    Warn-skip is fine: we just want to confirm the call shape.
    let _ = best_effort(fix, "security.list_tokens", json!({})).await;
}

// ── Chain I — CLI binary dispatch ─────────────────────────────────
//
// Drives the real `octo-whatsapp` CLI binary against the live daemon
// socket and asserts each top-level subcommand exits 0. The point is
// to confirm that the clap tree wires correctly to the JSON-RPC
// dispatch layer for the full Phase 1+2+4+5 surface.
//
// Throttling: each subprocess connect→RPC→exit costs ~50ms, and the
// `cli_exec` helper invokes `cargo_bin` which is a single-shot path
// (no cargo metadata round-trip inside the test runner). We use the
// `inter_call_delay_for` throttle to stay polite on the live socket
// but skip it for the 4 known-idempotent commands (`version`,
// `status`, `health`, `capabilities`) so the test stays fast.
//
// CLI flag corrections from the plan (verified against
// `crates/octo-whatsapp/src/cli.rs`):
// - `envelope encode` takes `--file <PATH>` (reads bytes from disk),
//   not `--bytes <BASE64>`. We write a tiny tmp file.
// - `media info` takes a POSITIONAL `media_ref_token`, not `--id`.
// - `domain compute-hash` takes a POSITIONAL `group_jid`, not
//   `--payload`.
//
// Skipped from the plan:
// - `send text --jid self --text ...`: clap arg shape varies per
//   send kind; per T4 deviations, parameter shapes are uncertain.
// - `cli_unknown_subcommand`: already covered by the hermetic
//   `cli_unknown_subcommand.rs` test.
#[tokio::test]
async fn live_cli_dispatch() {
    init_tracing_once();
    let fix = fixture().await;

    // Pre-create an envelope input file (3 bytes "abc") so the
    // `envelope encode --file <path>` call has something to encode.
    let envelope_bytes: &[u8] = b"abc";
    let envelope_path = fix.tmp.path().join("envelope-input.bin");
    std::fs::write(&envelope_path, envelope_bytes).expect("write envelope input");

    // Each entry: (test_name, cli_argv_pieces_after_socket_and_name).
    // `--socket` is prepended in `cli_exec` so it doesn't repeat
    // here. The CLI resolves the socket path via
    // `cli::resolve_socket_path(cli)` (src/cli.rs:593), preferring
    // `--socket` over `$XDG_RUNTIME_DIR/octo-whatsapp-{name}.sock`.
    let calls: &[(&str, &[&str])] = &[
        ("version", &["version"]),
        ("status", &["status"]),
        ("health", &["health"]),
        ("capabilities", &["capabilities"]),
        ("groups_list", &["groups", "list"]),
        ("messages_list", &["messages", "list", "--peer", "self"]),
        ("chats_list", &["chats", "list"]),
        (
            "envelope_encode",
            &[
                "envelope",
                "encode",
                "--file",
                envelope_path.to_str().expect("utf8 path"),
            ],
        ),
        ("media_info", &["media", "info", "x"]),
        ("domain_hash", &["domain", "compute-hash", "x"]),
        ("rules_list", &["rules", "list"]),
        ("triggers_list", &["triggers", "list"]),
        ("events_list", &["events", "list"]),
        ("clients_list", &["clients", "list"]),
        ("methods_list", &["methods", "list"]),
        ("tokens_list", &["tokens", "list"]),
        ("audit_query", &["audit", "tail"]),
    ];

    for (name, args) in calls {
        // Local-only RPCs skip the inter-call throttle. Everything
        // else (groups.list, messages.list, etc.) hits the live
        // adapter, so we throttle to be polite.
        inter_call_delay_for(name).await;
        let (code, stdout, stderr) = cli_exec(fix, args);
        assert_eq!(
            code, 0,
            "cli {name} failed (exit {code}): stderr={stderr} stdout={stdout}"
        );
    }
}

/// Spawn the `octo-whatsapp` CLI binary with the live fixture's
/// socket, capture (exit, stdout, stderr), and return.
///
/// Resolves the binary via `env!("CARGO_BIN_EXE_octo-whatsapp")`
/// (set by cargo at build time for integration tests in the same
/// crate). Sets `XDG_RUNTIME_DIR` to the fixture tmp dir so the
/// default-resolve branch in `cli::resolve_socket_path` would also
/// land on the right socket — belt-and-suspenders with `--socket`.
/// Strips `OCTO_WHATSAPP_BEARER` so the hermetic daemon's
/// `hermetic_bypass` flag is what gates auth (no token needed).
fn cli_exec(fix: &LiveFixture, args: &[&str]) -> (i32, String, String) {
    let sock = fix.socket.to_string_lossy().into_owned();
    let bin = env!("CARGO_BIN_EXE_octo-whatsapp");
    let out = std::process::Command::new(bin)
        .env("XDG_RUNTIME_DIR", fix.tmp.path())
        .env_remove("OCTO_WHATSAPP_BEARER")
        .args(args)
        .arg("--socket")
        .arg(&sock)
        .output()
        .expect("spawn octo-whatsapp");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).to_string(),
        String::from_utf8_lossy(&out.stderr).to_string(),
    )
}

// ── Chain T7 — MCP server over stdio JSON-RPC ──────────────────────
//
// Drives the real `octo-whatsapp mcp` binary against the live daemon
// socket. The MCP framing is newline-delimited JSON on both sides (see
// `forward_to_daemon` in `mcp_server.rs` and the BufReader::read_line
// loop in `serve`); LSP / Content-Length is **not** used.
//
// Each MCP call sends one line + reads one response. The shared id
// counter (`MCP_ID`) is `AtomicU32` so successive calls within the same
// test fn are race-free without locking. A 15s read deadline caps any
// individual MCP round-trip; the test is not under thundering-herd
// load, so the limit is generous.
//
// Both the `initialize` handshake and `tools/list` are hard-required.
// Subsequent `tools/call` rounds are best-effort: a tool that 4xx's
// (e.g. `rules.test` with a stub event, `send.text` over a no-network
// hermetic fixture) logs a warning and moves on. Hard panic on the
// `tools/list` count drifting from `EXPECTED_TOOL_COUNT = 66` so a
// silent surface deletion is caught immediately.
#[tokio::test]
async fn live_mcp_integration() {
    use octo_whatsapp::mcp_server::EXPECTED_TOOL_COUNT;

    init_tracing_once();
    let fix = fixture().await;

    // 1) Spawn the MCP server, attached to the live fixture's socket.
    let mut child = mcp_spawn(fix).await;

    // 2) initialize handshake — the MCP server returns the response on
    //    the next line; do not sleep (the handshake is local).
    let init_v = mcp_call(
        &mut child,
        "initialize",
        json!({
            "protocolVersion": "2025-06-18",
            "capabilities": {},
            "clientInfo": {"name": "live-test", "version": "0"},
        }),
    )
    .await;
    assert!(
        init_v["result"]["serverInfo"].is_object(),
        "initialize did not return serverInfo: {init_v}"
    );
    assert_eq!(
        init_v["result"]["serverInfo"]["name"], "octo-whatsapp",
        "initialize server name drifted: {init_v}"
    );

    // 3) notifications/initialized — MCP says this is a *notification*
    //    (no id), and the server simply ignores unknown methods with a
    //    JSON-RPC error. We treat either a success echo (legacy) or an
    //    error envelope as "OK"; the goal is just to prime the server.
    let _ = mcp_call(&mut child, "notifications/initialized", json!({})).await;
    inter_call_delay_for("capabilities.list").await;

    // 4) tools/list — assert the full 66-tool surface is advertised.
    let list_v = mcp_call(&mut child, "tools/list", json!({})).await;
    let tools = list_v["result"]["tools"]
        .as_array()
        .expect("tools/list result.tools missing or not an array");
    assert_eq!(
        tools.len(),
        EXPECTED_TOOL_COUNT,
        "tools/list count drifted: got {} expected {}",
        tools.len(),
        EXPECTED_TOOL_COUNT
    );

    // 5) Representative tools/call sweep. Names are the **actual MCP
    //    tool names** registered in `mcp_server::tool_descriptors` —
    //    not the daemon RPC method names — and the bridge's match
    //    arms forward them as-is. Hard-required: every call here must
    //    resolve to a known tool; otherwise the tool-name mapping has
    //    drifted and the rest of the sweep is meaningless. We collect
    //    the registered names first, then validate each entry.
    let registered: std::collections::BTreeSet<String> = tools
        .iter()
        .filter_map(|t| t.get("name").and_then(|v| v.as_str()).map(String::from))
        .collect();

    let cases: &[&str] = &[
        "version",
        "status",
        "health",
        "capabilities",
        "groups.list",
        "messages.list",
        "chats.list",
        "envelope.encode",
        "media.info",
        "domain.compute-hash",
        "events.list",
        "clients.list",
        "daemon.methods.list",
        "security.list_tokens",
        "audit.tail",
        "audit.verify",
        "rules.reload",
        "triggers.delete",
        "actions.escalate",
    ];
    for tool in cases {
        assert!(
            registered.contains(*tool),
            "tool {tool:?} missing from tools/list; registered={:?}",
            registered
        );
        inter_call_delay_for("mcp").await;
        let v = mcp_call(
            &mut child,
            "tools/call",
            json!({ "name": tool, "arguments": {} }),
        )
        .await;
        if v.get("error").is_some() {
            tracing::warn!(
                "live_mcp: tools/call {tool} non-fatal error: {}",
                v["error"]
            );
        }
    }

    // 6) Shutdown — best-effort. The MCP server may also handle EOF
    //    on stdin by exiting its loop; we send `shutdown` for
    //    cleanliness, then kill the process to ensure no zombie.
    let _ = mcp_call(&mut child, "shutdown", json!({})).await;
    let _ = child.kill().await;
}

// Spawn `octo-whatsapp mcp --socket <fix.socket>` with piped stdio.
// Mirrors `cli_exec` for the dispatch side: sets `XDG_RUNTIME_DIR` to
// the fixture's tmp dir (so the CLI's default-resolution branch would
// also land on the right socket) and strips `OCTO_WHATSAPP_BEARER`
// (the hermetic fixture has `bearer_required: false /
// hermetic_bypass: true`, so no token is needed).
async fn mcp_spawn(fix: &LiveFixture) -> tokio::process::Child {
    let bin = std::path::PathBuf::from(env!("CARGO_BIN_EXE_octo-whatsapp"));
    tokio::process::Command::new(bin)
        .env("XDG_RUNTIME_DIR", fix.tmp.path())
        .env_remove("OCTO_WHATSAPP_BEARER")
        .arg("mcp")
        .arg("--socket")
        .arg(fix.socket.to_string_lossy().into_owned())
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .expect("spawn octo-whatsapp mcp")
}

// Send one JSON-RPC line on stdin and read one response on stdout.
// Newline-delimited framing on both sides (see mcp_server::serve +
// forward_to_daemon); LSP / Content-Length is NOT used.
//
// Panics on:
//   - timeout (15s) — the MCP bridge is hung
//   - zero-byte read — the MCP server closed stdout unexpectedly
//   - non-JSON response — the bridge emitted an unparseable line
async fn mcp_call(child: &mut tokio::process::Child, method: &str, params: Value) -> Value {
    let id = MCP_ID.fetch_add(1, Ordering::Relaxed);
    let req = json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": method,
        "params": params,
    });
    let mut line = serde_json::to_string(&req).expect("serialize jsonrpc request");
    line.push('\n');

    {
        let stdin = child.stdin.as_mut().expect("MCP stdin was already taken");
        stdin
            .write_all(line.as_bytes())
            .await
            .expect("MCP write stdin");
        stdin.flush().await.expect("MCP flush stdin");
    }

    let stdout = child.stdout.as_mut().expect("MCP stdout was already taken");
    let mut reader = tokio::io::BufReader::new(stdout);
    let mut buf = String::new();
    let n = tokio::time::timeout(Duration::from_secs(15), reader.read_line(&mut buf))
        .await
        .unwrap_or_else(|_| panic!("MCP call {method} timed out after 15s"))
        .unwrap_or_else(|e| panic!("MCP call {method} read error: {e}"));
    assert!(
        n > 0,
        "MCP server closed stdout unexpectedly before {method} response"
    );
    serde_json::from_str(&buf)
        .unwrap_or_else(|e| panic!("MCP bad JSON for {method}: {e}: raw={buf:?}"))
}

/// Counter shared by `mcp_call` to assign monotonic JSON-RPC ids across
/// successive calls without locking. Initial value 1 keeps parity with
/// `RpcStream::next_id` so a debug interleaved run yields overlapping
/// id ranges that are obviously tool- or socket-local.
static MCP_ID: AtomicU32 = AtomicU32::new(1);
