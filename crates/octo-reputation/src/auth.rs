//! Governance / suspension / slash authorisation types.
//!
//! Per RFC-0968 §21: every authoritative signature or registration carries
//! one of these types and is shape-validated before any chain-side effect.
//! Real signature verification is stubbed (`verify_governance_suspension`,
//! `slash_recorder`) pending a later mission that owns governance key
//! provisioning. The shape, freshness, and quorum checks all land now so the
//! production signer can be swapped in later without API churn.

use serde::{Deserialize, Serialize};

use crate::constants::{
    BLAKE3_GOVERNANCE_SET_DOMAIN, GOVERNANCE_QUORUM, MAX_GOVERNANCE_SNAPSHOT_AGE_SECS,
};
use crate::error::ReputationError;
use crate::types::{RecorderDid, RecorderId};

/// A snapshot of the governance committee at a given moment. Every
/// authoritative proof references one of these.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GovernanceSnapshot {
    pub finalized_at_unix: u64,
    pub governance_set_hash: [u8; 32],
    pub members: Vec<[u8; 32]>,
}

impl GovernanceSnapshot {
    pub fn age_secs(&self, now_unix: u64) -> u64 {
        now_unix.saturating_sub(self.finalized_at_unix)
    }

    pub fn is_fresh(&self, now_unix: u64) -> bool {
        self.age_secs(now_unix) <= MAX_GOVERNANCE_SNAPSHOT_AGE_SECS
    }

    pub fn quorum_count(&self) -> u32 {
        self.members.len() as u32
    }
}

/// Where slashed tokens are routed. RFC-0968 §21 amendment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SlashDestination {
    /// Protocol treasury.
    Treasury,
    /// Tokens are destroyed.
    Burn,
    /// Rewarded to the named validator DID.
    RewardValidator { did: RecorderDid },
}

impl SlashDestination {
    pub fn discriminant(self) -> u8 {
        match self {
            Self::Treasury => 0x01,
            Self::Burn => 0x02,
            Self::RewardValidator { .. } => 0x03,
        }
    }

    pub fn matches_field(self, field: u8) -> bool {
        self.discriminant() == field
    }

    /// Canonical byte form for the chain-tx byte-equality on-wire
    /// lock (mission 0851p-a AC, RFC-0968 §21 + §23 Review-Round-7
    /// vector). The encoding is `discriminant || payload_bytes`:
    ///
    /// - `Treasury`        → `[0x01]`
    /// - `Burn`            → `[0x02]`
    /// - `RewardValidator` → `[0x03 || did.0 (52 bytes)]`
    ///
    /// Discriminant is disjoint from `AssetTag` (`0x00`/`0x01`/`0x02`)
    /// so chain-side parsers can branch first on `byte[0]`.
    pub fn canonical_bytes(self) -> Vec<u8> {
        match self {
            Self::Treasury => vec![0x01],
            Self::Burn => vec![0x02],
            Self::RewardValidator { did } => {
                let mut v = Vec::with_capacity(53);
                v.push(0x03);
                v.extend_from_slice(did.as_bytes());
                v
            }
        }
    }
}

/// Asset tag for the slashed amount. RFC-0968 §21.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AssetTag {
    None = 0x00,
    Octo = 0x01,
    RoleToken = 0x02,
}

impl AssetTag {
    pub fn from_discriminant(d: u8) -> Result<Self, ReputationError> {
        Ok(match d {
            0x00 => Self::None,
            0x01 => Self::Octo,
            0x02 => Self::RoleToken,
            _other => return Err(ReputationError::ChainRefInvalid("asset_tag")),
        })
    }
}

/// Authorisation for suspending or slashing a recorder. Carries a fresh
/// governance snapshot + 3 distinct signatures (quorum). The signatures are
/// not verified here — that is the deferred responsibility of the signer
/// subsystem. The shape, freshness, and quorum checks are.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GovernanceProof {
    /// Public key of the primary signer (32 bytes).
    pub governance_pubkey: [u8; 32],
    /// Recorder being suspended or slashed.
    pub recorder_id: RecorderId,
    /// BLAKE3 over the action payload — binds signature to reason.
    pub reason_hash: [u8; 32],
    /// Signature. The bytes the signature covers depend on whether
    /// `slash_destination` is `Some`:
    ///
    /// - Suspension (`slash_destination == None`):
    ///   `BLAKE3(BLAKE3_GOVERNANCE_PROOF_DOMAIN || reason_hash)`.
    /// - Slash (`slash_destination == Some(_)`):
    ///   the canonical preimage returned by
    ///   [`GovernanceProof::slash_signature_preimage`] — the
    ///   chain-tx byte-equality on-wire lock (mission 0851p-a AC,
    ///   RFC-0968 §21 + §23 Review-Round-7).
    ///
    /// The chain-tx layer MUST re-derive the preimage from the
    /// signed fields and verify the signature against it; the
    /// `issue_governance_slash` byte-equality gate guarantees the
    /// caller-supplied fields match the signed fields so a chain-tx
    /// builder cannot suppress-destination-on-chain.
    pub signature: Vec<u8>,
    /// Snapshot under which this proof is valid.
    pub snapshot: GovernanceSnapshot,
    /// Hash of the governance set at snapshot time. Must match
    /// `snapshot.governance_set_hash`.
    pub governance_set_hash: [u8; 32],
    /// Slash-specific fields. `None` for suspension proofs.
    pub slash_destination: Option<SlashDestination>,
    pub slash_amount: u64,
    pub slash_asset: AssetTag,
}

impl GovernanceProof {
    /// Canonical byte preimage that the signature covers for a slash
    /// proof. Returns `None` for suspension proofs
    /// (`slash_destination == None`); returns `Some(bytes)` for slash
    /// proofs.
    ///
    /// Encoding (mission 0851p-a AC + RFC-0968 §21/§23 Review-Round-7):
    ///
    /// ```text
    /// BLAKE3(BLAKE3_REPUTATION_SUSPENSION_DOMAIN
    ///     || recorder_id.0.to_be_bytes()       // 8 bytes
    ///     || reason_hash                       // 32 bytes
    ///     || slash_destination.canonical_bytes // 1 or 53 bytes
    ///     || slash_amount.to_be_bytes()        // 8 bytes
    ///     || slash_asset as u8                 // 1 byte
    ///     || governance_pubkey                 // 32 bytes
    ///     || now_unix.to_be_bytes())           // 8 bytes
    /// ```
    ///
    /// Domain separator (`BLAKE3_REPUTATION_SUSPENSION_DOMAIN`) is
    /// the canonical domain for governance-bound actions (per
    /// `constants.rs`). The BLAKE3 outer call commits to the
    /// canonical bytes; the ed25519 signature over those 32 bytes
    /// binds all four slash fields + recorder + pubkey + timestamp.
    ///
    /// Any chain-tx layer (e.g. `octo-bootstrap`) MUST re-derive
    /// this preimage from the proof's signed fields and verify the
    /// signature against it. Doing only the field-byte-equality
    /// check (caller-supplied vs. signed) without the on-wire
    /// signature check leaves the door open to a
    /// suppress-destination-on-chain attack (RFC §3652-3653).
    pub fn slash_signature_preimage(&self, now_unix: u64) -> Option<Vec<u8>> {
        let dest = self.slash_destination.as_ref()?;
        let mut buf: Vec<u8> = Vec::with_capacity(141);
        // Domain separator (the governance-bound action domain).
        buf.extend_from_slice(crate::constants::BLAKE3_REPUTATION_SUSPENSION_DOMAIN);
        // recorder_id as 8-byte big-endian u64.
        buf.extend_from_slice(self.recorder_id.as_bytes());
        // reason_hash.
        buf.extend_from_slice(&self.reason_hash);
        // slash_destination.canonical_bytes (1 or 53 bytes).
        buf.extend_from_slice(&dest.canonical_bytes());
        // slash_amount as 8-byte big-endian u64.
        buf.extend_from_slice(&self.slash_amount.to_be_bytes());
        // slash_asset as 1 byte.
        buf.push(self.slash_asset as u8);
        // governance_pubkey.
        buf.extend_from_slice(&self.governance_pubkey);
        // now_unix as 8-byte big-endian u64.
        buf.extend_from_slice(&now_unix.to_be_bytes());
        Some(buf)
    }

    /// BLAKE3 digest of the slash signature preimage. The signature
    /// in `self.signature` is an ed25519 signature over these 32
    /// bytes. Convenience for chain-tx verification paths that need
    /// the digest without the raw preimage.
    pub fn slash_signature_digest(&self, now_unix: u64) -> Option<[u8; 32]> {
        let preimage = self.slash_signature_preimage(now_unix)?;
        let mut out = [0u8; 32];
        let digest = blake3::hash(&preimage);
        out.copy_from_slice(digest.as_bytes());
        Some(out)
    }
}

/// Authorisation for `verify_governance_suspension` (read-side). Carries
/// `(auth: &SuspensionAuth, snapshot: &GovernanceSnapshot, now_unix)` per
/// the canonical `ReputationStore` trait signature.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SuspensionAuth {
    pub governance_pubkey: [u8; 32],
    pub recorder_id: RecorderId,
    pub reason_hash: [u8; 32],
    pub signature: Vec<u8>,
    pub snapshot: GovernanceSnapshot,
    pub governance_set_hash: [u8; 32],
}

/// 8-field ChainRef verification contract per RFC-0968 §21 Review Round 8.
/// Every recorder registration carries one. Each field must validate before
/// the 3-guard stake check is evaluated.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChainRef {
    pub chain_id: u32,
    pub block_height: u64,
    pub tx_hash: [u8; 32],
    pub recorder_did: RecorderDid,
    pub octo_stake: u64,
    pub role_stake: u64,
    pub role_token_kind: u32,
    pub lock_until_unix: u64,
}

impl ChainRef {
    /// 8-field validation. Each field has a structural rule; failure returns
    /// `ChainRefInvalid("field_name")`.
    pub fn verify(&self) -> Result<(), ReputationError> {
        if self.chain_id == 0 {
            return Err(ReputationError::ChainRefInvalid("chain_id"));
        }
        if self.block_height == 0 {
            return Err(ReputationError::ChainRefInvalid("block_height"));
        }
        if self.tx_hash == [0u8; 32] {
            return Err(ReputationError::ChainRefInvalid("tx_hash"));
        }
        if self.octo_stake == 0 {
            return Err(ReputationError::ChainRefInvalid("octo_stake"));
        }
        if self.role_stake == 0 {
            return Err(ReputationError::ChainRefInvalid("role_stake"));
        }
        if self.role_token_kind == 0 {
            return Err(ReputationError::ChainRefInvalid("role_token_kind"));
        }
        if self.lock_until_unix == 0 {
            return Err(ReputationError::ChainRefInvalid("lock_until_unix"));
        }
        Ok(())
    }
}

/// Compute the governance set hash under the governance-set domain.
/// `BLAKE3(BLAKE3_GOVERNANCE_SET_DOMAIN || sorted_member_pubkeys_concat)`.
pub fn governance_set_hash(members: &[[u8; 32]]) -> [u8; 32] {
    let mut sorted: Vec<[u8; 32]> = members.to_vec();
    sorted.sort_unstable();
    let mut hasher = blake3::Hasher::new();
    hasher.update(BLAKE3_GOVERNANCE_SET_DOMAIN);
    for m in &sorted {
        hasher.update(m);
    }
    let out = hasher.finalize();
    let mut arr = [0u8; 32];
    arr.copy_from_slice(out.as_bytes());
    arr
}

/// Required quorum size per amendment 24.
pub fn required_quorum() -> u32 {
    GOVERNANCE_QUORUM
}

// ---------------------------------------------------------------------------
// Attestor types — RFC-0968 §12 + amendments 22, 28
//
// An Attestor is a replication peer that signs `Attestation` records
// indicating it has observed a `SignalEvent` gossiped from another node.
// Attestors are NOT authoritative — the recorder's signature is the only
// authority for the event itself. Attestor signatures are transport
// metadata that boost a `reputation_event` from "seen by 1 node" to "seen
// by N nodes" for quorum purposes.
// ---------------------------------------------------------------------------

use crate::types::EventId;

/// 52-byte attestor DID, structurally identical to `RecorderDid` but kept
/// as a distinct newtype so the type system prevents a recorder from
/// passing as an attestor (or vice versa) without explicit conversion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AttestorId(#[serde(with = "hex::serde")] [u8; 52]);

impl AttestorId {
    pub const fn from_array(arr: [u8; 52]) -> Self {
        Self(arr)
    }

    pub fn as_bytes(&self) -> &[u8; 52] {
        &self.0
    }
}

/// Lightweight attestor registration record per RFC-0968 §12 amendment
/// 22. Stored in the `reputation_attestors` table. `peer_set_id` is the
/// libp2p peer-set identifier; the same attestor may register multiple
/// peer-set IDs over its lifetime (e.g., after key rotation).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AttestorRegistration {
    /// Canonical attestor DID.
    pub attestor_did: AttestorId,
    /// ed25519 public key of the attestor.
    pub pubkey: [u8; 32],
    /// libp2p peer-set identifier (32 bytes, opaque).
    pub peer_set_id: [u8; 32],
    /// Unix seconds at registration request.
    pub requested_at_unix: u64,
    /// Unix seconds at registration finalization.
    pub registered_at_unix: u64,
}

/// Attestor authentication envelope carried in gossip frames. Real
/// signature verification is deferred to the signer subsystem; the
/// shape + freshness checks land now so the production signer can be
/// swapped in later without API churn.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AttestorAuth {
    /// ed25519 public key of the attestor.
    pub attestor_pubkey: [u8; 32],
    /// Attestor DID — must satisfy `attestor_did == derived from attestor_pubkey`.
    pub attestor_did: AttestorId,
    /// `BLAKE3(BLAKE3_REPUTATION_AUDIT_NONCE_DOMAIN || attestor_did || event_id || observed_at_unix)`.
    pub event_digest: [u8; 32],
    /// ed25519 signature over `BLAKE3(BLAKE3_REPUTATION_AUDIT_NONCE_DOMAIN || attestor_did || event_id || observed_at_unix)`.
    pub signature: Vec<u8>,
    /// Unix seconds when the attestor observed the event.
    pub observed_at_unix: u64,
    /// Source mission this attestation came from (cross-mission bridge).
    pub source_mission: String,
    /// Source domain within the source mission.
    pub source_domain: String,
}

/// A single attestation record — one row in `reputation_attestations`.
/// Records that a specific `AttestorId` observed `event_id` at a specific
/// `observed_at_unix`. Multiple attestors per event are stored as
/// multiple rows; the `attestor_quorum_reached` count distinct rows.
///
/// `recorder_did`, `source_mission`, `source_domain` are envelope-level
/// fields (mission 0968 Phase 4, RFC-0968 §12): the recorder is the
/// publisher of the inner event, and the source mission/domain trace
/// provenance across the mesh. They are required by the v004 schema
/// (`reputation_attestations`) and are persisted on every attestation
/// row so quorum and catch-up can be filtered without a join.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Attestation {
    /// Storage-assigned monotonic attestation id.
    pub attestation_id: u64,
    /// Attestor that observed the event.
    pub attestor: AttestorId,
    /// Recorder whose event is being attested. Identifies the row in
    /// `reputation_events` whose DID the attestation is for.
    pub recorder_did: RecorderDid,
    /// Event being attested.
    pub event_id: EventId,
    /// ed25519 signature from the attestor.
    pub signature: Vec<u8>,
    /// Unix seconds when the attestor observed the event.
    pub observed_at_unix: u64,
    /// Unix seconds when the attestation was received by the local store.
    pub received_at_unix: u64,
    /// Source mission identifier (matches the enclosing envelope's
    /// `source_mission`; e.g., `mon:whatsapp:phase-1`).
    pub source_mission: String,
    /// Source domain within the source mission (matches the enclosing
    /// envelope's `source_domain`).
    pub source_domain: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constants::MAX_GOVERNANCE_SNAPSHOT_AGE_SECS;

    fn dummy_snapshot(now: u64) -> GovernanceSnapshot {
        GovernanceSnapshot {
            finalized_at_unix: now,
            governance_set_hash: [1u8; 32],
            members: vec![[1u8; 32], [2u8; 32], [3u8; 32]],
        }
    }

    #[test]
    fn snapshot_age_zero_is_fresh() {
        let s = dummy_snapshot(1000);
        assert_eq!(s.age_secs(1000), 0);
        assert!(s.is_fresh(1000));
    }

    #[test]
    fn snapshot_stale_after_max_age() {
        let s = dummy_snapshot(1000);
        let stale = 1000 + MAX_GOVERNANCE_SNAPSHOT_AGE_SECS + 1;
        assert!(!s.is_fresh(stale));
    }

    #[test]
    fn quorum_is_three() {
        assert_eq!(required_quorum(), 3);
    }

    #[test]
    fn chain_ref_rejects_zero_chain_id() {
        let cr = ChainRef {
            chain_id: 0,
            block_height: 1,
            tx_hash: [1u8; 32],
            recorder_did: RecorderDid::from_array([0u8; 52]),
            octo_stake: 4000,
            role_stake: 1000,
            role_token_kind: 1,
            lock_until_unix: 9999999999,
        };
        let err = cr.verify().unwrap_err();
        assert_eq!(err.discriminant(), 0x29);
    }

    #[test]
    fn chain_ref_accepts_well_formed() {
        let cr = ChainRef {
            chain_id: 7,
            block_height: 100,
            tx_hash: [1u8; 32],
            recorder_did: RecorderDid::from_array([0u8; 52]),
            octo_stake: 4000,
            role_stake: 1000,
            role_token_kind: 1,
            lock_until_unix: 9_999_999_999,
        };
        assert!(cr.verify().is_ok());
    }

    #[test]
    fn governance_set_hash_is_order_independent() {
        let a = [[1u8; 32], [2u8; 32], [3u8; 32]];
        let b = [[3u8; 32], [1u8; 32], [2u8; 32]];
        assert_eq!(governance_set_hash(&a), governance_set_hash(&b));
    }

    // -- Chain-tx byte-equality on-wire lock (mission 0851p-a AC) --

    fn dummy_proof() -> GovernanceProof {
        let now: u64 = 1_700_000_000;
        GovernanceProof {
            governance_pubkey: [7u8; 32],
            recorder_id: RecorderId::from_u64(42),
            reason_hash: [0xAB; 32],
            signature: vec![0u8; 96],
            snapshot: dummy_snapshot(now),
            governance_set_hash: [1u8; 32],
            slash_destination: Some(SlashDestination::Treasury),
            slash_amount: 1_000,
            slash_asset: AssetTag::Octo,
        }
    }

    #[test]
    fn slash_signature_preimage_is_none_for_suspension() {
        // Suspension proofs have no slash fields; preimage MUST
        // be `None` so the suspension signature path stays separate.
        let mut p = dummy_proof();
        p.slash_destination = None;
        assert!(p.slash_signature_preimage(1_700_000_000).is_none());
        assert!(p.slash_signature_digest(1_700_000_000).is_none());
    }

    #[test]
    fn slash_signature_preimage_includes_all_signed_fields() {
        // Deterministic byte-level shape pin. The preimage layout is
        // the on-wire lock contract — chain-tx layer MUST verify the
        // signature against BLAKE3 of these exact bytes.
        let p = dummy_proof();
        let pre = p.slash_signature_preimage(1_700_000_000).unwrap();
        let domain = crate::constants::BLAKE3_REPUTATION_SUSPENSION_DOMAIN;
        // Domain (35) + recorder_id (8) + reason_hash (32) +
        // dest canonical (1 for Treasury) + amount (8) + asset (1) +
        // pubkey (32) + now (8) = 125 bytes.
        assert_eq!(
            pre.len(),
            domain.len() + 8 + 32 + 1 + 8 + 1 + 32 + 8,
            "preimage length mismatch — on-wire contract changed"
        );
        // Domain separator is at the head.
        assert_eq!(&pre[..domain.len()], domain);
        // recorder_id (u64 BE) immediately after the domain.
        let off = domain.len();
        assert_eq!(&pre[off..off + 8], &42u64.to_be_bytes());
        // reason_hash at offset off+8 .. off+40.
        assert_eq!(&pre[off + 8..off + 40], &[0xAB; 32]);
        // slash_destination (1 byte) at offset off+40: 0x01 = Treasury.
        assert_eq!(pre[off + 40], 0x01);
        // slash_amount (u64 BE) at offset off+41 .. off+49.
        assert_eq!(&pre[off + 41..off + 49], &1_000u64.to_be_bytes());
        // slash_asset (1 byte) at offset off+49: 0x01 = Octo.
        assert_eq!(pre[off + 49], 0x01);
        // governance_pubkey at offset off+50 .. off+82.
        assert_eq!(&pre[off + 50..off + 82], &[7u8; 32]);
        // now_unix (u64 BE) at offset off+82 .. off+90.
        assert_eq!(&pre[off + 82..off + 90], &1_700_000_000u64.to_be_bytes());
    }

    #[test]
    fn slash_signature_preimage_changes_with_each_signed_field() {
        // Every signed field is bound: mutating ANY of them changes
        // the preimage (and therefore the BLAKE3 digest the
        // signature covers). This is the on-wire byte-equality lock.
        let now = 1_700_000_000;
        let base = dummy_proof();
        let base_digest = base.slash_signature_digest(now).unwrap();
        // Mutate destination.
        let mut p = base.clone();
        p.slash_destination = Some(SlashDestination::Burn);
        assert_ne!(p.slash_signature_digest(now).unwrap(), base_digest);
        // Mutate amount.
        let mut p = base.clone();
        p.slash_amount = 9_999;
        assert_ne!(p.slash_signature_digest(now).unwrap(), base_digest);
        // Mutate asset.
        let mut p = base.clone();
        p.slash_asset = AssetTag::RoleToken;
        assert_ne!(p.slash_signature_digest(now).unwrap(), base_digest);
        // Mutate recorder_id.
        let mut p = base.clone();
        p.recorder_id = RecorderId::from_u64(43);
        assert_ne!(p.slash_signature_digest(now).unwrap(), base_digest);
        // Mutate governance_pubkey.
        let mut p = base.clone();
        p.governance_pubkey = [8u8; 32];
        assert_ne!(p.slash_signature_digest(now).unwrap(), base_digest);
        // Mutate reason_hash.
        let mut p = base.clone();
        p.reason_hash = [0xCD; 32];
        assert_ne!(p.slash_signature_digest(now).unwrap(), base_digest);
        // Mutate now_unix.
        assert_ne!(
            base.slash_signature_digest(now + 1).unwrap(),
            base_digest,
            "now_unix is part of the preimage — replay protection"
        );
    }

    #[test]
    fn slash_signature_preimage_differs_per_destination_variant() {
        // RewardValidator MUST include the 52-byte DID in the preimage;
        // Treasury and Burn are 1-byte discriminants. The on-wire
        // encoding therefore differs by 52 bytes.
        let now = 1_700_000_000;
        let mut base = dummy_proof();
        let pre_treasury = base.slash_signature_preimage(now).unwrap();
        base.slash_destination = Some(SlashDestination::Burn);
        let pre_burn = base.slash_signature_preimage(now).unwrap();
        let reward_did = RecorderDid::from_array([0xEE; 52]);
        base.slash_destination = Some(SlashDestination::RewardValidator { did: reward_did });
        let pre_reward = base.slash_signature_preimage(now).unwrap();
        assert_eq!(pre_treasury.len(), pre_burn.len());
        assert_eq!(pre_reward.len(), pre_treasury.len() + 52);
        let domain = crate::constants::BLAKE3_REPUTATION_SUSPENSION_DOMAIN;
        let dest_off = domain.len() + 8 + 32;
        // The destination encoding sits at the same offset for all 3
        // variants, so chain-tx parsers can read it deterministically.
        assert_eq!(pre_treasury[dest_off], 0x01);
        assert_eq!(pre_burn[dest_off], 0x02);
        assert_eq!(pre_reward[dest_off], 0x03);
        // RewardValidator DID bytes follow the discriminant.
        assert_eq!(
            &pre_reward[dest_off + 1..dest_off + 1 + 52],
            reward_did.as_bytes()
        );
    }

    #[test]
    fn slash_destination_canonical_bytes_encoding() {
        assert_eq!(SlashDestination::Treasury.canonical_bytes(), vec![0x01]);
        assert_eq!(SlashDestination::Burn.canonical_bytes(), vec![0x02]);
        let did = RecorderDid::from_array([0xCD; 52]);
        let mut expected = vec![0x03];
        expected.extend_from_slice(did.as_bytes());
        assert_eq!(
            SlashDestination::RewardValidator { did }.canonical_bytes(),
            expected
        );
    }
}
