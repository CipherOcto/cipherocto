//! Smoke test that the crate compiles and exposes the public API.
//! Mission AC line 125: "crates/octo-adapter-telegram/ crate compiles to cdylib and rlib with default features"

use octo_adapter_telegram::{MockTelegramClient, TelegramAdapter, TelegramClient, TelegramConfig};

#[test]
fn test_crate_exports_required_types() {
    // These are compile-time assertions: if the types don't exist or aren't public,
    // the test file won't compile.
    let _: Option<TelegramConfig> = None;
    let _: Option<TelegramAdapter<MockTelegramClient>> = None;
    let _: Option<Box<dyn TelegramClient>> = None;
}

#[test]
fn test_default_config_is_valid() {
    let config = TelegramConfig::default();
    // 0850f preserved default: groups is empty Vec
    assert!(config.groups.is_empty());
    // webhook_port is None (uses long-polling/TDLib push)
    assert!(config.webhook_port.is_none());
}
