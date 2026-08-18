//! Paid-query caveat bridge (RFC-0871 §Implementation Phases Phase 5,
//! mission 0871e-paid-query-caveat).
//!
//! Layer E extension crate per [[cipherocto-design-principles]] —
//! per-extension crate pattern (RFC-0957 v2.0 §Per-Extension Crate
//! Layout). This crate owns the **paid-query caveat bridge** between
//! `octo-wallet` (capability substrate), `octo-cap-macaroon` (macaroon
//! crypto foundation), and `quota-router-core` (forwarding target).
//!
//! ## Scope (Mission 0871e Phase 5 MVP)
//!
//! This Phase 5 MVP delivers the bridge pattern only — it proves the
//! per-extension crate shape that future payment variants (subscription,
//! per-token billing, metered egress) will plug into. Specifically it
//! owns:
//!
//! 1. The `PaidQueryCaveat` data type — `(caveat_name, budget, model)`
//!    caveat composition per RFC-0965 reserved discriminator 0x1A.
//! 2. The `verify_paid_query` primitive — verifies a `MacaroonId` is
//!    associated with a caveat whose `budget >= query_cost` and
//!    returns a `PaidQueryDecision` (proceed / reject / partial).
//! 3. The `RateLimitBudget` rate-limit accumulator — per-holder spend
//!    tracker that the wallet uses to gate downstream calls.
//!
//! ## MVP disclosures
//!
//! Phase 5 MVP is intentionally minimal — the full RFC-0871 Phase 5
//! surface lives in follow-on missions:
//!
//! - `PaymentCaveat` composition into the `octo-cap-macaroon` caveat
//!   chain (RFC-0957 §Algorithms caveat verification step) lands in
//!   mission 0957 Phase 2 follow-on.
//! - `RouterAnnouncePayload::pricing_policy` extension + atomic drain
//!   (RFC-0862 atomic transaction substrate) lands in a follow-on
//!   mission once the quota-router proxy integration is wired.
//! - `PaymentReceipt` event emission lands alongside the atomic drain
//!   mission.
//!
//! ## Layer discipline
//!
//! This crate sits at **Layer E** (per-extension). It depends on:
//!
//! - Layer A: `blake3` (keyed-hash primitive).
//! - Layer 1: `octo-protocol` (payload-kind UUID + envelope types).
//! - Layer B: `octo-wallet`, `octo-ident` (capability + identity
//!   substrate — for `CapabilityToken` type reference + DID codec).
//! - Layer 4: `octo-cap-macaroon` (macaroon crypto foundation —
//!   `MacaroonId` alias + `hmac_blake3` primitive).
//!
//! The reverse direction is forbidden: this crate is consumed by
//! `octo-wallet-node` (Layer C wallet specialized node) via its
//! `PAID_QUERY_VERIFY` handler.
//!
//! ## Wire format
//!
//! `PaidQueryCaveat` + `PaidQueryDecision` wire forms are borsh-encoded
//! (`borsh = "=1.5.0"`, matching the wallet-node envelope pin).
//! The `caveat_name` field carries the RFC-0965 discriminator string
//! (`"paid-query/v1"`) so future variants can be distinguished on
//! decode without changing the borsh schema.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

use std::sync::Arc;

// `borsh` crate import REMOVED (mission 0862-c9 RETIRED). Borsh
// derives on `PaidQueryDecision`/`PaidQueryRequest`/`PaidQueryResponse`/
// `PaymentReceipt` were dropped because `Dqa` does not impl
// `BorshSerialize`/`BorshDeserialize` in the upstream git-dep
// `octo-determin` crate. The crate-level dependency is still
// retained for any future re-introduction when the substrate
// gains Borsh impls.
use octo_cap_macaroon::MacaroonId;
use octo_determin::{dqa_cmp, dqa_sub, Dqa};
use octo_ident::WireDid;
use octo_protocol::PayloadKindId;
use thiserror::Error;

/// Re-export of the paid-query caveat type from the macaroon substrate
/// (mission 0957-phase2b migration). The canonical home is now
/// `octo_cap_macaroon::PaymentCaveat`; this re-export preserves all
/// existing `octo-paid-query` call sites without churn.
///
/// Discriminator string is `PAID_QUERY_CAVEAT_NAME` (re-exported from
/// cap-macaroon).
pub use octo_cap_macaroon::PaymentCaveat as PaidQueryCaveat;

/// Re-export for backward compat. Canonical home is
/// `octo_cap_macaroon::PAID_QUERY_CAVEAT_NAME`.
pub use octo_cap_macaroon::PAID_QUERY_CAVEAT_NAME;

/// Paid-query verify payload-kind UUID (RFC-0871 §Wallet Node
/// Lifecycle, mission 0871e).
///
/// Re-exported from `octo-protocol::payload_kind::PAID_QUERY_VERIFY`
/// for ergonomic consumption by the wallet-node handler. The crate
/// root does not duplicate the constant; the re-export keeps the
/// dependency on `octo-protocol` minimal in calling code.
pub use octo_protocol::payload_kind::PAID_QUERY_VERIFY;

/// (2026-08-17) `MicroOctoW` type alias was RETIRED project-wide. Cost
/// arithmetic uses `octo_determin::Dqa` directly at `scale = 0`
/// (integer-valued) per RFC-0862 v2.0.3 + RFC-0965 §3. Arithmetic
/// uses `dqa_cmp`/`dqa_sub` (the `Dqa` type does not implement
/// `Ord`/`Sub` directly — see `determin/src/dqa.rs`).

/// Paid-query caveat (RFC-0965 reserved discriminator 0x1A).
///
/// **Mission 0957-phase2b:** migrated to the macaroon substrate
/// (`octo_cap_macaroon::PaymentCaveat`); this re-export preserves
/// all existing call sites. Methods (`new`, `is_expired`,
/// `matches_model`, `verify`, `attenuate`) live on the migrated
/// type. See `crates/octo-cap-macaroon/src/caveat/payment.rs` for
/// the canonical home.
/// Decision returned by `verify_paid_query` after comparing the
/// caveat against the proposed `query_cost`.
///
/// The verifier is **read-only** in Phase 5 MVP — the wallet applies
/// the decision via its own `RateLimitBudget` mutation. Follow-on
/// missions (atomic drain, RFC-0862) will return the mutation inside
/// the decision so the proxy can apply it transactionally.
//
// Borsh derives intentionally OMITTED (mission 0862-c9 RETIRED):
// `Dqa` does not impl `BorshSerialize`/`BorshDeserialize` in the
// upstream git-dep `octo-determin` crate. Adopting Borsh for `Dqa`
// is an additive Layer A change that requires pushing the determin
// crate to `next` first (deferred — not authorized today).
// Consumers needing borsh wire form for these structs must wait
// for the follow-on mission that ships `BorshSerialize`/`BorshDeserialize`
// impls for `Dqa` in the substrate. The non-borsh wire shape (JSON
// + canonical serde) is preserved by the upstream call sites.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PaidQueryDecision {
    /// Query is authorized; full cost can be deducted; remaining
    /// budget = `caveat.budget - query_cost`.
    Proceed {
        /// Remaining budget after this query (Dqa).
        remaining_budget: Dqa,
    },
    /// Query exceeds caveat budget but could proceed at a partial
    /// cost. `max_allowed_cost = caveat.budget` is the highest cost
    /// the verifier will accept; the caller (router) can either
    /// downgrade the model or reject.
    Partial {
        /// Highest cost the verifier will accept (caveat.budget).
        max_allowed_cost: Dqa,
    },
    /// Query is rejected. `reason` carries a discriminator byte so
    /// the caller can log + surface to the holder without parsing
    /// a free-form string.
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
//
// Borsh derives intentionally OMITTED (mission 0862-c9 RETIRED): the
// `borsh` crate import is currently unused in this module after
// dropping borsh on `Dqa`-bearing structs. The enum has no `Dqa`
// fields and could trivially re-add Borsh, but the import was
// dropped for consistency.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PaidQueryRejectionReason {
    /// `caveat.budget == 0` (no prepaid capacity left).
    BudgetExhausted,
    /// `now_unix_ms > caveat.expires_at_unix_ms`.
    Expired,
    /// `query_model` does not match caveat's `model` scope (and
    /// caveat is not a wildcard).
    ModelMismatch,
    /// `query_cost > caveat.budget` (and the caller did not ask for
    /// `Partial`).
    CostExceedsBudget,
}

/// Per-holder rate-limit accumulator (Phase 5 MVP).
///
/// Tracks prepaid spend per `(holder_did, macaroon_id)` so that
/// `verify_paid_query` can drain atomically. Delegates to a
/// `SpendLedger` backend (RFC-0862 atomic transaction substrate) —
/// default backend is `InMemorySpendLedger`; production deployments
/// inject a persistent ledger via `RateLimitBudget::with_ledger`.
///
/// All mutations flow through `try_deduct` which returns the
/// remaining balance (or a `PaidQueryError`). This is the single
/// entry point so persistence can be retrofitted without touching
/// call sites.
#[derive(Clone, Debug)]
pub struct RateLimitBudget {
    ledger: Arc<dyn SpendLedger>,
}

impl Default for RateLimitBudget {
    fn default() -> Self {
        Self::new()
    }
}

impl RateLimitBudget {
    /// Construct an in-memory budget (default backend).
    #[must_use]
    pub fn new() -> Self {
        Self {
            ledger: Arc::new(InMemorySpendLedger::new()),
        }
    }

    /// Construct a budget backed by an injected `SpendLedger`. Used by
    /// production deployments (Stoolap-backed ledger) and by tests
    /// that supply a custom backend.
    #[must_use]
    pub fn with_ledger(ledger: Arc<dyn SpendLedger>) -> Self {
        Self { ledger }
    }

    /// Seed the budget for a `(holder_did, macaroon_id)` pair. Used
    /// when the wallet provisions a capability — the caveat's budget
    /// is registered as the holder's prepaid balance.
    ///
    /// # Errors
    /// Returns `PaidQueryError::UnknownHolder` if the backend storage
    /// fails (storage errors are mapped to `UnknownHolder` so the
    /// handler fails closed).
    pub fn seed(
        &self,
        holder_did: &WireDid,
        macaroon_id: &MacaroonId,
        budget: Dqa,
    ) -> Result<(), PaidQueryError> {
        self.ledger
            .seed(holder_did, macaroon_id, budget)
            .map_err(Into::into)
    }

    /// Atomically deduct `cost` from the `(holder_did, macaroon_id)`
    /// balance. Returns the new remaining balance on success, or a
    /// `PaidQueryError` on insufficient balance / missing record /
    /// storage failure.
    ///
    /// # Errors
    /// - `PaidQueryError::UnknownHolder` if no balance exists for the
    ///   `(holder_did, macaroon_id)` pair (wallet must `seed` first)
    ///   OR if the backend storage fails (storage errors fail closed).
    /// - `PaidQueryError::InsufficientBalance` if balance < cost.
    pub fn try_deduct(
        &self,
        holder_did: &WireDid,
        macaroon_id: &MacaroonId,
        cost: Dqa,
    ) -> Result<Dqa, PaidQueryError> {
        self.ledger
            .try_deduct(holder_did, macaroon_id, cost)
            .map_err(Into::into)
    }

    /// Read-only balance lookup. Returns `None` if no record exists.
    ///
    /// # Errors
    /// Returns `PaidQueryError::UnknownHolder` on backend storage
    /// failure (fail closed).
    pub fn balance(
        &self,
        holder_did: &WireDid,
        macaroon_id: &MacaroonId,
    ) -> Result<Option<Dqa>, PaidQueryError> {
        self.ledger
            .balance(holder_did, macaroon_id)
            .map_err(Into::into)
    }
}

/// Verification + decision primitive (Phase 5 MVP).
///
/// Verifies that a `(macaroon_id, caveat)` pair authorizes a query
/// costing `query_cost` against `query_model`, returning a
/// `PaidQueryDecision`. The caller (wallet node) then applies the
/// decision — Phase 5 MVP is read-only, follow-on atomic-drain
/// mission will fold the drain into this function.
///
/// # Parameters
///
/// - `macaroon_id` — `MacaroonId` of the capability token being
///   verified (16 bytes, RFC-0957 §Wire Format).
/// - `caveat` — the `PaidQueryCaveat` being asserted (RFC-0965
///   reserved discriminator 0x1A).
/// - `query_cost` — proposed query cost in Dqa.
/// - `query_model` — proposed model identifier; must match
///   `caveat.model` (unless caveat model is `""` wildcard).
/// - `now_unix_ms` — current time in unix milliseconds; used for
///   expiry check.
///
/// # Returns
///
/// `PaidQueryDecision::Proceed { remaining_budget }` if the query
/// is fully authorized; `Partial { max_allowed_cost }` if the
/// query exceeds budget but a partial-cost downgrade is possible;
/// `Reject { reason }` if the caveat is expired, exhausted, or
/// mismatched.
#[must_use]
pub fn verify_paid_query(
    macaroon_id: &MacaroonId,
    caveat: &PaidQueryCaveat,
    query_cost: Dqa,
    query_model: &str,
    now_unix_ms: u64,
) -> PaidQueryDecision {
    // Defensive: macaroon_id must be non-zero (all-zero would
    // indicate an uninitialised identifier from a buggy caller).
    if macaroon_id.iter().all(|b| *b == 0) {
        return PaidQueryDecision::Reject {
            reason: PaidQueryRejectionReason::BudgetExhausted,
        };
    }

    // Expiry gate first — never spend against an expired caveat
    // (RFC-0871 §Adversary A10 — post-expiry queries).
    if caveat.is_expired(now_unix_ms) {
        return PaidQueryDecision::Reject {
            reason: PaidQueryRejectionReason::Expired,
        };
    }

    // Model scope gate — caveat binds to a specific model (or "").
    if !caveat.matches_model(query_model) {
        return PaidQueryDecision::Reject {
            reason: PaidQueryRejectionReason::ModelMismatch,
        };
    }

    // Budget gate — query must fit within caveat budget.
    if caveat.budget.value == 0 {
        return PaidQueryDecision::Reject {
            reason: PaidQueryRejectionReason::BudgetExhausted,
        };
    }
    if dqa_cmp(query_cost, caveat.budget) > 0 {
        return PaidQueryDecision::Partial {
            max_allowed_cost: caveat.budget,
        };
    }

    // dqa_sub returns `Result` — for `scale = 0` operands with
    // `cost <= budget` (guarded by dqa_cmp above) it is always `Ok`.
    PaidQueryDecision::Proceed {
        remaining_budget: dqa_sub(caveat.budget, query_cost).expect("guarded by dqa_cmp"),
    }
}

/// Typed errors for `RateLimitBudget` operations.
#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum PaidQueryError {
    /// No balance record exists for the `(holder_did, macaroon_id)`
    /// pair. The wallet must `seed` the budget before queries can
    /// be deducted (typically happens at capability mint time per
    /// RFC-0957 §Algorithms).
    #[error("no balance record for holder/macaroon; wallet must seed before deduct")]
    UnknownHolder,
    /// Balance < proposed cost. Carries both numbers for caller
    /// diagnostics.
    #[error("insufficient balance: balance={balance:?}, cost={cost:?}")]
    InsufficientBalance {
        /// Current balance (Dqa).
        balance: Dqa,
        /// Proposed cost (Dqa).
        cost: Dqa,
    },
}

pub mod ledger;
pub use ledger::{InMemorySpendLedger, SpendLedger, SpendLedgerError};

/// Re-export the wallet `CapabilityToken` type so downstream crates
/// (wallet-node handler) can refer to the full capability substrate
/// without reaching into the wallet crate's `capability::` module
/// path. This keeps the public surface of the paid-query extension
/// stable across future wallet re-organisations.
pub use octo_wallet::capability::CapabilityToken;

/// Re-export the wallet `CapabilityKey` so the wallet-node handler
/// can sign caveats with the holder's capability key without
/// importing from the wallet root.
pub use octo_wallet::CapabilityKey;

/// Convenience: build a `PAID_QUERY_VERIFY`-shaped request envelope
/// payload (the `(macaroon_id, caveat, query_cost, query_model,
/// now_unix_ms)` tuple) as borsh bytes. The handler decodes with
/// `PaidQueryRequest::from_borsh`.
///
/// Borsh schema is fixed-position (struct-of-fields); order MUST be
/// `macaroon_id, caveat, query_cost, query_model, now_unix_ms`.
//
// Borsh derives intentionally OMITTED (mission 0862-c9 RETIRED):
// `Dqa` does not impl `BorshSerialize`/`BorshDeserialize` in the
// upstream git-dep `octo-determin` crate. See NOTE on
// `PaidQueryDecision` for context.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PaidQueryRequest {
    /// Macaroon identifier of the capability being verified.
    pub macaroon_id: MacaroonId,
    /// Caveat composition being asserted.
    pub caveat: PaidQueryCaveat,
    /// Proposed query cost (Dqa).
    pub query_cost: Dqa,
    /// Proposed model identifier.
    pub query_model: String,
    /// Current time (unix milliseconds) for expiry check.
    pub now_unix_ms: u64,
}

impl PaidQueryRequest {
    // Borsh methods intentionally OMITTED (mission 0862-c9 RETIRED,
    // follows from the dropped `BorshSerialize`/`BorshDeserialize`
    // derives — see NOTE on `PaidQueryDecision`). Callers needing the
    // borsh wire form for these structs await the follow-on mission
    // that ships `Borsh` impls for `Dqa` upstream.
}

/// Wire form for the `PaidQueryDecision` response envelope. The
/// `PaidQueryDecision` enum is the canonical type, but the response
/// also carries the `payload_kind` for the originating request so
/// the receiver can route it back to the correct handler context.
//
// Borsh derives intentionally OMITTED (mission 0862-c9 RETIRED):
// `Dqa` does not impl `BorshSerialize`/`BorshDeserialize` in the
// upstream git-dep `octo-determin` crate. See NOTE on
// `PaidQueryDecision` for context.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PaidQueryResponse {
    /// Decision returned by `verify_paid_query`.
    pub decision: PaidQueryDecision,
    /// Echo of the request's `macaroon_id` (lets the caller correlate
    /// response ↔ request without re-encoding the caveat).
    pub macaroon_id: MacaroonId,
    /// Originating payload kind (always `PAID_QUERY_VERIFY` for
    /// Phase 5 MVP; kept in the response so future variants can
    /// multiplex without rewriting the handler dispatch).
    pub request_payload_kind: PayloadKindId,
    /// Atomic-drain receipt (mission 0871e-phase5b). Carries the
    /// amount deducted from the spend ledger on a `Proceed`
    /// decision and the remaining balance post-drain. On
    /// `Partial` / `Reject` the receipt reports `drained_amount = 0`
    /// and the prior balance (no mutation occurred).
    pub receipt: PaymentReceipt,
}

impl PaidQueryResponse {
    // Borsh methods intentionally OMITTED (mission 0862-c9 RETIRED,
    // follows from the dropped `BorshSerialize`/`BorshDeserialize`
    // derives — see NOTE on `PaidQueryDecision`). Callers needing the
    // borsh wire form for these structs await the follow-on mission
    // that ships `Borsh` impls for `Dqa` upstream.
}

/// Atomic-drain receipt (RFC-0862 atomic transaction substrate,
/// mission 0871e-phase5b).
///
/// Carries the post-drain state for the `(holder_did, macaroon_id)`
/// spend-ledger entry. On `Proceed`, `drained_amount > 0` and
/// `remaining_budget < pre_drain_budget`. On `Partial` / `Reject`,
/// `drained_amount == 0` and `remaining_budget == pre_drain_budget`
/// (no mutation occurred).
///
/// `drained_amount` is a `u128` to mirror the spend-ledger's
/// arithmetic type (RFC-0871 §Adversary A7 — overflow impossible at
/// worst-case scale).
//
// Borsh derives intentionally OMITTED (mission 0862-c9 RETIRED):
// `Dqa` does not impl `BorshSerialize`/`BorshDeserialize` in the
// upstream git-dep `octo-determin` crate. See NOTE on
// `PaidQueryDecision` for context.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PaymentReceipt {
    /// Amount deducted from the spend ledger on this call. Zero on
    /// non-`Proceed` decisions.
    pub drained_amount: Dqa,
    /// Remaining balance AFTER the drain. Equals `pre_drain_budget`
    /// when no drain occurred (decision was `Partial` / `Reject`).
    pub remaining_budget: Dqa,
}

impl PaymentReceipt {
    /// Construct a no-drain receipt (decision was `Partial` or
    /// `Reject`; ledger unchanged).
    ///
    /// `drained_amount` is the canonical zero (RFC-0862 v2.0.3 §3
    /// — `CANONICAL_ZERO`). The field is set via the exposed
    /// public-field initialization since `Dqa::new` is not a
    /// `const fn` (returns `Result`).
    #[must_use]
    pub const fn no_drain(remaining_budget: Dqa) -> Self {
        Self {
            drained_amount: Dqa { value: 0, scale: 0 },
            remaining_budget,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_macaroon_id() -> MacaroonId {
        let mut id = [0u8; 16];
        for (i, b) in id.iter_mut().enumerate() {
            *b = (i as u8).wrapping_add(1);
        }
        id
    }

    fn sample_holder() -> WireDid {
        WireDid::new("did:octo:zTestHolderPaidQuery".to_string())
    }

    /// Test helper: build a `Dqa` at `scale = 0` from an integer literal.
    fn dqa(n: i64) -> Dqa {
        Dqa::new(n, 0).expect("scale=0 always valid")
    }

    fn fresh_caveat(budget: i64, model: &str, expires_at: u64) -> PaidQueryCaveat {
        PaidQueryCaveat::new(dqa(budget), model, expires_at)
    }

    #[test]
    fn caveat_constructor_sets_canonical_name() {
        let c = PaidQueryCaveat::new(dqa(1_000_000), "gpt-4", u64::MAX);
        assert_eq!(c.caveat_name, PAID_QUERY_CAVEAT_NAME);
        assert_eq!(c.caveat_name, "paid-query/v1");
        assert_eq!(c.budget.value, 1_000_000);
        assert_eq!(c.budget.scale, 0);
        assert_eq!(c.model, "gpt-4");
        assert_eq!(c.expires_at_unix_ms, u64::MAX);
    }

    #[test]
    fn caveat_expiry_predicate() {
        let c = fresh_caveat(100, "gpt-4", 1_000_000);
        assert!(!c.is_expired(500_000));
        assert!(!c.is_expired(999_999));
        assert!(!c.is_expired(1_000_000));
        assert!(c.is_expired(1_000_001));
        // u64::MAX = never expires.
        let never = fresh_caveat(100, "gpt-4", u64::MAX);
        assert!(!never.is_expired(u64::MAX));
        assert!(!never.is_expired(0));
    }

    #[test]
    fn caveat_model_match_includes_wildcard() {
        let specific = fresh_caveat(100, "gpt-4", u64::MAX);
        assert!(specific.matches_model("gpt-4"));
        assert!(!specific.matches_model("gpt-3.5"));
        let wildcard = fresh_caveat(100, "", u64::MAX);
        assert!(wildcard.matches_model("gpt-4"));
        assert!(wildcard.matches_model("anything"));
        assert!(wildcard.matches_model(""));
    }

    #[test]
    fn verify_proceeds_when_budget_covers_cost() {
        let mac = sample_macaroon_id();
        let c = fresh_caveat(1_000, "gpt-4", u64::MAX);
        let d = verify_paid_query(&mac, &c, dqa(250), "gpt-4", 0);
        assert!(d.is_proceed());
        assert_eq!(
            d,
            PaidQueryDecision::Proceed {
                remaining_budget: dqa(750)
            }
        );
    }

    #[test]
    fn verify_partial_when_cost_exceeds_budget() {
        let mac = sample_macaroon_id();
        let c = fresh_caveat(100, "gpt-4", u64::MAX);
        let d = verify_paid_query(&mac, &c, dqa(500), "gpt-4", 0);
        assert_eq!(
            d,
            PaidQueryDecision::Partial {
                max_allowed_cost: dqa(100)
            }
        );
    }

    #[test]
    fn verify_rejects_when_expired() {
        let mac = sample_macaroon_id();
        let c = fresh_caveat(1_000, "gpt-4", 100);
        let d = verify_paid_query(&mac, &c, dqa(10), "gpt-4", 200);
        assert_eq!(
            d,
            PaidQueryDecision::Reject {
                reason: PaidQueryRejectionReason::Expired
            }
        );
    }

    #[test]
    fn verify_rejects_when_model_mismatch() {
        let mac = sample_macaroon_id();
        let c = fresh_caveat(1_000, "gpt-4", u64::MAX);
        let d = verify_paid_query(&mac, &c, dqa(10), "claude-3-opus", 0);
        assert_eq!(
            d,
            PaidQueryDecision::Reject {
                reason: PaidQueryRejectionReason::ModelMismatch
            }
        );
    }

    #[test]
    fn verify_rejects_when_budget_exhausted() {
        let mac = sample_macaroon_id();
        let c = fresh_caveat(0, "gpt-4", u64::MAX);
        let d = verify_paid_query(&mac, &c, dqa(1), "gpt-4", 0);
        assert_eq!(
            d,
            PaidQueryDecision::Reject {
                reason: PaidQueryRejectionReason::BudgetExhausted
            }
        );
    }

    #[test]
    fn verify_rejects_zero_macaroon_id() {
        // All-zero macaroon_id is a defensive sentinel for an
        // uninitialised identifier — never authorise.
        let mac = [0u8; 16];
        let c = fresh_caveat(1_000, "gpt-4", u64::MAX);
        let d = verify_paid_query(&mac, &c, dqa(10), "gpt-4", 0);
        assert_eq!(
            d,
            PaidQueryDecision::Reject {
                reason: PaidQueryRejectionReason::BudgetExhausted
            }
        );
    }

    #[test]
    fn rate_limit_budget_seed_and_deduct() {
        let b = RateLimitBudget::new();
        let holder = sample_holder();
        let mac = sample_macaroon_id();
        b.seed(&holder, &mac, dqa(1_000)).unwrap();

        // First deduct succeeds; remaining = 750.
        let remaining = b.try_deduct(&holder, &mac, dqa(250)).unwrap();
        assert_eq!(remaining, dqa(750));

        // Second deduct brings remaining to 500.
        let remaining = b.try_deduct(&holder, &mac, dqa(250)).unwrap();
        assert_eq!(remaining, dqa(500));

        // Insufficient balance.
        let err = b.try_deduct(&holder, &mac, dqa(600)).unwrap_err();
        assert_eq!(
            err,
            PaidQueryError::InsufficientBalance {
                balance: dqa(500),
                cost: dqa(600)
            }
        );

        // Balance lookup unchanged after rejection.
        assert_eq!(b.balance(&holder, &mac).unwrap(), Some(dqa(500)));
    }

    #[test]
    fn rate_limit_budget_unknown_holder() {
        let b = RateLimitBudget::new();
        let holder = sample_holder();
        let mac = sample_macaroon_id();
        let err = b.try_deduct(&holder, &mac, dqa(1)).unwrap_err();
        assert_eq!(err, PaidQueryError::UnknownHolder);
        assert_eq!(b.balance(&holder, &mac).unwrap(), None);
    }

    #[test]
    fn rate_limit_budget_isolation_between_holders() {
        let b = RateLimitBudget::new();
        let h1 = WireDid::new("did:octo:zHolder1".to_string());
        let h2 = WireDid::new("did:octo:zHolder2".to_string());
        let mac = sample_macaroon_id();
        b.seed(&h1, &mac, dqa(100)).unwrap();
        b.seed(&h2, &mac, dqa(200)).unwrap();

        assert_eq!(b.try_deduct(&h1, &mac, dqa(50)).unwrap(), dqa(50));
        assert_eq!(b.try_deduct(&h2, &mac, dqa(50)).unwrap(), dqa(150));
        // h1 unaffected by h2's deduct.
        assert_eq!(b.balance(&h1, &mac).unwrap(), Some(dqa(50)));
        assert_eq!(b.balance(&h2, &mac).unwrap(), Some(dqa(150)));
    }

    // Borsh round-trip tests (`paid_query_request_borsh_round_trip` /
    // `paid_query_response_borsh_round_trip`) intentionally REMOVED
    // (mission 0862-c9 RETIRED). `BorshSerialize`/`BorshDeserialize`
    // for `Dqa` is an additive Layer A change awaiting the determin
    // substrate push. Re-add when the upstream borsh impls land.

    #[test]
    fn paid_query_payload_kind_is_the_documented_uuid() {
        // Defensive: keep the local re-export in lockstep with
        // octo-protocol so a future refactor of the protocol
        // payload_kind module surfaces immediately.
        let expected: [u8; 16] = [
            0x00, 0x09, 0x00, 0x06, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x01,
        ];
        assert_eq!(PAID_QUERY_VERIFY.as_bytes(), &expected);
    }
}
