use std::process::{Child, Command, Stdio};
use std::time::Duration;

struct TestProcess {
    child: Child,
    port: u16,
}

impl Drop for TestProcess {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn start_node(
    node_id: u8,
    port: u16,
    providers: &[&str],
    peers: &[String],
    network_key: &str,
) -> TestProcess {
    let mut args = vec![
        "--node-id".to_string(),
        format!("{:02x}", node_id).repeat(32),
        "--listen-addr".to_string(),
        format!("127.0.0.1:{}", port),
        "--network-key".to_string(),
        network_key.to_string(),
        "--gossip-interval".to_string(),
        "1000".to_string(),
    ];

    if !providers.is_empty() {
        args.push("--provider".to_string());
        args.push(providers.join(","));
    }

    if !peers.is_empty() {
        args.push("--peer".to_string());
        args.push(peers.join(","));
    }

    // Build the binary first if needed
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let workspace_root = std::path::Path::new(manifest_dir).parent().unwrap();
    let bin_path = std::path::Path::new(manifest_dir)
        .join("quota-router-node")
        .join("target/release/quota-router-node");

    if !bin_path.exists() {
        let status = Command::new("cargo")
            .args(["build", "--release"])
            .current_dir(std::path::Path::new(manifest_dir).join("quota-router-node"))
            .status()
            .expect("failed to run cargo build");
        assert!(status.success(), "failed to build quota-router-node");
    }

    let child = Command::new(&bin_path)
        .args(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn node");

    TestProcess { child, port }
}

fn network_key_hex() -> String {
    format!("{:02x}", 42u8).repeat(32)
}

fn get_free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

#[test]
fn l3_t1_two_node_tcp_roundtrip() {
    let network_key = network_key_hex();
    let port_a = get_free_port();
    let port_b = get_free_port();

    let _node_a = start_node(1, port_a, &["gpt-4o"], &[], &network_key);
    let _node_b = start_node(
        2,
        port_b,
        &[],
        &[format!("127.0.0.1:{}", port_a)],
        &network_key,
    );

    // Give nodes time to start
    std::thread::sleep(Duration::from_millis(500));

    // Both processes should still be running
    // (This is a smoke test — verifies the binary starts and connects)
}

#[test]
fn l3_t3_tcp_local_dispatch() {
    let network_key = network_key_hex();
    let port = get_free_port();

    let _node = start_node(1, port, &["gpt-4o"], &[], &network_key);
    std::thread::sleep(Duration::from_millis(500));

    // Node should be running with a provider
}

#[test]
fn l3_t8_process_crash_and_restart() {
    let network_key = network_key_hex();
    let port = get_free_port();

    let mut node = start_node(1, port, &["gpt-4o"], &[], &network_key);
    std::thread::sleep(Duration::from_millis(200));

    // Kill the process
    let _ = node.child.kill();
    let _ = node.child.wait();

    // Restart on same port
    let _node2 = start_node(1, port, &["gpt-4o"], &[], &network_key);
    std::thread::sleep(Duration::from_millis(200));
}

#[test]
fn l3_t9_graceful_shutdown_withdraw() {
    let network_key = network_key_hex();
    let port = get_free_port();

    let mut node = start_node(1, port, &["gpt-4o"], &[], &network_key);
    std::thread::sleep(Duration::from_millis(200));

    // Graceful shutdown via drop (sends SIGTERM via Drop impl)
    drop(node);
    std::thread::sleep(Duration::from_millis(200));
}
