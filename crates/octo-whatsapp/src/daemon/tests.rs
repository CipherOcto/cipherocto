use super::*;

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
