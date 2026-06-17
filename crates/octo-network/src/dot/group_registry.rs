//! `GroupRegistry` — RFC-0850p-c §1, §B
//!
//! The `GroupRegistry` is the local node's view of all transport group
//! bindings on the DOT mesh. It maintains:
//!
//! - `bindings: BTreeMap<(platform, group_jid), GroupBinding>` — the
//!   primary index, keyed by platform + group JID
//! - `domain_index: BTreeMap<(mission_id, domain_id, platform), (platform, group_jid)>`
//!   — the reverse index, used to enforce the multi-platform rule
//!   (RFC-0850p-c §5): at most one group per `(mission_id, domain_id,
//!   platform)` triple
//! - `unbound_quarantine: BTreeMap<UnboundQuarantineKey, UnboundQuarantineEntry>`
//!   — bindings that have been moved to `UnboundQuarantined` state,
//!   preserved for `REJOIN_GRANT_TIMEOUT = 50` epochs to allow rejoin
//! - `rejoin_attempts: BTreeMap<[u8; 32], u16>` — counts rejoin attempts
//!   per kicked node (per RFC-0850p-e §"REJOIN flow"; default cap
//!   `MAX_REJOIN_ATTEMPTS = 3`)
//!
//! The registry is shared across adapters (one registry per node, not
//! per adapter) so that BIND/UNBIND events on one platform are visible
//! to all other adapters.
//!
//! See mission `missions/claimed/0850p-c-base.md` (Phase 2) and
//! `missions/claimed/0850p-e-kick-detection.md` (Phase 2 — `unbound_quarantine`)
//! for the full requirements.

use std::collections::BTreeMap;

use super::binding::{
    BindingError, GroupBinding, GroupState, UnbindAuthority, UnbindEnvelope,
};
use super::dc_envelopes::InviteEnvelope;

/// Default recovery window for the `unbound_quarantine` map, in epochs.
///
/// After `REJOIN_GRANT_TIMEOUT = 50` epochs, the entry is purged by the
/// GC sweep. This matches the `REJOIN_GRANT_TIMEOUT` constant used by
/// the DC orchestrator (see mission 0850p-e §"REJOIN flow").
pub const REJOIN_GRANT_TIMEOUT: u64 = 50;

/// Default cap on rejoin attempts per kicked node, per mission 0850p-e.
pub const DEFAULT_MAX_REJOIN_ATTEMPTS: u16 = 3;

/// Composite key for the `unbound_quarantine` map.
pub type UnboundQuarantineKey = ([u8; 32], [u8; 32], String); // (mission_id, domain_id, platform)

/// A quarantined binding entry.
///
/// The original `GroupBinding` is preserved (minus the state) for the
/// duration of the recovery window so that a successful re-BIND can
/// restore the binding atomically.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnboundQuarantineEntry {
    /// Epoch at which the binding was moved to `UnboundQuarantined`.
    pub unbound_at_epoch: u64,
    /// Recovery window in epochs (typically `REJOIN_GRANT_TIMEOUT = 50`).
    pub recovery_window_epochs: u64,
    /// The original `GroupBinding` (without the state).
    pub original_binding: GroupBinding,
    /// Authority that caused the unbind (kick, slash, etc.).
    pub unbind_authority: UnbindAuthority,
}

/// The transport group binding registry.
#[derive(Debug, Clone, Default)]
pub struct GroupRegistry {
    /// Primary index: `(platform, group_jid) -> GroupBinding`.
    bindings: BTreeMap<(String, String), GroupBinding>,
    /// Reverse index: `(mission_id, domain_id, platform) -> (platform, group_jid)`.
    /// Used to enforce the multi-platform rule.
    domain_index: BTreeMap<([u8; 32], [u8; 32], String), (String, String)>,
    /// Quarantined bindings awaiting REJOIN (RFC-0850p-e §"unbound_quarantine").
    unbound_quarantine: BTreeMap<UnboundQuarantineKey, UnboundQuarantineEntry>,
    /// Rejoin attempt counter per kicked-node id (RFC-0850p-e §"REJOIN flow").
    rejoin_attempts: BTreeMap<[u8; 32], u16>,
    /// Pending invite envelopes (RFC-0850p-d Phase 2 — `Inviting` state).
    /// Keyed by the invitee's public key. A binding in `Inviting` state
    /// has at least one entry in this map; transition to `Bound` happens
    /// when all invites are acknowledged or expire.
    pending_invites: BTreeMap<[u8; 32], InviteEnvelope>,
}

impl GroupRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a new binding. Validates the multi-platform rule.
    ///
    /// Returns `Err(BindingError::AlreadyBound)` if a binding already
    /// exists for `(platform, group_jid)`, and
    /// `Err(BindingError::MultiPlatformViolation)` if a different group
    /// is already bound for `(mission_id, domain_id, platform)`.
    pub fn register_binding(&mut self, binding: GroupBinding) -> Result<(), BindingError> {
        let key = (binding.platform.clone(), binding.group_jid.clone());

        if self.bindings.contains_key(&key) {
            return Err(BindingError::AlreadyBound {
                platform: binding.platform,
                group_jid: binding.group_jid,
            });
        }

        let domain_key = (
            binding.mission_id,
            binding.domain_id,
            binding.platform.clone(),
        );
        if self.domain_index.contains_key(&domain_key) {
            return Err(BindingError::MultiPlatformViolation {
                mission_id: binding.mission_id,
                domain_id: binding.domain_id,
                platform: binding.platform,
            });
        }

        self.domain_index
            .insert(domain_key, key.clone());
        self.bindings.insert(key, binding);
        Ok(())
    }

    /// Look up a binding by `(platform, group_jid)`.
    pub fn lookup_by_group(&self, platform: &str, group_jid: &str) -> Option<&GroupBinding> {
        self.bindings.get(&(platform.to_string(), group_jid.to_string()))
    }

    /// Look up a binding by `(mission_id, domain_id, platform)` reverse
    /// index.
    pub fn lookup_by_domain(
        &self,
        mission_id: &[u8; 32],
        domain_id: &[u8; 32],
        platform: &str,
    ) -> Option<&GroupBinding> {
        let key = (*mission_id, *domain_id, platform.to_string());
        let (platform, group_jid) = self.domain_index.get(&key)?;
        self.bindings.get(&(platform.clone(), group_jid.clone()))
    }

    /// Number of registered bindings.
    pub fn len(&self) -> usize {
        self.bindings.len()
    }

    /// `true` if no bindings are registered.
    pub fn is_empty(&self) -> bool {
        self.bindings.is_empty()
    }

    /// Iterate over all bindings.
    pub fn iter(&self) -> impl Iterator<Item = (&(String, String), &GroupBinding)> {
        self.bindings.iter()
    }

    // -------------------------------------------------------------------------
    // State transitions (RFC-0850p-c §1 "Binding State Machine")
    // -------------------------------------------------------------------------

    /// Transition a binding to `Bound`. The binding must already exist;
    /// the transition updates `state`, `renewed_at_epoch`, and
    /// `binding_hash`.
    pub fn transition_to_bound(
        &mut self,
        platform: &str,
        group_jid: &str,
        renewed_at_epoch: u64,
        binding_hash: [u8; 32],
    ) -> Result<(), BindingError> {
        let key = (platform.to_string(), group_jid.to_string());
        let binding = self.bindings.get_mut(&key).ok_or(BindingError::NotFound {
            platform: platform.to_string(),
            group_jid: group_jid.to_string(),
        })?;

        match binding.state {
            GroupState::Unbound
            | GroupState::ReBinding
            | GroupState::UnboundQuarantined
            | GroupState::Bound
            | GroupState::Creating
            | GroupState::Inviting
            | GroupState::UnboundAllPending => {
                binding.state = GroupState::Bound;
                binding.renewed_at_epoch = renewed_at_epoch;
                binding.binding_hash = binding_hash;
                Ok(())
            }
            GroupState::UnboundAllDone => Err(BindingError::InvalidTransition {
                from: binding.state,
                to: GroupState::Bound,
            }),
        }
    }

    /// Transition a binding to `ReBinding`.
    pub fn transition_to_rebinding(
        &mut self,
        platform: &str,
        group_jid: &str,
    ) -> Result<(), BindingError> {
        let key = (platform.to_string(), group_jid.to_string());
        let binding = self.bindings.get_mut(&key).ok_or(BindingError::NotFound {
            platform: platform.to_string(),
            group_jid: group_jid.to_string(),
        })?;
        match binding.state {
            GroupState::Bound
            | GroupState::ReBinding
            | GroupState::UnboundQuarantined
            | GroupState::Inviting
            | GroupState::UnboundAllPending => {
                binding.state = GroupState::ReBinding;
                Ok(())
            }
            GroupState::Unbound | GroupState::Creating | GroupState::UnboundAllDone => {
                Err(BindingError::InvalidTransition {
                    from: binding.state,
                    to: GroupState::ReBinding,
                })
            }
        }
    }

    /// Transition a binding to `UnboundQuarantined`. The original binding
    /// is moved to the `unbound_quarantine` map for the configured
    /// recovery window.
    ///
    /// The `domain_index` entry is also removed so that the
    /// `(mission_id, domain_id, platform)` triple can be re-bound
    /// after restoration. The reverse index is re-added on
    /// `restore_from_quarantine`.
    pub fn transition_to_unbound_quarantined(
        &mut self,
        platform: &str,
        group_jid: &str,
        unbound_at_epoch: u64,
        authority: UnbindAuthority,
        recovery_window_epochs: u64,
    ) -> Result<(), BindingError> {
        let key = (platform.to_string(), group_jid.to_string());
        let binding = self.bindings.remove(&key).ok_or(BindingError::NotFound {
            platform: platform.to_string(),
            group_jid: group_jid.to_string(),
        })?;

        let domain_key = (
            binding.mission_id,
            binding.domain_id,
            binding.platform.clone(),
        );
        self.domain_index.remove(&domain_key);

        let entry = UnboundQuarantineEntry {
            unbound_at_epoch,
            recovery_window_epochs,
            original_binding: binding,
            unbind_authority: authority,
        };
        self.unbound_quarantine.insert(domain_key, entry);
        Ok(())
    }

    /// Transition a binding to `Unbound` (full unbinding; the binding is
    /// removed entirely).
    pub fn transition_to_unbound(
        &mut self,
        platform: &str,
        group_jid: &str,
    ) -> Result<UnbindEnvelope, BindingError> {
        let key = (platform.to_string(), group_jid.to_string());
        let binding = self.bindings.remove(&key).ok_or(BindingError::NotFound {
            platform: platform.to_string(),
            group_jid: group_jid.to_string(),
        })?;

        let domain_key = (
            binding.mission_id,
            binding.domain_id,
            binding.platform.clone(),
        );
        self.domain_index.remove(&domain_key);

        // Build a synthetic UnbindEnvelope for the caller to sign.
        // (The actual signing happens outside the registry; the caller
        // uses the returned envelope to drive the ceremony.)
        Ok(UnbindEnvelope {
            domain_id: binding.domain_id,
            group_jid: binding.group_jid,
            platform: binding.platform,
            authority: UnbindAuthority::CoordinatorResign,
            reason: String::new(),
            current_epoch: binding.renewed_at_epoch,
            nonce: [0u8; 32],
            unbind_hash: [0u8; 32],
            signature: [0u8; 64],
        })
    }

    // -------------------------------------------------------------------------
    // Quarantine helpers (RFC-0850p-e §"unbound_quarantine")
    // -------------------------------------------------------------------------

    /// Look up a quarantined entry.
    pub fn lookup_quarantine(
        &self,
        mission_id: &[u8; 32],
        domain_id: &[u8; 32],
        platform: &str,
    ) -> Option<&UnboundQuarantineEntry> {
        self.unbound_quarantine
            .get(&(*mission_id, *domain_id, platform.to_string()))
    }

    /// Restore a binding from quarantine (called on `REJOIN_GRANT`).
    /// Returns `QuarantineExpired` if the recovery window has passed.
    pub fn restore_from_quarantine(
        &mut self,
        mission_id: &[u8; 32],
        domain_id: &[u8; 32],
        platform: &str,
        current_epoch: u64,
    ) -> Result<GroupBinding, BindingError> {
        let key = (*mission_id, *domain_id, platform.to_string());
        let entry = self
            .unbound_quarantine
            .remove(&key)
            .ok_or_else(|| BindingError::NotFound {
                platform: platform.to_string(),
                group_jid: "<quarantine>".to_string(),
            })?;

        if current_epoch
            >= entry.unbound_at_epoch.saturating_add(entry.recovery_window_epochs)
        {
            return Err(BindingError::QuarantineExpired {
                platform: platform.to_string(),
                group_jid: entry.original_binding.group_jid,
            });
        }

        let mut binding = entry.original_binding;
        binding.state = GroupState::Bound;
        binding.renewed_at_epoch = current_epoch;
        self.register_binding(binding.clone())?;
        Ok(binding)
    }

    /// Purge quarantine entries whose recovery window has expired.
    /// Returns the number of entries purged.
    pub fn gc_quarantine(&mut self, current_epoch: u64) -> usize {
        let expired: Vec<UnboundQuarantineKey> = self
            .unbound_quarantine
            .iter()
            .filter(|(_, entry)| {
                current_epoch
                    >= entry
                        .unbound_at_epoch
                        .saturating_add(entry.recovery_window_epochs)
            })
            .map(|(k, _)| k.clone())
            .collect();
        let n = expired.len();
        for k in expired {
            self.unbound_quarantine.remove(&k);
        }
        n
    }

    /// Number of quarantined entries.
    pub fn quarantine_len(&self) -> usize {
        self.unbound_quarantine.len()
    }

    // -------------------------------------------------------------------------
    // Rejoin attempt helpers (RFC-0850p-e §"REJOIN flow")
    // -------------------------------------------------------------------------

    /// Increment the rejoin attempt counter for a kicked node and return
    /// the new value. Returns `Err(NonceReplay)` (re-using the error
    /// variant, since both are "too many attempts") if the count exceeds
    /// `max_attempts`.
    ///
    /// The counter is keyed by the kicked node's peer id (not group_jid)
    /// so a node cannot reset its counter by changing groups.
    pub fn try_increment_rejoin(
        &mut self,
        node_id: &[u8; 32],
        max_attempts: u16,
    ) -> Result<u16, BindingError> {
        let count = self.rejoin_attempts.entry(*node_id).or_insert(0);
        *count = count.saturating_add(1);
        if *count > max_attempts {
            return Err(BindingError::NonceReplay { nonce: *node_id });
        }
        Ok(*count)
    }

    /// Reset the rejoin attempt counter for a node (e.g., after a
    /// successful re-BIND).
    pub fn reset_rejoin(&mut self, node_id: &[u8; 32]) {
        self.rejoin_attempts.remove(node_id);
    }

    // -------------------------------------------------------------------------
    // DC-initiated group state transitions (RFC-0850p-d Phase 2)
    // -------------------------------------------------------------------------

    /// Transition a binding to `Creating`. The binding must be in
    /// `Unbound` or `ReBinding` state.
    pub fn transition_to_creating(
        &mut self,
        platform: &str,
        group_jid: &str,
    ) -> Result<(), BindingError> {
        let key = (platform.to_string(), group_jid.to_string());
        let binding = self.bindings.get_mut(&key).ok_or(BindingError::NotFound {
            platform: platform.to_string(),
            group_jid: group_jid.to_string(),
        })?;
        match binding.state {
            GroupState::Unbound | GroupState::ReBinding => {
                binding.state = GroupState::Creating;
                Ok(())
            }
            other => Err(BindingError::InvalidTransition {
                from: other,
                to: GroupState::Creating,
            }),
        }
    }

    /// Transition a binding to `Inviting` (called on first `INVITE`
    /// emission).
    pub fn transition_to_inviting(
        &mut self,
        platform: &str,
        group_jid: &str,
    ) -> Result<(), BindingError> {
        let key = (platform.to_string(), group_jid.to_string());
        let binding = self.bindings.get_mut(&key).ok_or(BindingError::NotFound {
            platform: platform.to_string(),
            group_jid: group_jid.to_string(),
        })?;
        match binding.state {
            GroupState::Bound | GroupState::Inviting => {
                binding.state = GroupState::Inviting;
                Ok(())
            }
            other => Err(BindingError::InvalidTransition {
                from: other,
                to: GroupState::Inviting,
            }),
        }
    }

    /// Transition a binding from `Inviting` back to `Bound` (all invites
    /// acknowledged or expired).
    pub fn transition_inviting_to_bound(
        &mut self,
        platform: &str,
        group_jid: &str,
    ) -> Result<(), BindingError> {
        let key = (platform.to_string(), group_jid.to_string());
        let binding = self.bindings.get_mut(&key).ok_or(BindingError::NotFound {
            platform: platform.to_string(),
            group_jid: group_jid.to_string(),
        })?;
        match binding.state {
            GroupState::Inviting | GroupState::Bound => {
                binding.state = GroupState::Bound;
                Ok(())
            }
            other => Err(BindingError::InvalidTransition {
                from: other,
                to: GroupState::Bound,
            }),
        }
    }

    // -------------------------------------------------------------------------
    // Pending invites (RFC-0850p-d Phase 2)
    // -------------------------------------------------------------------------

    /// Register a pending invite. Returns `true` if the invite was new,
    /// `false` if it replaced an existing one for the same invitee.
    pub fn register_pending_invite(&mut self, invite: InviteEnvelope) -> bool {
        let key = invite.invitee_pubkey;
        self.pending_invites.insert(key, invite).is_none()
    }

    /// Remove a pending invite (on acknowledgement or expiry).
    /// Returns the removed invite if it existed.
    pub fn remove_pending_invite(&mut self, invitee: &[u8; 32]) -> Option<InviteEnvelope> {
        self.pending_invites.remove(invitee)
    }

    /// Look up a pending invite.
    pub fn lookup_pending_invite(&self, invitee: &[u8; 32]) -> Option<&InviteEnvelope> {
        self.pending_invites.get(invitee)
    }

    /// Number of pending invites.
    pub fn pending_invites_len(&self) -> usize {
        self.pending_invites.len()
    }

    /// Remove all expired pending invites (invites whose `expires_at_epoch`
    /// is <= `current_epoch`). Returns the number of invites removed.
    pub fn gc_pending_invites(&mut self, current_epoch: u64) -> usize {
        let expired: Vec<[u8; 32]> = self
            .pending_invites
            .iter()
            .filter(|(_, inv)| inv.expires_at_epoch <= current_epoch)
            .map(|(k, _)| *k)
            .collect();
        let n = expired.len();
        for k in expired {
            self.pending_invites.remove(&k);
        }
        n
    }

    // -------------------------------------------------------------------------
    // Decommission transitions (RFC-0850p-f Phase 2)
    // -------------------------------------------------------------------------

    /// Transition a binding to `UnboundAllPending` (DC broadcasts
    /// `UnbindAllEnvelope`; awaiting ACK from all members).
    pub fn transition_to_unbound_all_pending(
        &mut self,
        platform: &str,
        group_jid: &str,
    ) -> Result<(), BindingError> {
        let key = (platform.to_string(), group_jid.to_string());
        let binding = self.bindings.get_mut(&key).ok_or(BindingError::NotFound {
            platform: platform.to_string(),
            group_jid: group_jid.to_string(),
        })?;
        match binding.state {
            GroupState::Bound
            | GroupState::Inviting
            | GroupState::ReBinding
            | GroupState::UnboundAllPending => {
                binding.state = GroupState::UnboundAllPending;
                Ok(())
            }
            other => Err(BindingError::InvalidTransition {
                from: other,
                to: GroupState::UnboundAllPending,
            }),
        }
    }

    /// Transition a binding to `UnboundAllDone` (terminal; all members
    /// have left the platform). The binding is removed from the
    /// registry; a synthetic `UnbindEnvelope` is returned for the
    /// caller to use in constructing the corresponding
    /// `UnbindAllDoneEnvelope`.
    pub fn transition_to_unbound_all_done(
        &mut self,
        platform: &str,
        group_jid: &str,
    ) -> Result<UnbindEnvelope, BindingError> {
        let key = (platform.to_string(), group_jid.to_string());
        let binding = self.bindings.remove(&key).ok_or(BindingError::NotFound {
            platform: platform.to_string(),
            group_jid: group_jid.to_string(),
        })?;
        let domain_key = (
            binding.mission_id,
            binding.domain_id,
            binding.platform.clone(),
        );
        self.domain_index.remove(&domain_key);
        Ok(UnbindEnvelope {
            domain_id: binding.domain_id,
            group_jid: binding.group_jid,
            platform: binding.platform,
            authority: UnbindAuthority::CoordinatorResign,
            reason: String::new(),
            current_epoch: binding.renewed_at_epoch,
            nonce: [0u8; 32],
            unbind_hash: [0u8; 32],
            signature: [0u8; 64],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_binding(
        platform: &str,
        group_jid: &str,
        mission_id: [u8; 32],
        domain_id: [u8; 32],
    ) -> GroupBinding {
        GroupBinding {
            group_jid: group_jid.into(),
            platform: platform.into(),
            mission_id,
            domain_id,
            domain_coordinator_id: [3u8; 32],
            bound_at_epoch: 100,
            renewed_at_epoch: 100,
            state: GroupState::Unbound,
            binding_hash: [0u8; 32],
        }
    }

    #[test]
    fn register_and_lookup() {
        let mut reg = GroupRegistry::new();
        let b = make_binding("whatsapp", "g1", [1u8; 32], [2u8; 32]);
        reg.register_binding(b).unwrap();
        assert_eq!(reg.len(), 1);
        assert!(reg.lookup_by_group("whatsapp", "g1").is_some());
        assert!(reg.lookup_by_domain(&[1u8; 32], &[2u8; 32], "whatsapp").is_some());
    }

    #[test]
    fn register_rejects_duplicate() {
        let mut reg = GroupRegistry::new();
        let b1 = make_binding("whatsapp", "g1", [1u8; 32], [2u8; 32]);
        reg.register_binding(b1).unwrap();
        let b2 = make_binding("whatsapp", "g1", [1u8; 32], [2u8; 32]);
        assert!(matches!(
            reg.register_binding(b2),
            Err(BindingError::AlreadyBound { .. })
        ));
    }

    #[test]
    fn register_rejects_multi_platform_violation() {
        let mut reg = GroupRegistry::new();
        let b1 = make_binding("whatsapp", "g1", [1u8; 32], [2u8; 32]);
        reg.register_binding(b1).unwrap();
        // Different group_jid, same (mission, domain, platform) — forbidden.
        let b2 = make_binding("whatsapp", "g2", [1u8; 32], [2u8; 32]);
        assert!(matches!(
            reg.register_binding(b2),
            Err(BindingError::MultiPlatformViolation { .. })
        ));
        // Same (mission, domain, platform=matrix) IS allowed.
        let b3 = make_binding("matrix", "m1", [1u8; 32], [2u8; 32]);
        reg.register_binding(b3).unwrap();
        assert_eq!(reg.len(), 2);
    }

    #[test]
    fn transition_to_bound_updates_state() {
        let mut reg = GroupRegistry::new();
        let b = make_binding("whatsapp", "g1", [1u8; 32], [2u8; 32]);
        reg.register_binding(b).unwrap();
        reg.transition_to_bound("whatsapp", "g1", 200, [42u8; 32])
            .unwrap();
        let b = reg.lookup_by_group("whatsapp", "g1").unwrap();
        assert_eq!(b.state, GroupState::Bound);
        assert_eq!(b.renewed_at_epoch, 200);
        assert_eq!(b.binding_hash, [42u8; 32]);
    }

    #[test]
    fn transition_to_rebinding_from_unbound_rejected() {
        let mut reg = GroupRegistry::new();
        let b = make_binding("whatsapp", "g1", [1u8; 32], [2u8; 32]);
        reg.register_binding(b).unwrap();
        // State is Unbound; cannot transition to ReBinding.
        assert!(matches!(
            reg.transition_to_rebinding("whatsapp", "g1"),
            Err(BindingError::InvalidTransition { .. })
        ));
    }

    #[test]
    fn transition_to_rebinding_from_bound_ok() {
        let mut reg = GroupRegistry::new();
        let b = make_binding("whatsapp", "g1", [1u8; 32], [2u8; 32]);
        reg.register_binding(b).unwrap();
        reg.transition_to_bound("whatsapp", "g1", 200, [0u8; 32])
            .unwrap();
        reg.transition_to_rebinding("whatsapp", "g1").unwrap();
        let b = reg.lookup_by_group("whatsapp", "g1").unwrap();
        assert_eq!(b.state, GroupState::ReBinding);
    }

    #[test]
    fn transition_to_unbound_quarantined_moves_to_quarantine() {
        let mut reg = GroupRegistry::new();
        let b = make_binding("whatsapp", "g1", [1u8; 32], [2u8; 32]);
        reg.register_binding(b).unwrap();
        reg.transition_to_unbound_quarantined(
            "whatsapp",
            "g1",
            1000,
            UnbindAuthority::SlashVote,
            REJOIN_GRANT_TIMEOUT,
        )
        .unwrap();
        assert_eq!(reg.len(), 0);
        assert_eq!(reg.quarantine_len(), 1);
        let entry = reg
            .lookup_quarantine(&[1u8; 32], &[2u8; 32], "whatsapp")
            .unwrap();
        assert_eq!(entry.unbind_authority, UnbindAuthority::SlashVote);
        assert_eq!(entry.recovery_window_epochs, 50);
    }

    #[test]
    fn restore_from_quarantine_within_window() {
        let mut reg = GroupRegistry::new();
        let b = make_binding("whatsapp", "g1", [1u8; 32], [2u8; 32]);
        reg.register_binding(b).unwrap();
        reg.transition_to_unbound_quarantined(
            "whatsapp",
            "g1",
            1000,
            UnbindAuthority::SlashVote,
            REJOIN_GRANT_TIMEOUT,
        )
        .unwrap();
        // Within window (50 epochs)
        let restored = reg
            .restore_from_quarantine(&[1u8; 32], &[2u8; 32], "whatsapp", 1030)
            .unwrap();
        assert_eq!(restored.state, GroupState::Bound);
        assert_eq!(restored.renewed_at_epoch, 1030);
        assert_eq!(reg.quarantine_len(), 0);
        assert_eq!(reg.len(), 1);
    }

    #[test]
    fn restore_from_quarantine_after_expiry_rejected() {
        let mut reg = GroupRegistry::new();
        let b = make_binding("whatsapp", "g1", [1u8; 32], [2u8; 32]);
        reg.register_binding(b).unwrap();
        reg.transition_to_unbound_quarantined(
            "whatsapp",
            "g1",
            1000,
            UnbindAuthority::SlashVote,
            REJOIN_GRANT_TIMEOUT,
        )
        .unwrap();
        // Past the 50-epoch window
        assert!(matches!(
            reg.restore_from_quarantine(&[1u8; 32], &[2u8; 32], "whatsapp", 1100),
            Err(BindingError::QuarantineExpired { .. })
        ));
    }

    #[test]
    fn gc_quarantine_purges_expired() {
        let mut reg = GroupRegistry::new();
        let b1 = make_binding("whatsapp", "g1", [1u8; 32], [2u8; 32]);
        let b2 = make_binding("matrix", "m1", [1u8; 32], [3u8; 32]);
        reg.register_binding(b1).unwrap();
        reg.register_binding(b2).unwrap();
        reg.transition_to_unbound_quarantined(
            "whatsapp",
            "g1",
            1000,
            UnbindAuthority::SlashVote,
            REJOIN_GRANT_TIMEOUT,
        )
        .unwrap();
        reg.transition_to_unbound_quarantined(
            "matrix",
            "m1",
            1100,
            UnbindAuthority::CoordinatorResign,
            REJOIN_GRANT_TIMEOUT,
        )
        .unwrap();
        assert_eq!(reg.quarantine_len(), 2);
        let purged = reg.gc_quarantine(1100);
        // g1's window: 1000..1050; at epoch 1100 it's expired.
        // m1's window: 1100..1150; at epoch 1100 it's still active.
        assert_eq!(purged, 1);
        assert_eq!(reg.quarantine_len(), 1);
    }

    #[test]
    fn try_increment_rejoin_caps_at_max() {
        let mut reg = GroupRegistry::new();
        let node_id = [1u8; 32];
        for i in 1..=3 {
            assert_eq!(reg.try_increment_rejoin(&node_id, 3).unwrap(), i);
        }
        // Fourth attempt must fail.
        assert!(reg.try_increment_rejoin(&node_id, 3).is_err());
        // Reset clears the counter.
        reg.reset_rejoin(&node_id);
        assert_eq!(reg.try_increment_rejoin(&node_id, 3).unwrap(), 1);
    }

    // -- DC-initiated transitions (RFC-0850p-d Phase 2) ---------------------

    fn make_invite(invitee: [u8; 32], expires: u64) -> InviteEnvelope {
        InviteEnvelope {
            domain_id: [1u8; 32],
            group_jid: "g1@g.us".into(),
            platform: "whatsapp".into(),
            invitee_pubkey: invitee,
            nonce: [7u8; 32],
            invite_token: [0u8; 32],
            mission_id: [2u8; 32],
            current_epoch: 100,
            expires_at_epoch: expires,
            invite_hash: [0u8; 32],
            signature: [0u8; 64],
        }
    }

    #[test]
    fn transition_to_creating_from_unbound_ok() {
        let mut reg = GroupRegistry::new();
        let b = make_binding("whatsapp", "g1", [1u8; 32], [2u8; 32]);
        reg.register_binding(b).unwrap();
        reg.transition_to_creating("whatsapp", "g1").unwrap();
        let b = reg.lookup_by_group("whatsapp", "g1").unwrap();
        assert_eq!(b.state, GroupState::Creating);
    }

    #[test]
    fn transition_to_creating_from_bound_rejected() {
        let mut reg = GroupRegistry::new();
        let b = make_binding("whatsapp", "g1", [1u8; 32], [2u8; 32]);
        reg.register_binding(b).unwrap();
        reg.transition_to_bound("whatsapp", "g1", 100, [0u8; 32])
            .unwrap();
        assert!(matches!(
            reg.transition_to_creating("whatsapp", "g1"),
            Err(BindingError::InvalidTransition { .. })
        ));
    }

    #[test]
    fn transition_to_inviting_from_bound_ok() {
        let mut reg = GroupRegistry::new();
        let b = make_binding("whatsapp", "g1", [1u8; 32], [2u8; 32]);
        reg.register_binding(b).unwrap();
        reg.transition_to_bound("whatsapp", "g1", 100, [0u8; 32])
            .unwrap();
        reg.transition_to_inviting("whatsapp", "g1").unwrap();
        let b = reg.lookup_by_group("whatsapp", "g1").unwrap();
        assert_eq!(b.state, GroupState::Inviting);
    }

    #[test]
    fn transition_inviting_to_bound() {
        let mut reg = GroupRegistry::new();
        let b = make_binding("whatsapp", "g1", [1u8; 32], [2u8; 32]);
        reg.register_binding(b).unwrap();
        reg.transition_to_bound("whatsapp", "g1", 100, [0u8; 32])
            .unwrap();
        reg.transition_to_inviting("whatsapp", "g1").unwrap();
        reg.transition_inviting_to_bound("whatsapp", "g1")
            .unwrap();
        let b = reg.lookup_by_group("whatsapp", "g1").unwrap();
        assert_eq!(b.state, GroupState::Bound);
    }

    #[test]
    fn pending_invite_register_lookup_remove() {
        let mut reg = GroupRegistry::new();
        let inv = make_invite([1u8; 32], 200);
        assert!(reg.register_pending_invite(inv.clone()));
        // Re-registering the same invitee returns false.
        assert!(!reg.register_pending_invite(inv));
        assert_eq!(reg.pending_invites_len(), 1);
        assert!(reg.lookup_pending_invite(&[1u8; 32]).is_some());
        let removed = reg.remove_pending_invite(&[1u8; 32]).unwrap();
        assert_eq!(removed.invitee_pubkey, [1u8; 32]);
        assert_eq!(reg.pending_invites_len(), 0);
    }

    #[test]
    fn gc_pending_invites_purges_expired() {
        let mut reg = GroupRegistry::new();
        reg.register_pending_invite(make_invite([1u8; 32], 100));
        reg.register_pending_invite(make_invite([2u8; 32], 200));
        reg.register_pending_invite(make_invite([3u8; 32], 300));
        // At epoch 250, invites 1 and 2 are expired.
        let purged = reg.gc_pending_invites(250);
        assert_eq!(purged, 2);
        assert_eq!(reg.pending_invites_len(), 1);
    }

    // -------------------------------------------------------------------------
    // Decommission transitions (RFC-0850p-f Phase 2)
    // -------------------------------------------------------------------------

    #[test]
    fn transition_to_unbound_all_pending_from_bound() {
        let mut reg = GroupRegistry::new();
        reg.register_binding(make_binding("wa", "g1", [1u8; 32], [2u8; 32]))
            .unwrap();
        reg.transition_to_bound("wa", "g1", 0, [0u8; 32]).unwrap();
        assert!(reg
            .transition_to_unbound_all_pending("wa", "g1")
            .is_ok());
        assert_eq!(
            reg.lookup_by_group("wa", "g1").unwrap().state,
            GroupState::UnboundAllPending,
        );
    }

    #[test]
    fn transition_to_unbound_all_pending_idempotent() {
        let mut reg = GroupRegistry::new();
        reg.register_binding(make_binding("wa", "g1", [1u8; 32], [2u8; 32]))
            .unwrap();
        reg.transition_to_bound("wa", "g1", 0, [0u8; 32]).unwrap();
        reg.transition_to_unbound_all_pending("wa", "g1").unwrap();
        // Second call should be idempotent (UnboundAllPending in OK arm).
        assert!(reg
            .transition_to_unbound_all_pending("wa", "g1")
            .is_ok());
    }

    #[test]
    fn transition_to_unbound_all_pending_rejects_unbound() {
        let mut reg = GroupRegistry::new();
        reg.register_binding(make_binding("wa", "g1", [1u8; 32], [2u8; 32]))
            .unwrap();
        reg.transition_to_bound("wa", "g1", 0, [0u8; 32]).unwrap();
        // transition_to_unbound removes the binding; follow-up returns NotFound.
        reg.transition_to_unbound("wa", "g1").unwrap();
        assert!(matches!(
            reg.transition_to_unbound_all_pending("wa", "g1"),
            Err(BindingError::NotFound { .. })
        ));
    }

    #[test]
    fn transition_to_unbound_all_pending_not_found() {
        let mut reg = GroupRegistry::new();
        assert!(matches!(
            reg.transition_to_unbound_all_pending("wa", "nope"),
            Err(BindingError::NotFound { .. })
        ));
    }

    #[test]
    fn transition_to_unbound_all_done_removes_binding() {
        let mut reg = GroupRegistry::new();
        reg.register_binding(make_binding("wa", "g1", [1u8; 32], [2u8; 32]))
            .unwrap();
        reg.transition_to_bound("wa", "g1", 0, [0u8; 32]).unwrap();
        reg.transition_to_unbound_all_pending("wa", "g1").unwrap();
        let result = reg.transition_to_unbound_all_done("wa", "g1");
        assert!(result.is_ok());
        // Binding is removed; domain_index also cleared.
        assert!(reg.lookup_by_group("wa", "g1").is_none());
        assert_eq!(reg.len(), 0);
    }

    #[test]
    fn transition_to_unbound_all_done_rejects_done() {
        let mut reg = GroupRegistry::new();
        reg.register_binding(make_binding("wa", "g1", [1u8; 32], [2u8; 32]))
            .unwrap();
        reg.transition_to_bound("wa", "g1", 0, [0u8; 32]).unwrap();
        reg.transition_to_unbound_all_pending("wa", "g1").unwrap();
        reg.transition_to_unbound_all_done("wa", "g1").unwrap();
        // Second call: binding is gone -> NotFound.
        assert!(matches!(
            reg.transition_to_unbound_all_done("wa", "g1"),
            Err(BindingError::NotFound { .. })
        ));
    }
}
