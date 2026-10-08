//! Caveat predicate evaluation as an open extension point (RFC-0974 §Amendment 1).
//!
//! # Why this exists
//!
//! RFC-0957 §Verify-Time Extension specifies *how* to verify a macaroon chain but
//! never says where caveat predicates are evaluated, and RFC-0965 §Out of Scope
//! declined the question on the record ("Caveat verification algorithms. This RFC
//! defines encoding only"). Nothing has picked it up since: evaluation is hardcoded
//! across the crate and the wider workspace.
//!
//! The result is a half-built extension surface. A third party can add a caveat
//! type — `Caveat` is `#[non_exhaustive]` with a `Raw` escape hatch — but cannot
//! supply the logic that decides whether it holds. An unregistered `Raw` caveat is
//! rejected by `set_subsumes_with_registry`, so the surface currently lets an
//! outsider add types that are guaranteed to be refused.
//!
//! This module closes that. It adds one trait, one registry, and one dispatch
//! function.
//!
//! # Two design decisions worth defending
//!
//! **The registry key is the existing `RawCaveat.name` string, not a new id type.**
//! `AttestationStrategy` is keyed by a 128-bit `AttestationTypeId`, and it would
//! have been consistent to mint a parallel `CaveatTypeId`. That would be a second
//! identifier derivation for a concept that already has a wire-level name — the
//! name that `is_raw_name_registered` already keys on. Introducing one would be a
//! parallel abstraction with its own allocation record and its own
//! cross-implementation compatibility burden, for no additional guarantee. Core
//! variants map to their RFC-0964 short tag; `Raw` maps to the caller-supplied name.
//! Adding a caveat a third party can actually evaluate requires no edit to this
//! crate.
//!
//! **Operations are an open fact bag, not a struct.** A `ProposedOperation` with
//! named fields (`model: String`, `chain_id: [u8; 32]`, ...) would need a core edit
//! every time an evaluator needs a fact nobody anticipated — and an edit to a
//! crate that third parties already depend on. A typed-value map keyed by name
//! means the set of facts an operation can carry is decided by whoever builds the
//! operation, which is the party that actually knows. This mirrors the
//! `CanonicalContext::Bytes | Structured` split in RFC-0965 §RedemptionContext.
//!
//! # What is deliberately NOT an extension point
//!
//! Attenuation subsumption (`set_subsumes`, `parent_caveat_implies`) stays
//! core-owned and hardcoded. Subsumption is a one-sided relation — `parent ⊇
//! child` — and an externally supplied implementation that returns `true`
//! incorrectly forges a legitimate delegation chain. Evaluation can fail safe;
//! subsumption cannot. If a use case genuinely needs external subsumption, the
//! answer is an RFC that changes the delegation trust model, not a trait that lets
//! a crate assert `true`.

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;

use super::Caveat;
use crate::clock::Clock;

// ---------------------------------------------------------------------------
// Operation facts
// ---------------------------------------------------------------------------

/// A typed value an operation can carry.
///
/// Deliberately closed over *value shapes* while the *set of facts* stays open.
/// A new kind of value needs a variant here (a core edit, and one that is honest:
/// it is a change to what an operation can express); a new **fact name** does not.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum OperationFact {
    /// Boolean fact.
    Bool(bool),
    /// Unsigned 64-bit fact. Also the shape used for Unix timestamps.
    U64(u64),
    /// Unsigned 128-bit fact. Also the shape used for token amounts.
    U128(u128),
    /// Fixed 32-byte fact — chain ids, vault ids, digests.
    Hash32([u8; 32]),
    /// Opaque byte blob, hex-encoded on any wire boundary.
    Bytes(Vec<u8>),
    /// UTF-8 text. DIDs, model names, provider ids.
    Text(String),
    /// Ordered list of facts. Nested lists are permitted.
    List(Vec<OperationFact>),
}

impl OperationFact {
    /// Borrow as `bool`, or `None` if this fact is a different shape.
    ///
    /// A shape mismatch is reported as absence rather than as a panic or a
    /// silent coercion: "the operation carries no such fact" and "the operation
    /// carries it with the wrong type" lead to the same fail-closed outcome, and
    /// an evaluator that got a coerced value would be a real bug.
    #[must_use]
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(b) => Some(*b),
            _ => None,
        }
    }

    /// Borrow as `u64`, or `None` if this fact is a different shape.
    #[must_use]
    pub fn as_u64(&self) -> Option<u64> {
        match self {
            Self::U64(v) => Some(*v),
            _ => None,
        }
    }

    /// Borrow as `u128`, or `None` if this fact is a different shape.
    #[must_use]
    pub fn as_u128(&self) -> Option<u128> {
        match self {
            Self::U128(v) => Some(*v),
            _ => None,
        }
    }

    /// Borrow as `[u8; 32]`, or `None` if this fact is a different shape.
    #[must_use]
    pub fn as_hash32(&self) -> Option<[u8; 32]> {
        match self {
            Self::Hash32(v) => Some(*v),
            _ => None,
        }
    }

    /// Borrow as a string slice, or `None` if this fact is a different shape.
    #[must_use]
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::Text(v) => Some(v.as_str()),
            _ => None,
        }
    }

    /// Borrow as a byte slice, or `None` if this fact is a different shape.
    #[must_use]
    pub fn as_bytes(&self) -> Option<&[u8]> {
        match self {
            Self::Bytes(v) => Some(v.as_slice()),
            _ => None,
        }
    }

    /// Borrow as a list, or `None` if this fact is a different shape.
    #[must_use]
    pub fn as_list(&self) -> Option<&[OperationFact]> {
        match self {
            Self::List(v) => Some(v.as_slice()),
            _ => None,
        }
    }

    /// The shape name, for refusal messages.
    #[must_use]
    pub fn shape(&self) -> &'static str {
        match self {
            Self::Bool(_) => "bool",
            Self::U64(_) => "u64",
            Self::U128(_) => "u128",
            Self::Hash32(_) => "hash32",
            Self::Bytes(_) => "bytes",
            Self::Text(_) => "text",
            Self::List(_) => "list",
        }
    }
}

/// Well-known fact keys.
///
/// Namespaced under `cipherocto/` so a third-party fact cannot collide with a
/// core one. Core evaluators read these; core operation builders write them.
/// Nothing enforces the prefix — an unregistered fact name is simply a fact
/// nobody has agreed to mean, and it fails closed at the evaluator that wanted it.
pub mod fact_key {
    /// `Hash32` — chain the operation is being redeemed on.
    pub const CHAIN_ID: &str = "cipherocto/chain_id";
    /// `Hash32` — vault the operation targets.
    pub const VAULT_ID: &str = "cipherocto/vault_id";
    /// `Text` — DID of the presenter.
    pub const HOLDER_DID: &str = "cipherocto/holder_did";
    /// `Text` — model the operation invokes.
    pub const MODEL: &str = "cipherocto/model";
    /// `Text` — provider the operation routes to.
    pub const PROVIDER: &str = "cipherocto/provider";
    /// `Hash32` — digest of the exact invocation, for `InvocationHashBind`.
    pub const REQUEST_HASH: &str = "cipherocto/request_hash";
    /// `U128` — amount the operation spends.
    pub const AMOUNT: &str = "cipherocto/amount";
    /// `Text` — ask this operation settles, if any.
    pub const ASK_ID: &str = "cipherocto/ask_id";
}

/// The operation a capability is being presented for.
///
/// An open bag of named facts. Facts the operation does not carry are simply
/// absent, and an evaluator that needs one says so via
/// [`CaveatEvaluator::required_facts`] so the refusal can name it.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct ProposedOperation {
    facts: BTreeMap<String, OperationFact>,
}

impl ProposedOperation {
    /// An operation carrying no facts. Every evaluator that declares a
    /// requirement refuses.
    #[must_use]
    pub fn empty() -> Self {
        Self::default()
    }

    /// Builder-style insert, consuming and returning `Self`.
    #[must_use]
    pub fn with(mut self, key: impl Into<String>, fact: OperationFact) -> Self {
        self.facts.insert(key.into(), fact);
        self
    }

    /// Insert or replace a fact in place.
    pub fn set(&mut self, key: impl Into<String>, fact: OperationFact) {
        self.facts.insert(key.into(), fact);
    }

    /// Read a fact by name.
    #[must_use]
    pub fn fact(&self, key: &str) -> Option<&OperationFact> {
        self.facts.get(key)
    }

    /// Whether a fact is present, whatever its shape.
    #[must_use]
    pub fn has(&self, key: &str) -> bool {
        self.facts.contains_key(key)
    }

    /// How many facts the operation carries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.facts.len()
    }

    /// Whether the operation carries no facts.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.facts.is_empty()
    }

    /// The fact names present, in key order.
    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.facts.keys().map(String::as_str)
    }
}

// ---------------------------------------------------------------------------
// Evaluation context
// ---------------------------------------------------------------------------

/// What a [`CaveatEvaluator`] is given.
///
/// Carries per-call facts only. Anything an evaluator needs across calls — a
/// vault lookup, a balance reader, an HTTP client — belongs in the evaluator
/// itself, constructed at registration. That is the dependency-inversion
/// boundary: the side that owns the data owns the access to it.
pub struct CaveatEvalCtx<'a> {
    /// The caveat under evaluation, exactly as it appeared in the capability.
    ///
    /// A core-keyed evaluator must check this is the variant it expects and
    /// refuse a [`Caveat::Raw`] carrying a colliding name. The registry refuses
    /// that case by default; see [`CaveatEvaluator::handles_raw`].
    pub caveat: &'a Caveat,
    /// The operation being attempted.
    pub operation: &'a ProposedOperation,
    /// Wall clock. Always present — no evaluator may capture its own time
    /// source, or two nodes would disagree about expiry.
    pub clock: &'a dyn Clock,
}

/// Why a caveat was refused.
///
/// Core-owned on purpose: an extension crate cannot construct a refusal reason
/// for a caveat type it does not own.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum CaveatRefusal {
    /// No evaluator is registered under this key. Fail-closed.
    EvaluatorNotFound(String),
    /// The operation does not carry a fact the evaluator declared it needs.
    ///
    /// Distinguished from `Violated` because the two mean different things to a
    /// caller: a `MissingFact` is a wiring problem at the redemption site, and a
    /// `Violated` is the predicate doing its job.
    MissingFact {
        /// Fact key the evaluator named.
        key: String,
        /// Registry key of the evaluator that wanted it.
        evaluator: String,
    },
    /// An RFC-range key was claimed by an evaluator that does not opt into
    /// handling `Raw` caveats, and a `Raw` caveat arrived under that name.
    ///
    /// This is the spoofing guard. Without it, minting
    /// `Raw { name: "vault", .. }` would reach an evaluator written for
    /// `Caveat::Vault`, which may then approve on the strength of bytes it does
    /// not understand.
    KeyCollision {
        /// The contested registry key.
        key: String,
    },
    /// The evaluator refused, and this is why.
    Violated {
        /// Registry key of the refusing evaluator.
        evaluator: String,
        /// Evaluator-supplied detail.
        detail: Cow<'static, str>,
    },
}

impl fmt::Display for CaveatRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EvaluatorNotFound(k) => {
                write!(f, "no caveat evaluator registered for key {k}")
            }
            Self::MissingFact { key, evaluator } => {
                write!(
                    f,
                    "caveat evaluator {evaluator} requires operation fact {key}, which this operation does not carry"
                )
            }
            Self::KeyCollision { key } => {
                write!(
                    f,
                    "caveat key {key} is a core caveat name, and the registered evaluator does not accept Raw caveats"
                )
            }
            Self::Violated { evaluator, detail } => {
                write!(f, "caveat evaluator {evaluator} refused: {detail}")
            }
        }
    }
}

impl std::error::Error for CaveatRefusal {}

// ---------------------------------------------------------------------------
// The trait
// ---------------------------------------------------------------------------

/// Decides whether a caveat holds for a proposed operation.
///
/// Core owns the trait and never an implementation. There is no default
/// evaluator and no fallback: a caveat with no registered evaluator refuses.
///
/// # Determinism
///
/// `evaluate` MUST be a pure function of `(ctx, evaluator state)`. The same
/// inputs must produce the same outcome on every node, or verification diverges
/// across the fleet — which is a consensus failure, not a policy difference.
/// Concretely: read the clock from `ctx.clock`, never from `std::time`; do not
/// reach the network; do not consult wall-local state.
pub trait CaveatEvaluator: Send + Sync {
    /// Registry key. For a core variant this is its RFC-0964 short tag; for a
    /// third-party caveat it is the `RawCaveat.name` the holder minted with.
    fn key(&self) -> &str;

    /// Operation facts this evaluator needs. Refused with
    /// [`CaveatRefusal::MissingFact`] when absent, *before* `evaluate` runs.
    ///
    /// Declaring a fact is not optional bookkeeping: an evaluator that reads an
    /// absent fact and decides it means "absent" is making that call silently,
    /// and the resulting refusal — or worse, its absence — cannot be traced
    /// back to a wiring mistake at the redemption site.
    fn required_facts(&self) -> &[&str] {
        &[]
    }

    /// Whether this evaluator accepts a [`Caveat::Raw`] under its key.
    ///
    /// Defaults to `false`. Only an evaluator that genuinely parses the raw
    /// bytes should return `true`; see [`CaveatRefusal::KeyCollision`].
    fn handles_raw(&self) -> bool {
        false
    }

    /// Evaluate the predicate. `Err` refuses.
    fn evaluate(&self, ctx: &CaveatEvalCtx<'_>) -> Result<(), CaveatRefusal>;
}

/// Lets a pre-built `Arc<dyn CaveatEvaluator>` be registered directly, without
/// the caller having to unwrap and re-wrap it.
///
/// Without this, `register` would accept owned evaluators only, and a caller
/// holding a trait object would have to downcast — or keep two copies.
impl<T: CaveatEvaluator + ?Sized> CaveatEvaluator for Arc<T> {
    fn key(&self) -> &str {
        (**self).key()
    }
    fn required_facts(&self) -> &[&str] {
        (**self).required_facts()
    }
    fn handles_raw(&self) -> bool {
        (**self).handles_raw()
    }
    fn evaluate(&self, ctx: &CaveatEvalCtx<'_>) -> Result<(), CaveatRefusal> {
        (**self).evaluate(ctx)
    }
}

// ---------------------------------------------------------------------------
// Registry
// ---------------------------------------------------------------------------

/// What a successful registration produced.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct RegistrationOutcome {
    /// The key now held in the registry.
    pub key: String,
    /// Whether this registration shadows a core [`CaveatName`].
    ///
    /// Permitted, and in fact the migration path for core caveats out of
    /// hardcoded logic — but it means an operator has taken over a built-in
    /// check, so it is reported rather than inferred.
    pub shadows_core_caveat: bool,
}

/// Why a registration was refused.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum RegisterError {
    /// The key is already registered. A second claim never replaces the
    /// incumbent — otherwise a later-loaded extension silently changes the
    /// meaning of every capability already in flight.
    AlreadyRegistered(String),
    /// The key is empty. An empty key is indistinguishable from "unset" in a
    /// lookup, so it is refused rather than stored.
    EmptyKey,
}

/// Fail-closed registry of caveat evaluators.
///
/// Resolution is a pure lookup on the key: no fallback, no default evaluator,
/// and an unregistered key refuses.
#[derive(Clone, Default)]
pub struct CaveatEvaluatorRegistry {
    evaluators: BTreeMap<String, Arc<dyn CaveatEvaluator>>,
}

impl CaveatEvaluatorRegistry {
    /// An empty registry. Every caveat refuses.
    #[must_use]
    pub fn empty() -> Self {
        Self::default()
    }

    /// Register an evaluator.
    ///
    /// Exclusive in both directions: the key is claimed once and a second claim
    /// is refused rather than replacing the incumbent.
    ///
    /// # Errors
    /// [`RegisterError::EmptyKey`] if the evaluator's key is empty, or
    /// [`RegisterError::AlreadyRegistered`] if the key is taken.
    pub fn register<E: CaveatEvaluator + 'static>(
        &mut self,
        evaluator: E,
    ) -> Result<RegistrationOutcome, RegisterError> {
        let key = evaluator.key().to_owned();
        if key.is_empty() {
            return Err(RegisterError::EmptyKey);
        }
        if self.evaluators.contains_key(&key) {
            return Err(RegisterError::AlreadyRegistered(key));
        }
        let shadows_core_caveat = is_core_caveat_key(&key);
        self.evaluators.insert(key.clone(), Arc::new(evaluator));
        Ok(RegistrationOutcome {
            key,
            shadows_core_caveat,
        })
    }

    /// Resolve a key to its evaluator. Fail-closed: `None` means refuse.
    #[must_use]
    pub fn resolve(&self, key: &str) -> Option<&Arc<dyn CaveatEvaluator>> {
        self.evaluators.get(key)
    }

    /// How many evaluators are registered.
    #[must_use]
    pub fn len(&self) -> usize {
        self.evaluators.len()
    }

    /// Whether the registry is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.evaluators.is_empty()
    }

    /// The registered keys, in key order.
    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.evaluators.keys().map(String::as_str)
    }

    /// Keys registered here that shadow a core [`CaveatName`].
    ///
    /// Intended for boot-time reporting: an operator should be able to see
    /// which built-in checks their configuration has taken over.
    pub fn core_shadowing_keys(&self) -> impl Iterator<Item = &str> + '_ {
        self.evaluators
            .keys()
            .filter(|k| is_core_caveat_key(k.as_str()))
            .map(String::as_str)
    }
}

impl fmt::Debug for CaveatEvaluatorRegistry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CaveatEvaluatorRegistry")
            .field("keys", &self.evaluators.keys().collect::<Vec<_>>())
            .finish()
    }
}

/// Whether a registry key names a core caveat variant.
///
/// Derived from [`CaveatName::from_short_tag`] rather than a hand-written list,
/// so a new core variant is automatically covered rather than silently
/// unregistered.
#[must_use]
pub fn is_core_caveat_key(key: &str) -> bool {
    super::CaveatName::from_short_tag(key).is_some()
}

// ---------------------------------------------------------------------------
// Dispatch
// ---------------------------------------------------------------------------

/// Evaluate every caveat against a proposed operation. All must pass.
///
/// Composition is logical AND over an ordered set, per the composition principle:
/// a caveat set constrains an operation, and each constraint is independently
/// necessary. The set order is the macaroon's authenticated order and is
/// preserved so the refusal reported is deterministic.
///
/// # Errors
/// The first refusal in caveat order. Failing fast is deliberate — once one
/// predicate refuses, evaluating the rest is wasted work and can only produce a
/// less informative answer.
pub fn evaluate_all(
    registry: &CaveatEvaluatorRegistry,
    caveats: &[Caveat],
    operation: &ProposedOperation,
    clock: &dyn Clock,
) -> Result<(), CaveatRefusal> {
    for caveat in caveats {
        evaluate_one(registry, caveat, operation, clock)?;
    }
    Ok(())
}

/// Evaluate a single caveat.
///
/// # Errors
/// See [`evaluate_all`].
pub fn evaluate_one(
    registry: &CaveatEvaluatorRegistry,
    caveat: &Caveat,
    operation: &ProposedOperation,
    clock: &dyn Clock,
) -> Result<(), CaveatRefusal> {
    let key = caveat.evaluator_key();
    let evaluator = registry
        .resolve(&key)
        .ok_or_else(|| CaveatRefusal::EvaluatorNotFound(key.to_string()))?;

    // Spoofing guard. A `Raw` caveat whose name collides with a core caveat
    // would otherwise be handed to an evaluator written for the core variant.
    // Evaluators that genuinely parse raw bytes opt in explicitly.
    if matches!(caveat, Caveat::Raw(_)) && is_core_caveat_key(&key) && !evaluator.handles_raw() {
        return Err(CaveatRefusal::KeyCollision {
            key: key.to_string(),
        });
    }

    // Requirement gate, before the evaluator sees anything. Without it, an
    // evaluator reading an absent fact decides silently what its absence means.
    for required in evaluator.required_facts() {
        if !operation.has(required) {
            return Err(CaveatRefusal::MissingFact {
                key: required.to_string(),
                evaluator: key.to_string(),
            });
        }
    }

    let ctx = CaveatEvalCtx {
        caveat,
        operation,
        clock,
    };
    evaluator.evaluate(&ctx)
}

// ---------------------------------------------------------------------------
// Reference implementation
// ---------------------------------------------------------------------------

/// Expiry evaluator for [`Caveat::Before`], on a seconds granularity.
///
/// Ships as the reference shape for a core-keyed evaluator and as the first
/// step of moving core caveats off hardcoded logic. It is **additive** — nothing
/// in the existing verify path calls it yet — so registering it changes no
/// behaviour on its own.
#[derive(Debug, Clone, Copy, Default)]
pub struct BeforeEvaluator;

impl CaveatEvaluator for BeforeEvaluator {
    fn key(&self) -> &str {
        "before"
    }

    fn evaluate(&self, ctx: &CaveatEvalCtx<'_>) -> Result<(), CaveatRefusal> {
        let Some(Caveat::Before(deadline)) = Some(ctx.caveat) else {
            return Err(CaveatRefusal::Violated {
                evaluator: "before".to_owned(),
                detail: "caveat is not a Before variant".into(),
            });
        };
        // `Before` carries seconds; the clock speaks milliseconds.
        let now_secs = ctx.clock.unix_millis() / 1_000;
        if now_secs <= *deadline {
            Ok(())
        } else {
            Err(CaveatRefusal::Violated {
                evaluator: "before".to_owned(),
                detail: Cow::Owned(format!("expired: now {now_secs} > deadline {deadline}")),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::caveat::CaveatName;
    use crate::clock::FixedClock;

    fn op() -> ProposedOperation {
        ProposedOperation::empty()
    }

    struct Always {
        key: &'static str,
    }

    impl CaveatEvaluator for Always {
        fn key(&self) -> &str {
            self.key
        }
        fn evaluate(&self, _ctx: &CaveatEvalCtx<'_>) -> Result<(), CaveatRefusal> {
            Ok(())
        }
    }

    struct NeedsBalance;

    impl CaveatEvaluator for NeedsBalance {
        fn key(&self) -> &str {
            "escrow_balance"
        }
        fn required_facts(&self) -> &[&str] {
            &["app.example/balance"]
        }
        fn evaluate(&self, ctx: &CaveatEvalCtx<'_>) -> Result<(), CaveatRefusal> {
            let Some(OperationFact::U128(bal)) = ctx.operation.fact("app.example/balance") else {
                return Err(CaveatRefusal::Violated {
                    evaluator: "escrow_balance".to_owned(),
                    detail: "balance fact has wrong shape".into(),
                });
            };
            // The caveat carries `{"required": <u128>}`.
            let required = match ctx.caveat {
                Caveat::Raw(raw) => serde_json::from_slice::<serde_json::Value>(&raw.value)
                    .ok()
                    .and_then(|v| v.get("required").and_then(serde_json::Value::as_u64)),
                _ => None,
            };
            let Some(required) = required else {
                return Err(CaveatRefusal::Violated {
                    evaluator: "escrow_balance".to_owned(),
                    detail: "caveat carries no parsable `required` amount".into(),
                });
            };
            if *bal >= u128::from(required) {
                Ok(())
            } else {
                Err(CaveatRefusal::Violated {
                    evaluator: "escrow_balance".to_owned(),
                    detail: Cow::Owned(format!("balance {bal} below required {required}")),
                })
            }
        }
    }

    fn raw(name: &str, value: &str) -> Caveat {
        Caveat::Raw(crate::caveat::RawCaveat {
            name: name.to_owned(),
            value: value.as_bytes().to_vec(),
        })
    }

    fn reg(entries: Vec<(&'static str, bool)>) -> CaveatEvaluatorRegistry {
        let mut r = CaveatEvaluatorRegistry::empty();
        for (key, handles_raw) in entries {
            let e: Arc<dyn CaveatEvaluator> = if handles_raw {
                Arc::new(RawAware { key })
            } else {
                Arc::new(Always { key })
            };
            let _ = r.register(e);
        }
        r
    }

    struct RawAware {
        key: &'static str,
    }

    impl CaveatEvaluator for RawAware {
        fn key(&self) -> &str {
            self.key
        }
        fn handles_raw(&self) -> bool {
            true
        }
        fn evaluate(&self, _ctx: &CaveatEvalCtx<'_>) -> Result<(), CaveatRefusal> {
            Ok(())
        }
    }

    // --- registry mechanics -------------------------------------------------

    #[test]
    fn empty_registry_refuses_every_caveat() {
        let r = CaveatEvaluatorRegistry::empty();
        let c = raw("anything", "x");
        assert_eq!(
            evaluate_one(&r, &c, &op(), &FixedClock::new(0)),
            Err(CaveatRefusal::EvaluatorNotFound("anything".to_owned()))
        );
    }

    #[test]
    fn registration_is_exclusive() {
        let mut r = CaveatEvaluatorRegistry::empty();
        assert_eq!(r.register(Always { key: "x" }).unwrap().key, "x".to_owned());
        assert_eq!(
            r.register(Always { key: "x" }),
            Err(RegisterError::AlreadyRegistered("x".to_owned()))
        );
        assert_eq!(r.len(), 1);
    }

    #[test]
    fn empty_key_is_refused() {
        let mut r = CaveatEvaluatorRegistry::empty();
        assert_eq!(r.register(Always { key: "" }), Err(RegisterError::EmptyKey));
        assert!(r.is_empty());
    }

    #[test]
    fn core_key_registration_reports_shadowing() {
        let mut r = CaveatEvaluatorRegistry::empty();
        let out = r.register(Always { key: "vault" }).unwrap();
        assert!(out.shadows_core_caveat);
        let out = r
            .register(Always {
                key: "my_app/vault",
            })
            .unwrap();
        assert!(!out.shadows_core_caveat);
        assert_eq!(r.core_shadowing_keys().collect::<Vec<_>>(), vec!["vault"]);
    }

    #[test]
    fn core_key_detection_follows_the_caveat_name_table() {
        assert!(is_core_caveat_key("vault"));
        assert!(is_core_caveat_key("amount_max"));
        assert!(!is_core_caveat_key("app.example/escrow"));
        // If a core variant is added, this follows automatically rather than
        // needing the list updated here.
        assert_eq!(
            CaveatName::from_short_tag("asset_binding").is_some(),
            is_core_caveat_key("asset_binding")
        );
    }

    // --- key derivation -----------------------------------------------------

    #[test]
    fn core_variant_key_is_its_short_tag() {
        assert_eq!(Caveat::Vault([0u8; 32]).evaluator_key(), "vault");
        assert_eq!(Caveat::Before(100).evaluator_key(), "before");
    }

    #[test]
    fn raw_key_is_the_caller_supplied_name() {
        assert_eq!(
            raw("app.example/escrow", "x").evaluator_key(),
            "app.example/escrow"
        );
    }

    // --- spoofing guard -----------------------------------------------------

    #[test]
    fn raw_caveat_cannot_reach_a_core_evaluator() {
        // Operator registered an evaluator for "vault" believing it is the
        // built-in. An attacker mints Raw{name:"vault"} and must not reach it.
        let r = reg(vec![("vault", false)]);
        assert_eq!(
            evaluate_one(
                &r,
                &raw("vault", "attacker bytes"),
                &op(),
                &FixedClock::new(0)
            ),
            Err(CaveatRefusal::KeyCollision {
                key: "vault".to_owned()
            })
        );
    }

    #[test]
    fn raw_aware_evaluator_does_receive_the_raw_caveat() {
        let r = reg(vec![("vault", true)]);
        assert!(evaluate_one(&r, &raw("vault", "x"), &op(), &FixedClock::new(0)).is_ok());
    }

    #[test]
    fn raw_under_non_core_name_always_dispatches() {
        let r = reg(vec![("app.example/x", false)]);
        assert!(evaluate_one(&r, &raw("app.example/x", "x"), &op(), &FixedClock::new(0)).is_ok());
    }

    // --- required facts -----------------------------------------------------

    #[test]
    fn missing_required_fact_refuses_before_the_evaluator_runs() {
        let mut r = CaveatEvaluatorRegistry::empty();
        r.register(NeedsBalance).unwrap();
        let err =
            evaluate_one(&r, &raw("escrow_balance", "{}"), &op(), &FixedClock::new(0)).unwrap_err();
        assert_eq!(
            err,
            CaveatRefusal::MissingFact {
                key: "app.example/balance".to_owned(),
                evaluator: "escrow_balance".to_owned(),
            }
        );
    }

    #[test]
    fn present_required_fact_passes_the_gate() {
        let mut r = CaveatEvaluatorRegistry::empty();
        r.register(NeedsBalance).unwrap();
        let o = op().with("app.example/balance", OperationFact::U128(50));
        // Reaches the evaluator, which then fails on the caveat shape — proving
        // the gate let it through rather than short-circuiting.
        let err =
            evaluate_one(&r, &raw("escrow_balance", "{}"), &o, &FixedClock::new(0)).unwrap_err();
        assert!(matches!(err, CaveatRefusal::Violated { .. }));
    }

    // --- set semantics ------------------------------------------------------

    #[test]
    fn all_caveats_must_pass() {
        let r = reg(vec![("a", false), ("b", false)]);
        let set = vec![raw("a", ""), raw("b", "")];
        assert!(evaluate_all(&r, &set, &op(), &FixedClock::new(0)).is_ok());
    }

    #[test]
    fn one_unregistered_member_fails_the_set() {
        let r = reg(vec![("a", false)]);
        let set = vec![raw("a", ""), raw("b", "")];
        assert_eq!(
            evaluate_all(&r, &set, &op(), &FixedClock::new(0)),
            Err(CaveatRefusal::EvaluatorNotFound("b".to_owned()))
        );
    }

    #[test]
    fn empty_caveat_set_passes() {
        let r = CaveatEvaluatorRegistry::empty();
        assert!(evaluate_all(&r, &[], &op(), &FixedClock::new(0)).is_ok());
    }

    #[test]
    fn refusal_reports_the_first_member_in_set_order() {
        let r = reg(vec![("a", false)]);
        let set = vec![raw("zzz", ""), raw("aaa", "")];
        assert_eq!(
            evaluate_all(&r, &set, &op(), &FixedClock::new(0)),
            Err(CaveatRefusal::EvaluatorNotFound("zzz".to_owned()))
        );
    }

    // --- reference implementation ------------------------------------------

    fn before_reg() -> CaveatEvaluatorRegistry {
        let mut r = CaveatEvaluatorRegistry::empty();
        r.register(BeforeEvaluator).unwrap();
        r
    }

    #[test]
    fn before_evaluator_accepts_before_deadline() {
        let r = before_reg();
        // 100s deadline, clock at 50s.
        assert!(evaluate_one(&r, &Caveat::Before(100), &op(), &FixedClock::new(50_000)).is_ok());
    }

    #[test]
    fn before_evaluator_refuses_after_deadline() {
        let r = before_reg();
        assert!(evaluate_one(&r, &Caveat::Before(100), &op(), &FixedClock::new(101_000)).is_err());
    }

    #[test]
    fn before_evaluator_boundary_is_inclusive() {
        let r = before_reg();
        assert!(evaluate_one(&r, &Caveat::Before(100), &op(), &FixedClock::new(100_000)).is_ok());
    }

    #[test]
    fn before_evaluator_refuses_a_wrong_variant() {
        let r = before_reg();
        assert!(evaluate_one(
            &r,
            &Caveat::Model("m".to_owned()),
            &op(),
            &FixedClock::new(0)
        )
        .is_err());
    }

    #[test]
    fn before_evaluator_shadows_the_core_key() {
        // A core-keyed evaluator reports that it takes over a built-in check,
        // so boot-time reporting can surface it.
        let mut r = CaveatEvaluatorRegistry::empty();
        assert!(r.register(BeforeEvaluator).unwrap().shadows_core_caveat);
        assert_eq!(r.core_shadowing_keys().collect::<Vec<_>>(), vec!["before"]);
    }

    // --- operation bag ------------------------------------------------------

    #[test]
    fn facts_round_trip_by_shape() {
        let o = ProposedOperation::empty()
            .with("h", OperationFact::Hash32([7u8; 32]))
            .with("t", OperationFact::Text("m".to_owned()))
            .with("u", OperationFact::U64(9))
            .with("w", OperationFact::U128(11))
            .with("b", OperationFact::Bytes(vec![1, 2]))
            .with("l", OperationFact::List(vec![OperationFact::Bool(true)]))
            .with("z", OperationFact::Bool(false));
        assert_eq!(
            o.fact("h").and_then(OperationFact::as_hash32),
            Some([7u8; 32])
        );
        assert_eq!(o.fact("t").and_then(OperationFact::as_str), Some("m"));
        assert_eq!(o.fact("u").and_then(OperationFact::as_u64), Some(9));
        assert_eq!(o.fact("w").and_then(OperationFact::as_u128), Some(11));
        assert_eq!(
            o.fact("b").and_then(OperationFact::as_bytes),
            Some(&[1u8, 2][..])
        );
        assert_eq!(
            o.fact("l")
                .and_then(OperationFact::as_list)
                .map(<[OperationFact]>::len),
            Some(1)
        );
        assert_eq!(o.fact("z").and_then(OperationFact::as_bool), Some(false));
        assert_eq!(o.len(), 7);
        assert!(!o.is_empty());
    }

    #[test]
    fn shape_mismatch_reads_as_absent_not_as_a_coercion() {
        let f = OperationFact::Text("5".to_owned());
        assert_eq!(f.as_u64(), None);
        assert_eq!(f.shape(), "text");
        assert_eq!(OperationFact::Bool(true).as_u64(), None);
    }

    #[test]
    fn set_replaces_and_keys_are_ordered() {
        let mut o = ProposedOperation::empty().with("b", OperationFact::U64(1));
        o.set("a", OperationFact::U64(2));
        o.set("b", OperationFact::U64(3));
        assert_eq!(o.fact("b").and_then(OperationFact::as_u64), Some(3));
        assert_eq!(o.keys().collect::<Vec<_>>(), vec!["a", "b"]);
    }

    #[test]
    fn fact_key_constants_are_namespaced_and_distinct() {
        let keys = [
            fact_key::CHAIN_ID,
            fact_key::VAULT_ID,
            fact_key::HOLDER_DID,
            fact_key::MODEL,
            fact_key::PROVIDER,
            fact_key::REQUEST_HASH,
            fact_key::AMOUNT,
            fact_key::ASK_ID,
        ];
        for k in keys {
            assert!(k.starts_with("cipherocto/"), "{k} must be namespaced");
        }
        let mut sorted = keys.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), keys.len(), "fact keys must be distinct");
    }

    // --- registry introspection --------------------------------------------

    #[test]
    fn keys_are_ordered_and_len_agrees() {
        let r = reg(vec![("z", false), ("a", false), ("m", false)]);
        assert_eq!(r.keys().collect::<Vec<_>>(), vec!["a", "m", "z"]);
        assert_eq!(r.len(), 3);
        assert!(!r.is_empty());
    }

    #[test]
    fn debug_lists_keys_without_evaluator_internals() {
        let r = reg(vec![("a", false)]);
        let s = format!("{r:?}");
        assert!(s.contains("CaveatEvaluatorRegistry"));
        assert!(s.contains("\"a\""));
    }

    #[test]
    fn registry_debug_and_default_are_consistent() {
        let r = CaveatEvaluatorRegistry::default();
        assert!(r.is_empty());
        assert_eq!(r.core_shadowing_keys().count(), 0);
    }
}
