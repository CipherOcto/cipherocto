//! `labels.create` — create a new label. Returns the server-assigned
//! label id (used by `labels.add_chat_label`, `labels.delete`, etc.).

use serde::Deserialize;
use serde_json::{json, Value};

use super::super::protocol::{RpcError, RpcErrorCode};
use super::super::server::RpcHandler;
use crate::daemon::DaemonHandle;

#[derive(Deserialize)]
struct Params {
    label_id: String,
    name: String,
    #[serde(default)]
    color: Option<i32>,
}

#[derive(Debug)]
pub struct LabelsCreate;

#[async_trait::async_trait]
impl RpcHandler for LabelsCreate {
    fn name(&self) -> &'static str {
        "labels.create"
    }

    async fn call(&self, h: DaemonHandle, params: Value) -> Result<Value, RpcError> {
        let p: Params = serde_json::from_value(params).map_err(|e| RpcError {
            code: RpcErrorCode::InvalidParams.as_i32(),
            message: format!("invalid params: {e}"),
            data: None,
        })?;
        if p.label_id.trim().is_empty() || p.name.trim().is_empty() {
            return Err(RpcError {
                code: RpcErrorCode::InvalidParams.as_i32(),
                message: "label_id and name must be non-empty".into(),
                data: None,
            });
        }
        let color = p.color.unwrap_or(0);
        let adapter = h.adapter().ok_or(RpcError {
            code: RpcErrorCode::NotConnected.as_i32(),
            message: "no adapter bound to daemon".into(),
            data: None,
        })?;
        adapter
            .create_label(&p.label_id, &p.name, color)
            .await
            .map_err(|e| RpcError {
                code: RpcErrorCode::InternalError.as_i32(),
                message: format!("labels.create failed: {e}"),
                data: Some(json!({"label_id": p.label_id, "name": p.name, "color": color})),
            })?;
        Ok(json!({
            "status": "created",
            "label_id": p.label_id,
            "name": p.name,
            "color": color,
        }))
    }
}
