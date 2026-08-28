//! Tracing redaction layer — RFC-0011 §Redaction Layer.

use std::borrow::Cow;
use std::fmt;
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
    ("sig", REDACTED_SIG),
    ("signature", REDACTED_SIG),
    ("holder_sig", REDACTED_SIG),
    ("keypair", REDACTED_PAIR),
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

/// Locate a run of exactly 128 hex characters (an Ed25519 signature).
pub fn find_128_hex(s: &str) -> Option<(usize, usize)> {
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i].is_ascii_hexdigit() {
            let start = i;
            while i < b.len() && b[i].is_ascii_hexdigit() {
                i += 1;
            }
            if i - start == 128 {
                return Some((start, i));
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

/// Redact secret material appearing anywhere in a free-form string.
pub fn redact_string(s: &str) -> Cow<'_, str> {
    let mut owned: Option<String> = None;

    if let Some((start, end)) = find_bearer_ci(s) {
        let mut o = owned.take().unwrap_or_else(|| s.to_string());
        o.replace_range(start..end, REDACTED_BEARER);
        owned = Some(o);
    }

    {
        let current: &str = owned.as_deref().unwrap_or(s);
        if let Some((start, end)) = find_128_hex(current) {
            let mut o = current.to_string();
            o.replace_range(start..end, REDACTED_SIG);
            owned = Some(o);
        }
    }

    // `field=value` patterns for sensitive field names.
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

/// Find the next `sensitive_field=value` span not already redacted.
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
            if !name.is_empty() && !redact_by_field(name, "").is_empty() {
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

/// `tracing` layer that redacts secret material from every emitted event.
#[derive(Debug, Default, Clone, Copy)]
pub struct OctoCliRedactor;

impl<S: Subscriber> Layer<S> for OctoCliRedactor {
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let mut visitor = FieldVisitor::default();
        event.record(&mut visitor);
        for (name, value) in &visitor.fields {
            let safe = redact_by_field(name, value);
            let safe = if std::ptr::eq(safe, value.as_str()) {
                redact_string(value).into_owned()
            } else {
                safe.to_string()
            };
            eprintln!("{name}={safe}");
        }
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
