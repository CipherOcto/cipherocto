//! Recipient reference (RFC-0871 §Data Structures).
//!
//! Three variants: direct node-id, domain-scoped fan-out, or mesh broadcast.
//! No central enum for the underlying target — `Direct` carries the raw 32-byte
//! node id; `Domain` carries a WireDid (canonical per RFC-0010).

use borsh::{BorshDeserialize, BorshSerialize};

use octo_ident::WireDid;

/// Where the envelope should be routed. Per RFC-0871 §Data Structures:
/// - `Direct([u8; 32])`: a specific node id.
/// - `Domain(WireDid)`: any node serving the given DID's domain (fan-out).
/// - `Broadcast`: mesh-wide broadcast.
#[derive(Clone, Debug, PartialEq, Eq, Hash, BorshSerialize, BorshDeserialize)]
pub enum RecipientRef {
    /// Specific 32-byte node id.
    Direct([u8; 32]),
    /// Any node serving the given DID's domain.
    Domain(WireDid),
    /// Mesh-wide broadcast.
    Broadcast,
}

impl RecipientRef {
    /// True if this recipient is a direct node address (no fan-out).
    #[must_use]
    pub fn is_direct(&self) -> bool {
        matches!(self, RecipientRef::Direct(_))
    }

    /// True if this recipient triggers mesh fan-out (`Domain` or `Broadcast`).
    #[must_use]
    pub fn is_fanout(&self) -> bool {
        matches!(self, RecipientRef::Domain(_) | RecipientRef::Broadcast)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn borsh_round_trip_direct() {
        let r = RecipientRef::Direct([0xab; 32]);
        let bytes = borsh::to_vec(&r).unwrap();
        let back: RecipientRef = borsh::from_slice(&bytes).unwrap();
        assert_eq!(back, r);
    }

    #[test]
    fn borsh_round_trip_broadcast() {
        let r = RecipientRef::Broadcast;
        let bytes = borsh::to_vec(&r).unwrap();
        let back: RecipientRef = borsh::from_slice(&bytes).unwrap();
        assert_eq!(back, r);
    }

    #[test]
    fn fanout_classification() {
        let d = RecipientRef::Direct([0; 32]);
        assert!(d.is_direct());
        assert!(!d.is_fanout());
        let b = RecipientRef::Broadcast;
        assert!(!b.is_direct());
        assert!(b.is_fanout());
    }
}
