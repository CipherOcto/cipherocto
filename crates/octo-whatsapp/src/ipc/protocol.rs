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

/// Standard JSON-RPC 2.0 codes + CipherOcto custom codes (-32001 .. -32099).
/// See design §Error codes. Wire numbers are a load-bearing contract —
/// every code below maps 1:1 to the design's table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum RpcErrorCode {
    // JSON-RPC 2.0 standard
    ParseError = -32700,
    InvalidRequest = -32600,
    MethodNotFound = -32601,
    InvalidParams = -32602,
    InternalError = -32603,

    // CipherOcto custom — design §Error codes
    /// `BotState::Replaced` observed (design line 619).
    SessionLostReplaced = -32001,
    /// `BotState::LoggedOut` observed (design line 620).
    SessionLostLoggedOut = -32000,
    /// `BotState::SessionExpired` observed (design line 621).
    SessionLostExpired = -31999,
    NotConfigured = -32002,
    RateLimited = -32003,
    PayloadTooLarge = -32004,
    /// `groups.*` admin operation by non-admin (design line 625).
    GroupNotAdmin = -32005,
    /// `send.*` native mode — all fallbacks exhausted (design line 626).
    FallbackExhausted = -32006,
    /// `triggers.run` — text exceeds ARG_MAX (design line 627).
    PayloadTooLargeForTrigger = -32007,
    /// `actions.escalate` — target unreachable after retries (design line 628).
    EscalationFailed = -32008,
    /// MCP `tools/call` — tool toggled off after `tools/list` (design line 629).
    ToolDisabled = -32009,
    /// Outbound RPC — JID not in peer_allowlist (design line 630).
    PeerNotAllowed = -32010,
    /// Stoolap-backed RPC before `start_bot()` populated `DaemonState.store`
    /// (design line 631).
    StoreNotReady = -32011,
    NotConnected = -32012,
    EditWindowExpired = -32013,
    DeleteWindowExpired = -32014,
    /// `reconnect.now` — operator forced immediate reconnect while one was
    /// in progress (design line 635).
    BackoffCancelled = -32015,
    /// `rules.update` / `rules.patch` — etag mismatch (design line 636).
    RuleConflict = -32020,
    /// `rules.create` / `rules.update` — regex fails ReDoS classifier
    /// (design line 637).
    RuleRegexUnsafe = -32021,
    /// `rules.match` — predicate exceeded regex timeout (design line 638).
    RuleMatchTimeout = -32022,
    /// `triggers.run` — trigger.enabled = false (design line 639).
    TriggerDisabled = -32030,
    /// `media.upload` / `send.* --file` / `profile.picture` / `groups.icon` —
    /// path outside `allowed_upload_roots` (design line 640).
    UploadPathDenied = -32040,
    /// RPC adapter — uncategorized adapter error (design line 641).
    Internal = -32050,
    /// Media-upload pre-flight: in-flight upload semaphore saturated.
    /// Internal-state code (no design slot, kept post-R1).
    Busy = -32052,
    /// Media-upload pre-flight: scratch-disk root unreachable.
    /// Internal-state code (no design slot, kept post-R1).
    DiskUnreachable = -32053,
    /// `CoordinatorAdmin::*` — `PlatformAdapterError::Unimplemented`
    /// (design line 642).
    Unimplemented = -32060,
    /// SIGTERM in flight; refusing new RPCs (design line 643).
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
            RpcErrorCode::SessionLostReplaced => -32001,
            RpcErrorCode::SessionLostLoggedOut => -32000,
            RpcErrorCode::SessionLostExpired => -31999,
            RpcErrorCode::NotConfigured => -32002,
            RpcErrorCode::RateLimited => -32003,
            RpcErrorCode::PayloadTooLarge => -32004,
            RpcErrorCode::GroupNotAdmin => -32005,
            RpcErrorCode::FallbackExhausted => -32006,
            RpcErrorCode::PayloadTooLargeForTrigger => -32007,
            RpcErrorCode::EscalationFailed => -32008,
            RpcErrorCode::ToolDisabled => -32009,
            RpcErrorCode::PeerNotAllowed => -32010,
            RpcErrorCode::StoreNotReady => -32011,
            RpcErrorCode::NotConnected => -32012,
            RpcErrorCode::EditWindowExpired => -32013,
            RpcErrorCode::DeleteWindowExpired => -32014,
            RpcErrorCode::BackoffCancelled => -32015,
            RpcErrorCode::RuleConflict => -32020,
            RpcErrorCode::RuleRegexUnsafe => -32021,
            RpcErrorCode::RuleMatchTimeout => -32022,
            RpcErrorCode::TriggerDisabled => -32030,
            RpcErrorCode::UploadPathDenied => -32040,
            RpcErrorCode::Internal => -32050,
            RpcErrorCode::Busy => -32052,
            RpcErrorCode::DiskUnreachable => -32053,
            RpcErrorCode::Unimplemented => -32060,
            RpcErrorCode::ShuttingDown => -32099,
            RpcErrorCode::Other(c) => c,
        }
    }
}

impl RpcError {
    pub fn method_not_found<E: std::fmt::Display>(m: E) -> Self {
        Self {
            code: RpcErrorCode::MethodNotFound.as_i32(),
            message: m.to_string(),
            data: None,
        }
    }
    pub fn invalid_params<E: std::fmt::Display>(m: E) -> Self {
        Self {
            code: RpcErrorCode::InvalidParams.as_i32(),
            message: m.to_string(),
            data: None,
        }
    }
    pub fn rate_limited<E: std::fmt::Display>(m: E) -> Self {
        Self {
            code: RpcErrorCode::RateLimited.as_i32(),
            message: m.to_string(),
            data: None,
        }
    }
    pub fn conflict_with_etag(id: String, current_etag: String, current_version: u64) -> Self {
        let data = serde_json::json!({
            "resource_id": id,
            "current_etag": current_etag,
            "current_version": current_version,
        });
        Self {
            code: RpcErrorCode::RuleConflict.as_i32(),
            message: format!("etag conflict on {id}"),
            data: Some(data),
        }
    }
    pub fn exec_failed<E: std::fmt::Display>(m: E) -> Self {
        Self {
            code: RpcErrorCode::InternalError.as_i32(),
            message: m.to_string(),
            data: None,
        }
    }
    pub fn not_supported<E: std::fmt::Display>(m: E) -> Self {
        Self {
            code: RpcErrorCode::Unimplemented.as_i32(),
            message: m.to_string(),
            data: None,
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
