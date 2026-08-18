//! `WALLET_PAID_QUERY_VERIFY` wire form (mission 0862-c9 RETIRED).
//!
//! The typed request/response structs in `octo_paid_query` no longer
//! carry borsh derives (`Dqa` does not impl `BorshSerialize` /
//! `BorshDeserialize` in the upstream `octo-determin` git dep). This
//! module defines the **JSON wire form** that the wallet-node
//! dispatch reads and writes at the envelope boundary, plus
//! converters to/from the typed structs.
//!
//! Wire form is JSON via `serde_json`. `Dqa` encodes as the canonical
//! object `{"value": <i64>, "scale": <u8>}` per RFC-0105 §3
//! (representation only — encoding is JSON, not the 16-byte BE
//! `DqaEncoding`). `MacaroonId = [u8; 16]` encodes as a 32-char hex
//! string. `PayloadKindId` encodes as a 32-char hex string of the
//! inner 16-byte UUID.
//!
//! ## Why JSON
//!
//! Borsh was the previous wire form (mission 0862-c9 RETIRED). JSON
//! is the next-best substrate that requires no upstream `Dqa`
//! trait impls and round-trips typed ↔ wire without bespoke
//! serializer code. The eventual re-introduction of borsh for the
//! `octo-paid-query` envelope awaits the follow-on mission that
//! ships `BorshSerialize`/`BorshDeserialize` for `Dqa` upstream.
use std::fmt::Write as _;

use octo_determin::Dqa;
use octo_paid_query::{
    PaidQueryDecision, PaidQueryRejectionReason, PaidQueryRequest, PaidQueryResponse,
    PaymentReceipt,
};
use serde_json::{json, Value};

/// Convert a `Dqa` to its JSON wire form.
fn dqa_to_json(d: Dqa) -> Value {
    json!({ "value": d.value, "scale": d.scale })
}

/// Convert a JSON `Dqa` wire form to a `Dqa`. The `value` field is
/// required to be `i64`; `scale` is `u8` (JSON clamps to `0..=255`
/// automatically).
fn dqa_from_json(v: &Value) -> Result<Dqa, String> {
    let obj = v
        .as_object()
        .ok_or_else(|| "expected Dqa object".to_owned())?;
    let value = obj
        .get("value")
        .and_then(Value::as_i64)
        .ok_or_else(|| "Dqa.value missing or not i64".to_owned())?;
    let scale = obj
        .get("scale")
        .and_then(Value::as_u64)
        .ok_or_else(|| "Dqa.scale missing or not u8".to_owned())?;
    if scale > u8::MAX as u64 {
        return Err(format!("Dqa.scale {scale} out of range"));
    }
    Dqa::new(value, scale as u8).map_err(|e| format!("Dqa::new: {e:?}"))
}

fn bytes_to_hex(bytes: &[u8; 16]) -> String {
    let mut s = String::with_capacity(32);
    for b in bytes {
        let _ = write!(s, "{b:02x}");
    }
    s
}

fn hex_to_bytes(s: &str) -> Result<[u8; 16], String> {
    if s.len() != 32 {
        return Err(format!(
            "expected 32-char hex string, got length {}",
            s.len()
        ));
    }
    let mut out = [0u8; 16];
    for (i, chunk) in s.as_bytes().chunks(2).enumerate() {
        let pair = std::str::from_utf8(chunk).map_err(|e| format!("hex utf8: {e}"))?;
        out[i] = u8::from_str_radix(pair, 16).map_err(|e| format!("hex parse: {e}"))?;
    }
    Ok(out)
}

/// Convert a `PaidQueryRequest` to its JSON wire form.
#[must_use]
pub fn request_to_value(req: &PaidQueryRequest) -> Value {
    json!({
        "macaroon_id": bytes_to_hex(&req.macaroon_id),
        "caveat": serde_json::to_value(&req.caveat).expect("PaymentCaveat is JSON-safe"),
        "query_cost": dqa_to_json(req.query_cost),
        "query_model": req.query_model,
        "now_unix_ms": req.now_unix_ms,
    })
}

/// Convert a `PaidQueryResponse` to its JSON wire form.
#[must_use]
pub fn response_to_value(resp: &PaidQueryResponse) -> Value {
    json!({
        "decision": decision_to_json(&resp.decision),
        "macaroon_id": bytes_to_hex(&resp.macaroon_id),
        "request_payload_kind": bytes_to_hex(&resp.request_payload_kind.0),
        "receipt": receipt_to_json(&resp.receipt),
    })
}

/// Convert a `PaidQueryDecision` to its JSON wire form.
fn decision_to_json(d: &PaidQueryDecision) -> Value {
    match d {
        PaidQueryDecision::Proceed { remaining_budget } => json!({
            "kind": "proceed",
            "remaining_budget": dqa_to_json(*remaining_budget),
        }),
        PaidQueryDecision::Partial { max_allowed_cost } => json!({
            "kind": "partial",
            "max_allowed_cost": dqa_to_json(*max_allowed_cost),
        }),
        PaidQueryDecision::Reject { reason } => json!({
            "kind": "reject",
            "reason": reason_tag(*reason),
        }),
    }
}

/// Encode the rejection reason as a stable string (so the wire form
/// does not break when the enum grows). Backwards-compatible with
/// `Debug` repr.
fn reason_tag(r: PaidQueryRejectionReason) -> &'static str {
    match r {
        PaidQueryRejectionReason::BudgetExhausted => "budget_exhausted",
        PaidQueryRejectionReason::Expired => "expired",
        PaidQueryRejectionReason::ModelMismatch => "model_mismatch",
        PaidQueryRejectionReason::CostExceedsBudget => "cost_exceeds_budget",
    }
}

fn receipt_to_json(r: &PaymentReceipt) -> Value {
    json!({
        "drained_amount": dqa_to_json(r.drained_amount),
        "remaining_budget": dqa_to_json(r.remaining_budget),
    })
}

/// Decode a `PaidQueryRequest` from a JSON wire form.
///
/// # Errors
/// Returns `String` describing the failure mode (missing field,
/// wrong type, out-of-range), suitable for surfacing as
/// `ProtocolError::AuthorizationFailed` at the dispatch boundary.
pub fn request_from_value(v: &Value) -> Result<PaidQueryRequest, String> {
    let obj = v
        .as_object()
        .ok_or_else(|| "expected request object".to_owned())?;
    let macaroon_id_hex = obj
        .get("macaroon_id")
        .and_then(Value::as_str)
        .ok_or_else(|| "macaroon_id missing or not string".to_owned())?;
    let macaroon_id = hex_to_bytes(macaroon_id_hex)?;
    let caveat_v = obj
        .get("caveat")
        .ok_or_else(|| "caveat missing".to_owned())?;
    let caveat: octo_paid_query::PaidQueryCaveat =
        serde_json::from_value(caveat_v.clone()).map_err(|e| format!("caveat: {e}"))?;
    let query_cost = dqa_from_json(
        obj.get("query_cost")
            .ok_or_else(|| "query_cost missing".to_owned())?,
    )?;
    let query_model = obj
        .get("query_model")
        .and_then(Value::as_str)
        .ok_or_else(|| "query_model missing or not string".to_owned())?
        .to_owned();
    let now_unix_ms = obj
        .get("now_unix_ms")
        .and_then(Value::as_u64)
        .ok_or_else(|| "now_unix_ms missing or not u64".to_owned())?;
    Ok(PaidQueryRequest {
        macaroon_id,
        caveat,
        query_cost,
        query_model,
        now_unix_ms,
    })
}

/// Decode a `PaidQueryResponse` from a JSON wire form.
///
/// # Errors
/// Returns `String` describing the failure mode.
pub fn response_from_value(v: &Value) -> Result<PaidQueryResponse, String> {
    let obj = v
        .as_object()
        .ok_or_else(|| "expected response object".to_owned())?;
    let macaroon_id = hex_to_bytes(
        obj.get("macaroon_id")
            .and_then(Value::as_str)
            .ok_or_else(|| "macaroon_id missing or not string".to_owned())?,
    )?;
    let pk_hex = obj
        .get("request_payload_kind")
        .and_then(Value::as_str)
        .ok_or_else(|| "request_payload_kind missing or not string".to_owned())?;
    let pk_bytes = hex_to_bytes(pk_hex)?;
    let request_payload_kind = octo_protocol::PayloadKindId(pk_bytes);
    let receipt_v = obj
        .get("receipt")
        .ok_or_else(|| "receipt missing".to_owned())?;
    let receipt = receipt_from_value(receipt_v)?;
    let decision_v = obj
        .get("decision")
        .ok_or_else(|| "decision missing".to_owned())?;
    let decision = decision_from_value(decision_v)?;
    Ok(PaidQueryResponse {
        decision,
        macaroon_id,
        request_payload_kind,
        receipt,
    })
}

fn receipt_from_value(v: &Value) -> Result<PaymentReceipt, String> {
    let obj = v
        .as_object()
        .ok_or_else(|| "expected receipt object".to_owned())?;
    let drained_amount = dqa_from_json(
        obj.get("drained_amount")
            .ok_or_else(|| "drained_amount missing".to_owned())?,
    )?;
    let remaining_budget = dqa_from_json(
        obj.get("remaining_budget")
            .ok_or_else(|| "remaining_budget missing".to_owned())?,
    )?;
    Ok(PaymentReceipt {
        drained_amount,
        remaining_budget,
    })
}

/// Reconstruct a `PaidQueryRejectionReason` from its wire string.
fn reason_from_tag(s: &str) -> Result<PaidQueryRejectionReason, String> {
    match s {
        "budget_exhausted" => Ok(PaidQueryRejectionReason::BudgetExhausted),
        "expired" => Ok(PaidQueryRejectionReason::Expired),
        "model_mismatch" => Ok(PaidQueryRejectionReason::ModelMismatch),
        "cost_exceeds_budget" => Ok(PaidQueryRejectionReason::CostExceedsBudget),
        other => Err(format!("unknown reason tag: {other}")),
    }
}

fn decision_from_value(v: &Value) -> Result<PaidQueryDecision, String> {
    let obj = v
        .as_object()
        .ok_or_else(|| "expected decision object".to_owned())?;
    let kind = obj
        .get("kind")
        .and_then(Value::as_str)
        .ok_or_else(|| "decision.kind missing".to_owned())?;
    match kind {
        "proceed" => {
            let remaining_budget = dqa_from_json(
                obj.get("remaining_budget")
                    .ok_or_else(|| "proceed.remaining_budget missing".to_owned())?,
            )?;
            Ok(PaidQueryDecision::Proceed { remaining_budget })
        }
        "partial" => {
            let max_allowed_cost = dqa_from_json(
                obj.get("max_allowed_cost")
                    .ok_or_else(|| "partial.max_allowed_cost missing".to_owned())?,
            )?;
            Ok(PaidQueryDecision::Partial { max_allowed_cost })
        }
        "reject" => {
            let reason = reason_from_tag(
                obj.get("reason")
                    .and_then(Value::as_str)
                    .ok_or_else(|| "reject.reason missing".to_owned())?,
            )?;
            Ok(PaidQueryDecision::Reject { reason })
        }
        other => Err(format!("unknown decision.kind: {other}")),
    }
}

/// Encode the `PaidQueryRequest` as JSON bytes (the dispatch wire
/// form for the wallet-node envelope).
///
/// # Errors
/// Returns `String` describing any serialization failure (none
/// expected under the current shape).
pub fn request_to_bytes(req: &PaidQueryRequest) -> Result<Vec<u8>, String> {
    serde_json::to_vec(&request_to_value(req)).map_err(|e| format!("request ser: {e}"))
}

/// Decode a `PaidQueryRequest` from the dispatch wire bytes.
///
/// # Errors
/// Returns `String` describing the failure mode.
pub fn request_from_bytes(bytes: &[u8]) -> Result<PaidQueryRequest, String> {
    let v: Value = serde_json::from_slice(bytes).map_err(|e| format!("request de: {e}"))?;
    request_from_value(&v)
}

/// Encode the `PaidQueryResponse` as JSON bytes (the response
/// payload carried by `HandlerOutput::response_payload`).
///
/// # Errors
/// Returns `String` describing any serialization failure.
pub fn response_to_bytes(resp: &PaidQueryResponse) -> Result<Vec<u8>, String> {
    serde_json::to_vec(&response_to_value(resp)).map_err(|e| format!("response ser: {e}"))
}

/// Decode a `PaidQueryResponse` from the response payload bytes.
///
/// # Errors
/// Returns `String` describing the failure mode.
pub fn response_from_bytes(bytes: &[u8]) -> Result<PaidQueryResponse, String> {
    let v: Value = serde_json::from_slice(bytes).map_err(|e| format!("response de: {e}"))?;
    response_from_value(&v)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dqa(n: i64) -> Dqa {
        Dqa::new(n, 0).expect("scale=0 always valid")
    }

    #[test]
    fn dqa_round_trips_through_json() {
        let v = dqa_to_json(dqa(1_234));
        assert_eq!(v["value"], 1_234);
        assert_eq!(v["scale"], 0);
        let back = dqa_from_json(&v).unwrap();
        assert_eq!(back.value, 1_234);
        assert_eq!(back.scale, 0);
    }

    #[test]
    fn hex_round_trips_16_bytes() {
        let bytes: [u8; 16] = [
            0x00, 0x09, 0x00, 0x06, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x01,
        ];
        let hex = bytes_to_hex(&bytes);
        assert_eq!(hex.len(), 32);
        let back = hex_to_bytes(&hex).unwrap();
        assert_eq!(back, bytes);
    }

    #[test]
    fn hex_to_bytes_rejects_bad_length() {
        assert!(hex_to_bytes("abc").is_err());
    }

    #[test]
    fn hex_to_bytes_rejects_invalid_chars() {
        assert!(hex_to_bytes(&"zz".repeat(16)).is_err());
    }
}
