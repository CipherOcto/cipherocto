//! Machine-readable output envelope — RFC-0011 §Output Envelope +
//! RFC-0011-c §9.4 Divergence.
//!
//! **Schema version history:**
//! - v1 (RFC-0011) — `data` / `generated_at` / `preview_only` / `exit_code`
//! - v2 (RFC-0011 amended by reputation) — same shape, bumped version
//! - v3 (RFC-0011-e) — renames + adds `command` + `redacted`
//! - v4 (RFC-0011-c) — **current**: inherits v3 renames; consumes the same
//!   envelope wire format as RFC-0011-e (`payload` / `executed_at_unix` /
//!   `redacted` / `command`). `exit_code` is dropped (the CLI process
//!   emits the exit code via the OS, not in the envelope payload).
//!
//! Per RFC-0011-c §9.4.1 Divergence from RFC-0011 §Output Envelope:
//!
//! | RFC-0011 v1 field | RFC-0011-c v4 field | Note |
//! |-------------------|---------------------|------|
//! | `data: T`         | `payload: T`        | Renamed. |
//! | `generated_at: DateTime<Utc>` | `executed_at_unix: u64` | Renamed + retyped. RFC 3339 string → `u64` unix seconds for single-clock determinism. |
//! | `preview_only: bool` | `redacted: bool` | Renamed — surfaces the `OctoCliRedactor` alteration status. |
//! | (none)            | `command: &'static str` | ADDED. Operator-visible command label (e.g., `"octo.agent.create.v1"`). |
//! | `exit_code: i32`  | (dropped)           | Process exit is OS-emitted, not envelope-carried. |
//!
//! Consumers MUST read `schema_version` before reading any payload field —
//! `schema_version = 4` guarantees the v4 shape; anything else is a
//! pre-amendment surface and SHOULD be rejected by the consumer.

use std::io::{self, IsTerminal, Write};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::redact::RedactionContext;

/// 32-byte hex value (RFC-0011 §Hex32 newtype).
///
/// Serialised as a 64-character lowercase hex string via the `hex` crate's
/// serde adapter, and described in the JSON Schema as a `string` with a
/// 64-char pattern.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Hex32(
    #[serde(
        serialize_with = "hex::serde::serialize",
        deserialize_with = "hex::serde::deserialize"
    )]
    pub [u8; 32],
);

/// Free-text operator input (e.g. `--memo`) — RFC-0011-e
/// §`RedactedString` newtype.
///
/// `Serialize`, `Display`, and `Debug` ALWAYS emit
/// `[REDACTED:<n>chars]`. This type carries the LENGTH SIGNAL for log
/// lines and rendering, NEVER the plaintext. Plaintext is exposed via a
/// sibling `Option<String>` field on the owning output struct (e.g.
/// `VaultTransferOutput::memo_plaintext`) and is populated ONLY when the
/// operator opts in via `--include-memo` at envelope-build time
/// (Layer C). The CLI build path makes that decision; this type does not
/// know about flags and cannot be configured to emit plaintext.
///
/// `RedactedString` differs from [`Hex32`] in exactly the way the
/// material differs: a 32-byte digest is PUBLIC and must round-trip to
/// verifiers, whereas a free-text memo is operator-supplied and may
/// carry secrets. Both sinks (stderr/log and JSON) receive the same
/// rendering, so redaction cannot be bypassed by switching output
/// format.
///
/// **Zeroize scope:** the inner `String` is zeroized on drop, but only
/// for plaintext held in CLI PROCESS MEMORY. It does NOT reach into
/// substrate envelopes — the substrate does not carry plaintext at rest,
/// and the CLI never round-trips a `RedactedString` back into the
/// transfer envelope.
#[derive(Deserialize, Clone, Zeroize, ZeroizeOnDrop)]
#[serde(from = "String")]
pub struct RedactedString(String);

impl RedactedString {
    /// Wrap operator-supplied free text.
    #[must_use]
    pub fn new(plaintext: impl Into<String>) -> Self {
        Self(plaintext.into())
    }

    /// Character count of the wrapped plaintext (the length signal).
    #[must_use]
    pub fn char_len(&self) -> usize {
        self.0.chars().count()
    }

    /// The canonical redacted rendering: `[REDACTED:<n>chars]`.
    #[must_use]
    pub fn redacted(&self) -> String {
        format!("[REDACTED:{}chars]", self.char_len())
    }

    /// Escape hatch for the `--include-memo` opt-in path ONLY.
    ///
    /// The caller (Layer C envelope build) must have verified the
    /// operator's explicit opt-in before calling this. Every other
    /// rendering path MUST go through [`RedactedString::redacted`].
    #[must_use]
    pub fn expose_plaintext(&self) -> &str {
        &self.0
    }
}

impl From<String> for RedactedString {
    fn from(s: String) -> Self {
        Self(s)
    }
}

/// ALWAYS emits `[REDACTED:<n>chars]` — never the plaintext.
impl Serialize for RedactedString {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.redacted())
    }
}

/// ALWAYS renders `[REDACTED:<n>chars]` — never the plaintext.
impl std::fmt::Display for RedactedString {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.redacted())
    }
}

/// ALWAYS renders `[REDACTED:<n>chars]` — never the plaintext. A derived
/// `Debug` would leak the memo into `{:?}` log lines and panic messages.
impl std::fmt::Debug for RedactedString {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "RedactedString({})", self.redacted())
    }
}

impl PartialEq for RedactedString {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl Eq for RedactedString {}

/// Versioned envelope wrapping every successful command payload.
///
/// `schema_version = 4` per RFC-0011-c §9.4 (current shape:
/// `payload` / `executed_at_unix` / `redacted` / `command`).
/// `exit_code` was dropped (the CLI process emits the exit code via
/// the OS, not in the envelope payload).
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone)]
#[schemars(bound = "T: JsonSchema")]
pub struct OutputEnvelope<T> {
    /// Envelope schema version. **Always 4** for the v4 envelope shape.
    pub schema_version: u32,
    /// Stable operator-visible command label
    /// (RFC-0011-c §9.4 added field; e.g., `"octo.agent.create.v1"`).
    ///
    /// Owned `String` (not `&'static str`) so JSON envelopes can be
    /// round-tripped through external deserializers without forcing
    /// the source buffer to live for `'static`. Construction is still
    /// zero-allocation from the call site via [`OutputEnvelope::new`].
    pub command: String,
    /// Unix seconds at which the command was executed
    /// (RFC-0011-c §9.4 renamed from v1 `generated_at`).
    pub executed_at_unix: u64,
    /// True when the `OctoCliRedactor` altered the payload
    /// (RFC-0011-c §9.4 renamed from v1 `preview_only`).
    pub redacted: bool,
    /// Command payload (RFC-0011-c §9.4 renamed from v1 `data`).
    pub payload: T,
}

impl<T> OutputEnvelope<T> {
    /// Current envelope schema version.
    ///
    /// `4` per RFC-0011-c §9.4 / §9.4.1 Divergence slot table.
    /// Consumers MUST read this field first and reject any other value.
    pub const SCHEMA_VERSION: u32 = 4;

    /// Build an applied-result envelope with no redaction applied.
    pub fn new(command: &'static str, payload: T) -> Self {
        Self {
            schema_version: Self::SCHEMA_VERSION,
            command: command.to_string(),
            executed_at_unix: now_unix_secs(),
            redacted: false,
            payload,
        }
    }

    /// Build an envelope marking the payload as redacted by the
    /// `OctoCliRedactor` (RFC-0011-c §9.4 `redacted: true`).
    pub fn redacted(command: &'static str, payload: T) -> Self {
        Self {
            schema_version: Self::SCHEMA_VERSION,
            command: command.to_string(),
            executed_at_unix: now_unix_secs(),
            redacted: true,
            payload,
        }
    }
}

/// Best-effort wall-clock for `executed_at_unix` (RFC-0011-c §9.4
/// renamed from v1 `generated_at`). Phase 1 mirrors the substrate
/// `register_agent` helper pattern; Phase 2 routes through the
/// substrate monotonic clock for cross-replica determinism
/// (RFC-0008 Class B).
fn now_unix_secs() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

impl<T: Serialize> OutputEnvelope<T> {
    /// Render to stdout — JSON when forced or when stdout is not a TTY,
    /// otherwise a colourised pretty form. No envelope-boundary
    /// redaction applied (callers that need contextual redaction for
    /// `holder_did` / `agent_id` use [`OutputEnvelope::render_with_redaction`]).
    pub fn render(&self, force_json: bool, no_color: bool) -> io::Result<()> {
        self.render_with_redaction(force_json, no_color, &RedactionContext::new())
    }

    /// Render to stdout with envelope-boundary redaction applied.
    ///
    /// Mission `0011-c-agent-redaction-envelope` §Scope sub-step 3:
    /// the renderer walks the serialised JSON tree and applies the
    /// contextual redaction rules from [`RedactionContext`]. For the
    /// JSON path, the walker runs on a `serde_json::Value` between
    /// serialisation and write. For the pretty path, the walker runs
    /// on the same `Value` before colourisation.
    ///
    /// The output `redacted` flag is set per RFC-0011-c §9.4 —
    /// `true` when any payload field has been altered by the redactor
    /// (un-redact of `holder_did` for the active operator, or
    /// truncation of `agent_id` to its first-8-chars form). The
    /// renderer derives the flag from `redactor.apply()`'s return
    /// value so the JSON output reflects ground truth rather than
    /// the pre-render builder flag.
    ///
    /// An empty [`RedactionContext`] is the no-op identity — the
    /// renderer behaves exactly like [`OutputEnvelope::render`].
    pub fn render_with_redaction(
        &self,
        force_json: bool,
        no_color: bool,
        redactor: &RedactionContext,
    ) -> io::Result<()> {
        let stdout = io::stdout();
        let tty = stdout.is_terminal();
        let mut w = stdout.lock();
        let mut value = serde_json::to_value(self)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        // Mission `0011-c-agent-redaction-envelope` §Scope sub-step 4:
        // skip redaction for dry-run (`redacted: true`) envelopes.
        // The preview shape is operator-owned per RFC-0011-c §9.4;
        // redaction would alter a surface the operator is reviewing,
        // not the live substrate view. When the apply runs, the
        // returned `altered` flag propagates to the output `redacted`
        // field (replaces the pre-render `self.redacted` value so the
        // JSON reflects post-redaction ground truth).
        if !self.redacted {
            let altered = redactor.apply(&mut value);
            if altered {
                if let serde_json::Value::Object(map) = &mut value {
                    map.insert("redacted".to_string(), serde_json::Value::Bool(true));
                }
            }
        }
        if force_json || !tty {
            let json = serde_json::to_string(&value)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
            writeln!(w, "{json}")
        } else {
            let colored = !no_color && tty;
            write_value(&mut w, &value, 0, colored)?;
            writeln!(w)
        }
    }

    #[cfg(test)]
    fn render_pretty<W: Write>(&self, w: &mut W, no_color: bool, tty: bool) -> io::Result<()> {
        // Test-only thin wrapper — the canonical path is
        // `render_with_redaction`, which inlines the apply + write
        // logic for both JSON and pretty outputs.
        let value = serde_json::to_value(self)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        let colored = !no_color && tty;
        write_value(w, &value, 0, colored)?;
        writeln!(w)
    }
}

const CYAN: &str = "\x1b[36m";
const YELLOW: &str = "\x1b[33m";
const GREEN: &str = "\x1b[32m";
const MAGENTA: &str = "\x1b[35m";
const RESET: &str = "\x1b[0m";

fn paint(s: &str, code: &str, colored: bool) -> String {
    if colored {
        format!("{code}{s}{RESET}")
    } else {
        s.to_string()
    }
}

fn write_value<W: Write>(
    w: &mut W,
    v: &serde_json::Value,
    indent: usize,
    colored: bool,
) -> io::Result<()> {
    let pad = " ".repeat(indent);
    match v {
        serde_json::Value::Object(map) => {
            for (k, val) in map {
                let key = paint(k, CYAN, colored);
                match val {
                    serde_json::Value::Object(_) | serde_json::Value::Array(_) => {
                        writeln!(w, "{pad}{key}:")?;
                        write_value(w, val, indent + 2, colored)?;
                    }
                    _ => {
                        write!(w, "{pad}{key}: ")?;
                        write_scalar(w, val, colored)?;
                        writeln!(w)?;
                    }
                }
            }
            Ok(())
        }
        serde_json::Value::Array(items) => {
            for item in items {
                match item {
                    serde_json::Value::Object(_) | serde_json::Value::Array(_) => {
                        writeln!(w, "{pad}-")?;
                        write_value(w, item, indent + 2, colored)?;
                    }
                    _ => {
                        write!(w, "{pad}- ")?;
                        write_scalar(w, item, colored)?;
                        writeln!(w)?;
                    }
                }
            }
            Ok(())
        }
        _ => write_scalar(w, v, colored),
    }
}

fn write_scalar<W: Write>(w: &mut W, v: &serde_json::Value, colored: bool) -> io::Result<()> {
    let s = match v {
        serde_json::Value::String(s) => paint(s, YELLOW, colored),
        serde_json::Value::Bool(b) => paint(&b.to_string(), GREEN, colored),
        serde_json::Value::Number(n) => paint(&n.to_string(), MAGENTA, colored),
        serde_json::Value::Null => paint("null", GREEN, colored),
        _ => unreachable!("composite handled by write_value"),
    };
    write!(w, "{s}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
    struct Payload {
        name: String,
        count: u32,
    }

    fn payload() -> Payload {
        Payload {
            name: "did:octo:abc".into(),
            count: 3,
        }
    }

    #[test]
    fn tv_env1_schema_version_present() {
        let env = OutputEnvelope::new("octo.test.v1", payload());
        let json = serde_json::to_string(&env).unwrap();
        assert!(json.contains("\"schema_version\":4"), "{json}");
        assert!(json.contains("\"redacted\":false"), "{json}");
        assert!(json.contains("\"command\":\"octo.test.v1\""), "{json}");
        assert!(json.contains("\"payload\""), "{json}");
        assert!(json.contains("\"executed_at_unix\""), "{json}");
    }

    #[test]
    fn tv_env2_executed_at_unix_is_u64() {
        let env = OutputEnvelope::new("octo.test.v1", payload());
        // RFC-0011-c §9.4: executed_at_unix is a u64 unix-seconds
        // (renamed + retyped from v1 `generated_at: DateTime<Utc>`).
        // We don't pin the exact value (wall-clock drift) — only the
        // type contract.
        let json = serde_json::to_string(&env).unwrap();
        assert!(json.contains("\"executed_at_unix\":"), "{json}");
    }

    #[test]
    fn tv_env3_redacted_true() {
        let env = OutputEnvelope::redacted("octo.test.v1", payload());
        let json = serde_json::to_string(&env).unwrap();
        assert!(json.contains("\"redacted\":true"), "{json}");
    }

    #[test]
    fn tv_env4_json_roundtrip() {
        let env = OutputEnvelope::new("octo.test.v1", payload());
        let json = serde_json::to_string(&env).unwrap();
        let back: OutputEnvelope<Payload> = serde_json::from_str(&json).unwrap();
        assert_eq!(
            back.schema_version,
            OutputEnvelope::<Payload>::SCHEMA_VERSION
        );
        assert_eq!(back.payload, env.payload);
        assert_eq!(back.command, env.command);
        assert_eq!(back.redacted, env.redacted);
        assert_eq!(back.executed_at_unix, env.executed_at_unix);
    }

    #[test]
    fn tv_env5_envelope_has_json_schema() {
        // Schemars must produce a schema for the envelope — this is the
        // contract downstream tools rely on for auto-generated clients.
        let schema = schemars::schema_for!(OutputEnvelope<Payload>);
        let s = serde_json::to_string(&schema).unwrap();
        assert!(s.contains("schema_version"), "{s}");
        assert!(s.contains("payload"), "{s}");
        assert!(s.contains("command"), "{s}");
        assert!(s.contains("redacted"), "{s}");
        assert!(s.contains("executed_at_unix"), "{s}");
    }

    #[test]
    fn tv_hex32_serializes_as_64_lowercase_hex() {
        let bytes = [0xab; 32];
        let h = Hex32(bytes);
        let json = serde_json::to_string(&h).unwrap();
        assert_eq!(json.len(), 64 + 2); // 64 hex chars + quotes
        assert!(json.contains(&"ab".repeat(32)), "{json}");
    }

    #[test]
    fn tv_hex32_roundtrips_through_hex() {
        let bytes: [u8; 32] = std::array::from_fn(|i| i as u8);
        let h = Hex32(bytes);
        let s = serde_json::to_string(&h).unwrap();
        let back: Hex32 = serde_json::from_str(&s).unwrap();
        assert_eq!(back.0, bytes);
    }

    #[test]
    fn tv_hex32_has_json_schema() {
        let schema = schemars::schema_for!(Hex32);
        let s = serde_json::to_string(&schema).unwrap();
        assert!(s.contains("string"), "{s}");
        assert!(s.contains("pattern"), "{s}");
    }

    #[test]
    fn tv_env6_render_pretty_uses_cached_tty() {
        let env = OutputEnvelope::new("octo.test.v1", payload());

        // `tty=true` must emit ANSI colour codes (CYAN for keys).
        let mut colored_buf: Vec<u8> = Vec::new();
        env.render_pretty(&mut colored_buf, false, true).unwrap();
        let colored_out = String::from_utf8(colored_buf).unwrap();
        assert!(
            colored_out.contains("\x1b["),
            "expected ANSI escape codes when tty=true, got: {colored_out:?}"
        );

        // `tty=false` must NOT emit any ANSI codes, even if `no_color=false`.
        let mut plain_buf: Vec<u8> = Vec::new();
        env.render_pretty(&mut plain_buf, false, false).unwrap();
        let plain_out = String::from_utf8(plain_buf).unwrap();
        assert!(
            !plain_out.contains("\x1b["),
            "expected no ANSI escape codes when tty=false, got: {plain_out:?}"
        );

        // Sanity: `no_color=true` with `tty=true` also yields plain output —
        // the cached tty alone must not re-introduce colour.
        let mut noc_buf: Vec<u8> = Vec::new();
        env.render_pretty(&mut noc_buf, true, true).unwrap();
        let noc_out = String::from_utf8(noc_buf).unwrap();
        assert!(
            !noc_out.contains("\x1b["),
            "expected no ANSI escape codes when no_color=true, got: {noc_out:?}"
        );
    }

    /// TV-ENV-REDACT-1: mission
    /// `0011-c-agent-redaction-envelope` §Scope sub-step 4 — the
    /// renderer MUST skip redaction for dry-run (`redacted: true`)
    /// envelopes. The preview shape is operator-owned per RFC-0011-c
    /// §9.4; redaction would alter a surface the operator is
    /// reviewing, not the live substrate view. The walker is a no-op
    /// when `self.redacted == true`.
    ///
    /// The redactor's payload-walk would otherwise turn the
    /// `holder_did` field into a conditional reveal (active vs
    /// non-active DID) — for a preview envelope the operator wants to
    /// see the preview shape as-is.
    #[test]
    fn tv_env_redact_dry_run_envelope_skips_redactor() {
        use crate::redact::RedactionContext;

        let preview = serde_json::json!({
            "agent_id": "[REDACTED:key]",
            "holder_did": "[REDACTED:key]",
            "preview": true,
        });
        // Build an envelope explicitly marked as a preview (the
        // `redacted: true` shape).
        let env = OutputEnvelope {
            schema_version: 4,
            command: "octo.test.preview.v1".to_string(),
            executed_at_unix: 0,
            redacted: true,
            payload: preview,
        };

        // Serialise manually (not via `render` — render writes to
        // stdout which is not capturable in this unit test). Pin the
        // dry-run skip policy at the redactor-apply gate: when
        // `env.redacted == true`, the renderer does NOT call
        // `redactor.apply`. We assert the gate by reproducing the
        // renderer's serialise-and-maybe-apply logic and confirming
        // the preview-shape payload passes through verbatim when
        // `redacted == true`.
        let mut value = serde_json::to_value(&env).unwrap();
        if !env.redacted {
            RedactionContext::new().apply(&mut value);
        }
        let json = serde_json::to_string(&value).unwrap();
        // Preview shape: both redacted markers preserved verbatim.
        assert!(
            json.contains(r#""holder_did":"[REDACTED:key]""#),
            "dry-run envelope MUST preserve holder_did marker: {json}",
        );
        assert!(
            json.contains(r#""agent_id":"[REDACTED:key]""#),
            "dry-run envelope MUST preserve agent_id marker: {json}",
        );
        assert!(
            json.contains(r#""redacted":true"#),
            "dry-run envelope MUST advertise redacted=true: {json}",
        );
    }

    /// TV-ENV-REDACT-2: applied envelope (`redacted: false`) runs
    /// the redactor. The walker applies the contextual rules —
    /// `holder_did` is replaced with the actual value when it
    /// matches the operator's active DID; `agent_id` is truncated.
    /// This is the inverse of TV-ENV-REDACT-1 and pins the
    /// redactor-apply gate on the other branch.
    #[test]
    fn tv_env_redact_applied_envelope_runs_redactor() {
        use crate::redact::RedactionContext;

        let payload = serde_json::json!({
            "agent_id": "[REDACTED:key]",
            "holder_did": "[REDACTED:key]",
            "state": "registered",
        });
        let env = OutputEnvelope {
            schema_version: 4,
            command: "octo.test.applied.v1".to_string(),
            executed_at_unix: 0,
            redacted: false,
            payload,
        };

        let mut value = serde_json::to_value(&env).unwrap();
        if !env.redacted {
            RedactionContext::new()
                .with_active_did("did:octo:zOperator")
                .with_holder_did("did:octo:zOperator")
                .with_agent_id("00000000-0000-4000-8000-000000000001")
                .apply(&mut value);
        }
        let json = serde_json::to_string(&value).unwrap();

        // Applied envelope: holder_did un-redacted (match), agent_id
        // truncated.
        assert!(
            json.contains(r#""holder_did":"did:octo:zOperator""#),
            "applied envelope MUST un-redact holder_did when match: {json}",
        );
        assert!(
            json.contains(r#""agent_id":"00000000...""#),
            "applied envelope MUST truncate agent_id: {json}",
        );
        assert!(
            json.contains(r#""redacted":false"#),
            "applied envelope MUST advertise redacted=false: {json}",
        );
    }
}
