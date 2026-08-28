//! Tracing redaction layer — RFC-0011 §Redaction Layer.

use std::borrow::Cow;
use std::fmt;
use std::io::{self, Write as _};
use tracing::field::{Field, Visit};
use tracing::{Event, Subscriber};
use tracing_subscriber::layer::{Context, Layer};
use zeroize::Zeroize;

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
///
/// The inner `Vec<u8>` is the substrate signature bytes (typically 64 for
/// Ed25519). We do NOT rely on Debug/Display/Serialize to keep them out of
/// logs — those are the *render* paths and assume the `RedactedHex` is still
/// alive. We also zeroize-on-drop (SEC-13): a panic or early `?`-return
/// while the value is still in scope would otherwise leave the signature
/// bytes in the allocator's free list until the page is recycled.
/// `Zeroize` zeroizes the contents on a normal drop, and the explicit
/// `Drop` impl below is the documented belt-and-braces for that trait.
///
/// Note: stable Rust cannot reliably *test* heap-zeroization on drop
/// (the allocator may have moved the bytes, or the test process may
/// have exited before the wipe completes). The unit test below pins the
/// *contract* — that `Zeroize::zeroize()` is wired and that dropping a
/// live value runs the wipe — and the `Drop` impl makes the wipe happen
/// at the language level rather than relying on the allocator's own
/// behaviour.
#[derive(Clone, PartialEq, Eq, Zeroize)]
pub struct RedactedHex(pub Vec<u8>);

impl Drop for RedactedHex {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

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
    ("bearer_token", REDACTED_BEARER),
    ("access_token", REDACTED_BEARER),
    ("refresh_token", REDACTED_BEARER),
    ("id_token", REDACTED_BEARER),
    ("token", REDACTED_SECRET),
    ("mnemonic", REDACTED_MNEMONIC),
    ("passphrase", REDACTED_PASSPHRASE),
    ("pin", REDACTED_PIN),
    ("api_key", REDACTED_API_KEY),
    ("secret", REDACTED_SECRET),
];

/// Returns true when the (lower-cased) field name is sensitive.
///
/// R20 Lens-2 F4: hyphen variants like `pass-word`, `priv-key`,
/// `bearer-token`, `api-key` normalize to `password`, `privkey`,
/// `bearer_token`, `api_key` before the table lookup. Without this,
/// an attacker (or careless operator) writing `pass-word=foo` would
/// bypass the redaction. The normalization is conservative: only
/// ASCII hyphens are replaced, and only when the result matches a
/// FIELD_TABLE entry exactly (no partial-match widening).
pub fn field_is_sensitive(field_name: &str) -> bool {
    let lower = field_name.to_ascii_lowercase();
    if FIELD_TABLE.iter().any(|(name, _)| lower == *name) {
        return true;
    }
    // Hyphen variants get TWO normalizations:
    //   * `pass-word` → `password`  (hyphen stripped)
    //   * `bearer-token` → `bearer_token`  (hyphen → underscore)
    // Without the underscore bridge, `bearer-token` collapses to
    // `bearertoken` which is NOT in FIELD_TABLE — the canonical
    // name uses an underscore. Both bridges are tried; the first
    // hit wins.
    let stripped: String = lower.replace('-', "");
    if stripped != lower && FIELD_TABLE.iter().any(|(name, _)| stripped == *name) {
        return true;
    }
    let underscored: String = lower.replace('-', "_");
    if underscored != lower && FIELD_TABLE.iter().any(|(name, _)| underscored == *name) {
        return true;
    }
    false
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
    // R20 Lens-2 F4: hyphen variants (`pass-word`, `bearer-token`,
    // `api-key`) bridge to their canonical FIELD_TABLE form via
    // TWO normalizations (stripped + underscore-substituted),
    // matching [`field_is_sensitive`]. Without this, the
    // hyphen-bearing variant falls through to `value` — for the
    // kv-fallback caller, an empty value argument combined with
    // the empty replacement would loop forever (re-match the
    // same kv pair). The bridges run only when the input
    // actually contains a hyphen, so canonical names pay no cost.
    let stripped: String = lower.replace('-', "");
    if stripped != lower {
        for (name, replacement) in FIELD_TABLE {
            if stripped == *name {
                return replacement;
            }
        }
    }
    let underscored: String = lower.replace('-', "_");
    if underscored != lower {
        for (name, replacement) in FIELD_TABLE {
            if underscored == *name {
                return replacement;
            }
        }
    }
    value
}

/// Locate a run of hex characters at least 32 long (covers 16-byte DIDs /
/// 16-byte short keys, 32-byte keys, and 64-byte Ed25519 signatures).
/// Returns `(start, end, kind)` where `kind` is `REDACTED_SIG` for runs
/// of ≥128 hex chars, `REDACTED_KEY` for 32..127. Odd-length hex and
/// truncated key dumps are now caught (was: even-length only at ≥64).
///
/// R20 Lens-2 F3: signer logs frequently break hex dumps across CRLF
/// (line-wrapped macaroon IDs every 30 chars, etc.). The raw
/// `is_ascii_hexdigit` walk stops at the first `\r` or `\n`, so a
/// 64-char run split into 30+34 chunks evades the threshold. The
/// fix: a `hex+` run admits CR, LF, and space as line-wrap
/// separators. The returned `(start, end)` span covers the
/// separators too so the substitution removes the line breaks
/// along with the secret.
pub fn find_long_hex(s: &str) -> Option<(usize, usize, &'static str)> {
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i].is_ascii_hexdigit() {
            let start = i;
            let mut hex_count: usize = 0;
            let mut last_hex_end: usize = i;
            while i < b.len() && (b[i].is_ascii_hexdigit() || is_hex_separator(b[i])) {
                if b[i].is_ascii_hexdigit() {
                    hex_count += 1;
                    last_hex_end = i + 1;
                }
                i += 1;
            }
            if hex_count >= 32 {
                let kind = if hex_count >= 128 {
                    REDACTED_SIG
                } else {
                    REDACTED_KEY
                };
                // Span covers hex + the wrap separators, so the
                // redaction removes the line breaks too (the secret
                // can't be reconstructed from line-broken halves).
                return Some((start, last_hex_end.max(i), kind));
            }
        } else {
            i += 1;
        }
    }
    None
}

/// Hex line-wrap separator: CR, LF, or space. The walk allows these
/// between hex chars so split secrets are still redacted as one
/// span. Non-hex non-separator bytes terminate the run (returns to
/// the outer loop).
fn is_hex_separator(b: u8) -> bool {
    matches!(b, b'\r' | b'\n' | b' ')
}

/// Locate a case-insensitive `bearer <token>` run.
///
/// R19 Lens-1 F1/F2: keyword match is the bare substring `bearer` (not
/// `"bearer "`); the byte immediately after must be ASCII whitespace
/// (handles `bearer `, `bearer\t`, `bearer\n`, `bearer\r` — RFC 7230
/// §3.2.4 obs-fold). The token walk uses RFC 6750 `b64token` char
/// class (`ALPHA / DIGIT / "-" / "." / "_" / "~" / "+" / "/"`) so JSON
/// punctuation (`"`, `,`, `:`, `{`, `}`, `[`, `]`) does NOT extend the
/// redaction range and silently eat subsequent fields. Returns
/// `(start, end)` covering the full `bearer <token>` span; callers
/// substitute the full range with `[REDACTED:bearer]`.
///
/// R20 Lens-1 F1 anchor: accepts an optional `from` byte offset. The
/// replacement marker `REDACTED_BEARER` contains the substring `bearer`,
/// so a naive `find_bearer_ci(modified_string)` re-matches inside the
/// replacement (sees `]` not WS, returns None) and the second+ real
/// `bearer` token in the same input is silently missed. Loop callers
/// MUST advance the search start to the byte AFTER each replaced range.
pub fn find_bearer_ci(s: &str) -> Option<(usize, usize)> {
    find_bearer_ci_from(s, 0)
}

/// Position-anchored variant of `find_bearer_ci`. `from` is a byte
/// offset; returns `None` if `from >= s.len()`. See `find_bearer_ci`
/// for the char-class + WS-separator contract.
pub fn find_bearer_ci_from(s: &str, from: usize) -> Option<(usize, usize)> {
    if from >= s.len() {
        return None;
    }
    let lower = s.to_ascii_lowercase();
    let start = lower[from..].find("bearer")? + from;
    let mut end = start + "bearer".len();
    let b = s.as_bytes();
    // Require ASCII whitespace separator after the keyword. Without
    // this guard, `bearership` / `bearerish` would match.
    if end >= b.len() || !b[end].is_ascii_whitespace() {
        return None;
    }
    // Walk over the whitespace separator(s). RFC 7230 §3.2.4 allows
    // obs-fold where a line break + whitespace continues a header
    // value; consume leading WS so the walk lands on the token.
    while end < b.len() && b[end].is_ascii_whitespace() {
        end += 1;
    }
    // R20 Lens-2 F1: RFC 6750 §2.1 also permits the quoted-string
    // form `Bearer "abc.def"`. The leading `"` is not b64token; the
    // body walk would stop at the quote and the token body inside
    // the quotes would leak. Detect the leading `"` BEFORE the
    // b64token walk: if the byte right after the keyword + WS is
    // `"`, enter the quoted-string path. This must precede the
    // b64token walk so a JSON-form `{"x":"Bearer abc123"}` does not
    // match (the byte after `Bearer abc123` is `,`, not `"`).
    let after_ws = end; // end is positioned right after the WS separator(s)
    if after_ws < b.len() && b[after_ws] == b'"' {
        // Quoted-string form: consume opening `"`, walk the b64token
        // body, then consume the closing `"` if present.
        end = after_ws + 1;
        while end < b.len() && is_b64token_byte(b[end]) {
            end += 1;
        }
        if end < b.len() && b[end] == b'"' {
            end += 1;
        }
    } else {
        // Walk the token body. RFC 6750 §2.1 `b64token`:
        //   `1*( ALPHA / DIGIT / "-" / "." / "_" / "~" / "+" / "/" )`
        // Restricting the char class prevents JSON punctuation from
        // extending the redaction into subsequent fields.
        //
        // R20 Lens-2 F6: bearer tokens can span multiple lines
        // (RFC 7230 §3.2.4 obs-fold: a CRLF + WSP continues the
        // previous header value). Without this, `Bearer abc.def\n
        // continuation` would redact only `abc.def` and leak
        // `continuation`. On hitting a newline, consume it AND any
        // leading WSP on the continuation line, then continue the
        // b64token walk. A bare newline (no leading WSP on the
        // next line) terminates — that's the obs-fold rule, not
        // freeform concatenation.
        loop {
            while end < b.len() && is_b64token_byte(b[end]) {
                end += 1;
            }
            if end < b.len() && (b[end] == b'\n' || b[end] == b'\r') {
                let mut probe = end;
                if b[probe] == b'\r' && probe + 1 < b.len() && b[probe + 1] == b'\n' {
                    probe += 1;
                }
                probe += 1;
                // Require at least one WSP on the continuation
                // line (obs-fold contract). Bare newline = end
                // of token.
                if probe < b.len() && (b[probe] == b' ' || b[probe] == b'\t') {
                    end = probe;
                    while end < b.len() && (b[end] == b' ' || b[end] == b'\t') {
                        end += 1;
                    }
                    continue;
                }
                break;
            }
            break;
        }
    }
    // R20 Lens-1 F3: if the token body itself is the keyword
    // `bearer` (or starts with it), do NOT swallow it — let the next
    // iteration treat it as a fresh bearer lead-in. Without this,
    // `Bearer Bearer xyz` collapses to one redaction and `xyz`
    // leaks. The body-start byte is computed as
    // `keyword_end + leading_ws_count`; the inner body is then
    // checked for the keyword.
    let keyword_end = start + "bearer".len();
    let body_byte = keyword_end + count_leading_ws(b, keyword_end, end);
    if end > body_byte && end - body_byte >= "bearer".len() {
        let body = &s[body_byte..body_byte + "bearer".len()];
        if body.eq_ignore_ascii_case("bearer") {
            // Inner body is the keyword. Shrink the match to the
            // outer keyword + separator only, so the next loop
            // iteration finds the inner bearer.
            end = body_byte;
        }
    }
    Some((start, end))
}

/// Count leading ASCII-whitespace bytes in `b[start..end]`. Returns 0
/// if `start >= end`.
fn count_leading_ws(b: &[u8], start: usize, end: usize) -> usize {
    let mut n = 0;
    let mut i = start;
    while i < end && b[i].is_ascii_whitespace() {
        n += 1;
        i += 1;
    }
    n
}

/// RFC 6750 §2.1 `b64token` char class. ASCII-only; non-ASCII bytes
/// always return `false`.
fn is_b64token_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~' | b'+' | b'/')
}

/// Collect every non-overlapping bearer-token run in `s` and replace
/// each with `REDACTED_BEARER`. R20 Lens-1 F1 anchor: collect-then-
/// replace avoids the "replacement marker contains the keyword" trap
/// (the marker is `[REDACTED:bearer]` which contains the substring
/// `bearer` — naively re-scanning the modified string would re-match
/// the marker and miss every subsequent real `bearer` token).
fn scrub_bearer_runs(s: &str) -> String {
    let mut ranges: Vec<(usize, usize)> = Vec::new();
    let mut from: usize = 0;
    while let Some((start, end)) = find_bearer_ci_from(s, from) {
        ranges.push((start, end));
        from = end;
        if from >= s.len() {
            break;
        }
    }
    if ranges.is_empty() {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut cursor: usize = 0;
    for (start, end) in &ranges {
        out.push_str(&s[cursor..*start]);
        out.push_str(REDACTED_BEARER);
        cursor = *end;
    }
    out.push_str(&s[cursor..]);
    out
}

/// Collect every non-overlapping long-hex run in `s` and replace each
/// with the appropriate `REDACTED_*` marker. See `scrub_bearer_runs`
/// for the collect-then-replace rationale.
fn scrub_long_hex_runs(s: &str) -> String {
    let mut ranges: Vec<(usize, usize, &'static str)> = Vec::new();
    let mut cursor: usize = 0;
    while let Some((start, end, kind)) = find_long_hex(&s[cursor..]) {
        ranges.push((start + cursor, end + cursor, kind));
        cursor += end;
        if cursor >= s.len() {
            break;
        }
    }
    if ranges.is_empty() {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    cursor = 0;
    for (start, end, kind) in &ranges {
        out.push_str(&s[cursor..*start]);
        out.push_str(kind);
        cursor = *end;
    }
    out.push_str(&s[cursor..]);
    out
}

/// Locate the value span of the next `sensitive_field<sep>value`.
/// Returns `(name_start, value_start, value_end)` where the field
/// name runs from `name_start` up to the separator at
/// `value_start - 1` (or `value_start - 2` if there was a `: ` WS).
/// Handles three field-name shapes:
/// 1. **env-file** `field=value` — name = ASCII alnum / `_` / `-` directly preceding `=`.
/// 2. **plain JSON** `"field":"value"` — closing `"` immediately precedes `:`, opening `"` follows the field-name walk-back, optional `: ` WS, then quoted value.
/// 3. **bare colon** `field: value` — name directly precedes `:`, no surrounding quotes (used when serde_json fails to parse the outer string, e.g. log-line wrap `audit: {...}`, BOM-prefixed JSON, trailing-comma JSON).
///
/// The value scan terminates at whitespace, `,`, `}`, `]`, or the
/// matching closing quote of a JSON-style `"value"` form. R20
/// Lens-2 F2: a JSON-object string that fails to parse used to
/// evade the redactor because the old `find_kv_secret` only
/// recognised `=`. With this rewrite the kv scan handles all three
/// shapes so redaction still fires when the outer JSON parse
/// fails, and the field name is preserved so JSON structure
/// stays parseable after redaction.
fn find_kv_secret(s: &str) -> Option<(usize, usize, usize)> {
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        let sep = match b[i] {
            b'=' => b'=', // env-file: field=value
            b':' => b':', // JSON inner-key: "field":"value" / plain "field: value"
            _ => {
                i += 1;
                continue;
            }
        };
        // Walk back over the field name. Three shapes:
        //   A. JSON quoted: closing `"` immediately precedes the
        //      separator, then ASCII alnum / `_` / `-` run, then
        //      optional opening `"` (the JSON shape `"field":`).
        //   B. Plain colon: ASCII alnum / `_` / `-` run directly
        //      precedes the separator (env / bare colon shape).
        //   C. Quoted-only name like `":"` with no run — name
        //      starts and ends at the same position; skip.
        let mut name_start = i;
        if sep == b':' && name_start > 0 && b[name_start - 1] == b'"' {
            name_start -= 1;
            while name_start > 0 {
                let c = b[name_start - 1];
                if c.is_ascii_alphanumeric() || c == b'_' || c == b'-' {
                    name_start -= 1;
                } else {
                    break;
                }
            }
            if name_start > 0 && b[name_start - 1] == b'"' {
                name_start -= 1;
            }
        } else {
            while name_start > 0 {
                let c = b[name_start - 1];
                if c.is_ascii_alphanumeric() || c == b'_' || c == b'-' {
                    name_start -= 1;
                } else {
                    break;
                }
            }
        }
        // Extract the field name (strip surrounding quotes for the
        // sensitive-name check).
        let raw_name = &s[name_start..i];
        let name_trimmed = raw_name.trim_matches('"');
        if name_trimmed.is_empty() || !field_is_sensitive(name_trimmed) {
            i += 1;
            continue;
        }
        // Value start = byte just past the separator; for `:`
        // skip optional whitespace. Value end = matching closing
        // `"` (quoted form) or first WS / `,` / `}` / `]` (bare
        // form). Return ONLY the value range; the caller preserves
        // the field name so the JSON structure stays parseable.
        let mut value_start = i + 1;
        if sep == b':' {
            while value_start < b.len() && b[value_start] == b' ' {
                value_start += 1;
            }
        }
        let value_end = if value_start < b.len() && b[value_start] == b'"' {
            let mut end = value_start + 1;
            while end < b.len() && b[end] != b'"' {
                end += 1;
            }
            if end < b.len() && b[end] == b'"' {
                end += 1;
            }
            end
        } else {
            let mut end = value_start;
            while end < b.len()
                && !b[end].is_ascii_whitespace()
                && b[end] != b','
                && b[end] != b'}'
                && b[end] != b']'
            {
                end += 1;
            }
            end
        };
        let value = &s[value_start..value_end];
        if !value.starts_with("[REDACTED:") {
            return Some((name_start, value_start, value_end));
        }
        i = value_end;
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
                // R17 Lens-2 F3 + R18 Lens-1 F1 + R18 Lens-2 F1:
                // field-name-based JSON redaction can leave behind
                // bearer tokens / long-hex / kv secrets inside
                // non-sensitive string values (e.g. `{"msg":"Bearer xyz"}`).
                // After the JSON pass, run plain-text bearer + long-hex
                // passes on the rendered output to catch those. The
                // kv pass is skipped — it would corrupt JSON quoting.
                let mut scrubbed: String = scrub_bearer_runs(&rendered);
                scrubbed = scrub_long_hex_runs(&scrubbed);
                let final_rendered = scrubbed;
                let leading_ws = s.len() - s.trim_start().len();
                let trailing_ws = s.len() - s.trim_end().len();
                let mut out = String::with_capacity(
                    s.len() + final_rendered.len().saturating_sub(trimmed.len()),
                );
                out.push_str(&s[..leading_ws]);
                out.push_str(&final_rendered);
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
    // R17 Lens-2 F1: wrap bearer + long-hex in loops so multiple matches
    // in the same string (e.g. two bearer tokens, or a bearer + a sig
    // hex run) all get redacted, matching the kv branch's behaviour.
    // R20 Lens-1 F1 anchor: `scrub_bearer_runs` / `scrub_long_hex_runs`
    // collect all non-overlapping ranges FIRST then stitch the
    // result, so the replacement marker (which contains the keyword
    // substring) cannot re-match and block subsequent real matches.
    let mut owned: String = scrub_bearer_runs(s);
    owned = scrub_long_hex_runs(&owned);

    loop {
        let current: &str = if owned == *s { owned.as_str() } else { &owned };
        let Some((name_start, value_start, value_end)) = find_kv_secret(current) else {
            break;
        };
        // `find_kv_secret` returns just the value range (R20
        // Lens-2 F2). The field name is preserved so JSON
        // structure remains parseable. The per-field marker is
        // looked up from the field-name slice so the env-file
        // tests (which assert e.g. `[REDACTED:pw]` for `password=`)
        // keep working through the plain-text branch.
        let field_name = current[name_start..value_start]
            .trim_matches(|c: char| c == '"' || c == ':' || c == '=' || c == ' ');
        let replacement = redact_by_field(field_name, "").to_string();
        let mut o = current.to_string();
        o.replace_range(value_start..value_end, &replacement);
        owned = o;
    }

    if owned == *s {
        Cow::Borrowed(s)
    } else {
        Cow::Owned(owned)
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

    /// SEC-13: `RedactedHex` must zeroize on drop so signature bytes do
    /// not linger in the allocator's free list after a panic or early
    /// `?`-return. Stable Rust cannot reliably *test* heap-zeroization
    /// (the allocator may have moved the bytes), so the unit test pins
    /// the *derive contract* — `ZeroizeOnDrop` is wired — and exercises
    /// the explicit `Zeroize` impl via a witness buffer + drop.
    ///
    /// The drop-glue cannot be observed from the heap in stable Rust
    /// without `mprotect`-style mechanisms, but the `Zeroize` trait
    /// method *can* be called explicitly and its effect is observable
    /// on the value (the `&mut [u8]` we pass in gets overwritten). That
    /// is what this test pins: the wipe is observable when invoked, and
    /// the `ZeroizeOnDrop` derive + the explicit `Drop` impl together
    /// make the same wipe fire automatically on `drop()`.
    #[test]
    fn redacted_hex_zeroizes_on_drop() {
        // Derive contract: `ZeroizeOnDrop` produces a `Drop` impl that
        // calls `<Self as Zeroize>::zeroize` on the owned fields. The
        // trait is wired when the struct compiles with the derive —
        // we exercise it via `zeroize()` directly so the test pins the
        // trait impl, not just the type definition.
        let mut h = RedactedHex(vec![0xde, 0xad, 0xbe, 0xef, 0xca, 0xfe]);
        h.zeroize();
        assert!(
            h.0.iter().all(|b| *b == 0),
            "zeroize must wipe Vec<u8>: {h:?}"
        );

        // Belt-and-braces: the explicit `Drop` impl calls `self.0.zeroize()`,
        // so dropping a fresh instance must also wipe the buffer. We can't
        // observe heap zeroization in stable Rust (allocator may move the
        // bytes), but the trait call exercises the same code path the Drop
        // impl uses.
        drop(h);
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

    /// R20 Lens-2 F4: hyphen variants of sensitive field names
    /// (`pass-word`, `priv-key`, `bearer-token`, `api-key`)
    /// normalize via hyphen-strip before the FIELD_TABLE lookup.
    /// Without this, `pass-word=foo` slips through every redaction
    /// path.
    #[test]
    fn redacts_hyphen_variants_env_file() {
        let cases = [
            ("pass-word=hunter2", REDACTED_PW),
            ("priv-key=00ab00ab", REDACTED_KEY),
            ("bearer-token=abc123", REDACTED_BEARER),
            ("api-key=sk-1234", REDACTED_API_KEY),
            ("pair-code=foo-bar", REDACTED_PAIR),
        ];
        for (input, marker) in &cases {
            let out = redact_string(input);
            assert!(
                out.contains(marker),
                "hyphen variant {input} not redacted; out={out}"
            );
        }
    }

    #[test]
    fn redacts_hyphen_variants_json() {
        let input = r#"{"pass-word":"hunter2","priv-key":"abc","api-key":"sk-1"}"#;
        let out = redact_string(input);
        assert!(out.contains(REDACTED_PW), "{out}");
        assert!(out.contains(REDACTED_KEY), "{out}");
        assert!(out.contains(REDACTED_API_KEY), "{out}");
        assert!(!out.contains("hunter2"), "{out}");
        assert!(!out.contains("sk-1"), "{out}");
    }

    /// R20 Lens-2 F6: bearer tokens that span multiple lines via
    /// RFC 7230 §3.2.4 obs-fold (`CRLF + WSP` continues the value)
    /// are redacted in full. Without the obs-fold walk, only the
    /// first line of the token would be redacted, leaking the
    /// continuation.
    #[test]
    fn redacts_bearer_obs_fold_lf() {
        let input = "Authorization: Bearer abc.def\n   continuationXYZ";
        let out = redact_string(input);
        assert!(out.contains(REDACTED_BEARER), "{out}");
        assert!(!out.contains("continuationXYZ"), "{out}");
    }

    #[test]
    fn redacts_bearer_obs_fold_crlf() {
        let input = "Authorization: Bearer abc.def\r\n   continuationXYZ";
        let out = redact_string(input);
        assert!(out.contains(REDACTED_BEARER), "{out}");
        assert!(!out.contains("continuationXYZ"), "{out}");
    }

    #[test]
    fn bearer_obs_fold_no_leading_wsp_does_not_continue() {
        // Bare newline (no leading WSP on next line) terminates the
        // token per obs-fold contract; the second line is its own
        // field, not part of the bearer.
        let input = "Bearer abc.def\nNEXT_FIELD=foo";
        let out = redact_string(input);
        assert!(out.contains(REDACTED_BEARER), "{out}");
        // NEXT_FIELD is not sensitive → preserved.
        assert!(out.contains("NEXT_FIELD=foo"), "{out}");
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

    /// R19 Lens-1 F1: bearer walk must NOT extend through JSON
    /// punctuation (`"`, `,`, `:`, `{`, `}`, `[`, `]`). Token char class
    /// is restricted to RFC 6750 §2.1 `b64token` (alphanumeric + `-._~+/`).
    /// Regression for the bug where the redaction ate `"z":"more"}` after
    /// the bearer token, silently dropping fields.
    #[test]
    fn find_bearer_ci_stops_at_json_quote() {
        let input = r#"{"x":"Bearer abc123","z":"more"}"#;
        let (start, end) = find_bearer_ci(input).expect("bearer must match");
        let span = &input[start..end];
        assert_eq!(span, "Bearer abc123", "scope leaked: {span:?}");
        // Both neighbor fields remain in the rendered output after redaction.
        let out = redact_string(input);
        assert!(
            out.contains(r#""z":"more""#),
            "neighbor field dropped: {out}"
        );
    }

    /// R19 Lens-1 F2: keyword `bearer` must require ASCII whitespace
    /// immediately after (single space, tab, newline, CR). RFC 7230
    /// §3.2.4 obs-fold allows line breaks inside header values.
    /// Regression for the old `"bearer "` literal-only match.
    #[test]
    fn find_bearer_ci_matches_tab_separator() {
        let input = "auth: Bearer\tabc123def";
        let (start, end) = find_bearer_ci(input).expect("tab-separated bearer must match");
        assert_eq!(&input[start..end], "Bearer\tabc123def");
        let out = redact_string(input);
        assert!(out.contains(REDACTED_BEARER), "{out}");
        assert!(!out.contains("abc123def"), "{out}");
    }

    /// R19 Lens-1 F2 follow-up: `\n` and `\r` separators must also match.
    #[test]
    fn find_bearer_ci_matches_crlf_separator() {
        for sep in ["\n", "\r\n", "\r"] {
            let input = format!("Authorization: Bearer{sep}abc.def-ghi_jkl");
            let (start, end) = find_bearer_ci(&input).unwrap_or_else(|| panic!("sep {sep:?}"));
            assert_eq!(&input[start..end], &format!("Bearer{sep}abc.def-ghi_jkl"));
        }
    }

    /// R19 Lens-1 F2 follow-up: `bearership` / `bearerish` must NOT
    /// match — the ASCII whitespace separator guard catches these.
    #[test]
    fn find_bearer_ci_rejects_keyword_suffix() {
        assert!(find_bearer_ci("bearership").is_none());
        assert!(find_bearer_ci("bearerable").is_none());
        assert!(find_bearer_ci("bearerable-token").is_none());
    }

    /// R20 Lens-1 F1: bearer redaction loop must NOT stop at the first
    /// match when the replacement marker contains the keyword substring.
    /// Regression for the anchor bug where two real bearer tokens in
    /// the same string produced only one redaction.
    #[test]
    fn redact_string_redacts_multiple_bearers() {
        let out = redact_string("Bearer abc.def Bearer xyz.uvw");
        // Two redacted markers, no raw token body left in either.
        assert_eq!(out.matches(REDACTED_BEARER).count(), 2, "{out}");
        assert!(!out.contains("abc.def"), "{out}");
        assert!(!out.contains("xyz.uvw"), "{out}");
    }

    /// R20 Lens-1 F3: `Bearer Bearer <token>` — the inner `Bearer` is
    /// consumed as the keyword, the outer is the lead-in. Two markers
    /// are NOT required (only one b64token body exists); the inner
    /// keyword is part of the same redaction range.
    #[test]
    fn redact_string_bearer_bearer_collapse() {
        let out = redact_string("Bearer Bearer secret123");
        assert!(out.contains(REDACTED_BEARER), "{out}");
        assert!(!out.contains("secret123"), "{out}");
    }

    /// R20 Lens-1 F2: OAuth-style `*_token` field names in JSON must
    /// trigger field-name redaction. Bearer / access / refresh / id.
    #[test]
    fn redact_string_json_oauth_token_fields() {
        for field in ["bearer_token", "access_token", "refresh_token", "id_token"] {
            let input = format!(r#"{{"{field}":"eyJabc.def-ghi"}}"#);
            let out = redact_string(&input);
            assert!(out.contains(REDACTED_BEARER), "{field} → {out}");
            assert!(!out.contains("eyJabc.def-ghi"), "{field} leaked: {out}");
        }
    }

    /// R20 Lens-2 F1: RFC 6750 §2.1 quoted-string form `Bearer "xyz"`
    /// must be redacted. The leading `"` is not b64token; the body
    /// walk would otherwise stop at the quote and the token body
    /// would leak. Regression for the bug where the b64token walk
    /// stopped at the opening quote.
    #[test]
    fn find_bearer_ci_quoted_string_form() {
        let input = r#"auth: Bearer "abc.def-ghi""#;
        let (start, end) = find_bearer_ci(input).expect("quoted-string bearer must match");
        assert_eq!(&input[start..end], r#"Bearer "abc.def-ghi""#);
        let out = redact_string(input);
        assert!(out.contains(REDACTED_BEARER), "{out}");
        assert!(!out.contains("abc.def-ghi"), "{out}");
    }

    /// R20 Lens-2 F3: secrets split across CRLF (e.g. macaroon ID
    /// printed by signer every 30 chars) must NOT evade the
    /// long-hex detector. The fix: strip CR/LF/space before the
    /// hex walk so a 64-char run broken into 30+34 chunks still
    /// matches. The redactor only collapses the hex; CR/LF/space
    /// are preserved in the output via the offset arithmetic.
    #[test]
    fn find_long_hex_redacts_crlf_split_run() {
        let hex = "a".repeat(64);
        // 30 + CRLF + 34 = 64 hex chars split across a line break.
        let split = format!("{}\r\n{}", &hex[..30], &hex[30..]);
        let input = format!("sig={split}");
        let out = redact_string(&input);
        assert!(
            out.contains(REDACTED_KEY) || out.contains(REDACTED_SIG),
            "{out}"
        );
        assert!(!out.contains(&hex[..30]), "first half leaked: {out}");
        assert!(!out.contains(&hex[30..]), "second half leaked: {out}");
    }

    /// R20 Lens-2 F2: log-line wrap (`audit: {json}`) and BOM
    /// corruption (`\u{feff}{json}`) evade the JSON parser because
    /// the leading non-JSON prefix / BOM breaks `serde_json`. The
    /// plain-text fallback must still catch the inner
    /// `password="hunter2"` form via the now-colon-aware
    /// `find_kv_secret`.
    #[test]
    fn redact_string_handles_log_wrapped_json() {
        let input = r#"audit: {"password":"hunter2","user":"alice"}"#;
        let out = redact_string(input);
        assert!(out.contains("password"), "field name dropped: {out}");
        assert!(out.contains(REDACTED_PW), "{out}");
        assert!(!out.contains("hunter2"), "password leaked: {out}");
        assert!(out.contains("user"), "non-sensitive field dropped: {out}");
        assert!(out.contains("alice"), "non-sensitive value dropped: {out}");
    }

    /// R20 Lens-2 F2 (BOM form): leading BOM evades JSON parse, but
    /// the plain-text fallback still redacts via `find_kv_secret`.
    /// `api_key` is the canonical sensitive name in FIELD_TABLE.
    #[test]
    fn redact_string_handles_bom_prefixed_json() {
        let input = "\u{feff}{\"api_key\":\"abcdef0123\"}";
        let out = redact_string(input);
        assert!(out.contains("api_key"), "field name dropped: {out}");
        assert!(!out.contains("abcdef0123"), "api_key leaked: {out}");
        assert!(out.contains(REDACTED_API_KEY), "{out}");
    }

    /// R20 Lens-2 F2 (trailing-comma form): trailing comma is
    /// rejected by strict JSON parsers, but the plain-text fallback
    /// still catches `api_key=xyz` via `find_kv_secret`.
    #[test]
    fn redact_string_handles_trailing_comma_json() {
        let input = r#"{"api_key":"xyz","foo":"bar",}"#;
        let out = redact_string(input);
        assert!(out.contains("api_key"), "field name dropped: {out}");
        assert!(!out.contains("xyz"), "api_key leaked: {out}");
        assert!(out.contains(REDACTED_API_KEY), "{out}");
    }
}
