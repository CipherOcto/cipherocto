//! Payment caveat — single-element budget composition for paid queries
//! (RFC-0965 §1 Caveat type enumeration; 0x1A-0xCF reserved range at
//! v2.1 §4 Discriminator; mission 0957-phase2b).
//!
//! Moved from `crates/octo-paid-query/src/lib.rs` as part of mission
//! 0957-phase2b — the caveat data type is a macaroon substrate
//! concern (Layer 4) per the per-extension crate model. The
//! `octo-paid-query` crate now re-exports this type and owns only
//! the Phase 5 MVP primitives (`RateLimitBudget`,
//! `verify_paid_query`, request/response envelopes).
//!
//! Attenuation invariant (RFC-0957 §3.5): `attenuate` only NARROWS —
//! the new caveat's `budget` MUST be ≤ self's `budget` and
//! `expires_at_unix_ms` MUST be ≤ self's. Widening is a hard error.
//!
//! Discriminator string: `"paid-query/v1"` (the legacy constant
//! `PAID_QUERY_CAVEAT_NAME` is preserved for backward compat with
//! `octo-paid-query` callers). The `caveat_name` field on the struct
//! carries the discriminator so future variants
//! (`"paid-query/subscription/v1"`, etc.) can be distinguished on
//! decode without changing the schema.
//!
//! **Type note (2026-08-17):** the `MicroOctoW` type alias was RETIRED
//! project-wide. Amount-bearing fields use `octo_determin::Dqa`
//! directly with `scale = 0` enforced at the substrate boundary.
//! Arithmetic uses `dqa_cmp`/`dqa_sub` (the `Dqa` type does not
//! implement `Ord`/`Sub` directly — see `determin/src/dqa.rs`).
//!
//! **Mission E (RFC-0965):** asset-binding extension per §2.1.
//! Adds `asset_id`, `registry_snapshot_epoch`, `nonce` fields;
//! widens `attenuate` to 4-arg accepting `new_asset_id` +
//! `&dyn AssetRegistry`; adds `verify()` (commits nonce) + `validate()`
//! (no nonce commit) gates; legacy-form deserialization rejection
//! via custom `Deserialize` impl; `Caveat::AssetBinding` co-bound
//! rule enforced at `set_subsumes` boundary (caveat/mod.rs).

use serde::{Deserialize, Serialize};

use octo_determin::{dqa_cmp, dqa_sub, Dqa};

use crate::dqa_serde;
use crate::{
    sovereign_nonce_namespace, AssetId, AssetRegistry, Epoch, Nonce, NonceError, NonceEventKind,
    NonceRegistry,
};

/// RFC-0965 caveat discriminator string for the paid-query variant.
///
/// First slot in the `0x1A`-`0xCF` reserved range per RFC-0871
/// §Implementation Phases Phase 5.
pub const PAID_QUERY_CAVEAT_NAME: &str = "paid-query/v1";

/// Canonical OCTO-W asset_id (RFC-0105 §3.1).
///
/// `OCTO_W_ASSET_ID = AssetId::derive("OCTO-W")` — the native
/// governance token. Used by Mission E legacy-form deserialization
/// rejection (§2.4) to distinguish legacy OCTO-W budget
/// caveats from new asset-generic PaymentCaveats.
pub const OCTO_W_ASSET_ID_BYTES: [u8; 32] = [
    // BLAKE3("cipherocto/asset/v1/OCTO-W") — pin the canonical derivation
    // at the substrate boundary. Per RFC-0105 §3.1 with the
    // asset_id derivation rule `BLAKE3("cipherocto/asset/v1/" + role_token)`.
    0x7a, 0x9c, 0x12, 0x4f, 0xa3, 0x88, 0xd1, 0x5b, 0x67, 0xe2, 0x0c, 0x44, 0x91, 0xb7, 0x3a, 0xfe,
    0x55, 0x80, 0x21, 0xd9, 0x6c, 0x46, 0xae, 0xb3, 0x88, 0x4d, 0x97, 0x71, 0xe2, 0x33, 0x10, 0x5c,
];

/// Get the canonical OCTO-W asset_id from the pin (avoids recomputing BLAKE3
/// for every deserialization).
#[must_use]
pub fn octo_w_asset_id() -> AssetId {
    AssetId::from_bytes(OCTO_W_ASSET_ID_BYTES)
}

/// Payment caveat — bounds holder spend against `budget` over queries
/// against `model` (RFC-0965 reserved discriminator `0x1A`).
///
/// A `PaymentCaveat` is a single-element composition in the macaroon
/// caveat chain. The verifier (`PaymentCaveat::verify`) checks
/// `budget >= query_cost` and returns a `PaidQueryDecision`
/// (proceed / partial / reject).
///
/// Wire form: `serde_json` (canonical, per `Caveat::Payment` variant
/// tagging in `caveat/mod.rs`). Amount-bearing fields use `Dqa` with
/// `#[serde(with = "dqa_serde::field")]` to encode the 16-byte BE
/// `DqaEncoding` wire form (per RFC-0105 §Canonical Encoding).
///
/// **Mission E field additions** (RFC-0965 §2.1):
/// - `asset_id: AssetId` — binds caveat to a specific asset
/// - `registry_snapshot_epoch: Epoch` — stale-snapshot detection
/// - `nonce: Nonce` — anti-replay (NonceRegistry key)
#[derive(
    Clone,
    Debug,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    borsh::BorshSerialize,
    borsh::BorshDeserialize,
)]
pub struct PaymentCaveat {
    /// RFC-0965 caveat discriminator string. Always
    /// `"paid-query/v1"` for this variant; future variants carry
    /// distinct strings.
    pub caveat_name: String,
    /// Asset bound to this caveat (RFC-0965 §2.1). `AssetId`
    /// prevents the holder from spending a USDC budget against an
    /// OCTO-W query (or vice versa).
    #[serde(with = "serde_asset_id")]
    pub asset_id: AssetId,
    /// Prepaid spend budget. Holder can spend up to this amount
    /// across all queries matching `model`. `Dqa` at `scale = 0`
    /// (integer micro-OCTO-W).
    #[serde(with = "dqa_serde::field")]
    pub budget: Dqa,
    /// Model identifier this caveat applies to (`"gpt-4"`,
    /// `"claude-3-opus"`, etc.). Empty string `""` means "any
    /// model".
    pub model: String,
    /// Unix-time millisecond expiry. `u64::MAX` means "never
    /// expires".
    pub expires_at_unix_ms: u64,
    /// Snapshot of the registry epoch at mint/attenuation time
    /// (RFC-0965 §2.1). `validate()` checks
    /// `current_epoch >= registry_snapshot_epoch` (else stale).
    #[serde(with = "serde_epoch")]
    pub registry_snapshot_epoch: Epoch,
    /// Anti-replay nonce (RFC-0965 §2.1). `verify()` calls
    /// `NonceRegistry::observe`; `validate()` calls
    /// `NonceRegistry::observe_readonly`.
    #[serde(with = "serde_nonce")]
    pub nonce: Nonce,
}

/// Serde adapter for `[u8; 32]` newtype fields (32-byte hex string).
mod serde_bytes_arr32 {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(bytes: &[u8; 32], s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&hex::encode(bytes))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<[u8; 32], D::Error> {
        let s = String::deserialize(d)?;
        let v = hex::decode(&s).map_err(serde::de::Error::custom)?;
        if v.len() != 32 {
            return Err(serde::de::Error::custom(format!(
                "expected 32 bytes, got {}",
                v.len()
            )));
        }
        let mut out = [0u8; 32];
        out.copy_from_slice(&v);
        Ok(out)
    }
}

/// Serde adapter for `AssetId` newtype (32-byte hex string).
mod serde_asset_id {
    use serde::{Deserializer, Serializer};

    use super::serde_bytes_arr32 as inner;
    use crate::AssetId;

    pub fn serialize<S: Serializer>(id: &AssetId, s: S) -> Result<S::Ok, S::Error> {
        inner::serialize(&id.0, s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<AssetId, D::Error> {
        let bytes = inner::deserialize(d)?;
        Ok(AssetId::from_bytes(bytes))
    }
}

/// Serde adapter for `Nonce` newtype (32-byte hex string).
mod serde_nonce {
    use serde::{Deserializer, Serializer};

    use super::serde_bytes_arr32 as inner;
    use crate::Nonce;

    pub fn serialize<S: Serializer>(n: &Nonce, s: S) -> Result<S::Ok, S::Error> {
        inner::serialize(&n.0, s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Nonce, D::Error> {
        let bytes = inner::deserialize(d)?;
        Ok(Nonce::from_bytes(bytes))
    }
}

/// Serde adapter for `Epoch` u64 newtype (decimal string).
mod serde_epoch {
    use serde::{Deserialize, Deserializer, Serializer};

    use crate::Epoch;

    pub fn serialize<S: Serializer>(e: &Epoch, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&e.0.to_string())
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Epoch, D::Error> {
        let s = String::deserialize(d)?;
        let v: u64 = s.parse().map_err(serde::de::Error::custom)?;
        Ok(Epoch::new(v))
    }
}

impl PaymentCaveat {
    /// Construct a new payment caveat (Mission E full-args constructor).
    ///
    /// For legacy 3-arg construction (no asset_id, no epoch, no nonce),
    /// call [`PaymentCaveat::legacy_3arg`] — it carries a
    /// `#[deprecated]` warning per RFC-0965 §4.1 6-week window.
    #[must_use]
    pub fn new(
        asset_id: AssetId,
        budget: Dqa,
        model: impl Into<String>,
        expires_at_unix_ms: u64,
        registry_snapshot_epoch: Epoch,
        nonce: Nonce,
    ) -> Self {
        Self {
            caveat_name: PAID_QUERY_CAVEAT_NAME.to_string(),
            asset_id,
            budget,
            model: model.into(),
            expires_at_unix_ms,
            registry_snapshot_epoch,
            nonce,
        }
    }

    /// Legacy 3-arg constructor (RFC-0965 §4.1 — 6-week
    /// `#[deprecated]` window). Defaults: `asset_id = OCTO_W_ASSET_ID`,
    /// `registry_snapshot_epoch = Epoch(0)`, `nonce = Nonce([0u8; 32])`.
    #[deprecated(note = "use 6-arg `new()` per RFC-0965 §2.1; defaults asset_id to OCTO-W")]
    pub fn legacy_3arg(budget: Dqa, model: impl Into<String>, expires_at_unix_ms: u64) -> Self {
        Self {
            caveat_name: PAID_QUERY_CAVEAT_NAME.to_string(),
            asset_id: AssetId::from_bytes(OCTO_W_ASSET_ID_BYTES),
            budget,
            model: model.into(),
            expires_at_unix_ms,
            registry_snapshot_epoch: Epoch::new(0),
            nonce: Nonce::from_bytes([0u8; 32]),
        }
    }

    /// True if `now_unix_ms > expires_at_unix_ms`. `u64::MAX` returns
    /// `false` (never expires).
    #[must_use]
    pub fn is_expired(&self, now_unix_ms: u64) -> bool {
        self.expires_at_unix_ms != u64::MAX && now_unix_ms > self.expires_at_unix_ms
    }

    /// True if `query_model` matches the caveat's model scope. Empty
    /// caveat `model` matches any query model (wildcard).
    #[must_use]
    pub fn matches_model(&self, query_model: &str) -> bool {
        self.model.is_empty() || self.model == query_model
    }

    /// 4-arg attenuate per RFC-0965 §2.2.
    ///
    /// Gates:
    /// 1. Registry resolves `self.asset_id` (else `AssetUnknown`)
    /// 2. `new_asset_id == self.asset_id` (else `AssetMismatch`)
    /// 3. Scale-binding — `new_budget.wire_scale == self.budget.wire_scale`
    ///    (else `ScaleMismatch`)
    /// 4. Budget narrowing — `new_budget <= self.budget` (else `BudgetWidened`)
    /// 5. Expiry narrowing — `new_expires_at_unix_ms <= self.expires_at_unix_ms`
    ///    (else `ExpiryWidened`)
    ///
    /// # Errors
    /// `AttenuationError` on any gate failure. The returned
    /// `PaymentCaveat` preserves the original `model`,
    /// `registry_snapshot_epoch`, and `nonce` (the nonce is bound to
    /// the caveat; a re-mint creates a new nonce).
    pub fn attenuate(
        &self,
        new_budget: Dqa,
        new_expires_at_unix_ms: u64,
        new_asset_id: AssetId,
        registry: &dyn AssetRegistry,
    ) -> Result<Self, AttenuationError> {
        // Gate 1: registry resolves asset_id
        let meta = registry
            .metadata(&self.asset_id)
            .map_err(|_| AttenuationError::AssetUnknown(self.asset_id))?;
        // Gate 2: asset equality
        if new_asset_id.0 != self.asset_id.0 {
            return Err(AttenuationError::AssetMismatch {
                current: self.asset_id,
                proposed: new_asset_id,
            });
        }
        // Gate 3: scale-binding
        if new_budget.scale != self.budget.scale || new_budget.scale != meta.wire_scale {
            return Err(AttenuationError::ScaleMismatch {
                current: self.budget.scale,
                proposed: new_budget.scale,
            });
        }
        // Gate 4: budget narrowing
        if dqa_cmp(new_budget, self.budget) > 0 {
            return Err(AttenuationError::BudgetWidened {
                current: self.budget,
                proposed: new_budget,
            });
        }
        // Gate 5: expiry narrowing
        let same_never_expires =
            self.expires_at_unix_ms == u64::MAX && new_expires_at_unix_ms == u64::MAX;
        if !same_never_expires && new_expires_at_unix_ms > self.expires_at_unix_ms {
            return Err(AttenuationError::ExpiryWidened {
                current: self.expires_at_unix_ms,
                proposed: new_expires_at_unix_ms,
            });
        }
        Ok(Self {
            caveat_name: PAID_QUERY_CAVEAT_NAME.to_string(),
            asset_id: self.asset_id,
            budget: new_budget,
            model: self.model.clone(),
            expires_at_unix_ms: new_expires_at_unix_ms,
            registry_snapshot_epoch: self.registry_snapshot_epoch,
            nonce: self.nonce,
        })
    }

    /// Legacy 2-arg attenuate (RFC-0965 §4.1 — 6-week
    /// `#[deprecated]` window). No asset_id, no registry handle.
    #[deprecated(
        note = "use 4-arg `attenuate(new_budget, new_expires, new_asset_id, &registry)` per RFC-0965 §2.2"
    )]
    pub fn attenuate_legacy_2arg(
        &self,
        new_budget: Dqa,
        new_expires_at_unix_ms: u64,
    ) -> Result<Self, AttenuationError> {
        if dqa_cmp(new_budget, self.budget) > 0 {
            return Err(AttenuationError::BudgetWidened {
                current: self.budget,
                proposed: new_budget,
            });
        }
        let same_never_expires =
            self.expires_at_unix_ms == u64::MAX && new_expires_at_unix_ms == u64::MAX;
        if !same_never_expires && new_expires_at_unix_ms > self.expires_at_unix_ms {
            return Err(AttenuationError::ExpiryWidened {
                current: self.expires_at_unix_ms,
                proposed: new_expires_at_unix_ms,
            });
        }
        Ok(Self {
            caveat_name: PAID_QUERY_CAVEAT_NAME.to_string(),
            asset_id: self.asset_id,
            budget: new_budget,
            model: self.model.clone(),
            expires_at_unix_ms: new_expires_at_unix_ms,
            registry_snapshot_epoch: self.registry_snapshot_epoch,
            nonce: self.nonce,
        })
    }

    /// Verify a query proposal against this caveat (commits nonce).
    /// Returns the canonical `PaymentDecision`.
    ///
    /// Gates (RFC-0965 §2.3, 8 gates):
    /// 0. Registry resolves asset_id (else `AssetUnknown`)
    /// 1. Scale-binding (else `ScaleMismatch`)
    /// 2. Stale-snapshot — `current_epoch >= self.registry_snapshot_epoch` (else `StaleSnapshot`)
    /// 3. Anti-replay — `nonce_registry.observe(NonceEventKind::Payment, &pk, &self.nonce.0)`
    ///    (else `Replay`); `pk = meta.governance_pubkey.unwrap_or_else(|| sovereign_nonce_namespace(&self.asset_id))`
    /// 4. Expiry (else `Expired`)
    /// 5. Model match (else `ModelMismatch`)
    /// 6. Budget exhaust (else `BudgetExhausted`)
    /// 7. Cost > budget (else `Partial` or `Proceed`)
    ///
    /// `verify()` commits the nonce via `observe` (mutating). For
    /// non-mutating checks (audit pre-flight, batch replay), use
    /// [`PaymentCaveat::validate`].
    #[allow(clippy::too_many_arguments)]
    pub fn verify(
        &self,
        query_cost: Dqa,
        query_model: &str,
        now_unix_ms: u64,
        current_epoch: Epoch,
        registry: &dyn AssetRegistry,
        nonce_registry: &mut dyn NonceRegistry,
    ) -> PaymentDecision {
        let meta = match registry.metadata(&self.asset_id) {
            Ok(m) => m,
            Err(_) => {
                return PaymentDecision::Reject {
                    reason: PaymentRejectionReason::AssetUnknown,
                };
            }
        };
        if query_cost.scale != self.budget.scale || query_cost.scale != meta.wire_scale {
            return PaymentDecision::Reject {
                reason: PaymentRejectionReason::ScaleMismatch {
                    caveat_scale: self.budget.scale,
                    query_cost_scale: query_cost.scale,
                },
            };
        }
        if current_epoch.0 < self.registry_snapshot_epoch.0 {
            return PaymentDecision::Reject {
                reason: PaymentRejectionReason::StaleSnapshot {
                    snapshot: self.registry_snapshot_epoch.0,
                    live: current_epoch.0,
                },
            };
        }
        let pk = meta
            .governance_pubkey
            .unwrap_or_else(|| sovereign_nonce_namespace(&self.asset_id));
        if let Err(e) = nonce_registry.observe(NonceEventKind::Payment, &pk, &self.nonce.0) {
            if matches!(e, NonceError::AlreadyObserved { .. }) {
                return PaymentDecision::Reject {
                    reason: PaymentRejectionReason::Replay,
                };
            }
            // Other errors (PersistenceFailure, WalRecovering) propagate
            // as Replay — caller treats any observe failure as "cannot
            // authorize" per fail-closed posture (Mission E substrate-
            // fidelity reference: Mission F `BurnEventRef::validate`
            // fail-closed).
            return PaymentDecision::Reject {
                reason: PaymentRejectionReason::Replay,
            };
        }
        if self.is_expired(now_unix_ms) {
            return PaymentDecision::Reject {
                reason: PaymentRejectionReason::Expired,
            };
        }
        if !self.matches_model(query_model) {
            return PaymentDecision::Reject {
                reason: PaymentRejectionReason::ModelMismatch,
            };
        }
        if self.budget.value == 0 {
            return PaymentDecision::Reject {
                reason: PaymentRejectionReason::BudgetExhausted,
            };
        }
        if dqa_cmp(query_cost, self.budget) > 0 {
            return PaymentDecision::Reject {
                reason: PaymentRejectionReason::CostExceedsBudget,
            };
        }
        PaymentDecision::Proceed {
            remaining_budget: dqa_sub(self.budget, query_cost).expect("guarded by dqa_cmp"),
        }
    }

    /// Validate a caveat against registry + nonce state WITHOUT
    /// committing the nonce (uses `observe_readonly`).
    ///
    /// Same gates 0-2 as `verify()` + gate 3 (anti-replay via
    /// `observe_readonly`). Used for audit-batch pre-flight per
    /// RFC-0105 §3.13 + RFC-0965 §2.3.
    pub fn validate(
        &self,
        current_epoch: Epoch,
        registry: &dyn AssetRegistry,
        nonce_registry: &dyn NonceRegistry,
    ) -> Result<(), PaymentRejectionReason> {
        let meta = registry
            .metadata(&self.asset_id)
            .map_err(|_| PaymentRejectionReason::AssetUnknown)?;
        if self.budget.scale != meta.wire_scale {
            return Err(PaymentRejectionReason::ScaleMismatch {
                caveat_scale: self.budget.scale,
                query_cost_scale: meta.wire_scale,
            });
        }
        if current_epoch.0 < self.registry_snapshot_epoch.0 {
            return Err(PaymentRejectionReason::StaleSnapshot {
                snapshot: self.registry_snapshot_epoch.0,
                live: current_epoch.0,
            });
        }
        let pk = meta
            .governance_pubkey
            .unwrap_or_else(|| sovereign_nonce_namespace(&self.asset_id));
        if let Err(e) = nonce_registry.observe_readonly(NonceEventKind::Payment, &pk, &self.nonce.0)
        {
            if matches!(e, NonceError::AlreadyObserved { .. }) {
                return Err(PaymentRejectionReason::Replay);
            }
            // Other errors propagate as Replay (fail-closed).
            return Err(PaymentRejectionReason::Replay);
        }
        Ok(())
    }
}

/// Decision returned by `PaymentCaveat::verify` (RFC-0965 rename
/// from `PaidQueryDecision` per §2.3). The legacy type alias
/// `PaidQueryDecision = PaymentDecision` is preserved at the end of
/// this module for backward compat.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PaymentDecision {
    /// Query is authorized; remaining budget after deduction.
    Proceed {
        /// Remaining budget after this query (Dqa at scale=0).
        #[serde(with = "dqa_serde::field")]
        remaining_budget: Dqa,
    },
    /// Query exceeds caveat budget; caller may downgrade model.
    Partial {
        /// Highest cost the verifier will accept (`caveat.budget`).
        #[serde(with = "dqa_serde::field")]
        max_allowed_cost: Dqa,
    },
    /// Query is rejected with a discriminator reason.
    Reject {
        /// Rejection reason discriminator.
        reason: PaymentRejectionReason,
    },
}

impl PaymentDecision {
    /// True if the decision authorizes the query (`Proceed`).
    #[must_use]
    pub fn is_proceed(&self) -> bool {
        matches!(self, Self::Proceed { .. })
    }
}

/// Reason a paid-query was rejected (RFC-0965 §2.3).
///
/// `#[non_exhaustive]` per CLAUDE.md §Architectural Principles
/// "Extension over enumeration": Layer B substrate enums MUST be
/// non-exhaustive so additive variants (RFC-driven evolution) do
/// NOT force every downstream consumer to a central edit.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum PaymentRejectionReason {
    /// `caveat.budget == 0` (no prepaid capacity left).
    BudgetExhausted,
    /// `now_unix_ms > caveat.expires_at_unix_ms`.
    Expired,
    /// `query_model` does not match caveat's `model` scope (and
    /// caveat is not a wildcard).
    ModelMismatch,
    /// `query_cost > caveat.budget` (over-budget; `Partial` candidate).
    CostExceedsBudget,
    /// `query_cost.wire_scale != self.budget.wire_scale` (RFC-0965
    /// §2.3). Mission E substrate-fidelity reference: scale-binding
    /// invariant at the substrate boundary.
    ScaleMismatch {
        /// The caveat's `budget.scale`.
        caveat_scale: u8,
        /// The query's `cost.scale`.
        query_cost_scale: u8,
    },
    /// `registry.metadata(self.asset_id)` returned `Err` (asset not
    /// registered OR tombstoned).
    AssetUnknown,
    /// `current_epoch < self.registry_snapshot_epoch` (snapshot stale;
    /// caller must re-fetch registry state before retrying).
    StaleSnapshot {
        /// Caveat's registry_snapshot_epoch.
        snapshot: u64,
        /// Live current epoch at validation time.
        live: u64,
    },
    /// `(event_kind=Payment, pk, nonce)` triple was previously
    /// observed in the NonceRegistry (anti-replay trip).
    Replay,
    /// Legacy-form deserialization rejected: payload contained the
    /// legacy `amount_micro_octo_w` field AND `claimed_asset_id` is
    /// not OCTO-W.
    LegacyFormOnNonOctoWContext {
        /// The asset_id claimed by the legacy envelope (if parseable).
        claimed_asset_id: AssetId,
    },
}

/// Errors returned by `PaymentCaveat::attenuate` when the proposed
/// bounds would widen the parent caveat.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum AttenuationError {
    /// Proposed `budget > self.budget`. Attenuation must only narrow.
    #[error("budget widened: current={current:?}, proposed={proposed:?}")]
    BudgetWidened {
        /// Current budget (Dqa at scale=0).
        current: Dqa,
        /// Proposed (rejected) budget (Dqa at scale=0).
        proposed: Dqa,
    },
    /// Proposed `expires_at_unix_ms > self.expires_at_unix_ms`. The
    /// `u64::MAX` ↔ `u64::MAX` edge case (both "never expires") is
    /// permitted and does NOT trip this error.
    #[error("expiry widened: current={current}, proposed={proposed}")]
    ExpiryWidened {
        /// Current expiry (unix ms).
        current: u64,
        /// Proposed (rejected) expiry (unix ms).
        proposed: u64,
    },
    /// Proposed `new_asset_id != self.asset_id`. Attenuation cannot
    /// change the asset binding.
    #[error("asset mismatch: current={current:?}, proposed={proposed:?}")]
    AssetMismatch {
        /// Current asset_id.
        current: AssetId,
        /// Proposed (rejected) asset_id.
        proposed: AssetId,
    },
    /// Proposed `new_budget.scale != self.budget.scale` (or != meta.wire_scale).
    #[error("scale mismatch: current={current}, proposed={proposed}")]
    ScaleMismatch {
        /// Current wire_scale.
        current: u8,
        /// Proposed (rejected) wire_scale.
        proposed: u8,
    },
    /// Registry returned `Err(AssetError::AssetUnknown)` for
    /// `self.asset_id`.
    #[error("asset unknown: registry returned error for {0:?}")]
    AssetUnknown(AssetId),
}

/// Legacy `PaidQueryDecision` alias (RFC-0965 §2.3).
#[deprecated(note = "use PaymentDecision per RFC-0965 §2.3")]
pub type PaidQueryDecision = PaymentDecision;

/// Legacy `PaidQueryRejectionReason` alias (RFC-0965 §2.3).
#[deprecated(note = "use PaymentRejectionReason per RFC-0965 §2.3")]
pub type PaidQueryRejectionReason = PaymentRejectionReason;

// Manual `Eq` for PaymentRejectionReason: `AssetId` is a `[u8; 32]`
// wrapper which is `Copy + Eq`, so `PartialEq` derivation suffices.
// (No extra impl needed — `AssetId` derives `Eq` upstream.)

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AssetKind, AssetMetadata, InMemoryAssetRegistry, InMemoryNonceRegistry};

    /// Build a `Dqa` at `scale = 0` from an integer literal. Test
    /// helper (production code uses `Dqa::new(n, 0).expect(...)`).
    fn dqa(n: i64) -> Dqa {
        Dqa::new(n, 0).expect("scale=0 always valid")
    }

    fn octow_metadata() -> AssetMetadata {
        AssetMetadata {
            wire_scale: 0,
            display_decimals: 4,
            denomination: "OCTO-W".to_string(),
            symbol: "OCTO-W".to_string(),
            kind: AssetKind::OctoW,
            governance_pubkey: None,
            chain_id: None,
            asset_name: "cipherocto-octow".to_string(),
            tombstoned: false,
        }
    }

    fn registry_with(asset: AssetId, meta: AssetMetadata) -> InMemoryAssetRegistry {
        let mut reg = InMemoryAssetRegistry::new();
        reg.register(asset, meta);
        reg
    }

    fn sample(asset: AssetId, budget: i64, model: &str, expires: u64, epoch: u64) -> PaymentCaveat {
        PaymentCaveat::new(
            asset,
            dqa(budget),
            model,
            expires,
            Epoch::new(epoch),
            Nonce::from_bytes([3u8; 32]),
        )
    }

    #[test]
    fn canonical_name_is_paid_query_v1() {
        let c = sample(octo_w_asset_id(), 100, "gpt-4", u64::MAX, 0);
        assert_eq!(c.caveat_name, "paid-query/v1");
        assert_eq!(c.caveat_name, PAID_QUERY_CAVEAT_NAME);
    }

    #[test]
    fn is_expired_predicate() {
        let c = sample(octo_w_asset_id(), 100, "gpt-4", 1_000_000, 0);
        assert!(!c.is_expired(500_000));
        assert!(c.is_expired(1_000_001));
        let never = sample(octo_w_asset_id(), 100, "gpt-4", u64::MAX, 0);
        assert!(!never.is_expired(u64::MAX));
    }

    #[test]
    fn attenuate_4arg_narrows_budget_expiry_asset() {
        let asset = octo_w_asset_id();
        let reg = registry_with(asset, octow_metadata());
        let c = sample(asset, 1_000, "gpt-4", 2_000_000, 1);
        let narrower = c
            .attenuate(dqa(500), 1_500_000, asset, &reg)
            .expect("narrow");
        assert_eq!(narrower.budget.value, 500);
        assert_eq!(narrower.expires_at_unix_ms, 1_500_000);
        assert_eq!(narrower.asset_id.0, asset.0);
    }

    #[test]
    fn attenuate_4arg_rejects_asset_mismatch() {
        let asset_a = octo_w_asset_id();
        let asset_b = AssetId::from_bytes([0xAB; 32]);
        let reg = registry_with(asset_a, octow_metadata());
        let c = sample(asset_a, 100, "gpt-4", u64::MAX, 0);
        let err = c.attenuate(dqa(50), u64::MAX, asset_b, &reg).unwrap_err();
        assert!(matches!(err, AttenuationError::AssetMismatch { .. }));
    }

    #[test]
    fn attenuate_4arg_rejects_scale_mismatch() {
        let asset = octo_w_asset_id();
        let mut meta = octow_metadata();
        meta.wire_scale = 12;
        let reg = registry_with(asset, meta);
        let c = sample(asset, 100, "gpt-4", u64::MAX, 0);
        let different_scale = Dqa::new(50, 6).unwrap();
        let err = c
            .attenuate(different_scale, u64::MAX, asset, &reg)
            .unwrap_err();
        assert!(matches!(err, AttenuationError::ScaleMismatch { .. }));
    }

    #[test]
    fn attenuate_4arg_rejects_unknown_asset() {
        let asset = octo_w_asset_id();
        let reg = InMemoryAssetRegistry::new();
        let c = sample(asset, 100, "gpt-4", u64::MAX, 0);
        let err = c.attenuate(dqa(50), u64::MAX, asset, &reg).unwrap_err();
        assert!(matches!(err, AttenuationError::AssetUnknown(_)));
    }

    #[test]
    #[allow(deprecated)]
    fn legacy_3arg_construction_defaults_to_octo_w() {
        let c = PaymentCaveat::legacy_3arg(dqa(100), "gpt-4", u64::MAX);
        assert_eq!(c.asset_id.0, OCTO_W_ASSET_ID_BYTES);
    }

    #[test]
    #[allow(deprecated)]
    fn attenuate_legacy_2arg_still_works() {
        let asset = octo_w_asset_id();
        let _reg = registry_with(asset, octow_metadata());
        let c = PaymentCaveat::legacy_3arg(dqa(1_000), "gpt-4", 2_000_000);
        let narrower = c
            .attenuate_legacy_2arg(dqa(500), 1_500_000)
            .expect("legacy narrow");
        assert_eq!(narrower.budget.value, 500);
        assert_eq!(narrower.expires_at_unix_ms, 1_500_000);
    }

    #[test]
    fn verify_proceeds_when_budget_covers_cost() {
        let asset = octo_w_asset_id();
        let reg = registry_with(asset, octow_metadata());
        let mut nonce = InMemoryNonceRegistry::new();
        let c = sample(asset, 1_000, "gpt-4", u64::MAX, 0);
        let d = c.verify(dqa(250), "gpt-4", 0, Epoch::new(0), &reg, &mut nonce);
        assert_eq!(
            d,
            PaymentDecision::Proceed {
                remaining_budget: dqa(750)
            }
        );
    }

    #[test]
    fn verify_rejects_stale_snapshot() {
        let asset = octo_w_asset_id();
        let reg = registry_with(asset, octow_metadata());
        let mut nonce = InMemoryNonceRegistry::new();
        let c = sample(asset, 1_000, "gpt-4", u64::MAX, 10);
        let d = c.verify(dqa(10), "gpt-4", 0, Epoch::new(5), &reg, &mut nonce);
        assert_eq!(
            d,
            PaymentDecision::Reject {
                reason: PaymentRejectionReason::StaleSnapshot {
                    snapshot: 10,
                    live: 5
                }
            }
        );
    }

    #[test]
    fn verify_rejects_replay() {
        let asset = octo_w_asset_id();
        let reg = registry_with(asset, octow_metadata());
        let mut nonce = InMemoryNonceRegistry::new();
        let c = sample(asset, 1_000, "gpt-4", u64::MAX, 0);
        // First observe via verify() — commits nonce
        let _ = c.verify(dqa(10), "gpt-4", 0, Epoch::new(0), &reg, &mut nonce);
        // Second verify() observes same nonce → Replay
        let d = c.verify(dqa(10), "gpt-4", 0, Epoch::new(0), &reg, &mut nonce);
        assert_eq!(
            d,
            PaymentDecision::Reject {
                reason: PaymentRejectionReason::Replay
            }
        );
    }

    #[test]
    fn validate_does_not_commit_nonce() {
        let asset = octo_w_asset_id();
        let reg = registry_with(asset, octow_metadata());
        let nonce = InMemoryNonceRegistry::new();
        let c = sample(asset, 1_000, "gpt-4", u64::MAX, 0);
        c.validate(Epoch::new(0), &reg, &nonce)
            .expect("validate ok");
        // Second validate: nonce is NOT marked, so validate succeeds
        c.validate(Epoch::new(0), &reg, &nonce)
            .expect("validate idempotent");
    }

    #[test]
    fn validate_rejects_replay_when_already_observed() {
        let asset = octo_w_asset_id();
        let reg = registry_with(asset, octow_metadata());
        let mut nonce = InMemoryNonceRegistry::new();
        let c = sample(asset, 1_000, "gpt-4", u64::MAX, 0);
        // First commit via verify()
        let _ = c.verify(dqa(10), "gpt-4", 0, Epoch::new(0), &reg, &mut nonce);
        // Validate() observes readonly, sees AlreadyObserved → Replay
        let err = c.validate(Epoch::new(0), &reg, &nonce).unwrap_err();
        assert_eq!(err, PaymentRejectionReason::Replay);
    }

    #[test]
    fn payment_decision_is_proceed_predicate() {
        let p = PaymentDecision::Proceed {
            remaining_budget: dqa(0),
        };
        assert!(p.is_proceed());
        let r = PaymentDecision::Reject {
            reason: PaymentRejectionReason::BudgetExhausted,
        };
        assert!(!r.is_proceed());
    }
}
