//! `domain.compute-hash` — BLAKE3-256 deterministic domain-id
//! computation.
//!
//! Computes `blake3("whatsapp:" + jid.trim().to_lowercase())` and
//! returns the hex digest. The input must be either:
//!
//!   - Digits only (e.g. `"120363012345678901"`) — a bare group ID
//!     that will be promoted to `<digits>@g.us` semantics by the
//!     caller.
//!   - `<digits>@g.us` — a complete group JID.
//!
//! The normalization matches
//! `octo_adapter_whatsapp::WhatsAppWebAdapter::domain_hash_str`:
//!
//!   - `trim()` leading/trailing whitespace.
//!   - `to_lowercase()` (so `120363012345678901@G.US` and
//!     ` 120363012345678901@g.us ` produce the same hash).
//!
//! This is the runtime-facing façade for the same hash the
//! `BroadcastDomainId::from_jid` path computes internally. Exposing
//! it as an RPC lets external tools compute the canonical
//! `domain_id` for a group without instantiating the adapter.

use serde::Deserialize;
use serde_json::{json, Value};

use super::super::protocol::{RpcError, RpcErrorCode};
use super::super::server::RpcHandler;
use crate::daemon::DaemonHandle;

#[derive(Deserialize)]
struct Params {
    /// Group JID in either digits-only or `<digits>@g.us` form.
    jid: String,
}

#[derive(Debug)]
pub struct DomainComputeHash;

#[async_trait::async_trait]
impl RpcHandler for DomainComputeHash {
    fn name(&self) -> &'static str {
        "domain.compute-hash"
    }

    async fn call(&self, _h: DaemonHandle, params: Value) -> Result<Value, RpcError> {
        let p: Params = serde_json::from_value(params).map_err(|e| RpcError {
            code: RpcErrorCode::InvalidParams.as_i32(),
            message: format!("invalid params: {e}"),
            data: None,
        })?;

        // Normalize: trim + lowercase. Done BEFORE validation so a
        // mixed-case `@G.US` passes the digit-suffix check.
        let normalized = p.jid.trim().to_lowercase();
        validate_jid_shape(&normalized).map_err(|msg| RpcError {
            code: RpcErrorCode::InvalidParams.as_i32(),
            message: msg,
            data: Some(json!({
                "input": p.jid,
                "expected_format": "digits-only or <digits>@g.us",
            })),
        })?;

        // Compute BLAKE3-256 directly (don't require a bound adapter
        // — the hash is a pure function of the input and the prefix
        // `"whatsapp:"`, identical to
        // `WhatsAppWebAdapter::domain_hash_str`).
        let domain_id = octo_adapter_whatsapp::WhatsAppWebAdapter::domain_hash(&normalized);

        Ok(json!({
            "domain_id": hex_lower(&domain_id),
            "input": normalized,
        }))
    }
}

/// Validate the input is either:
///   - all ASCII digits, OR
///   - matches `^\d+@g\.us$`.
fn validate_jid_shape(s: &str) -> Result<(), String> {
    if s.is_empty() {
        return Err("jid must not be empty".to_string());
    }
    if let Some(local) = s.strip_suffix("@g.us") {
        if !local.is_empty() && local.chars().all(|c| c.is_ascii_digit()) {
            return Ok(());
        }
        return Err(format!(
            "jid {s:?} is not a valid <digits>@g.us form (local part must be digits)"
        ));
    }
    if s.chars().all(|c| c.is_ascii_digit()) {
        return Ok(());
    }
    Err(format!(
        "jid {s:?} is neither digits-only nor <digits>@g.us"
    ))
}

fn hex_lower(bytes: &[u8; 32]) -> String {
    let mut s = String::with_capacity(64);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::WhatsAppRuntimeConfig;
    use crate::daemon::Daemon;

    fn handle() -> DaemonHandle {
        let cfg = WhatsAppRuntimeConfig::from_toml(br#"name = "x""#).unwrap();
        Daemon::new(cfg).handle()
    }

    #[tokio::test]
    async fn digits_only_input_is_accepted() {
        let v = DomainComputeHash
            .call(handle(), serde_json::json!({ "jid": "1234567890" }))
            .await
            .unwrap();
        assert_eq!(v["domain_id"].as_str().unwrap().len(), 64);
        assert_eq!(v["input"], "1234567890");
    }

    #[tokio::test]
    async fn g_us_jid_is_accepted() {
        let v = DomainComputeHash
            .call(handle(), serde_json::json!({ "jid": "1234567890@g.us" }))
            .await
            .unwrap();
        assert_eq!(v["input"], "1234567890@g.us");
    }

    #[tokio::test]
    async fn whitespace_is_trimmed() {
        let v = DomainComputeHash
            .call(
                handle(),
                serde_json::json!({ "jid": "  1234567890@g.us  " }),
            )
            .await
            .unwrap();
        assert_eq!(v["input"], "1234567890@g.us");
    }

    #[tokio::test]
    async fn uppercase_g_us_is_normalized() {
        let v = DomainComputeHash
            .call(handle(), serde_json::json!({ "jid": "1234567890@G.US" }))
            .await
            .unwrap();
        assert_eq!(v["input"], "1234567890@g.us");
    }

    #[tokio::test]
    async fn rejects_non_group_jid() {
        let err = DomainComputeHash
            .call(
                handle(),
                serde_json::json!({ "jid": "user@s.whatsapp.net" }),
            )
            .await
            .unwrap_err();
        assert_eq!(err.code, -32602);
    }

    #[tokio::test]
    async fn rejects_empty_jid() {
        let err = DomainComputeHash
            .call(handle(), serde_json::json!({ "jid": "" }))
            .await
            .unwrap_err();
        assert_eq!(err.code, -32602);
    }

    #[tokio::test]
    async fn deterministic_for_same_input() {
        let a = DomainComputeHash
            .call(handle(), serde_json::json!({ "jid": "1234567890@g.us" }))
            .await
            .unwrap();
        let b = DomainComputeHash
            .call(handle(), serde_json::json!({ "jid": "1234567890@g.us" }))
            .await
            .unwrap();
        assert_eq!(a["domain_id"], b["domain_id"]);
    }
}
