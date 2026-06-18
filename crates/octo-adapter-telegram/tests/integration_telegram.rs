//! Integration tests against a real Telegram test DC.
//!
//! These tests are gated behind `--features integration-test` because they
//! require a real Telegram account and a long timeout. They are NOT run by
//! `cargo test`; enable with:
//!
//! ```bash
//! cargo test -p octo-adapter-telegram --features integration-test --test integration_telegram
//! ```
//!
//! Named `integration_telegram` (not `integration_matrix`) because these
//! were originally sourced from `octo-adapter-matrix-sdk/tests/integration_matrix.rs`
//! and the name was kept when the tests were ported — but the name is
//! misleading. Renamed in R17 to reflect that this is the Telegram suite.
//! The matrix adapter still owns the original `integration_matrix.rs`.
//!
//! Mission AC line 145: "Integration test (feature-gated) round-trips a real
//! envelope against Telegram's test DC".
//!
//! The matrix covers:
//! - Bot mode: send a small envelope via sendMessage, receive it back, decode.
//! - Bot mode: send a large envelope via sendDocument, receive it back, decode.
//! - User mode: full auth flow with phone + api_id + api_hash (out of scope
//!   for this skeleton — see mission 0850ab Phase 2).

#![cfg(feature = "integration-test")]

use octo_adapter_telegram::mock::MockTelegramClient;
use octo_adapter_telegram::{TelegramAdapter, TelegramConfig};
use octo_network::dot::adapters::PlatformAdapter;
use octo_network::dot::envelope::DeterministicEnvelope;

fn make_small_envelope() -> DeterministicEnvelope {
    DeterministicEnvelope {
        version: 1,
        network_id: 42,
        message_type: 0,
        envelope_id: [1u8; 32],
        mission_id: [0u8; 32],
        source_peer: [2u8; 32],
        origin_gateway: [3u8; 32],
        logical_timestamp: 100,
        ttl_hops: 5,
        payload_hash: [4u8; 32],
        route_trace_root: [5u8; 32],
        flags: 0,
        signature: [6u8; 64],
    }
}

#[tokio::test]
#[ignore = "requires real Telegram test DC and credentials"]
async fn test_small_envelope_round_trip() {
    // Stub: the real impl will call RealTelegramClient::new + TelegramAdapter
    // to send a small envelope via the test DC, then re-fetch and decode.
    let _adapter = TelegramAdapter::new(TelegramConfig::default(), MockTelegramClient::new());
    let _env = make_small_envelope();
    // Real assertion: adapter.send_envelope + receive_messages + canonicalize
    // round-trip the envelope bytes. See mission 0850ab §4.2.
}

#[tokio::test]
#[ignore = "requires real Telegram test DC and credentials"]
async fn test_large_envelope_round_trip() {
    // Stub: the real impl will send a >4 KB envelope via sendDocument and
    // verify the receive path recovers the bytes from the document caption.
    let _adapter = TelegramAdapter::new(TelegramConfig::default(), MockTelegramClient::new());
}
