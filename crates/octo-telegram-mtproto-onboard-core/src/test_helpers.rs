//! Test helpers — **only** compiled under `#[cfg(test)]`.
//!
//! Production crates (which build with the default features,
//! i.e. `real-network`) cannot see this module. The test-only
//! visibility is structural: tests use the `MockTelegramMtprotoClient`
//! which is gated on the `test-mock` Cargo feature in the
//! adapter crate, and which is **not** re-exported from
//! `connect.rs` (the production wiring module).
//!
//! The single entry point is [`mock_adapter_for_test`], which
//! builds a `MtprotoTelegramAdapter<MockTelegramMtprotoClient>`
//! over a `StoolapSession` rooted in the supplied data dir.
//! All unit tests that previously hand-rolled this in
//! `bot_token.rs`, `user_code.rs`, etc. delegate here.

#![cfg(test)]

use std::path::Path;
use std::sync::Arc;

use octo_adapter_telegram_mtproto::{
    MockTelegramMtprotoClient, MtprotoTelegramAdapter, MtprotoTelegramConfig,
};

/// Build a mock-backed adapter for unit tests.
///
/// * `data_dir` — on-disk location of the session file (and
///   config the flows write). Created if it does not exist.
/// * Returns an `Arc<MtprotoTelegramAdapter<MockTelegramMtprotoClient>>`
///   ready to be passed to `bot_token::run`, `user_code::run`,
///   `qr_login::run`.
///
/// The mock client:
///
/// * accepts any `bot_token` for `connect_bot_token`
/// * accepts any phone + code for `connect_user`
/// * accepts any password for `submit_password`
/// * accepts any QR token for `qr_login` and `poll_qr_login`
/// * resolves a self-handle with `user_id = 1` and `username = "mock_user"`
///
/// All flows reachable from this adapter complete against
/// the mock without a real Telegram DC. The integration tests
/// (gated on `INTEGRATION_TESTS=1` and the `integration-test`
/// feature in the adapter crate) drive the real client.
pub fn mock_adapter_for_test(
    data_dir: &Path,
) -> Arc<MtprotoTelegramAdapter<MockTelegramMtprotoClient>> {
    let cfg = MtprotoTelegramConfig {
        api_id: Some(12345),
        api_hash: Some("fakehash".to_string()),
        data_dir: Some(data_dir.to_path_buf()),
        ..Default::default()
    };
    let client = Arc::new(MockTelegramMtprotoClient::new());
    Arc::new(MtprotoTelegramAdapter::new(cfg, client))
}
