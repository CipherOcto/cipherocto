//! Coordinator Term Handover — RFC-0855p-e (v1.3 reconciled)
//!
//! Implements the `HandoverRequestEnvelope` (subtype `b"HORQ"`),
//! `HandoverAckEnvelope` (subtype `b"HOAK"`),
//! `HandoverDoneEnvelope` (subtype `b"HODN"`), and
//! `HandoverCancelEnvelope` (subtype `b"HORC"`) types, plus the supporting
//! `HandoverReason`, `CoordinatorRole`, `SlashTally`, `SlashEvent`,
//! `SenderStateSnapshotOrdinal`, `MeshAggregatedSignature`,
//! `horq_quorum(witness_set_size)`, and HOAK second-witness quorum gate.
//!
//! See RFC-0855p-e §"Data Structure (preliminary)" and
//! `missions/claimed/0855p-e-handover-envelope-substrate.md` Phase 1+2.
//!
//! ## Canonical 10-byte header
//!
//! All three envelopes use the canonical 10-byte header per RFC-0850p-c §A:
//! `envelope_type = b"DOT1"`, the per-envelope subtype tag, and
//! `version = u16 // 0x0001`. Bodies are serialized in field-declaration
//! order, with fixed-size integers big-endian, byte arrays verbatim, and
//! `String`/`Vec<u8>` length-prefixed by a big-endian `u32` count.

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use thiserror::Error;

use super::binding::{header, write_string, GroupBinding, ENVELOPE_TYPE, ENVELOPE_VERSION};

#[cfg(test)]
use super::binding::GroupState;
use super::error::DotError;

// -----------------------------------------------------------------------------
// v1.3 cross-RFC canonical home re-export
// -----------------------------------------------------------------------------

/// Forward-skew tolerance (envelope epoch ahead of recipient head). Canonical
/// home: RFC-0855p-d1 (per plateau closure `docs/audits/2026-09-02-rfc-0855p-de-review-plateau.md`).
/// Re-exported here so handover acceptance can use `±MAX_FSKEW_EPOCHS = 4`
/// without importing from d1.
pub use super::subgroup_state::MAX_FSKEW_EPOCHS;

// -----------------------------------------------------------------------------
// Subtype tags
// -----------------------------------------------------------------------------

/// Subtype tag for `HandoverRequestEnvelope`.
pub const HANDOVER_REQUEST_TAG: [u8; 4] = *b"HORQ";
/// Subtype tag for `HandoverAckEnvelope`.
pub const HANDOVER_ACK_TAG: [u8; 4] = *b"HOAK";
/// Subtype tag for `HandoverDoneEnvelope`.
pub const HANDOVER_DONE_TAG: [u8; 4] = *b"HODN";
/// Subtype tag for `HandoverCancelEnvelope` (incumbent cancel during lockout).
pub const HANDOVER_REQUEST_CANCEL: [u8; 4] = *b"HORC";

// -----------------------------------------------------------------------------
// BLAKE3 domain separation contexts (RFC-0853)
// -----------------------------------------------------------------------------

/// BLAKE3 domain separation string for HORQ replay-key tuple derivation.
pub const HORQ_CONTEXT: &str = "DOT/1/HANDOVER_REQUEST";
/// BLAKE3 domain separation string for HOAK replay-key tuple derivation.
pub const HOAK_CONTEXT: &str = "DOT/1/HANDOVER_ACK";
/// BLAKE3 domain separation string for HODN replay-key tuple derivation.
pub const HODN_CONTEXT: &str = "DOT/1/HANDOVER_DONE";
/// BLAKE3 domain separation string for HORC replay-key tuple derivation.
pub const HORC_CONTEXT: &str = "DOT/1/HANDOVER_CANCEL";

/// BLAKE3 domain separation string for the cross-envelope MeshAggregatedSignature.
pub const MESH_AGGREGATED_SIGNATURE: &str = "DOT/1/HANDOVER_MAS";

// -----------------------------------------------------------------------------
// v1.3 named constants (HANDOVER_RACE_WINDOW split per plateau closure)
// -----------------------------------------------------------------------------

/// Concurrent HORQ race window (backward + concurrent bound). Distinct from
/// `HORQ_BACKWARD_WINDOW` (backward-replay bound) and `HANDOVER_FORWARD_SKEW_BOOST`
/// (forward-skew tolerance). Per RFC-0855p-e §Layer placement table.
pub const HANDOVER_RACE_WINDOW: u64 = 5;

/// HORQ backward-replay window (replaces the deprecated `HANDOVER_REPLAY_WINDOW`
/// phantom constant). Replay-key rejection range when accepting HORQ envelopes.
pub const HORQ_BACKWARD_WINDOW: u64 = 5;

/// Forward-skew boost (HODN acceptance site). 0 = no boost; recipient uses
/// canonical `MAX_FSKEW_EPOCHS = 4` from d1. Reserved for future RFC-amendment
/// without breaking the constant surface.
pub const HANDOVER_FORWARD_SKEW_BOOST: u64 = 0;

// -----------------------------------------------------------------------------
// SenderStateSnapshotOrdinal (Layer A — typed wrapper with private field)
// -----------------------------------------------------------------------------

/// Typed ordinal for the sender's state snapshot at HORQ emission time.
/// Internal field is PRIVATE (NOT `pub u8`) so external callers MUST route
/// through `new()` for validation per RFC-0855p-e §Data Structure invariant.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SenderStateSnapshotOrdinal(u8);

impl SenderStateSnapshotOrdinal {
    /// Active state (witness may accept single HOAK).
    pub const ACTIVE: u8 = 0x01;
    /// Pending state (witness MUST gather `horq_quorum(witness_set_size)`
    /// distinct second-witness HOAKs before accepting HORQ).
    pub const PENDING: u8 = 0x02;
    /// Suspect state (witness MUST gather full witness-set quorum + report).
    pub const SUSPECT: u8 = 0x03;

    /// Construct from a raw byte. Returns `InvalidOrdinal` for unknown values.
    pub fn new(byte: u8) -> Result<Self, InvalidOrdinalError> {
        match byte {
            Self::ACTIVE | Self::PENDING | Self::SUSPECT => Ok(Self(byte)),
            _ => Err(InvalidOrdinalError { got: byte }),
        }
    }

    pub fn as_byte(&self) -> u8 {
        self.0
    }

    pub fn is_active(&self) -> bool {
        self.0 == Self::ACTIVE
    }
}

/// Error type for `SenderStateSnapshotOrdinal::new`.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
#[error("invalid SenderStateSnapshotOrdinal byte 0x{got:02x} (must be 0x01=Active, 0x02=Pending, 0x03=Suspect)")]
pub struct InvalidOrdinalError {
    pub got: u8,
}

// -----------------------------------------------------------------------------
// Layer-B re-exports from octo-coordinator-types shared crate
// (per mission 0855p-e-coordinator-types-shared-crate)
// -----------------------------------------------------------------------------

/// Slash tally update event — shared Layer-B type per RFC-0855p-e
/// §Layer-C Substrate Types follow-on note. Canonical home:
/// `octo_coordinator_types::SlashTallyUpdate`. Re-exported here so
/// downstream consumers can continue importing from `octo_network::dot::handover`.
pub use octo_coordinator_types::SlashTallyUpdate;

/// `SlashReasonCode` typed discriminator — shared Layer-B type. Canonical
/// home: `octo_coordinator_types::SlashReasonCode`. Re-exported here for
/// backward compat. The shared crate variant is `#[non_exhaustive]`-free
/// with `Extension(u16)` catching user-extension registry 0x0100-0xFFFF;
/// `reason_id()` returns raw u16 for wire mapping.
pub use octo_coordinator_types::{HandoverReasonTypeId, SlashReasonCode};

// -----------------------------------------------------------------------------
// MeshAggregatedSignature (Layer C — bitmap + aggregated BLS signature)
// -----------------------------------------------------------------------------

/// Mesh-aggregated signature surface for HODN acceptance. Bitmap covers both
/// HORC + S2PA predecessors per RFC-0855p-e §Mesh Aggregated Signature Coverage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MeshAggregatedSignature {
    /// Signers bitmap (bit i set = witness i signed).
    pub signers_bitmap: Vec<u8>,
    /// BLS12-381 G1 compressed aggregated signature (48 bytes).
    pub aggregated_signature: [u8; 48],
}

impl MeshAggregatedSignature {
    /// Construct from signers bitmap + aggregated signature.
    pub fn new(signers_bitmap: Vec<u8>, aggregated_signature: [u8; 48]) -> Self {
        Self {
            signers_bitmap,
            aggregated_signature,
        }
    }

    /// Count of distinct signers (bits set).
    pub fn count_signers(&self) -> u32 {
        self.signers_bitmap
            .iter()
            .map(|b| b.count_ones())
            .sum::<u32>()
    }
}

// -----------------------------------------------------------------------------
// horq_quorum (Layer C — distinct from hodn_quorum in d3)
// -----------------------------------------------------------------------------

/// HORQ-side quorum policy: minimum distinct witnesses for HORQ acceptance.
///
/// Distinct from `hodn_quorum` (d3 canonical home per plateau closure); both
/// functions take `witness_set_size`, but the policy domain differs (HORQ-side
/// vs HODN-side). Kept separate on purpose per `docs/audits/2026-09-02-rfc-0855p-de-review-plateau.md`.
///
/// Formula: `max(witness_set_size * 2 / 3, 2)` (floor 2; 0 only when set is empty).
/// Examples: 0→0, 1→2, 2→2, 3→2, 6→4, 9→6.
pub fn horq_quorum(witness_set_size: usize) -> usize {
    if witness_set_size == 0 {
        return 0;
    }
    (witness_set_size * 2 / 3).max(2)
}

/// Forward-reference re-export of `hodn_quorum` (canonical home lives in d3
/// per `docs/audits/2026-09-02-rfc-0855p-de-review-plateau.md`). Formula
/// `(wss * 2).div_ceil(3)` per RFC-0855p-d3 §Data Structure.
pub use super::subgroup_routing::hodn_quorum;

// -----------------------------------------------------------------------------
// HandoverReason
// -----------------------------------------------------------------------------

/// Reason a coordinator is initiating a handover.
///
/// See RFC-0855p-e §"Data Structure (preliminary)".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum HandoverReason {
    /// Coordinator chooses to hand over voluntarily (e.g., maintenance window).
    Voluntary = 0x00,
    /// Coordinator term limit reached; scheduled handover.
    Scheduled = 0x01,
    /// Coordinator failed heartbeat checks; the witness quorum flagged it
    /// as suspect and the coordinator is handing over.
    Suspect = 0x02,
    /// Coordinator was slashed; forced handover to recover the term.
    Demoting = 0x03,
    /// Mission terminated; the coordinator hands over its final state.
    MissionTerminated = 0x04,
}

impl HandoverReason {
    /// Construct from wire byte.
    pub fn from_byte(byte: u8) -> Option<Self> {
        match byte {
            0x00 => Some(Self::Voluntary),
            0x01 => Some(Self::Scheduled),
            0x02 => Some(Self::Suspect),
            0x03 => Some(Self::Demoting),
            0x04 => Some(Self::MissionTerminated),
            _ => None,
        }
    }

    /// Returns the wire byte.
    pub fn as_byte(self) -> u8 {
        self as u8
    }
}

// -----------------------------------------------------------------------------
// CoordinatorRole
// -----------------------------------------------------------------------------

/// Type of coordinator initiating the handover.
///
/// (R16 R1-L3 fix: this enum was referenced by RFC-0855p-e §"Data Structure"
/// but was not defined in v0.1 of the RFC. Inlined here.)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum CoordinatorRole {
    /// Mission Coordinator (RFC-0855p-b).
    MissionCoordinator = 0x00,
    /// Domain Coordinator (RFC-0855p-c).
    DomainCoordinator = 0x01,
    /// Witness Coordinator (RFC-0855p-b §4).
    WitnessCoordinator = 0x02,
}

impl CoordinatorRole {
    /// Construct from wire byte.
    pub fn from_byte(byte: u8) -> Option<Self> {
        match byte {
            0x00 => Some(Self::MissionCoordinator),
            0x01 => Some(Self::DomainCoordinator),
            0x02 => Some(Self::WitnessCoordinator),
            _ => None,
        }
    }

    /// Returns the wire byte.
    pub fn as_byte(self) -> u8 {
        self as u8
    }
}

// -----------------------------------------------------------------------------
// SlashTally + SlashEvent
// -----------------------------------------------------------------------------

/// A single slash event.
///
/// See RFC-0855p-e §"SlashTally struct" (R16 R1-H5 fix: inlined here; the
/// previous version referenced non-existent RFC-0855p-b.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlashEvent {
    /// Slash reason code (per RFC-0855p-b §B code space 0x0001-0xFFFF).
    pub slash_reason_code: u16,
    /// Public key of the slashed peer.
    pub slashed_peer_id: [u8; 32],
    /// Number of witness signatures collected.
    pub witness_count: u16,
    /// BLAKE3 hash of the evidence envelope chain.
    pub slash_evidence_hash: [u8; 32],
    /// Epoch when the slash was applied.
    pub epoch: u64,
    /// Coordinator's signature over the event payload.
    pub signature: [u8; 64],
}

impl SlashEvent {
    /// Serialize the slash event payload (everything except the signature).
    pub fn payload_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(2 + 32 + 2 + 32 + 8);
        buf.extend_from_slice(&self.slash_reason_code.to_be_bytes());
        buf.extend_from_slice(&self.slashed_peer_id);
        buf.extend_from_slice(&self.witness_count.to_be_bytes());
        buf.extend_from_slice(&self.slash_evidence_hash);
        buf.extend_from_slice(&self.epoch.to_be_bytes());
        buf
    }

    /// Sign the event in place.
    ///
    /// R17 R1-LOW-10 fix: the call
    /// `ed25519_dalek::Signer::sign(key, &&payload).to_bytes()` has
    /// been replaced with the equivalent shorthand `key.sign(&&payload).to_bytes()`
    /// (via the `Signer` trait brought into scope at the top of the
    /// module). The behaviour is identical; the new form matches
    /// every other `sign(...)` call site in the DOT protocol.
    pub fn sign(&mut self, key: &SigningKey) {
        let payload = self.payload_bytes();
        self.signature = key.sign(&payload).to_bytes();
    }

    /// Verify the coordinator's signature.
    ///
    /// R17 R1-HIGH-8 fix: the `blake3::hash(&&payload)` is now computed
    /// inside the `map_err` closure, so it only runs on the error
    /// path. Previously it ran unconditionally on every successful
    /// verify, wasting ~1µs of CPU per call.
    pub fn verify(&self, coordinator_pubkey: &VerifyingKey) -> Result<(), DotError> {
        let payload = self.payload_bytes();
        let sig = Signature::from_bytes(&self.signature);
        coordinator_pubkey
            .verify(&payload, &sig)
            .map_err(|_| DotError::InvalidSignature {
                envelope_id: *blake3::hash(&payload).as_bytes(),
            })?;
        Ok(())
    }
}

/// Per-coordinator slash tally.
///
/// On handover, the tally is transferred to the successor so the new
/// coordinator continues enforcement (per RFC-0855p-e Design Goal 2).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SlashTally {
    /// Slash events the coordinator has witnessed/been a party to.
    pub slash_events: Vec<SlashEvent>,
    /// Epoch of the most recent tally update.
    pub last_updated_epoch: u64,
}

impl SlashTally {
    /// Construct an empty tally.
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of slash events.
    pub fn len(&self) -> usize {
        self.slash_events.len()
    }

    /// `true` if the tally has no events.
    pub fn is_empty(&self) -> bool {
        self.slash_events.is_empty()
    }

    /// Append a slash event and update the timestamp.
    ///
    /// R17 R1-MEDIUM-10 fix: previously this function silently
    /// accepted any `SlashEvent` without verifying its signature.
    /// A forged event could be smuggled into the tally and then
    /// transferred to a successor coordinator on handover, poisoning
    /// the successor's slash state.
    ///
    /// The signature is now verified against the provided coordinator
    /// public key. A forged or tampered event is rejected with
    /// `HandoverError::SlashTallyInvalid` (R17 R1-MEDIUM-9 fix: this
    /// also puts the previously-unused `HandoverError` enum to work).
    pub fn append(
        &mut self,
        event: SlashEvent,
        coordinator_pubkey: &VerifyingKey,
        current_epoch: u64,
    ) -> Result<(), HandoverError> {
        event
            .verify(coordinator_pubkey)
            .map_err(|_| HandoverError::SlashTallyInvalid {
                index: self.slash_events.len(),
            })?;
        self.slash_events.push(event);
        self.last_updated_epoch = current_epoch;
        Ok(())
    }

    /// Serialize the tally body (for hashing/signing in handover envelopes).
    pub fn body_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(8 + 16 * self.slash_events.len());
        buf.extend_from_slice(&(self.slash_events.len() as u32).to_be_bytes());
        for ev in &self.slash_events {
            buf.extend_from_slice(&ev.payload_bytes());
            buf.extend_from_slice(&ev.signature);
        }
        buf.extend_from_slice(&self.last_updated_epoch.to_be_bytes());
        buf
    }
}

// -----------------------------------------------------------------------------
// Handover envelope types
// -----------------------------------------------------------------------------

/// Coordinator handover request (DOT/1/HANDOVER_REQUEST).
///
/// See RFC-0855p-e §"Data Structure (preliminary)". R16 R1-C1 fix: the
/// 1-byte subtype + 1-byte version stub from v0.1 has been replaced with the
/// canonical 10-byte header per RFC-0850p-c §A.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HandoverRequestEnvelope {
    /// `b"DOT1"`.
    pub envelope_type: [u8; 4],
    /// `b"HORQ"`.
    pub envelope_subtype: [u8; 4],
    /// `0x0001` (canonical version).
    pub version: u16,
    /// Current coordinator's peer_id.
    pub coordinator_id: [u8; 32],
    /// Proposed successor's peer_id.
    pub successor_id: [u8; 32],
    /// Type of coordinator handing over.
    pub coordinator_role: CoordinatorRole,
    /// Current term id.
    pub current_term_id: [u8; 32],
    /// Proposed new term id.
    pub new_term_id: [u8; 32],
    /// Slash tally to be transferred.
    pub slash_tally: SlashTally,
    /// Group bindings to be transferred.
    pub group_bindings: Vec<GroupBinding>,
    /// BLAKE3 hash of pending envelopes to be transferred.
    pub pending_envelopes_hash: [u8; 32],
    /// Reason for handover.
    pub reason: HandoverReason,
    /// 32-byte random nonce (R17 R1-HIGH-7: was `[u8; 16]`, now 32 for consistency).
    pub nonce: [u8; 32],
    /// Current epoch.
    pub current_epoch: u64,
    /// `BLAKE3-256(header || body)`.
    pub handover_hash: [u8; 32],
    /// Ed25519 signature over `handover_hash`.
    pub signature: [u8; 64],
}

impl HandoverRequestEnvelope {
    /// Construct a new `HandoverRequestEnvelope` with the canonical header
    /// populated. The caller fills in the rest of the fields and then calls
    /// `sign(...)` before transmitting.
    pub fn new(
        coordinator_id: [u8; 32],
        successor_id: [u8; 32],
        coordinator_role: CoordinatorRole,
        current_term_id: [u8; 32],
        new_term_id: [u8; 32],
        reason: HandoverReason,
        current_epoch: u64,
    ) -> Self {
        Self {
            envelope_type: ENVELOPE_TYPE,
            envelope_subtype: HANDOVER_REQUEST_TAG,
            version: ENVELOPE_VERSION,
            coordinator_id,
            successor_id,
            coordinator_role,
            current_term_id,
            new_term_id,
            slash_tally: SlashTally::new(),
            group_bindings: Vec::new(),
            pending_envelopes_hash: [0u8; 32],
            reason,
            nonce: [0u8; 32],
            current_epoch,
            handover_hash: [0u8; 32],
            signature: [0u8; 64],
        }
    }

    /// Serialize the body (everything after the 10-byte header) to bytes.
    pub fn body_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(512);
        buf.extend_from_slice(&self.coordinator_id);
        buf.extend_from_slice(&self.successor_id);
        buf.push(self.coordinator_role.as_byte());
        buf.extend_from_slice(&self.current_term_id);
        buf.extend_from_slice(&self.new_term_id);
        buf.extend_from_slice(&self.slash_tally.body_bytes());
        // Group bindings: length-prefixed (u32 BE count), then each binding
        // serialized by its DCS-canonical layout. For the handover envelope
        // we use a length-prefixed binary representation (R17 R1-MEDIUM-7
        // fix: the previous comment claimed this was "JSON-ish" — it has
        // never been JSON, it is a length-prefixed binary payload built
        // from `group_binding_payload(gb)`). This keeps this module
        // self-contained (DCS-canonical GroupBinding serialization is
        // owned by `binding.rs` and depends on the full envelope family).
        buf.extend_from_slice(&(self.group_bindings.len() as u32).to_be_bytes());
        for gb in &self.group_bindings {
            let payload = group_binding_payload(gb);
            buf.extend_from_slice(&(payload.len() as u32).to_be_bytes());
            buf.extend_from_slice(&payload);
        }
        buf.extend_from_slice(&self.pending_envelopes_hash);
        buf.push(self.reason.as_byte());
        buf.extend_from_slice(&self.nonce);
        buf.extend_from_slice(&self.current_epoch.to_be_bytes());
        buf
    }

    /// Compute `handover_hash = BLAKE3-256(header || body)`.
    pub fn compute_handover_hash(&self) -> [u8; 32] {
        let mut buf = Vec::with_capacity(10 + 512);
        buf.extend_from_slice(&header(HANDOVER_REQUEST_TAG));
        buf.extend_from_slice(&self.body_bytes());
        *blake3::hash(&buf).as_bytes()
    }

    /// Sign the envelope in place. Recomputes `handover_hash` and signs it.
    pub fn sign(&mut self, key: &SigningKey) {
        self.handover_hash = self.compute_handover_hash();
        self.signature = key.sign(&self.handover_hash).to_bytes();
    }

    /// Verify the signature against the coordinator's public key.
    pub fn verify(&self, coordinator_pubkey: &VerifyingKey) -> Result<(), DotError> {
        let computed = self.compute_handover_hash();
        if computed != self.handover_hash {
            return Err(DotError::Serialization(format!(
                "HandoverRequestEnvelope: handover_hash mismatch (computed {:02x?}, stored {:02x?})",
                &computed[..8],
                &self.handover_hash[..8]
            )));
        }
        let sig = Signature::from_bytes(&self.signature);
        coordinator_pubkey
            .verify(&self.handover_hash, &sig)
            .map_err(|_e| DotError::InvalidSignature {
                envelope_id: self.handover_hash,
            })?;
        Ok(())
    }
}

/// Witness ACK of a HANDOVER_REQUEST (DOT/1/HANDOVER_ACK).
///
/// See RFC-0855p-e §"Data Structure" — R16 R2 fix: the v0.2 RFC listed
/// this envelope in the Envelope Type Added table (subtype `b"HOAK"`) but
/// did not define the struct. Added here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HandoverAckEnvelope {
    /// `b"DOT1"`.
    pub envelope_type: [u8; 4],
    /// `b"HOAK"`.
    pub envelope_subtype: [u8; 4],
    /// `0x0001` (canonical version).
    pub version: u16,
    /// BLAKE3-256 of the HANDOVER_REQUEST envelope being acked.
    pub handover_request_hash: [u8; 32],
    /// Witness's peer_id.
    pub witness_id: [u8; 32],
    /// Witness's current epoch.
    pub witness_epoch: u64,
    /// BLAKE3-256(handover_request_hash || witness_id || witness_epoch).
    pub ack_hash: [u8; 32],
    /// 32-byte random nonce (R17 R1-HIGH-7: was `[u8; 16]`, now 32 for consistency).
    pub nonce: [u8; 32],
    /// Ed25519 signature over `ack_hash`.
    pub signature: [u8; 64],
}

impl HandoverAckEnvelope {
    /// Construct a new `HandoverAckEnvelope` with the canonical header
    /// populated. The caller MUST set `nonce` and call `sign(...)` before
    /// transmitting (R17 R1-CRITICAL-1 fix: nonce is now in the hash).
    pub fn new(handover_request_hash: [u8; 32], witness_id: [u8; 32], witness_epoch: u64) -> Self {
        let ack_hash = compute_ack_hash(
            &handover_request_hash,
            &witness_id,
            witness_epoch,
            &[0u8; 32],
        );
        Self {
            envelope_type: ENVELOPE_TYPE,
            envelope_subtype: HANDOVER_ACK_TAG,
            version: ENVELOPE_VERSION,
            handover_request_hash,
            witness_id,
            witness_epoch,
            ack_hash,
            nonce: [0u8; 32],
            signature: [0u8; 64],
        }
    }

    /// Compute the ACK hash from `(handover_request_hash, witness_id, witness_epoch, nonce)`.
    pub fn compute_ack_hash(&self) -> [u8; 32] {
        compute_ack_hash(
            &self.handover_request_hash,
            &self.witness_id,
            self.witness_epoch,
            &self.nonce,
        )
    }

    /// Serialize the body (everything after the 10-byte header) to bytes.
    pub fn body_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(32 + 32 + 8 + 32 + 16);
        buf.extend_from_slice(&self.handover_request_hash);
        buf.extend_from_slice(&self.witness_id);
        buf.extend_from_slice(&self.witness_epoch.to_be_bytes());
        buf.extend_from_slice(&self.ack_hash);
        buf.extend_from_slice(&self.nonce);
        buf
    }

    /// Sign in place. Recomputes `ack_hash` and signs it.
    pub fn sign(&mut self, key: &SigningKey) {
        self.ack_hash = self.compute_ack_hash();
        self.signature = key.sign(&self.ack_hash).to_bytes();
    }

    /// Verify against the witness's public key.
    pub fn verify(&self, witness_pubkey: &VerifyingKey) -> Result<(), DotError> {
        let computed = self.compute_ack_hash();
        if computed != self.ack_hash {
            return Err(DotError::Serialization(format!(
                "HandoverAckEnvelope: ack_hash mismatch (computed {:02x?}, stored {:02x?})",
                &computed[..8],
                &self.ack_hash[..8]
            )));
        }
        let sig = Signature::from_bytes(&self.signature);
        witness_pubkey
            .verify(&self.ack_hash, &sig)
            .map_err(|_e| DotError::InvalidSignature {
                envelope_id: self.ack_hash,
            })?;
        Ok(())
    }
}

/// New coordinator's confirmation (DOT/1/HANDOVER_DONE).
///
/// See RFC-0855p-e §"Data Structure" — R16 R2 fix: same as
/// `HandoverAckEnvelope`; the struct was missing from the v0.2 RFC.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HandoverDoneEnvelope {
    /// `b"DOT1"`.
    pub envelope_type: [u8; 4],
    /// `b"HODN"`.
    pub envelope_subtype: [u8; 4],
    /// `0x0001` (canonical version).
    pub version: u16,
    /// BLAKE3-256 of the HANDOVER_REQUEST being confirmed.
    pub handover_request_hash: [u8; 32],
    /// New coordinator's peer_id.
    pub new_coordinator_id: [u8; 32],
    /// Epoch at which the new coordinator accepts.
    pub accepted_epoch: u64,
    /// BLAKE3-256(handover_request_hash || new_coordinator_id || accepted_epoch).
    pub done_hash: [u8; 32],
    /// 32-byte random nonce (R17 R1-HIGH-7: was `[u8; 16]`, now 32 for consistency).
    pub nonce: [u8; 32],
    /// Ed25519 signature over `done_hash`.
    pub signature: [u8; 64],
}

impl HandoverDoneEnvelope {
    /// Construct a new `HandoverDoneEnvelope` with the canonical header
    /// populated. The caller MUST set `nonce` and call `sign(...)` before
    /// transmitting (R17 R1-CRITICAL-1 fix: nonce is now in the hash).
    pub fn new(
        handover_request_hash: [u8; 32],
        new_coordinator_id: [u8; 32],
        accepted_epoch: u64,
    ) -> Self {
        let done_hash = compute_done_hash(
            &handover_request_hash,
            &new_coordinator_id,
            accepted_epoch,
            &[0u8; 32],
        );
        Self {
            envelope_type: ENVELOPE_TYPE,
            envelope_subtype: HANDOVER_DONE_TAG,
            version: ENVELOPE_VERSION,
            handover_request_hash,
            new_coordinator_id,
            accepted_epoch,
            done_hash,
            nonce: [0u8; 32],
            signature: [0u8; 64],
        }
    }

    /// Compute `done_hash` from `(handover_request_hash, new_coordinator_id, accepted_epoch, nonce)`.
    pub fn compute_done_hash(&self) -> [u8; 32] {
        compute_done_hash(
            &self.handover_request_hash,
            &self.new_coordinator_id,
            self.accepted_epoch,
            &self.nonce,
        )
    }

    /// Serialize the body (everything after the 10-byte header) to bytes.
    pub fn body_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(32 + 32 + 8 + 32 + 16);
        buf.extend_from_slice(&self.handover_request_hash);
        buf.extend_from_slice(&self.new_coordinator_id);
        buf.extend_from_slice(&self.accepted_epoch.to_be_bytes());
        buf.extend_from_slice(&self.done_hash);
        buf.extend_from_slice(&self.nonce);
        buf
    }

    /// Sign in place. Recomputes `done_hash` and signs it.
    pub fn sign(&mut self, key: &SigningKey) {
        self.done_hash = self.compute_done_hash();
        self.signature = key.sign(&self.done_hash).to_bytes();
    }

    /// Verify against the new coordinator's public key.
    pub fn verify(&self, new_coordinator_pubkey: &VerifyingKey) -> Result<(), DotError> {
        let computed = self.compute_done_hash();
        if computed != self.done_hash {
            return Err(DotError::Serialization(format!(
                "HandoverDoneEnvelope: done_hash mismatch (computed {:02x?}, stored {:02x?})",
                &computed[..8],
                &self.done_hash[..8]
            )));
        }
        let sig = Signature::from_bytes(&self.signature);
        new_coordinator_pubkey
            .verify(&self.done_hash, &sig)
            .map_err(|_e| DotError::InvalidSignature {
                envelope_id: self.done_hash,
            })?;
        Ok(())
    }
}

// -----------------------------------------------------------------------------
// HandoverCancelEnvelope (DOT/1/HANDOVER_CANCEL)
// -----------------------------------------------------------------------------

/// Incumbent coordinator's cancel envelope (DOT/1/HORC). Clears incumbent-HORQ
/// lockout rule per RFC-0855p-e §Security Considerations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HandoverCancelEnvelope {
    /// `b"DOT1"`.
    pub envelope_type: [u8; 4],
    /// `b"HORC"`.
    pub envelope_subtype: [u8; 4],
    /// `0x0001` (canonical version).
    pub version: u16,
    /// Incumbent coordinator's peer_id.
    pub incumbent_coordinator_id: [u8; 32],
    /// Current term id (the term being cancelled).
    pub current_term_id: [u8; 32],
    /// BLAKE3-256 of the in-flight HORQ envelope being cancelled.
    pub cancelled_horq_hash: [u8; 32],
    /// Current epoch.
    pub current_epoch: u64,
    /// 16-byte random nonce.
    pub nonce: [u8; 16],
    /// BLAKE3-256(incumbent_coordinator_id || current_term_id ||
    ///   cancelled_horq_hash || current_epoch || nonce)
    ///   — domain-prefixed per RFC-0855p-e §Payload Hash.
    pub payload_hash: [u8; 32],
    /// Ed25519 signature over `payload_hash`.
    pub signature: [u8; 64],
}

impl HandoverCancelEnvelope {
    /// Construct a new `HandoverCancelEnvelope` with canonical header populated.
    pub fn new(
        incumbent_coordinator_id: [u8; 32],
        current_term_id: [u8; 32],
        cancelled_horq_hash: [u8; 32],
        current_epoch: u64,
    ) -> Self {
        let nonce = [0u8; 16];
        let payload_hash = compute_horc_payload_hash(
            &incumbent_coordinator_id,
            &current_term_id,
            &cancelled_horq_hash,
            current_epoch,
            &nonce,
        );
        Self {
            envelope_type: ENVELOPE_TYPE,
            envelope_subtype: HANDOVER_REQUEST_CANCEL,
            version: ENVELOPE_VERSION,
            incumbent_coordinator_id,
            current_term_id,
            cancelled_horq_hash,
            current_epoch,
            nonce,
            payload_hash,
            signature: [0u8; 64],
        }
    }

    /// Serialize the body (everything after the 10-byte header) to bytes.
    pub fn body_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(32 + 32 + 32 + 8 + 16 + 32);
        buf.extend_from_slice(&self.incumbent_coordinator_id);
        buf.extend_from_slice(&self.current_term_id);
        buf.extend_from_slice(&self.cancelled_horq_hash);
        buf.extend_from_slice(&self.current_epoch.to_be_bytes());
        buf.extend_from_slice(&self.nonce);
        buf.extend_from_slice(&self.payload_hash);
        buf
    }

    /// Compute `payload_hash` with current field values.
    pub fn compute_payload_hash(&self) -> [u8; 32] {
        compute_horc_payload_hash(
            &self.incumbent_coordinator_id,
            &self.current_term_id,
            &self.cancelled_horq_hash,
            self.current_epoch,
            &self.nonce,
        )
    }

    /// Sign the envelope in place. Recomputes `payload_hash` and signs it.
    pub fn sign(&mut self, key: &SigningKey) {
        self.payload_hash = self.compute_payload_hash();
        self.signature = key.sign(&self.payload_hash).to_bytes();
    }

    /// Verify the signature against the incumbent coordinator's public key.
    pub fn verify(&self, incumbent_pubkey: &VerifyingKey) -> Result<(), DotError> {
        let computed = self.compute_payload_hash();
        if computed != self.payload_hash {
            return Err(DotError::Serialization(format!(
                "HandoverCancelEnvelope: payload_hash mismatch (computed {:02x?}, stored {:02x?})",
                &computed[..8],
                &self.payload_hash[..8]
            )));
        }
        let sig = Signature::from_bytes(&self.signature);
        incumbent_pubkey
            .verify(&self.payload_hash, &sig)
            .map_err(|_e| DotError::InvalidSignature {
                envelope_id: self.payload_hash,
            })?;
        Ok(())
    }
}

// -----------------------------------------------------------------------------
// HOAK second-witness quorum gate (RFC-0855p-e §Security Considerations)
// -----------------------------------------------------------------------------

/// HOAK second-witness quorum gate result per RFC-0855p-e §Security
/// Considerations: when accepting a HORQ whose `sender_state_snapshot_ordinal
/// != SenderStateSnapshotOrdinal::ACTIVE`, count distinct
/// `attests_to_predecessor_state=true` HOAK signatures per
/// `(coordinator_id, coordinator_term_id, current_epoch)` and reject unless
/// count reaches `horq_quorum(witness_set_size)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HoakQuorumDecision {
    /// Gate passes — `count >= horq_quorum(witness_set_size)`.
    Pass {
        distinct_witnesses: usize,
        required: usize,
    },
    /// Gate fails — `count < horq_quorum(witness_set_size)`.
    Fail {
        distinct_witnesses: usize,
        required: usize,
    },
}

/// Evaluate the HOAK second-witness quorum gate.
///
/// `attests_to_predecessor_state_per_witness` is `witness_set_size`-long;
/// `true` at index i means witness i's HOAK attests to the predecessor state
/// (the second-witness quorum path).
pub fn evaluate_hoak_second_witness_quorum(
    sender_ordinal: &SenderStateSnapshotOrdinal,
    witness_set_size: usize,
    attests_to_predecessor_state_per_witness: &[bool],
) -> HoakQuorumDecision {
    if sender_ordinal.is_active() {
        // Active state: gate is automatic pass (single HOAK sufficient).
        return HoakQuorumDecision::Pass {
            distinct_witnesses: attests_to_predecessor_state_per_witness.len(),
            required: 0,
        };
    }
    let distinct_witnesses = attests_to_predecessor_state_per_witness
        .iter()
        .filter(|b| **b)
        .count();
    let required = horq_quorum(witness_set_size);
    if distinct_witnesses >= required {
        HoakQuorumDecision::Pass {
            distinct_witnesses,
            required,
        }
    } else {
        HoakQuorumDecision::Fail {
            distinct_witnesses,
            required,
        }
    }
}

// -----------------------------------------------------------------------------
// Helpers
// -----------------------------------------------------------------------------

/// BLAKE3-256(handover_request_hash || witness_id || witness_epoch || nonce).
///
/// R17 R1-CRITICAL-1 fix: nonce is now INCLUDED so swapping or stripping
/// the nonce changes the hash and breaks the signature.
fn compute_ack_hash(
    handover_request_hash: &[u8; 32],
    witness_id: &[u8; 32],
    witness_epoch: u64,
    nonce: &[u8; 32],
) -> [u8; 32] {
    let mut buf = Vec::with_capacity(32 + 32 + 8 + 32);
    buf.extend_from_slice(handover_request_hash);
    buf.extend_from_slice(witness_id);
    buf.extend_from_slice(&witness_epoch.to_be_bytes());
    buf.extend_from_slice(nonce);
    *blake3::hash(&buf).as_bytes()
}

/// BLAKE3-256(handover_request_hash || new_coordinator_id || accepted_epoch || nonce).
///
/// R17 R1-CRITICAL-1 fix: nonce is now INCLUDED so swapping or stripping
/// the nonce changes the hash and breaks the signature.
fn compute_done_hash(
    handover_request_hash: &[u8; 32],
    new_coordinator_id: &[u8; 32],
    accepted_epoch: u64,
    nonce: &[u8; 32],
) -> [u8; 32] {
    let mut buf = Vec::with_capacity(32 + 32 + 8 + 32);
    buf.extend_from_slice(handover_request_hash);
    buf.extend_from_slice(new_coordinator_id);
    buf.extend_from_slice(&accepted_epoch.to_be_bytes());
    buf.extend_from_slice(nonce);
    *blake3::hash(&buf).as_bytes()
}

/// Canonical `GroupBinding` payload for handover (DCS-style:
/// strings length-prefixed, fixed-size fields big-endian, byte arrays
/// verbatim, `state` as a single byte).
fn group_binding_payload(gb: &GroupBinding) -> Vec<u8> {
    let mut buf = Vec::with_capacity(256);
    write_string(&mut buf, &gb.group_jid);
    write_string(&mut buf, &gb.platform);
    buf.extend_from_slice(&gb.mission_id);
    buf.extend_from_slice(&gb.domain_id);
    buf.extend_from_slice(&gb.domain_coordinator_id);
    buf.extend_from_slice(&gb.bound_at_epoch.to_be_bytes());
    buf.extend_from_slice(&gb.renewed_at_epoch.to_be_bytes());
    buf.push(gb.state.as_byte());
    buf.extend_from_slice(&gb.binding_hash);
    buf
}

/// BLAKE3-256(incumbent_coordinator_id || current_term_id ||
///   cancelled_horq_hash || current_epoch || nonce).
///
/// Domain-prefixed per RFC-0855p-e §Payload Hash (HORC envelope).
fn compute_horc_payload_hash(
    incumbent_coordinator_id: &[u8; 32],
    current_term_id: &[u8; 32],
    cancelled_horq_hash: &[u8; 32],
    current_epoch: u64,
    nonce: &[u8; 16],
) -> [u8; 32] {
    let mut buf = Vec::with_capacity(32 + 32 + 32 + 8 + 16);
    buf.extend_from_slice(incumbent_coordinator_id);
    buf.extend_from_slice(current_term_id);
    buf.extend_from_slice(cancelled_horq_hash);
    buf.extend_from_slice(&current_epoch.to_be_bytes());
    buf.extend_from_slice(nonce);
    *blake3::hash(&buf).as_bytes()
}

// -----------------------------------------------------------------------------
// Errors
// -----------------------------------------------------------------------------

/// Handover-specific errors.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum HandoverError {
    /// A slash tally entry failed signature verification.
    #[error("slash tally entry {index} failed signature verification")]
    SlashTallyInvalid {
        /// Index of the invalid entry.
        index: usize,
    },
    /// A group binding in the handover has an unknown state byte.
    #[error("group binding has unknown state byte 0x{byte:02x}")]
    UnknownGroupState {
        /// The unknown byte.
        byte: u8,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::SigningKey;

    fn make_key(seed: u8) -> SigningKey {
        SigningKey::from_bytes(&[seed; 32])
    }

    fn make_slash_event(reason: u16, slashed: [u8; 32], epoch: u64) -> SlashEvent {
        let mut ev = SlashEvent {
            slash_reason_code: reason,
            slashed_peer_id: slashed,
            witness_count: 3,
            slash_evidence_hash: [0x99u8; 32],
            epoch,
            signature: [0u8; 64],
        };
        ev.sign(&make_key(11));
        ev
    }

    #[test]
    fn handover_reason_round_trip() {
        for r in [
            HandoverReason::Voluntary,
            HandoverReason::Scheduled,
            HandoverReason::Suspect,
            HandoverReason::Demoting,
            HandoverReason::MissionTerminated,
        ] {
            assert_eq!(HandoverReason::from_byte(r.as_byte()), Some(r));
        }
        assert_eq!(HandoverReason::from_byte(0x05), None);
    }

    #[test]
    fn coordinator_role_round_trip() {
        for r in [
            CoordinatorRole::MissionCoordinator,
            CoordinatorRole::DomainCoordinator,
            CoordinatorRole::WitnessCoordinator,
        ] {
            assert_eq!(CoordinatorRole::from_byte(r.as_byte()), Some(r));
        }
        assert_eq!(CoordinatorRole::from_byte(0x03), None);
    }

    #[test]
    fn slash_event_sign_verify() {
        let key = make_key(11);
        let pubkey = key.verifying_key();
        let ev = make_slash_event(0x000E, [0x77u8; 32], 100);
        assert!(ev.verify(&pubkey).is_ok());
        // Tampered evidence -> signature fails.
        let mut bad = ev.clone();
        bad.slash_evidence_hash = [0x00u8; 32];
        assert!(bad.verify(&pubkey).is_err());
    }

    #[test]
    fn slash_tally_append_and_body() {
        let mut tally = SlashTally::new();
        assert!(tally.is_empty());
        let coord_key = make_key(11);
        let coord_pubkey = coord_key.verifying_key();
        tally
            .append(
                make_slash_event(0x000E, [0x77u8; 32], 50),
                &coord_pubkey,
                50,
            )
            .unwrap();
        tally
            .append(
                make_slash_event(0x000F, [0x88u8; 32], 100),
                &coord_pubkey,
                100,
            )
            .unwrap();
        assert_eq!(tally.len(), 2);
        assert_eq!(tally.last_updated_epoch, 100);
        let body = tally.body_bytes();
        assert!(!body.is_empty());
    }

    #[test]
    fn slash_tally_append_rejects_forged_event() {
        // R17 R1-MEDIUM-10 regression: a forged SlashEvent (signed by
        // a different key than the one we claim) must be rejected
        // when appended to the tally. Previously `append` blindly
        // accepted any event, allowing a malicious coordinator to
        // poison a successor's tally on handover.
        let mut tally = SlashTally::new();
        let claimed_key = make_key(11);
        let real_key = make_key(22);
        let claimed_pubkey = claimed_key.verifying_key();

        // Build a SlashEvent signed by the wrong key.
        let mut ev = SlashEvent {
            slash_reason_code: 0x000E,
            slashed_peer_id: [0x77u8; 32],
            witness_count: 3,
            slash_evidence_hash: [0x99u8; 32],
            epoch: 50,
            signature: [0u8; 64],
        };
        ev.sign(&real_key);

        let result = tally.append(ev, &claimed_pubkey, 50);
        assert!(matches!(
            result,
            Err(HandoverError::SlashTallyInvalid { index: 0 })
        ));
        assert!(tally.is_empty(), "forged event must NOT be in the tally");
    }

    #[test]
    fn handover_request_sign_verify_round_trip() {
        let key = make_key(1);
        let pubkey = key.verifying_key();
        // make_slash_event signs with make_key(11) (the coordinator
        // that emitted the slash). Use that key's verifying key when
        // appending, since `append` now verifies the event signature
        // (R17 R1-MEDIUM-10).
        let slash_coord_key = make_key(11);
        let slash_coord_pubkey = slash_coord_key.verifying_key();
        let mut tally = SlashTally::new();
        tally
            .append(
                make_slash_event(0x000E, [0x77u8; 32], 100),
                &slash_coord_pubkey,
                100,
            )
            .unwrap();

        let mut env = HandoverRequestEnvelope::new(
            [0x11u8; 32],
            [0x22u8; 32],
            CoordinatorRole::DomainCoordinator,
            [0x33u8; 32],
            [0x44u8; 32],
            HandoverReason::Voluntary,
            200,
        );
        env.slash_tally = tally;
        env.group_bindings.push(GroupBinding {
            group_jid: "120363@g.us".to_string(),
            platform: "whatsapp".to_string(),
            mission_id: [0x55u8; 32],
            domain_id: [0x66u8; 32],
            domain_coordinator_id: [0x77u8; 32],
            bound_at_epoch: 1,
            renewed_at_epoch: 100,
            state: GroupState::Bound,
            binding_hash: [0x88u8; 32],
        });
        env.pending_envelopes_hash = [0x99u8; 32];
        env.nonce = [0xAAu8; 32];

        env.sign(&key);
        assert!(env.verify(&pubkey).is_ok());
    }

    #[test]
    fn handover_request_signature_failure_on_tamper() {
        let key = make_key(1);
        let pubkey = key.verifying_key();
        let mut env = HandoverRequestEnvelope::new(
            [0x11u8; 32],
            [0x22u8; 32],
            CoordinatorRole::MissionCoordinator,
            [0x33u8; 32],
            [0x44u8; 32],
            HandoverReason::Scheduled,
            200,
        );
        env.nonce = [0xAAu8; 32];
        env.sign(&key);
        env.current_epoch = 999;
        assert!(env.verify(&pubkey).is_err());
    }

    #[test]
    fn handover_request_wrong_key_fails() {
        let key = make_key(1);
        let other = make_key(2);
        let mut env = HandoverRequestEnvelope::new(
            [0x11u8; 32],
            [0x22u8; 32],
            CoordinatorRole::WitnessCoordinator,
            [0x33u8; 32],
            [0x44u8; 32],
            HandoverReason::Demoting,
            200,
        );
        env.sign(&key);
        assert!(env.verify(&other.verifying_key()).is_err());
    }

    #[test]
    fn handover_ack_sign_verify_round_trip() {
        let key = make_key(3);
        let pubkey = key.verifying_key();
        let mut env = HandoverAckEnvelope::new([0xA1u8; 32], *pubkey.as_bytes(), 250);
        env.nonce = [0xC1u8; 32];
        env.sign(&key);
        assert!(env.verify(&pubkey).is_ok());
    }

    // R17 R1-CRITICAL-1 regression test: changing the nonce must change
    // the hash so an attacker cannot swap a stored envelope's nonce to
    // bypass replay protection.
    #[test]
    fn handover_ack_nonce_changes_hash() {
        let key = make_key(3);
        let pubkey = key.verifying_key();
        let mut env = HandoverAckEnvelope::new([0xA1u8; 32], *pubkey.as_bytes(), 250);
        env.nonce = [0xC1u8; 32];
        env.sign(&key);
        let original_hash = env.ack_hash;
        env.nonce = [0xC2u8; 32];
        env.sign(&key);
        assert_ne!(env.ack_hash, original_hash);
    }

    #[test]
    fn handover_ack_compute_matches_manual_blake3() {
        let key = make_key(3);
        let pubkey = key.verifying_key();
        let mut env = HandoverAckEnvelope::new([0xA1u8; 32], *pubkey.as_bytes(), 250);
        env.nonce = [0xC1u8; 32];
        env.sign(&key);
        // Recompute manually — R17 R1-CRITICAL-1 fix: nonce included.
        let mut buf = Vec::new();
        buf.extend_from_slice(&[0xA1u8; 32]);
        buf.extend_from_slice(pubkey.as_bytes());
        buf.extend_from_slice(&250u64.to_be_bytes());
        buf.extend_from_slice(&env.nonce);
        let expected = *blake3::hash(&buf).as_bytes();
        assert_eq!(env.ack_hash, expected);
    }

    #[test]
    fn handover_ack_tamper_fails() {
        let key = make_key(3);
        let pubkey = key.verifying_key();
        let mut env = HandoverAckEnvelope::new([0xA1u8; 32], *pubkey.as_bytes(), 250);
        env.nonce = [0xC1u8; 32];
        env.sign(&key);
        env.witness_epoch = 999;
        assert!(env.verify(&pubkey).is_err());
    }

    #[test]
    fn handover_done_sign_verify_round_trip() {
        let key = make_key(4);
        let pubkey = key.verifying_key();
        let mut env = HandoverDoneEnvelope::new([0xD1u8; 32], *pubkey.as_bytes(), 500);
        env.nonce = [0xE1u8; 32];
        env.sign(&key);
        assert!(env.verify(&pubkey).is_ok());
    }

    // R17 R1-CRITICAL-1 regression test: changing the nonce must change
    // the hash so an attacker cannot swap a stored envelope's nonce to
    // bypass replay protection.
    #[test]
    fn handover_done_nonce_changes_hash() {
        let key = make_key(4);
        let pubkey = key.verifying_key();
        let mut env = HandoverDoneEnvelope::new([0xD1u8; 32], *pubkey.as_bytes(), 500);
        env.nonce = [0xE1u8; 32];
        env.sign(&key);
        let original_hash = env.done_hash;
        env.nonce = [0xE2u8; 32];
        env.sign(&key);
        assert_ne!(env.done_hash, original_hash);
    }

    #[test]
    fn handover_done_compute_matches_manual_blake3() {
        let key = make_key(4);
        let pubkey = key.verifying_key();
        let mut env = HandoverDoneEnvelope::new([0xD1u8; 32], *pubkey.as_bytes(), 500);
        env.nonce = [0xE1u8; 32];
        env.sign(&key);
        // Recompute manually — R17 R1-CRITICAL-1 fix: nonce included.
        let mut buf = Vec::new();
        buf.extend_from_slice(&[0xD1u8; 32]);
        buf.extend_from_slice(pubkey.as_bytes());
        buf.extend_from_slice(&500u64.to_be_bytes());
        buf.extend_from_slice(&env.nonce);
        let expected = *blake3::hash(&buf).as_bytes();
        assert_eq!(env.done_hash, expected);
    }

    #[test]
    fn handover_done_tamper_fails() {
        let key = make_key(4);
        let pubkey = key.verifying_key();
        let mut env = HandoverDoneEnvelope::new([0xD1u8; 32], *pubkey.as_bytes(), 500);
        env.nonce = [0xE1u8; 32];
        env.sign(&key);
        env.accepted_epoch = 999;
        assert!(env.verify(&pubkey).is_err());
    }

    #[test]
    fn handover_headers_canonical() {
        let mut env = HandoverRequestEnvelope::new(
            [0; 32],
            [0; 32],
            CoordinatorRole::MissionCoordinator,
            [0; 32],
            [0; 32],
            HandoverReason::Voluntary,
            0,
        );
        env.sign(&make_key(0));
        assert_eq!(env.envelope_type, *b"DOT1");
        assert_eq!(env.envelope_subtype, *b"HORQ");
        assert_eq!(env.version, 1);

        let ack = HandoverAckEnvelope::new([0; 32], [0; 32], 0);
        assert_eq!(ack.envelope_type, *b"DOT1");
        assert_eq!(ack.envelope_subtype, *b"HOAK");
        assert_eq!(ack.version, 1);

        let done = HandoverDoneEnvelope::new([0; 32], [0; 32], 0);
        assert_eq!(done.envelope_type, *b"DOT1");
        assert_eq!(done.envelope_subtype, *b"HODN");
        assert_eq!(done.version, 1);
    }

    #[test]
    fn handover_request_with_multiple_bindings() {
        let key = make_key(5);
        let pubkey = key.verifying_key();
        let mut env = HandoverRequestEnvelope::new(
            [0x11u8; 32],
            [0x22u8; 32],
            CoordinatorRole::DomainCoordinator,
            [0x33u8; 32],
            [0x44u8; 32],
            HandoverReason::Suspect,
            1_000,
        );
        for i in 0..5 {
            env.group_bindings.push(GroupBinding {
                group_jid: format!("g{i}@example.com"),
                platform: "matrix".to_string(),
                mission_id: [0x55u8; 32],
                domain_id: [i as u8; 32],
                domain_coordinator_id: [0x77u8; 32],
                bound_at_epoch: 1,
                renewed_at_epoch: 100,
                state: GroupState::Bound,
                binding_hash: [i as u8; 32],
            });
        }
        env.sign(&key);
        assert!(env.verify(&pubkey).is_ok());
    }

    #[test]
    fn slash_event_payload_deterministic() {
        let ev1 = make_slash_event(0x000E, [0x77u8; 32], 100);
        let ev2 = make_slash_event(0x000E, [0x77u8; 32], 100);
        assert_eq!(ev1.payload_bytes(), ev2.payload_bytes());
        assert_eq!(ev1.signature, ev2.signature);
    }

    // ============================================================================
    // v1.3 RFC-0855p-e reconciled test vectors
    // ============================================================================

    // TV-HO-1: valid HORQ acceptance; sender_state_snapshot_ordinal = Active; happy path.
    #[test]
    fn tv_ho_1_horq_acceptance_active_ordinal() {
        let ordinal = SenderStateSnapshotOrdinal::new(SenderStateSnapshotOrdinal::ACTIVE).unwrap();
        assert!(ordinal.is_active());
        // witness_set_size=3 → horq_quorum(3)=2. Single HOAK with Active ordinal
        // is sufficient (gate is automatic pass for Active).
        let decision = evaluate_hoak_second_witness_quorum(&ordinal, 3, &[true]);
        assert!(matches!(
            decision,
            HoakQuorumDecision::Pass {
                distinct_witnesses: 1,
                required: 0
            }
        ));
    }

    // TV-HO-2: HORC envelope — incumbent cancel during lockout.
    #[test]
    fn tv_ho_2_horc_envelope_sign_verify() {
        let key = make_key(7);
        let pubkey = key.verifying_key();
        let mut env = HandoverCancelEnvelope::new(
            [0x11u8; 32],
            [0x22u8; 32],
            [0x33u8; 32], // cancelled_horq_hash
            200,
        );
        env.nonce = [0x44u8; 16];
        env.sign(&key);
        assert!(env.verify(&pubkey).is_ok());
    }

    // TV-HO-3: HORC payload_hash changes when nonce changes.
    #[test]
    fn tv_ho_3_horc_payload_hash_nonce_changes() {
        let key = make_key(7);
        let mut env = HandoverCancelEnvelope::new([0x11u8; 32], [0x22u8; 32], [0x33u8; 32], 200);
        env.nonce = [0x01; 16];
        env.sign(&key);
        let original = env.payload_hash;
        env.nonce = [0x02; 16];
        env.sign(&key);
        assert_ne!(env.payload_hash, original);
    }

    // TV-HO-4: HORC wrong key fails.
    #[test]
    fn tv_ho_4_horc_wrong_key_fails() {
        let key = make_key(7);
        let other = make_key(8);
        let mut env = HandoverCancelEnvelope::new([0x11u8; 32], [0x22u8; 32], [0x33u8; 32], 200);
        env.nonce = [0x01; 16];
        env.sign(&key);
        assert!(env.verify(&other.verifying_key()).is_err());
    }

    // TV-HO-5: HANDOVER_RACE_WINDOW + HORQ_BACKWARD_WINDOW + HANDOVER_FORWARD_SKEW_BOOST
    // constants split correctly per plateau closure.
    #[test]
    fn tv_ho_5_race_window_constants_split() {
        assert_eq!(HANDOVER_RACE_WINDOW, 5);
        assert_eq!(HORQ_BACKWARD_WINDOW, 5);
        assert_eq!(HANDOVER_FORWARD_SKEW_BOOST, 0);
        assert_eq!(MAX_FSKEW_EPOCHS, 4);
    }

    // TV-HO-6: BLAKE3 domain separation contexts match RFC-0853.
    #[test]
    fn tv_ho_6_blake3_contexts() {
        assert_eq!(HORQ_CONTEXT, "DOT/1/HANDOVER_REQUEST");
        assert_eq!(HOAK_CONTEXT, "DOT/1/HANDOVER_ACK");
        assert_eq!(HODN_CONTEXT, "DOT/1/HANDOVER_DONE");
        assert_eq!(HORC_CONTEXT, "DOT/1/HANDOVER_CANCEL");
        assert_eq!(MESH_AGGREGATED_SIGNATURE, "DOT/1/HANDOVER_MAS");
    }

    // TV-HO-7: SenderStateSnapshotOrdinal private field; new() + InvalidOrdinal.
    #[test]
    fn tv_ho_7_sender_state_snapshot_ordinal() {
        // Active round-trip.
        let active = SenderStateSnapshotOrdinal::new(0x01).unwrap();
        assert_eq!(active.as_byte(), 0x01);
        assert!(active.is_active());
        // Pending + Suspect.
        let pending = SenderStateSnapshotOrdinal::new(0x02).unwrap();
        assert!(!pending.is_active());
        let suspect = SenderStateSnapshotOrdinal::new(0x03).unwrap();
        assert!(!suspect.is_active());
        // Invalid byte → InvalidOrdinalError.
        let bad = SenderStateSnapshotOrdinal::new(0x00);
        assert!(matches!(bad, Err(InvalidOrdinalError { got: 0x00 })));
        let bad2 = SenderStateSnapshotOrdinal::new(0xFF);
        assert!(matches!(bad2, Err(InvalidOrdinalError { got: 0xFF })));
    }

    // TV-HO-8: SlashReasonCode 0x0013-0x0016 entries (per RFC-0855p-e §Future Work F-7).
    #[test]
    fn tv_ho_8_slash_reason_code_future_work_f7() {
        assert_eq!(SlashReasonCode::FalseAttestation.reason_id(), 0x0013);
        assert_eq!(SlashReasonCode::QuorumTimeout.reason_id(), 0x0014);
        assert_eq!(SlashReasonCode::TallyTamper.reason_id(), 0x0015);
        assert_eq!(SlashReasonCode::LateDelivery.reason_id(), 0x0016);
        // Extension variant.
        let ext = SlashReasonCode::Extension(0x0100);
        assert_eq!(ext.reason_id(), 0x0100);
    }

    // TV-HO-9: HOAK second-witness quorum gate — witness_set_size=3, horq_quorum(3)=2.
    // Pending ordinal requires ≥2 distinct second-witness HOAKs.
    #[test]
    fn tv_ho_9_hoak_second_witness_quorum_gate() {
        let pending = SenderStateSnapshotOrdinal::new(SenderStateSnapshotOrdinal::PENDING).unwrap();
        // 1 second-witness → reject (count=1, required=2).
        let decision = evaluate_hoak_second_witness_quorum(&pending, 3, &[true, false, false]);
        assert!(matches!(
            decision,
            HoakQuorumDecision::Fail {
                distinct_witnesses: 1,
                required: 2
            }
        ));
        // 2 distinct second-witness HOAKs → gate passes.
        let decision = evaluate_hoak_second_witness_quorum(&pending, 3, &[true, true, false]);
        assert!(matches!(
            decision,
            HoakQuorumDecision::Pass {
                distinct_witnesses: 2,
                required: 2
            }
        ));
        // Active ordinal → automatic pass (no quorum required).
        let active = SenderStateSnapshotOrdinal::new(SenderStateSnapshotOrdinal::ACTIVE).unwrap();
        let decision = evaluate_hoak_second_witness_quorum(&active, 3, &[true]);
        assert!(matches!(
            decision,
            HoakQuorumDecision::Pass {
                distinct_witnesses: 1,
                required: 0
            }
        ));
    }

    // TV-HO-10: horq_quorum vs hodn_quorum are DISTINCT functions (per plateau closure).
    #[test]
    fn tv_ho_10_horq_quorum_distinct_from_hodn_quorum() {
        // witness_set_size=3 → both return 2 (per mission spec).
        // Formula: max(witness_set_size * 2 / 3, 2).
        assert_eq!(horq_quorum(0), 0);
        assert_eq!(horq_quorum(1), 2);
        assert_eq!(horq_quorum(2), 2);
        assert_eq!(horq_quorum(3), 2);
        assert_eq!(horq_quorum(6), 4);
        assert_eq!(horq_quorum(9), 6);
        // Same formula for hodn_quorum (placeholder; canonical home in d3).
        assert_eq!(hodn_quorum(3), 2);
    }

    // TV-HO-11: MeshAggregatedSignature count_signers.
    #[test]
    fn tv_ho_11_mesh_aggregated_signature_signers_count() {
        let bitmap = vec![0b0000_0111, 0b0000_0011]; // 5 distinct signers
        let mas = MeshAggregatedSignature::new(bitmap, [0u8; 48]);
        assert_eq!(mas.count_signers(), 5);
        let empty = MeshAggregatedSignature::new(vec![], [0u8; 48]);
        assert_eq!(empty.count_signers(), 0);
    }

    // TV-HO-12: HandoverReasonTypeId typed discriminator.
    #[test]
    fn tv_ho_12_handover_reason_type_id() {
        let id = HandoverReasonTypeId([0xABu8; 16]);
        assert_eq!(id.0, [0xABu8; 16]);
    }

    // TV-HO-13: SlashTallyUpdate basic shape.
    #[test]
    fn tv_ho_13_slash_tally_update() {
        let upd = SlashTallyUpdate {
            slash_reason_code: 0x0013,
            slashed_peer_id: [0x77u8; 32],
            witness_count: 3,
            epoch: 100,
        };
        assert_eq!(upd.slash_reason_code, 0x0013);
        assert_eq!(upd.witness_count, 3);
    }
}
