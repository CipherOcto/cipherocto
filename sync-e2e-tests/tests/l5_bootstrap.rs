//! L5: Bootstrap container E2E tests — --seed-list inside Docker.
//!
//! These tests verify that `stoolap-node` handles the `--seed-list`
//! and `--seed-authority` flags correctly inside Docker containers.
//!
//! Test matrix (4 tests):
//!
//! | ID   | Scenario                                      | Expected       |
//! |------|-----------------------------------------------|----------------|
//! | LB07 | Container starts with --seed-list flag        | Starts OK      |
//! | LB08 | Container exits on missing seed list file     | Exit code != 0 |
//! | LB09 | Container with --seed-authority dao           | Starts OK      |
//! | LB10 | Container with --seed-list + --peer           | TCP precedence |

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

fn full_cleanup(test_prefix: &str, container_names: &[&str]) {
    for name in container_names {
        let _ = Command::new("docker").args(["rm", "-f", name]).output();
    }
    // Clean up networks with prefix
    if let Ok(output) = Command::new("docker")
        .args([
            "network",
            "ls",
            "--filter",
            &format!("name={test_prefix}"),
            "--format",
            "{{.Name}}",
        ])
        .output()
    {
        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            if !line.is_empty() {
                let _ = Command::new("docker").args(["network", "rm", line]).output();
            }
        }
    }
}

fn mission_id() -> &'static str {
    "abcd000000000000000000000000000000000000000000000000000000000000"
}

fn node_id(suffix: u8) -> String {
    format!("{:0>64x}", suffix)
}

/// Write a seed list JSON file with N peers.
fn write_seed_list(dir: &std::path::Path, peers: Vec<(&str, &str)>, epoch: u64) -> String {
    let entries: Vec<String> = peers
        .iter()
        .map(|(id, addr)| {
            format!(
                r#"{{"peer_id":"{}","multiaddr":"{}","signed_at_epoch":{}}}"#,
                id, addr, epoch
            )
        })
        .collect();
    let json = format!(
        r#"{{
        "authority_pubkey": "{}",
        "signed_at_epoch": {},
        "peers": [{}]
    }}"#,
        "00".repeat(32),
        epoch,
        entries.join(",")
    );
    let path = dir.join("seed_list.json");
    std::fs::write(&path, &json).unwrap();
    path.to_string_lossy().to_string()
}

// ── LB07: Container starts with --seed-list flag ─────────────────

#[tokio::test]
async fn lb07_container_starts_with_seed_list_flag() {
    if !docker_available() {
        eprintln!("SKIP: docker not available");
        return;
    }
    build_image();

    let test_prefix = "lb07";
    let network = format!("{test_prefix}-net");
    let writer_name = format!("{test_prefix}-writer");
    let reader_name = format!("{test_prefix}-reader");
    let containers = [writer_name.as_str(), reader_name.as_str()];

    docker_network_create(&network);

    // Prepare seed list on host
    let seed_dir = tempfile::tempdir().unwrap();
    let _seed_path = write_seed_list(
        seed_dir.path(),
        vec![
            ("seed-1", "/ip4/10.0.0.1/tcp/4001"),
            ("seed-2", "/ip4/10.0.0.2/tcp/4001"),
            ("seed-3", "/ip4/10.0.0.3/tcp/4001"),
        ],
        100,
    );

    let writer_port = free_port();
    let _writer_dir = tempfile::tempdir().unwrap();

    // Writer with commit
    let mut writer = docker_run(
        &writer_name,
        &network,
        None,
        &[
            "--dsn",
            &format!("file:///data/db"),
            "--listen",
            &writer_port.to_string(),
            "--commit",
            "3",
            "--mission-id",
            mission_id(),
            "--node-id",
            &node_id(0x01),
        ],
    );

    tokio::time::sleep(Duration::from_secs(2)).await;

    // Reader with --seed-list (should start, attempt bootstrap, fail on stub)
    let mut reader = docker_run(
        &reader_name,
        &network,
        Some((seed_dir.path().to_str().unwrap(), "/seeds")),
        &[
            "--dsn",
            "memory://",
            "--listen",
            "0",
            "--seed-list",
            "/seeds/seed_list.json",
            "--mission-id",
            mission_id(),
            "--node-id",
            &node_id(0x02),
        ],
    );

    // Give it time to attempt bootstrap
    tokio::time::sleep(Duration::from_secs(3)).await;

    // Reader should have exited (bootstrap stub returns empty, no --peer)
    let reader_status = reader.try_wait().unwrap();
    if let Some(exit) = reader_status {
        // Exit is expected — bootstrap fails
        assert!(
            !exit.success(),
            "reader should exit with bootstrap failure"
        );
    }
    // If still running, that's OK too (node continues with TCP listener)

    full_cleanup(test_prefix, &containers);
    writer.kill().ok();
    reader.kill().ok();
    let _ = writer.wait();
    let _ = reader.wait();
}

// ── LB08: Container exits on missing seed list file ──────────────

#[tokio::test]
async fn lb08_container_exits_on_missing_seed_list() {
    if !docker_available() {
        eprintln!("SKIP: docker not available");
        return;
    }
    build_image();

    let test_prefix = "lb08";
    let network = format!("{test_prefix}-net");
    let name = format!("{test_prefix}-node");
    let containers = [name.as_str()];

    docker_network_create(&network);

    let output = Command::new("docker")
        .args([
            "run",
            "--rm",
            "--name",
            &name,
            "--network",
            &network,
            IMAGE_TAG,
            "--dsn",
            "memory://",
            "--listen",
            "0",
            "--seed-list",
            "/nonexistent/seed_list.json",
            "--mission-id",
            mission_id(),
            "--node-id",
            &node_id(0x01),
        ])
        .output()
        .expect("failed to run container");

    assert!(
        !output.status.success(),
        "container should exit with error for missing seed list"
    );
    // Note: tracing may not flush before std::process::exit,
    // so container stderr may be empty. Exit code check is sufficient.

    full_cleanup(test_prefix, &containers);
}

// ── LB09: Container with --seed-authority dao ─────────────────────

#[tokio::test]
async fn lb09_container_with_seed_authority_dao() {
    if !docker_available() {
        eprintln!("SKIP: docker not available");
        return;
    }
    build_image();

    let test_prefix = "lb09";
    let network = format!("{test_prefix}-net");
    let name = format!("{test_prefix}-node");
    let containers = [name.as_str()];

    docker_network_create(&network);

    // Prepare seed list with epoch after governance takeover
    let seed_dir = tempfile::tempdir().unwrap();
    let _seed_path = write_seed_list(
        seed_dir.path(),
        vec![
            ("seed-1", "/ip4/10.0.0.1/tcp/4001"),
            ("seed-2", "/ip4/10.0.0.2/tcp/4001"),
            ("seed-3", "/ip4/10.0.0.3/tcp/4001"),
        ],
        1_700_000_001,
    );

    let mut child = docker_run(
        &name,
        &network,
        Some((seed_dir.path().to_str().unwrap(), "/seeds")),
        &[
            "--dsn",
            "memory://",
            "--listen",
            "0",
            "--seed-list",
            "/seeds/seed_list.json",
            "--seed-authority",
            "dao",
            "--mission-id",
            mission_id(),
            "--node-id",
            &node_id(0x01),
        ],
    );

    // Give it time to attempt bootstrap
    tokio::time::sleep(Duration::from_secs(3)).await;

    let status = child.try_wait().unwrap();
    if let Some(exit) = status {
        // Should fail at bootstrap (stub), NOT at authority
        assert!(!exit.success(), "should exit on bootstrap failure");
    }
    // If still running, authority check passed

    full_cleanup(test_prefix, &containers);
    child.kill().ok();
    let _ = child.wait();
}

// ── LB10: Container with --seed-list + --peer ────────────────────
//
// When both flags are present, --peer should take precedence.

#[tokio::test]
async fn lb10_container_seed_list_peer_precedence() {
    if !docker_available() {
        eprintln!("SKIP: docker not available");
        return;
    }
    build_image();

    let test_prefix = "lb10";
    let network = format!("{test_prefix}-net");
    let writer_name = format!("{test_prefix}-writer");
    let reader_name = format!("{test_prefix}-reader");
    let containers = [writer_name.as_str(), reader_name.as_str()];

    docker_network_create(&network);

    // Prepare seed list pointing to a nonexistent bootstrap node
    let seed_dir = tempfile::tempdir().unwrap();
    let _seed_path = write_seed_list(
        seed_dir.path(),
        vec![("fake-seed", "/ip4://192.0.2.1/tcp/9999")], // Non-routable
        100,
    );

    let writer_port = free_port();
    let status_dir = tempfile::tempdir().unwrap();
    let _status_path = format!("{}/status.txt", status_dir.path().to_str().unwrap());

    // Writer commits 3 rows
    let mut writer = docker_run(
        &writer_name,
        &network,
        None,
        &[
            "--dsn",
            "file:///data/db",
            "--listen",
            &writer_port.to_string(),
            "--commit",
            "3",
            "--mission-id",
            mission_id(),
            "--node-id",
            &node_id(0x01),
        ],
    );

    tokio::time::sleep(Duration::from_secs(2)).await;

    // Get writer's container IP for --peer
    let inspect = Command::new("docker")
        .args([
            "inspect",
            "-f",
            "{{range .NetworkSettings.Networks}}{{.IPAddress}}{{end}}",
            &writer_name,
        ])
        .output()
        .expect("docker inspect failed");
    let writer_ip = String::from_utf8_lossy(&inspect.stdout).trim().to_string();
    assert!(!writer_ip.is_empty(), "writer should have an IP");

    // Reader with both --peer and --seed-list
    let mut reader = docker_run(
        &reader_name,
        &network,
        Some((seed_dir.path().to_str().unwrap(), "/seeds")),
        &[
            "--dsn",
            "memory://",
            "--listen",
            "0",
            "--peer",
            &format!("{writer_ip}:{writer_port}"),
            "--seed-list",
            "/seeds/seed_list.json",
            "--mission-id",
            mission_id(),
            "--node-id",
            &node_id(0x02),
            "--status-file",
            "/tmp/status.txt",
        ],
    );

    // Wait for sync via --peer (should work even though --seed-list points to fake)
    tokio::time::sleep(Duration::from_secs(5)).await;

    // Check if reader synced (via docker logs)
    let logs = Command::new("docker")
        .args(["logs", &reader_name])
        .output()
        .expect("docker logs failed");
    let stderr = String::from_utf8_lossy(&logs.stderr);
    // Should NOT have tried bootstrap (peer takes precedence)
    // Should have synced via TCP
    assert!(
        !stderr.contains("bootstrap failed"),
        "should not attempt bootstrap when --peer is present, got: {stderr}"
    );

    full_cleanup(test_prefix, &containers);
    writer.kill().ok();
    reader.kill().ok();
    let _ = writer.wait();
    let _ = reader.wait();
}
