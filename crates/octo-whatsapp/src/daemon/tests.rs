use super::*;
use crate::test_mock_adapter::MockAdapter;
use std::sync::Arc;

#[tokio::test(flavor = "multi_thread")]
async fn handle_phase_starts_booting() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let (_d, h) = Daemon::new_for_tests(tmp.path());
    assert_eq!(h.phase(), DaemonPhase::Booting);
}

#[tokio::test(flavor = "multi_thread")]
async fn cancel_token_is_linked() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let (d, h) = Daemon::new_for_tests(tmp.path());
    assert!(!h.cancel_token().is_cancelled());
    d.cancel_token().cancel();
    assert!(h.cancel_token().is_cancelled());
}

#[tokio::test(flavor = "multi_thread")]
async fn bind_adapter_stores_adapter() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let (_daemon, handle) = Daemon::new_for_tests(tmp.path());
    let adapter = Arc::new(MockAdapter::new());
    handle.bind_adapter(adapter.clone());
    assert!(
        handle.adapter().is_some(),
        "adapter slot must be populated after bind_adapter"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn bind_adapter_is_idempotent_when_no_events_stream() {
    // MockAdapter returns None from subscribe_raw_events (default trait impl),
    // so the connection-watcher is NOT spawned. Second bind_adapter call must
    // still succeed (single-bind-per-daemon contract).
    let tmp = tempfile::tempdir().expect("tempdir");
    let (_daemon, handle) = Daemon::new_for_tests(tmp.path());
    let adapter = Arc::new(MockAdapter::new());
    handle.bind_adapter(adapter.clone());
    handle.bind_adapter(adapter.clone());
    assert!(handle.adapter().is_some());
}

#[tokio::test(flavor = "multi_thread")]
async fn daemon_new_initializes_accounts_store() {
    // Phase 6.1 T6.1.2: DaemonInner owns a `MultiAccountStore` opened at
    // startup via `MultiAccountStore::open_default()`. `DaemonHandle::accounts()`
    // returns a guard that exposes `list`/`info`/`use_account`. On a fresh
    // machine the index file does not exist; `open_default()` returns an
    // empty in-memory store, so `.list()` yields `[]`. We only assert the
    // accessor does not panic and returns without error. `tokio::test` is
    // required because `Daemon::new` spawns the rules-persister actor,
    // which needs a Tokio runtime.
    let tmp = tempfile::tempdir().expect("tempdir");
    let (_daemon, handle) = Daemon::new_for_tests(tmp.path());
    let _entries = handle.accounts().list();
    // No panic → store opened successfully (or fell back to `None`; both
    // cases yield an empty Vec via the guard's `unwrap_or_default()`);
}

#[tokio::test(flavor = "multi_thread")]
async fn rebind_adapter_for_replaces_slot_with_new_adapter() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let (_daemon, handle) = Daemon::new_for_tests(tmp.path());

    let adapter_a = std::sync::Arc::new(crate::test_mock_adapter::MockAdapter::new());
    handle.bind_adapter(adapter_a.clone());
    assert!(handle.adapter().is_some(), "first bind must populate slot");

    let new_session = tmp.path().join("account-b.session.db");
    handle.rebind_adapter_for("account-b", &new_session);

    assert!(
        handle.adapter().is_some(),
        "slot must remain populated after rebind"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn rebind_adapter_for_works_when_no_adapter_bound_yet() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let (_daemon, handle) = Daemon::new_for_tests(tmp.path());
    assert!(handle.adapter().is_none(), "slot starts empty");

    let new_session = tmp.path().join("default.session.db");
    handle.rebind_adapter_for("default", &new_session);

    assert!(handle.adapter().is_some());
}

#[tokio::test(flavor = "multi_thread")]
async fn new_for_tests_creates_daemon_with_paths_in_tmpdir() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let (_daemon, handle) = Daemon::new_for_tests(tmp.path());
    // The store must be open and queryable.
    assert_eq!(
        handle.accounts().list().len(),
        0,
        "fresh tmpdir -> empty index"
    );
    // The index file must exist at tmpdir, NOT under $HOME/.local/share/octo/whatsapp.
    let expected_index = tmp.path().join("data/index.json");
    assert!(
        expected_index.exists(),
        "store must live at tmpdir/data/index.json; got {:?}",
        expected_index
    );
}

// ── AppState-sync flag (synced) tests ─────────────────────────────────
//
// The `synced` field of `daemon.status.get` is wired to the
// `is_synced_flag` atomic, which is flipped by a daemon-side
// sync-watcher task spawned in `bind_adapter` whenever the
// adapter's `synced_notify()` returns `Some(...)`. The adapter
// fires `notify_waiters()` on `Event::OfflineSyncCompleted`
// (and on the 0-conversation terminal `Event::HistorySync`).
//
// These tests cover:
//   1. Default: MockAdapter without `synced_notify` set → flag
//      stays `false` (the trait default returns `None`).
//   2. Rebind: `bind_adapter` resets the flag to `false` even if
//      a prior adapter's watcher had flipped it to `true`.
//   3. End-to-end: a custom `Notify` fired after `bind_adapter`
//      causes the flag to flip to `true` within a short poll
//      window.

#[tokio::test(flavor = "multi_thread")]
async fn bind_adapter_does_not_set_synced_when_synced_notify_is_none() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let (_daemon, handle) = Daemon::new_for_tests(tmp.path());
    // MockAdapter without `with_synced_notify` returns `None` from
    // the trait method — no sync-watcher task spawned, flag stays
    // its default `false`.
    let adapter = Arc::new(MockAdapter::new());
    handle.bind_adapter(adapter);
    assert!(
        !handle
            .is_synced_flag()
            .load(std::sync::atomic::Ordering::SeqCst),
        "synced must stay false when adapter exposes no synced_notify"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn bind_adapter_resets_synced_flag_so_rebind_starts_from_false() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let (_daemon, handle) = Daemon::new_for_tests(tmp.path());
    let notify = Arc::new(tokio::sync::Notify::new());
    // Adapter A exposes a Notify and fires it before bind so the
    // sync-watcher flips the flag to true.
    let adapter_a = Arc::new(MockAdapter::new().with_synced_notify(notify.clone()));
    handle.bind_adapter(adapter_a);
    // Yield so the spawned sync-watcher task registers its
    // `notified()` future BEFORE we fire the notify — see the
    // matching note on test 3 below.
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    notify.notify_waiters();
    // Wait up to 500ms for the watcher task to observe + flip.
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(500);
    while !handle
        .is_synced_flag()
        .load(std::sync::atomic::Ordering::SeqCst)
        && std::time::Instant::now() < deadline
    {
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert!(
        handle
            .is_synced_flag()
            .load(std::sync::atomic::Ordering::SeqCst),
        "notify must flip the flag before the rebind test step"
    );
    // Rebind to a fresh adapter — bind_adapter must reset the flag
    // synchronously before spawning the new sync-watcher task.
    let adapter_b = Arc::new(MockAdapter::new());
    handle.bind_adapter(adapter_b);
    assert!(
        !handle
            .is_synced_flag()
            .load(std::sync::atomic::Ordering::SeqCst),
        "bind_adapter must reset is_synced to false on rebind"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn synced_flag_flips_when_adapter_notifies_after_bind() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let (_daemon, handle) = Daemon::new_for_tests(tmp.path());
    let notify = Arc::new(tokio::sync::Notify::new());
    let adapter = Arc::new(MockAdapter::new().with_synced_notify(notify.clone()));
    handle.bind_adapter(adapter);
    assert!(
        !handle
            .is_synced_flag()
            .load(std::sync::atomic::Ordering::SeqCst),
        "flag must be false immediately after bind (no notify yet)"
    );
    // Fire the notify. The sync-watcher loop observes it and
    // stores `true`. Poll up to 500ms for the flip.
    // Yield so the spawned sync-watcher task registers its
    // `notified()` future BEFORE we fire the notify. Otherwise
    // `notify_waiters()` can land before the task is scheduled and
    // the notification is lost (Notify only wakes waiters
    // registered at the time of the call).
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    notify.notify_waiters();
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(500);
    while !handle
        .is_synced_flag()
        .load(std::sync::atomic::Ordering::SeqCst)
        && std::time::Instant::now() < deadline
    {
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert!(
        handle
            .is_synced_flag()
            .load(std::sync::atomic::Ordering::SeqCst),
        "firing synced_notify must flip the daemon's is_synced_flag"
    );
}

// ── Pairing stall timer (Phase 6.12.5) ────────────────────────────
//
// The production const `PAIRING_STALL_SECS = 45s` is too slow for
// hermetic tests; these exercise `run_connection_watcher_inner` with
// a 150ms threshold instead.

/// 150ms is short enough to keep tests fast (multi-thread runtime
/// wakeup latency dominates below ~50ms) but long enough to avoid
/// flake on a busy CI box. Tuned via empirical runs on this
/// developer's machine; bump up if a slow CI starts flake-ing.
const TEST_STALL: std::time::Duration = std::time::Duration::from_millis(150);

/// Build a `broadcast::Sender<Arc<InboundEvent>>` + paired receiver
/// and spawn `run_connection_watcher_inner` against a fresh daemon.
/// Returns the sender (caller drives events) and the daemon handle
/// (caller asserts state).
async fn spawn_watcher() -> (
    tokio::sync::broadcast::Sender<std::sync::Arc<crate::events::InboundEvent>>,
    DaemonHandle,
    tempfile::TempDir,
) {
    let tmp = tempfile::tempdir().expect("tempdir");
    let (_daemon, handle) = Daemon::new_for_tests(tmp.path());
    let (tx, rx) =
        tokio::sync::broadcast::channel::<std::sync::Arc<crate::events::InboundEvent>>(64);
    let cancel = handle.cancel_token().clone();
    tokio::spawn(super::run_connection_watcher_inner(
        rx,
        handle.clone(),
        cancel,
        TEST_STALL,
    ));
    // Brief yield so the spawned task is scheduled and observing
    // the broadcast channel before the test publishes.
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    (tx, handle, tmp)
}

#[tokio::test(flavor = "multi_thread")]
async fn pairing_stall_timer_fires_awaiting_user_action() {
    let (tx, handle, _tmp) = spawn_watcher().await;

    // Send a PairingQrCode-classified typed event. The watcher
    // inspects `event_kind()` for the stable per-variant label.
    tx.send(std::sync::Arc::new(
        crate::events::InboundEvent::PairingQrCode {
            qr_code: "x".to_string(),
            ref_string: String::new(),
            timeout: 60,
            ts_unix_ms: 0,
            ts_mono_ns: 0,
        },
    ))
    .expect("send PairingQrCode");

    // Wait long enough for the classifier + stall timer to fire.
    tokio::time::sleep(TEST_STALL * 3).await;

    assert_eq!(
        handle.bot_state(),
        BotStateMirror::AwaitingUserAction,
        "stall timer must fire AwaitingUserAction when no terminal event follows pairing prompt"
    );

    // Cancel the watcher to clean up the task.
    handle.cancel_token().cancel();
}

#[tokio::test(flavor = "multi_thread")]
async fn pairing_stall_timer_cleared_by_terminal_event() {
    let (tx, handle, _tmp) = spawn_watcher().await;

    tx.send(std::sync::Arc::new(
        crate::events::InboundEvent::PairingQrCode {
            qr_code: "x".to_string(),
            ref_string: String::new(),
            timeout: 60,
            ts_unix_ms: 0,
            ts_mono_ns: 0,
        },
    ))
    .expect("send PairingQrCode");
    // Send Connected before the stall timer fires.
    tx.send(std::sync::Arc::new(
        crate::events::InboundEvent::Connected {
            ts_unix_ms: 0,
            ts_mono_ns: 0,
        },
    ))
    .expect("send Connected");

    // Wait > stall threshold — if the timer were still armed it
    // would fire by now.
    tokio::time::sleep(TEST_STALL * 3).await;

    assert_eq!(
        handle.bot_state(),
        BotStateMirror::Connected,
        "terminal event must clear stall timer; Connected state must stick"
    );

    handle.cancel_token().cancel();
}

#[tokio::test(flavor = "multi_thread")]
async fn pairing_stall_timer_resets_on_re_pair() {
    let (tx, handle, _tmp) = spawn_watcher().await;

    // First pairing attempt: PairingQr then LoggedOut (server
    // rejected). The LoggedOut is a terminal event that clears the
    // timer.
    tx.send(std::sync::Arc::new(
        crate::events::InboundEvent::PairingQrCode {
            qr_code: "a".to_string(),
            ref_string: String::new(),
            timeout: 60,
            ts_unix_ms: 0,
            ts_mono_ns: 0,
        },
    ))
    .expect("send PairingQrCode #1");
    // Wait less than the stall threshold, then send LoggedOut.
    tokio::time::sleep(TEST_STALL / 2).await;
    tx.send(std::sync::Arc::new(
        crate::events::InboundEvent::LoggedOut {
            cause: Some("LoggedOut".to_string()),
            on_connect: true,
            payload: serde_json::Value::Null,
            ts_unix_ms: 0,
            ts_mono_ns: 0,
        },
    ))
    .expect("send LoggedOut");
    tokio::time::sleep(TEST_STALL / 2).await;

    assert_eq!(
        handle.bot_state(),
        BotStateMirror::LoggedOut,
        "after server-side rejection, state must be LoggedOut"
    );

    // Re-pair: new PairingQrCode. The timer must restart from
    // scratch; without a fresh timeout window it would NOT fire
    // before the test's total elapsed time.
    tx.send(std::sync::Arc::new(
        crate::events::InboundEvent::PairingQrCode {
            qr_code: "b".to_string(),
            ref_string: String::new(),
            timeout: 60,
            ts_unix_ms: 0,
            ts_mono_ns: 0,
        },
    ))
    .expect("send PairingQrCode #2");

    // Wait > TEST_STALL so the timer fires for the SECOND pair.
    tokio::time::sleep(TEST_STALL * 3).await;

    assert_eq!(
        handle.bot_state(),
        BotStateMirror::AwaitingUserAction,
        "stall timer must reset on re-pair; second PairingQrCode with no terminal must trigger AwaitingUserAction"
    );

    handle.cancel_token().cancel();
}

#[tokio::test(flavor = "multi_thread")]
async fn pairing_stall_does_not_fire_without_pairing_prompt() {
    let (tx, handle, _tmp) = spawn_watcher().await;

    // Send Connected directly with no prior PairingQrCode. The
    // stall timer is only armed by PairingQr/Code events, so no
    // timer should fire.
    tx.send(std::sync::Arc::new(
        crate::events::InboundEvent::Connected {
            ts_unix_ms: 0,
            ts_mono_ns: 0,
        },
    ))
    .expect("send Connected");

    tokio::time::sleep(TEST_STALL * 3).await;

    assert_eq!(
        handle.bot_state(),
        BotStateMirror::Connected,
        "no pairing prompt -> no stall timer -> Connected sticks"
    );

    handle.cancel_token().cancel();
}

// ── SHORTCAKE_PASSKEY classifier arms (Session 4 of wacore-webauthn
//    plan, RFC-0909) ───────────────────────────────────────────────
//
// The server sends three event variants during a SHORTCAKE_PASSKEY
// link flow. Each must transition the BotState mirror to the right
// value so `status.get` reflects what the operator sees on the
// phone. The Debug format used here mirrors the upstream wacore
// `Event` enum's auto-derived `Debug` (escape characters preserved
// as `\"` in the stringified JSON field).

#[tokio::test(flavor = "multi_thread")]
async fn pair_passkey_request_event_marks_awaiting_passkey() {
    let (tx, handle, _tmp) = spawn_watcher().await;

    // The wacore `Event::PairPasskeyRequest` typed payload carries
    // `request_options_json` only (no `auth` field). Pass a
    // plausible WebAuthn challenge shape so the classifier matches
    // on the `event_kind()` label.
    tx.send(std::sync::Arc::new(
        crate::events::InboundEvent::PairPasskeyRequest {
            auth: String::new(),
            request_json: r#"{"challenge":"abc","rpId":"web.whatsapp.com"}"#.to_string(),
            ts_unix_ms: 0,
            ts_mono_ns: 0,
        },
    ))
    .expect("send PairPasskeyRequest");

    // Brief yield — the watcher's recv loop runs in the same tokio
    // runtime; 10ms is enough for the spawn-and-receive cycle on
    // every CI box we've tuned for.
    tokio::time::sleep(TEST_STALL / 2).await;

    assert_eq!(
        handle.bot_state(),
        BotStateMirror::AwaitingPasskey,
        "PairPasskeyRequest must transition BotStateMirror to AwaitingPasskey"
    );

    // Surface via status.get: handler reads BotStateMirror and
    // returns the matching label + hint.
    let status = StatusGet
        .call(handle.clone(), serde_json::Value::Null)
        .await
        .expect("status.get");
    assert_eq!(status["bot_state"], "AwaitingPasskey");
    let hint = status["bot_state_hint"]
        .as_str()
        .expect("bot_state_hint must be a string");
    assert!(
        hint.contains("SHORTCAKE_PASSKEY"),
        "hint must mention SHORTCAKE_PASSKEY; got {hint:?}"
    );

    handle.cancel_token().cancel();
}

#[tokio::test(flavor = "multi_thread")]
async fn pair_passkey_confirmation_event_keeps_awaiting_passkey() {
    let (tx, handle, _tmp) = spawn_watcher().await;

    tx.send(std::sync::Arc::new(
        crate::events::InboundEvent::PairPasskeyConfirmation {
            auth: String::new(),
            confirmation_json: r#"{"code":"ABCD1234","skip_handoff_ux":false}"#.to_string(),
            ts_unix_ms: 0,
            ts_mono_ns: 0,
        },
    ))
    .expect("send PairPasskeyConfirmation");

    tokio::time::sleep(TEST_STALL / 2).await;

    assert_eq!(
        handle.bot_state(),
        BotStateMirror::AwaitingPasskey,
        "PairPasskeyConfirmation must keep BotStateMirror at AwaitingPasskey (still waiting for phone-side handoff)"
    );

    handle.cancel_token().cancel();
}

#[tokio::test(flavor = "multi_thread")]
async fn pair_passkey_error_event_advances_to_logged_out() {
    let (tx, handle, _tmp) = spawn_watcher().await;

    tx.send(std::sync::Arc::new(
        crate::events::InboundEvent::PairPasskeyError {
            auth: String::new(),
            error_json: "user_cancelled".to_string(),
            ts_unix_ms: 0,
            ts_mono_ns: 0,
        },
    ))
    .expect("send PairPasskeyError");

    tokio::time::sleep(TEST_STALL / 2).await;

    assert_eq!(
        handle.bot_state(),
        BotStateMirror::LoggedOut,
        "PairPasskeyError is terminal: BotStateMirror must advance to LoggedOut"
    );
    // Phase also flips — the classify_event arm returned
    // `phase_changed = true`. Daemon is no longer in Booting.
    assert_ne!(
        handle.phase(),
        DaemonPhase::Booting,
        "PairPasskeyError must move DaemonPhase out of Booting (terminal session-lost)"
    );

    handle.cancel_token().cancel();
}

#[tokio::test(flavor = "multi_thread")]
async fn awaiting_passkey_clears_pairing_stall_timer() {
    // Regression guard: if the pairing stall timer fires AFTER a
    // PairPasskeyRequest, it would overwrite AwaitingPasskey with
    // AwaitingUserAction. The watcher's `pairing_started_at`
    // management must clear the timer when AwaitingPasskey is set.
    let (tx, handle, _tmp) = spawn_watcher().await;

    // Pairing prompt arms the timer.
    tx.send(std::sync::Arc::new(
        crate::events::InboundEvent::PairingQrCode {
            qr_code: "x".to_string(),
            ref_string: String::new(),
            timeout: 60,
            ts_unix_ms: 0,
            ts_mono_ns: 0,
        },
    ))
    .expect("send PairingQrCode");
    // Server asks for passkey (well within the stall window).
    tokio::time::sleep(TEST_STALL / 4).await;
    tx.send(std::sync::Arc::new(
        crate::events::InboundEvent::PairPasskeyRequest {
            auth: String::new(),
            request_json: r#"{"challenge":"abc"}"#.to_string(),
            ts_unix_ms: 0,
            ts_mono_ns: 0,
        },
    ))
    .expect("send PairPasskeyRequest");

    // Wait well past the stall window. If the timer had not been
    // cleared, it would fire here and flip state to
    // AwaitingUserAction.
    tokio::time::sleep(TEST_STALL * 3).await;

    assert_eq!(
        handle.bot_state(),
        BotStateMirror::AwaitingPasskey,
        "AwaitingPasskey must not be clobbered by the pairing stall timer"
    );

    handle.cancel_token().cancel();
}

// Helper trait used by the SHORTCAKE_PASSKEY hermetic tests to
// render a `status.get` Value from a handle. Defined here (not
// in `ipc/handlers/status.rs`) because the tests own the call
// site to avoid a public-API addition for one test helper.
use crate::ipc::handlers::status::StatusGet;
use crate::ipc::server::RpcHandler;
