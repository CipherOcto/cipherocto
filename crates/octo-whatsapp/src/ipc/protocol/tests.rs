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

#[test]
fn busy_serializes_to_minus_32005() {
    assert_eq!(RpcErrorCode::Busy.as_i32(), -32005);
}

#[test]
fn disk_unreachable_serializes_to_minus_32006() {
    assert_eq!(RpcErrorCode::DiskUnreachable.as_i32(), -32006);
}

#[test]
fn busy_and_group_not_admin_share_wire_code() {
    // Both are "capacity exhausted" from the client's perspective; the
    // variant distinction is for the Rust side, the wire side collapses.
    assert_eq!(
        RpcErrorCode::Busy.as_i32(),
        RpcErrorCode::GroupNotAdmin.as_i32(),
    );
}

#[test]
fn disk_unreachable_and_fallback_exhausted_share_wire_code() {
    assert_eq!(
        RpcErrorCode::DiskUnreachable.as_i32(),
        RpcErrorCode::FallbackExhausted.as_i32(),
    );
}

#[test]
fn edit_window_serializes_to_minus_32013() {
    assert_eq!(RpcErrorCode::EditWindowExpired.as_i32(), -32013);
}

#[test]
fn delete_window_serializes_to_minus_32014() {
    assert_eq!(RpcErrorCode::DeleteWindowExpired.as_i32(), -32014);
}
