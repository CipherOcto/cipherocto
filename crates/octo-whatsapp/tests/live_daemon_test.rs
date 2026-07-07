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
use std::sync::atomic::{AtomicBool, Ordering};
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
        arr.len() >= 60,
        "daemon.methods.list len = {} (expected >= 60): {m}",
        arr.len()
    );
}

#[tokio::test]
async fn live_chain_h_daemon_control() {
    let fix = fixture().await;
    let _r = rpc_call(&fix.rpc, "reconnect.now", json!({}))
        .await
        .unwrap();
    // reconnect may return {} or a status; the fact that .await did
    // not return Err means the daemon accepted the call. Wait for
    // the underlying WS layer to settle before sampling health.
    tokio::time::sleep(Duration::from_secs(5)).await;
    inter_call_delay_for("health.get").await;
    let h = rpc_call(&fix.rpc, "health.get", json!({})).await.unwrap();
    assert_eq!(h["ok"], true, "health after reconnect: {h}");
}
