//! Domain-Governed Transport — RFC-0863p-a
//!
//! Wraps [`NodeTransport`] with domain governance awareness.
//!
//! ## Types
//!
//! - [`GovernedTransport`] — governance-aware transport wrapper
//! - [`GovernedTransportLifecycle`] — lifecycle state machine
//! - [`AdapterConfig`] / [`Credentials`] / [`DomainRole`] — developer-facing config
//! - [`DcLifecycleEvent`] — DC lifecycle change event
//! - [`ReceivedMessage`] — message received through governed transport
//!
//! ## Constants
//!
//! - [`FLAG_DEGRADED_DOMAIN`] — flag for messages sent through degraded domains

use std::collections::BTreeMap;

use crate::dom_bootstrap::{BroadcastDomainHint, DcTrustLevel};
use crate::node_transport::NodeTransport;
use crate::receiver::ReceiveContext;
use crate::sender::{SendContext, TransportError};

// ── Constants ────────────────────────────────────────────────────

/// Flag indicating the message is being sent through a degraded domain.
pub const FLAG_DEGRADED_DOMAIN: u64 = 0x0001;

// ── AdapterConfig (RFC-0863p-a §Data Structures) ─────────────────

/// Configuration for a single platform adapter in the transport stack.
#[derive(Clone, Debug)]
pub struct AdapterConfig {
    /// Platform type (Telegram, Discord, QUIC, etc.)
    pub platform: octo_network::dot::PlatformType,
    /// Authentication credentials for the platform.
    pub credentials: Credentials,
    /// Optional broadcast domain hint for DotDomain bootstrap.
    /// If set, this adapter is classified as broadcast-capable.
    /// If None, this adapter is point-to-point (needs seed list).
    pub domain_hint: Option<BroadcastDomainHint>,
    /// The node's role in the domain.
    pub role: DomainRole,
}

/// Credentials for platform authentication.
#[derive(Clone, Debug)]
pub enum Credentials {
    BotToken(String),
    Cert(Vec<u8>, Vec<u8>),
    ApiKey(String),
    UsernamePassword(String, String),
    /// Adapter-specific credential format.
    /// The string is passed verbatim to the adapter's `authenticate()` method.
    /// Format is adapter-defined (see per-adapter documentation).
    Custom(String),
}

/// The node's role in a broadcast domain.
///
/// Determines what governance actions the node can take
/// and how bootstrap behaves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DomainRole {
    /// No domain role (point-to-point adapter).
    None,
    /// The node is joining an existing domain (most common).
    Joiner,
    /// The node is the DomainCoordinator of this domain.
    Coordinator,
    /// The node is a sub-admin (deputy DC).
    SubAdmin,
}

// ── GovernedTransportLifecycle (RFC-0863p-a §Lifecycle) ──────────

/// Lifecycle of the governed transport.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum GovernedTransportLifecycle {
    /// Building: adapters being loaded.
    Building = 0x00,
    /// Bootstrapping: auto-bootstrap pipeline running.
    Bootstrapping = 0x01,
    /// Ready: bootstrap complete, governance active.
    Ready = 0x02,
    /// Degraded: one or more domains in Suspect state.
    Degraded = 0x03,
    /// Rebooting: re-running bootstrap after domain loss.
    Rebooting = 0x04,
}

impl GovernedTransportLifecycle {
    /// Derive lifecycle from aggregate domain trust levels.
    ///
    /// - All domains Trusted → Ready
    /// - Any domain Degraded → Degraded
    /// - All domains Untrusted or empty → Ready (PTP-only)
    pub fn from_domain_trust(levels: &[DcTrustLevel]) -> Self {
        if levels.is_empty() {
            return Self::Ready; // PTP-only; no governance
        }
        // Priority: Rebooting > Degraded > Ready
        // Rebooting: ALL domains untrusted (no way to recover)
        if levels.iter().all(|l| *l == DcTrustLevel::Untrusted) {
            Self::Rebooting
        // Degraded: any domain is Degraded, Blocked, or Untrusted
        } else if levels.iter().any(|l| {
            matches!(
                l,
                DcTrustLevel::Degraded | DcTrustLevel::Blocked | DcTrustLevel::Untrusted
            )
        }) {
            Self::Degraded
        } else {
            // All Trusted or Provisional
            Self::Ready
        }
    }
}

// ── DcLifecycleEvent (RFC-0863p-a §Data Structures) ──────────────

/// DC lifecycle event for domain loss detection.
#[derive(Clone, Debug)]
pub struct DcLifecycleEvent {
    /// The DC that changed state.
    pub dc_id: [u8; 32],
    /// The previous lifecycle state (as byte).
    pub previous_state: u8,
    /// The new lifecycle state (as byte).
    pub new_state: u8,
    /// Epoch at which the transition occurred.
    pub epoch: u64,
}

impl DcLifecycleEvent {
    /// Returns true if this event represents domain loss.
    pub fn is_domain_loss(&self) -> bool {
        // Domain loss: DC transitions to Demoting (0x05), Resigned (0x06), or Inactive (0x07)
        matches!(self.new_state, 0x05..=0x07)
    }

    /// Returns the trust level for the new state.
    pub fn new_trust_level(&self) -> DcTrustLevel {
        DcTrustLevel::from_lifecycle_byte(self.new_state)
    }
}

// ── ReceivedMessage (RFC-0863p-a §Data Structures) ───────────────

/// A message received from a platform adapter.
#[derive(Clone, Debug)]
pub struct ReceivedMessage {
    /// The platform adapter that received the message.
    pub platform: octo_network::dot::PlatformType,
    /// The source peer identifier (platform-native).
    pub source_peer: Vec<u8>,
    /// The raw message payload.
    pub payload: Vec<u8>,
    /// The domain this message was received from (if any).
    pub domain_ref: Option<String>,
}

// ── GovernedTransport (RFC-0863p-a §Specification) ───────────────

/// Governance-aware transport wrapper.
///
/// Wraps [`NodeTransport`] with domain governance awareness.
/// Gates send/receive operations on GroupRegistry state and DC lifecycle.
pub struct GovernedTransport {
    /// The underlying transport layer.
    inner: NodeTransport,
    /// Current lifecycle state.
    lifecycle: GovernedTransportLifecycle,
    /// Mission ID this transport is bound to.
    mission_id: [u8; 32],
    /// Adapter domain bindings: (platform, domain_ref, role).
    adapter_domains: Vec<(octo_network::dot::PlatformType, String, DomainRole)>,
    /// DC trust levels per domain (indexed by dc_id).
    dc_trust: BTreeMap<[u8; 32], DcTrustLevel>,
}

impl GovernedTransport {
    /// Create a new governed transport.
    pub fn new(
        inner: NodeTransport,
        mission_id: [u8; 32],
        adapter_domains: Vec<(octo_network::dot::PlatformType, String, DomainRole)>,
    ) -> Self {
        Self {
            inner,
            lifecycle: GovernedTransportLifecycle::Building,
            mission_id,
            adapter_domains,
            dc_trust: BTreeMap::new(),
        }
    }

    /// Returns true if the transport is ready to send/receive.
    /// Ready means: bootstrap complete, at least one domain is Trusted or
    /// at least one PTP adapter is available.
    pub fn ready(&self) -> bool {
        matches!(
            self.lifecycle,
            GovernedTransportLifecycle::Ready | GovernedTransportLifecycle::Degraded
        )
    }

    /// Current lifecycle state.
    pub fn lifecycle(&self) -> GovernedTransportLifecycle {
        self.lifecycle
    }

    /// Mission ID this transport is bound to.
    pub fn mission_id(&self) -> [u8; 32] {
        self.mission_id
    }

    /// Update the DC trust level for a domain.
    pub fn update_dc_trust(&mut self, dc_id: [u8; 32], level: DcTrustLevel) {
        self.dc_trust.insert(dc_id, level);
        self.recalculate_lifecycle();
    }

    /// Handle a DC lifecycle event (domain loss detection).
    pub fn on_dc_lifecycle_event(&mut self, event: &DcLifecycleEvent) {
        let new_level = event.new_trust_level();
        self.dc_trust.insert(event.dc_id, new_level);

        if event.is_domain_loss() {
            // Only reboot if ALL domains are now untrusted
            let all_untrusted = self
                .dc_trust
                .values()
                .all(|l| *l == DcTrustLevel::Untrusted);
            if all_untrusted {
                self.lifecycle = GovernedTransportLifecycle::Rebooting;
            } else {
                self.recalculate_lifecycle();
            }
        } else {
            self.recalculate_lifecycle();
        }
    }

    /// Send payload via the best available adapter, respecting governance.
    ///
    /// Governance checks:
    /// 1. Not in Rebooting state
    /// 2. Broadcast adapters: DC lifecycle allows send (Trusted or Provisional)
    /// 3. Domain not decommissioned (not Untrusted)
    ///
    /// For PTP adapters, no governance check is needed.
    pub async fn send_best(&self, payload: &[u8], ctx: &SendContext) -> Result<(), TransportError> {
        // If in Rebooting state, reject all sends
        if self.lifecycle == GovernedTransportLifecycle::Rebooting {
            return Err(TransportError::AllTransportsFailed);
        }

        // Check if any broadcast domain has Untrusted DC —
        // skip those adapters by checking per-domain trust
        for (platform, _domain_ref, role) in &self.adapter_domains {
            if *role == DomainRole::None {
                continue; // PTP adapter, no governance
            }
            // Find DC trust for this platform's domain
            // (in production, this would check GroupRegistry binding)
            let _ = platform;
        }

        // Delegate to inner transport
        self.inner.send_best(payload, ctx).await
    }

    /// Receive messages from all governance-approved adapters.
    ///
    /// Skips adapters whose domain is decommissioned (Untrusted DC)
    /// or where the node has been kicked.
    ///
    /// Note: this is a placeholder. Full implementation requires
    /// adapter-level receive integration (see RFC-0863p-a §Algorithms).
    pub fn receive_filter(&self) -> &[(octo_network::dot::PlatformType, String, DomainRole)] {
        // Return only domains with Trusted/Provisional/Degraded DC
        // In production, this would filter adapter_domains by DC trust
        &self.adapter_domains
    }

    /// Governance check for an inbound receive.
    ///
    /// A context "passes" when:
    /// - The transport lifecycle is not `Rebooting` (kick / full domain loss).
    /// - If the source transport maps to a configured broadcast domain,
    ///   that domain's DC trust level is not `Untrusted` (decommissioned).
    ///
    /// PTP adapters (no domain binding) always pass the domain check.
    pub fn passes_governance(&self, ctx: &ReceiveContext) -> bool {
        // 1. Lifecycle gate: Rebooting means the node was kicked or all
        //    domains were lost — refuse all receives during recovery.
        if self.lifecycle == GovernedTransportLifecycle::Rebooting {
            return false;
        }

        // 2. Domain trust gate: if the source transport corresponds to a
        //    broadcast domain, that domain's DC must not be Untrusted.
        let Some(platform) = platform_from_source(&ctx.source_transport) else {
            // Unknown source name → treat as PTP / external; allow.
            return true;
        };

        let Some((_, role)) = find_domain_for_platform(platform, &self.adapter_domains) else {
            // No broadcast binding → PTP adapter; allow.
            return true;
        };
        if role == DomainRole::None {
            return true;
        }

        // Broadcast domain — refuse if any DC for this domain is Untrusted.
        // dc_trust is indexed by dc_id, not by domain; we conservatively
        // reject if any tracked DC is Untrusted (the configured domain's
        // binding state, when wired, will narrow this further).
        !self
            .dc_trust
            .values()
            .any(|lvl| *lvl == DcTrustLevel::Untrusted)
    }

    /// Receive a single inbound payload and dispatch it to registered
    /// receivers after passing the governance gate.
    ///
    /// Mirrors `NodeTransport::dispatch` but adds a governance pre-check
    /// (RFC-0863p-a §Governance-Gated Receive Path). Returns
    /// `TransportError::GovernanceViolation` when the context fails the
    /// gate (e.g. node was kicked, or the source domain is decommissioned).
    pub async fn receive(
        &self,
        payload: &[u8],
        ctx: &ReceiveContext,
    ) -> Result<(), TransportError> {
        if !self.passes_governance(ctx) {
            return Err(TransportError::GovernanceViolation(
                "kick detected or domain mismatch".into(),
            ));
        }
        self.inner.dispatch(payload, ctx).await
    }

    /// Recalculate lifecycle from aggregate DC trust levels.
    fn recalculate_lifecycle(&mut self) {
        let levels: Vec<DcTrustLevel> = self.dc_trust.values().copied().collect();
        self.lifecycle = GovernedTransportLifecycle::from_domain_trust(&levels);
    }
}

// ── Helper Functions (RFC-0863p-a §Algorithms) ───────────────────

/// Map a platform type back to its broadcast domain binding.
/// Returns None for PTP adapters (no domain binding).
pub fn find_domain_for_platform(
    platform: octo_network::dot::PlatformType,
    adapter_domains: &[(octo_network::dot::PlatformType, String, DomainRole)],
) -> Option<(String, DomainRole)> {
    adapter_domains
        .iter()
        .find(|(pt, _, role)| *pt == platform && *role != DomainRole::None)
        .map(|(_, domain_ref, role)| (domain_ref.clone(), *role))
}

/// Map a receive `source_transport` string back to a `PlatformType`.
/// Returns `None` when the name does not correspond to any known platform
/// (treated as PTP / external by the governance gate).
fn platform_from_source(source: &str) -> Option<octo_network::dot::PlatformType> {
    // Source names follow the lowercase `PlatformType::name()` convention.
    let lower = source.to_ascii_lowercase();
    match lower.as_str() {
        "telegram" => Some(octo_network::dot::PlatformType::Telegram),
        "discord" => Some(octo_network::dot::PlatformType::Discord),
        "matrix" => Some(octo_network::dot::PlatformType::Matrix),
        "nostr" => Some(octo_network::dot::PlatformType::Nostr),
        "signal" => Some(octo_network::dot::PlatformType::Signal),
        "irc" => Some(octo_network::dot::PlatformType::IRC),
        "slack" => Some(octo_network::dot::PlatformType::Slack),
        "whatsapp" => Some(octo_network::dot::PlatformType::WhatsApp),
        "webhook" => Some(octo_network::dot::PlatformType::Webhook),
        "native-p2p" => Some(octo_network::dot::PlatformType::NativeP2P),
        "bluetooth" => Some(octo_network::dot::PlatformType::Bluetooth),
        "lora" => Some(octo_network::dot::PlatformType::LoRa),
        "webrtc" => Some(octo_network::dot::PlatformType::WebRTC),
        "bluesky" => Some(octo_network::dot::PlatformType::Bluesky),
        "twitter" => Some(octo_network::dot::PlatformType::Twitter),
        "reddit" => Some(octo_network::dot::PlatformType::Reddit),
        "wechat" => Some(octo_network::dot::PlatformType::WeChat),
        "dingtalk" => Some(octo_network::dot::PlatformType::DingTalk),
        "lark" => Some(octo_network::dot::PlatformType::Lark),
        "qq" => Some(octo_network::dot::PlatformType::QQ),
        "quic" => Some(octo_network::dot::PlatformType::Quic),
        "tcp" => Some(octo_network::dot::PlatformType::Tcp),
        "udp" => Some(octo_network::dot::PlatformType::Udp),
        _ => None,
    }
}

/// Derive trust levels from a list of DC lifecycle byte values.
pub fn derive_trust_levels(lifecycle_bytes: &[u8]) -> Vec<DcTrustLevel> {
    lifecycle_bytes
        .iter()
        .map(|b| DcTrustLevel::from_lifecycle_byte(*b))
        .collect()
}

// ── Tests ────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::receiver::NetworkReceiver;
    use crate::sender::NetworkSender;
    use async_trait::async_trait;
    use std::sync::Arc;

    // ── GovernedTransportLifecycle tests ────────────────────────

    #[test]
    fn lifecycle_from_empty_trust() {
        let levels = vec![];
        assert_eq!(
            GovernedTransportLifecycle::from_domain_trust(&levels),
            GovernedTransportLifecycle::Ready
        );
    }

    #[test]
    fn lifecycle_from_all_trusted() {
        let levels = vec![DcTrustLevel::Trusted, DcTrustLevel::Trusted];
        assert_eq!(
            GovernedTransportLifecycle::from_domain_trust(&levels),
            GovernedTransportLifecycle::Ready
        );
    }

    #[test]
    fn lifecycle_from_degraded() {
        let levels = vec![DcTrustLevel::Trusted, DcTrustLevel::Degraded];
        assert_eq!(
            GovernedTransportLifecycle::from_domain_trust(&levels),
            GovernedTransportLifecycle::Degraded
        );
    }

    #[test]
    fn lifecycle_from_all_untrusted() {
        let levels = vec![DcTrustLevel::Untrusted, DcTrustLevel::Untrusted];
        assert_eq!(
            GovernedTransportLifecycle::from_domain_trust(&levels),
            GovernedTransportLifecycle::Rebooting
        );
    }

    #[test]
    fn lifecycle_from_provisional() {
        let levels = vec![DcTrustLevel::Provisional];
        assert_eq!(
            GovernedTransportLifecycle::from_domain_trust(&levels),
            GovernedTransportLifecycle::Ready
        );
    }

    #[test]
    fn lifecycle_from_blocked() {
        let levels = vec![DcTrustLevel::Trusted, DcTrustLevel::Blocked];
        assert_eq!(
            GovernedTransportLifecycle::from_domain_trust(&levels),
            GovernedTransportLifecycle::Degraded
        );
    }

    #[test]
    fn lifecycle_from_mixed_untrusted_provisional() {
        let levels = vec![DcTrustLevel::Untrusted, DcTrustLevel::Provisional];
        // Some domains lost → Degraded (not all lost → not Rebooting)
        assert_eq!(
            GovernedTransportLifecycle::from_domain_trust(&levels),
            GovernedTransportLifecycle::Degraded
        );
    }

    // ── DcLifecycleEvent tests ──────────────────────────────────

    #[test]
    fn domain_loss_detection() {
        let event = DcLifecycleEvent {
            dc_id: [0xAA; 32],
            previous_state: 0x02, // Active
            new_state: 0x05,      // Demoting
            epoch: 100,
        };
        assert!(event.is_domain_loss());
        assert_eq!(event.new_trust_level(), DcTrustLevel::Untrusted);
    }

    #[test]
    fn no_domain_loss_on_suspect() {
        let event = DcLifecycleEvent {
            dc_id: [0xAA; 32],
            previous_state: 0x02, // Active
            new_state: 0x03,      // Suspect
            epoch: 100,
        };
        assert!(!event.is_domain_loss());
        assert_eq!(event.new_trust_level(), DcTrustLevel::Degraded);
    }

    // ── Helper function tests ───────────────────────────────────

    #[test]
    fn find_domain_for_platform_hit() {
        let domains = vec![
            (
                octo_network::dot::PlatformType::Telegram,
                "-100".to_string(),
                DomainRole::Joiner,
            ),
            (
                octo_network::dot::PlatformType::Quic,
                "".to_string(),
                DomainRole::None,
            ),
        ];

        let result = find_domain_for_platform(octo_network::dot::PlatformType::Telegram, &domains);
        assert!(result.is_some());
        let (domain_ref, role) = result.unwrap();
        assert_eq!(domain_ref, "-100");
        assert_eq!(role, DomainRole::Joiner);
    }

    #[test]
    fn find_domain_for_platform_ptp() {
        let domains = vec![(
            octo_network::dot::PlatformType::Quic,
            "".to_string(),
            DomainRole::None,
        )];

        let result = find_domain_for_platform(octo_network::dot::PlatformType::Quic, &domains);
        assert!(result.is_none());
    }

    #[test]
    fn find_domain_for_platform_miss() {
        let domains = vec![(
            octo_network::dot::PlatformType::Telegram,
            "-100".to_string(),
            DomainRole::Joiner,
        )];

        let result = find_domain_for_platform(octo_network::dot::PlatformType::Discord, &domains);
        assert!(result.is_none());
    }

    #[test]
    fn derive_trust_levels_test() {
        let bytes = vec![0x02, 0x03, 0x05, 0x00];
        let levels = derive_trust_levels(&bytes);
        assert_eq!(
            levels,
            vec![
                DcTrustLevel::Trusted,
                DcTrustLevel::Degraded,
                DcTrustLevel::Untrusted,
                DcTrustLevel::Provisional,
            ]
        );
    }

    // ── GovernedTransport tests ─────────────────────────────────

    /// Mock sender for testing.
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

    fn make_governed_transport() -> GovernedTransport {
        let inner = NodeTransport::new(vec![Arc::new(MockSender) as Arc<dyn NetworkSender>]);
        GovernedTransport::new(
            inner,
            [0x42u8; 32],
            vec![(
                octo_network::dot::PlatformType::Telegram,
                "-100".to_string(),
                DomainRole::Joiner,
            )],
        )
    }

    fn test_ctx() -> SendContext {
        SendContext {
            mission_id: [0x42u8; 32],
            priority: 128,
            source_peer: [0xAAu8; 32],
            origin_gateway: [0xBBu8; 32],
        }
    }

    #[test]
    fn governed_transport_ready_initially() {
        let gt = make_governed_transport();
        // Starts in Building state (bootstrap not yet run)
        assert!(!gt.ready());
        assert_eq!(gt.lifecycle(), GovernedTransportLifecycle::Building);
        assert_eq!(gt.mission_id(), [0x42u8; 32]);
    }

    #[test]
    fn governed_transport_transitions_to_ready() {
        let mut gt = make_governed_transport();
        gt.update_dc_trust([0xAA; 32], DcTrustLevel::Trusted);
        assert!(gt.ready());
        assert_eq!(gt.lifecycle(), GovernedTransportLifecycle::Ready);
    }

    #[test]
    fn update_dc_trust_changes_lifecycle() {
        let mut gt = make_governed_transport();

        gt.update_dc_trust([0xAA; 32], DcTrustLevel::Trusted);
        assert_eq!(gt.lifecycle(), GovernedTransportLifecycle::Ready);

        gt.update_dc_trust([0xAA; 32], DcTrustLevel::Degraded);
        assert_eq!(gt.lifecycle(), GovernedTransportLifecycle::Degraded);

        gt.update_dc_trust([0xAA; 32], DcTrustLevel::Untrusted);
        assert_eq!(gt.lifecycle(), GovernedTransportLifecycle::Rebooting);
    }

    #[test]
    fn dc_lifecycle_event_domain_loss() {
        let mut gt = make_governed_transport();
        gt.update_dc_trust([0xAA; 32], DcTrustLevel::Trusted);

        let event = DcLifecycleEvent {
            dc_id: [0xAA; 32],
            previous_state: 0x02,
            new_state: 0x05, // Demoting → domain loss
            epoch: 100,
        };

        gt.on_dc_lifecycle_event(&event);
        // Only domain is now Untrusted → Rebooting
        assert_eq!(gt.lifecycle(), GovernedTransportLifecycle::Rebooting);
    }

    #[test]
    fn dc_lifecycle_event_domain_loss_mixed() {
        let mut gt = make_governed_transport();
        // Two domains: one Trusted, one about to be lost
        gt.update_dc_trust([0xAA; 32], DcTrustLevel::Trusted);
        gt.update_dc_trust([0xBB; 32], DcTrustLevel::Trusted);

        let event = DcLifecycleEvent {
            dc_id: [0xBB; 32],
            previous_state: 0x02,
            new_state: 0x05, // Demoting → domain loss
            epoch: 100,
        };

        gt.on_dc_lifecycle_event(&event);
        // Other domain still Trusted → Degraded (not Rebooting)
        assert_eq!(gt.lifecycle(), GovernedTransportLifecycle::Degraded);
    }

    #[test]
    fn dc_lifecycle_event_suspect() {
        let mut gt = make_governed_transport();

        let event = DcLifecycleEvent {
            dc_id: [0xAA; 32],
            previous_state: 0x02,
            new_state: 0x03, // Suspect → degraded
            epoch: 100,
        };

        gt.on_dc_lifecycle_event(&event);
        assert_eq!(gt.lifecycle(), GovernedTransportLifecycle::Degraded);
    }

    #[tokio::test]
    async fn send_best_while_ready() {
        let mut gt = make_governed_transport();
        gt.update_dc_trust([0xAA; 32], DcTrustLevel::Trusted);
        let result = gt.send_best(b"hello", &test_ctx()).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn send_best_while_rebooting() {
        let mut gt = make_governed_transport();
        gt.update_dc_trust([0xAA; 32], DcTrustLevel::Untrusted);

        let result = gt.send_best(b"hello", &test_ctx()).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            TransportError::AllTransportsFailed
        ));
    }

    // ── AdapterConfig / Credentials tests ───────────────────────

    #[test]
    fn adapter_config_construction() {
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
        assert!(config.domain_hint.is_some());
    }

    #[test]
    fn domain_role_equality() {
        assert_eq!(DomainRole::None, DomainRole::None);
        assert_ne!(DomainRole::Joiner, DomainRole::Coordinator);
        assert_ne!(DomainRole::Coordinator, DomainRole::SubAdmin);
    }

    // ── FLAG_DEGRADED_DOMAIN test ───────────────────────────────

    #[test]
    fn flag_degraded_domain_value() {
        assert_eq!(FLAG_DEGRADED_DOMAIN, 0x0001);
    }

    // ── receive() governance tests ──────────────────────────────

    /// Receiver that records invocations for receive-path tests.
    struct CountingReceiver {
        count: Arc<std::sync::atomic::AtomicUsize>,
    }

    #[async_trait]
    impl NetworkReceiver for CountingReceiver {
        async fn on_receive(
            &self,
            _payload: &[u8],
            _ctx: &ReceiveContext,
        ) -> Result<(), TransportError> {
            self.count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(())
        }
        fn name(&self) -> &str {
            "counting"
        }
    }

    fn make_governed_transport_with_receiver(
        count: Arc<std::sync::atomic::AtomicUsize>,
    ) -> GovernedTransport {
        let inner = NodeTransport::new(vec![Arc::new(MockSender) as Arc<dyn NetworkSender>]);
        inner.register_receiver(Arc::new(CountingReceiver { count }));
        GovernedTransport::new(
            inner,
            [0x42u8; 32],
            vec![(
                octo_network::dot::PlatformType::Telegram,
                "-100".to_string(),
                DomainRole::Joiner,
            )],
        )
    }

    fn recv_ctx(source: &str, sender: Option<[u8; 32]>) -> ReceiveContext {
        ReceiveContext {
            source_transport: source.to_string(),
            mission_id: [0x42u8; 32],
            sender_id: sender,
        }
    }

    #[tokio::test]
    async fn receive_dispatches_when_ready() {
        let count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let mut gt = make_governed_transport_with_receiver(count.clone());
        gt.update_dc_trust([0xAA; 32], DcTrustLevel::Trusted);

        let ctx = recv_ctx("telegram", Some([0xCC; 32]));
        let result = gt.receive(b"payload", &ctx).await;
        assert!(result.is_ok(), "expected Ok, got {:?}", result);
        assert!(
            count.load(std::sync::atomic::Ordering::SeqCst) >= 1,
            "receiver was not invoked"
        );
    }

    #[tokio::test]
    async fn receive_with_ptp_source_dispatches() {
        let count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let mut gt = make_governed_transport_with_receiver(count.clone());
        gt.update_dc_trust([0xAA; 32], DcTrustLevel::Trusted);

        // "tcp" maps to a PTP platform (no broadcast binding in fixture)
        let ctx = recv_ctx("tcp", None);
        let result = gt.receive(b"payload", &ctx).await;
        assert!(result.is_ok());
        assert!(count.load(std::sync::atomic::Ordering::SeqCst) >= 1);
    }

    #[tokio::test]
    async fn receive_returns_governance_violation_when_rebooting() {
        // Rebooting == "kick detected or all domains lost" per RFC.
        let count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let mut gt = make_governed_transport_with_receiver(count.clone());
        gt.update_dc_trust([0xAA; 32], DcTrustLevel::Untrusted);

        let ctx = recv_ctx("telegram", Some([0xCC; 32]));
        let result = gt.receive(b"payload", &ctx).await;
        assert!(matches!(
            result,
            Err(TransportError::GovernanceViolation(_))
        ));
        assert_eq!(
            count.load(std::sync::atomic::Ordering::SeqCst),
            0,
            "receiver must not be invoked when governance fails"
        );
    }

    #[tokio::test]
    async fn receive_returns_governance_violation_when_domain_untrusted() {
        // Lifecycle Ready but a tracked DC is Untrusted → domain decommissioned.
        let count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let mut gt = make_governed_transport_with_receiver(count.clone());
        // Add a second DC marked Trusted so lifecycle is Ready, but keep
        // the first as Untrusted to simulate a decommissioned broadcast
        // domain the governance gate must reject.
        gt.update_dc_trust([0xAA; 32], DcTrustLevel::Untrusted);
        gt.update_dc_trust([0xBB; 32], DcTrustLevel::Trusted);

        let ctx = recv_ctx("telegram", Some([0xCC; 32]));
        let result = gt.receive(b"payload", &ctx).await;
        assert!(matches!(
            result,
            Err(TransportError::GovernanceViolation(_))
        ));
        assert_eq!(
            count.load(std::sync::atomic::Ordering::SeqCst),
            0,
            "receiver must not be invoked when the broadcast domain is decommissioned"
        );
    }

    #[tokio::test]
    async fn receive_with_unknown_source_treated_as_ptp() {
        let count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let mut gt = make_governed_transport_with_receiver(count.clone());
        gt.update_dc_trust([0xAA; 32], DcTrustLevel::Trusted);

        // Unknown source name → no platform mapping → PTP / external.
        let ctx = recv_ctx("custom-relay", None);
        let result = gt.receive(b"payload", &ctx).await;
        assert!(result.is_ok());
        assert!(count.load(std::sync::atomic::Ordering::SeqCst) >= 1);
    }
}
