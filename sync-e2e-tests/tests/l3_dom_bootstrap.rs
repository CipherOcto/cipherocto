//! L3 E2E tests for DotDomain Bootstrap Mode (RFC-0851p-b)
//!
//! Tests the `dom_bootstrap` module: `DcTrustLevel`, `BroadcastDomainHint`,
//! `DotDomainBootstrapConfig`, `dotdomain_bootstrap()` algorithm,
//! `PlatformAdapterDotDomain` trait, and error paths.

use octo_network::dot::adapters::{
    CapabilityReport, DeliveryReceipt, PlatformAdapter, RawPlatformMessage,
};
use octo_network::dot::envelope::DeterministicEnvelope;
use octo_network::dot::error::PlatformAdapterError;
use octo_network::dot::BroadcastDomainId;
use octo_network::dot::PlatformType;
use octo_transport::dom_bootstrap::{
    dotdomain_bootstrap, BroadcastDomainHint, DcTrustLevel, DotDomainBootstrapConfig,
    DotDomainError, PlatformAdapterDotDomain, GADV_REQ_SUBTYPE, MAX_ATTEST_AGE_EPOCHS,
};
use std::time::Duration;

// ── Mock adapter ─────────────────────────────────────────────────

struct MockDomainAdapter {
    join_ok: bool,
    send_gadv_ok: bool,
    attest_response: Option<Vec<u8>>,
    gadv_responses: Vec<Vec<u8>>,
}

impl MockDomainAdapter {
    fn successful(gadv_count: usize) -> Self {
        Self {
            join_ok: true,
            send_gadv_ok: true,
            attest_response: Some(vec![0u8; 64]),
            gadv_responses: (0..gadv_count).map(|i| vec![i as u8; 128]).collect(),
        }
    }

    fn no_attestation() -> Self {
        Self {
            join_ok: true,
            send_gadv_ok: true,
            attest_response: None,
            gadv_responses: vec![vec![0u8; 128]],
        }
    }

    fn join_fails() -> Self {
        Self {
            join_ok: false,
            send_gadv_ok: true,
            attest_response: Some(vec![0u8; 64]),
            gadv_responses: vec![],
        }
    }

    fn gadv_send_fails() -> Self {
        Self {
            join_ok: true,
            send_gadv_ok: false,
            attest_response: Some(vec![0u8; 64]),
            gadv_responses: vec![],
        }
    }

    fn no_gadv() -> Self {
        Self {
            join_ok: true,
            send_gadv_ok: true,
            attest_response: Some(vec![0u8; 64]),
            gadv_responses: vec![],
        }
    }

    fn small_attestation() -> Self {
        Self {
            join_ok: true,
            send_gadv_ok: true,
            attest_response: Some(vec![0u8; 16]), // < 32 bytes → invalid
            gadv_responses: vec![vec![0u8; 128]],
        }
    }
}

#[async_trait::async_trait]
impl PlatformAdapter for MockDomainAdapter {
    async fn send_envelope(
        &self,
        _domain: &BroadcastDomainId,
        _envelope: &DeterministicEnvelope,
    ) -> Result<DeliveryReceipt, PlatformAdapterError> {
        Ok(DeliveryReceipt {
            platform_message_id: "mock".to_string(),
            delivered_at: 0,
        })
    }

    async fn receive_messages(
        &self,
        _domain: &BroadcastDomainId,
    ) -> Result<Vec<RawPlatformMessage>, PlatformAdapterError> {
        Ok(vec![])
    }

    fn canonicalize(
        &self,
        _raw: &RawPlatformMessage,
    ) -> Result<DeterministicEnvelope, PlatformAdapterError> {
        Ok(DeterministicEnvelope::default())
    }

    fn capabilities(&self) -> CapabilityReport {
        CapabilityReport {
            max_payload_bytes: 4096,
            supports_fragmentation: false,
            supports_encryption: false,
            supports_raw_binary: true,
            rate_limit_per_second: 100,
            media_capabilities: None,
            ..Default::default()
        }
    }

    fn domain_id(&self, platform_id: &str) -> BroadcastDomainId {
        BroadcastDomainId::new(PlatformType::Telegram, platform_id)
    }

    fn platform_type(&self) -> PlatformType {
        PlatformType::Telegram
    }
}

#[async_trait::async_trait]
impl PlatformAdapterDotDomain for MockDomainAdapter {
    async fn join_domain(&self, _domain_ref: &str) -> Result<(), PlatformAdapterError> {
        if self.join_ok {
            Ok(())
        } else {
            Err(PlatformAdapterError::Unreachable {
                platform: "mock".to_string(),
                reason: "join failed".to_string(),
            })
        }
    }

    async fn send_gadv_request(&self, _domain_ref: &str) -> Result<(), PlatformAdapterError> {
        if self.send_gadv_ok {
            Ok(())
        } else {
            Err(PlatformAdapterError::Unreachable {
                platform: "mock".to_string(),
                reason: "gadv send failed".to_string(),
            })
        }
    }

    async fn receive_attestation(
        &self,
        _timeout: Duration,
    ) -> Result<Option<Vec<u8>>, PlatformAdapterError> {
        Ok(self.attest_response.clone())
    }

    async fn receive_gadv_responses(
        &self,
        _timeout: Duration,
        _max_count: usize,
    ) -> Result<Vec<Vec<u8>>, PlatformAdapterError> {
        // Return ALL responses — the algorithm enforces the per-domain cap
        Ok(self.gadv_responses.clone())
    }
}

// ── D01-D10: DcTrustLevel tests ─────────────────────────────────

#[test]
fn d01_dc_trust_level_all_lifecycle_states() {
    // All 8 RFC-0855p-b states
    assert_eq!(
        DcTrustLevel::from_lifecycle_byte(0x00),
        DcTrustLevel::Provisional
    ); // Designated
    assert_eq!(
        DcTrustLevel::from_lifecycle_byte(0x01),
        DcTrustLevel::Provisional
    ); // Elected
    assert_eq!(
        DcTrustLevel::from_lifecycle_byte(0x02),
        DcTrustLevel::Trusted
    ); // Active
    assert_eq!(
        DcTrustLevel::from_lifecycle_byte(0x03),
        DcTrustLevel::Degraded
    ); // Suspect
    assert_eq!(
        DcTrustLevel::from_lifecycle_byte(0x04),
        DcTrustLevel::Blocked
    ); // Handover
    assert_eq!(
        DcTrustLevel::from_lifecycle_byte(0x05),
        DcTrustLevel::Untrusted
    ); // Demoting
    assert_eq!(
        DcTrustLevel::from_lifecycle_byte(0x06),
        DcTrustLevel::Untrusted
    ); // Resigned
    assert_eq!(
        DcTrustLevel::from_lifecycle_byte(0x07),
        DcTrustLevel::Untrusted
    ); // Inactive
}

#[test]
fn d02_dc_trust_level_unknown_state_is_untrusted() {
    assert_eq!(
        DcTrustLevel::from_lifecycle_byte(0x08),
        DcTrustLevel::Untrusted
    );
    assert_eq!(
        DcTrustLevel::from_lifecycle_byte(0xFF),
        DcTrustLevel::Untrusted
    );
    assert_eq!(
        DcTrustLevel::from_lifecycle_byte(0xFE),
        DcTrustLevel::Untrusted
    );
}

#[test]
fn d03_dc_trust_level_ordering() {
    assert!(DcTrustLevel::Trusted < DcTrustLevel::Provisional);
    assert!(DcTrustLevel::Provisional < DcTrustLevel::Degraded);
    assert!(DcTrustLevel::Degraded < DcTrustLevel::Blocked);
    assert!(DcTrustLevel::Blocked < DcTrustLevel::Untrusted);
}

#[test]
fn d04_dc_trust_level_allows_bootstrap() {
    assert!(DcTrustLevel::Trusted.allows_bootstrap());
    assert!(DcTrustLevel::Provisional.allows_bootstrap());
    assert!(DcTrustLevel::Degraded.allows_bootstrap());
    assert!(!DcTrustLevel::Blocked.allows_bootstrap());
    assert!(!DcTrustLevel::Untrusted.allows_bootstrap());
}

#[test]
fn d05_dc_trust_level_allows_send() {
    assert!(DcTrustLevel::Trusted.allows_send());
    assert!(DcTrustLevel::Provisional.allows_send());
    assert!(!DcTrustLevel::Degraded.allows_send());
    assert!(!DcTrustLevel::Blocked.allows_send());
    assert!(!DcTrustLevel::Untrusted.allows_send());
}

// ── D06-D10: BroadcastDomainHint tests ──────────────────────────

#[test]
fn d06_broadcast_domain_hint_builder() {
    let hint = BroadcastDomainHint::new(PlatformType::Telegram, "-1001234567890")
        .with_mission([0x42u8; 32])
        .with_dc([0xAAu8; 32]);

    assert_eq!(hint.platform, PlatformType::Telegram);
    assert_eq!(hint.domain_ref, "-1001234567890");
    assert_eq!(hint.expected_mission_id, Some([0x42u8; 32]));
    assert_eq!(hint.expected_dc_id, Some([0xAAu8; 32]));
}

#[test]
fn d07_broadcast_domain_hint_minimal() {
    let hint = BroadcastDomainHint::new(PlatformType::Matrix, "!room:example.com");
    assert_eq!(hint.platform, PlatformType::Matrix);
    assert_eq!(hint.expected_mission_id, None);
    assert_eq!(hint.expected_dc_id, None);
}

#[test]
fn d08_broadcast_domain_hint_all_platforms() {
    for (platform, domain) in [
        (PlatformType::Telegram, "-100123"),
        (PlatformType::Discord, "channel:123"),
        (PlatformType::Matrix, "!room:server"),
        (PlatformType::IRC, "#channel@server"),
    ] {
        let hint = BroadcastDomainHint::new(platform, domain);
        assert_eq!(hint.platform, platform);
        assert_eq!(hint.domain_ref, domain);
    }
}

#[test]
fn d09_config_defaults() {
    let config = DotDomainBootstrapConfig::default();
    assert_eq!(config.discovery_timeout, Duration::from_secs(10));
    assert_eq!(config.min_gadv_responses, 1);
    assert!(config.require_dc_attestation);
    assert_eq!(config.max_peers_per_domain, 64);
}

#[test]
fn d10_constants_values() {
    assert_eq!(MAX_ATTEST_AGE_EPOCHS, 100);
    assert_eq!(GADV_REQ_SUBTYPE, *b"GDRQ");
}

// ── D11-D20: dotdomain_bootstrap algorithm tests ────────────────

#[tokio::test]
async fn d11_successful_bootstrap() {
    let adapter = MockDomainAdapter::successful(3);
    let config = DotDomainBootstrapConfig {
        domain_hint: BroadcastDomainHint::new(PlatformType::Telegram, "-1001234567890")
            .with_mission([0x42u8; 32]),
        min_gadv_responses: 1,
        ..Default::default()
    };

    let result = dotdomain_bootstrap(&config, &adapter, 50).await.unwrap();
    assert_eq!(result.peers_discovered, 3);
    assert!(result.high_confidence);
    assert!(result.dc_attestation.is_some());
    assert_eq!(result.bound_mission_id, Some([0x42u8; 32]));
}

#[tokio::test]
async fn d12_attestation_timeout() {
    let adapter = MockDomainAdapter::no_attestation();
    let config = DotDomainBootstrapConfig {
        require_dc_attestation: true,
        ..Default::default()
    };

    let result = dotdomain_bootstrap(&config, &adapter, 50).await;
    assert!(matches!(result, Err(DotDomainError::DcAttestationTimeout)));
}

#[tokio::test]
async fn d13_join_fails() {
    let adapter = MockDomainAdapter::join_fails();
    let config = DotDomainBootstrapConfig::default();

    let result = dotdomain_bootstrap(&config, &adapter, 50).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn d14_gadv_send_fails() {
    let adapter = MockDomainAdapter::gadv_send_fails();
    let config = DotDomainBootstrapConfig::default();

    let result = dotdomain_bootstrap(&config, &adapter, 50).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn d15_no_gadv_responses() {
    let adapter = MockDomainAdapter::no_gadv();
    let config = DotDomainBootstrapConfig::default();

    let result = dotdomain_bootstrap(&config, &adapter, 50).await;
    assert!(matches!(result, Err(DotDomainError::GadvTimeout { .. })));
}

#[tokio::test]
async fn d16_degraded_no_attestation() {
    let adapter = MockDomainAdapter::successful(2);
    let config = DotDomainBootstrapConfig {
        require_dc_attestation: false,
        min_gadv_responses: 1,
        ..Default::default()
    };

    let result = dotdomain_bootstrap(&config, &adapter, 50).await.unwrap();
    assert_eq!(result.peers_discovered, 2);
    assert!(!result.high_confidence);
    assert!(result.dc_attestation.is_none());
}

#[tokio::test]
async fn d17_per_domain_peer_cap() {
    let adapter = MockDomainAdapter::successful(100);
    let config = DotDomainBootstrapConfig {
        max_peers_per_domain: 5,
        min_gadv_responses: 1,
        ..Default::default()
    };

    let result = dotdomain_bootstrap(&config, &adapter, 50).await.unwrap();
    assert_eq!(result.peers_discovered, 5);
    assert_eq!(result.rejected_peers.len(), 95);
    assert!(result.rejected_peers.iter().all(|r| matches!(
        r.reason,
        octo_transport::dom_bootstrap::RejectionReason::DomainPeerCapExceeded
    )));
}

#[tokio::test]
async fn d18_small_attestation_rejected() {
    let adapter = MockDomainAdapter::small_attestation();
    let config = DotDomainBootstrapConfig {
        require_dc_attestation: true,
        ..Default::default()
    };

    let result = dotdomain_bootstrap(&config, &adapter, 50).await;
    assert!(matches!(result, Err(DotDomainError::DcAttestationInvalid)));
}

#[tokio::test]
async fn d19_high_confidence_requires_attestation_and_min_responses() {
    let adapter = MockDomainAdapter::successful(3);
    let config = DotDomainBootstrapConfig {
        require_dc_attestation: true,
        min_gadv_responses: 3,
        ..Default::default()
    };

    let result = dotdomain_bootstrap(&config, &adapter, 50).await.unwrap();
    assert!(result.high_confidence);
    assert_eq!(result.peers_discovered, 3);
}

#[tokio::test]
async fn d20_low_confidence_below_min_responses() {
    let adapter = MockDomainAdapter::successful(1);
    let config = DotDomainBootstrapConfig {
        require_dc_attestation: true,
        min_gadv_responses: 5, // need 5, got 1
        ..Default::default()
    };

    let result = dotdomain_bootstrap(&config, &adapter, 50).await.unwrap();
    assert!(!result.high_confidence); // DC attested but below min
    assert_eq!(result.peers_discovered, 1);
}
