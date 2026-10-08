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
///
/// `Send + Sync` matches `AttestationStrategy` and is required in practice: the
/// registry is held in an `Arc` on a `VerifyContext` that crosses threads. A
/// registration holding a non-`Sync` admission side would make the whole
/// registry non-`Sync` and unusable from the verification path.
pub trait AttestationAdmission: Send + Sync {
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
    strategies: BTreeMap<AttestationTypeId, Registration>,
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
        admission: Option<Arc<dyn AttestationAdmission>>,
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

        self.strategies.insert(
            type_id,
            Registration {
                strategy,
                admission,
            },
        );
        Ok(type_id)
    }

    /// Resolve an id to its strategy. Fail-closed: `None` means reject.
    #[must_use]
    pub fn resolve(&self, type_id: &AttestationTypeId) -> Option<&Arc<dyn AttestationStrategy>> {
        self.strategies.get(type_id).map(|r| &r.strategy)
    }

    /// Resolve the full registration, including its admission side.
    ///
    /// This is what the dispatch reads at step 4. Resolving to the bare strategy
    /// and reconstructing a `Registration` with `admission: None` would make the
    /// coverage gate read the permissive value for every strategy, which is the
    /// vacuous-check failure the trait split exists to prevent.
    pub fn resolve_registration(&self, type_id: &AttestationTypeId) -> Option<&Registration> {
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
            .register_strategy(Arc::new(Yes), None, "octo-cap-zk", &allocs)
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
            .register_strategy(Arc::new(Yes), None, "octo-cap-poi", &allocs)
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
            reg.register_strategy(Arc::new(Yes), None, "octo-cap-zk", &allocs)
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
            .register_strategy(Arc::new(User), None, "anybody", &allocs)
            .is_ok());
    }

    #[test]
    fn second_claim_never_replaces_the_incumbent() {
        let mut allocs = AllocationRecord::empty();
        allocs.allocate(rfc_id(1), "octo-cap-zk");
        let mut reg = AttestationRegistry::empty();
        reg.register_strategy(Arc::new(Yes), None, "octo-cap-zk", &allocs)
            .unwrap();
        let err = reg
            .register_strategy(Arc::new(Yes), None, "octo-cap-zk", &allocs)
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

// ---------------------------------------------------------------------------
// Dispatch (RFC-0973 §Algorithms)
// ---------------------------------------------------------------------------

/// The operator's set of accepted attestation types.
///
/// An **empty set is a permitted operator choice**, not a misconfiguration.
/// Running a node that verifies no attestations is a real weakening — such a
/// node relies entirely on the RFC-0957 macaroon path and inherits its custody
/// properties — and the mitigation is that it is visible rather than silent. A
/// node therefore starts with an empty set and advertises it, rather than
/// refusing to start.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AcceptanceSet {
    ids: Vec<AttestationTypeId>,
}

impl AcceptanceSet {
    /// An empty set. The node accepts no attestations, and says so.
    #[must_use]
    pub fn empty() -> Self {
        Self::default()
    }

    /// Build from an explicit list, de-duplicated in first-seen order.
    #[must_use]
    pub fn from_ids(ids: impl IntoIterator<Item = AttestationTypeId>) -> Self {
        let mut seen = std::collections::BTreeSet::new();
        let mut out = Vec::new();
        for id in ids {
            if seen.insert(id) {
                out.push(id);
            }
        }
        Self { ids: out }
    }

    /// Whether this type is accepted here.
    #[must_use]
    pub fn accepts(&self, type_id: &AttestationTypeId) -> bool {
        self.ids.contains(type_id)
    }

    /// The accepted ids.
    #[must_use]
    pub fn ids(&self) -> &[AttestationTypeId] {
        &self.ids
    }

    /// Whether the set accepts nothing.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }
}

/// A policy that accepts everything the set already accepted.
///
/// Only the refuse-only direction is implemented. A `PolicySource` can decline
/// an attestation or decline to decide it, and never widen acceptance — a policy
/// that could permit something the acceptance set excluded would move the
/// operator's decision from one place to two.
#[derive(Clone, Copy, Debug, Default)]
pub struct PermissivePolicy;

impl PolicySource for PermissivePolicy {
    fn accepts(&self, _type_id: &AttestationTypeId) -> bool {
        true
    }

    fn validate(&self, _ctx: &AttestationVerifyCtx<'_>) -> Result<(), AttestationError> {
        Ok(())
    }
}

/// One rejection from the dispatch, one row per gate.
///
/// Variants are separate rather than collapsed into a reason string so an
/// operator can tell them apart: an unsatisfying attestation is the strategy
/// reaching a verdict, an error is the strategy declining to reach one, and a
/// panic is the strategy not returning at all.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum DispatchRejection {
    /// Step 0. Unknown capability, or inactive for any reason.
    #[allow(clippy::enum_variant_names)]
    UnknownOrInactiveCapability {
        /// Whether the cause was expiry rather than absence.
        expired: bool,
    },
    /// Step 0b. The RFC-0957 verification failed. Carries its own reason.
    MacaroonVerification(String),
    /// Step 2. The operator does not accept this strategy here.
    NotAcceptedHere(AttestationTypeId),
    /// Step 3. No strategy is registered for this id.
    UnknownStrategy(AttestationTypeId),
    /// Step 4. This node cannot verify what it cannot produce.
    CannotVerifyWhatItCannotProduce,
    /// Step 5. The form carries nothing an attestation could bind to.
    NoBindableValue,
    /// Step 5. The strategy reached a verdict and the relation does not hold.
    Unsatisfied(String),
    /// Step 5. The strategy declined to reach a verdict.
    StrategyError(String),
    /// Step 5. The strategy did not return at all (unwinding builds only).
    StrategyPanicked,
}

impl fmt::Display for DispatchRejection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownOrInactiveCapability { expired: true } => {
                f.write_str("capability no longer active, expired rather than unknown")
            }
            Self::UnknownOrInactiveCapability { expired: false } => {
                f.write_str("unknown or inactive capability")
            }
            Self::MacaroonVerification(reason) => {
                write!(f, "RFC-0957 reason: {reason}")
            }
            Self::NotAcceptedHere(id) => write!(f, "strategy not accepted here: {id}"),
            Self::UnknownStrategy(id) => write!(f, "unknown strategy id: {id}"),
            Self::CannotVerifyWhatItCannotProduce => {
                f.write_str("node cannot verify what it cannot produce")
            }
            Self::NoBindableValue => {
                f.write_str("capability carries nothing an attestation could bind to")
            }
            Self::Unsatisfied(reason) => write!(f, "verification unsatisfied: {reason}"),
            Self::StrategyError(reason) => write!(f, "strategy could not decide: {reason}"),
            Self::StrategyPanicked => f.write_str("strategy panicked"),
        }
    }
}

impl std::error::Error for DispatchRejection {}

/// What the node can serve, for the verification-side coverage gate at step 4.
pub trait PayloadCoverage {
    /// Whether the node serves every one of these payload kinds.
    fn serves_all(&self, kinds: &[PayloadKindId]) -> bool;
}

/// The dispatch's view of the node.
pub trait NodeView {
    /// The operator acceptance set.
    fn accept_set(&self) -> &AcceptanceSet;
    /// What the node can serve.
    fn coverage(&self) -> &dyn PayloadCoverage;
    /// The operator policy.
    fn policy(&self) -> &dyn PolicySource;
}

/// Everything the dispatch reads besides the capability itself.
pub struct DispatchInputs<'a> {
    /// Whether the capability is present and active. Step 0.
    pub lookup_active: bool,
    /// Whether the capability is present at all, used only to tell "expired"
    /// from "unknown" in the reason.
    pub present: bool,
    /// The RFC-0957 verification result. Step 0b.
    pub macaroon_ok: std::result::Result<(), String>,
    /// The attestation registry. Step 3.
    pub registry: &'a AttestationRegistry,
    /// The node. Steps 2 and 4.
    pub node: &'a dyn NodeView,
    /// The expectation, built by core from the capability and never from the
    /// attestation. `None` is the step-5 gate.
    pub expectation: Option<Box<dyn Expectation + 'a>>,
}

impl fmt::Debug for DispatchInputs<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DispatchInputs")
            .field("lookup_active", &self.lookup_active)
            .field("present", &self.present)
            .field("macaroon_ok", &self.macaroon_ok.is_ok())
            .field(
                "expectation",
                &self.expectation.as_ref().map(|_| "<dyn Expectation>"),
            )
            .finish()
    }
}

/// The fail-closed dispatch, in the RFC's order.
///
/// Steps 0 and 0b run on **both** branches. An attested capability is still a
/// capability: the attestation is an additional check layered on top of the ones
/// the token format already defines, never a replacement. Running it only for
/// unattested tokens would accept a copied token whose signature was stripped,
/// on the strength of a possession proof alone.
///
/// Step 0 is load-bearing rather than an optimization. Steps 1 to 5 can be
/// satisfied by an attacker who minted their own capability; only step 0 makes
/// acceptance mean anything.
///
/// # Errors
/// One [`DispatchRejection`] per gate, each distinguishable to the operator.
pub fn verify_capability_attestation(
    capability: &octo_cap_macaroon::macaroon::Macaroon,
    inputs: &DispatchInputs<'_>,
) -> Result<AttestationOutcome, DispatchRejection> {
    // Step 0. Authorisation precedes attestation.
    if !inputs.lookup_active {
        return Err(DispatchRejection::UnknownOrInactiveCapability {
            expired: inputs.present,
        });
    }

    // Step 0b. Runs on BOTH branches and is not optional on the attested one.
    if let Err(reason) = &inputs.macaroon_ok {
        return Err(DispatchRejection::MacaroonVerification(reason.clone()));
    }

    // Step 1. An absent attestation id accepts, having already run step 0b.
    let Some(envelope) = capability.attestation.as_ref() else {
        return Ok(AttestationOutcome::Satisfied);
    };
    let type_id = envelope.type_id;

    // Step 2. The operator acceptance set.
    if !inputs.node.accept_set().accepts(&type_id) {
        return Err(DispatchRejection::NotAcceptedHere(type_id));
    }

    // Step 3. Strategy resolution, fail-closed.
    let Some(registration) = inputs.registry.resolve_registration(&type_id) else {
        return Err(DispatchRejection::UnknownStrategy(type_id));
    };
    let strategy = &registration.strategy;

    // Step 4. The verification-side coverage gate, reachable per call. Boot
    // deliberately does not evaluate it, so this stays reachable for a node
    // whose coverage changed after boot.
    let admission = admission_of(registration);
    if admission.verifier_needs_producing_kinds
        && !inputs
            .node
            .coverage()
            .serves_all(&admission.required_payload_kinds)
    {
        return Err(DispatchRejection::CannotVerifyWhatItCannotProduce);
    }

    // Step 5. The expectation comes from the capability, never the attestation.
    // Mint refuses to attach an attestation to a form with nothing to bind to,
    // so this is unreachable through the mint path and reachable only through a
    // registry record written directly. The gate is still defined rather than
    // assumed, so the failure is a rejection and not a panic naming the strategy.
    let Some(expectation) = inputs.expectation.as_deref() else {
        return Err(DispatchRejection::NoBindableValue);
    };

    let ctx = AttestationVerifyCtx {
        capability,
        envelope,
        expectation,
        policy: inputs.node.policy(),
    };

    // Step 5a. The panic arm is a step of the dispatch, not a wrapper around it:
    // it sits beside the Err arm because a strategy that panics has produced no
    // outcome at all, and mapping that onto an ordinary error would let a future
    // implementation turn it into a success.
    //
    // AssertUnwindSafe discharges a compile-time obligation and leaves the
    // runtime question open, so this containment is conditional on an unwinding
    // build (RFC-0973 A4).
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| strategy.verify(&ctx)));

    match outcome {
        Err(_panic) => Err(DispatchRejection::StrategyPanicked),
        Ok(Err(e)) => Err(DispatchRejection::StrategyError(e.to_string())),
        Ok(Ok(AttestationOutcome::Unsatisfied { reason })) => {
            Err(DispatchRejection::Unsatisfied(reason))
        }
        Ok(Ok(AttestationOutcome::Satisfied)) => Ok(AttestationOutcome::Satisfied),
    }
}

#[cfg(test)]
mod dispatch_tests {
    use super::*;
    use octo_cap_macaroon::macaroon::Macaroon;
    use octo_protocol::payload_kind::PayloadKindId;

    fn tid(n: u8) -> AttestationTypeId {
        let mut b = [0u8; 16];
        b[15] = n;
        AttestationTypeId::from_bytes(b)
    }

    fn user_tid(n: u8) -> AttestationTypeId {
        let mut b = [0x80u8; 16];
        b[15] = n;
        AttestationTypeId::from_bytes(b)
    }

    fn mac() -> Macaroon {
        Macaroon::mint(&[11u8; 32]).unwrap()
    }

    struct Node {
        set: AcceptanceSet,
        serves: bool,
        policy: PermissivePolicy,
    }
    impl PayloadCoverage for Node {
        fn serves_all(&self, _kinds: &[PayloadKindId]) -> bool {
            self.serves
        }
    }
    impl NodeView for Node {
        fn accept_set(&self) -> &AcceptanceSet {
            &self.set
        }
        fn coverage(&self) -> &dyn PayloadCoverage {
            self
        }
        fn policy(&self) -> &dyn PolicySource {
            &self.policy
        }
    }

    fn node(ids: Vec<AttestationTypeId>) -> Node {
        Node {
            set: AcceptanceSet::from_ids(ids),
            serves: false,
            policy: PermissivePolicy,
        }
    }

    struct Expect;
    impl Expectation for Expect {
        fn bindable_value(&self, _ctx: &AttestationVerifyCtx<'_>) -> Option<Vec<u8>> {
            Some(vec![0x11; 32])
        }
    }

    /// Always accepts.
    struct Ok1(AttestationTypeId);
    impl AttestationStrategy for Ok1 {
        fn type_id(&self) -> AttestationTypeId {
            self.0
        }
        fn verify(
            &self,
            _c: &AttestationVerifyCtx<'_>,
        ) -> Result<AttestationOutcome, AttestationError> {
            Ok(AttestationOutcome::Satisfied)
        }
    }

    /// Verdict: relation does not hold.
    struct Unsat(AttestationTypeId);
    impl AttestationStrategy for Unsat {
        fn type_id(&self) -> AttestationTypeId {
            self.0
        }
        fn verify(
            &self,
            _c: &AttestationVerifyCtx<'_>,
        ) -> Result<AttestationOutcome, AttestationError> {
            Ok(AttestationOutcome::Unsatisfied {
                reason: "does not hold".to_owned(),
            })
        }
    }

    /// Declines to decide.
    struct Errs(AttestationTypeId);
    impl AttestationStrategy for Errs {
        fn type_id(&self) -> AttestationTypeId {
            self.0
        }
        fn verify(
            &self,
            _c: &AttestationVerifyCtx<'_>,
        ) -> Result<AttestationOutcome, AttestationError> {
            Err(AttestationError::StrategyRefused {
                reason: "cannot decide".to_owned(),
            })
        }
    }

    /// Does not return.
    struct Panicky(AttestationTypeId);
    impl AttestationStrategy for Panicky {
        fn type_id(&self) -> AttestationTypeId {
            self.0
        }
        fn verify(
            &self,
            _c: &AttestationVerifyCtx<'_>,
        ) -> Result<AttestationOutcome, AttestationError> {
            panic!("strategy exploded");
        }
    }

    /// Needs producing kinds the node does not serve.
    struct NeedsProducing(AttestationTypeId);
    impl AttestationStrategy for NeedsProducing {
        fn type_id(&self) -> AttestationTypeId {
            self.0
        }
        fn verify(
            &self,
            _c: &AttestationVerifyCtx<'_>,
        ) -> Result<AttestationOutcome, AttestationError> {
            Ok(AttestationOutcome::Satisfied)
        }
    }
    static NEEDED_KINDS: [PayloadKindId; 1] = [PayloadKindId::from_bytes([1u8; 16])];

    impl AttestationAdmission for NeedsProducing {
        fn required_payload_kinds(&self) -> &[PayloadKindId] {
            &NEEDED_KINDS
        }
        fn verifier_needs_producing_kinds(&self) -> bool {
            true
        }
        fn admission_profile(&self) -> Option<AdmissionProfile> {
            Some(AdmissionProfile::default())
        }
    }

    fn registry_with(
        s: Arc<dyn AttestationStrategy>,
        a: Option<Arc<dyn AttestationAdmission>>,
    ) -> AttestationRegistry {
        let id = s.type_id();
        let mut allocs = AllocationRecord::empty();
        // RFC-range ids need an owner before they can be claimed at all.
        allocs.allocate(id, "test");
        let mut reg = AttestationRegistry::empty();
        reg.register_strategy(s, a, "test", &allocs).unwrap();
        reg
    }

    fn attested(id: AttestationTypeId) -> Macaroon {
        let mut m = mac();
        attach_envelope(&mut m, AttestationEnvelope::new(id, vec![1, 2, 3]), true).unwrap();
        m
    }

    fn inputs<'a>(
        registry: &'a AttestationRegistry,
        node: &'a Node,
        expectation: Option<Box<dyn Expectation + 'a>>,
    ) -> DispatchInputs<'a> {
        DispatchInputs {
            lookup_active: true,
            present: true,
            macaroon_ok: Ok(()),
            registry,
            node,
            expectation,
        }
    }

    // --- step 0 ---

    #[test]
    fn step0_rejects_an_inactive_capability_before_anything_else() {
        let reg = AttestationRegistry::empty();
        let n = node(vec![]);
        let mut i = inputs(&reg, &n, Some(Box::new(Expect)));
        i.lookup_active = false;
        i.present = false;
        // Even with a perfect macaroon result and an accepted id, step 0 wins.
        assert_eq!(
            verify_capability_attestation(&attested(tid(1)), &i).unwrap_err(),
            DispatchRejection::UnknownOrInactiveCapability { expired: false }
        );
    }

    #[test]
    fn step0_distinguishes_expired_from_unknown() {
        let reg = AttestationRegistry::empty();
        let n = node(vec![]);
        let mut i = inputs(&reg, &n, Some(Box::new(Expect)));
        i.lookup_active = false;
        i.present = true;
        assert_eq!(
            verify_capability_attestation(&attested(tid(1)), &i).unwrap_err(),
            DispatchRejection::UnknownOrInactiveCapability { expired: true }
        );
    }

    // --- step 0b: the gate that had no carrier ---

    #[test]
    fn step0b_runs_on_the_attested_branch_too() {
        // A forged macaroon on an ATTESTED token must reject. If 0b ran only for
        // unattested tokens, this would pass, which is the whole defect.
        let reg = registry_with(Arc::new(Ok1(tid(1))), None);
        let n = node(vec![tid(1)]);
        let mut i = inputs(&reg, &n, Some(Box::new(Expect)));
        i.macaroon_ok = Err("holder signature failed".to_owned());
        assert_eq!(
            verify_capability_attestation(&attested(tid(1)), &i).unwrap_err(),
            DispatchRejection::MacaroonVerification("holder signature failed".to_owned())
        );
    }

    #[test]
    fn step0b_runs_on_the_unattested_branch() {
        let reg = AttestationRegistry::empty();
        let n = node(vec![]);
        let mut i = inputs(&reg, &n, Some(Box::new(Expect)));
        i.macaroon_ok = Err("caveat violated".to_owned());
        assert_eq!(
            verify_capability_attestation(&mac(), &i).unwrap_err(),
            DispatchRejection::MacaroonVerification("caveat violated".to_owned())
        );
    }

    // --- step 1 ---

    #[test]
    fn an_unattested_capability_accepts_after_0b() {
        let reg = AttestationRegistry::empty();
        let n = node(vec![]);
        let i = inputs(&reg, &n, Some(Box::new(Expect)));
        assert!(verify_capability_attestation(&mac(), &i)
            .unwrap()
            .is_satisfied());
    }

    // --- step 2 ---

    #[test]
    fn step2_rejects_a_type_outside_the_acceptance_set() {
        let reg = registry_with(Arc::new(Ok1(tid(1))), None);
        let n = node(vec![]); // accepts nothing: a permitted, visible choice
        let i = inputs(&reg, &n, Some(Box::new(Expect)));
        assert_eq!(
            verify_capability_attestation(&attested(tid(1)), &i).unwrap_err(),
            DispatchRejection::NotAcceptedHere(tid(1))
        );
    }

    // --- step 3 ---

    #[test]
    fn step3_is_fail_closed_for_an_unregistered_id() {
        let reg = AttestationRegistry::empty();
        let n = node(vec![tid(9)]);
        let i = inputs(&reg, &n, Some(Box::new(Expect)));
        assert_eq!(
            verify_capability_attestation(&attested(tid(9)), &i).unwrap_err(),
            DispatchRejection::UnknownStrategy(tid(9))
        );
    }

    // --- step 4: reads the real admission, not a permissive stand-in ---

    #[test]
    fn step4_rejects_when_the_node_cannot_verify_what_it_cannot_produce() {
        let s = Arc::new(NeedsProducing(tid(1)));
        let reg = registry_with(s.clone(), Some(s as Arc<dyn AttestationAdmission>));
        let n = node(vec![tid(1)]); // NoCoverage serves nothing
        let i = inputs(&reg, &n, Some(Box::new(Expect)));
        assert_eq!(
            verify_capability_attestation(&attested(tid(1)), &i).unwrap_err(),
            DispatchRejection::CannotVerifyWhatItCannotProduce
        );
    }

    #[test]
    fn step4_passes_when_the_node_serves_the_kinds() {
        let s = Arc::new(NeedsProducing(tid(1)));
        let reg = registry_with(s.clone(), Some(s as Arc<dyn AttestationAdmission>));
        let mut n = node(vec![tid(1)]);
        n.serves = true;
        let i = inputs(&reg, &n, Some(Box::new(Expect)));
        assert!(verify_capability_attestation(&attested(tid(1)), &i)
            .unwrap()
            .is_satisfied());
    }

    /// A strategy that omits the admission trait yields the permissive value, so
    /// the gate does not fire. This is the fail-open path Lifecycle Requirement 1
    /// closes by review rather than at runtime.
    #[test]
    fn a_strategy_without_the_admission_trait_does_not_trip_step4() {
        let reg = registry_with(Arc::new(Ok1(tid(1))), None);
        let n = node(vec![tid(1)]); // NoCoverage
        let i = inputs(&reg, &n, Some(Box::new(Expect)));
        assert!(verify_capability_attestation(&attested(tid(1)), &i)
            .unwrap()
            .is_satisfied());
    }

    // --- step 5 ---

    #[test]
    fn step5_rejects_a_form_with_no_bindable_value() {
        let reg = registry_with(Arc::new(Ok1(tid(1))), None);
        let n = node(vec![tid(1)]);
        let i = inputs(&reg, &n, None); // no expectation
        assert_eq!(
            verify_capability_attestation(&attested(tid(1)), &i).unwrap_err(),
            DispatchRejection::NoBindableValue
        );
    }

    #[test]
    fn an_unsatisfying_attestation_carries_the_strategys_reason() {
        let reg = registry_with(Arc::new(Unsat(tid(1))), None);
        let n = node(vec![tid(1)]);
        let i = inputs(&reg, &n, Some(Box::new(Expect)));
        assert_eq!(
            verify_capability_attestation(&attested(tid(1)), &i).unwrap_err(),
            DispatchRejection::Unsatisfied("does not hold".to_owned())
        );
    }

    #[test]
    fn an_error_is_distinguishable_from_an_unsatisfied_verdict() {
        let reg = registry_with(Arc::new(Errs(tid(1))), None);
        let n = node(vec![tid(1)]);
        let i = inputs(&reg, &n, Some(Box::new(Expect)));
        // A strategy that cannot decide has not verified anything, and the
        // operator needs to tell that apart from a strategy reaching a verdict.
        assert!(matches!(
            verify_capability_attestation(&attested(tid(1)), &i).unwrap_err(),
            DispatchRejection::StrategyError(_)
        ));
    }

    #[test]
    fn a_panic_becomes_a_rejection_not_a_propagated_fault() {
        let reg = registry_with(Arc::new(Panicky(tid(1))), None);
        let n = node(vec![tid(1)]);
        let i = inputs(&reg, &n, Some(Box::new(Expect)));
        assert_eq!(
            verify_capability_attestation(&attested(tid(1)), &i).unwrap_err(),
            DispatchRejection::StrategyPanicked
        );
    }

    // --- user-range ids follow the same fail-closed path ---

    #[test]
    fn an_unregistered_user_range_id_rejects_rather_than_defaulting() {
        let reg = AttestationRegistry::empty();
        let n = node(vec![user_tid(5)]);
        let i = inputs(&reg, &n, Some(Box::new(Expect)));
        assert_eq!(
            verify_capability_attestation(&attested(user_tid(5)), &i).unwrap_err(),
            DispatchRejection::UnknownStrategy(user_tid(5))
        );
    }

    #[test]
    fn acceptance_set_de_duplicates() {
        let s = AcceptanceSet::from_ids([tid(1), tid(1), tid(2)]);
        assert_eq!(s.ids().len(), 2);
    }
}

// ---------------------------------------------------------------------------
// Boot validation (RFC-0973 §Algorithms, `validate_policy`)
// ---------------------------------------------------------------------------

/// What a node can serve, and what it has budget for, at boot.
pub trait NodeCapability {
    /// Whether the node serves this payload kind in order to produce.
    fn serves(&self, kind: &PayloadKindId) -> bool;
    /// Whether the node has headroom for a circuit of this many words at this
    /// security level.
    fn has_headroom(&self, casm_words: u64, security_bits: u64) -> bool;
}

/// A boot-time refusal.
///
/// Every variant names the offending strategy, and the two kinds-and-budget
/// failures additionally name what was missing or exceeded, since the strategy
/// alone would not tell an operator whether to serve a kind or raise a budget.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum BootRefusal {
    /// An acceptance entry names a strategy that does not resolve.
    UnresolvableStrategy(AttestationTypeId),
    /// The node does not serve a payload kind the strategy requires in order to
    /// produce.
    MissingPayloadKind {
        /// The strategy that needs it.
        strategy: AttestationTypeId,
        /// The kind the node does not serve.
        missing: PayloadKindId,
    },
    /// The strategy's circuit cost is not measured.
    ///
    /// Decisive: the node refuses rather than passing on a guess.
    UnmeasuredCost(AttestationTypeId),
    /// The strategy's measured cost exceeds the node's budget.
    BudgetExceeded {
        /// The strategy that exceeds it.
        strategy: AttestationTypeId,
        /// Words the strategy needs.
        required: u64,
    },
}

impl fmt::Display for BootRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnresolvableStrategy(id) => write!(f, "{id}: not a resolvable strategy"),
            Self::MissingPayloadKind { strategy, missing } => {
                write!(
                    f,
                    "{strategy}: does not serve required payload kind {missing:?}"
                )
            }
            Self::UnmeasuredCost(id) => write!(f, "{id}: circuit cost is unmeasured"),
            Self::BudgetExceeded { strategy, required } => {
                write!(f, "{strategy}: needs {required} CASM words, over budget")
            }
        }
    }
}

impl std::error::Error for BootRefusal {}

/// Validate the operator's policy against what the node can actually serve.
///
/// Runs before the node serves anything. Four refusals, each decisive:
///
/// 1. an acceptance entry naming an unresolvable strategy,
/// 2. a required payload kind the node does not serve — the **producing** side,
/// 3. an unmeasured circuit cost,
/// 4. a measured cost over budget.
///
/// The verification-side requirement is a *different* gate and is deliberately
/// **not** checked here, so dispatch step 4 stays reachable for a node that boots
/// but cannot verify. Boot answers "what must this node produce"; step 4 answers
/// "what must it check".
///
/// A strategy omitting the admission trait skips both budget gates. That is the
/// fail-open path Lifecycle Requirement 1 closes by review rather than at
/// runtime, and it is why [`register_strategy`] records the admission side.
///
/// # Errors
/// The first [`BootRefusal`] encountered, in the order above.
pub fn validate_policy(
    accept_set: &AcceptanceSet,
    registry: &AttestationRegistry,
    node: &dyn NodeCapability,
) -> Result<(), BootRefusal> {
    for type_id in accept_set.ids() {
        // 1. Resolvability.
        let Some(registration) = registry.resolve_registration(type_id) else {
            return Err(BootRefusal::UnresolvableStrategy(*type_id));
        };

        // The admission side is absent when the strategy omits the trait. Both
        // budget gates are then skipped, which is the documented fail-open path.
        let Some(admission) = registration.admission.as_deref() else {
            continue;
        };

        // 2. Producing-side coverage. Runs only when kinds are non-empty.
        for kind in admission.required_payload_kinds() {
            if !node.serves(kind) {
                return Err(BootRefusal::MissingPayloadKind {
                    strategy: *type_id,
                    missing: *kind,
                });
            }
        }

        // 3 and 4. The budget gates.
        let Some(profile) = admission.admission_profile() else {
            continue;
        };
        match profile.casm_cost_hint_words {
            CostFigure::Unmeasured => {
                return Err(BootRefusal::UnmeasuredCost(*type_id));
            }
            CostFigure::Measured(words) => {
                if !node.has_headroom(words, profile.security_bits.bits()) {
                    return Err(BootRefusal::BudgetExceeded {
                        strategy: *type_id,
                        required: words,
                    });
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod boot_tests {
    use super::*;
    use octo_protocol::payload_kind::PayloadKindId;

    fn tid(n: u8) -> AttestationTypeId {
        let mut b = [0u8; 16];
        b[15] = n;
        AttestationTypeId::from_bytes(b)
    }

    fn kind(n: u8) -> PayloadKindId {
        PayloadKindId::from_bytes([n; 16])
    }

    struct Nodecap {
        serves: bool,
        budget: u64,
    }
    impl NodeCapability for Nodecap {
        fn serves(&self, _k: &PayloadKindId) -> bool {
            self.serves
        }
        fn has_headroom(&self, casm_words: u64, _bits: u64) -> bool {
            casm_words <= self.budget
        }
    }

    struct Profile {
        id: AttestationTypeId,
        kinds: Vec<PayloadKindId>,
        cost: CostFigure,
    }
    impl AttestationStrategy for Profile {
        fn type_id(&self) -> AttestationTypeId {
            self.id
        }
        fn verify(
            &self,
            _c: &AttestationVerifyCtx<'_>,
        ) -> Result<AttestationOutcome, AttestationError> {
            Ok(AttestationOutcome::Satisfied)
        }
    }
    impl AttestationAdmission for Profile {
        fn required_payload_kinds(&self) -> &[PayloadKindId] {
            &self.kinds
        }
        fn admission_profile(&self) -> Option<AdmissionProfile> {
            Some(AdmissionProfile {
                casm_cost_hint_words: self.cost,
                security_bits: SecurityFigure::from_prover_parameters(128),
            })
        }
    }

    /// Register one strategy with a matching admission side.
    ///
    /// Both arcs are built from the SAME parameters. Registering a strategy with
    /// a differently-configured admission object would make the gates read a
    /// stand-in rather than the strategy's own claim.
    fn registry_for(
        id: AttestationTypeId,
        kinds: Vec<PayloadKindId>,
        cost: CostFigure,
    ) -> AttestationRegistry {
        let mut allocs = AllocationRecord::empty();
        allocs.allocate(id, "test");
        let s: Arc<dyn AttestationStrategy> = Arc::new(Profile {
            id,
            kinds: kinds.clone(),
            cost,
        });
        let a: Arc<dyn AttestationAdmission> = Arc::new(Profile { id, kinds, cost });
        let mut reg = AttestationRegistry::empty();
        reg.register_strategy(s, Some(a), "test", &allocs).unwrap();
        reg
    }

    #[test]
    fn boot_refuses_an_unresolvable_acceptance_entry() {
        let reg = AttestationRegistry::empty();
        let set = AcceptanceSet::from_ids([tid(3)]);
        let node = Nodecap {
            serves: true,
            budget: u64::MAX,
        };
        assert_eq!(
            validate_policy(&set, &reg, &node).unwrap_err(),
            BootRefusal::UnresolvableStrategy(tid(3))
        );
    }

    #[test]
    fn boot_refuses_a_missing_payload_kind_by_name() {
        let reg = registry_for(tid(1), vec![kind(4)], CostFigure::Measured(10));
        let set = AcceptanceSet::from_ids([tid(1)]);
        let node = Nodecap {
            serves: false,
            budget: u64::MAX,
        };
        assert_eq!(
            validate_policy(&set, &reg, &node).unwrap_err(),
            BootRefusal::MissingPayloadKind {
                strategy: tid(1),
                missing: kind(4)
            }
        );
    }

    #[test]
    fn boot_refuses_an_unmeasured_cost_rather_than_guessing() {
        let reg = registry_for(tid(1), vec![], CostFigure::Unmeasured);
        let set = AcceptanceSet::from_ids([tid(1)]);
        let node = Nodecap {
            serves: true,
            budget: u64::MAX,
        };
        assert_eq!(
            validate_policy(&set, &reg, &node).unwrap_err(),
            BootRefusal::UnmeasuredCost(tid(1))
        );
    }

    #[test]
    fn boot_refuses_a_cost_over_budget_and_names_the_figure() {
        let reg = registry_for(tid(1), vec![], CostFigure::Measured(5000));
        let set = AcceptanceSet::from_ids([tid(1)]);
        let node = Nodecap {
            serves: true,
            budget: 1000,
        };
        assert_eq!(
            validate_policy(&set, &reg, &node).unwrap_err(),
            BootRefusal::BudgetExceeded {
                strategy: tid(1),
                required: 5000
            }
        );
    }

    #[test]
    fn a_satisfied_policy_boots() {
        let reg = registry_for(tid(1), vec![kind(4)], CostFigure::Measured(500));
        let set = AcceptanceSet::from_ids([tid(1)]);
        let node = Nodecap {
            serves: true,
            budget: 1000,
        };
        assert!(validate_policy(&set, &reg, &node).is_ok());
    }

    /// The producing-side gate must not consume the verification-side gate, or
    /// boot would answer a question step 4 owns.
    #[test]
    fn boot_does_not_evaluate_the_verification_side_requirement() {
        struct VerifyOnly(AttestationTypeId);
        impl AttestationStrategy for VerifyOnly {
            fn type_id(&self) -> AttestationTypeId {
                self.0
            }
            fn verify(
                &self,
                _c: &AttestationVerifyCtx<'_>,
            ) -> Result<AttestationOutcome, AttestationError> {
                Ok(AttestationOutcome::Satisfied)
            }
        }
        impl AttestationAdmission for VerifyOnly {
            // Produces nothing, but needs a node that also produces in order to
            // verify. Boot must pass this; step 4 is where it bites.
            fn required_payload_kinds(&self) -> &[PayloadKindId] {
                &[]
            }
            fn verifier_needs_producing_kinds(&self) -> bool {
                true
            }
            fn admission_profile(&self) -> Option<AdmissionProfile> {
                Some(AdmissionProfile::default())
            }
        }

        let id = tid(2);
        let mut allocs = AllocationRecord::empty();
        allocs.allocate(id, "test");
        let s: Arc<dyn AttestationStrategy> = Arc::new(VerifyOnly(id));
        let a: Arc<dyn AttestationAdmission> = Arc::new(VerifyOnly(id));
        let mut reg = AttestationRegistry::empty();
        reg.register_strategy(s, Some(a), "test", &allocs).unwrap();

        let set = AcceptanceSet::from_ids([id]);
        let node = Nodecap {
            serves: false,
            budget: u64::MAX,
        };
        assert!(
            validate_policy(&set, &reg, &node).is_ok(),
            "boot must leave the verification-side gate to step 4"
        );
    }

    /// An empty acceptance set is a permitted choice, not a boot failure.
    #[test]
    fn an_empty_acceptance_set_boots() {
        let reg = AttestationRegistry::empty();
        let set = AcceptanceSet::empty();
        let node = Nodecap {
            serves: false,
            budget: 0,
        };
        assert!(validate_policy(&set, &reg, &node).is_ok());
        assert!(set.is_empty());
    }
}
