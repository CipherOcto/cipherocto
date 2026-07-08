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

/// Build a `broadcast::Sender<String>` + paired receiver and spawn
/// `run_connection_watcher_inner` against a fresh daemon. Returns
/// the sender (caller drives events) and the daemon handle (caller
/// asserts state).
async fn spawn_watcher() -> (
    tokio::sync::broadcast::Sender<String>,
    DaemonHandle,
    tempfile::TempDir,
) {
    let tmp = tempfile::tempdir().expect("tempdir");
    let (_daemon, handle) = Daemon::new_for_tests(tmp.path());
    let (tx, rx) = tokio::sync::broadcast::channel::<String>(64);
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

    // Send a PairingQr-classified event (the wacore Debug form
    // starts with `Event::PairingQrCode(...)` per the classifier;
    // we use the inner identifier after `Event::` stripping).
    tx.send("Event::PairingQrCode { code: \"x\", timeout: 60s }".to_string())
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

    tx.send("Event::PairingQrCode { code: \"x\", timeout: 60s }".to_string())
        .expect("send PairingQrCode");
    // Send Connected before the stall timer fires.
    tx.send("Event::Connected(Connected { .. })".to_string())
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
    tx.send("Event::PairingQrCode { code: \"a\", timeout: 60s }".to_string())
        .expect("send PairingQrCode #1");
    // Wait less than the stall threshold, then send LoggedOut.
    tokio::time::sleep(TEST_STALL / 2).await;
    tx.send("LoggedOut(LoggedOut { on_connect: true, reason: LoggedOut })".to_string())
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
    tx.send("Event::PairingQrCode { code: \"b\", timeout: 60s }".to_string())
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
    tx.send("Event::Connected(Connected { .. })".to_string())
        .expect("send Connected");

    tokio::time::sleep(TEST_STALL * 3).await;

    assert_eq!(
        handle.bot_state(),
        BotStateMirror::Connected,
        "no pairing prompt -> no stall timer -> Connected sticks"
    );

    handle.cancel_token().cancel();
}
