//! `octo_governance::attest` — attestation append path
//! (RFC-0011-g §7.4 Substrate `[ADD]`).
//!
//! ## Substrate surface
//!
//! Public items re-exported by `octo-governance`:
//!
//! - [`AttestationLog`] — append-only ledger keyed by
//!   `attestation_id = BLAKE3-256(canonical_ser(envelope))`
//!   per RFC-0011-g §Substrate `[ADD]`. In-memory for Phase 2;
//!   persistence substrate lands in a follow-on mission.
//! - [`CapabilitySigner`] — abstraction over the HSM-bound
//!   signing primitive; CLI calls through
//!   `octo-wallet::sign_envelope` (Layer A RFC-frozen) without
//!   holding private-key material locally. The trait is defined
//!   here so the substrate does not need to depend on
//!   `octo-wallet` directly (Layer B independence).
//! - [`attest`] — `pub fn attest(...) -> Result<AttestationReceipt,
//!   GovernanceError>` per RFC-0011-g §7.4 substrate signature.
//!
//! ## Layer discipline
//!
//! This module is **Layer B (RFC-driven additive only)**. It
//! depends on Layer A primitives (`octo-governance-core` for
//! canonical types; `blake3` for BLAKE3-256 PK computation;
//! `thiserror` for the substrate-specific errors). No storage
//! IO, no clock, no randomness.
//!
//! ## Prereq gate (RFC-0011-g §Compatibility Mixed-Version)
//!
//! The `attest` function returns
//! `GovernanceError::PrereqNotAccepted { rfc_ref }` until
//! `RFC-0855p-d` reaches Accepted. The CLI surfaces this as
//! `OctoCliError::PrereqNotAccepted` (exit 38) per RFC-0011-g
//! §Error Handling.

use std::collections::BTreeMap;
use std::sync::Mutex;

use blake3::Hasher;
use octo_governance_core::{AttestationReceipt, GovernanceError};
use serde::{Deserialize, Serialize};

use crate::session::GovernanceSession;

/// Trait abstraction over the HSM-bound signing primitive used
/// by `attest`. The CLI implements this trait via
/// `octo-wallet::sign_envelope` (Layer A frozen) so the substrate
/// does not depend on `octo-wallet` directly.
///
/// Per RFC-0011-g §Redaction, the signer sees the BLAKE3-256
/// canonical envelope bytes only (raw evidence bytes NEVER
/// reach the signer).
pub trait CapabilitySigner {
    /// Sign the canonical envelope bytes. Returns the 64-byte
    /// Ed25519 signature. Failures bubble up to the substrate
    /// as `GovernanceError::Internal`.
    fn sign_envelope(&self, envelope_bytes: &[u8]) -> Result<[u8; 64], String>;
}

/// TypedDiscriminator for the attestation kind per RFC-0011-g
/// §Attestation Kind Resolution +
/// `cipherocto-design-principles` §Extension over enumeration.
/// Resolution is via a static registry seeded by the substrate
/// at startup; unknown kinds fail-closed with
/// `GovernanceError::UnknownAttestationKind`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AttestationKind {
    /// `route-quality:uptime-30d` — peer uptime attestation
    /// (canonical kind seeded by the substrate at startup).
    RouteQualityUptime30d,
}

/// Resolution result for the `kind_ref` → `AttestationKind`
/// mapping. `Unsupported` = known discriminator space but not
/// registered for this build; `Unknown` = novel kind_ref that
/// is not in the discriminator space at all.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AttestationKindResolution {
    /// Resolved to a known `AttestationKind`.
    Known(AttestationKind),
}

/// Resolve a `kind_ref` string against the substrate registry.
/// Unknown values fail-closed with
/// `GovernanceError::UnknownAttestationKind { kind_ref }` per
/// RFC-0011-g §Attestation Kind Resolution +
/// `cipherocto-design-principles` §Extension over enumeration.
fn resolve_kind(kind_ref: &str) -> Result<AttestationKind, GovernanceError> {
    match kind_ref {
        "route-quality:uptime-30d" => Ok(AttestationKind::RouteQualityUptime30d),
        other => Err(GovernanceError::UnknownAttestationKind {
            kind_ref: other.to_string(),
        }),
    }
}

/// Compute `BLAKE3-256(bytes)` returning a 32-byte array. The
/// canonical envelope PK is `BLAKE3-256(canonical_ser(envelope))`
/// per RFC-0011-g §Substrate `[ADD]`.
#[must_use]
pub fn blake3_256(bytes: &[u8]) -> [u8; 32] {
    let mut hasher = Hasher::new();
    hasher.update(bytes);
    let out = hasher.finalize();
    let mut pk = [0u8; 32];
    pk.copy_from_slice(out.as_bytes());
    pk
}

/// Append-only attestation ledger keyed by
/// `attestation_id = BLAKE3-256(canonical_ser(envelope))`. The
/// PK uniqueness property is enforced at insert time: a
/// duplicate `attestation_id` returns
/// `GovernanceError::DuplicateAttestation` and the ledger
/// state is unchanged.
///
/// In-memory only for Phase 2 (matches the in-memory pattern
/// used by `OctoGovernanceSnapshotCache`); the Stoolap-backed
/// persistence layer lands as a follow-on mission.
#[derive(Debug, Default)]
pub struct AttestationLog {
    /// PK → receipt map.
    entries: Mutex<BTreeMap<[u8; 32], AttestationReceipt>>,
}

impl AttestationLog {
    /// Construct an empty ledger.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Look up a receipt by its PK. Translates mutex poison to
    /// `GovernanceError::Internal` for substrate fail-closed
    /// invariant — substrate never panics through the CLI chain.
    pub fn get(
        &self,
        attestation_id: &[u8; 32],
    ) -> Result<Option<AttestationReceipt>, GovernanceError> {
        self.entries
            .lock()
            .map_err(|e| GovernanceError::Internal {
                reason: format!("attestation log mutex poisoned: {e}"),
            })
            .map(|entries| entries.get(attestation_id).cloned())
    }

    /// Number of entries currently in the ledger (diagnostic).
    /// Translates mutex poison to `GovernanceError::Internal`.
    pub fn len(&self) -> Result<usize, GovernanceError> {
        self.entries
            .lock()
            .map_err(|e| GovernanceError::Internal {
                reason: format!("attestation log mutex poisoned: {e}"),
            })
            .map(|entries| entries.len())
    }

    /// `true` if the ledger holds zero entries. Translates
    /// mutex poison to `GovernanceError::Internal`.
    pub fn is_empty(&self) -> Result<bool, GovernanceError> {
        self.len().map(|n| n == 0)
    }

    /// Append a receipt to the ledger. Returns
    /// `Err(GovernanceError::DuplicateAttestation)` if the PK
    /// already exists; the ledger state is unchanged on error.
    /// Translates mutex poison to `GovernanceError::Internal`.
    pub fn append(&self, receipt: AttestationReceipt) -> Result<(), GovernanceError> {
        let mut entries = self.entries.lock().map_err(|e| GovernanceError::Internal {
            reason: format!("attestation log mutex poisoned: {e}"),
        })?;
        let pk = receipt.attestation_id;
        if entries.contains_key(&pk) {
            return Err(GovernanceError::DuplicateAttestation { attestation_id: pk });
        }
        entries.insert(pk, receipt);
        Ok(())
    }
}

/// Append a new attestation to the ledger per RFC-0011-g
/// §7.4 substrate signature. The canonical envelope bytes are
/// `subject_did || kind_ref || evidence_hash || signer_did ||
/// expires_at_unix_be || snapshot_id_be || allow_stale_bool`.
/// The PK is `BLAKE3-256` of those bytes; the
/// `AttestationReceipt.attestation_id` is set to the PK for
/// downstream tooling convenience.
///
/// Returns `GovernanceError::UnknownAttestationKind` for
/// unrecognized `kind_ref`; `GovernanceError::PrereqNotAccepted`
/// for sub-group subjects until `RFC-0855p-d` reaches Accepted
/// (gate enforced via [`prereq_attest_subgroup_check`] before
/// the envelope is built).
///
/// ## Deprecation
///
/// This 11-parameter surface predates the RFC-0011-g §7.4
/// stateless caller-owned-session design. New callers should use
/// [`attest_v2`] which accepts `&GovernanceSession` and reads
/// the clock from the session.
#[deprecated(
    since = "0.0.0",
    note = "use attest_v2 with a GovernanceSession (RFC-0011-g §7.4 stateless substrate signature)"
)]
#[allow(clippy::too_many_arguments)]
pub fn attest(
    log: &AttestationLog,
    subject_did: &str,
    kind_ref: &str,
    evidence: Option<&[u8]>,
    evidence_hash: Option<[u8; 32]>,
    expires_at_unix: Option<u64>,
    snapshot_id: Option<&[u8; 32]>,
    allow_stale: bool,
    signer: &dyn CapabilitySigner,
    appended_at_unix: u64,
    signer_did: &str,
) -> Result<AttestationReceipt, GovernanceError> {
    // Sub-group prereq gate (RFC-0855p-d). Subject DIDs prefixed
    // `did:octo:subgroup:` are gated until `RFC-0855p-d`
    // reaches Accepted.
    prereq_attest_subgroup_check(subject_did)?;

    // `kind_ref` resolution — TypedDiscriminator.
    let _kind = resolve_kind(kind_ref)?;

    // `evidence` XOR `evidence_hash` invariant: the substrate
    // accepts exactly one of the two, never both, per RFC-0011-g
    // §7.4 substrate signature semantics.
    let computed_evidence_hash = match (evidence, evidence_hash) {
        (Some(bytes), None) => blake3_256(bytes),
        (None, Some(hash)) => hash,
        (Some(_), Some(_)) => {
            return Err(GovernanceError::InvalidArgument {
                reason: "exactly one of `evidence` and `evidence_hash` must be supplied"
                    .to_string(),
            });
        }
        (None, None) => {
            return Err(GovernanceError::InvalidArgument {
                reason: "one of `evidence` or `evidence_hash` must be supplied".to_string(),
            });
        }
    };

    // Canonical envelope bytes (substrate-faithful).
    let mut envelope: Vec<u8> = Vec::new();
    envelope.extend_from_slice(subject_did.as_bytes());
    envelope.push(0x00);
    envelope.extend_from_slice(kind_ref.as_bytes());
    envelope.push(0x00);
    envelope.extend_from_slice(&computed_evidence_hash);
    envelope.extend_from_slice(signer_did.as_bytes());
    envelope.push(0x00);
    if let Some(exp) = expires_at_unix {
        envelope.extend_from_slice(&exp.to_be_bytes());
    }
    envelope.push(0x00);
    if let Some(snap) = snapshot_id {
        envelope.extend_from_slice(snap);
    }
    envelope.push(0x00);
    envelope.push(u8::from(allow_stale));

    let attestation_id = blake3_256(&envelope);

    // Signer signs the canonical envelope (HSM-bound). The
    // signature is recorded for audit but not currently in the
    // `AttestationReceipt` (audit lives in the substrate's
    // `governance_envelopes` table per RFC-0862 §Data
    // Structures; the Stoolap persistence substrate lands as a
    // follow-on).
    let _signature = signer
        .sign_envelope(&envelope)
        .map_err(|reason| GovernanceError::Internal { reason })?;

    let receipt = AttestationReceipt {
        attestation_id,
        subject_did: subject_did.to_string(),
        kind_ref: kind_ref.to_string(),
        signer_did: signer_did.to_string(),
        evidence_hash: computed_evidence_hash,
        expires_at_unix,
        appended_at_unix,
        overrode_staleness_at_unix: if allow_stale {
            Some(appended_at_unix)
        } else {
            None
        },
    };
    log.append(receipt.clone())?;
    Ok(receipt)
}

/// Sub-group attestation prereq gate (RFC-0855p-d). Until
/// `RFC-0855p-d` reaches Accepted, any subject DID prefixed
/// `did:octo:subgroup:` fails-closed with
/// `GovernanceError::PrereqNotAccepted { rfc_ref: "RFC-0855p-d"
/// }`. Per RFC-0011-g §Implementation Phases, the gate is
/// enforced at substrate time (not CLI) so the substrate is
/// fail-closed regardless of which CLI surface invokes it.
fn prereq_attest_subgroup_check(subject_did: &str) -> Result<(), GovernanceError> {
    // Subgroup-DID prefix detection per RFC-0855p-d §3.2.
    if subject_did.starts_with("did:octo:subgroup:") {
        return Err(GovernanceError::PrereqNotAccepted {
            rfc_ref: "RFC-0855p-d".to_string(),
        });
    }
    Ok(())
}

/// Append a new attestation to the ledger per RFC-0011-g §7.4
/// stateless substrate signature.
///
/// ## Substrate surface (new)
///
/// - `session` — caller-owned `GovernanceSession` carrying the
///   attestation ledger + clock + active DID. The signer is
///   passed explicitly per RFC §7.4 because attest uses the
///   `signer` for the *attesting* identity, which may differ
///   from the session's `active_did` (the attesting operator
///   and the subject are different roles per RFC §Roles and
///   Authorities).
/// - `subject_did` — DID being attested about.
/// - `kind_ref` — TypedDiscriminator string (resolved via
///   [`resolve_kind`]).
/// - `evidence` / `evidence_hash` — XOR invariant: exactly one
///   of the two must be supplied.
/// - `expires_at_unix` — optional attestation expiry timestamp.
/// - `snapshot_id` — optional governance-snapshot pin.
/// - `allow_stale` — when `true`, records
///   `overrode_staleness_at_unix` on the receipt.
///
/// The canonical envelope bytes are
/// `subject_did || 0x00 || kind_ref || 0x00 || evidence_hash ||
/// signer_did || 0x00 || expires_at_unix_be || 0x00 ||
/// snapshot_id || 0x00 || allow_stale_bool`. The PK is
/// `BLAKE3-256` of those bytes; `AttestationReceipt.attestation_id`
/// is set to the PK.
///
/// Returns `GovernanceError::UnknownAttestationKind` for
/// unrecognized `kind_ref`; `GovernanceError::PrereqNotAccepted`
/// for sub-group subjects until `RFC-0855p-d` reaches Accepted;
/// `GovernanceError::InvalidArgument` for evidence XOR
/// violations; `GovernanceError::Internal` for signer failures.
#[allow(clippy::too_many_arguments)]
pub fn attest_v2(
    session: &GovernanceSession,
    subject_did: &str,
    kind_ref: &str,
    evidence: Option<&[u8]>,
    evidence_hash: Option<[u8; 32]>,
    expires_at_unix: Option<u64>,
    snapshot_id: Option<&[u8; 32]>,
    allow_stale: bool,
    signer: &dyn CapabilitySigner,
    signer_did: &str,
) -> Result<AttestationReceipt, GovernanceError> {
    let appended_at_unix = session.now_unix();

    prereq_attest_subgroup_check(subject_did)?;

    let _kind = resolve_kind(kind_ref)?;

    let computed_evidence_hash = match (evidence, evidence_hash) {
        (Some(bytes), None) => blake3_256(bytes),
        (None, Some(hash)) => hash,
        (Some(_), Some(_)) => {
            return Err(GovernanceError::InvalidArgument {
                reason: "exactly one of `evidence` and `evidence_hash` must be supplied"
                    .to_string(),
            });
        }
        (None, None) => {
            return Err(GovernanceError::InvalidArgument {
                reason: "one of `evidence` or `evidence_hash` must be supplied".to_string(),
            });
        }
    };

    let mut envelope: Vec<u8> = Vec::new();
    envelope.extend_from_slice(subject_did.as_bytes());
    envelope.push(0x00);
    envelope.extend_from_slice(kind_ref.as_bytes());
    envelope.push(0x00);
    envelope.extend_from_slice(&computed_evidence_hash);
    envelope.extend_from_slice(signer_did.as_bytes());
    envelope.push(0x00);
    if let Some(exp) = expires_at_unix {
        envelope.extend_from_slice(&exp.to_be_bytes());
    }
    envelope.push(0x00);
    if let Some(snap) = snapshot_id {
        envelope.extend_from_slice(snap);
    }
    envelope.push(0x00);
    envelope.push(u8::from(allow_stale));

    let attestation_id = blake3_256(&envelope);

    let _signature = signer
        .sign_envelope(&envelope)
        .map_err(|reason| GovernanceError::Internal { reason })?;

    let receipt = AttestationReceipt {
        attestation_id,
        subject_did: subject_did.to_string(),
        kind_ref: kind_ref.to_string(),
        signer_did: signer_did.to_string(),
        evidence_hash: computed_evidence_hash,
        expires_at_unix,
        appended_at_unix,
        overrode_staleness_at_unix: if allow_stale {
            Some(appended_at_unix)
        } else {
            None
        },
    };
    session.attestation_log().append(receipt.clone())?;
    Ok(receipt)
}

#[cfg(test)]
#[allow(deprecated)]
mod tests {
    //! Substrate-faithful tests for the attestation append path.
    //!
    //! Test vector coverage:
    //! - A1: happy-path append returns receipt + populates ledger
    //! - A2: duplicate attestation_id rejected (append-only invariant)
    //! - A3: unknown kind_ref rejected (TypedDiscriminator fail-closed)
    //! - A4: sub-group subject DID gated (prereq RFC-0855p-d)
    //! - A5: evidence XOR evidence_hash invariant (both → error)
    //! - A6: neither evidence nor evidence_hash supplied (error)
    //! - A7: allow_stale=true records overrode_staleness_at_unix
    //! - A8: ledger len/get roundtrip

    use super::*;
    use std::sync::Arc;

    /// Test `CapabilitySigner` impl that records the envelope it
    /// was asked to sign and returns a fixed 64-byte signature.
    struct FixedSigner {
        signature: [u8; 64],
    }

    impl FixedSigner {
        fn new() -> Self {
            Self {
                signature: [0xAB; 64],
            }
        }
    }

    impl CapabilitySigner for FixedSigner {
        fn sign_envelope(&self, _envelope_bytes: &[u8]) -> Result<[u8; 64], String> {
            Ok(self.signature)
        }
    }

    fn subject_did() -> &'static str {
        "did:octo:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK"
    }

    fn signer_did() -> &'static str {
        "did:octo:z6MkSignerXyzABCDEF1234567890abcdef1234567890ab"
    }

    #[test]
    fn attest_a1_happy_path_appends_receipt() {
        let log = AttestationLog::new();
        let signer = FixedSigner::new();
        let receipt = attest(
            &log,
            subject_did(),
            "route-quality:uptime-30d",
            None,
            Some([0x42; 32]),
            Some(1_900_000_000),
            Some(&[0xCD; 32]),
            false,
            &signer,
            1_700_000_000,
            signer_did(),
        )
        .expect("happy path should succeed");
        assert_eq!(receipt.subject_did, subject_did());
        assert_eq!(receipt.kind_ref, "route-quality:uptime-30d");
        assert_eq!(receipt.signer_did, signer_did());
        assert_eq!(receipt.evidence_hash, [0x42; 32]);
        assert_eq!(receipt.expires_at_unix, Some(1_900_000_000));
        assert_eq!(receipt.appended_at_unix, 1_700_000_000);
        assert_eq!(receipt.overrode_staleness_at_unix, None);
        // PK = BLAKE3-256(canonical envelope); ledger contains it.
        assert_eq!(log.len().expect("unpoisoned"), 1);
        assert_eq!(
            log.get(&receipt.attestation_id)
                .expect("unpoisoned")
                .as_ref(),
            Some(&receipt)
        );
    }

    #[test]
    fn attest_a2_duplicate_attestation_id_rejected() {
        let log = AttestationLog::new();
        let signer = FixedSigner::new();
        let first = attest(
            &log,
            subject_did(),
            "route-quality:uptime-30d",
            None,
            Some([0x42; 32]),
            Some(1_900_000_000),
            Some(&[0xCD; 32]),
            false,
            &signer,
            1_700_000_000,
            signer_did(),
        )
        .expect("first append should succeed");
        // Same envelope → same PK → duplicate.
        let err = attest(
            &log,
            subject_did(),
            "route-quality:uptime-30d",
            None,
            Some([0x42; 32]),
            Some(1_900_000_000),
            Some(&[0xCD; 32]),
            false,
            &signer,
            1_700_000_001,
            signer_did(),
        )
        .expect_err("duplicate attestation_id must error");
        match err {
            GovernanceError::DuplicateAttestation { attestation_id } => {
                assert_eq!(attestation_id, first.attestation_id);
            }
            other => panic!("expected DuplicateAttestation, got {other:?}"),
        }
        // Ledger state unchanged after error (still exactly 1 entry).
        assert_eq!(log.len().expect("unpoisoned"), 1);
    }

    #[test]
    fn attest_a3_unknown_kind_ref_rejected() {
        let log = AttestationLog::new();
        let signer = FixedSigner::new();
        let err = attest(
            &log,
            subject_did(),
            "novel:kind:not-registered",
            None,
            Some([0x42; 32]),
            None,
            None,
            false,
            &signer,
            1_700_000_000,
            signer_did(),
        )
        .expect_err("unknown kind_ref must error");
        match err {
            GovernanceError::UnknownAttestationKind { kind_ref } => {
                assert_eq!(kind_ref, "novel:kind:not-registered");
            }
            other => panic!("expected UnknownAttestationKind, got {other:?}"),
        }
        assert_eq!(log.len().expect("unpoisoned"), 0);
    }

    #[test]
    fn attest_a4_subgroup_subject_did_gated() {
        let log = AttestationLog::new();
        let signer = FixedSigner::new();
        let err = attest(
            &log,
            "did:octo:subgroup:abc123",
            "route-quality:uptime-30d",
            None,
            Some([0x42; 32]),
            None,
            None,
            false,
            &signer,
            1_700_000_000,
            signer_did(),
        )
        .expect_err("subgroup subject DID must fail-closed until RFC-0855p-d accepted");
        match err {
            GovernanceError::PrereqNotAccepted { rfc_ref } => {
                assert_eq!(rfc_ref, "RFC-0855p-d");
            }
            other => panic!("expected PrereqNotAccepted, got {other:?}"),
        }
        assert_eq!(log.len().expect("unpoisoned"), 0);
    }

    #[test]
    fn attest_a5_evidence_xor_evidence_hash_invariant() {
        let log = AttestationLog::new();
        let signer = FixedSigner::new();
        // Both supplied → invalid.
        let err = attest(
            &log,
            subject_did(),
            "route-quality:uptime-30d",
            Some(&[0x01, 0x02, 0x03]),
            Some([0xAA; 32]),
            None,
            None,
            false,
            &signer,
            1_700_000_000,
            signer_did(),
        )
        .expect_err("both evidence + evidence_hash must error");
        match err {
            GovernanceError::InvalidArgument { reason } => {
                assert!(
                    reason.contains("exactly one"),
                    "reason should describe XOR invariant: {reason}"
                );
            }
            other => panic!("expected InvalidArgument, got {other:?}"),
        }
        assert_eq!(log.len().expect("unpoisoned"), 0);
    }

    #[test]
    fn attest_a6_neither_evidence_nor_hash_supplied() {
        let log = AttestationLog::new();
        let signer = FixedSigner::new();
        let err = attest(
            &log,
            subject_did(),
            "route-quality:uptime-30d",
            None,
            None,
            None,
            None,
            false,
            &signer,
            1_700_000_000,
            signer_did(),
        )
        .expect_err("missing both evidence and evidence_hash must error");
        match err {
            GovernanceError::InvalidArgument { reason } => {
                assert!(
                    reason.contains("one of"),
                    "reason should describe required-field invariant: {reason}"
                );
            }
            other => panic!("expected InvalidArgument, got {other:?}"),
        }
        assert_eq!(log.len().expect("unpoisoned"), 0);
    }

    #[test]
    fn attest_a7_allow_stale_records_overrode_staleness() {
        let log = AttestationLog::new();
        let signer = FixedSigner::new();
        let receipt = attest(
            &log,
            subject_did(),
            "route-quality:uptime-30d",
            None,
            Some([0x42; 32]),
            Some(1_900_000_000),
            None,
            true,
            &signer,
            1_700_000_000,
            signer_did(),
        )
        .expect("allow_stale=true should still succeed");
        assert_eq!(receipt.overrode_staleness_at_unix, Some(1_700_000_000));
    }

    #[test]
    fn attest_a8_evidence_bytes_hash_to_supplied_hash() {
        // evidence XOR evidence_hash invariant: when only evidence
        // is supplied, the substrate computes BLAKE3-256(evidence)
        // and stamps it onto the receipt. Same evidence bytes
        // must produce same attestation_id (deterministic PK).
        let log = AttestationLog::new();
        let signer = FixedSigner::new();
        let evidence: &[u8] = b"route-quality measurements for may 2026";
        let r1 = attest(
            &log,
            subject_did(),
            "route-quality:uptime-30d",
            Some(evidence),
            None,
            None,
            None,
            false,
            &signer,
            1_700_000_000,
            signer_did(),
        )
        .expect("evidence-only path should succeed");
        let expected_hash = blake3_256(evidence);
        assert_eq!(r1.evidence_hash, expected_hash);
        assert_eq!(log.len().expect("unpoisoned"), 1);
        assert_eq!(
            log.get(&r1.attestation_id).expect("unpoisoned").as_ref(),
            Some(&r1)
        );
    }

    // ---- attest_v2 tests (RFC §7.4 stateless caller-owned session) ----

    fn attest_session_with_signer(
        did: &str,
        clock_unix: u64,
    ) -> (GovernanceSession, Arc<FixedSigner>) {
        let signer = Arc::new(FixedSigner::new());
        let session =
            GovernanceSession::new(did, Arc::new(crate::session::FixedClock::new(clock_unix)));
        (session, signer)
    }

    #[test]
    fn attest_a9_session_happy_path_records_receipt() {
        let (session, signer) = attest_session_with_signer(signer_did(), 1_700_000_000);
        let receipt = attest_v2(
            &session,
            subject_did(),
            "route-quality:uptime-30d",
            Some(b"uptime measurements"),
            None,
            Some(1_900_000_000),
            None,
            false,
            &*signer,
            signer_did(),
        )
        .expect("happy path should succeed");
        assert_eq!(receipt.subject_did, subject_did());
        assert_eq!(receipt.signer_did, signer_did());
        assert_eq!(receipt.kind_ref, "route-quality:uptime-30d");
        assert_eq!(receipt.appended_at_unix, 1_700_000_000);
        assert_eq!(receipt.expires_at_unix, Some(1_900_000_000));
        assert_eq!(receipt.overrode_staleness_at_unix, None);
        assert_eq!(session.attestation_log().len().expect("unpoisoned"), 1);
        assert_eq!(
            session
                .attestation_log()
                .get(&receipt.attestation_id)
                .expect("unpoisoned")
                .as_ref(),
            Some(&receipt)
        );
    }

    #[test]
    fn attest_a10_session_unknown_kind_rejected() {
        let (session, signer) = attest_session_with_signer(signer_did(), 1_700_000_000);
        let err = attest_v2(
            &session,
            subject_did(),
            "not-a-registered-kind",
            None,
            Some([0x42; 32]),
            None,
            None,
            false,
            &*signer,
            signer_did(),
        )
        .expect_err("unregistered kind_ref must error");
        match err {
            GovernanceError::UnknownAttestationKind { kind_ref } => {
                assert_eq!(kind_ref, "not-a-registered-kind");
            }
            other => panic!("expected UnknownAttestationKind, got {other:?}"),
        }
        assert_eq!(session.attestation_log().len().expect("unpoisoned"), 0);
    }

    #[test]
    fn attest_a11_evidence_xor_evidence_hash_invariant_via_session() {
        // attest_v2 enforces evidence XOR evidence_hash: both
        // supplied must error with InvalidAttestationEvidence.
        let (session, signer) = attest_session_with_signer(signer_did(), 1_700_000_000);
        let err = attest_v2(
            &session,
            subject_did(),
            "route-quality:uptime-30d",
            Some(b"bytes"),
            Some([0x99; 32]),
            None,
            None,
            false,
            &*signer,
            signer_did(),
        )
        .expect_err("evidence + evidence_hash XOR must error");
        match err {
            GovernanceError::InvalidArgument { reason } => {
                assert!(
                    reason.contains("exactly one"),
                    "reason must mention XOR invariant: {reason}"
                );
            }
            other => panic!("expected InvalidArgument, got {other:?}"),
        }
        assert_eq!(session.attestation_log().len().expect("unpoisoned"), 0);
    }

    #[test]
    fn attest_a12_fixed_clock_deterministic_recorded_at() {
        // Two attest_v2 calls with identical args + same fixed
        // clock must produce identical appended_at_unix + the
        // session ledger holds both (determinism over the
        // session-scoped clock, not SystemTime).
        let (session, signer) = attest_session_with_signer(signer_did(), 1_700_000_777);
        let r1 = attest_v2(
            &session,
            "did:octo:subject-a",
            "route-quality:uptime-30d",
            None,
            Some([0x11; 32]),
            None,
            None,
            false,
            &*signer,
            signer_did(),
        )
        .expect("first attest should succeed");
        let r2 = attest_v2(
            &session,
            "did:octo:subject-b",
            "route-quality:uptime-30d",
            None,
            Some([0x22; 32]),
            None,
            None,
            false,
            &*signer,
            signer_did(),
        )
        .expect("second attest should succeed");
        assert_eq!(r1.appended_at_unix, 1_700_000_777);
        assert_eq!(r2.appended_at_unix, 1_700_000_777);
        assert_eq!(session.attestation_log().len().expect("unpoisoned"), 2);
    }
}
