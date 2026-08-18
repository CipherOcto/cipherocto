//! Payment caveat — single-element budget composition for paid queries
//! (RFC-0871 §Implementation Phases Phase 5, RFC-0965 §3 reserved
//! discriminator `0x1A`).
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

use serde::{Deserialize, Serialize};

use octo_determin::{dqa_cmp, dqa_sub, Dqa};

use crate::dqa_serde;

/// RFC-0965 caveat discriminator string for the paid-query variant.
///
/// First slot in the `0x1A`-`0xCF` reserved range per RFC-0871
/// §Implementation Phases Phase 5.
pub const PAID_QUERY_CAVEAT_NAME: &str = "paid-query/v1";

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
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaymentCaveat {
    /// RFC-0965 caveat discriminator string. Always
    /// `"paid-query/v1"` for this variant; future variants carry
    /// distinct strings.
    pub caveat_name: String,
    /// Prepaid spend budget. Holder can spend up to this amount
    /// across all queries matching `model`. `Dqa` at `scale = 0`
    /// (integer micro-OCTO_W).
    #[serde(with = "dqa_serde::field")]
    pub budget: Dqa,
    /// Model identifier this caveat applies to (`"gpt-4"`,
    /// `"claude-3-opus"`, etc.). Empty string `""` means "any
    /// model".
    pub model: String,
    /// Unix-time millisecond expiry. `u64::MAX` means "never
    /// expires".
    pub expires_at_unix_ms: u64,
}

impl PaymentCaveat {
    /// Construct a new payment caveat with the canonical name.
    #[must_use]
    pub fn new(budget: Dqa, model: impl Into<String>, expires_at_unix_ms: u64) -> Self {
        Self {
            caveat_name: PAID_QUERY_CAVEAT_NAME.to_string(),
            budget,
            model: model.into(),
            expires_at_unix_ms,
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

    /// Narrow this caveat to a smaller budget and/or earlier expiry.
    ///
    /// Returns `Ok(PaymentCaveat)` with the narrower fields, or
    /// `Err(AttenuationError)` if either bound would widen. The
    /// `model` field is preserved (a payment caveat can only bind
    /// to the same model — widening to wildcard via attenuation is
    /// forbidden by RFC-0957 §3.5).
    ///
    /// # Errors
    /// `AttenuationError::BudgetWidened` if `new_budget > self.budget`.
    /// `AttenuationError::ExpiryWidened` if `new_expires_at_unix_ms >
    /// self.expires_at_unix_ms` (except `u64::MAX` self.expiry +
    /// `u64::MAX` new is allowed — both "never expires").
    pub fn attenuate(
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
            budget: new_budget,
            model: self.model.clone(),
            expires_at_unix_ms: new_expires_at_unix_ms,
        })
    }

    /// Verify a query proposal against this caveat. Returns the
    /// canonical `PaidQueryDecision` (Proceed / Partial / Reject).
    ///
    /// Decision rules (mirrors `octo-paid-query::verify_paid_query`
    /// semantics; the function moved here as part of phase2b):
    ///
    /// 1. Expiry gate first — never spend against an expired caveat.
    /// 2. Model scope gate — caveat binds to a specific model
    ///    (or "" wildcard).
    /// 3. Budget gate — `budget == 0` exhausts; `query_cost > budget`
    ///    yields `Partial { max_allowed_cost: budget }`.
    /// 4. Otherwise `Proceed { remaining_budget: budget - query_cost }`.
    #[must_use]
    pub fn verify(
        &self,
        query_cost: Dqa,
        query_model: &str,
        now_unix_ms: u64,
    ) -> PaidQueryDecision {
        if self.is_expired(now_unix_ms) {
            return PaidQueryDecision::Reject {
                reason: PaidQueryRejectionReason::Expired,
            };
        }
        if !self.matches_model(query_model) {
            return PaidQueryDecision::Reject {
                reason: PaidQueryRejectionReason::ModelMismatch,
            };
        }
        if self.budget.value == 0 {
            return PaidQueryDecision::Reject {
                reason: PaidQueryRejectionReason::BudgetExhausted,
            };
        }
        if dqa_cmp(query_cost, self.budget) > 0 {
            return PaidQueryDecision::Partial {
                max_allowed_cost: self.budget,
            };
        }
        // dqa_sub returns `Result` — for `scale = 0` operands with
        // `cost <= budget` it is always `Ok` (no underflow). Unwrap
        // is safe under the `dqa_cmp > 0` guard above.
        PaidQueryDecision::Proceed {
            remaining_budget: dqa_sub(self.budget, query_cost).expect("guarded by dqa_cmp"),
        }
    }
}

/// Decision returned by `PaymentCaveat::verify`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PaidQueryDecision {
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
        reason: PaidQueryRejectionReason,
    },
}

impl PaidQueryDecision {
    /// True if the decision authorizes the query (`Proceed`).
    #[must_use]
    pub fn is_proceed(&self) -> bool {
        matches!(self, Self::Proceed { .. })
    }
}

/// Reason a paid-query was rejected.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PaidQueryRejectionReason {
    /// `caveat.budget == 0` (no prepaid capacity left).
    BudgetExhausted,
    /// `now_unix_ms > caveat.expires_at_unix_ms`.
    Expired,
    /// `query_model` does not match caveat's `model` scope (and
    /// caveat is not a wildcard).
    ModelMismatch,
    /// `query_cost > caveat.budget`.
    CostExceedsBudget,
}

/// Errors returned by `PaymentCaveat::attenuate` when the proposed
/// bounds would widen the parent caveat.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
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
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a `Dqa` at `scale = 0` from an integer literal. Test
    /// helper (production code uses `Dqa::new(n, 0).expect(...)`).
    fn dqa(n: i64) -> Dqa {
        Dqa::new(n, 0).expect("scale=0 always valid")
    }

    fn sample(budget: i64, model: &str, expires: u64) -> PaymentCaveat {
        PaymentCaveat::new(dqa(budget), model, expires)
    }

    #[test]
    fn canonical_name_is_paid_query_v1() {
        let c = sample(100, "gpt-4", u64::MAX);
        assert_eq!(c.caveat_name, "paid-query/v1");
        assert_eq!(c.caveat_name, PAID_QUERY_CAVEAT_NAME);
    }

    #[test]
    fn is_expired_predicate() {
        let c = sample(100, "gpt-4", 1_000_000);
        assert!(!c.is_expired(500_000));
        assert!(!c.is_expired(999_999));
        assert!(!c.is_expired(1_000_000));
        assert!(c.is_expired(1_000_001));
        let never = sample(100, "gpt-4", u64::MAX);
        assert!(!never.is_expired(u64::MAX));
        assert!(!never.is_expired(0));
    }

    #[test]
    fn matches_model_includes_wildcard() {
        let specific = sample(100, "gpt-4", u64::MAX);
        assert!(specific.matches_model("gpt-4"));
        assert!(!specific.matches_model("gpt-3.5"));
        let wildcard = sample(100, "", u64::MAX);
        assert!(wildcard.matches_model("gpt-4"));
        assert!(wildcard.matches_model("anything"));
        assert!(wildcard.matches_model(""));
    }

    #[test]
    fn attenuate_narrows_budget_and_expiry() {
        let c = sample(1_000, "gpt-4", 2_000_000);
        let narrower = c.attenuate(dqa(500), 1_500_000).expect("narrow");
        assert_eq!(narrower.budget.value, 500);
        assert_eq!(narrower.budget.scale, 0);
        assert_eq!(narrower.expires_at_unix_ms, 1_500_000);
        assert_eq!(narrower.model, "gpt-4");
    }

    #[test]
    fn attenuate_rejects_budget_widening() {
        let c = sample(100, "gpt-4", u64::MAX);
        let err = c.attenuate(dqa(200), u64::MAX).unwrap_err();
        assert!(matches!(err, AttenuationError::BudgetWidened { .. }));
    }

    #[test]
    fn attenuate_rejects_expiry_widening() {
        let c = sample(100, "gpt-4", 1_000_000);
        let err = c.attenuate(dqa(100), 2_000_000).unwrap_err();
        assert!(matches!(err, AttenuationError::ExpiryWidened { .. }));
    }

    #[test]
    fn attenuate_allows_never_expires_to_never_expires() {
        let c = sample(100, "gpt-4", u64::MAX);
        let same = c.attenuate(dqa(100), u64::MAX).expect("same");
        assert_eq!(same.expires_at_unix_ms, u64::MAX);
    }

    #[test]
    fn verify_proceeds_when_budget_covers_cost() {
        let c = sample(1_000, "gpt-4", u64::MAX);
        let d = c.verify(dqa(250), "gpt-4", 0);
        assert_eq!(
            d,
            PaidQueryDecision::Proceed {
                remaining_budget: dqa(750)
            }
        );
    }

    #[test]
    fn verify_partial_when_cost_exceeds_budget() {
        let c = sample(100, "gpt-4", u64::MAX);
        let d = c.verify(dqa(500), "gpt-4", 0);
        assert_eq!(
            d,
            PaidQueryDecision::Partial {
                max_allowed_cost: dqa(100)
            }
        );
    }

    #[test]
    fn verify_rejects_expired() {
        let c = sample(1_000, "gpt-4", 100);
        let d = c.verify(dqa(10), "gpt-4", 200);
        assert_eq!(
            d,
            PaidQueryDecision::Reject {
                reason: PaidQueryRejectionReason::Expired
            }
        );
    }

    #[test]
    fn verify_rejects_model_mismatch() {
        let c = sample(1_000, "gpt-4", u64::MAX);
        let d = c.verify(dqa(10), "claude-3-opus", 0);
        assert_eq!(
            d,
            PaidQueryDecision::Reject {
                reason: PaidQueryRejectionReason::ModelMismatch
            }
        );
    }

    #[test]
    fn verify_rejects_budget_exhausted() {
        let c = sample(0, "gpt-4", u64::MAX);
        let d = c.verify(dqa(1), "gpt-4", 0);
        assert_eq!(
            d,
            PaidQueryDecision::Reject {
                reason: PaidQueryRejectionReason::BudgetExhausted
            }
        );
    }
}
