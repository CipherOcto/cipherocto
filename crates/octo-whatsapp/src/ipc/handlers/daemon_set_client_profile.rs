//! `daemon.set_client_profile` — set the client profile presented
//! to WA on (re)connect.
//!
//! `platform` is one of `"web"` / `"android"` / `"smb_android"` /
//! `"ios"` / `"macos"` / `"windows"`. Other params default to the
//! platform's built-in values when omitted. Note: this RPC does
//! NOT trigger a reconnect — the new profile applies on the next
//! `daemon.start`.

use serde::Deserialize;
use serde_json::{json, Value};

use super::super::protocol::{RpcError, RpcErrorCode};
use super::super::server::RpcHandler;
use crate::daemon::DaemonHandle;

#[derive(Deserialize)]
struct Params {
    platform: String,
    #[serde(default)]
    os_version: Option<String>,
    #[serde(default)]
    manufacturer: Option<String>,
    #[serde(default)]
    locale_language: Option<String>,
    #[serde(default)]
    locale_country: Option<String>,
    #[serde(default)]
    passive_login: Option<bool>,
}

#[derive(Debug)]
pub struct DaemonSetClientProfile;

#[async_trait::async_trait]
impl RpcHandler for DaemonSetClientProfile {
    fn name(&self) -> &'static str {
        "daemon.set_client_profile"
    }

    async fn call(&self, h: DaemonHandle, params: Value) -> Result<Value, RpcError> {
        let p: Params = serde_json::from_value(params).map_err(|e| RpcError {
            code: RpcErrorCode::InvalidParams.as_i32(),
            message: format!("invalid params: {e}"),
            data: None,
        })?;
        let adapter = h.adapter().ok_or(RpcError {
            code: RpcErrorCode::NotConnected.as_i32(),
            message: "no adapter bound to daemon".into(),
            data: None,
        })?;
        adapter
            .set_client_profile(
                &p.platform,
                p.os_version.as_deref(),
                p.manufacturer.as_deref(),
                p.locale_language.as_deref(),
                p.locale_country.as_deref(),
                p.passive_login,
            )
            .await
            .map_err(|e| RpcError {
                code: RpcErrorCode::InternalError.as_i32(),
                message: format!("adapter set_client_profile failed: {e}"),
                data: None,
            })?;
        Ok(json!({"status": "set", "platform": p.platform}))
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
        let err = DaemonSetClientProfile
            .call(handle(), serde_json::json!({"platform": "web"}))
            .await
            .unwrap_err();
        assert_eq!(err.code, RpcErrorCode::NotConnected.as_i32());
    }

    #[tokio::test]
    async fn unknown_platform_accepted_by_mock() {
        // Note: the mock adapter does NOT validate the platform
        // string (returns Ok directly), so this test guards the
        // wiring while the real adapter's platform validation is
        // exercised by a dedicated integration test.
        let r = DaemonSetClientProfile
            .call(handle_with_mock(), serde_json::json!({"platform": "beos"}))
            .await
            .unwrap();
        assert_eq!(r["platform"], "beos");
    }

    #[tokio::test]
    async fn success_path_with_mock() {
        let r = DaemonSetClientProfile
            .call(
                handle_with_mock(),
                serde_json::json!({
                    "platform": "ios",
                    "os_version": "18.0",
                    "locale_language": "pt",
                    "locale_country": "BR",
                    "passive_login": true,
                }),
            )
            .await
            .unwrap();
        assert_eq!(r["status"], "set");
        assert_eq!(r["platform"], "ios");
    }
}
