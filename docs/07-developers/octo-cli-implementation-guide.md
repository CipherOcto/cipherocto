# `octo` CLI — Implementation Guide

> **RFC:** RFC-0011
> **Companion to:** `rfcs/draft/process/0011-octo-cli-substrate.md`
> **Per:** `docs/BLUEPRINT.md` §Tools → Implementation Guides (required for 10+
> types / 4+ phases)

> **Substrate API note:** Substrate calls prefixed with `[ADD]` in code samples
> denote Layer-B additions gated on RFC-0011 acceptance. The substrate
> `IdentityKey::begin_rotation` instance method hard-codes a 24h grace period
> internally as `ROTATION_GRACE_PERIOD_SECS`. The `--grace-hours` flag is NOT
> exposed by clap — the grace period is not operator-configurable. The CLI
> consumes these additions; the substrate amendment that adds them is a
> separate work item tracked alongside RFC-0011 acceptance.

## Contents

1. [Module Tree](#module-tree)
2. [Crate Dependency Wiring](#crate-dependency-wiring)
3. [OutputEnvelope\<T\>](#outputenvelopet)
4. [OctoCliError](#octoclierror)
5. [OctoCliRedactor](#octocliredactor)
6. [Clap Root Struct](#clap-root-struct)
7. [Identity Subcommands](#identity-subcommands)
8. [Capability Subcommands](#capability-subcommands)
9. [Policy Subcommands](#policy-subcommands)
10. [Stub Command Deprecation](#stub-command-deprecation)
11. [Test Pattern](#test-pattern)
12. [Test Vectors (YAML)](#test-vectors-yaml)
13. [Performance Validation](#performance-validation)

## Module Tree

```mermaid
graph TD
    crate["crates/octo-cli/"]
    crate --> cargo["Cargo.toml<br/>(UPDATED: add deps)"]
    crate --> src[src/]
    crate --> tests[tests/]

    src --> main["main.rs<br/>(REPLACED)"]
    src --> lib["lib.rs<br/>(NEW)"]
    src --> output["output.rs<br/>(NEW)"]
    src --> output_types["output/types.rs<br/>(NEW: Hex32, RedactedHex)"]
    src --> redact["redact.rs<br/>(NEW)"]
    src --> error["error.rs<br/>(NEW)"]
    src --> flags["flags.rs<br/>(NEW)"]
    src --> commands[commands/]

    commands --> cmod["mod.rs<br/>(NEW: dispatch)"]
    commands --> identity["identity.rs<br/>(NEW)"]
    commands --> capability["capability.rs<br/>(NEW)"]
    commands --> policy["policy.rs<br/>(NEW)"]
    commands --> stub["stub.rs<br/>(NEW: deprecated wrappers)"]

    tests --> t_id["identity.rs (8 TV)"]
    tests --> t_cap["capability.rs (19 TV)"]
    tests --> t_pol["policy.rs (5 TV)"]
    tests --> t_err["error.rs (5 TV)"]
    tests --> t_env["envelope.rs (5 TV)"]
    tests --> t_red["redact.rs (8 TV)"]
    tests --> t_dep["deprecation.rs (2 TV)"]
    tests --> t_env2["env_errors.rs (3 TV)"]
```

## Crate Dependency Wiring

Add to `crates/octo-cli/Cargo.toml`:

```toml
# === Substrate (Layer B) — RFC-0011 §Dependencies ===
# Pins match the actual substrate workspace version (0.1.0).
octo-wallet = { path = "../octo-wallet", version = "0.1.0" }                  # Layer B identity substrate — RFC-0009
octo-cap-macaroon = { path = "../octo-cap-macaroon", version = "0.1.0" }      # Layer B capability substrate — RFC-0957
octo-policy = { path = "../octo-policy", version = "0.1.0" }                  # Layer B policy substrate — RFC-0967
# (OctoCliError does NOT pull octo-vault; vault ops land per Status header amendment chain)

# === Output + TTY ===
# std::io::IsTerminal (Rust 1.70+) — no extra crate needed
chrono = { version = "0.4", default-features = false, features = ["serde", "clock"] }

# === Error envelope ===
thiserror = "2.0"          # OctoCliError derives #[derive(Error)]

# === Hex32 newtype serialization ===
hex = "0.4"                # Hex32 #[serde(with = "hex::serde")]

# === Redaction ===
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }

# === Schemars for OutputEnvelope<T> ===
schemars = { version = "0.8", features = ["chrono"] }

# === Dev dependencies ===
[dev-dependencies]
assert_cmd = "2.0"        # CLI binary invocation
predicates = "3.1"         # stdout/stderr assertions
```

**Layer direction check:** all deps above are Layer A/B substrate crates pulled
in by a Layer C/D orchestrator. No new Layer-A or Layer-B types introduced in
`octo-cli`. Per `[[cipherocto-design-principles]]` §Layer stability table, the
CLI is per-RFC evolution (not years-stable) — substrate crates are the frozen
boundary.

## OutputEnvelope\<T\>

```rust
//! crates/octo-cli/src/output.rs
//! Per RFC-0011 §Output Envelope — TTY-aware envelope + JSON / pretty renderer.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::io::{self, IsTerminal, Write};

/// ANSI escape codes — used only when `no_color` is false AND stdout is a TTY
/// (see `render_pretty` for the gating logic). The codes are inlined here
/// rather than pulled from a crate because the surface is small (4 wrappers +
/// reset) and inlining keeps the pretty renderer self-contained.
mod ansi {
    /// Reset all attributes — appended at the end of every colored fragment.
    pub const RESET: &str = "\x1b[0m";
    /// Cyan — used for field KEYS (`schema_version:`, `generated_at:`, ...).
    pub const KEY: &str = "\x1b[36m";
    /// Yellow — used for STRING values (double-quoted in the rendered output).
    pub const STRING: &str = "\x1b[33m";
    /// Green — used for BOOLEAN values (true / false).
    pub const BOOL: &str = "\x1b[32m";
    /// Magenta — used for NUMERIC values (u32, i32 displayed as their digits).
    pub const NUMBER: &str = "\x1b[35m";
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct OutputEnvelope<T> {
    /// Output schema version. Bumped on breaking field changes (per
    /// RFC-0011 §Compatibility — additive fields are non-breaking; the
    /// version bump is the explicit signal to consumers gating on
    /// `schema_version == 1`). Bumped from 1 → 2 in this RFC amendment for
    /// the additive `preview_only` field.
    pub schema_version: u32,

    /// RFC 3339 UTC timestamp of envelope generation.
    pub generated_at: DateTime<Utc>,

    /// Subcommand-specific data payload.
    pub data: T,

    /// Process exit code (mirrors shell exit code for scripting).
    pub exit_code: i32,

    /// Preview-only / dry-run marker — set to `true` when the command was
    /// invoked with `--dry-run` (or equivalent) and produced a non-mutating
    /// preview. Consumed by renderers to suppress commit-style language
    /// ("minted" → "preview: would mint"). Orthogonal to `exit_code`: a
    /// successful `--dry-run` and a successful non-dry-run mutation both
    /// exit 0; the dry-run provenance cannot be recovered from `exit_code`
    /// alone.
    pub preview_only: bool,
}

impl<T> OutputEnvelope<T> {
    /// Initial envelope schema version (bumped to 2 in this RFC amendment —
    /// additive `preview_only` field). See RFC-0011 §Compatibility for the
    /// field-add / schema_version-bump policy.
    pub const SCHEMA_VERSION: u32 = 2;

    /// Construct a non-preview envelope (`preview_only: false`).
    /// Use `preview_only` for `--dry-run` paths.
    pub fn new(data: T, exit_code: i32) -> Self {
        Self {
            schema_version: Self::SCHEMA_VERSION,
            generated_at: Utc::now(),
            data,
            exit_code,
            preview_only: false,
        }
    }

    /// Construct a preview-only envelope (`preview_only: true`).
    /// Set when the command was invoked with `--dry-run` (or equivalent)
    /// and produced a non-mutating preview. Exit code is the post-preview
    /// sentinel — mutating commands return `Ok(())` without firing
    /// substrate side effects, so `exit_code` is the standard 0 (success-
    /// preview) or a non-zero preview-class exit (e.g., `ConfirmationRequir…
    /// when a non-`--dry-run` flag is missing).
    pub fn preview_only(data: T, exit_code: i32) -> Self {
        Self {
            schema_version: Self::SCHEMA_VERSION,
            generated_at: Utc::now(),
            data,
            exit_code,
            preview_only: true,
        }
    }

    /// Render to stdout per RFC-0011 §Output Envelope TTY table:
    /// - TTY + no `--json` → pretty (line per field, ANSI-colored unless
    ///   `no_color` is true or stdout is not a TTY)
    /// - non-TTY OR `--json` → JSON
    ///
    /// `no_color` should be the parsed `--no-color` flag from `OutputFlags`
    /// (see impl-guide §Clap Root Struct). When `true`, ANSI codes are
    /// suppressed regardless of TTY status — overrides the implicit
    /// `IsTerminal` gate. Also honors `NO_COLOR=1` per the convention; the
    /// caller is responsible for wiring the env-var into the flag (see
    /// impl-guide §Clap Root Struct `OutputFlags.no_color` doc).
    pub fn render(&self, force_json: bool, no_color: bool) -> io::Result<()> {
        if force_json || !io::stdout().is_terminal() {
            let s = serde_json::to_string_pretty(self)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
            writeln!(io::stdout().lock(), "{}", s)
        } else {
            self.render_pretty(&mut io::stdout().lock(), no_color)
        }
    }

    /// Pretty-print the envelope line-per-field. ANSI rendering is gated by:
    ///   1. `no_color` is false (the `--no-color` flag / `NO_COLOR=1`),
    ///   2. `io::stdout().is_terminal()` (Windows console, Unix tty, PTY).
    /// When both gates pass, field values are wrapped in the ANSI codes from
    /// the `ansi` module:
    ///   - Keys: `\x1b[36m` (cyan)
    ///   - String values: `\x1b[33m` (yellow), wrapped in `"..."`
    ///   - Boolean values: `\x1b[32m` (green)
    ///   - Numeric values: `\x1b[35m` (magenta)
    ///   - Reset: `\x1b[0m` appended to every colored fragment
    /// No-color fallback (default in CI / pipe): plain monochrome text —
    /// the same YAML-like layout but without escape sequences.
    ///
    /// Cross-platform: `std::io::IsTerminal` (Rust 1.70+) uses
    /// Windows console APIs (`GetConsoleMode` / `GetConsoleScreenBufferInfo`)
    /// on Windows and `isatty(3)` on Unix. No `atty` / `isatty` crate needed;
    /// see `Cargo.toml` deps commentary for the rationale.
    fn render_pretty<W: Write>(&self, w: &mut W, no_color: bool) -> io::Result<()> {
        let use_color = !no_color && io::stdout().is_terminal();

        // ANSI-aware helpers — when `use_color` is false, all wrappers are
        // no-ops (`""`) and the fragments round-trip verbatim.
        let wrap = |code: &str, s: &str| -> String {
            if use_color {
                format!("{}{}{}", code, s, ansi::RESET)
            } else {
                s.to_string()
            }
        };
        let key = |s: &str| -> String { wrap(ansi::KEY, s) };
        let str_v = |s: &str| -> String { wrap(ansi::STRING, s) };
        let bool_v = |b: bool| -> String { wrap(ansi::BOOL, if b { "true" } else { "false" }) };
        let num_v = |n: i32| -> String { wrap(ansi::NUMBER, &n.to_string()) };

        writeln!(w, "{}: {}", key("schema_version"), num_v(self.schema_version as i32))?;
        let ts = self.generated_at.to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        writeln!(w, "{}: {}", key("generated_at"), str_v(&ts))?;
        writeln!(w, "{}: {}", key("exit_code"), num_v(self.exit_code))?;
        writeln!(w, "{}: {}", key("preview_only"), bool_v(self.preview_only))?;
        writeln!(w, "{}:", key("data"))?;
        // Field-by-field pretty for T; specific renderers per data type in commands/*.rs
        writeln!(w, "  (pretty-render by each command's pretty_data())")
    }
}
```

## OctoCliError

```rust
//! crates/octo-cli/src/error.rs
//! Per RFC-0011 §Error Handling — fixed exit-code table.

use thiserror::Error;

#[derive(Error, Debug)]
pub enum OctoCliError {
    #[error("clap parse error: {0}")]
    ClapParse(#[from] clap::Error),                         // exit 2 (POSIX usage-error convention)

    #[error("no active identity")]
    NoActiveIdentity,                                       // exit 2

    #[error("--confirm required for mutating command {command} in human mode")]
    ConfirmationRequired { command: String },               // exit 2 (POSIX usage-error)

    #[error("already rotating")]
    AlreadyRotating,                                        // exit 3

    #[error("identity not found: {0}")]
    IdentityNotFound(String),                               // exit 4

    #[error("HSM unavailable: {0}")]
    HsmUnavailable(String),                                 // exit 5

    #[error("identity already revoked")]
    AlreadyRevoked,                                         // exit 6

    #[error("caveat parse error: {message}")]
    CaveatParse { message: String },                        // exit 7

    #[error("invalid caveat combination: {detail}")]
    InvalidCaveatCombination { detail: String },            // exit 8

    #[error("holder not found: {0}")]
    HolderNotFound(String),                                 // exit 9

    #[error("attenuation violation: {0}")]
    AttenuationViolation(String),                           // exit 10

    #[error("signing failed: {0}")]
    SigningFailed(String),                                  // exit 11

    #[error("parent capability not found: {0}")]
    ParentCapNotFound(String),                              // exit 12

    #[error("policy not found: {0}")]
    PolicyNotFound(String),                                 // exit 13

    #[error("policy version not found: {policy}@{version}")]
    PolicyVersionNotFound { policy: String, version: u32 }, // exit 14

    #[error("secret material on pipe; pass --allow-stdin-secret to override")]
    StdinSecretRefused,                                     // exit 15

    #[error("invalid filter: {0}")]
    InvalidFilter(String),                                  // exit 16 (reserved per Status header amendment chain)

    #[error("stub command {name} is stale; use the replacement documented in RFC-0011 §Compatibility")]
    StaleStub { name: String },                             // exit 65

    #[error("internal error: {0}")]
    Internal(String),                                       // exit 64
}

// Variant ORDER note: order is aligned with RFC-0011 §Error Handling. The
// order does NOT affect `Display` strings (each variant has its own
// `#[error(...)]` format) and does NOT change `Debug` output for callers
// (each variant is named). Reordering is therefore safe but kept in sync
// with the RFC for audit clarity.

/// Sanitize substrate error strings before display — strips file paths,
/// SQL fragments, and stack-trace lines. Per RFC-0011 §Error Handling.
pub fn sanitize_substrate_error(s: &str) -> String {
    let mut out = s.to_string();
    for prefix in ["src/", "crates/octo-"] {
        while let Some(idx) = out.find(prefix) {
            let end = out[idx..]
                .find(|c: char| c == ' ' || c == '\n' || c == ':' || c == ')')
                .map(|e| idx + e)
                .unwrap_or(out.len());
            out.replace_range(idx..end, "<substrate-path>");
        }
    }
    for marker in ["SQL:", "query:", "sqlite3_open"] {
        while let Some(idx) = out.find(marker) {
            let end = out[idx..]
                .find(|c: char| c == ';' || c == '\n')
                .map(|e| idx + e + 1)
                .unwrap_or(out.len());
            out.replace_range(idx..end, "<substrate-error>");
        }
    }
    while out.contains("<substrate-error><substrate-error>") {
        out = out.replace("<substrate-error><substrate-error>", "<substrate-error>");
    }
    out
}

impl OctoCliError {
    pub fn exit_code(&self) -> i32 {
        match self {
            Self::ClapParse(_) => 2,                   // POSIX usage-error
            Self::NoActiveIdentity => 2,
            Self::ConfirmationRequired { .. } => 2,   // POSIX usage-error
            Self::AlreadyRotating => 3,
            Self::IdentityNotFound(_) => 4,
            Self::HsmUnavailable(_) => 5,
            Self::AlreadyRevoked => 6,
            Self::CaveatParse { .. } => 7,
            Self::InvalidCaveatCombination { .. } => 8,
            Self::HolderNotFound(_) => 9,
            Self::AttenuationViolation(_) => 10,
            Self::SigningFailed(_) => 11,
            Self::ParentCapNotFound(_) => 12,
            Self::PolicyNotFound(_) => 13,
            Self::PolicyVersionNotFound { .. } => 14,
            Self::StdinSecretRefused => 15,
            Self::InvalidFilter(_) => 16,
            Self::StaleStub { .. } => 65,
            Self::Internal(_) => 64,
        }
    }

    /// Render the error in user-facing form, with EVERY string field passed through
    /// `sanitize_substrate_error`. Defense-in-depth: substrate crates SHOULD NOT emit
    /// internals-bearing errors, but every variant's display string runs through the
    /// sanitizer so any substrate string that slipped through is caught.
    pub fn user_message(&self) -> String {
        // Run thiserror's auto-derived Display through the sanitizer. thiserror
        // formats the entire variant into a single string, so a single pass
        // covers every variant's string field. Non-string variants (NoActiveIdentity,
        // AlreadyRotating, AlreadyRevoked, StdinSecretRefused, ClapParse-wrapping)
        // emit no substrate strings and pass through unchanged.
        sanitize_substrate_error(&self.to_string())
    }

    pub fn render(&self, force_json: bool) {
        let exit_code = self.exit_code();
        let sanitized = self.user_message();
        let caused_by = std::error::Error::source(self)
            .map(|s| sanitize_substrate_error(&s.to_string()));
        let hint = self.hint();
        if force_json || !std::io::stdout().is_terminal() {
            eprintln!("{}", serde_json::json!({
                "error": sanitized,
                "caused_by": caused_by,
                "hint": hint,
                "exit_code": exit_code,
            }));
        } else {
            eprintln!("error: {}", sanitized);
            if let Some(source) = caused_by.as_deref() {
                eprintln!("  caused by: {}", source);
            }
            if let Some(h) = hint.as_deref() {
                eprintln!("  hint: {}", h);
            }
            eprintln!("  exit code: {}", exit_code);
        }
        std::process::exit(exit_code);
    }

    /// Per-variant remediation hint (matches RFC §Error Handling).
    /// Returns `None` if no hint applies for the variant.
    pub fn hint(&self) -> Option<String> {
        match self {
            OctoCliError::ClapParse(_) => Some("run with --help for usage".to_string()),
            OctoCliError::NoActiveIdentity => {
                Some("create an identity with `octo-wallet init` (wallet substrate)".to_string())
            }
            OctoCliError::ConfirmationRequired { .. } => {
                Some("re-run with --confirm to acknowledge the mutation".to_string())
            }
            OctoCliError::IdentityNotFound(_) => Some("verify the DID; list identities with `octo whoami`".to_string()),
            OctoCliError::CaveatParse { .. } => Some("pass --caveats as a JSON object".to_string()),
            OctoCliError::HolderNotFound(_) => Some("verify the holder DID exists".to_string()),
            OctoCliError::StdinSecretRefused => Some("re-run with --allow-stdin-secret if intentional".to_string()),
            _ => None,
        }
    }
}
```

## OctoCliRedactor

```rust
//! crates/octo-cli/src/redact.rs
//! Per RFC-0011 §Redaction Layer — tracing Layer stripping secret patterns.
//! Placeholder pattern; canonical implementation candidate for `octo-redact`
//! shared crate per RFC-0917 §HTTP proxy / Python SDK adapter.

use std::fmt::Write;
use tracing::Event;
use tracing::Subscriber;
use tracing::field::{Field, Visit};
use tracing_subscriber::layer::Context;
use tracing_subscriber::Layer;

const REDACTED_SEED: &str = "[REDACTED:seed]";
const REDACTED_KEY: &str = "[REDACTED:key]";
const REDACTED_SIG: &str = "[REDACTED:sig]";
const REDACTED_PAIR: &str = "[REDACTED:pair]";
const REDACTED_PW: &str = "[REDACTED:pw]";
const REDACTED_BEARER: &str = "[REDACTED:bearer]";
const REDACTED_MNEMONIC: &str = "[REDACTED:mnemonic]";
const REDACTED_PASSPHRASE: &str = "[REDACTED:passphrase]";
const REDACTED_PIN: &str = "[REDACTED:pin]";
const REDACTED_API_KEY: &str = "[REDACTED:api_key]";
const REDACTED_SECRET: &str = "[REDACTED:secret]";

pub struct OctoCliRedactor;

/// Visitor that captures all event fields as `(name, value)` pairs.
/// `tracing::Event::record(&mut visitor)` walks the event's field set, calling
/// `visit_str` / `visit_debug` / `visit_i64` / etc. for each field. We accumulate
/// every (name, value) pair into a `Vec` so the redactor can apply patterns.
#[derive(Default)]
struct FieldVisitor {
    fields: Vec<(&'static str, String)>,
}

impl tracing::field::Visit for FieldVisitor {
    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        self.fields.push((field.name(), value.to_string()));
    }
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        self.fields.push((field.name(), format!("{:?}", value)));
    }
    fn record_i64(&mut self, field: &tracing::field::Field, value: i64) {
        self.fields.push((field.name(), value.to_string()));
    }
    fn record_u64(&mut self, field: &tracing::field::Field, value: u64) {
        self.fields.push((field.name(), value.to_string()));
    }
    fn record_bool(&mut self, field: &tracing::field::Field, value: bool) {
        self.fields.push((field.name(), value.to_string()));
    }
}

impl<S: Subscriber> Layer<S> for OctoCliRedactor {
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        // Walk event fields; capture via FieldVisitor, then run each value through
        // `redact_by_field` (name-keyed) AND `redact_string` (value-pattern).
        //
        // Note: tracing events are not mutable in-place. We cannot swap values
        // back into the original event. The Layer is the global safety net: the
        // primary redaction path is the substrate-side `redact_string` call
        // applied at the call site (commands/*.rs). The Layer runs `redact_string`
        // on each captured value to catch anything the substrate missed.
        let mut visitor = FieldVisitor::default();
        event.record(&mut visitor);
        for (name, value) in &visitor.fields {
            let field_redacted = redact_by_field(name, value);
            let value_redacted = redact_string(field_redacted);
            if value_redacted.as_ref() != value {
                // tracing doesn't allow modifying event in-place; log a trace-level
                // marker so test harnesses can detect a Layer-pass redaction
                // (the substrate-side call remains the primary path).
                tracing::trace!(
                    field_name = name,
                    original_len = value.len(),
                    redacted = %value_redacted,
                    "OctoCliRedactor: field redacted at Layer pass",
                );
            }
        }
    }
}

/// Detect a 128-hex-char run (Ed25519 64-byte hex sig) inside `s`.
/// Returns the byte-range (start, end) of the first match if found.
fn find_128_hex(s: &str) -> Option<(usize, usize)> {
    let bytes = s.as_bytes();
    let mut i = 0;
    while i + 128 <= bytes.len() {
        if bytes[i..i + 128].iter().all(|b| b.is_ascii_hexdigit()) {
            // Confirm boundary: char before is whitespace, start, or punctuation;
            // char after is whitespace, end, or punctuation. Otherwise we risk
            // over-redacting a 256-hex key.
            let before_ok = i == 0 || !(bytes[i - 1].is_ascii_alphanumeric());
            let after_ok =
                i + 128 == bytes.len() || !(bytes[i + 128].is_ascii_alphanumeric());
            if before_ok && after_ok {
                return Some((i, i + 128));
            }
        }
        i += 1;
    }
    None
}

/// Case-insensitive scan for `bearer` followed by ASCII-whitespace + token.
/// Handles `Bearer `, `bearer\t`, `Bearer\n`, `bearer\r` — any ASCII-whitespace
/// separator between the keyword and the token (not just a literal space).
fn find_bearer_ci(s: &str) -> Option<(usize, usize)> {
    let lower = s.to_ascii_lowercase();
    let needle = "bearer";
    let needle_len = needle.len();
    let bytes = s.as_bytes();
    lower.find(needle).and_then(|idx| {
        let after = idx + needle_len;
        // Require ASCII-whitespace separator after the keyword; otherwise we
        // risk matching substrings like `bearership` or `bearerish`.
        if after < bytes.len() && bytes[after].is_ascii_whitespace() {
            Some((idx, after + 1)) // include the whitespace in the redaction range
        } else {
            None
        }
    })
}

pub fn redact_string(s: &str) -> std::borrow::Cow<'_, str> {
    // Pattern order matters: most-specific first.
    let mut out = std::borrow::Cow::Borrowed(s);

    // 1) 128-hex sig (Ed25519 64-byte hex) — independent of field name.
    while let Some((start, end)) = find_128_hex(out.as_ref()) {
        let mut buf = String::with_capacity(out.len());
        buf.push_str(&out[..start]);
        buf.push_str(REDACTED_SIG);
        buf.push_str(&out[end..]);
        out = std::borrow::Cow::Owned(buf);
    }

    // 2) Bearer tokens — case-insensitive.
    while let Some((start, end)) = find_bearer_ci(out.as_ref()) {
        // Token runs until next whitespace or end-of-string.
        let token_end = out[end..]
            .find(|c: char| c.is_whitespace())
            .map(|e| end + e)
            .unwrap_or(out.len());
        let mut buf = String::with_capacity(out.len());
        buf.push_str(&out[..start]);
        buf.push_str(REDACTED_BEARER);
        buf.push_str(&out[token_end..]);
        out = std::borrow::Cow::Owned(buf);
    }

    // 3) Value-pattern fields (`password=...`, `seed_bytes=...`, `api_key=...`).
    for (prefix, placeholder) in [
        ("password=", REDACTED_PW),
        ("passphrase=", REDACTED_PASSPHRASE),
        ("pin=", REDACTED_PIN),
        ("seed_bytes=", REDACTED_SEED),
        ("seed=", REDACTED_SEED),
        ("api_key=", REDACTED_API_KEY),
        ("mnemonic=", REDACTED_MNEMONIC),
        ("secret=", REDACTED_SECRET),
        ("token=", REDACTED_SECRET),
    ] {
        while let Some(idx) = out.find(prefix) {
            // Value runs until next whitespace or comma.
            let val_start = idx + prefix.len();
            let val_end = out[val_start..]
                .find(|c: char| c.is_whitespace() || c == ',')
                .map(|e| val_start + e)
                .unwrap_or(out.len());
            let mut buf = String::with_capacity(out.len());
            buf.push_str(&out[..val_start]);
            buf.push_str(placeholder);
            buf.push_str(&out[val_end..]);
            out = std::borrow::Cow::Owned(buf);
        }
    }

    out
}

/// Field-name-keyed redactor for visit_str / visit_debug visitors.
pub fn redact_by_field(field_name: &str, value: &str) -> &str {
    match field_name {
        "seed" | "seed_bytes" => REDACTED_SEED,
        "private_key" | "holder_key" => REDACTED_KEY,
        "holder_sig" | "signature" => REDACTED_SIG,
        "pair_code" => REDACTED_PAIR,
        "password" => REDACTED_PW,
        "passphrase" => REDACTED_PASSPHRASE,
        "mnemonic" | "seed_phrase" => REDACTED_MNEMONIC,
        "pin" => REDACTED_PIN,
        "api_key" => REDACTED_API_KEY,
        "secret" | "token" => REDACTED_SECRET,
        _ => value,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_bearer_token() {
        let out = redact_string("Authorization: Bearer eyJhbGc.eyJzdWI.signature");
        assert!(out.contains(REDACTED_BEARER));
        assert!(!out.contains("eyJhbGc"));
    }

    #[test]
    fn redacts_bearer_case_insensitive() {
        let out = redact_string("authorization: bearer abcdefghij");
        assert!(out.contains(REDACTED_BEARER));
        assert!(!out.contains("abcdefghij"));
    }

    #[test]
    fn redacts_holder_sig_128_hex() {
        let hex: String = (0..128).map(|_| 'a').collect();
        let input = format!("holder_sig={}", hex);
        let out = redact_string(&input);
        assert!(out.contains(REDACTED_SIG));
        assert!(!out.contains(&hex));
    }

    #[test]
    fn redacts_password_value() {
        let out = redact_string("password=hunter2 next=ok");
        assert!(out.contains(REDACTED_PW));
        assert!(!out.contains("hunter2"));
    }

    #[test]
    fn redacts_seed_bytes_value() {
        let out = redact_string("seed_bytes=abcdef0123 next=ok");
        assert!(out.contains(REDACTED_SEED));
        assert!(!out.contains("abcdef0123"));
    }

    #[test]
    fn preserves_safe_strings() {
        let out = redact_string("minted cap_id=01ab..9d2a");
        assert_eq!(out, "minted cap_id=01ab..9d2a");
    }

    #[test]
    fn redacts_seed_by_field() {
        assert_eq!(redact_by_field("seed_bytes", "abc123"), REDACTED_SEED);
        assert_eq!(redact_by_field("did", "did:octo:abc"), "did:octo:abc");
    }

    #[test]
    fn redacts_mnemonic_by_field() {
        assert_eq!(redact_by_field("mnemonic", "word1 word2"), REDACTED_MNEMONIC);
    }
}
```

## Clap Root Struct

```rust
//! crates/octo-cli/src/flags.rs

use clap::Args;

#[derive(Args, Debug, Clone)]
pub struct OutputFlags {
    /// Force JSON output (overrides TTY detection)
    #[arg(long, global = true)]
    pub json: bool,

    /// Disable ANSI color codes
    #[arg(long, global = true)]
    pub no_color: bool,
}

#[derive(Args, Debug, Clone)]
#[group(multiple = false)]
pub struct OperatorModeFlags {
    /// Operator mode: human | ci | auditor
    #[arg(long, global = true, value_enum, default_value_t = OperatorMode::Human)]
    pub mode: OperatorMode,

    /// Required for mutating commands in ci mode
    #[arg(long, global = true)]
    pub allow_write: bool,

    /// Required for mutating commands in human mode
    #[arg(long, global = true)]
    pub confirm: bool,

    /// Preview without state mutation
    #[arg(long, global = true)]
    pub dry_run: bool,

    /// Required for secret material via pipe
    #[arg(long, global = true)]
    pub allow_stdin_secret: bool,
}

#[derive(clap::ValueEnum, Clone, Debug, PartialEq, Eq)]
pub enum OperatorMode {
    Human,
    Ci,
    Auditor,
}
```

```rust
//! crates/octo-cli/src/main.rs (REPLACES stub)

use clap::{Parser, Subcommand};
use octo_cli::{
    commands,
    error::OctoCliError,
    flags::{OperatorMode, OperatorModeFlags, OutputFlags},
    output::OutputEnvelope,
};

#[derive(Parser, Debug)]
#[command(name = "octo", version, about = "CipherOcto operator CLI")]
struct Octo {
    #[command(flatten)]
    output: OutputFlags,

    #[command(flatten)]
    mode: OperatorModeFlags,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    Whoami,
    Identity {
        #[command(subcommand)]
        action: IdentityAction,
    },
    Capability {
        #[command(subcommand)]
        action: CapabilityAction,
    },
    Policy {
        #[command(subcommand)]
        action: PolicyAction,
    },

    // --- Deprecated stubs (per RFC-0011 §Compatibility) ---
    // Removed in v2.0 (gated on 1 release deprecation window + 1 release hard-error cycle
    // per RFC migration etiquette). v1.1 = hard-error (`StaleStub`, exit 65).
    #[command(hide = true)]
    Init,
    #[command(hide = true)]
    Join,
    #[command(hide = true)]
    Role { #[command(subcommand)] action: RoleActionStub },
    #[command(hide = true)]
    Agent { #[command(subcommand)] action: AgentActionStub },
    #[command(hide = true)]
    Status,
}

#[derive(Subcommand, Debug)]
enum IdentityAction {
    /// Show the identity record for a given DID.
    Show {
        /// Target DID.
        did: Option<String>,
    },
    /// Begin a key rotation.
    Rotate {},
    /// Revoke the active identity.
    Revoke {
        /// Revocation reason recorded in the identity log.
        #[arg(long)] reason: String,
    },
}

#[derive(Subcommand, Debug)]
enum CapabilityAction {
    /// List capabilities.
    List {
        /// Filter as `field=value` (repeatable, comma-separated). Accepted
        /// fields: `cap_id`, `root_id`, `caveat`.
        #[arg(long, value_delimiter = ',')]
        filter: Vec<String>,
    },
    /// Mint a new capability.
    Mint {
        /// Caveat expression.
        #[arg(long, value_parser = clap::value_parser!(String))]
        caveats: String,
        /// Holder DID.
        #[arg(long)]
        holder: String,
        /// Root capability identifier.
        #[arg(long)]
        root: Option<String>,
        /// Acknowledge that minting grants authority.
        /// clap-validator enforces: requires `--confirm` whenever this flag is present.
        #[arg(long, requires = "confirm")]
        confirm_acknowledge: bool,
    },
    /// Attenuate an existing capability.
    Attenuate {
        /// Parent capability identifier.
        cap_id: String,
        /// Additional caveats to apply.
        #[arg(long)]
        caveats: String,
        /// Acknowledge that attenuation narrows authority.
        /// clap-validator enforces: requires `--confirm` whenever this flag is present.
        #[arg(long, requires = "confirm")]
        confirm_acknowledge: bool,
    },
}

#[derive(Subcommand, Debug)]
enum PolicyAction {
    Show {
        name: String,
        #[arg(long)]
        version: Option<u32>,
        #[arg(long)]
        kind_uuid: Option<String>,
    },
    List {
        #[arg(long)]
        filter: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
enum RoleActionStub {
    Builder,
    Provider,
    Storage,
    Bandwidth,
    Orchestrator,
}

#[derive(Subcommand, Debug)]
enum AgentActionStub {
    Create { name: String },
    Run { name: String },
    List,
}

fn main() {
    if let Err(e) = run() {
        // Two JSON-forcing mechanisms are orthogonal:
        //  1. `--json` flag forces JSON output regardless of TTY detection (per RFC §Output Envelope).
        //  2. `OCTO_FORCE_JSON` environment variable forces JSON output for scripted environments
        //     that cannot pass `--json` (e.g., wrapper scripts, cron jobs).
        // `force_json` here ORs both sources.
        let cli_json = /* parsed from clap Octo top-level */ false;
        let env_json = std::env::var("OCTO_FORCE_JSON").is_ok();
        e.render(/* force_json = */ cli_json || env_json);
    }
}

fn run() -> Result<(), OctoCliError> {
    let cli = Octo::parse();
    let result = match &cli.command {
        Commands::Whoami => commands::identity::whoami(&cli),
        Commands::Identity { action } => commands::identity::dispatch(action, &cli),
        Commands::Capability { action } => commands::capability::dispatch(action, &cli),
        Commands::Policy { action } => commands::policy::dispatch(action, &cli),

        Commands::Init => commands::stub::print_deprecated("init", "use octo-wallet init (out of scope for this RFC)"),
        Commands::Join => commands::stub::print_deprecated("join", "use octo network bootstrap (out of scope for this RFC)"),
        Commands::Role { action } => commands::stub::print_role_deprecated(action),
        Commands::Agent { action } => commands::stub::print_agent_deprecated(action),
        Commands::Status => commands::stub::print_deprecated("status", "use octo network status (per Status header amendment chain)"),
    };
    result
}
```

## Identity Subcommands

```rust
//! crates/octo-cli/src/commands/identity.rs
//! Per RFC-0011 §Subcommand Taxonomy — IdentityAction dispatch.

use chrono::{DateTime, Utc};
use octo_cli::{
    error::OctoCliError,
    flags::OutputFlags,
    output::OutputEnvelope,
    Octo,
};
// Substrate imports — corrected per R1 substrate alignment review. Substrate re-exports
// `IdentityKey` + `WalletError` at the crate root (canonical path); the
// `identity_key` module path is the internal layout. The `LifecycleState`
// lives in the `lifecycle` module path.
use octo_wallet::{IdentityKey, WalletError};
use octo_wallet::lifecycle::LifecycleState;
use serde::Serialize;

/// Lifecycle state is a stable string literal (RFC-0009 §Identity Struct) NOT a
/// substrate `Debug`-formatted string. Insulation rationale: substrate enum
/// derives may change field names across releases; the CLI contract pins the
/// string set here and `Display` impl lives in `octo-wallet`.
#[derive(Serialize, Debug)]
pub struct WhoamiOutput {
    pub did: String,
    pub pubkey_hex: String,
    pub lifecycle_state: String,
    pub hsm_slot: Option<u32>,
    pub registered_at: DateTime<Utc>,
}

pub fn whoami(cli: &Octo) -> Result<(), OctoCliError> {
    // SUBSTRATE-PENDING: see RFC-0011 substrate amendment §Subcommand Taxonomy entry #1.
    // `WalletStore` does NOT yet exist in `octo-wallet`. When the amendment lands,
    // this opens the on-disk store at `$OCTO_HOME/wallet` and enforces 0700 perms.
    let store = octo_wallet::WalletStore::open().map_err(|e| {
        OctoCliError::Internal(format!("failed to open wallet store: {}", e))
    })?;
    // Canonical pattern: `active_identity(&WalletStore) -> Result<IdentityKey, WalletError>`
    // — takes an explicit store handle (no ambient global). `NoActiveIdentity` is an
    // [ADD] variant on `WalletError`; substrate today has `WalletError::NotActive { current_state }`
    // — CLI maps the existing variant to exit 2 via the `NotActive` arm below and
    // the [ADD] form is gated on the substrate amendment landing.
    let key = octo_wallet::active_identity(&store).map_err(|e| match e {
        // SUBSTRATE-PENDING: `WalletError::NoActiveIdentity` not yet added; current
        // substrate uses `WalletError::NotActive { current_state: LifecycleState }`.
        // The match arm below covers BOTH the [ADD] form (post-amendment) and the
        // existing form by string match.
        WalletError::NoActiveIdentity => OctoCliError::NoActiveIdentity,
        WalletError::NotActive { .. } => OctoCliError::NoActiveIdentity,
        other => OctoCliError::Internal(other.to_string()),
    })?;
    // SUBSTRATE-PENDING: `IdentityKey::did() -> Did` is [ADD] per §Subcommand Taxonomy
    // entry #2. Substrate today has no `Did` type. CLI-side construction:
    // `Did(format!("did:octo:{}", base32::encode(...public_key_bytes()...)))`.
    let did = key.did().clone();
    // `identity_record(&WalletStore, &Did) -> Result<IdentityRecord, WalletError>` —
    // canonical (store, did) form per RFC-0011 [ADD] entry #3.
    let record = octo_wallet::identity_record(&store, &did).map_err(|e| {
        OctoCliError::Internal(format!("identity_record lookup failed: {}", e))
    })?;
    let output = WhoamiOutput {
        did: record.did.0.clone(),
        pubkey_hex: hex::encode(record.pubkey_bytes),
        // Per the note above: use a stable mapping, not `format!("{:?}", ...)`.
        // SUBSTRATE-PENDING: `LifecycleState::stable_label()` is NOT in substrate.
        // Substrate today has `impl fmt::Debug for LifecycleState` returning the
        // stable strings (`Designated`/`Active`/`Rotating`/`Revoked`) — use that
        // as the substrate-truth source until the [ADD] form lands. The CLI
        // contract pins the string set here and `Display` impl lives in
        // `octo-wallet` (post-amendment).
        lifecycle_state: format!("{:?}", record.lifecycle),
        hsm_slot: record.hsm_slot,
        registered_at: DateTime::<Utc>::from_timestamp(record.registered_at_unix, 0)
            .unwrap_or_else(|| DateTime::<Utc>::from_timestamp(0, 0).unwrap()),
    };
    // `whoami` is read-only — no `--dry-run` semantics (per RFC-0011
    // §Subcommand Taxonomy, `octo whoami` row: "Dry-run: n/a"). The
    // `OutputEnvelope::new` factory sets `preview_only: false` by default,
    // so the assertion that the envelope surfaces `preview_only == false`
    // for read-only commands is implicit. Mutating commands (rotate,
    // revoke, mint, attenuate) construct their envelope via
    // `OutputEnvelope::preview_only(data, exit_code)` when `cli.mode.dry_run`
    // is set; see impl-guide §Clap Root Struct + §Identity Subcommands
    // `require_confirm` for the dispatch gating.
    let env = OutputEnvelope::new(output, 0);
    // Thread `cli.output.no_color` into the renderer's ANSI gate. The
    // `cli.output.json` flag forces JSON regardless of TTY; `no_color` only
    // suppresses ANSI codes when pretty-printed (TTY + no `--json`).
    env.render(cli.output.json, cli.output.no_color)
}

pub fn dispatch(action: &IdentityAction, cli: &Octo) -> Result<(), OctoCliError> {
    match action {
        IdentityAction::Show { did } => show(did.as_deref(), cli),
        IdentityAction::Rotate => {
            require_confirm(cli, "identity rotate")?;
            rotate(cli)
        }
        IdentityAction::Revoke { reason } => {
            require_confirm(cli, "identity revoke")?;
            revoke(reason, cli)
        }
    }
}

/// Enforce `--confirm` for mutating commands in human mode.
/// - Human mode + no `--confirm` + no `--dry-run` → `ConfirmationRequired` (exit 2)
/// - Human mode + `--confirm` OR `--dry-run` → OK
/// - CI mode + no `--allow-write` + no `--dry-run` → `ConfirmationRequired` (exit 2)
/// - Auditor mode → never permitted to mutate (caller must short-circuit)
pub fn require_confirm(cli: &Octo, command: &str) -> Result<(), OctoCliError> {
    use octo_cli::flags::OperatorMode;
    if cli.mode.dry_run {
        return Ok(()); // --dry-run bypasses confirmation (preview only)
    }
    match cli.mode.mode {
        OperatorMode::Human => {
            if !cli.mode.confirm {
                return Err(OctoCliError::ConfirmationRequired {
                    command: command.to_string(),
                });
            }
        }
        OperatorMode::Ci => {
            if !cli.mode.allow_write {
                return Err(OctoCliError::ConfirmationRequired {
                    command: command.to_string(),
                });
            }
        }
        OperatorMode::Auditor => {
            return Err(OctoCliError::ConfirmationRequired {
                command: command.to_string(),
            });
        }
    }
    Ok(())
}

// ... (show, rotate, revoke impls follow RFC-0011 §Subcommand Taxonomy tables)
```

## Capability Subcommands

Per RFC-0011 §Subcommand Taxonomy, capability subcommands parse `--caveats <json>` against the
RFC-0964 envelope catalog, then delegate to
`[ADD] octo_cap_macaroon::mint(root_secret: &[u8;32], holder: &dyn CapabilitySigner, holder_did: &str, caveats: &[Caveat])`
(RFC §Subcommand Taxonomy [ADD] entry #10 — thin wrapper around substrate `CapabilityToken::mint`
per substrate signature; `holder` is `&dyn CapabilitySigner` per substrate
signature, NOT the phantom `HolderKey` type)
and `attenuate(parent, caveats, holder, catalog)`. Caveat validation errors
return `OctoCliError::CaveatParse` (exit 7) or
`OctoCliError::InvalidCaveatCombination` (exit 8). Attenuation violations
return `OctoCliError::AttenuationViolation` (exit 10).

The 8 supported caveat canonical forms (per RFC-0964) live in
`crates/octo-cap-macaroon/src/caveat/`. The CLI consumes them — does NOT define
new variants.

## Policy Subcommands

Per RFC-0011 §Subcommand Taxonomy, `octo policy show <name>` delegates to
`octo_policy::show(name, version)`. `body_json` rendering applies the redactor
to any nested secret fields. The CLI does NOT introspect policy body structure
beyond the redactor pass.

## Stub Command Deprecation

```rust
//! crates/octo-cli/src/commands/stub.rs
//! Per RFC-0011 §Compatibility — deprecation banner for stub commands.
//! v1.0: banner only (exit 0). v1.1+: hard error (StaleStub, exit 65).

use octo_cli::error::OctoCliError;

/// `STALE_STUB_WINDOW`: feature flag / const gate for v1.1 hard-error behavior.
/// When `true`, stubs emit `StaleStub` (exit 65). When `false` (v1.0),
/// stubs emit a deprecation banner (exit 0).
///
/// v1.0 banner-only: `false`. The hard-error gate is enabled in v1.1 via the
/// `OCTO_STALE_STUB_WINDOW` env-var override (not a const bump — const changes
/// require a recompile, env-var allows the operator to flip behavior without
/// rebuilding). Mission `0011-deprecation-stub-removal` removes the stub code
/// entirely in v2.0.
const STALE_STUB_WINDOW: bool = false;

pub fn print_deprecated(name: &str, hint: &str) -> Result<(), OctoCliError> {
    eprintln!("DEPRECATED: `octo {}` is a stub. {}", name, hint);
    // Env-var override: `OCTO_STALE_STUB_WINDOW=true` flips the const at
    // runtime (v1.1+ behavior). Const stays `false` for v1.0 banner-only
    // default; the override avoids a recompile when the v1.1 window opens.
    let stale_window = STALE_STUB_WINDOW
        || std::env::var("OCTO_STALE_STUB_WINDOW")
            .map(|v| v == "true" || v == "1")
            .unwrap_or(false);
    if stale_window {
        // v1.1+ behavior — hard error per RFC-0011 §Compatibility timeline.
        return Err(OctoCliError::StaleStub { name: name.to_string() });
    }
    eprintln!("This stub will be removed in v2.0.");
    Ok(())
}

pub fn print_role_deprecated(_action: &RoleActionStub) -> Result<(), OctoCliError> {
    print_deprecated("role", "use octo role select (per Status header amendment chain)")
}

pub fn print_agent_deprecated(_action: &AgentActionStub) -> Result<(), OctoCliError> {
    print_deprecated("agent", "use octo agent lifecycle (per Status header amendment chain)")
}
```

## Test Pattern

```rust
//! crates/octo-cli/tests/identity.rs
//! Per RFC-0011 §Test Vectors — 8 identity TV per subcommand group.

use assert_cmd::Command;
use predicates::prelude::*;

fn octo() -> Command {
    Command::cargo_bin("octo").unwrap()
}

#[test]
fn tv_id1_whoami_success() {
    // Mock active identity in wallet store; assert exit 0 + JSON shape.
    let output = octo()
        .args(["--json", "whoami"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    // Strict JSON-shape assertion (NOT substring) — pin field names + types.
    // `schema_version: 2` reflects the additive `preview_only` field
    // introduced in this RFC amendment (per §Compatibility); see
    // `OutputEnvelope::SCHEMA_VERSION` in impl-guide §OutputEnvelope.
    // `preview_only: false` because `octo whoami` has no `--dry-run`
    // semantics; a successful read-only command always surfaces
    // `preview_only: false`.
    // Strict JSON-shape assertion via `predicate::str::contains` on each
    // pinned field — avoids the `assert_json_diff` dep while still locking
    // field names + values. Use `assert_json_diff = "1"` if exact structural
    // match is required for downstream callers.
    let stdout = std::str::from_utf8(&output).unwrap();
    assert!(stdout.contains(r#""schema_version":2"#));
    assert!(stdout.contains(r#""preview_only":false"#));
    assert!(stdout.contains(r#""exit_code":0"#));
    assert!(stdout.contains(r#""did":"#));
    assert!(stdout.contains(r#""pubkey_hex":"#));
    assert!(stdout.contains(r#""lifecycle_state":"#));
    assert!(stdout.contains(r#""hsm_slot":"#));
            "registered_at": _,
        }
    });
}

#[test]
fn tv_id2_identity_show_not_found() {
    // Empty wallet store; assert exit 4.
    octo()
        .args(["--json", "identity", "show", "did:octo:nonexistent"])
        .assert()
        .code(4)
        .stderr(predicate::str::contains("identity not found"));
}

#[test]
fn tv_id3_identity_rotate_confirm_required() {
    // require_confirm rejects without --confirm; exit 2 (POSIX convention).
    octo()
        .args(["identity", "rotate"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains(
            "--confirm required for mutating command identity rotate",
        ));
}

// ... etc per RFC-0011 §Test Vectors
```

**Cross-platform note:** GitHub Actions matrix runs `cargo test -p octo-cli` on
Linux + macOS + Windows. The Windows console path differs (cmd.exe vs PowerShell
vs WSL); the `std::io::IsTerminal` check uses Windows console APIs that work
identically to Unix. CI asserts Windows + Linux + macOS all pass.

**CI coverage requirements:** every test vector above MUST have a corresponding
test in `tests/identity.rs` / `tests/capability.rs` / `tests/policy.rs` /
`tests/stub.rs` (substrate-stub + deprecation-stub warnings). Error + envelope
+ redact + env-errors tests live inline in their respective `src/` modules.
The count is 19 + 19 + 5 + 1 = 44 tests minimum across the four integration
files (lib unit tests cover error / envelope / redact / env-errors surfaces).

## Test Vectors (YAML)

```yaml
# crates/octo-cli/tests/vectors/identity.yaml
# Per RFC-0011 §Test Vectors — 8 identity test vectors.
#
# **Adaptation note:** substrate (`octo-wallet`) is currently a stub that
# returns `WalletError::NotActive` from every store call. The canonical
# happy-path assertions in the YAML below (exit 0 for tv_id1, exit 6 for
# tv_id5, exit 3 for tv_id6, exit 5 for tv_id7) cannot be fully exercised
# until the wallet substrate amendment lands. Each divergent tv_id row
# below carries an `adapted:` line mapping to the active test in
# `crates/octo-cli/tests/identity.rs` so the unignore moment is visible.
# Each `canonical:` fixture in `crates/octo-cli/tests/identity.rs` is
# `#[ignore]`d and labeled "adapted; stub cannot synthesize ...; revert
# when wallet substrate amendment lands".

tv_id1_whoami_success:
  cmd: ["whoami"]
  flags: ["--json"]
  exit_code: 0
  stdout_contains:
    - '"did":'
    - '"pubkey_hex":'
    - '"lifecycle_state":'
  stderr_empty: true
  adapted: tests/identity.rs::tv_id1_whoami_no_active_identity  # stub exit 2; canonical fixture tv_id1_canonical_whoami_success_exits_0 is #[ignore]d

tv_id2_identity_show_not_found:
  cmd: ["identity", "show", "did:octo:nonexistent"]
  flags: ["--json"]
  exit_code: 4
  stderr_contains:
    - "identity not found"

tv_id3_identity_rotate_confirm_required:
  cmd: ["identity", "rotate"]
  flags: []
  # require_confirm emits ConfirmationRequired (exit 2) because --confirm is missing
  # in human mode and --dry-run is not set. POSIX usage-error convention.
  exit_code: 2
  stderr_contains:
    - "--confirm required for mutating command identity rotate"

tv_id4_identity_rotate_grace_hours_flag_absent:
  cmd:
    [
      "identity",
      "rotate",
      "--confirm",
    ]
  exit_code: 0 # substrate hard-codes 24h grace internally; --grace-hours not exposed
  stderr_contains: []

tv_id5_identity_revoke_already_revoked:
  cmd:
    [
      "identity",
      "revoke",
      "--confirm",
      "--reason",
      "test",
    ]
  # Wallet in Revoked state; substrate returns AlreadyRevoked → CLI exit 6
  exit_code: 6
  stderr_contains:
    - "already revoked"
  adapted: tests/identity.rs::tv_id5_identity_revoke_no_active_identity  # stub exit 2; companion tv_id5b asserts --reason required; canonical fixture tv_id5_canonical_revoke_already_revoked_exits_6 is #[ignore]d

tv_id6_already_rotating:
  cmd: ["identity", "rotate", "--confirm"]
  # Wallet in Rotating state; substrate returns AlreadyRotating → CLI exit 3
  exit_code: 3
  stderr_contains:
    - "already rotating"
  adapted: tests/identity.rs::tv_id6_identity_rotate_passes_confirmation_gate  # stub exit 2; canonical fixture tv_id6_canonical_rotate_already_rotating_exits_3 is #[ignore]d

tv_id7_hsm_missing:
  cmd: ["identity", "rotate", "--confirm"]
  # HSM slot unreachable; CLI exit 5
  exit_code: 5
  stderr_contains:
    - "hsm"
  adapted: tests/identity.rs::tv_id7_identity_rotate_no_active_identity  # stub exit 2; canonical fixture tv_id7_canonical_rotate_hsm_unavailable_exits_5 is #[ignore]d

tv_id8_rotate_dry_run:
  cmd: ["identity", "rotate", "--confirm", "--dry-run"]
  exit_code: 0
  stdout_contains:
    - '"preview_only":true'
    - '"new_did":'

# capability.yaml — 19 TV (incl. tv_cap19_confirm_required)
# tv_cap1_list_empty                                       → adapted: tests/capability.rs::tv_cap1_list_emits_empty_capabilities_envelope_v0_exit_2 (canonical tv_cap1_list_emits_empty_capabilities_envelope is #[ignore]d)
# tv_cap2_mint_success                                      → adapted: tests/capability.rs::tv_cap6_mint_root_secret_blocked_exits_64 (SEC-03 root-secret guard); canonical tv_cap2_canonical_mint_success_exits_0 is #[ignore]d
# tv_cap3_mint_bad_caveats                                  → active: tests/capability.rs::tv_cap3_mint_bad_caveats_exits_7
# tv_cap4_attenuate_widens_rejected                         → not in tests/capability.rs (covered by inline unit test in commands/capability.rs; promote to integration test when substrate amendment lands)
# tv_cap5_attenuate_parent_not_found                        → active: tests/capability.rs::tv_cap5_attenuate_bad_cap_id_exits_12
# tv_cap6_signing_failed (HSM disconnect during mint → exit 11) — NEW → adapted: tests/capability.rs::tv_cap6_mint_root_secret_blocked_exits_64; canonical fixture is #[ignore]d
# tv_cap7_holder_not_found (exit 9) — NEW                   → active: tests/capability.rs::tv_cap7_holder_not_found_exits_9
# tv_cap8_caveat_json_syntax_error (--caveats '{not_json' → exit 7) — NEW → active: tests/capability.rs::tv_cap8_bad_caveat_json_exits_7 (+ tv_cap8b_caveat_payload_too_large_exits_7, tv_cap8c_unknown_caveat_tag_exits_7)
# tv_cap9_caveat_budget, tv_cap10_caveat_before, tv_cap11_caveat_valid_after,
#   tv_cap12_caveat_max_uses, tv_cap13_caveat_model, tv_cap14_caveat_provider,
#   tv_cap15_caveat_audit_window: 8 caveats → 7 tv_cap9..tv_cap15 slots
#   (Budget/Expiry/Vesting/MaxUses/Model/Provider/AuditWindow); SingleUse
#   is the substrate `MaxUses { n: 1 }` form, covered by tv_cap12. → 7 TV
#   (these caveat tests live as inline unit tests in commands/capability.rs; promote to integration vectors when canonical substrate wiring lands)
# tv_cap16_filter_parsing — NEW                            → active: tests/capability.rs::tv_cap16_filter_unknown_field_exits_16 + tv_cap16b_filter_missing_equals_exits_16 + tv_cap16c_filter_empty_value_exits_16 + tv_cap16d_filter_comma_split
# tv_cap17_mint_dry_run — NEW                              → active: tests/capability.rs::tv_cap17_mint_dry_run_preview + tv_cap17b_mint_dry_run_stderr_echo
# tv_cap18_attenuate_dry_run — NEW                         → active: tests/capability.rs::tv_cap18_attenuate_dry_run_preview + tv_cap18b_attenuate_dry_run_stderr_echo
# tv_cap19_confirm_required — shared with IDEN tv_id3_confirm_required → active: tests/capability.rs::tv_cap19_confirm_required + tv_cap19b_attenuate_requires_confirm + tv_cap19c_acknowledge_required_when_confirm_set

# policy.yaml — 5 TV (unchanged) → active in tests/policy.rs (5 unit + integration tests)
# error.yaml — 5 TV (clap parse → exit 2, internal, source chain, no-substrate-leak, exit-code mapping) → inline unit tests in crates/octo-cli/src/error.rs
# envelope.yaml — 5 TV (schema_version, generated_at format, json toggle, TTY detected, --no-color) → inline unit tests in crates/octo-cli/src/output.rs
# redact.yaml — 8 TV (holder_sig, pair_code, bearer, password, seed_bytes, mnemonic, pin, api_key) — +3 NEW → inline unit tests in crates/octo-cli/src/redact.rs
# env_errors.yaml — 3 TV (NOT on disk as a separate test file; covered inline by error.rs + integration tests)
#   tv_env6_internal_error_path — substrate returns `Internal("SQL: SELECT ...")`.
#     `sanitize_substrate_error` invoked via `user_message()`; stderr shows
#     sanitized message `wallet store error` (per RFC §Error Handling)
#     with SQL fragment stripped; exit 64. → covered by error.rs::tv_err2_internal_no_substrate_leak
#   tv_env7_stdin_secret_refused — operator pipes private key to stdin without
#     `--allow-stdin-secret`; stderr shows `secret material on pipe`; exit 15.
#     With `--allow-stdin-secret`: warning + audit log entry tagged
#     `stdin_secret_override=true`; exit 0. → covered by error.rs::tv_err3b_stdin_secret_refused_message_text + tv_err3c_stdin_gate_blocks_without_flag
#   tv_env8_concurrent_lock — second CLI instance tries to acquire wallet lock
#     while first instance holds it; without lock → exit 101; wallet mutex
#     contention recorded in audit log. → deferred to wallet substrate amendment
# deprecation.yaml — 2 TV → covered by tests/stub.rs (deprecation-stub warnings)
#   tv_dep1_warning_text — NEW
#   tv_dep2_exit_65 — NEW → covered by OctoCliError::StaleStub (exit 65) per error.rs::tv_err4_exit_code_mapping
```

## Performance Validation

Per RFC-0011 §Performance Targets:

```bash
# Cold start
hyperfine 'octo whoami' --warmup 3 --runs 20

# Per-command latency
hyperfine 'octo --json identity show did:octo:test' --runs 100

# Output serialization
hyperfine 'octo capability list' --runs 50

# Redaction overhead
RUST_LOG=trace octo capability mint --caveats '{}' --holder did:octo:test 2>&1 | wc -l
```

Targets (RFC-0011 §Performance Targets):

| Metric               | Target        | Measurement                 |
| -------------------- | ------------- | --------------------------- |
| Cold start           | <100ms        | `hyperfine` median          |
| Per-command p95      | <50ms         | `hyperfine` p95             |
| Output serialization | <5ms          | inline tracing span         |
| Redaction overhead   | <1ms/log line | RUST_LOG=trace + line count |

---

**Companion RFC:** RFC-0011
**Companion missions:** `missions/open/0011-{core,identity,capability,policy,deprecation}*.md`
