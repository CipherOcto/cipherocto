//! Attestation substrate (RFC-0973 §Phase 1).
//!
//! The declaration, the two traits, the registry, and the allocation record.
//! **No strategy crate is shipped here and nothing is verified at dispatch
//! yet** — this is the substrate the later missions build on.
//!
//! # What is deliberately not here
//!
//! No attested capability is minted by this module. A capability carrying an
//! envelope MUST NOT be accepted without that envelope being checked, and the
//! check lives in the dispatch (mission A). Landing the wire form before the
//! check is safe only because mint refuses to attach an envelope; that refusal
//! is the standing invariant this module exists to hold.
//!
//! # Trait split
//!
//! `AttestationStrategy` is required. `AttestationAdmission` is an optional
//! companion. The split is load-bearing rather than cosmetic: it is what
//! separates a strategy that implements nothing — and therefore declares no
//! deployment requirement and no cost — from one that implements the admission
//! trait and answers `None`. Neither trait carries a default, because absence
//! is a claim the strategy must be true about, so it is written rather than
//! inherited.

use core::fmt;
use std::collections::BTreeMap;
use std::sync::Arc;

use octo_cap_macaroon::attestation::{AttestationEnvelope, AttestationTypeId};
use octo_protocol::payload_kind::PayloadKindId;

// Re-exported so a strategy crate names one import for the whole surface.
pub use octo_cap_macaroon::attestation::{
    decode_envelope, encode_envelope, EnvelopeDecodeError, DEFAULT_MAX_ENVELOPE_PAYLOAD,
};

/// Why attestation verification failed.
///
/// Core-owned: `verify` returns it, and a strategy in an extension crate cannot
/// construct an error type it does not own.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum AttestationError {
    /// No strategy is registered for the envelope's type id. Fail-closed: an
    /// unregistered id never falls back to "probably fine".
    StrategyNotFound(AttestationTypeId),
    /// The strategy itself refused, with a reason it chose.
    StrategyRefused {
        /// Strategy-chosen explanation. Never carries attestation bytes.
        reason: String,
    },
    /// The context was not usable, independent of the strategy.
    MalformedContext,
}

impl fmt::Display for AttestationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::StrategyNotFound(id) => write!(f, "no strategy registered for {id}"),
            Self::StrategyRefused { reason } => write!(f, "strategy refused: {reason}"),
            Self::MalformedContext => f.write_str("attestation verification context is malformed"),
        }
    }
}

impl std::error::Error for AttestationError {}

/// What a relation actually costs a node to verify.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CostFigure {
    /// Measured in CASM words.
    Measured(u64),
    /// Not measured yet.
    ///
    /// This is **not** the same as "no cost". An unmeasured cost fails the boot
    /// gate outright, which is what makes the bare figure safe to pass: a node
    /// never starts having admitted a strategy whose cost it does not know.
    Unmeasured,
}

/// Security parameters a strategy claims its relation needs.
///
/// The field is private and there is no bare-integer constructor: the only way
/// in is [`Self::from_prover_parameters`], and the only way out is [`Self::bits`].
/// That is what keeps this the single route to a value, so a caller cannot read
/// it as a struct field and supply it another way.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct SecurityFigure(u64);

impl SecurityFigure {
    /// Sole constructor. Takes named prover parameters, never a bare integer,
    /// so the security claim stays traceable to what produced it.
    #[must_use]
    pub const fn from_prover_parameters(bits: u64) -> Self {
        Self(bits)
    }

    /// Sole accessor.
    #[must_use]
    pub const fn bits(&self) -> u64 {
        self.0
    }
}

impl fmt::Debug for SecurityFigure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SecurityFigure({} bits)", self.0)
    }
}

/// Named inputs to [`SecurityFigure::from_prover_parameters`].
#[derive(Clone, Copy, Debug, Default)]
pub struct SecurityParams {
    /// Claimed security level in bits.
    pub security_bits: u64,
    /// Number of queries the prover budget assumes.
    pub num_queries: u64,
}

/// What admitting one strategy costs a node.
///
/// Returned by [`AttestationAdmission::admission_profile`]. A strategy that
/// carries a proof **must** implement the admission trait, because returning
/// `None` from it is indistinguishable at runtime from "this strategy carries
/// no circuit cost", and that is a false self-description only review catches.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct AdmissionProfile {
    /// Circuit cost of verifying this relation.
    pub casm_cost_hint_words: CostFigure,
    /// Security level the relation achieves.
    pub security_bits: SecurityFigure,
}

impl Default for AdmissionProfile {
    /// A zero-cost profile. This is the value a strategy carries when it answers
    /// `None` to `admission_profile`, i.e. it is claiming no circuit cost.
    ///
    /// It is deliberately **not** [`CostFigure::Unmeasured`]: the two are
    /// different claims, and collapsing them would make an unmeasured proof-
    /// bearing strategy indistinguishable from a genuinely free one.
    fn default() -> Self {
        Self {
            casm_cost_hint_words: CostFigure::Measured(0),
            security_bits: SecurityFigure::from_prover_parameters(0),
        }
    }
}

/// What a node must serve to verify a strategy, and what admitting it costs.
///
/// Built at the dispatch and boot sites. Always present — there is no
/// `Option<Admission>` there, because an absent profile and a permissive
/// strategy must not be the same value.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Admission {
    /// Payload kinds a node must serve to produce a valid attestation.
    pub required_payload_kinds: Vec<PayloadKindId>,
    /// Whether **verification**, as distinct from production, needs a node that
    /// also produces.
    pub verifier_needs_producing_kinds: bool,
    /// The cost profile, always present.
    pub profile: AdmissionProfile,
}

impl Admission {
    /// The permissive value used when a strategy does not implement
    /// [`AttestationAdmission`].
    ///
    /// Empty kinds, no producing requirement, and a zero-cost profile. It is a
    /// *distinct* value from "unmeasured", not a stand-in for it.
    #[must_use]
    pub fn permissive() -> Self {
        Self {
            required_payload_kinds: Vec::new(),
            verifier_needs_producing_kinds: false,
            profile: AdmissionProfile {
                casm_cost_hint_words: CostFigure::Measured(0),
                security_bits: SecurityFigure::from_prover_parameters(0),
            },
        }
    }
}

/// The relation a strategy checks.
///
/// `verify` is a pure function of (capability, envelope, context): the same
/// inputs must produce the same outcome on every node, or verification is
/// non-deterministic across the fleet.
pub trait AttestationStrategy: Send + Sync {
    /// Stable 128-bit discriminator, RFC-allocated or user range.
    fn type_id(&self) -> AttestationTypeId;

    /// Verify an attestation against a capability.
    ///
    /// # Errors
    /// Returns [`AttestationError`] when the relation does not hold or the
    /// context is unusable. Never returns `Ok` for an envelope whose relation
    /// it could not evaluate.
    fn verify(
        &self,
        ctx: &AttestationVerifyCtx<'_>,
    ) -> Result<AttestationOutcome, AttestationError>;
}

/// Optional companion: what a node must serve to verify, and what admitting
/// this strategy costs. A strategy that does not implement this declares no
/// deployment requirement and no cost.
pub trait AttestationAdmission {
    /// Payload kinds a node must serve to produce a valid attestation.
    /// Empty means the strategy imposes no deployment requirement.
    fn required_payload_kinds(&self) -> &[PayloadKindId] {
        &[]
    }

    /// Whether **verification** — as distinct from production — needs a node
    /// that also produces.
    fn verifier_needs_producing_kinds(&self) -> bool {
        false
    }

    /// Circuit budget facts for admission control.
    ///
    /// There is deliberately **no default**: returning `None` is how a strategy
    /// states it carries no circuit cost, and that is a claim it must be true
    /// about. A cost-bearing strategy returning `None` skips both budget gates.
    fn admission_profile(&self) -> Option<AdmissionProfile>;
}

/// What the strategy must extract from the capability to check its relation.
///
/// A strategy supplies this. It is a function the strategy implements, not a
/// value, so the strategy cannot be handed the capability id and asked to
/// compare a token against itself — the capability id hashes the canonical form
/// the envelope sits inside, so an expectation returning it would be a function
/// of the attestation.
pub trait Expectation {
    /// Extract the bindable value this relation checks against.
    ///
    /// Must return `None` when the capability form carries no value outside its
    /// own hashed form. Mint refuses rather than calling an expectation that
    /// could only return the capability id.
    fn bindable_value(&self, ctx: &AttestationVerifyCtx<'_>) -> Option<Vec<u8>>;
}

/// Operator policy consulted by the dispatch.
///
/// Core owns the trait and never the implementation: a policy is an operator
/// choice, and an operator that could not express one would simply configure a
/// strategy out rather than a policy in.
pub trait PolicySource: Send + Sync {
    /// Whether the operator accepts attestations of this type at all.
    fn accepts(&self, type_id: &AttestationTypeId) -> bool;

    /// Whether this capability form, having passed the operator's own rules,
    /// may proceed.
    fn validate(&self, ctx: &AttestationVerifyCtx<'_>) -> Result<(), AttestationError>;
}

/// What the strategy is given.
///
/// `expectation` is a required field, not an `Option`: a relation with nothing
/// to check against is not a relation, and making the caller remember to supply
/// it is how it goes missing.
pub struct AttestationVerifyCtx<'a> {
    /// The capability being presented.
    pub capability: &'a octo_cap_macaroon::macaroon::Macaroon,
    /// The envelope recovered from its canonical form.
    pub envelope: &'a AttestationEnvelope,
    /// How to extract the bindable value. Required.
    pub expectation: &'a dyn Expectation,
    /// Operator policy.
    pub policy: &'a dyn PolicySource,
}

impl fmt::Debug for AttestationVerifyCtx<'_> {
    /// Prints the envelope type id only. The capability carries a root-secret
    /// hash and the envelope carries a proof payload, and dumping either into a
    /// panic message or a log line is exactly what RFC-0973 §Adversary A5 warns
    /// against.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AttestationVerifyCtx")
            .field("capability_id", &"[REDACTED 32 bytes]")
            .field("envelope_type_id", &self.envelope.type_id)
            .field("expectation", &"<dyn Expectation>")
            .field("policy", &"<dyn PolicySource>")
            .finish()
    }
}

/// The result of a relation.
///
/// A reason is **required** on rejection and absent on satisfaction, enforced by
/// the shape of the enum rather than by convention: the satisfied variant has
/// nowhere to put one.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum AttestationOutcome {
    /// The relation holds.
    Satisfied,
    /// The relation does not hold, and this is why.
    Unsatisfied {
        /// Strategy-chosen explanation. Must not contain attestation bytes.
        reason: String,
    },
}

impl AttestationOutcome {
    /// Whether the relation holds.
    #[must_use]
    pub fn is_satisfied(&self) -> bool {
        matches!(self, Self::Satisfied)
    }
}

/// Why a registration was refused.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum RegisterError {
    /// The id is already registered. A second claim never replaces the
    /// incumbent.
    AlreadyRegistered(AttestationTypeId),
    /// An RFC-range id that the allocation record does not contain.
    NotAllocated(AttestationTypeId),
    /// Present in the allocation record but allocated to a different crate.
    WrongOwner {
        /// The id that was refused.
        type_id: AttestationTypeId,
        /// Crate the record allocated it to.
        allocated_to: String,
        /// Crate that tried to claim it, as observed at link time.
        claimant: String,
    },
}

/// The RFC-range ids core has allocated, each naming the crate it is allocated
/// to.
///
/// Until this exists the RFC range admits nothing. Binding the owner is what
/// stops any crate claiming a core-sanctioned id and serving whatever it likes
/// under it.
#[derive(Clone, Debug, Default)]
pub struct AllocationRecord {
    entries: BTreeMap<AttestationTypeId, String>,
}

impl AllocationRecord {
    /// An empty record. Admits nothing in the RFC range.
    #[must_use]
    pub fn empty() -> Self {
        Self::default()
    }

    /// Allocate an RFC-range id to a crate.
    pub fn allocate(&mut self, type_id: AttestationTypeId, owner: impl Into<String>) {
        self.entries.insert(type_id, owner.into());
    }

    /// The crate an id is allocated to, if any.
    #[must_use]
    pub fn owner_of(&self, type_id: &AttestationTypeId) -> Option<&str> {
        self.entries.get(type_id).map(String::as_str)
    }

    /// How many ids the record holds.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the record holds nothing.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Fail-closed registry of attestation strategies.
///
/// Resolution is a pure lookup on a stable 128-bit key: no fallback, no
/// default strategy, and an unregistered id rejects.
#[derive(Clone, Default)]
pub struct AttestationRegistry {
    strategies: BTreeMap<AttestationTypeId, Arc<dyn AttestationStrategy>>,
}

impl AttestationRegistry {
    /// An empty registry. Every id rejects.
    #[must_use]
    pub fn empty() -> Self {
        Self::default()
    }

    /// Register a strategy.
    ///
    /// `claimant` is the crate identity the **registration host observes at
    /// link time** — never anything the strategy reports about itself. A
    /// strategy asked to name itself would name whatever the record names,
    /// which would make the owner binding vacuous.
    ///
    /// Every id is exclusively claimed, in either range, and a second claim is
    /// refused rather than replacing the incumbent.
    ///
    /// # Errors
    /// [`RegisterError::AlreadyRegistered`] if the id is taken,
    /// [`RegisterError::NotAllocated`] if an RFC-range id is absent from the
    /// allocation record, or [`RegisterError::WrongOwner`] if it is present
    /// but allocated elsewhere.
    pub fn register_strategy(
        &mut self,
        strategy: Arc<dyn AttestationStrategy>,
        claimant: &str,
        allocations: &AllocationRecord,
    ) -> Result<AttestationTypeId, RegisterError> {
        let type_id = strategy.type_id();

        if self.strategies.contains_key(&type_id) {
            return Err(RegisterError::AlreadyRegistered(type_id));
        }

        // Only the RFC range is allocated. A user-range id is any crate's own,
        // so ownership is the claimant's business, not core's.
        if type_id.is_rfc_range() {
            match allocations.owner_of(&type_id) {
                None => return Err(RegisterError::NotAllocated(type_id)),
                Some(owner) if owner != claimant => {
                    return Err(RegisterError::WrongOwner {
                        type_id,
                        allocated_to: owner.to_owned(),
                        claimant: claimant.to_owned(),
                    });
                }
                Some(_) => {}
            }
        }

        self.strategies.insert(type_id, strategy);
        Ok(type_id)
    }

    /// Resolve an id to its strategy. Fail-closed: `None` means reject.
    #[must_use]
    pub fn resolve(&self, type_id: &AttestationTypeId) -> Option<&Arc<dyn AttestationStrategy>> {
        self.strategies.get(type_id)
    }

    /// How many strategies are registered.
    #[must_use]
    pub fn len(&self) -> usize {
        self.strategies.len()
    }

    /// Whether the registry is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.strategies.is_empty()
    }

    /// The registered ids, in key order.
    pub fn ids(&self) -> impl Iterator<Item = &AttestationTypeId> {
        self.strategies.keys()
    }
}

impl fmt::Debug for AttestationRegistry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let ids: Vec<_> = self
            .strategies
            .keys()
            .map(AttestationTypeId::to_string)
            .collect();
        f.debug_struct("AttestationRegistry")
            .field("strategies", &ids)
            .finish()
    }
}

/// A strategy plus its optional admission side.
///
/// Registration stores this rather than a bare strategy so [`admission_of`] can
/// tell "implements nothing" from "implements the trait and answers `None`".
#[derive(Clone)]
pub struct Registration {
    /// The relation.
    pub strategy: Arc<dyn AttestationStrategy>,
    /// The admission side, when the strategy implements it.
    pub admission: Option<Arc<dyn AttestationAdmission>>,
}

impl fmt::Debug for Registration {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Registration")
            .field("type_id", &self.strategy.type_id())
            .field("has_admission", &self.admission.is_some())
            .finish()
    }
}

/// Build the `Admission` a dispatch or boot site should use.
///
/// There is deliberately no function taking a bare `&dyn AttestationStrategy`.
/// Stable Rust has no trait upcast from `dyn AttestationStrategy` to
/// `dyn AttestationAdmission`, and faking one by returning `None` would collapse
/// the very distinction the split exists to preserve: "implements nothing" and
/// "implements the trait and answers None" would read identically, which is the
/// fail-open path. A [`Registration`] carries both sides instead.
#[must_use]
pub fn admission_of(registration: &Registration) -> Admission {
    let Some(adm) = registration.admission.as_deref() else {
        return Admission::permissive();
    };
    Admission {
        required_payload_kinds: adm.required_payload_kinds().to_vec(),
        verifier_needs_producing_kinds: adm.verifier_needs_producing_kinds(),
        profile: adm.admission_profile().unwrap_or_default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rfc_id(n: u8) -> AttestationTypeId {
        let mut b = [0u8; 16];
        b[15] = n;
        AttestationTypeId::from_bytes(b)
    }

    fn user_id(n: u8) -> AttestationTypeId {
        let mut b = [0x80u8; 16];
        b[15] = n;
        AttestationTypeId::from_bytes(b)
    }

    struct Yes;
    impl AttestationStrategy for Yes {
        fn type_id(&self) -> AttestationTypeId {
            rfc_id(1)
        }
        fn verify(
            &self,
            _ctx: &AttestationVerifyCtx<'_>,
        ) -> Result<AttestationOutcome, AttestationError> {
            Ok(AttestationOutcome::Satisfied)
        }
    }

    struct No;
    impl AttestationStrategy for No {
        fn type_id(&self) -> AttestationTypeId {
            rfc_id(2)
        }
        fn verify(
            &self,
            _ctx: &AttestationVerifyCtx<'_>,
        ) -> Result<AttestationOutcome, AttestationError> {
            Ok(AttestationOutcome::Unsatisfied {
                reason: "relation does not hold".to_owned(),
            })
        }
    }

    struct ProofBearing;
    impl AttestationStrategy for ProofBearing {
        fn type_id(&self) -> AttestationTypeId {
            rfc_id(3)
        }
        fn verify(
            &self,
            _ctx: &AttestationVerifyCtx<'_>,
        ) -> Result<AttestationOutcome, AttestationError> {
            Ok(AttestationOutcome::Satisfied)
        }
    }
    impl AttestationAdmission for ProofBearing {
        fn admission_profile(&self) -> Option<AdmissionProfile> {
            // Carries a proof, so it must NOT answer None: that would read as
            // "no circuit cost" and skip both budget gates.
            Some(AdmissionProfile {
                casm_cost_hint_words: CostFigure::Unmeasured,
                security_bits: SecurityFigure::from_prover_parameters(128),
            })
        }
    }

    struct ClaimsNoCost;
    impl AttestationStrategy for ClaimsNoCost {
        fn type_id(&self) -> AttestationTypeId {
            rfc_id(4)
        }
        fn verify(
            &self,
            _ctx: &AttestationVerifyCtx<'_>,
        ) -> Result<AttestationOutcome, AttestationError> {
            Ok(AttestationOutcome::Satisfied)
        }
    }
    impl AttestationAdmission for ClaimsNoCost {
        fn admission_profile(&self) -> Option<AdmissionProfile> {
            None
        }
    }

    #[test]
    fn empty_registry_rejects_everything() {
        let reg = AttestationRegistry::empty();
        assert!(reg.is_empty());
        assert!(reg.resolve(&rfc_id(1)).is_none());
    }

    #[test]
    fn unallocated_rfc_range_id_is_refused() {
        let mut reg = AttestationRegistry::empty();
        let allocs = AllocationRecord::empty();
        let err = reg
            .register_strategy(Arc::new(Yes), "octo-cap-zk", &allocs)
            .unwrap_err();
        assert_eq!(err, RegisterError::NotAllocated(rfc_id(1)));
        assert!(reg.is_empty());
    }

    #[test]
    fn rfc_range_id_allocated_to_another_crate_is_refused() {
        let mut allocs = AllocationRecord::empty();
        allocs.allocate(rfc_id(1), "octo-cap-zk");
        let mut reg = AttestationRegistry::empty();
        let err = reg
            .register_strategy(Arc::new(Yes), "octo-cap-poi", &allocs)
            .unwrap_err();
        assert_eq!(
            err,
            RegisterError::WrongOwner {
                type_id: rfc_id(1),
                allocated_to: "octo-cap-zk".to_owned(),
                claimant: "octo-cap-poi".to_owned(),
            }
        );
    }

    #[test]
    fn matching_owner_registers() {
        let mut allocs = AllocationRecord::empty();
        allocs.allocate(rfc_id(1), "octo-cap-zk");
        let mut reg = AttestationRegistry::empty();
        assert_eq!(
            reg.register_strategy(Arc::new(Yes), "octo-cap-zk", &allocs)
                .unwrap(),
            rfc_id(1)
        );
        assert!(reg.resolve(&rfc_id(1)).is_some());
    }

    #[test]
    fn user_range_needs_no_allocation() {
        struct User;
        impl AttestationStrategy for User {
            fn type_id(&self) -> AttestationTypeId {
                user_id(9)
            }
            fn verify(
                &self,
                _ctx: &AttestationVerifyCtx<'_>,
            ) -> Result<AttestationOutcome, AttestationError> {
                Ok(AttestationOutcome::Satisfied)
            }
        }
        let mut reg = AttestationRegistry::empty();
        let allocs = AllocationRecord::empty();
        assert!(reg
            .register_strategy(Arc::new(User), "anybody", &allocs)
            .is_ok());
    }

    #[test]
    fn second_claim_never_replaces_the_incumbent() {
        let mut allocs = AllocationRecord::empty();
        allocs.allocate(rfc_id(1), "octo-cap-zk");
        let mut reg = AttestationRegistry::empty();
        reg.register_strategy(Arc::new(Yes), "octo-cap-zk", &allocs)
            .unwrap();
        let err = reg
            .register_strategy(Arc::new(Yes), "octo-cap-zk", &allocs)
            .unwrap_err();
        assert_eq!(err, RegisterError::AlreadyRegistered(rfc_id(1)));
        assert_eq!(reg.len(), 1);
    }

    #[test]
    fn admission_is_permissive_when_the_trait_is_absent() {
        let reg = Registration {
            strategy: Arc::new(Yes),
            admission: None,
        };
        assert_eq!(admission_of(&reg), Admission::permissive());
    }

    #[test]
    fn proof_bearing_strategy_reports_unmeasured_not_none() {
        let reg = Registration {
            strategy: Arc::new(ProofBearing),
            admission: Some(Arc::new(ProofBearing)),
        };
        let adm = admission_of(&reg);
        assert_eq!(adm.profile.casm_cost_hint_words, CostFigure::Unmeasured);
        assert_ne!(adm, Admission::permissive());
    }

    #[test]
    fn answering_none_is_a_distinct_value_from_permissive() {
        let reg = Registration {
            strategy: Arc::new(ClaimsNoCost),
            admission: Some(Arc::new(ClaimsNoCost)),
        };
        let adm = admission_of(&reg);
        assert_eq!(adm.profile.casm_cost_hint_words, CostFigure::Measured(0));
        // Both have a zero-cost profile, but the required-kinds emptiness came
        // from the trait rather than from absence, and the distinction is what
        // review checks.
        assert!(adm.required_payload_kinds.is_empty());
    }

    /// A strategy that refuses returns `Unsatisfied` carrying its own reason,
    /// and `verify` returning `Ok` does not mean the relation held.
    #[test]
    fn a_refusing_strategy_is_ok_but_unsatisfied() {
        struct AllowAll;
        impl Expectation for AllowAll {
            fn bindable_value(&self, _ctx: &AttestationVerifyCtx<'_>) -> Option<Vec<u8>> {
                Some(vec![0u8; 32])
            }
        }
        impl PolicySource for AllowAll {
            fn accepts(&self, _type_id: &AttestationTypeId) -> bool {
                true
            }
            fn validate(&self, _ctx: &AttestationVerifyCtx<'_>) -> Result<(), AttestationError> {
                Ok(())
            }
        }
        let mac = octo_cap_macaroon::macaroon::Macaroon::mint(&[5u8; 32]).unwrap();
        let envelope =
            AttestationEnvelope::new(AttestationTypeId::from_bytes([2u8; 16]), vec![0u8; 4]);
        let expect = AllowAll;
        let policy = AllowAll;
        let ctx = AttestationVerifyCtx {
            capability: &mac,
            envelope: &envelope,
            expectation: &expect,
            policy: &policy,
        };

        let out = No.verify(&ctx).unwrap();
        assert!(!out.is_satisfied());
        assert_eq!(
            out,
            AttestationOutcome::Unsatisfied {
                reason: "relation does not hold".to_owned()
            }
        );
    }

    #[test]
    fn security_figure_has_no_bare_integer_route() {
        let fig = SecurityFigure::from_prover_parameters(128);
        assert_eq!(fig.bits(), 128);
        // Debug must not expose the field as a struct field that could be read.
        assert!(format!("{fig:?}").contains("128 bits"));
    }

    #[test]
    fn outcome_requires_a_reason_on_rejection() {
        // Compile-time proof of the shape: the satisfied variant has no field
        // to carry a reason.
        let ok = AttestationOutcome::Satisfied;
        assert!(ok.is_satisfied());
        let bad = AttestationOutcome::Unsatisfied {
            reason: "nope".to_owned(),
        };
        assert!(!bad.is_satisfied());
    }
}

// ---------------------------------------------------------------------------
// Mint rule (RFC-0973 §Scope item 9)
// ---------------------------------------------------------------------------

/// Why the mint-side attestation rules refused.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum AttestRefusal {
    /// The capability form carries no value outside its own hashed form, so
    /// there is nothing for a relation to bind to.
    ///
    /// The capability id is explicitly **not** an acceptable substitute: it
    /// hashes the canonical form the envelope sits inside, so an expectation
    /// returning it would be a function of the attestation, and the strategy
    /// would be comparing a token against itself.
    NoBindableValue,
    /// The capability was already registered under a different id than the one
    /// its post-attestation form hashes to. Registration must happen in the same
    /// operation that attaches the envelope; registering first and attesting
    /// second would leave every attested capability absent from the registry.
    AlreadyRegisteredUnderOtherId {
        /// The id currently on the record.
        registered: [u8; 32],
        /// The id the post-attestation form hashes to.
        expected: [u8; 32],
    },
}

/// Attach an envelope to a capability and return the post-attestation form.
///
/// `expectation` must yield a bindable value for the form. Returning `None` is
/// [`AttestRefusal::NoBindableValue`] and is the refusal that makes the rest of
/// the rule meaningful.
///
/// The caller registers under `macaroon.id` **after** this returns — the id has
/// already been recomputed over the post-attestation canonical form, which is
/// the whole point: registering first would key the record on an id the token
/// no longer has.
///
/// # Errors
/// [`AttestRefusal::NoBindableValue`] when the form has nothing to bind.
pub fn attach_envelope(
    macaroon: &mut octo_cap_macaroon::macaroon::Macaroon,
    envelope: AttestationEnvelope,
    has_bindable_value: bool,
) -> Result<(), AttestRefusal> {
    if !has_bindable_value {
        return Err(AttestRefusal::NoBindableValue);
    }
    macaroon.attestation = Some(envelope);
    // Recompute over the post-attestation form. Registering under the old id
    // would make the registry unreachable for this capability.
    macaroon.id = octo_cap_macaroon::macaroon::compute_capability_id(macaroon);
    Ok(())
}

// ---------------------------------------------------------------------------
// Attenuation rule (RFC-0973 §Scope item 10)
// ---------------------------------------------------------------------------

/// Why the attenuation-side attestation rules refused.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum AttenuateRefusal {
    /// An unattested parent may not produce an attested child. Stripping the
    /// envelope is a downgrade, not an attenuation.
    WouldAddAttestation,
    /// The child carries a different `(type_id, payload)` pair than the parent.
    /// Substitution is not attenuation either.
    WouldSubstituteEnvelope,
    /// The parent has no registry record at all.
    ParentRecordAbsent,
    /// The parent record exists but is revoked.
    ParentRevoked,
    /// The parent record exists but is past its TTL.
    ParentExpired,
}

/// What the attenuation caller observed about the parent's registry record.
///
/// Modelled as an enum rather than a bool so "absent", "revoked" and "expired"
/// stay distinguishable: they are different refusals and collapse to different
/// reasons for the holder.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ParentState {
    /// No record.
    Absent,
    /// Record exists and is live.
    Active,
    /// Record exists but is revoked.
    Revoked,
    /// Record exists but is expired.
    Expired,
}

/// Carry a parent's envelope forward to the child, unchanged.
///
/// Three refusals, all distinct, plus the registry-state conditioning:
///
/// - **stripping** — an unattested parent may not produce an attested child;
/// - **substitution** — no different `(type_id, payload)` pair;
/// - **parent state** — absent, revoked, or expired all refuse.
///
/// The state check is what stops a bearer from attenuating a revoked capability
/// into a fresh active record the revocation never named. The revocation half is
/// a record flag and needs no clock; the expiry half is read here, at the point
/// the bearer performs the attenuation, which is a different vantage point from
/// the verifier and inherits the same untrusted-time exposure.
///
/// The child is registered under its recomputed id, in the same operation.
///
/// # Errors
/// See [`AttenuateRefusal`].
pub fn carry_envelope(
    parent: &octo_cap_macaroon::macaroon::Macaroon,
    child: &mut octo_cap_macaroon::macaroon::Macaroon,
    parent_state: ParentState,
) -> Result<(), AttenuateRefusal> {
    match parent_state {
        ParentState::Absent => return Err(AttenuateRefusal::ParentRecordAbsent),
        ParentState::Revoked => return Err(AttenuateRefusal::ParentRevoked),
        ParentState::Expired => return Err(AttenuateRefusal::ParentExpired),
        ParentState::Active => {}
    }

    match (&parent.attestation, &child.attestation) {
        // Stripping: parent had none, child has one.
        (None, Some(_)) => return Err(AttenuateRefusal::WouldAddAttestation),
        // Substitution: the pair must be carried forward byte-identical.
        (Some(a), Some(b)) if a != b => return Err(AttenuateRefusal::WouldSubstituteEnvelope),
        _ => {}
    }

    // Carry it forward unchanged, then re-key the child.
    child.attestation.clone_from(&parent.attestation);
    child.id = octo_cap_macaroon::macaroon::compute_capability_id(child);
    Ok(())
}

#[cfg(test)]
mod rule_tests {
    use super::*;
    use octo_cap_macaroon::macaroon::compute_capability_id;

    fn mac() -> octo_cap_macaroon::macaroon::Macaroon {
        octo_cap_macaroon::macaroon::Macaroon::mint(&[3u8; 32]).unwrap()
    }

    fn tid(n: u8) -> AttestationTypeId {
        AttestationTypeId::from_bytes([n; 16])
    }

    #[test]
    fn mint_refuses_a_form_with_no_bindable_value() {
        let mut m = mac();
        let before = m.id;
        let err = attach_envelope(
            &mut m,
            AttestationEnvelope::new(tid(1), vec![1]),
            /* has_bindable_value = */ false,
        )
        .unwrap_err();
        assert_eq!(err, AttestRefusal::NoBindableValue);
        assert!(m.attestation.is_none(), "a refused attach must not mutate");
        assert_eq!(m.id, before, "a refused attach must not re-key");
    }

    #[test]
    fn mint_registers_under_the_post_attestation_id() {
        let mut m = mac();
        let before = m.id;
        attach_envelope(&mut m, AttestationEnvelope::new(tid(1), vec![1]), true).unwrap();

        assert_ne!(m.id, before, "the id must move with the envelope");
        assert_eq!(
            m.id,
            compute_capability_id(&m),
            "and must be the recomputed one"
        );
        assert!(m.attestation.is_some());
    }

    fn attested(n: u8) -> octo_cap_macaroon::macaroon::Macaroon {
        let mut m = mac();
        attach_envelope(&mut m, AttestationEnvelope::new(tid(n), vec![1]), true).unwrap();
        m
    }

    #[test]
    fn attenuation_refuses_stripping() {
        let parent = mac(); // unattested
        let mut child = mac();
        child.attestation = Some(AttestationEnvelope::new(tid(1), vec![1]));
        assert_eq!(
            carry_envelope(&parent, &mut child, ParentState::Active).unwrap_err(),
            AttenuateRefusal::WouldAddAttestation
        );
    }

    #[test]
    fn attenuation_refuses_substitution() {
        let parent = attested(1);
        let mut child = attested(2);
        assert_eq!(
            carry_envelope(&parent, &mut child, ParentState::Active).unwrap_err(),
            AttenuateRefusal::WouldSubstituteEnvelope
        );
    }

    #[test]
    fn attenuation_carries_the_envelope_forward_unchanged() {
        let parent = attested(1);
        let mut child = mac();
        carry_envelope(&parent, &mut child, ParentState::Active).unwrap();
        assert_eq!(child.attestation, parent.attestation);
        assert_eq!(child.id, compute_capability_id(&child));
    }

    /// The revocation hole: without the state check a bearer could attenuate a
    /// revoked capability into a fresh active record the revocation never named.
    #[test]
    fn a_revoked_parent_cannot_be_attenuated_into_a_fresh_record() {
        let parent = attested(1);
        let mut child = mac();

        assert_eq!(
            carry_envelope(&parent, &mut child, ParentState::Revoked).unwrap_err(),
            AttenuateRefusal::ParentRevoked
        );
        assert!(
            child.attestation.is_none() && child.id != parent.id,
            "the refused child must not have been keyed"
        );
    }

    #[test]
    fn absent_and_expired_parents_refuse_differently() {
        let parent = attested(1);
        let mut a = mac();
        let mut b = mac();
        assert_eq!(
            carry_envelope(&parent, &mut a, ParentState::Absent).unwrap_err(),
            AttenuateRefusal::ParentRecordAbsent
        );
        assert_eq!(
            carry_envelope(&parent, &mut b, ParentState::Expired).unwrap_err(),
            AttenuateRefusal::ParentExpired
        );
    }

    /// The standing invariant: this mission never mints an attested capability,
    /// so none can exist that the dispatch is not yet checking.
    #[test]
    fn plain_mint_never_attaches_an_envelope() {
        let m = mac();
        assert!(m.attestation.is_none());
    }
}
