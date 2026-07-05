use super::*;
use crate::config::WhatsAppRuntimeConfig;
use crate::daemon::Daemon;

struct EchoHandler;

#[async_trait::async_trait]
impl RpcHandler for EchoHandler {
    fn name(&self) -> &'static str {
        "echo"
    }
    async fn call(&self, _h: DaemonHandle, params: Value) -> Result<Value, RpcError> {
        Ok(params)
    }
}

#[tokio::test]
async fn dispatch_routes_to_registered_handler() {
    let reg = HandlerRegistry::new().register(Arc::new(EchoHandler));
    let cfg = WhatsAppRuntimeConfig::from_toml(br#"name = "x""#).unwrap();
    let handle = Daemon::new(cfg).handle();
    let req = RpcRequest {
        id: 7,
        method: "echo".to_string(),
        params: serde_json::json!({"a": 1}),
    };
    let resp = reg.dispatch(handle, req).await;
    assert_eq!(resp.id, 7);
    assert_eq!(resp.result.unwrap(), serde_json::json!({"a": 1}));
    assert!(resp.error.is_none());
}

#[tokio::test]
async fn unknown_method_returns_method_not_found() {
    let reg = HandlerRegistry::new();
    let cfg = WhatsAppRuntimeConfig::from_toml(br#"name = "x""#).unwrap();
    let handle = Daemon::new(cfg).handle();
    let req = RpcRequest {
        id: 8,
        method: "no.such.method".to_string(),
        params: Value::Null,
    };
    let resp = reg.dispatch(handle, req).await;
    assert!(resp.result.is_none());
    let err = resp.error.unwrap();
    assert_eq!(err.code, -32601);
}

#[tokio::test]
async fn bind_creates_socket_file_with_0600() {
    let tmp = tempfile::TempDir::new().unwrap();
    let sock = tmp.path().join("t.sock");
    let _server = UnixSocketServer::bind(&sock).unwrap();
    let meta = std::fs::metadata(&sock).unwrap();
    assert_eq!(meta.permissions().mode() & 0o777, 0o600);
}
