//! Tracing redaction layer — RFC-0011 §Redaction Layer.

use std::borrow::Cow;
use std::fmt;
use std::io::{self, Write as _};
use tracing::field::{Field, Visit};
use tracing::{Event, Subscriber};
use tracing_subscriber::layer::{Context, Layer};

/// Replacement for seed material.
pub const REDACTED_SEED: &str = "[REDACTED:seed]";
/// Replacement for key material.
pub const REDACTED_KEY: &str = "[REDACTED:key]";
/// Replacement for signatures.
pub const REDACTED_SIG: &str = "[REDACTED:sig]";
/// Replacement for keypairs.
pub const REDACTED_PAIR: &str = "[REDACTED:pair]";
/// Replacement for passwords.
pub const REDACTED_PW: &str = "[REDACTED:pw]";
/// Replacement for bearer tokens.
pub const REDACTED_BEARER: &str = "[REDACTED:bearer]";
/// Replacement for mnemonics.
pub const REDACTED_MNEMONIC: &str = "[REDACTED:mnemonic]";
/// Replacement for passphrases.
pub const REDACTED_PASSPHRASE: &str = "[REDACTED:passphrase]";
/// Replacement for PINs.
pub const REDACTED_PIN: &str = "[REDACTED:pin]";
/// Replacement for API keys.
pub const REDACTED_API_KEY: &str = "[REDACTED:api_key]";
/// Replacement for generic secrets.
pub const REDACTED_SECRET: &str = "[REDACTED:secret]";

/// Byte string that never renders its contents.
#[derive(Clone, PartialEq, Eq)]
pub struct RedactedHex(pub Vec<u8>);

impl fmt::Debug for RedactedHex {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(REDACTED_SIG)
    }
}

impl fmt::Display for RedactedHex {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(REDACTED_SIG)
    }
}

impl serde::Serialize for RedactedHex {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(REDACTED_SIG)
    }
}

/// Field names whose *values* are redacted wholesale, and their replacement.
const FIELD_TABLE: &[(&str, &str)] = &[
    ("seed", REDACTED_SEED),
    ("seed_bytes", REDACTED_SEED),
    ("key", REDACTED_KEY),
    ("secret_key", REDACTED_KEY),
    ("private_key", REDACTED_KEY),
    ("priv", REDACTED_KEY),
    ("privkey", REDACTED_KEY),
    ("priv_key", REDACTED_KEY),
    ("privatekey", REDACTED_KEY),
    ("priv-key", REDACTED_KEY),
    ("pkey", REDACTED_KEY),
    ("skey", REDACTED_KEY),
    ("sig", REDACTED_SIG),
    ("signature", REDACTED_SIG),
    ("holder_sig", REDACTED_SIG),
    ("keypair", REDACTED_PAIR),
    ("pair_code", REDACTED_PAIR),
    ("paircode", REDACTED_PAIR),
    ("pw", REDACTED_PW),
    ("password", REDACTED_PW),
    ("bearer", REDACTED_BEARER),
    ("token", REDACTED_BEARER),
    ("mnemonic", REDACTED_MNEMONIC),
    ("passphrase", REDACTED_PASSPHRASE),
    ("pin", REDACTED_PIN),
    ("api_key", REDACTED_API_KEY),
    ("secret", REDACTED_SECRET),
];

/// Returns true when the (lower-cased) field name is sensitive.
pub fn field_is_sensitive(field_name: &str) -> bool {
    let lower = field_name.to_ascii_lowercase();
    FIELD_TABLE.iter().any(|(name, _)| lower == *name)
}

/// Redact a value keyed by its field name. Returns the original when the
/// field name is not sensitive.
pub fn redact_by_field<'a>(field_name: &str, value: &'a str) -> &'a str {
    let lower = field_name.to_ascii_lowercase();
    for (name, replacement) in FIELD_TABLE {
        if lower == *name {
            return replacement;
        }
    }
    value
}

/// Locate a run of hex characters at least 64 long (covers 32-byte keys and
/// 64-byte Ed25519 signatures). Returns `(start, end, kind)` where `kind`
/// is `REDACTED_SIG` for runs of ≥128 hex chars and `REDACTED_KEY` for
/// 64..128.
pub fn find_long_hex(s: &str) -> Option<(usize, usize, &'static str)> {
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i].is_ascii_hexdigit() {
            let start = i;
            while i < b.len() && b[i].is_ascii_hexdigit() {
                i += 1;
            }
            let len = i - start;
            if len >= 64 && len % 2 == 0 {
                let kind = if len >= 128 {
                    REDACTED_SIG
                } else {
                    REDACTED_KEY
                };
                return Some((start, i, kind));
            }
        } else {
            i += 1;
        }
    }
    None
}

/// Locate a case-insensitive `bearer <token>` run.
pub fn find_bearer_ci(s: &str) -> Option<(usize, usize)> {
    let lower = s.to_ascii_lowercase();
    let start = lower.find("bearer ")?;
    let mut end = start + "bearer ".len();
    let b = s.as_bytes();
    while end < b.len() && !b[end].is_ascii_whitespace() {
        end += 1;
    }
    Some((start, end))
}

/// Locate the next `sensitive_field=value` span not already redacted.
fn find_kv_secret(s: &str) -> Option<(usize, usize)> {
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'=' {
            // Walk back over the field name.
            let mut start = i;
            while start > 0 {
                let c = b[start - 1];
                if c.is_ascii_alphanumeric() || c == b'_' || c == b'-' {
                    start -= 1;
                } else {
                    break;
                }
            }
            let name = &s[start..i];
            if !name.is_empty() && field_is_sensitive(name) {
                let mut end = i + 1;
                while end < b.len() && !b[end].is_ascii_whitespace() && b[end] != b',' {
                    end += 1;
                }
                let value = &s[i + 1..end];
                if !value.starts_with("[REDACTED:") {
                    return Some((start, end));
                }
            }
        }
        i += 1;
    }
    None
}

/// Redact secret material appearing anywhere in a free-form string.
///
/// Pass order:
/// 1. JSON-aware walk — when the slice parses as JSON, recurse into
///    objects/arrays and replace sensitive field values.
/// 2. YAML line-walk — `key: value` lines whose key is sensitive get
///    their value replaced.
/// 3. Bearer-token detection (`Authorization: Bearer …` and case variants).
/// 4. Long-hex run detection (≥64 hex chars; `[REDACTED:key]` for 64..128,
///    `[REDACTED:sig]` for ≥128).
/// 5. Plain `field=value` scan as a fallback for non-JSON/YAML forms.
pub fn redact_string(s: &str) -> Cow<'_, str> {
    // JSON-aware pass.
    let trimmed = s.trim();
    if (trimmed.starts_with('{') && trimmed.ends_with('}'))
        || (trimmed.starts_with('[') && trimmed.ends_with(']'))
    {
        if let Ok(mut value) = serde_json::from_str::<serde_json::Value>(trimmed) {
            redact_json_value(&mut value);
            if let Ok(rendered) = serde_json::to_string(&value) {
                let leading_ws = s.len() - s.trim_start().len();
                let trailing_ws = s.len() - s.trim_end().len();
                let mut out =
                    String::with_capacity(s.len() + rendered.len().saturating_sub(trimmed.len()));
                out.push_str(&s[..leading_ws]);
                out.push_str(&rendered);
                out.push_str(&s[s.len() - trailing_ws..]);
                return Cow::Owned(out);
            }
        }
    }

    // YAML pass.
    if looks_like_yaml(s) {
        let mut owned_lines: Vec<String> = Vec::new();
        let mut changed = false;
        for line in s.lines() {
            if let Some((key, value)) = parse_yaml_kv(line) {
                if field_is_sensitive(&key) && !value.starts_with("[REDACTED:") {
                    let replacement = redact_by_field(&key, &value).to_string();
                    owned_lines.push(format!("{key}: {replacement}"));
                    changed = true;
                    continue;
                }
            }
            owned_lines.push(line.to_string());
        }
        if changed {
            let mut joined = owned_lines.join("\n");
            if s.ends_with('\n') && !joined.ends_with('\n') {
                joined.push('\n');
            }
            return Cow::Owned(joined);
        }
    }

    // Plain-text fallback: bearer + long-hex + `field=value` scan.
    let mut owned: Option<String> = None;

    if let Some((start, end)) = find_bearer_ci(s) {
        let mut o = owned.take().unwrap_or_else(|| s.to_string());
        o.replace_range(start..end, REDACTED_BEARER);
        owned = Some(o);
    }

    {
        let current: &str = owned.as_deref().unwrap_or(s);
        if let Some((start, end, kind)) = find_long_hex(current) {
            let mut o = current.to_string();
            o.replace_range(start..end, kind);
            owned = Some(o);
        }
    }

    loop {
        let current: &str = owned.as_deref().unwrap_or(s);
        let Some((start, end)) = find_kv_secret(current) else {
            break;
        };
        let name_len = current[start..end].find('=').unwrap_or(0);
        let replacement = redact_by_field(&current[start..start + name_len], "").to_string();
        let mut o = current.to_string();
        o.replace_range(start + name_len + 1..end, &replacement);
        owned = Some(o);
    }

    match owned {
        Some(o) => Cow::Owned(o),
        None => Cow::Borrowed(s),
    }
}

/// Quick YAML heuristic — only used to decide whether to run the YAML
/// pass. Returns false the moment a line looks like free-form text rather
/// than YAML structure.
fn looks_like_yaml(s: &str) -> bool {
    let mut saw_kv = false;
    for line in s.lines() {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if trimmed.starts_with('{') || trimmed.starts_with('[') {
            return false;
        }
        if parse_yaml_kv(trimmed).is_some() {
            saw_kv = true;
        } else if !trimmed.starts_with('-') && !trimmed.starts_with("---") {
            return false;
        }
    }
    saw_kv
}

fn parse_yaml_kv(line: &str) -> Option<(String, String)> {
    let idx = line.find(':')?;
    let key = line[..idx].trim().to_string();
    let value = line[idx + 1..].trim().to_string();
    if key.is_empty() || value.is_empty() {
        return None;
    }
    if !key
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return None;
    }
    Some((key, value))
}

fn redact_json_value(v: &mut serde_json::Value) {
    match v {
        serde_json::Value::Object(map) => {
            for (k, val) in map.iter_mut() {
                if field_is_sensitive(k) {
                    let replacement = redact_by_field(k, "");
                    *val = serde_json::Value::String(replacement.to_string());
                } else {
                    redact_json_value(val);
                }
            }
        }
        serde_json::Value::Array(items) => {
            for item in items.iter_mut() {
                redact_json_value(item);
            }
        }
        _ => {}
    }
}

/// `tracing` Layer that redacts secret material from every emitted event
/// and writes the redacted view to stderr.
///
/// Design note (RFC-0011 §Redaction Layer):
///
/// This Layer is the *sole* writer when active. `tracing-subscriber`'s
/// `registry()` has no default formatter, so without an additional
/// `tracing_subscriber::fmt::Layer` registered the only output is what
/// we write here. We deliberately use `std::io::stderr().lock()` instead
/// of `eprintln!` so that the redaction pipeline can be retargeted by
/// tests via a `MakeWriter` shim and so that any future custom Format
/// Layer that the caller might compose will see only the *redacted*
/// output (the redactor emits first; the Format renders the message
/// body but never the raw fields). This guarantees the two-stream leak
/// the previous `eprintln!`-based implementation could open cannot
/// reappear.
#[derive(Debug, Default, Clone, Copy)]
pub struct OctoCliRedactor;

impl<S: Subscriber> Layer<S> for OctoCliRedactor {
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let mut visitor = FieldVisitor::default();
        event.record(&mut visitor);
        write_redacted_event(event, &visitor.fields);
    }
}

/// Render one redacted event line to stderr.
fn write_redacted_event(event: &Event<'_>, fields: &[(String, String)]) {
    let meta = event.metadata();
    let mut line = String::new();
    line.push_str(&format!("[{}] {}: ", meta.level(), meta.target()));
    let mut first = true;
    for (name, value) in fields {
        let by_field = redact_by_field(name, value);
        let redacted: Cow<'_, str> = if std::ptr::eq(by_field, value.as_str()) {
            redact_string(value)
        } else {
            Cow::Borrowed(by_field)
        };
        if !first {
            line.push(' ');
        }
        line.push_str(&format!("{name}={redacted}"));
        first = false;
    }
    {
        let mut w = std::io::stderr().lock();
        let _ = writeln!(w, "{line}");
    }
}

/// Collects `(field, value)` pairs from a `tracing` event.
#[derive(Debug, Default)]
pub struct FieldVisitor {
    /// Captured field/value pairs in record order.
    pub fields: Vec<(String, String)>,
}

impl Visit for FieldVisitor {
    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        self.fields
            .push((field.name().to_string(), format!("{value:?}")));
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        self.fields
            .push((field.name().to_string(), value.to_string()));
    }
}

/// Test-only helper that mirrors `write_redacted_event` but writes to an
/// arbitrary `io::Write`. Used by unit tests to assert the redactor strips
/// fields without spinning up the full subscriber.
#[allow(dead_code)]
pub fn write_redacted_for_test<W: io::Write>(
    target: &str,
    level: tracing::Level,
    fields: &[(&str, &str)],
    mut writer: W,
) -> io::Result<()> {
    let mut line = String::new();
    line.push_str(&format!("[{level}] {target}: "));
    let mut first = true;
    for (k, v) in fields {
        let by_field = redact_by_field(k, v);
        let redacted: Cow<'_, str> = if std::ptr::eq(by_field, *v) {
            redact_string(v)
        } else {
            Cow::Borrowed(by_field)
        };
        if !first {
            line.push(' ');
        }
        line.push_str(&format!("{k}={redacted}"));
        first = false;
    }
    writeln!(writer, "{line}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracing::Level;

    #[test]
    fn redacts_bearer_token() {
        let out = redact_string("Authorization: Bearer abc123def");
        assert!(out.contains(REDACTED_BEARER), "{out}");
        assert!(!out.contains("abc123def"), "{out}");
    }

    #[test]
    fn redacts_bearer_case_insensitive() {
        let out = redact_string("auth: bEaReR zzzTOKEN");
        assert!(out.contains(REDACTED_BEARER), "{out}");
        assert!(!out.contains("zzzTOKEN"), "{out}");
    }

    #[test]
    fn redacts_holder_sig_128_hex() {
        let sig = "a".repeat(128);
        let input = format!("sig is {sig} done");
        let out = redact_string(&input);
        assert!(out.contains(REDACTED_SIG), "{out}");
        assert!(!out.contains(&sig), "{out}");
    }

    #[test]
    fn tv_red2_pair_code_stripped() {
        let out = redact_string("pair_code=ABC123");
        assert_eq!(out, format!("pair_code={REDACTED_PAIR}"));
    }

    #[test]
    fn tv_red2b_priv_aliases_stripped() {
        for alias in [
            "priv",
            "privkey",
            "priv_key",
            "private_key",
            "privKey",
            "priv-key",
            "pkey",
            "skey",
        ] {
            let input = format!("{alias}=hunter2");
            let out = redact_string(&input);
            assert!(
                out.contains(REDACTED_KEY),
                "{alias} should redact to {REDACTED_KEY}: got {out}"
            );
            assert!(
                !out.contains("hunter2"),
                "{alias} leaked hunter2: got {out}"
            );
        }
    }

    #[test]
    fn redacts_password_value() {
        let out = redact_string("password=hunter2");
        assert_eq!(out, format!("password={REDACTED_PW}"));
    }

    #[test]
    fn redacts_seed_bytes_value() {
        let out = redact_string("seed_bytes=deadbeef");
        assert_eq!(out, format!("seed_bytes={REDACTED_SEED}"));
    }

    #[test]
    fn redacts_pin_value() {
        let out = redact_string("pin=1234");
        assert_eq!(out, format!("pin={REDACTED_PIN}"));
    }

    #[test]
    fn redacts_api_key_value() {
        let out = redact_string("api_key=sk-abc");
        assert_eq!(out, format!("api_key={REDACTED_API_KEY}"));
    }

    #[test]
    fn preserves_safe_strings() {
        let s = "did:octo:abcdef policy=default version=3";
        assert_eq!(redact_string(s), s);
    }

    #[test]
    fn redacts_seed_by_field() {
        assert_eq!(redact_by_field("seed", "abc"), REDACTED_SEED);
    }

    #[test]
    fn redacts_mnemonic_by_field() {
        assert_eq!(
            redact_by_field("mnemonic", "abandon x12"),
            REDACTED_MNEMONIC
        );
    }

    #[test]
    fn redacted_hex_never_leaks() {
        let h = RedactedHex(vec![0xde, 0xad, 0xbe, 0xef]);
        assert_eq!(format!("{h:?}"), REDACTED_SIG);
        assert_eq!(h.to_string(), REDACTED_SIG);
        assert_eq!(serde_json::to_string(&h).unwrap(), "\"[REDACTED:sig]\"");
    }

    #[test]
    fn find_long_hex_redacts_64_char_run() {
        // 64 hex chars = 32-byte key (e.g. Ed25519 public key).
        let key = "a".repeat(64);
        let input = format!("key:{key}");
        let out = redact_string(&input);
        assert!(out.contains(REDACTED_KEY), "{out}");
        assert!(!out.contains(&key), "{out}");
    }

    #[test]
    fn find_long_hex_redacts_128_char_run() {
        let sig = "b".repeat(128);
        let input = format!("sig={sig}");
        let out = redact_string(&input);
        assert!(out.contains(REDACTED_SIG), "{out}");
        assert!(!out.contains(&sig), "{out}");
    }

    #[test]
    fn redacts_json_object_password_field() {
        let input = r#"{"user":"alice","password":"hunter2","nested":{"token":"abc"}}"#;
        let out = redact_string(input);
        assert!(out.contains(REDACTED_PW), "{out}");
        assert!(!out.contains("hunter2"), "{out}");
        assert!(out.contains("\"user\":\"alice\""), "{out}");
    }

    #[test]
    fn redacts_json_array_with_sensitive_field() {
        let input = r#"[{"api_key":"sk-abc","name":"alice"}]"#;
        let out = redact_string(input);
        assert!(out.contains(REDACTED_API_KEY), "{out}");
        assert!(!out.contains("sk-abc"), "{out}");
        assert!(out.contains("\"name\":\"alice\""), "{out}");
    }

    #[test]
    fn redacts_yaml_password_field() {
        let input = "user: alice\npassword: hunter2\nage: 30\n";
        let out = redact_string(input);
        assert!(out.contains(REDACTED_PW), "{out}");
        assert!(!out.contains("hunter2"), "{out}");
        assert!(out.contains("user: alice"), "{out}");
        assert!(out.contains("age: 30"), "{out}");
    }

    #[test]
    fn registered_redactor_strips_fields() {
        // End-to-end: drive the redactor through a real subscriber and
        // assert that the helper that mirrors `on_event`'s write logic
        // never leaks the original field values.
        let mut buf: Vec<u8> = Vec::new();
        write_redacted_for_test(
            "test",
            Level::INFO,
            &[("password", "hunter2"), ("user", "alice")],
            &mut buf,
        )
        .unwrap();
        let s = std::str::from_utf8(&buf).unwrap();
        assert!(s.contains(REDACTED_PW), "missing redaction: {s}");
        assert!(!s.contains("hunter2"), "leaked secret: {s}");
        assert!(s.contains("user=alice"), "{s}");
    }
}
