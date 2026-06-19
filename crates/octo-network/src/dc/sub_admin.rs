//! Sub-admin designation (mission 0855p-c-sub-admins).
//!
//! DomainCoordinators can designate sub-admins (e.g., a deputy
//! admin in case the primary is unreachable); sub-admins can
//! sign envelopes but only within a `SubAdminAuthority` policy.
//!
//! ## Activation
//!
//! - `SUB_ADMIN_ACTIVATION_EPOCHS = 10` (~10 minutes of
//!   primary DC silence before the sub-admin activates)
//!
//! ## Authority
//!
//! - `CAN_BIND` (default yes) — sign BIND for new members
//! - `CAN_REBIND` (default no) — sign REBIND
//! - `CAN_UNBIND` (default no) — sign UNBIND
//! - `CAN_SLASH` (default no) — sign slash votes
//! - `CAN_ATTEST` (default yes) — publish PLATFORM_ADMIN_ATTEST

use bitflags::bitflags;
use serde::{Deserialize, Serialize};

/// Activation delay (per mission spec): 10 epochs.
pub const SUB_ADMIN_ACTIVATION_EPOCHS: u64 = 10;

bitflags! {
    /// Sub-admin authority bitfield.
    #[derive(Default, Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct SubAdminAuthority: u32 {
        /// Can sign BIND for new members.
        const CAN_BIND  = 0x01;
        /// Can sign REBIND.
        const CAN_REBIND = 0x02;
        /// Can sign UNBIND.
        const CAN_UNBIND = 0x04;
        /// Can sign slash votes.
        const CAN_SLASH  = 0x08;
        /// Can publish PLATFORM_ADMIN_ATTEST.
        const CAN_ATTEST = 0x10;
    }
}

// Manual serde impl: bitflags 2.x requires the `serde` feature
// for derived impls. We use a manual u32-backed impl to avoid
// the feature-gate.
impl Serialize for SubAdminAuthority {
    fn serialize<S: serde::Serializer>(&self, ser: S) -> Result<S::Ok, S::Error> {
        ser.serialize_u32(self.bits())
    }
}

impl<'de> Deserialize<'de> for SubAdminAuthority {
    fn deserialize<D: serde::Deserializer<'de>>(de: D) -> Result<Self, D::Error> {
        let bits = u32::deserialize(de)?;
        Ok(SubAdminAuthority::from_bits_truncate(bits))
    }
}

impl SubAdminAuthority {
    /// Returns the default authority per mission spec: CAN_BIND + CAN_ATTEST.
    pub fn default_per_mission() -> Self {
        SubAdminAuthority::CAN_BIND | SubAdminAuthority::CAN_ATTEST
    }
}

/// A sub-admin designation envelope.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubAdminDesignation {
    pub domain_id: String,
    pub primary_dc_pubkey: Vec<u8>,
    pub sub_admin_pubkey: Vec<u8>,
    pub authority: SubAdminAuthority,
    pub signed_at_epoch: u64,
    pub signature: Vec<u8>,
}

impl SubAdminDesignation {
    /// Returns true if the sub-admin has the given authority bit.
    pub fn has_authority(&self, auth: SubAdminAuthority) -> bool {
        self.authority.contains(auth)
    }
}

/// State of a sub-admin.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SubAdminState {
    /// Designated but not yet active (primary is reachable).
    Standby,
    /// Primary DC has been silent for SUB_ADMIN_ACTIVATION_EPOCHS;
    /// sub-admin is now active.
    Active,
    /// Primary returned; sub-admin is deactivated.
    Deactivated,
}

/// Check whether a sub-admin should be activated given the
/// primary DC's `last_heartbeat_epoch`.
///
/// Returns `true` (activate) if the primary has been silent for
/// >= SUB_ADMIN_ACTIVATION_EPOCHS.
pub fn should_activate_sub_admin(last_primary_heartbeat_epoch: u64, current_epoch: u64) -> bool {
    current_epoch.saturating_sub(last_primary_heartbeat_epoch) >= SUB_ADMIN_ACTIVATION_EPOCHS
}

/// Multi-sub-admin 2/3 vote: pick the active sub-admin from a
/// set of votes.
///
/// Each vote is `(sub_admin_pubkey, weight)`. The active
/// sub-admin is the one with the most weight; ties broken by
/// lower `pubkey`.
pub fn elect_active_sub_admin(
    votes: &[(Vec<u8>, u64)],
    total_sub_admins: usize,
) -> Option<Vec<u8>> {
    if votes.is_empty() || total_sub_admins == 0 {
        return None;
    }
    // Aggregate weights per sub_admin (saturating to defend against
    // hostile weight spam from a single voter's repeated entries).
    let mut agg: std::collections::HashMap<Vec<u8>, u64> = std::collections::HashMap::new();
    for (sa, w) in votes {
        let entry = agg.entry(sa.clone()).or_insert(0);
        *entry = entry.saturating_add(*w);
    }
    // 2/3 of total sub-admins must vote. Use saturating math to avoid
    // overflow on adversarial `total_sub_admins` values.
    let distinct: std::collections::HashSet<Vec<u8>> =
        votes.iter().map(|(k, _)| k.clone()).collect();
    if distinct.len().saturating_mul(3) < total_sub_admins.saturating_mul(2) {
        return None;
    }
    // Pick the highest-weight sub_admin; tie-break: lower pubkey
    // (lex order of bytes).
    let mut best: Option<(Vec<u8>, u64)> = None;
    for (sa, w) in agg {
        best = match best {
            None => Some((sa, w)),
            Some((_b_sa, b_w)) if w > b_w => Some((sa, w)),
            Some((b_sa, b_w)) if w == b_w && sa < b_sa => Some((sa, w)),
            Some(other) => Some(other),
        };
    }
    best.map(|(sa, _)| sa)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_authority_is_bind_and_attest() {
        let auth = SubAdminAuthority::default_per_mission();
        assert!(auth.contains(SubAdminAuthority::CAN_BIND));
        assert!(auth.contains(SubAdminAuthority::CAN_ATTEST));
        assert!(!auth.contains(SubAdminAuthority::CAN_REBIND));
        assert!(!auth.contains(SubAdminAuthority::CAN_UNBIND));
        assert!(!auth.contains(SubAdminAuthority::CAN_SLASH));
    }

    #[test]
    fn should_activate_after_silence() {
        assert!(!should_activate_sub_admin(1000, 1005)); // 5 epochs < 10
        assert!(should_activate_sub_admin(1000, 1010)); // 10 epochs >= 10
        assert!(should_activate_sub_admin(1000, 1100)); // 100 epochs
    }

    #[test]
    fn activate_constant() {
        assert_eq!(SUB_ADMIN_ACTIVATION_EPOCHS, 10);
    }

    #[test]
    fn designation_has_authority() {
        let d = SubAdminDesignation {
            domain_id: "d1".into(),
            primary_dc_pubkey: vec![0xAA],
            sub_admin_pubkey: vec![0xBB],
            authority: SubAdminAuthority::CAN_BIND | SubAdminAuthority::CAN_REBIND,
            signed_at_epoch: 0,
            signature: vec![],
        };
        assert!(d.has_authority(SubAdminAuthority::CAN_BIND));
        assert!(d.has_authority(SubAdminAuthority::CAN_REBIND));
        assert!(!d.has_authority(SubAdminAuthority::CAN_SLASH));
    }

    #[test]
    fn elect_active_sub_admin_2_of_3() {
        let votes = vec![(vec![0xAA], 1), (vec![0xBB], 1), (vec![0xCC], 1)];
        let sa = elect_active_sub_admin(&votes, 3).unwrap();
        // All have weight 1; tie-break: lower pubkey.
        assert_eq!(sa, vec![0xAA]);
    }

    #[test]
    fn elect_active_sub_admin_quorum_check() {
        let votes = vec![(vec![0xAA], 1)]; // only 1 of 3 voted
        let result = elect_active_sub_admin(&votes, 3);
        assert!(result.is_none());
    }

    #[test]
    fn elect_active_sub_admin_weighted() {
        let votes = vec![(vec![0xAA], 1), (vec![0xBB], 5), (vec![0xCC], 1)];
        let sa = elect_active_sub_admin(&votes, 3).unwrap();
        assert_eq!(sa, vec![0xBB]);
    }

    #[test]
    fn elect_active_sub_admin_empty() {
        assert!(elect_active_sub_admin(&[], 3).is_none());
    }

    #[test]
    fn authority_serde_roundtrip() {
        let auth = SubAdminAuthority::CAN_BIND | SubAdminAuthority::CAN_SLASH;
        let json = serde_json::to_string(&auth).unwrap();
        let back: SubAdminAuthority = serde_json::from_str(&json).unwrap();
        assert_eq!(back, auth);
    }
}
