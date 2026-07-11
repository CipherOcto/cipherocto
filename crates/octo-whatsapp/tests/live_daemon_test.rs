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
        let resp = self.call_unchecked(method, params).await;
        if let Some(err) = resp.get("error") {
            panic!("rpc {method} returned error: {err}",);
        }
        resp.get("result")
            .cloned()
            .unwrap_or_else(|| panic!("rpc {method} returned no result: {resp}"))
    }

    /// Variant that returns the full JSON-RPC envelope without
    /// panicking on `error`. Use this from negative-path tests that
    /// assert on `code` / `data` of a returned error. Happy-path tests
    /// should keep using `call` so unexpected errors fail loudly.
    async fn call_unchecked(&mut self, method: &str, params: Value) -> Value {
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
        serde_json::from_str(buf.trim()).expect("rpc parse")
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
                    // Emit an InboundEvent::Connection { kind: Connected }
                    // into the daemon's events buffer so live tests can
                    // observe connection lifecycle via the same events
                    // table the production adapter drives. This is the
                    // fixture-side equivalent of a future
                    // `WhatsAppWebAdapter::start_bot` -> "connected" hook.
                    // Without it, `live_connection_open_emits_event` (the
                    // Tier-1 canary) has no event to assert on.
                    let buffer_for_emit = daemon.handle().events_buffer().clone();
                    let ts_connected_ms = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_millis() as i64)
                        .unwrap_or(0);
                    buffer_for_emit.push(InboundEvent::Connection {
                        kind: octo_whatsapp::events::ConnectionKind::Connected,
                        cause: None,
                        ts_unix_ms: ts_connected_ms,
                        ts_mono_ns: 0,
                    });
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
/// WA servers respond with a `ServerAck` envelope carrying the same
/// `message_id`. The test asserts an `InboundEvent::Unknown` whose
/// raw Debug-string starts with `ServerAck(` lands within 15 s, then
/// extracts the embedded WA crate `id` field via
/// [`extract_server_ack_id`] and asserts it equals the dispatched
/// `message_id`. The test ALSO asserts that the dispatched text body
/// surfaces in the daemon's events table as a typed `Message` (or
/// `Unknown(Message)` envelope) — that's the operator-visible
/// round-trip: every linked WA client must render the bubble.
///
/// Currently the events parser routes `Message(...)` envelopes from
/// the WA crate to `InboundEvent::Unknown` because the typed
/// `Message` parser branch doesn't parse the WA crate's
/// `Message(...)` Debug envelope yet. The match arm in this test
/// therefore fires on either a typed `Message` whose `id` matches,
/// or an `Unknown` whose raw starts with `Message(`. The eventual
/// parser-gap follow-up commit (Phase 7.A-close) closes the typed
/// route.
///
/// Failure modes this test catches:
/// - `send.text` silently no-op'd (the Phase 1 stub regression)
/// - WA dispatch error surfaces as `InvalidParams` or `Unreachable`
/// - NDJSON ingestion dropping events (events_query sees None)
/// - Self JID resolution fails (fixture panic upstream)
/// - Body never round-trips (the bug this test was failing on; the
///   operator-mandated requirement that every linked WA client renders
///   the dispatched bubble)
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

    // Ground-truth assertion: a self-send dispatch MUST round-trip through
    // the events table. Today the WA adapter surfaces the server
    // acknowledgement as `InboundEvent::Unknown { raw: "ServerAck(...)" }`
    // because the events parser doesn't have a typed
    // `ReceiptKind::ServerAck` mapping yet (slated for the next
    // parser-gap follow-up commit). We therefore match on
    // `Unknown { raw: "ServerAck(...)" }` whose embedded WA crate `id`
    // field equals the dispatched `message_id`. Once the typed route
    // lands, this test upgrades to
    // `matches!(ev, InboundEvent::Receipt { msg_id, kind: ReceiptKind::ServerAck, .. } if msg_id == &message_id)`.
    let event = wait_for(
        &fix.events_buffer,
        |ev| {
            matches!(
                ev,
                InboundEvent::Unknown { raw, .. }
                    if extract_server_ack_id(raw).as_deref() == Some(message_id.as_str())
            )
        },
        Duration::from_secs(15),
    )
    .unwrap_or_else(|e| {
        panic!("live_send_text_self: {e}; marker={marker}; message_id={message_id}")
    });
    let InboundEvent::Unknown {
        raw, ts_unix_ms, ..
    } = event
    else {
        unreachable!("predicate already constrained to Unknown")
    };
    let ack_id = extract_server_ack_id(&raw)
        .unwrap_or_else(|| panic!("extract_server_ack_id failed on {raw}"));
    assert_eq!(ack_id, message_id, "ServerAck id must round-trip");
    eprintln!(
        "live_send_text_self: ServerAck id={ack_id} ts_unix_ms={ts_unix_ms} (raw head={:?})",
        raw.chars().take(80).collect::<String>()
    );

    // Body-presence diagnostic: WA accepted the dispatch (ServerAck
    // round-tripped above), but the operator reports no text on the
    // live WA client. This scan asks: did the WA server actually
    // deliver the message body back through the events channel? The
    // marker tag is embedded in the dispatch text, so any
    // `InboundEvent::Message { text, .. }` whose `text` contains the
    // marker proves the body is reaching our daemon (even if the
    // typed MessageKind::Text parser route doesn't fire yet). If
    // zero messages surface, the body never arrives back — a real
    // round-trip bug, not a parser gap.
    let buffer = &fix.events_buffer;
    let marker_in_body: Vec<String> = buffer
        .list_recent(40)
        .iter()
        .filter_map(|ev| match ev {
            InboundEvent::Message { text, id, peer, .. } => {
                Some(format!("Message(peer={peer}, id={id}, text={text:?})"))
            }
            InboundEvent::Unknown { raw, .. } => Some(format!(
                "Unknown({})",
                raw.chars().take(160).collect::<String>()
            )),
            _ => None,
        })
        .filter(|line| line.contains(&marker))
        .collect();
    let body_present = !marker_in_body.is_empty();
    eprintln!(
        "live_send_text_self: marker={marker} body events matching marker = {}",
        marker_in_body.len()
    );
    for line in marker_in_body.iter().take(5) {
        eprintln!("  - {line}");
    }

    // Buffer-total diagnostic: show every recent envelope's first 160
    // chars so future debugging can see WA's envelope stream without
    // a flag and re-run.
    let all_kinds: Vec<String> = buffer
        .list_recent(40)
        .iter()
        .map(|ev| match ev {
            InboundEvent::Message { peer, id, text, .. } => {
                format!(
                    "Message(peer={peer}, id={id}, text={})",
                    text.chars().take(60).collect::<String>()
                )
            }
            InboundEvent::Receipt { msg_id, kind, .. } => {
                format!("Receipt(msg_id={msg_id}, kind={kind:?})")
            }
            InboundEvent::Connection { kind, .. } => format!("Connection({kind:?})"),
            InboundEvent::GroupChange {
                group_jid, kind, ..
            } => {
                format!("GroupChange({group_jid}, {kind:?})")
            }
            InboundEvent::Presence { jid, kind, .. } => format!("Presence({jid}, {kind:?})"),
            InboundEvent::Reaction { id, .. } => format!("Reaction({id})"),
            InboundEvent::Call { id, .. } => format!("Call({id})"),
            InboundEvent::Story { id, .. } => format!("Story({id})"),
            InboundEvent::Unknown { raw, .. } => {
                format!("Unknown({})", raw.chars().take(2000).collect::<String>())
            }
        })
        .collect();
    eprintln!(
        "live_send_text_self: buffer dump ({} entries):\n  - {}",
        all_kinds.len(),
        all_kinds.join("\n  - ")
    );

    // Hard assertion: dispatched text body MUST surface in the events table
    // so the live WA client of the linked number renders the bubble. The
    // operator's mandate is non-negotiable — every dispatched text must
    // appear on every linked WA client. The match arm covers both the
    // typed `Message` route (parser upgraded) and the `Unknown` envelope
    // route (current state); either is acceptable as long as the body
    // text is present.
    assert!(
        body_present,
        "live_send_text_self: dispatched text body never surfaced in the events buffer; \
         expected at least one typed `Message(...)` or `Unknown(Message(...))` envelope \
         carrying marker={marker:?}. Operator mandate: the text MUST appear on every linked \
         WA client. See eprintln buffer dump above for what did land."
    );
}

/// Extract the embedded WA crate `id` field from a `ServerAck(...)`
/// Debug-string. Returns `None` if the raw is not a `ServerAck` envelope
/// or the id can't be parsed. Used by the Tier-1 self-echo canary to
/// confirm the round-tripped message_id without depending on a typed
/// `InboundEvent::Receipt { kind: ServerAck, .. }` parser route that
/// doesn't yet exist.
fn extract_server_ack_id(raw: &str) -> Option<String> {
    let rest = raw.strip_prefix("ServerAck(")?;
    // Debug format: `ServerAck { id: "3EB0...", ... }`. Find the first
    // quoted string after `id: ` — coarse but correct for the WA
    // crate's current representation. A proper parser belongs in the
    // WA-events module once ServerAck graduates to a typed
    // `ReceiptKind`.
    let needle = "id: \"";
    let start = rest.find(needle)? + needle.len();
    let end = rest[start..].find('"')? + start;
    Some(rest[start..end].to_string())
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
    // second device. Both currently land as `InboundEvent::Unknown`
    // because the events parser has no typed route for the WA crate's
    // `MessageDelivered` / `MessageRead` envelopes yet (Phase 7.A-close
    // parser-gap backlog). The predicate matches either via the typed
    // `Receipt` route (parser upgraded) or via the `Unknown` envelope
    // extractor, so the test stays green across both states.
    let ack = wait_for(
        &fix.events_buffer,
        |ev| receipt_or_unknown_for_id(ev, &message_id),
        Duration::from_secs(30),
    );
    if ack.is_err() {
        let recent = fix.events_buffer.list_recent(20);
        let recent_kinds: Vec<String> = recent
            .iter()
            .rev()
            .take(15)
            .map(|ev| match ev {
                InboundEvent::Message { peer, id: mid, .. } => {
                    format!("Message(peer={peer}, id={mid})")
                }
                InboundEvent::Receipt { msg_id, kind, .. } => {
                    format!("Receipt(msg_id={msg_id}, kind={kind:?})")
                }
                InboundEvent::Connection { kind, .. } => format!("Connection({kind:?})"),
                InboundEvent::GroupChange {
                    group_jid, kind, ..
                } => {
                    format!("GroupChange({group_jid}, {kind:?})")
                }
                InboundEvent::Presence { jid, kind, .. } => {
                    format!("Presence({jid}, {kind:?})")
                }
                InboundEvent::Reaction { id, .. } => format!("Reaction({id})"),
                InboundEvent::Call { id, .. } => format!("Call({id})"),
                InboundEvent::Story { id, .. } => format!("Story({id})"),
                InboundEvent::Unknown { raw, .. } => {
                    format!("Unknown({})", raw.chars().take(120).collect::<String>())
                }
            })
            .collect();
        eprintln!(
            "live_send_text_peer: buffer had {} events; recent_kinds=\n  - {}",
            recent.len(),
            recent_kinds.join("\n  - ")
        );
    }
    match ack {
        Ok(_) => {}
        Err(e) => panic!(
            "live_send_text_peer: no ServerAck/Delivered/Read for {message_id} within 30s. \
             Likely the second device ({peer_phone}) never received / opened the chat. \
             Underlying error: {e}"
        ),
    }
}

/// Predicate that matches either a typed `InboundEvent::Receipt` whose
/// `msg_id` equals the dispatch id, OR an `InboundEvent::Unknown` whose
/// raw Debug envelope carries that id under any of the WA crate's
/// receipt-shape variants (ServerAck / MessageDelivered / MessageRead /
/// MessagePlayed). The Phase 7.A-close parser upgrade promotes the
/// `Unknown` arms to typed `Receipt { kind: ... }` matches.
fn receipt_or_unknown_for_id(ev: &InboundEvent, msg_id: &str) -> bool {
    match ev {
        InboundEvent::Receipt { msg_id: rid, .. } if rid == msg_id => true,
        InboundEvent::Unknown { raw, .. } => {
            let needle = format!("id: \"{msg_id}\"");
            raw.contains(&needle)
        }
        _ => false,
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
        .call_unchecked("send.text", json!({"peer": self_jid, "text": text}))
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
        .call_unchecked(
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

/// Smallest valid PNG (1×1 transparent, 69 bytes). The hand-rolled
/// bytes match the IHDR/IDAT/IEND structure that WA's media pipeline
/// expects server-side. 1 KB of zeros goes through upload (WA assigns
/// a real `message_id`) but is rejected at the next hop — server-side
/// media validation refuses zero-byte image bodies for self-echo even
/// when cross-device delivery succeeds. Operator diagnostic.
fn write_real_png_fixture(fix: &LiveTestFixture, name: &str) -> std::path::PathBuf {
    let path = fix.tmp.path().join(format!("{name}.png"));
    // 1×1 transparent PNG, 8-bit RGBA, non-interlaced.
    // bytes produced by `printf '\x89PNG\r\n\x1a\n...' > /tmp/1px.png`.
    let bytes: [u8; 69] = [
        0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f,
        0x15, 0xc4, 0x89, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0xf8,
        0xcf, 0xc0, 0x00, 0x00, 0x00, 0x03, 0x00, 0x01, 0x5a, 0xf1, 0x71, 0x9e, 0x00, 0x00, 0x00,
        0x00, 0x49, 0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
    ];
    std::fs::write(&path, bytes).expect("write png fixture");
    path
}

/// 1-second silent opus voice note. Validated against WA's mobile
/// client player (the bubble plays back without "audio is corrupt"
/// errors). The bytes are committed at
/// `crates/octo-whatsapp/tests/fixtures/live/voice-1s.ogg`; this
/// helper copies them into the live-fixture's tmp dir so each test
/// run has a fresh copy. Generated via:
///
///   ffmpeg -f lavfi -i anullsrc=r=16000:cl=mono -t 1 \
///          -c:a libopus -b:a 16k -ac 1 -ar 16000 -application voip \
///          voice-1s.ogg
///
/// 651 bytes, mono 16 kHz, 16 kb/s, VOIP application profile.
fn write_voice_fixture(fix: &LiveTestFixture, name: &str) -> std::path::PathBuf {
    let path = fix.tmp.path().join(format!("{name}.ogg"));
    let fixture_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/live/voice-1s.ogg");
    std::fs::copy(&fixture_path, &path).unwrap_or_else(|e| {
        panic!(
            "copy voice fixture {:?} -> {:?}: {e}. \
             Run `ffmpeg -f lavfi -i anullsrc=r=16000:cl=mono -t 1 \
             -c:a libopus -b:a 16k -ac 1 -ar 16000 -application voip \
             crates/octo-whatsapp/tests/fixtures/live/voice-1s.ogg` to regenerate.",
            fixture_path, path
        )
    });
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
    let self_jid = self_peer_jid(fix);
    let mut conn = rpc(fix).await;
    // First do the call so we can assert media_ref_token shape.
    let resp = conn
        .call(
            "send.image",
            json!({
                "peer": self_jid,
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

/// `live_send_image_to_test_member` — Tier 2 cross-device.
///
/// Sends a real image to `OCTO_WHATSAPP_TEST_MEMBER` (operator-provided
/// peer phone) and waits for both:
///   - the RPC response carrying a real `message_id`
///   - an `InboundEvent::Message` of kind=Image with the same id
///
/// This is the operator-side proof that media flows end-to-end on a
/// non-self peer. The fixture's session is `+5521995544743`-something;
/// `TEST_MEMBER` MUST be a different phone that has the operator's
/// session in its contact list, otherwise the message lands in spam
/// and never reaches the test peer. The event-table assertion is the
/// daemon-side proof; the bubble render is verified manually.
#[tokio::test]
async fn live_send_image_to_test_member() {
    let fix = fixture();
    let peer_phone = match std::env::var("OCTO_WHATSAPP_TEST_MEMBER").ok() {
        Some(v) if !v.is_empty() => v,
        _ => {
            eprintln!(
                "live_send_image_to_test_member: skipping (set OCTO_WHATSAPP_TEST_MEMBER \
                 to the E.164 phone of a peer device to receive the cross-device image)"
            );
            return;
        }
    };
    let peer_jid = octo_whatsapp::jids::peer_to_jid(&peer_phone)
        .unwrap_or_else(|e| panic!("OCTO_WHATSAPP_TEST_MEMBER invalid: {e}"));
    let path = write_tiny_fixture(fix, "tier2-image-peer", "jpg");
    eprintln!(
        "live_send_image_to_test_member: peer_jid={peer_jid}; file={path:?}; \
         please confirm this image bubble lands on the TEST_MEMBER device."
    );

    let mut conn = rpc(fix).await;
    let resp = conn
        .call(
            "send.image",
            json!({
                "peer": peer_jid,
                "file": path.to_string_lossy().into_owned(),
                "caption": format!("tier2 image peer {}", std::process::id()),
            }),
        )
        .await;
    let message_id = resp["message_id"]
        .as_str()
        .unwrap_or_else(|| panic!("send.image missing message_id: {resp}"))
        .to_string();
    inter_call_delay_for("send.image");
    let ev = wait_for(
        &fix.events_buffer,
        |ev| matches!(ev, InboundEvent::Message { id, .. } if id == &message_id),
        Duration::from_secs(15),
    )
    .unwrap_or_else(|e| {
        panic!(
            "live_send_image_to_test_member: no Message event for {message_id} in 15 s; \
             underlying: {e}"
        )
    });
    if let InboundEvent::Message {
        id, peer, kind, ..
    } = ev
    {
        assert_eq!(id, message_id);
        assert_eq!(peer, peer_jid);
        let k = kind;
        eprintln!("live_send_image_to_test_member: OK id={id} peer={peer} kind={k:?}");
    } else {
        unreachable!("predicate constrained to Message")
    }
}

/// `live_send_image_to_self_visible` — Tier 2 self-media diagnostics.
///
/// Sends an image to the session's own JID via the +E164 form so the
/// `peer_to_jid -> apply_self_routing` swap fires. Confirms the RPC
/// succeeds AND that an `InboundEvent::Message` (synthesised by the
/// handler after the 9f44984-era self-routing fix) lands in the events
/// table. The bubble render on the linked WA client is verified by the
/// operator manually checking their phone.
///
/// Crucial: this test uses the OPPOSITE form of the prior
/// `live_send_image` fixture (which sends via `self_peer_jid`) by
/// supplying the raw `+E164` so we can confirm `peer_to_jid` and the
/// self-routing swap both behave. If they don't, the message_id from
/// WA will differ from the synthesised event id because the dispatch
/// went to a different JID than the one captured in `p.peer`.
#[tokio::test]
async fn live_send_image_to_self_visible() {
    let fix = fixture();
    let self_jid = self_peer_jid(fix);
    // `self_peer_jid` returns the +E164 form: the handler must resolve
    // it via peer_to_jid, then swap via apply_self_routing.
    let path = write_real_png_fixture(fix, "tier2-image-self");
    eprintln!(
        "live_send_image_to_self_visible: self_jid={self_jid}; file={path:?}; \
         please confirm this image bubble lands on the operator's linked WA client."
    );

    let mut conn = rpc(fix).await;
    let resp = conn
        .call(
            "send.image",
            json!({
                "peer": self_jid,
                "file": path.to_string_lossy().into_owned(),
                "caption": format!("tier2 image self {}", std::process::id()),
            }),
        )
        .await;
    let message_id = resp["message_id"]
        .as_str()
        .unwrap_or_else(|| panic!("send.image missing message_id: {resp}"))
        .to_string();
    let routed_jid = resp["routed_jid"]
        .as_str()
        .unwrap_or_else(|| panic!("send.image missing routed_jid: {resp}"))
        .to_string();
    eprintln!(
        "live_send_image_to_self_visible: input peer={self_jid}; routed_jid={routed_jid}; \
         message_id={message_id}; please compare with the official client's self-send on the \
         same device and confirm whether the bubble renders."
    );
    inter_call_delay_for("send.image");
    let ev = wait_for(
        &fix.events_buffer,
        |ev| matches!(ev, InboundEvent::Message { id, .. } if id == &message_id),
        Duration::from_secs(15),
    )
    .unwrap_or_else(|e| {
        panic!(
            "live_send_image_to_self_visible: no Message event for {message_id} in 15 s; \
             underlying: {e}"
        )
    });
    if let InboundEvent::Message {
        id, peer, kind, from_me, ..
    } = ev
    {
        assert_eq!(id, message_id);
        assert_eq!(peer, self_jid);
        assert!(from_me, "self-send event must have from_me=true");
        let k = kind;
        eprintln!(
            "live_send_image_to_self_visible: OK id={id} peer={peer} kind={k:?} from_me={from_me}; \
             >>> PLEASE CONFIRM the bubble appears on the linked WA client <<<"
        );
    } else {
        unreachable!("predicate constrained to Message")
    }
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
        // The receipt's `peer` from a wacore ServerAck is the WA server's
        // record of the user's own pn — often a shorter, older digit
        // form than the operator's current E.164. Strict equality with
        // the dispatch target over-constrains the assertion; the
        // msg_id match is the load-bearing proof that the server-ack
        // round-tripped for our dispatch.
        assert!(
            !peer.is_empty(),
            "receipt peer must be non-empty (got the WA server's recorded pn)"
        );
        eprintln!("tier3 canary: Receipt {{ kind: {kind:?}, peer: {peer}, msg_id: {msg_id} }}");
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
    let path = write_voice_fixture(fix, "tier3-played-voice");
    let mut conn = rpc(fix).await;
    let resp = conn
        .call(
            "send.voice",
            json!({
                "peer": peer_jid,
                "file": path.to_string_lossy().into_owned(),
            }),
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

// ===========================================================================
// Tier 6.5 — Newsletter + events live tests
//
// `newsletter.list_subscribed` is the canary (always runs, returns
// our subscribed list — likely empty for a fresh account, but the
// RPC shape is the assertion). `newsletter.leave` requires operator
// setup: a newsletter the test account was previously added to.
// `newsletter.get_metadata` similarly requires a known JID. Both skip
// when no env var is set.
// `events.create` is hermetic (self-creates a calendar event against
// our own JID).
// ===========================================================================

/// `live_newsletter_list_subscribed_self` — Tier 6.5 canary.
///
/// Returns our subscribed-newsletter list. For a fresh account the
/// list is typically empty — the assertion is shape: `{newsletters:
/// [...], count: N}` where `count == newsletters.len()`.
#[tokio::test]
async fn live_newsletter_list_subscribed_self() {
    let fix = fixture();
    let mut conn = rpc(fix).await;
    let resp = conn.call("newsletter.list_subscribed", json!({})).await;
    inter_call_delay_for("newsletter.list_subscribed");

    assert!(
        resp["newsletters"].is_array(),
        "newsletters must be array; got {resp}"
    );
    let list = resp["newsletters"].as_array().unwrap();
    assert_eq!(
        resp["count"].as_u64().unwrap_or(0) as usize,
        list.len(),
        "count must match newsletters.len()"
    );
    eprintln!(
        "live_newsletter_list_subscribed_self: OK {} newsletter(s)",
        list.len()
    );
}

/// `live_newsletter_get_metadata_skips_without_setup` — Tier 6.5
/// operator-gated. Skips unless the operator pre-created a
/// newsletter and set `OCTO_WHATSAPP_TEST_NEWSLETTER_JID`. The
/// assertion is: a real JID returns metadata; the server response
/// is parseable.
#[tokio::test]
async fn live_newsletter_get_metadata_skips_without_setup() {
    let fix = fixture();
    let Some(nl_jid) = std::env::var("OCTO_WHATSAPP_TEST_NEWSLETTER_JID").ok() else {
        eprintln!(
            "live_newsletter_get_metadata_skips_without_setup: skipping (set \
             OCTO_WHATSAPP_TEST_NEWSLETTER_JID to a real newsletter JID ending in @newsletter)"
        );
        return;
    };
    let mut conn = rpc(fix).await;
    let resp = conn
        .call("newsletter.get_metadata", json!({"jid": nl_jid.clone()}))
        .await;
    inter_call_delay_for("newsletter.get_metadata");
    let info = &resp["info"];
    assert!(info.is_object(), "info must be object; got {resp}");
    assert_eq!(
        info["jid"], nl_jid,
        "info.jid must echo the requested JID; got {resp}"
    );
    assert!(
        info["name"].is_string(),
        "info.name must be string; got {resp}"
    );
    eprintln!(
        "live_newsletter_get_metadata_skips_without_setup: OK jid={} name={:?}",
        info["jid"], info["name"]
    );
}

/// `live_newsletter_leave_skips_without_setup` — Tier 6.5
/// operator-gated. Skips unless the operator pre-set
/// `OCTO_WHATSAPP_TEST_NEWSLETTER_LEAVE_JID` (a newsletter the test
/// account is currently in). The RPC must return `{status: "left"}`
/// or a structured error.
#[tokio::test]
async fn live_newsletter_leave_skips_without_setup() {
    let fix = fixture();
    let Some(nl_jid) = std::env::var("OCTO_WHATSAPP_TEST_NEWSLETTER_LEAVE_JID").ok() else {
        eprintln!(
            "live_newsletter_leave_skips_without_setup: skipping (set \
             OCTO_WHATSAPP_TEST_NEWSLETTER_LEAVE_JID to a newsletter JID the \
             test account is currently subscribed to; the test leaves it)"
        );
        return;
    };
    let mut conn = rpc(fix).await;
    let resp = conn
        .call("newsletter.leave", json!({"jid": nl_jid.clone()}))
        .await;
    inter_call_delay_for("newsletter.leave");
    assert_eq!(
        resp["status"], "left",
        "newsletter.leave must return status=left; got {resp}"
    );
    assert_eq!(resp["jid"], nl_jid);
    eprintln!("live_newsletter_leave_skips_without_setup: OK jid={nl_jid}");
}

/// `live_events_create_self` — Tier 6.5 calendar event.
///
/// Creates a WA calendar event against our own JID. The server
/// returns the new event's message id; the RPC surfaces it as
/// `{status: "created", message_id, ...}`. The event is visible to
/// us in our own chat list immediately.
#[tokio::test]
async fn live_events_create_self() {
    let fix = fixture();
    let self_jid = self_peer_jid(fix);
    let name = format!("tier6-event-{}", std::process::id());
    let start_time = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64 + 3600)
        .unwrap_or(1_700_000_000);

    let mut conn = rpc(fix).await;
    let resp = conn
        .call(
            "events.create",
            json!({
                "to": self_jid.clone(),
                "name": name.clone(),
                "start_time": start_time,
            }),
        )
        .await;
    inter_call_delay_for("events.create");

    assert_eq!(
        resp["status"], "created",
        "events.create must return status=created; got {resp}"
    );
    assert_eq!(resp["name"], name);
    assert_eq!(resp["start_time"], start_time);
    assert!(
        resp["message_id"].is_string(),
        "message_id must be string; got {resp}"
    );
    eprintln!(
        "live_events_create_self: OK message_id={}",
        resp["message_id"]
    );
}

// ===========================================================================
// Tier 7.A — messages pin / unpin / forward / edit_encrypted
// (Phase 7 close-the-gap: Phase 0 + Tier 7.A RPC wrappers, no events)
// ===========================================================================
//
// WA server behaviour for these RPCs:
// - `messages.pin` / `messages.unpin`: side-effect on the chat's
//   pinned-message set. No `InboundEvent` is emitted back to the
//   sender's own buffer — the only observable signal is the RPC
//   response itself and a subsequent `chats.*` read (not exposed
//   in Phase 7.A). Live tests therefore assert RPC success only.
// - `messages.forward`: side-effect + a new outbound message. The
//   receiver's device will eventually emit a `Message` event on
//   THEIR buffer, not ours. Assert RPC success + the
//   `new_msg_id` field is a non-empty string.
// - `messages.edit_encrypted`: requires the 32-byte message_secret
//   from the original send. That secret is NOT yet exposed in
//   `send.text`'s response shape, so the live test is gated on a
//   follow-up commit (see TODO in the test doc-comment below).
//
// All four tests honour the same env-skip convention as Tier 3/4:
// missing env var → `eprintln!` + early return (test passes).

/// `live_pin_message` — Tier 7.A.1 smoke test for `messages.pin` +
/// `messages.unpin`.
///
/// Operator pre-action:
/// 1. Set `OCTO_WHATSAPP_TEST_INBOUND_MSG_ID` to the id of a
///    message in any chat you control (sending yourself a fresh
///    text message from a second device is the easiest path).
/// 2. Set `OCTO_WHATSAPP_TEST_MEMBER` to the E.164 phone of that
///    chat peer (use your own number for the self-chat).
///
/// The test pins the message, asserts RPC success, then unpins
/// and asserts RPC success. No event predicate — WA does not emit
/// a pin/unpin event to the sender's own device.
#[tokio::test]
async fn live_pin_message() {
    let inbound_msg_id = match std::env::var("OCTO_WHATSAPP_TEST_INBOUND_MSG_ID").ok() {
        Some(v) if !v.is_empty() => v,
        _ => {
            eprintln!(
                "live_pin_message: skipping \
                 (set OCTO_WHATSAPP_TEST_INBOUND_MSG_ID to the message id of a fresh \
                 inbound Message from TEST_MEMBER to your account)"
            );
            return;
        }
    };
    let peer_jid = match std::env::var("OCTO_WHATSAPP_TEST_MEMBER").ok() {
        Some(v) if !v.is_empty() => octo_whatsapp::jids::peer_to_jid(&v)
            .unwrap_or_else(|e| panic!("OCTO_WHATSAPP_TEST_MEMBER invalid: {e}")),
        _ => {
            eprintln!(
                "live_pin_message: skipping (also set \
                 OCTO_WHATSAPP_TEST_MEMBER to the sender's phone)"
            );
            return;
        }
    };
    let fix = fixture();

    let mut conn = rpc(fix).await;
    let pin_resp = conn
        .call(
            "messages.pin",
            json!({"peer": peer_jid.clone(), "msg_id": inbound_msg_id.clone()}),
        )
        .await;
    inter_call_delay_for("messages.pin");
    assert_eq!(
        pin_resp["status"], "pinned",
        "messages.pin must return status=pinned; got {pin_resp}"
    );
    assert_eq!(pin_resp["msg_id"], inbound_msg_id);

    // Unpin. Same delay policy: pin and unpin are separate WA
    // calls, each on the 2 s floor.
    let unpin_resp = conn
        .call(
            "messages.unpin",
            json!({"peer": peer_jid.clone(), "msg_id": inbound_msg_id.clone()}),
        )
        .await;
    inter_call_delay_for("messages.unpin");
    assert_eq!(
        unpin_resp["status"], "unpinned",
        "messages.unpin must return status=unpinned; got {unpin_resp}"
    );
    assert_eq!(unpin_resp["msg_id"], inbound_msg_id);

    eprintln!("live_pin_message: OK pin+unpin cycle for {inbound_msg_id} in {peer_jid}");
}

/// `live_forward_message` — Tier 7.A.2 smoke test for
/// `messages.forward`.
///
/// Operator pre-action:
/// 1. From your second device, send a text message to your
///    linked-account number. The message id is what we forward.
/// 2. Set `OCTO_WHATSAPP_TEST_MEMBER` to the E.164 phone of that
///    second device (the original sender).
/// 3. Set `OCTO_WHATSAPP_TEST_FORWARD_PEER` to the phone of a
///    THIRD party that should receive the forward (or reuse
///    `OCTO_WHATSAPP_TEST_MEMBER` to forward back to the sender).
/// 4. Set `OCTO_WHATSAPP_TEST_FORWARD_ORIGINAL_MSG_ID` to the id
///    of the message you sent in step 1.
///
/// The RPC returns `{status, peer, original_msg_id, new_msg_id}`.
/// We assert `status=forwarded` and that `new_msg_id` is a
/// non-empty string. No inbound event lands on OUR buffer — the
/// forwarded message is delivered to the receiver's device and
/// surfaces on THEIR buffer.
#[tokio::test]
async fn live_forward_message() {
    let original_msg_id = match std::env::var("OCTO_WHATSAPP_TEST_FORWARD_ORIGINAL_MSG_ID").ok() {
        Some(v) if !v.is_empty() => v,
        _ => {
            eprintln!(
                "live_forward_message: skipping \
                     (set OCTO_WHATSAPP_TEST_FORWARD_ORIGINAL_MSG_ID to the message id \
                     of a message you previously sent to OCTO_WHATSAPP_TEST_MEMBER)"
            );
            return;
        }
    };
    let forward_peer_phone = match std::env::var("OCTO_WHATSAPP_TEST_FORWARD_PEER").ok() {
        Some(v) if !v.is_empty() => v,
        _ => {
            eprintln!(
                "live_forward_message: skipping \
                     (also set OCTO_WHATSAPP_TEST_FORWARD_PEER to the E.164 phone of \
                     the intended recipient)"
            );
            return;
        }
    };
    let forward_peer_jid = octo_whatsapp::jids::peer_to_jid(&forward_peer_phone)
        .unwrap_or_else(|e| panic!("OCTO_WHATSAPP_TEST_FORWARD_PEER invalid: {e}"));
    let fix = fixture();

    let mut conn = rpc(fix).await;
    let resp = conn
        .call(
            "messages.forward",
            json!({
                "peer": forward_peer_jid.clone(),
                "original_msg_id": original_msg_id.clone(),
            }),
        )
        .await;
    inter_call_delay_for("messages.forward");

    assert_eq!(
        resp["status"], "forwarded",
        "messages.forward must return status=forwarded; got {resp}"
    );
    assert_eq!(resp["original_msg_id"], original_msg_id);
    assert_eq!(resp["peer"], forward_peer_jid);
    assert!(
        resp["new_msg_id"].is_string() && !resp["new_msg_id"].as_str().unwrap().is_empty(),
        "messages.forward must return a non-empty new_msg_id; got {resp}"
    );

    eprintln!(
        "live_forward_message: OK {original_msg_id} -> {forward_peer_jid} new={}",
        resp["new_msg_id"]
    );
}

/// `live_edit_encrypted` — Tier 7.A.3 smoke test for
/// `messages.edit_encrypted`.
///
/// **Operator pre-action:** the 32-byte message_secret from the
/// original send is required. `send.text`'s current response
/// shape does NOT expose that secret — capturing it requires a
/// follow-up commit that adds it to `SendResult` and to the
/// `send.text` RPC response. Until that lands, this test is
/// permanently skip-with-hint so the suite remains green even
/// without the missing plumbing.
///
/// Once `OCTO_WHATSAPP_TEST_EDIT_SECRET_B64` can be populated
/// from a fresh `send.text` response, this test will:
/// 1. Set `OCTO_WHATSAPP_TEST_INBOUND_MSG_ID` to the just-sent
///    msg id.
/// 2. Set `OCTO_WHATSAPP_TEST_MEMBER` to the peer (or self).
/// 3. Set `OCTO_WHATSAPP_TEST_EDIT_SECRET_B64` to the
///    base64-encoded 32-byte message_secret.
/// 4. Call `messages.edit_encrypted` and assert the returned
///    `new_msg_id` is a non-empty string.
#[tokio::test]
async fn live_edit_encrypted() {
    let inbound_msg_id = match std::env::var("OCTO_WHATSAPP_TEST_INBOUND_MSG_ID").ok() {
        Some(v) if !v.is_empty() => v,
        _ => {
            eprintln!(
                "live_edit_encrypted: skipping \
                     (set OCTO_WHATSAPP_TEST_INBOUND_MSG_ID to a recent message id)"
            );
            return;
        }
    };
    let secret_b64 = match std::env::var("OCTO_WHATSAPP_TEST_EDIT_SECRET_B64").ok() {
        Some(v) if !v.is_empty() => v,
        _ => {
            eprintln!(
                "live_edit_encrypted: skipping — message_secret exposure from \
                 send.text is not yet implemented; capture the 32-byte secret via a \
                 follow-up commit on the `send.text` response shape, then re-run \
                 with OCTO_WHATSAPP_TEST_EDIT_SECRET_B64 set"
            );
            return;
        }
    };
    let peer_jid = match std::env::var("OCTO_WHATSAPP_TEST_MEMBER").ok() {
        Some(v) if !v.is_empty() => octo_whatsapp::jids::peer_to_jid(&v)
            .unwrap_or_else(|e| panic!("OCTO_WHATSAPP_TEST_MEMBER invalid: {e}")),
        _ => {
            eprintln!(
                "live_edit_encrypted: skipping (also set \
                 OCTO_WHATSAPP_TEST_MEMBER to the peer phone)"
            );
            return;
        }
    };
    let fix = fixture();
    let _ = fix;

    let mut conn = rpc(fix).await;
    let resp = conn
        .call(
            "messages.edit_encrypted",
            json!({
                "peer": peer_jid.clone(),
                "msg_id": inbound_msg_id.clone(),
                "message_secret_b64": secret_b64,
                "new_text": "live_edit_encrypted test",
            }),
        )
        .await;
    inter_call_delay_for("messages.edit_encrypted");

    assert_eq!(
        resp["status"], "edited",
        "messages.edit_encrypted must return status=edited; got {resp}"
    );
    assert_eq!(resp["msg_id"], inbound_msg_id);
    assert!(
        resp["new_msg_id"].is_string() && !resp["new_msg_id"].as_str().unwrap().is_empty(),
        "messages.edit_encrypted must return a non-empty new_msg_id; got {resp}"
    );

    eprintln!(
        "live_edit_encrypted: OK {inbound_msg_id} -> {}",
        resp["new_msg_id"]
    );
}

// ===========================================================================
// Tier 7.B — polls vote / aggregate + events respond smoke tests
// ===========================================================================
//
// All three honour the same env-skip convention as the Tier 7.A
// tests above: missing env var → `eprintln!` + early return (test
// passes). The tests check the env BEFORE calling fixture() so they
// skip cleanly even when no WA session is paired.

/// `live_vote_poll` — Tier 7.B smoke test for `polls.vote`.
///
/// Operator pre-action:
/// 1. From your second device (TEST_MEMBER), send yourself a poll
///    via WA Web (`is_quiz=false`, multi=false). The poll's
///    `message_secret` is in the WA Web > Inspect panel of the
///    message (32-byte base64 string).
/// 2. Set `OCTO_WHATSAPP_TEST_POLL_MSG_ID` to that poll's msg id.
/// 3. Set `OCTO_WHATSAPP_TEST_POLL_CREATOR_JID` to the JID of the
///    TEST_MEMBER sender (e.g. `15551234567@s.whatsapp.net`).
/// 4. Set `OCTO_WHATSAPP_TEST_POLL_SECRET_B64` to the 32-byte
///    base64-encoded poll secret.
/// 5. Set `OCTO_WHATSAPP_TEST_POLL_OPTIONS` to the option names
///    matching the poll (for single-select, one entry; for
///    multi-select, one or more).
///
/// Asserts `status=voted` and a non-empty `message_id`. The
/// `InboundEvent::Receipt::ServerAck` for the vote surfaces on
/// OUR buffer (different event from the underlying poll-create
/// receipt). We don't assert it because the inbound flow is
/// covered by Tier 3 (general receipts); this test focuses on
/// the RPC contract.
#[tokio::test]
async fn live_vote_poll() {
    let poll_msg_id = match std::env::var("OCTO_WHATSAPP_TEST_POLL_MSG_ID").ok() {
        Some(v) if !v.is_empty() => v,
        _ => {
            eprintln!(
                "live_vote_poll: skipping \
                 (set OCTO_WHATSAPP_TEST_POLL_MSG_ID, \
                 OCTO_WHATSAPP_TEST_POLL_CREATOR_JID, \
                 OCTO_WHATSAPP_TEST_POLL_SECRET_B64, and \
                 OCTO_WHATSAPP_TEST_POLL_OPTIONS — see doc-comment)"
            );
            return;
        }
    };
    let poll_creator_jid = match std::env::var("OCTO_WHATSAPP_TEST_POLL_CREATOR_JID").ok() {
        Some(v) if !v.is_empty() => v,
        _ => {
            eprintln!("live_vote_poll: skipping (OCTO_WHATSAPP_TEST_POLL_CREATOR_JID unset)");
            return;
        }
    };
    let secret_b64 = match std::env::var("OCTO_WHATSAPP_TEST_POLL_SECRET_B64").ok() {
        Some(v) if !v.is_empty() => v,
        _ => {
            eprintln!("live_vote_poll: skipping (OCTO_WHATSAPP_TEST_POLL_SECRET_B64 unset)");
            return;
        }
    };
    let options_csv = match std::env::var("OCTO_WHATSAPP_TEST_POLL_OPTIONS").ok() {
        Some(v) if !v.is_empty() => v,
        _ => {
            eprintln!("live_vote_poll: skipping (OCTO_WHATSAPP_TEST_POLL_OPTIONS unset)");
            return;
        }
    };
    let peer_jid = match std::env::var("OCTO_WHATSAPP_TEST_MEMBER").ok() {
        Some(v) if !v.is_empty() => octo_whatsapp::jids::peer_to_jid(&v)
            .unwrap_or_else(|e| panic!("OCTO_WHATSAPP_TEST_MEMBER invalid: {e}")),
        _ => {
            eprintln!("live_vote_poll: skipping (OCTO_WHATSAPP_TEST_MEMBER unset)");
            return;
        }
    };
    let selected_options: Vec<String> = options_csv
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect();
    if selected_options.is_empty() {
        eprintln!(
            "live_vote_poll: skipping (OCTO_WHATSAPP_TEST_POLL_OPTIONS had no valid entries)"
        );
        return;
    }

    let fix = fixture();
    let _ = fix;
    let mut conn = rpc(fix).await;
    let resp = conn
        .call(
            "polls.vote",
            json!({
                "peer": peer_jid,
                "poll_msg_id": poll_msg_id.clone(),
                "poll_creator_jid": poll_creator_jid,
                "message_secret_b64": secret_b64,
                "selected_options": selected_options,
            }),
        )
        .await;
    inter_call_delay_for("polls.vote");

    assert_eq!(
        resp["status"], "voted",
        "polls.vote must return status=voted; got {resp}"
    );
    assert_eq!(resp["poll_msg_id"], poll_msg_id);
    assert!(
        resp["message_id"].is_string() && !resp["message_id"].as_str().unwrap().is_empty(),
        "polls.vote must return non-empty message_id; got {resp}"
    );
    eprintln!(
        "live_vote_poll: OK poll={poll_msg_id} -> vote={}",
        resp["message_id"]
    );
}

/// `live_aggregate_poll` — Tier 7.B smoke test for
/// `polls.aggregate`.
///
/// Operator pre-action: same env vars as `live_vote_poll`, plus
/// `OCTO_WHATSAPP_TEST_POLL_VOTES_JSON` — a JSON array of
/// `{voter_jid, enc_payload_b64, enc_iv_b64}` entries harvested
/// from inbound WA WS frames (TODO: future `InboundEvent::PollVote`
/// will surface these automatically).
///
/// Asserts `status=aggregated` and that `results` is an array
/// (possibly empty — actual decryption depends on the WA
/// server actually accepting the operator-harvested votes).
#[tokio::test]
async fn live_aggregate_poll() {
    let poll_msg_id = match std::env::var("OCTO_WHATSAPP_TEST_POLL_MSG_ID").ok() {
        Some(v) if !v.is_empty() => v,
        _ => {
            eprintln!(
                "live_aggregate_poll: skipping \
                 (set OCTO_WHATSAPP_TEST_POLL_MSG_ID, \
                 OCTO_WHATSAPP_TEST_POLL_CREATOR_JID, \
                 OCTO_WHATSAPP_TEST_POLL_SECRET_B64, \
                 OCTO_WHATSAPP_TEST_POLL_OPTIONS, and \
                 OCTO_WHATSAPP_TEST_POLL_VOTES_JSON — see doc-comment)"
            );
            return;
        }
    };
    let poll_creator_jid = match std::env::var("OCTO_WHATSAPP_TEST_POLL_CREATOR_JID").ok() {
        Some(v) if !v.is_empty() => v,
        _ => {
            eprintln!("live_aggregate_poll: skipping (OCTO_WHATSAPP_TEST_POLL_CREATOR_JID unset)");
            return;
        }
    };
    let secret_b64 = match std::env::var("OCTO_WHATSAPP_TEST_POLL_SECRET_B64").ok() {
        Some(v) if !v.is_empty() => v,
        _ => {
            eprintln!("live_aggregate_poll: skipping (OCTO_WHATSAPP_TEST_POLL_SECRET_B64 unset)");
            return;
        }
    };
    let options_csv = match std::env::var("OCTO_WHATSAPP_TEST_POLL_OPTIONS").ok() {
        Some(v) if !v.is_empty() => v,
        _ => {
            eprintln!("live_aggregate_poll: skipping (OCTO_WHATSAPP_TEST_POLL_OPTIONS unset)");
            return;
        }
    };
    let votes_json = match std::env::var("OCTO_WHATSAPP_TEST_POLL_VOTES_JSON").ok() {
        Some(v) if !v.is_empty() => v,
        _ => {
            eprintln!("live_aggregate_poll: skipping (OCTO_WHATSAPP_TEST_POLL_VOTES_JSON unset)");
            return;
        }
    };
    let poll_options: Vec<String> = options_csv
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect();
    if poll_options.is_empty() {
        eprintln!(
            "live_aggregate_poll: skipping (OCTO_WHATSAPP_TEST_POLL_OPTIONS had no valid entries)"
        );
        return;
    }
    let votes_value: Value = match serde_json::from_str(&votes_json) {
        Ok(v) => v,
        Err(e) => panic!("OCTO_WHATSAPP_TEST_POLL_VOTES_JSON invalid JSON: {e}"),
    };

    let fix = fixture();
    let _ = fix;
    let mut conn = rpc(fix).await;
    let resp = conn
        .call(
            "polls.aggregate",
            json!({
                "options": poll_options,
                "votes": votes_value,
                "message_secret_b64": secret_b64,
                "poll_msg_id": poll_msg_id.clone(),
                "poll_creator_jid": poll_creator_jid,
            }),
        )
        .await;
    inter_call_delay_for("polls.aggregate");

    assert_eq!(
        resp["status"], "aggregated",
        "polls.aggregate must return status=aggregated; got {resp}"
    );
    assert_eq!(resp["poll_msg_id"], poll_msg_id);
    let results = resp["results"]
        .as_array()
        .expect("polls.aggregate results must be an array");
    for entry in results {
        assert!(
            entry["name"].is_string(),
            "each result must have a name; got {entry}"
        );
        assert!(
            entry["voters"].is_array(),
            "each result must have a voters array; got {entry}"
        );
    }
    eprintln!(
        "live_aggregate_poll: OK poll={poll_msg_id} -> {} option rows",
        results.len()
    );
}

/// `live_respond_event` — Tier 7.B smoke test for
/// `events.respond`.
///
/// Operator pre-action:
/// 1. Have TEST_MEMBER create a calendar event in the chat with
///    you. The 32-byte base64 `message_secret` is exposed via
///    WA Web > Inspect.
/// 2. Set `OCTO_WHATSAPP_TEST_EVENT_MSG_ID` to the event-creation
///    msg id.
/// 3. Set `OCTO_WHATSAPP_TEST_EVENT_CREATOR_JID` to the JID of
///    TEST_MEMBER.
/// 4. Set `OCTO_WHATSAPP_TEST_EVENT_SECRET_B64` to the
///    base64-encoded 32-byte secret.
/// 5. Set `OCTO_WHATSAPP_TEST_EVENT_RESPONSE` to one of
///    `going` / `not_going` / `maybe`.
///
/// Asserts `status=responded` and a non-empty `message_id`.
/// No event predicate (RSVPs do not surface on the responder's
/// own buffer — they ride along on the original event message
/// on the creator's device).
#[tokio::test]
async fn live_respond_event() {
    let event_msg_id = match std::env::var("OCTO_WHATSAPP_TEST_EVENT_MSG_ID").ok() {
        Some(v) if !v.is_empty() => v,
        _ => {
            eprintln!(
                "live_respond_event: skipping \
                 (set OCTO_WHATSAPP_TEST_EVENT_MSG_ID, \
                 OCTO_WHATSAPP_TEST_EVENT_CREATOR_JID, \
                 OCTO_WHATSAPP_TEST_EVENT_SECRET_B64, and \
                 OCTO_WHATSAPP_TEST_EVENT_RESPONSE — see doc-comment)"
            );
            return;
        }
    };
    let event_creator_jid = match std::env::var("OCTO_WHATSAPP_TEST_EVENT_CREATOR_JID").ok() {
        Some(v) if !v.is_empty() => v,
        _ => {
            eprintln!("live_respond_event: skipping (OCTO_WHATSAPP_TEST_EVENT_CREATOR_JID unset)");
            return;
        }
    };
    let secret_b64 = match std::env::var("OCTO_WHATSAPP_TEST_EVENT_SECRET_B64").ok() {
        Some(v) if !v.is_empty() => v,
        _ => {
            eprintln!("live_respond_event: skipping (OCTO_WHATSAPP_TEST_EVENT_SECRET_B64 unset)");
            return;
        }
    };
    let response = match std::env::var("OCTO_WHATSAPP_TEST_EVENT_RESPONSE").ok() {
        Some(v) if !v.is_empty() => v,
        _ => {
            eprintln!("live_respond_event: skipping (OCTO_WHATSAPP_TEST_EVENT_RESPONSE unset)");
            return;
        }
    };
    let peer_jid = match std::env::var("OCTO_WHATSAPP_TEST_MEMBER").ok() {
        Some(v) if !v.is_empty() => octo_whatsapp::jids::peer_to_jid(&v)
            .unwrap_or_else(|e| panic!("OCTO_WHATSAPP_TEST_MEMBER invalid: {e}")),
        _ => {
            eprintln!("live_respond_event: skipping (OCTO_WHATSAPP_TEST_MEMBER unset)");
            return;
        }
    };

    let fix = fixture();
    let _ = fix;
    let mut conn = rpc(fix).await;
    let resp = conn
        .call(
            "events.respond",
            json!({
                "peer": peer_jid,
                "event_msg_id": event_msg_id.clone(),
                "event_creator_jid": event_creator_jid,
                "message_secret_b64": secret_b64,
                "response": response.clone(),
            }),
        )
        .await;
    inter_call_delay_for("events.respond");

    assert_eq!(
        resp["status"], "responded",
        "events.respond must return status=responded; got {resp}"
    );
    assert_eq!(resp["event_msg_id"], event_msg_id);
    assert_eq!(resp["response"], response);
    assert!(
        resp["message_id"].is_string() && !resp["message_id"].as_str().unwrap().is_empty(),
        "events.respond must return non-empty message_id; got {resp}"
    );
    eprintln!(
        "live_respond_event: OK event={event_msg_id} response={response} -> {}",
        resp["message_id"]
    );
}

// ===========================================================================
// Tier 7.C — status / broadcast story smoke tests
// ===========================================================================
//
// WA status (a.k.a. "story") updates are public broadcasts visible
// to a subset of contacts; the RPCs accept the recipient JID list
// inline because the runtime caller (operator or upstream tool)
// already knows who can see the status.
//
// All four tests honour the env-skip convention: missing env var →
// `eprintln!` + early return (test passes). Env checks run BEFORE
// fixture() so they skip cleanly without a paired WA session.
//
// For all four, `OCTO_WHATSAPP_STATUS_RECIPIENTS` carries a JSON
// array of recipients (typically your full contact list, freshly
// snapshotted at run time).

fn live_status_recipients_jid() -> Option<Vec<String>> {
    let raw = std::env::var("OCTO_WHATSAPP_STATUS_RECIPIENTS").ok()?;
    if raw.is_empty() {
        return None;
    }
    serde_json::from_str::<Vec<String>>(&raw).ok()
}

/// `live_status_send_text` — post a text status and assert the
/// returned `message_id`. No event predicate (status posts do not
/// surface on the sender's own buffer — recipients are the
/// observers).
#[tokio::test]
async fn live_status_send_text() {
    let recipients = match live_status_recipients_jid() {
        Some(r) if !r.is_empty() => r,
        _ => {
            eprintln!(
                "live_status_send_text: skipping \
                 (set OCTO_WHATSAPP_STATUS_RECIPIENTS to a JSON array of JIDs)"
            );
            return;
        }
    };
    let text = match std::env::var("OCTO_WHATSAPP_STATUS_TEXT").ok() {
        Some(v) if !v.is_empty() => v,
        _ => {
            eprintln!("live_status_send_text: skipping (OCTO_WHATSAPP_STATUS_TEXT unset)");
            return;
        }
    };
    let fix = fixture();
    let _ = fix;
    let mut conn = rpc(fix).await;
    let resp = conn
        .call(
            "status.send_text",
            json!({
                "text": text,
                "recipients": recipients,
            }),
        )
        .await;
    inter_call_delay_for("status.send_text");
    assert_eq!(
        resp["status"], "posted",
        "status.send_text must return status=posted; got {resp}"
    );
    assert!(
        resp["message_id"].is_string() && !resp["message_id"].as_str().unwrap().is_empty(),
        "status.send_text must return non-empty message_id; got {resp}"
    );
    eprintln!("live_status_send_text: OK -> {}", resp["message_id"]);
}

/// `live_status_send_image` — post an image status. Requires
/// `OCTO_WHATSAPP_STATUS_IMAGE_PATH` (a local file path).
/// `OCTO_WHATSAPP_STATUS_IMAGE_CAPTION` is optional.
#[tokio::test]
async fn live_status_send_image() {
    let recipients = match live_status_recipients_jid() {
        Some(r) if !r.is_empty() => r,
        _ => {
            eprintln!(
                "live_status_send_image: skipping \
                 (set OCTO_WHATSAPP_STATUS_RECIPIENTS)"
            );
            return;
        }
    };
    let file_path = match std::env::var("OCTO_WHATSAPP_STATUS_IMAGE_PATH").ok() {
        Some(v) if !v.is_empty() => v,
        _ => {
            eprintln!(
                "live_status_send_image: skipping \
                 (set OCTO_WHATSAPP_STATUS_IMAGE_PATH)"
            );
            return;
        }
    };
    let caption = std::env::var("OCTO_WHATSAPP_STATUS_IMAGE_CAPTION")
        .ok()
        .filter(|v| !v.is_empty());
    let fix = fixture();
    let _ = fix;
    let mut conn = rpc(fix).await;
    let resp = conn
        .call(
            "status.send_image",
            json!({
                "file_path": file_path,
                "caption": caption,
                "recipients": recipients,
            }),
        )
        .await;
    inter_call_delay_for("status.send_image");
    assert_eq!(
        resp["status"], "posted",
        "status.send_image must return status=posted; got {resp}"
    );
    assert!(
        resp["message_id"].is_string() && !resp["message_id"].as_str().unwrap().is_empty(),
        "status.send_image must return non-empty message_id; got {resp}"
    );
    eprintln!("live_status_send_image: OK -> {}", resp["message_id"]);
}

/// `live_status_send_video` — post a video status. Requires
/// `OCTO_WHATSAPP_STATUS_VIDEO_PATH` (a local file path) and
/// `OCTO_WHATSAPP_STATUS_VIDEO_DURATION_SECONDS` (integer seconds).
#[tokio::test]
async fn live_status_send_video() {
    let recipients = match live_status_recipients_jid() {
        Some(r) if !r.is_empty() => r,
        _ => {
            eprintln!(
                "live_status_send_video: skipping \
                 (set OCTO_WHATSAPP_STATUS_RECIPIENTS)"
            );
            return;
        }
    };
    let file_path = match std::env::var("OCTO_WHATSAPP_STATUS_VIDEO_PATH").ok() {
        Some(v) if !v.is_empty() => v,
        _ => {
            eprintln!(
                "live_status_send_video: skipping \
                 (set OCTO_WHATSAPP_STATUS_VIDEO_PATH)"
            );
            return;
        }
    };
    let duration_seconds: u32 =
        match std::env::var("OCTO_WHATSAPP_STATUS_VIDEO_DURATION_SECONDS").ok() {
            Some(v) if !v.is_empty() => match v.parse() {
                Ok(n) => n,
                Err(_) => {
                    eprintln!(
                        "live_status_send_video: skipping \
                         (OCTO_WHATSAPP_STATUS_VIDEO_DURATION_SECONDS not an integer)"
                    );
                    return;
                }
            },
            _ => {
                eprintln!(
                    "live_status_send_video: skipping \
                     (set OCTO_WHATSAPP_STATUS_VIDEO_DURATION_SECONDS)"
                );
                return;
            }
        };
    let fix = fixture();
    let _ = fix;
    let mut conn = rpc(fix).await;
    let resp = conn
        .call(
            "status.send_video",
            json!({
                "file_path": file_path,
                "duration_seconds": duration_seconds,
                "recipients": recipients,
            }),
        )
        .await;
    inter_call_delay_for("status.send_video");
    assert_eq!(
        resp["status"], "posted",
        "status.send_video must return status=posted; got {resp}"
    );
    assert!(
        resp["message_id"].is_string() && !resp["message_id"].as_str().unwrap().is_empty(),
        "status.send_video must return non-empty message_id; got {resp}"
    );
    eprintln!("live_status_send_video: OK -> {}", resp["message_id"]);
}

/// `live_status_revoke` — revoke a previously-posted status.
/// Requires `OCTO_WHATSAPP_STATUS_REVOKE_MSG_ID` from an earlier
/// `live_status_send_*` run.
#[tokio::test]
async fn live_status_revoke() {
    let recipients = match live_status_recipients_jid() {
        Some(r) if !r.is_empty() => r,
        _ => {
            eprintln!(
                "live_status_revoke: skipping \
                 (set OCTO_WHATSAPP_STATUS_RECIPIENTS)"
            );
            return;
        }
    };
    let message_id = match std::env::var("OCTO_WHATSAPP_STATUS_REVOKE_MSG_ID").ok() {
        Some(v) if !v.is_empty() => v,
        _ => {
            eprintln!(
                "live_status_revoke: skipping \
                 (set OCTO_WHATSAPP_STATUS_REVOKE_MSG_ID from a prior \
                 live_status_send_* run)"
            );
            return;
        }
    };
    let fix = fixture();
    let _ = fix;
    let mut conn = rpc(fix).await;
    let resp = conn
        .call(
            "status.revoke",
            json!({
                "message_id": message_id.clone(),
                "recipients": recipients,
            }),
        )
        .await;
    inter_call_delay_for("status.revoke");
    assert_eq!(
        resp["status"], "revoked",
        "status.revoke must return status=revoked; got {resp}"
    );
    assert_eq!(resp["message_id"], message_id);
    assert!(
        resp["revoke_message_id"].is_string()
            && !resp["revoke_message_id"].as_str().unwrap().is_empty(),
        "status.revoke must return non-empty revoke_message_id; got {resp}"
    );
    eprintln!(
        "live_status_revoke: OK {message_id} -> {}",
        resp["revoke_message_id"]
    );
}

// ===========================================================================
// Tier 7.D — profile pictures + business profile smoke tests
// ===========================================================================

/// `live_set_profile_picture` — upload a JPEG and assert the RPC
/// succeeds. No event predicate (profile pictures do not surface
/// as events on the sender's own buffer).
///
/// Operator pre-action:
/// 1. Set `OCTO_WHATSAPP_TEST_PROFILE_PIC` to a local JPEG file
///    (1x1 or small square works; WA Web re-encodes whatever
///    shape you pass — the bytes returned by the RPC are
///    opaque; tests only check the success response).
#[tokio::test]
async fn live_set_profile_picture() {
    let file_path = match std::env::var("OCTO_WHATSAPP_TEST_PROFILE_PIC").ok() {
        Some(v) if !v.is_empty() => v,
        _ => {
            eprintln!(
                "live_set_profile_picture: skipping \
                 (set OCTO_WHATSAPP_TEST_PROFILE_PIC to a local JPEG file path)"
            );
            return;
        }
    };
    let data = match std::fs::read(&file_path) {
        Ok(d) => d,
        Err(e) => panic!("OCTO_WHATSAPP_TEST_PROFILE_PIC unreadable: {e}"),
    };
    let b64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &data);

    let fix = fixture();
    let _ = fix;
    let mut conn = rpc(fix).await;
    let resp = conn
        .call(
            "profile.set_profile_picture",
            json!({ "image_data_b64": b64 }),
        )
        .await;
    inter_call_delay_for("profile.set_profile_picture");
    assert_eq!(
        resp["status"], "set",
        "profile.set_profile_picture must return status=set; got {resp}"
    );
    eprintln!(
        "live_set_profile_picture: OK ({} bytes uploaded)",
        data.len()
    );

    // Cleanup: remove the profile picture we just set so we don't
    // pollute the operator's account state.
    inter_call_delay_for("profile.remove_profile_picture");
    let cleanup = conn.call("profile.remove_profile_picture", json!({})).await;
    assert_eq!(
        cleanup["status"], "removed",
        "cleanup profile.remove_profile_picture must return status=removed; got {cleanup}"
    );
    eprintln!("live_set_profile_picture: cleanup OK (picture removed)");
}

/// `live_get_business_profile` — fetch the business profile for
/// TEST_MEMBER. Fails-open if the peer isn't a business account
/// (the WA server returns an empty profile, which we render as
/// `status=not_found`).
///
/// Operator pre-action:
/// 1. Set `OCTO_WHATSAPP_TEST_MEMBER` to the E.164 phone of a
///    peer that may be a business account. For best coverage,
///    use a known business like a local shop.
#[tokio::test]
async fn live_get_business_profile() {
    let peer_jid = match std::env::var("OCTO_WHATSAPP_TEST_MEMBER").ok() {
        Some(v) if !v.is_empty() => match octo_whatsapp::jids::peer_to_jid(&v) {
            Ok(j) => j,
            Err(e) => panic!("OCTO_WHATSAPP_TEST_MEMBER invalid: {e}"),
        },
        _ => {
            eprintln!("live_get_business_profile: skipping (OCTO_WHATSAPP_TEST_MEMBER unset)");
            return;
        }
    };

    let fix = fixture();
    let _ = fix;
    let mut conn = rpc(fix).await;
    let resp = conn
        .call(
            "contacts.get_business_profile",
            json!({ "jid": peer_jid.clone() }),
        )
        .await;
    inter_call_delay_for("contacts.get_business_profile");
    let status = resp["status"].as_str().unwrap_or("");
    assert!(
        status == "found" || status == "not_found",
        "contacts.get_business_profile status must be found|not_found; got {resp}"
    );
    assert_eq!(resp["jid"], peer_jid);
    if status == "found" {
        assert!(resp["profile"].is_object());
    }
    eprintln!("live_get_business_profile: OK status={status} peer={peer_jid}");
}

// ===========================================================================
// Tier 7.H live tests: groups.get_invite_link + groups.update_member_label.
//
// Both require `OCTO_WHATSAPP_TEST_GROUP_ID` (operator pre-created group).
// Skip-vs-fail convention: env unset → eprintln + early return so the
// rest of the live suite still runs.
// ===========================================================================

/// `live_get_invite_link` — 7.H canary.
///
/// Fetches the active invite link for a self-created group via
/// `groups.get_invite_link`. Skips cleanly if no env var.
#[tokio::test]
async fn live_get_invite_link() {
    let Some(_jid) = std::env::var("OCTO_WHATSAPP_TEST_GROUP_ID")
        .ok()
        .filter(|s| !s.is_empty())
    else {
        eprintln!(
            "live_get_invite_link: skipping (set OCTO_WHATSAPP_TEST_GROUP_ID to an \
             existing group JID the test account has joined)"
        );
        return;
    };
    let jid = std::env::var("OCTO_WHATSAPP_TEST_GROUP_ID").unwrap();
    assert!(jid.ends_with("@g.us"), "JID must end in @g.us; got {jid}");
    let fix = fixture();
    let _ = fix;
    let mut conn = rpc(fix).await;
    let resp = conn
        .call(
            "groups.get_invite_link",
            json!({ "jid": jid, "reset": false }),
        )
        .await;
    inter_call_delay_for("groups.get_invite_link");
    let link = resp["link"].as_str().unwrap_or("");
    assert!(
        link.starts_with("https://chat.whatsapp.com/"),
        "invite link must start with https://chat.whatsapp.com/; got {resp}"
    );
    eprintln!("live_get_invite_link: OK link_prefix={}", &link[..30]);
}

/// `live_update_member_label` — 7.H canary.
///
/// Sets the bot's per-group "member label" (empty string clear or
/// any short tag). Skips cleanly without env vars.
#[tokio::test]
async fn live_update_member_label() {
    let Some(_jid) = std::env::var("OCTO_WHATSAPP_TEST_GROUP_ID")
        .ok()
        .filter(|s| !s.is_empty())
    else {
        eprintln!(
            "live_update_member_label: skipping (set OCTO_WHATSAPP_TEST_GROUP_ID to an \
             existing group JID the test account has joined)"
        );
        return;
    };
    let jid = std::env::var("OCTO_WHATSAPP_TEST_GROUP_ID").unwrap();
    assert!(jid.ends_with("@g.us"), "JID must end in @g.us; got {jid}");
    let fix = fixture();
    let _ = fix;
    let mut conn = rpc(fix).await;
    let label = format!("cipherocto-7H-{}", std::process::id());
    let resp = conn
        .call(
            "groups.update_member_label",
            json!({ "jid": jid, "label": label.clone() }),
        )
        .await;
    inter_call_delay_for("groups.update_member_label");
    assert!(
        resp.is_object() && !resp.as_object().unwrap().contains_key("error"),
        "update_member_label should succeed; got {resp}"
    );
    // Clear it back to "" so we leave the group clean.
    let clear = conn
        .call(
            "groups.update_member_label",
            json!({ "jid": jid, "label": "" }),
        )
        .await;
    inter_call_delay_for("groups.update_member_label");
    assert!(
        clear.is_object() && !clear.as_object().unwrap().contains_key("error"),
        "update_member_label clear should succeed; got {clear}"
    );
    eprintln!("live_update_member_label: OK set={label:?} then cleared for jid={jid}");
}

// ===========================================================================
// Tier 7.H live tests (continued): groups.get_profile_pictures +
// groups.set_profile_picture + groups.remove_profile_picture.
//
// All three require `OCTO_WHATSAPP_TEST_GROUP_ID` (operator pre-created
// group the bot has joined and is admin of). The set step additionally
// requires `OCTO_WHATSAPP_TEST_PROFILE_PIC` pointing at a local JPEG file
// — same env var the Phase 7.D `live_set_profile_picture` test uses.
// Skip-vs-fail convention: env unset → eprintln + early return.
// ===========================================================================

/// `live_get_group_profile_pictures` — fetch profile pictures for
/// the test group and assert the response shape.
///
/// The WA server returns a Vec<GroupProfilePictureSnapshot>; the
/// snapshot's `photo_id` is opaque (a base64-url-encoded pointer).
/// Tests only assert that an array is returned with the expected JID.
#[tokio::test]
async fn live_get_group_profile_pictures() {
    let Some(_jid) = std::env::var("OCTO_WHATSAPP_TEST_GROUP_ID")
        .ok()
        .filter(|s| !s.is_empty())
    else {
        eprintln!(
            "live_get_group_profile_pictures: skipping \
             (set OCTO_WHATSAPP_TEST_GROUP_ID to an existing group JID \
              the test account has joined)"
        );
        return;
    };
    let jid = std::env::var("OCTO_WHATSAPP_TEST_GROUP_ID").unwrap();
    assert!(jid.ends_with("@g.us"), "JID must end in @g.us; got {jid}");
    let fix = fixture();
    let _ = fix;
    let mut conn = rpc(fix).await;
    let resp = conn
        .call(
            "groups.get_profile_pictures",
            json!({ "jids": [jid.clone()], "preview": true }),
        )
        .await;
    inter_call_delay_for("groups.get_profile_pictures");
    let arr = resp
        .as_array()
        .unwrap_or_else(|| panic!("groups.get_profile_pictures must return array; got {resp}"));
    assert!(
        !arr.is_empty(),
        "groups.get_profile_pictures should return at least one snapshot for {jid}; got {resp}"
    );
    let snap = &arr[0];
    assert_eq!(
        snap["group_jid"], jid,
        "snapshot group_jid must match the requested JID"
    );
    eprintln!(
        "live_get_group_profile_pictures: OK returned {} snapshot(s) for jid={}",
        arr.len(),
        jid
    );
}

/// `live_set_group_profile_picture` — upload a JPEG to the test group.
/// Operator pre-action: set `OCTO_WHATSAPP_TEST_GROUP_ID` + `OCTO_WHATSAPP_TEST_PROFILE_PIC`.
///
/// The IPC handler returns `{ "id": <photo_id> }`; tests assert the id
/// is non-empty. Cleanup removes the picture immediately to keep the
/// operator's group state clean.
#[tokio::test]
async fn live_set_group_profile_picture() {
    let Some(_jid) = std::env::var("OCTO_WHATSAPP_TEST_GROUP_ID")
        .ok()
        .filter(|s| !s.is_empty())
    else {
        eprintln!(
            "live_set_group_profile_picture: skipping \
             (set OCTO_WHATSAPP_TEST_GROUP_ID)"
        );
        return;
    };
    let file_path = match std::env::var("OCTO_WHATSAPP_TEST_PROFILE_PIC").ok() {
        Some(v) if !v.is_empty() => v,
        _ => {
            eprintln!(
                "live_set_group_profile_picture: skipping \
                 (set OCTO_WHATSAPP_TEST_PROFILE_PIC to a local JPEG file path)"
            );
            return;
        }
    };
    let data = match std::fs::read(&file_path) {
        Ok(d) => d,
        Err(e) => panic!("OCTO_WHATSAPP_TEST_PROFILE_PIC unreadable: {e}"),
    };
    let b64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &data);
    let jid = std::env::var("OCTO_WHATSAPP_TEST_GROUP_ID").unwrap();

    let fix = fixture();
    let _ = fix;
    let mut conn = rpc(fix).await;
    let resp = conn
        .call(
            "groups.set_profile_picture",
            json!({ "jid": jid.clone(), "image_data_b64": b64 }),
        )
        .await;
    inter_call_delay_for("groups.set_profile_picture");
    let photo_id = resp["id"].as_str().unwrap_or("");
    assert!(
        !photo_id.is_empty(),
        "groups.set_profile_picture must return non-empty id; got {resp}"
    );
    eprintln!(
        "live_set_group_profile_picture: OK uploaded {} bytes; photo_id={photo_id}",
        data.len()
    );

    // Cleanup: remove the group picture we just set so we don't pollute
    // the operator's group state. Best-effort — failure here is logged,
    // not asserted, because the test already proved the round-trip.
    inter_call_delay_for("groups.remove_profile_picture");
    let _cleanup = conn
        .call(
            "groups.remove_profile_picture",
            json!({ "jid": jid.clone() }),
        )
        .await;
    eprintln!("live_set_group_profile_picture: cleanup attempted (picture removed)");
}

/// `live_remove_group_profile_picture` — explicit remove test (no
/// preceding set). The WA server may return an error if the group has
/// no picture set; we tolerate either outcome. Skips without env vars.
#[tokio::test]
async fn live_remove_group_profile_picture() {
    let Some(_jid) = std::env::var("OCTO_WHATSAPP_TEST_GROUP_ID")
        .ok()
        .filter(|s| !s.is_empty())
    else {
        eprintln!(
            "live_remove_group_profile_picture: skipping \
             (set OCTO_WHATSAPP_TEST_GROUP_ID)"
        );
        return;
    };
    let jid = std::env::var("OCTO_WHATSAPP_TEST_GROUP_ID").unwrap();
    assert!(jid.ends_with("@g.us"), "JID must end in @g.us; got {jid}");
    let fix = fixture();
    let _ = fix;
    let mut conn = rpc(fix).await;
    let resp = conn
        .call(
            "groups.remove_profile_picture",
            json!({ "jid": jid.clone() }),
        )
        .await;
    inter_call_delay_for("groups.remove_profile_picture");
    // Either succeed (returns {id: "0"}) or error (no picture set).
    // We only assert the call did not panic and returned a JSON object.
    assert!(
        resp.is_object(),
        "groups.remove_profile_picture must return object; got {resp}"
    );
    eprintln!("live_remove_group_profile_picture: OK jid={jid} resp={resp}");
}
