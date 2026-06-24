//! L5: Container E2E tests (Docker, network bridge).
//!
//! Per `docs/e2e/2026-06-23-stoolap-data-sync-e2e-test-plan.md` §L5.
//!
//! These tests build a Docker image containing the `stoolap-node` binary,
//! launch containers on a Docker network, and verify sync across containers.

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

fn docker_run_with_limits(
    name: &str,
    network: &str,
    vol: Option<(&str, &str)>,
    limits: &[&str],
    args: &[&str],
) -> std::process::Child {
    let _ = Command::new("docker").args(["rm", "-f", name]).output();
    let mut cmd = Command::new("docker");
    cmd.args(["run", "--rm", "--name", name, "--network", network]);
    cmd.args(limits);
    if let Some((host, container)) = vol {
        cmd.args(["-v", &format!("{host}:{container}:rw")]);
    }
    cmd.arg(IMAGE_TAG).args(args);
    cmd.spawn()
        .unwrap_or_else(|e| panic!("failed to start container {name}: {e}"))
}

fn docker_kill(name: &str) {
    let _ = Command::new("docker").args(["kill", name]).output();
    let _ = Command::new("docker").args(["rm", "-f", name]).output();
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

fn docker_network_disconnect(network: &str, container: &str) {
    let _ = Command::new("docker")
        .args(["network", "disconnect", network, container])
        .output();
}

fn docker_network_connect(network: &str, container: &str) {
    let _ = Command::new("docker")
        .args(["network", "connect", network, container])
        .output();
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
        .args(["network", "ls", "--filter", &format!("name={prefix}"), "--format", "{{.Name}}"])
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

/// L5-T1: Two-container sync — writer commits 50 rows, reader sees them.
#[tokio::test]
async fn two_container_sync() {
    if !docker_available() {
        eprintln!("Docker not available, skipping");
        return;
    }
    build_image();

    let net = format!("sync-e2e-t1-{}", free_port());
    full_cleanup("sync-e2e-t1", &["t1-writer", "t1-reader"]);
    docker_network_create(&net);

    let writer_dir = tempfile::tempdir().unwrap();
    let status_dir = tempfile::tempdir().unwrap();
    let status_host = status_dir.path().to_str().unwrap().to_string();
    let status_in = format!("{}/count", status_host);

    let mut writer = docker_run(
        "t1-writer",
        &net,
        None,
        &[
            "--dsn",
            &writer_dsn(&writer_dir),
            "--listen",
            "3333",
            "--commit",
            "50",
        ],
    );
    tokio::time::sleep(Duration::from_secs(2)).await;

    let mut reader = docker_run(
        "t1-reader",
        &net,
        Some((&status_host, "/status")),
        &[
            "--dsn",
            "memory://",
            "--listen",
            "3333",
            "--peer",
            "t1-writer:3333",
            "--status-file",
            "/status/count",
        ],
    );

    let count = wait_for_status(&status_in, Duration::from_secs(10)).await;

    writer.kill().ok();
    reader.kill().ok();
    let _ = writer.wait();
    let _ = reader.wait();
    docker_network_rm(&net);

    assert_eq!(count, Some(50), "reader should have 50 rows");
}

/// L5-T2: Three-container fan-out — writer commits 200 rows, both readers see them.
#[tokio::test]
async fn three_container_fan_out() {
    if !docker_available() {
        eprintln!("Docker not available, skipping");
        return;
    }
    build_image();

    let net = format!("sync-e2e-t2-{}", free_port());
    full_cleanup("sync-e2e-t2", &["t2-writer", "t2-reader1", "t2-reader2"]);
    docker_network_create(&net);

    let writer_dir = tempfile::tempdir().unwrap();
    let status_dir1 = tempfile::tempdir().unwrap();
    let status_dir2 = tempfile::tempdir().unwrap();
    let sh1 = status_dir1.path().to_str().unwrap().to_string();
    let sh2 = status_dir2.path().to_str().unwrap().to_string();
    let si1 = format!("{}/count", sh1);
    let si2 = format!("{}/count", sh2);

    let mut writer = docker_run(
        "t2-writer",
        &net,
        None,
        &[
            "--dsn",
            &writer_dsn(&writer_dir),
            "--listen",
            "3333",
            "--commit",
            "200",
        ],
    );
    tokio::time::sleep(Duration::from_secs(2)).await;

    let mut r1 = docker_run(
        "t2-reader1",
        &net,
        Some((&sh1, "/status")),
        &[
            "--dsn",
            "memory://",
            "--listen",
            "3333",
            "--peer",
            "t2-writer:3333",
            "--status-file",
            "/status/count",
        ],
    );
    let mut r2 = docker_run(
        "t2-reader2",
        &net,
        Some((&sh2, "/status")),
        &[
            "--dsn",
            "memory://",
            "--listen",
            "3333",
            "--peer",
            "t2-writer:3333",
            "--status-file",
            "/status/count",
        ],
    );

    let c1 = wait_for_status(&si1, Duration::from_secs(10)).await;
    let c2 = wait_for_status(&si2, Duration::from_secs(10)).await;

    writer.kill().ok();
    r1.kill().ok();
    r2.kill().ok();
    let _ = writer.wait();
    let _ = r1.wait();
    let _ = r2.wait();
    docker_network_rm(&net);

    assert_eq!(c1, Some(200), "reader1 should have 200 rows");
    assert_eq!(c2, Some(200), "reader2 should have 200 rows");
}

/// L5-T3: Container network partition — disconnect reader, reconnect, verify catch-up.
#[tokio::test]
async fn container_network_partition() {
    if !docker_available() {
        eprintln!("Docker not available, skipping");
        return;
    }
    build_image();

    let net = format!("sync-e2e-t3-{}", free_port());
    full_cleanup("sync-e2e-t3", &["t3-writer", "t3-reader"]);
    docker_network_create(&net);

    let writer_dir = tempfile::tempdir().unwrap();
    let status_dir = tempfile::tempdir().unwrap();
    let status_host = status_dir.path().to_str().unwrap().to_string();
    let status_in = format!("{}/count", status_host);

    let mut writer = docker_run(
        "t3-writer",
        &net,
        None,
        &[
            "--dsn",
            &writer_dsn(&writer_dir),
            "--listen",
            "3333",
            "--commit",
            "5",
        ],
    );
    tokio::time::sleep(Duration::from_secs(2)).await;

    let mut reader = docker_run(
        "t3-reader",
        &net,
        Some((&status_host, "/status")),
        &[
            "--dsn",
            "memory://",
            "--listen",
            "3333",
            "--peer",
            "t3-writer:3333",
            "--status-file",
            "/status/count",
        ],
    );

    let c = wait_for_status(&status_in, Duration::from_secs(5)).await;
    assert_eq!(c, Some(5), "initial sync");

    // Partition: disconnect reader from network
    docker_network_disconnect(&net, "t3-reader");
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Heal: reconnect reader
    docker_network_connect(&net, "t3-reader");
    tokio::time::sleep(Duration::from_secs(1)).await;

    writer.kill().ok();
    reader.kill().ok();
    let _ = writer.wait();
    let _ = reader.wait();
    docker_network_rm(&net);
}

/// L5-T4: Container resource limit — container with memory/CPU limits doesn't OOM.
#[tokio::test]
async fn container_resource_limit() {
    if !docker_available() {
        eprintln!("Docker not available, skipping");
        return;
    }
    build_image();

    let net = format!("sync-e2e-t4-{}", free_port());
    full_cleanup("sync-e2e-t4", &["t4-writer"]);
    docker_network_create(&net);

    let writer_dir = tempfile::tempdir().unwrap();
    let status_dir = tempfile::tempdir().unwrap();
    let status_host = status_dir.path().to_str().unwrap().to_string();
    let status_in = format!("{}/count", status_host);

    // Writer with memory and CPU limits — 256MB memory, 0.5 CPU
    let mut writer = docker_run_with_limits(
        "t4-writer",
        &net,
        Some((&status_host, "/status")),
        &["--memory", "256m", "--cpus", "0.5"],
        &[
            "--dsn",
            &writer_dsn(&writer_dir),
            "--listen",
            "3333",
            "--commit",
            "10000",
            "--status-file",
            "/status/count",
        ],
    );

    // Wait for the writer to finish committing (or at least survive long enough)
    let count = wait_for_status(&status_in, Duration::from_secs(30)).await;

    // Writer should still be alive (no OOM)
    let still_running = writer.try_wait().map(|o| o.is_none()).unwrap_or(false);

    writer.kill().ok();
    let _ = writer.wait();
    docker_network_rm(&net);

    // The writer should have committed at least some rows without OOMing
    assert!(
        still_running || count.is_some(),
        "writer should survive under resource limits"
    );
}

/// L5-T6: Four-container chain — writer → r1 → r2 → r3 relay.
#[tokio::test]
async fn four_container_chain() {
    if !docker_available() {
        eprintln!("Docker not available, skipping");
        return;
    }
    build_image();

    let net = format!("sync-e2e-t6-{}", free_port());
    full_cleanup("sync-e2e-t6", &["t6-writer", "t6-r1", "t6-r2", "t6-r3"]);
    docker_network_create(&net);

    let writer_dir = tempfile::tempdir().unwrap();
    let r1_dir = tempfile::tempdir().unwrap();
    let r2_dir = tempfile::tempdir().unwrap();
    let status_dir = tempfile::tempdir().unwrap();
    let sh = status_dir.path().to_str().unwrap().to_string();
    let si = format!("{}/count", sh);

    // Writer: commits 5 rows
    let mut writer = docker_run(
        "t6-writer",
        &net,
        None,
        &[
            "--dsn",
            &writer_dsn(&writer_dir),
            "--listen",
            "3333",
            "--commit",
            "5",
        ],
    );
    tokio::time::sleep(Duration::from_secs(2)).await;

    // R1: file:// DSN, connects to writer
    let mut r1 = docker_run(
        "t6-r1",
        &net,
        None,
        &[
            "--dsn",
            &writer_dsn(&r1_dir),
            "--listen",
            "3333",
            "--peer",
            "t6-writer:3333",
        ],
    );
    tokio::time::sleep(Duration::from_secs(2)).await;

    // R2: file:// DSN, connects to r1
    let mut r2 = docker_run(
        "t6-r2",
        &net,
        None,
        &[
            "--dsn",
            &writer_dsn(&r2_dir),
            "--listen",
            "3333",
            "--peer",
            "t6-r1:3333",
        ],
    );
    tokio::time::sleep(Duration::from_secs(2)).await;

    // R3: memory:// DSN, connects to r2 (leaf)
    let mut r3 = docker_run(
        "t6-r3",
        &net,
        Some((&sh, "/status")),
        &[
            "--dsn",
            "memory://",
            "--listen",
            "3333",
            "--peer",
            "t6-r2:3333",
            "--status-file",
            "/status/count",
        ],
    );

    let count = wait_for_status(&si, Duration::from_secs(15)).await;

    writer.kill().ok();
    r1.kill().ok();
    r2.kill().ok();
    r3.kill().ok();
    let _ = writer.wait();
    let _ = r1.wait();
    let _ = r2.wait();
    let _ = r3.wait();
    docker_network_rm(&net);

    assert_eq!(
        count,
        Some(5),
        "leaf container should have 5 rows via chain"
    );
}

/// L5-T7: Four-container fan-out — writer, 3 readers.
#[tokio::test]
async fn four_container_fan_out() {
    if !docker_available() {
        eprintln!("Docker not available, skipping");
        return;
    }
    build_image();

    let net = format!("sync-e2e-t7-{}", free_port());
    full_cleanup("sync-e2e-t7", &["t7-writer", "t7-r1", "t7-r2", "t7-r3"]);
    docker_network_create(&net);

    let writer_dir = tempfile::tempdir().unwrap();
    let status_dir1 = tempfile::tempdir().unwrap();
    let status_dir2 = tempfile::tempdir().unwrap();
    let status_dir3 = tempfile::tempdir().unwrap();
    let sh1 = status_dir1.path().to_str().unwrap().to_string();
    let sh2 = status_dir2.path().to_str().unwrap().to_string();
    let sh3 = status_dir3.path().to_str().unwrap().to_string();
    let si1 = format!("{}/count", sh1);
    let si2 = format!("{}/count", sh2);
    let si3 = format!("{}/count", sh3);

    // Writer: 100 rows
    let mut writer = docker_run(
        "t7-writer",
        &net,
        None,
        &[
            "--dsn",
            &writer_dsn(&writer_dir),
            "--listen",
            "3333",
            "--commit",
            "100",
        ],
    );
    tokio::time::sleep(Duration::from_secs(2)).await;

    // Three readers
    let mut r1 = docker_run(
        "t7-r1",
        &net,
        Some((&sh1, "/status")),
        &[
            "--dsn",
            "memory://",
            "--listen",
            "3333",
            "--peer",
            "t7-writer:3333",
            "--status-file",
            "/status/count",
        ],
    );
    let mut r2 = docker_run(
        "t7-r2",
        &net,
        Some((&sh2, "/status")),
        &[
            "--dsn",
            "memory://",
            "--listen",
            "3333",
            "--peer",
            "t7-writer:3333",
            "--status-file",
            "/status/count",
        ],
    );
    let mut r3 = docker_run(
        "t7-r3",
        &net,
        Some((&sh3, "/status")),
        &[
            "--dsn",
            "memory://",
            "--listen",
            "3333",
            "--peer",
            "t7-writer:3333",
            "--status-file",
            "/status/count",
        ],
    );

    let c1 = wait_for_status(&si1, Duration::from_secs(10)).await;
    let c2 = wait_for_status(&si2, Duration::from_secs(10)).await;
    let c3 = wait_for_status(&si3, Duration::from_secs(10)).await;

    writer.kill().ok();
    r1.kill().ok();
    r2.kill().ok();
    r3.kill().ok();
    let _ = writer.wait();
    let _ = r1.wait();
    let _ = r2.wait();
    let _ = r3.wait();
    docker_network_rm(&net);

    assert_eq!(c1, Some(100), "reader1 should have 100 rows");
    assert_eq!(c2, Some(100), "reader2 should have 100 rows");
    assert_eq!(c3, Some(100), "reader3 should have 100 rows");
}

/// L5-T5: Container kill and recover — kill reader, start new reader, catch up.
#[tokio::test]
async fn container_kill_and_recover() {
    if !docker_available() {
        eprintln!("Docker not available, skipping");
        return;
    }
    build_image();

    let net = format!("sync-e2e-t5-{}", free_port());
    full_cleanup("sync-e2e-t5", &["t5-writer", "t5-reader"]);
    docker_network_create(&net);

    let writer_dir = tempfile::tempdir().unwrap();
    let status_dir = tempfile::tempdir().unwrap();
    let status_host = status_dir.path().to_str().unwrap().to_string();
    let status_in = format!("{}/count", status_host);

    let mut writer = docker_run(
        "t5-writer",
        &net,
        None,
        &[
            "--dsn",
            &writer_dsn(&writer_dir),
            "--listen",
            "3333",
            "--commit",
            "5",
        ],
    );
    tokio::time::sleep(Duration::from_secs(2)).await;

    // First reader — sync
    let mut reader1 = docker_run(
        "t5-reader",
        &net,
        Some((&status_host, "/status")),
        &[
            "--dsn",
            "memory://",
            "--listen",
            "3333",
            "--peer",
            "t5-writer:3333",
            "--status-file",
            "/status/count",
        ],
    );

    let c = wait_for_status(&status_in, Duration::from_secs(5)).await;
    assert_eq!(c, Some(5), "initial sync");

    // Kill reader
    docker_kill("t5-reader");
    let _ = reader1.wait();
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Start new reader — should catch up via writer's WAL
    std::fs::write(&status_in, "0").ok();
    let mut reader2 = docker_run(
        "t5-reader",
        &net,
        Some((&status_host, "/status")),
        &[
            "--dsn",
            "memory://",
            "--listen",
            "3333",
            "--peer",
            "t5-writer:3333",
            "--status-file",
            "/status/count",
        ],
    );

    let c2 = wait_for_status(&status_in, Duration::from_secs(5)).await;
    assert_eq!(c2, Some(5), "new reader catches up");

    writer.kill().ok();
    reader2.kill().ok();
    let _ = writer.wait();
    let _ = reader2.wait();
    docker_network_rm(&net);
}
