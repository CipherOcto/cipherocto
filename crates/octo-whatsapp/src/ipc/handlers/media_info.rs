//! `media.info` — look up metadata for a stored media artifact by its
//! opaque `media_ref_token`.
//!
//! Phase 2 stub: returns `None` because no media metadata cache is wired
//! yet. Phase 3 owns media metadata persistence and will resolve tokens
//! against the StoolapStore-backed media table.

use serde::Deserialize;
use serde_json::{json, Value};

use super::super::protocol::{RpcError, RpcErrorCode};
use super::super::server::RpcHandler;
use crate::daemon::DaemonHandle;

#[derive(Deserialize)]
#[allow(dead_code)] // `media_ref_token` reserved for Phase 3 metadata lookup.
struct Params {
    media_ref_token: String,
}

#[derive(Debug)]
pub struct MediaInfo;

#[async_trait::async_trait]
impl RpcHandler for MediaInfo {
    fn name(&self) -> &'static str {
        "media.info"
    }

    async fn call(&self, _h: DaemonHandle, params: Value) -> Result<Value, RpcError> {
        let p: Params = serde_json::from_value(params).map_err(|e| RpcError {
            code: RpcErrorCode::InvalidParams.as_i32(),
            message: format!("invalid params: {e}"),
            data: None,
        })?;
        // TODO(phase3): wire to StoolapStore media table; until then we
        // resolve `media_ref_token` to null. This handler does NOT
        // consult the adapter (it's a stub for the metadata cache), so
        // no NotConnected guard is required.
        Ok(json!({
            "info": Value::Null,
            "media_ref_token": p.media_ref_token,
            "phase": "phase2",
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daemon::Daemon;

    #[tokio::test]
    async fn media_info_returns_null_in_phase2() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let h = Daemon::new_for_tests(tmp.path()).1;
        let v = MediaInfo
            .call(h, serde_json::json!({"media_ref_token": "abc"}))
            .await
            .unwrap();
        assert!(v["info"].is_null());
        assert_eq!(v["phase"], "phase2");
    }
}
