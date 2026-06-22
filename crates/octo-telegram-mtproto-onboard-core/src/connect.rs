//! Production wiring: connect a real MTProto Telegram adapter.
//!
//! This module is the **only** way to obtain a `MtprotoTelegramAdapter`
//! in production code. It delegates to
//! `octo_adapter_telegram_mtproto::factory::connect_real`, which:
//!
//! 1. Validates the config (rejecting bot-mode configs without
//!    a `bot_token`, user-mode configs without a `phone`, etc.).
//! 2. Opens (or creates) the on-disk `StoolapSession` at
//!    `<data_dir>/session.db`.
//! 3. Spawns the `SenderPool` runner task that drives the
//!    `grammers_client::Client` background networking.
//! 4. Wraps the resulting `RealTelegramMtprotoClient` in a
//!    `MtprotoTelegramAdapter`.
//!
//! ## What is **not** in this module
//!
//! - No `MockTelegramMtprotoClient`. Production code MUST NOT
//!   be able to construct a mock-backed adapter — the
//!   project rule "no mocks in production code paths" is
//!   enforced structurally: the mock is `#[cfg(test)]`-only
//!   inside the adapter crate, and the `test-mock` feature
//!   in this crate is reserved for tests.
//! - No stub-mode or "fake client" fallback. If the real
//!   client fails to connect (network down, DNS failure, TLS
//!   handshake failure), the error is propagated as
//!   `OnboardError::Network` so the operator gets a clear
//!   "no internet" / "firewall blocking Telegram" message
//!   instead of a misleading success.
//!
//! ## Feature gate
//!
//! The whole module is gated on `real-network` because the
//! `RealTelegramMtprotoClient` it references is gated on the
//! same feature in the adapter crate. A test-only build
//! (`--features test-mock --no-default-features`) cannot see
//! this module — tests must use `crate::test_helpers` instead.

#![cfg(feature = "real-network")]

use std::sync::Arc;

use octo_adapter_telegram_mtproto::{
    factory::connect_real, MtprotoTelegramAdapter, RealTelegramMtprotoClient,
};

use crate::error::OnboardError;

/// Concrete production adapter type: an adapter backed by a
/// real `RealTelegramMtprotoClient`. Callers that want to
/// invoke the lifecycle methods directly (e.g. for whoami)
/// can name the type explicitly.
pub type RealMtprotoTelegramAdapter = MtprotoTelegramAdapter<RealTelegramMtprotoClient>;

/// Connect a production MTProto Telegram adapter against
/// Telegram. Performs the initial `initConnection` handshake
/// but does NOT sign in — the caller then chooses the auth
/// mode (`bot_token`, `user_code`, or `qr_login`) and calls
/// the corresponding flow's `run` function.
///
/// On success returns an `Arc<RealMtprotoTelegramAdapter>`
/// ready to drive `bot_token::run`, `user_code::run`, or
/// `qr_login::run`.
///
/// ### Errors
///
/// All errors are mapped to `OnboardError`:
/// - `OnboardError::Config` for invalid configs (missing
///   `api_id`, missing `bot_token` in bot mode, etc.)
/// - `OnboardError::Network` for transport-level failures
///   (TCP / TLS / DNS).
/// - `OnboardError::Adapter` for any other adapter-side
///   failure (session-store issues, etc.).
pub async fn connect(
    cfg: octo_adapter_telegram_mtproto::MtprotoTelegramConfig,
) -> Result<Arc<RealMtprotoTelegramAdapter>, OnboardError> {
    connect_real(cfg)
        .await
        .map(Arc::new)
        .map_err(map_adapter_error)
}

/// Map a `MtprotoTelegramError` returned by the factory into
/// the most specific `OnboardError` variant. Mirrors the
/// per-flow `map_adapter_error` helpers in `bot_token.rs`,
/// `user_code.rs`, and `qr_login.rs` so the connect path
/// never silently drops error context.
fn map_adapter_error(err: octo_adapter_telegram_mtproto::MtprotoTelegramError) -> OnboardError {
    use octo_adapter_telegram_mtproto::MtprotoTelegramError as E;
    match err {
        E::Config(_) => OnboardError::Config(err.to_string()),
        E::Auth(_) => OnboardError::TelegramApi(err.to_string()),
        E::Rpc { .. } => OnboardError::TelegramApi(err.to_string()),
        E::RateLimited { .. } => OnboardError::TelegramApi(err.to_string()),
        E::Session(_) => OnboardError::Adapter(err.to_string()),
        E::Network(_) => OnboardError::Network(err.to_string()),
        E::Capability(_) => OnboardError::Adapter(err.to_string()),
        E::NotReady(_) => OnboardError::NotReady {
            // Connect time = no last-observed state yet. Use
            // the error message as a stand-in for diagnostics.
            last_state: format!("connect: {}", err),
        },
        E::Envelope(_) => OnboardError::Adapter(err.to_string()),
        E::Internal(_) => OnboardError::Adapter(err.to_string()),
        E::QrLoginHandle { .. } => OnboardError::Adapter(err.to_string()),
        // Forward-compatible: any future variants land here.
        other => OnboardError::Adapter(other.to_string()),
    }
}
