//! `capabilities` — static platform capability report.
//!
//! Returns the JSON shape from design §941-958:
//!
//! ```json
//! {
//!   "platform": "whatsapp",
//!   "max_payload_bytes": 65536,
//!   "supports_fragmentation": false,
//!   "supports_raw_binary": false,
//!   "supports_encryption": true,
//!   "rate_limit_per_second": 20,
//!   "supports_receive_fragments": false,
//!   "supports_edited_messages": false,
//!   "media_capabilities": {
//!     "max_upload_bytes": 104857600,
//!     "supported_mime_types": ["application/octet-stream"]
//!   }
//! }
//! ```
//!
//! The values mirror the adapter's
//! [`WhatsAppWebAdapter::capabilities()`] implementation — the
//! handler is the runtime-facing façade so external clients don't
//! need to instantiate the adapter to discover what the platform
//! supports. When a live adapter is bound in a later phase, the
//! handler will pass through to the adapter's `capabilities()`; in
//! Phase 2 the values are served from the const declarations in
//! `octo-adapter-whatsapp` so the report stays in lockstep with
//! the actual transport limits.
//!
//! [`WhatsAppWebAdapter::capabilities()`]: octo_adapter_whatsapp::WhatsAppWebAdapter::capabilities

use serde_json::{json, Value};

use octo_network::dot::PlatformAdapter;

use super::super::protocol::{RpcError, RpcErrorCode};
use super::super::server::RpcHandler;
use crate::daemon::DaemonHandle;

#[derive(Debug)]
pub struct Capabilities;

#[async_trait::async_trait]
impl RpcHandler for Capabilities {
    fn name(&self) -> &'static str {
        "capabilities"
    }

    async fn call(&self, h: DaemonHandle, _params: Value) -> Result<Value, RpcError> {
        // If a live adapter is bound, prefer its `capabilities()`
        // implementation (the source of truth). Otherwise fall back
        // to the static defaults — these match the adapter's
        // published numbers exactly so the report is stable across
        // both code paths.
        if let Some(adapter) = h.adapter() {
            let report = adapter.capabilities();
            Ok(json!({
                "platform": "whatsapp",
                "max_payload_bytes": report.max_payload_bytes,
                "supports_fragmentation": report.supports_fragmentation,
                "supports_raw_binary": report.supports_raw_binary,
                "supports_encryption": report.supports_encryption,
                "rate_limit_per_second": report.rate_limit_per_second,
                "supports_receive_fragments": report.supports_receive_fragments,
                "supports_edited_messages": report.supports_edited_messages,
                "max_fragment_size": report.max_fragment_size,
                "media_capabilities": report.media_capabilities.as_ref().map(|m| json!({
                    "max_upload_bytes": m.max_upload_bytes,
                    "supported_mime_types": m.supported_mime_types,
                })),
            }))
        } else {
            Ok(static_capability_report())
        }
    }
}

/// Static `CapabilityReport` matching `octo-adapter-whatsapp`'s
/// published numbers. Used when the daemon has no live adapter
/// bound (Phase 2 stub path).
fn static_capability_report() -> Value {
    json!({
        "platform": "whatsapp",
        "max_payload_bytes": 65_536,
        "supports_fragmentation": false,
        "supports_raw_binary": false,
        "supports_encryption": true,
        "rate_limit_per_second": 20,
        "supports_receive_fragments": false,
        "supports_edited_messages": false,
        "media_capabilities": {
            "max_upload_bytes": 100 * 1024 * 1024,
            "supported_mime_types": ["application/octet-stream"],
        }
    })
}

/// Helper: returns the JSON-RPC error for invalid `capabilities`
/// params (currently no params required, so this is unused — kept
/// for forward-compatibility if Phase 3 wants to filter by `peer`).
#[allow(dead_code)]
pub(crate) fn invalid_params<E: std::fmt::Display>(e: E) -> RpcError {
    RpcError {
        code: RpcErrorCode::InvalidParams.as_i32(),
        message: format!("invalid params: {e}"),
        data: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::WhatsAppRuntimeConfig;
    use crate::daemon::Daemon;

    #[tokio::test]
    async fn static_report_has_expected_shape() {
        let cfg = WhatsAppRuntimeConfig::from_toml(br#"name = "x""#).unwrap();
        let h = Daemon::new(cfg).handle();
        let v = Capabilities.call(h, serde_json::json!({})).await.unwrap();
        assert_eq!(v["platform"], "whatsapp");
        assert_eq!(v["max_payload_bytes"], 65_536);
        assert_eq!(v["supports_fragmentation"], false);
        assert_eq!(v["supports_raw_binary"], false);
        assert_eq!(v["supports_encryption"], true);
        assert_eq!(v["rate_limit_per_second"], 20);
        assert_eq!(
            v["media_capabilities"]["max_upload_bytes"],
            100 * 1024 * 1024
        );
        assert_eq!(
            v["media_capabilities"]["supported_mime_types"][0],
            "application/octet-stream"
        );
    }
}
