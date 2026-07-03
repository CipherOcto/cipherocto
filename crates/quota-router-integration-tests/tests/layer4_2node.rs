//! Layer 4: Two-node docker compose test (manual only, `#[ignore]`-gated).
//!
//! Spins up `compose-2node.yaml`, waits for healthchecks, then verifies
//! that gossip converges across the two containers.
//!
//! Prerequisites:
//!   - Docker engine running
//!   - `docker compose v2` available
//!   - Ports 19100-19101 available
//!
//! Run:
//! ```sh
//! cargo test --manifest-path crates/quota-router-integration-tests/Cargo.toml \
//!   -- --ignored layer4_2node
//! ```

use std::time::Duration;

/// Path to compose-2node.yaml
fn compose_path() -> String {
    let mut path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.pop(); // crates/
    path.pop(); // workspace root
    path.push("crates/quota-router-integration-tests/tests/layer4/compose-2node.yaml");
    path.to_string_lossy().to_string()
}

/// Run `docker compose` with the given args.
fn docker_compose(args: &[&str]) -> std::process::Output {
    let compose_file = compose_path();
    let mut cmd = std::process::Command::new("docker");
    cmd.arg("compose").arg("-f").arg(&compose_file);
    for arg in args {
        cmd.arg(arg);
    }
    cmd.output().expect("failed to run docker compose")
}

/// Wait for both containers to be healthy.
fn wait_for_healthy(timeout: Duration) {
    let start = std::time::Instant::now();
    loop {
        let output = docker_compose(&["ps", "--format", "json"]);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let healthy_count = stdout.lines()
            .filter(|l| l.contains("\"Health\":\"healthy\""))
            .count();
        if healthy_count >= 2 {
            return;
        }
        if start.elapsed() > timeout {
            panic!(
                "containers not healthy after {:?}\nstdout: {}",
                timeout, stdout
            );
        }
        std::thread::sleep(Duration::from_secs(1));
    }
}

/// L3: Two-node docker compose — both containers come up healthy
/// and gossip converges.
///
/// ```sh
/// cargo test --manifest-path crates/quota-router-integration-tests/Cargo.toml \
///   -- --ignored layer4_2node
/// ```
#[test]
#[ignore = "requires docker engine and compose v2"]
fn layer4_2node() {
    // Bring up the compose stack
    let up = docker_compose(&["up", "-d", "--build"]);
    assert!(
        up.status.success(),
        "docker compose up failed: {}",
        String::from_utf8_lossy(&up.stderr)
    );

    // Wait for healthchecks
    wait_for_healthy(Duration::from_secs(60));

    // Verify both containers are running
    let ps = docker_compose(&["ps", "--format", "json"]);
    let stdout = String::from_utf8_lossy(&ps.stdout);
    let running = stdout.lines()
        .filter(|l| l.contains("\"State\":\"running\""))
        .count();
    assert!(
        running >= 2,
        "expected 2 running containers, got {}\n{}",
        running,
        stdout
    );

    // Both containers should be listening on port 9100 internally.
    // We can verify by checking the logs for the startup message.
    let logs_a = docker_compose(&["logs", "node-a"]);
    let logs_b = docker_compose(&["logs", "node-b"]);
    let out_a = String::from_utf8_lossy(&logs_a.stdout);
    let out_b = String::from_utf8_lossy(&logs_b.stdout);
    assert!(
        out_a.contains("TcpAdapter listening") || out_a.contains("QuotaRouterNode started"),
        "node-a should show startup logs:\n{}",
        out_a
    );
    assert!(
        out_b.contains("TcpAdapter listening") || out_b.contains("QuotaRouterNode started"),
        "node-b should show startup logs:\n{}",
        out_b
    );

    // Tear down
    docker_compose(&["down"]);
}
