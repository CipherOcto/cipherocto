//! `media.fetch_sticker_pack` — fetch a first-party sticker pack by
//! its public `pack_id` from the WA CDN.
//!
//! Read-only: no `InboundEvent` is produced. Returns the flattened
//! pack metadata so the runtime caller does not need to know about
//! `wacore::sticker_pack::StickerPack`.

use serde::Deserialize;
use serde_json::{json, Value};

use super::super::protocol::{RpcError, RpcErrorCode};
use super::super::server::RpcHandler;
use crate::daemon::DaemonHandle;

#[derive(Deserialize)]
struct Params {
    pack_id: String,
    /// Locale for localized pack names; defaults to `"en"` when the
    /// caller omits the field (matches WA Web's default).
    #[serde(default = "default_locale")]
    locale: String,
}

fn default_locale() -> String {
    "en".to_string()
}

#[derive(Debug)]
pub struct MediaFetchStickerPack;

#[async_trait::async_trait]
impl RpcHandler for MediaFetchStickerPack {
    fn name(&self) -> &'static str {
        "media.fetch_sticker_pack"
    }

    async fn call(&self, h: DaemonHandle, params: Value) -> Result<Value, RpcError> {
        let p: Params = serde_json::from_value(params).map_err(|e| RpcError {
            code: RpcErrorCode::InvalidParams.as_i32(),
            message: format!("invalid params: {e}"),
            data: None,
        })?;
        if p.pack_id.trim().is_empty() {
            return Err(RpcError {
                code: RpcErrorCode::InvalidParams.as_i32(),
                message: "pack_id must be non-empty".into(),
                data: None,
            });
        }
        let adapter = h.adapter().ok_or(RpcError {
            code: RpcErrorCode::NotConnected.as_i32(),
            message: "no adapter bound to daemon".into(),
            data: None,
        })?;
        let pack = adapter
            .fetch_sticker_pack(&p.pack_id, &p.locale)
            .await
            .map_err(|e| RpcError {
                code: RpcErrorCode::InternalError.as_i32(),
                message: format!("adapter fetch_sticker_pack failed: {e}"),
                data: None,
            })?;
        Ok(json!({
            "status": "fetched",
            "pack_id": p.pack_id,
            "locale": p.locale,
            "pack": pack,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daemon::Daemon;
    use crate::test_mock_adapter::MockAdapter;
    use std::sync::Arc;

    fn handle() -> DaemonHandle {
        let tmp = tempfile::tempdir().expect("tempdir");
        Daemon::new_for_tests(tmp.path()).1
    }

    fn handle_with_mock() -> DaemonHandle {
        let h = handle();
        h.bind_adapter(Arc::new(MockAdapter::new()));
        h
    }

    #[tokio::test]
    async fn not_connected_returns_minus_32012() {
        let err = MediaFetchStickerPack
            .call(
                handle(),
                serde_json::json!({"pack_id": "abc", "locale": "en"}),
            )
            .await
            .unwrap_err();
        assert_eq!(err.code, RpcErrorCode::NotConnected.as_i32());
    }

    #[tokio::test]
    async fn empty_pack_id_returns_invalid_params() {
        let err = MediaFetchStickerPack
            .call(
                handle_with_mock(),
                serde_json::json!({"pack_id": "   ", "locale": "en"}),
            )
            .await
            .unwrap_err();
        assert_eq!(err.code, RpcErrorCode::InvalidParams.as_i32());
    }

    #[tokio::test]
    async fn default_locale_is_en_when_omitted() {
        let r = MediaFetchStickerPack
            .call(handle_with_mock(), serde_json::json!({"pack_id": "abc"}))
            .await
            .unwrap();
        assert_eq!(r["status"], "fetched");
        assert_eq!(r["locale"], "en");
        assert_eq!(r["pack"]["sticker_pack_id"], "fake-pack-id");
        assert_eq!(r["pack"]["name"], "Fake Pack");
    }

    #[tokio::test]
    async fn success_path_with_mock() {
        let r = MediaFetchStickerPack
            .call(
                handle_with_mock(),
                serde_json::json!({"pack_id": "abc", "locale": "pt-BR"}),
            )
            .await
            .unwrap();
        assert_eq!(r["status"], "fetched");
        assert_eq!(r["pack_id"], "abc");
        assert_eq!(r["locale"], "pt-BR");
        assert_eq!(r["pack"]["publisher"], "Fake Publisher");
        assert!(r["pack"]["stickers"].is_array());
    }
}
