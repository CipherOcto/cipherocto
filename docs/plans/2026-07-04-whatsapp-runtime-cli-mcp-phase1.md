# WhatsApp Runtime CLI + MCP — Phase 1 Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Build a long-lived `octo-whatsapp` daemon binary that owns the WhatsApp WebSocket and exposes 12 RPC methods over a unix-socket JSON-RPC control surface, with a thin CLI mirror and a thin MCP server over stdio. Onboarding subcommands delegate to the existing `octo-whatsapp-onboard-core` library.

**Architecture:** Single tokio runtime, supervised multi-task via `JoinSet` + `CancellationToken`. Daemon owns `WhatsAppWebAdapter`; CLI and MCP connect to the daemon via unix socket. Stoolap handle is shared via `Arc<StoolapStore>` cloned from the adapter at startup (never per-client, never directly depended on by the runtime crate — only via `octo-adapter-whatsapp`). Phase 1 covers the read path + minimal write path (`send.text`, `groups.create|list|info|leave`, `messages.list`); rules/triggers are read-only stubs; events are read-only with no `tail` yet.

**Tech Stack:** Rust 2021, `tokio` 1, `clap` 4 derive, `serde`/`serde_json`, `arc-swap` 1, `tokio-util` (CancellationToken), `tracing`+`tracing-subscriber`+`tracing-appender`, `nix` (unix socket + SO_PEERCRED), `whatsapp-rust` (already in `octo-adapter-whatsapp`), `assert_cmd`+`predicates`+`tempfile` for integration tests.

**Reference docs:**
- Design: `docs/plans/2026-07-04-whatsapp-runtime-cli-mcp-design.md` (full spec)
- §Rollout (Phase 1 only): design lines 1631-1645
- §IPC Contract + Error codes: design lines 536-653
- §Daemon Lifecycle + lock ordering: design lines 324-415
- §MCP Server: design lines 821-862

**Source crates to read before starting:**
- `crates/octo-adapter-whatsapp/src/adapter.rs` (the `WhatsAppWebAdapter` + `start_bot()` at lines 1246-1248)
- `crates/octo-adapter-whatsapp/src/state.rs` (the `BotState` enum, 7 variants)
- `crates/octo-network/src/dot/adapters/coordinator_admin.rs` (the `CoordinatorAdmin` trait, 24 methods)
- `crates/octo-whatsapp-onboard-core/src/lib.rs` (delegation target for `onboard` subcommand)
- `crates/octo-runtime/src/` (reusable supervisor primitives if they exist; otherwise build local)
- `crates/octo-telegram-onboard/` (exemplar for single-binary multi-mode CLI structure)

---

## Conventions used in every task

- **TDD cycle**: every task follows `red → green → commit`.
- **Files**: always exact paths, never "create `src/foo.rs`" without path.
- **Commands**: every shell command is runnable as-is.
- **Clippy gate**: every task's final commit must pass `cargo clippy --all-targets -- -D warnings` on the touched crate (workspace gate is at Task 65).
- **Worktree**: all commands assume cwd `/home/mmacedoeu/_w/ai/cipherocto/.worktrees/whatsapp-runtime-cli-mcp`.
- **Format**: run `cargo fmt -- crates/octo-whatsapp/` before every commit (the user-facing rule applies).
- **Commit messages**: conventional commits (`feat:`, `chore:`, `test:`, `fix:`, `docs:`).

---

# Part A — Workspace scaffolding (Tasks 1-4)

## Task 1: Exclude `octo-whatsapp` from the workspace

**Files:**
- Modify: `Cargo.toml:8` (the `exclude` list)

**Step 1:** Edit root `Cargo.toml` and add `crates/octo-whatsapp` to the `exclude` list (right after `crates/octo-whatsapp-onboard`, before the closing `]`).

**Step 2:** Verify the workspace still resolves.

Run: `cargo check --workspace --exclude octo-adapter-telegram 2>&1 | tail -5`
Expected: `Finished` line; no error.

**Step 3:** Commit.

```bash
git add Cargo.toml
git commit -m "chore(workspace): exclude octo-whatsapp from default workspace build"
```

## Task 2: Create `octo-whatsapp` crate scaffold

**Files:**
- Create: `crates/octo-whatsapp/Cargo.toml`
- Create: `crates/octo-whatsapp/src/lib.rs`

**Step 1:** Write `Cargo.toml` with Phase 1 deps (NO schemars, NO landlock, NO seccomp, NO openat2, NO rmcp — those are Phase 2+):

```toml
[package]
name = "octo-whatsapp"
version = "0.1.0"
edition = "2021"

[[bin]]
name = "octo-whatsapp"
path = "src/main.rs"

[lib]
name = "octo_whatsapp"
path = "src/lib.rs"

[dependencies]
# Internal — adapter is the only thing we depend on for protocol; onboard-core
# is for delegation; network is for trait types only (we use them through the
# adapter); runtime is for the supervisor primitive if it exists.
octo-adapter-whatsapp      = { path = "../octo-adapter-whatsapp" }
octo-whatsapp-onboard-core = { path = "../octo-whatsapp-onboard-core" }
octo-network               = { path = "../octo-network" }

# Async + serialization
tokio       = { version = "1", features = ["full"] }
tokio-util  = { version = "0.7", features = ["rt"] }
async-trait = "0.1"
arc-swap    = "1"
serde       = { version = "1", features = ["derive"] }
serde_json  = "1"

# CLI
clap        = { version = "4.5", features = ["derive", "wrap_help"] }
clap_complete = "4.5"

# Observability
tracing            = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "json"] }
tracing-appender   = "0.2"

# Unix socket + SO_PEERCRED (Linux-only; we test on Linux only)
nix = { version = "0.29", features = ["fs", "socket", "uio", "feature"] }

# Peer JID parsing/formatting
phonenumber = "0.3"

# Error mapping
thiserror = "1"
anyhow    = "1"

[features]
default       = []
live-whatsapp = ["octo-adapter-whatsapp/live-whatsapp"]

[dev-dependencies]
tempfile   = "3"
assert_cmd = "2"
predicates = "3"
tokio      = { version = "1", features = ["full", "test-util"] }
```

**Step 2:** Write `src/lib.rs` with placeholder module declarations (we will fill these in subsequent tasks):

```rust
//! `octo-whatsapp` — long-lived daemon for the WhatsApp adapter.
//!
//! See `docs/plans/2026-07-04-whatsapp-runtime-cli-mcp-design.md` for the
//! full design. Phase 1 (MVP) covers the daemon + unix socket + JSON-RPC +
//! the 12 method surfaces listed in §Rollout, plus CLI and MCP mirrors.

#![deny(rust_2018_idioms)]
#![warn(missing_debug_implementations)]

pub mod config;
pub mod daemon;
pub mod events;
pub mod ipc;
pub mod jids;
pub mod onboarding;
pub mod rules;
pub mod triggers;

pub use config::WhatsAppRuntimeConfig;
pub use daemon::{Daemon, DaemonHandle};
```

**Step 3:** Create stub files for each module (just empty `pub` items so the crate compiles). Each file gets one line: `// TODO: see Phase 1 task N.`

Files to create:
- `crates/octo-whatsapp/src/config.rs` — `// TODO: see Phase 1 task 5.`
- `crates/octo-whatsapp/src/daemon.rs` — `// TODO: see Phase 1 task 14.`
- `crates/octo-whatsapp/src/events.rs` — `// TODO: see Phase 1 task 19.`
- `crates/octo-whatsapp/src/jids.rs` — `// TODO: see Phase 1 task 9.`
- `crates/octo-whatsapp/src/onboarding.rs` — `// TODO: see Phase 1 task 51.`
- `crates/octo-whatsapp/src/rules.rs` — `// TODO: see Phase 1 task 22.`
- `crates/octo-whatsapp/src/triggers.rs` — `// TODO: see Phase 1 task 23.`
- `crates/octo-whatsapp/src/ipc/mod.rs` — `// TODO: see Phase 1 task 24.`
- `crates/octo-whatsapp/src/ipc/protocol.rs` — `// TODO: see Phase 1 task 28.`
- `crates/octo-whatsapp/src/ipc/server.rs` — `// TODO: see Phase 1 task 32.`
- `crates/octo-whatsapp/src/main.rs` — see code below

**Step 4:** Write `src/main.rs`:

```rust
fn main() {
    eprintln!("octo-whatsapp: stub — Phase 1 in progress");
    std::process::exit(2);
}
```

**Step 5:** Verify the crate compiles standalone.

Run: `cargo check --manifest-path crates/octo-whatsapp/Cargo.toml 2>&1 | tail -10`
Expected: `Finished` line; no error. (Stub modules with just `// TODO` are valid.)

**Step 6:** Commit.

```bash
git add crates/octo-whatsapp/
git commit -m "chore(octo-whatsapp): scaffold Phase 1 crate skeleton"
```

## Task 3: Add `octo-cli-meta` `whatsapp-cli` feature

**Files:**
- Modify: `crates/octo-cli-meta/Cargo.toml`

**Step 1:** Add `octo-whatsapp` as an optional dep and the `whatsapp-cli` feature.

Find the `[dependencies]` section of `crates/octo-cli-meta/Cargo.toml`. After the existing entries, add:

```toml
octo-whatsapp = { path = "../octo-whatsapp", optional = true }
```

Find the `[features]` section. Add:

```toml
whatsapp-cli = ["dep:octo-whatsapp"]
```

**Step 2:** Verify meta-crate still builds.

Run: `cargo check -p octo-cli-meta --features whatsapp-cli 2>&1 | tail -5`
Expected: `Finished`.

**Step 3:** Commit.

```bash
git add crates/octo-cli-meta/Cargo.toml
git commit -m "feat(octo-cli-meta): add whatsapp-cli feature"
```

## Task 4: Verify workspace baseline still clean

Run: `cargo check --workspace --exclude octo-adapter-telegram 2>&1 | tail -3`
Expected: clean `Finished` line.

If new warnings appear in the touched crates (none expected), fix them before continuing. Run `cargo fmt -- crates/octo-whatsapp/ crates/octo-cli-meta/` if needed.

---

# Part B — JID normalization (Tasks 5-13)

The `jids` module owns peer/JID parsing. Every CLI/RPC entry point that takes a peer or group MUST go through these helpers (locked-in invariant: never construct a JID inline).

## Task 5: Failing test for `peer_to_jid` happy path

**Files:**
- Modify: `crates/octo-whatsapp/src/jids.rs`
- Create: `crates/octo-whatsapp/src/jids/tests.rs` (use `#[cfg(test)] mod tests;`)

**Step 1:** Replace the `jids.rs` stub with:

```rust
//! Peer and group JID normalization. Every CLI/RPC entry point that takes a
//! peer or group MUST route through these helpers.

use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum JidError {
    #[error("expected E.164, <digits>@s.whatsapp.net, or <digits>@lid; got {0:?}")]
    InvalidPeerFormat(String),
    #[error("expected <digits>@g.us; got {0:?}")]
    InvalidGroupFormat(String),
    #[error("phone number invalid: {0}")]
    InvalidPhone(String),
}

pub fn peer_to_jid(input: &str) -> Result<String, JidError> {
    todo!("Phase 1 Task 6")
}

pub fn group_to_jid(input: &str) -> Result<String, JidError> {
    todo!("Phase 1 Task 9")
}

#[cfg(test)]
mod tests;
```

**Step 2:** Create `jids/tests.rs`:

```rust
use super::*;

#[test]
fn peer_to_jid_accepts_e164_us() {
    let jid = peer_to_jid("+15551234567").unwrap();
    assert_eq!(jid, "15551234567@s.whatsapp.net");
}

#[test]
fn peer_to_jid_accepts_e164_br() {
    let jid = peer_to_jid("+5511987654321").unwrap();
    assert_eq!(jid, "5511987654321@s.whatsapp.net");
}

#[test]
fn peer_to_jid_accepts_s_whatsapp_net_explicit() {
    let jid = peer_to_jid("15551234567@s.whatsapp.net").unwrap();
    assert_eq!(jid, "15551234567@s.whatsapp.net");
}

#[test]
fn peer_to_jid_accepts_lid() {
    let jid = peer_to_jid("1234567890@lid").unwrap();
    assert_eq!(jid, "1234567890@lid");
}

#[test]
fn peer_to_jid_strips_leading_plus() {
    let jid = peer_to_jid("+447911123456").unwrap();
    assert!(jid.ends_with("@s.whatsapp.net"));
    assert!(!jid.starts_with("+"));
}

#[test]
fn peer_to_jid_rejects_empty() {
    assert_eq!(
        peer_to_jid(""),
        Err(JidError::InvalidPeerFormat(String::new())),
    );
}

#[test]
fn peer_to_jid_rejects_group_jid() {
    assert!(matches!(
        peer_to_jid("120363@g.us"),
        Err(JidError::InvalidPeerFormat(_))
    ));
}

#[test]
fn peer_to_jid_rejects_arbitrary_at_sign() {
    assert!(matches!(
        peer_to_jid("foo@bar"),
        Err(JidError::InvalidPeerFormat(_))
    ));
}
```

**Step 3:** Run the test. Expect FAIL with `not yet implemented`.

Run: `cargo test -p octo-whatsapp --lib jids:: 2>&1 | tail -10`
Expected: `thread '...' panicked at 'not yet implemented'`.

## Task 6: Implement `peer_to_jid`

**Files:**
- Modify: `crates/octo-whatsapp/src/jids.rs`

**Step 1:** Replace `todo!()` in `peer_to_jid`:

```rust
pub fn peer_to_jid(input: &str) -> Result<String, JidError> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(JidError::InvalidPeerFormat(trimmed.to_string()));
    }
    if trimmed.ends_with("@lid") {
        let digits = trimmed.trim_end_matches("@lid");
        if digits.chars().all(|c| c.is_ascii_digit()) && !digits.is_empty() {
            return Ok(format!("{digits}@lid"));
        }
        return Err(JidError::InvalidPeerFormat(trimmed.to_string()));
    }
    if trimmed.ends_with("@s.whatsapp.net") {
        return Ok(trimmed.to_string());
    }
    if trimmed.contains('@') {
        return Err(JidError::InvalidPeerFormat(trimmed.to_string()));
    }
    if trimmed.contains('@') || trimmed.contains(' ') {
        return Err(JidError::InvalidPeerFormat(trimmed.to_string()));
    }
    let digits = trimmed.trim_start_matches('+');
    if !digits.chars().all(|c| c.is_ascii_digit()) || digits.is_empty() {
        return Err(JidError::InvalidPeerFormat(trimmed.to_string()));
    }
    // Light validation: 7–15 digits (E.164 max length).
    if digits.len() < 7 || digits.len() > 15 {
        return Err(JidError::InvalidPhone(trimmed.to_string()));
    }
    Ok(format!("{digits}@s.whatsapp.net"))
}
```

**Step 2:** Run the tests.

Run: `cargo test -p octo-whatsapp --lib jids::tests::peer 2>&1 | tail -5`
Expected: all 8 tests PASS.

## Task 7: Failing test for `group_to_jid`

**Files:**
- Modify: `crates/octo-whatsapp/src/jids/tests.rs`

**Step 1:** Append tests:

```rust
#[test]
fn group_to_jid_accepts_canonical() {
    let jid = group_to_jid("120363123456789@g.us").unwrap();
    assert_eq!(jid, "120363123456789@g.us");
}

#[test]
fn group_to_jid_rejects_dm_jid() {
    assert!(matches!(
        group_to_jid("15551234567@s.whatsapp.net"),
        Err(JidError::InvalidGroupFormat(_))
    ));
}

#[test]
fn group_to_jid_rejects_lid() {
    assert!(matches!(
        group_to_jid("1234@lid"),
        Err(JidError::InvalidGroupFormat(_))
    ));
}

#[test]
fn group_to_jid_rejects_bare_digits() {
    assert!(matches!(
        group_to_jid("120363123456789"),
        Err(JidError::InvalidGroupFormat(_))
    ));
}
```

**Step 2:** Run; expect FAIL with `not yet implemented`.

Run: `cargo test -p octo-whatsapp --lib jids::tests::group 2>&1 | tail -5`

## Task 8: Implement `group_to_jid`

**Files:**
- Modify: `crates/octo-whatsapp/src/jids.rs`

**Step 1:** Replace `todo!()`:

```rust
pub fn group_to_jid(input: &str) -> Result<String, JidError> {
    let trimmed = input.trim();
    if !trimmed.ends_with("@g.us") {
        return Err(JidError::InvalidGroupFormat(trimmed.to_string()));
    }
    let digits = trimmed.trim_end_matches("@g.us");
    if digits.chars().all(|c| c.is_ascii_digit())
        && !digits.is_empty()
        && digits.len() >= 10
    {
        Ok(trimmed.to_string())
    } else {
        Err(JidError::InvalidGroupFormat(trimmed.to_string()))
    }
}
```

**Step 2:** Run.

Run: `cargo test -p octo-whatsapp --lib jids:: 2>&1 | tail -3`
Expected: 12 tests PASS.

**Step 3:** Commit.

```bash
git add crates/octo-whatsapp/src/jids.rs
git commit -m "feat(octo-whatsapp): add peer_to_jid + group_to_jid normalization"
```

---

# Part C — Configuration (Tasks 9-13)

## Task 9: Failing test for `WhatsAppRuntimeConfig::from_toml`

**Files:**
- Modify: `crates/octo-whatsapp/src/config.rs`

**Step 1:** Replace the stub:

```rust
//! Runtime configuration loaded from a TOML file.
//!
//! Phase 1: minimal schema (name + paths + socket). Rules, triggers,
//! event-retention, observability, and security fields arrive in later
//! phases. The schema is intentionally additive.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("toml parse: {0}")]
    Toml(#[from] toml::de::Error),
    #[error("invalid name {0:?}: must match [a-z0-9_-]+")]
    InvalidName(String),
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct WhatsAppRuntimeConfig {
    pub name: String,
    #[serde(default = "default_data_dir")]
    pub data_dir: PathBuf,
    #[serde(default = "default_log_dir")]
    pub log_dir: PathBuf,
    #[serde(default = "default_socket_dir")]
    pub socket_dir: PathBuf,
}

fn default_data_dir() -> PathBuf {
    PathBuf::from("/var/lib/octo/whatsapp")
}
fn default_log_dir() -> PathBuf {
    PathBuf::from("/var/log/octo/whatsapp")
}
fn default_socket_dir() -> PathBuf {
    PathBuf::from("/run/octo/whatsapp")
}

impl WhatsAppRuntimeConfig {
    pub fn from_toml(bytes: &[u8]) -> Result<Self, ConfigError> {
        todo!("Phase 1 Task 10")
    }

    pub fn from_path(path: &Path) -> Result<Self, ConfigError> {
        todo!("Phase 1 Task 10")
    }

    pub fn socket_path(&self) -> PathBuf {
        self.socket_dir.join(format!("octo-whatsapp-{}.sock", self.name))
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        todo!("Phase 1 Task 11")
    }
}

#[cfg(test)]
mod tests;
```

(Add `toml = "0.8"` to `[dependencies]` in `Cargo.toml` if not present.)

**Step 2:** Create `config/tests.rs`:

```rust
use super::*;

const MINIMAL: &str = r#"
name = "default"
"#;

#[test]
fn from_toml_parses_minimal() {
    let cfg = WhatsAppRuntimeConfig::from_toml(MINIMAL.as_bytes()).unwrap();
    assert_eq!(cfg.name, "default");
}

#[test]
fn defaults_apply() {
    let cfg = WhatsAppRuntimeConfig::from_toml(MINIMAL.as_bytes()).unwrap();
    assert_eq!(cfg.data_dir, PathBuf::from("/var/lib/octo/whatsapp"));
    assert_eq!(cfg.log_dir, PathBuf::from("/var/log/octo/whatsapp"));
    assert_eq!(cfg.socket_dir, PathBuf::from("/run/octo/whatsapp"));
}

#[test]
fn override_paths() {
    let cfg = WhatsAppRuntimeConfig::from_toml(
        br#"
name = "alice"
data_dir = "/srv/whatsapp/alice/data"
log_dir  = "/srv/whatsapp/alice/log"
socket_dir = "/run/user/1000"
"#,
    )
    .unwrap();
    assert_eq!(cfg.name, "alice");
    assert_eq!(cfg.data_dir, PathBuf::from("/srv/whatsapp/alice/data"));
    assert_eq!(cfg.log_dir, PathBuf::from("/srv/whatsapp/alice/log"));
    assert_eq!(cfg.socket_dir, PathBuf::from("/run/user/1000"));
}

#[test]
fn socket_path_uses_name() {
    let cfg = WhatsAppRuntimeConfig::from_toml(MINIMAL.as_bytes()).unwrap();
    assert_eq!(
        cfg.socket_path(),
        PathBuf::from("/run/octo/whatsapp/octo-whatsapp-default.sock"),
    );
}

#[test]
fn from_path_reads_file(tmp: std::path::PathBuf) {
    let p = tmp.join("config.toml");
    std::fs::write(&p, MINIMAL).unwrap();
    let cfg = WhatsAppRuntimeConfig::from_path(&p).unwrap();
    assert_eq!(cfg.name, "default");
}
```

**Step 3:** Run; expect FAIL with `not yet implemented`.

Run: `cargo test -p octo-whatsapp --lib config:: 2>&1 | tail -5`

## Task 10: Implement `from_toml` and `from_path`

**Files:**
- Modify: `crates/octo-whatsapp/src/config.rs`

**Step 1:** Replace `todo!()`:

```rust
pub fn from_toml(bytes: &[u8]) -> Result<Self, ConfigError> {
    let cfg: Self = toml::from_slice(bytes)?;
    cfg.validate()?;
    Ok(cfg)
}

pub fn from_path(path: &Path) -> Result<Self, ConfigError> {
    let bytes = std::fs::read(path)?;
    Self::from_toml(&bytes)
}
```

## Task 11: Implement `validate`

**Files:**
- Modify: `crates/octo-whatsapp/src/config.rs`

**Step 1:** Replace `todo!()` in `validate`:

```rust
pub fn validate(&self) -> Result<(), ConfigError> {
    if self.name.is_empty()
        || !self
            .name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
    {
        return Err(ConfigError::InvalidName(self.name.clone()));
    }
    Ok(())
}
```

**Step 2:** Add the `tmp` test helper. Append to `config/tests.rs`:

```rust
#[test]
fn validate_rejects_uppercase() {
    let mut cfg = WhatsAppRuntimeConfig::from_toml(MINIMAL.as_bytes()).unwrap();
    cfg.name = "Default".to_string();
    assert!(matches!(cfg.validate(), Err(ConfigError::InvalidName(_))));
}

#[test]
fn validate_rejects_path_traversal() {
    let mut cfg = WhatsAppRuntimeConfig::from_toml(MINIMAL.as_bytes()).unwrap();
    cfg.name = "../etc".to_string();
    assert!(matches!(cfg.validate(), Err(ConfigError::InvalidName(_))));
}

#[test]
fn validate_rejects_empty_name() {
    let mut cfg = WhatsAppRuntimeConfig::from_toml(MINIMAL.as_bytes()).unwrap();
    cfg.name = String::new();
    assert!(matches!(cfg.validate(), Err(ConfigError::InvalidName(_))));
}
```

**Step 3:** Run.

Run: `cargo test -p octo-whatsapp --lib config:: 2>&1 | tail -3`
Expected: all 7 tests PASS.

**Step 4:** Commit.

```bash
git add crates/octo-whatsapp/src/config.rs Cargo.toml
git commit -m "feat(octo-whatsapp): add WhatsAppRuntimeConfig TOML loader"
```

---

# Part D — InboundEvent scaffolding (Tasks 12-13)

The `events` module will hold the typed `InboundEvent` enum and the parser. Phase 1 only needs the enum shell and a `parse_unknown` fallback — full parsing arrives in Phase 3.

## Task 12: Failing test for `InboundEvent::parse`

**Files:**
- Modify: `crates/octo-whatsapp/src/events.rs`

**Step 1:** Replace the stub:

```rust
//! Typed inbound event model + parser. Phase 1: only the `Unknown` fallback
//! is exercised; the full parser arrives in Phase 3 alongside the event
//! router and `events.tail`.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EventEnvelope {
    pub raw: String,
    pub ts_unix_ms: i64,
    pub ts_mono_ns: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum InboundEvent {
    Unknown {
        raw: String,
        ts_unix_ms: i64,
        ts_mono_ns: u64,
    },
}

impl InboundEvent {
    pub fn parse(env: EventEnvelope) -> Self {
        // Phase 1: every event is `Unknown`. Phase 3 will introduce a real
        // parser that classifies by `format!("{:?}", ev)` output shape.
        let _ = env;
        todo!("Phase 1 Task 13")
    }
}

#[cfg(test)]
mod tests;
```

**Step 2:** Create `events/tests.rs`:

```rust
use super::*;

#[test]
fn parse_returns_unknown() {
    let env = EventEnvelope {
        raw: "any string".to_string(),
        ts_unix_ms: 1_700_000_000_000,
        ts_mono_ns: 123_456_789,
    };
    let ev = InboundEvent::parse(env);
    assert!(matches!(ev, InboundEvent::Unknown { .. }));
}
```

**Step 3:** Run; expect FAIL with `not yet implemented`.

Run: `cargo test -p octo-whatsapp --lib events:: 2>&1 | tail -5`

## Task 13: Implement `InboundEvent::parse`

**Files:**
- Modify: `crates/octo-whatsapp/src/events.rs`

**Step 1:** Replace `todo!()`:

```rust
pub fn parse(env: EventEnvelope) -> Self {
    InboundEvent::Unknown {
        raw: env.raw,
        ts_unix_ms: env.ts_unix_ms,
        ts_mono_ns: env.ts_mono_ns,
    }
}
```

**Step 2:** Run.

Run: `cargo test -p octo-whatsapp --lib events:: 2>&1 | tail -3`
Expected: 1 test PASS.

**Step 3:** Commit.

```bash
git add crates/octo-whatsapp/src/events.rs
git commit -m "feat(octo-whatsapp): add InboundEvent stub (Unknown-only in Phase 1)"
```

---

# Part E — Rules / Triggers stubs (Tasks 14-16)

These are read-only in Phase 1. The stub returns an empty vec; the full `arc_swap::ArcSwap<Ruleset>` machinery arrives in Phase 4.

## Task 14: `rules.rs` stub with empty list

**Files:**
- Modify: `crates/octo-whatsapp/src/rules.rs`

```rust
//! Rules engine stub. Phase 1: read-only empty list. Phase 4 will introduce
//! `arc_swap::ArcSwap<Ruleset>`, the matcher pool, and the rule_draft →
//! rule_approved flow.

#[derive(Debug, Clone, Default)]
pub struct RulesView {
    pub rules: Vec<Rule>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct Rule {
    pub id: String,
    pub name: String,
    pub enabled: bool,
}

impl RulesView {
    pub fn empty() -> Self {
        Self { rules: Vec::new() }
    }
    pub fn list(&self) -> &[Rule] {
        &self.rules
    }
    pub fn get(&self, _id: &str) -> Option<&Rule> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_view() {
        let v = RulesView::empty();
        assert!(v.list().is_empty());
        assert!(v.get("anything").is_none());
    }
}
```

Run: `cargo test -p octo-whatsapp --lib rules:: 2>&1 | tail -3` — expect PASS.

Commit:
```bash
git add crates/octo-whatsapp/src/rules.rs
git commit -m "feat(octo-whatsapp): add rules read-only stub (Phase 1)"
```

## Task 15: `triggers.rs` stub

Same shape as `rules.rs`. Replace `crates/octo-whatsapp/src/triggers.rs`:

```rust
//! Triggers stub. Phase 1: read-only empty list. Phase 4 will add the
//! stateful agent-target registry.

#[derive(Debug, Clone, Default)]
pub struct TriggersView {
    pub triggers: Vec<Trigger>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct Trigger {
    pub id: String,
    pub name: String,
    pub enabled: bool,
}

impl TriggersView {
    pub fn empty() -> Self {
        Self { triggers: Vec::new() }
    }
    pub fn list(&self) -> &[Trigger] {
        &self.triggers
    }
    pub fn get(&self, _id: &str) -> Option<&Trigger> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_view() {
        let v = TriggersView::empty();
        assert!(v.list().is_empty());
        assert!(v.get("anything").is_none());
    }
}
```

Run: `cargo test -p octo-whatsapp --lib triggers:: 2>&1 | tail -3` — expect PASS.

Commit:
```bash
git add crates/octo-whatsapp/src/triggers.rs
git commit -m "feat(octo-whatsapp): add triggers read-only stub (Phase 1)"
```

## Task 16: Onboarding passthrough

**Files:**
- Modify: `crates/octo-whatsapp/src/onboarding.rs`

Read `crates/octo-whatsapp-onboard-core/src/lib.rs` to find the entry points (`pair_link`, `qr_link`, `wait_for_connected`, `list_sessions`, etc.). The passthrough delegates to those.

```rust
//! Onboarding passthrough. The runtime does NOT auto-onboard; operators
//! always invoke `octo-whatsapp onboard qr-link|pair-link|...` themselves.
//! Phase 1: thin re-exports + command builders. No daemon is involved.

pub use octo_whatsapp_onboard_core::{
    CoreError,
    PairLinkArgs as CorePairLinkArgs,
    QrLinkArgs as CoreQrLinkArgs,
    wait_for_connected,
};

#[derive(Debug, Clone)]
pub enum OnboardCommand {
    QrLink { timeout_secs: u64 },
    PairLink { phone: String },
    Whoami,
    SessionList,
    SessionVerify { name: String },
    SessionRemove { name: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_construction() {
        let c = OnboardCommand::QrLink { timeout_secs: 120 };
        assert!(matches!(c, OnboardCommand::QrLink { timeout_secs: 120 }));
    }
}
```

Run: `cargo test -p octo-whatsapp --lib onboarding:: 2>&1 | tail -3` — expect PASS.

Commit:
```bash
git add crates/octo-whatsapp/src/onboarding.rs
git commit -m "feat(octo-whatsapp): add onboarding passthrough module"
```

---

# Part F — IPC protocol types (Tasks 17-22)

## Task 17: Failing test for `RpcRequest::from_json`

**Files:**
- Modify: `crates/octo-whatsapp/src/ipc/mod.rs`
- Modify: `crates/octo-whatsapp/src/ipc/protocol.rs`

**Step 1:** Replace `ipc/mod.rs`:

```rust
pub mod protocol;
pub mod server;

pub use protocol::{RpcError, RpcErrorCode, RpcRequest, RpcResponse};
```

**Step 2:** Replace `ipc/protocol.rs`:

```rust
//! JSON-RPC 2.0 protocol types. Newline-delimited JSON, one request and one
//! response per line. See RFC-RPC2 for the wire format (we are not embedding
//! the full spec here; this module is a strict subset of JSON-RPC 2.0).

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RpcRequest {
    pub id: u64,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RpcResponse {
    pub id: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<RpcError>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

/// Standard JSON-RPC 2.0 codes + CIPHEROCTO custom codes (-32001 .. -32099).
/// See design §Error codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RpcErrorCode {
    // JSON-RPC 2.0 standard
    ParseError = -32700,
    InvalidRequest = -32600,
    MethodNotFound = -32601,
    InvalidParams = -32602,
    InternalError = -32603,

    // CipherOcto custom
    SessionLost = -32001,
    NotConfigured = -32002,
    RateLimited = -32003,
    PayloadTooLarge = -32004,
    GroupNotAdmin = -32005,
    FallbackExhausted = -32006,
    NotConnected = -32012,
    EditWindowExpired = -32013,
    DeleteWindowExpired = -32014,
    Internal = -32050,
    Unimplemented = -32060,
    ShuttingDown = -32099,

    /// Generic / unknown — only used for forward-compatibility with codes
    /// this binary does not yet know about.
    Other(i32),
}

impl RpcErrorCode {
    pub fn as_i32(self) -> i32 {
        match self {
            RpcErrorCode::Other(c) => c,
            other => other as i32,
        }
    }
}

impl RpcRequest {
    pub fn from_json(bytes: &[u8]) -> Result<Self, RpcParseError> {
        todo!("Phase 1 Task 18")
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RpcParseError {
    #[error("malformed JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("missing field: {0}")]
    MissingField(&'static str),
    #[error("invalid id: must be u64")]
    InvalidId,
}

#[cfg(test)]
mod tests;
```

**Step 3:** Create `ipc/protocol/tests.rs`:

```rust
use super::*;

#[test]
fn parse_minimal_request() {
    let r: RpcRequest = serde_json::from_slice(br#"{"id":1,"method":"status.get"}"#).unwrap();
    assert_eq!(r.id, 1);
    assert_eq!(r.method, "status.get");
    assert_eq!(r.params, Value::Null);
}

#[test]
fn parse_request_with_params() {
    let r: RpcRequest = serde_json::from_slice(
        br#"{"id":42,"method":"send.text","params":{"peer":"+15551234567","text":"hi"}}"#,
    )
    .unwrap();
    assert_eq!(r.id, 42);
    assert_eq!(r.method, "send.text");
    assert_eq!(r.params["peer"], "+15551234567");
    assert_eq!(r.params["text"], "hi");
}

#[test]
fn parse_missing_method_fails() {
    let res: Result<RpcRequest, _> = serde_json::from_slice(br#"{"id":1}"#);
    assert!(res.is_err());
}

#[test]
fn parse_string_id_rejected() {
    let res: Result<RpcRequest, _> =
        serde_json::from_slice(br#"{"id":"abc","method":"x"}"#);
    assert!(res.is_err());
}

#[test]
fn response_with_result() {
    let r = RpcResponse {
        id: 1,
        result: Some(serde_json::json!({"ok": true})),
        error: None,
    };
    let s = serde_json::to_string(&r).unwrap();
    assert!(s.contains("\"result\""));
    assert!(!s.contains("\"error\""));
}

#[test]
fn response_with_error() {
    let r = RpcResponse {
        id: 1,
        result: None,
        error: Some(RpcError {
            code: -32601,
            message: "Method not found".to_string(),
            data: None,
        }),
    };
    let s = serde_json::to_string(&r).unwrap();
    assert!(s.contains("\"error\""));
    assert!(!s.contains("\"result\""));
}
```

**Step 4:** Run; expect PASS for the serde-based parse cases.

Run: `cargo test -p octo-whatsapp --lib ipc::protocol:: 2>&1 | tail -3`
Expected: 6 tests PASS (we use `serde_json::from_slice` directly under the hood).

## Task 18: Implement `RpcRequest::from_json`

**Files:**
- Modify: `crates/octo-whatsapp/src/ipc/protocol.rs`

**Step 1:** Replace `todo!()`:

```rust
pub fn from_json(bytes: &[u8]) -> Result<Self, RpcParseError> {
    let v: serde_json::Value = serde_json::from_slice(bytes)?;
    let obj = v
        .as_object()
        .ok_or(RpcParseError::MissingField("object"))?;
    let id = obj
        .get("id")
        .ok_or(RpcParseError::MissingField("id"))?
        .as_u64()
        .ok_or(RpcParseError::InvalidId)?;
    let method = obj
        .get("method")
        .ok_or(RpcParseError::MissingField("method"))?
        .as_str()
        .ok_or(RpcParseError::MissingField("method"))?
        .to_string();
    let params = obj.get("params").cloned().unwrap_or(Value::Null);
    Ok(Self { id, method, params })
}
```

**Step 2:** Add tests for the new helper in `ipc/protocol/tests.rs`:

```rust
#[test]
fn from_json_helper_matches_serde() {
    let r = RpcRequest::from_json(br#"{"id":7,"method":"x"}"#).unwrap();
    assert_eq!(r.id, 7);
    assert_eq!(r.method, "x");
}

#[test]
fn from_json_helper_rejects_missing_method() {
    assert!(RpcRequest::from_json(br#"{"id":1}"#).is_err());
}
```

**Step 3:** Run.

Run: `cargo test -p octo-whatsapp --lib ipc::protocol:: 2>&1 | tail -3`
Expected: 8 tests PASS.

**Step 4:** Commit.

```bash
git add crates/octo-whatsapp/src/ipc/
git commit -m "feat(octo-whatsapp): add JSON-RPC protocol types"
```

## Task 19: `DaemonState` skeleton

The `DaemonState` is the long-lived shared state. Phase 1 only needs placeholders for the things the 12 RPC methods touch.

**Files:**
- Modify: `crates/octo-whatsapp/src/daemon.rs`

```rust
//! Long-lived daemon. Owns the adapter, the unix-socket server, the
//! event router stub, and the shared stoolap handle.

use std::sync::Arc;

use octo_adapter_whatsapp::WhatsAppWebAdapter;
use tokio_util::sync::CancellationToken;
use tracing::info;

use crate::config::WhatsAppRuntimeConfig;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DaemonPhase {
    Booting,
    Connected,
    SessionLost,
    ShuttingDown,
}

/// Shared, cheaply-cloneable handle to daemon state.
#[derive(Clone)]
pub struct DaemonHandle {
    inner: Arc<DaemonInner>,
}

struct DaemonInner {
    config: WhatsAppRuntimeConfig,
    cancel: CancellationToken,
    phase: tokio::sync::RwLock<DaemonPhase>,
}

impl DaemonHandle {
    pub fn phase(&self) -> DaemonPhase {
        *self.inner.phase.blocking_read()
    }

    pub fn config(&self) -> &WhatsAppRuntimeConfig {
        &self.inner.config
    }

    pub fn cancel_token(&self) -> CancellationToken {
        self.inner.cancel.clone()
    }
}

pub struct Daemon {
    config: WhatsAppRuntimeConfig,
    cancel: CancellationToken,
}

impl Daemon {
    pub fn new(config: WhatsAppRuntimeConfig) -> Self {
        Self {
            config,
            cancel: CancellationToken::new(),
        }
    }

    pub fn handle(&self) -> DaemonHandle {
        DaemonHandle {
            inner: Arc::new(DaemonInner {
                config: self.config.clone(),
                cancel: self.cancel.clone(),
                phase: tokio::sync::RwLock::new(DaemonPhase::Booting),
            }),
        }
    }

    pub async fn run(self, _adapter: WhatsAppWebAdapter) -> anyhow::Result<()> {
        info!(name = self.config.name.as_str(), "daemon stub: exiting immediately");
        // Phase 1 stub: real boot arrives in Task 26.
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handle_phase_starts_booting() {
        let cfg = WhatsAppRuntimeConfig::from_toml(br#"name = "x""#).unwrap();
        let d = Daemon::new(cfg);
        let h = d.handle();
        assert_eq!(h.phase(), DaemonPhase::Booting);
    }

    #[test]
    fn cancel_token_is_linked() {
        let cfg = WhatsAppRuntimeConfig::from_toml(br#"name = "x""#).unwrap();
        let d = Daemon::new(cfg);
        let h = d.handle();
        assert!(!h.cancel_token().is_cancelled());
        d.cancel.cancel();
        assert!(h.cancel_token().is_cancelled());
    }
}
```

Run: `cargo test -p octo-whatsapp --lib daemon:: 2>&1 | tail -3` — expect PASS.

Commit:
```bash
git add crates/octo-whatsapp/src/daemon.rs
git commit -m "feat(octo-whatsapp): add Daemon + DaemonHandle stub"
```

---

# Part G — RPC method registry + dispatcher (Tasks 20-30)

This is the heart of Phase 1. Twelve RPC methods, each implemented as a function that takes the daemon handle + params and returns a `Result<Value, RpcError>`.

## Task 20: Method handler trait + registry

**Files:**
- Modify: `crates/octo-whatsapp/src/ipc/server.rs`

**Step 1:** Replace the stub:

```rust
//! Unix-socket JSON-RPC server. Phase 1: handler trait + registry.
//! The actual socket plumbing arrives in Task 25.

use std::collections::HashMap;
use std::sync::Arc;

use serde_json::Value;

use super::protocol::{RpcError, RpcRequest, RpcResponse};
use crate::daemon::DaemonHandle;

/// One RPC method handler.
#[async_trait::async_trait]
pub trait RpcHandler: Send + Sync {
    fn name(&self) -> &'static str;
    async fn call(&self, handle: DaemonHandle, params: Value) -> Result<Value, RpcError>;
}

pub struct HandlerRegistry {
    handlers: HashMap<&'static str, Arc<dyn RpcHandler>>,
}

impl HandlerRegistry {
    pub fn new() -> Self {
        Self { handlers: HashMap::new() }
    }

    pub fn register(mut self, h: Arc<dyn RpcHandler>) -> Self {
        self.handlers.insert(h.name(), h);
        self
    }

    pub fn get(&self, name: &str) -> Option<Arc<dyn RpcHandler>> {
        self.handlers.get(name).cloned()
    }

    pub fn contains(&self, name: &str) -> bool {
        self.handlers.contains_key(name)
    }

    pub async fn dispatch(
        &self,
        handle: DaemonHandle,
        req: RpcRequest,
    ) -> RpcResponse {
        match self.handlers.get(req.method.as_str()) {
            Some(h) => match h.call(handle, req.params).await {
                Ok(result) => RpcResponse {
                    id: req.id,
                    result: Some(result),
                    error: None,
                },
                Err(err) => RpcResponse {
                    id: req.id,
                    result: None,
                    error: Some(err),
                },
            },
            None => RpcResponse {
                id: req.id,
                result: None,
                error: Some(RpcError {
                    code: super::protocol::RpcErrorCode::MethodNotFound.as_i32(),
                    message: format!("method {:?} not found in this build", req.method),
                    data: Some(serde_json::json!({
                        "api_version": env!("CARGO_PKG_VERSION"),
                        "available_in": "phase2_or_later",
                    })),
                }),
            },
        }
    }
}

impl Default for HandlerRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests;
```

**Step 2:** Add `async-trait` to the deps (it's already in `Cargo.toml` from Task 2).

**Step 3:** Create `ipc/server/tests.rs`:

```rust
use super::*;
use crate::config::WhatsAppRuntimeConfig;
use crate::daemon::Daemon;

struct EchoHandler;

#[async_trait::async_trait]
impl RpcHandler for EchoHandler {
    fn name(&self) -> &'static str { "echo" }
    async fn call(&self, _h: DaemonHandle, params: Value) -> Result<Value, RpcError> {
        Ok(params)
    }
}

#[tokio::test]
async fn dispatch_routes_to_registered_handler() {
    let reg = HandlerRegistry::new().register(Arc::new(EchoHandler));
    let cfg = WhatsAppRuntimeConfig::from_toml(br#"name = "x""#).unwrap();
    let handle = Daemon::new(cfg).handle();
    let req = RpcRequest {
        id: 7,
        method: "echo".to_string(),
        params: serde_json::json!({"a": 1}),
    };
    let resp = reg.dispatch(handle, req).await;
    assert_eq!(resp.id, 7);
    assert_eq!(resp.result.unwrap(), serde_json::json!({"a": 1}));
    assert!(resp.error.is_none());
}

#[tokio::test]
async fn unknown_method_returns_method_not_found() {
    let reg = HandlerRegistry::new();
    let cfg = WhatsAppRuntimeConfig::from_toml(br#"name = "x""#).unwrap();
    let handle = Daemon::new(cfg).handle();
    let req = RpcRequest {
        id: 8,
        method: "no.such.method".to_string(),
        params: Value::Null,
    };
    let resp = reg.dispatch(handle, req).await;
    assert!(resp.result.is_none());
    let err = resp.error.unwrap();
    assert_eq!(err.code, -32601);
}
```

**Step 4:** Run.

Run: `cargo test -p octo-whatsapp --lib ipc::server:: 2>&1 | tail -3`
Expected: 2 tests PASS.

Commit:
```bash
git add crates/octo-whatsapp/src/ipc/server.rs
git commit -m "feat(octo-whatsapp): add RPC handler trait + registry"
```

## Task 21: `version.get` handler

**Files:**
- Create: `crates/octo-whatsapp/src/ipc/handlers.rs`
- Create: `crates/octo-whatsapp/src/ipc/handlers/version.rs`

**Step 1:** Add `handlers.rs`:

```rust
//! Concrete RPC handlers. One file per logical group.

pub mod version;
pub mod status;
pub mod health;
pub mod send_text;
pub mod groups;
pub mod messages;
pub mod rules;
pub mod triggers;
pub mod events;
pub mod daemon_ops;
```

**Step 2:** Add `handlers/version.rs`:

```rust
use serde_json::Value;

use super::super::protocol::{RpcError, RpcErrorCode};
use crate::daemon::DaemonHandle;

pub struct VersionGet;

#[async_trait::async_trait]
impl super::super::server::RpcHandler for VersionGet {
    fn name(&self) -> &'static str { "version.get" }

    async fn call(&self, _h: DaemonHandle, _params: Value) -> Result<Value, RpcError> {
        Ok(serde_json::json!({
            "daemon_api_version": "1.0.0+phase1",
            "daemon_binary_version": env!("CARGO_PKG_VERSION"),
            "build_timestamp": env!("VERGEN_BUILD_TIMESTAMP", "unknown"),
            "phase": "phase1",
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::super::server::RpcHandler;
    use crate::config::WhatsAppRuntimeConfig;
    use crate::daemon::Daemon;

    #[tokio::test]
    async fn version_get_returns_phase1() {
        let cfg = WhatsAppRuntimeConfig::from_toml(br#"name = "x""#).unwrap();
        let h = Daemon::new(cfg).handle();
        let v = VersionGet.call(h, serde_json::Value::Null).await.unwrap();
        assert_eq!(v["daemon_api_version"], "1.0.0+phase1");
        assert_eq!(v["phase"], "phase1");
    }
}
```

The other handler files (`status.rs`, etc.) start as one-line stubs that `todo!()` on call. We'll implement them in Tasks 22-30.

**Step 3:** Add a `handlers/mod.rs` file too. Actually wait — the design says `ipc/handlers.rs`, not `ipc/handlers/mod.rs`. Let me restructure to a single flat `ipc/handlers.rs` file with submodules inline.

Restructure: delete `ipc/handlers.rs` content from above; create `ipc/handlers.rs` as a single file containing all handler modules.

For each of `status.rs`, `health.rs`, `send_text.rs`, `groups.rs`, `messages.rs`, `rules.rs`, `triggers.rs`, `events.rs`, `daemon_ops.rs`: create a file at `crates/octo-whatsapp/src/ipc/handlers/<name>.rs` and a `handlers/mod.rs`.

**Step 2 (corrected):** Set up the directory:

```rust
// crates/octo-whatsapp/src/ipc/handlers/mod.rs
pub mod daemon_ops;
pub mod events;
pub mod groups;
pub mod health;
pub mod messages;
pub mod rules;
pub mod send_text;
pub mod status;
pub mod triggers;
pub mod version;
```

Then `ipc/handlers.rs` re-exports the module:

```rust
// crates/octo-whatsapp/src/ipc/handlers.rs
mod handlers_impl;
pub use handlers_impl::*;
```

Hmm, this is getting tangled. Cleaner: keep `ipc/handlers/` as a directory module. Remove the `ipc/handlers.rs` file. Update `ipc/mod.rs`:

```rust
pub mod handlers;
pub mod protocol;
pub mod server;
```

**Step 4:** Run.

Run: `cargo test -p octo-whatsapp --lib ipc::handlers::version 2>&1 | tail -3`
Expected: 1 test PASS.

Commit:
```bash
git add crates/octo-whatsapp/src/ipc/
git commit -m "feat(octo-whatsapp): add version.get handler"
```

## Task 22: `status.get` handler

The status response is the 4-signal readiness breakdown (per design §Readiness — Connected/SessionValid/Synced/Ready), plus dropped counters.

**Files:**
- Modify: `crates/octo-whatsapp/src/ipc/handlers/status.rs`

```rust
use serde_json::Value;

use super::super::server::RpcHandler;
use crate::daemon::DaemonHandle;

pub struct StatusGet;

#[async_trait::async_trait]
impl RpcHandler for StatusGet {
    fn name(&self) -> &'static str { "status.get" }

    async fn call(&self, handle: DaemonHandle, _params: Value) -> Result<Value, RpcError> {
        Ok(serde_json::json!({
            "phase": format!("{:?}", handle.phase()).to_lowercase(),
            "connected": false,           // adapter not yet wired (Phase 2)
            "session_valid": false,
            "synced": false,
            "ready": false,
            "bot_state": "Disconnected",
            "dropped_inbound": 0u64,
            "last_event_ts_unix_ms": 0i64,
            "sink_lagged_total": {"mcp": 0u64, "cli": 0u64, "rules": 0u64},
            "stoolap_persist_queue_depth": 0u64,
        }))
    }
}
```

The other handlers (`health.rs`, `send_text.rs`, etc.) for now: each is a one-line `pub struct X;` plus `impl RpcHandler for X { fn name(&self) -> &'static str { "x.y" } async fn call(...) -> _ { todo!() } }`. They'll be filled in Tasks 23-30.

Add tests covering the static shape of the response (no business logic yet — just that the keys are present).

Run: `cargo test -p octo-whatsapp --lib ipc::handlers::status 2>&1 | tail -3` — expect PASS.

Commit:
```bash
git add crates/octo-whatsapp/src/ipc/handlers/
git commit -m "feat(octo-whatsapp): add status.get handler (Phase 1 stub)"
```

## Task 23: `health.get` handler

**Files:**
- Modify: `crates/octo-whatsapp/src/ipc/handlers/health.rs`

```rust
use serde_json::Value;
use super::super::server::RpcHandler;
use crate::daemon::DaemonHandle;

pub struct HealthGet;

#[async_trait::async_trait]
impl RpcHandler for HealthGet {
    fn name(&self) -> &'static str { "health.get" }

    async fn call(&self, handle: DaemonHandle, _p: Value) -> Result<Value, super::super::protocol::RpcError> {
        Ok(serde_json::json!({
            "ok": true,
            "phase": format!("{:?}", handle.phase()).to_lowercase(),
            "pid": std::process::id(),
        }))
    }
}
```

Run: `cargo test -p octo-whatsapp --lib ipc::handlers::health 2>&1 | tail -3` — expect PASS.

Commit:
```bash
git add crates/octo-whatsapp/src/ipc/handlers/health.rs
git commit -m "feat(octo-whatsapp): add health.get handler"
```

## Task 24: `send.text` handler — 65,536-byte ceiling

This is the load-bearing test of the design. The ceiling is enforced **pre-flight** — we never contact WhatsApp with over-size text.

**Files:**
- Modify: `crates/octo-whatsapp/src/ipc/handlers/send_text.rs`

```rust
use serde::Deserialize;
use serde_json::Value;

use super::super::protocol::{RpcError, RpcErrorCode};
use super::super::server::RpcHandler;
use crate::daemon::DaemonHandle;

/// Maximum raw text payload size (inclusive), per RFC-0850 §8.6.
pub const MAX_TEXT_BYTES: usize = 65_536;

#[derive(Deserialize)]
struct Params {
    peer: String,
    text: String,
    #[serde(default)]
    reply_to: Option<String>,
    #[serde(default)]
    mentions: Vec<String>,
}

pub struct SendText;

#[async_trait::async_trait]
impl RpcHandler for SendText {
    fn name(&self) -> &'static str { "send.text" }

    async fn call(&self, _h: DaemonHandle, params: Value) -> Result<Value, RpcError> {
        let p: Params = serde_json::from_value(params).map_err(|e| RpcError {
            code: RpcErrorCode::InvalidParams.as_i32(),
            message: format!("invalid params: {e}"),
            data: None,
        })?;

        let bytes = p.text.len(); // byte length, not char count (ASCII-only path)
        if bytes > MAX_TEXT_BYTES {
            return Err(RpcError {
                code: RpcErrorCode::PayloadTooLarge.as_i32(),
                message: format!(
                    "text payload is {bytes} bytes; ceiling is {MAX_TEXT_BYTES}; use send.doc for larger payloads"
                ),
                data: Some(serde_json::json!({
                    "size_bytes": bytes,
                    "max_bytes": MAX_TEXT_BYTES,
                    "hint": "use send.doc",
                })),
            });
        }

        // Phase 1: validate peer, do not actually send. The actual call into
        // CoordinatorAdmin happens in Task 33.
        let _jid = crate::jids::peer_to_jid(&p.peer).map_err(|e| RpcError {
            code: RpcErrorCode::InvalidParams.as_i32(),
            message: format!("invalid peer: {e}"),
            data: Some(serde_json::json!({"expected_format": "E.164 or <digits>@s.whatsapp.net or <digits>@lid"})),
        })?;

        // Defer real send to Phase 2 (adapter is not yet wired in Phase 1).
        Ok(serde_json::json!({
            "status": "queued_for_phase2",
            "peer": p.peer,
            "size_bytes": bytes,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::WhatsAppRuntimeConfig;
    use crate::daemon::Daemon;

    fn handle() -> DaemonHandle {
        let cfg = WhatsAppRuntimeConfig::from_toml(br#"name = "x""#).unwrap();
        Daemon::new(cfg).handle()
    }

    #[tokio::test]
    async fn accepts_exactly_65536() {
        let text = "a".repeat(MAX_TEXT_BYTES);
        let v = SendText
            .call(handle(), serde_json::json!({"peer": "+15551234567", "text": text}))
            .await
            .unwrap();
        assert_eq!(v["status"], "queued_for_phase2");
        assert_eq!(v["size_bytes"], MAX_TEXT_BYTES);
    }

    #[tokio::test]
    async fn rejects_65537() {
        let text = "a".repeat(MAX_TEXT_BYTES + 1);
        let err = SendText
            .call(handle(), serde_json::json!({"peer": "+15551234567", "text": text}))
            .await
            .unwrap_err();
        assert_eq!(err.code, -32004);
        assert_eq!(err.data.unwrap()["max_bytes"], MAX_TEXT_BYTES);
    }

    #[tokio::test]
    async fn rejects_invalid_peer() {
        let err = SendText
            .call(handle(), serde_json::json!({"peer": "not-a-peer", "text": "hi"}))
            .await
            .unwrap_err();
        assert_eq!(err.code, -32602);
    }
}
```

Run: `cargo test -p octo-whatsapp --lib ipc::handlers::send_text 2>&1 | tail -3` — expect 3 tests PASS.

Commit:
```bash
git add crates/octo-whatsapp/src/ipc/handlers/send_text.rs
git commit -m "feat(octo-whatsapp): add send.text handler with 65,536-byte ceiling"
```

## Task 25: `groups.*` handler stubs (4 methods)

**Files:**
- Modify: `crates/octo-whatsapp/src/ipc/handlers/groups.rs`

```rust
use serde_json::Value;

use super::super::protocol::{RpcError, RpcErrorCode};
use super::super::server::RpcHandler;
use crate::daemon::DaemonHandle;

pub struct GroupsCreate;
pub struct GroupsList;
pub struct GroupsInfo;
pub struct GroupsLeave;

#[async_trait::async_trait]
impl RpcHandler for GroupsCreate {
    fn name(&self) -> &'static str { "groups.create" }
    async fn call(&self, h: DaemonHandle, p: Value) -> Result<Value, RpcError> {
        // Phase 1 stub: parse subject + members, validate, defer real create.
        let _ = (h, p);
        Err(RpcError {
            code: RpcErrorCode::NotConnected.as_i32(),
            message: "adapter not wired in Phase 1; will be implemented in Phase 2".to_string(),
            data: None,
        })
    }
}

#[async_trait::async_trait]
impl RpcHandler for GroupsList {
    fn name(&self) -> &'static str { "groups.list" }
    async fn call(&self, h: DaemonHandle, p: Value) -> Result<Value, RpcError> {
        let _ = (h, p);
        Err(RpcError {
            code: RpcErrorCode::NotConnected.as_i32(),
            message: "adapter not wired in Phase 1".to_string(),
            data: None,
        })
    }
}

#[async_trait::async_trait]
impl RpcHandler for GroupsInfo {
    fn name(&self) -> &'static str { "groups.info" }
    async fn call(&self, h: DaemonHandle, p: Value) -> Result<Value, RpcError> {
        let _ = (h, p);
        Err(RpcError {
            code: RpcErrorCode::NotConnected.as_i32(),
            message: "adapter not wired in Phase 1".to_string(),
            data: None,
        })
    }
}

#[async_trait::async_trait]
impl RpcHandler for GroupsLeave {
    fn name(&self) -> &'static str { "groups.leave" }
    async fn call(&self, h: DaemonHandle, p: Value) -> Result<Value, RpcError> {
        let _ = (h, p);
        Err(RpcError {
            code: RpcErrorCode::NotConnected.as_i32(),
            message: "adapter not wired in Phase 1".to_string(),
            data: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::WhatsAppRuntimeConfig;
    use crate::daemon::Daemon;

    fn handle() -> DaemonHandle {
        let cfg = WhatsAppRuntimeConfig::from_toml(br#"name = "x""#).unwrap();
        Daemon::new(cfg).handle()
    }

    #[tokio::test]
    async fn groups_create_returns_not_connected_in_phase1() {
        let err = GroupsCreate
            .call(handle(), serde_json::json!({"subject":"ops","members":["+15551234567"]}))
            .await
            .unwrap_err();
        assert_eq!(err.code, -32012);
    }

    #[tokio::test]
    async fn groups_list_returns_not_connected_in_phase1() {
        let err = GroupsList
            .call(handle(), Value::Null)
            .await
            .unwrap_err();
        assert_eq!(err.code, -32012);
    }
}
```

Run: `cargo test -p octo-whatsapp --lib ipc::handlers::groups 2>&1 | tail -3` — expect PASS.

Commit:
```bash
git add crates/octo-whatsapp/src/ipc/handlers/groups.rs
git commit -m "feat(octo-whatsapp): add groups.* stubs (Phase 1 NotConnected)"
```

## Task 26: `messages.list` handler

`messages.list` is the FIRST RPC that touches stoolap. The Phase 1 path uses `list_persisted_conversations` from the adapter.

**Files:**
- Modify: `crates/octo-whatsapp/src/ipc/handlers/messages.rs`

```rust
use serde::Deserialize;
use serde_json::Value;

use super::super::protocol::{RpcError, RpcErrorCode};
use super::super::server::RpcHandler;
use crate::daemon::DaemonHandle;

#[derive(Deserialize, Default)]
struct Params {
    #[serde(default)]
    peer: Option<String>,
    #[serde(default)]
    limit: Option<u32>,
}

pub struct MessagesList;

#[async_trait::async_trait]
impl RpcHandler for MessagesList {
    fn name(&self) -> &'static str { "messages.list" }

    async fn call(&self, h: DaemonHandle, params: Value) -> Result<Value, RpcError> {
        let p: Params = serde_json::from_value(params).unwrap_or_default();

        // Phase 1: stub — adapter not wired. Return empty list with limit echoed.
        let _ = h;
        Ok(serde_json::json!({
            "messages": [],
            "limit": p.limit.unwrap_or(50),
            "phase": "phase1",
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::WhatsAppRuntimeConfig;
    use crate::daemon::Daemon;

    #[tokio::test]
    async fn messages_list_returns_empty_in_phase1() {
        let cfg = WhatsAppRuntimeConfig::from_toml(br#"name = "x""#).unwrap();
        let h = Daemon::new(cfg).handle();
        let v = MessagesList
            .call(h, serde_json::json!({"limit": 10}))
            .await
            .unwrap();
        assert!(v["messages"].as_array().unwrap().is_empty());
        assert_eq!(v["limit"], 10);
    }
}
```

Run: `cargo test -p octo-whatsapp --lib ipc::handlers::messages 2>&1 | tail -3` — expect PASS.

Commit:
```bash
git add crates/octo-whatsapp/src/ipc/handlers/messages.rs
git commit -m "feat(octo-whatsapp): add messages.list handler stub"
```

## Task 27: `rules.list`, `rules.get` handlers (read-only)

**Files:**
- Modify: `crates/octo-whatsapp/src/ipc/handlers/rules.rs`

```rust
use serde_json::Value;

use super::super::server::RpcHandler;
use crate::daemon::DaemonHandle;
use crate::rules::RulesView;

pub struct RulesList;
pub struct RulesGet;

#[async_trait::async_trait]
impl RpcHandler for RulesList {
    fn name(&self) -> &'static str { "rules.list" }
    async fn call(&self, _h: DaemonHandle, _p: Value) -> Result<Value, super::super::protocol::RpcError> {
        Ok(serde_json::json!({
            "rules": RulesView::empty().list(),
            "phase": "phase1_readonly",
        }))
    }
}

#[async_trait::async_trait]
impl RpcHandler for RulesGet {
    fn name(&self) -> &'static str { "rules.get" }
    async fn call(&self, _h: DaemonHandle, p: Value) -> Result<Value, super::super::protocol::RpcError> {
        let id = p.get("id").and_then(|v| v.as_str()).unwrap_or("");
        Ok(serde_json::json!({
            "id": id,
            "found": RulesView::empty().get(id).is_some(),
            "phase": "phase1_readonly",
        }))
    }
}
```

Run: `cargo test -p octo-whatsapp --lib ipc::handlers::rules 2>&1 | tail -3` — expect PASS.

Commit:
```bash
git add crates/octo-whatsapp/src/ipc/handlers/rules.rs
git commit -m "feat(octo-whatsapp): add rules.list/get handlers (Phase 1 read-only)"
```

## Task 28: `triggers.list`, `triggers.get` handlers

**Files:**
- Modify: `crates/octo-whatsapp/src/ipc/handlers/triggers.rs`

Mirror `rules.rs` but using `TriggersView`. Stub returns empty list / not-found.

Commit: `git commit -m "feat(octo-whatsapp): add triggers.list/get handlers (Phase 1 read-only)"`

## Task 29: `events.list`, `events.show` handlers

**Files:**
- Modify: `crates/octo-whatsapp/src/ipc/handlers/events.rs`

`events.list` returns `{"events": [], "phase": "phase1_no_tail"}`. `events.show` returns `-32601 MethodNotFound` (it's not part of Phase 1's hard surface — but actually design says `events.show` IS in Phase 1, just no `events.tail`). Re-read: design line 1638 lists `events.list|show` as Phase 1.

`events.show` looks up by id in the (empty) in-memory buffer.

Commit: `git commit -m "feat(octo-whatsapp): add events.list/show handlers"`

## Task 30: `reconnect.now`, `shutdown` handlers

**Files:**
- Modify: `crates/octo-whatsapp/src/ipc/handlers/daemon_ops.rs`

```rust
use serde_json::Value;

use super::super::protocol::{RpcError, RpcErrorCode};
use super::super::server::RpcHandler;
use crate::daemon::{DaemonHandle, DaemonPhase};

pub struct ReconnectNow;
pub struct Shutdown;

#[async_trait::async_trait]
impl RpcHandler for ReconnectNow {
    fn name(&self) -> &'static str { "reconnect.now" }
    async fn call(&self, h: DaemonHandle, _p: Value) -> Result<Value, RpcError> {
        // Phase 1: nothing to reconnect to. Return ok=true, daemon stays in current phase.
        let _ = h;
        Ok(serde_json::json!({"ok": true, "phase": "phase1_no_reconnect"}))
    }
}

#[async_trait::async_trait]
impl RpcHandler for Shutdown {
    fn name(&self) -> &'static str { "shutdown" }
    async fn call(&self, h: DaemonHandle, _p: Value) -> Result<Value, RpcError> {
        h.cancel_token().cancel();
        // Caller (the daemon's supervisor) sees the cancel and exits.
        // We mark phase ShuttingDown so subsequent RPCs return -32099.
        Ok(serde_json::json!({"ok": true}))
    }
}
```

The `DaemonHandle` needs an `async` `set_phase` method — extend `daemon.rs`:

```rust
impl DaemonHandle {
    pub async fn set_phase(&self, p: DaemonPhase) {
        *self.inner.phase.write().await = p;
    }
}
```

Tests for `shutdown` must be careful: cancelling the token is permanent for the lifetime of the handle.

Commit: `git commit -m "feat(octo-whatsapp): add reconnect.now + shutdown handlers"`

## Task 31: Registry builder

**Files:**
- Modify: `crates/octo-whatsapp/src/ipc/handlers/mod.rs`

Add a `build_registry()` function that constructs a `HandlerRegistry` with all 12 handlers registered.

```rust
pub fn build_registry() -> super::server::HandlerRegistry {
    use super::server::HandlerRegistry;
    use std::sync::Arc;

    HandlerRegistry::new()
        .register(Arc::new(version::VersionGet))
        .register(Arc::new(status::StatusGet))
        .register(Arc::new(health::HealthGet))
        .register(Arc::new(send_text::SendText))
        .register(Arc::new(groups::GroupsCreate))
        .register(Arc::new(groups::GroupsList))
        .register(Arc::new(groups::GroupsInfo))
        .register(Arc::new(groups::GroupsLeave))
        .register(Arc::new(messages::MessagesList))
        .register(Arc::new(rules::RulesList))
        .register(Arc::new(rules::RulesGet))
        .register(Arc::new(triggers::TriggersList))
        .register(Arc::new(triggers::TriggersGet))
        .register(Arc::new(events::EventsList))
        .register(Arc::new(events::EventsShow))
        .register(Arc::new(daemon_ops::ReconnectNow))
        .register(Arc::new(daemon_ops::Shutdown))
}
```

Add a test that asserts each name is registered (no unknown-method errors for the 12 phase-1 methods).

Commit: `git commit -m "feat(octo-whatsapp): add build_registry() for Phase 1 surface"`

---

# Part H — Unix socket server (Tasks 32-38)

## Task 32: Bind + listen

**Files:**
- Modify: `crates/octo-whatsapp/src/ipc/server.rs`

```rust
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use nix::sys::socket::{self, Backlog, SockaddrUn};
use nix::unistd;
use tokio::net::UnixListener;
use tracing::{info, warn};

use super::protocol::{RpcParseError, RpcRequest, RpcResponse};
use crate::daemon::DaemonHandle;

pub struct UnixSocketServer {
    pub socket_path: PathBuf,
}

impl UnixSocketServer {
    pub fn bind(path: &Path) -> std::io::Result<Self> {
        if path.exists() {
            std::fs::remove_file(path)?;
        }
        let listener = UnixListener::bind(path)?;
        // Socket file mode 0600.
        let perms = std::fs::Permissions::from_mode(0o600);
        std::fs::set_permissions(path, perms)?;
        info!(socket = ?path, "bound unix socket");
        Ok(Self { socket_path: path.to_path_buf() })
    }

    pub fn listener(&self) -> std::io::Result<UnixListener> {
        UnixListener::bind(&self.socket_path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn bind_creates_socket_file_with_0600() {
        let tmp = TempDir::new().unwrap();
        let sock = tmp.path().join("t.sock");
        let _server = UnixSocketServer::bind(&sock).unwrap();
        let meta = std::fs::metadata(&sock).unwrap();
        assert_eq!(meta.permissions().mode() & 0o777, 0o600);
    }
}
```

Run: `cargo test -p octo-whatsapp --lib ipc::server::tests::bind 2>&1 | tail -3` — expect PASS.

Commit: `git commit -m "feat(octo-whatsapp): add unix socket bind + 0600 perms"`

## Task 33: Accept loop

**Files:**
- Modify: `crates/octo-whatsapp/src/ipc/server.rs`

Add an async `serve` method:

```rust
impl UnixSocketServer {
    pub async fn serve(
        self,
        handle: DaemonHandle,
        registry: Arc<HandlerRegistry>,
        cancel: tokio_util::sync::CancellationToken,
    ) -> anyhow::Result<()> {
        let listener = self.listener()?;
        loop {
            tokio::select! {
                _ = cancel.cancelled() => {
                    info!("unix socket server: cancel observed, exiting");
                    let _ = std::fs::remove_file(&self.socket_path);
                    return Ok(());
                }
                accept = listener.accept() => {
                    let (stream, _addr) = match accept {
                        Ok(p) => p,
                        Err(e) => {
                            warn!(error = %e, "accept failed");
                            continue;
                        }
                    };
                    let h = handle.clone();
                    let r = registry.clone();
                    tokio::spawn(async move {
                        if let Err(e) = handle_conn(stream, h, r).await {
                            warn!(error = %e, "connection handler error");
                        }
                    });
                }
            }
        }
    }
}

async fn handle_conn(
    mut stream: tokio::net::UnixStream,
    handle: DaemonHandle,
    registry: Arc<HandlerRegistry>,
) -> anyhow::Result<()> {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    let (read_half, mut write_half) = stream.split();
    let mut reader = BufReader::new(read_half);
    let mut line = String::new();
    loop {
        line.clear();
        let n = reader.read_line(&mut line).await?;
        if n == 0 {
            return Ok(()); // EOF
        }
        let req = match RpcRequest::from_json(line.as_bytes()) {
            Ok(r) => r,
            Err(e) => {
                let resp = RpcResponse {
                    id: 0,
                    result: None,
                    error: Some(super::protocol::RpcError {
                        code: super::protocol::RpcErrorCode::ParseError.as_i32(),
                        message: format!("parse error: {e}"),
                        data: None,
                    }),
                };
                let mut s = serde_json::to_string(&resp)?;
                s.push('\n');
                write_half.write_all(s.as_bytes()).await?;
                continue;
            }
        };
        let resp = registry.dispatch(handle.clone(), req).await;
        let mut s = serde_json::to_string(&resp)?;
        s.push('\n');
        write_half.write_all(s.as_bytes()).await?;
    }
}
```

This is a stub — no per-conn idle timeout yet (Task 36). It works for the happy path.

Run: `cargo check -p octo-whatsapp --all-targets 2>&1 | tail -3` — expect clean.

Commit: `git commit -m "feat(octo-whatsapp): add unix socket accept loop + line-delimited dispatch"`

## Task 34: End-to-end integration test

**Files:**
- Create: `crates/octo-whatsapp/tests/it_ipc_roundtrip.rs`

```rust
use std::os::unix::net::UnixStream as StdUnixStream;
use std::io::{Read, Write};

use octo_whatsapp::config::WhatsAppRuntimeConfig;
use octo_whatsapp::daemon::Daemon;
use octo_whatsapp::ipc::handlers::build_registry;
use octo_whatsapp::ipc::server::{HandlerRegistry, UnixSocketServer};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ipc_roundtrip_via_unix_socket() {
    let tmp = tempfile::tempdir().unwrap();
    let sock = tmp.path().join("octo-whatsapp-test.sock");

    let cfg = WhatsAppRuntimeConfig::from_toml(br#"name = "x""#).unwrap();
    let daemon = Daemon::new(cfg);
    let handle = daemon.handle();
    let cancel = daemon.cancel_token_clone();
    let registry = std::sync::Arc::new(build_registry());

    let _server = UnixSocketServer::bind(&sock).unwrap();
    let server_path = sock.clone();
    let server_cancel = cancel.clone();
    let server_handle = handle.clone();
    let server_registry = registry.clone();
    let server_task = tokio::spawn(async move {
        UnixSocketServer {
            socket_path: server_path,
        }
        .serve(server_handle, server_registry, server_cancel)
        .await
    });

    // Give the server a moment to bind.
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    // Connect from another thread (std UnixStream is blocking; run in spawn_blocking).
    let resp_json = tokio::task::spawn_blocking(move || {
        let mut s = StdUnixStream::connect(&sock).unwrap();
        let req = serde_json::json!({"id": 1, "method": "version.get"});
        let mut line = serde_json::to_string(&req).unwrap();
        line.push('\n');
        s.write_all(line.as_bytes()).unwrap();
        let mut buf = String::new();
        s.read_to_string(&mut buf).unwrap();
        buf
    })
    .await
    .unwrap();

    let resp: serde_json::Value = serde_json::from_str(resp_json.trim()).unwrap();
    assert_eq!(resp["id"], 1);
    assert_eq!(resp["result"]["daemon_api_version"], "1.0.0+phase1");

    cancel.cancel();
    server_task.await.unwrap().unwrap();
}
```

You'll need to add a `cancel_token_clone()` accessor on `Daemon` (or make `cancel` public).

Run: `cargo test -p octo-whatsapp --test it_ipc_roundtrip 2>&1 | tail -10` — expect PASS.

Commit: `git commit -m "test(octo-whatsapp): add it_ipc_roundtrip hermetic e2e"`

---

# Part I — Stoolap uniqueness invariant (Task 35)

## Task 35: Grep invariant test

**Files:**
- Create: `crates/octo-whatsapp/tests/it_stoolap_uniqueness.rs`

```rust
//! Invariant: the runtime crate MUST NOT directly depend on stoolap.
//! All stoolap access goes via `Arc<StoolapStore>` cloned from
//! `octo-adapter-whatsapp` at startup. This test enforces that by greping
//! the source tree for forbidden patterns.

use std::fs;
use std::path::Path;

#[test]
fn no_direct_stoolap_dependency() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut bad = Vec::new();
    for entry in walkdir(&src) {
        if entry.ends_with(".rs") {
            let content = fs::read_to_string(&entry).unwrap();
            for (lineno, line) in content.lines().enumerate() {
                if line.contains("stoolap") && !line.trim_start().starts_with("//") {
                    bad.push(format!("{}:{}: {}", entry.display(), lineno + 1, line));
                }
            }
        }
    }
    assert!(
        bad.is_empty(),
        "octo-whatsapp src/ must not mention 'stoolap' directly; offenders:\n{}",
        bad.join("\n"),
    );
}

fn walkdir(p: &Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    if p.is_dir() {
        for entry in fs::read_dir(p).unwrap() {
            let e = entry.unwrap().path();
            if e.is_dir() {
                out.extend(walkdir(&e));
            } else if e.extension().map(|x| x == "rs").unwrap_or(false) {
                out.push(e);
            }
        }
    }
    out
}
```

Run: `cargo test -p octo-whatsapp --test it_stoolap_uniqueness 2>&1 | tail -5` — expect PASS (we haven't imported stoolap).

Commit: `git commit -m "test(octo-whatsapp): add stoolap uniqueness invariant test"`

---

# Part J — Daemon boot integration (Tasks 36-40)

## Task 36: Wire the adapter into the daemon

**Files:**
- Modify: `crates/octo-whatsapp/src/daemon.rs`

Read `crates/octo-adapter-whatsapp/src/lib.rs` to find how to construct `WhatsAppWebAdapter`. Replace `Daemon::run`:

```rust
impl Daemon {
    pub async fn run(self) -> anyhow::Result<()> {
        info!(name = self.config.name.as_str(), "daemon: starting");

        // Phase 1 stub: skip adapter boot entirely. The supervisor pattern
        // arrives in Phase 2; for now we just bind the socket and exit on cancel.
        let cancel = self.cancel.clone();
        let handle = self.handle();

        // Server task (Phase 1: trivial, no methods actually wired yet
        // beyond what `build_registry()` covers, all returning NotConnected).
        let registry = std::sync::Arc::new(crate::ipc::handlers::build_registry());
        let sock = self.config.socket_path();
        let server = crate::ipc::server::UnixSocketServer::bind(&sock)?;
        let server_task = {
            let cancel = cancel.clone();
            let handle = handle.clone();
            tokio::spawn(async move { server.serve(handle, registry, cancel).await })
        };

        // Block on cancel.
        cancel.cancelled().await;
        info!("daemon: cancel observed; waiting for server to drain");

        let _ = server_task.await;
        info!("daemon: exited");
        Ok(())
    }
}
```

Run: `cargo check -p octo-whatsapp --all-targets 2>&1 | tail -3` — expect clean.

Commit: `git commit -m "feat(octo-whatsapp): wire socket server into Daemon::run"`

## Task 37: Daemon liveness integration test

**Files:**
- Create: `crates/octo-whatsapp/tests/it_bot_liveness.rs`

Hermetic test: start the daemon in a temp socket dir, hit it with `health.get`, then `shutdown`, assert clean exit.

```rust
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::time::Duration;

use octo_whatsapp::config::WhatsAppRuntimeConfig;
use octo_whatsapp::daemon::Daemon;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn daemon_starts_responds_and_shuts_down() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg = WhatsAppRuntimeConfig {
        name: "test".to_string(),
        data_dir: tmp.path().join("data"),
        log_dir: tmp.path().join("log"),
        socket_dir: tmp.path().to_path_buf(),
    };
    cfg.validate().unwrap();
    std::fs::create_dir_all(cfg.data_dir.clone()).unwrap();
    std::fs::create_dir_all(cfg.log_dir.clone()).unwrap();

    let daemon = Daemon::new(cfg.clone());
    let cancel = daemon.cancel_token_clone();
    let daemon_task = tokio::spawn(async move { daemon.run().await });

    // Wait for socket to appear.
    let sock = cfg.socket_path();
    for _ in 0..50 {
        if sock.exists() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(sock.exists(), "socket file was never created");

    // Send health.get.
    let resp_json = tokio::task::spawn_blocking({
        let sock = sock.clone();
        move || {
            let mut s = UnixStream::connect(&sock).unwrap();
            let req = serde_json::json!({"id": 1, "method": "health.get"});
            let mut line = serde_json::to_string(&req).unwrap();
            line.push('\n');
            s.write_all(line.as_bytes()).unwrap();
            let mut buf = String::new();
            s.read_to_string(&mut buf).unwrap();
            buf
        }
    })
    .await
    .unwrap();

    let resp: serde_json::Value = serde_json::from_str(resp_json.trim()).unwrap();
    assert_eq!(resp["id"], 1);
    assert_eq!(resp["result"]["ok"], true);

    // Shutdown.
    cancel.cancel();
    daemon_task.await.unwrap().unwrap();
    assert!(!sock.exists(), "socket file should be removed on shutdown");
}
```

Run: `cargo test -p octo-whatsapp --test it_bot_liveness 2>&1 | tail -10` — expect PASS.

Commit: `git commit -m "test(octo-whatsapp): add daemon liveness integration test"`

## Task 38: `send.text` ceiling integration test

**Files:**
- Create: `crates/octo-whatsapp/tests/it_send_text_ceiling.rs`

Send a 65,536-byte text via the running daemon, assert ok. Send a 65,537-byte text, assert `-32004 PayloadTooLarge` with no WhatsApp contact (we'd need a mock to verify the no-contact part — for Phase 1, just check the code).

Commit: `git commit -m "test(octo-whatsapp): add send.text ceiling integration test"`

---

# Part K — CLI (Tasks 39-50)

## Task 39: CLI scaffolding with clap derive

**Files:**
- Create: `crates/octo-whatsapp/src/cli.rs`
- Modify: `crates/octo-whatsapp/src/lib.rs` (add `pub mod cli;`)
- Modify: `crates/octo-whatsapp/src/main.rs`

**Step 1:** Create `cli.rs`:

```rust
//! CLI for `octo-whatsapp`. Subcommand tree mirrors the RPC surface.

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "octo-whatsapp", version, about = "WhatsApp runtime + CLI + MCP")]
pub struct Cli {
    /// Daemon socket path. Defaults to $XDG_RUNTIME_DIR/octo-whatsapp-{name}.sock.
    #[arg(long, global = true)]
    pub socket: Option<PathBuf>,

    /// Daemon name (multi-instance). Default: "default".
    #[arg(long, global = true, default_value = "default")]
    pub name: String,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Run as a long-lived daemon (the default for `systemd`).
    Daemon,
    /// Run as an MCP server over stdio (JSON-RPC 2.0).
    Mcp,
    /// Print version info.
    Version,
    /// Print daemon status (boot/connected/session-lost/etc).
    Status,
    /// Print daemon health.
    Health,
    /// Send a text message.
    Send(SendArgs),
    /// Group operations.
    Groups(GroupsCmd),
    /// Message operations.
    Messages(MessagesCmd),
    /// Rule operations (Phase 1: read-only).
    Rules(RulesCmd),
    /// Trigger operations (Phase 1: read-only).
    Triggers(TriggersCmd),
    /// Event operations (Phase 1: list/show only).
    Events(EventsCmd),
    /// Force a reconnect of the underlying WebSocket.
    Reconnect,
    /// Gracefully shut down the daemon.
    Shutdown,
    /// Onboarding passthrough (delegates to octo-whatsapp-onboard-core).
    Onboard(OnboardCmd),
}

#[derive(Debug, Args)]
pub struct SendArgs {
    #[command(subcommand)]
    pub kind: SendKind,
}

#[derive(Debug, Subcommand)]
pub enum SendKind {
    Text {
        peer: String,
        #[arg(long)]
        text: String,
    },
}

#[derive(Debug, Args)]
pub struct GroupsCmd {
    #[command(subcommand)]
    pub action: GroupsAction,
}

#[derive(Debug, Subcommand)]
pub enum GroupsAction {
    Create { #[arg(long)] subject: String, #[arg(long)] members: Vec<String> },
    List,
    Info { jid: String },
    Leave { jid: String },
}

#[derive(Debug, Args)]
pub struct MessagesCmd {
    #[command(subcommand)]
    pub action: MessagesAction,
}

#[derive(Debug, Subcommand)]
pub enum MessagesAction {
    List {
        #[arg(long)] peer: Option<String>,
        #[arg(long)] limit: Option<u32>,
    },
}

#[derive(Debug, Args)]
pub struct RulesCmd {
    #[command(subcommand)]
    pub action: RulesAction,
}

#[derive(Debug, Subcommand)]
pub enum RulesAction {
    List,
    Get { id: String },
}

#[derive(Debug, Args)]
pub struct TriggersCmd {
    #[command(subcommand)]
    pub action: TriggersAction,
}

#[derive(Debug, Subcommand)]
pub enum TriggersAction {
    List,
    Get { id: String },
}

#[derive(Debug, Args)]
pub struct EventsCmd {
    #[command(subcommand)]
    pub action: EventsAction,
}

#[derive(Debug, Subcommand)]
pub enum EventsAction {
    List,
    Show { id: String },
}

#[derive(Debug, Args)]
pub struct OnboardCmd {
    #[command(subcommand)]
    pub action: OnboardAction,
}

#[derive(Debug, Subcommand)]
pub enum OnboardAction {
    QrLink { #[arg(long, default_value_t = 120)] timeout: u64 },
    PairLink { phone: String },
    Whoami,
    Session { #[command(subcommand)] action: SessionCmd },
}

#[derive(Debug, Subcommand)]
pub enum SessionCmd {
    List,
    Verify { name: String },
    Remove { name: String },
}
```

**Step 2:** Modify `main.rs`:

```rust
use clap::Parser;
use octo_whatsapp::cli::{Cli, Command};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Daemon => octo_whatsapp::daemon::run_daemon(&cli).await,
        Command::Version => octo_whatsapp::cli::print_version().await,
        // Other commands: stub for now.
        other => {
            eprintln!("octo-whatsapp: command {:?} not yet wired in Phase 1", other);
            std::process::exit(2);
        }
    }
}
```

Run: `cargo run --bin octo-whatsapp -- --help 2>&1 | tail -20` — expect help text.

Commit: `git commit -m "feat(octo-whatsapp): add CLI subcommand tree (Phase 1)"`

## Task 40: CLI → RPC client

**Files:**
- Modify: `crates/octo-whatsapp/src/cli.rs`

Add a `run_rpc(cli, method, params) -> anyhow::Result<Value>` helper that:
1. Resolves `--socket` or derives from `--name`.
2. Connects via `std::os::unix::net::UnixStream`.
3. Writes `{"id":1,"method":M,"params":P}\n`.
4. Reads one line, parses, returns `result` or errors with `error.message`.

Implement this in a test against the running daemon (re-use the boot logic from Task 37).

Commit: `git commit -m "feat(octo-whatsapp): add CLI→RPC client helper"`

## Tasks 41-50: Wire each leaf command

For each of the 12 RPC methods, wire the CLI subcommand to call `run_rpc` and pretty-print the result. One task per top-level command:
- Task 41: `version`, `status`, `health`
- Task 42: `send text`
- Task 43: `groups create|list|info|leave`
- Task 44: `messages list`
- Task 45: `rules list|get`
- Task 46: `triggers list|get`
- Task 47: `events list|show`
- Task 48: `reconnect`, `shutdown`
- Task 49: `onboard` passthrough (qr-link, pair-link, whoami, session *)
- Task 50: `--json` flag for machine-readable output

Each task: 1 test (assert_cmd-style binary invocation against a spawned daemon), 1 commit.

---

# Part L — MCP server (Tasks 51-58)

## Task 51: MCP server scaffolding

**Files:**
- Create: `crates/octo-whatsapp/src/mcp_server.rs`

Phase 1 MCP is a thin proxy: receive JSON-RPC on stdin, forward to daemon socket, write response on stdout. No `rmcp` SDK yet (that's Phase 2+).

```rust
//! MCP server (stdio JSON-RPC). Phase 1: thin proxy to the daemon.

use std::io::{self, BufRead, Write};
use std::path::Path;

use serde_json::Value;

pub async fn serve(socket: &Path) -> anyhow::Result<()> {
    let stdin = io::stdin();
    let mut stdout = io::stdout();
    let mut reader = stdin.lock();
    let mut line = String::new();
    loop {
        line.clear();
        let n = reader.read_line(&mut line)?;
        if n == 0 {
            return Ok(());
        }
        let req: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(e) => {
                let err = serde_json::json!({"jsonrpc":"2.0","id":null,"error":{"code":-32700,"message":format!("parse: {e}")}});
                writeln!(stdout, "{}", err)?;
                stdout.flush()?;
                continue;
            }
        };
        let method = req.get("method").and_then(|v| v.as_str()).unwrap_or("");
        // Translate MCP methods to daemon RPC.
        let daemon_method = match method {
            "initialize" => continue_with_init(&mut stdout, &req).await?,
            "tools/list" => "version.get".to_string(), // stub: Phase 1 MCP exposes only the version method
            "tools/call" => req.get("params")
                .and_then(|p| p.get("name"))
                .and_then(|n| n.as_str())
                .unwrap_or("")
                .to_string(),
            "ping" => "health.get".to_string(),
            other => {
                let err = serde_json::json!({"jsonrpc":"2.0","id":req.get("id").cloned().unwrap_or(Value::Null),"error":{"code":-32601,"message":format!("method {:?} not implemented in Phase 1", other)}});
                writeln!(stdout, "{}", err)?;
                stdout.flush()?;
                continue;
            }
        };
        // Forward to daemon.
        let params = req.get("params").cloned().unwrap_or(Value::Null);
        let daemon_resp = forward_to_daemon(socket, &daemon_method, params).await?;
        let mcp_resp = serde_json::json!({
            "jsonrpc": "2.0",
            "id": req.get("id").cloned().unwrap_or(Value::Null),
            "result": daemon_resp,
        });
        writeln!(stdout, "{}", mcp_resp)?;
        stdout.flush()?;
    }
}

async fn continue_with_init(stdout: &mut io::StdoutLock<'_>, req: &Value) -> anyhow::Result<String> {
    let resp = serde_json::json!({
        "jsonrpc": "2.0",
        "id": req.get("id").cloned().unwrap_or(Value::Null),
        "result": {
            "protocolVersion": "2025-06-18",
            "serverInfo": {"name": "octo-whatsapp", "version": env!("CARGO_PKG_VERSION")},
            "capabilities": {"tools": {}},
        },
    });
    writeln!(stdout, "{}", resp)?;
    stdout.flush()?;
    Ok(String::new()) // empty method, no further dispatch
}

async fn forward_to_daemon(socket: &Path, method: &str, params: Value) -> anyhow::Result<Value> {
    use std::io::{Read, Write};
    use std::os::unix::net::UnixStream;
    let mut s = UnixStream::connect(socket)?;
    let req = serde_json::json!({"id": 1, "method": method, "params": params});
    let mut line = serde_json::to_string(&req)?;
    line.push('\n');
    s.write_all(line.as_bytes())?;
    let mut buf = String::new();
    s.read_to_string(&mut buf)?;
    let resp: Value = serde_json::from_str(buf.trim())?;
    Ok(resp.get("result").cloned().unwrap_or(Value::Null))
}
```

**Step 2:** Wire `mcp` into `cli.rs` (replace the `Command::Mcp` stub in `main.rs`).

Commit: `git commit -m "feat(octo-whatsapp): add MCP stdio server (Phase 1 thin proxy)"`

## Task 52: MCP initialize test

**Files:**
- Create: `crates/octo-whatsapp/tests/it_mcp_initialize.rs`

Spawn the daemon in a thread, then spawn `octo-whatsapp mcp --socket …`, pipe in an `initialize` request, assert the response carries `protocolVersion: "2025-06-18"`.

Commit: `git commit -m "test(octo-whatsapp): add MCP initialize handshake test"`

## Task 53: MCP tools/call → daemon ping

**Files:**
- Create: `crates/octo-whatsapp/tests/it_mcp_ping.rs`

Send `tools/call` with name=`ping`, assert daemon's `health.get` was invoked (look at result shape).

Commit: `git commit -m "test(octo-whatsapp): add MCP tools/call ping test"`

---

# Part M — End-to-end daemon integration tests (Tasks 54-58)

## Task 54: Stub adapter for hermetic tests

Create a `StubAdapter` that satisfies `PlatformAdapter` (the trait from `octo-network`) but returns canned responses. This lets us test the daemon's RPC methods without a real WhatsApp connection.

Actually — Phase 1's RPC methods all return `NotConnected` for adapter-touching methods. So we don't strictly need a stub adapter yet. Skip this task; revisit in Phase 2.

## Task 55: Multi-RPC sequence test

`tests/it_multi_rpc_sequence.rs`: connect, send `version.get` → `health.get` → `shutdown`, assert all three return correctly, daemon exits.

Commit: `git commit -m "test(octo-whatsapp): add multi-RPC sequence integration test"`

## Task 56: Malformed input handling

`tests/it_malformed_input.rs`: send `{"id":"not-an-int","method":"x"}\n`, assert `-32700 ParseError`. Send `{"id":1}\n` (missing method), assert same.

Commit: `git commit -m "test(octo-whatsapp): add malformed-input handling tests"`

## Task 57: Unknown method handling

`tests/it_unknown_method.rs`: send `{"id":1,"method":"some.future.method"}\n`, assert `-32601` with `data.api_version` and `data.available_in` populated.

Commit: `git commit -m "test(octo-whatsapp): add unknown-method handling test"`

## Task 58: Concurrent client test

`tests/it_concurrent_clients.rs`: spawn 8 tokio tasks, each opens its own connection and fires 5 requests in sequence. Assert all 40 responses arrive correctly.

Commit: `git commit -m "test(octo-whatsapp): add concurrent-clients stress test"`

---

# Part N — CLI integration tests (Tasks 59-62)

## Task 59: assert_cmd smoke test for `version` CLI

`crates/octo-whatsapp/tests/cli_version.rs`: spawn the daemon, then invoke `octo-whatsapp version --socket …` via `assert_cmd`, assert stdout contains `"daemon_api_version": "1.0.0+phase1"`.

Commit: `git commit -m "test(octo-whatsapp): add CLI version assert_cmd smoke test"`

## Task 60: CLI `status` smoke test

Same pattern for `status`.

Commit: `git commit -m "test(octo-whatsapp): add CLI status assert_cmd smoke test"`

## Task 61: CLI `onboard qr-link --help` smoke test

The `onboard` subcommand must NOT require a daemon (it's standalone, by design — see the design's "Onboarding passthrough" section). Test that `octo-whatsapp onboard qr-link --help` works without a running daemon.

Commit: `git commit -m "test(octo-whatsapp): add CLI onboard standalone smoke test"`

## Task 62: CLI unknown subcommand

`octo-whatsapp nope` should exit non-zero with a clear clap error message. Assert exit code 2.

Commit: `git commit -m "test(octo-whatsapp): add CLI unknown-subcommand error test"`

---

# Part O — Documentation + CI (Tasks 63-67)

## Task 63: Tag daemon.api.version

Bump `daemon_api_version` in `handlers/version.rs` to `"1.0.0+phase1"` (already there). Add a `docs/CHANGELOG.md` entry under the daemon runtime section.

Commit: `git commit -m "docs(octo-whatsapp): tag Phase 1 daemon.api.version"`

## Task 64: Update CLAUDE.md / workspace overview

Edit the root `CLAUDE.md` "Planned Modules" section to remove "Assistant Core / Agent Runtime / Local Inference Engine / Secure Execution Sandbox / Node Identity System / Hybrid Blockchain Coordination / Developer SDK / Deployment Toolkit" — replace with the actual implemented runtime status (this is too speculative for a Phase 1 commit; defer or just update the "current focus" line).

Actually, leave `CLAUDE.md` alone — it documents the Ocean Stack. The crate listing in the root `Cargo.toml` already reflects reality.

Instead: add a `crates/octo-whatsapp/README.md` describing the Phase 1 surface.

Commit: `git commit -m "docs(octo-whatsapp): add crate README"`

## Task 65: Add CI workflow entries

**Files:**
- Modify: `.github/workflows/ci.yml`

Append (per design §CI Integration, line 1079):

```yaml
  whatsapp-cli-check:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - run: cargo check -p octo-cli-meta --features whatsapp-cli

  whatsapp-cli-clippy:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: clippy
      - run: cargo clippy -p octo-cli-meta --features whatsapp-cli --all-targets -- -D warnings

  whatsapp-cli-tests:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - run: cargo test -p octo-cli-meta --features whatsapp-cli
```

Commit: `git commit -m "ci(octo-whatsapp): add CI workflow entries for whatsapp-cli"`

## Task 66: Full clippy + format gate

Run from worktree root:

```bash
cargo fmt -- crates/octo-whatsapp/
cargo clippy -p octo-whatsapp --all-targets -- -D warnings
cargo test -p octo-whatsapp --lib
cargo test -p octo-whatsapp --tests
```

Fix any warnings. Commit fixes if needed.

## Task 67: Coverage gate (best-effort)

```bash
cargo llvm-cov -p octo-whatsapp --html
```

Open the report; identify the lowest-coverage module and add tests to push overall line coverage ≥ 85%.

Commit: `git commit -m "test(octo-whatsapp): raise coverage to meet Phase 1 gate"`

---

# Part P — Final phase review + tag (Tasks 68-70)

## Task 68: Update design doc's status section

**Files:**
- Modify: `docs/plans/2026-07-04-whatsapp-runtime-cli-mcp-design.md`

Update the line 4 status from `Approved (post-brainstorm)` to `Implemented — Phase 1 complete`. Add a note linking to the implementation commit.

Commit: `git commit -m "docs(design): mark WhatsApp runtime Phase 1 as implemented"`

## Task 69: Pre-merge verification

From the worktree root:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings 2>&1 | tail -5
cargo test -p octo-whatsapp --all-features 2>&1 | tail -10
cargo test -p octo-cli-meta --features whatsapp-cli 2>&1 | tail -10
```

All must pass. Address any failures before continuing.

## Task 70: Create PR to `next`

Branch is `feat/whatsapp-runtime-cli-mcp`. Open a PR to `next` (NOT `main` — per CLAUDE.md, only `next` accepts feature streams).

Run: `gh pr create --base next --title "feat(octo-whatsapp): Phase 1 MVP — daemon + CLI + MCP" --body "..."`

**STOP HERE for user review before merging.**

---

# Appendix — Phase 1 acceptance criteria

The following checklist is the gate between Phase 1 and Phase 2:

- [ ] `cargo check -p octo-cli-meta --features whatsapp-cli` clean
- [ ] `cargo clippy -p octo-cli-meta --features whatsapp-cli --all-targets -- -D warnings` clean
- [ ] `cargo test -p octo-whatsapp` green (all unit + integration tests pass)
- [ ] `cargo test -p octo-cli-meta --features whatsapp-cli` green
- [ ] `daemon.api.version == "1.0.0+phase1"` exposed via `version.get` and `version` CLI
- [ ] 12 RPC methods (per design §Rollout) respond correctly; non-Phase-1 methods return `-32601 MethodNotFound` with `data.api_version = "1.0.0+phase1"` and `data.available_in = "phaseN"`
- [ ] `send.text` enforces 65,536-byte ceiling pre-flight, returns `-32004` for over-size text, never contacts WhatsApp
- [ ] `octo-whatsapp onboard qr-link|pair-link|...` works WITHOUT a running daemon
- [ ] `octo-whatsapp status --socket ...` works WITH a running daemon
- [ ] Stoolap uniqueness invariant test passes (no direct stoolap dep)
- [ ] MCP `initialize` handshake returns `protocolVersion: "2025-06-18"`
- [ ] CLI mirror for all 12 RPC methods
- [ ] Socket file mode `0600` enforced
- [ ] Daemon removes its socket file on shutdown

Phase 2 begins when all boxes are checked and the user approves the PR.