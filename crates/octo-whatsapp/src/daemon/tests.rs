use super::*;
use crate::test_mock_adapter::MockAdapter;
use std::sync::Arc;

#[tokio::test(flavor = "multi_thread")]
async fn handle_phase_starts_booting() {
    let cfg = WhatsAppRuntimeConfig::from_toml(br#"name = "x""#).unwrap();
    let d = Daemon::new(cfg);
    let h = d.handle();
    assert_eq!(h.phase(), DaemonPhase::Booting);
}

#[tokio::test(flavor = "multi_thread")]
async fn cancel_token_is_linked() {
    let cfg = WhatsAppRuntimeConfig::from_toml(br#"name = "x""#).unwrap();
    let d = Daemon::new(cfg);
    let h = d.handle();
    assert!(!h.cancel_token().is_cancelled());
    d.cancel_token().cancel();
    assert!(h.cancel_token().is_cancelled());
}

#[tokio::test(flavor = "multi_thread")]
async fn bind_adapter_stores_adapter() {
    let cfg = WhatsAppRuntimeConfig {
        name: "test-bind".into(),
        ..Default::default()
    };
    let daemon = Daemon::new(cfg);
    let handle = daemon.handle();
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
    let cfg = WhatsAppRuntimeConfig {
        name: "test-bind-idem".into(),
        ..Default::default()
    };
    let daemon = Daemon::new(cfg);
    let handle = daemon.handle();
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
    let cfg = WhatsAppRuntimeConfig {
        name: "test-acct-init".into(),
        ..Default::default()
    };
    let daemon = Daemon::new(cfg);
    let _entries = daemon.handle().accounts().list();
    // No panic → store opened successfully (or fell back to `None`; both
    // cases yield an empty Vec via the guard's `unwrap_or_default()`);
}

#[tokio::test(flavor = "multi_thread")]
async fn rebind_adapter_for_replaces_slot_with_new_adapter() {
    let cfg = crate::config::WhatsAppRuntimeConfig {
        name: "test-rebind".into(),
        ..Default::default()
    };
    let daemon = Daemon::new(cfg);
    let handle = daemon.handle();

    let adapter_a = std::sync::Arc::new(crate::test_mock_adapter::MockAdapter::new());
    handle.bind_adapter(adapter_a.clone());
    assert!(handle.adapter().is_some(), "first bind must populate slot");

    let tmp = tempfile::tempdir().expect("tempdir");
    let new_session = tmp.path().join("account-b.session.db");
    handle.rebind_adapter_for("account-b", &new_session);

    assert!(
        handle.adapter().is_some(),
        "slot must remain populated after rebind"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn rebind_adapter_for_works_when_no_adapter_bound_yet() {
    let cfg = crate::config::WhatsAppRuntimeConfig {
        name: "test-rebind-empty".into(),
        ..Default::default()
    };
    let daemon = Daemon::new(cfg);
    let handle = daemon.handle();
    assert!(handle.adapter().is_none(), "slot starts empty");

    let tmp = tempfile::tempdir().expect("tempdir");
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
