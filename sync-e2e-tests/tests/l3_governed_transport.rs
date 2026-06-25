//! L3 E2E tests for Domain-Governed Transport (RFC-0863p-a)
//!
//! Tests the `governed_transport` module: `GovernedTransport`,
//! `GovernedTransportLifecycle`, `AdapterConfig`, `DomainRole`,
//! `DcLifecycleEvent`, `find_domain_for_platform`, `derive_trust_levels`.

use octo_transport::governed_transport::{
    AdapterConfig, Credentials, DcLifecycleEvent, DomainRole, FLAG_DEGRADED_DOMAIN,
    GovernedTransport, GovernedTransportLifecycle, ReceivedMessage,
    derive_trust_levels, find_domain_for_platform,
};
use octo_transport::dom_bootstrap::{BroadcastDomainHint, DcTrustLevel};
use octo_transport::node_transport::NodeTransport;
use octo_transport::sender::{NetworkSender, SendContext, TransportError};
use async_trait::async_trait;
use std::sync::Arc;

// ── Mock sender ──────────────────────────────────────────────────

struct MockSender;

#[async_trait]
impl NetworkSender for MockSender {
    async fn send(&self, _payload: &[u8], _ctx: &SendContext) -> Result<(), TransportError> {
        Ok(())
    }
    fn name(&self) -> &str {
        "mock"
    }
    fn is_healthy(&self) -> bool {
        true
    }
}

fn make_transport() -> GovernedTransport {
    let inner = NodeTransport::new(vec![Arc::new(MockSender) as Arc<dyn NetworkSender>]);
    GovernedTransport::new(
        inner,
        [0x42u8; 32],
        vec![
            (octo_network::dot::PlatformType::Telegram, "-100".to_string(), DomainRole::Joiner),
        ],
    )
}

fn ctx() -> SendContext {
    SendContext {
        mission_id: [0x42u8; 32],
        priority: 128,
        source_peer: [0xAAu8; 32],
        origin_gateway: [0xBBu8; 32],
    }
}

// ── GT01-GT10: GovernedTransportLifecycle tests ──────────────────

#[test]
fn gt01_lifecycle_empty_trust_is_ready() {
    assert_eq!(
        GovernedTransportLifecycle::from_domain_trust(&[]),
        GovernedTransportLifecycle::Ready
    );
}

#[test]
fn gt02_lifecycle_all_trusted() {
    assert_eq!(
        GovernedTransportLifecycle::from_domain_trust(&[DcTrustLevel::Trusted, DcTrustLevel::Trusted]),
        GovernedTransportLifecycle::Ready
    );
}

#[test]
fn gt03_lifecycle_degraded() {
    assert_eq!(
        GovernedTransportLifecycle::from_domain_trust(&[DcTrustLevel::Trusted, DcTrustLevel::Degraded]),
        GovernedTransportLifecycle::Degraded
    );
}

#[test]
fn gt04_lifecycle_all_untrusted_is_rebooting() {
    assert_eq!(
        GovernedTransportLifecycle::from_domain_trust(&[DcTrustLevel::Untrusted, DcTrustLevel::Untrusted]),
        GovernedTransportLifecycle::Rebooting
    );
}

#[test]
fn gt05_lifecycle_provisional_is_ready() {
    assert_eq!(
        GovernedTransportLifecycle::from_domain_trust(&[DcTrustLevel::Provisional]),
        GovernedTransportLifecycle::Ready
    );
}

#[test]
fn gt06_lifecycle_blocked_is_degraded() {
    assert_eq!(
        GovernedTransportLifecycle::from_domain_trust(&[DcTrustLevel::Trusted, DcTrustLevel::Blocked]),
        GovernedTransportLifecycle::Degraded
    );
}

#[test]
fn gt07_lifecycle_mixed_untrusted_is_degraded() {
    assert_eq!(
        GovernedTransportLifecycle::from_domain_trust(&[DcTrustLevel::Untrusted, DcTrustLevel::Provisional]),
        GovernedTransportLifecycle::Degraded
    );
}

#[test]
fn gt08_lifecycle_single_untrusted_is_rebooting() {
    assert_eq!(
        GovernedTransportLifecycle::from_domain_trust(&[DcTrustLevel::Untrusted]),
        GovernedTransportLifecycle::Rebooting
    );
}

// ── GT09-GT15: GovernedTransport state tests ─────────────────────

#[test]
fn gt09_starts_in_building() {
    let gt = make_transport();
    assert!(!gt.ready());
    assert_eq!(gt.lifecycle(), GovernedTransportLifecycle::Building);
}

#[test]
fn gt10_transitions_to_ready_on_trusted() {
    let mut gt = make_transport();
    gt.update_dc_trust([0xAA; 32], DcTrustLevel::Trusted);
    assert!(gt.ready());
    assert_eq!(gt.lifecycle(), GovernedTransportLifecycle::Ready);
}

#[test]
fn gt11_transitions_to_degraded() {
    let mut gt = make_transport();
    gt.update_dc_trust([0xAA; 32], DcTrustLevel::Degraded);
    assert!(gt.ready());
    assert_eq!(gt.lifecycle(), GovernedTransportLifecycle::Degraded);
}

#[test]
fn gt12_transitions_to_rebooting_on_all_untrusted() {
    let mut gt = make_transport();
    gt.update_dc_trust([0xAA; 32], DcTrustLevel::Untrusted);
    assert!(!gt.ready());
    assert_eq!(gt.lifecycle(), GovernedTransportLifecycle::Rebooting);
}

#[test]
fn gt13_domain_loss_only_reboots_when_all_untrusted() {
    let mut gt = make_transport();
    gt.update_dc_trust([0xAA; 32], DcTrustLevel::Trusted);
    gt.update_dc_trust([0xBB; 32], DcTrustLevel::Trusted);

    // Lose domain BB
    gt.on_dc_lifecycle_event(&DcLifecycleEvent {
        dc_id: [0xBB; 32],
        previous_state: 0x02,
        new_state: 0x05, // Demoting
        epoch: 100,
    });

    // AA still Trusted → Degraded, not Rebooting
    assert_eq!(gt.lifecycle(), GovernedTransportLifecycle::Degraded);
}

#[test]
fn gt14_domain_loss_reboots_when_all_untrusted() {
    let mut gt = make_transport();
    gt.update_dc_trust([0xAA; 32], DcTrustLevel::Trusted);

    gt.on_dc_lifecycle_event(&DcLifecycleEvent {
        dc_id: [0xAA; 32],
        previous_state: 0x02,
        new_state: 0x05, // Demoting
        epoch: 100,
    });

    assert_eq!(gt.lifecycle(), GovernedTransportLifecycle::Rebooting);
}

#[test]
fn gt15_suspect_event_is_degraded() {
    let mut gt = make_transport();
    gt.update_dc_trust([0xAA; 32], DcTrustLevel::Trusted);

    gt.on_dc_lifecycle_event(&DcLifecycleEvent {
        dc_id: [0xAA; 32],
        previous_state: 0x02,
        new_state: 0x03, // Suspect
        epoch: 100,
    });

    assert_eq!(gt.lifecycle(), GovernedTransportLifecycle::Degraded);
}

// ── GT16-GT20: send_best governance tests ────────────────────────

#[tokio::test]
async fn gt16_send_best_while_ready() {
    let mut gt = make_transport();
    gt.update_dc_trust([0xAA; 32], DcTrustLevel::Trusted);
    assert!(gt.send_best(b"hello", &ctx()).await.is_ok());
}

#[tokio::test]
async fn gt17_send_best_while_building() {
    let gt = make_transport();
    // Building state should still allow sends (inner transport handles it)
    assert!(gt.send_best(b"hello", &ctx()).await.is_ok());
}

#[tokio::test]
async fn gt18_send_best_while_rebooting_fails() {
    let mut gt = make_transport();
    gt.update_dc_trust([0xAA; 32], DcTrustLevel::Untrusted);
    assert!(gt.send_best(b"hello", &ctx()).await.is_err());
}

#[tokio::test]
async fn gt19_send_best_while_degraded() {
    let mut gt = make_transport();
    gt.update_dc_trust([0xAA; 32], DcTrustLevel::Degraded);
    assert!(gt.send_best(b"hello", &ctx()).await.is_ok());
}

// ── GT20-GT25: DcLifecycleEvent tests ────────────────────────────

#[test]
fn gt20_domain_loss_detection() {
    let event = DcLifecycleEvent {
        dc_id: [0xAA; 32],
        previous_state: 0x02,
        new_state: 0x05, // Demoting
        epoch: 100,
    };
    assert!(event.is_domain_loss());
    assert_eq!(event.new_trust_level(), DcTrustLevel::Untrusted);
}

#[test]
fn gt21_no_domain_loss_on_active() {
    let event = DcLifecycleEvent {
        dc_id: [0xAA; 32],
        previous_state: 0x01,
        new_state: 0x02, // Active
        epoch: 100,
    };
    assert!(!event.is_domain_loss());
    assert_eq!(event.new_trust_level(), DcTrustLevel::Trusted);
}

#[test]
fn gt22_resigned_is_domain_loss() {
    let event = DcLifecycleEvent {
        dc_id: [0xAA; 32],
        previous_state: 0x02,
        new_state: 0x06, // Resigned
        epoch: 100,
    };
    assert!(event.is_domain_loss());
}

#[test]
fn gt23_inactive_is_domain_loss() {
    let event = DcLifecycleEvent {
        dc_id: [0xAA; 32],
        previous_state: 0x02,
        new_state: 0x07, // Inactive
        epoch: 100,
    };
    assert!(event.is_domain_loss());
}

// ── GT24-GT28: Helper function tests ─────────────────────────────

#[test]
fn gt24_find_domain_hit() {
    let domains = vec![
        (octo_network::dot::PlatformType::Telegram, "-100".to_string(), DomainRole::Joiner),
        (octo_network::dot::PlatformType::Quic, "".to_string(), DomainRole::None),
    ];
    let result = find_domain_for_platform(octo_network::dot::PlatformType::Telegram, &domains);
    assert_eq!(result, Some(("-100".to_string(), DomainRole::Joiner)));
}

#[test]
fn gt25_find_domain_ptp_returns_none() {
    let domains = vec![
        (octo_network::dot::PlatformType::Quic, "".to_string(), DomainRole::None),
    ];
    assert!(find_domain_for_platform(octo_network::dot::PlatformType::Quic, &domains).is_none());
}

#[test]
fn gt26_find_domain_miss() {
    let domains = vec![
        (octo_network::dot::PlatformType::Telegram, "-100".to_string(), DomainRole::Joiner),
    ];
    assert!(find_domain_for_platform(octo_network::dot::PlatformType::Discord, &domains).is_none());
}

#[test]
fn gt27_derive_trust_levels() {
    let levels = derive_trust_levels(&[0x02, 0x03, 0x05, 0x00]);
    assert_eq!(levels, vec![
        DcTrustLevel::Trusted,
        DcTrustLevel::Degraded,
        DcTrustLevel::Untrusted,
        DcTrustLevel::Provisional,
    ]);
}

#[test]
fn gt28_derive_trust_levels_empty() {
    assert!(derive_trust_levels(&[]).is_empty());
}

// ── GT29-GT32: AdapterConfig / DomainRole tests ──────────────────

#[test]
fn gt29_adapter_config_construction() {
    let config = AdapterConfig {
        platform: octo_network::dot::PlatformType::Telegram,
        credentials: Credentials::BotToken("token".to_string()),
        domain_hint: Some(BroadcastDomainHint::new(
            octo_network::dot::PlatformType::Telegram,
            "-100",
        )),
        role: DomainRole::Joiner,
    };
    assert_eq!(config.platform, octo_network::dot::PlatformType::Telegram);
    assert_eq!(config.role, DomainRole::Joiner);
}

#[test]
fn gt30_domain_role_variants() {
    assert_eq!(DomainRole::None, DomainRole::None);
    assert_eq!(DomainRole::Joiner, DomainRole::Joiner);
    assert_eq!(DomainRole::Coordinator, DomainRole::Coordinator);
    assert_eq!(DomainRole::SubAdmin, DomainRole::SubAdmin);
    assert_ne!(DomainRole::None, DomainRole::Joiner);
}

#[test]
fn gt31_credentials_variants() {
    let c1 = Credentials::BotToken("t".to_string());
    let c2 = Credentials::Cert(vec![1], vec![2]);
    let c3 = Credentials::ApiKey("k".to_string());
    let c4 = Credentials::UsernamePassword("u".to_string(), "p".to_string());
    let c5 = Credentials::Custom("c".to_string());

    // Just verify they construct and clone
    let _ = (c1.clone(), c2.clone(), c3.clone(), c4.clone(), c5.clone());
}

#[test]
fn gt32_flag_degraded_domain() {
    assert_eq!(FLAG_DEGRADED_DOMAIN, 0x0001);
}

// ── GT33: mission_id ─────────────────────────────────────────────

#[test]
fn gt33_mission_id() {
    let gt = make_transport();
    assert_eq!(gt.mission_id(), [0x42u8; 32]);
}
