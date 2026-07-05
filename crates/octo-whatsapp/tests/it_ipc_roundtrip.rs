//! Hermetic end-to-end test for the unix-socket JSON-RPC server.
//!
//! Flow:
//! 1. Bind a `UnixSocketServer` in a TempDir.
//! 2. Spawn `serve()` on a background task.
//! 3. Connect from the test using a blocking `std::os::unix::net::UnixStream`
//!    (driven via `spawn_blocking` so we don't stall the runtime).
//! 4. Send one line-delimited JSON-RPC `version.get` request.
//! 5. Read the response line and assert the daemon echoes
//!    `daemon_api_version = "1.0.0+phase1"`.
//! 6. Trigger cancellation; the accept loop must remove the socket file
//!    and the spawn task must complete with Ok.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream as StdUnixStream;
use std::sync::Arc;
use std::time::Duration;

use octo_whatsapp::config::WhatsAppRuntimeConfig;
use octo_whatsapp::daemon::Daemon;
use octo_whatsapp::ipc::handlers::build_registry;
use octo_whatsapp::ipc::server::{HandlerRegistry, UnixSocketServer};
use tokio_util::sync::CancellationToken;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ipc_roundtrip_via_unix_socket() {
    let tmp = tempfile::tempdir().unwrap();
    let sock = tmp.path().join("octo-whatsapp-test.sock");

    let cfg = WhatsAppRuntimeConfig::from_toml(br#"name = "x""#).unwrap();
    let daemon = Daemon::new(cfg);
    let cancel: CancellationToken = daemon.cancel_token();
    let handle = daemon.handle();
    let registry: Arc<HandlerRegistry> = Arc::new(build_registry());

    // Confirm bind() sets up the socket file (sync part), then serve()
    // re-binds an async listener on the same path.
    let _server = UnixSocketServer::bind(&sock).unwrap();
    let server_path = sock.clone();
    let server_cancel = cancel.clone();
    let server_handle = handle.clone();
    let server_registry = registry.clone();
    let server_task = tokio::spawn(async move {
        UnixSocketServer {
            socket_path: server_path,
        }
        .serve(server_handle, server_registry, server_cancel)
        .await
    });

    // Server doesn't hold the listener past `bind`, so give serve() a tick
    // to call `listener()` and bind the actual async listener. If we
    // connect before that, we'll see ECONNREFUSED.
    let sock_for_thread = sock.clone();
    let connect_thread = tokio::task::spawn_blocking(move || -> StdUnixStream {
        // Try a few times; serve() may need one retry after listener() runs.
        let mut last_err = None;
        for _ in 0..20 {
            match StdUnixStream::connect(&sock_for_thread) {
                Ok(s) => return s,
                Err(e) => {
                    last_err = Some(e);
                    std::thread::sleep(Duration::from_millis(10));
                }
            }
        }
        panic!("connect kept failing: {:?}", last_err);
    });
    let mut stream = connect_thread.await.unwrap();

    // Drive the request + response on the blocking thread so we don't
    // stall the runtime. Use a one-line read so we don't depend on EOF
    // (the server keeps connections open for further requests).
    let resp_json = tokio::task::spawn_blocking(move || {
        let req = serde_json::json!({"id": 1, "method": "version.get"});
        let mut line = serde_json::to_string(&req).unwrap();
        line.push('\n');
        stream.write_all(line.as_bytes()).unwrap();
        // read exactly one response line
        let mut reader = BufReader::new(stream);
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        line
    })
    .await
    .unwrap();

    let resp: serde_json::Value = serde_json::from_str(resp_json.trim()).unwrap();
    assert_eq!(resp["id"], 1);
    assert_eq!(resp["result"]["daemon_api_version"], "1.0.0+phase1");

    cancel.cancel();
    let serve_result = server_task.await.unwrap();
    assert!(
        serve_result.is_ok(),
        "serve() must exit cleanly on cancel; got {serve_result:?}"
    );

    // Clean shutdown: the socket file must be gone.
    assert!(
        !sock.exists(),
        "socket file {:?} must be removed on shutdown",
        sock
    );
}
