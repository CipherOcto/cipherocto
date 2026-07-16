# WhatsApp Runtime CLI + MCP — Phase 4 (Rules & Triggers)

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task.

**Goal:** Phase 4 of the WhatsApp Runtime CLI + MCP design — Rules engine + Triggers + Action dispatchers + Audit log + Trigger sandboxing + CLI/MCP/RPC integration. Closes the §Phase 4 commitments in `docs/plans/2026-07-04-whatsapp-runtime-cli-mcp-design.md`.

**Architecture:**
1. **Rules engine** — `Predicate` (EventKind | PeerGlob | SenderGlob | TextRegex | FromJid | GroupOnly) + `Rule` + `Ruleset`. Storage `arc_swap::ArcSwap<Ruleset>`, lock-free reads, atomic swap. CRUD through `DaemonState::mutate_rules(closure)`. ReDoS classifier via simple heuristic (nested quantifiers + backreferences). RFC 8785 canonical etag for optimistic concurrency.
2. **Triggers** — `Trigger` struct + `RunnerSpec` (Shell | Http | Agent) + `TriggersRegistry` (ArcSwap). rate_limit, timeout_ms, retries, last_run. AgentRunnerShell runs commands with full sandbox (Part E); AgentRunnerHttp posts webhooks.
3. **Action dispatchers** — webhook (HMAC-signed, TLS-only, idempotency key, domain allowlist), agent_run (trigger invocation), shell (args-as-argv, env_clear), mcp_notify (per-client fanout), escalate (priority bump). Every action: audit row + rate-limit + timeout + redaction.
4. **Audit log** — per-RPC row in `audit_log` table; SHA-256 hash chain; ring-buffer eviction; external anchor every N rows; `chain.verify` RPC.
5. **Trigger sandbox** (Linux) — Landlock allowlist + seccomp deny list + rlimit + pidfd child watcher + PGID kill. Non-Linux = `NotSupported` error.

**Tech Stack:** Rust 2021 + async-trait + arc-swap + regex + sha2 + tokio. Linux sandbox via `landlock`, `seccompiler`, `nix`. Schemars derives for tool schemas.

**Pre-requisites:**
- Branch: `feat/whatsapp-runtime-cli-mcp` (continue stacking on Phase 1+2+2.5+3)
- Worktree: `.worktrees/whatsapp-runtime-cli-mcp`
- 322/322 lib tests + 41 integration binaries passing
- Phase 3 coverage: 85.57% / 86.67% cleared

**Acceptance gates:**
- 7 task parts complete (A-G)
- All existing tests still pass
- `cargo llvm-cov --no-default-features --features test-helpers -p octo-whatsapp --summary-only`:
  - **rules.rs ≥ 90% lines / ≥ 85% branches**
  - **triggers.rs ≥ 75% / ≥ 65%**
  - **actions/*.rs ≥ 80% / ≥ 70%**
  - **octo-whatsapp overall ≥ 85% / ≥ 75%**
- `cargo clippy --all-targets --all-features -- -D warnings` clean
- `cargo fmt --check` clean
- `daemon.api.version = "1.0.0+phase4"`
- No push, no PR (per user decision 2026-07-05)

---

## Architectural decisions

### A1. ReDoS classifier — simple, not Re2/RegexpSecret

Use a static heuristic on the regex pattern: count nested quantifiers, unbounded `.*`/`.+` adjacent to literals, alternations inside quantifiers. Reject with `-32021 RuleRegexUnsafe` if heuristic trips. Not perfect but matches design's "classifier" wording and is testable. Real ReDoS protection = per-match timeout (10ms) + 4KiB input truncation.

### A2. Canonical JSON for etag — RFC 8785 subset

We don't need full RFC 8785. Use a sorted-keys canonical form: `BTreeMap<String, Value>` → walk and emit `{"k1":v1,"k2":v2}` with stable ordering. Sufficient for etag stability. Hash with SHA-256 → hex.

### A3. Trigger sandbox on non-Linux

`cfg(target_os = "linux")` gates Landlock + seccomp. On macOS/Windows, `AgentRunnerShell` returns `-32601 NotSupported` with `data.reason = "linux-only"`. No fallback to permissive — fail closed.

### A4. Audit hash chain SHA-256, not HMAC

The design says SHA-256 chain. HMAC would require a key. SHA-256 is sufficient for tamper-evidence (an attacker who can modify the table can rewrite the chain). External anchor is the actual integrity primitive.

### A5. arc_swap cost model

Predicate evaluation holds `arc_swap::Guard` only long enough to clone `Vec<Arc<Rule>>` for matched rules. Guard dropped before action dispatch. This prevents old generation from pinning.

### A6. Stub mutation tests

The design says "Mutation-tested separately" for rules.rs. We don't have `mutants` or `cargo-mutants` in workspace. Approximation: write at minimum 30 unit tests per Predicate variant, hit every branch, run with `--cfg mutation` flag that toggles `|| true` to `|| false` in match arms and re-runs tests. Cheap-ish and catches dead code paths.

---

## Part A — Rules engine core (Tasks 1-12)

### Task 1: Create `rules/predicate.rs` skeleton

```rust
//! Predicate tree for rule matching.

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Predicate {
    /// Matches any event (used as a default no-op).
    True,
    /// Matches specific event kind, e.g. "message", "reaction".
    EventKind { kinds: Vec<String> },
    /// Peer JID glob (e.g. "*@g.us" matches any group).
    PeerGlob { pattern: String },
    /// Sender JID glob.
    SenderGlob { pattern: String },
    /// Text regex (ReDoS-classified).
    TextRegex { pattern: String },
    /// Source JID exact match.
    FromJid { jid: String },
    /// Group-only filter.
    GroupOnly { value: bool },
    /// All sub-predicates must match.
    And(Vec<Predicate>),
    /// Any sub-predicate matches.
    Or(Vec<Predicate>),
    /// Sub-predicate must NOT match.
    Not(Box<Predicate>),
}
```

**File:** `crates/octo-whatsapp/src/rules/predicate.rs` (~150 LoC + 30+ tests).

### Task 2: Implement `Predicate::matches(&event, &now) -> bool`

```rust
impl Predicate {
    pub fn matches(&self, ev: &InboundEvent, now_ms: i64) -> bool { /* walk tree */ }
}
```

- `EventKind`: check `event_kind(ev)` in `kinds`.
- `PeerGlob`: linear glob match (design §Security: "Linear-time glob engine"). Wildcards `*` only.
- `SenderGlob`: same.
- `TextRegex`: compile once per match attempt (regex::Regex::new) or cache via `OnceCell`. Per-match timeout 10ms via regex's `DFA` limit — but std regex has no timeout; use `std::thread::spawn` + join with timeout? Simpler: 4 KiB input truncation (per design) limits worst case. ReDoS-unsafe patterns rejected at create-time.
- `FromJid`: exact string match on `event.from()`.
- `GroupOnly`: match `event.is_group()`.
- `And`/`Or`/`Not`: recursive.
- `True`: always.

Add helper `event_kind(&InboundEvent) -> &'static str` (e.g. "message", "reaction", "group_change", "presence", "connection", "receipt", "call", "story", "unknown").

**Test:** 12 tests covering each variant + recursion + truncation.

### Task 3: ReDoS classifier

```rust
pub fn classify_regex(pattern: &str) -> Result<(), ReDoSError> {
    // Heuristic:
    // - reject nested quantifiers: `(a+)+`, `(.*)+`
    // - reject unbounded alternation inside quantifier: `(a|b)+`
    // - reject backreferences
    // - allow simple character classes, literal, single quantifier
}
```

Tests: 8 cases (`a*b`, `(a+)+` rejected, `.*` accepted, `(a|b)+` rejected, `[a-z]+` accepted, etc.).

### Task 4: Canonical etag (RFC 8785 subset)

```rust
pub fn canonical_etag(value: &impl Serialize) -> String {
    let json = serde_json::to_value(value).unwrap();
    let mut buf = Vec::new();
    write_canonical(&mut buf, &json);
    let digest = sha2::Sha256::digest(&buf);
    hex::encode(digest)
}

fn write_canonical(buf: &mut Vec<u8>, v: &Value) {
    match v {
        Value::Null => buf.extend_from_slice(b"null"),
        Value::Bool(b) => buf.extend_from_slice(if *b { b"true" } else { b"false" }),
        Value::Number(n) => buf.extend_from_slice(n.to_string().as_bytes()),
        Value::String(s) => buf.extend_from_slice(format!("\"{}\"", escape(s)).as_bytes()),
        Value::Array(a) => { buf.push(b'['); for (i, x) in a.iter().enumerate() {
            if i > 0 { buf.push(b','); } write_canonical(buf, x);
        } buf.push(b']'); }
        Value::Object(m) => {
            let mut keys: Vec<&String> = m.keys().collect();
            keys.sort();
            buf.push(b'{');
            for (i, k) in keys.iter().enumerate() {
                if i > 0 { buf.push(b','); }
                buf.extend_from_slice(format!("\"{}\":", escape(k)).as_bytes());
                write_canonical(buf, &m[*k]);
            }
            buf.push(b'}');
        }
    }
}
```

Tests: 5 (key order independence, nested objects, arrays, mixed types).

### Task 5: `Rule` struct

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rule {
    pub id: String,                  // slug
    pub version: u64,                // monotonic per id
    pub enabled: bool,
    pub priority: i32,               // higher matches first
    pub predicate: Predicate,
    pub actions: Vec<ActionSpec>,    // stub for Part C
    pub cooldown_ms: u64,
    pub ttl_until: Option<i64>,      // unix ms; auto-expire
    pub created_by: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub etag: String,                // sha256 hex
    pub state: RuleState,            // Draft | Approved | Disabled
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RuleState { Draft, Approved, Disabled }
```

Stub `ActionSpec` enum forward declaration:
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ActionSpec {
    Webhook { url: String, secret_env: Option<String> },
    AgentRun { trigger_id: String },
    Shell { argv: Vec<String>, timeout_ms: u64 },
    McpNotify { template: String },
    Escalate { target: String },
}
```

### Task 6: `Ruleset` with ArcSwap

```rust
pub struct Ruleset {
    pub rules: Vec<Arc<Rule>>,
    pub by_id: HashMap<String, Arc<Rule>>,
    pub version: u64,            // bumped on every mutation
}

pub struct RulesState {
    inner: ArcSwap<Ruleset>,
}
```

Operations:
- `load() -> arc_swap::Guard<Arc<Ruleset>>` for read.
- `store(new: Arc<Ruleset>)` for atomic swap.
- `mutate(closure)` helper that locks, modifies, swaps.

### Task 7: `RuleStore` with CRUD + optimistic concurrency

```rust
pub struct RuleStore {
    state: RulesState,
    last_swap: AtomicU64,            // monotonic generation
    swap_skipped: AtomicU64,         // metric: sweeper overflow
}

impl RuleStore {
    pub fn create(&self, draft: RuleDraft) -> Result<Rule, RuleError>;
    pub fn update(&self, id: &str, etag: &str, patch: RulePatch) -> Result<Rule, RuleError>;
    pub fn delete(&self, id: &str, etag: &str) -> Result<(), RuleError>;
    pub fn enable(&self, id: &str, enabled: bool) -> Result<Rule, RuleError>;
    pub fn approve(&self, id: &str, operator_token: &str) -> Result<Rule, RuleError>;
    pub fn list(&self) -> Vec<Arc<Rule>>;
    pub fn get(&self, id: &str) -> Option<Arc<Rule>>;
    pub fn match_event(&self, ev: &InboundEvent, now_ms: i64) -> Vec<Arc<Rule>>; // priority sort + cooldown filter
}
```

Errors:
```rust
pub enum RuleError {
    NotFound,
    Conflict { current_etag: String, current_version: u64 },
    InvalidPredicate(String),
    UnsafeRegex(String),
    AlreadyApproved,
    NotDraft,
    TtlExpired,
    InvalidId(String),
}
```

Cooldown enforcement: per-rule `last_fire: Mutex<HashMap<String, i64>>` keyed on `rule.id` (one rule fires at most once per `cooldown_ms`).

### Task 8: Wire `RuleStore` into `DaemonInner`

Add field `pub rules: Arc<RuleStore>`. Init in `Daemon::handle()` with empty store. Expose `h.rules()` accessor.

### Task 9: Replace handlers/rules.rs

Add 11 handlers:
- `RulesList` — already there; switch to `h.rules().list()`.
- `RulesGet` — already there; switch to `h.rules().get(id)`.
- `RulesCreate` — `{ id, predicate, actions, priority, cooldown_ms, ttl_until? }` → 200 with rule + etag, or `RuleError` → `-32020`/`-32021`.
- `RulesUpdate` — `{ id, etag, predicate?, actions?, ... }` → optimistic concurrency.
- `RulesPatch` — RFC 6902 JSON Patch (subset: `add`/`remove`/`replace`) for selective edits.
- `RulesDelete` — `{ id, etag }` → 204.
- `RulesEnable` / `RulesDisable` — `{ id }` → flipped.
- `RulesReload` — re-read rules.toml from disk, replace whole `Ruleset`.
- `RulesTest` — `{ event }` → `{ matched: [{rule_id, would_fire}], not_fired_due_to_cooldown: [...] }` without executing actions.
- `RulesFlush` — sync debounced disk writes (stub returns 200; debounce impl is for `rules_persister` task).
- `RulesApprove` — `{ id, operator_token }` → transitions `Draft → Approved`. Requires operator capability (out of scope for unit tests; gate on header in Part F).

### Task 10: Rule draft default + auto-approve policy

Per design §Security "rule_draft auto-approve": `[security] auto_approve_rules = true` → create returns `Approved` directly; else `Draft`.

Add config knob `SecurityConfig { auto_approve_rules: bool }` to `WhatsAppRuntimeConfig` (default `false`).

### Task 11: Rate-limit on `rules.create`/`rules.update`

Per design §Hot mutation safety: 10/min per caller_uid. Stub: per-caller counter map keyed on `(caller_uid, hour_bucket)`. Returns `-32003 RateLimited` when over.

(For hermetic tests, gate behind `caller_uid` header on daemon side. For Phase 4 handler tests, the test caller has fixed uid "test".)

### Task 12: Tests for Part A

- Predicate: 30 unit tests (each variant + And/Or/Not + boundary)
- ReDoS: 8 tests
- Canonical etag: 5 tests
- RuleStore CRUD: 15 tests (create happy path, conflict on update, delete with wrong etag, list ordering by priority, match_event with cooldown, approve flow, reload clears store)
- Handlers: 11 tests (one per new RPC) — total ~25 handler tests with MockAdapter already bound in helper.

Run: `cargo test -p octo-whatsapp --features test-helpers rules` → expect 50+ tests.

Commit: `feat(rules): predicate evaluator + ArcSwap<Ruleset> + CRUD with optimistic concurrency (Phase 4 Part A)`.

---

## Part B — Triggers registry (Tasks 13-19)

### Task 13: `Trigger` struct

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trigger {
    pub id: String,
    pub version: u64,
    pub enabled: bool,
    pub runner: RunnerSpec,
    pub rate_limit: Option<RateLimit>,
    pub timeout_ms: u64,
    pub retries: u32,
    pub last_run: Option<RunRecord>,
    pub history_cap: u32,
    pub created_by: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub etag: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RunnerSpec {
    Shell { argv: Vec<String>, cwd: Option<String>, env_passthrough: Vec<String> },
    Http { url: String, method: String, headers: BTreeMap<String,String>, signing_secret_env: Option<String> },
    Agent { agent_id: String, input_template: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimit {
    pub per_second: u32,
    pub burst: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunRecord {
    pub started_at: i64,
    pub finished_at: i64,
    pub exit_code: i32,
    pub stdout_sha256: String,
    pub stderr_sha256: String,
    pub truncated: bool,
}
```

### Task 14: `TriggersRegistry`

Same ArcSwap pattern as `Ruleset`. `TriggerStore` with CRUD + `run(id, payload) -> RunRecord`. Stub `run` for Part C — returns `NotImplemented` until dispatcher is wired.

### Task 15: Wire into `DaemonInner`

`pub triggers: Arc<TriggerStore>`.

### Task 16: Replace handlers/triggers.rs

- `TriggersList` / `TriggersGet` — switch to live store.
- `TriggersCreate` — `{ id, runner, timeout_ms, ... }`.
- `TriggersRun` — `{ id, payload }` → `{ run_id, started_at }` (async).
- `TriggersUpdate` / `TriggersDelete` — optimistic concurrency.

### Task 17: `triggers.list` rate-limit

Per-trigger rate limit + cooldown (reuse cooldown infra from Part A).

### Task 18: Tests for Part B

- Trigger struct serde: 5 tests
- TriggersRegistry CRUD: 12 tests
- rate_limit enforcement: 4 tests
- Handlers: 6 tests

### Task 19: Commit Part B

Commit: `feat(triggers): registry + RunnerSpec + CRUD + rate-limit (Phase 4 Part B)`.

---

## Part C — Action dispatchers (Tasks 20-27)

### Task 20: `actions/mod.rs` skeleton

```rust
pub mod webhook;
pub mod agent_run;
pub mod shell;
pub mod mcp_notify;
pub mod escalate;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionContext {
    pub rule_id: String,
    pub event: InboundEvent,
    pub caller_uid: String,
}

#[async_trait]
pub trait Action: Send + Sync {
    fn kind(&self) -> &'static str;
    async fn execute(&self, spec: &ActionSpec, ctx: &ActionContext) -> Result<ActionResult, ActionError>;
}
```

### Task 21: webhook dispatcher

- POST to URL.
- Refuse `http://` (TLS only) → `-32054 WebhookNotConfigured` if no signing secret.
- Domain allowlist check (linear glob).
- HMAC-SHA256 signature header `X-Octowhatsapp-Signature: t=<unix>,v1=<hex>`.
- Idempotency key `X-Octo-Idempotency-Key: <UUID>`.
- Timeout via `tokio::time::timeout`.
- Capture status code, response body (truncated 64 KiB).
- Audit row with method, url_host, status.

### Task 22: agent_run dispatcher

- Look up trigger by id.
- Call `trigger_store.run(id, ActionContext)`.
- Bubble up error.

### Task 23: shell dispatcher (Linux only)

- Refuse if not Linux (Part E sandboxing).
- Args-as-argv.
- env_clear() with allowlist (`HOME`, `PATH`, `LANG`, `TZ`, `OCTO_*` opt-in).
- EVENT_TEXT env var if text ≤64 KiB, else stdin.
- Returns process exit + bounded output.

### Task 24: mcp_notify dispatcher

- Iterate MCP client registry.
- Push event to each client's write task via the existing per-client mpsc.
- Rate-limit per client.

### Task 25: escalate dispatcher

- Bumps priority + sends to a named target (e.g., "operator", "oncall").
- Stub: returns a `target_token` UUID, records escalation in audit.

### Task 26: Wire dispatcher into rule matching

`Ruleset::match_event` returns matched rules → `execute_actions(rule, event, ctx)` walks `rule.actions`, calls appropriate dispatcher. Each action gets a timeout (`rule.cooldown_ms` is per-rule; per-action timeout is trigger-defined).

### Task 27: Tests for Part C

Per-dispatcher: 4-6 tests covering happy + rejection + timeout + audit row. Total ~25 tests.

Commit: `feat(actions): webhook/agent_run/shell/mcp_notify/escalate dispatchers (Phase 4 Part C)`.

---

## Part D — Audit log (Tasks 28-35)

### Task 28: Audit table schema

In-memory ring buffer (replaces stoolap for Phase 4 hermetic tests; stoolap integration is Phase 5).

```rust
pub struct AuditEntry {
    pub seq_no: u64,
    pub ts_unix_ms: i64,
    pub ts_mono_ns: u128,
    pub caller_uid: String,
    pub caller_pid: u32,
    pub method: String,
    pub args_canonical_sha256: String,
    pub result_status: String,         // "ok" | "error:<code>"
    pub latency_ms: u64,
    pub prev_audit_hash: String,       // hex
    pub this_hash: String,             // hex
}
```

### Task 29: `AuditLog` ring buffer

```rust
pub struct AuditLog {
    inner: Mutex<VecDeque<AuditEntry>>,
    max_rows: usize,
    seq_no: AtomicU64,
    truncated_total: AtomicU64,
    external_anchor_every: usize,
}

impl AuditLog {
    pub fn record(&self, entry: AuditEntryInput) -> u64; // returns seq_no
    pub fn tail(&self, since_seq: u64, limit: usize) -> Vec<AuditEntry>;
    pub fn verify_chain(&self) -> ChainVerifyResult;
    pub fn truncated_total(&self) -> u64;
    pub fn external_anchor_path(&self) -> Option<PathBuf>;
}
```

### Task 30: Hash chain implementation

```rust
fn compute_hash(prev_hash: &str, entry: &AuditEntryInput) -> String {
    let mut h = Sha256::new();
    h.update(prev_hash.as_bytes());
    h.update(entry.seq_no.to_le_bytes());
    h.update(entry.ts_unix_ms.to_le_bytes());
    h.update(entry.caller_uid.as_bytes());
    h.update(entry.method.as_bytes());
    h.update(entry.args_canonical_sha256.as_bytes());
    h.update(entry.result_status.as_bytes());
    hex::encode(h.finalize())
}
```

External anchor: when `seq_no % external_anchor_every == 0`, append to anchor file. Anchor path from `[security] audit_external_anchor_path` (default `/var/log/audit.octo-whatsapp.log`). Created with `mode 0600`.

### Task 31: `verify_chain` walk

Walks `seq_no = 1..=last`, recomputes hash, asserts `prev_audit_hash` matches. Returns `ChainVerifyResult { ok: bool, broken_at_seq: Option<u64>, verified_count: u64 }`.

### Task 32: RPC integration

- `audit.tail` — `{ since_seq?, limit? }` → `{ entries: [...], truncated_total }`.
- `audit.verify` — → `{ ok, broken_at_seq?, verified_count }`.

### Task 33: Wire into RPC middleware

Every RPC call wraps: record audit entry pre-execution (with `result_status = "pending"`); update entry post-execution with status + latency. This is a small middleware closure in `ipc/server.rs` that wraps `RpcHandler::call`.

### Task 34: Tests for Part D

- Hash chain: 6 tests (empty, single, multi, tamper detection, ring eviction).
- verify_chain: 4 tests (ok, broken_at_seq=N, empty buffer, ring with break in middle).
- Handlers: 4 tests (audit.tail happy, audit.verify happy, audit.verify broken, audit.tail truncated).

### Task 35: Commit Part D

Commit: `feat(audit): ring-buffer audit log with SHA-256 hash chain + verify (Phase 4 Part D)`.

---

## Part E — Trigger runner sandboxing (Linux only) (Tasks 36-43)

### Task 36: Trigger runner module structure

```
crates/octo-whatsapp/src/actions/runner/
├── mod.rs
├── shell.rs              # cross-platform stub
├── shell_linux.rs        # Linux impl
├── shell_other.rs        # NotSupported stub
```

### Task 37: Cross-platform shell stub

```rust
#[cfg(not(target_os = "linux"))]
pub async fn run_shell(_argv: &[String], _timeout_ms: u64) -> Result<RunRecord, ActionError> {
    Err(ActionError::NotSupported { reason: "linux-only".into() })
}
```

### Task 38: Linux shell — `prctl(PR_SET_NO_NEW_PRIVS)` + `execveat`

Use `nix` crate for `prctl`, `execveat`, `openat`, `fstat`. Resolve executable path via `openat(O_NOFOLLOW | O_PATH)`. Verify `S_ISREG && nlink == 1`. Compute sha256 of executable. Record in audit.

### Task 39: Linux shell — `fork()` + `kill(-PGID, SIGKILL)` on timeout

Spawn child with `setsid`. Parent waits with `tokio::time::timeout`. On timeout: `kill(-pgid, SIGKILL)`.

### Task 40: Linux shell — Landlock allowlist (optional feature)

```toml
landlock = ["dep:landlock"]
```

Behind `#[cfg(all(target_os = "linux", feature = "landlock"))]`. Allowlist: `/usr`, `/lib`, `/lib64`, `/bin`, `/sbin`, `/etc/ld.so.cache`, `/etc/alternatives`, `/etc/resolv.conf`. Apply before `execveat`.

### Task 41: Linux shell — seccomp filter (optional feature)

```toml
seccomp = ["dep:seccompiler"]
```

Behind `#[cfg(all(target_os = "linux", feature = "seccomp"))]`. Use `seccompiler` to compile a filter that denies socket/io_uring/userfaultfd/keyctl/bpf/ptrace/kexec/mount and allows read/write/open/close/stat/mmap/mprotect/brk/exit/futex/clock_gettime/getrandom.

### Task 42: pidfd child watcher

Use `pidfd_open` + `poll` (Linux 5.4+) via `nix::unistd::Pid::from_raw` + `nix::sys::epoll`. Watches for SIGCHLD-equivalent.

### Task 43: Tests for Part E

- Cross-platform stub: 2 tests (NotSupported error on non-Linux)
- Linux sandbox (only when running on Linux): 5 tests using `/bin/true`, `/bin/false`, `/bin/echo`, `/bin/sleep`
- pidfd watcher: 2 tests
- Ring buffer for stdout/stderr: 3 tests (truncation at 1 MiB)

Commit: `feat(runner): Linux trigger sandboxing with Landlock+seccomp+rlimit+pidfd (Phase 4 Part E)`.

---

## Part F — CLI + MCP + new RPC integration (Tasks 44-50)

### Task 44: New RPC registry

Wire 17 new RPC methods:
- `rules.create`, `rules.update`, `rules.patch`, `rules.delete`, `rules.enable`, `rules.disable`, `rules.reload`, `rules.test`, `rules.flush`, `rules.approve` (10)
- `triggers.create`, `triggers.run`, `triggers.update`, `triggers.delete` (4)
- `audit.tail`, `audit.verify` (2)
- `actions.escalate` (1)

Update `ipc/handlers/mod.rs` registry. Expected total: 12 (Phase 1) + 25 (Phase 2) + 7 (Phase 3) + 17 (Phase 4) = **61 methods**.

### Task 45: CLI subcommands

```bash
octo whatsapp rules list|get|create|update|patch|delete|enable|disable|reload|test|flush|approve
octo whatsapp triggers list|get|create|update|delete|run
octo whatsapp audit tail|verify
octo whatsapp actions escalate
```

Clap derives. Each subcommand has `--socket` + per-method flags.

### Task 46: MCP tools

EXPECTED_TOOL_COUNT: 46 (Phase 3) + 17 (Phase 4) = **63**.

Add 17 tool descriptors + dispatch arms. Each tool calls the matching RPC over the unix socket.

### Task 47: Version bump

`daemon.api.version = "1.0.0+phase4"`. Update 6+ integration test files asserting `"1.0.0+phase3"` → `"1.0.0+phase4"`.

### Task 48: integration test markers

Replace `phase3` markers with `phase4` in 6+ integration test files (CLI, IPC, MCP, etc.).

### Task 49: README update

Status line: "Phase 4 (Rules & Triggers) — implemented". Add §CLI + §MCP for the new methods.

### Task 50: Commit Part F

Commit: `feat(ipc/cli/mcp): 17 new RPC methods + CLI subcommands + MCP tools for Phase 4`.

---

## Part G — Tests + coverage + handoff (Tasks 51-58)

### Task 51: Mutation-style tests

Add `--cfg mutation` test runner that flips `|| true` to `|| false` in Predicate branches and asserts tests fail. Run baseline + mutation variants. Document in `crates/octo-whatsapp/tests/it_rules_mutation.rs`.

### Task 52: Integration tests

- `tests/it_rules_hot_swap.rs` — create rule, fire event, see match. Update rule, fire same event, see different match. Verify ArcSwap semantics (no torn reads).
- `tests/it_audit_chain_integrity.rs` — record 100 entries, verify_chain ok. Tamper with row 50, verify_chain detects break at seq=51.
- `tests/it_actions_rejection.rs` — webhook without secret refuses; shell on non-Linux refuses; action timeout kills process.

### Task 53: Coverage measurement

Run `cargo llvm-cov --no-default-features --features test-helpers -p octo-whatsapp --summary-only` and grep per-module results.

If rules.rs < 90/85: add more unit tests for Predicate boundary cases.
If triggers.rs < 75/65: add CRUD edge cases.
If actions/*.rs < 80/70: add timeout + rejection tests.

### Task 54: Clippy + fmt

`cargo clippy --all-targets --all-features -- -D warnings` clean.
`cargo fmt -- --check` clean.

### Task 55: Final commit + version tag

Commit: `test(phase4): integration tests + coverage gate verified (Phase 4 Part G)`.

### Task 56: Handoff memory

Create `memory/whatsapp-phase4-handoff.md` — full Phase 4 status, commit log, test count delta, coverage results, RPC/MCP/CLI surface deltas, architectural decisions A1-A6, Phase 5 prerequisites.

### Task 57: MEMORY.md index update

Update the `octo-whatsapp runtime CLI + MCP` line in MEMORY.md with Phase 4 complete + new coverage numbers + new RPC/MCP counts.

### Task 58: Final report to user

```
Phase 4 (Rules & Triggers) — COMPLETE

Commits added: <list>
daemon.api.version: 1.0.0+phase4
RPC methods: 61 (12 + 25 + 7 + 17)
MCP tools: 63 (16 + 30 + 7 + 17 — wait, count MCP per design)
CLI subcommands: 5 new top-level + ~25 subcommands

Coverage:
  - rules.rs: XX.XX% / XX.XX% (target ≥90/85)
  - triggers.rs: XX.XX% / XX.XX% (target ≥75/65)
  - actions/*.rs: XX.XX% / XX.XX% (target ≥80/70)
  - octo-whatsapp overall: XX.XX% / XX.XX% (target ≥85/75)

clippy + fmt: clean
Branch: feat/whatsapp-runtime-cli-mcp (local-only, no push, no PR)

Phase 5 (Hardening) unblocked.
```

---

## Critical files

**Modified:**
- `crates/octo-whatsapp/src/rules.rs` (replaced stub; now wraps `rules/` module)
- `crates/octo-whatsapp/src/triggers.rs` (replaced stub)
- `crates/octo-whatsapp/src/ipc/handlers/rules.rs` (11 handlers vs 2)
- `crates/octo-whatsapp/src/ipc/handlers/triggers.rs` (6 handlers vs 2)
- `crates/octo-whatsapp/src/daemon.rs` (add `rules` + `triggers` + `audit_log` fields)
- `crates/octo-whatsapp/src/config.rs` (add `SecurityConfig`, `RulesConfig`, `TriggersConfig`, `ActionsConfig`)
- `crates/octo-whatsapp/src/lib.rs` (declare new modules)
- `crates/octo-whatsapp/src/cli.rs` (5 new top-level subcommands)
- `crates/octo-whatsapp/src/mcp_server.rs` (17 new tool descriptors + dispatch)
- `crates/octo-whatsapp/src/ipc/handlers/mod.rs` (17 new registrations)
- `crates/octo-whatsapp/Cargo.toml` (deps: `arc-swap`, `regex`, `sha2`, `hex`, optional `landlock`/`seccompiler`/`nix`)
- `crates/octo-whatsapp/README.md` (status update)

**Created:**
- `crates/octo-whatsapp/src/rules/` (predicate.rs, ruleset.rs, rule_store.rs, etag.rs, mod.rs)
- `crates/octo-whatsapp/src/triggers/` (trigger.rs, registry.rs, mod.rs)
- `crates/octo-whatsapp/src/actions/` (mod.rs, webhook.rs, agent_run.rs, shell.rs, mcp_notify.rs, escalate.rs, runner/{mod,shell_linux,shell_other}.rs)
- `crates/octo-whatsapp/src/audit.rs` (AuditLog + AuditEntry + verify_chain)
- `crates/octo-whatsapp/tests/it_rules_hot_swap.rs`
- `crates/octo-whatsapp/tests/it_audit_chain_integrity.rs`
- `crates/octo-whatsapp/tests/it_actions_rejection.rs`
- `crates/octo-whatsapp/tests/it_rules_mutation.rs`
- `memory/whatsapp-phase4-handoff.md`
- `docs/plans/2026-07-06-whatsapp-runtime-cli-mcp-phase4.md` (this file)

**Untouched:**
- `crates/octo-network/src/dot/adapters/mod.rs` (no PlatformAdapter change)
- All Phase 1/2/3 code remains working

---

## Verification

```bash
cargo check --workspace --all-features
cargo test -p octo-whatsapp --features test-helpers
cargo test -p octo-adapter-whatsapp --features test-helpers --test inherent_smoke
cargo clippy -p octo-whatsapp -p octo-adapter-whatsapp --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
cargo llvm-cov --no-default-features --features test-helpers -p octo-whatsapp --summary-only
```

Expected:
- `cargo test -p octo-whatsapp`: 322 (existing) + 50 (rules) + 25 (triggers) + 25 (actions) + 14 (audit) + 5 (sandbox) + 5 (integration) = **~446 tests**
- `cargo llvm-cov --summary-only`:
  - lines ≥ 85.00%, branches ≥ 75.00%
  - rules.rs ≥ 90/85
  - triggers.rs ≥ 75/65
  - actions/*.rs ≥ 80/70
- `cargo clippy`: 0 warnings
- `cargo fmt --check`: 0 diff