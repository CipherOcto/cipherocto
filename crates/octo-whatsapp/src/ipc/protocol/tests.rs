use super::*;

#[test]
fn parse_minimal_request() {
    let r: RpcRequest = serde_json::from_slice(br#"{"id":1,"method":"status.get"}"#).unwrap();
    assert_eq!(r.id, 1);
    assert_eq!(r.method, "status.get");
    assert_eq!(r.params, Value::Null);
}

#[test]
fn parse_request_with_params() {
    let r: RpcRequest = serde_json::from_slice(
        br#"{"id":42,"method":"send.text","params":{"peer":"+15551234567","text":"hi"}}"#,
    )
    .unwrap();
    assert_eq!(r.id, 42);
    assert_eq!(r.method, "send.text");
    assert_eq!(r.params["peer"], "+15551234567");
    assert_eq!(r.params["text"], "hi");
}

#[test]
fn parse_missing_method_fails() {
    let res: Result<RpcRequest, _> = serde_json::from_slice(br#"{"id":1}"#);
    assert!(res.is_err());
}

#[test]
fn parse_string_id_rejected() {
    let res: Result<RpcRequest, _> = serde_json::from_slice(br#"{"id":"abc","method":"x"}"#);
    assert!(res.is_err());
}

#[test]
fn response_with_result() {
    let r = RpcResponse {
        id: 1,
        result: Some(serde_json::json!({"ok": true})),
        error: None,
    };
    let s = serde_json::to_string(&r).unwrap();
    assert!(s.contains("\"result\""));
    assert!(!s.contains("\"error\""));
}

#[test]
fn response_with_error() {
    let r = RpcResponse {
        id: 1,
        result: None,
        error: Some(RpcError {
            code: -32601,
            message: "Method not found".to_string(),
            data: None,
        }),
    };
    let s = serde_json::to_string(&r).unwrap();
    assert!(s.contains("\"error\""));
    assert!(!s.contains("\"result\""));
}

#[test]
fn from_json_helper_matches_serde() {
    let r = RpcRequest::from_json(br#"{"id":7,"method":"x"}"#).unwrap();
    assert_eq!(r.id, 7);
    assert_eq!(r.method, "x");
}

#[test]
fn from_json_helper_rejects_missing_method() {
    assert!(RpcRequest::from_json(br#"{"id":1}"#).is_err());
}

/// Wire-code contract — these numbers are load-bearing per design §Error
/// codes. Any change here MUST match the design's table verbatim.
#[test]
fn busy_serializes_to_minus_32052() {
    assert_eq!(RpcErrorCode::Busy.as_i32(), -32052);
}

#[test]
fn disk_unreachable_serializes_to_minus_32053() {
    assert_eq!(RpcErrorCode::DiskUnreachable.as_i32(), -32053);
}

#[test]
fn group_not_admin_serializes_to_minus_32005() {
    assert_eq!(RpcErrorCode::GroupNotAdmin.as_i32(), -32005);
}

#[test]
fn fallback_exhausted_serializes_to_minus_32006() {
    assert_eq!(RpcErrorCode::FallbackExhausted.as_i32(), -32006);
}

#[test]
fn edit_window_serializes_to_minus_32013() {
    assert_eq!(RpcErrorCode::EditWindowExpired.as_i32(), -32013);
}

#[test]
fn delete_window_serializes_to_minus_32014() {
    assert_eq!(RpcErrorCode::DeleteWindowExpired.as_i32(), -32014);
}

#[test]
fn session_lost_split_codes() {
    assert_eq!(RpcErrorCode::SessionLostReplaced.as_i32(), -32001);
    assert_eq!(RpcErrorCode::SessionLostLoggedOut.as_i32(), -32000);
    assert_eq!(RpcErrorCode::SessionLostExpired.as_i32(), -31999);
}

#[test]
fn rule_codes_match_design_table() {
    assert_eq!(RpcErrorCode::BackoffCancelled.as_i32(), -32015);
    assert_eq!(RpcErrorCode::RuleConflict.as_i32(), -32020);
    assert_eq!(RpcErrorCode::RuleRegexUnsafe.as_i32(), -32021);
    assert_eq!(RpcErrorCode::RuleMatchTimeout.as_i32(), -32022);
    assert_eq!(RpcErrorCode::TriggerDisabled.as_i32(), -32030);
    assert_eq!(RpcErrorCode::UploadPathDenied.as_i32(), -32040);
}

#[test]
fn new_codes_match_design_table() {
    assert_eq!(RpcErrorCode::PayloadTooLargeForTrigger.as_i32(), -32007);
    assert_eq!(RpcErrorCode::EscalationFailed.as_i32(), -32008);
    assert_eq!(RpcErrorCode::ToolDisabled.as_i32(), -32009);
    assert_eq!(RpcErrorCode::PeerNotAllowed.as_i32(), -32010);
    assert_eq!(RpcErrorCode::StoreNotReady.as_i32(), -32011);
}
