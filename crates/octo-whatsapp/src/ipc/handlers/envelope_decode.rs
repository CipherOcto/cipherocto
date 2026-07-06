//! `envelope.decode` — DOT/1/{base64url-no-pad} decode to wire bytes.
//!
//! Inverse of [`envelope_encode`]. Returns the wire bytes as a hex
//! string in `wire_hex` and a base64url-encoded copy in `wire_b64` so
//! the caller can pick whichever encoding round-trips cleanly into
//! their downstream transport without an extra base64 round.
//!
//! [`envelope_encode`]: super::envelope_encode

use serde::Deserialize;
use serde_json::{json, Value};

use base64::{engine::general_purpose::STANDARD as B64, Engine as _};

use super::super::protocol::{RpcError, RpcErrorCode};
use super::super::server::RpcHandler;
use crate::daemon::DaemonHandle;

#[derive(Deserialize)]
struct Params {
    /// DOT/1/{base64url-no-pad} string. Required.
    encoded: String,
}

#[derive(Debug)]
pub struct EnvelopeDecode;

#[async_trait::async_trait]
impl RpcHandler for EnvelopeDecode {
    fn name(&self) -> &'static str {
        "envelope.decode"
    }

    async fn call(&self, _h: DaemonHandle, params: Value) -> Result<Value, RpcError> {
        let p: Params = serde_json::from_value(params).map_err(|e| RpcError {
            code: RpcErrorCode::InvalidParams.as_i32(),
            message: format!("invalid params: {e}"),
            data: None,
        })?;

        let wire = octo_adapter_whatsapp::WhatsAppWebAdapter::decode_envelope(&p.encoded).map_err(
            |e| RpcError {
                code: RpcErrorCode::InvalidParams.as_i32(),
                message: format!("decode_envelope failed: {e}"),
                data: None,
            },
        )?;

        // Hex (lowercase, no separator) is the cheapest round-trip
        // for callers that just want bytes back without another
        // base64 step. `wire_b64` is the standard base64 form so
        // tools that already speak base64 can decode without
        // installing `hex`.
        let wire_hex = wire.iter().map(|b| format!("{b:02x}")).collect::<String>();
        let wire_b64 = B64.encode(&wire);

        Ok(json!({
            "wire_hex": wire_hex,
            "wire_b64": wire_b64,
            "wire_bytes": wire.len(),
        }))
    }
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

    #[test]
    fn name_is_envelope_decode() {
        assert_eq!(EnvelopeDecode.name(), "envelope.decode");
    }

    #[tokio::test]
    async fn invalid_params_returns_minus_32602() {
        // Missing required `encoded` field.
        let err = EnvelopeDecode
            .call(handle(), serde_json::json!({}))
            .await
            .unwrap_err();
        assert_eq!(err.code, RpcErrorCode::InvalidParams.as_i32());
    }

    #[tokio::test]
    async fn invalid_payload_returns_minus_32602() {
        // Garbage string without the DOT/1/ prefix.
        let err = EnvelopeDecode
            .call(handle(), serde_json::json!({"encoded": "not-an-envelope"}))
            .await
            .unwrap_err();
        assert_eq!(err.code, RpcErrorCode::InvalidParams.as_i32());
    }

    #[tokio::test]
    async fn success_path_returns_decoded_envelope() {
        // Build a valid DOT/1/{b64} envelope and decode it.
        let wire = vec![0xde, 0xad, 0xbe, 0xef, 0x01, 0x02];
        let encoded = octo_adapter_whatsapp::WhatsAppWebAdapter::encode_envelope(&wire);
        let r = EnvelopeDecode
            .call(handle(), serde_json::json!({"encoded": encoded}))
            .await
            .unwrap();
        assert_eq!(r["wire_bytes"], wire.len());
        assert_eq!(r["wire_hex"], "deadbeef0102");
        // Base64 must round-trip.
        let decoded_b64 = base64::engine::general_purpose::STANDARD
            .decode(r["wire_b64"].as_str().unwrap())
            .unwrap();
        assert_eq!(decoded_b64, wire);
    }
}
