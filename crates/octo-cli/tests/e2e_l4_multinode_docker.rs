//! Layer 4 (cross-container) multi-node end-to-end suite — the
//! centrepiece of the multi-peer operator test programme.
//!
//! **Every test in this file is `#[ignore]`d** and needs a running
//! docker engine with compose v2. Run them with:
//!
//! ```text
//! cargo test -p octo-cli --test e2e_l4_multinode_docker -- --ignored --test-threads=1
//! ```
//!
//! `--test-threads=1` is a recommendation, not a requirement: each
//! scenario gets its own compose project name, so they are safe to
//! run concurrently. It is suggested only because the first run builds
//! the node image and concurrent builds serialise on the same docker
//! layer cache.
//!
//! # What a "node" is here
//!
//! Two inputs, and only two, distinguish one octo node from another:
//! its `$OCTO_HOME` and its network namespace. This suite isolates
//! both at once — a private named volume for the home, a container
//! filesystem for everything else, and a compose bridge with per-node
//! DNS aliases for the network. That is what makes a scenario here a
//! genuinely cross-container result rather than a temp-directory
//! result wearing a container costume.
//!
//! The cross-process suite (`e2e_l3_multinode.rs`) already covers
//! "separate processes, separate homes, one machine". What only this
//! suite can prove is the network claim: that an endpoint an operator
//! records in one container's peer table names an address that is
//! genuinely routable from that container, and that the address stops
//! working when the peer container stops.
//!
//! # What a node container is not
//!
//! `octo` is a one-shot dispatcher. There is no `serve` subcommand and
//! no daemon mode, so a node container holds no long-running octo
//! process. It provides the environment — the home, the filesystem,
//! and the network namespace — and the suite drives `octo` inside it
//! with `docker compose exec`. Likewise, `octo` opens no listening
//! socket, so the suite starts the listener it probes with
//! `netcat`. Both facts are stated here rather than papered over,
//! because a test that implied octo was serving would be asserting
//! something the binary does not do.

mod common;

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::thread;
use std::time::{Duration, Instant};

use common::{canonical_did, Envelope, UNTRUSTED};
use serde_json::Value;

/// Per-test compose project counter. Combined with the process id and
/// a slug it makes every project name unique, so scenarios running
/// concurrently own disjoint networks, volumes, and container names.
static PROJECT_COUNTER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// The port the suite listens on and records in peer tables. Inside an
/// isolated container this is unconstrained; it matches the port the
/// quota-router Layer 4 harness uses so both suites read alike.
const PROBE_PORT: u16 = 9100;

/// Script that keeps a TCP listener up on `PROBE_PORT`.
///
/// A loop rather than a single `nc -l` because a listener that exits
/// after one connection would turn a second probe into a false
/// partition. The suite re-launches this after any container restart,
/// since a detached `exec` dies with its container.
fn listener_script() -> String {
    format!("while true; do nc -l {PROBE_PORT} >/dev/null 2>&1 || sleep 1; done")
}

/// One node in the suite, addressed by compose service name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Node {
    A,
    B,
    C,
}

impl std::fmt::Display for Node {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.service())
    }
}

impl Node {
    const ALL: [Node; 3] = [Node::A, Node::B, Node::C];

    /// Compose service name.
    fn service(self) -> &'static str {
        match self {
            Node::A => "node-a",
            Node::B => "node-b",
            Node::C => "node-c",
        }
    }

    /// Address the other nodes may legitimately record for this one.
    /// Compose gives every service its own name as a network alias, so
    /// this is the address a real operator on the same bridge would
    /// write.
    fn dialable(self) -> String {
        format!("tcp://{}:{PROBE_PORT}", self.service())
    }
}

/// A compose stack owned by exactly one scenario.
struct Compose {
    project: String,
    file: PathBuf,
}

impl Compose {
    /// Build the node image, start the stack, and wait for every
    /// service to report healthy.
    fn start(slug: &str, file_name: &str) -> Self {
        let stack = Self {
            project: {
                let n = PROJECT_COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                format!("octo-e2e-l4-{slug}-{}-{n}", std::process::id())
            },
            file: docker_dir().join(file_name),
        };
        build_node_image_once();
        // `--wait` blocks until every service's healthcheck passes.
        // The timeout stops a broken image from hanging the suite
        // forever; without it a failing test is indistinguishable from
        // a wedged one.
        stack.run_ok(
            &["up", "-d", "--wait", "--wait-timeout", "180"],
            "start the node stack",
        );
        stack
    }

    /// The directory holding the Dockerfile, compose files, and
    /// entrypoint, resolved relative to this crate so the suite runs
    /// from any working directory.
    fn dir() -> PathBuf {
        docker_dir()
    }

    fn base(&self, sub: &[&str]) -> Command {
        let mut cmd = Command::new("docker");
        cmd.arg("compose")
            .arg("-p")
            .arg(&self.project)
            .arg("-f")
            .arg(&self.file)
            .args(sub);
        cmd
    }

    /// Run a compose subcommand, requiring success and surfacing both
    /// streams on failure.
    fn run_ok(&self, sub: &[&str], what: &str) -> Output {
        let out = self
            .base(sub)
            .output()
            .unwrap_or_else(|e| panic!("failed to spawn `docker compose {}`: {e}", sub.join(" ")));
        assert!(
            out.status.success(),
            "could not {what}: `docker compose {}` exited {:?}\n--- stdout ---\n{}\n--- stderr ---\n{}",
            sub.join(" "),
            out.status.code(),
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr),
        );
        out
    }

    /// Run `octo` inside one node container.
    ///
    /// The compose service already exports `OCTO_HOME`, `NO_COLOR`,
    /// and `OCTO_FORCE_JSON`, so the invocation needs no environment
    /// plumbing of its own.
    fn octo(&self, node: Node, args: &[&str]) -> Output {
        self.octo_env(node, &[], args)
    }

    /// Run `octo` inside one node container with extra environment.
    fn octo_env(&self, node: Node, env: &[(&str, &str)], args: &[&str]) -> Output {
        let mut cmd = self.base(&["exec", "-T"]);
        for (k, v) in env {
            cmd.arg("-e").arg(format!("{k}={v}"));
        }
        cmd.arg(node.service()).arg("octo").args(args);
        cmd.output()
            .unwrap_or_else(|e| panic!("failed to spawn octo in {}: {e}", node.service()))
    }

    /// Run a shell snippet inside one node container.
    fn sh(&self, node: Node, script: &str) -> Output {
        self.base(&["exec", "-T", node.service(), "sh", "-c", script])
            .output()
            .unwrap_or_else(|e| panic!("failed to spawn sh in {}: {e}", node.service()))
    }

    /// Start a detached long-running command in a node container.
    fn sh_detached(&self, node: Node, script: &str) {
        self.run_ok(
            &["exec", "-d", node.service(), "sh", "-c", script],
            "launch a detached command",
        );
    }

    fn restart(&self, node: Node) {
        self.run_ok(&["restart", node.service()], "restart a node");
    }

    fn stop(&self, node: Node) {
        self.run_ok(&["stop", node.service()], "stop a node");
    }

    fn start_node(&self, node: Node) {
        self.run_ok(&["start", node.service()], "start a node");
    }

    /// Destroy the stack, its volumes, and its network.
    fn down_v(&self) {
        // Best effort by design: `Drop` calls this too, and a scenario
        // that already tore down successfully must not fail on the
        // second pass.
        let _ = self.base(&["down", "-v", "--remove-orphans"]).output();
    }

    /// `octo mesh peer add` under the scripted write gate.
    fn peer_add(&self, node: Node, did: &str, endpoint: &str) -> Output {
        self.octo(
            node,
            &[
                "--mode",
                "ci",
                "--allow-write",
                "mesh",
                "peer",
                "add",
                did,
                "--endpoint",
                endpoint,
                "--json",
            ],
        )
    }

    /// `octo mesh peer list`, parsed.
    fn peer_list(&self, node: Node) -> Envelope {
        let out = self.octo(node, &["mesh", "peer", "list", "--json"]);
        assert!(
            out.status.success(),
            "peer list on {} failed: {}",
            node.service(),
            String::from_utf8_lossy(&out.stderr)
        );
        Envelope::parse(&String::from_utf8_lossy(&out.stdout))
    }
}

/// Teardown on unwind. Without this a panicking scenario leaks a
/// compose stack, its volumes, and its network for the rest of the
/// machine's life, and the next run trips over the leftovers.
impl Drop for Compose {
    fn drop(&mut self) {
        self.down_v();
    }
}

fn docker_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("e2e_docker")
}

/// Build the node image once, so a missing docker engine or a broken
/// Dockerfile is reported as one clear failure rather than as a
/// dozen identical ones.
fn ensure_docker_available() {
    let out = Command::new("docker")
        .arg("compose")
        .arg("version")
        .output()
        .expect(
            "`docker compose` is not runnable: the Layer 4 suite needs a docker engine \
                 with compose v2 on PATH",
        );
    assert!(
        out.status.success(),
        "`docker compose version` failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// Build the node image exactly once per process, under a project name
/// that is deliberately NOT the scenario's.
///
/// This saves time, not disk, and the distinction is measured rather
/// than assumed. Every service in both compose files names the same
/// fixed tag and every scenario builds from the same context and the
/// same Dockerfile, so the fifteen builds were fourteen redundant ones:
/// each re-stats the context, re-sends it to the builder, and
/// re-resolves. BuildKit's cache is content-addressed and global, so
/// those fourteen were already hitting — running the old
/// build-per-scenario pattern fifteen times costs 3.526 GB of cache,
/// exactly what the single build costs. The redundancy was real; the
/// disk cost was not.
///
/// The build runs under its own project name because the tag is
/// explicit: compose writes `octo-e2e-l4:local` no matter which
/// project built it, so a scenario's own `up` finds the image already
/// present and does not rebuild. Isolation is unaffected — the project
/// name is what separates the networks, volumes, and container names,
/// and that is still unique per scenario.
fn build_node_image_once() {
    static ONCE: std::sync::OnceLock<()> = std::sync::OnceLock::new();
    ONCE.get_or_init(|| {
        let out = Command::new("docker")
            .arg("compose")
            .arg("-p")
            .arg("octo-e2e-l4-image")
            .arg("-f")
            .arg(docker_dir().join("compose-2node.yaml"))
            .arg("build")
            .output()
            .expect("failed to spawn `docker compose build`");
        assert!(
            out.status.success(),
            "could not build the node image: `docker compose build` exited {:?}\n--- stdout ---\n{}\n--- stderr ---\n{}",
            out.status.code(),
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
    });
}

/// The endpoint string recorded for one peer, read back out of the
/// operator-facing peer list.
fn endpoint_of(envelope: &Envelope, did: &str) -> String {
    envelope
        .peers()
        .iter()
        .find(|p| p.get("peer_did").and_then(Value::as_str) == Some(did))
        .and_then(|p| p.get("endpoint").and_then(Value::as_str))
        .unwrap_or_else(|| panic!("no peer record for {did} in {}", envelope.payload))
        .to_string()
}

/// Split `tcp://host:port` into its parts.
fn split_tcp(endpoint: &str) -> (String, String) {
    let rest = endpoint
        .strip_prefix("tcp://")
        .unwrap_or_else(|| panic!("endpoint {endpoint} is not a tcp endpoint"));
    let (host, port) = rest
        .rsplit_once(':')
        .unwrap_or_else(|| panic!("endpoint {endpoint} carries no port"));
    (host.to_string(), port.to_string())
}

/// Poll a TCP connect from one node until it succeeds or `timeout`
/// elapses. Used only on the heal side of a partition, where the
/// container has just been started and needs a moment to join the
/// bridge; a bounded poll is honest about that, where a fixed sleep
/// would be a guess dressed as a synchronisation primitive.
///
/// The project name and compose file are threaded through explicitly.
/// Omitting them would silently target the default project in the
/// working directory, which is the kind of probe that reports "peer
/// unreachable" for the wrong reason.
fn wait_for_reachability(
    stack: &Compose,
    from: Node,
    host: &str,
    port: &str,
    timeout: Duration,
) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        let probe = stack
            .base(&[
                "exec",
                "-T",
                from.service(),
                "nc",
                "-z",
                "-w",
                "2",
                host,
                port,
            ])
            .output();
        if let Ok(out) = probe {
            if out.status.success() {
                return true;
            }
        }
        if Instant::now() >= deadline {
            return false;
        }
        thread::sleep(Duration::from_millis(500));
    }
}

/// Give the docker bridge a beat to withdraw a stopped container's
/// DNS entry and reject in-flight connections before probing for
/// failure. A partition assertion that races the fabric proves
/// nothing.
const PARTITION_SETTLE: Duration = Duration::from_secs(2);

// ── Scenarios ───────────────────────────────────────────────────────────────

/// A freshly started node answers reads from an empty home and, in
/// doing so, writes nothing.
#[test]
#[ignore = "requires a running docker engine and compose v2"]
fn l4_fresh_node_answers_reads_without_writing_a_peer_table() {
    ensure_docker_available();
    let stack = Compose::start("fresh", "compose-2node.yaml");

    for node in [Node::A, Node::B] {
        let listed = stack.peer_list(node);
        assert_eq!(
            listed.total_count(),
            0,
            "{} must start with an empty peer table",
            node.service()
        );
        assert!(listed.peers().is_empty());

        // A read must not materialise the file. A node that gained an
        // empty peers.toml on first read would make "the table exists"
        // mean nothing as a durability signal later in the suite.
        let exists = stack.sh(node, "test -e /octo/home/mesh/peers.toml");
        assert!(
            !exists.status.success(),
            "{} gained a peer table from a read alone",
            node.service()
        );
    }
}

/// A binding made on one node is invisible to the other: the volumes
/// are separate, so neither node can observe the other's write.
#[test]
#[ignore = "requires a running docker engine and compose v2"]
fn l4_peer_binding_is_visible_only_on_the_node_that_made_it() {
    ensure_docker_available();
    let stack = Compose::start("isolated", "compose-2node.yaml");

    let peer_for_a = canonical_did(60);
    let peer_for_b = canonical_did(61);

    assert!(stack
        .peer_add(Node::A, &peer_for_a, &Node::B.dialable())
        .status
        .success());
    assert!(stack
        .peer_add(Node::B, &peer_for_b, &Node::A.dialable())
        .status
        .success());

    let on_a = stack.peer_list(Node::A);
    let on_b = stack.peer_list(Node::B);
    assert_eq!(
        on_a.peer_dids(),
        vec![peer_for_a.clone()],
        "node-a must see only the binding it made"
    );
    assert_eq!(
        on_b.peer_dids(),
        vec![peer_for_b.clone()],
        "node-b must see only the binding it made"
    );

    // Cross-container isolation is a filesystem claim as much as a
    // behavioural one, so confirm node-a's table on disk never
    // mentions the binding made inside node-b. A shared volume behind
    // two service names would satisfy every assertion above this line.
    let leaked = stack.sh(
        Node::A,
        &format!("grep -q {peer_for_b} /octo/home/mesh/peers.toml"),
    );
    assert!(
        !leaked.status.success(),
        "node-b's binding must not appear in node-a's peer table on disk"
    );
}

/// The peer table lives on a named volume, so it survives a container
/// restart with its owner-only permissions intact.
#[test]
#[ignore = "requires a running docker engine and compose v2"]
fn l4_peer_table_survives_a_container_restart() {
    ensure_docker_available();
    let stack = Compose::start("durable", "compose-2node.yaml");

    let peer = canonical_did(62);
    assert!(stack
        .peer_add(Node::A, &peer, &Node::B.dialable())
        .status
        .success());

    stack.restart(Node::A);

    let listed = stack.peer_list(Node::A);
    assert_eq!(
        listed.peer_dids(),
        vec![peer.clone()],
        "the binding must outlive the container that made it"
    );
    assert_eq!(
        endpoint_of(&listed, &peer),
        Node::B.dialable(),
        "a restart must not rewrite the recorded endpoint"
    );

    // Owner-only permissions are a substrate contract, and a volume
    // mount must not quietly relax them.
    let perms = stack.sh(Node::A, "stat -c %a /octo/home/mesh/peers.toml");
    assert_eq!(
        String::from_utf8_lossy(&perms.stdout).trim(),
        "700",
        "the peer table must keep owner-only permissions across a restart"
    );
}

/// A node inside a container reports no identity, exactly as the
/// cross-process suite found on the host. Pinning it here stops the
/// two suites from drifting apart on a substrate stub.
#[test]
#[ignore = "requires a running docker engine and compose v2"]
fn l4_node_reports_no_active_identity() {
    ensure_docker_available();
    let stack = Compose::start("identity", "compose-2node.yaml");

    let out = stack.octo(Node::A, &["whoami"]);
    assert_eq!(
        out.status.code(),
        Some(2),
        "a node with no registered identity must report the no-active-identity exit"
    );
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("no active identity"),
        "the envelope must name the condition, got: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        !stack
            .sh(Node::A, "test -e /octo/home/wallet")
            .status
            .success(),
        "asking whoami must not materialise a wallet store"
    );
}

/// `OCTO_AUDIT=1` is a read-only enforcement switch. Inside a
/// container, where a fleet-wide environment is the normal way to
/// apply one, it must beat an explicit write mode on the command
/// line — otherwise any caller opts itself out of the policy the
/// environment just declared.
#[test]
#[ignore = "requires a running docker engine and compose v2"]
fn l4_audit_switch_denies_writes_despite_an_explicit_write_mode() {
    ensure_docker_available();
    let stack = Compose::start("audit", "compose-2node.yaml");

    let peer = canonical_did(63);
    let out = stack.octo_env(
        Node::A,
        &[("OCTO_AUDIT", "1")],
        &[
            "--mode",
            "ci",
            "--allow-write",
            "mesh",
            "peer",
            "add",
            &peer,
            "--endpoint",
            &Node::B.dialable(),
            "--json",
        ],
    );
    assert_eq!(
        out.status.code(),
        Some(2),
        "the audit switch must deny the write even with an explicit write mode"
    );
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("read-only"),
        "the denial must state the read-only reason, got: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        stack.peer_list(Node::A).total_count(),
        0,
        "a denied write must leave the peer table untouched"
    );
}

/// The control for the audit scenario: the same command, without the
/// switch, writes. Without this a suite could pass by denying
/// everything.
#[test]
#[ignore = "requires a running docker engine and compose v2"]
fn l4_write_mode_writes_when_the_audit_switch_is_absent() {
    ensure_docker_available();
    let stack = Compose::start("ciwrite", "compose-2node.yaml");

    let peer = canonical_did(64);
    assert!(
        stack
            .peer_add(Node::A, &peer, &Node::B.dialable())
            .status
            .success(),
        "the same invocation denied in the audit scenario must succeed without the switch"
    );
    assert_eq!(stack.peer_list(Node::A).total_count(), 1);
}

/// The centrepiece. An endpoint an operator records in one container's
/// peer table must name an address that is genuinely routable from
/// that container, and the address is read back out of the table
/// rather than reused from the input, so the test proves what is
/// *stored* is what is *dialable*.
#[test]
#[ignore = "requires a running docker engine and compose v2"]
fn l4_recorded_tcp_endpoint_is_reachable_from_the_recording_node() {
    ensure_docker_available();
    let stack = Compose::start("star", "compose-2node.yaml");

    stack.sh_detached(Node::B, &listener_script());
    assert!(
        wait_for_reachability(
            &stack,
            Node::B,
            "127.0.0.1",
            &PROBE_PORT.to_string(),
            Duration::from_secs(30)
        ),
        "the listener never came up on {}",
        Node::B.service()
    );

    let peer = canonical_did(65);
    assert!(stack
        .peer_add(Node::A, &peer, &Node::B.dialable())
        .status
        .success());

    // Read the endpoint back out of the operator-facing list rather
    // than reusing the string that went in.
    let listed = stack.peer_list(Node::A);
    let recorded = endpoint_of(&listed, &peer);
    assert_eq!(
        recorded,
        Node::B.dialable(),
        "the stored endpoint must be the one the operator supplied"
    );

    let (host, port) = split_tcp(&recorded);
    assert_eq!(host, Node::B.service());
    assert_eq!(port, PROBE_PORT.to_string());

    let probe = stack.sh(Node::A, &format!("nc -z -w 3 {host} {port}"));
    assert!(
        probe.status.success(),
        "the endpoint {} recorded on {} must be dialable from that node's own network \
         namespace; probe stderr: {}",
        recorded,
        Node::A.service(),
        String::from_utf8_lossy(&probe.stderr)
    );
}

/// A stopped peer container must break the recorded address, and a
/// restarted one must restore it, with the operator's own record
/// unchanged throughout. This is the only scenario in the suite that
/// models a network partition.
#[test]
#[ignore = "requires a running docker engine and compose v2"]
fn l4_stopping_the_peer_container_breaks_the_recorded_endpoint_and_healing_restores_it() {
    ensure_docker_available();
    let stack = Compose::start("partition", "compose-2node.yaml");

    let port = PROBE_PORT.to_string();
    stack.sh_detached(Node::B, &listener_script());
    assert!(wait_for_reachability(
        &stack,
        Node::B,
        "127.0.0.1",
        &port,
        Duration::from_secs(30)
    ));

    let peer = canonical_did(66);
    assert!(stack
        .peer_add(Node::A, &peer, &Node::B.dialable())
        .status
        .success());
    let recorded = endpoint_of(&stack.peer_list(Node::A), &peer);
    let (host, port) = split_tcp(&recorded);

    stack.stop(Node::B);
    thread::sleep(PARTITION_SETTLE);

    // Probe more than once: a single sample could catch a stray
    // success and mask the partition, and could equally trip on a
    // slow refusal. Three samples is a claim, not a proof, but it is
    // a claim the fabric can actually support.
    for attempt in 1..=3 {
        let probe = stack.sh(Node::A, &format!("nc -z -w 2 {host} {port}"));
        assert!(
            !probe.status.success(),
            "attempt {attempt}: {} must not be dialable while {} is stopped",
            recorded,
            Node::B.service()
        );
    }

    stack.start_node(Node::B);
    // A detached exec dies with its container, so the listener has to
    // be relaunched on the healed node. That is a property of the
    // harness, not of the mesh, and the test must not paper over it.
    stack.sh_detached(Node::B, &listener_script());

    assert!(
        wait_for_reachability(&stack, Node::A, &host, &port, Duration::from_secs(60)),
        "{} must become dialable again once {} is running",
        recorded,
        Node::B.service()
    );

    assert_eq!(
        endpoint_of(&stack.peer_list(Node::A), &peer),
        recorded,
        "the operator's record must survive the partition unchanged"
    );
}

/// Three nodes, full mesh: every ordered pair bound, so every node
/// holds exactly two out-edges and none of them can see the other
/// four. The union across the three tables is the full set of six
/// directed bindings.
#[test]
#[ignore = "requires a running docker engine and compose v2"]
fn l4_three_node_full_mesh_converges_on_six_directed_bindings() {
    ensure_docker_available();
    let stack = Compose::start("fullmesh", "compose-3node.yaml");

    // One distinct peer DID per directed edge, so a binding that
    // appears on two nodes is detectable rather than coincidental.
    let mut edge_did = BTreeSet::new();
    let mut seed = 70u8;
    for from in Node::ALL {
        for to in Node::ALL {
            if from == to {
                continue;
            }
            let did = canonical_did(seed);
            seed += 1;
            assert!(
                stack.peer_add(from, &did, &to.dialable()).status.success(),
                "adding {from} -> {to} failed"
            );
            edge_did.insert(did);
        }
    }

    let mut union = BTreeSet::new();
    for node in Node::ALL {
        let listed = stack.peer_list(node);
        assert_eq!(
            listed.total_count(),
            2,
            "{} must hold exactly its two out-edges",
            node.service()
        );
        let mine: BTreeSet<String> = listed.peer_dids().into_iter().collect();
        for other in Node::ALL {
            if other != node {
                let theirs: BTreeSet<String> =
                    stack.peer_list(other).peer_dids().into_iter().collect();
                let overlap: Vec<&String> = mine.intersection(&theirs).collect();
                assert!(
                    overlap.is_empty(),
                    "{} and {} must not share a binding, found {overlap:?}",
                    node.service(),
                    other.service()
                );
            }
        }
        union.extend(mine);
    }

    assert_eq!(
        union.len(),
        6,
        "the full mesh is six directed bindings, got {union:?}"
    );
    assert_eq!(
        union, edge_did,
        "the union must be exactly the edges written"
    );
}

/// Endpoint validation is per-node state, so a scheme rejected on one
/// node must leave that node's table as empty as it found it — and
/// must not touch its peers.
#[test]
#[ignore = "requires a running docker engine and compose v2"]
fn l4_endpoint_scheme_allowlist_is_enforced_per_node() {
    ensure_docker_available();
    let stack = Compose::start("schemes", "compose-2node.yaml");

    for bad in [
        "file:///etc/passwd",
        "http://node-b:9100",
        "ws://node-b:9100",
    ] {
        let peer = canonical_did(80 + bad.len() as u8);
        let out = stack.peer_add(Node::A, &peer, bad);
        assert_eq!(
            out.status.code(),
            Some(28),
            "disallowed scheme {bad} must be refused with the endpoint-scheme exit"
        );
    }

    assert_eq!(
        stack.peer_list(Node::A).total_count(),
        0,
        "a rejected endpoint must not be recorded"
    );
    assert_eq!(
        stack.peer_list(Node::B).total_count(),
        0,
        "a rejection on one node must not disturb another"
    );
}

/// Teardown removes everything the scenario owned. A harness that
/// leaves volumes and networks behind makes every later run
/// non-reproducible, so this is asserted rather than assumed.
#[test]
#[ignore = "requires a running docker engine and compose v2"]
fn l4_teardown_removes_every_volume_and_network_the_scenario_owned() {
    ensure_docker_available();
    let stack = Compose::start("teardown", "compose-2node.yaml");

    assert!(stack
        .peer_add(Node::A, &canonical_did(90), &Node::B.dialable())
        .status
        .success());

    let project = stack.project.clone();
    stack.down_v();

    for kind in ["volume", "network"] {
        let out = Command::new("docker")
            .arg(format!("{kind}s"))
            .arg("ls")
            .arg("-q")
            .arg("--filter")
            .arg(format!("label=com.docker.compose.project={project}"))
            .output()
            .expect("query docker for leftovers");
        let leftovers = String::from_utf8_lossy(&out.stdout);
        assert!(
            leftovers.trim().is_empty(),
            "scenario {project} left {kind}s behind: {}",
            leftovers.trim()
        );
    }
}

/// Guards the suite's own plumbing: the docker fixtures must be where
/// the test expects them, and the entrypoint must be executable in the
/// image. A missing or non-executable entrypoint fails every scenario
/// with the same opaque `exec format error`, so it is checked once,
/// here, where the message can be about the fixture.
#[test]
#[ignore = "requires a running docker engine and compose v2"]
fn l4_fixtures_are_present_on_the_test_filesystem() {
    let dir = Compose::dir();
    for file in [
        "Dockerfile",
        "compose-2node.yaml",
        "compose-3node.yaml",
        "entrypoint.sh",
    ] {
        assert!(
            dir.join(file).exists(),
            "the Layer 4 suite is missing its fixture {file}"
        );
    }

    // The compose build context and the Dockerfile path inside it are
    // both resolved relative to the compose file's own directory. A
    // wrong depth yields a context that exists but is not the
    // workspace root, and docker reports it as a bare
    // `lstat <path>: no such file or directory` that names the
    // resolved directory rather than the mistake.
    //
    // Reading the paths out of the compose file rather than restating
    // them here is the point: a hardcoded copy in the test is a second
    // place to get the depth wrong, and it would still pass when the
    // compose file it was copied from had drifted.
    for compose in ["compose-2node.yaml", "compose-3node.yaml"] {
        let text = std::fs::read_to_string(dir.join(compose))
            .unwrap_or_else(|e| panic!("reading {compose}: {e}"));
        let compose_dir = dir.clone();

        let context = field(&text, "context:")
            .unwrap_or_else(|| panic!("{compose} declares no build context"));
        let context = compose_dir.join(context);
        assert!(
            context.join("Cargo.toml").exists(),
            "{compose} build context must resolve to the workspace root, found {}",
            context.display()
        );

        let dockerfile = field(&text, "dockerfile:")
            .unwrap_or_else(|| panic!("{compose} declares no dockerfile"));
        assert!(
            context.join(dockerfile).exists(),
            "{compose} names a Dockerfile that does not exist under its context: {dockerfile}"
        );

        // The pinned toolchain has to be inside the context, or the
        // image compiles on whatever channel the base image ships and
        // drifts from the one CI uses.
        assert!(
            context.join("rust-toolchain.toml").exists(),
            "the build context for {compose} must carry the pinned toolchain"
        );
    }
}

/// Pull the value after `key:` out of a compose file, trimming quotes
/// and trailing comments. Deliberately a line scan rather than a YAML
/// dependency: this crate has no serde_yaml in its dev-dependencies,
/// and adding one to assert three strings would be the larger change.
fn field<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    text.lines()
        .find(|l| l.trim_start().starts_with(key))
        .and_then(|l| l.split_once(':'))
        .map(|(_, v)| v.split('#').next().unwrap_or(v).trim().trim_matches('"'))
        .filter(|v| !v.is_empty())
}

/// Reading the guide's own value spaces against a real node: the
/// trust-level filter takes the typed discriminator, and every peer a
/// `peer add` writes starts Untrusted.
#[test]
#[ignore = "requires a running docker engine and compose v2"]
fn l4_trust_level_filter_reads_the_urn_value_space() {
    ensure_docker_available();
    let stack = Compose::start("trust", "compose-2node.yaml");

    let peer = canonical_did(95);
    assert!(stack
        .peer_add(Node::A, &peer, &Node::B.dialable())
        .status
        .success());

    let listed = stack.peer_list(Node::A);
    let entry: &Value = listed
        .peers()
        .iter()
        .find(|p| p.get("peer_did").and_then(Value::as_str) == Some(peer.as_str()))
        .expect("the added peer is listed");
    assert_eq!(
        entry["trust_level"], UNTRUSTED,
        "a peer added through the CLI starts Untrusted"
    );

    // Filtering on the discriminator that is actually stored returns
    // the peer; filtering on a different one returns nothing while
    // leaving the table intact. The distinction between those two
    // outcomes is what a human-word value space silently collapses.
    let mut cmd = stack.base(&["exec", "-T", Node::A.service(), "octo"]);
    cmd.args([
        "mesh",
        "peer",
        "list",
        "--filter-trust",
        UNTRUSTED,
        "--json",
    ]);
    let matched = cmd.output().expect("spawn filtered list");
    assert!(matched.status.success());
    let matched = Envelope::parse(&String::from_utf8_lossy(&matched.stdout));
    assert_eq!(matched.filtered_count(), 1);
    assert_eq!(matched.total_count(), 1);

    let other = "urn:octo:trust-level:00000000-0000-0000-0000-000000000001";
    let mut cmd = stack.base(&["exec", "-T", Node::A.service(), "octo"]);
    cmd.args(["mesh", "peer", "list", "--filter-trust", other, "--json"]);
    let unmatched = cmd.output().expect("spawn filtered list");
    assert!(unmatched.status.success());
    let unmatched = Envelope::parse(&String::from_utf8_lossy(&unmatched.stdout));
    assert_eq!(unmatched.filtered_count(), 0, "no peer has been promoted");
    assert_eq!(
        unmatched.total_count(),
        1,
        "filtering must not mutate the table"
    );
}

/// The operator guide's disaster-recovery procedure, run verbatim
/// inside a node container: snapshot the home, wipe it, restore it, and
/// check the peer table is back.
///
/// This exists because the guide's procedure had the property that
/// running it destroyed the artifact it was restoring from. The backup
/// was written under `$OCTO_HOME/backup/`, and the restore step began
/// with `rm -rf "$OCTO_HOME"`, so the extract that followed failed on a
/// file the wipe had already deleted. A test that only checks the peer
/// table is populated would have passed; this one performs the wipe.
///
/// The cross-node part is what makes it worth a container: the restore
/// has to produce a peer table whose recorded endpoint is *dialable
/// again*, which means the restored address is resolved by the compose
/// bridge and not merely present in a file.
#[test]
#[ignore = "requires a running docker engine and compose v2"]
fn l4_wipe_and_restore_recovers_a_dialable_peer_table() {
    ensure_docker_available();
    let stack = Compose::start("restore", "compose-2node.yaml");

    stack.sh_detached(Node::B, &listener_script());
    assert!(
        wait_for_reachability(
            &stack,
            Node::B,
            "127.0.0.1",
            &PROBE_PORT.to_string(),
            Duration::from_secs(30)
        ),
        "the listener never came up on {}",
        Node::B.service()
    );

    let peer = canonical_did(66);
    assert!(stack
        .peer_add(Node::A, &peer, &Node::B.dialable())
        .status
        .success());
    assert_eq!(
        endpoint_of(&stack.peer_list(Node::A), &peer),
        Node::B.dialable()
    );

    // §22 Setup. The backup directory is outside the home, and the
    // archive is built with relative member names, so it survives the
    // wipe and can land in whatever path the home now occupies.
    let backup = stack.sh(
        Node::A,
        r#"
        set -eu
        export OCTO_BACKUP_DIR=/octo/backup-outside
        rm -rf "$OCTO_BACKUP_DIR"
        mkdir -p "$OCTO_BACKUP_DIR"
        tar -czf "$OCTO_BACKUP_DIR/home.tar.gz" \
            --exclude='./backup' --exclude='./data/*.stoolap' \
            -C "$OCTO_HOME" .
        "#,
    );
    assert!(
        backup.status.success(),
        "the snapshot step failed: {}",
        String::from_utf8_lossy(&backup.stderr)
    );

    // §22 Operate: wipe, recreate, extract.
    //
    // The wipe empties the home rather than removing it. `$OCTO_HOME` is
    // a volume mount point in any containerized deployment, and
    // `rm -rf` on a mount point fails with `Device or resource busy` —
    // so the guide's `rm -rf "$OCTO_HOME"` aborts the whole procedure
    // under `set -e` at exactly the moment the operator needs it to
    // work. Emptying the directory achieves the same thing and works in
    // both cases.
    let restore = stack.sh(
        Node::A,
        r#"
        set -eu
        export OCTO_BACKUP_DIR=/octo/backup-outside
        find "$OCTO_HOME" -mindepth 1 -delete
        chmod 0700 "$OCTO_HOME"
        tar -xzf "$OCTO_BACKUP_DIR/home.tar.gz" -C "$OCTO_HOME"
        "#,
    );
    assert!(
        restore.status.success(),
        "the restore step failed: {}",
        String::from_utf8_lossy(&restore.stderr)
    );

    // The peer table is back, with the endpoint the operator recorded.
    let listed = stack.peer_list(Node::A);
    assert_eq!(
        listed.payload["total_count"], 1,
        "the restored home must hold the peer that was there before the wipe"
    );
    let recorded = endpoint_of(&listed, &peer);
    assert_eq!(recorded, Node::B.dialable());

    // And the restored record still resolves to a live peer, which is
    // the part a file-presence assertion would miss.
    let (host, port) = split_tcp(&recorded);
    let probe = stack.sh(Node::A, &format!("nc -z -w 3 {host} {port}"));
    assert!(
        probe.status.success(),
        "the restored endpoint {} must be dialable again after recovery; probe stderr: {}",
        recorded,
        String::from_utf8_lossy(&probe.stderr)
    );

    // The backup directory is outside the home, so the wipe could not
    // have taken the archive with it. This is the assertion that fails
    // against the pre-fix procedure.
    let survivor = stack.sh(
        Node::A,
        "test -f /octo/backup-outside/home.tar.gz && echo SURVIVED",
    );
    assert!(
        String::from_utf8_lossy(&survivor.stdout).contains("SURVIVED"),
        "the archive must live outside $OCTO_HOME, or the wipe destroys the only copy"
    );
}

/// The guide says a mesh peer table is rebuildable from network gossip.
/// There is no gossip refresh on the CLI, so the only way to move a
/// peer table between nodes is to snapshot it and restore it — which
/// makes the cross-node case the one operators actually need.
///
/// It also has to preserve isolation. A restored table on node B must
/// not become visible to node A, or "restoring a backup" silently
/// merges two nodes into one.
#[test]
#[ignore = "requires a running docker engine and compose v2"]
fn l4_restoring_one_nodes_peer_table_onto_another_does_not_break_isolation() {
    ensure_docker_available();
    let stack = Compose::start("xnode", "compose-2node.yaml");

    let peer = canonical_did(67);
    assert!(stack
        .peer_add(Node::A, &peer, &Node::B.dialable())
        .status
        .success());
    assert_eq!(stack.peer_list(Node::A).payload["total_count"], 1);
    assert_eq!(
        stack.peer_list(Node::B).payload["total_count"],
        0,
        "{} must start with an empty table",
        Node::B.service()
    );

    // Export A's table from inside A, with relative member names so it
    // can be extracted into B's home.
    let dump = stack.sh(
        Node::A,
        r#"
        set -eu
        rm -rf /octo/exports && mkdir -p /octo/exports
        tar -czf /octo/exports/from-a.tar.gz -C "$OCTO_HOME" ./mesh
        "#,
    );
    assert!(
        dump.status.success(),
        "node A could not export its peer table: {}",
        String::from_utf8_lossy(&dump.stderr)
    );

    // Move A's archive into B. `compose cp` is addressed by service
    // rather than by container name, so this does not depend on compose's
    // `{project}-{service}-{index}` naming — and it is how an operator
    // actually transfers a backup between machines.
    let copied = stack
        .base(&[
            "cp",
            "node-a:/octo/exports/from-a.tar.gz",
            "/tmp/from-a.tar.gz",
        ])
        .output()
        .expect("docker compose cp out of node-a");
    assert!(
        copied.status.success(),
        "node A's table could not be copied off: {}",
        String::from_utf8_lossy(&copied.stderr)
    );
    let copied = stack
        .base(&[
            "cp",
            "/tmp/from-a.tar.gz",
            "node-b:/octo/incoming-from-a.tar.gz",
        ])
        .output()
        .expect("docker compose cp into node-b");
    assert!(
        copied.status.success(),
        "the archive could not be moved from {} to {}: {}",
        Node::A.service(),
        Node::B.service(),
        String::from_utf8_lossy(&copied.stderr)
    );

    let applied = stack.sh(
        Node::B,
        r#"
        set -eu
        tar -xzf /octo/incoming-from-a.tar.gz -C "$OCTO_HOME"
        "#,
    );
    assert!(
        applied.status.success(),
        "node B could not restore node A's table: {}",
        String::from_utf8_lossy(&applied.stderr)
    );

    // B now holds A's peer, and the endpoint it names is B's own dialable
    // address, resolved on the shared bridge.
    let listed = stack.peer_list(Node::B);
    assert_eq!(
        listed.payload["total_count"], 1,
        "the restored table must be readable on the node it was restored onto"
    );
    assert_eq!(endpoint_of(&listed, &peer), Node::B.dialable());

    // And A is unchanged: a restore is a local write, not a gossip
    // event. If this ever changes, the two nodes have silently merged.
    assert_eq!(
        stack.peer_list(Node::A).payload["total_count"],
        1,
        "node A must still hold exactly its own peer"
    );
}
