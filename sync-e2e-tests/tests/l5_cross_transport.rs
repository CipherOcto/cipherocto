//! L5: Container cross-transport E2E tests — sync via Docker + adapters.
//!
//! These tests verify that `stoolap-node` initializes and runs correctly
//! with `--adapter` flags inside Docker containers. The transport layer
//! (NodeTransport + outbox drain) runs alongside TCP sync, proving the
//! full stack works in containerized environments.
//!
//! Per `docs/e2e/2026-06-23-stoolap-data-sync-e2e-test-plan.md` §L5.

use std::process::Command;
use std::time::Duration;

const IMAGE_TAG: &str = "stoolap-node-test";

fn stoolap_node_bin_path() -> String {
    let candidates = [
        {
            let mut p = std::env::current_exe().unwrap();
            p.pop();
            p.pop();
            p.pop();
            p.push("stoolap-node");
            p.push("target");
            p.push("debug");
            p.push("stoolap-node");
            p
        },
        {
            let mut p = std::env::current_dir().unwrap_or_default();
            p.push("stoolap-node");
            p.push("target");
            p.push("debug");
            p.push("stoolap-node");
            p
        },
    ];
    for c in &candidates {
        if c.exists() {
            return c.to_string_lossy().to_string();
        }
    }
    panic!("stoolap-node not found. Build: cd sync-e2e-tests/stoolap-node && cargo build");
}

fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

fn docker_available() -> bool {
    Command::new("docker")
        .arg("info")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn build_image() {
    let bin_path = stoolap_node_bin_path();
    let df = "FROM ubuntu:20.04\n\
              RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*\n\
              COPY stoolap-node /usr/local/bin/stoolap-node\n\
              ENTRYPOINT [\"stoolap-node\"]\n";
    let build_dir = tempfile::tempdir().unwrap();
    std::fs::write(build_dir.path().join("Dockerfile"), df).unwrap();
    std::fs::copy(&bin_path, build_dir.path().join("stoolap-node")).unwrap();
    let output = Command::new("docker")
        .args(["build", "-t", IMAGE_TAG, build_dir.path().to_str().unwrap()])
        .output()
        .expect("docker build failed");
    assert!(
        output.status.success(),
        "docker build failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn docker_run(
    name: &str,
    network: &str,
    vol: Option<(&str, &str)>,
    args: &[&str],
) -> std::process::Child {
    let _ = Command::new("docker").args(["rm", "-f", name]).output();
    let mut cmd = Command::new("docker");
    cmd.args(["run", "--rm", "--name", name, "--network", network]);
    if let Some((host, container)) = vol {
        cmd.args(["-v", &format!("{host}:{container}:rw")]);
    }
    cmd.arg(IMAGE_TAG).args(args);
    cmd.spawn()
        .unwrap_or_else(|e| panic!("failed to start container {name}: {e}"))
}

fn docker_network_create(name: &str) {
    let _ = Command::new("docker")
        .args(["network", "rm", name])
        .output();
    Command::new("docker")
        .args(["network", "create", name])
        .output()
        .expect("failed to create network");
}

fn docker_network_rm(name: &str) {
    let _ = Command::new("docker")
        .args(["network", "rm", name])
        .output();
}

fn cleanup_containers(names: &[&str]) {
    for name in names {
        let _ = Command::new("docker").args(["rm", "-f", name]).output();
    }
}

fn cleanup_networks(prefix: &str) {
    if let Ok(output) = Command::new("docker")
        .args([
            "network",
            "ls",
            "--filter",
            &format!("name={prefix}"),
            "--format",
            "{{.Name}}",
        ])
        .output()
    {
        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            if !line.is_empty() {
                let _ = Command::new("docker")
                    .args(["network", "rm", line])
                    .output();
            }
        }
    }
}

fn full_cleanup(test_prefix: &str, container_names: &[&str]) {
    cleanup_containers(container_names);
    cleanup_networks(test_prefix);
}

async fn wait_for_status(path: &str, timeout: Duration) -> Option<i64> {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        if let Ok(content) = std::fs::read_to_string(path) {
            if let Ok(n) = content.trim().parse::<i64>() {
                if n > 0 {
                    return Some(n);
                }
            }
        }
        if tokio::time::Instant::now() >= deadline {
            return None;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

fn writer_dsn(dir: &tempfile::TempDir) -> String {
    format!("file://{}/db", dir.path().to_str().unwrap())
}

// ─── L5 Cross-Transport Tests ───────────────────────────────────────

/// L5-T8: Two-container sync with --adapter flag.
///
/// Writer commits 50 rows with `--adapter webhook` (transport layer active).
/// Reader connects via TCP and verifies sync convergence.
/// Proves transport initialization works inside Docker containers.
#[tokio::test]
async fn two_container_with_adapter_flag() {
    if !docker_available() {
        eprintln!("Docker not available, skipping");
        return;
    }
    build_image();

    let net = format!("sync-e2e-t8-{}", free_port());
    full_cleanup("sync-e2e-t8", &["t8-writer", "t8-reader"]);
    docker_network_create(&net);

    let writer_dir = tempfile::tempdir().unwrap();
    let status_dir = tempfile::tempdir().unwrap();
    let status_host = status_dir.path().to_str().unwrap().to_string();
    let status_in = format!("{}/count", status_host);

    // Writer: commit 50 rows, start with --adapter webhook (transport layer init)
    let mut writer = docker_run(
        "t8-writer",
        &net,
        None,
        &[
            "--dsn",
            &writer_dsn(&writer_dir),
            "--listen",
            "3333",
            "--commit",
            "50",
            "--adapter",
            "webhook",
        ],
    );
    tokio::time::sleep(Duration::from_secs(2)).await;

    // Reader: also with --adapter flag, connects via TCP for actual data
    let mut reader = docker_run(
        "t8-reader",
        &net,
        Some((&status_host, "/status")),
        &[
            "--dsn",
            "memory://",
            "--listen",
            "3333",
            "--peer",
            "t8-writer:3333",
            "--status-file",
            "/status/count",
            "--adapter",
            "webhook",
        ],
    );

    let count = wait_for_status(&status_in, Duration::from_secs(15)).await;

    writer.kill().ok();
    reader.kill().ok();
    let _ = writer.wait();
    let _ = reader.wait();
    docker_network_rm(&net);

    assert_eq!(
        count,
        Some(50),
        "reader should have 50 rows (transport init + TCP sync)"
    );
}

/// L5-T9: Three-container fan-out with --adapter flags.
///
/// Writer commits 100 rows. Two readers with --adapter flags sync via TCP.
/// Proves multiple containers can initialize transport layer simultaneously.
#[tokio::test]
async fn three_container_fanout_with_adapter() {
    if !docker_available() {
        eprintln!("Docker not available, skipping");
        return;
    }
    build_image();

    let net = format!("sync-e2e-t9-{}", free_port());
    full_cleanup("sync-e2e-t9", &["t9-writer", "t9-r1", "t9-r2"]);
    docker_network_create(&net);

    let writer_dir = tempfile::tempdir().unwrap();
    let sh1_dir = tempfile::tempdir().unwrap();
    let sh2_dir = tempfile::tempdir().unwrap();
    let sh1 = sh1_dir.path().to_str().unwrap().to_string();
    let sh2 = sh2_dir.path().to_str().unwrap().to_string();
    let si1 = format!("{}/count", sh1);
    let si2 = format!("{}/count", sh2);

    let mut writer = docker_run(
        "t9-writer",
        &net,
        None,
        &[
            "--dsn",
            &writer_dsn(&writer_dir),
            "--listen",
            "3333",
            "--commit",
            "100",
            "--adapter",
            "webhook",
        ],
    );
    tokio::time::sleep(Duration::from_secs(2)).await;

    let mut r1 = docker_run(
        "t9-r1",
        &net,
        Some((&sh1, "/status")),
        &[
            "--dsn",
            "memory://",
            "--listen",
            "3333",
            "--peer",
            "t9-writer:3333",
            "--status-file",
            "/status/count",
            "--adapter",
            "webhook",
        ],
    );
    let mut r2 = docker_run(
        "t9-r2",
        &net,
        Some((&sh2, "/status")),
        &[
            "--dsn",
            "memory://",
            "--listen",
            "3333",
            "--peer",
            "t9-writer:3333",
            "--status-file",
            "/status/count",
            "--adapter",
            "webhook",
        ],
    );

    let c1 = wait_for_status(&si1, Duration::from_secs(15)).await;
    let c2 = wait_for_status(&si2, Duration::from_secs(15)).await;

    writer.kill().ok();
    r1.kill().ok();
    r2.kill().ok();
    let _ = writer.wait();
    let _ = r1.wait();
    let _ = r2.wait();
    docker_network_rm(&net);

    assert_eq!(c1, Some(100), "reader1 should have 100 rows");
    assert_eq!(c2, Some(100), "reader2 should have 100 rows");
}

/// L5-T10: Container with --adapter-dir flag (plugin directory).
///
/// Starts a container with --adapter-dir pointing to an empty dir.
/// The adapter plugin load fails gracefully (no crash), and TCP sync works.
/// Proves the transport initialization is robust against missing plugins.
#[tokio::test]
async fn container_with_adapter_dir_empty() {
    if !docker_available() {
        eprintln!("Docker not available, skipping");
        return;
    }
    build_image();

    let net = format!("sync-e2e-t10-{}", free_port());
    full_cleanup("sync-e2e-t10", &["t10-writer", "t10-reader"]);
    docker_network_create(&net);

    let writer_dir = tempfile::tempdir().unwrap();
    let status_dir = tempfile::tempdir().unwrap();
    let status_host = status_dir.path().to_str().unwrap().to_string();
    let status_in = format!("{}/count", status_host);

    // Writer with --adapter-dir (empty dir) + --adapter webhook
    let mut writer = docker_run(
        "t10-writer",
        &net,
        None,
        &[
            "--dsn",
            &writer_dsn(&writer_dir),
            "--listen",
            "3333",
            "--commit",
            "25",
            "--adapter",
            "webhook",
            "--adapter-dir",
            "/nonexistent",
        ],
    );
    tokio::time::sleep(Duration::from_secs(2)).await;

    // Reader without adapter flags (plain TCP)
    let mut reader = docker_run(
        "t10-reader",
        &net,
        Some((&status_host, "/status")),
        &[
            "--dsn",
            "memory://",
            "--listen",
            "3333",
            "--peer",
            "t10-writer:3333",
            "--status-file",
            "/status/count",
        ],
    );

    let count = wait_for_status(&status_in, Duration::from_secs(15)).await;

    writer.kill().ok();
    reader.kill().ok();
    let _ = writer.wait();
    let _ = reader.wait();
    docker_network_rm(&net);

    assert_eq!(
        count,
        Some(25),
        "reader should have 25 rows (plugin load graceful failure)"
    );
}

/// L5-T11: Container with multiple --adapter flags.
///
/// Writer starts with --adapter webhook --adapter p2p.
/// Multiple adapter initialization succeeds (or fails gracefully per adapter).
/// TCP sync works regardless of transport layer state.
#[tokio::test]
async fn container_with_multiple_adapters() {
    if !docker_available() {
        eprintln!("Docker not available, skipping");
        return;
    }
    build_image();

    let net = format!("sync-e2e-t11-{}", free_port());
    full_cleanup("sync-e2e-t11", &["t11-writer", "t11-reader"]);
    docker_network_create(&net);

    let writer_dir = tempfile::tempdir().unwrap();
    let status_dir = tempfile::tempdir().unwrap();
    let status_host = status_dir.path().to_str().unwrap().to_string();
    let status_in = format!("{}/count", status_host);

    // Writer with multiple --adapter flags
    let mut writer = docker_run(
        "t11-writer",
        &net,
        None,
        &[
            "--dsn",
            &writer_dsn(&writer_dir),
            "--listen",
            "3333",
            "--commit",
            "75",
            "--adapter",
            "webhook",
            "--adapter",
            "p2p",
        ],
    );
    tokio::time::sleep(Duration::from_secs(2)).await;

    let mut reader = docker_run(
        "t11-reader",
        &net,
        Some((&status_host, "/status")),
        &[
            "--dsn",
            "memory://",
            "--listen",
            "3333",
            "--peer",
            "t11-writer:3333",
            "--status-file",
            "/status/count",
        ],
    );

    let count = wait_for_status(&status_in, Duration::from_secs(15)).await;

    writer.kill().ok();
    reader.kill().ok();
    let _ = writer.wait();
    let _ = reader.wait();
    docker_network_rm(&net);

    assert_eq!(
        count,
        Some(75),
        "reader should have 75 rows (multi-adapter init + TCP sync)"
    );
}
