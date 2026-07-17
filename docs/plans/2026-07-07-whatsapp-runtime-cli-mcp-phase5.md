# WhatsApp Runtime CLI + MCP — Phase 5 (Hardening)

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task.

**Goal:** Phase 5 of the WhatsApp Runtime CLI + MCP design — production hardening per `docs/plans/2026-07-04-whatsapp-runtime-cli-mcp-design.md` §Phase 5 + close Phase 4 carryover gaps. Daemon reaches operator-grade deployability (token rotation, observability, sandbox enforcement, rules persistence, packaging).

**Architecture (3 sub-phases):**

**5a — Security + observability + rules durability:**
1. **Token rotation** — `security.rotate_token` RPC + grace period + revocation list; `token.revoke_all` incident path; bound to `(token_id, PID, starttime)`; `subtle::ConstantTimeEq`; 256-bit min entropy; grace persisted to `$DATADIR/tokens/grace.json` (mode 0600, fsync-before-ack).
2. **Bearer auth** — header `Authorization: Bearer …`; per-IP failed-auth counter + 1-Hz backoff cap; replay-nonce table (5-min TTL).
3. **Prometheus metrics** — `[observability.metrics] prometheus_listen` (default `null`); 14 named counters/gauges/histograms per design §Observability; high-cardinality labels HMAC-hashed to 8 hex chars.
4. **Health surfaces** — HTTP `/health` liveness + HTTP `/ready` readiness on `[observability.health] http_listen` (default `127.0.0.1:7778`); 503 when `!connected || !session_valid`.
5. **OTLP tracing** — `[observability.tracing] otlp_endpoint` optional; spans wrap RPC handling, rule matching, trigger execution.
6. **`rules_persister` task** — single owner of rules.toml disk writes; debounce 100ms; atomic temp-file + rename; WAL at `~/.local/share/octo/whatsapp/rules.wal`; flush on shutdown.
7. **Phase 4 carryover closure** — CLI + MCP wrappers for the 17 new Phase 4 RPC methods; Landlock + seccomp concrete application in `shell_linux.rs`; production wiring of trigger dispatcher into `EventsRouter`.

**5b — Packaging:**
8. **Dockerfile** — multi-stage; `USER 1000`; `VOLUME [/var/lib/octo/whatsapp, /var/log/octo/whatsapp]`; `HEALTHCHECK` via unix socket `/ready` (`--interval=30s --timeout=5s --start-period=60s --retries=3`).
9. **systemd unit** — `Type=simple`, `Restart=on-failure`, `DynamicUser=yes`, `StateDirectory=octo/whatsapp`, `ProtectSystem=strict`, `NoNewPrivileges=true`, `ProtectHome=read-only`, `MemoryDenyWriteExecute=true`.
10. **Man pages + completions** — `cargo run -- gen-manpages` and `gen-completions` subcommands; emit `.1` man pages and bash/zsh/fish completion files into `packaging/man/` + `packaging/completions/`.
11. **Debian package** — `cargo-deb` config in `packaging/deb/`; metadata `name = "octo-whatsapp"`, depends on `libc6`, conflict-free install.

**5c — Chaos tests:**
12. **toxiproxy network partition** — partition between daemon and adapter for 30s; assert reconnect + `Reconnecting` state + auto-recovery.
13. **slow disk** — `LD_PRELOAD` shim or temp fs with 1MiB/s throttle; assert `StorageDegraded` + refusal + recovery on `daemon.recover_storage`.
14. **OOM cgroup** — set cgroup memory limit to 100 MiB; allocate past limit; assert daemon recovers + emits `daemon.oom_recovered` event.
15. **clock skew** — `tokio::time::pause()` + advance/rewind; assert monotonic timestamps remain monotonic, audit seq_no monotonic, expiry times correct.

**Tech Stack additions:** `prometheus = "0.13"`, `opentelemetry = "0.27"`, `opentelemetry-otlp = "0.27"`, `tracing-opentelemetry = "0.28"`, `axum = "0.7"` (HTTP health server), `cargo-deb` (build-time).

**Pre-requisites:**
- Branch: `feat/whatsapp-runtime-cli-mcp` (continue stacking on Phase 1-4)
- Worktree: `.worktrees/whatsapp-runtime-cli-mcp`
- 452 lib tests + 41 integration tests passing (Phase 4 baseline)
- All Phase 4 coverage gates cleared (87.35% / 85.80%)

**Acceptance gates (cumulative across 5a/5b/5c):**
- 60+ new tasks complete
- All existing tests still pass (no regressions)
- `cargo llvm-cov --no-default-features --features test-helpers -p octo-whatsapp --summary-only`:
  - **overall ≥ 85% lines / ≥ 75% branches** (Phase 4 baseline preserved)
- `cargo clippy --all-targets --all-features -- -D warnings` clean
- `cargo fmt -- --check` clean
- `daemon.api.version = "1.0.0+phase5"`
- `cargo build --release --target x86_64-unknown-linux-gnu` produces static binary
- `docker build` produces working container with `/ready` HEALTHCHECK passing
- `cargo deb` produces `.deb` package that installs cleanly on Debian 12
- No push, no PR (per user decision 2026-07-05)

---

## Architectural decisions

### A1. Token rotation grace persisted, not in-memory

Per design §Open Question #5: "Grace state persisted to `$DATADIR/tokens/grace.json` (mode 0600, fsync-before-ack) with absolute expiry; systemd restart does not truncate grace." Implementation: `TokenStore` owns a `parking_lot::Mutex<GraceState>` plus a `tempfile::NamedTempFile` write → `persist_noclobber` → `fsync` before returning success. On startup, load `grace.json`; entries past absolute expiry are silently dropped.

### A2. Prometheus on a SEPARATE listener from health, OR same with bearer

Per design §Observability: "/metrics requires bearer token when TCP is enabled, OR is on a separate `[observability.health] http_listen` (default `127.0.0.1:7778`)." Implementation: ONE HTTP listener (`axum` on `[observability.health] http_listen`), three routes: `/health`, `/ready`, `/metrics`. `/metrics` ALWAYS requires bearer (regardless of TCP-vs-unix), since the daemon binds loopback by default but operators may expose via reverse proxy. No co-hosting with unix socket.

### A3. Landlock + seccomp behind explicit features, no auto-enable

Optional features `landlock` and `seccomp` in Cargo.toml. The default `cargo build` does NOT enable them. Operators enable per-deployment. `shell_linux.rs` runtime-detects feature presence at startup; if enabled, applies the sandbox; if disabled, no-op (process_group + timeout + kill still apply as base defenses).

### A4. rules_persister is a SINGLE tokio task with mpsc

Per design §Process Model: `rules_persister` is a single tokio task; receives mutate requests via bounded mpsc (cap=256, drop-newest + counter). `RuleStore` writes go through this mpsc — `create/update/delete/replace_all` push a `PersistOp` and either await a oneshot ack (sync callers) or fire-and-forget (tests). Debounce window: 100ms after last op, then atomic temp-file + rename.

### A5. CLI/MCP wrappers for Phase 4 methods are STRICT — no new methods added

The 17 Phase 4 methods already exist in RPC layer. CLI/MCP must mirror the EXISTING 17 — no surface addition. Existing CLI uses `clap` derives; reuse the pattern. MCP uses schemars-derived JSON Schema per tool.

### A6. Chaos tests are integration-only, gated on env

`cargo test --features chaos-tests` runs them. Default `cargo test` skips. Each chaos test sets up its own scenario (spawn toxiproxy subprocess, fork+exec with LD_PRELOAD, etc.). Tests must clean up after themselves — no leaked processes or cgroup entries.

### A7. Health surfaces bound to loopback ONLY

`[observability.health] http_listen` default is `127.0.0.1:7778`. Reject startup if config specifies non-loopback bind. Operators wanting external access must proxy + auth at the proxy layer (out of scope for this crate).

### A8. Packaging is non-blocking — daemon builds without Dockerfile/systemd present

Dockerfile + systemd + debian sit in `packaging/` outside `src/`. CI for the daemon doesn't build the package (that's release-only). The plan's acceptance gate is "build works on a release-tagged commit", not "every CI run produces a .deb".

---

## Part A — Token rotation + grace period (Tasks 1-9)

### Task 1: `tokens.rs` module

Create `crates/octo-whatsapp/src/security/tokens.rs`:

```rust
//! Bearer-token store with rotation, grace period, and revocation list.
//! Phase 5 §Security.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use subtle::ConstantTimeEq;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenDescriptor {
    pub token_id: String,       // first 8 hex of HMAC(token, "octo-id-salt")
    pub secret: String,         // 256-bit hex; zeroed after copy
    pub label: String,          // human-readable name
    pub created_at_unix_ms: i64,
    pub expires_at_unix_ms: Option<i64>,
    pub revoked: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraceEntry {
    pub old_token_id: String,
    pub new_token_id: String,
    pub expires_at_unix_ms: i64,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraceFile {
    pub entries: Vec<GraceEntry>,
}

#[derive(Debug, thiserror::Error)]
pub enum TokenError {
    #[error("invalid token: {0}")]
    Invalid(String),
    #[error("unknown token_id: {0}")]
    UnknownToken(String),
    #[error("token revoked: {0}")]
    Revoked(String),
    #[error("token expired")]
    Expired,
    #[error("token entropy too low: need >= 256 bits, got {got_bits}")]
    WeakToken { got_bits: u32 },
    #[error("grace period invalid: {0}")]
    GraceInvalid(String),
    #[error("storage error: {0}")]
    Storage(String),
}

pub type TokenResult<T> = Result<T, TokenError>;

#[derive(Debug)]
pub struct TokenStore {
    tokens: Mutex<HashMap<String, TokenDescriptor>>,  // by token_id
    secrets: Mutex<HashMap<String, String>>,           // by token_id → secret (for verification)
    grace: Mutex<GraceFile>,
    grace_path: Option<std::path::PathBuf>,
    default_grace_ms: i64,
}

impl TokenStore {
    pub fn new(grace_path: Option<std::path::PathBuf>, default_grace_ms: i64) -> Self { ... }
    pub fn load_from_env(&self, env_var: &str) -> TokenResult<TokenDescriptor>;
    pub fn verify(&self, presented: &str) -> TokenResult<&TokenDescriptor>;
    pub fn rotate(&self, old_token_id: &str, new_secret_hex: &str, grace_ms: i64, label: &str) -> TokenResult<GraceEntry>;
    pub fn revoke(&self, token_id: &str) -> TokenResult<()>;
    pub fn revoke_all(&self) -> usize;
    pub fn list_active(&self) -> Vec<TokenDescriptor>;
    pub fn list_grace(&self) -> Vec<GraceEntry>;
    pub fn persist_grace(&self) -> TokenResult<()>;
    pub fn sweep_expired(&self, now_unix_ms: i64);
}
```

Comparison uses `subtle::ConstantTimeEq`. Token entropy check: hex-decoded length * 4 ≥ 256 bits.

### Task 2: TokenStore unit tests

12+ tests: load_from_env happy + missing + weak; verify happy + wrong + revoked + expired; rotate grace + grace-after-revoke; revoke_all clears grace; persistence round-trip via tempfile::tempdir; constant-time comparison (test that verify uses ConstantTimeEq — inspect via debug_assert or doc test).

### Task 3: `security.rotate_token` + `token.revoke_all` + `token.list` handlers

Create `crates/octo-whatsapp/src/ipc/handlers/security_tokens.rs`:

```rust
pub struct SecurityRotateToken;
#[async_trait]
impl RpcHandler for SecurityRotateToken {
    fn method(&self) -> &'static str { "security.rotate_token" }
    async fn call(&self, h: DaemonHandle, p: Value) -> RpcResult<Value> {
        // params: { "old_token_id": "...", "new_secret_hex": "...", "grace_ms": 60000, "label": "rotated-2026-07-07" }
        let new_secret = p["new_secret_hex"].as_str().ok_or(...)?;
        let grace_ms = p["grace_ms"].as_i64().unwrap_or(60_000);
        let entry = h.tokens().rotate(old_id, new_secret, grace_ms.clamp(1000, 300_000), label)?;
        h.tokens().persist_grace()?;
        Ok(json!({ "old_token_id": entry.old_token_id, "new_token_id": entry.new_token_id, "grace_expires_at_unix_ms": entry.expires_at_unix_ms }))
    }
}

pub struct SecurityRevokeAllTokens;
pub struct SecurityListTokens;
```

### Task 4: Wire TokenStore into DaemonHandle

`DaemonInner` gets `pub tokens: Arc<TokenStore>`. Init in `Daemon::handle()` from `[security] bearer_token_env` + `[security] grace_path` (default `~/.local/share/octo/whatsapp/tokens/grace.json`) + `[security] grace_period_ms` (default 60000, clamp 1000..300000).

### Task 5: Bearer auth middleware in IPC server

Edit `crates/octo-whatsapp/src/ipc/server.rs`:

```rust
fn authenticate(presented: Option<&str>, tokens: &TokenStore) -> Result<TokenDescriptor, TokenError> {
    let p = presented.ok_or(TokenError::Invalid("missing Authorization header".into()))?;
    let bearer = p.strip_prefix("Bearer ").ok_or(TokenError::Invalid("not Bearer scheme".into()))?;
    let desc = tokens.verify(bearer)?;
    Ok(desc.clone())
}
```

Wire into the `serve_unix` and (future) `serve_tcp` loops. On failure: increment per-IP counter (loopback IP `127.0.0.1`/`::1`), apply 1-Hz backoff if > 5 failed in last 60s, return JSON-RPC error `-32050` with `data.kind = "unauthorized"`.

### Task 6: CLI + MCP wrappers for token methods

Edit `crates/octo-whatsapp/src/cli.rs` — add `TokenCmd` enum with subcommands `rotate`, `revoke-all`, `list`. Mirror to `crates/octo-whatsapp/src/mcp_server.rs` — three new tool descriptors.

### Task 7: Per-IP failed-auth counter + backoff

```rust
struct AuthBackoff {
    by_ip: Mutex<HashMap<IpAddr, VecDeque<i64>>>,  // timestamps of recent failures
    cap_per_sec: AtomicU64,
}
```

1-Hz cap = 1.0 failures/sec sustained; if exceeded, return -32050 immediately without invoking verify. Replay-nonce table (5-min TTL) for TCP path — out of scope for hermetic tests, but struct defined.

### Task 8: Grace file persistence

Use `tempfile::NamedTempFile::new_in(parent_dir)` + `write_all` + `sync_all()` + `persist_noclobber(target_path)`. On startup, `load_grace()` reads + parses + filters expired (entries past `expires_at_unix_ms`).

### Task 9: Commit Part A

Commit: `feat(security): token rotation RPC + grace period + revocation list + bearer auth middleware (Phase 5 Part A)`.

---

## Part B — Prometheus metrics + health surfaces + OTLP (Tasks 10-22)

### Task 10: `observability/metrics.rs` module

Create `crates/octo-whatsapp/src/observability/metrics.rs`:

```rust
//! Prometheus metrics registry.
//! Phase 5 §Observability.

use prometheus::{Counter, CounterVec, Gauge, GaugeVec, HistogramVec, HistogramOpts, Opts, Registry, TextEncoder, Encoder};
use std::sync::Arc;
use parking_lot::Mutex;

pub struct Metrics {
    pub registry: Registry,
    pub daemon_uptime_seconds: Gauge,
    pub bot_state: GaugeVec,
    pub connected: Gauge,
    pub inbound_events_total: CounterVec,
    pub outbound_messages_total: CounterVec,
    pub rule_matches_total: CounterVec,
    pub trigger_runs_total: CounterVec,
    pub audit_rows_total: Counter,
    pub stoolap_lock_wait_seconds: HistogramVec,
    pub stoolap_lock_held_seconds: HistogramVec,
    pub rate_limit_dropped_total: CounterVec,
    pub rpc_latency_seconds: HistogramVec,
    pub auth_failed_total: CounterVec,
}

impl Metrics {
    pub fn new() -> Result<Arc<Self>, prometheus::Error> { ... }
    pub fn render(&self) -> Result<String, prometheus::Error> {
        let mut buf = Vec::new();
        let encoder = TextEncoder::new();
        encoder.encode(&self.registry.gather(), &mut buf)?;
        Ok(String::from_utf8(buf)?)
    }
}
```

### Task 11: HMAC-hash helper for high-cardinality labels

```rust
pub fn hash_label(secret: &[u8], value: &str) -> String {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;
    let mut mac = Hmac::<Sha256>::new_from_slice(secret).unwrap();
    mac.update(value.as_bytes());
    let result = mac.finalize().into_bytes();
    hex::encode(&result[..4])  // 8 hex chars
}
```

Bounded cardinality: same secret used for all labels; rotated only on `metrics.rotate_secret` (admin RPC).

### Task 12: Wire Metrics into DaemonHandle

Add `pub metrics: Arc<Metrics>` field. Increment counters on:
- Inbound event: `inbound_events_total{kind=hash(event_kind)}` += 1
- Outbound RPC: `outbound_messages_total{kind=hash(method),result="ok"|"error"}` += 1
- Rule match: `rule_matches_total{rule_id=hash(rule.id)}` += 1
- Trigger run: `trigger_runs_total{trigger_id=hash(trigger.id),result=...}` += 1
- Auth failure: `auth_failed_total{ip=...}` += 1

### Task 13: `observability/health_server.rs` — axum HTTP server

```rust
pub async fn run_health_server(
    bind: SocketAddr,
    metrics: Arc<Metrics>,
    is_ready: Arc<AtomicBool>,
    is_live: Arc<AtomicBool>,
    bearer: Option<Arc<TokenStore>>,
    cancel: CancellationToken,
) -> std::io::Result<()>;
```

Three routes:
- `GET /health` — 200 if `is_live.load()`, else 503. Liveness = process up + unix socket bound.
- `GET /ready` — 200 if `is_ready.load()`, else 503. Readiness = `connected && session_valid`.
- `GET /metrics` — 200 with Prometheus text format. Always requires bearer (returns 401 if missing/invalid).

`is_live` updated by main task on startup + on SIGHUP; `is_ready` updated by connection state watcher.

### Task 14: Config additions for observability

Edit `crates/octo-whatsapp/src/config.rs`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObservabilityConfig {
    #[serde(default)]
    pub metrics: MetricsConfig,
    #[serde(default)]
    pub health: HealthConfig,
    #[serde(default)]
    pub tracing: TracingConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsConfig {
    pub prometheus_listen: Option<String>,  // default None
    pub label_hash_secret: Option<String>,  // 32-byte hex
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthConfig {
    pub http_listen: Option<String>,  // default "127.0.0.1:7778"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TracingConfig {
    pub otlp_endpoint: Option<String>,  // default None
    pub service_name: Option<String>,   // default "octo-whatsapp"
}
```

Reject startup if `health.http_listen` parses to a non-loopback bind.

### Task 15: OTLP tracing exporter

```rust
#[cfg(feature = "otlp")]
pub fn init_otlp(endpoint: &str, service_name: &str) -> Result<Tracer, opentelemetry::Error> {
    let exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_tonic()
        .with_endpoint(endpoint)
        .build()?;
    let provider = opentelemetry_sdk::trace::TracerProvider::builder()
        .with_batch_exporter(exporter, opentelemetry_sdk::runtime::Tokio)
        .with_resource(Resource::new([KeyValue::new("service.name", service_name.to_string())]))
        .build();
    let tracer = provider.tracer("octo-whatsapp");
    let _ = tracing_opentelemetry::OpenTelemetryLayer::new(tracer);
    Ok(tracer)
}
```

Behind `feature = "otlp"` (default off). `tracing-subscriber` registered with both fmt layer + OpenTelemetry layer when enabled.

### Task 16: RPC latency instrumentation middleware

Wrap each RPC call in a span: `tracing::info_span!("rpc", method = %method, caller_uid = %uid)`. Increment `rpc_latency_seconds{method=hash(method)}` histogram on completion.

### Task 17: Update health.get handler

`crates/octo-whatsapp/src/ipc/handlers/health.rs` — extended to return `daemon.ready` JSON with `connected`, `session_valid`, `bot_state`, `socket_bound`, `storage_state`, `uptime_seconds`. Reads from `Metrics` + `DaemonState` snapshots.

### Task 18: Health server tests

6 hermetic tests: `/health` 200 when live, 503 when not; `/ready` 200 when ready; `/metrics` 401 without bearer, 200 with bearer; bearer rejection logs `auth_failed_total{ip="127.0.0.1"}` += 1; non-loopback bind rejected at config validation.

### Task 19: Metrics render test

Assert `Metrics::render()` produces text containing the 14 metric names. Use `#[test]` hermetic — no server, no network.

### Task 20: OTLP test (gated)

`#[cfg(feature = "otlp")] #[tokio::test] async fn otlp_init_succeeds_with_mock_endpoint()` — start a mock TCP listener, init OTLP, emit one span, assert it arrives. Mock listener on `127.0.0.1:0`.

### Task 21: Version bump

Edit `crates/octo-whatsapp/src/daemon.rs`: `pub fn version() -> &'static str { "1.0.0+phase5" }`. Update `health.get` to return this. Update 6+ integration tests asserting the version string.

### Task 22: Commit Part B

Commit: `feat(observability): Prometheus metrics + HTTP health/ready + OTLP tracing + token auth on /metrics (Phase 5 Part B)`.

---

## Part C — Rules persistence (Tasks 23-31)

### Task 23: `rules_persister` task

Create `crates/octo-whatsapp/src/rules/persister.rs`:

```rust
pub struct RulesPersister {
    tx: mpsc::Sender<PersistOp>,
    cancel: CancellationToken,
}

pub enum PersistOp {
    Upsert(Rule),
    Delete(String),
    ReplaceAll(Vec<Rule>),
    FlushSync(oneshot::Sender<()>),  // for shutdown
}

impl RulesPersister {
    pub fn spawn(storage_path: PathBuf, wal_path: PathBuf, debounce_ms: u64) -> (Self, JoinHandle<()>) { ... }
    pub async fn enqueue(&self, op: PersistOp) -> Result<(), mpsc::error::SendError<PersistOp>>;
}
```

Loop:
1. Receive op (with select on cancel).
2. If `FlushSync`, write immediately + ack + continue.
3. Otherwise, debounce: sleep `debounce_ms` after last op (reset on new op).
4. Coalesce: keep only the latest `Upsert` per rule_id; collapse multiple `Delete`s; `ReplaceAll` cancels everything pending.
5. Atomic write: serialize current ruleset to `tempfile::NamedTempFile::new_in(parent_dir)` → `sync_all()` → `persist_noclobber(target_path)`.
6. WAL append: each swap writes a WAL entry `seq_no | op_json | sha256` with `fsync` before ack.

### Task 24: Persister unit tests

8 tests: upsert coalesces two updates within debounce; delete + upsert-race resolves to latest; replace_all flushes pending; wal fsync on every entry; crash-recovery via WAL replay (test: write WAL manually, start persister, assert rules loaded); shutdown flushes pending via FlushSync.

### Task 25: WAL format + replay

WAL format: `<8-byte LE seq_no><4-byte LE payload_len><payload_json><32-byte sha256>`. Append-only. On startup, replay from seq_no=1; if final entry's sha256 doesn't match, truncate to last valid entry + log warning.

### Task 26: Wire persister into RuleStore

Edit `rules/rule_store.rs`: replace direct mutation with `self.persister.enqueue(...)`. Add `pub fn persister(&self) -> &RulesPersister` accessor. `DaemonInner` gets `pub rules_persister: Arc<RulesPersister>`.

### Task 27: Wire `rules.reload` to disk read

Edit `crates/octo-whatsapp/src/ipc/handlers/rules.rs` — `RulesReload` now:
1. Reads `rules.toml` from `RulesConfig.storage_path`.
2. Parses + validates (schema + ReDoS classifier).
3. Calls `RuleStore::replace_all(rules)`.
4. Returns `{ "loaded_count": N, "previous_count": M, "diff": [...] }`.

`ReplaceAll` flows through persister (Task 26) for disk durability.

### Task 28: `rules.toml` schema

```toml
# rules.toml — durable rule storage
# Phase 5 §Hot mutation safety
version = 1

[[rule]]
id = "echo-text"
version = 1
enabled = true
priority = 100
state = "approved"
created_by = "operator"
created_at = 1751894400000
updated_at = 1751894400000
etag = "..."

[rule.predicate]
kind = "and"
children = [
  { kind = "event_kind", kinds = ["message"] },
  { kind = "peer_glob", pattern = "*@g.us" },
]

[[rule.actions]]
kind = "agent_run"
trigger_id = "echo-bot"
```

### Task 29: SIGHUP triggers `rules.reload`

Wire `signal_handler` task (currently may be stubbed) — on SIGHUP, call `rules.reload` RPC + `triggers.reload` RPC + log reconfiguration events.

### Task 30: Integration test — rules persist across restart

`tests/it_rules_persistence.rs`: start daemon A, create rule R1, kill A; start daemon B with same storage_path, assert R1 present with same etag + version.

### Task 31: Commit Part C

Commit: `feat(rules): rules_persister task with debounced atomic writes + WAL + disk reload (Phase 5 Part C)`.

---

## Part D — Landlock + seccomp concrete application (Tasks 32-38)

### Task 32: Landlock allowlist helper

Edit `crates/octo-whatsapp/src/actions/runner/shell_linux.rs`:

```rust
#[cfg(all(target_os = "linux", feature = "landlock"))]
fn apply_landlock() -> std::io::Result<()> {
    use landlock::{Ruleset, RulesetAttr, Access, RulesetStatus};
    let abi = landlock::ABI::V1;
    let mut ruleset = Ruleset::default()
        .handle_access(Access::FS)?
        .create()?
        .add_rules(landlock::path::RODirs::from_paths([
            "/usr", "/lib", "/lib64", "/bin", "/sbin",
        ]))?
        .add_rules(landlock::path::ROFiles::from_paths([
            "/etc/ld.so.cache", "/etc/resolv.conf", "/etc/alternatives",
        ]))?
        .restrict_self()?;
    Ok(())
}

#[cfg(not(all(target_os = "linux", feature = "landlock")))]
fn apply_landlock() -> std::io::Result<()> { Ok(()) }
```

### Task 33: seccomp filter helper

```rust
#[cfg(all(target_os = "linux", feature = "seccomp"))]
fn apply_seccomp() -> std::io::Result<()> {
    use seccompiler::{SeccompAction, SeccompFilter, SeccompRule, TargetArch};
    let filter: SeccompFilter = SeccompFilter::new(
        vec![/* deny socket, io_uring, userfaultfd, keyctl, bpf, ptrace, kexec, mount */],
        SeccompAction::Allow, SeccompAction::KillProcess,
        TargetArch::x86_64,
    )?;
    seccompiler::apply_filter(&filter)?;
    Ok(())
}

#[cfg(not(all(target_os = "linux", feature = "seccomp")))]
fn apply_seccomp() -> std::io::Result<()> { Ok(()) }
```

Seccomp filter: full allowlist (read/write/open/close/stat/mmap/mprotect/brk/exit/futex/clock_gettime/getrandom); deny socket, io_uring, userfaultfd, keyctl, bpf, ptrace, kexec, mount.

### Task 34: rlimit helper

```rust
fn apply_rlimit() -> std::io::Result<()> {
    use nix::sys::resource::{setrlimit, Resource, RLIM_INFINITY};
    setrlimit(Resource::RLIMIT_AS, &(RLIM_INFINITY, RLIM_INFINITY))?;     // unbounded memory
    setrlimit(Resource::RLIMIT_NOFILE, &(256, 256))?;                       // 256 fds
    setrlimit(Resource::RLIMIT_NPROC, &(256, 256))?;                        // 256 processes
    Ok(())
}
```

### Task 35: Order of application in shell_linux

Apply in this order BEFORE `execveat`:
1. `prctl(PR_SET_NO_NEW_PRIVS)` (already in code)
2. `process_group(0)` (already in code)
3. Landlock (if feature)
4. seccomp (if feature)
5. rlimit (always)
6. `execveat(fd, "", argv, envp, AT_EMPTY_PATH)` (already in code)

Landlock MUST apply before seccomp (seccomp KILL_PROCESS is irreversible).

### Task 36: pidfd child watcher

```rust
#[cfg(target_os = "linux")]
fn watch_child_pidfd(pid: i32, kill_timeout: Duration) -> std::io::Result<()> {
    use nix::sys::epoll::{epoll_create, epoll_ctl, EpollEvent, EpollFlags};
    use nix::sys::signalfd;
    use std::os::unix::io::RawFd;
    let pidfd = nix::unistd::Pid::from_raw(pid);
    let epoll_fd = epoll_create()?;
    epoll_ctl(epoll_fd, EpollFlags::EPOLL_CTL_ADD, pidfd.as_raw(), EpollEvent::empty())?;
    // wait + handle
}
```

Optional; falls back to SIGCHLD poll if pidfd_open unavailable.

### Task 37: Tests for Landlock + seccomp

5 tests (gated on features):
- `apply_landlock_succeeds_with_minimal_paths` — verifies no panic
- `apply_seccomp_succeeds_with_filter` — verifies no panic
- `apply_rlimit_reduces_fd_count` — verifies NOFILE limit enforced
- `shell_linux_with_sandbox_runs_true` — `/bin/true` exits 0
- `shell_linux_with_sandbox_blocks_network` — `/bin/true --version` succeeds but `/usr/bin/curl http://x` fails (killed by seccomp)

### Task 38: Commit Part D

Commit: `feat(runner): Landlock allowlist + seccomp filter + rlimit concrete application (Phase 5 Part D)`.

---

## Part E — CLI + MCP wrappers for Phase 4 methods (Tasks 39-44)

### Task 39: Audit CLI/MCP wrappers

`octo whatsapp audit tail|verify` — already exists as RPC `audit.tail`/`audit.verify`. Add CLI subcommand `AuditCmd { Tail { since_seq, limit }, Verify }`. MCP: add `audit_tail` + `audit_verify` tool descriptors.

### Task 40: Rules CLI/MCP wrappers (10 methods)

CLI: `RulesCmd { List, Get(id), Create(json), Update(id, etag, json), Patch(id, etag, json), Delete(id, etag), Enable(id), Disable(id), Approve(id, token), Reload, Flush, Test(event) }`. MCP: 10 tool descriptors (`rules_create`, `rules_update`, ...).

### Task 41: Triggers CLI/MCP wrappers (4 methods)

CLI: `TriggersCmd { List, Get(id), Create(json), Update(id, etag, json), Delete(id, etag), Run(id, payload) }`. MCP: 4 tool descriptors.

### Task 42: Actions CLI/MCP wrappers

CLI: `ActionsCmd { Escalate { target, reason } }`. MCP: `actions_escalate` tool descriptor.

### Task 43: assert_cmd integration tests

Create `crates/octo-whatsapp/tests/cli_phase5.rs` — `assert_cmd` tests for the 17 new CLI subcommands (each invokes `octo-whatsapp --socket /tmp/... <subcommand> --help` and asserts exit 0 + expected stdout marker).

### Task 44: Commit Part E

Commit: `feat(cli/mcp): CLI subcommands + MCP tool descriptors for 17 Phase 4 RPC methods (Phase 5 Part E)`.

---

## Part F — Production trigger dispatcher wiring (Tasks 45-49)

### Task 45: EventsRouter → RulesStore fan-out

Edit `crates/octo-whatsapp/src/events_router.rs`: for each inbound `InboundEvent`, call `RuleStore::match_event(event, now_ms)` → for each matched `Arc<Rule>`, dispatch its `actions` via `actions::dispatch(...)`. Update `rule_matches_total` metric.

### Task 46: ActionContext with DaemonHandle

```rust
pub struct ActionContext {
    pub rule_id: String,
    pub rule_version: u64,
    pub event: InboundEvent,
    pub caller_uid: String,
    pub daemon: DaemonHandle,  // for webhook → daemon.http_post; agent_run → triggers.run; etc.
}
```

### Task 47: Wire webhook dispatcher

`webhook::dispatch(spec, ctx)` calls `ctx.daemon.http_post(url, headers, body).await` — uses the same reqwest client as the rest of the daemon. Timeout via `tokio::time::timeout(spec.timeout_ms)`. HMAC signing using `ctx.daemon.webhook_secret()`.

### Task 48: Wire agent_run dispatcher

`agent_run::dispatch(spec, ctx)` calls `ctx.daemon.triggers().run(spec.trigger_id, &ctx.event, now_ms).await`. Bubbles errors.

### Task 49: Commit Part F

Commit: `feat(events): wire trigger dispatcher into EventsRouter with full ActionContext (Phase 5 Part F)`.

---

## Part G — Dockerfile + systemd unit + man pages + Debian package (Tasks 50-58)

### Task 50: Multi-stage Dockerfile

`packaging/docker/Dockerfile`:

```dockerfile
FROM rust:1.83-slim AS builder
WORKDIR /build
COPY . .
RUN cargo build --release --bin octo-whatsapp -p octo-whatsapp

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates libssl3 tini && rm -rf /var/lib/apt/lists/*
RUN groupadd -g 1000 octo && useradd -u 1000 -g octo -d /var/lib/octo/whatsapp -s /usr/sbin/nologin octo
COPY --from=builder /build/target/release/octo-whatsapp /usr/local/bin/octo-whatsapp
USER 1000
VOLUME ["/var/lib/octo/whatsapp", "/var/log/octo/whatsapp"]
EXPOSE 7778
HEALTHCHECK --interval=30s --timeout=5s --start-period=60s --retries=3 \
  CMD ["octo-whatsapp", "health", "--probe"] || exit 1
ENTRYPOINT ["/usr/bin/tini", "--", "/usr/local/bin/octo-whatsapp"]
```

### Task 51: Build + test Dockerfile

```bash
docker build -f packaging/docker/Dockerfile -t octo-whatsapp:test .
docker run -d --name octo-test octo-whatsapp:test --help
docker exec octo-test /usr/local/bin/octo-whatsapp version
docker stop octo-test && docker rm octo-test
```

### Task 52: systemd unit

`packaging/systemd/octo-whatsapp.service`:

```ini
[Unit]
Description=Octo WhatsApp Runtime Daemon
Documentation=https://github.com/cipherocto/octo-whatsapp
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
ExecStart=/usr/bin/octo-whatsapp daemon
Restart=on-failure
RestartSec=5s
DynamicUser=yes
StateDirectory=octo/whatsapp
LogsDirectory=octo/whatsapp
ProtectSystem=strict
ProtectHome=read-only
NoNewPrivileges=true
MemoryDenyWriteExecute=true
PrivateTmp=true
ProtectKernelTunables=true
ProtectKernelModules=true
ProtectControlGroups=true
RestrictAddressFamilies=AF_UNIX AF_INET AF_INET6
RestrictNamespaces=true
RestrictRealtime=true
SystemCallArchitectures=native
SystemCallFilter=@system-service ~@privileged ~@resources
UMask=0077
CapabilityBoundingSet=
AmbientCapabilities=

[Install]
WantedBy=multi-user.target
```

### Task 53: systemd-analyze verify

```bash
cp packaging/systemd/octo-whatsapp.service /etc/systemd/system/
systemd-analyze verify /etc/systemd/system/octo-whatsapp.service
```

Should pass with no warnings (or only `[Install]` warnings if not in target dir).

### Task 54: gen-manpages subcommand

Edit `crates/octo-whatsapp/src/cli.rs` — add `GenManpages { output_dir: PathBuf }` and `GenCompletions { shell: String, output_dir: PathBuf }` subcommands. Use `clap_mangen` + `clap_complete`. Each writes `.1` files for bash/zsh/fish.

### Task 55: Man page content tests

`tests/it_manpages.rs`: run `octo-whatsapp gen-manpages --output-dir /tmp/test-manpages`, assert ≥1 `.1` file exists with header `Octo-Whatsapp`, SYNOPSIS section, OPTIONS section, ≥1 EXAMPLE.

### Task 56: cargo-deb config

`packaging/deb/cargo-deb.toml` (or `debian/` directory):

```toml
[package]
name = "octo-whatsapp"
version = "0.1.0"
edition = "2021"

[deb]
name = "octo-whatsapp"
depends = "$auto, libc6"
section = "net"
priority = "optional"
maintainer = "CipherOcto <ops@cipherocto.example>"
description = "WhatsApp Web runtime, CLI, and MCP server (private AI assistant substrate)"
extended-description = """\
Octo-Whatsapp is a private-by-default runtime for WhatsApp Web sessions,
exposing CLI and MCP-server surfaces for agent-driven use."""
assets = [
  ["target/release/octo-whatsapp", "usr/bin/", "755"],
  ["packaging/systemd/octo-whatsapp.service", "lib/systemd/system/", "644"],
]
```

### Task 57: Build + inspect Debian package

```bash
cargo install cargo-deb --locked
cargo deb --no-build -p octo-whatsapp
dpkg-deb -I target/debian/octo-whatsapp_*.deb
dpkg-deb -c target/debian/octo-whatsapp_*.deb
```

Assert: depends OK, files at `/usr/bin/octo-whatsapp` mode 755 + `/lib/systemd/system/octo-whatsapp.service` mode 644.

### Task 58: Commit Part G

Commit: `feat(packaging): Dockerfile + systemd unit + man pages + Debian package (Phase 5 Part G)`.

---

## Part H — Chaos tests (Tasks 59-66)

### Task 59: Chaos test feature gate

Add `[features] chaos-tests = []` to `crates/octo-whatsapp/Cargo.toml`. All chaos tests gated `#[cfg(feature = "chaos-tests")]`.

### Task 60: Toxiproxy network partition

```rust
#[cfg(feature = "chaos-tests")]
#[tokio::test]
async fn chaos_toxiproxy_partition_recovers() {
    // Start mock "adapter" TCP listener on 127.0.0.1:0
    // Spawn daemon pointed at mock
    // Connect to toxiproxy via cargo (skip if not installed)
    // Add latency 30s via toxiproxy API
    // Trigger reconnect; assert daemon enters Reconnecting
    // Restore; assert reconnect succeeds within 60s
}
```

Skip-on-missing: if `toxiproxy-cli` not in PATH, `eprintln!("SKIP: toxiproxy not available"); return;`.

### Task 61: Slow disk simulation

```rust
#[cfg(feature = "chaos-tests")]
#[tokio::test]
async fn chaos_slow_disk_enters_storage_degraded() {
    // Create temp dir; mount tmpfs with size limit OR use FUSE throttle
    // Simulate: rules_persister takes 10s per write
    // Assert daemon emits daemon.storage_degraded
    // Run daemon.recover_storage; assert recovery
}
```

### Task 62: OOM cgroup

```rust
#[cfg(feature = "chaos-tests")]
#[tokio::test]
async fn chaos_oom_cgroup_recovers() {
    // Skip if /sys/fs/cgroup/cgroup.controllers missing (no cgroup v2)
    // Create child cgroup with memory.max = 100MiB
    // Spawn daemon in that cgroup
    // Allocate 200 MiB; assert cgroup OOM kill
    // Restart daemon; assert clean recovery + daemon.oom_recovered event
}
```

### Task 63: Clock skew forward

```rust
#[cfg(feature = "chaos-tests")]
#[tokio::test]
async fn chaos_clock_skew_forward_keeps_monotonic() {
    // tokio::time::pause()
    // Advance 1 hour; assert audit seq_no monotonic + ts_mono_ns monotonic
    // Audit row ts_unix_ms reflects jump; ts_mono_ns does NOT
}
```

### Task 64: Clock skew backward

```rust
#[cfg(feature = "chaos-tests")]
#[tokio::test]
async fn chaos_clock_skew_backward_no_double_fire() {
    // tokio::time::pause(); rewind 5 minutes
    // Fire rule with cooldown 60s
    // Rewind; assert cooldown gate still respects monotonic anchor (not wall clock)
}
```

### Task 65: File descriptor exhaustion

```rust
#[cfg(feature = "chaos-tests")]
#[tokio::test]
async fn chaos_fd_exhaustion_emits_metric() {
    // Open 1024 fds in test process; assert subsequent open returns EMFILE
    // Daemon should emit metric + log warning; not panic
}
```

### Task 66: Commit Part H

Commit: `test(chaos): per-feature chaos tests (toxiproxy, slow disk, OOM, clock skew, fd exhaustion) (Phase 5 Part H)`.

---

## Part I — Final coverage + handoff (Tasks 67-72)

### Task 67: Coverage measurement

Run `cargo llvm-cov --no-default-features --features test-helpers -p octo-whatsapp --summary-only`. Target: overall ≥ 85% / ≥ 75%. Add tests per-module if any falls below Phase 4 baseline.

### Task 68: Clippy + fmt

```bash
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
```

Both must pass with zero diff.

### Task 69: Build verification

```bash
cargo build --release --target x86_64-unknown-linux-gnu -p octo-whatsapp
docker build -f packaging/docker/Dockerfile -t octo-whatsapp:phase5 .
cargo deb --no-build -p octo-whatsapp
```

All three produce artifacts without error.

### Task 70: Handoff memory

Create `memory/whatsapp-phase5-handoff.md` — Phase 5 status, 10+ commit log, test count delta (Phase 4 452 → Phase 5 ~600+), coverage results, RPC surface delta (77 → 80+, adding `security.rotate_token` + `security.revoke_all` + `security.list_tokens`), MCP surface delta, observability surface delta, packaging artifacts (Docker image, .deb, systemd unit), architectural decisions A1-A8, Phase 6 prerequisites.

### Task 71: MEMORY.md index update

Update Phase 4 line in MEMORY.md → Phase 5 line. Include: 80+ RPC methods, ~600 lib tests, coverage numbers (≥85/75 overall preserved), daemon.api.version = "1.0.0+phase5", all Phase 5 deliverables shipped (token rotation, observability, rules persistence, Landlock+seccomp, CLI/MCP wrappers, trigger dispatcher production wiring, packaging), Phase 6 deferred.

### Task 72: Final report

```
Phase 5 (Hardening) — COMPLETE

Commits added: <list>
daemon.api.version: 1.0.0+phase5
RPC methods: 80 (77 + 3 security)
MCP tools: 80 (mirror)
Observability: Prometheus /metrics + HTTP /health + HTTP /ready + OTLP tracing
Packaging: Dockerfile + systemd unit + .deb package + man pages + completions
Sandboxing: Landlock allowlist + seccomp filter + rlimit + pidfd (gated)
Token rotation: grace period + revocation list + bearer auth
Rules persistence: rules_persister task + WAL + atomic writes + reload

Coverage:
  - rules.rs: XX.XX% / XX.XX%
  - triggers.rs: XX.XX% / XX.XX%
  - actions/*.rs: XX.XX% / XX.XX%
  - observability/*.rs: XX.XX% / XX.XX%
  - security/tokens.rs: XX.XX% / XX.XX%
  - octo-whatsapp overall: XX.XX% / XX.XX% (target ≥85/75)

clippy + fmt: clean
Branch: feat/whatsapp-runtime-cli-mcp (local-only, no push, no PR)

Phase 6 deferred: multi-account, real agent runner, GraphQL gateway.
```

---

## Critical files

**Modified:**
- `crates/octo-whatsapp/Cargo.toml` (deps: `prometheus`, `opentelemetry`, `axum`, `hmac`, optional `landlock`, `seccompiler`, optional `otlp`)
- `crates/octo-whatsapp/src/lib.rs` (declare new modules)
- `crates/octo-whatsapp/src/daemon.rs` (TokenStore + Metrics + persister + health server fields; version bump)
- `crates/octo-whatsapp/src/config.rs` (ObservabilityConfig + SecurityConfig additions)
- `crates/octo-whatsapp/src/cli.rs` (CLI subcommands for tokens + observability + rules + triggers + audit + actions + gen-manpages + gen-completions)
- `crates/octo-whatsapp/src/mcp_server.rs` (MCP tool descriptors mirror)
- `crates/octo-whatsapp/src/ipc/handlers/mod.rs` (3 new security handlers + audit/Rules/Triggers/Actions handlers mirror)
- `crates/octo-whatsapp/src/ipc/server.rs` (bearer auth middleware + per-IP backoff)
- `crates/octo-whatsapp/src/rules/rule_store.rs` (route mutations through persister)
- `crates/octo-whatsapp/src/events_router.rs` (wire trigger dispatcher)
- `crates/octo-whatsapp/src/actions/runner/shell_linux.rs` (Landlock + seccomp + rlimit)
- `crates/octo-whatsapp/README.md` (Phase 5 status)

**Created:**
- `crates/octo-whatsapp/src/security/tokens.rs`
- `crates/octo-whatsapp/src/observability/{mod,metrics,health_server,otlp}.rs`
- `crates/octo-whatsapp/src/rules/persister.rs`
- `crates/octo-whatsapp/src/ipc/handlers/security_tokens.rs`
- `crates/octo-whatsapp/tests/it_rules_persistence.rs`
- `crates/octo-whatsapp/tests/it_manpages.rs`
- `crates/octo-whatsapp/tests/chaos/{toxiproxy,slow_disk,oom,clock_skew,fd_exhaustion}.rs`
- `packaging/docker/Dockerfile`
- `packaging/systemd/octo-whatsapp.service`
- `packaging/deb/cargo-deb.toml`
- `memory/whatsapp-phase5-handoff.md`
- `docs/plans/2026-07-07-whatsapp-runtime-cli-mcp-phase5.md` (this file)

**Untouched:**
- `crates/octo-network/src/dot/adapters/mod.rs` (no PlatformAdapter change)
- All Phase 1-4 code remains working

---

## Verification

```bash
cargo check --workspace --all-features
cargo test -p octo-whatsapp --features test-helpers
cargo test -p octo-whatsapp --features chaos-tests
cargo test -p octo-adapter-whatsapp --features test-helpers --test inherent_smoke
cargo clippy --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
cargo llvm-cov --no-default-features --features test-helpers -p octo-whatsapp --summary-only
docker build -f packaging/docker/Dockerfile -t octo-whatsapp:test .
cargo deb --no-build -p octo-whatsapp
```

Expected:
- `cargo test -p octo-whatsapp`: 452 (existing) + ~150 (Phase 5 new) + ~10 (chaos) = **~612 tests**
- `cargo llvm-cov --summary-only`: lines ≥ 85.00%, branches ≥ 75.00% (Phase 4 baseline preserved)
- `cargo clippy`: 0 warnings
- `cargo fmt --check`: 0 diff
- `docker build`: produces octo-whatsapp:test image
- `cargo deb`: produces .deb package

---

## YAGNI guardrails

- ❌ No GraphQL gateway (Phase 6+).
- ❌ No multi-account adapter plumbing (Phase 6+).
- ❌ No TLS certificate pinning / rotation (Phase 6+).
- ❌ No Wasm sandbox (Phase 6+).
- ❌ Do NOT add full RFC 8785 implementation (subset suffices).
- ❌ Do NOT add `keyring` integration (env > file path is enough for Phase 5).
- ❌ Do NOT modify `crates/octo-network/src/dot/adapters/mod.rs` (no PlatformAdapter change).
- ❌ Do NOT enable Landlock/seccomp by default — operators opt in via feature.