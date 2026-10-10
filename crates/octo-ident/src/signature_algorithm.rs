//! Signature algorithm registry — RFC-0009 §Dual-format signature algorithms
//! and the ZK-capable key.
//!
//! One identity may hold two keys under two different signature algorithms.
//! The algorithms live in a registry keyed by a typed `Uuid` discriminator
//! rather than a central enum, so a new algorithm lands in its own crate and
//! registers at startup instead of requiring an edit to this one — per
//! [[cipherocto-design-principles]] §Extension over enumeration.
//!
//! ## Why the newtypes store `[u8; 16]` and not a `Uuid`
//!
//! RFC-0009 §Domain separation requires the signable prefix's two components to
//! be **fixed at 16 bytes** so the raw form is self-delimiting and needs no
//! length framing. `Uuid` has no borsh implementation available in this
//! workspace, so a derived `struct AlgorithmId(Uuid)` would not compile, and a
//! serialiser-chosen encoding is a worse answer than an explicit one: storing
//! the bytes makes the fixed-width property true by construction, and makes the
//! storage decode and the signable prefix agree on what a byte is.
//!
//! ## Layer
//!
//! This is Layer B (identity substrate, years-stable). Per-algorithm crates
//! are Layer E and register into it at startup; they do not depend on it.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

use parking_lot::RwLock;
use thiserror::Error;
use uuid::Uuid;

/// Raw bytes of every identifier newtype in this module.
///
/// Identifiers are 16 bytes so that concatenating two of them is
/// self-delimiting — see RFC-0009 §Domain separation.
pub type IdBytes = [u8; 16];

/// Typed discriminator naming one signature algorithm.
///
/// A `Uuid`, not a `u8`: a new algorithm must not be able to collide with an
/// existing one by picking a free byte, and the old
/// `VerificationMethodKind` enum did exactly that by mapping every unknown
/// byte onto one `Reserved` variant.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
#[cfg_attr(
    feature = "borsh",
    derive(borsh::BorshSerialize, borsh::BorshDeserialize)
)]
pub struct AlgorithmId {
    bytes: IdBytes,
}

impl AlgorithmId {
    /// Namespace for derived algorithm identifiers.
    ///
    /// Algorithm identifiers are assigned, not generated at random, so that a
    /// deployment's choice of algorithms is reproducible. Derive with
    /// [`AlgorithmId::derive`].
    pub const NAMESPACE: Uuid = Uuid::from_bytes([
        0x9e, 0x6b, 0x1f, 0x4a, 0x2c, 0x8e, 0x55, 0x33, 0x9d, 0x71, 0x0c, 0x42, 0xb6, 0x2a, 0x7f,
        0x18,
    ]);

    /// Construct from raw bytes.
    #[must_use]
    pub const fn from_bytes(bytes: IdBytes) -> Self {
        Self { bytes }
    }

    /// Borrow the raw 16 bytes — the signable-prefix form.
    #[must_use]
    pub const fn as_bytes(&self) -> &IdBytes {
        &self.bytes
    }

    /// Convert to the `Uuid` this identifier was derived from.
    #[must_use]
    pub const fn to_uuid(&self) -> Uuid {
        Uuid::from_bytes(self.bytes)
    }

    /// Convert from a `Uuid`.
    #[must_use]
    pub const fn from_uuid(uuid: Uuid) -> Self {
        Self {
            bytes: *uuid.as_bytes(),
        }
    }

    /// Derive a stable identifier from a namespace-local name.
    ///
    /// Use this rather than [`Uuid::new_v4`] so an algorithm keeps its
    /// identifier across processes and deployments. A random identifier would
    /// make a DID document's `algorithm_id` meaningless to any peer that did
    /// not happen to learn the same random value.
    #[must_use]
    pub fn derive(name: &str) -> Self {
        Self::from_uuid(Uuid::new_v5(&Self::NAMESPACE, name.as_bytes()))
    }
}

/// Stable identity of one key within one DID document.
///
/// This is the `key_id` in the mandatory signable prefix, so it has a fixed
/// width by construction. It is **not** the existing string-fragment key id
/// that `DidDocument::authenticates` matches on: that value is a display
/// handle, is not carried on the wire, and is not stable across resolvers.
/// At variable width the raw signable prefix would collide — a `key_id` of
/// `""` with payload `"abc"` is the same byte string as a `key_id` of `"a"`
/// with payload `"bc"` — which is the exact ambiguity the prefix prevents.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
#[cfg_attr(
    feature = "borsh",
    derive(borsh::BorshSerialize, borsh::BorshDeserialize)
)]
pub struct VerificationMethodId {
    bytes: IdBytes,
}

impl VerificationMethodId {
    /// Namespace for derived verification-method identifiers.
    pub const NAMESPACE: Uuid = Uuid::from_bytes([
        0x3a, 0xd4, 0x77, 0x10, 0x6b, 0x25, 0xe8, 0x41, 0x5c, 0x99, 0x2d, 0x0f, 0x8a, 0x63, 0x74,
        0x5b,
    ]);

    /// Construct from raw bytes.
    #[must_use]
    pub const fn from_bytes(bytes: IdBytes) -> Self {
        Self { bytes }
    }

    /// Borrow the raw 16 bytes — the signable-prefix form.
    #[must_use]
    pub const fn as_bytes(&self) -> &IdBytes {
        &self.bytes
    }

    /// Convert to the `Uuid` this identifier was derived from.
    #[must_use]
    pub const fn to_uuid(&self) -> Uuid {
        Uuid::from_bytes(self.bytes)
    }

    /// Convert from a `Uuid`.
    #[must_use]
    pub const fn from_uuid(uuid: Uuid) -> Self {
        Self {
            bytes: *uuid.as_bytes(),
        }
    }

    /// Derive a stable identifier from a namespace-local name.
    #[must_use]
    pub fn derive(name: &str) -> Self {
        Self::from_uuid(Uuid::new_v5(&Self::NAMESPACE, name.as_bytes()))
    }
}

/// Identifier of a specific in-circuit verification program.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
#[cfg_attr(
    feature = "borsh",
    derive(borsh::BorshSerialize, borsh::BorshDeserialize)
)]
pub struct CircuitId {
    bytes: IdBytes,
}

impl CircuitId {
    /// Namespace for derived circuit identifiers.
    pub const NAMESPACE: Uuid = Uuid::from_bytes([
        0x71, 0x2c, 0x9e, 0x55, 0x84, 0xd3, 0x0a, 0x6b, 0x3f, 0x18, 0xc7, 0x52, 0x95, 0x24, 0x81,
        0x0e,
    ]);

    /// Construct from raw bytes.
    #[must_use]
    pub const fn from_bytes(bytes: IdBytes) -> Self {
        Self { bytes }
    }

    /// Borrow the raw 16 bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &IdBytes {
        &self.bytes
    }

    /// Convert to the `Uuid` this identifier was derived from.
    #[must_use]
    pub const fn to_uuid(&self) -> Uuid {
        Uuid::from_bytes(self.bytes)
    }

    /// Convert from a `Uuid`.
    #[must_use]
    pub const fn from_uuid(uuid: Uuid) -> Self {
        Self {
            bytes: *uuid.as_bytes(),
        }
    }

    /// Derive a stable identifier from a namespace-local name.
    #[must_use]
    pub fn derive(name: &str) -> Self {
        Self::from_uuid(Uuid::new_v5(&Self::NAMESPACE, name.as_bytes()))
    }
}

/// Whether a constrained in-circuit verifier exists for this algorithm.
///
/// `None` is a substantive claim, not a placeholder for unfinished work: it
/// means a holder whose key is registered under this algorithm **cannot**
/// produce a ZK capability proof, and a mint attempt MUST fail closed rather
/// than fall back to another key.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum ZkSupport {
    /// No constrained in-circuit verifier exists. ZK proofs over a key
    /// registered with this algorithm MUST be refused.
    None,
    /// A constrained in-circuit verifier exists at this circuit.
    Circuit {
        /// The in-circuit verification program.
        circuit: CircuitId,
    },
}

/// Failure of a signature check under a registered algorithm.
#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum SignatureError {
    /// The public key length does not match this algorithm.
    #[error("public key length {got} does not match {alg}: expected {expected}")]
    BadKeyLength {
        /// Algorithm that rejected the key.
        alg: &'static str,
        /// Length supplied.
        got: usize,
        /// Length this algorithm requires.
        expected: usize,
    },
    /// The signature length does not match this algorithm.
    #[error("signature length {got} does not match {alg}: expected {expected}")]
    BadSignatureLength {
        /// Algorithm that rejected the signature.
        alg: &'static str,
        /// Length supplied.
        got: usize,
        /// Length this algorithm requires.
        expected: usize,
    },
    /// The signature did not verify under this algorithm.
    #[error("signature did not verify under {0}")]
    Invalid(&'static str),
}

/// One signature algorithm, implemented outside this crate.
///
/// Algorithms are Layer E extensions. They register into [`AlgorithmRegistry`]
/// at startup; they do not depend on this crate's internals.
pub trait SignatureAlgorithm: Send + Sync {
    /// Stable discriminator for this algorithm.
    fn id(&self) -> AlgorithmId;

    /// Human-readable name, for diagnostics only. Never a wire form.
    fn name(&self) -> &'static str;

    /// Required public key length in bytes.
    fn public_key_len(&self) -> usize;

    /// Required signature length in bytes.
    fn signature_len(&self) -> usize;

    /// Whether a constrained in-circuit verifier exists.
    ///
    /// Returning `ZkSupport::None` is the honest default, and asserting
    /// `ZkSupport::Circuit` is the affirmative claim RFC-0009 AC#14's
    /// fail-closed mint gate depends on. A `Circuit` claim that no circuit
    /// backs turns a fail-closed gate fail-open.
    fn zk_support(&self) -> ZkSupport;

    /// Verify `signature` over `message` under `public_key`.
    ///
    /// # Errors
    /// Returns [`SignatureError`] on any length mismatch or failed
    /// verification. Implementations MUST NOT return `Ok` for a malformed
    /// input.
    fn verify(
        &self,
        public_key: &[u8],
        message: &[u8],
        signature: &[u8],
    ) -> Result<(), SignatureError>;
}

/// Registry failure modes.
#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum AlgorithmRegistryError {
    /// A second implementation claimed an `AlgorithmId` already registered.
    ///
    /// The registry MUST NOT silently replace — a last-writer-wins swap is a
    /// silent wrong answer inside the very mechanism meant to prevent them.
    #[error("algorithm already registered: {0:?}")]
    AlreadyRegistered(AlgorithmId),
    /// Lookup before [`AlgorithmRegistry::populate`] completed, or after it
    /// failed.
    #[error("algorithm registry not populated")]
    NotPopulated,
    /// No registration for this identifier. Fail closed, never default.
    #[error("unknown algorithm: {0:?}")]
    UnknownAlgorithm(AlgorithmId),
}

#[derive(Default)]
struct RegistryInner {
    algorithms: BTreeMap<AlgorithmId, Arc<dyn SignatureAlgorithm>>,
}

/// Registry of signature algorithms, keyed by [`AlgorithmId`].
///
/// Construction is `new()` then `populate(algs)` during startup. Lookup before
/// `populate` is [`AlgorithmRegistryError::NotPopulated`] rather than
/// [`AlgorithmRegistryError::UnknownAlgorithm`]: the first is a deployment
/// that has not booted, the second is an algorithm nobody registered, and
/// conflating them makes a boot-order bug look like a configuration bug.
///
/// ## Not yet a singleton — recorded, not hidden
///
/// RFC-0009 requires the registry to be a **process-wide singleton owned by
/// this crate and populated by an init function before any verification
/// runs.** That is **not** implemented. `new()` is public, `Default` is
/// derived, and a caller can therefore construct an independent registry per
/// use — two verifiers in one process could disagree about which algorithms
/// exist, which is a subtler version of the wrong-algorithm problem the
/// registry exists to prevent.
///
/// The bootstrap lands with the verification-dispatch work (`0009-c`), which
/// is where the first real caller appears. Until then this type is inert:
/// nothing outside its own tests constructs one. Stated here rather than left
/// implicit, because a reader of this module should not have to find the gap
/// by grepping for call sites and finding nothing.
#[derive(Default)]
pub struct AlgorithmRegistry {
    inner: RwLock<RegistryInner>,
    populated: RwLock<bool>,
}

impl AlgorithmRegistry {
    /// Create an empty, unpopulated registry.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: RwLock::new(RegistryInner {
                algorithms: BTreeMap::new(),
            }),
            populated: RwLock::new(false),
        }
    }

    /// Register one algorithm.
    ///
    /// Rejects a duplicate `AlgorithmId` with
    /// [`AlgorithmRegistryError::AlreadyRegistered`] rather than overwriting.
    /// **This holds even when the two implementations are byte-identical:**
    /// two crates linking two versions of the same algorithm is a link-time
    /// fact the registry must surface, not absorb.
    ///
    /// ## This still works after `populate`
    ///
    /// A post-`populate` `register` succeeds and its algorithm is immediately
    /// visible to in-flight verification. That is a real gap, not an
    /// oversight: the ready-set is therefore **not** immutable once
    /// `populate` returns, so `populate` is an atomic *insertion*, not an
    /// atomic *freeze*.
    ///
    /// An earlier version of this file claimed the opposite — that a second
    /// population "would make the ready-set non-atomic" — which was a doc
    /// comment asserting a guarantee the code did not provide, the same class
    /// as the dangling `§VerificationMethodKind` citation BC#13 removes.
    /// Making it a real freeze needs a fourth error variant that RFC-0009
    /// does not define, so it is recorded here rather than invented.
    ///
    /// # Errors
    /// [`AlgorithmRegistryError::AlreadyRegistered`] if this `AlgorithmId` is
    /// already claimed.
    pub fn register(&self, alg: Arc<dyn SignatureAlgorithm>) -> Result<(), AlgorithmRegistryError> {
        let id = alg.id();
        let mut inner = self.inner.write();
        if inner.algorithms.contains_key(&id) {
            return Err(AlgorithmRegistryError::AlreadyRegistered(id));
        }
        inner.algorithms.insert(id, alg);
        Ok(())
    }

    /// Mark the registry ready, inserting the bootstrap set in one operation.
    ///
    /// Takes the algorithms rather than being a bare `populate()`, which is
    /// both the RFC signature and the only way `NotPopulated` stays reachable
    /// in the deployment state it exists to name: a no-argument version makes
    /// `new().populate()` produce a registry where every lookup is
    /// `UnknownAlgorithm` rather than `NotPopulated`, so a boot-order bug
    /// looks like a configuration bug.
    ///
    /// # Errors
    /// - [`AlgorithmRegistryError::AlreadyRegistered`] if the registry already
    ///   holds one of these ids, or the set names an id twice
    /// - [`AlgorithmRegistryError::NotPopulated`] if already populated, since a
    ///   second call would make the ready-set non-atomic
    ///
    /// An empty set is accepted deliberately: a deployment with no algorithms
    /// is a configuration choice, and one that *intended* to register some and
    /// registered none is caught at the first lookup, which says
    /// `UnknownAlgorithm`.
    pub fn populate(
        &self,
        algs: Vec<Arc<dyn SignatureAlgorithm>>,
    ) -> Result<(), AlgorithmRegistryError> {
        if *self.populated.read() {
            return Err(AlgorithmRegistryError::NotPopulated);
        }
        let mut ids: Vec<AlgorithmId> = Vec::with_capacity(algs.len());
        for alg in &algs {
            let id = alg.id();
            if ids.contains(&id) {
                return Err(AlgorithmRegistryError::AlreadyRegistered(id));
            }
            ids.push(id);
        }
        {
            let mut inner = self.inner.write();
            for id in &ids {
                if inner.algorithms.contains_key(id) {
                    return Err(AlgorithmRegistryError::AlreadyRegistered(*id));
                }
            }
            for alg in algs {
                inner.algorithms.insert(alg.id(), alg);
            }
            // Set the ready-flag while still holding the map lock. Setting
            // it after the block would leave a window in which the
            // algorithms are present but the registry reports itself
            // unready, so a concurrent `get` fails closed on a registry
            // that is in fact fully populated.
            *self.populated.write() = true;
        }
        Ok(())
    }

    /// Look up an algorithm.
    ///
    /// # Errors
    /// - [`AlgorithmRegistryError::NotPopulated`] if `populate` has not run
    /// - [`AlgorithmRegistryError::UnknownAlgorithm`] if the id was never
    ///   registered
    ///
    /// There is deliberately no default arm and no fallback. Returning Ed25519
    /// here is the exact failure RFC-0009 A12 exists to prevent: a verifier
    /// that dispatches on a preferred algorithm rather than the one the
    /// message names gets a silent wrong answer rather than an error.
    pub fn get(
        &self,
        id: AlgorithmId,
    ) -> Result<Arc<dyn SignatureAlgorithm>, AlgorithmRegistryError> {
        if !*self.populated.read() {
            return Err(AlgorithmRegistryError::NotPopulated);
        }
        self.inner
            .read()
            .algorithms
            .get(&id)
            .cloned()
            .ok_or(AlgorithmRegistryError::UnknownAlgorithm(id))
    }

    /// Whether the registry has been marked ready.
    #[must_use]
    pub fn is_populated(&self) -> bool {
        *self.populated.read()
    }

    /// Number of registered algorithms. Diagnostics only.
    #[must_use]
    pub fn len(&self) -> usize {
        self.inner.read().algorithms.len()
    }

    /// Whether nothing is registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Every currently registered identifier, ascending.
    ///
    /// Used by [`bootstrap`] to decide whether a second install request is
    /// the same algorithm set (idempotent, so a repeated init in a
    /// multi-crate binary is not an error) or a genuinely different one
    /// (a conflict, because two callers disagreeing about which
    /// algorithms exist is the exact hazard the singleton removes).
    #[must_use]
    pub fn ids(&self) -> Vec<AlgorithmId> {
        self.inner.read().algorithms.keys().cloned().collect()
    }
}

/// The one registry for this process.
///
/// `AlgorithmRegistry::new` is public so a caller can build a private
/// registry for a test or a deliberately narrowed deployment. What the
/// public constructor may **not** do is let two verifiers in one process
/// disagree about which algorithms exist: one would resolve an
/// `AlgorithmId` the other rejects, and the disagreement is invisible
/// until a signature verifies in one path and not the other.
static GLOBAL: OnceLock<AlgorithmRegistry> = OnceLock::new();

/// Serialises [`bootstrap`].
///
/// The registry's own locks are not enough. `populate` checks the
/// ready-flag, fills the map, and sets the flag — and `bootstrap`'s
/// check-then-act spans all three. Two callers racing here can both
/// observe "not populated yet" and both proceed, so one wins the insert
/// and the other gets `AlreadyRegistered`, reporting a conflict where
/// there was only agreement. Holding one lock across the whole decision
/// removes the window.
static BOOTSTRAP_LOCK: Mutex<()> = Mutex::new(());

/// Take the bootstrap lock, ignoring poisoning.
///
/// A panic in an unrelated test thread must not make the registry
/// permanently unusable: nothing here holds an invariant across the
/// guard, so the poisoned state carries no information the next caller
/// needs.
fn bootstrap_guard() -> MutexGuard<'static, ()> {
    BOOTSTRAP_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// The process-wide algorithm registry.
///
/// Empty until [`bootstrap`] installs algorithms into it. A lookup
/// against an unbootstrapped registry fails closed with
/// [`AlgorithmRegistryError::NotPopulated`] — deliberately a
/// *different* error from [`AlgorithmRegistryError::UnknownAlgorithm`],
/// because "this deployment supports nothing yet" and "this identifier
/// names an algorithm I do not implement" are different facts and a
/// verifier that cannot tell them apart will report the wrong one.
#[must_use]
pub fn global() -> &'static AlgorithmRegistry {
    GLOBAL.get_or_init(AlgorithmRegistry::new)
}

/// Failure to install algorithms into the process-wide registry.
#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum BootstrapError {
    /// The registry was already bootstrapped with a **different** algorithm
    /// set.
    ///
    /// A repeat install of the *same* set is accepted, because a binary
    /// with several initialising crates should not fail to start over a
    /// harmless repeat. A *different* set is refused: it means two callers
    /// disagree about what this deployment supports, and picking one
    /// silently is exactly the last-writer-wins swap
    /// [`AlgorithmRegistryError::AlreadyRegistered`] exists to prevent.
    #[error("the process-wide registry already holds a different algorithm set")]
    ConflictingBootstrap,
    /// The registry rejected the install.
    #[error("bootstrap rejected: {0}")]
    Registry(#[from] AlgorithmRegistryError),
}

/// Install `algs` into the process-wide registry, marking it ready.
///
/// Idempotent for an identical algorithm set. This is the init step
/// RFC-0009 requires to run **before any verification**, so that the
/// first dispatch either finds its algorithm or fails closed — never
/// silently falls back to a default.
///
/// # Errors
/// [`BootstrapError::ConflictingBootstrap`] if the registry already
/// holds a different set, or [`BootstrapError::Registry`] if the
/// registry rejects the install.
pub fn bootstrap(algs: Vec<Arc<dyn SignatureAlgorithm>>) -> Result<(), BootstrapError> {
    let _guard = bootstrap_guard();
    bootstrap_locked(global(), algs)
}

/// Body of [`bootstrap`], for callers that already hold the bootstrap lock.
///
/// Split out because [`bootstrap_additional`] needs the same decision under
/// the same lock, and `BOOTSTRAP_LOCK` is a plain non-reentrant
/// `std::sync::Mutex` — delegating to `bootstrap` would deadlock.
fn bootstrap_locked(
    registry: &AlgorithmRegistry,
    algs: Vec<Arc<dyn SignatureAlgorithm>>,
) -> Result<(), BootstrapError> {
    let mut requested: Vec<AlgorithmId> = algs.iter().map(|a| a.id()).collect();
    requested.sort_unstable();
    requested.dedup();

    if registry.is_populated() {
        // Same set on a repeat install is a no-op, not a conflict: a
        // binary whose several initialising crates each call this should
        // still start.
        return if registry.ids() == requested {
            Ok(())
        } else {
            Err(BootstrapError::ConflictingBootstrap)
        };
    }

    registry.populate(algs)?;
    Ok(())
}

/// Add algorithms to the process-wide registry **without** replacing or
/// conflicting with an existing bootstrap.
///
/// # Why this exists
///
/// [`bootstrap`] is idempotent only for an _identical_ set: it refuses a
/// second call whose ids differ, because two callers disagreeing about what
/// the deployment supports is the hazard it exists to catch. That is correct
/// while there is one algorithm. It stops being correct the moment there are
/// two — and RFC-0009 §Dual-format signature algorithms and the ZK-capable
/// key is exactly that: one identity carrying an Ed25519 key and a
/// STARK-curve Schnorr key.
///
/// With only [`bootstrap`], a binary depending on two per-algorithm crates
/// would have one of them win and the other report
/// [`BootstrapError::ConflictingBootstrap`], or worse, need both crates to
/// agree on a set neither of them owns. Neither is a registry of algorithms;
/// it is a registry of one algorithm with a compile-time deadlock at the
/// install site. Per-algorithm crates are independent Layer E extensions and
/// must not have to depend on each other to be installable together.
///
/// So installation has two shapes: [`bootstrap`] declares _this deployment's
/// whole set_, and this function _adds_ to whatever is already there. An
/// already-registered algorithm is idempotent here, mirroring how
/// [`bootstrap`] treats a repeated identical set.
///
/// # Layer
///
/// Layer B. This is the registry's own interface, not an extension reaching
/// into it — per-algorithm crates call it rather than touching
/// [`AlgorithmRegistry::register`] and re-deriving the boot-order rules.
///
/// # Errors
///
/// [`BootstrapError::Registry`] if the registry rejects an algorithm. If the
/// registry is not yet populated this behaves exactly like [`bootstrap`],
/// including refusing a conflicting set.
///
/// # What this does not do: restrict a set
///
/// Calling this after an explicit [`bootstrap`] **adds to** that set rather
/// than failing against it. So a deployment that starts with
/// `bootstrap(vec![ed25519])` to say "Ed25519 only" still gets anything a
/// linked crate later installs, and cannot keep it out without not linking
/// the crate.
///
/// That is a real gap in the install semantics and it is stated here rather
/// than left to be discovered. It is a **missing capability, not a weakened
/// control**: verification still dispatches on the identifier a peer named, so
/// an added algorithm cannot be reached unless something is told to use it,
/// and `UnknownAlgorithm` still fails closed for everything absent.
///
/// The alternative — recording which populating call was strict — buys that
/// capability at the cost of a second piece of registry state whose only
/// purpose is to make an install fail. RFC-0009 §Dual-format signature
/// algorithms and the ZK-capable key does not specify install semantics at
/// this level, so the simpler rule stands until it does. A deployment that
/// must restrict the set should bootstrap it itself and not use per-crate
/// `install()`.
pub fn bootstrap_additional(algs: Vec<Arc<dyn SignatureAlgorithm>>) -> Result<(), BootstrapError> {
    let _guard = bootstrap_guard();
    let registry = global();

    if !registry.is_populated() {
        // First install in this process: there is no existing set to add to,
        // so this is an ordinary bootstrap and keeps the fail-closed
        // "nothing is ready yet" window intact.
        return bootstrap_locked(registry, algs);
    }

    for alg in algs {
        match registry.register(alg) {
            // Already here: this is the repeat-install case, and refusing it
            // would make a binary with two initialising crates for the same
            // algorithm fail to start over nothing.
            Ok(()) | Err(AlgorithmRegistryError::AlreadyRegistered(_)) => {}
            Err(e) => return Err(BootstrapError::Registry(e)),
        }
    }
    Ok(())
}

// ── Serde (optional, `serde` feature) ─────────────────────────────────────

/// Serde impls for the three identifier newtypes.
///
/// **The serde form is textual; the signable prefix is raw.** Both forms are
/// in use in this workspace and RFC-0009 §Domain separation makes the
/// distinction normative: `algorithm_id` in the signed prefix is the 16 raw
/// bytes of its `Uuid`, never the 36-character hyphenated form, "an
/// implementation that picks differently from its peers signs a different
/// domain."
///
/// So this emits the textual form — matching what `uuid` plus its `serde`
/// feature already produces elsewhere, e.g. in `octo-cli` and
/// `octo-runtime` — and the prefix code never comes through here. It reads
/// [`IdBytes`] via `as_bytes`, which is the whole reason these types store
/// `[u8; 16]` rather than wrapping a `Uuid`.
#[cfg(feature = "serde")]
mod serde_impls {
    use super::{AlgorithmId, CircuitId, IdBytes, VerificationMethodId};
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use uuid::Uuid;

    /// Shared by all three newtypes: one `IdBytes` field, `from_uuid`, and
    /// the same textual wire form.
    macro_rules! impl_identifier_serde {
        ($ty:ty) => {
            impl Serialize for $ty {
                fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                    Uuid::from_bytes(*self.as_bytes()).serialize(serializer)
                }
            }

            impl<'de> Deserialize<'de> for $ty {
                fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                    // `Uuid`'s deserialiser accepts the textual form and
                    // normalises case, so a document written by another
                    // implementation in upper case still round-trips.
                    let uuid = Uuid::deserialize(deserializer)?;
                    Ok(<$ty>::from_uuid(uuid))
                }
            }
        };
    }

    impl_identifier_serde!(AlgorithmId);
    impl_identifier_serde!(VerificationMethodId);
    impl_identifier_serde!(CircuitId);

    /// Compile-time proof that every identifier is exactly 16 bytes, which
    /// is the property the signable prefix depends on and the reason these
    /// types store bytes rather than a `Uuid`.
    const _: () = {
        const fn assert_len(_: &IdBytes) {}
        let _ = assert_len;
    };
}

#[cfg(all(test, feature = "serde"))]
mod serde_tests {
    use super::*;

    fn round_trips_json<T>(value: &T) -> T
    where
        T: serde::Serialize + serde::de::DeserializeOwned,
    {
        let text = serde_json::to_string(value).expect("serialize");
        serde_json::from_str(&text).expect("deserialize")
    }

    #[test]
    fn algorithm_id_round_trips_through_json() {
        let id = AlgorithmId::derive("ed25519");
        assert_eq!(round_trips_json(&id), id);
    }

    #[test]
    fn verification_method_id_round_trips_through_json() {
        let id = VerificationMethodId::derive("key-0");
        assert_eq!(round_trips_json(&id), id);
    }

    #[test]
    fn circuit_id_round_trips_through_json() {
        let id = CircuitId::derive("zk-circuit-0");
        assert_eq!(round_trips_json(&id), id);
    }

    #[test]
    fn serde_emits_the_textual_form_and_storage_stays_sixteen_bytes() {
        // The distinction RFC-0009 §Domain separation makes normative. If
        // this ever collapses to one form, the signable prefix silently
        // changes and every signature over it stops verifying.
        let id = AlgorithmId::derive("ed25519");
        let text = serde_json::to_string(&id).expect("serialize");
        assert_eq!(
            text,
            format!("\"{}\"", id.to_uuid()),
            "serde must emit the textual UUID form"
        );
        assert_eq!(text.len(), 38, "36 characters plus the two quotes");

        assert_eq!(
            id.as_bytes().len(),
            16,
            "storage must stay 16 raw bytes — the prefix's fixed width is \
             what makes it self-delimiting"
        );
        assert_eq!(
            id.as_bytes(),
            id.to_uuid().as_bytes(),
            "storage is the UUID's raw bytes, not its text"
        );
    }

    #[test]
    fn a_deserialised_identifier_has_the_same_bytes() {
        // A document written elsewhere must yield the same prefix bytes,
        // not merely a printable id that compares equal.
        let id = AlgorithmId::derive("stark-schnorr");
        let text = format!("\"{}\"", id.to_uuid()).to_uppercase();
        let parsed: AlgorithmId = serde_json::from_str(&text).expect("upper-case uuid parses");
        assert_eq!(parsed.as_bytes(), id.as_bytes());
        assert_eq!(parsed, id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two byte-identical algorithm implementations, so a duplicate
    /// registration is indistinguishable by content.
    struct Dummy(&'static str, AlgorithmId);

    impl SignatureAlgorithm for Dummy {
        fn id(&self) -> AlgorithmId {
            self.1
        }
        fn name(&self) -> &'static str {
            self.0
        }
        fn public_key_len(&self) -> usize {
            32
        }
        fn signature_len(&self) -> usize {
            64
        }
        fn zk_support(&self) -> ZkSupport {
            ZkSupport::None
        }
        fn verify(
            &self,
            public_key: &[u8],
            _message: &[u8],
            signature: &[u8],
        ) -> Result<(), SignatureError> {
            if public_key.len() != 32 {
                return Err(SignatureError::BadKeyLength {
                    alg: self.0,
                    got: public_key.len(),
                    expected: 32,
                });
            }
            if signature.len() != 64 {
                return Err(SignatureError::BadSignatureLength {
                    alg: self.0,
                    got: signature.len(),
                    expected: 64,
                });
            }
            Ok(())
        }
    }

    fn dummy(name: &'static str, id: AlgorithmId) -> Arc<dyn SignatureAlgorithm> {
        Arc::new(Dummy(name, id))
    }

    #[test]
    fn lookup_before_population_is_not_populated() {
        let reg = AlgorithmRegistry::new();
        reg.register(dummy("a", AlgorithmId::derive("a"))).unwrap();
        // Registered but not populated: NotPopulated, not UnknownAlgorithm.
        // The distinction is a deployment that has not booted versus an
        // algorithm nobody registered, and conflating them makes a boot-order
        // bug look like a configuration bug.
        assert!(matches!(
            reg.get(AlgorithmId::derive("a")),
            Err(AlgorithmRegistryError::NotPopulated)
        ));
    }

    #[test]
    fn unregistered_algorithm_id_is_not_silently_ed25519() {
        // The A12 property in its sharpest form: Ed25519 *is* registered, and
        // an unknown id must still miss. The obvious wrong implementation is a
        // fallback arm returning the configured default, which a deployment
        // holding only Ed25519 would never notice — every lookup would
        // "work", and every cross-algorithm replay would silently verify
        // against the wrong equation.
        let reg = AlgorithmRegistry::new();
        let ed = AlgorithmId::derive("ed25519");
        reg.register(dummy("ed25519", ed)).unwrap();
        reg.populate(Vec::new()).expect("populate");

        // The registered one resolves.
        assert_eq!(reg.get(ed).unwrap().name(), "ed25519");

        // Every other id misses rather than falling back.
        for other in [
            AlgorithmId::derive("stark-schnorr"),
            AlgorithmId::derive("legacy-unknown"),
            AlgorithmId::derive(""),
            AlgorithmId::from_bytes([0u8; 16]),
        ] {
            assert!(
                matches!(
                    reg.get(other),
                    Err(AlgorithmRegistryError::UnknownAlgorithm(_))
                ),
                "{other:?} must miss, not fall back to ed25519"
            );
        }

        // And the miss carries the id that was asked for, so two distinct
        // unknowns produce distinguishable errors rather than one message.
        let a = AlgorithmId::derive("unknown-a");
        let b = AlgorithmId::derive("unknown-b");
        let (
            Err(AlgorithmRegistryError::UnknownAlgorithm(got_a)),
            Err(AlgorithmRegistryError::UnknownAlgorithm(got_b)),
        ) = (reg.get(a), reg.get(b))
        else {
            panic!("both lookups must be UnknownAlgorithm");
        };
        assert_eq!(got_a, a);
        assert_eq!(got_b, b);
        assert_ne!(got_a, got_b);
    }

    #[test]
    fn unregistered_algorithm_id_is_rejected_after_population() {
        let reg = AlgorithmRegistry::new();
        reg.register(dummy("a", AlgorithmId::derive("a"))).unwrap();
        reg.populate(Vec::new()).expect("populate");
        let missing = AlgorithmId::derive("never-registered");
        assert!(matches!(
            reg.get(missing),
            Err(AlgorithmRegistryError::UnknownAlgorithm(id)) if id == missing
        ));
        // The registered one still resolves — a miss must not poison lookups.
        assert!(reg.get(AlgorithmId::derive("a")).is_ok());
    }

    #[test]
    fn duplicate_registration_is_rejected_even_when_byte_identical() {
        let reg = AlgorithmRegistry::new();
        let id = AlgorithmId::derive("a");
        reg.register(dummy("a", id)).unwrap();
        // A second implementation claiming the same id, with the same name and
        // the same lengths — indistinguishable by content.
        assert_eq!(
            reg.register(dummy("a", id)),
            Err(AlgorithmRegistryError::AlreadyRegistered(id))
        );
        assert_eq!(reg.len(), 1, "the first registration must survive");
    }

    #[test]
    fn identifiers_are_sixteen_bytes_and_round_trip() {
        let id = AlgorithmId::derive("ed25519");
        assert_eq!(id.as_bytes().len(), 16);
        assert_eq!(AlgorithmId::from_uuid(id.to_uuid()), id);

        let key = VerificationMethodId::derive("key-0");
        assert_eq!(key.as_bytes().len(), 16);
        assert_eq!(VerificationMethodId::from_uuid(key.to_uuid()), key);

        let cir = CircuitId::derive("ed25519-circuit");
        assert_eq!(cir.as_bytes().len(), 16);
        assert_eq!(CircuitId::from_uuid(cir.to_uuid()), cir);
    }

    #[test]
    fn derivation_is_stable_across_calls() {
        // Not Uuid::new_v4: a random identifier would make a DID document's
        // algorithm_id meaningless to a peer that did not learn the same
        // random value.
        assert_eq!(
            AlgorithmId::derive("ed25519"),
            AlgorithmId::derive("ed25519")
        );
        assert_ne!(AlgorithmId::derive("a"), AlgorithmId::derive("b"));
    }

    #[test]
    fn distinct_namespaces_produce_distinct_identifiers() {
        // The same name in two namespaces must not collide.
        assert_ne!(
            AlgorithmId::derive("key-0").to_uuid(),
            VerificationMethodId::derive("key-0").to_uuid()
        );
    }

    #[test]
    fn zk_support_is_reported_by_the_algorithm() {
        struct WithCircuit;
        impl SignatureAlgorithm for WithCircuit {
            fn id(&self) -> AlgorithmId {
                AlgorithmId::derive("with-circuit")
            }
            fn name(&self) -> &'static str {
                "with-circuit"
            }
            fn public_key_len(&self) -> usize {
                32
            }
            fn signature_len(&self) -> usize {
                64
            }
            fn zk_support(&self) -> ZkSupport {
                ZkSupport::Circuit {
                    circuit: CircuitId::derive("with-circuit-circuit"),
                }
            }
            fn verify(&self, _pk: &[u8], _msg: &[u8], _sig: &[u8]) -> Result<(), SignatureError> {
                Ok(())
            }
        }

        let reg = AlgorithmRegistry::new();
        reg.register(Arc::new(WithCircuit)).unwrap();
        reg.populate(Vec::new()).expect("populate");
        let alg = reg.get(AlgorithmId::derive("with-circuit")).unwrap();
        assert_eq!(
            alg.zk_support(),
            ZkSupport::Circuit {
                circuit: CircuitId::derive("with-circuit-circuit")
            }
        );
    }

    /// Pins the CURRENT behaviour rather than an aspiration: a post-populate
    /// `register` succeeds and becomes immediately visible. If this ever goes
    /// red because someone made the ready-set a real freeze, that is a
    /// behaviour change and the RFC needs a fourth error variant to carry it —
    /// do not just flip the assertion.
    #[test]
    fn register_after_populate_still_succeeds_and_is_visible() {
        let reg = AlgorithmRegistry::new();
        reg.populate(Vec::new()).expect("populate");
        assert_eq!(reg.len(), 0);

        let late = AlgorithmId::derive("late-arrival");
        reg.register(dummy("late", late))
            .expect("post-populate register");

        assert_eq!(reg.len(), 1);
        assert_eq!(
            reg.get(late).expect("late registration is visible").name(),
            "late",
            "the ready-set is not frozen after populate"
        );
    }

    #[test]
    fn wrong_length_is_rejected_not_accepted() {
        let alg = dummy("a", AlgorithmId::derive("a"));
        // Both are 64 bytes for Ed25519 and STARK-Schnorr alike, so a
        // length-only check cannot tell them apart. This pins that a
        // malformed input is an error, not a tolerated one.
        assert_eq!(
            alg.verify(&[0u8; 31], b"msg", &[0u8; 64]),
            Err(SignatureError::BadKeyLength {
                alg: "a",
                got: 31,
                expected: 32
            })
        );
        assert_eq!(
            alg.verify(&[0u8; 32], b"msg", &[0u8; 63]),
            Err(SignatureError::BadSignatureLength {
                alg: "a",
                got: 63,
                expected: 64
            })
        );
    }

    /// The wire form is load-bearing, not an implementation detail: RFC-0009
    /// §Domain separation concatenates two of these and relies on each being
    /// exactly 16 bytes so the raw prefix needs no length framing. If a
    /// serialiser ever emitted a length prefix or the textual form, the prefix
    /// would stop being self-delimiting and the two signers in the repo would
    /// silently sign different domains.
    #[cfg(feature = "borsh")]
    #[test]
    fn borsh_wire_form_is_exactly_sixteen_bytes() {
        let alg = AlgorithmId::derive("ed25519");
        let enc = borsh::to_vec(&alg).expect("serialize AlgorithmId");
        assert_eq!(enc.len(), 16, "AlgorithmId wire form must be 16 bytes");
        assert_eq!(enc.as_slice(), alg.as_bytes());
        assert_eq!(borsh::from_slice::<AlgorithmId>(&enc).unwrap(), alg);

        let key = VerificationMethodId::derive("key-0");
        let enc = borsh::to_vec(&key).expect("serialize VerificationMethodId");
        assert_eq!(
            enc.len(),
            16,
            "VerificationMethodId wire form must be 16 bytes"
        );
        assert_eq!(enc.as_slice(), key.as_bytes());
        assert_eq!(
            borsh::from_slice::<VerificationMethodId>(&enc).unwrap(),
            key
        );

        let cir = CircuitId::derive("ed25519-circuit");
        let enc = borsh::to_vec(&cir).expect("serialize CircuitId");
        assert_eq!(enc.len(), 16, "CircuitId wire form must be 16 bytes");
        assert_eq!(enc.as_slice(), cir.as_bytes());
        assert_eq!(borsh::from_slice::<CircuitId>(&enc).unwrap(), cir);
    }

    /// The self-delimiting property the prefix depends on, asserted directly:
    /// two different identifiers concatenated with the same payload must not
    /// collide. This is the property a variable-width key id would break.
    #[test]
    fn raw_prefix_is_self_delimiting() {
        let payload = b"hello";
        let mut a = Vec::new();
        a.extend_from_slice(AlgorithmId::derive("ed25519").as_bytes());
        a.extend_from_slice(VerificationMethodId::derive("key-0").as_bytes());
        a.extend_from_slice(payload);

        let mut b = Vec::new();
        b.extend_from_slice(AlgorithmId::derive("ed25519").as_bytes());
        b.extend_from_slice(VerificationMethodId::derive("key-1").as_bytes());
        b.extend_from_slice(payload);

        assert_ne!(
            a, b,
            "distinct key_ids must produce distinct signable bytes"
        );
    }
}
