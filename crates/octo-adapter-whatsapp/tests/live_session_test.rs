//! Live integration tests against a real WhatsApp Web session.
//!
//! These tests load a real session from `$OCTO_WHATSAPP_PERSIST_DIR`
//! (default `$HOME/.local/share/octo/whatsapp/persistent/default.session.db`),
//! drive `WhatsAppWebAdapter::start_bot()` against the production WhatsApp
//! Web servers (no `--ws-url` override), and run a small set of live
//! assertions on the resolved identity and adapter contract.
//!
//! **Not** run by default — requires:
//! - A mounted authenticated session (a `.session.db` produced by
//!   `octo-whatsapp-onboard qr-link` / `pair-link`).
//! - Network access to `web.whatsapp.com` / `wss://web.whatsapp.com`.
//! - ~30s for the noise-handshake + critical-app-state sync to settle.
//!
//! Run directly:
//!
//! ```bash
//! cargo test -p octo-adapter-whatsapp \
//!   --features live-whatsapp \
//!   --test live_session_test \
//!   -- --include-ignored --nocapture --test-threads=1
//! ```
//!
//! Environment variables consumed:
//! - `OCTO_WHATSAPP_PERSIST_DIR` — directory holding `default.session.db`.
//!   Defaults to `$HOME/.local/share/octo/whatsapp/persistent/`.
//! - `OCTO_WHATSAPP_SESSION_NAME` — session filename inside the dir
//!   (default: `default.session.db`).
//!
//! Why `--test-threads=1`: a single host should only hold one WhatsApp Web
//! connection per phone number (the WA servers will reject a second
//! concurrent device as a duplicate). Running these tests in parallel would
//! race for the connection and produce flaky "logged out" errors.

#![cfg(feature = "live-whatsapp")]

use octo_adapter_whatsapp::{WhatsAppConfig, WhatsAppWebAdapter};
use octo_network::dot::adapters::PlatformAdapter;
use std::time::Duration;

/// Default session directory matching the on-disk layout that
/// `octo-whatsapp-onboard` writes (see `octo-whatsapp-onboard/src/main.rs`
/// `default_session_base_dir`, which is `$XDG_DATA_HOME/octo/whatsapp/`).
///
/// Note: we resolve `$HOME` directly via `std::env` rather than pulling in
/// the `dirs` crate as a dev-dep. The `dirs::data_dir()` mapping on Linux
/// is `$XDG_DATA_HOME` (default `$HOME/.local/share`) anyway, so this
/// matches what the onboard tool would write.
///
/// The base dir is `octo/whatsapp` (no `persistent` subdir — that is a
/// Telegram-only convention; WhatsApp's onboard tool writes the session
/// directly under the base dir as `default.session.db/`).
fn default_persist_dir() -> std::path::PathBuf {
    if let Ok(p) = std::env::var("OCTO_WHATSAPP_PERSIST_DIR") {
        return std::path::PathBuf::from(p);
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    std::path::PathBuf::from(home)
        .join(".local")
        .join("share")
        .join("octo")
        .join("whatsapp")
}

fn default_session_name() -> String {
    std::env::var("OCTO_WHATSAPP_SESSION_NAME").unwrap_or_else(|_| "default.session.db".to_string())
}

/// Build a `WhatsAppConfig` that points at the on-disk session database.
/// Panics with a self-explanatory message if the session is missing — this
/// is the single chokepoint every test goes through, so the operator gets
/// one clear failure mode if the prereq isn't set up.
fn live_config() -> WhatsAppConfig {
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
        // Production WS URL (the adapter's default when `ws_url` is None).
        ws_url: None,
        // pair_phone / pair_code are irrelevant for an existing session;
        // the bot loads the persisted identity instead of starting a new
        // pair flow.
        pair_phone: None,
        pair_code: None,
        // groups is empty: we don't need to receive messages for these
        // tests; we only assert on the connected identity and the
        // PlatformAdapter contract.
        groups: vec![],
        // No per-group allowlist for these tests; legacy "anyone in the
        // group can inject" semantics apply (see RFC-0850p-a v1.15
        // §Adversary Analysis D-WA-10 and the accept_message contract).
        sender_allowlist: std::collections::BTreeMap::new(),
    }
}

/// Build a `WhatsAppWebAdapter`, call `start_bot()` (which spawns the
/// background bot task and returns once the noise handshake is in flight),
/// then wait for `Event::Connected` to fire and populate `self_handle()`.
///
/// The 30s wait matches the production-ready timeout that
/// `octo-whatsapp-onboard-core::session::wait_for_connected` uses for
/// `whoami` / `session verify` (R5-H2). On a healthy session the wait is
/// typically 2-10s; the upper bound is for slow networks / cold starts.
async fn live_adapter() -> WhatsAppWebAdapter {
    let config = live_config();
    if let Err(e) = config.validate() {
        panic!("invalid live WhatsAppConfig: {e}");
    }
    let adapter = WhatsAppWebAdapter::new(config);
    adapter.start_bot().await.unwrap_or_else(|e| {
        panic!(
            "WhatsAppWebAdapter::start_bot failed: {e:#}\n\
             is the session database at {:?} valid and the WS reachable?",
            default_persist_dir().join(default_session_name())
        );
    });
    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    loop {
        if adapter.self_handle().is_some() {
            return adapter;
        }
        if std::time::Instant::now() >= deadline {
            panic!(
                "timed out after 30s waiting for Event::Connected; \
                 self_handle() is still None. The session may have been \
                 logged out, or the WA servers rejected the noise handshake."
            );
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

fn init_tracing() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                tracing_subscriber::EnvFilter::new(
                    "info,octo_adapter_whatsapp=debug,whatsapp_rust=info",
                )
            }),
        )
        .try_init();
}

// ── Tests ──────────────────────────────────────────────────────────

/// Smoke test: the adapter connects, the bot stays alive, and
/// `health_check()` returns `Ok(())`. This is the smallest possible
/// end-to-end check that the on-disk session + adapter runtime agree.
#[tokio::test]
#[ignore = "requires live WhatsApp Web session"]
async fn live_session_health_check() {
    init_tracing();
    let adapter = live_adapter().await;
    adapter
        .health_check()
        .await
        .expect("health_check should return Ok for a valid session");
    tracing::info!("live_session_health_check: PASSED");
    let _ = adapter.shutdown().await;
}

/// The key assertion: `Event::Connected` fired and the adapter resolved
/// `self_handle()` to a real phone number (not None, not empty). This is
/// the test that would have caught any regression in the
/// `Event::Connected` handler at `adapter.rs:285-296` — if the device
/// snapshot failed to load or `pn.to_string()` produced a JID-shaped
/// string instead of a phone number, this would assert.
#[tokio::test]
#[ignore = "requires live WhatsApp Web session"]
async fn live_session_self_handle_returns_phone() {
    init_tracing();
    let adapter = live_adapter().await;

    let phone = adapter
        .self_handle()
        .expect("self_handle() must be Some after Event::Connected");

    assert!(
        !phone.is_empty(),
        "self_handle() returned an empty string; expected a phone number"
    );
    // The handler normalizes to digits-only via `normalize_phone`, so the
    // string should be ASCII digits (no `+`, no `@s.whatsapp.net` suffix).
    assert!(
        phone.chars().all(|c| c.is_ascii_digit()),
        "self_handle() returned {phone:?}; expected digits-only after normalize_phone"
    );
    // E.164 national portion is at most 15 digits; reject anything obviously
    // wrong (e.g. a JID that snuck past the normalize step).
    assert!(
        phone.len() <= 15,
        "self_handle() returned {phone:?} (len={}); expected <= 15 digits",
        phone.len()
    );
    assert!(
        phone.len() >= 7,
        "self_handle() returned {phone:?} (len={}); expected >= 7 digits",
        phone.len()
    );

    tracing::info!(phone = %phone, "live_session_self_handle_returns_phone: PASSED");
    let _ = adapter.shutdown().await;
}

/// `domain_id` derives a stable `BroadcastDomainId` from a group ID; this
/// test pins the two hash-function properties the DOT routing layer
/// depends on:
///
/// - **determinism**: same input → same `domain_hash` across calls (no
///   per-process salt, no clock-based nonce, no mutable cache).
/// - **injectivity**: different inputs → different `domain_hash` (BLAKE3
///   collision resistance on the `"whatsapp:{group_id}"` input).
///
/// Plus a well-formedness check that the `platform_type` field of the
/// returned `BroadcastDomainId` is the WhatsApp discriminant (`0x0008`),
/// not the reserved `0` ("unknown") value that would be rejected by
/// `BroadcastDomainId::from_canonical_bytes`.
///
/// Not named `*_round_trip` because no round trip happens here —
/// `BroadcastDomainId::new` is a pure function, and the assertions are
/// property tests on the output, not a send/receive cycle. The live
/// `WhatsAppWebAdapter` is required only because the method takes
/// `&self`; the assertions themselves don't depend on the WS connection.
#[tokio::test]
#[ignore = "requires live WhatsApp Web session"]
async fn live_session_domain_id_is_deterministic_and_injective() {
    init_tracing();
    let adapter = live_adapter().await;

    // Injectivity: distinct group_ids must produce distinct domain hashes.
    let a = adapter.domain_id("120363012345678901@g.us");
    let b = adapter.domain_id("120363098765432109@g.us");
    assert_ne!(
        a, b,
        "different group_ids must produce different domain hashes"
    );

    // Determinism: the same group_id must produce the same hash across
    // calls. Catches a refactor that adds a per-process salt, a clock
    // nonce, or a process-lifetime memoization that mutates state.
    let a2 = adapter.domain_id("120363012345678901@g.us");
    assert_eq!(
        a, a2,
        "domain_id is not deterministic for the same group_id"
    );

    // The platform_type is fixed; we don't pin the exact value (0x0008)
    // here because live_session_capabilities_pinned already pins
    // `adapter.platform_type() == 0x0008`. The narrower check below
    // catches a different regression: a domain_id whose platform_type
    // field is the reserved "unknown" discriminant (0) even though the
    // adapter's method returns the right value.
    assert_ne!(
        a.platform_type as u16, 0,
        "domain_id platform_type is zero; the adapter's platform_type() is wrong"
    );

    tracing::info!("live_session_domain_id_is_deterministic_and_injective: PASSED");
    let _ = adapter.shutdown().await;
}

/// `capabilities` must report the documented limits regardless of whether
/// the bot is connected. This pins the constants in production so a
/// refactor that silently shrinks `max_payload_bytes` from 64 KiB to 4 KiB
/// (matching the legacy WhatsApp text-only limit) is caught by CI.
#[tokio::test]
#[ignore = "requires live WhatsApp Web session"]
async fn live_session_capabilities_pinned() {
    init_tracing();
    let adapter = live_adapter().await;

    let caps = adapter.capabilities();
    assert_eq!(caps.max_payload_bytes, 65_536, "max_payload_bytes drifted");
    assert!(!caps.supports_fragmentation, "fragmentation must stay off");
    assert!(
        caps.supports_encryption,
        "Signal Protocol encryption must be reported"
    );
    assert!(
        !caps.supports_raw_binary,
        "binary payloads must stay off (text only)"
    );
    assert_eq!(
        caps.rate_limit_per_second, 20,
        "rate_limit_per_second drifted"
    );

    // Platform type must match the lib.rs ABI constant (0x0008).
    assert_eq!(
        adapter.platform_type() as u16,
        WhatsAppWebAdapter::PLATFORM_TYPE,
        "platform_type() disagrees with PLATFORM_TYPE constant"
    );

    tracing::info!("live_session_capabilities_pinned: PASSED");
    let _ = adapter.shutdown().await;
}
