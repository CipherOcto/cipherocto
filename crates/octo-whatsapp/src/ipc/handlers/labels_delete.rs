//! `labels.delete` — remove a label by id.

use serde::Deserialize;
use serde_json::{json, Value};

use super::super::protocol::{RpcError, RpcErrorCode};
use super::super::server::RpcHandler;
use crate::daemon::DaemonHandle;

#[derive(Deserialize)]
struct Params {
    label_id: String,
}

#[derive(Debug)]
pub struct LabelsDelete;

#[async_trait::async_trait]
impl RpcHandler for LabelsDelete {
    fn name(&self) -> &'static str {
        "labels.delete"
    }

    async fn call(&self, h: DaemonHandle, params: Value) -> Result<Value, RpcError> {
        let p: Params = serde_json::from_value(params).map_err(|e| RpcError {
            code: RpcErrorCode::InvalidParams.as_i32(),
            message: format!("invalid params: {e}"),
            data: None,
        })?;
        if p.label_id.trim().is_empty() {
            return Err(RpcError {
                code: RpcErrorCode::InvalidParams.as_i32(),
                message: "label_id cannot be empty".into(),
                data: None,
            });
        }
        let adapter = h.adapter().ok_or(RpcError {
            code: RpcErrorCode::NotConnected.as_i32(),
            message: "no adapter bound to daemon".into(),
            data: None,
        })?;
        adapter
            .delete_label(&p.label_id)
            .await
            .map_err(|e| RpcError {
                code: RpcErrorCode::InternalError.as_i32(),
                message: format!("labels.delete failed: {e}"),
                data: Some(json!({"label_id": p.label_id})),
            })?;
        Ok(json!({
            "status": "deleted",
            "label_id": p.label_id,
        }))
    }
}
