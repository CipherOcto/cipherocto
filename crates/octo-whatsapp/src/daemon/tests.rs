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
