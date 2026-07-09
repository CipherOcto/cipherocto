//! `contacts.get_profile_picture` — fetch the profile-picture URL for a
//! peer. `preview = true` requests the thumbnail; `false` requests the
//! full image. Returns `null` URL when the peer has no picture (or has
//! hidden it via privacy).

use serde::Deserialize;
use serde_json::{json, Value};

use super::super::protocol::{RpcError, RpcErrorCode};
use super::super::server::RpcHandler;
use crate::daemon::DaemonHandle;

#[derive(Deserialize)]
struct Params {
    peer: String,
    #[serde(default)]
    preview: Option<bool>,
}

#[derive(Debug)]
pub struct ContactsGetProfilePicture;

#[async_trait::async_trait]
impl RpcHandler for ContactsGetProfilePicture {
    fn name(&self) -> &'static str {
        "contacts.get_profile_picture"
    }

    async fn call(&self, h: DaemonHandle, params: Value) -> Result<Value, RpcError> {
        let p: Params = serde_json::from_value(params).map_err(|e| RpcError {
            code: RpcErrorCode::InvalidParams.as_i32(),
            message: format!("invalid params: {e}"),
            data: None,
        })?;

        let jid = crate::jids::peer_to_jid(&p.peer).map_err(|e| RpcError {
            code: RpcErrorCode::InvalidParams.as_i32(),
            message: format!("invalid peer: {e}"),
            data: Some(json!({
                "expected_format": "E.164 or <digits>@s.whatsapp.net or <digits>@lid"
            })),
        })?;

        let adapter = h.adapter().ok_or(RpcError {
            code: RpcErrorCode::NotConnected.as_i32(),
            message: "no adapter bound to daemon".into(),
            data: None,
        })?;
        let preview = p.preview.unwrap_or(true);
        let url = adapter
            .get_profile_picture_url(jid.as_str(), preview)
            .await
            .map_err(|e| RpcError {
                code: RpcErrorCode::InternalError.as_i32(),
                message: format!("contacts.get_profile_picture failed: {e}"),
                data: Some(json!({"peer": p.peer, "jid": jid, "preview": preview})),
            })?;

        Ok(json!({
            "peer": p.peer,
            "jid": jid,
            "preview": preview,
            "url": url,
            "found": url.is_some(),
        }))
    }
}
