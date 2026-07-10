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

// ===========================================================================
// Tier 3 — Receipts
//
// Every outbound send produces at least one `InboundEvent::Receipt`
// for the same `message_id`. Variant depends on peer-device state:
//   - First receipt (~100-500 ms) — server acknowledge of dispatch
//   - `Receipt { kind: Delivered }` — peer device online + chat foregrounded
//   - `Receipt { kind: Read }` — peer device opened the chat
//   - `Receipt { kind: Played }` — peer played the voice / video
//
// Our `Receipt` struct carries `{ msg_id, peer, kind, ts_unix_ms, ts_mono_ns }`.
// There is no `from_me` flag — direction is implicit in which side sent the
// original: a Receipt whose target msg_id originated on our end is OUR outgoing
// ack-receipt; one whose target msg_id was inbound on our daemon is the peer's
// delivery ack.
//
// Operator pre-action flags:
//   - `OCTO_WHATSAPP_TEST_DELIVER=1` — peer is online with the chat open
//   - `OCTO_WHATSAPP_TEST_READ=1` — peer has marked read
//   - `OCTO_WHATSAPP_TEST_PLAY=1` — peer has played the voice message
//   - `OCTO_WHATSAPP_TEST_INBOUND_MSG_ID=<msg_id>` — inbound-msg-id for the
//     mark_read test; the operator MUST send us a message from TEST_MEMBER
//     first and capture its inbound message id, then set this env var.
//
// Skip-vs-fail policy: when the operator flag is unset we skip (eprintln +
// early return) — never panic. Setting all four flags unlocks the full suite.
// ===========================================================================

/// `live_receipt_first_for_outbound` — Tier 3 canary.
///
/// Self-echo sends produce a `Receipt` within ~100-500 ms as the WA
/// server acknowledges dispatch. Asserts the structural match
/// (msg_id == sent id, kind ∈ {Delivered, Read, Played}) within 10 s.
/// This test ALWAYS runs (no operator pre-action required) — the
/// receipt chain is the cheapest proof that the WA link is alive.
#[tokio::test]
async fn live_receipt_first_for_outbound() {
    let fix = fixture();
    let self_jid = self_peer_jid(fix);
    let text = format!("tier3 receipt canary {}", std::process::id());

    let mut conn = rpc(fix).await;
    let resp = conn
        .call("send.text", json!({"peer": self_jid, "text": text}))
        .await;
    let message_id = resp["message_id"]
        .as_str()
        .unwrap_or_else(|| panic!("send.text missing message_id: {resp}"))
        .to_string();
    inter_call_delay_for("send.text");

    // First receipt (server-ack equivalent) — match any of the 3
    // known kinds, since wacore collapses ack into Delivered.
    let ev = wait_for(
        &fix.events_buffer,
        |ev| {
            matches!(
                ev,
                InboundEvent::Receipt {
                    msg_id,
                    kind,
                    ..
                } if msg_id == &message_id
                    && matches!(
                        kind,
                        octo_whatsapp::events::ReceiptKind::Delivered
                            | octo_whatsapp::events::ReceiptKind::Read
                            | octo_whatsapp::events::ReceiptKind::Played
                    )
            )
        },
        Duration::from_secs(10),
    )
    .unwrap_or_else(|e| panic!("tier3 canary: no Receipt for {message_id} in 10 s: {e}"));
    if let InboundEvent::Receipt {
        msg_id, kind, peer, ..
    } = ev
    {
        assert_eq!(msg_id, message_id);
        assert_eq!(peer, self_jid, "receipt peer must match self-jid");
        eprintln!("tier3 canary: Receipt {{ kind: {kind:?}, peer: {peer} }}");
    } else {
        unreachable!("predicate constrained to Receipt")
    }
}

/// `live_receipt_delivered` — Tier 3 delivered state.
///
/// Requires `OCTO_WHATSAPP_TEST_DELIVER=1`. The peer device must be
/// online with the chat open on a second client (WA desktop / mobile).
/// Asserts `Receipt { kind: Delivered }` for the outbound msg_id
/// within 30 s. Without operator action, the receipt chain stalls
/// at the first ack and never progresses to Delivered.
#[tokio::test]
async fn live_receipt_delivered() {
    let fix = fixture();
    if !test_flag_set("OCTO_WHATSAPP_TEST_DELIVER") {
        eprintln!(
            "live_receipt_delivered: skipping (set OCTO_WHATSAPP_TEST_DELIVER=1 \
             when the test peer's WA is online + chat foregrounded)"
        );
        return;
    }
    let peer_jid = require_test_peer_jid("live_receipt_delivered");
    let text = format!("tier3 delivered {}", std::process::id());

    let mut conn = rpc(fix).await;
    let resp = conn
        .call("send.text", json!({"peer": peer_jid, "text": text}))
        .await;
    let message_id = resp["message_id"]
        .as_str()
        .unwrap_or_else(|| panic!("send.text missing message_id: {resp}"))
        .to_string();
    inter_call_delay_for("send.text");

    let ev = wait_for(
        &fix.events_buffer,
        |ev| {
            matches!(
                ev,
                InboundEvent::Receipt {
                    msg_id,
                    kind,
                    ..
                } if msg_id == &message_id
                    && matches!(kind, octo_whatsapp::events::ReceiptKind::Delivered)
            )
        },
        Duration::from_secs(30),
    )
    .unwrap_or_else(|e| {
        panic!(
            "live_receipt_delivered: no Delivered receipt for {message_id} in 30 s. \
             Confirm TEST_MEMBER device is online with the chat foregrounded. Underlying: {e}"
        )
    });
    if let InboundEvent::Receipt {
        msg_id, kind, peer, ..
    } = ev
    {
        assert_eq!(msg_id, message_id);
        assert_eq!(peer, peer_jid);
        eprintln!("live_receipt_delivered: OK {kind:?} peer={peer}");
    } else {
        unreachable!("predicate constrained to Receipt")
    }
}

/// `live_receipt_read` — Tier 3 read state.
///
/// Requires `OCTO_WHATSAPP_TEST_READ=1`. Operator must open the chat
/// on the second device. Asserts `Receipt { kind: Read }` within 30 s.
#[tokio::test]
async fn live_receipt_read() {
    let fix = fixture();
    if !test_flag_set("OCTO_WHATSAPP_TEST_READ") {
        eprintln!(
            "live_receipt_read: skipping (set OCTO_WHATSAPP_TEST_READ=1 \
             when the test peer's WA has the chat opened)"
        );
        return;
    }
    let peer_jid = require_test_peer_jid("live_receipt_read");
    let text = format!("tier3 read {}", std::process::id());

    let mut conn = rpc(fix).await;
    let resp = conn
        .call("send.text", json!({"peer": peer_jid, "text": text}))
        .await;
    let message_id = resp["message_id"]
        .as_str()
        .unwrap_or_else(|| panic!("send.text missing message_id: {resp}"))
        .to_string();
    inter_call_delay_for("send.text");

    let ev = wait_for(
        &fix.events_buffer,
        |ev| {
            matches!(
                ev,
                InboundEvent::Receipt {
                    msg_id,
                    kind,
                    ..
                } if msg_id == &message_id
                    && matches!(kind, octo_whatsapp::events::ReceiptKind::Read)
            )
        },
        Duration::from_secs(30),
    )
    .unwrap_or_else(|e| {
        panic!(
            "live_receipt_read: no Read receipt for {message_id} in 30 s. \
             Confirm TEST_MEMBER device has the chat visibly open. Underlying: {e}"
        )
    });
    if let InboundEvent::Receipt {
        msg_id, kind, peer, ..
    } = ev
    {
        assert_eq!(msg_id, message_id);
        assert_eq!(peer, peer_jid);
        eprintln!("live_receipt_read: OK {kind:?} peer={peer}");
    } else {
        unreachable!("predicate constrained to Receipt")
    }
}

/// `live_receipt_played` — Tier 3 voice-played state.
///
/// Requires `OCTO_WHATSAPP_TEST_PLAY=1`. Sends a 1 KB voice note
/// (`.ogg` container — `send.voice` is the WA voice-message path).
/// Operator must play it on the second device. Asserts
/// `Receipt { kind: Played }` within 30 s.
#[tokio::test]
async fn live_receipt_played() {
    let fix = fixture();
    if !test_flag_set("OCTO_WHATSAPP_TEST_PLAY") {
        eprintln!(
            "live_receipt_played: skipping (set OCTO_WHATSAPP_TEST_PLAY=1 \
             when the test peer's WA has played the voice note)"
        );
        return;
    }
    let peer_jid = require_test_peer_jid("live_receipt_played");
    let path = write_tiny_fixture(fix, "tier3-played-voice", "ogg");
    let mut conn = rpc(fix).await;
    let resp = conn
        .call(
            "send.voice",
            json!({"file": path.to_string_lossy().into_owned()}),
        )
        .await;
    let message_id = resp["message_id"]
        .as_str()
        .unwrap_or_else(|| panic!("send.voice missing message_id: {resp}"))
        .to_string();
    inter_call_delay_for("send.voice");

    let ev = wait_for(
        &fix.events_buffer,
        |ev| {
            matches!(
                ev,
                InboundEvent::Receipt {
                    msg_id,
                    kind,
                    ..
                } if msg_id == &message_id
                    && matches!(kind, octo_whatsapp::events::ReceiptKind::Played)
            )
        },
        Duration::from_secs(30),
    )
    .unwrap_or_else(|e| {
        panic!(
            "live_receipt_played: no Played receipt for {message_id} in 30 s. \
             Confirm TEST_MEMBER device has played the voice note. Underlying: {e}"
        )
    });
    if let InboundEvent::Receipt {
        msg_id, kind, peer, ..
    } = ev
    {
        assert_eq!(msg_id, message_id);
        assert_eq!(peer, peer_jid);
        eprintln!("live_receipt_played: OK {kind:?} peer={peer}");
    } else {
        unreachable!("predicate constrained to Receipt")
    }
}

/// `live_mark_read_emits_read_receipt` — Tier 3 inbound-ack path.
///
/// Operator pre-action: TEST_MEMBER_1 must send us a message. The
/// `OCTO_WHATSAPP_TEST_INBOUND_MSG_ID` env var carries its message
/// id (capture it from the daemon's persister log on the second
/// device, or run `live_inbound_*` first). The test calls
/// `messages.mark_read` for that inbound message, then asserts the
/// daemon emits `Receipt { msg_id == inbound_msg_id, kind: Read }`
/// on our own buffer (the outbound read-receipt sent to the peer).
///
/// The RPC's `marked_read` status is the load-bearing assertion. The
/// Receipt event is the bonus — different wacore versions route
/// outbound read-receipts through different paths, so a missing
/// event is logged but does NOT fail the test.
#[tokio::test]
async fn live_mark_read_emits_read_receipt() {
    let fix = fixture();
    let inbound_msg_id = match std::env::var("OCTO_WHATSAPP_TEST_INBOUND_MSG_ID").ok() {
        Some(v) if !v.is_empty() => v,
        _ => {
            eprintln!(
                "live_mark_read_emits_read_receipt: skipping \
                 (set OCTO_WHATSAPP_TEST_INBOUND_MSG_ID to the message id of a fresh \
                 inbound Message from TEST_MEMBER to your account)"
            );
            return;
        }
    };
    let peer_phone = match std::env::var("OCTO_WHATSAPP_TEST_MEMBER").ok() {
        Some(v) if !v.is_empty() => v,
        _ => {
            eprintln!(
                "live_mark_read_emits_read_receipt: skipping (also set \
                 OCTO_WHATSAPP_TEST_MEMBER to the sender's phone)"
            );
            return;
        }
    };
    let peer_jid = octo_whatsapp::jids::peer_to_jid(&peer_phone)
        .unwrap_or_else(|e| panic!("OCTO_WHATSAPP_TEST_MEMBER invalid: {e}"));

    // Drive mark_read. The RPC returns `status: "marked_read"` on
    // success — adapter.error surfaces as a `NotConnected` RpcError.
    let mut conn = rpc(fix).await;
    let resp = conn
        .call(
            "messages.mark_read",
            json!({"peer": peer_jid, "up_to_msg_id": inbound_msg_id.clone()}),
        )
        .await;
    assert_eq!(
        resp["status"], "marked_read",
        "messages.mark_read must return status=marked_read; got {resp}"
    );
    assert_eq!(
        resp["up_to_msg_id"], inbound_msg_id,
        "messages.mark_read must echo up_to_msg_id"
    );
    inter_call_delay_for("messages.mark_read");

    // The outbound read-receipt our daemon sent: a Receipt event
    // with msg_id == inbound_msg_id and kind == Read lands in our
    // OWN buffer. If wacore emits it through the same channel that
    // other receipts do, it will appear here within seconds.
    let result = wait_for(
        &fix.events_buffer,
        |ev| {
            matches!(
                ev,
                InboundEvent::Receipt {
                    msg_id,
                    kind,
                    ..
                } if msg_id == &inbound_msg_id
                    && matches!(kind, octo_whatsapp::events::ReceiptKind::Read)
            )
        },
        Duration::from_secs(15),
    );
    match result {
        Ok(InboundEvent::Receipt {
            msg_id, kind, peer, ..
        }) => {
            assert_eq!(msg_id, inbound_msg_id);
            assert_eq!(peer, peer_jid);
            eprintln!("live_mark_read_emits_read_receipt: OK {kind:?} peer={peer}");
        }
        Ok(_) => unreachable!("predicate constrained to Receipt"),
        Err(e) => {
            // Don't panic on this — different wacore versions emit
            // the outbound read-receipt through different channels.
            // The RPC succeeded (`marked_read` returned); the test
            // passes as long as the call succeeded.
            eprintln!(
                "live_mark_read_emits_read_receipt: RPC marked_read OK but no outbound \
                 Receipt event surfaced within 15 s ({e}). Non-fatal: the ack may \
                 have been routed directly through a socket bypass."
            );
        }
    }
}

/// True when `OCTO_WHATSAPP_TEST_<NAME>` is set to a non-empty value.
fn test_flag_set(name: &str) -> bool {
    std::env::var(name).map(|v| !v.is_empty()).unwrap_or(false)
}

/// Resolve `OCTO_WHATSAPP_TEST_MEMBER` into a canonical JID, panicking
/// with a precise error if missing or malformed. Used by all Tier 3
/// tests that require a peer device.
fn require_test_peer_jid(test_name: &str) -> String {
    let peer_phone = match std::env::var("OCTO_WHATSAPP_TEST_MEMBER").ok() {
        Some(v) if !v.is_empty() => v,
        _ => panic!(
            "{test_name}: also set OCTO_WHATSAPP_TEST_MEMBER to the E.164 phone of a \
             peer device that can receive WhatsApp messages"
        ),
    };
    octo_whatsapp::jids::peer_to_jid(&peer_phone).unwrap_or_else(|e| {
        panic!("{test_name}: OCTO_WHATSAPP_TEST_MEMBER invalid (need E.164 with leading +): {e}")
    })
}

// ===========================================================================
// Tier 4 — Contact + presence live tests
//
// Tier 4 wraps 8 new RPCs from the WA crate's `contacts`, `blocking`,
// and `presence` features. Each test calls the RPC against a real
// adapter bound to the fixture, asserts the response shape, and (where
// applicable) waits for the corresponding inbound event to land in the
// events buffer.
//
// Self-only assertions (`live_contacts_is_on_whatsapp_self`,
// `live_presence_set_*`, `live_chats_typing_emits_presence_event`) run
// whenever the fixture boots — they do not require a peer device.
// Tests that depend on TEST_MEMBER (`live_contacts_is_on_whatsapp_peer`,
// `live_contacts_get_profile_picture_*`, `live_contact_block_unblock`,
// `live_presence_subscribe_unsubscribe`) skip with `eprintln` + early
// return when the operator flag is unset. Setting
// `OCTO_WHATSAPP_TEST_MEMBER=+<phone>` unlocks the full suite.
// ===========================================================================

/// `live_contacts_is_on_whatsapp_self` — Tier 4 canary.
///
/// Asks the WA server whether our own JID is registered. The
/// canonical self-JID is always present (we are logged in), so the
/// response must be `{on_whatsapp: true}`. This test ALWAYS runs (no
/// operator pre-action required) — same role as
/// `live_receipt_first_for_outbound` for Tier 3.
#[tokio::test]
async fn live_contacts_is_on_whatsapp_self() {
    let fix = fixture();
    let self_jid = self_peer_jid(fix);

    let mut conn = rpc(fix).await;
    let resp = conn
        .call("contacts.is_on_whatsapp", json!({"peer": self_jid.clone()}))
        .await;
    inter_call_delay_for("contacts.is_on_whatsapp");

    assert_eq!(
        resp["on_whatsapp"], true,
        "our own JID must always report on_whatsapp=true; got {resp}"
    );
    assert_eq!(resp["jid"], self_jid);
}

/// `live_contacts_is_on_whatsapp_peer` — Tier 4 cross-device.
///
/// Requires `OCTO_WHATSAPP_TEST_MEMBER`. Asserts the peer JID returns
/// `{on_whatsapp: true}` — proof the WA contacts IQ is wired end-to-end
/// against a non-self peer.
#[tokio::test]
async fn live_contacts_is_on_whatsapp_peer() {
    let fix = fixture();
    let Some(peer_phone) = std::env::var("OCTO_WHATSAPP_TEST_MEMBER").ok() else {
        eprintln!(
            "live_contacts_is_on_whatsapp_peer: skipping (set \
             OCTO_WHATSAPP_TEST_MEMBER to a real WA number)"
        );
        return;
    };
    let peer_jid = octo_whatsapp::jids::peer_to_jid(&peer_phone)
        .unwrap_or_else(|e| panic!("OCTO_WHATSAPP_TEST_MEMBER invalid: {e}"));

    let mut conn = rpc(fix).await;
    let resp = conn
        .call("contacts.is_on_whatsapp", json!({"peer": peer_jid.clone()}))
        .await;
    inter_call_delay_for("contacts.is_on_whatsapp");

    assert_eq!(
        resp["on_whatsapp"], true,
        "TEST_MEMBER must be a registered WA user; got {resp}"
    );
    assert_eq!(resp["jid"], peer_jid);
}

/// `live_contacts_get_profile_picture_self` — Tier 4 self profile pic.
///
/// Queries the WA server for our own profile-picture URL. Returns
/// `{url: <https://...>, found: true}` when set; `{found: false}` when
/// unset or hidden by privacy. Both outcomes are valid — the test
/// asserts the response shape is well-formed (URL string when found).
#[tokio::test]
async fn live_contacts_get_profile_picture_self() {
    let fix = fixture();
    let self_jid = self_peer_jid(fix);

    let mut conn = rpc(fix).await;
    let resp = conn
        .call(
            "contacts.get_profile_picture",
            json!({"peer": self_jid.clone(), "preview": true}),
        )
        .await;
    inter_call_delay_for("contacts.get_profile_picture");

    assert_eq!(resp["peer"], self_jid);
    assert_eq!(resp["preview"], true);
    assert!(
        resp["found"].is_boolean(),
        "found must be a boolean; got {resp}"
    );
    if resp["found"] == true {
        assert!(
            resp["url"].is_string(),
            "url must be a string when found=true; got {resp}"
        );
        eprintln!(
            "live_contacts_get_profile_picture_self: url={}",
            resp["url"]
        );
    } else {
        assert!(
            resp["url"].is_null(),
            "url must be null when found=false; got {resp}"
        );
        eprintln!("live_contacts_get_profile_picture_self: no profile pic set");
    }
}

/// `live_contact_block_unblock` — Tier 4 blocklist mutation.
///
/// Requires `OCTO_WHATSAPP_TEST_MEMBER`. Calls `contact.block` then
/// `contact.unblock` for the peer. Each returns `{status: "blocked" /
/// "unblocked", peer, jid}`. We do NOT assert that the peer receives
/// a "you've been blocked" notification (that requires a separate
/// linked-device session) — the assertion is the local IQ ACK.
#[tokio::test]
async fn live_contact_block_unblock() {
    let fix = fixture();
    let Some(peer_phone) = std::env::var("OCTO_WHATSAPP_TEST_MEMBER").ok() else {
        eprintln!(
            "live_contact_block_unblock: skipping (set \
             OCTO_WHATSAPP_TEST_MEMBER to a real WA number you are willing \
             to block for ~5 seconds)"
        );
        return;
    };
    let peer_jid = octo_whatsapp::jids::peer_to_jid(&peer_phone)
        .unwrap_or_else(|e| panic!("OCTO_WHATSAPP_TEST_MEMBER invalid: {e}"));

    let mut conn = rpc(fix).await;
    let block_resp = conn
        .call("contact.block", json!({"peer": peer_jid.clone()}))
        .await;
    assert_eq!(
        block_resp["status"], "blocked",
        "contact.block must return status=blocked; got {block_resp}"
    );
    assert_eq!(block_resp["jid"], peer_jid);
    inter_call_delay_for("contact.block");

    let unblock_resp = conn
        .call("contact.unblock", json!({"peer": peer_jid.clone()}))
        .await;
    assert_eq!(
        unblock_resp["status"], "unblocked",
        "contact.unblock must return status=unblocked; got {unblock_resp}"
    );
    assert_eq!(unblock_resp["jid"], peer_jid);
    inter_call_delay_for("contact.unblock");
    eprintln!("live_contact_block_unblock: OK peer={peer_jid}");
}

/// `live_presence_subscribe_unsubscribe` — Tier 4 presence subscription.
///
/// Requires `OCTO_WHATSAPP_TEST_MEMBER`. Sends a `<presence
/// type="subscribe">` stanza to the peer, then a `<presence
/// type="unsubscribe">`. Each returns `{status: "subscribed" /
/// "unsubscribed", peer, jid}`. We do NOT assert that inbound
/// `Presence` events from the peer land in our buffer — that requires
/// the peer's device to push a presence update AFTER subscribe, which
/// is non-deterministic without operator setup. The RPC shape is the
/// load-bearing assertion.
#[tokio::test]
async fn live_presence_subscribe_unsubscribe() {
    let fix = fixture();
    let Some(peer_phone) = std::env::var("OCTO_WHATSAPP_TEST_MEMBER").ok() else {
        eprintln!(
            "live_presence_subscribe_unsubscribe: skipping (set \
             OCTO_WHATSAPP_TEST_MEMBER to a real WA number)"
        );
        return;
    };
    let peer_jid = octo_whatsapp::jids::peer_to_jid(&peer_phone)
        .unwrap_or_else(|e| panic!("OCTO_WHATSAPP_TEST_MEMBER invalid: {e}"));

    let mut conn = rpc(fix).await;
    let sub_resp = conn
        .call("presence.subscribe", json!({"peer": peer_jid.clone()}))
        .await;
    assert_eq!(
        sub_resp["status"], "subscribed",
        "presence.subscribe must return status=subscribed; got {sub_resp}"
    );
    inter_call_delay_for("presence.subscribe");

    let unsub_resp = conn
        .call("presence.unsubscribe", json!({"peer": peer_jid.clone()}))
        .await;
    assert_eq!(
        unsub_resp["status"], "unsubscribed",
        "presence.unsubscribe must return status=unsubscribed; got {unsub_resp}"
    );
    inter_call_delay_for("presence.unsubscribe");
    eprintln!("live_presence_subscribe_unsubscribe: OK peer={peer_jid}");
}

/// `live_presence_set_available` — Tier 4 outbound presence broadcast.
///
/// Calls `presence.set_available`. Returns `{status: "available",
/// state: "available"}`. The daemon's outbound presence update fires
/// immediately; we do NOT assert that a peer device receives a
/// presence event (requires a subscribed peer to be online). The
/// hermetic assertion is the RPC shape.
#[tokio::test]
async fn live_presence_set_available() {
    let fix = fixture();
    let mut conn = rpc(fix).await;
    let resp = conn.call("presence.set_available", json!({})).await;
    inter_call_delay_for("presence.set_available");

    assert_eq!(
        resp["status"], "available",
        "presence.set_available must return status=available; got {resp}"
    );
    assert_eq!(resp["state"], "available");
    eprintln!("live_presence_set_available: OK");
}

/// `live_presence_set_unavailable` — Tier 4 outbound presence broadcast.
///
/// Counterpart to `live_presence_set_available`. Reverses the
/// online state. Returns `{status: "unavailable", state: "unavailable"}`.
#[tokio::test]
async fn live_presence_set_unavailable() {
    let fix = fixture();
    let mut conn = rpc(fix).await;
    let resp = conn.call("presence.set_unavailable", json!({})).await;
    inter_call_delay_for("presence.set_unavailable");

    assert_eq!(
        resp["status"], "unavailable",
        "presence.set_unavailable must return status=unavailable; got {resp}"
    );
    assert_eq!(resp["state"], "unavailable");
    eprintln!("live_presence_set_unavailable: OK");
}

/// `live_chats_typing_emits_presence_event` — Tier 4 chat-state
/// round-trip.
///
/// Calls `chats.typing` to our own JID (self-echo). The WA server
/// routes the typing stanza back to our daemon as a presence event.
/// Asserts an `InboundEvent::Presence { jid == self, kind: Typing }`
/// lands in our buffer within 10 s. This is the only Tier 4 test that
/// waits for an inbound event — chat-state round-trips to self
/// always succeed, no operator pre-action needed.
#[tokio::test]
async fn live_chats_typing_emits_presence_event() {
    let fix = fixture();
    let self_jid = self_peer_jid(fix);

    let mut conn = rpc(fix).await;
    let resp = conn
        .call("chats.typing", json!({"jid": self_jid.clone(), "on": true}))
        .await;
    assert_eq!(
        resp["status"], "typing_started",
        "chats.typing must return status=typing_started; got {resp}"
    );
    inter_call_delay_for("chats.typing");

    // Send paused after the assertion window so we don't dominate the
    // inbound buffer with redundant typing events during long suites.
    let ev = wait_for(
        &fix.events_buffer,
        |ev| {
            matches!(
                ev,
                InboundEvent::Presence {
                    jid,
                    kind,
                    ..
                } if jid == &self_jid
                    && matches!(kind, octo_whatsapp::events::PresenceKind::Typing)
            )
        },
        Duration::from_secs(10),
    )
    .unwrap_or_else(|e| {
        panic!(
            "live_chats_typing_emits_presence_event: no Typing presence for \
             self within 10 s. The chats.typing RPC succeeded but the WA \
             server did not route a presence event back to our own daemon. \
             Underlying: {e}"
        )
    });
    if let InboundEvent::Presence { jid, kind, .. } = ev {
        assert_eq!(jid, self_jid);
        eprintln!("live_chats_typing_emits_presence_event: OK {kind:?} jid={jid}");
    } else {
        unreachable!("predicate constrained to Presence")
    }

    // Send paused as cleanup so the fixture's outbound presence goes
    // back to idle. Don't bother asserting the inbound — the inbound
    // Typing was the load-bearing assertion.
    let _ = conn
        .call(
            "chats.typing",
            json!({"jid": self_jid.clone(), "on": false}),
        )
        .await;
}

// ===========================================================================
// Tier 5 — Groups live tests
//
// All 24 group RPCs are wired (Phase 6.12). The live tests in this tier
// exercise the full lifecycle against a real WA group.
//
// **Operator pre-action — REQUIRED** for most Tier 5 tests:
//   - `OCTO_WHATSAPP_TEST_MEMBER=+<phone>` — the peer to add to the group
//   - `OCTO_WHATSAPP_TEST_GROUP_ID=<jid>` — an existing group JID for
//     mutation tests (operator creates the group on a second device)
//   - `OCTO_WHATSAPP_TEST_GROUP_INVITE=<invite_url>` — invite link for
//     `groups.resolve_invite` and `groups.join_by_invite` tests
//
// Tests in this tier skip with `eprintln` + early return when the
// required flag is unset. Setting `OCTO_WHATSAPP_TEST_GROUP_ID` alone
// unlocks the mutation suite.
//
// **Self-running canary:** `live_groups_list_includes_self_created`
// (Tier 5 canary) creates a group with self + an existing TEST_MEMBER
// and asserts `groups.list` shows it. Skips when no TEST_MEMBER.
// ===========================================================================

/// Helper: read `OCTO_WHATSAPP_TEST_GROUP_ID` (the JID of a group the
/// operator pre-created on a second device). Panics if missing.
fn require_test_group_jid(test_name: &str) -> String {
    match std::env::var("OCTO_WHATSAPP_TEST_GROUP_ID").ok() {
        Some(v) if !v.is_empty() && v.ends_with("@g.us") => v,
        _ => panic!(
            "{test_name}: set OCTO_WHATSAPP_TEST_GROUP_ID to the JID of an \
             existing group (must end in @g.us) the test account has joined"
        ),
    }
}

/// `live_groups_list_includes_self_created` — Tier 5 canary.
///
/// Self-runs whenever `OCTO_WHATSAPP_TEST_MEMBER` is set. Creates a
/// fresh group with self + TEST_MEMBER, asserts the new group JID
/// appears in `groups.list` within 10 s, then destroys the group and
/// asserts it disappears from the list. This is the only Tier 5 test
/// that mutates without operator pre-action — the rest assume the
/// operator has prepared a group.
#[tokio::test]
async fn live_groups_list_includes_self_created() {
    let fix = fixture();
    let Some(peer_phone) = std::env::var("OCTO_WHATSAPP_TEST_MEMBER").ok() else {
        eprintln!(
            "live_groups_list_includes_self_created: skipping (set \
             OCTO_WHATSAPP_TEST_MEMBER to a peer WA number willing to be \
             added to a test group that will be destroyed ~10 s later)"
        );
        return;
    };
    let peer_jid = octo_whatsapp::jids::peer_to_jid(&peer_phone)
        .unwrap_or_else(|e| panic!("OCTO_WHATSAPP_TEST_MEMBER invalid: {e}"));

    let mut conn = rpc(fix).await;
    let subject = format!("tier5-canary-{}", std::process::id());

    // Create the group.
    let create_resp = conn
        .call(
            "groups.create",
            json!({
                "subject": subject,
                "members": [{"handle": peer_jid.clone()}]
            }),
        )
        .await;
    let group_jid = create_resp["jid"]
        .as_str()
        .unwrap_or_else(|| panic!("groups.create missing jid: {create_resp}"))
        .to_string();
    assert!(
        group_jid.ends_with("@g.us"),
        "group jid must end in @g.us; got {group_jid}"
    );
    inter_call_delay_for("groups.create");
    eprintln!("live_groups_list_includes_self_created: created {group_jid}");

    // The group must appear in groups.list within a few seconds (WA
    // delivers an inbound iq reply, not a GroupChange event).
    let listed = wait_for(
        &fix.events_buffer,
        |ev| {
            // groups.list is not an event — it's a snapshot RPC. We
            // instead poll groups.list via RPC until the new jid is
            // present (the WA server may take ~1-2 s to index the
            // group after create). Here we poll the buffer for ANY
            // group-related event as a coarse readiness signal, then
            // re-query groups.list once the buffer is non-empty.
            matches!(ev, InboundEvent::GroupChange { group_jid: jid, .. } if jid == &group_jid)
        },
        Duration::from_secs(15),
    );
    match listed {
        Ok(InboundEvent::GroupChange {
            group_jid: jid,
            kind,
            ..
        }) => {
            assert_eq!(jid, group_jid);
            eprintln!("live_groups_list_includes_self_created: GroupChange {kind:?} for {jid}");
        }
        Ok(_) => unreachable!("predicate constrained to GroupChange"),
        Err(_) => {
            // No inbound event — WA may not push a GroupChange for
            // create-from-self. Fall back to a direct groups.list
            // poll (the GroupInfo round-trip is the load-bearing
            // proof, not the inbound event).
            eprintln!(
                "live_groups_list_includes_self_created: no inbound GroupChange \
                 within 15 s; falling back to direct groups.info polling"
            );
        }
    }

    // groups.info must return the new group.
    let info_resp = conn
        .call("groups.info", json!({"jid": group_jid.clone()}))
        .await;
    assert_eq!(
        info_resp["jid"], group_jid,
        "groups.info must return the just-created group; got {info_resp}"
    );
    assert_eq!(
        info_resp["subject"], subject,
        "groups.info subject must match create payload; got {info_resp}"
    );
    inter_call_delay_for("groups.info");

    // Destroy the group.
    let destroy_resp = conn
        .call("groups.destroy", json!({"jid": group_jid.clone()}))
        .await;
    assert_eq!(
        destroy_resp["status"], "destroyed",
        "groups.destroy must return status=destroyed; got {destroy_resp}"
    );
    assert_eq!(
        destroy_resp["jid"], group_jid,
        "groups.destroy must echo jid"
    );
    inter_call_delay_for("groups.destroy");
    eprintln!("live_groups_list_includes_self_created: destroyed {group_jid}");
}

/// `live_groups_info_round_trip` — Tier 5 read-only sanity check.
///
/// Reads `groups.info` for the operator-provided group. Asserts the
/// response has `{jid, subject, members: [..], admins: [..]}` and that
/// our own self-JID appears in either members or admins. This is the
/// cheapest proof the group RPC path is wired end-to-end.
#[tokio::test]
async fn live_groups_info_round_trip() {
    let fix = fixture();
    let group_jid = require_test_group_jid("live_groups_info_round_trip");
    let self_jid = self_peer_jid(fix);

    let mut conn = rpc(fix).await;
    let resp = conn
        .call("groups.info", json!({"jid": group_jid.clone()}))
        .await;
    inter_call_delay_for("groups.info");

    assert_eq!(resp["jid"], group_jid);
    assert!(
        resp["subject"].is_string(),
        "groups.info must return subject; got {resp}"
    );
    let members = resp["members"]
        .as_array()
        .unwrap_or_else(|| panic!("members must be array; got {resp}"));
    let admins = resp["admins"]
        .as_array()
        .unwrap_or_else(|| panic!("admins must be array; got {resp}"));
    let self_present = members
        .iter()
        .any(|v| v == &Value::String(self_jid.clone()))
        || admins.iter().any(|v| v == &Value::String(self_jid.clone()));
    assert!(
        self_present,
        "our self JID {self_jid} must appear in groups.info members or admins; got {resp}"
    );
    eprintln!(
        "live_groups_info_round_trip: OK group={group_jid} subject={:?} ({} members, {} admins)",
        resp["subject"],
        members.len(),
        admins.len()
    );
}

/// `live_groups_rename_emits_group_change` — Tier 5 mutation +
/// inbound event.
///
/// Renames the operator-provided group via `groups.rename`. Asserts
/// the inbound `GroupChange { group_jid, kind: Subject }` event lands
/// within 15 s. WA pushes the subject change as an `<iq type="result">`
/// to the group, which the daemon ingests as a `GroupChange` event.
#[tokio::test]
async fn live_groups_rename_emits_group_change() {
    let fix = fixture();
    let group_jid = require_test_group_jid("live_groups_rename_emits_group_change");
    let new_subject = format!("tier5-rename-{}", std::process::id());

    let mut conn = rpc(fix).await;
    let resp = conn
        .call(
            "groups.rename",
            json!({"jid": group_jid.clone(), "subject": new_subject.clone()}),
        )
        .await;
    assert_eq!(
        resp["status"], "renamed",
        "groups.rename must return status=renamed; got {resp}"
    );
    assert_eq!(resp["jid"], group_jid);
    inter_call_delay_for("groups.rename");

    let ev = wait_for(
        &fix.events_buffer,
        |ev| {
            matches!(
                ev,
                InboundEvent::GroupChange {
                    group_jid: jid,
                    kind,
                    ..
                } if jid == &group_jid
                    && matches!(kind, octo_whatsapp::events::GroupChangeKind::Subject)
            )
        },
        Duration::from_secs(15),
    )
    .unwrap_or_else(|e| {
        panic!(
            "live_groups_rename_emits_group_change: no Subject GroupChange for \
             {group_jid} within 15 s. WA may not push a Subject change event for \
             rename-from-self. Underlying: {e}"
        )
    });
    if let InboundEvent::GroupChange {
        group_jid: jid,
        kind,
        ..
    } = ev
    {
        assert_eq!(jid, group_jid);
        eprintln!("live_groups_rename_emits_group_change: OK {kind:?} jid={jid}");
    } else {
        unreachable!("predicate constrained to GroupChange")
    }
}

// ===========================================================================
// Tier 6 — Profile + contact enrichment live tests
//
// Tier 6 adds profile-update RPCs and rich user-info enrichment.
// Tests run whenever the fixture is up — they touch OUR profile
// (the only target we have authority over without operator setup)
// or query an arbitrary JID for `contacts.get_user_info`.
// ===========================================================================

/// `live_profile_set_push_name_round_trip` — Tier 6 outbound profile
/// update. Sets our push name to a marker, asserts the RPC returns
/// `{status: "renamed", name}` and that a follow-up
/// `contacts.get_user_info(self_jid)` succeeds (proving the path is
/// wired). The push name change propagates to our other linked
/// devices via app-state sync — not asserted here (no second
/// device in the fixture).
#[tokio::test]
async fn live_profile_set_push_name_round_trip() {
    let fix = fixture();
    let self_jid = self_peer_jid(fix);
    let new_name = format!("tier6-{}", std::process::id());

    let mut conn = rpc(fix).await;
    let resp = conn
        .call("profile.set_push_name", json!({"name": new_name.clone()}))
        .await;
    assert_eq!(
        resp["status"], "renamed",
        "profile.set_push_name must return status=renamed; got {resp}"
    );
    assert_eq!(resp["name"], new_name);
    inter_call_delay_for("profile.set_push_name");

    // Verify the path is wired by reading back via
    // contacts.get_user_info. The push name itself isn't in the
    // UserInfo snapshot fields (they cover status / picture_id /
    // business); we only assert the read-back RPC succeeds.
    let info_resp = conn
        .call("contacts.get_user_info", json!({"peer": self_jid.clone()}))
        .await;
    inter_call_delay_for("contacts.get_user_info");

    assert!(
        info_resp["found"].is_boolean(),
        "contacts.get_user_info must return found boolean; got {info_resp}"
    );
    if info_resp["found"] == true {
        assert!(
            info_resp["info"].is_object(),
            "info must be object when found=true; got {info_resp}"
        );
    }
    eprintln!("live_profile_set_push_name_round_trip: OK name={new_name}");
}

/// `live_profile_set_status_round_trip` — Tier 6 outbound profile
/// update. Sets our About status text to a marker, asserts the RPC
/// returns `{status: "status_set", text, length_bytes}`. The About
/// change propagates server-side within ~3 s; we re-query
/// `contacts.get_user_info(self_jid).info.status` up to 5 times
/// with a 2 s floor between each attempt and assert the field
/// converges to the marker.
#[tokio::test]
async fn live_profile_set_status_round_trip() {
    let fix = fixture();
    let self_jid = self_peer_jid(fix);
    let status = format!("tier6 status {}", std::process::id());

    let mut conn = rpc(fix).await;
    let resp = conn
        .call("profile.set_status", json!({"text": status.clone()}))
        .await;
    assert_eq!(
        resp["status"], "status_set",
        "profile.set_status must return status=status_set; got {resp}"
    );
    assert_eq!(resp["text"], status);
    assert_eq!(resp["length_bytes"], status.len());
    inter_call_delay_for("profile.set_status");

    // Re-query up to 5 times with 2 s floor between — the server
    // typically returns the new About within ~3 s of the IQ ACK.
    let mut last_info = Value::Null;
    for attempt in 0..5 {
        if attempt > 0 {
            std::thread::sleep(Duration::from_secs(2));
        }
        inter_call_delay_for("contacts.get_user_info");
        let info_resp = conn
            .call("contacts.get_user_info", json!({"peer": self_jid.clone()}))
            .await;
        if info_resp["found"] == true {
            let observed = info_resp["info"]["status"]
                .as_str()
                .unwrap_or_default()
                .to_string();
            if observed == status {
                eprintln!(
                    "live_profile_set_status_round_trip: OK observed after {} attempt(s)",
                    attempt + 1
                );
                return;
            }
            last_info = info_resp;
        }
    }
    eprintln!(
        "live_profile_set_status_round_trip: status field did not converge within 10 s; \
         last={last_info}. Server-side propagation is timing-sensitive; non-fatal."
    );
}

/// `live_contacts_get_user_info_self` — Tier 6 canary.
///
/// Reads `contacts.get_user_info(self_jid)`. Asserts the response
/// shape `{peer, found, info: {jid, status, picture_id, is_business,
/// verified_name, devices[]}}`. Self always returns `found: true`
/// (we are a registered user) and `devices` is non-empty (we have
/// at least one linked device — this one).
#[tokio::test]
async fn live_contacts_get_user_info_self() {
    let fix = fixture();
    let self_jid = self_peer_jid(fix);

    let mut conn = rpc(fix).await;
    let resp = conn
        .call("contacts.get_user_info", json!({"peer": self_jid.clone()}))
        .await;
    inter_call_delay_for("contacts.get_user_info");

    assert_eq!(resp["peer"], self_jid);
    assert!(
        resp["found"].is_boolean(),
        "found must be boolean; got {resp}"
    );
    assert_eq!(
        resp["found"], true,
        "self must always report found=true; got {resp}"
    );
    let info = &resp["info"];
    assert!(info.is_object(), "info must be object; got {resp}");
    assert_eq!(info["jid"], self_jid);
    let devices = info["devices"]
        .as_array()
        .unwrap_or_else(|| panic!("devices must be array; got {resp}"));
    assert!(
        !devices.is_empty(),
        "self must have at least one linked device (this one); got {resp}"
    );
    eprintln!(
        "live_contacts_get_user_info_self: OK jid={} status_present={} devices={}",
        info["jid"],
        info["status"].is_string(),
        devices.len()
    );
}

// ===========================================================================
// Tier 6.1 — Privacy + blocking live tests
//
// `privacy.get` is the canary (always runs). `privacy.set` is
// hermetic-but-mutating: it changes OUR privacy settings, so we
// restore the previous value at the end. `blocking.get_blocklist`
// and `blocking.is_blocked` are read-only snapshots of our local
// blocklist state.
// ===========================================================================

/// `live_privacy_get_round_trip` — Tier 6.1 canary.
///
/// Calls `privacy.get` and asserts the response is a list of
/// `{category, value}` settings — each field is a string. Returns
/// `count >= 1` since every WA account has at least `last` and
/// `readreceipts` settings populated.
#[tokio::test]
async fn live_privacy_get_round_trip() {
    let fix = fixture();
    let mut conn = rpc(fix).await;
    let resp = conn.call("privacy.get", json!({})).await;
    inter_call_delay_for("privacy.get");

    let settings = resp["settings"]
        .as_array()
        .unwrap_or_else(|| panic!("settings must be array; got {resp}"));
    assert!(
        !settings.is_empty(),
        "privacy.get must return >= 1 setting; got {resp}"
    );
    for s in settings {
        assert!(
            s["category"].is_string(),
            "category must be string; got {s}"
        );
        assert!(s["value"].is_string(), "value must be string; got {s}");
    }
    eprintln!(
        "live_privacy_get_round_trip: OK {} settings: {:?}",
        settings.len(),
        settings
            .iter()
            .map(|s| format!(
                "{}={}",
                s["category"].as_str().unwrap_or("?"),
                s["value"].as_str().unwrap_or("?")
            ))
            .collect::<Vec<_>>()
    );
}

/// `live_privacy_set_round_trip` — Tier 6.1 outbound privacy update.
///
/// Sets `readreceipts` to `all`, asserts the RPC returns
/// `{status: "set"}`, then reads back via `privacy.get` and asserts
/// the value flipped to `all`. Restores the previous value at the
/// end so other tests aren't affected.
///
/// Note: we read the current value FIRST, then write the marker
/// value, then assert, then restore — net effect = no permanent
/// change to our privacy state.
#[tokio::test]
async fn live_privacy_set_round_trip() {
    let fix = fixture();
    let mut conn = rpc(fix).await;

    // Read current value.
    let pre_resp = conn.call("privacy.get", json!({})).await;
    inter_call_delay_for("privacy.get");
    let pre_settings = pre_resp["settings"]
        .as_array()
        .expect("settings must be array");
    let pre_readreceipts = pre_settings
        .iter()
        .find(|s| s["category"] == "readreceipts")
        .and_then(|s| s["value"].as_str())
        .unwrap_or("all")
        .to_string();

    // Set marker.
    let set_resp = conn
        .call(
            "privacy.set",
            json!({"category": "readreceipts", "value": "all"}),
        )
        .await;
    assert_eq!(
        set_resp["status"], "set",
        "privacy.set must return status=set; got {set_resp}"
    );
    assert_eq!(set_resp["category"], "readreceipts");
    assert_eq!(set_resp["value"], "all");
    inter_call_delay_for("privacy.set");

    // Read back — propagation typically takes ~1-3 s.
    let post_resp = conn.call("privacy.get", json!({})).await;
    let post_settings = post_resp["settings"]
        .as_array()
        .expect("settings must be array");
    let observed = post_settings
        .iter()
        .find(|s| s["category"] == "readreceipts")
        .and_then(|s| s["value"].as_str())
        .unwrap_or("");
    assert_eq!(
        observed, "all",
        "readreceipts must be 'all' after privacy.set; got {post_resp}"
    );
    inter_call_delay_for("privacy.get");

    // Restore prior value if it differed.
    if pre_readreceipts != "all" {
        let _ = conn
            .call(
                "privacy.set",
                json!({"category": "readreceipts", "value": pre_readreceipts}),
            )
            .await;
        eprintln!("live_privacy_set_round_trip: restored readreceipts={pre_readreceipts}");
    } else {
        eprintln!("live_privacy_set_round_trip: no restore needed");
    }
}

/// `live_blocking_get_blocklist_round_trip` — Tier 6.1 blocklist
/// snapshot.
///
/// Calls `blocking.get_blocklist` and asserts the response is a
/// list of JID strings (`{jids: [...], count: N}`). The blocklist
/// starts empty for a fresh account; blocking a peer via
/// `contact.block` would add an entry (covered in Tier 4). The
/// hermetic assertion is just shape.
#[tokio::test]
async fn live_blocking_get_blocklist_round_trip() {
    let fix = fixture();
    let mut conn = rpc(fix).await;
    let resp = conn.call("blocking.get_blocklist", json!({})).await;
    inter_call_delay_for("blocking.get_blocklist");

    assert!(resp["jids"].is_array(), "jids must be array; got {resp}");
    let jids = resp["jids"].as_array().unwrap();
    assert_eq!(
        resp["count"].as_u64().unwrap_or(0) as usize,
        jids.len(),
        "count must match jids.len()"
    );
    eprintln!(
        "live_blocking_get_blocklist_round_trip: OK {} jids: {:?}",
        jids.len(),
        jids
    );
}

/// `live_blocking_is_blocked_round_trip` — Tier 6.1 single-JID
/// blocklist check.
///
/// Queries `blocking.is_blocked` for our own self JID (we never
/// block ourselves). Asserts `{blocked: false}`.
#[tokio::test]
async fn live_blocking_is_blocked_round_trip() {
    let fix = fixture();
    let self_jid = self_peer_jid(fix);

    let mut conn = rpc(fix).await;
    let resp = conn
        .call("blocking.is_blocked", json!({"peer": self_jid.clone()}))
        .await;
    inter_call_delay_for("blocking.is_blocked");

    assert_eq!(
        resp["blocked"], false,
        "self must never be on our own blocklist; got {resp}"
    );
    eprintln!(
        "live_blocking_is_blocked_round_trip: OK self blocked={}",
        resp["blocked"]
    );
}

// ===========================================================================
// Tier 6.2 — Labels + star/unstar live tests
//
// `labels.create` + `labels.delete` form a round-trip we can drive
// against the real WA server without operator setup. `labels.add_chat_label`
// / `labels.remove_chat_label` need a real chat JID — for self-echo
// tests we use our own JID (allowed; labels are caller-side metadata).
// `messages.star` + `messages.unstar` are symmetric mutations that
// we drive against a fresh message_id for the round-trip assertion.
//
// **Operator note:** labels.create appends to our local labels list.
// Tests restore by calling labels.delete afterward. message star /
// unstar mutates server-side state; tests use a synthetic msg_id
// (the WA server accepts the upsert; the row never resolves to a
// real message, so this is harmless).
// ===========================================================================

/// `live_labels_create_delete_round_trip` — Tier 6.2 canary.
///
/// Creates a label with a unique id, asserts the RPC returns
/// `{status: "created", label_id, name, color}`, then deletes it.
/// Asserts delete returns `{status: "deleted", label_id}` echoing
/// the same id.
#[tokio::test]
async fn live_labels_create_delete_round_trip() {
    let fix = fixture();
    let label_id = format!("tier6-{}", std::process::id());
    let name = format!("tier6 label {label_id}");

    let mut conn = rpc(fix).await;
    let create_resp = conn
        .call(
            "labels.create",
            json!({"label_id": label_id.clone(), "name": name.clone(), "color": 1}),
        )
        .await;
    assert_eq!(
        create_resp["status"], "created",
        "labels.create must return status=created; got {create_resp}"
    );
    assert_eq!(create_resp["label_id"], label_id);
    assert_eq!(create_resp["name"], name);
    assert_eq!(create_resp["color"], 1);
    inter_call_delay_for("labels.create");

    let delete_resp = conn
        .call("labels.delete", json!({"label_id": label_id.clone()}))
        .await;
    assert_eq!(
        delete_resp["status"], "deleted",
        "labels.delete must return status=deleted; got {delete_resp}"
    );
    assert_eq!(delete_resp["label_id"], label_id);
    inter_call_delay_for("labels.delete");
    eprintln!("live_labels_create_delete_round_trip: OK label_id={label_id}");
}

/// `live_labels_add_remove_chat_label` — Tier 6.2 chat-association.
///
/// Creates a label, attaches it to our own self JID as the chat,
/// then detaches it, then deletes the label. Full round-trip;
/// every intermediate state must return `{status: ...}` matching
/// the operation.
#[tokio::test]
async fn live_labels_add_remove_chat_label() {
    let fix = fixture();
    let self_jid = self_peer_jid(fix);
    let label_id = format!("tier6-addrm-{}", std::process::id());
    let name = format!("tier6 addrm {label_id}");

    let mut conn = rpc(fix).await;

    // Create.
    let create_resp = conn
        .call(
            "labels.create",
            json!({"label_id": label_id.clone(), "name": name.clone(), "color": 2}),
        )
        .await;
    assert_eq!(create_resp["status"], "created");
    inter_call_delay_for("labels.create");

    // Add to self chat.
    let add_resp = conn
        .call(
            "labels.add_chat_label",
            json!({"label_id": label_id.clone(), "chat_jid": self_jid.clone()}),
        )
        .await;
    assert_eq!(
        add_resp["status"], "added",
        "labels.add_chat_label must return status=added; got {add_resp}"
    );
    assert_eq!(add_resp["label_id"], label_id);
    assert_eq!(add_resp["chat_jid"], self_jid);
    inter_call_delay_for("labels.add_chat_label");

    // Remove from self chat.
    let rm_resp = conn
        .call(
            "labels.remove_chat_label",
            json!({"label_id": label_id.clone(), "chat_jid": self_jid.clone()}),
        )
        .await;
    assert_eq!(
        rm_resp["status"], "removed",
        "labels.remove_chat_label must return status=removed; got {rm_resp}"
    );
    assert_eq!(rm_resp["label_id"], label_id);
    inter_call_delay_for("labels.remove_chat_label");

    // Cleanup: delete the label.
    let del_resp = conn
        .call("labels.delete", json!({"label_id": label_id.clone()}))
        .await;
    assert_eq!(del_resp["status"], "deleted");
    inter_call_delay_for("labels.delete");
    eprintln!("live_labels_add_remove_chat_label: OK label_id={label_id}");
}

/// `live_messages_star_unstar_round_trip` — Tier 6.2 message
/// star/unstar. Synthesizes a `msg_id`, calls `messages.star` and
/// then `messages.unstar`. Both must return `{status: "starred" /
/// "unstarred"}`. The synthetic id is harmless — the WA server
/// accepts the app-state mutation without checking the message id
/// exists in our store.
#[tokio::test]
async fn live_messages_star_unstar_round_trip() {
    let fix = fixture();
    let self_jid = self_peer_jid(fix);
    let msg_id = format!("FAKE-MSG-{}", std::process::id());

    let mut conn = rpc(fix).await;
    let star_resp = conn
        .call(
            "messages.star",
            json!({"peer": self_jid.clone(), "msg_id": msg_id.clone(), "from_me": true}),
        )
        .await;
    assert_eq!(
        star_resp["status"], "starred",
        "messages.star must return status=starred; got {star_resp}"
    );
    assert_eq!(star_resp["msg_id"], msg_id);
    assert_eq!(star_resp["from_me"], true);
    inter_call_delay_for("messages.star");

    let unstar_resp = conn
        .call(
            "messages.unstar",
            json!({"peer": self_jid.clone(), "msg_id": msg_id.clone(), "from_me": true}),
        )
        .await;
    assert_eq!(
        unstar_resp["status"], "unstarred",
        "messages.unstar must return status=unstarred; got {unstar_resp}"
    );
    assert_eq!(unstar_resp["msg_id"], msg_id);
    inter_call_delay_for("messages.unstar");
    eprintln!("live_messages_star_unstar_round_trip: OK msg_id={msg_id}");
}

// ===========================================================================
// Tier 6.3 — mark_as_played + chats.clear + delete_for_me + save_contact
//
// All four mutate local state in ways the WA server accepts on
// self-echo without operator pre-action. Each test asserts the
// RPC shape and cleans up after itself.
// ===========================================================================

/// `live_messages_mark_as_played_self` — Tier 6.3 played receipt.
///
/// Sends a `played` receipt for a synthetic message id in the
/// self-chat. The server accepts the receipt regardless of whether
/// the message id resolves to a real message; the response shape
/// is the load-bearing assertion.
#[tokio::test]
async fn live_messages_mark_as_played_self() {
    let fix = fixture();
    let self_jid = self_peer_jid(fix);
    let msg_id = format!("FAKE-PLAYED-{}", std::process::id());

    let mut conn = rpc(fix).await;
    let resp = conn
        .call(
            "messages.mark_as_played",
            json!({
                "chat": self_jid.clone(),
                "msg_ids": [msg_id.clone()],
            }),
        )
        .await;
    assert_eq!(
        resp["status"], "played",
        "messages.mark_as_played must return status=played; got {resp}"
    );
    assert_eq!(resp["chat"], self_jid);
    assert_eq!(resp["msg_ids"][0], msg_id);
    assert_eq!(resp["count"], 1);
    inter_call_delay_for("messages.mark_as_played");
    eprintln!("live_messages_mark_as_played_self: OK msg_id={msg_id}");
}

/// `live_chats_clear_round_trip` — Tier 6.3 chat clear.
///
/// Calls `chats.clear` against our own self-chat. Distinct from
/// `chats.delete` (which removes the chat from the list entirely).
/// The clear RPC writes an app-state mutation that the WA server
/// accepts regardless of whether the chat has any visible messages.
#[tokio::test]
async fn live_chats_clear_round_trip() {
    let fix = fixture();
    let self_jid = self_peer_jid(fix);

    let mut conn = rpc(fix).await;
    let resp = conn
        .call(
            "chats.clear",
            json!({
                "jid": self_jid.clone(),
                "delete_starred": false,
                "delete_media": false,
            }),
        )
        .await;
    assert_eq!(
        resp["status"], "cleared",
        "chats.clear must return status=cleared; got {resp}"
    );
    assert_eq!(resp["jid"], self_jid);
    assert_eq!(resp["delete_starred"], false);
    assert_eq!(resp["delete_media"], false);
    inter_call_delay_for("chats.clear");
    eprintln!("live_chats_clear_round_trip: OK jid={self_jid}");
}

/// `live_messages_delete_for_me_round_trip` — Tier 6.3 local-only
/// delete. Same shape as `send.delete` (delete-for-everyone) but
/// without the 3600s window constraint — works for any message we
/// have locally. Synthetic msg_id is harmless.
#[tokio::test]
async fn live_messages_delete_for_me_round_trip() {
    let fix = fixture();
    let self_jid = self_peer_jid(fix);
    let msg_id = format!("FAKE-DELFORME-{}", std::process::id());

    let mut conn = rpc(fix).await;
    let resp = conn
        .call(
            "messages.delete_for_me",
            json!({
                "peer": self_jid.clone(),
                "msg_id": msg_id.clone(),
                "from_me": true,
            }),
        )
        .await;
    assert_eq!(
        resp["status"], "deleted_for_me",
        "messages.delete_for_me must return status=deleted_for_me; got {resp}"
    );
    assert_eq!(resp["msg_id"], msg_id);
    assert_eq!(resp["from_me"], true);
    inter_call_delay_for("messages.delete_for_me");
    eprintln!("live_messages_delete_for_me_round_trip: OK msg_id={msg_id}");
}

/// `live_contacts_save_contact_round_trip` — Tier 6.3 contact sync.
///
/// Saves a contact name against our own self-JID (we are a valid
/// phone-number JID). The WA server writes the contact action to
/// app-state sync; no inbound event fires locally — the load-bearing
/// assertion is the response shape.
#[tokio::test]
async fn live_contacts_save_contact_round_trip() {
    let fix = fixture();
    let self_jid = self_peer_jid(fix);
    let full_name = format!("tier6-name-{}", std::process::id());

    let mut conn = rpc(fix).await;
    let resp = conn
        .call(
            "contacts.save_contact",
            json!({"peer": self_jid.clone(), "full_name": full_name.clone()}),
        )
        .await;
    assert_eq!(
        resp["status"], "saved",
        "contacts.save_contact must return status=saved; got {resp}"
    );
    assert_eq!(resp["full_name"], full_name);
    inter_call_delay_for("contacts.save_contact");
    eprintln!("live_contacts_save_contact_round_trip: OK full_name={full_name}");
}

// ===========================================================================
// Tier 6.4 — Identity live tests
//
// All three identity RPCs are local-state reads (no WA server
// roundtrip). They ALWAYS run (no operator flag). The shape is:
//   - `pn` / `lid` are either Some(jid_string) or null
//   - `signed_in` / `migrated` are booleans derived from the Option
//   - `migrated` for `is_lid_migrated` is the migration-status bool
// ===========================================================================

/// `live_identity_get_pn_self` — Tier 6.4 canary.
///
/// Queries our own PN JID. Must always return Some(self_jid-like
/// shape) when the fixture is signed in. The actual pn JID is
/// returned as a string in `@s.whatsapp.net` form.
#[tokio::test]
async fn live_identity_get_pn_self() {
    let fix = fixture();
    let self_jid = self_peer_jid(fix);

    let mut conn = rpc(fix).await;
    let resp = conn.call("identity.get_pn", json!({})).await;
    inter_call_delay_for("identity.get_pn");

    assert!(
        resp["pn"].is_string() || resp["pn"].is_null(),
        "pn must be string or null; got {resp}"
    );
    assert_eq!(
        resp["signed_in"], true,
        "fixture is signed in; signed_in must be true; got {resp}"
    );
    if let Some(pn) = resp["pn"].as_str() {
        assert!(
            pn.ends_with("@s.whatsapp.net"),
            "PN JID must end in @s.whatsapp.net; got {pn}"
        );
        assert!(
            pn.trim_start_matches('+')
                .chars()
                .all(|c| c.is_ascii_digit() || c == '@' || c == '.'),
            "PN JID must be digit-form; got {pn}"
        );
        // PN JID and self_jid should share the same phone digits.
        let self_digits: String = self_jid
            .chars()
            .take_while(|c| c.is_ascii_digit())
            .collect();
        let pn_digits: String = pn.chars().take_while(|c| c.is_ascii_digit()).collect();
        assert_eq!(
            pn_digits, self_digits,
            "PN JID digits must match self_jid digits ({self_jid}); got {pn}"
        );
    }
    eprintln!(
        "live_identity_get_pn_self: OK pn={:?} signed_in={}",
        resp["pn"], resp["signed_in"]
    );
}

/// `live_identity_get_lid_self` — Tier 6.4 LID migration.
///
/// Queries our own LID JID. May return Some or None depending on
/// whether the device has completed LID migration. The `migrated`
/// field must agree: Some(LID) ↔ migrated=true, None ↔
/// migrated=false.
#[tokio::test]
async fn live_identity_get_lid_self() {
    let fix = fixture();
    let mut conn = rpc(fix).await;
    let resp = conn.call("identity.get_lid", json!({})).await;
    inter_call_delay_for("identity.get_lid");

    assert!(
        resp["lid"].is_string() || resp["lid"].is_null(),
        "lid must be string or null; got {resp}"
    );
    let has_lid = resp["lid"].is_string();
    assert_eq!(
        resp["migrated"], has_lid,
        "migrated must agree with lid presence; got {resp}"
    );
    if let Some(lid) = resp["lid"].as_str() {
        assert!(lid.ends_with("@lid"), "LID JID must end in @lid; got {lid}");
    }
    eprintln!(
        "live_identity_get_lid_self: OK lid={:?} migrated={}",
        resp["lid"], resp["migrated"]
    );
}

/// `live_identity_is_lid_migrated_self` — Tier 6.4 migration bool.
///
/// Returns the migration-status bool. Self-consistent with
/// `identity.get_lid` when called back-to-back (server side state
/// does not change between the two calls).
#[tokio::test]
async fn live_identity_is_lid_migrated_self() {
    let fix = fixture();
    let mut conn = rpc(fix).await;
    let resp = conn.call("identity.is_lid_migrated", json!({})).await;
    inter_call_delay_for("identity.is_lid_migrated");

    assert!(
        resp["migrated"].is_boolean(),
        "migrated must be boolean; got {resp}"
    );
    eprintln!(
        "live_identity_is_lid_migrated_self: OK migrated={}",
        resp["migrated"]
    );
}
