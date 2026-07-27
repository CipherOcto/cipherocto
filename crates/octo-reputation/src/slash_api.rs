//! Round 7 CRITICAL gov-2 byte-equality gate for canonical slash
//! issuance (mission 0851p-a-bootstrap-slashing, RFC-0968 §21 +
//! §23 Review-Round-7 vector).
//!
//! Authority model: every `slash_recorder` invocation flows through
//! `issue_governance_slash`. The caller supplies the slash fields as
//! separate args (`destination`, `amount`, `asset`) AND a signed
//! `GovernanceProof` whose `slash_destination / slash_amount /
//! slash_asset` fields ARE the same values (signed). Before any chain
//! tx (or, in this session, before delegating to
//! `ReputationStore::slash_recorder`), the function performs an
//! independent byte-equality check on EACH of the three slash fields;
//! any mismatch short-circuits with `SlashDestinationMismatch = 0x16`.
//!
//! ## Why the gate lives at the API boundary (not inside `slash_recorder`)
//!
//! The trait method `ReputationStore::slash_recorder` takes only a
//! `GovernanceProof` — it has no caller-supplied args to compare
//! against. The byte-equality rule (RFC §3652-3653) requires the
//! comparison be done at the invocation boundary where the caller is
//! about to commit to specific slash args (e.g., a chain-tx payload).
//! That boundary is exactly `issue_governance_slash`.
//!
//! ## Production chain-tx layer (deferred)
//!
//! The "any chain tx" referenced above is not yet implemented
//! (`octo-bootstrap` is the consumer per mission 0851p-a AC). When
//! it lands, the chain-tx constructor MUST take the same `amount` /
//! `asset` / `destination` and serialize them into a payload that the
//! chain validates byte-equal to the `GovernanceProof` fields. The
//! byte-equality gate here is the precondition; the chain validation
//! is the on-wire lock. Doing only one of the two leaves the door
//! open to a "suppress-destination-on-chain" attack vector (RFC
//! §3652-3653).

use crate::auth::{AssetTag, GovernanceProof, SlashDestination};
use crate::error::ReputationError;
use crate::store::{ReputationStore, StoreResult};
use crate::types::RecorderId;

/// Field that mismatched in the byte-equality gate. The discriminator
/// shape mirrors the existing `SlashDestinationMismatch { expected:
/// u8, actual: u8 } = 0x16` variant — `expected` carries the field
/// tag (matches `Field` discriminant below), `actual` carries the
/// signed-value byte for the destination / asset mismatch case. For
/// amount, `actual` is the amount's low byte (informational only; full
/// amount recovered from the proof itself).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MismatchField {
    /// `slash_destination` byte-equality check failed.
    Destination,
    /// `slash_amount` byte-equality check failed.
    Amount,
    /// `slash_asset` byte-equality check failed.
    Asset,
}

impl MismatchField {
    /// Field tag byte for the `expected` slot. 0xD1-D3 keep the values
    /// disjoint from any `SlashDestination` discriminant (`0x01-0x03`)
    /// and from `AssetTag` (which is `0x00/0x01/0x02`).
    pub fn as_byte(self) -> u8 {
        match self {
            Self::Destination => 0xD1,
            Self::Amount => 0xD2,
            Self::Asset => 0xD3,
        }
    }
}

/// Issue a canonical governance slash. Performs the Round 7 CRITICAL
/// gov-2 byte-equality gate, then delegates to
/// `ReputationStore::slash_recorder`.
///
/// Independent byte-equality checks, in declaration order:
/// 1. `proof.slash_destination == Some(destination)`
/// 2. `proof.slash_amount == amount`
/// 3. `proof.slash_asset == asset`
///
/// Each mismatch returns `SlashDestinationMismatch { field, ... }` —
/// the caller can branch on `discriminant() == 0x16` AND inspect
/// `field` to know which check fired.
pub async fn issue_governance_slash<S>(
    store: &S,
    recorder_id: RecorderId,
    destination: SlashDestination,
    amount: u64,
    asset: AssetTag,
    proof: GovernanceProof,
    now_unix: u64,
) -> StoreResult<()>
where
    S: ReputationStore + ?Sized,
{
    let _ = now_unix; // signature reserves the slot for chain-tx timestamp

    // Field 1: destination. Caller's value vs signed proof's value.
    let signed_dest = proof
        .slash_destination
        .ok_or(ReputationError::SlashDestinationMismatch {
            expected: MismatchField::Destination.as_byte(),
            actual: 0,
        })?;
    if signed_dest != destination {
        return Err(ReputationError::SlashDestinationMismatch {
            expected: MismatchField::Destination.as_byte(),
            actual: destination.discriminant(),
        });
    }

    // Field 2: amount.
    if proof.slash_amount != amount {
        return Err(ReputationError::SlashDestinationMismatch {
            expected: MismatchField::Amount.as_byte(),
            actual: (amount & 0xFF) as u8,
        });
    }

    // Field 3: asset.
    if proof.slash_asset != asset {
        return Err(ReputationError::SlashDestinationMismatch {
            expected: MismatchField::Asset.as_byte(),
            actual: asset as u8,
        });
    }

    // All three fields byte-equal. Bind recorder_id to the proof's
    // recorded target so a stale proof for a different recorder
    // cannot be replayed against the current one (parity with the
    // governance suspension binding analogue — RFC-0968 §13 row 273).
    if proof.recorder_id != recorder_id {
        return Err(ReputationError::SlashDestinationMismatch {
            expected: MismatchField::Asset.as_byte(), // asset-as-recorder-id proxy
            actual: 0xFF,
        });
    }

    // Delegate to the canonical store. The trait impl validates
    // freshness + governance quorum + non-zero amount/asset.
    store.slash_recorder(proof).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::{AssetTag, GovernanceProof, GovernanceSnapshot, SlashDestination};
    use crate::constants::GOVERNANCE_QUORUM;
    use crate::store::InMemoryReputationStore;
    use crate::types::RecorderId;

    /// Helper: build a `GovernanceProof` whose three slash fields
    /// match the function's separate args. `now_unix` is set inside
    /// the snapshot for `is_fresh`. The signature is a 96-byte stub
    /// (3 × 32) consistent with existing test fixtures.
    fn proof_with(
        recorder_id: RecorderId,
        destination: Option<SlashDestination>,
        amount: u64,
        asset: AssetTag,
        now_unix: u64,
    ) -> GovernanceProof {
        GovernanceProof {
            governance_pubkey: [1u8; 32],
            recorder_id,
            reason_hash: [0u8; 32],
            signature: vec![0u8; 96],
            snapshot: GovernanceSnapshot {
                finalized_at_unix: now_unix,
                governance_set_hash: [0u8; 32],
                members: (0..GOVERNANCE_QUORUM).map(|i| [i as u8; 32]).collect(),
            },
            governance_set_hash: [0u8; 32],
            slash_destination: destination,
            slash_amount: amount,
            slash_asset: asset,
        }
    }

    fn store() -> InMemoryReputationStore {
        InMemoryReputationStore::new()
    }

    #[tokio::test]
    async fn gov_2_happy_path_calls_slash_recorder() {
        let s = store();
        let rid = RecorderId::from_u64(7);
        let now = 1_700_000_000;
        let proof = proof_with(
            rid,
            Some(SlashDestination::Treasury),
            1_000,
            AssetTag::Octo,
            now,
        );
        // Same args the proof signed → must delegate through.
        let r = issue_governance_slash(
            &s,
            rid,
            SlashDestination::Treasury,
            1_000,
            AssetTag::Octo,
            proof,
            now,
        )
        .await;
        assert!(r.is_ok(), "gov-2 happy path must succeed: {r:?}");
    }

    #[tokio::test]
    async fn gov_2_destination_mismatch_rejected_with_0x16() {
        let s = store();
        let rid = RecorderId::from_u64(8);
        let now = 1_700_000_000;
        // Proof says Treasury, caller says Burn.
        let proof = proof_with(
            rid,
            Some(SlashDestination::Treasury),
            1_000,
            AssetTag::Octo,
            now,
        );
        let r = issue_governance_slash(
            &s,
            rid,
            SlashDestination::Burn,
            1_000,
            AssetTag::Octo,
            proof,
            now,
        )
        .await;
        let err = r.unwrap_err();
        assert_eq!(err.discriminant(), 0x16);
    }

    #[tokio::test]
    async fn gov_2_amount_mismatch_rejected_with_0x16() {
        let s = store();
        let rid = RecorderId::from_u64(9);
        let now = 1_700_000_000;
        let proof = proof_with(
            rid,
            Some(SlashDestination::Treasury),
            1_000,
            AssetTag::Octo,
            now,
        );
        let r = issue_governance_slash(
            &s,
            rid,
            SlashDestination::Treasury,
            9_999, // mismatch: signed 1000, caller 9999
            AssetTag::Octo,
            proof,
            now,
        )
        .await;
        let err = r.unwrap_err();
        assert_eq!(err.discriminant(), 0x16);
    }

    #[tokio::test]
    async fn gov_2_asset_mismatch_rejected_with_0x16() {
        let s = store();
        let rid = RecorderId::from_u64(10);
        let now = 1_700_000_000;
        let proof = proof_with(
            rid,
            Some(SlashDestination::Treasury),
            1_000,
            AssetTag::Octo, // signed Octo
            now,
        );
        let r = issue_governance_slash(
            &s,
            rid,
            SlashDestination::Treasury,
            1_000,
            AssetTag::RoleToken, // caller passes RoleToken
            proof,
            now,
        )
        .await;
        let err = r.unwrap_err();
        assert_eq!(err.discriminant(), 0x16);
    }

    #[tokio::test]
    async fn gov_2_recorder_id_mismatch_rejected_with_0x16() {
        // Sign a proof for recorder A; caller invokes for recorder B.
        let s = store();
        let now = 1_700_000_000;
        let proof_a = proof_with(
            RecorderId::from_u64(11),
            Some(SlashDestination::Treasury),
            1_000,
            AssetTag::Octo,
            now,
        );
        let r = issue_governance_slash(
            &s,
            RecorderId::from_u64(99), // wrong recorder
            SlashDestination::Treasury,
            1_000,
            AssetTag::Octo,
            proof_a,
            now,
        )
        .await;
        let err = r.unwrap_err();
        assert_eq!(err.discriminant(), 0x16);
    }
}
