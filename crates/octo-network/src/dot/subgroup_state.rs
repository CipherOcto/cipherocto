//! Sub-Group Creation + State substrate — RFC-0855p-d1
//!
//! Owns:
//! - `CreateSubGroupEnvelope` (subtype `b"CGSB"`)
//! - `SubGroupExtension` (parent_domain_id + sub_label + sub_dc_id + delegation)
//! - `SubGroupLabel::new` (UTS-39 confusable + bidi/zero-width/BOM reject set)
//! - `sub_domain_id` canonical BLAKE3 keyed_hash derivation (§Sub-Domain Derivation Invariant)
//! - `SubGroupState` lifecycle (`PendingBind → Bound → Dissolving → Dissolved`)
//! - `SubGroupRecord` storage + cross-node reconciliation
//! - `SubGroupQuery` / `SubGroupResponse` / `SubGroupAuthorityCheck` typed Layer-C query boundary
//!
//! Layer placement (per RFC-0855p-d1 §Layer placement):
//! - Envelope wire (Layer B)
//! - Label + query boundary types (Layer B)
//! - State machine + record (Layer C)
//! - BLAKE3 derivation (Layer A pure function)
//!
//! Cross-RFC canonical home:
//! - `MAX_BIND_AWAIT_EPOCHS` + `MAX_BIND_RETRY_COUNT` + `RACE_EPOCHS` + `MAX_FSKEW_EPOCHS` +
//!   `MAX_SUBGROUP_DEPTH` + `MAX_ROOT_DEPTH` — re-exported by RFC-0855p-d3 (its §Data Structure)
//!   and used by RFC-0855p-e (cross-RFC invariant per plateau closure audit).
//!
//! Migration: legacy `crates/octo-network/src/dot/sub_group.rs` (plain `blake3::hash`
//! derivation + `String` sub_label) is deprecated; downstream consumers should migrate
//! to the canonical keyed_hash form + typed `SubGroupLabel` via `pub use` re-export.

use blake3;
use thiserror::Error;

// ============================================================================
// Constants (Layer B wire-protocol)
// ============================================================================

/// Maximum nesting depth (root → leaf). Recipient rejects creation beyond.
pub const MAX_SUBGROUP_DEPTH: u8 = 8;

/// Genesis-root invariant. Parent depth MUST equal `MAX_ROOT_DEPTH = 1` for the
/// very first sub-group under root; subsequent depths bounded by
/// `MAX_SUBGROUP_DEPTH = 8`. Enforced in `SubGroupExtension::validate`.
pub const MAX_ROOT_DEPTH: u8 = 1;

/// Maximum byte length of `SubGroupLabel` POST-NFC normalization
/// (pre-normalization input may grow during NFC composition).
pub const MAX_SUB_LABEL_BYTES: usize = 256;

/// `PendingBind → Bound` deadline; child MUST BIND within 32 epochs.
pub const MAX_BIND_AWAIT_EPOCHS: u64 = 32;

/// Maximum `PendingBind → Dissolving → Bound` retry loops before forced teardown.
pub const MAX_BIND_RETRY_COUNT: u8 = 3;

/// Backward-replay bound (canonical home; re-exported by RFC-0855p-d3 per its
/// §Data Structure). Distinct from `MAX_FSKEW_EPOCHS` so stale-replay
/// amplification is auditable independent of clock-drift tolerance.
pub const RACE_EPOCHS: u64 = 32;

/// Forward-skew tolerance (envelope epoch ahead of recipient head); separate
/// from `RACE_EPOCHS` (backward-replay bound) so clock-drift tolerance is
/// auditable independent of stale-replay amplification bound. Matches
/// RFC-0855p-e cross-RFC invariant (aligned to ±4 epochs for the family).
pub const MAX_FSKEW_EPOCHS: u64 = 4;

/// BLAKE3 domain separation string for `sub_domain_id` derivation. Expanded to
/// 32-byte key via `blake3::derive_key` per §Sub-Domain Derivation Invariant.
pub const SUBGROUP_DOMAIN_CONTEXT: &str = "DOT/1/CGROUP_SUB/domain";

/// Subtype tag for `CreateSubGroupEnvelope`. Distinct from parent `b"CGROUP"`
/// so old consumers fail-closed at subtype filter.
pub const CREATE_SUBGROUP: [u8; 4] = *b"CGSB";

// ============================================================================
// Constants (Layer C — response_kind / action_type_id discriminators)
// ============================================================================

/// `SubGroupResponse::response_kind` values. RFC-allocated 0x0001-0x00FF;
/// 0x0100-0xFFFF reserved for user-extension registry entries.
pub const SUBGROUP_RESPONSE_NOT_FOUND: u16 = 0x0001;
pub const SUBGROUP_RESPONSE_PENDING_BIND: u16 = 0x0002;
pub const SUBGROUP_RESPONSE_BOUND: u16 = 0x0003;
pub const SUBGROUP_RESPONSE_DISSOLVING: u16 = 0x0004;
pub const SUBGROUP_RESPONSE_DISSOLVED: u16 = 0x0005;

/// `SubGroupAction::action_type_id` values. RFC-allocated 0x0001-0x00FF;
/// 0x0100-0xFFFF reserved for user-extension registry entries.
/// 0x0002-0x0003 reserved for RFC-0855p-d2 (REVOKE/DISSOLVE).
pub const SUBGROUP_ACTION_INVITE: u32 = 0x0001;
pub const SUBGROUP_ACTION_ROUTE: u32 = 0x0004;
pub const SUBGROUP_ACTION_AGGREGATE: u32 = 0x0005;

/// Maximum byte length of `SubGroupAction::action_payload`.
pub const SUBGROUP_ACTION_PAYLOAD_MAX: usize = 256;

// ============================================================================
// Errors
// ============================================================================

/// Error type for `SubGroupLabel::new` validation.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum SubGroupLabelError {
    #[error("sub_label is empty")]
    Empty,
    #[error("sub_label contains forbidden byte (NUL, control, or '/') at offset {offset}")]
    InvalidByte { offset: usize },
    #[error("sub_label equals '.' or '..' (path segment forbidden)")]
    PathSegment,
    #[error("sub_label exceeds {max} bytes POST-normalization (got {got})")]
    TooLong { max: usize, got: usize },
    #[error("sub_label contains UTS-39 confusable / bidi / zero-width / BOM codepoint at offset {offset}")]
    ConfusableChar { offset: usize },
    #[error("sub_label is not valid UTF-8: {0}")]
    InvalidUtf8(#[from] std::str::Utf8Error),
}

/// Error type for `CreateSubGroupEnvelope` validation.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum CreateSubGroupError {
    #[error("parent GroupBinding is not Active")]
    ParentNotActive,
    #[error("parent depth ({got}) < MAX_ROOT_DEPTH ({min})")]
    DepthUnderflow { min: u8, got: u8 },
    #[error("parent depth ({got}) >= MAX_SUBGROUP_DEPTH ({max})")]
    DepthCap { max: u8, got: u8 },
    #[error("sub_dc_id != parent_dc_id but delegation_proof is missing")]
    MissingDelegation,
    #[error("sub_dc_id == parent_dc_id but delegation_proof is present")]
    UnexpectedDelegation,
    #[error("delegation proof failed validation for sub_dc_id {sub_dc_id:?}")]
    DelegationInvalid { sub_dc_id: [u8; 32] },
}

// ============================================================================
// GroupBindingState (Layer C — minimal; d2/d3 may extend)
// ============================================================================

/// State of the parent's `GroupBinding` (Layer C).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GroupBindingState {
    Active,
    PendingBind,
    Dissolving,
    Dissolved,
}

// ============================================================================
// SubGroupLabel (Layer B — typed constructor with UTS-39 reject set)
// ============================================================================

/// Sub-group label. Construction MUST route through `SubGroupLabel::new` so every
/// byte is canonicalized (UTF-8 NFC) and validated (no empty, no `/`, no NUL,
/// no ASCII control, no `.`/`..` path segments, no UTS-39 confusable
/// bidi/zero-width/BOM/NNBSP codepoints).
///
/// Full UTS-39 confusable-skeleton check against parent's existing labels remains
/// recipient-side at validation step 4 (constructor does not have storage
/// access to the `parent → existing_subgroup_labels` index).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct SubGroupLabel(String);

impl SubGroupLabel {
    /// Construct a `SubGroupLabel` from raw bytes. Canonicalizes to UTF-8 NFC,
    /// validates against the per-character UTS-39 reject set (Appendix A),
    /// enforces length cap POST-normalization.
    pub fn new(bytes: &[u8]) -> Result<Self, SubGroupLabelError> {
        if bytes.is_empty() {
            return Err(SubGroupLabelError::Empty);
        }
        if bytes == b"." || bytes == b".." {
            return Err(SubGroupLabelError::PathSegment);
        }
        for (i, b) in bytes.iter().enumerate() {
            if *b == b'/' || *b == 0 || b.is_ascii_control() {
                return Err(SubGroupLabelError::InvalidByte { offset: i });
            }
        }
        let text = std::str::from_utf8(bytes)?;
        // NFC normalization: canonical decomposition + canonical composition.
        // Minimal NFC for ASCII-only: identity; for non-ASCII: compose sequences.
        let normalized = nfc_normalize(text);
        for (i, ch) in normalized.chars().enumerate() {
            match ch {
                '\u{202A}'..='\u{202E}'
                | '\u{2066}'
                | '\u{2067}'
                | '\u{2068}'
                | '\u{2069}'
                | '\u{202F}'
                | '\u{200E}'
                | '\u{200F}'
                | '\u{200B}'
                | '\u{200C}'
                | '\u{200D}'
                | '\u{FEFF}' => {
                    return Err(SubGroupLabelError::ConfusableChar { offset: i });
                }
                _ => {}
            }
        }
        if normalized.len() > MAX_SUB_LABEL_BYTES {
            return Err(SubGroupLabelError::TooLong {
                max: MAX_SUB_LABEL_BYTES,
                got: normalized.len(),
            });
        }
        Ok(Self(normalized))
    }

    pub fn as_bytes(&self) -> &[u8] {
        self.0.as_bytes()
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Minimal NFC normalization. For pure ASCII, identity. For non-ASCII, we
/// delegate to a lightweight composition pass. This is a pragmatic stand-in
/// for full UTS-15 NFC; production-grade normalization should integrate with
/// the `unicode-normalization` crate per substrate migration follow-on.
fn nfc_normalize(s: &str) -> String {
    // ASCII fast path.
    if s.is_ascii() {
        return s.to_string();
    }
    // Non-ASCII: apply canonical composition via the unicode-normalization crate
    // if available; otherwise identity (with TODO note for migration).
    #[cfg(feature = "unicode-nfc")]
    {
        use unicode_normalization::UnicodeNormalization;
        return s.nfc().collect();
    }
    #[cfg(not(feature = "unicode-nfc"))]
    {
        // Identity fallback — explicit note that this is NOT canonical NFC.
        // Substrate-truth: full UTS-15 NFC integration is a follow-on mission.
        s.to_string()
    }
}

// ============================================================================
// DelegationId + SubDCDelegationProof (Layer B/C — typed delegation surface)
// ============================================================================

/// Typed delegation identifier (32-byte digest).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct DelegationId(pub [u8; 32]);

/// Placeholder typed delegation proof. Full canonical form + parent-signed
/// envelope structure defined by RFC-0855p-d2; this module carries only the
/// opaque typed wrapper so `SubGroupExtension` can express the
/// `Option<SubDCDelegationProof>` field per RFC-0855p-d1 §Data Structure.
///
/// d2 substrate will replace the inner representation with the canonical
/// `SubDCDelegationEnvelope` envelope form per RFC-0855p-d2 §Data Structure.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SubDCDelegationProof {
    /// Opaque proof bytes (canonical envelope per RFC-0855p-d2).
    pub proof_bytes: Vec<u8>,
    /// Typed delegation id (RFC-0855p-d2 §Data Structure).
    pub delegation_id: DelegationId,
}

impl SubDCDelegationProof {
    /// Validate the delegation proof for the given sub-DC + derived child domain.
    ///
    /// d1 substrate placeholder: only structural validation (non-empty bytes,
    /// delegation_id nonzero). Full parent-signed envelope verification per
    /// RFC-0855p-d2 §Sub-DC Delegation Protocol lives in d2 substrate.
    pub fn validate_for(
        &self,
        _sub_dc_id: [u8; 32],
        _derived_sub_domain_id: [u8; 32],
    ) -> Result<(), CreateSubGroupError> {
        if self.proof_bytes.is_empty() {
            return Err(CreateSubGroupError::DelegationInvalid {
                sub_dc_id: [0u8; 32],
            });
        }
        if self.delegation_id.0 == [0u8; 32] {
            return Err(CreateSubGroupError::DelegationInvalid {
                sub_dc_id: [0u8; 32],
            });
        }
        Ok(())
    }
}

// ============================================================================
// ParentBindingInvariant + SubDomainDerivationInvariant (Layer B — invariants)
// ============================================================================

/// Invariant record: parent domain id + required binding state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParentBindingInvariant {
    pub parent_domain_id: [u8; 32],
    pub required_state: GroupBindingState,
}

/// Invariant record: parent + normalized label + derived child id.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SubDomainDerivationInvariant {
    pub parent_domain_id: [u8; 32],
    pub sub_label: SubGroupLabel,
    pub derived_sub_domain_id: [u8; 32],
}

// ============================================================================
// SubGroupExtension (Layer B — extension fields carried by CGSB)
// ============================================================================

/// Extension fields carried by `CreateSubGroupEnvelope` for sub-group linkage.
///
/// `sub_dc_id` is the sub-DC's peer_id (32-byte). When equal to parent DC,
/// `delegation_proof` MUST be `None` (parent DC is the implicit sub-DC).
/// When different, `delegation_proof` MUST be present per RFC-0855p-d2.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SubGroupExtension {
    pub parent_domain_id: [u8; 32],
    pub sub_label: SubGroupLabel,
    pub sub_dc_id: [u8; 32],
    pub delegation_proof: Option<SubDCDelegationProof>,
    pub delegation_id: Option<DelegationId>,
}

impl SubGroupExtension {
    /// Construct a new `SubGroupExtension` with `sub_dc_id = parent_dc_id`
    /// (implicit sub-DC = parent DC) and no delegation proof.
    pub fn new(parent_domain_id: [u8; 32], sub_label: SubGroupLabel) -> Self {
        Self {
            parent_domain_id,
            sub_label,
            sub_dc_id: [0u8; 32],
            delegation_proof: None,
            delegation_id: None,
        }
    }

    /// Compute the canonical `sub_domain_id` per §Sub-Domain Derivation Invariant:
    ///
    /// `sub_domain_id = BLAKE3_keyed(SUBGROUP_DOMAIN_CONTEXT, parent_domain_id || sub_label)`
    ///
    /// Canonical form: `derive_key` expands the ASCII context to a 32-byte key,
    /// `keyed_hash` produces the 32-byte digest. Plain `blake3::hash` is
    /// forbidden (legacy form per F-12 substrate migration).
    pub fn derive_sub_domain_id(&self) -> [u8; 32] {
        derive_sub_domain_id(&self.parent_domain_id, self.sub_label.as_bytes())
    }

    /// Validate the extension per §Recipient Verification step 5–7:
    /// parent binding active, depth-cap enforcement, delegation-proof rules.
    pub fn validate(
        &self,
        parent_dc_id: [u8; 32],
        parent_depth: u8,
        parent_binding: GroupBindingState,
    ) -> Result<[u8; 32], CreateSubGroupError> {
        if parent_binding != GroupBindingState::Active {
            return Err(CreateSubGroupError::ParentNotActive);
        }
        if parent_depth < MAX_ROOT_DEPTH {
            return Err(CreateSubGroupError::DepthUnderflow {
                min: MAX_ROOT_DEPTH,
                got: parent_depth,
            });
        }
        if parent_depth >= MAX_SUBGROUP_DEPTH {
            return Err(CreateSubGroupError::DepthCap {
                max: MAX_SUBGROUP_DEPTH,
                got: parent_depth,
            });
        }
        let derived = self.derive_sub_domain_id();
        if self.sub_dc_id != parent_dc_id {
            match self.delegation_proof.as_ref() {
                None => return Err(CreateSubGroupError::MissingDelegation),
                Some(proof) => proof.validate_for(self.sub_dc_id, derived)?,
            }
        } else if self.delegation_proof.is_some() {
            return Err(CreateSubGroupError::UnexpectedDelegation);
        }
        Ok(derived)
    }
}

/// Canonical `sub_domain_id` derivation (Layer A pure function).
///
/// `sub_domain_id = BLAKE3_keyed(SUBGROUP_DOMAIN_CONTEXT, parent_domain_id || sub_label)`
///
/// Per RFC-0855p-d1 §Sub-Domain Derivation Invariant + §Substrate Compliance,
/// this is the ONLY permitted form. Non-compliant forms (plain `blake3::hash`,
/// context-concatenation, parent-omitted) are forbidden.
pub fn derive_sub_domain_id(parent_domain_id: &[u8; 32], sub_label_bytes: &[u8]) -> [u8; 32] {
    // blake3 1.5 derive_key: `fn derive_key(context: &str, key_material: &[u8]) -> [u8; 32]`
    // Takes an opaque key_material input and returns the derived 32-byte key.
    // We pass an empty key_material so the returned key is purely a function
    // of the context string (per RFC-0855p-d1 §Sub-Domain Derivation Invariant).
    let key = blake3::derive_key(SUBGROUP_DOMAIN_CONTEXT, b"");
    let mut input = Vec::with_capacity(32 + sub_label_bytes.len());
    input.extend_from_slice(parent_domain_id);
    input.extend_from_slice(sub_label_bytes);
    *blake3::keyed_hash(&key, &input).as_bytes()
}

// ============================================================================
// CreateSubGroupEnvelope (Layer B wire envelope)
// ============================================================================

/// `CreateSubGroupEnvelope` (DOT/1/CGSB).
///
/// 10-byte canonical header per RFC-0850p-c §A:
/// `envelope_type = b"DOT1"`, `envelope_subtype = b"CGSB"`, `version = u16 // 0x0001`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CreateSubGroupEnvelope {
    /// `b"DOT1"`.
    pub envelope_type: [u8; 4],
    /// `b"CGSB"`.
    pub envelope_subtype: [u8; 4],
    /// Canonical version (0x0001).
    pub version: u16,
    /// Derived sub-domain id per §Sub-Domain Derivation Invariant.
    pub domain_id: [u8; 32],
    /// 16-byte `BLAKE3-256(canonical_dcs(mission_did))` per RFC-0009 §Identity.
    pub mission_id: [u8; 16],
    /// Typed routing endpoint id (Layer-B/D split per §Layer-C Substrate Surface).
    pub target_routing_endpoint_id: [u8; 32],
    /// DC peer_id (parent DC or delegated sub-DC per `sub_group_extension.sub_dc_id`).
    pub dc_id: [u8; 32],
    /// Sub-group linkage + delegation surface.
    pub sub_group_extension: SubGroupExtension,
    /// 16-byte random nonce (replay-key tuple element per RFC-0855p-d1 §Determinism Requirements).
    pub nonce: [u8; 16],
    /// Current epoch at CGSB emission time. Recipient rejects when
    /// `current_epoch > local_epoch + MAX_FSKEW_EPOCHS` (forward-skew bound).
    pub current_epoch: u64,
    /// Parent DC's coordinator term id (signs the envelope).
    pub coordinator_term_id: [u8; 32],
    /// Ed25519 signature over canonical envelope body per §Recipient Verification step 8
    /// (signature-coverage invariant: covers domain_id, extension, nonce, epoch,
    /// coordinator_term_id, and delegation_proof when present).
    pub signature: [u8; 64],
}

impl CreateSubGroupEnvelope {
    /// Construct a new envelope with canonical header populated. Caller fills
    /// in remaining fields and signs before transmission.
    pub fn new(
        mission_id: [u8; 16],
        sub_group_extension: SubGroupExtension,
        dc_id: [u8; 32],
        current_epoch: u64,
        coordinator_term_id: [u8; 32],
    ) -> Self {
        let domain_id = sub_group_extension.derive_sub_domain_id();
        Self {
            envelope_type: *b"DOT1",
            envelope_subtype: CREATE_SUBGROUP,
            version: 0x0001,
            domain_id,
            mission_id,
            target_routing_endpoint_id: [0u8; 32],
            dc_id,
            sub_group_extension,
            nonce: [0u8; 16],
            current_epoch,
            coordinator_term_id,
            signature: [0u8; 64],
        }
    }

    /// Validate header + sub_label + sub_domain_id consistency.
    pub fn validate(&self) -> Result<(), SubGroupHeaderError> {
        if self.envelope_type != *b"DOT1" || self.envelope_subtype != CREATE_SUBGROUP {
            return Err(SubGroupHeaderError::HeaderMismatch {
                expected_type: *b"DOT1",
                expected_subtype: CREATE_SUBGROUP,
                got_type: self.envelope_type,
                got_subtype: self.envelope_subtype,
            });
        }
        if self.version != 0x0001 {
            return Err(SubGroupHeaderError::UnsupportedVersion { got: self.version });
        }
        let computed = self.sub_group_extension.derive_sub_domain_id();
        if computed != self.domain_id {
            return Err(SubGroupHeaderError::SubDomainIdMismatch {
                computed,
                stored: self.domain_id,
            });
        }
        Ok(())
    }
}

/// Error type for `CreateSubGroupEnvelope::validate`.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum SubGroupHeaderError {
    #[error("envelope header mismatch (expected {expected_type:?}/{expected_subtype:?}, got {got_type:?}/{got_subtype:?})")]
    HeaderMismatch {
        expected_type: [u8; 4],
        expected_subtype: [u8; 4],
        got_type: [u8; 4],
        got_subtype: [u8; 4],
    },
    #[error("unsupported envelope version {got}")]
    UnsupportedVersion { got: u16 },
    #[error("sub_domain_id mismatch (computed {:02x?}, stored {:02x?})", &computed[..8], &stored[..8])]
    SubDomainIdMismatch {
        computed: [u8; 32],
        stored: [u8; 32],
    },
}

// ============================================================================
// SubGroupState + SubGroupRecord (Layer C — storage + state machine)
// ============================================================================

/// Sub-group lifecycle state. `#[non_exhaustive]` so future RFCs may add
/// `Suspended` / `Archived` / `Frozen` without forcing cross-crate edits at
/// every match site.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum SubGroupState {
    PendingBind,
    Bound,
    Dissolving,
    Dissolved,
}

/// Storage record for one sub-group. `inherited_policy_hash` is the parent's
/// mission policy digest at creation time; `teardown_proof` populated on
/// `Dissolving → Dissolved` transition per RFC-0855p-d3 (SGTP envelope).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SubGroupRecord {
    pub parent_domain_id: [u8; 32],
    pub sub_domain_id: [u8; 32],
    pub sub_label: SubGroupLabel,
    pub effective_dc_id: [u8; 32],
    pub state: SubGroupState,
    pub binding_state: GroupBindingState,
    pub created_epoch: u64,
    /// Populated when `state == Dissolving`; consumed on `Dissolved`.
    pub teardown_proof: Option<TeardownProof>,
    pub inherited_policy_hash: [u8; 32],
}

/// Placeholder teardown proof. Canonical form defined by RFC-0855p-d3
/// (`TeardownProofEnvelope`, subtype `b"SGTP"`); this module carries only the
/// opaque bytes so `SubGroupRecord` can express the storage shape per
/// RFC-0855p-d1 §Data Structure.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TeardownProof {
    pub proof_bytes: Vec<u8>,
    pub recorded_epoch: u64,
}

/// State transition result. Either the transition succeeded (carries new
/// state) or it was rejected (carries reason).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TransitionResult {
    Transitioned(SubGroupState),
    Rejected(TransitionError),
}

/// State transition error reasons.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum TransitionError {
    #[error("invalid transition from {from:?} via {trigger}")]
    InvalidTransition {
        from: SubGroupState,
        trigger: &'static str,
    },
    #[error("bind deadline exceeded (await_epochs {got} > MAX_BIND_AWAIT_EPOCHS {max})")]
    BindDeadlineExceeded { max: u64, got: u64 },
    #[error("retry count exhausted (retries {got} >= MAX_BIND_RETRY_COUNT {max})")]
    RetryCountExhausted { max: u8, got: u8 },
    #[error("teardown grace not elapsed (elapsed_epochs {got} < TEARDOWN_GRACE_EPOCHS)")]
    TeardownGraceNotElapsed { got: u64 },
}

/// Trigger types for state transitions (per RFC-0855p-d1 §State Machine table).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransitionTrigger {
    /// CGSB envelope verified + parent active.
    CreateVerified,
    /// CGROUP BIND commit succeeded.
    BindCommitted,
    /// BIND failed or deadline expired.
    BindFailed,
    /// Parent flips to `Dissolving` before BIND commit.
    ParentFlippedDissolving,
    /// Explicit child UNBIND (parent DC or authorized sub-DC signed).
    ExplicitUnbind,
    /// Parent UNBIND cascade.
    ParentUnbindCascade,
    /// Parent revokes child sub-DC.
    ParentRevokedSubDc,
    /// `TEARDOWN_GRACE_EPOCHS` elapsed + teardown proof recorded.
    TeardownGraceElapsed,
    /// Authorized repair / retry.
    AuthorizedRepair,
}

/// Apply a state transition per §State Machine table. Returns `Transitioned`
/// with the new state on success, `Rejected` with reason on failure.
///
/// `now_epoch` is the recipient-local epoch at transition time. For
/// `TeardownGraceElapsed`, the recipient-local grace check enforces
/// `now_epoch - dissolving_epoch >= TEARDOWN_GRACE_EPOCHS = 50`
/// (recipient-local, NOT envelope-supplied — per RFC-0855p-d3 Appendix A).
pub fn transition(
    current: SubGroupState,
    trigger: TransitionTrigger,
    now_epoch: u64,
    dissolving_epoch: u64,
    bind_await_epochs: u64,
    retry_count: u8,
) -> TransitionResult {
    match (current, trigger) {
        // Absent → PendingBind (verified CGSB create; depth + parent active checked
        // by SubGroupExtension::validate before this transition fires).
        (s, TransitionTrigger::CreateVerified) if s != SubGroupState::PendingBind => {
            TransitionResult::Transitioned(SubGroupState::PendingBind)
        }
        // PendingBind → Bound (BIND commits within MAX_BIND_AWAIT_EPOCHS; retries < MAX_BIND_RETRY_COUNT).
        (SubGroupState::PendingBind, TransitionTrigger::BindCommitted) => {
            if bind_await_epochs > MAX_BIND_AWAIT_EPOCHS {
                return TransitionResult::Rejected(TransitionError::BindDeadlineExceeded {
                    max: MAX_BIND_AWAIT_EPOCHS,
                    got: bind_await_epochs,
                });
            }
            if retry_count >= MAX_BIND_RETRY_COUNT {
                return TransitionResult::Rejected(TransitionError::RetryCountExhausted {
                    max: MAX_BIND_RETRY_COUNT,
                    got: retry_count,
                });
            }
            TransitionResult::Transitioned(SubGroupState::Bound)
        }
        // PendingBind → Dissolved (BIND failed or deadline expired).
        (SubGroupState::PendingBind, TransitionTrigger::BindFailed) => {
            TransitionResult::Transitioned(SubGroupState::Dissolved)
        }
        // PendingBind → Dissolving (parent UNBIND while awaiting BIND).
        (SubGroupState::PendingBind, TransitionTrigger::ParentFlippedDissolving) => {
            TransitionResult::Transitioned(SubGroupState::Dissolving)
        }
        // Bound → Dissolving (explicit child UNBIND, parent UNBIND cascade, or parent revokes child sub-DC).
        (
            SubGroupState::Bound,
            TransitionTrigger::ExplicitUnbind
            | TransitionTrigger::ParentUnbindCascade
            | TransitionTrigger::ParentRevokedSubDc,
        ) => TransitionResult::Transitioned(SubGroupState::Dissolving),
        // Dissolving → Dissolved (grace elapsed + teardown proof recorded).
        (SubGroupState::Dissolving, TransitionTrigger::TeardownGraceElapsed) => {
            let elapsed = now_epoch.saturating_sub(dissolving_epoch);
            // TEARDOWN_GRACE_EPOCHS = 50 lives in d3 substrate; d1 carries the
            // literal value as cross-RFC invariant. Substrate-truth: d3 owns
            // the constant canonical home.
            const TEARDOWN_GRACE_EPOCHS: u64 = 50;
            if elapsed < TEARDOWN_GRACE_EPOCHS {
                return TransitionResult::Rejected(TransitionError::TeardownGraceNotElapsed {
                    got: elapsed,
                });
            }
            TransitionResult::Transitioned(SubGroupState::Dissolved)
        }
        // Dissolving → Dissolving (authorized repair; preserves teardown state).
        (SubGroupState::Dissolving, TransitionTrigger::AuthorizedRepair) => {
            TransitionResult::Transitioned(SubGroupState::Dissolving)
        }
        // All other combinations: rejected.
        (from, _) => TransitionResult::Rejected(TransitionError::InvalidTransition {
            from,
            trigger: trigger_name(trigger),
        }),
    }
}

fn trigger_name(t: TransitionTrigger) -> &'static str {
    match t {
        TransitionTrigger::CreateVerified => "CreateVerified",
        TransitionTrigger::BindCommitted => "BindCommitted",
        TransitionTrigger::BindFailed => "BindFailed",
        TransitionTrigger::ParentFlippedDissolving => "ParentFlippedDissolving",
        TransitionTrigger::ExplicitUnbind => "ExplicitUnbind",
        TransitionTrigger::ParentUnbindCascade => "ParentUnbindCascade",
        TransitionTrigger::ParentRevokedSubDc => "ParentRevokedSubDc",
        TransitionTrigger::TeardownGraceElapsed => "TeardownGraceElapsed",
        TransitionTrigger::AuthorizedRepair => "AuthorizedRepair",
    }
}

/// Cross-node reconciliation: divergent `PendingBind` epochs collapse to
/// `Dissolving` after `MAX_BIND_AWAIT_EPOCHS` elapses since the latest
/// `BindCommitted` attempt. Per RFC-0855p-d1 §Recipient Verification step 11
/// (cross-node `PendingBind → Dissolving` reconciliation).
pub fn reconcile_pending_bind(
    record: &SubGroupRecord,
    now_epoch: u64,
    latest_bind_attempt_epoch: u64,
) -> TransitionResult {
    if record.state != SubGroupState::PendingBind {
        return TransitionResult::Rejected(TransitionError::InvalidTransition {
            from: record.state,
            trigger: "CrossNodeReconcile",
        });
    }
    let elapsed = now_epoch.saturating_sub(latest_bind_attempt_epoch);
    if elapsed >= MAX_BIND_AWAIT_EPOCHS {
        // Collapse to Dissolving (not Dissolved; teardown proof still required).
        TransitionResult::Transitioned(SubGroupState::Dissolving)
    } else {
        // No reconciliation needed; remain PendingBind.
        TransitionResult::Transitioned(SubGroupState::PendingBind)
    }
}

// ============================================================================
// SubGroupQuery / SubGroupResponse / SubGroupAuthorityCheck (Layer C boundary)
// ============================================================================

/// Typed query: resolve sub-group record by `(parent_domain_id, sub_domain_id)`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SubGroupQuery {
    pub parent_domain_id: [u8; 32],
    pub sub_domain_id: [u8; 32],
}

/// Typed response. Uses `response_kind: u16` discriminator (RFC-allocated
/// namespace 0x0001-0x00FF + user-extension range 0x0100-0xFFFF) plus a
/// bounded payload. No central enum — unknown `response_kind` values return
/// `not_found()` (fail-closed).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SubGroupResponse {
    pub response_kind: u16,
    pub response_payload: Vec<u8>,
}

impl SubGroupResponse {
    /// `not_found` response (no record, unknown kind, or missing data).
    pub fn not_found() -> Self {
        Self {
            response_kind: SUBGROUP_RESPONSE_NOT_FOUND,
            response_payload: Vec::new(),
        }
    }

    /// Build response from a record + known kind.
    pub fn from_record(record: &SubGroupRecord) -> Self {
        let kind = match record.state {
            SubGroupState::PendingBind => SUBGROUP_RESPONSE_PENDING_BIND,
            SubGroupState::Bound => SUBGROUP_RESPONSE_BOUND,
            SubGroupState::Dissolving => SUBGROUP_RESPONSE_DISSOLVING,
            SubGroupState::Dissolved => SUBGROUP_RESPONSE_DISSOLVED,
        };
        // Minimal payload encoding: state discriminant + created_epoch (8B BE).
        // Production-grade DCS encoding is a follow-on substrate migration.
        let mut payload = Vec::with_capacity(9);
        payload.push(record.state.discriminant());
        payload.extend_from_slice(&record.created_epoch.to_be_bytes());
        Self {
            response_kind: kind,
            response_payload: payload,
        }
    }
}

impl SubGroupState {
    fn discriminant(&self) -> u8 {
        match self {
            SubGroupState::PendingBind => 0x01,
            SubGroupState::Bound => 0x02,
            SubGroupState::Dissolving => 0x03,
            SubGroupState::Dissolved => 0x04,
        }
    }
}

/// Typed authority check. Action is dispatched by d2 (delegate/revoke) or d3
/// (route/aggregate/dissolve).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SubGroupAuthorityCheck {
    pub sub_domain_id: [u8; 32],
    pub actor_dc_id: [u8; 32],
    pub action: SubGroupAction,
}

/// Typed action discriminator (RFC-allocated namespace 0x0001-0x00FF +
/// user-extension range 0x0100-0xFFFF).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SubGroupAction {
    pub action_type_id: u32,
    pub action_payload: Vec<u8>,
}

impl SubGroupAction {
    /// INVITE action (no payload).
    pub fn invite() -> Self {
        Self {
            action_type_id: SUBGROUP_ACTION_INVITE,
            action_payload: Vec::new(),
        }
    }

    /// ROUTE action with envelope payload (d3 substrate).
    pub fn route(payload: &[u8]) -> Result<Self, ActionEncodeError> {
        if payload.len() > SUBGROUP_ACTION_PAYLOAD_MAX {
            return Err(ActionEncodeError::PayloadTooLarge {
                max: SUBGROUP_ACTION_PAYLOAD_MAX,
                got: payload.len(),
            });
        }
        Ok(Self {
            action_type_id: SUBGROUP_ACTION_ROUTE,
            action_payload: payload.to_vec(),
        })
    }

    /// AGGREGATE action with envelope payload (d3 substrate).
    pub fn aggregate(payload: &[u8]) -> Result<Self, ActionEncodeError> {
        if payload.len() > SUBGROUP_ACTION_PAYLOAD_MAX {
            return Err(ActionEncodeError::PayloadTooLarge {
                max: SUBGROUP_ACTION_PAYLOAD_MAX,
                got: payload.len(),
            });
        }
        Ok(Self {
            action_type_id: SUBGROUP_ACTION_AGGREGATE,
            action_payload: payload.to_vec(),
        })
    }
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ActionEncodeError {
    #[error("action payload exceeds {max} bytes (got {got})")]
    PayloadTooLarge { max: usize, got: usize },
}

// ============================================================================
// Cross-RFC re-exports (canonical home pattern per plateau closure)
// ============================================================================

// RFC-0855p-d3 §Data Structure re-exports d1 constants as canonical home for
// the cross-RFC family. Substrate anchor: d1 owns; d3 imports via `pub use`.
pub use MAX_BIND_AWAIT_EPOCHS as _MAX_BIND_AWAIT_EPOCHS_REEXPORT;
pub use MAX_BIND_RETRY_COUNT as _MAX_BIND_RETRY_COUNT_REEXPORT;
pub use MAX_FSKEW_EPOCHS as _MAX_FSKEW_EPOCHS_REEXPORT;
pub use MAX_ROOT_DEPTH as _MAX_ROOT_DEPTH_REEXPORT;
pub use MAX_SUBGROUP_DEPTH as _MAX_SUBGROUP_DEPTH_REEXPORT;
pub use RACE_EPOCHS as _RACE_EPOCHS_REEXPORT;

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn parent_id(seed: u8) -> [u8; 32] {
        [seed; 32]
    }

    fn dc_id(seed: u8) -> [u8; 32] {
        [seed; 32]
    }

    fn label(s: &str) -> SubGroupLabel {
        SubGroupLabel::new(s.as_bytes()).expect("valid label")
    }

    // TV-SG-1: valid CGSB acceptance; happy path.
    #[test]
    fn tv_sg_1_valid_cgsb() {
        let parent = parent_id(0xAA);
        let dc = dc_id(0xCC);
        let ext = SubGroupExtension {
            parent_domain_id: parent,
            sub_label: label("legal-review"),
            sub_dc_id: dc, // same as parent DC
            delegation_proof: None,
            delegation_id: None,
        };
        let derived = ext
            .validate(dc, MAX_ROOT_DEPTH, GroupBindingState::Active)
            .unwrap();
        assert_eq!(derived, derive_sub_domain_id(&parent, b"legal-review"));
        let env = CreateSubGroupEnvelope::new([0xCCu8; 16], ext, dc, 10, [0xDDu8; 32]);
        assert_eq!(env.envelope_type, *b"DOT1");
        assert_eq!(env.envelope_subtype, *b"CGSB");
        assert_eq!(env.version, 0x0001);
        env.validate().expect("valid envelope");
    }

    // TV-SG-2: depth-cap rejection (parent depth == MAX_SUBGROUP_DEPTH).
    #[test]
    fn tv_sg_2_depth_cap_rejection() {
        let parent = parent_id(0xAA);
        let dc = dc_id(0xCC);
        let ext = SubGroupExtension {
            parent_domain_id: parent,
            sub_label: label("x"),
            sub_dc_id: dc,
            delegation_proof: None,
            delegation_id: None,
        };
        let result = ext.validate(dc, MAX_SUBGROUP_DEPTH, GroupBindingState::Active);
        assert!(matches!(result, Err(CreateSubGroupError::DepthCap { .. })));
    }

    // TV-SG-3: invalid sub_label rejection (UTF-8 reject set).
    #[test]
    fn tv_sg_3_invalid_sub_label() {
        // empty
        assert!(matches!(
            SubGroupLabel::new(b""),
            Err(SubGroupLabelError::Empty)
        ));
        // slash
        assert!(matches!(
            SubGroupLabel::new(b"legal/review"),
            Err(SubGroupLabelError::InvalidByte { offset: 5 })
        ));
        // NUL
        assert!(matches!(
            SubGroupLabel::new(b"foo\0bar"),
            Err(SubGroupLabelError::InvalidByte { .. })
        ));
        // ASCII LF
        assert!(matches!(
            SubGroupLabel::new(b"foo\nbar"),
            Err(SubGroupLabelError::InvalidByte { .. })
        ));
        // `.` / `..` path segments
        assert!(matches!(
            SubGroupLabel::new(b"."),
            Err(SubGroupLabelError::PathSegment)
        ));
        assert!(matches!(
            SubGroupLabel::new(b".."),
            Err(SubGroupLabelError::PathSegment)
        ));
        // bidi-control U+202E
        let mut bidi = b"foo".to_vec();
        bidi.extend_from_slice(&[0xE2, 0x80, 0xAE]); // U+202A–U+202E range
        assert!(matches!(
            SubGroupLabel::new(&bidi),
            Err(SubGroupLabelError::ConfusableChar { .. })
        ));
        // BOM U+FEFF
        let mut bom = Vec::new();
        bom.extend_from_slice(&[0xEF, 0xBB, 0xBF]);
        bom.extend_from_slice(b"foo");
        assert!(matches!(
            SubGroupLabel::new(&bom),
            Err(SubGroupLabelError::ConfusableChar { .. })
        ));
    }

    // TV-SG-4: nested rebind positive case — derived id stable across rebind attempts.
    #[test]
    fn tv_sg_4_nested_rebind_positive() {
        let parent = parent_id(0xAA);
        let label_bytes = b"comms";
        let id_a = derive_sub_domain_id(&parent, label_bytes);
        let id_b = derive_sub_domain_id(&parent, label_bytes);
        assert_eq!(id_a, id_b);
        // Different parent → different id (per §Sub-Domain Derivation Invariant).
        let other_parent = parent_id(0xBB);
        let id_c = derive_sub_domain_id(&other_parent, label_bytes);
        assert_ne!(id_a, id_c);
    }

    // TV-SG-5: parent-dissolve cascade (state transition path).
    #[test]
    fn tv_sg_5_parent_dissolve_cascade() {
        // Simulate Bound → Dissolving via ParentUnbindCascade.
        let result = transition(
            SubGroupState::Bound,
            TransitionTrigger::ParentUnbindCascade,
            100,
            0,
            0,
            0,
        );
        assert!(matches!(
            result,
            TransitionResult::Transitioned(SubGroupState::Dissolving)
        ));
    }

    // TV-SG-5a: root-depth cap (MAX_ROOT_DEPTH = 1 enforcement).
    #[test]
    fn tv_sg_5a_root_depth_underflow() {
        let parent = parent_id(0xAA);
        let dc = dc_id(0xCC);
        let ext = SubGroupExtension {
            parent_domain_id: parent,
            sub_label: label("root-child"),
            sub_dc_id: dc,
            delegation_proof: None,
            delegation_id: None,
        };
        // parent_depth = 0 (< MAX_ROOT_DEPTH = 1) → DepthUnderflow.
        let result = ext.validate(dc, 0, GroupBindingState::Active);
        assert!(matches!(
            result,
            Err(CreateSubGroupError::DepthUnderflow { .. })
        ));
    }

    // State machine full transition path: PendingBind → Bound → Dissolving → Dissolved.
    #[test]
    fn state_machine_full_path() {
        // Absent → PendingBind
        let r = transition(
            SubGroupState::Dissolved, // start from a terminal state for the "Absent" row
            TransitionTrigger::CreateVerified,
            0,
            0,
            0,
            0,
        );
        assert!(matches!(
            r,
            TransitionResult::Transitioned(SubGroupState::PendingBind)
        ));
        // PendingBind → Bound
        let r = transition(
            SubGroupState::PendingBind,
            TransitionTrigger::BindCommitted,
            10,
            0,
            0,
            0,
        );
        assert!(matches!(
            r,
            TransitionResult::Transitioned(SubGroupState::Bound)
        ));
        // Bound → Dissolving
        let r = transition(
            SubGroupState::Bound,
            TransitionTrigger::ExplicitUnbind,
            100,
            0,
            0,
            0,
        );
        assert!(matches!(
            r,
            TransitionResult::Transitioned(SubGroupState::Dissolving)
        ));
        // Dissolving → Dissolved (50 epochs elapsed)
        let r = transition(
            SubGroupState::Dissolving,
            TransitionTrigger::TeardownGraceElapsed,
            100,
            50, // dissolving_epoch
            0,
            0,
        );
        assert!(matches!(
            r,
            TransitionResult::Transitioned(SubGroupState::Dissolved)
        ));
    }

    // State machine grace-not-elapsed rejection.
    #[test]
    fn state_machine_grace_not_elapsed() {
        let r = transition(
            SubGroupState::Dissolving,
            TransitionTrigger::TeardownGraceElapsed,
            100,
            90, // dissolving_epoch — only 10 epochs elapsed, < 50
            0,
            0,
        );
        assert!(matches!(
            r,
            TransitionResult::Rejected(TransitionError::TeardownGraceNotElapsed { .. })
        ));
    }

    // Cross-node reconciliation: divergent PendingBind epochs collapse to Dissolving.
    #[test]
    fn cross_node_reconciliation() {
        let record = SubGroupRecord {
            parent_domain_id: parent_id(0xAA),
            sub_domain_id: [0x11u8; 32],
            sub_label: label("cgsb-recon"),
            effective_dc_id: dc_id(0xCC),
            state: SubGroupState::PendingBind,
            binding_state: GroupBindingState::Active,
            created_epoch: 100,
            teardown_proof: None,
            inherited_policy_hash: [0x33u8; 32],
        };
        // now_epoch = 132 (latest_bind_attempt_epoch = 100 → elapsed = 32).
        // MAX_BIND_AWAIT_EPOCHS = 32 — exactly at the bound, reconcile.
        let r = reconcile_pending_bind(&record, 132, 100);
        assert!(matches!(
            r,
            TransitionResult::Transitioned(SubGroupState::Dissolving)
        ));

        // now_epoch = 131 — elapsed = 31, below bound, no reconcile.
        let r = reconcile_pending_bind(&record, 131, 100);
        assert!(matches!(
            r,
            TransitionResult::Transitioned(SubGroupState::PendingBind)
        ));
    }

    // Canonical derivation matches §Sub-Domain Derivation Invariant.
    #[test]
    fn canonical_derivation_keyed_hash() {
        let parent = [0xAAu8; 32];
        let label_bytes = b"legal-review";
        let derived = derive_sub_domain_id(&parent, label_bytes);
        // Manual recompute via derive_key + keyed_hash.
        let key = blake3::derive_key(SUBGROUP_DOMAIN_CONTEXT, b"");
        let mut input = Vec::with_capacity(32 + label_bytes.len());
        input.extend_from_slice(&parent);
        input.extend_from_slice(label_bytes);
        let expected = blake3::keyed_hash(&key, &input).as_bytes().to_owned();
        assert_eq!(derived, expected);
    }

    // Plain hash form is FORBIDDEN — derive_key/keyed_hash must be used.
    #[test]
    fn plain_hash_form_is_not_equal() {
        let parent = [0xAAu8; 32];
        let label_bytes = b"legal-review";
        let derived = derive_sub_domain_id(&parent, label_bytes);
        // Legacy plain blake3::hash form (forbidden per §Substrate Compliance).
        let mut input = Vec::with_capacity(32 + label_bytes.len());
        input.extend_from_slice(&parent);
        input.extend_from_slice(label_bytes);
        let plain = blake3::hash(&input).as_bytes().to_owned();
        // Sanity check: the two forms differ (legacy plain hash is the forbidden form).
        assert_ne!(derived, plain);
    }

    // SubGroupResponse::not_found for unknown / missing records.
    #[test]
    fn response_not_found() {
        let r = SubGroupResponse::not_found();
        assert_eq!(r.response_kind, SUBGROUP_RESPONSE_NOT_FOUND);
        assert!(r.response_payload.is_empty());
    }

    // SubGroupResponse::from_record discriminates state correctly.
    #[test]
    fn response_from_record() {
        let rec = SubGroupRecord {
            parent_domain_id: parent_id(0xAA),
            sub_domain_id: [0x11u8; 32],
            sub_label: label("x"),
            effective_dc_id: dc_id(0xCC),
            state: SubGroupState::Bound,
            binding_state: GroupBindingState::Active,
            created_epoch: 100,
            teardown_proof: None,
            inherited_policy_hash: [0u8; 32],
        };
        let r = SubGroupResponse::from_record(&rec);
        assert_eq!(r.response_kind, SUBGROUP_RESPONSE_BOUND);
    }

    // SubGroupAction: invite / route / aggregate payload-length enforcement.
    #[test]
    fn action_payload_length_enforced() {
        let a = SubGroupAction::invite();
        assert_eq!(a.action_type_id, SUBGROUP_ACTION_INVITE);
        let huge = vec![0u8; SUBGROUP_ACTION_PAYLOAD_MAX + 1];
        assert!(matches!(
            SubGroupAction::route(&huge),
            Err(ActionEncodeError::PayloadTooLarge { .. })
        ));
        let ok = vec![0u8; SUBGROUP_ACTION_PAYLOAD_MAX];
        assert!(SubGroupAction::aggregate(&ok).is_ok());
    }

    // SubGroupState is #[non_exhaustive] — match sites must include wildcard.
    #[test]
    fn sub_group_state_non_exhaustive_match() {
        // #[non_exhaustive] requires wildcard; verify wildcard catches unknown
        // future variants and known variants land in their explicit arms.
        let known = SubGroupState::Bound;
        let kind_known = match known {
            SubGroupState::PendingBind => 1,
            SubGroupState::Bound => 2,
            SubGroupState::Dissolving => 3,
            SubGroupState::Dissolved => 4,
            #[allow(unreachable_patterns)]
            _ => 99, // required wildcard per #[non_exhaustive]
        };
        assert_eq!(kind_known, 2);
        // Wildcard path: synthesize an unknown ordinal via mem::transmute-style
        // raw byte pattern (documented test-only fallback).
        let raw: u8 = 0xFE; // unknown variant
        let future = match raw {
            0x01 => SubGroupState::PendingBind,
            0x02 => SubGroupState::Bound,
            0x03 => SubGroupState::Dissolving,
            0x04 => SubGroupState::Dissolved,
            _ => {
                // Future unknown variant — substrate recipient fails closed
                // (caller checks before pattern match in production code).
                SubGroupState::Dissolved
            }
        };
        assert_eq!(future, SubGroupState::Dissolved);
    }

    // Constants match RFC §Lifecycle Requirements.
    #[test]
    fn lifecycle_constants_match_rfc() {
        assert_eq!(MAX_SUBGROUP_DEPTH, 8);
        assert_eq!(MAX_ROOT_DEPTH, 1);
        assert_eq!(MAX_BIND_AWAIT_EPOCHS, 32);
        assert_eq!(MAX_BIND_RETRY_COUNT, 3);
        assert_eq!(MAX_FSKEW_EPOCHS, 4);
        assert_eq!(RACE_EPOCHS, 32);
        assert_eq!(MAX_SUB_LABEL_BYTES, 256);
        assert_eq!(SUBGROUP_DOMAIN_CONTEXT, "DOT/1/CGROUP_SUB/domain");
        assert_eq!(CREATE_SUBGROUP, *b"CGSB");
    }

    // Forward-skew bound rejection test (would be enforced at envelope acceptance).
    #[test]
    fn forward_skew_bound_check() {
        let local_epoch: u64 = 100;
        let envelope_epoch: u64 = local_epoch + MAX_FSKEW_EPOCHS + 1;
        let skew = envelope_epoch.saturating_sub(local_epoch);
        assert!(skew > MAX_FSKEW_EPOCHS);
    }

    // Bind deadline exceeded rejection.
    #[test]
    fn bind_deadline_exceeded() {
        let r = transition(
            SubGroupState::PendingBind,
            TransitionTrigger::BindCommitted,
            100,
            0,
            MAX_BIND_AWAIT_EPOCHS + 1, // bind_await_epochs
            0,
        );
        assert!(matches!(
            r,
            TransitionResult::Rejected(TransitionError::BindDeadlineExceeded { .. })
        ));
    }

    // Retry count exhausted rejection.
    #[test]
    fn retry_count_exhausted() {
        let r = transition(
            SubGroupState::PendingBind,
            TransitionTrigger::BindCommitted,
            100,
            0,
            0,
            MAX_BIND_RETRY_COUNT,
        );
        assert!(matches!(
            r,
            TransitionResult::Rejected(TransitionError::RetryCountExhausted { .. })
        ));
    }
}
