//! `HolderRecord` struct (RFC-0957-A1 §Data Structures).
//!
//! Content-addressable storage record. PK is `cap_root_hash` (32-byte BLAKE3).
//! 10 fields total. Manual redacting `Debug` impl per RFC-0957-A1 §Security.
//!
//! Constructors: `from_bearer`, `from_capability`. `from_hop_capability` lives
//! in sub-mission 0970-a (cross-mission dependency on RFC-0970).

use serde::{Deserialize, Serialize};

use crate::bearer_capsule_stub::BearerCapsule;
use crate::holder_kind::HolderKind;

mod serde_bytes_32 {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S: Serializer>(bytes: &[u8; 32], s: S) -> Result<S::Ok, S::Error> {
        serde_bytes::Bytes::new(bytes).serialize(s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<[u8; 32], D::Error> {
        let v: serde_bytes::ByteArray<32> = serde_bytes::ByteArray::deserialize(d)?;
        Ok(v.into_array())
    }
}

/// Per RFC-0957-A1 §Data Structures.
///
/// PK = `cap_root_hash` (32-byte BLAKE3-derived from credential).
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HolderRecord {
    /// 32-byte BLAKE3 root hash of the credential (PK).
    #[serde(with = "serde_bytes_32")]
    pub cap_root_hash: [u8; 32],

    /// Discriminator (per `HolderKind`).
    pub kind: HolderKind,

    /// Holder DID (per RFC-0009 §Identity Key Format).
    /// `holder_did` is the DID that owns this credential.
    pub holder_did: String,

    /// Holder Ed25519 public key (32 bytes; per RFC-0009 §Capability Keys).
    #[serde(with = "serde_bytes_32")]
    pub holder_pub: [u8; 32],

    /// Audience DID (the next hop for `HopCapability`; the buyer for `Bearer` /
    /// `V1` / `ZKBearing`). For `V1` and `ZKBearing` records, this equals `holder_did`.
    /// For `HopCapability`, this is the next hop's node DID.
    pub audience_did: String,

    /// Canonical caveats bytes (RFC-0126 canonical_ser of the typed caveat list).
    pub caveats_canonical: Vec<u8>,

    /// Ask binding (RFC-0959 §Ask). `None` for non-market tokens and for HopCapability.
    #[serde(with = "serde_bytes_option_32")]
    pub ask_id: Option<[u8; 32]>,

    /// Unix timestamp of mint in MILLISECONDS.
    pub mint_at_millis_unix: u64,

    /// Unix timestamp of expiry in MILLISECONDS.
    /// MUST match the credential's `Caveat::BeforeMillis(u64)` caveat.
    pub ttl_millis_unix: u64,

    /// When the record was revoked (RFC-0957-A1 §Lifecycle).
    /// `Some(ts)` means revoked; `None` means active.
    /// Replaces the prior `ttl_millis_unix = 0` revocation signal.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "serde_bytes_option_u64"
    )]
    pub revoked_at_millis_unix: Option<u64>,
}

mod serde_bytes_option_32 {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S: Serializer>(v: &Option<[u8; 32]>, s: S) -> Result<S::Ok, S::Error> {
        match v {
            Some(b) => serde_bytes::ByteArray::new(*b).serialize(s),
            None => s.serialize_none(),
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<[u8; 32]>, D::Error> {
        let opt: Option<serde_bytes::ByteArray<32>> = Option::deserialize(d)?;
        Ok(opt.map(|ba| ba.into_array()))
    }
}

mod serde_bytes_option_u64 {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(v: &Option<u64>, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_some(&v.unwrap_or(0))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<u64>, D::Error> {
        Ok(Some(u64::deserialize(d)?))
    }
}

// Per RFC-0957-A1 §Data Structures — struct defined above (continued).
// (The struct spans an earlier location in this file; the impls are
// anchored below.)

// Manual Debug redaction per RFC-0957-A1 §Security.
impl std::fmt::Debug for HolderRecord {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HolderRecord")
            .field("cap_root_hash", &"<redacted 32 bytes>")
            .field("kind", &self.kind)
            .field("holder_did", &self.holder_did)
            .field("holder_pub", &"<redacted 32 bytes>")
            .field("audience_did", &self.audience_did)
            .field(
                "caveats_canonical",
                &format_args!("<redacted {} bytes>", self.caveats_canonical.len()),
            )
            .field("ask_id", &self.ask_id.map(|_| "<redacted 32 bytes>"))
            .field("mint_at_millis_unix", &self.mint_at_millis_unix)
            .field("ttl_millis_unix", &self.ttl_millis_unix)
            .field("revoked_at_millis_unix", &self.revoked_at_millis_unix)
            .finish()
    }
}

impl HolderRecord {
    /// Build a `HolderRecord` for a `Bearer` (RFC-0903) credential.
    /// `ttl_millis_unix` is the credential's expiry in MILLISECONDS.
    /// R20-N3 fix: `buyer_holder_pub` parameter — `holder_pub` column is
    /// NOT NULL and the BearerCapsule does not carry it; must be plumbed in.
    pub fn from_bearer(
        bearer: &BearerCapsule,
        buyer_holder_pub: &[u8; 32],
        holder_did: &str,
        ask_id: [u8; 32],
        ttl_millis_unix: u64,
    ) -> Self {
        Self {
            cap_root_hash: bearer.bearer_capsule_hash,
            kind: HolderKind::Bearer,
            holder_did: holder_did.to_string(),
            holder_pub: *buyer_holder_pub,
            audience_did: holder_did.to_string(),
            caveats_canonical: Vec::new(),
            ask_id: Some(ask_id),
            mint_at_millis_unix: 0, // caller patches via `mint_at_millis_unix` post-construct
            ttl_millis_unix,
            revoked_at_millis_unix: None,
        }
    }

    /// Build a `HolderRecord` for a `V1` (RFC-0957) or `ZKBearing` (RFC-0958) capability.
    /// `ttl_millis_unix` is the credential's expiry in MILLISECONDS.
    /// R23-N2 fix: `holder_pub` is REQUIRED parameter.
    pub fn from_capability(
        cap_token: &CapabilityTokenLike,
        holder_pub: &[u8; 32],
        holder_did: &str,
        ask_id: Option<[u8; 32]>,
        ttl_millis_unix: u64,
    ) -> Self {
        let kind = match cap_token.class {
            CapabilityClass::V1 => HolderKind::V1,
            CapabilityClass::ZKBearing => HolderKind::ZKBearing,
        };
        Self {
            cap_root_hash: cap_token.cap_root_hash,
            kind,
            holder_did: holder_did.to_string(),
            holder_pub: *holder_pub,
            audience_did: holder_did.to_string(),
            caveats_canonical: Vec::new(),
            ask_id,
            mint_at_millis_unix: 0,
            ttl_millis_unix,
            revoked_at_millis_unix: None,
        }
    }

    /// Whether the record is currently active (not revoked, not expired).
    /// TV14 contract: `ttl_millis_unix == 0` + `revoked_at_millis_unix == None`
    /// is "active, no TTL expiry" — perpetual.
    pub fn is_active_at(&self, now_millis_unix: u64) -> bool {
        if self.revoked_at_millis_unix.is_some() {
            return false;
        }
        if self.ttl_millis_unix == 0 {
            return true; // perpetual
        }
        now_millis_unix < self.ttl_millis_unix
    }
}

/// Minimal projection of `octo-wallet::capability::CapabilityToken` that
/// 0957-c needs to call `from_capability`. The full `CapabilityToken` lives
/// in `octo-wallet` (which depends on `quota-router-storage` indirectly);
/// we extract the minimum surface here to avoid the dep inversion.
#[derive(Clone, Copy, Serialize, Deserialize)]
pub struct CapabilityTokenLike {
    /// `cap_root_hash` (32-byte BLAKE3-derived).
    #[serde(with = "serde_bytes_32")]
    pub cap_root_hash: [u8; 32],
    /// Discrimination: V1 vs ZKBearing. Other variants map to V1 here.
    pub class: CapabilityClass,
}

/// Class discriminator for `CapabilityTokenLike`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum CapabilityClass {
    /// RFC-0957 v1 macaroon.
    V1,
    /// RFC-0958 ZK-bearing (proof-bundle subclass).
    ZKBearing,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bearer() -> BearerCapsule {
        BearerCapsule {
            bearer_capsule_hash: [0x42; 32],
            encrypted_capsule: vec![0x01, 0x02],
            seller_signature: [0x55; 64],
        }
    }

    #[test]
    fn from_bearer_sets_cap_root_hash_from_capsule_hash() {
        let b = bearer();
        let pub_key = [0x77; 32];
        let ask_id = [0x33; 32];
        let r =
            HolderRecord::from_bearer(&b, &pub_key, "did:octo:buyer", ask_id, 1_700_000_000_000);
        assert_eq!(r.cap_root_hash, b.bearer_capsule_hash);
        assert_eq!(r.kind, HolderKind::Bearer);
        assert_eq!(r.holder_pub, pub_key);
        assert_eq!(r.holder_did, "did:octo:buyer");
        assert_eq!(r.ask_id, Some(ask_id));
        assert_eq!(r.ttl_millis_unix, 1_700_000_000_000);
        assert_eq!(r.revoked_at_millis_unix, None);
    }

    #[test]
    fn from_capability_v1_sets_kind_v1() {
        let tok = CapabilityTokenLike {
            cap_root_hash: [0x11; 32],
            class: CapabilityClass::V1,
        };
        let r = HolderRecord::from_capability(
            &tok,
            &[0x22; 32],
            "did:octo:holder",
            Some([0x33; 32]),
            1_700_000_000_000,
        );
        assert_eq!(r.kind, HolderKind::V1);
        assert_eq!(r.cap_root_hash, [0x11; 32]);
        assert_eq!(r.holder_pub, [0x22; 32]);
    }

    #[test]
    fn from_capability_zk_bearing_sets_kind_zk_bearing() {
        let tok = CapabilityTokenLike {
            cap_root_hash: [0x11; 32],
            class: CapabilityClass::ZKBearing,
        };
        let r = HolderRecord::from_capability(
            &tok,
            &[0x22; 32],
            "did:octo:holder",
            None,
            1_700_000_000_000,
        );
        assert_eq!(r.kind, HolderKind::ZKBearing);
        assert_eq!(r.ask_id, None);
    }

    #[test]
    fn is_active_when_unrevoked_and_unexpired() {
        let mut r = HolderRecord::from_bearer(
            &bearer(),
            &[0x77; 32],
            "did:octo:buyer",
            [0x33; 32],
            1_700_000_000_000,
        );
        assert!(r.is_active_at(1_699_999_999_999));
        assert!(!r.is_active_at(1_700_000_000_000)); // boundary: now == ttl = expired
        assert!(!r.is_active_at(1_700_000_000_001));
        r.revoked_at_millis_unix = Some(1_600_000_000_000);
        assert!(!r.is_active_at(1_500_000_000_000));
    }

    #[test]
    fn debug_redacts_credential_material() {
        let r = HolderRecord::from_bearer(
            &bearer(),
            &[0x77; 32],
            "did:octo:buyer",
            [0xAB; 32],
            1_700_000_000_000,
        );
        let s = format!("{:?}", r);
        assert!(s.contains("redacted"), "expected redaction: {s}");
        // Original bytes must NOT appear in the debug output.
        // The hash is [0x42; 32] = "42...42" repeated.
        assert!(!s.contains("4242"), "leaked hash bytes: {s}");
        // The pub key is [0x77; 32] = "7777...".
        assert!(!s.contains("7777"), "leaked holder_pub bytes: {s}");
        // The ask_id is [0xAB; 32] = "ABAB...".
        assert!(!s.contains("ABAB"), "leaked ask_id bytes: {s}");
    }

    #[test]
    fn revoked_at_millis_distinct_from_ttl_millis() {
        // TV14: ttl_millis_unix=0 + revoked_at_millis_unix=None = "active, no TTL expiry".
        let r = HolderRecord::from_bearer(&bearer(), &[0x77; 32], "did:octo:buyer", [0x33; 32], 0);
        assert_eq!(r.ttl_millis_unix, 0);
        assert_eq!(r.revoked_at_millis_unix, None);
        assert!(r.is_active_at(0)); // any now_millis < 0 impossible; assert expire semantics
        assert!(r.is_active_at(u64::MAX)); // ttl=0 + not revoked = perpetual

        // Revoked record.
        let mut r2 = r.clone();
        r2.revoked_at_millis_unix = Some(1_700_000_000_000);
        assert!(!r2.is_active_at(1_700_000_000_001));
    }

    #[test]
    fn serde_json_round_trip() {
        let r = HolderRecord::from_bearer(
            &bearer(),
            &[0x77; 32],
            "did:octo:buyer",
            [0x33; 32],
            1_700_000_000_000,
        );
        let s = serde_json::to_string(&r).unwrap();
        let back: HolderRecord = serde_json::from_str(&s).unwrap();
        assert_eq!(r, back);
    }
}
