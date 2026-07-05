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

    #[test]
    fn name_is_envelope_decode() {
        assert_eq!(EnvelopeDecode.name(), "envelope.decode");
    }
}
