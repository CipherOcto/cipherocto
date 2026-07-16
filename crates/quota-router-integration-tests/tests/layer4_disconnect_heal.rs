//! Layer 4: Disconnect and heal test (manual only, `#[ignore]`-gated).
//!
//! Spins up `compose-2node.yaml`, stops node B, verifies node A
//! continues to serve, then restarts node B and verifies rejoin.
//!
//! ```sh
//! cargo test --manifest-path crates/quota-router-integration-tests/Cargo.toml \
//!   -- --ignored layer4_disconnect_heal
//! ```

use std::time::Duration;

fn compose_path() -> String {
    let mut path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.pop();
    path.pop();
    path.push("crates/quota-router-integration-tests/tests/layer4/compose-2node.yaml");
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

fn wait_for_healthy(timeout: Duration) {
    let start = std::time::Instant::now();
    loop {
        let output = docker_compose(&["ps", "--format", "json"]);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let healthy_count = stdout
            .lines()
            .filter(|l| l.contains("\"Health\":\"healthy\""))
            .count();
        if healthy_count >= 2 {
            return;
        }
        if start.elapsed() > timeout {
            panic!("containers not healthy after {:?}", timeout);
        }
        std::thread::sleep(Duration::from_secs(1));
    }
}

fn count_running() -> usize {
    let ps = docker_compose(&["ps", "--format", "json"]);
    let stdout = String::from_utf8_lossy(&ps.stdout);
    stdout
        .lines()
        .filter(|l| l.contains("\"State\":\"running\""))
        .count()
}

/// L4: Stop node B, verify node A continues, restart B, verify rejoin.
#[test]
#[ignore = "requires docker engine and compose v2"]
fn layer4_disconnect_heal() {
    // Bring up
    let up = docker_compose(&["up", "-d", "--build"]);
    assert!(up.status.success(), "docker compose up failed");
    wait_for_healthy(Duration::from_secs(60));

    // Both running
    assert_eq!(count_running(), 2, "should start with 2 running");

    // Stop node B
    let stop = docker_compose(&["stop", "node-b"]);
    assert!(stop.status.success(), "docker compose stop failed");

    // Wait for node B to stop
    std::thread::sleep(Duration::from_secs(3));

    // Node A should still be running (degraded but functional)
    let running = count_running();
    assert_eq!(running, 1, "should have 1 running after stop");

    // Verify node A is still healthy
    let logs_a = docker_compose(&["logs", "node-a"]);
    let out_a = String::from_utf8_lossy(&logs_a.stdout);
    assert!(
        out_a.contains("TcpAdapter listening") || out_a.contains("QuotaRouterNode started"),
        "node-a should still be running"
    );

    // Restart node B
    let start = docker_compose(&["start", "node-b"]);
    assert!(start.status.success(), "docker compose start failed");

    // Wait for rejoin
    wait_for_healthy(Duration::from_secs(60));

    // Both should be running again
    assert_eq!(count_running(), 2, "should have 2 running after restart");

    // Tear down
    docker_compose(&["down"]);
}
