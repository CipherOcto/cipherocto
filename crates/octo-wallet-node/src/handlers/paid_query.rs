//! `WALLET_PAID_QUERY_VERIFY` handler (RFC-0871 §Wallet Node Lifecycle,
//! mission 0871e-paid-query-caveat Phase 5).
//!
//! Receives: `PaidQueryRequest { macaroon_id, caveat, query_cost, query_model, now_unix_ms }`.
//! Returns:  `PaidQueryResponse { decision, macaroon_id, request_payload_kind, receipt }`.
//!
//! ## Phase 5 atomic drain (mission 0871e-phase5b)
//!
//! On a `Proceed` decision the handler atomically drains the
//! holder's spend ledger (`SpendLedger::try_deduct`) and emits a
//! `PaymentReceipt` carrying the post-drain state. On `Partial` /
//! `Reject` no drain occurs (the receipt reports `drained_amount = 0`).
//!
//! ## Layer boundary
//!
//! This handler delegates the **verification primitive** to
//! `octo_paid_query::verify_paid_query` (Layer E extension crate) and
//! the **drain primitive** to the injected `SpendLedger`. It does
//! **not**:
//!
//! - Reach into `octo-wallet` macaroon internals (the substrate lives
//!   in `octo-cap-macaroon` and the bridge crate `octo-paid-query`).
//! - Touch the network or storage (handler is pure given the ledger).
//!
//! ## Why a new payload kind?
//!
//! `PAID_QUERY_VERIFY` (UUID `0x0009:0006:0000:0000:0000:0000:0000:0001`)
//! is a wallet-served verifier: the holder sends the wallet a macaroon
//! id + caveat and asks "may I spend this much on this model right
//! now?". The wallet returns a `PaidQueryDecision` + `PaymentReceipt`
//! that the holder's routing layer (quota-router-core proxy) consumes
//! to gate the downstream provider call. The wallet is the authority
//! on `caveat.budget` semantics (it minted the caveat) — the proxy
//! merely enforces the decision.
//!
//! ## Wire form (mission 0862-c9 RETIRED)
//!
//! The typed `PaidQueryRequest` / `PaidQueryResponse` structs no
//! longer carry borsh derives (`Dqa` does not impl
//! `BorshSerialize` / `BorshDeserialize` in the upstream
//! `octo-determin` git dep). The wallet-node envelope uses JSON via
//! `paid_query_wire::request_to_bytes` / `response_to_bytes`.
//! `Dqa` encodes as `{"value": <i64>, "scale": <0..=255>}`; macaroon
//! / payload-kind UUIDs encode as 32-char hex strings.

use std::sync::Arc;

use octo_determin::Dqa;
use octo_ident::WireDid;
use octo_paid_query::{
    verify_paid_query, PaidQueryDecision, PaidQueryRequest, PaidQueryResponse, PaymentReceipt,
    SpendLedger,
};
use octo_protocol::ProtocolError;

use super::paid_query_wire;
use super::HandlerOutput;

/// Request payload for `WALLET_PAID_QUERY_VERIFY`.
///
/// Wire form: JSON — see `paid_query_wire::request_to_bytes` /
/// `request_from_bytes`. Fixed-position `{macaroon_id, caveat,
/// query_cost, query_model, now_unix_ms}` per `octo-paid-query`
/// §PaidQueryRequest.
pub type PaidQueryVerifyRequest = PaidQueryRequest;

/// `WALLET_PAID_QUERY_VERIFY` handler implementation.
///
/// Carries a `SpendLedger` slot for atomic drain. Production
/// deployments inject a Stoolap-backed ledger; tests use the
/// default `InMemorySpendLedger` via [`Self::new`].
pub struct PaidQueryVerifyHandler {
    ledger: Arc<dyn SpendLedger>,
}

impl PaidQueryVerifyHandler {
    /// Construct a new `PaidQueryVerifyHandler` with the default
    /// in-memory spend ledger.
    #[must_use]
    pub fn new() -> Self {
        Self::with_ledger(Arc::new(octo_paid_query::InMemorySpendLedger::new()))
    }

    /// Construct a handler backed by an injected `SpendLedger`. Used
    /// by production deployments (Stoolap-backed) and by tests that
    /// supply a custom backend.
    #[must_use]
    pub fn with_ledger(ledger: Arc<dyn SpendLedger>) -> Self {
        Self { ledger }
    }

    /// Verify the paid-query request and produce a response envelope.
    ///
    /// On `Proceed`, atomically drains `query_cost` from the spend
    /// ledger entry keyed by `(holder_did, macaroon_id)`. The
    /// `holder_did` is derived from the `caveat.audience` field if
    /// present, else falls back to an empty `WireDid` (the ledger
    /// returns `UnknownHolder` → the handler emits a no-drain
    /// receipt with `Reject::BudgetExhausted`).
    ///
    /// # Errors
    /// Returns `ProtocolError::AuthorizationFailed` if the JSON
    /// response payload fails to serialize.
    pub fn handle(&self, req: &PaidQueryVerifyRequest) -> Result<HandlerOutput, ProtocolError> {
        self.handle_with_holder(req, None)
    }

    /// Verify + drain variant that takes an explicit `holder_did`
    /// hint (used by the wallet's mint path so the spend ledger can
    /// be seeded at the same boundary that minted the caveat).
    ///
    /// # Errors
    /// Returns `ProtocolError::AuthorizationFailed` if the JSON
    /// response payload fails to serialize.
    pub fn handle_with_holder(
        &self,
        req: &PaidQueryVerifyRequest,
        holder_did_hint: Option<&WireDid>,
    ) -> Result<HandlerOutput, ProtocolError> {
        let decision = verify_paid_query(
            &req.macaroon_id,
            &req.caveat,
            req.query_cost,
            &req.query_model,
            req.now_unix_ms,
        );

        // Resolve holder_did: hint → empty fallback. The
        // `PaymentCaveat` does not currently carry an audience
        // field; callers that have a holder_did MUST supply it via
        // the hint (the wallet's mint path supplies it after
        // seeding the ledger).
        let holder_did = holder_did_hint
            .cloned()
            .unwrap_or_else(|| WireDid::new(String::new()));

        // Atomic drain on Proceed; otherwise no mutation. Failures
        // surface as a `Reject::BudgetExhausted` decision with a
        // no-drain receipt (the handler fails closed — the proxy
        // cannot proceed on an unknown holder).
        let (decision, receipt) = match decision.clone() {
            PaidQueryDecision::Proceed {
                remaining_budget: _,
            } => match self
                .ledger
                .try_deduct(&holder_did, &req.macaroon_id, req.query_cost)
            {
                Ok(new_remaining) => (
                    decision,
                    PaymentReceipt {
                        drained_amount: req.query_cost,
                        remaining_budget: new_remaining,
                    },
                ),
                Err(_) => (
                    PaidQueryDecision::Reject {
                        reason: octo_paid_query::PaidQueryRejectionReason::BudgetExhausted,
                    },
                    PaymentReceipt::no_drain(dqa_zero()),
                ),
            },
            other => {
                // Partial / Reject — no drain. Recover the pre-call
                // balance for the receipt (best-effort; missing
                // holder is reported as zero).
                let pre_balance = self
                    .ledger
                    .balance(&holder_did, &req.macaroon_id)
                    .ok()
                    .flatten()
                    .unwrap_or_else(dqa_zero);
                let receipt = match &other {
                    PaidQueryDecision::Partial { max_allowed_cost } => {
                        PaymentReceipt::no_drain(*max_allowed_cost)
                    }
                    PaidQueryDecision::Reject { .. } => PaymentReceipt::no_drain(pre_balance),
                    PaidQueryDecision::Proceed { .. } => unreachable!(),
                };
                (other, receipt)
            }
        };

        let note = format!(
            "paid-query verify decision: {} (drained={:?}, remaining={:?})",
            describe_decision(&decision),
            receipt.drained_amount,
            receipt.remaining_budget
        );
        let response = PaidQueryResponse {
            decision,
            macaroon_id: req.macaroon_id,
            request_payload_kind: octo_paid_query::PAID_QUERY_VERIFY,
            receipt,
        };
        let payload = paid_query_wire::response_to_bytes(&response)
            .map_err(|e| ProtocolError::AuthorizationFailed(e))?;
        Ok(HandlerOutput::response(payload, octo_paid_query::PAID_QUERY_VERIFY).with_note(note))
    }
}

impl Default for PaidQueryVerifyHandler {
    fn default() -> Self {
        Self::new()
    }
}

/// Canonical zero `Dqa` for code paths that need a default balance
/// (e.g. `unwrap_or` on a missing holder). `Dqa::new` is not a
/// `const fn` (returns `Result`), so the literal is the only
/// way to write this inline without a runtime unwrap.
///
/// Uses `Dqa { value: 0, scale: 0 }` per RFC-0862 v2.0.3 §3.
#[inline]
fn dqa_zero() -> Dqa {
    Dqa { value: 0, scale: 0 }
}

/// Human-readable description of a `PaidQueryDecision` for the
/// handler's log note. Mirrors the wire-form enum exactly; never
/// appears on the wire itself.
fn describe_decision(decision: &PaidQueryDecision) -> String {
    match decision {
        PaidQueryDecision::Proceed { remaining_budget } => {
            format!("proceed (remaining={remaining_budget:?})")
        }
        PaidQueryDecision::Partial { max_allowed_cost } => {
            format!("partial (max_allowed={max_allowed_cost:?})")
        }
        PaidQueryDecision::Reject { reason } => format!("reject ({reason:?})"),
    }
}

// `describe_decision` is only used by `handle` for the optional
// `note` field on `HandlerOutput`. Keep it local to avoid leaking
// into the public surface — handlers should not stringify decision
// reasons for caller consumption (callers decode the JSON payload
// for the typed enum).

#[cfg(test)]
mod tests {
    use super::*;
    use octo_paid_query::{PaidQueryCaveat, PaidQueryRejectionReason};

    fn sample_macaroon_id() -> [u8; 16] {
        let mut id = [0u8; 16];
        for (i, b) in id.iter_mut().enumerate() {
            *b = (i as u8).wrapping_add(1);
        }
        id
    }

    fn sample_holder() -> WireDid {
        WireDid::new("did:octo:zHolderDrain".to_string())
    }

    /// Test helper: build a `Dqa` at `scale = 0` from an integer literal.
    fn dqa(n: i64) -> Dqa {
        Dqa::new(n, 0).expect("scale=0 always valid")
    }

    /// Helper: build a handler with the in-memory ledger pre-seeded.
    fn handler_seeded(budget: i64) -> (PaidQueryVerifyHandler, [u8; 16]) {
        let mac = sample_macaroon_id();
        let ledger = Arc::new(octo_paid_query::InMemorySpendLedger::new());
        ledger.seed(&sample_holder(), &mac, dqa(budget)).unwrap();
        (PaidQueryVerifyHandler::with_ledger(ledger), mac)
    }

    #[test]
    fn handler_proceeds_within_budget() {
        let mac = sample_macaroon_id();
        let ledger = Arc::new(octo_paid_query::InMemorySpendLedger::new());
        ledger.seed(&sample_holder(), &mac, dqa(1_000)).unwrap();
        let handler = PaidQueryVerifyHandler::with_ledger(ledger);
        let req = PaidQueryRequest {
            macaroon_id: mac,
            caveat: PaidQueryCaveat::new(dqa(1_000), "gpt-4", u64::MAX),
            query_cost: dqa(250),
            query_model: "gpt-4".to_string(),
            now_unix_ms: 0,
        };
        let out = handler
            .handle_with_holder(&req, Some(&sample_holder()))
            .unwrap();
        let payload = out.response_payload.expect("response_payload set");
        let response = paid_query_wire::response_from_bytes(&payload).unwrap();
        assert_eq!(response.macaroon_id, req.macaroon_id);
        assert!(matches!(
            response.decision,
            PaidQueryDecision::Proceed { .. }
        ));
        if let PaidQueryDecision::Proceed { remaining_budget } = response.decision {
            assert_eq!(remaining_budget, dqa(750));
        }
    }

    /// TV1 (mission 0871e-phase5b) — `Proceed` decision drains:
    /// response carries `PaymentReceipt { drained_amount: 250,
    /// remaining_budget: 750 }`.
    #[test]
    fn handler_proceed_decision_drains_ledger() {
        let (handler, mac) = handler_seeded(1_000);
        let req = PaidQueryRequest {
            macaroon_id: mac,
            caveat: PaidQueryCaveat::new(dqa(1_000), "gpt-4", u64::MAX),
            query_cost: dqa(250),
            query_model: "gpt-4".to_string(),
            now_unix_ms: 0,
        };
        let out = handler
            .handle_with_holder(&req, Some(&sample_holder()))
            .unwrap();
        let payload = out.response_payload.expect("response_payload set");
        let response = paid_query_wire::response_from_bytes(&payload).unwrap();
        assert_eq!(
            response.receipt,
            PaymentReceipt {
                drained_amount: dqa(250),
                remaining_budget: dqa(750),
            }
        );
    }

    /// TV2 (mission 0871e-phase5b) — `Reject` decision no-drain:
    /// response carries `PaymentReceipt { drained_amount: 0,
    /// remaining_budget: <prior> }`.
    #[test]
    fn handler_reject_decision_does_not_drain() {
        let (handler, mac) = handler_seeded(1_000);
        let req = PaidQueryRequest {
            macaroon_id: mac,
            caveat: PaidQueryCaveat::new(dqa(1_000), "gpt-4", 100), // expires at 100
            query_cost: dqa(10),
            query_model: "gpt-4".to_string(),
            now_unix_ms: 500,
        };
        let out = handler
            .handle_with_holder(&req, Some(&sample_holder()))
            .unwrap();
        let payload = out.response_payload.expect("response_payload set");
        let response = paid_query_wire::response_from_bytes(&payload).unwrap();
        assert!(matches!(
            response.decision,
            PaidQueryDecision::Reject {
                reason: PaidQueryRejectionReason::Expired
            }
        ));
        assert_eq!(response.receipt.drained_amount, dqa_zero());
        assert_eq!(response.receipt.remaining_budget, dqa(1_000));
    }

    #[test]
    fn handler_rejects_expired_caveat() {
        let req = PaidQueryRequest {
            macaroon_id: sample_macaroon_id(),
            caveat: PaidQueryCaveat::new(dqa(1_000), "gpt-4", 100),
            query_cost: dqa(10),
            query_model: "gpt-4".to_string(),
            now_unix_ms: 500,
        };
        let out = PaidQueryVerifyHandler::new().handle(&req).unwrap();
        let payload = out.response_payload.expect("response_payload set");
        let response = paid_query_wire::response_from_bytes(&payload).unwrap();
        assert!(matches!(
            response.decision,
            PaidQueryDecision::Reject { .. }
        ));
    }

    #[test]
    fn handler_emits_paid_query_verify_payload_kind() {
        let req = PaidQueryRequest {
            macaroon_id: sample_macaroon_id(),
            caveat: PaidQueryCaveat::new(dqa(1_000), "gpt-4", u64::MAX),
            query_cost: dqa(10),
            query_model: "gpt-4".to_string(),
            now_unix_ms: 0,
        };
        let out = PaidQueryVerifyHandler::new().handle(&req).unwrap();
        assert_eq!(
            out.response_payload_kind,
            Some(octo_paid_query::PAID_QUERY_VERIFY)
        );
    }
}
