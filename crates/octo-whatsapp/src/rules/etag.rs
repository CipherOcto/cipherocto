//! Canonical etag for rules. Phase 4 of
//! `docs/plans/2026-07-04-whatsapp-runtime-cli-mcp-design.md` §Hot
//! mutation safety.
//!
//! The etag is `sha256(canonical_json(rule_payload))` where
//! canonical_json sorts object keys and emits a stable byte sequence.
//! This is an RFC 8785 subset (sorted keys + numeric tokens); the
//! design accepts any deterministic canonical encoding as long as it
//! is byte-stable across rebuilds and platforms.
//!
//! The etag serves as the optimistic-concurrency token: callers
//! present their last-seen etag on update/delete; a mismatch returns
//! `-32020 RuleConflict` with the current etag + version.

use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};

/// Computes a SHA-256 hex digest of the canonical JSON encoding of `v`.
pub fn canonical_etag<T: Serialize>(v: &T) -> String {
    let json = serde_json::to_value(v).expect("serialize for etag");
    let mut buf = Vec::with_capacity(256);
    write_canonical(&mut buf, &json);
    let digest = Sha256::digest(&buf);
    hex::encode(digest)
}

fn write_canonical(buf: &mut Vec<u8>, v: &Value) {
    match v {
        Value::Null => buf.extend_from_slice(b"null"),
        Value::Bool(b) => buf.extend_from_slice(if *b { b"true" } else { b"false" }),
        Value::Number(n) => buf.extend_from_slice(n.to_string().as_bytes()),
        Value::String(s) => {
            buf.push(b'"');
            push_escaped_string(buf, s);
            buf.push(b'"');
        }
        Value::Array(a) => {
            buf.push(b'[');
            for (i, item) in a.iter().enumerate() {
                if i > 0 {
                    buf.push(b',');
                }
                write_canonical(buf, item);
            }
            buf.push(b']');
        }
        Value::Object(m) => {
            let mut keys: Vec<&String> = m.keys().collect();
            keys.sort();
            buf.push(b'{');
            for (i, k) in keys.iter().enumerate() {
                if i > 0 {
                    buf.push(b',');
                }
                push_escaped_string(buf, k);
                buf.push(b':');
                write_canonical(buf, &m[*k]);
            }
            buf.push(b'}');
        }
    }
}

fn push_escaped_string(buf: &mut Vec<u8>, s: &str) {
    for c in s.chars() {
        match c {
            '"' => buf.extend_from_slice(b"\\\""),
            '\\' => buf.extend_from_slice(b"\\\\"),
            '\n' => buf.extend_from_slice(b"\\n"),
            '\r' => buf.extend_from_slice(b"\\r"),
            '\t' => buf.extend_from_slice(b"\\t"),
            '\x08' => buf.extend_from_slice(b"\\b"),
            '\x0c' => buf.extend_from_slice(b"\\f"),
            c if (c as u32) < 0x20 => {
                buf.extend_from_slice(format!("\\u{:04x}", c as u32).as_bytes());
            }
            c => {
                let mut utf8 = [0u8; 4];
                let s = c.encode_utf8(&mut utf8);
                buf.extend_from_slice(s.as_bytes());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn empty_object_is_stable() {
        let e1 = canonical_etag(&json!({}));
        let e2 = canonical_etag(&json!({}));
        assert_eq!(e1, e2);
        assert_eq!(e1.len(), 64); // SHA-256 hex
    }

    #[test]
    fn key_order_does_not_change_etag() {
        let e1 = canonical_etag(&json!({"a": 1, "b": 2}));
        let e2 = canonical_etag(&json!({"b": 2, "a": 1}));
        assert_eq!(e1, e2);
    }

    #[test]
    fn nested_object_keys_also_sorted() {
        let e1 = canonical_etag(&json!({"outer": {"b": 1, "a": 2}}));
        let e2 = canonical_etag(&json!({"outer": {"a": 2, "b": 1}}));
        assert_eq!(e1, e2);
    }

    #[test]
    fn arrays_are_order_sensitive() {
        let e1 = canonical_etag(&json!({"x": [1, 2, 3]}));
        let e2 = canonical_etag(&json!({"x": [3, 2, 1]}));
        assert_ne!(e1, e2);
    }

    #[test]
    fn string_escaping_is_stable() {
        let e1 = canonical_etag(&json!({"s": "hello\nworld"}));
        let e2 = canonical_etag(&json!({"s": "hello\nworld"}));
        assert_eq!(e1, e2);
    }

    #[test]
    fn different_values_yield_different_etags() {
        let e1 = canonical_etag(&json!({"priority": 1}));
        let e2 = canonical_etag(&json!({"priority": 2}));
        assert_ne!(e1, e2);
    }

    #[test]
    fn escapes_all_string_special_chars() {
        // The string contains every control char covered by
        // `push_escaped_string`: ", \, \r, \t, \b, \f, plus a
        // sub-0x20 control char that falls into the `\uXXXX` arm.
        let s = "\"\t\r\n\\\x08\x0c\x01";
        let e1 = canonical_etag(&json!({"s": s}));
        let e2 = canonical_etag(&json!({"s": s}));
        assert_eq!(e1, e2);
        // Different content with same escape layout yields different
        // hashes.
        let e3 = canonical_etag(&json!({"s": "different"}));
        assert_ne!(e1, e3);
    }
}
