//! `WALLET_PAID_QUERY_VERIFY` handler (RFC-0871 §Wallet Node Lifecycle,
//! mission 0871e-paid-query-caveat Phase 5).
//!
//! Receives: `PaidQueryRequest { macaroon_id, caveat, query_cost, query_model, now_unix_ms }`.
//! Returns:  `PaidQueryResponse { decision, macaroon_id, request_payload_kind }`.
//!
//! ## Layer boundary
//!
//! This handler delegates the **verification primitive** to
//! `octo_paid_query::verify_paid_query` (Layer E extension crate). It
//! does **not**:
//!
//! - Mutate any wallet state (Phase 5 MVP is read-only; the follow-on
//!   atomic-drain mission will plug `RateLimitBudget::try_deduct`
//!   into this flow).
//! - Reach into `octo-wallet` macaroon internals (the substrate lives
//!   in `octo-cap-macaroon` and the bridge crate `octo-paid-query`).
//! - Touch the network or storage (handler is pure).
//!
//! ## Why a new payload kind?
//!
//! `PAID_QUERY_VERIFY` (UUID `0x0009:0006:0000:0000:0000:0000:0000:0001`)
//! is a wallet-served verifier: the holder sends the wallet a macaroon
//! id + caveat and asks "may I spend this much on this model right
//! now?". The wallet returns a `PaidQueryDecision` that the holder's
//! routing layer (quota-router-core proxy) consumes to gate the
//! downstream provider call. The wallet is the authority on
//! `caveat.budget` semantics (it minted the caveat) — the proxy
//! merely enforces the decision.

use octo_paid_query::{verify_paid_query, PaidQueryDecision, PaidQueryRequest, PaidQueryResponse};
use octo_protocol::ProtocolError;

use super::HandlerOutput;

/// Request payload for `WALLET_PAID_QUERY_VERIFY`.
///
/// Wire form: `borsh::to_vec(&PaidQueryRequest)` — fixed-position
/// struct of `(macaroon_id, caveat, query_cost, query_model, now_unix_ms)`
/// per `octo-paid-query` §PaidQueryRequest.
pub type PaidQueryVerifyRequest = PaidQueryRequest;

/// `WALLET_PAID_QUERY_VERIFY` handler implementation.
///
/// Pure delegator: calls `verify_paid_query` and wraps the decision
/// in a `PaidQueryResponse` envelope. Phase 5 MVP carries no
/// `IdentityKey` dep (the handler does not sign anything — the
/// verifier decision is the response).
pub struct PaidQueryVerifyHandler;

impl PaidQueryVerifyHandler {
    /// Construct a new `PaidQueryVerifyHandler`.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    /// Verify the paid-query request and produce a response envelope.
    ///
    /// # Errors
    /// Returns `ProtocolError::AuthorizationFailed` if borsh decode
    /// of the response fails.
    pub fn handle(&self, req: &PaidQueryVerifyRequest) -> Result<HandlerOutput, ProtocolError> {
        let decision = verify_paid_query(
            &req.macaroon_id,
            &req.caveat,
            req.query_cost,
            &req.query_model,
            req.now_unix_ms,
        );
        let note = format!(
            "paid-query verify decision: {}",
            describe_decision(&decision)
        );
        let response = PaidQueryResponse {
            decision,
            macaroon_id: req.macaroon_id,
            request_payload_kind: octo_paid_query::PAID_QUERY_VERIFY,
        };
        let payload = response
            .to_borsh()
            .map_err(|e| ProtocolError::AuthorizationFailed(e.to_string()))?;
        Ok(HandlerOutput::response(payload, octo_paid_query::PAID_QUERY_VERIFY).with_note(note))
    }
}

impl Default for PaidQueryVerifyHandler {
    fn default() -> Self {
        Self::new()
    }
}

/// Human-readable description of a `PaidQueryDecision` for the
/// handler's log note. Mirrors the wire-form enum exactly; never
/// appears on the wire itself.
fn describe_decision(decision: &PaidQueryDecision) -> String {
    match decision {
        PaidQueryDecision::Proceed { remaining_budget } => {
            format!("proceed (remaining={remaining_budget})")
        }
        PaidQueryDecision::Partial { max_allowed_cost } => {
            format!("partial (max_allowed={max_allowed_cost})")
        }
        PaidQueryDecision::Reject { reason } => format!("reject ({reason:?})"),
    }
}

// `describe_decision` is only used by `handle` for the optional
// `note` field on `HandlerOutput`. Keep it local to avoid leaking
// into the public surface — handlers should not stringify decision
// reasons for caller consumption (callers decode the borsh payload
// for the typed enum).

#[cfg(test)]
mod tests {
    use super::*;
    use octo_paid_query::PaidQueryCaveat;

    fn sample_macaroon_id() -> [u8; 16] {
        let mut id = [0u8; 16];
        for (i, b) in id.iter_mut().enumerate() {
            *b = (i as u8).wrapping_add(1);
        }
        id
    }

    #[test]
    fn handler_proceeds_within_budget() {
        let req = PaidQueryRequest {
            macaroon_id: sample_macaroon_id(),
            caveat: PaidQueryCaveat::new(1_000, "gpt-4", u64::MAX),
            query_cost: 250,
            query_model: "gpt-4".to_string(),
            now_unix_ms: 0,
        };
        let out = PaidQueryVerifyHandler::new().handle(&req).unwrap();
        let payload = out.response_payload.expect("response_payload set");
        let response = PaidQueryResponse::from_borsh(&payload).unwrap();
        assert_eq!(response.macaroon_id, req.macaroon_id);
        assert!(matches!(
            response.decision,
            PaidQueryDecision::Proceed {
                remaining_budget: 750
            }
        ));
    }

    #[test]
    fn handler_rejects_expired_caveat() {
        let req = PaidQueryRequest {
            macaroon_id: sample_macaroon_id(),
            caveat: PaidQueryCaveat::new(1_000, "gpt-4", 100),
            query_cost: 10,
            query_model: "gpt-4".to_string(),
            now_unix_ms: 500,
        };
        let out = PaidQueryVerifyHandler::new().handle(&req).unwrap();
        let payload = out.response_payload.expect("response_payload set");
        let response = PaidQueryResponse::from_borsh(&payload).unwrap();
        assert!(matches!(
            response.decision,
            PaidQueryDecision::Reject { .. }
        ));
    }

    #[test]
    fn handler_emits_paid_query_verify_payload_kind() {
        let req = PaidQueryRequest {
            macaroon_id: sample_macaroon_id(),
            caveat: PaidQueryCaveat::new(1_000, "gpt-4", u64::MAX),
            query_cost: 10,
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
