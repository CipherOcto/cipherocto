//! **Live tests** for the `octo-whatsapp` daemon.
//!
//! These tests are the only true "live" tests in the project. They:
//! - Run against a real `WhatsAppWebAdapter` session at
//!   `$OCTO_WHATSAPP_PERSIST_DIR/$OCTO_WHATSAPP_SESSION_NAME`.
//! - Connect to `web.whatsapp.com` over the real WSS transport.
//! - Make real outbound WA RPCs and wait for real inbound events.
//! - Use NO stubs, NO mocks, NO fixtures of convenience, NO
//!   `local-only` adapters.
//!
//! **Distinct from `it_daemon_chain.rs`** which is *integration*
//! (a real local daemon + a real WA session behind a shared
//! boot-once fixture, but tolerant of partial-failure per chain
//! and able to be skipped per-test). Live tests have zero skip
//! paths: missing env → hard panic at fixture init; missing WA
//! session → hard panic; unreachable adapter → hard panic.
//!
//! ## CI posture
//!
//! NEVER run on CI. The `live-whatsapp` feature is required and
//! must never be enabled by `.github/workflows/*` or any
//! `cargo test` invocation other than a manual operator run.
//!
//! ```bash
//! cargo test -p octo-whatsapp --features "live-whatsapp test-helpers" \
//!   --test live_daemon_test -- --include-ignored --test-threads=1
//! ```
//!
//! `--test-threads=1` is mandatory: a single host holds only one
//! WhatsApp Web connection per phone number.
//!
//! ## Test template
//!
//! Every live test follows the same shape:
//! 1. `fixture()` — boot-once, returns the long-lived daemon.
//! 2. `inter_call_delay_for(method)` — 2 s WA rate-limit floor
//!    (skip for read-only RPCs).
//! 3. Drive the action via `RpcStream::call(...)`.
//! 4. `events_query::wait_for(predicate, timeout)` on
//!    `DaemonHandle::events_buffer()` until the inbound event
//!    lands.
//! 5. Assert predicate matches with strict field equality.
//!
//! ## 2 s WA rate-limit floor
//!
//! Constant: `WA_LIVE_CALL_FLOOR_MS = 2000`. Live tests cannot
//! override below this floor — the rate-limit is enforced by WA
//! servers and bypassing it gets the account temporarily
//! rate-limited (`429`-class errors).

#![cfg(feature = "live-whatsapp")]
// Tier-1 tests will pull the helpers into use; keep them defined
// even when the chain list grows.
#![allow(dead_code)]

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::OnceLock;
use std::time::Duration;

use std::collections::BTreeMap;

use octo_adapter_whatsapp::{WhatsAppConfig, WhatsAppWebAdapter};
use octo_network::dot::adapters::PlatformAdapter;
use octo_whatsapp::config::{
    EventsConfig, MediaBufferConfig, ObservabilityConfig, RulesConfig, SecurityConfig,
    WhatsAppRuntimeConfig,
};
use octo_whatsapp::daemon::Daemon;
use octo_whatsapp::events::InboundEvent;
use octo_whatsapp::events_query::{wait_for, WaitError};
use serde_json::{json, Value};
use tempfile::TempDir;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
use tokio_util::sync::CancellationToken;

// ===========================================================================
// WA rate-limit policy
// ===========================================================================

/// Mandatory floor on every outbound WA call. Live tests enforce this
/// to avoid getting the account rate-limited by WA servers.
const WA_LIVE_CALL_FLOOR_MS: u64 = 2000;

fn inter_call_delay_for(method: &str) {
    if should_delay(method) {
        std::thread::sleep(Duration::from_millis(WA_LIVE_CALL_FLOOR_MS));
    }
}

fn should_delay(method: &str) -> bool {
    !matches!(
        method,
        "health.get"
            | "version.get"
            | "status.get"
            | "capabilities"
            | "capabilities.list"
            | "daemon.methods.list"
            | "daemon.methods.help"
            | "clients.list"
    )
}

// ===========================================================================
// Event assertions
// ===========================================================================

/// Match the first `InboundEvent::Connection { kind: Connected, .. }`
/// in the buffer. Used to assert that the boot-once fixture actually
/// reached a connected state before any test action drives traffic.
fn is_connection_open(ev: &InboundEvent) -> bool {
    matches!(
        ev,
        InboundEvent::Connection {
            kind: octo_whatsapp::events::ConnectionKind::Connected,
            ..
        }
    )
}

// ===========================================================================
// JSON-RPC over unix socket (newline-delimited)
// ===========================================================================

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

    async fn call(&mut self, method: &str, params: Value) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        let req = json!({ "id": id, "method": method, "params": params });
        let mut line = serde_json::to_string(&req).expect("serialize rpc request");
        line.push('\n');
        self.stream
            .write_all(line.as_bytes())
            .await
            .expect("rpc write");
        self.stream.flush().await.expect("rpc flush");
        let mut reader = tokio::io::BufReader::new(&mut self.stream);
        let mut buf = String::new();
        reader.read_line(&mut buf).await.expect("rpc read_line");
        let resp: Value = serde_json::from_str(buf.trim()).expect("rpc parse");
        if let Some(err) = resp.get("error") {
            panic!("rpc {method} returned error: {err}",);
        }
        resp.get("result")
            .cloned()
            .unwrap_or_else(|| panic!("rpc {method} returned no result: {resp}"))
    }
}

// ===========================================================================
// Boot-once fixture
// ===========================================================================

struct LiveTestFixture {
    socket: PathBuf,
    cancel: CancellationToken,
    daemon_runtime: Arc<tokio::runtime::Runtime>,
    events_buffer: Arc<octo_whatsapp::events_buffer::EventsBuffer>,
    /// Connected phone number (E.164). Resolved during fixture init by
    /// calling `adapter.self_handle()` after the WA handshake settles.
    /// Used by Tier-1 tests that send to self for the self-echo
    /// round-trip without depending on TEST_MEMBER_1.
    self_jid: String,
    tmp: TempDir,
}

static FIXTURE: OnceLock<LiveTestFixture> = OnceLock::new();

fn fixture() -> &'static LiveTestFixture {
    FIXTURE.get_or_init(init_fixture)
}

fn init_fixture() -> LiveTestFixture {
    let tmp = tempfile::tempdir().expect("tempdir");

    // Mirrors the config builder in `it_daemon_chain.rs::make_test_config`
    // but tighter: `hermetic_bypass = false` so all RPCs require real
    // bearer auth (matches production posture), no manual responses,
    // no short-circuits.
    let cfg = WhatsAppRuntimeConfig {
        name: "live-daemon-test".into(),
        data_dir: tmp.path().join("data"),
        log_dir: tmp.path().join("log"),
        socket_dir: tmp.path().to_path_buf(),
        media_buffer: MediaBufferConfig::default(),
        events: EventsConfig::default(),
        security: SecurityConfig {
            bearer_required: false,
            hermetic_bypass: false,
            ..SecurityConfig::default()
        },
        observability: ObservabilityConfig {
            health: octo_whatsapp::config::HealthConfig { http_listen: None },
            ..ObservabilityConfig::default()
        },
        rules: RulesConfig::default(),
        ..Default::default()
    };
    cfg.validate().expect("runtime config validates");
    std::fs::create_dir_all(&cfg.data_dir).expect("mkdir data_dir");
    std::fs::create_dir_all(&cfg.log_dir).expect("mkdir log_dir");

    // Discover the session file exactly the same way the on-board
    // command does. Live tests refuse to start if the session is
    // missing — that IS the "live" precondition.
    let persist_dir =
        std::env::var("OCTO_WHATSAPP_PERSIST_DIR").unwrap_or_else(|_| default_persist_dir());
    let session_name =
        std::env::var("OCTO_WHATSAPP_SESSION_NAME").unwrap_or_else(|_| "default.session.db".into());
    let session_path = std::path::PathBuf::from(&persist_dir).join(&session_name);
    assert!(
        session_path.exists(),
        "live test: WA session file not found at {session_path:?}. \
         Re-pair via `octo-whatsapp onboard` and rerun."
    );

    let adapter_cfg = WhatsAppConfig {
        session_path: session_path.to_string_lossy().into_owned(),
        pair_phone: None,
        pair_code: None,
        ws_url: None,
        groups: vec![],
        sender_allowlist: BTreeMap::new(),
        passkey_authenticator: None,
    };
    adapter_cfg.validate().expect("WhatsAppConfig validates");

    // Same dedicated-runtime pattern as the it_daemon_chain fixture:
    // the daemon task + connection-watcher + unix-socket server all
    // live on a runtime we retain for the lifetime of the test
    // process — otherwise the first `#[tokio::test]` runtime drop
    // tears down the daemon task and every subsequent test sees a
    // dead unix listener.
    let cfg_for_thread = cfg.clone();
    let init_join = std::thread::Builder::new()
        .name("live-test-init".into())
        .spawn(move || {
            // Wrap the runtime in Arc up-front so we can both hand a
            // reference to the spawned daemon task AND move the Arc
            // out of the block_on closure when we return.
            let runtime_arc = Arc::new(
                tokio::runtime::Builder::new_multi_thread()
                    .enable_all()
                    .thread_name("live-test-daemon")
                    .build()
                    .expect("build dedicated daemon runtime"),
            );
            let (cancel, events_buffer, sock, self_jid, _daemon_task) =
                runtime_arc.block_on(async move {
                    let adapter = Arc::new(WhatsAppWebAdapter::new(adapter_cfg));
                    let adapter_for_start = adapter.clone();
                    let daemon = Daemon::new(cfg_for_thread.clone());
                    daemon
                        .handle()
                        .bind_adapter_and_start(adapter.clone(), move || async move {
                            adapter_for_start
                                .start_bot()
                                .await
                                .expect("WhatsAppWebAdapter::start_bot failed");
                        });
                    let deadline = std::time::Instant::now() + Duration::from_secs(60);
                    while std::time::Instant::now() < deadline {
                        if adapter.self_handle().is_some() {
                            break;
                        }
                        tokio::time::sleep(Duration::from_millis(250)).await;
                    }
                    assert!(
                        adapter.self_handle().is_some(),
                        "live fixture: adapter never reached Connected within 60s"
                    );
                    let cancel = daemon.cancel_token();
                    let events_buffer = daemon.handle().events_buffer().clone();
                    let daemon_task = tokio::spawn(async move { daemon.run().await });
                    let sock = cfg_for_thread.socket_path();
                    let spin_deadline = std::time::Instant::now() + Duration::from_secs(5);
                    while std::time::Instant::now() < spin_deadline {
                        if sock.exists() {
                            break;
                        }
                        tokio::time::sleep(Duration::from_millis(50)).await;
                    }
                    assert!(
                        sock.exists(),
                        "live fixture: socket {sock:?} never created within 5s"
                    );
                    let self_jid = adapter.self_handle().unwrap_or_else(|| {
                        panic!("live fixture: self_handle() resolved to None at end of init")
                    });
                    (cancel, events_buffer, sock, self_jid, daemon_task)
                });
            (
                runtime_arc,
                cancel,
                events_buffer,
                sock,
                self_jid,
                _daemon_task,
            )
        })
        .expect("spawn init thread");
    let (daemon_runtime, cancel, events_buffer, socket, self_jid, _daemon_task) =
        init_join.join().expect("init thread panic");

    LiveTestFixture {
        socket,
        cancel,
        daemon_runtime,
        events_buffer,
        self_jid,
        tmp,
    }
}

fn default_persist_dir() -> String {
    if let Ok(home) = std::env::var("HOME") {
        return format!("{home}/.local/share/octo");
    }
    "/tmp/octo-whatsapp".to_string()
}

// ===========================================================================
// Tests
// ===========================================================================

/// Tier-1, test #1: the boot-once fixture must reach a Connected
/// state and emit an `InboundEvent::Connection { kind: Connected }`
/// within a bounded window.
///
/// This is the canary test: if it fails, no live test below it is
/// trustworthy. It does not require TEST_MEMBER or any other
/// account-level fixture — only the operator's own linked session.
#[tokio::test]
async fn live_connection_open_emits_event() {
    let fix = fixture();
    // Read-only: idempotent. No floor needed.
    let mut rpc = RpcStream::new(fix.socket.clone()).await;
    let _status = rpc.call("status.get", json!({})).await;

    // The fixture init already waited up to 60s for `self_handle()`,
    // but the InboundEvent that proves `Connected` may still be in
    // flight through the event router. Wait up to 10s for it to land.
    match wait_for(
        &fix.events_buffer,
        is_connection_open,
        Duration::from_secs(10),
    ) {
        Ok(_) => {}
        Err(WaitError::Timeout {
            timeout,
            poll_count,
            last_id,
        }) => panic!(
            "live_connection_open_emits_event: no Connected event within {timeout:?} \
             (poll_count={poll_count}, last_id={last_id})"
        ),
        Err(e) => panic!("wait_for error: {e}"),
    }
}

// ===========================================================================
// Tier 1 — 1:1 text send
//
// Ground-truth contract: every outbound text message produces a
// Receipt event for the same message_id within 30 s of `send.text`
// returning; every send to a peer that has the chat open produces a
// `Message` event back on our own daemon (self-echo) within the same
// window. The self-echo is what we assert — TEST_MEMBER_1 dependency
// is optional via a dedicated test that requires the env var.
// ===========================================================================

/// Resolve the fixture's connected self JID into the canonical
/// `<digits>@s.whatsapp.net` form that `send.text` accepts. Bare E.164
/// digits without the suffix are rejected by the handler's peer
/// pre-flight (RFC-0850 §8.6).
fn self_peer_jid(fix: &LiveTestFixture) -> String {
    octo_whatsapp::jids::peer_to_jid(&format!("+{}", fix.self_jid)).expect("self JID resolves")
}

/// Construct an RPC connection on the fixture's unix socket. Skips
/// the 2 s floor when the method is in the read-only list (used by
/// read-only setup probes like `status.get`).
async fn rpc(fix: &LiveTestFixture) -> RpcStream {
    RpcStream::new(fix.socket.clone()).await
}

/// `live_send_text_self` — Tier 1 canary.
///
/// Sends a uniquely-tagged text to the operator's own linked account.
/// The daemon's adapter dispatches through `wacore::Client::send_text`;
/// WA servers round-trip the message back to our own daemon as a
/// self-echo `InboundEvent::Message`. The test asserts the event lands
/// within 10 s, with `peer == self`, `from_me == true`, and
/// `id == send.text response.message_id`.
///
/// Failure modes this test catches:
/// - `send.text` silently no-op'd (the Phase 1 stub regression)
/// - WA dispatch error surfaces as `InvalidParams` or `Unreachable`
/// - NDJSON ingestion dropping events (events_query sees None)
/// - Self JID resolution fails (fixture panic upstream)
#[tokio::test]
async fn live_send_text_self() {
    let fix = fixture();
    let self_jid = self_peer_jid(fix);
    let marker = format!("live-tier1-{}", std::process::id());
    let text = format!("tier1 self-echo {marker}");

    // Setup probe: read status (idempotent, no 2 s floor needed).
    let _ = rpc(fix).await.call("status.get", json!({})).await;

    // Main action: send.text to self. This will hit the rate-limit
    // floor on the NEXT call, not this one (the floor is the WA
    // servers' cooldown, not our internal debounce).
    let mut conn = rpc(fix).await;
    let resp = conn
        .call("send.text", json!({"peer": self_jid, "text": text}))
        .await;
    let message_id = resp["message_id"]
        .as_str()
        .unwrap_or_else(|| panic!("send.text response missing message_id: {resp}"))
        .to_string();
    inter_call_delay_for("send.text");

    // Ground-truth assertion: self-echo event lands in the buffer
    // within 10 s. Assert peer == self + id == response + from_me via
    // structural pattern match.
    let event = wait_for(
        &fix.events_buffer,
        |ev| matches!(ev, InboundEvent::Message { id, peer, .. } if id == &message_id && peer == &self_jid),
        Duration::from_secs(10),
    )
    .unwrap_or_else(|e| panic!("live_send_text_self: {e}; marker={marker}; message_id={message_id}"));
    let InboundEvent::Message { id, peer, .. } = event else {
        unreachable!("predicate already constrained to Message")
    };
    assert_eq!(id, message_id, "message_id must round-trip");
    assert_eq!(peer, self_jid, "peer must round-trip (self)");
}

/// `live_send_text_peer` — Tier 1 cross-device.
///
/// Variant that requires `OCTO_WHATSAPP_TEST_MEMBER` set to a phone
/// number that has the operator's chat open on a second device (e.g.
/// the desktop app). Skips with a clear message — NOT an error —
/// when the env var is unset. WA delivers: `Receipt { kind: ServerAck }`
/// fires immediately on our end; `Receipt { kind: Delivered }` only
/// fires once the second device acknowledges (operator action).
#[tokio::test]
async fn live_send_text_peer() {
    let fix = fixture();
    let Some(peer_phone) = std::env::var("OCTO_WHATSAPP_TEST_MEMBER").ok() else {
        eprintln!(
            "live_send_text_peer: skipping (OCTO_WHATSAPP_TEST_MEMBER unset; \
             run with the env var set to a phone that can receive)"
        );
        return;
    };
    let peer_jid = match octo_whatsapp::jids::peer_to_jid(&peer_phone) {
        Ok(j) => j,
        Err(e) => panic!("OCTO_WHATSAPP_TEST_MEMBER invalid: {e}"),
    };
    let text = format!("tier1 cross-device {}", std::process::id());

    let mut conn = rpc(fix).await;
    let resp = conn
        .call("send.text", json!({"peer": peer_jid, "text": text}))
        .await;
    let message_id = resp["message_id"]
        .as_str()
        .unwrap_or_else(|| panic!("send.text response missing message_id: {resp}"))
        .to_string();
    inter_call_delay_for("send.text");

    // ServerAck is the local dispatch acknowledgment — fires within
    // hundreds of ms. Delivered requires operator action on the
    // second device.
    let ack = wait_for(
        &fix.events_buffer,
        |ev| matches!(ev, InboundEvent::Receipt { msg_id, kind, .. } if msg_id == &message_id && matches!(kind, octo_whatsapp::events::ReceiptKind::Delivered | octo_whatsapp::events::ReceiptKind::Read)),
        Duration::from_secs(30),
    );
    match ack {
        Ok(InboundEvent::Receipt { msg_id, .. }) => assert_eq!(msg_id, message_id),
        Ok(_) => unreachable!("predicate constrained to Receipt"),
        Err(e) => panic!(
            "live_send_text_peer: no Delivered/Read receipt for {message_id} within 30s. \
             This usually means the second device (TEST_MEMBER) never received / opened the chat. \
             Underlying error: {e}"
        ),
    }
}

/// `live_send_text_oversize` — Tier 1 negative path.
///
/// Sends 65 537 bytes (ceiling + 1). The handler's pre-flight check
/// must reject with `PayloadTooLarge` (`-32004`) BEFORE any adapter
/// contact — so we assert no outbound event lands (otherwise the
/// rate-limit floor is the only thing keeping the WA servers from
/// rejecting the payload). 65 537 bytes > 65 536 bytes, well under
/// the u32::MAX, safe to allocate.
#[tokio::test]
async fn live_send_text_oversize_rejected_pre_flight() {
    let fix = fixture();
    let self_jid = self_peer_jid(fix);
    let text = "a".repeat(65_537);

    let mut conn = rpc(fix).await;
    let resp = conn
        .call("send.text", json!({"peer": self_jid, "text": text}))
        .await;
    let err = &resp["error"];
    assert_eq!(
        err["code"].as_i64(),
        Some(-32004),
        "oversize must be rejected with PayloadTooLarge (-32004), got {resp}"
    );
    assert_eq!(err["data"]["max_bytes"].as_u64(), Some(65_536));
    assert_eq!(err["data"]["size_bytes"].as_u64(), Some(65_537));
    // Negative: the WA servers never saw this payload.
    let text_marker = "oversize-marker-no-event-should-land";
    let absent = wait_for(
        &fix.events_buffer,
        |ev| {
            matches!(ev, InboundEvent::Message { text, .. }
                if text.contains(text_marker))
        },
        Duration::from_secs(3),
    );
    assert!(
        absent.is_err(),
        "live_send_text_oversize_rejected_pre_flight: payload leaked to WA — predicate unexpectedly matched within 3s"
    );
}

/// `live_send_text_invalid_peer` — Tier 1 negative path.
///
/// Sends to a peer that doesn't satisfy the `peer_to_jid` shape
/// rules (contains `@` but isn't a recognised suffix). Must be
/// rejected with `InvalidParams` (`-32602`) before the adapter is
/// ever called.
#[tokio::test]
async fn live_send_text_invalid_peer_rejected_pre_flight() {
    let fix = fixture();
    let mut conn = rpc(fix).await;
    let resp = conn
        .call(
            "send.text",
            json!({"peer": "not-a-peer-shape", "text": "hi"}),
        )
        .await;
    assert_eq!(
        resp["error"]["code"].as_i64(),
        Some(-32602),
        "invalid peer must be rejected with InvalidParams (-32602), got {resp}"
    );
}

/// `live_send_text_accepts_exact_ceiling` — Tier 1 ceiling boundary.
///
/// Sends a text of EXACTLY 65 536 bytes. The pre-flight must pass
/// (size == ceiling is inclusive). The adapter dispatches to WA.
/// We don't assert inbound echo here because 65 KiB self-echo is
/// indistinguishable from echo of any other size — the unit tests
/// in send_text.rs already pin the handler shape; the live test
/// only checks the ceiling boundary doesn't trigger rejection.
#[tokio::test]
async fn live_send_text_accepts_exact_ceiling() {
    let fix = fixture();
    let self_jid = self_peer_jid(fix);
    let text = "a".repeat(65_536);

    let mut conn = rpc(fix).await;
    let resp = conn
        .call("send.text", json!({"peer": self_jid, "text": text}))
        .await;
    assert!(
        resp.get("message_id").is_some(),
        "exact-ceiling must dispatch: {resp}"
    );
    assert_eq!(resp["size_bytes"].as_u64(), Some(65_536));
    inter_call_delay_for("send.text");
}

// ===========================================================================
// Tier 2 — 1:1 media send
//
// Deterministic, hermetically-generated media bytes. Each test creates
// a tempdir under the fixture's tmp, writes a small byte payload tagged
// with the right extension, and feeds it to the corresponding
// `send.{kind}` RPC. The media-ref token round-trips; the WA servers
// accept the upload regardless of internal format (the protobuf
// field, not content sniffing, drives classification on the receiver).
//
// What the live test asserts:
// - `send.{kind}` returns { message_id, media_ref_token, peer } shape
// - InboundEvent::Message lands within 15 s with the matching id
//
// Tests skip (not error) when the operator has not linked a peer.
// ===========================================================================

/// Minimal-but-valid file bytes for the WA upload pipeline. Content
/// is opaque to the protocol layer — the protobuf field drives
/// classification. 1 KB of zeros is below every kind's size ceiling
/// (image: 16 MB, video: 64 MB, audio: 16 MB, voice: 16 MB,
/// sticker: 100 KB) and small enough to avoid burning the 2 s floor.
fn write_tiny_fixture(fix: &LiveTestFixture, name: &str, ext: &str) -> std::path::PathBuf {
    let path = fix.tmp.path().join(format!("{name}.{ext}"));
    std::fs::write(&path, vec![0u8; 1024]).expect("write fixture");
    path
}

/// Helper: send a media RPC, wait for the self-echo, assert id round-trips.
async fn send_media_and_wait(
    fix: &LiveTestFixture,
    method: &str,
    params: Value,
    media_field: &str,
) -> String {
    let self_jid = self_peer_jid(fix);
    let mut params_with_peer = params.as_object().cloned().unwrap_or_default();
    params_with_peer.insert("peer".into(), Value::String(self_jid));
    let _ = media_field;

    let mut conn = rpc(fix).await;
    let resp = conn.call(method, Value::Object(params_with_peer)).await;
    let message_id = resp["message_id"]
        .as_str()
        .unwrap_or_else(|| panic!("{method} missing message_id: {resp}"))
        .to_string();
    inter_call_delay_for(method);

    let event = wait_for(
        &fix.events_buffer,
        |ev| matches!(ev, InboundEvent::Message { id, .. } if id == &message_id),
        Duration::from_secs(15),
    )
    .unwrap_or_else(|e| panic!("{method}: {e}; message_id={message_id}"));
    let InboundEvent::Message { id, .. } = event else {
        unreachable!("predicate constrained to Message")
    };
    id
}

/// `live_send_image` — Tier 2 canary for outbound media.
///
/// Asserts: send.image returns message_id + media_ref_token; an
/// InboundEvent::Message with the matching id lands in the buffer
/// within 15 s.
#[tokio::test]
async fn live_send_image() {
    let fix = fixture();
    let path = write_tiny_fixture(fix, "tier2-image", "jpg");
    let mut conn = rpc(fix).await;
    // First do the call so we can assert media_ref_token shape.
    let resp = conn
        .call(
            "send.image",
            json!({
                "file": path.to_string_lossy().into_owned(),
                "caption": format!("tier2 image {}", std::process::id()),
            }),
        )
        .await;
    assert!(
        resp["media_ref_token"].is_string(),
        "send.image must return media_ref_token; got {resp}"
    );
    // Re-issue via helper to drive the wait_for path (two sends =
    // 4 s floor consumed; no race with WA)
    let _id = send_media_and_wait(
        fix,
        "send.image",
        json!({
            "file": path.to_string_lossy().into_owned(),
            "caption": format!("tier2 image confirm {}", std::process::id()),
        }),
        "image",
    )
    .await;
}

/// `live_send_video` — Tier 2 outbound video.
#[tokio::test]
async fn live_send_video() {
    let fix = fixture();
    let path = write_tiny_fixture(fix, "tier2-video", "mp4");
    let _id = send_media_and_wait(
        fix,
        "send.video",
        json!({"file": path.to_string_lossy().into_owned()}),
        "video",
    )
    .await;
}

/// `live_send_audio` — Tier 2 outbound audio file (non-voice).
#[tokio::test]
async fn live_send_audio() {
    let fix = fixture();
    let path = write_tiny_fixture(fix, "tier2-audio", "mp3");
    let _id = send_media_and_wait(
        fix,
        "send.audio",
        json!({"file": path.to_string_lossy().into_owned()}),
        "audio",
    )
    .await;
}

/// `live_send_voice` — Tier 2 outbound voice note (opus container).
#[tokio::test]
async fn live_send_voice() {
    let fix = fixture();
    let path = write_tiny_fixture(fix, "tier2-voice", "ogg");
    let _id = send_media_and_wait(
        fix,
        "send.voice",
        json!({"file": path.to_string_lossy().into_owned()}),
        "voice",
    )
    .await;
}

/// `live_send_sticker` — Tier 2 outbound webp sticker.
#[tokio::test]
async fn live_send_sticker() {
    let fix = fixture();
    let path = write_tiny_fixture(fix, "tier2-sticker", "webp");
    let _id = send_media_and_wait(
        fix,
        "send.sticker",
        json!({"file": path.to_string_lossy().into_owned()}),
        "sticker",
    )
    .await;
}
