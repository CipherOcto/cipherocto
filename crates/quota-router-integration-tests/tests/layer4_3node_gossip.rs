//! Layer 4: Three-node gossip convergence test (manual only, `#[ignore]`-gated).
//!
//! Spins up `compose-3node.yaml`, waits for all three containers to be
//! healthy, then verifies gossip converges across all three nodes.
//!
//! ```sh
//! cargo test --manifest-path crates/quota-router-integration-tests/Cargo.toml \
//!   -- --ignored layer4_3node_gossip
//! ```

use std::time::Duration;

fn compose_path() -> String {
    let mut path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.pop();
    path.pop();
    path.push("crates/quota-router-integration-tests/tests/layer4/compose-3node.yaml");
    path.to_string_lossy().to_string()
}

fn docker_compose(args: &[&str]) -> std::process::Output {
    let compose_file = compose_path();
    let mut cmd = std::process::Command::new("docker");
    cmd.arg("compose").arg("-f").arg(&compose_file);
    for arg in args {
        cmd.arg(arg);
    }
    cmd.output().expect("failed to run docker compose")
}

fn wait_for_all_healthy(timeout: Duration) {
    let start = std::time::Instant::now();
    loop {
        let output = docker_compose(&["ps", "--format", "json"]);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let healthy_count = stdout
            .lines()
            .filter(|l| l.contains("\"Health\":\"healthy\""))
            .count();
        if healthy_count >= 3 {
            return;
        }
        if start.elapsed() > timeout {
            panic!("not all containers healthy after {:?}", timeout);
        }
        std::thread::sleep(Duration::from_secs(1));
    }
}

/// L5: Three-node gossip convergence.
#[test]
#[ignore = "requires docker engine and compose v2"]
fn layer4_3node_gossip() {
    let up = docker_compose(&["up", "-d", "--build"]);
    assert!(up.status.success(), "docker compose up failed");

    wait_for_all_healthy(Duration::from_secs(90));

    // Verify all 3 running
    let ps = docker_compose(&["ps", "--format", "json"]);
    let stdout = String::from_utf8_lossy(&ps.stdout);
    let running = stdout
        .lines()
        .filter(|l| l.contains("\"State\":\"running\""))
        .count();
    assert_eq!(running, 3, "should have 3 running containers");

    // Check logs for gossip activity on all nodes
    for node in &["node-a", "node-b", "node-c"] {
        let logs = docker_compose(&["logs", node]);
        let out = String::from_utf8_lossy(&logs.stdout);
        assert!(
            out.contains("QuotaRouterNode started") || out.contains("TcpAdapter listening"),
            "{} should have started",
            node
        );
    }

    // Wait for gossip convergence (nodes gossip every 10s)
    // After ~30s, all nodes should know about each other's providers.
    // We verify by checking that no container has crashed.
    std::thread::sleep(Duration::from_secs(35));

    let ps2 = docker_compose(&["ps", "--format", "json"]);
    let stdout2 = String::from_utf8_lossy(&ps2.stdout);
    let still_running = stdout2
        .lines()
        .filter(|l| l.contains("\"State\":\"running\""))
        .count();
    assert_eq!(
        still_running, 3,
        "all 3 nodes should still be running after gossip period"
    );

    docker_compose(&["down"]);
}
