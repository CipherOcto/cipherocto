//! JSON-RPC 2.0 protocol types. Newline-delimited JSON, one request and one
//! response per line. See RFC-RPC2 for the wire format (we are not embedding
//! the full spec here; this module is a strict subset of JSON-RPC 2.0).

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RpcRequest {
    pub id: u64,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RpcResponse {
    pub id: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<RpcError>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

/// Standard JSON-RPC 2.0 codes + CIPHEROCTO custom codes (-32001 .. -32099).
/// See design §Error codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum RpcErrorCode {
    // JSON-RPC 2.0 standard
    ParseError = -32700,
    InvalidRequest = -32600,
    MethodNotFound = -32601,
    InvalidParams = -32602,
    InternalError = -32603,

    // CipherOcto custom
    SessionLost = -32001,
    NotConfigured = -32002,
    RateLimited = -32003,
    PayloadTooLarge = -32004,
    /// Group-send attempted without admin/owner role.
    GroupNotAdmin = -32005,
    /// All fallback providers exhausted.
    FallbackExhausted = -32006,
    /// Media-upload pre-flight: in-flight upload semaphore saturated.
    /// Stored as a distinct Rust discriminant (-32007); serializes to
    /// the same wire code as `GroupNotAdmin` (-32005) — both are
    /// "capacity exhausted" from the client's perspective.
    Busy = -32007,
    /// Media-upload pre-flight: scratch-disk root unreachable.
    /// Stored as a distinct Rust discriminant (-32008); serializes to
    /// the same wire code as `FallbackExhausted` (-32006).
    DiskUnreachable = -32008,
    NotConnected = -32012,
    EditWindowExpired = -32013,
    DeleteWindowExpired = -32014,
    Internal = -32050,
    Unimplemented = -32060,
    ShuttingDown = -32099,

    /// Generic / unknown — only used for forward-compatibility with codes
    /// this binary does not yet know about.
    Other(i32),
}

impl RpcErrorCode {
    pub fn as_i32(self) -> i32 {
        match self {
            RpcErrorCode::ParseError => -32700,
            RpcErrorCode::InvalidRequest => -32600,
            RpcErrorCode::MethodNotFound => -32601,
            RpcErrorCode::InvalidParams => -32602,
            RpcErrorCode::InternalError => -32603,
            RpcErrorCode::SessionLost => -32001,
            RpcErrorCode::NotConfigured => -32002,
            RpcErrorCode::RateLimited => -32003,
            RpcErrorCode::PayloadTooLarge => -32004,
            RpcErrorCode::GroupNotAdmin => -32005,
            RpcErrorCode::FallbackExhausted => -32006,
            RpcErrorCode::Busy => -32005,
            RpcErrorCode::DiskUnreachable => -32006,
            RpcErrorCode::NotConnected => -32012,
            RpcErrorCode::EditWindowExpired => -32013,
            RpcErrorCode::DeleteWindowExpired => -32014,
            RpcErrorCode::Internal => -32050,
            RpcErrorCode::Unimplemented => -32060,
            RpcErrorCode::ShuttingDown => -32099,
            RpcErrorCode::Other(c) => c,
        }
    }
}

impl RpcRequest {
    pub fn from_json(bytes: &[u8]) -> Result<Self, RpcParseError> {
        let v: serde_json::Value = serde_json::from_slice(bytes)?;
        let obj = v.as_object().ok_or(RpcParseError::MissingField("object"))?;
        let id = obj
            .get("id")
            .ok_or(RpcParseError::MissingField("id"))?
            .as_u64()
            .ok_or(RpcParseError::InvalidId)?;
        let method = obj
            .get("method")
            .ok_or(RpcParseError::MissingField("method"))?
            .as_str()
            .ok_or(RpcParseError::MissingField("method"))?
            .to_string();
        let params = obj.get("params").cloned().unwrap_or(Value::Null);
        Ok(Self { id, method, params })
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RpcParseError {
    #[error("malformed JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("missing field: {0}")]
    MissingField(&'static str),
    #[error("invalid id: must be u64")]
    InvalidId,
}

#[cfg(test)]
mod tests;
