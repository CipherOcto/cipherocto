//! Integration tests for DOT envelope pipeline.
//!
//! Tests the full envelope lifecycle:
//! creation → wire serialization → deserialization → canonicalization

mod common;

use common::mock_adapter::MockPlatformAdapter;
use common::mock_network::MockNetwork;
use octo_network::dot::adapters::PlatformAdapter;
use octo_network::dot::domain::PlatformType;
use octo_network::dot::envelope::DeterministicEnvelope;

#[tokio::test]
async fn test_envelope_roundtrip_through_mock_adapter() {
    let adapter = MockPlatformAdapter::new(PlatformType::NativeP2P);
    let envelope = MockNetwork::make_envelope([0xAA; 32], 1, [0x01; 32], 1000);

    let wire = envelope.to_wire_bytes();
    assert!(!wire.is_empty());

    adapter.inject_message(wire.clone()).await;

    let domain = adapter.domain_id("test");
    let messages = adapter.receive_messages(&domain).await.unwrap();
    assert_eq!(messages.len(), 1);

    let recovered = adapter.canonicalize(&messages[0]).unwrap();
    assert_eq!(recovered.envelope_id, envelope.envelope_id);
    assert_eq!(recovered.network_id, envelope.network_id);
}

#[tokio::test]
async fn test_envelope_send_through_mock_adapter() {
    let adapter = MockPlatformAdapter::new(PlatformType::NativeP2P);
    let envelope = MockNetwork::make_envelope([0xBB; 32], 1, [0x02; 32], 2000);

    let domain = adapter.domain_id("test");
    let receipt = adapter
        .send_message(&domain, &envelope, b"test")
        .await
        .unwrap();
    assert!(!receipt.platform_message_id.is_empty());
    assert_eq!(adapter.outbound_count().await, 1);

    let outbound = adapter.outbound_messages().await;
    let recovered = DeterministicEnvelope::from_wire_bytes(&outbound[0]).unwrap();
    assert_eq!(recovered.envelope_id, envelope.envelope_id);
}

#[tokio::test]
async fn test_multi_adapter_deterministic() {
    let adapter1 = MockPlatformAdapter::new(PlatformType::NativeP2P);
    let adapter2 = MockPlatformAdapter::new(PlatformType::Quic);
    let envelope = MockNetwork::make_envelope([0xCC; 32], 1, [0x03; 32], 3000);

    let domain1 = adapter1.domain_id("test");
    let domain2 = adapter2.domain_id("test");
    adapter1
        .send_message(&domain1, &envelope, b"")
        .await
        .unwrap();
    adapter2
        .send_message(&domain2, &envelope, b"")
        .await
        .unwrap();

    let bytes1 = adapter1.outbound_messages().await;
    let bytes2 = adapter2.outbound_messages().await;
    assert_eq!(bytes1[0], bytes2[0]);
}

#[tokio::test]
async fn test_envelope_empty_payload_rejected() {
    let adapter = MockPlatformAdapter::new(PlatformType::NativeP2P);
    adapter.inject_message(vec![]).await;
    let domain = adapter.domain_id("test");
    let messages = adapter.receive_messages(&domain).await.unwrap();
    assert!(adapter.canonicalize(&messages[0]).is_err());
}

#[tokio::test]
async fn test_self_handle_prevents_relay_loop() {
    let adapter =
        MockPlatformAdapter::new(PlatformType::NativeP2P).with_self_handle("bot:12345".to_string());
    assert_eq!(adapter.self_handle(), Some("bot:12345".to_string()));
}
