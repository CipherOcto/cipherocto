//! `version.get` — daemon API + binary version.

use serde_json::Value;

use super::super::protocol::{RpcError, RpcErrorCode};
use super::super::server::RpcHandler;
use crate::daemon::DaemonHandle;

#[derive(Debug)]
pub struct VersionGet;

#[async_trait::async_trait]
impl RpcHandler for VersionGet {
    fn name(&self) -> &'static str {
        "version.get"
    }

    async fn call(&self, _h: DaemonHandle, _params: Value) -> Result<Value, RpcError> {
        Ok(serde_json::json!({
            "daemon_api_version": "1.0.0+phase4",
            "daemon_binary_version": env!("CARGO_PKG_VERSION"),
            "phase": "phase3",
            "rpc_error_code_max": RpcErrorCode::ShuttingDown.as_i32(),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::WhatsAppRuntimeConfig;
    use crate::daemon::Daemon;

    #[tokio::test]
    async fn version_get_returns_phase3() {
        let cfg = WhatsAppRuntimeConfig::from_toml(br#"name = "x""#).unwrap();
        let h = Daemon::new(cfg).handle();
        let v = VersionGet.call(h, Value::Null).await.unwrap();
        assert_eq!(v["daemon_api_version"], "1.0.0+phase4");
        assert_eq!(v["phase"], "phase3");
        assert_eq!(
            v["daemon_binary_version"],
            env!("CARGO_PKG_VERSION"),
            "binary version echoes Cargo package version"
        );
    }
}
