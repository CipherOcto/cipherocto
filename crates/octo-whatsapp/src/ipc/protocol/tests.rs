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
#[should_panic(expected = "Phase 1 Task 18")]
fn from_json_helper_is_todo_in_task17() {
    // The plan places `RpcRequest::from_json` in Task 18 (TDD green).
    // Until then, the helper panics with `todo!()` so this test pins the
    // TDD-red state. Task 18 will replace the body and remove this test
    // in favor of the two helper tests that follow.
    let _ = RpcRequest::from_json(br#"{"id":1,"method":"x"}"#);
}
