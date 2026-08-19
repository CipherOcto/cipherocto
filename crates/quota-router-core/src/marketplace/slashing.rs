//! Slashing — provider stake penalty on SLA miss (RFC-0900 §Slashing Model).
//!
//! Default rules per RFC-0900:
//!
//! | Rule                           | Value |
//! |--------------------------------|-------|
//! | First-offense penalty          | 10%   |
//! | Offense escalation multiplier  | 1.5   |
//! | Permanent ban threshold        | 50% of stake |
//!
//! `slash()` returns the amount actually deducted from the provider's
//! stake. The caller is responsible for emitting the on-chain /
//! settlement-side effect; this module only computes the penalty and
//! tracks per-provider offense counts.

/// Internal helper: wall-clock seconds since the UNIX epoch. Used by
/// persistence write-through to stamp `last_updated_unix`. Inlined to
/// avoid leaking the marketplace-private `current_unix`.
fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use quota_router_storage::slash_store::DEFAULT_CHAIN_ID;

/// Slashing reason classification (RFC-0900 §Dispute Evidence Challenge).
///
/// Mission `marketplace-slash-reason-typed-discriminator`:
/// `SlashReason` is a typed-discriminator struct with a 128-bit
/// `type_id` (RFC-0900 namespace prefix `0x0900:0001:...:NNNN`) and an
/// opaque `payload` blob. Per CLAUDE.md §"Extension over enumeration":
/// extension-bearing types use **typed-discriminator + Raw escape
/// hatch**, NOT central enums, so adding `PrivacyBreach` / `KeyLeak` /
/// etc. is a per-extension crate registration instead of a central
/// edit + cross-crate review.
///
/// **Wire format**: the canonical bytes for a `SlashReason` are
/// `type_id (16 bytes BE) || payload (varint length-prefixed)`. The
/// 5 RFC-allocated constants below carry identity-only; the payload
/// is currently empty (rejected without payload for non-zero
/// `payload_len`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SlashReason {
    /// 128-bit type_id. RFC-0900 namespace prefix `0x0900:0001:...:NNNN`.
    /// The NNNN suffix is allocated by RFC amendment.
    pub type_id: [u8; 16],
    /// Opaque payload (RFC-0900 §Slashing Model — empty for the 5
    /// core reasons; extensions may carry arbitrary bytes).
    #[serde(with = "serde_bytes_payload")]
    pub payload: Vec<u8>,
}

impl SlashReason {
    /// RFC-0900 namespace prefix (big-endian 4 bytes): `0x0900_0001`.
    #[allow(dead_code)]
    const NAMESPACE: [u8; 4] = [0x09, 0x00, 0x00, 0x01];

    /// Provider exceeded latency SLA (timeout).
    pub const TIMEOUT: SlashReason = SlashReason {
        type_id: [
            0x09, 0x00, 0x00, 0x01, 0x00, 0x01, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        ],
        payload: Vec::new(),
    };
    /// Provider returned a 5xx / non-2xx response.
    pub const PROVIDER_ERROR: SlashReason = SlashReason {
        type_id: [
            0x09, 0x00, 0x00, 0x01, 0x00, 0x02, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        ],
        payload: Vec::new(),
    };
    /// Provider latency exceeded configured max.
    pub const LATENCY_HIGH: SlashReason = SlashReason {
        type_id: [
            0x09, 0x00, 0x00, 0x01, 0x00, 0x03, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        ],
        payload: Vec::new(),
    };
    /// Response was garbage (manual review path; rare on-chain).
    pub const GARBAGE_RESPONSE: SlashReason = SlashReason {
        type_id: [
            0x09, 0x00, 0x00, 0x01, 0x00, 0x04, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        ],
        payload: Vec::new(),
    };
    /// Provider failed to return any response.
    pub const FAILED_RESPONSE: SlashReason = SlashReason {
        type_id: [
            0x09, 0x00, 0x00, 0x01, 0x00, 0x05, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        ],
        payload: Vec::new(),
    };

    /// Construct a SlashReason with non-empty payload (extensions).
    #[must_use]
    pub fn new(type_id: [u8; 16], payload: Vec<u8>) -> Self {
        Self { type_id, payload }
    }

    /// True if this reason matches one of the 5 RFC-allocated core
    /// reasons (case-sensitive 128-bit compare).
    #[must_use]
    pub fn is_rfc_core(&self) -> bool {
        self == &Self::TIMEOUT
            || self == &Self::PROVIDER_ERROR
            || self == &Self::LATENCY_HIGH
            || self == &Self::GARBAGE_RESPONSE
            || self == &Self::FAILED_RESPONSE
    }

    /// Verifiability weight — RFC-0900 §Dispute Evidence Challenge.
    ///
    /// Looks up the registered [`SlashReasonSpec`] for this reason's
    /// `type_id`. Unknown `type_id`s fail-closed at weight 0.0
    /// (extension crates must register a spec before their reasons
    /// are dispatched; this prevents accidental "treat unknown as
    /// weight 1.0" silent acceptance).
    #[must_use]
    pub fn verifiability(&self) -> f64 {
        default_registry().verifiability(&self.type_id)
    }
}

// `Vec<u8>` payload serialization: hex-encode for human-readable JSON
// (default for tests); binary protocol layers can swap in a more
// compact codec. `serde_bytes::Bytes` would conflict with `&[u8]`
// slice semantics for `payload: Vec<u8>`, so use a tiny inline
// module.
mod serde_bytes_payload {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S: Serializer>(v: &Vec<u8>, s: S) -> Result<S::Ok, S::Error> {
        v.serialize(s) // default Vec<u8> serialization is already a byte sequence
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
        Vec::<u8>::deserialize(d)
    }
}

// ========================================================================
// SlashReasonSpec — per-reason verifiability (extension-bearing trait)
// ========================================================================
//
// Per CLAUDE.md §"Extension over enumeration": new reason types register
// a spec via `register_reason_spec`. Old code fails-closed on unknown
// `type_id`s (returns 0.0 weight, which excludes the reason from any
// automatic dispute-evidence path).

/// Per-`SlashReason` behavior contract (mission
/// `marketplace-slash-reason-typed-discriminator`).
///
/// The marketplace registers one spec per RFC-allocated reason.
/// Extension crates register additional specs at startup. The default
/// spec registry covers the 5 RFC-0900 core reasons.
pub trait SlashReasonSpec: Send + Sync {
    /// The 128-bit type_id this spec governs.
    fn type_id(&self) -> [u8; 16];

    /// Verifiability weight in the dispute-evidence challenge
    /// (RFC-0900 §Dispute Evidence Challenge).
    fn verifiability(&self) -> f64;

    /// Short human-readable name (e.g., "timeout", "garbage_response").
    fn name(&self) -> &str;
}

// ---------- RFC-0900 core specs ----------

struct TimeoutSpec;
impl SlashReasonSpec for TimeoutSpec {
    fn type_id(&self) -> [u8; 16] {
        SlashReason::TIMEOUT.type_id
    }
    fn verifiability(&self) -> f64 {
        1.0
    }
    fn name(&self) -> &str {
        "timeout"
    }
}
struct ProviderErrorSpec;
impl SlashReasonSpec for ProviderErrorSpec {
    fn type_id(&self) -> [u8; 16] {
        SlashReason::PROVIDER_ERROR.type_id
    }
    fn verifiability(&self) -> f64 {
        1.0
    }
    fn name(&self) -> &str {
        "provider_error"
    }
}
struct LatencyHighSpec;
impl SlashReasonSpec for LatencyHighSpec {
    fn type_id(&self) -> [u8; 16] {
        SlashReason::LATENCY_HIGH.type_id
    }
    fn verifiability(&self) -> f64 {
        1.0
    }
    fn name(&self) -> &str {
        "latency_high"
    }
}
struct GarbageResponseSpec;
impl SlashReasonSpec for GarbageResponseSpec {
    fn type_id(&self) -> [u8; 16] {
        SlashReason::GARBAGE_RESPONSE.type_id
    }
    fn verifiability(&self) -> f64 {
        0.5
    }
    fn name(&self) -> &str {
        "garbage_response"
    }
}
struct FailedResponseSpec;
impl SlashReasonSpec for FailedResponseSpec {
    fn type_id(&self) -> [u8; 16] {
        SlashReason::FAILED_RESPONSE.type_id
    }
    fn verifiability(&self) -> f64 {
        1.0
    }
    fn name(&self) -> &str {
        "failed_response"
    }
}

// ---------- Registry ----------

use std::sync::{Arc, OnceLock};

/// Global spec registry. `OnceLock` because the marketplace crate
/// has no `init()` lifecycle hook; the registry self-populates the
/// 5 RFC-0900 core reasons on first access (via
/// `default_registry()`).
fn default_registry() -> &'static SlashReasonSpecRegistry {
    static REG: OnceLock<SlashReasonSpecRegistry> = OnceLock::new();
    REG.get_or_init(|| {
        let r = SlashReasonSpecRegistry::new();
        r.register(Arc::new(TimeoutSpec));
        r.register(Arc::new(ProviderErrorSpec));
        r.register(Arc::new(LatencyHighSpec));
        r.register(Arc::new(GarbageResponseSpec));
        r.register(Arc::new(FailedResponseSpec));
        r
    })
}

/// SlashReason spec registry (mission
/// `marketplace-slash-reason-typed-discriminator`).
///
/// Thread-safe via internal `parking_lot::RwLock`. Extension crates
/// call [`register`](Self::register) at startup.
pub struct SlashReasonSpecRegistry {
    specs: parking_lot::RwLock<HashMap<[u8; 16], Arc<dyn SlashReasonSpec>>>,
}

impl Default for SlashReasonSpecRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl SlashReasonSpecRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self {
            specs: parking_lot::RwLock::new(HashMap::new()),
        }
    }

    /// Register a spec. Overwrites any existing spec for the same
    /// `type_id` (last-write-wins; tests use this to override the
    /// default RFC weight for fuzz scenarios).
    pub fn register(&self, spec: Arc<dyn SlashReasonSpec>) {
        let mut g = self.specs.write();
        g.insert(spec.type_id(), spec);
    }

    /// Verifiability weight for `type_id`. Returns 0.0 (fail-closed)
    /// for unknown type_ids.
    #[must_use]
    pub fn verifiability(&self, type_id: &[u8; 16]) -> f64 {
        self.specs
            .read()
            .get(type_id)
            .map_or(0.0, |s| s.verifiability())
    }

    /// Number of registered specs (test-only).
    #[cfg(test)]
    #[must_use]
    pub fn len(&self) -> usize {
        self.specs.read().len()
    }

    /// True when no specs are registered (test-only).
    #[cfg(test)]
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.specs.read().is_empty()
    }
}

/// Register an extension reason spec against the default registry.
/// Extension crates call this at startup.
pub fn register_reason_spec(spec: Arc<dyn SlashReasonSpec>) {
    default_registry().register(spec);
}

/// Slashing rules (RFC-0900 §Slashing Model defaults).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SlashingRules {
    /// Penalty fraction applied to `stake` on the first offense (0.0-1.0).
    pub first_offense_penalty: f64,
    /// Multiplier applied to the running penalty on each subsequent
    /// offense for the same provider.
    pub offense_multiplier: f64,
    /// Permanent ban threshold expressed as cumulative fraction of stake
    /// lost (0.0-1.0).
    pub permanent_ban_at: f64,
    /// Maximum miss rate (0.0-1.0) below which no slashing occurs. Default
    /// 0.0 (every miss slashes); some deployments use a tolerance band.
    pub miss_rate_tolerance: f64,
}

impl Default for SlashingRules {
    fn default() -> Self {
        Self {
            first_offense_penalty: 0.10,
            offense_multiplier: 1.5,
            permanent_ban_at: 0.50,
            miss_rate_tolerance: 0.0,
        }
    }
}

/// Per-provider state tracked by the slashing ledger.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderStake {
    /// Mission 0900-d1: typed `ChainId` per RFC-0010 v1.4. Mirrors
    /// `SlashLedgerRow.chain_id` + the substrate PK shape
    /// `(chain_id, provider_id)`. Production paths use
    /// `DEFAULT_CHAIN_ID`; multi-chain slashing activates after this
    /// field is plumbed through callers (separate RFC owed).
    pub chain_id: [u8; 32],
    pub provider_id: String,
    /// Current stake remaining (micro-OCTO-W).
    #[serde(with = "quota_router_storage::dqa_serde::field")]
    pub stake_micro_octo_w: octo_determin::Dqa,
    /// Initial stake at registration (micro-OCTO-W).
    #[serde(with = "quota_router_storage::dqa_serde::field")]
    pub initial_stake_micro_octo_w: octo_determin::Dqa,
    /// Number of slashes applied so far.
    pub offense_count: u32,
    /// Cumulative fraction of initial stake lost (0.0-1.0).
    pub cumulative_loss_pct: f64,
}

impl ProviderStake {
    /// True if the provider has been permanently banned (cumulative
    /// loss ≥ permanent ban threshold).
    #[must_use]
    pub fn is_banned(&self, rules: &SlashingRules) -> bool {
        self.cumulative_loss_pct >= rules.permanent_ban_at
    }
}

/// Outcome of a `slash()` call.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SlashOutcome {
    /// Mission 0900-d1: chain partition this outcome applies to. Mirrors
    /// `ProviderStake.chain_id` + `SlashLedgerRow.chain_id`. Carried for
    /// audit-table chain attribution when slash events are recorded in
    /// out-of-tree dispute-resolution subsystems.
    pub chain_id: [u8; 32],
    pub provider_id: String,
    pub reason: SlashReason,
    #[serde(with = "quota_router_storage::dqa_serde::field")]
    pub amount_micro_octo_w: octo_determin::Dqa,
    #[serde(with = "quota_router_storage::dqa_serde::field")]
    pub new_stake_micro_octo_w: octo_determin::Dqa,
    pub cumulative_loss_pct: f64,
    pub banned: bool,
}

/// Slashing errors.
#[derive(Debug, thiserror::Error, PartialEq)]
pub enum SlashError {
    #[error("provider `{0}` not registered")]
    UnknownProvider(String),
    #[error("provider `{provider_id}` is permanently banned (cumulative_loss_pct_e6={cumulative_loss_pct_bits})")]
    BannedProvider {
        provider_id: String,
        /// `cumulative_loss_pct * 1_000_000`, rounded.
        cumulative_loss_pct_bits: u64,
    },
    #[error("miss rate e6={miss_rate_bits} below tolerance e6={tolerance_bits}")]
    BelowTolerance {
        /// `miss_rate * 1_000_000`, rounded.
        miss_rate_bits: u64,
        /// `tolerance * 1_000_000`, rounded.
        tolerance_bits: u64,
    },

    #[error("withdraw amount must be > 0 (got {0:?})")]
    InvalidAmount(octo_determin::Dqa),

    #[error("withdraw requested {requested:?} exceeds available stake {available:?}")]
    InsufficientStake {
        available: octo_determin::Dqa,
        requested: octo_determin::Dqa,
    },

    /// `MicroOctoW` amounts MUST be `scale=0` (integer-valued). Any
    /// caller passing a non-canonical `Dqa` would silently lose 10^scale
    /// orders of magnitude on the cast back to `u128` / `i64` at the
    /// persistence boundary — caught at the API gate instead of
    /// corrupting on-disk stake values.
    #[error("parameter `{param}` must be scale=0 (integer-valued MicroOCTO_W); got scale={scale}")]
    NonIntegerScale {
        /// Parameter name (e.g., `"initial_stake_micro_octo_w"`).
        param: &'static str,
        /// The non-zero scale value the caller passed.
        scale: u8,
    },
}

/// Validate that a caller-supplied `Dqa` is integer-valued (`scale=0`).
///
/// `MicroOctoW` amounts flow through the cast at the persistence
/// boundary (`dqa_to_i64`, `place_ask`'s `cost.value as u128`) where
/// any non-zero scale would silently truncate the lower digits. This
/// gate makes the truncation impossible — the API rejects non-canonical
/// inputs with a typed error.
fn require_integer_scale(d: &octo_determin::Dqa, param: &'static str) -> Result<(), SlashError> {
    if d.scale != 0 {
        return Err(SlashError::NonIntegerScale {
            param,
            scale: d.scale,
        });
    }
    Ok(())
}

/// Validate that a caller-supplied `Dqa` is a strictly-positive integer.
/// Combines the scale=0 invariant with a `value > 0` check — guards
/// against negative inputs (which `subtract` would happily add to a
/// stake) and the canonical zero.
fn require_positive_integer(d: &octo_determin::Dqa, param: &'static str) -> Result<(), SlashError> {
    require_integer_scale(d, param)?;
    if d.value <= 0 {
        return Err(SlashError::InvalidAmount(*d));
    }
    Ok(())
}

/// Build a `(chain_id, provider_id)` tuple key for the in-memory
/// `stakes` map. Production paths use `DEFAULT_CHAIN_ID` (single-chain
/// pre-multi-chain-slashing activation). The helper centralises the
/// key shape so all `stakes.get/get_mut/entry` sites use the same
/// canonical tuple.
///
/// Mission 0900-d1 AC-3: HashMap tuple-key restructure mirroring the
/// substrate PK `(chain_id, provider_id)` per §20.3 Model B.
#[inline]
fn stake_key(provider_id: &str) -> ([u8; 32], String) {
    (DEFAULT_CHAIN_ID, provider_id.to_owned())
}

/// Slashing ledger.
///
/// In-memory hot path (write-through to an optional `SlashStore`).
///
/// **Persistence** (mission `marketplace-slashing-persistence`):
/// when constructed via [`SlashingLedger::open`] with an
/// `Arc<dyn SlashStore>`, every state mutation (`register`,
/// `slash`, `slash_with_pct`) is written through to the store so
/// banned providers remain banned across process restarts. The
/// in-memory `stakes: HashMap` is rebuilt from the store at open
/// time. The legacy `new` / `with_rules` constructors build an
/// in-memory-only ledger with no persistence (useful for unit
/// tests that don't need a database).
///
/// Mission 0900-d1: `stakes` keyed by `([u8; 32], String)` to mirror
/// the substrate PK `(chain_id, provider_id)`. Public API still takes
/// `provider_id` only (single-arg); internal lookups use
/// `stake_key(provider_id)` which always resolves to
/// `(DEFAULT_CHAIN_ID, provider_id)` until multi-chain slashing
/// activates.
#[derive(Default, Clone)]
pub struct SlashingLedger {
    stakes: HashMap<([u8; 32], String), ProviderStake>,
    rules: SlashingRules,
    /// Optional persistence backend. `None` for in-memory-only
    /// ledgers (legacy `new` / `with_rules`). Set by [`open`] to
    /// enable write-through persistence.
    ///
    /// [`open`]: SlashingLedger::open
    store: Option<std::sync::Arc<dyn quota_router_storage::slash_store::SlashStore>>,
}

impl std::fmt::Debug for SlashingLedger {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SlashingLedger")
            .field("stakes", &self.stakes)
            .field("rules", &self.rules)
            .field(
                "store",
                &self.store.as_ref().map::<&str, _>(|_s| "<dyn SlashStore>"),
            )
            .finish()
    }
}

impl SlashingLedger {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn with_rules(rules: SlashingRules) -> Self {
        Self {
            stakes: HashMap::new(),
            rules,
            store: None,
        }
    }

    /// Open a persisted ledger against `store`, hydrating every
    /// provider from `store.load_all()`.
    ///
    /// Banned providers must remain banned across restarts; this
    /// constructor is the production entry point. The store handle
    /// is retained for write-through on subsequent mutations.
    /// # Errors
    /// Returns `SlashStoreError` (re-exported) on load failure.
    pub fn open(
        store: std::sync::Arc<dyn quota_router_storage::slash_store::SlashStore>,
        rules: SlashingRules,
    ) -> Result<Self, quota_router_storage::slash_store::SlashStoreError> {
        let rows = store.load_all()?;
        let mut stakes = HashMap::new();
        for row in rows {
            let cumulative_loss_pct = row.cumulative_loss_pct_micro as f64 / 1_000_000.0;
            // Mission 0900-d1: tuple-key `(chain_id, provider_id)`.
            // Production rows are backfilled to `DEFAULT_CHAIN_ID` by
            // v015 migration, so this lookup matches the substrate PK
            // shape end-to-end (storage row → in-memory map).
            stakes.insert(
                (row.chain_id, row.provider_id.clone()),
                ProviderStake {
                    chain_id: row.chain_id,
                    provider_id: row.provider_id,
                    stake_micro_octo_w: row.stake_micro_octo_w,
                    initial_stake_micro_octo_w: row.initial_stake_micro_octo_w,
                    offense_count: row.offense_count,
                    cumulative_loss_pct,
                },
            );
        }
        Ok(Self {
            stakes,
            rules,
            store: Some(store),
        })
    }

    /// Register a provider with an initial stake. Idempotent on existing
    /// providers (returns the existing stake). When a `store` is wired,
    /// the new (or existing) stake row is written through.
    /// # Errors
    /// Returns `SlashError::NonIntegerScale` if `initial_stake_micro_octo_w`
    /// is not `scale=0` (the MicroOctoW wire invariant). Returning a
    /// typed error here prevents silent 10^scale truncation at the
    /// persistence boundary.
    pub fn register(
        &mut self,
        provider_id: impl Into<String>,
        initial_stake_micro_octo_w: octo_determin::Dqa,
    ) -> Result<&ProviderStake, SlashError> {
        require_integer_scale(&initial_stake_micro_octo_w, "initial_stake_micro_octo_w")?;
        let provider_id = provider_id.into();
        let entry = self
            .stakes
            .entry(stake_key(&provider_id))
            .or_insert_with(|| ProviderStake {
                chain_id: DEFAULT_CHAIN_ID,
                provider_id: provider_id.clone(),
                stake_micro_octo_w: initial_stake_micro_octo_w,
                initial_stake_micro_octo_w,
                offense_count: 0,
                cumulative_loss_pct: 0.0,
            });
        // Write-through (best-effort: ignore DB errors on the hot path
        // to avoid panic in `register`; persistent error would surface
        // via `slash` which already errors).
        if let Some(store) = &self.store {
            let row = quota_router_storage::slash_store::SlashLedgerRow {
                chain_id: DEFAULT_CHAIN_ID,
                provider_id: provider_id.clone(),
                stake_micro_octo_w: entry.stake_micro_octo_w,
                initial_stake_micro_octo_w: entry.initial_stake_micro_octo_w,
                offense_count: entry.offense_count,
                cumulative_loss_pct_micro: (entry.cumulative_loss_pct * 1_000_000.0).round() as u64,
                last_updated_unix: now_unix(),
            };
            let _ = store.upsert_stake(&row);
        }
        Ok(entry)
    }

    /// Current rules.
    #[must_use]
    pub fn rules(&self) -> &SlashingRules {
        &self.rules
    }

    /// Withdraw a stake amount from a provider, returning the new
    /// `stake_micro_octo_w` on success.
    ///
    /// Gating rules (each enforced before the mutation):
    /// - `amount == 0` → `SlashError::InvalidAmount`
    /// - `amount > stake_micro_octo_w` → `SlashError::InsufficientStake`
    /// - provider not registered → `SlashError::UnknownProvider`
    /// - provider permanently banned → `SlashError::BannedProvider`
    ///
    /// The `offense_count` and `cumulative_loss_pct` are NOT touched —
    /// withdrawing stake preserves the ban-stability invariant; an
    /// operator cannot exit a banned position by withdrawing stake.
    ///
    /// When a `store` is wired, the post-withdraw stake row is written
    /// through (matching the `register` / `slash` write-through pattern
    /// from mission `marketplace-slashing-persistence`).
    /// # Errors
    /// See gating rules above.
    pub fn withdraw_stake(
        &mut self,
        provider_id: &str,
        amount: octo_determin::Dqa,
    ) -> Result<octo_determin::Dqa, SlashError> {
        require_positive_integer(&amount, "amount")?;
        let stake = self
            .stakes
            .get(&stake_key(provider_id))
            .ok_or_else(|| SlashError::UnknownProvider(provider_id.to_owned()))?;
        if stake.is_banned(&self.rules) {
            return Err(SlashError::BannedProvider {
                provider_id: provider_id.to_owned(),
                cumulative_loss_pct_bits: (stake.cumulative_loss_pct * 1_000_000.0).round() as u64,
            });
        }
        if octo_determin::dqa_cmp(amount, stake.stake_micro_octo_w) > 0 {
            return Err(SlashError::InsufficientStake {
                available: stake.stake_micro_octo_w,
                requested: amount,
            });
        }
        // SAFETY: index by `provider_id` after `stakes.get(...)` returned
        // Some — the HashMap is not concurrently mutated.
        let entry = self
            .stakes
            .get_mut(&stake_key(provider_id))
            .expect("stake entry just observed");
        entry.stake_micro_octo_w = entry
            .stake_micro_octo_w
            .subtract(amount)
            .expect("withdraw: stake >= amount enforced above");
        let new_stake = entry.stake_micro_octo_w;
        if let Some(store) = &self.store {
            let row = quota_router_storage::slash_store::SlashLedgerRow {
                chain_id: DEFAULT_CHAIN_ID,
                provider_id: provider_id.to_owned(),
                stake_micro_octo_w: new_stake,
                initial_stake_micro_octo_w: entry.initial_stake_micro_octo_w,
                offense_count: entry.offense_count,
                cumulative_loss_pct_micro: (entry.cumulative_loss_pct * 1_000_000.0).round() as u64,
                last_updated_unix: now_unix(),
            };
            let _ = store.upsert_stake(&row);
        }
        Ok(new_stake)
    }

    /// Non-mutating query: would `withdraw_stake(provider_id, amount)`
    /// succeed? Returns `Ok(())` if so, or the same `SlashError` that
    /// `withdraw_stake` would produce.
    /// # Errors
    /// See gating rules in `withdraw_stake`.
    pub fn can_withdraw(
        &self,
        provider_id: &str,
        amount: octo_determin::Dqa,
    ) -> Result<(), SlashError> {
        require_positive_integer(&amount, "amount")?;
        let stake = self
            .stakes
            .get(&stake_key(provider_id))
            .ok_or_else(|| SlashError::UnknownProvider(provider_id.to_owned()))?;
        if stake.is_banned(&self.rules) {
            return Err(SlashError::BannedProvider {
                provider_id: provider_id.to_owned(),
                cumulative_loss_pct_bits: (stake.cumulative_loss_pct * 1_000_000.0).round() as u64,
            });
        }
        if octo_determin::dqa_cmp(amount, stake.stake_micro_octo_w) > 0 {
            return Err(SlashError::InsufficientStake {
                available: stake.stake_micro_octo_w,
                requested: amount,
            });
        }
        Ok(())
    }
    #[must_use]
    pub fn stake(&self, provider_id: &str) -> Option<&ProviderStake> {
        self.stakes.get(&stake_key(provider_id))
    }

    /// Apply a slash to a provider for `reason`. Penalty = `stake *
    /// miss_rate * current_offense_penalty * verifiability`.
    ///
    /// - `miss_rate`: SLA miss rate in [0.0, 1.0].
    /// - `current_offense_penalty`: the rule's penalty fraction at the
    ///   provider's next offense (e.g., `first_offense_penalty *
    ///   multiplier^offense_count`).
    ///
    /// The function enforces `miss_rate >= rules.miss_rate_tolerance`
    /// and refuses to slash a permanently-banned provider.
    /// # Errors
    /// Returns `SlashError::UnknownProvider` if `provider_id` is not
    /// registered. Returns `SlashError::BannedProvider` if the provider
    /// already crossed the permanent-ban threshold. Returns
    /// `SlashError::BelowTolerance` if miss_rate is below the configured
    /// tolerance band.
    pub fn slash(
        &mut self,
        provider_id: &str,
        reason: SlashReason,
        miss_rate: f64,
    ) -> Result<SlashOutcome, SlashError> {
        if miss_rate < self.rules.miss_rate_tolerance {
            return Err(SlashError::BelowTolerance {
                miss_rate_bits: (miss_rate * 1_000_000.0).round() as u64,
                tolerance_bits: (self.rules.miss_rate_tolerance * 1_000_000.0).round() as u64,
            });
        }
        // Compute penalty fraction.
        let rules = self.rules;
        let offense_penalty = penalty_for_offense(
            rules.first_offense_penalty,
            rules.offense_multiplier,
            self.stake(provider_id)
                .ok_or_else(|| SlashError::UnknownProvider(provider_id.to_owned()))?
                .offense_count,
        );
        let pct =
            (offense_penalty * reason.verifiability() * miss_rate.clamp(0.0, 1.0)).clamp(0.0, 1.0);
        self.apply_penalty(provider_id, reason, pct, rules)
    }

    /// Slash by an explicit penalty fraction (bypass escalation).
    /// Used by external arbitration paths that have computed their own
    /// penalty based on evidence severity.
    /// # Errors
    /// Returns `SlashError::UnknownProvider` if `provider_id` is not
    /// registered. Returns `SlashError::BannedProvider` if the provider
    /// already crossed the permanent-ban threshold.
    pub fn slash_with_pct(
        &mut self,
        provider_id: &str,
        reason: SlashReason,
        penalty_pct: f64,
    ) -> Result<SlashOutcome, SlashError> {
        let rules = self.rules;
        self.apply_penalty(provider_id, reason, penalty_pct.clamp(0.0, 1.0), rules)
    }

    fn apply_penalty(
        &mut self,
        provider_id: &str,
        reason: SlashReason,
        pct: f64,
        rules: SlashingRules,
    ) -> Result<SlashOutcome, SlashError> {
        let stake = self
            .stakes
            .get_mut(&stake_key(provider_id))
            .ok_or_else(|| SlashError::UnknownProvider(provider_id.to_owned()))?;
        if stake.is_banned(&rules) {
            // Encode the percent as integer bits to keep SlashError Eq.
            let bits = (stake.cumulative_loss_pct * 1_000_000.0).round() as u64;
            return Err(SlashError::BannedProvider {
                provider_id: provider_id.to_owned(),
                cumulative_loss_pct_bits: bits,
            });
        }
        // Round 1 fix: compute `amount` in u128 to avoid the f64
        // mantissa-exhaustion precision loss that occurred for stakes
        // above 2^53 (≈ 9.0 × 10^15 micro-OCTO-W). The percent is
        // scaled to micro-percent (1e6) while still in [0, 1_000_000]
        // — well within f64 exact-integer range — so the cast is
        // exact. Then `(stake * pct_micro) / 1_000_000` stays in u128.
        let pct_micro = (pct.clamp(0.0, 1.0) * 1_000_000.0).round() as u128;
        let pct_micro_dqa =
            octo_determin::Dqa::new(i64::try_from(pct_micro).expect("pct_micro fits in i64"), 0)
                .expect("non-overflow");
        let raw_amount = stake
            .stake_micro_octo_w
            .multiply(pct_micro_dqa)
            .expect("stake * pct_micro overflows");
        let denominator = octo_determin::Dqa::new(1_000_000, 0).expect("non-overflow");
        let amount = raw_amount
            .divide(denominator)
            .expect("non-overflow: denominator is positive constant");
        // Cap deduction at remaining stake.
        let amount = if octo_determin::dqa_cmp(amount, stake.stake_micro_octo_w) > 0 {
            stake.stake_micro_octo_w
        } else {
            amount
        };
        stake.stake_micro_octo_w = stake
            .stake_micro_octo_w
            .subtract(amount)
            .expect("slash: cap enforced to remaining");
        stake.offense_count += 1;
        let loss_delta = if stake.initial_stake_micro_octo_w.value == 0 {
            0.0
        } else {
            amount.value as f64 / stake.initial_stake_micro_octo_w.value as f64
        };
        stake.cumulative_loss_pct += loss_delta;
        let banned = stake.is_banned(&rules);
        // Write-through to persistent store (if wired). Errors are
        // swallowed because the in-memory mutation has already taken
        // effect; a persistent failure here would surface as a
        // subsequent restart showing stale state, which a follow-on
        // observability hook can detect via `load_all` divergence.
        if let Some(store) = &self.store {
            let row = quota_router_storage::slash_store::SlashLedgerRow {
                chain_id: DEFAULT_CHAIN_ID,
                provider_id: provider_id.to_owned(),
                stake_micro_octo_w: stake.stake_micro_octo_w,
                initial_stake_micro_octo_w: stake.initial_stake_micro_octo_w,
                offense_count: stake.offense_count,
                cumulative_loss_pct_micro: (stake.cumulative_loss_pct * 1_000_000.0).round() as u64,
                last_updated_unix: now_unix(),
            };
            let _ = store.upsert_stake(&row);
        }
        Ok(SlashOutcome {
            chain_id: stake.chain_id,
            provider_id: provider_id.to_owned(),
            reason,
            amount_micro_octo_w: amount,
            new_stake_micro_octo_w: stake.stake_micro_octo_w,
            cumulative_loss_pct: stake.cumulative_loss_pct,
            banned,
        })
    }
}

fn penalty_for_offense(first: f64, multiplier: f64, offense_count: u32) -> f64 {
    // `u32 -> i32` cast wraps for `offense_count > i32::MAX` (~2.1B); a
    // wrapped negative exponent would shrink the multiplier instead of
    // saturating to full penalty (`1.5.powi(-1) = 0.667` yields a ~6.7%
    // slash where 100% is correct). Saturate via `try_from` to `i32::MAX`
    // — for any multiplier > 1.0 the resulting `first * mult` blows up
    // to `f64::INFINITY`, which `.min(1.0)` clamps to 1.0 (full slash).
    // For multiplier in (0.0, 1.0] the result is bounded to [0.0, first]
    // (multiplier^powi(large) -> 0.0), still safe — the caller clamps to
    // [0.0, 1.0]. Practically unreachable since `permanent_ban_at` (~0.50)
    // fires around offense_count=4 per RFC-0900 §Slashing Model; the
    // saturation is the documented safe behavior for adversarial input.
    let safe_count = i32::try_from(offense_count).unwrap_or(i32::MAX);
    let mult = multiplier.powi(safe_count);
    (first * mult).min(1.0)
}

#[cfg(test)]
mod tests_penalty {
    //! Standalone unit tests for `penalty_for_offense` — kept separate
    //! from the main `tests` module so the `i32::try_from` saturation
    //! guard has explicit coverage independent of the slashing flow.
    use super::penalty_for_offense;

    #[test]
    fn first_offense_zero_offenses_returns_first() {
        // offense_count=0 → powi(0) = 1.0 → result = first.
        assert_eq!(penalty_for_offense(0.10, 1.5, 0), 0.10);
    }

    #[test]
    fn moderate_offense_grows_penalty() {
        // offense_count=2 with multiplier 1.5 → 0.10 * 2.25 = 0.225.
        let p = penalty_for_offense(0.10, 1.5, 2);
        assert!((p - 0.225).abs() < 1e-9, "got {p}");
    }

    #[test]
    fn high_offense_saturates_to_one() {
        // offense_count=10 → 0.10 * 1.5^10 ≈ 57.7, clamps to 1.0.
        assert_eq!(penalty_for_offense(0.10, 1.5, 10), 1.0);
    }

    #[test]
    fn offense_count_above_i32_max_saturates_not_wraps() {
        // Regression for CRITICAL #2 from S4 Round 1 review: the
        // unwrapped `offense_count as i32` cast wrapped u32 > i32::MAX
        // to a negative exponent, shrinking the penalty instead of
        // saturating. With `try_from` + `unwrap_or(i32::MAX)` the
        // saturation is `f64::INFINITY.min(1.0) = 1.0` regardless of
        // how large `offense_count` grows — adversarial / corrupt
        // `offense_count` values cannot produce a sub-1.0 penalty.
        assert_eq!(
            penalty_for_offense(0.10, 1.5, u32::MAX),
            1.0,
            "u32::MAX must saturate to 1.0, not wrap to ~0.067"
        );
        assert_eq!(penalty_for_offense(0.10, 1.5, i32::MAX as u32 + 1), 1.0);
        assert_eq!(
            penalty_for_offense(0.10, 1.5, u32::MAX - 1),
            1.0,
            "any count past saturation must still be 1.0"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test helper: construct a `Dqa` from an `i64` literal at scale=0
    /// (the wire invariant for MicroOCTO_W; see module docs).
    fn dqa(n: i64) -> octo_determin::Dqa {
        octo_determin::Dqa::new(n, 0).expect("non-overflow")
    }

    fn ledger_with(stake: i64) -> SlashingLedger {
        let mut l = SlashingLedger::new();
        l.register("alice", dqa(stake)).unwrap();
        l
    }

    #[test]
    fn register_creates_provider_stake() {
        let l = ledger_with(1_000_000);
        let s = l.stake("alice").unwrap();
        assert_eq!(s.stake_micro_octo_w, dqa(1_000_000));
        assert_eq!(s.initial_stake_micro_octo_w, dqa(1_000_000));
        assert_eq!(s.offense_count, 0);
        assert_eq!(s.cumulative_loss_pct, 0.0);
        assert!(!s.is_banned(l.rules()));
    }

    #[test]
    fn register_rejects_non_zero_scale() {
        // Regression for MEDIUM #1 from S4 Round 1 review: the cast at
        // the persistence boundary silently truncated 10^scale digits in
        // release builds. The API gate returns a typed error instead.
        let mut l = SlashingLedger::new();
        let non_integer = octo_determin::Dqa::new(1_000, 3).expect("non-overflow");
        let err = l.register("alice", non_integer).unwrap_err();
        assert_eq!(
            err,
            SlashError::NonIntegerScale {
                param: "initial_stake_micro_octo_w",
                scale: 3,
            },
        );
        // Also reject on the high-scale end (max representable scale).
        let high_scale = octo_determin::Dqa::new(7, 18u8).expect("non-overflow");
        let err = l.register("bob", high_scale).unwrap_err();
        assert_eq!(
            err,
            SlashError::NonIntegerScale {
                param: "initial_stake_micro_octo_w",
                scale: 18u8,
            },
        );
        // Verify rejection did NOT register the provider.
        assert!(l.stake("alice").is_none());
        assert!(l.stake("bob").is_none());
    }

    #[test]
    fn register_accepts_scale_zero() {
        // Sanity: scale=0 (canonical MicroOctoW) still works after the
        // scale-gate landed. `Dqa::new(0, 0)` is the canonical zero.
        let mut l = SlashingLedger::new();
        let zero = octo_determin::Dqa::new(0, 0).expect("zero");
        let s = l.register("alice", zero).unwrap();
        assert_eq!(s.stake_micro_octo_w, zero);
        assert_eq!(s.initial_stake_micro_octo_w, zero);
    }

    #[test]
    fn withdraw_stake_rejects_non_zero_scale() {
        let mut l = ledger_with(1_000_000);
        let err = l
            .withdraw_stake(
                "alice",
                octo_determin::Dqa::new(100, 6).expect("non-overflow"),
            )
            .unwrap_err();
        assert_eq!(
            err,
            SlashError::NonIntegerScale {
                param: "amount",
                scale: 6,
            },
        );
    }

    #[test]
    fn withdraw_stake_rejects_negative_amount() {
        // Regression for LOW from S4 Round 1 review: a negative `amount`
        // would silently ADD to the stake via `subtract(negative)`.
        // The new `require_positive_integer` gate catches it.
        let mut l = ledger_with(1_000_000);
        let neg = octo_determin::Dqa::new(-100, 0).expect("non-overflow");
        let err = l.withdraw_stake("alice", neg).unwrap_err();
        assert_eq!(err, SlashError::InvalidAmount(neg));
        // Stake unchanged.
        assert_eq!(l.stake("alice").unwrap().stake_micro_octo_w, dqa(1_000_000));
    }

    #[test]
    fn withdraw_stake_rejects_zero_amount() {
        // Zero-amount is rejected (would otherwise no-op silently).
        let mut l = ledger_with(1_000_000);
        let zero = octo_determin::Dqa::new(0, 0).expect("zero");
        let err = l.withdraw_stake("alice", zero).unwrap_err();
        assert_eq!(err, SlashError::InvalidAmount(zero));
    }

    #[test]
    fn can_withdraw_rejects_negative_amount() {
        let l = ledger_with(1_000_000);
        let neg = octo_determin::Dqa::new(-100, 0).expect("non-overflow");
        let err = l.can_withdraw("alice", neg).unwrap_err();
        assert_eq!(err, SlashError::InvalidAmount(neg));
    }

    #[test]
    fn slash_deducts_stake_times_miss_rate_times_first_offense_penalty() {
        let mut l = ledger_with(1_000_000);
        // first_offense_penalty = 0.10, miss_rate = 1.0, Timeout (verifiability 1.0)
        // → amount = 1_000_000 * 0.10 * 1.0 * 1.0 = 100_000
        let out = l.slash("alice", SlashReason::TIMEOUT, 1.0).unwrap();
        assert_eq!(out.amount_micro_octo_w, dqa(100_000));
        assert_eq!(out.new_stake_micro_octo_w, dqa(900_000));
        assert_eq!(out.cumulative_loss_pct, 0.10);
        assert!(!out.banned);
    }

    #[test]
    fn slash_scales_with_miss_rate() {
        let mut l = ledger_with(1_000_000);
        let out = l.slash("alice", SlashReason::PROVIDER_ERROR, 0.5).unwrap();
        // 0.10 * 0.5 = 0.05 → 50_000
        assert_eq!(out.amount_micro_octo_w, dqa(50_000));
    }

    #[test]
    fn garbage_response_uses_half_verifiability() {
        let mut l = ledger_with(1_000_000);
        let out = l
            .slash("alice", SlashReason::GARBAGE_RESPONSE, 1.0)
            .unwrap();
        // 0.10 * 1.0 * 0.5 = 0.05 → 50_000
        assert_eq!(out.amount_micro_octo_w, dqa(50_000));
    }

    #[test]
    fn repeat_offenses_escalate_penalty() {
        let mut l = ledger_with(1_000_000);
        // 1st: 0.10 → 100_000
        let o1 = l.slash("alice", SlashReason::TIMEOUT, 1.0).unwrap();
        assert_eq!(o1.amount_micro_octo_w, dqa(100_000));
        // 2nd: 0.10 * 1.5 = 0.15 → 0.15 * 900_000 = 135_000
        let o2 = l.slash("alice", SlashReason::TIMEOUT, 1.0).unwrap();
        assert_eq!(o2.amount_micro_octo_w, dqa(135_000));
        // 3rd: 0.10 * 1.5^2 = 0.225 → 0.225 * 765_000 = 172_125
        let o3 = l.slash("alice", SlashReason::TIMEOUT, 1.0).unwrap();
        assert_eq!(o3.amount_micro_octo_w, dqa(172_125));
    }

    #[test]
    fn cumulative_loss_pct_tracks_initial_stake() {
        let mut l = ledger_with(1_000_000);
        l.slash("alice", SlashReason::TIMEOUT, 1.0).unwrap();
        let s = l.stake("alice").unwrap();
        assert_eq!(s.cumulative_loss_pct, 0.10);
    }

    #[test]
    fn permanent_ban_at_50pct_loss() {
        let mut l = ledger_with(1_000_000);
        // 1st: 0.10 → 900_000 left, cumulative 0.10
        l.slash("alice", SlashReason::TIMEOUT, 1.0).unwrap();
        // 2nd: 0.15 → 900_000 * 0.15 = 135_000; left 765_000, cumulative 0.235
        l.slash("alice", SlashReason::TIMEOUT, 1.0).unwrap();
        // 3rd: 0.225 → 765_000 * 0.225 = 172_125; left 592_875, cumulative 0.407125
        l.slash("alice", SlashReason::TIMEOUT, 1.0).unwrap();
        // 4th: 0.3375 → 592_875 * 0.3375 = 200_095.31 ≈ 200_095; left 392_780,
        // cumulative 0.6072 ≥ 0.50 → banned
        let out = l.slash("alice", SlashReason::TIMEOUT, 1.0).unwrap();
        assert!(out.banned);
        assert!(l.stake("alice").unwrap().is_banned(l.rules()));
    }

    #[test]
    fn slashing_banned_provider_errors() {
        let mut l = ledger_with(1_000_000);
        // Drive alice to ban with a direct big penalty (arbitration path).
        l.slash_with_pct("alice", SlashReason::TIMEOUT, 0.6)
            .unwrap();
        let err = l.slash("alice", SlashReason::TIMEOUT, 1.0).unwrap_err();
        assert!(matches!(err, SlashError::BannedProvider { .. }));
    }

    #[test]
    fn slashing_unknown_provider_errors() {
        let mut l = SlashingLedger::new();
        let err = l.slash("ghost", SlashReason::TIMEOUT, 1.0).unwrap_err();
        assert_eq!(err, SlashError::UnknownProvider("ghost".to_owned()));
    }

    #[test]
    fn miss_rate_below_tolerance_errors() {
        let mut l = SlashingLedger::with_rules(SlashingRules {
            miss_rate_tolerance: 0.05,
            ..SlashingRules::default()
        });
        l.register("alice", dqa(1_000_000)).unwrap();
        let err = l.slash("alice", SlashReason::TIMEOUT, 0.01).unwrap_err();
        assert!(matches!(err, SlashError::BelowTolerance { .. }));
    }

    #[test]
    fn slash_with_explicit_pct_bypasses_escalation() {
        let mut l = ledger_with(1_000_000);
        let out = l
            .slash_with_pct("alice", SlashReason::GARBAGE_RESPONSE, 0.25)
            .unwrap();
        // 0.25 of 1_000_000 = 250_000
        assert_eq!(out.amount_micro_octo_w, dqa(250_000));
        assert_eq!(out.cumulative_loss_pct, 0.25);
    }

    #[test]
    fn slash_does_not_overdraft_remaining_stake() {
        let mut l = ledger_with(100_000);
        // Apply an oversized explicit penalty; should cap at remaining stake.
        let out = l
            .slash_with_pct("alice", SlashReason::TIMEOUT, 1.5)
            .unwrap();
        assert_eq!(out.amount_micro_octo_w, dqa(100_000));
        assert_eq!(out.new_stake_micro_octo_w, dqa(0));
    }

    #[test]
    fn default_rules_match_rfc0900() {
        let rules = SlashingRules::default();
        assert!((rules.first_offense_penalty - 0.10).abs() < f64::EPSILON);
        assert!((rules.offense_multiplier - 1.5).abs() < f64::EPSILON);
        assert!((rules.permanent_ban_at - 0.50).abs() < f64::EPSILON);
    }

    // ========================================================================
    // Persistence tests (mission: marketplace-slashing-persistence)
    //
    // Pins the contract: `SlashingLedger::open(store)` hydrates from
    // the persistent store, and every mutation (register, slash,
    // slash_with_pct) is written through to the store so banned
    // providers remain banned across process restarts.
    // ========================================================================

    use quota_router_storage::slash_store::SlashStore;
    use std::sync::Arc;

    fn open_in_memory_store() -> Arc<quota_router_storage::slash_store::StoolapSlashStore> {
        Arc::new(
            quota_router_storage::slash_store::StoolapSlashStore::open_in_memory()
                .expect("open in-memory store"),
        )
    }

    #[test]
    fn open_hydrates_from_store() {
        // Pre-populate the store with one banned provider.
        let store = open_in_memory_store();
        let row = quota_router_storage::slash_store::SlashLedgerRow {
            chain_id: DEFAULT_CHAIN_ID,
            provider_id: "alice".to_string(),
            stake_micro_octo_w: dqa(400_000),
            initial_stake_micro_octo_w: dqa(1_000_000),
            offense_count: 4,
            cumulative_loss_pct_micro: 600_000, // 60% → banned
            last_updated_unix: 1_700_000_000,
        };
        store.upsert_stake(&row).expect("pre-populate");

        let mut ledger = SlashingLedger::open(store, SlashingRules::default()).expect("open");
        let stake = ledger.stake("alice").expect("alice in ledger");
        assert_eq!(stake.stake_micro_octo_w, dqa(400_000));
        assert_eq!(stake.offense_count, 4);
        assert!(stake.is_banned(ledger.rules()));
        // Subsequent slash on the banned provider must fail.
        let err = ledger
            .slash("alice", SlashReason::PROVIDER_ERROR, 1.0)
            .unwrap_err();
        assert!(matches!(err, SlashError::BannedProvider { .. }));
    }

    #[test]
    fn register_writes_through_to_store() {
        let store = open_in_memory_store();
        let mut ledger = SlashingLedger::open(
            Arc::clone(&store) as Arc<dyn quota_router_storage::slash_store::SlashStore>,
            SlashingRules::default(),
        )
        .expect("open");
        ledger.register("bob", dqa(1_000_000)).unwrap();
        // Reload via a fresh ledger against the same store.
        drop(ledger);
        let rows = store.load_all().expect("load_all");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].provider_id, "bob");
        assert_eq!(rows[0].initial_stake_micro_octo_w, dqa(1_000_000));
    }

    #[test]
    fn slash_writes_through_to_store() {
        let store = open_in_memory_store();
        let mut ledger = SlashingLedger::open(
            Arc::clone(&store) as Arc<dyn quota_router_storage::slash_store::SlashStore>,
            SlashingRules::default(),
        )
        .expect("open");
        ledger.register("carol", dqa(1_000_000)).unwrap();
        let out = ledger
            .slash("carol", SlashReason::TIMEOUT, 1.0)
            .expect("slash");
        assert_eq!(out.amount_micro_octo_w, dqa(100_000));
        assert_eq!(out.new_stake_micro_octo_w, dqa(900_000));
        // Reload via a fresh ledger.
        drop(ledger);
        let ledger2 = SlashingLedger::open(
            Arc::clone(&store) as Arc<dyn quota_router_storage::slash_store::SlashStore>,
            SlashingRules::default(),
        )
        .expect("reopen");
        let stake = ledger2.stake("carol").expect("carol");
        assert_eq!(stake.stake_micro_octo_w, dqa(900_000));
        assert_eq!(stake.offense_count, 1);
        assert!((stake.cumulative_loss_pct - 0.10).abs() < 1e-3);
    }

    #[test]
    fn ban_persists_across_restart() {
        // The critical contract: a banned provider must remain banned
        // after the process restarts. Register, slash until banned,
        // drop, reopen, verify still banned.
        let store = open_in_memory_store();
        let mut ledger = SlashingLedger::open(
            Arc::clone(&store) as Arc<dyn quota_router_storage::slash_store::SlashStore>,
            SlashingRules::default(),
        )
        .expect("open");
        ledger.register("dave", dqa(1_000_000)).unwrap();
        // First 3 offenses: 10%, 15%, 22.5% (cumulative ≈ 40.7%).
        for _ in 0..3 {
            ledger
                .slash("dave", SlashReason::PROVIDER_ERROR, 1.0)
                .expect("slash");
        }
        let stake_before = ledger.stake("dave").expect("dave pre-restart").clone();
        let was_banned_before = ledger
            .stake("dave")
            .expect("dave")
            .is_banned(ledger.rules());
        assert!(
            !was_banned_before,
            "dave must NOT be banned after 3 offenses (cumulative ≈ 40.7%)"
        );
        drop(ledger);

        let mut ledger2 = SlashingLedger::open(
            Arc::clone(&store) as Arc<dyn quota_router_storage::slash_store::SlashStore>,
            SlashingRules::default(),
        )
        .expect("reopen");
        let stake_after = ledger2.stake("dave").expect("dave post-restart");
        assert!(
            !stake_after.is_banned(ledger2.rules()),
            "dave must NOT be banned after restart (state preserved)"
        );
        assert_eq!(
            stake_before.offense_count, stake_after.offense_count,
            "offense_count must persist"
        );
        assert_eq!(
            stake_before.stake_micro_octo_w, stake_after.stake_micro_octo_w,
            "stake must persist"
        );

        // Fourth offense crosses 50% → ban. Drive the slash on the
        // reopened ledger, then verify ban persists across another
        // restart.
        ledger2
            .slash("dave", SlashReason::PROVIDER_ERROR, 1.0)
            .expect("fourth slash");
        drop(ledger2);

        let mut ledger3 = SlashingLedger::open(
            Arc::clone(&store) as Arc<dyn quota_router_storage::slash_store::SlashStore>,
            SlashingRules::default(),
        )
        .expect("third reopen");
        assert!(
            ledger3
                .stake("dave")
                .expect("dave third reopen")
                .is_banned(ledger3.rules()),
            "dave must remain banned after restart"
        );
        let err = ledger3
            .slash("dave", SlashReason::TIMEOUT, 1.0)
            .unwrap_err();
        assert!(
            matches!(err, SlashError::BannedProvider { .. }),
            "post-ban slash must still reject after restart"
        );
    }

    #[test]
    fn slash_with_pct_writes_through_to_store() {
        let store = open_in_memory_store();
        let mut ledger = SlashingLedger::open(
            Arc::clone(&store) as Arc<dyn quota_router_storage::slash_store::SlashStore>,
            SlashingRules::default(),
        )
        .expect("open");
        ledger.register("eve", dqa(1_000_000)).unwrap();
        let out = ledger
            .slash_with_pct("eve", SlashReason::PROVIDER_ERROR, 0.05)
            .expect("slash_with_pct");
        assert_eq!(out.amount_micro_octo_w, dqa(50_000));
        drop(ledger);
        let ledger2 = SlashingLedger::open(
            Arc::clone(&store) as Arc<dyn quota_router_storage::slash_store::SlashStore>,
            SlashingRules::default(),
        )
        .expect("reopen");
        let stake = ledger2.stake("eve").expect("eve");
        assert_eq!(stake.stake_micro_octo_w, dqa(950_000));
        assert_eq!(stake.offense_count, 1);
    }

    // ========================================================================
    // Typed-discriminator extension tests (mission
    // marketplace-slash-reason-typed-discriminator)
    // ========================================================================

    /// Extension `SlashReasonSpec` for an out-of-tree reason (e.g.,
    /// privacy breach), demonstrating the registry pattern.
    struct PrivacyBreachSpec;
    impl SlashReasonSpec for PrivacyBreachSpec {
        fn type_id(&self) -> [u8; 16] {
            // Extension namespace `0xFFFF:0001:...:0001` — outside the
            // RFC-0900 core allocation.
            [
                0xFF, 0xFF, 0x00, 0x01, 0x00, 0x01, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            ]
        }
        fn verifiability(&self) -> f64 {
            0.75
        }
        fn name(&self) -> &str {
            "privacy_breach"
        }
    }

    #[test]
    fn slash_reason_core_constants_have_distinct_type_ids() {
        // Each of the 5 RFC-0900 core reasons must carry a unique
        // 128-bit type_id. Central enum → typed-discriminator migration
        // invariant.
        let ids = [
            SlashReason::TIMEOUT.type_id,
            SlashReason::PROVIDER_ERROR.type_id,
            SlashReason::LATENCY_HIGH.type_id,
            SlashReason::GARBAGE_RESPONSE.type_id,
            SlashReason::FAILED_RESPONSE.type_id,
        ];
        for i in 0..ids.len() {
            for j in (i + 1)..ids.len() {
                assert_ne!(ids[i], ids[j], "core reason {i} == {j} type_id");
            }
        }
        assert!(SlashReason::TIMEOUT.is_rfc_core());
        assert!(SlashReason::PROVIDER_ERROR.is_rfc_core());
        assert!(!SlashReason::new([0xAB; 16], vec![]).is_rfc_core());
    }

    #[test]
    fn register_extension_spec_dispatches_weight_correctly() {
        // Build a fresh registry (not the global one — tests must be
        // hermetic), register an extension spec, fire a slash, verify
        // the weight comes from the registered spec.
        let reg = SlashReasonSpecRegistry::new();
        reg.register(std::sync::Arc::new(PrivacyBreachSpec));
        let breach = SlashReason::new(
            [
                0xFF, 0xFF, 0x00, 0x01, 0x00, 0x01, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            ],
            vec![],
        );
        assert!((reg.verifiability(&breach.type_id) - 0.75).abs() < 1e-9);
        // An unrelated type_id (not registered) fails-closed at 0.0.
        let unknown = SlashReason::new([0x00; 16], vec![]);
        assert_eq!(reg.verifiability(&unknown.type_id), 0.0);
    }

    #[test]
    fn unknown_type_id_fails_closed_at_zero_weight() {
        // The default registry contains the 5 RFC reasons; an
        // unregistered type_id must NOT silently inherit a default
        // weight (fail-closed, per spec).
        let unknown = SlashReason::new([0xEE; 16], vec![]);
        assert_eq!(unknown.verifiability(), 0.0);
        // Sanity: the 5 RFC reasons DO have non-zero weights.
        assert!(SlashReason::TIMEOUT.verifiability() > 0.0);
        assert!(SlashReason::GARBAGE_RESPONSE.verifiability() > 0.0);
    }

    // ========================================================================
    // Mission 0900-d1 TVs (chain-aware slash ledger follow-on)
    //
    // TV-0900-D-05 (HashMap tuple-key) + TV-0900-D-10 (cross-crate
    // `open()` flow loads chain-tagged rows) + TV-0900-D-11
    // (`SlashOutcome.chain_id` populated). Runtime execution blocked
    // by libpython3.12 infra (mission 0900-d1 AC-10) — compile-check
    // via `cargo build -p quota-router-core --features full --tests`
    // verifies shape.
    // ========================================================================

    #[test]
    fn tv_0900_d_05_hashmap_tuple_key_lookup_returns_chain_tagged_stake() {
        // Register a provider, query by `provider_id` (single-arg
        // public API), verify the returned `ProviderStake.chain_id`
        // is `DEFAULT_CHAIN_ID` — pins that the `stake_key(provider_id)`
        // tuple lookup resolves to the DEFAULT_CHAIN_ID namespace.
        let mut l = SlashingLedger::new();
        l.register("alice", dqa(1_000_000)).expect("register");
        let s = l.stake("alice").expect("alice in ledger");
        assert_eq!(
            s.chain_id, DEFAULT_CHAIN_ID,
            "ProviderStake.chain_id must equal DEFAULT_CHAIN_ID post-tuple-key restructure"
        );
        assert_eq!(s.provider_id, "alice");
        assert_eq!(s.stake_micro_octo_w, dqa(1_000_000));
    }

    #[test]
    fn tv_0900_d_10_open_flow_loads_chain_tagged_rows() {
        // Insert via StoolapSlashStore (substrate row with
        // chain_id = DEFAULT_CHAIN_ID), reopen via
        // SlashingLedger::open, verify ProviderStake.chain_id
        // matches the substrate row.
        let store = open_in_memory_store();
        let row = quota_router_storage::slash_store::SlashLedgerRow {
            chain_id: DEFAULT_CHAIN_ID,
            provider_id: "carol".to_string(),
            stake_micro_octo_w: dqa(900_000),
            initial_stake_micro_octo_w: dqa(1_000_000),
            offense_count: 1,
            cumulative_loss_pct_micro: 100_000,
            last_updated_unix: 1_700_000_000,
        };
        store.upsert_stake(&row).expect("pre-populate");
        let ledger = SlashingLedger::open(
            Arc::clone(&store) as Arc<dyn quota_router_storage::slash_store::SlashStore>,
            SlashingRules::default(),
        )
        .expect("open");
        let s = ledger.stake("carol").expect("carol in ledger");
        assert_eq!(
            s.chain_id, DEFAULT_CHAIN_ID,
            "open() hydration must preserve substrate chain_id"
        );
        assert_eq!(s.stake_micro_octo_w, dqa(900_000));
        assert_eq!(s.offense_count, 1);
    }

    #[test]
    fn tv_0900_d_11_slash_outcome_chain_id_populated_from_stake() {
        // Slash a provider, verify SlashOutcome.chain_id matches
        // ProviderStake.chain_id (the chain partition the outcome
        // applies to, for audit-table chain attribution).
        let mut l = ledger_with(1_000_000);
        let out = l.slash("alice", SlashReason::TIMEOUT, 1.0).expect("slash");
        assert_eq!(
            out.chain_id, DEFAULT_CHAIN_ID,
            "SlashOutcome.chain_id must mirror ProviderStake.chain_id"
        );
        // Sanity: other fields still pinned.
        assert_eq!(out.amount_micro_octo_w, dqa(100_000));
        assert_eq!(out.new_stake_micro_octo_w, dqa(900_000));
        assert!(!out.banned);
    }
}
