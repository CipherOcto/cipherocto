//! `identity.get_pn` — return our PN (phone-number) JID as a string,
//! or `null` if the device is not signed in. Read from the in-memory
//! device snapshot — no WA server roundtrip.

use serde_json::{json, Value};

use super::super::protocol::{RpcError, RpcErrorCode};
use super::super::server::RpcHandler;
use crate::daemon::DaemonHandle;

#[derive(Debug)]
pub struct IdentityGetPn;

#[async_trait::async_trait]
impl RpcHandler for IdentityGetPn {
    fn name(&self) -> &'static str {
        "identity.get_pn"
    }

    async fn call(&self, h: DaemonHandle, _params: Value) -> Result<Value, RpcError> {
        let adapter = h.adapter().ok_or(RpcError {
            code: RpcErrorCode::NotConnected.as_i32(),
            message: "no adapter bound to daemon".into(),
            data: None,
        })?;
        let pn = adapter.get_pn().await.map_err(|e| RpcError {
            code: RpcErrorCode::InternalError.as_i32(),
            message: format!("identity.get_pn failed: {e}"),
            data: None,
        })?;
        Ok(json!({
            "pn": pn,
            "signed_in": pn.is_some(),
        }))
    }
}
