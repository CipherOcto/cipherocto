//! Coordinator Term Handover — RFC-0855p-e
//!
//! Implements the `HandoverRequestEnvelope` (subtype `b"HORQ"`),
//! `HandoverAckEnvelope` (subtype `b"HOAK"`), and
//! `HandoverDoneEnvelope` (subtype `b"HODN"`) types, plus the supporting
//! `HandoverReason`, `CoordinatorRole`, `SlashTally`, and `SlashEvent`
//! types.
//!
//! See RFC-0855p-e §"Data Structure (preliminary)" and
//! `missions/claimed/0855p-e-handover-request-envelope.md` Phase 1.
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

use super::binding::{
    header, write_string, GroupBinding, GroupState, ENVELOPE_TYPE, ENVELOPE_VERSION,
};
use super::error::DotError;

// -----------------------------------------------------------------------------
// Subtype tags
// -----------------------------------------------------------------------------

/// Subtype tag for `HandoverRequestEnvelope`.
pub const HANDOVER_REQUEST_TAG: [u8; 4] = *b"HORQ";
/// Subtype tag for `HandoverAckEnvelope`.
pub const HANDOVER_ACK_TAG: [u8; 4] = *b"HOAK";
/// Subtype tag for `HandoverDoneEnvelope`.
pub const HANDOVER_DONE_TAG: [u8; 4] = *b"HODN";

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
    pub fn sign(&mut self, key: &SigningKey) {
        let payload = self.payload_bytes();
        self.signature = ed25519_dalek::Signer::sign(key, &payload).to_bytes();
    }

    /// Verify the coordinator's signature.
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
    pub fn append(&mut self, event: SlashEvent, current_epoch: u64) {
        self.slash_events.push(event);
        self.last_updated_epoch = current_epoch;
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
    /// 16-byte random nonce.
    pub nonce: [u8; 16],
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
            nonce: [0u8; 16],
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
        // we use a length-prefixed JSON-ish representation to keep this
        // module self-contained (DCS-canonical GroupBinding serialization
        // is owned by binding.rs and depends on the full envelope family).
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
    /// 16-byte random nonce.
    pub nonce: [u8; 16],
    /// Ed25519 signature over `ack_hash`.
    pub signature: [u8; 64],
}

impl HandoverAckEnvelope {
    /// Construct a new `HandoverAckEnvelope` with the canonical header
    /// populated.
    pub fn new(
        handover_request_hash: [u8; 32],
        witness_id: [u8; 32],
        witness_epoch: u64,
    ) -> Self {
        let ack_hash = compute_ack_hash(&handover_request_hash, &witness_id, witness_epoch);
        Self {
            envelope_type: ENVELOPE_TYPE,
            envelope_subtype: HANDOVER_ACK_TAG,
            version: ENVELOPE_VERSION,
            handover_request_hash,
            witness_id,
            witness_epoch,
            ack_hash,
            nonce: [0u8; 16],
            signature: [0u8; 64],
        }
    }

    /// Compute the ACK hash from `(handover_request_hash, witness_id, witness_epoch)`.
    pub fn compute_ack_hash(&self) -> [u8; 32] {
        compute_ack_hash(&self.handover_request_hash, &self.witness_id, self.witness_epoch)
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
    /// 16-byte random nonce.
    pub nonce: [u8; 16],
    /// Ed25519 signature over `done_hash`.
    pub signature: [u8; 64],
}

impl HandoverDoneEnvelope {
    /// Construct a new `HandoverDoneEnvelope` with the canonical header
    /// populated.
    pub fn new(
        handover_request_hash: [u8; 32],
        new_coordinator_id: [u8; 32],
        accepted_epoch: u64,
    ) -> Self {
        let done_hash = compute_done_hash(
            &handover_request_hash,
            &new_coordinator_id,
            accepted_epoch,
        );
        Self {
            envelope_type: ENVELOPE_TYPE,
            envelope_subtype: HANDOVER_DONE_TAG,
            version: ENVELOPE_VERSION,
            handover_request_hash,
            new_coordinator_id,
            accepted_epoch,
            done_hash,
            nonce: [0u8; 16],
            signature: [0u8; 64],
        }
    }

    /// Compute `done_hash` from `(handover_request_hash, new_coordinator_id, accepted_epoch)`.
    pub fn compute_done_hash(&self) -> [u8; 32] {
        compute_done_hash(
            &self.handover_request_hash,
            &self.new_coordinator_id,
            self.accepted_epoch,
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
// Helpers
// -----------------------------------------------------------------------------

/// BLAKE3-256(handover_request_hash || witness_id || witness_epoch).
fn compute_ack_hash(
    handover_request_hash: &[u8; 32],
    witness_id: &[u8; 32],
    witness_epoch: u64,
) -> [u8; 32] {
    let mut buf = Vec::with_capacity(32 + 32 + 8);
    buf.extend_from_slice(handover_request_hash);
    buf.extend_from_slice(witness_id);
    buf.extend_from_slice(&witness_epoch.to_be_bytes());
    *blake3::hash(&buf).as_bytes()
}

/// BLAKE3-256(handover_request_hash || new_coordinator_id || accepted_epoch).
fn compute_done_hash(
    handover_request_hash: &[u8; 32],
    new_coordinator_id: &[u8; 32],
    accepted_epoch: u64,
) -> [u8; 32] {
    let mut buf = Vec::with_capacity(32 + 32 + 8);
    buf.extend_from_slice(handover_request_hash);
    buf.extend_from_slice(new_coordinator_id);
    buf.extend_from_slice(&accepted_epoch.to_be_bytes());
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
        tally.append(make_slash_event(0x000E, [0x77u8; 32], 50), 50);
        tally.append(make_slash_event(0x000F, [0x88u8; 32], 100), 100);
        assert_eq!(tally.len(), 2);
        assert_eq!(tally.last_updated_epoch, 100);
        let body = tally.body_bytes();
        assert!(!body.is_empty());
    }

    #[test]
    fn handover_request_sign_verify_round_trip() {
        let key = make_key(1);
        let pubkey = key.verifying_key();
        let mut tally = SlashTally::new();
        tally.append(make_slash_event(0x000E, [0x77u8; 32], 100), 100);

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
        env.nonce = [0xAAu8; 16];

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
        env.nonce = [0xAAu8; 16];
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
        let mut env = HandoverAckEnvelope::new(
            [0xA1u8; 32],
            *pubkey.as_bytes(),
            250,
        );
        env.nonce = [0xC1u8; 16];
        env.sign(&key);
        assert!(env.verify(&pubkey).is_ok());
    }

    #[test]
    fn handover_ack_compute_matches_manual_blake3() {
        let key = make_key(3);
        let pubkey = key.verifying_key();
        let mut env = HandoverAckEnvelope::new(
            [0xA1u8; 32],
            *pubkey.as_bytes(),
            250,
        );
        env.nonce = [0xC1u8; 16];
        env.sign(&key);
        // Recompute manually.
        let mut buf = Vec::new();
        buf.extend_from_slice(&[0xA1u8; 32]);
        buf.extend_from_slice(pubkey.as_bytes());
        buf.extend_from_slice(&250u64.to_be_bytes());
        let expected = *blake3::hash(&buf).as_bytes();
        assert_eq!(env.ack_hash, expected);
    }

    #[test]
    fn handover_ack_tamper_fails() {
        let key = make_key(3);
        let pubkey = key.verifying_key();
        let mut env = HandoverAckEnvelope::new(
            [0xA1u8; 32],
            *pubkey.as_bytes(),
            250,
        );
        env.nonce = [0xC1u8; 16];
        env.sign(&key);
        env.witness_epoch = 999;
        assert!(env.verify(&pubkey).is_err());
    }

    #[test]
    fn handover_done_sign_verify_round_trip() {
        let key = make_key(4);
        let pubkey = key.verifying_key();
        let mut env = HandoverDoneEnvelope::new(
            [0xD1u8; 32],
            *pubkey.as_bytes(),
            500,
        );
        env.nonce = [0xE1u8; 16];
        env.sign(&key);
        assert!(env.verify(&pubkey).is_ok());
    }

    #[test]
    fn handover_done_compute_matches_manual_blake3() {
        let key = make_key(4);
        let pubkey = key.verifying_key();
        let mut env = HandoverDoneEnvelope::new(
            [0xD1u8; 32],
            *pubkey.as_bytes(),
            500,
        );
        env.nonce = [0xE1u8; 16];
        env.sign(&key);
        let mut buf = Vec::new();
        buf.extend_from_slice(&[0xD1u8; 32]);
        buf.extend_from_slice(pubkey.as_bytes());
        buf.extend_from_slice(&500u64.to_be_bytes());
        let expected = *blake3::hash(&buf).as_bytes();
        assert_eq!(env.done_hash, expected);
    }

    #[test]
    fn handover_done_tamper_fails() {
        let key = make_key(4);
        let pubkey = key.verifying_key();
        let mut env = HandoverDoneEnvelope::new(
            [0xD1u8; 32],
            *pubkey.as_bytes(),
            500,
        );
        env.nonce = [0xE1u8; 16];
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
}
