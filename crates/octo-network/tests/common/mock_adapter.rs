//! Mock Platform Adapter for integration testing.
//!
//! An in-memory implementation of `PlatformAdapter` that simulates
//! platform transport without network dependencies. Supports configurable
//! failure modes for testing adversarial scenarios.

use std::collections::{BTreeMap, VecDeque};
use std::sync::Arc;
use tokio::sync::Mutex;

use octo_network::dot::adapters::coordinator_admin::{
    AddMemberOutput, AdminCapabilityReport, CoordinatorAdmin, GroupHandle, GroupId, GroupMemberSpec,
};
use octo_network::dot::adapters::{
    CapabilityReport, DeliveryReceipt, PlatformAdapter, RawPlatformMessage,
};
use octo_network::dot::domain::{BroadcastDomainId, PlatformType};
use octo_network::dot::envelope::DeterministicEnvelope;
use octo_network::dot::error::PlatformAdapterError;
// `GroupMetadata`, `InviteRef`, `PeerId` are referenced by the
// `CoordinatorAdmin` trait surface; importing them is harmless even
// when the scriptable slots don't reference them, and the trait's
// future extensions may pull them in.
#[allow(unused_imports)]
use octo_network::dot::adapters::coordinator_admin::{GroupMetadata, InviteRef, PeerId};

/// Failure mode for the mock adapter.
#[derive(Clone, Debug)]
pub enum FailureMode {
    /// No failures — messages delivered normally
    None,
    /// Drop all outbound messages
    DropAll,
    /// Drop messages with a given probability (0-100)
    DropRandom(u8),
    /// Duplicate every message N times
    Duplicate(u8),
    /// Reorder messages (swap adjacent pairs)
    Reorder,
    /// Delay messages by N logical time units
    Delay(u64),
}

/// Scripted responses for [`CoordinatorAdmin`] methods on the mock.
///
/// Each field is an optional scripted return value. When `Some(_)`, the
/// corresponding trait method returns that value verbatim (after cloning
/// out from behind the mutex). When `None`, the trait method falls
/// through to the default `Unimplemented` error from the trait.
///
/// Tests set this via [`MockPlatformAdapter::with_admin_scripted`] to
/// drive cross-module flows without standing up a real platform.
#[derive(Clone, Debug, Default)]
pub struct AdminScripted {
    /// Scripted return for `create_group(subject, initial_members)`.
    pub create_group: Option<Result<GroupHandle, PlatformAdapterError>>,
    /// Scripted return for `add_member(group_id, member)`.
    pub add_member: Option<Result<AddMemberOutput, PlatformAdapterError>>,
}

/// Recorded call against a `CoordinatorAdmin` method on the mock.
///
/// Tests inspect [`MockPlatformAdapter::admin_calls`] to verify a flow
/// actually went through the bridge rather than bypassing it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AdminCall {
    CreateGroup {
        subject: String,
        initial_member_count: usize,
    },
    AddMember {
        group_id: String,
        member_handle: String,
        member_is_admin: bool,
    },
}

/// Mock platform adapter for integration testing.
///
/// Implements `PlatformAdapter` with in-memory message queues.
/// Supports message injection (for simulating inbound) and
/// observation (for asserting on outbound).
pub struct MockPlatformAdapter {
    /// Platform type this adapter simulates
    platform: PlatformType,
    /// Outbound messages (sent by the adapter)
    outbound: Arc<Mutex<Vec<Vec<u8>>>>,
    /// Inbound messages (to be received by the adapter)
    inbound: Arc<Mutex<VecDeque<RawPlatformMessage>>>,
    /// Failure mode
    failure_mode: FailureMode,
    /// Self handle for relay loop prevention
    self_id: Option<String>,
    /// Domain hash for this adapter
    domain_hash: [u8; 32],
    /// Message counter for unique IDs
    counter: Arc<Mutex<u64>>,
    /// Scripted `CoordinatorAdmin` responses. When `None` for a method,
    /// that method returns the trait's default `Unimplemented`.
    admin_scripted: Arc<Mutex<AdminScripted>>,
    /// Recorded `CoordinatorAdmin` calls (for test assertions).
    admin_calls: Arc<Mutex<Vec<AdminCall>>>,
}

impl MockPlatformAdapter {
    /// Create a new mock adapter for the given platform type.
    pub fn new(platform: PlatformType) -> Self {
        let domain_hash = *blake3::hash(format!("mock:{:?}", platform).as_bytes()).as_bytes();
        Self {
            platform,
            outbound: Arc::new(Mutex::new(Vec::new())),
            inbound: Arc::new(Mutex::new(VecDeque::new())),
            failure_mode: FailureMode::None,
            self_id: None,
            domain_hash,
            counter: Arc::new(Mutex::new(0)),
            admin_scripted: Arc::new(Mutex::new(AdminScripted::default())),
            admin_calls: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Set the failure mode.
    pub fn with_failure_mode(mut self, mode: FailureMode) -> Self {
        self.failure_mode = mode;
        self
    }

    /// Set the self handle for relay loop prevention.
    pub fn with_self_handle(mut self, handle: String) -> Self {
        self.self_id = Some(handle);
        self
    }

    /// Set the scripted `CoordinatorAdmin` responses.
    ///
    /// Tests use this to drive cross-module flows (e.g.
    /// `CoordinatorAdmin::create_group` → `BindEnvelope` →
    /// `DeterministicEnvelope` → `PlatformAdapter::send_envelope`)
    /// without standing up a real WhatsApp/IRC/etc. backend.
    pub fn with_admin_scripted(mut self, scripted: AdminScripted) -> Self {
        self.admin_scripted = Arc::new(Mutex::new(scripted));
        self
    }

    /// Mutate the scripted `CoordinatorAdmin` responses at test time.
    /// Useful when the same adapter is reused across multiple flow steps.
    pub async fn set_admin_scripted(&self, scripted: AdminScripted) {
        *self.admin_scripted.lock().await = scripted;
    }

    /// Snapshot the recorded `CoordinatorAdmin` calls so far.
    pub async fn admin_calls(&self) -> Vec<AdminCall> {
        self.admin_calls.lock().await.clone()
    }

    /// Inject a message into the inbound queue (simulates receiving from platform).
    pub async fn inject_message(&self, payload: Vec<u8>) {
        let mut inbound = self.inbound.lock().await;
        let mut counter = self.counter.lock().await;
        *counter += 1;
        inbound.push_back(RawPlatformMessage {
            platform_id: format!("mock-{}", *counter),
            payload,
            metadata: BTreeMap::new(),
        });
    }

    /// Get all outbound messages (for assertions).
    pub async fn outbound_messages(&self) -> Vec<Vec<u8>> {
        self.outbound.lock().await.clone()
    }

    /// Get the count of outbound messages.
    pub async fn outbound_count(&self) -> usize {
        self.outbound.lock().await.len()
    }

    /// Clear all queues.
    pub async fn clear(&self) {
        self.outbound.lock().await.clear();
        self.inbound.lock().await.clear();
    }
}

#[async_trait::async_trait]
impl PlatformAdapter for MockPlatformAdapter {
    async fn send_envelope(
        &self,
        _domain: &BroadcastDomainId,
        envelope: &DeterministicEnvelope,
    ) -> Result<DeliveryReceipt, PlatformAdapterError> {
        let wire_bytes = envelope.to_wire_bytes();

        // Apply failure mode
        match &self.failure_mode {
            FailureMode::DropAll => {
                return Err(PlatformAdapterError::Unreachable {
                    platform: format!("{:?}", self.platform),
                    reason: "drop all mode".into(),
                });
            }
            FailureMode::DropRandom(pct) => {
                let hash = blake3::hash(&wire_bytes);
                let byte = hash.as_bytes()[0];
                if byte < (*pct * 255 / 100) as u8 {
                    return Err(PlatformAdapterError::Unreachable {
                        platform: format!("{:?}", self.platform),
                        reason: "random drop".into(),
                    });
                }
            }
            _ => {}
        }

        let mut outbound = self.outbound.lock().await;
        outbound.push(wire_bytes.clone());

        // Handle duplication
        if let FailureMode::Duplicate(n) = &self.failure_mode {
            for _ in 0..*n {
                outbound.push(wire_bytes.clone());
            }
        }

        let mut counter = self.counter.lock().await;
        *counter += 1;

        Ok(DeliveryReceipt {
            platform_message_id: format!("mock-{}", *counter),
            delivered_at: 0,
        })
    }

    async fn receive_messages(
        &self,
        _domain: &BroadcastDomainId,
    ) -> Result<Vec<RawPlatformMessage>, PlatformAdapterError> {
        let mut inbound = self.inbound.lock().await;
        let messages: Vec<RawPlatformMessage> = inbound.drain(..).collect();

        // Handle reordering
        if let FailureMode::Reorder = &self.failure_mode {
            let mut reordered = messages;
            if reordered.len() >= 2 {
                reordered.swap(0, 1);
            }
            return Ok(reordered);
        }

        Ok(messages)
    }

    fn canonicalize(
        &self,
        raw: &RawPlatformMessage,
    ) -> Result<DeterministicEnvelope, PlatformAdapterError> {
        if raw.payload.is_empty() {
            return Err(PlatformAdapterError::ApiError {
                code: 400,
                message: "empty payload".into(),
            });
        }
        DeterministicEnvelope::from_wire_bytes(&raw.payload).map_err(|e| {
            PlatformAdapterError::ApiError {
                code: 400,
                message: format!("canonicalize: {e}"),
            }
        })
    }

    fn capabilities(&self) -> CapabilityReport {
        CapabilityReport {
            max_payload_bytes: 65536,
            supports_fragmentation: true,
            supports_encryption: true,
            supports_raw_binary: true,
            rate_limit_per_second: 10000,
            media_capabilities: None,
        }
    }

    fn domain_id(&self, platform_id: &str) -> BroadcastDomainId {
        BroadcastDomainId::new(self.platform, platform_id)
    }

    fn platform_type(&self) -> PlatformType {
        self.platform
    }

    fn self_handle(&self) -> Option<String> {
        self.self_id.clone()
    }

    /// Bridge: opt in to `CoordinatorAdmin`. The mock advertises admin
    /// support so cross-module e2e flows can exercise the trait through
    /// the same `&dyn PlatformAdapter` entry point real callers use.
    fn as_coordinator_admin(&self) -> Option<&dyn CoordinatorAdmin> {
        Some(self)
    }
}

// ── CoordinatorAdmin impl ────────────────────────────────────────────
//
// The mock opts in to the admin trait so e2e flows can exercise
// `create_group` / `add_member` through the `as_coordinator_admin`
// bridge. The scriptable methods (create_group, add_member) honour
// `self.admin_scripted`; everything else falls through to the trait's
// default `Unimplemented` so tests see the same "not implemented"
// signal a real adapter with a partial admin impl would return.
//
// `platform_name` and `admin_capabilities` are always overridden so the
// capability bit-flags truthfully reflect what the mock scripts (RFC-0861
// §1 capability-report honesty rule).

#[async_trait::async_trait]
impl CoordinatorAdmin for MockPlatformAdapter {
    fn platform_name(&self) -> String {
        format!("mock-{:?}", self.platform)
            .to_lowercase()
            .replace('"', "")
    }

    fn admin_capabilities(&self) -> AdminCapabilityReport {
        // The capability report truthfully reflects what the mock
        // currently scripts. Tests that enable create_group and/or
        // add_member see the corresponding bits set; everything else
        // stays false (per RFC-0861 §1 honesty rule).
        let scripted = self
            .admin_scripted
            .try_lock()
            .map(|g| g.clone())
            .unwrap_or_default();
        AdminCapabilityReport {
            can_create: scripted.create_group.is_some(),
            can_add_member: scripted.add_member.is_some(),
            ..AdminCapabilityReport::default()
        }
    }

    async fn create_group(
        &self,
        subject: &str,
        initial_members: &[GroupMemberSpec],
    ) -> Result<GroupHandle, PlatformAdapterError> {
        self.admin_calls.lock().await.push(AdminCall::CreateGroup {
            subject: subject.to_string(),
            initial_member_count: initial_members.len(),
        });
        let scripted = self.admin_scripted.lock().await.clone();
        match scripted.create_group {
            Some(result) => result,
            None => Err(PlatformAdapterError::Unimplemented {
                platform: self.platform_name(),
                action: "create_group".into(),
            }),
        }
    }

    async fn add_member(
        &self,
        group_id: &GroupId,
        member: &GroupMemberSpec,
    ) -> Result<AddMemberOutput, PlatformAdapterError> {
        self.admin_calls.lock().await.push(AdminCall::AddMember {
            group_id: group_id.to_string(),
            member_handle: member.handle.clone(),
            member_is_admin: member.is_admin,
        });
        let scripted = self.admin_scripted.lock().await.clone();
        match scripted.add_member {
            Some(result) => result,
            None => Err(PlatformAdapterError::Unimplemented {
                platform: self.platform_name(),
                action: "add_member".into(),
            }),
        }
    }

    // All other methods inherit the trait's default `Unimplemented`
    // return. Tests that need them can be extended by adding more
    // fields to `AdminScripted`.
}
