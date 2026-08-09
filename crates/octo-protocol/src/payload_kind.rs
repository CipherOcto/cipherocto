//! Payload kind discriminator (RFC-0871 §Data Structures).
//!
//! 128-bit UUID per RFC-0965 caveat discriminator pattern (16 bytes instead of 1).
//! Old code fails-closed on unknown discriminators (RFC-0965 §3.2 pattern).
//! No central enum: each new payload kind = new RFC + new UUID allocation.

use borsh::{BorshDeserialize, BorshSerialize};

/// 128-bit payload discriminator (UUID-shaped).
///
/// RFC-0871 §Data Structures. Wire form is a flat 16-byte big-endian UUID.
/// RFC-allocated namespace + user-extension range; see [`rfc_namespace`] /
/// [`user_extension_range`] / [`capability_extension_range`].
///
/// No `Display` / `FromStr` is provided — `PayloadKindId` is opaque on the
/// wire and meaningful only in the context of an RFC-allocated range. Cross-
/// mission identifiers are exchanged via their human-readable RFC-XXXX number,
/// not via the wire bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, BorshSerialize, BorshDeserialize)]
pub struct PayloadKindId(pub [u8; 16]);

impl PayloadKindId {
    /// Wrap a 16-byte buffer. No validation — caller must ensure the bytes
    /// were sourced from an RFC-allocated range or user-extension registration.
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }

    /// Borrow the inner 16-byte buffer.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }

    /// True if the discriminator sits in the RFC-allocated namespace
    /// (`0x0009_0000` … `0x0009_FFFF` for identity-substrate RFCs, the historical
    /// placeholder used by RFC-0871 §Test Vectors TV1).
    #[must_use]
    pub fn is_rfc_allocated(&self) -> bool {
        rfc_namespace().contains(&self.0)
    }

    /// True if the discriminator sits in the capability-extension namespace
    /// (RFC-0965 reserved range `0x0010_0000` … `0x0010_FFFF`).
    #[must_use]
    pub fn is_capability_extension(&self) -> bool {
        capability_extension_range().contains(&self.0)
    }

    /// True if the discriminator sits in the user-extension namespace
    /// (`0xFFFF_FF00` … `0xFFFF_FFFF`).
    #[must_use]
    pub fn is_user_extension(&self) -> bool {
        user_extension_range().contains(&self.0)
    }
}

/// RFC-allocated namespace (`0x0009_0000_0000_0000_0000_0000_0000_0000`
/// … `0x0009_FFFF_FFFF_FFFF_FFFF_FFFF_FFFF_FFFF`, first 16 bits = `0x0009`).
///
/// `0x0009` is the historical placeholder used by RFC-0871 §TV1 — concrete
/// sub-ranges are allocated per-RFC.
pub const fn rfc_namespace() -> RangeU128 {
    RangeU128 {
        start: 0x0009_0000_0000_0000_0000_0000_0000_0000,
        end: 0x0009_FFFF_FFFF_FFFF_FFFF_FFFF_FFFF_FFFF,
    }
}

/// Capability-extension namespace (RFC-0965 reserved range, first 16 bits = `0x0010`).
pub const fn capability_extension_range() -> RangeU128 {
    RangeU128 {
        start: 0x0010_0000_0000_0000_0000_0000_0000_0000,
        end: 0x0010_FFFF_FFFF_FFFF_FFFF_FFFF_FFFF_FFFF,
    }
}

/// User-extension namespace (last 256 values of UUID space).
pub const fn user_extension_range() -> RangeU128 {
    RangeU128 {
        start: 0xFFFF_FF00_0000_0000_0000_0000_0000_0000,
        end: 0xFFFF_FFFF_FFFF_FFFF_FFFF_FFFF_FFFF_FFFF,
    }
}

/// Inclusive-exclusive range over the 128-bit UUID space.
#[derive(Clone, Copy, Debug)]
pub struct RangeU128 {
    /// Inclusive start.
    pub start: u128,
    /// Inclusive end.
    pub end: u128,
}

impl RangeU128 {
    /// True if `value` falls within the range.
    #[must_use]
    pub fn contains(&self, bytes: &[u8; 16]) -> bool {
        let v = u128::from_be_bytes(*bytes);
        v >= self.start && v <= self.end
    }
}

/// Identity-resolve payload kind (RFC-0871 §Test Vectors TV1).
///
/// UUID: `0x0009:0001:0000:0000:0000:0000:0000:0001`
pub const IDENTITY_RESOLVE: PayloadKindId = PayloadKindId([
    0x00, 0x09, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01,
]);

/// Wallet sign Ed25519 (RFC-0871 §Wallet Node Lifecycle, Phase 2 mission 0871a).
///
/// UUID: `0x0009:0002:0000:0000:0000:0000:0000:0001`
pub const WALLET_SIGN_ED25519: PayloadKindId = PayloadKindId([
    0x00, 0x09, 0x00, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01,
]);

/// Wallet mint capability (RFC-0871 §Wallet Node Lifecycle, Phase 2 mission 0871a).
///
/// UUID: `0x0009:0002:0000:0000:0000:0000:0000:0002`
pub const WALLET_MINT_CAPABILITY: PayloadKindId = PayloadKindId([
    0x00, 0x09, 0x00, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02,
]);

/// Wallet attenuate capability (RFC-0871 §Wallet Node Lifecycle, Phase 2 mission 0871a).
///
/// UUID: `0x0009:0002:0000:0000:0000:0000:0000:0003`
pub const WALLET_ATTENUATE_CAPABILITY: PayloadKindId = PayloadKindId([
    0x00, 0x09, 0x00, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03,
]);

/// Wallet resolve DID (RFC-0871 §Test Vectors TV7).
///
/// UUID: `0x0009:0002:0000:0000:0000:0000:0000:0004`
pub const WALLET_RESOLVE_DID: PayloadKindId = PayloadKindId([
    0x00, 0x09, 0x00, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x04,
]);

// RFC-0870 quota router payload kinds (RFC-0870 §NodeEnvelope Adoption,
// mission 0870-b-envelope-adoption). Allocated in the RFC-0871
// `rfc_namespace` (`0x0009:...`) with sub-namespace `0x0003` (RFC-0870 — quota
// router mesh). The RFC-0870 amendment table uses conceptual `[0x87, 0x00, ...]`
// byte placeholders; this allocation is the canonical materialization per
// RFC-0871 §Ordering + RFC-0871 §Namespace. Mission 0870-b maps the legacy
// discriminator bytes (0xC3–0xCB) to these UUIDs at the quota-router-core
// boundary (see `crates/quota-router-core/src/node/envelope_v2.rs`).
//
// RFC-0870 §NodeEnvelope Adoption table:
//
// | Legacy discriminator | New PayloadKindId                |
// |----------------------|-----------------------------------|
// | 0xC3 (FWD_REQUEST)   | QUOTA_FORWARD_REQUEST             |
// | 0xC4 (FWD_RESPONSE)  | QUOTA_FORWARD_RESPONSE            |
// | 0xC5 (FWD_REJECT)    | QUOTA_FORWARD_REJECT              |
// | 0xC6 (CAP_GOSSIP)    | QUOTA_CAPACITY_GOSSIP             |
// | 0xC7 (CAP_REQUEST)   | QUOTA_CAPACITY_REQUEST            |
// | 0xCA (ROUTER_ANNC)   | QUOTA_ROUTER_ANNOUNCE             |
// | 0xCB (ROUTER_WITHD)  | QUOTA_ROUTER_WITHDRAW             |

/// Router announce (RFC-0870 §NodeEnvelope Adoption).
///
/// UUID: `0x0009:0003:0000:0000:0000:0000:0000:0000`
pub const QUOTA_ROUTER_ANNOUNCE: PayloadKindId = PayloadKindId([
    0x00, 0x09, 0x00, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
]);

/// Router withdraw (RFC-0870 §NodeEnvelope Adoption).
///
/// UUID: `0x0009:0003:0000:0000:0000:0000:0000:0001`
pub const QUOTA_ROUTER_WITHDRAW: PayloadKindId = PayloadKindId([
    0x00, 0x09, 0x00, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01,
]);

/// Capacity gossip (RFC-0870 §NodeEnvelope Adoption).
///
/// UUID: `0x0009:0003:0000:0000:0000:0000:0000:0002`
pub const QUOTA_CAPACITY_GOSSIP: PayloadKindId = PayloadKindId([
    0x00, 0x09, 0x00, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02,
]);

/// Capacity request (RFC-0870 §NodeEnvelope Adoption).
///
/// UUID: `0x0009:0003:0000:0000:0000:0000:0000:0003`
pub const QUOTA_CAPACITY_REQUEST: PayloadKindId = PayloadKindId([
    0x00, 0x09, 0x00, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03,
]);

/// Forward request (RFC-0870 §NodeEnvelope Adoption).
///
/// UUID: `0x0009:0003:0000:0000:0000:0000:0000:0010`
pub const QUOTA_FORWARD_REQUEST: PayloadKindId = PayloadKindId([
    0x00, 0x09, 0x00, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x10,
]);

/// Forward response (RFC-0870 §NodeEnvelope Adoption).
///
/// UUID: `0x0009:0003:0000:0000:0000:0000:0000:0011`
pub const QUOTA_FORWARD_RESPONSE: PayloadKindId = PayloadKindId([
    0x00, 0x09, 0x00, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x11,
]);

/// Forward reject (RFC-0870 §NodeEnvelope Adoption).
///
/// UUID: `0x0009:0003:0000:0000:0000:0000:0000:0012`
pub const QUOTA_FORWARD_REJECT: PayloadKindId = PayloadKindId([
    0x00, 0x09, 0x00, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x12,
]);

/// All RFC-0870 quota-router payload kinds (mission 0870-b-envelope-adoption).
///
/// Mission 0870-b uses this array to perform `on_receive` dispatch: a
/// borsh-deserialized `NodeEnvelope` whose `payload_kind` matches one of
/// these UUIDs is routed to the corresponding `QuotaRouterHandler::handle_*`
/// method. Unknown UUIDs in the RFC-0870 sub-namespace are dropped
/// fail-closed per RFC-0871 §Compatibility + RFC-0965 §3.2.
pub const QUOTA_PAYLOAD_KINDS: &[PayloadKindId] = &[
    QUOTA_ROUTER_ANNOUNCE,
    QUOTA_ROUTER_WITHDRAW,
    QUOTA_CAPACITY_GOSSIP,
    QUOTA_CAPACITY_REQUEST,
    QUOTA_FORWARD_REQUEST,
    QUOTA_FORWARD_RESPONSE,
    QUOTA_FORWARD_REJECT,
];

/// True if `kind` is an RFC-0870 quota-router payload kind.
#[must_use]
pub fn is_quota_payload_kind(kind: &PayloadKindId) -> bool {
    QUOTA_PAYLOAD_KINDS.contains(kind)
}

// RFC-0871 reputation anchor node payload kinds (RFC-0871 §Roles and
// Authorities, mission 0871c-reputation-anchor-node Phase 3). Allocated
// in the RFC-0871 `rfc_namespace` (`0x0009:...`) with sub-namespace
// `0x0004` (mission 0871c — reputation anchor specialized node). The
// full RFC-0968 / RFC-0955-R1 reputation surface (`REPUTATION_QUERY`,
// `REPUTATION_UPDATE`, `REPUTATION_ANCHOR`) lands in follow-on missions
// once the RFC-0968 reputation registry + RFC-0955-R1 anchoring substrate
// are production-ready (mission 0968a-reputation-anchoring in flight).
//
// Mission 0871c AC exposes ONLY the `REPUTATION_ANCHOR_QUERY` adapter
// stub — a typed hand-off point so the L1 quorum flow can route
// reputation-anchor lookups without the registry substrate being live.

/// Reputation anchor query (RFC-0871 §Roles and Authorities, mission 0871c).
///
/// Phase 3 MVP stub: validates a canonical DID via
/// `octo_ident::CanonicalCodec::parse(s, false)` and returns a placeholder
/// `(anchor_score, attestation_count)` response. The real lookup against
/// `octo-reputation` registry lands in mission 0968a-reputation-anchoring.
///
/// UUID: `0x0009:0004:0000:0000:0000:0000:0000:0001`
pub const REPUTATION_ANCHOR_QUERY: PayloadKindId = PayloadKindId([
    0x00, 0x09, 0x00, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01,
]);

/// All reputation-anchor payload kinds served by `ReputationAnchorNode`
/// (RFC-0871 §Roles and Authorities, mission 0871c-reputation-anchor-node).
///
/// Phase 3 MVP exposes only `REPUTATION_ANCHOR_QUERY`. Follow-on
/// missions add `REPUTATION_QUERY` / `REPUTATION_UPDATE` / `REPUTATION_ANCHOR`
/// once the RFC-0968 registry + RFC-0955-R1 anchoring substrate are wired.
pub const REPUTATION_PAYLOAD_KINDS: &[PayloadKindId] = &[REPUTATION_ANCHOR_QUERY];

/// True if `kind` is a reputation-anchor payload kind (RFC-0871
/// §Roles and Authorities, mission 0871c).
#[must_use]
pub fn is_reputation_payload_kind(kind: &PayloadKindId) -> bool {
    REPUTATION_PAYLOAD_KINDS.contains(kind)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_resolve_uuid_matches_tv1() {
        // RFC-0871 §TV1: payload_kind = UUID 0x0009:0001:0000:0000:0000:0000:0000:0001
        let expected: [u8; 16] = [
            0x00, 0x09, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x01,
        ];
        assert_eq!(IDENTITY_RESOLVE.0, expected);
    }

    #[test]
    fn rfc_namespace_classification() {
        assert!(IDENTITY_RESOLVE.is_rfc_allocated());
        assert!(!IDENTITY_RESOLVE.is_capability_extension());
        assert!(!IDENTITY_RESOLVE.is_user_extension());
    }

    #[test]
    fn user_extension_namespace_high_bytes() {
        let ext = PayloadKindId([0xFF; 16]);
        assert!(ext.is_user_extension());
        assert!(!ext.is_rfc_allocated());
    }

    #[test]
    fn borsh_round_trip() {
        let bytes = borsh::to_vec(&IDENTITY_RESOLVE).unwrap();
        let back: PayloadKindId = borsh::from_slice(&bytes).unwrap();
        assert_eq!(back, IDENTITY_RESOLVE);
    }

    #[test]
    fn quota_payload_kinds_are_distinct() {
        // Mission 0870-b AC: 7 RFC-0870 payload kinds must be pairwise distinct
        // (no two payloads share a UUID by accident).
        let kinds = QUOTA_PAYLOAD_KINDS;
        assert_eq!(kinds.len(), 7);
        for i in 0..kinds.len() {
            for j in (i + 1)..kinds.len() {
                assert_ne!(
                    kinds[i], kinds[j],
                    "duplicate UUID in QUOTA_PAYLOAD_KINDS at indices {i} / {j}"
                );
            }
        }
    }

    #[test]
    fn quota_payload_kinds_are_rfc_allocated() {
        // Mission 0870-b AC: every RFC-0870 payload kind MUST sit in the
        // RFC-0871 `rfc_namespace` (0x0009:0000…0x0009:FFFF). Legacy
        // outbound code reads the first byte to discriminate legacy vs new
        // envelopes; new envelopes MUST be borsh-decodable as `NodeEnvelope`.
        for kind in QUOTA_PAYLOAD_KINDS {
            assert!(
                kind.is_rfc_allocated(),
                "RFC-0870 payload kind {kind:?} not in rfc_namespace"
            );
        }
    }

    #[test]
    fn quota_payload_kinds_first16bits_0x0009_0003() {
        // Mission 0870-b AC: RFC-0870 sub-namespace = 0x0003 (within
        // RFC-0871 rfc_namespace). First 16 bits must be 0x0009; next 16
        // bits must be 0x0003.
        for kind in QUOTA_PAYLOAD_KINDS {
            assert_eq!(kind.0[0], 0x00);
            assert_eq!(kind.0[1], 0x09);
            assert_eq!(kind.0[2], 0x00);
            assert_eq!(kind.0[3], 0x03);
        }
    }

    #[test]
    fn is_quota_payload_kind_matches_array() {
        for kind in QUOTA_PAYLOAD_KINDS {
            assert!(is_quota_payload_kind(kind));
        }
        // A non-quota RFC-0871 payload kind must NOT match.
        assert!(!is_quota_payload_kind(&IDENTITY_RESOLVE));
        assert!(!is_quota_payload_kind(&WALLET_SIGN_ED25519));
    }

    #[test]
    fn reputation_anchor_query_uuid_matches_mission_0871c() {
        // Mission 0871c AC: REPUTATION_ANCHOR_QUERY UUID =
        // 0x0009:0004:0000:0000:0000:0000:0000:0001
        let expected: [u8; 16] = [
            0x00, 0x09, 0x00, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x01,
        ];
        assert_eq!(REPUTATION_ANCHOR_QUERY.0, expected);
    }

    #[test]
    fn reputation_payload_kinds_are_rfc_allocated() {
        // Mission 0871c AC: every reputation-anchor payload kind MUST sit in
        // the RFC-0871 `rfc_namespace` (0x0009:0000…0x0009:FFFF) with
        // sub-namespace `0x0004` (mission 0871c — reputation anchor).
        for kind in REPUTATION_PAYLOAD_KINDS {
            assert!(
                kind.is_rfc_allocated(),
                "RFC-0871 reputation payload kind {kind:?} not in rfc_namespace"
            );
            assert_eq!(kind.0[0], 0x00);
            assert_eq!(kind.0[1], 0x09);
            assert_eq!(kind.0[2], 0x00);
            assert_eq!(kind.0[3], 0x04);
        }
    }

    #[test]
    fn is_reputation_payload_kind_matches_array() {
        for kind in REPUTATION_PAYLOAD_KINDS {
            assert!(is_reputation_payload_kind(kind));
        }
        // A non-reputation RFC-0871 payload kind must NOT match.
        assert!(!is_reputation_payload_kind(&IDENTITY_RESOLVE));
        assert!(!is_reputation_payload_kind(&WALLET_SIGN_ED25519));
        assert!(!is_reputation_payload_kind(&QUOTA_ROUTER_ANNOUNCE));
    }

    #[test]
    fn reputation_payload_kinds_borsh_round_trip() {
        for kind in REPUTATION_PAYLOAD_KINDS {
            let bytes = borsh::to_vec(kind).unwrap();
            let back: PayloadKindId = borsh::from_slice(&bytes).unwrap();
            assert_eq!(back, *kind);
        }
    }

    #[test]
    fn reputation_anchor_query_does_not_collide_with_quota() {
        // Mission 0871c AC: REPUTATION_ANCHOR_QUERY must NOT collide with
        // the RFC-0870 quota-router sub-namespace (0x0003). The dispatcher
        // classifies by exact UUID match; a collision would silently route
        // reputation lookups to the quota-router receiver.
        assert!(!is_quota_payload_kind(&REPUTATION_ANCHOR_QUERY));
        assert!(!is_reputation_payload_kind(&QUOTA_ROUTER_ANNOUNCE));
        assert_ne!(REPUTATION_ANCHOR_QUERY, QUOTA_ROUTER_ANNOUNCE);
        assert_ne!(REPUTATION_ANCHOR_QUERY, WALLET_RESOLVE_DID);
    }

    #[test]
    fn quota_payload_kinds_borsh_round_trip() {
        for kind in QUOTA_PAYLOAD_KINDS {
            let bytes = borsh::to_vec(kind).unwrap();
            let back: PayloadKindId = borsh::from_slice(&bytes).unwrap();
            assert_eq!(back, *kind);
        }
    }
}
