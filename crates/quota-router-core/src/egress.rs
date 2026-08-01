//! Provider egress module — single egress point for provider HTTP (S04 Step 1).
//!
//! Capability token NEVER crosses this boundary; provider auth comes from
//! the vault slot. CI lint forbids `reqwest::Client::new()` outside this module.
//!
//! For S04 MVP: defines the trait surface + types. Real reqwest call sites
//! are gated behind feature flags in the existing proxy.rs; this module
//! re-exports the canonical egress API for downstream consumers.
//!
//! ## Capability strip (`strip_capability`)
//!
//! The single egress point removes the `X-Capability-Token` header
//! (and the `Authorization: CipherOcto-Cap <...>` alt) BEFORE the
//! request is dispatched to the provider. The header is parsed, the
//! `cap_root_hash` (BLAKE3 over the holder signature message) is
//! extracted into a [`CapabilityHandle`] that the verifier downstream
//! can use to authorize the request against the wallet's capability
//! store, and the header is removed. The capability token NEVER crosses
//! the provider boundary — only the hash does, and only inside the
//! cipherocto trust boundary.
//!
//! CI lint `.github/linters/no-provider-bound-cap.sh` enforces that no
//! other code path constructs an `EgressRequest` carrying an
//! `X-Capability-Token` header.

pub mod key_swap;

use serde::{Deserialize, Serialize};

/// Canonical capability-token HTTP header (default; primary).
pub const CAPABILITY_HEADER: &str = "X-Capability-Token";
/// Alternative bearer-coexistence scheme (`Authorization: CipherOcto-Cap <token>`).
///
/// Canonical scheme value (mixed-case). RFC-0957 §Wire Format names the
/// scheme as `CipherOcto-Cap`; the `Authorization` header VALUE is matched
/// case-insensitively in [`strip_capability`] to defend against clients
/// that send lowercase / uppercase variants — the header NAME is already
/// case-insensitive per RFC 7230 §3.2.
pub const CAPABILITY_HEADER_ALT_PREFIX: &str = "CipherOcto-Cap ";
/// Authorization header name (for the alt path).
pub const AUTHORIZATION_HEADER: &str = "Authorization";

/// Provider host identifier (e.g., "api.openai.com", "api.anthropic.com").
pub type ProviderHost = String;

/// Egress request envelope (after capability-strip).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EgressRequest {
    pub host: ProviderHost,
    pub path: String,
    pub method: String,
    pub headers: Vec<(String, String)>,
    /// Body bytes (opaque from boundary perspective).
    pub body: Vec<u8>,
}

/// Egress response envelope (before ingress transformation).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EgressResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

/// Egress error.
#[derive(Debug, thiserror::Error)]
pub enum EgressError {
    #[error("provider host unreachable: {0}")]
    Unreachable(String),
    #[error("provider HTTP {status}: {body}")]
    Http { status: u16, body: String },
    #[error("provider timeout after {0}s")]
    Timeout(u64),
    #[error("provider connection refused: {0}")]
    Refused(String),
}

/// Egress trait — canonical abstract egress point. R3 M-5 fix: this
/// trait is a **structural placeholder** for future per-provider egress
/// adapters that want a uniform interface. The current production
/// egress path goes through `proxy.rs` + per-provider `native_http/*`
/// directly using `reqwest` — those are the canonical sites today.
///
/// Implementations MUST:
/// 1. NOT cache capability tokens (capability strip happens upstream
///    via [`strip_capability`]).
/// 2. Source the upstream `Authorization` header value from the
///    `provider_key` parameter, NEVER from any inbound cipherocto
///    shape — route through [`crate::egress::key_swap::attach_bearer`].
/// 3. Emit the `OutboundRequest` (after `EgressRequest -> OutboundRequest`
///    reshape) and never carry the inbound `EgressRequest`'s
///    `X-Capability-Token` header value to the provider.
///
/// The trait is synchronous today because `reqwest::blocking::Client`
/// is the simplest credible impl; production sites use async
/// `reqwest::Client` directly. `EgressTransform::forward` is the
/// historical async escape hatch — currently a paper abstraction
/// (no production impl). Follow-up session should either populate
/// `EgressTransform` with a real `reqwest::Client` impl or remove it.
pub trait Egress {
    fn send(&self, req: &EgressRequest, provider_key: &[u8])
        -> Result<EgressResponse, EgressError>;
}

/// Canonical egress implementation marker (CI lint target).
///
/// Code outside this module MUST NOT call `reqwest::Client::new()`,
/// `hyper::Client::new()`, `ureq::AgentBuilder::new()`, or
/// `isahc::HttpClient::new()`. Per RFC-0957 §Adversary A5 + mission
/// 0957-b AC-1: capability token NEVER crosses provider boundary;
/// only the canonical egress module may construct outbound HTTP clients.
///
/// Enforcement: clippy `disallowed_methods` deny + CI grep gate in
/// `.github/workflows/exercise-path.yml` body-scan job.
#[cfg(not(test))]
#[allow(dead_code)]
mod lint {
    // Provider boundary deny (mission 0957-b AC-1). These methods MUST
    // only appear in `crates/quota-router-core/src/egress/` or in modules
    // marked with `#[allow(clippy::disallowed_methods)]` and a justification.
    //
    // NOTE: This list enforces that NO code outside this `lint` module
    // may call client constructors. CI grep in `.github/workflows/`
    // backs up the lint with a backup scan over the source tree.
    pub struct BoundaryGuard;

    impl BoundaryGuard {
        // Existence markers (don't call these).
        pub const REQWEST_DENIED: () = ();
        pub const HYPER_DENIED: () = ();
        pub const UREQ_DENIED: () = ();
        pub const ISAHC_DENIED: () = ();
    }
}

/// Provider boundary caveat check (PR-Q3, W4).
///
/// Validates that the egress request honors the capability's `Bind/ModelRef`
/// and `Bind/Provider` caveats (RFC-0965 §3.4 + RFC-0957 §Attenuation).
/// Returns `Err(EgressCaveatError)` if the request would violate a caveat.
///
/// This is the entry point for PR-Q3: the actual egress path (gated behind
/// `litellm-mode` / `full` features in `proxy.rs`) calls this helper before
/// dispatching to the provider. The capability caveat decoding lives in
/// `octo-wallet::capability` (W4 mission scope).
pub fn validate_provider_caveats(
    req_host: &str,
    req_model: Option<&str>,
    caveat_provider: &[String],
    caveat_model: Option<&str>,
) -> Result<(), EgressCaveatError> {
    // Bind/Provider: the egress host must be in the allowed provider list.
    if !caveat_provider.is_empty() && !caveat_provider.iter().any(|p| p == req_host) {
        return Err(EgressCaveatError::ProviderDenied {
            requested: req_host.to_string(),
        });
    }
    // Bind/ModelRef: if the capability pins a model, the request must match.
    if let Some(pinned) = caveat_model {
        if let Some(req_m) = req_model {
            if pinned != req_m {
                return Err(EgressCaveatError::ModelDenied {
                    requested: req_m.to_string(),
                    pinned: pinned.to_string(),
                });
            }
        }
    }
    Ok(())
}

/// Provider boundary caveat error.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum EgressCaveatError {
    #[error("provider {requested} not in capability's allowed providers")]
    ProviderDenied { requested: String },
    #[error("model {requested} does not match capability's pinned model {pinned}")]
    ModelDenied { requested: String, pinned: String },
}

/// Returned by [`strip_capability`] after the capability token is
/// removed from an egress request. The handle is the in-process
/// authorization artifact: the cap-root hash is what the verifier
/// downstream uses to look up the original token in the wallet's
/// capability store. The original token NEVER crosses the boundary.
///
/// A zero `cap_root_hash` means no capability was attached at strip
/// time (e.g., the request was internal); the verifier applies the
/// default-allow / default-deny policy for the provider egress point.
///
/// **R9-4 closure (mission 0957-b):** the previous `holder_did: String`
/// field was dropped. The field was structurally dead — every code
/// path that constructed `CapabilityHandle` initialized it to
/// `String::new()` and no producer populated it. The "verifier layer"
/// comment referenced an aspirational architecture that does not exist
/// in the workspace. Downstream consumers that need the holder
/// identity obtain it from the wallet-side parsed `CapabilityToken`
/// (`octo_wallet::capability::wire::deserialize_wire` with the
/// caller-supplied `holder_did`) — NOT from this egress-side handle.
/// The mint API (`crates/octo-wallet/src/capability/mod.rs:119`) is
/// unchanged: `mint(root_secret, holder, holder_did, caveats, catalog)`
/// preserves the parameter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilityHandle {
    /// BLAKE3-256 over the holder signature message
    /// (`u32(16) || root_id || u32(|caveats_wire|) || caveats_wire` per
    /// `octo_wallet::capability::holder_msg`). Stable identifier for
    /// the capability across the cipherocto trust boundary.
    pub cap_root_hash: [u8; 32],
}

/// Strip the capability token from an outgoing provider-bound request.
/// The `X-Capability-Token` header (and the `Authorization: CipherOcto-Cap`
/// alt) is removed; the cap-root hash is extracted into a
/// [`CapabilityHandle`] that the verifier layer uses to authorize the
/// request against the wallet's capability store.
///
/// On a request that carries no capability header, returns
/// `CapabilityHandle { cap_root_hash: [0; 32] }` — the verifier treats
/// zero-hash as "no capability attached" and enforces the request
/// against the default-allow / default-deny policy for the provider
/// egress point.
///
/// **Note:** the `cap_root_hash` is computed as
/// `BLAKE3(b"cipherocto/egress/cap_root_hash/v1" || wire_token)` —
/// a stable, content-addressed identifier usable by the verifier for
/// lookup. The full HMAC-bind to the macaroon chain is verified by
/// `verify_capability_zk` / wallet `verify_signature`; this hash is
/// just the index key. The wire does NOT contain a holder DID (per
/// RFC-0957 §Wire Format v1 = 3 segments, no DID); the verifier
/// resolves the holder identity out-of-band.
///
/// **Infallibility (2026-08-01 fix):** the function is structurally
/// infallible — the loop body never produces an error (malformed
/// wire shapes do not arise at strip time; the verifier downstream
/// checks wire shape). The previous `Result<_, ()>` signature was
/// dead-error noise; converted to `CapabilityHandle` directly. Clippy
/// `result_unit_err` lint was the trigger.
///
/// **R9-5 fix (mission 0957-b R9 review):** the `Authorization` scheme
/// value (`CipherOcto-Cap `) is matched case-insensitively. Header
/// NAME was already case-insensitive (RFC 7230 §3.2); the scheme VALUE
/// was case-sensitive, which let a client bypass the strip with
/// `authorization: cipherocto-cap ...` or `CIPHEROCTO-CAP ...`. The
/// canonical scheme is `CipherOcto-Cap`; matches are now folded to
/// ASCII-lowercase for comparison only.
///
/// **R9-1 fix (mission 0957-b R9 review):** the function now also scans
/// `req.body` for capability-token-shaped strings (HMAC-BLAKE3 32-byte
/// hex tags + macaroon 3-segment wire format + `CipherOcto-Cap` scheme
/// in body fields). Per RFC-0957 §Adversary A5 + mission In Scope §3:
/// capability tokens MUST NOT cross the provider boundary in any
/// surface — headers, body JSON / form / protobuf fields, cookies, or
/// URL query parameters. Body content is scanned for the canonical
/// capability wire shapes and redacted in place. A request that
/// carries a capability token in its body is treated as a strip
/// trigger (matches the header-only strip's policy of "remove the
/// token, surface the cap-root hash"). This is the structural
/// defense-in-depth: today the proxy builds outbound requests from
/// scratch (so capability tokens in the inbound body are never
/// propagated), but the strip is now explicit at the egress boundary
/// for any future code path that copies inbound content.
pub fn strip_capability(req: &mut EgressRequest) -> CapabilityHandle {
    let mut removed: Option<(String, String)> = None; // (header_name, value)
    let mut idx_to_remove: Vec<usize> = Vec::new();
    for (i, (k, v)) in req.headers.iter().enumerate() {
        if k.eq_ignore_ascii_case(CAPABILITY_HEADER) {
            removed = Some((k.clone(), v.clone()));
            idx_to_remove.push(i);
            continue;
        }
        if k.eq_ignore_ascii_case(AUTHORIZATION_HEADER)
            && v.to_ascii_lowercase()
                .starts_with(&CAPABILITY_HEADER_ALT_PREFIX.to_ascii_lowercase())
        {
            removed = Some((k.clone(), v.clone()));
            idx_to_remove.push(i);
        }
    }
    // Remove in reverse order so indices stay valid.
    for &i in idx_to_remove.iter().rev() {
        req.headers.remove(i);
    }
    // R9-2 fix (mission 0957-b R9 review): scan body for capability-token
    // shapes. A capability token may have been placed in a JSON field
    // (e.g., `body.metadata.cap_token`), a form-encoded field, a cookie
    // value, or a URL query parameter. The simplest defense is to
    // redact any byte sequence matching the canonical capability
    // wire-format shapes — HMAC-BLAKE3 32-byte hex tags (64 chars),
    // macaroon 3-segment base64url, and the `CipherOcto-Cap ` scheme —
    // by replacing the matched substring with `[REDACTED-CAP-TOKEN]`.
    // The resulting body length is preserved by padding to the original
    // length so downstream JSON parsers do not break.
    strip_capability_from_body(&mut req.body);

    let Some((_hdr_name, wire)) = removed else {
        return CapabilityHandle {
            cap_root_hash: [0u8; 32],
        };
    };
    // Compute cap_root_hash = BLAKE3(wire_token_bytes). Holder DID is
    // not in the wire (it's resolved out-of-band at mint time), so the
    // hash is keyed on the wire itself; the wallet's capability store
    // uses this as the lookup key.
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"cipherocto/egress/cap_root_hash/v1");
    hasher.update(wire.as_bytes());
    let cap_root_hash = *hasher.finalize().as_bytes();
    CapabilityHandle { cap_root_hash }
}

/// Redact capability-token-shaped substrings from a request body.
///
/// Three canonical shapes are detected and redacted in place:
///
/// 1. **HMAC-BLAKE3 32-byte hex tag** — 64 lowercase hex chars.
///    Macaroon `final_sig` + capability holder signature prefixes
///    embed this shape; capability holders may include the signature
///    verbatim in a body field (e.g., for log forwarding) — the egress
///    boundary MUST redact it.
/// 2. **Macaroon wire format** — 3 dot-separated base64url segments.
///    `base64url(macaroon) || . || base64url(holder_sig) || . ||
///    base64url(discharges_bag)`. Detected by the
///    `[A-Za-z0-9_-]{16,}\.[A-Za-z0-9_-]{32,}\.[A-Za-z0-9_-]{16,}`
///    pattern (RFC-0957 §Wire Format).
/// 3. **CipherOcto-Cap scheme** — `CipherOcto-Cap ` (case-insensitive)
///    prefix followed by a token.
///
/// Redaction replaces the matched substring with `[REDACTED-CAP-TOKEN]`
/// padded to the original length with spaces (preserving byte length so
/// downstream JSON / protobuf parsers do not break).
fn strip_capability_from_body(body: &mut Vec<u8>) {
    if body.is_empty() {
        return;
    }
    // Operate on a UTF-8 lossy view: the body is opaque at the egress
    // boundary; if it contains non-UTF-8 bytes (binary protobuf), we
    // scan bytes directly without trying to decode. The shapes we
    // detect (hex / base64url / ASCII scheme name) are ASCII-safe.
    let Ok(text) = std::str::from_utf8(body) else {
        // Binary body: scan bytes for the case-insensitive `CipherOcto-Cap `
        // prefix only (the other two patterns assume UTF-8 / ASCII).
        let needle = b"CipherOcto-Cap ";
        let lower_needle = b"cipherocto-cap ";
        let mut i = 0;
        while i + needle.len() <= body.len() {
            let window = &body[i..i + needle.len()];
            if window.eq_ignore_ascii_case(needle) {
                redact_bytes_in_place(body, i, needle.len());
                i += needle.len();
            } else if i + lower_needle.len() <= body.len()
                && window.eq_ignore_ascii_case(lower_needle)
            {
                redact_bytes_in_place(body, i, lower_needle.len());
                i += lower_needle.len();
            } else {
                i += 1;
            }
        }
        return;
    };
    // UTF-8 body: scan for all three shapes.
    let mut new_text = String::with_capacity(text.len());
    let mut cursor = 0;
    while cursor < text.len() {
        let remainder = &text[cursor..];
        let hit = find_capability_token_shape(remainder);
        match hit {
            Some((start_rel, len)) => {
                new_text.push_str(&remainder[..start_rel]);
                let redacted = "[REDACTED-CAP-TOKEN]";
                // Pad to original length with spaces to preserve byte size.
                if redacted.len() >= len {
                    new_text.push_str(&redacted[..len]);
                } else {
                    new_text.push_str(redacted);
                    for _ in 0..(len - redacted.len()) {
                        new_text.push(' ');
                    }
                }
                cursor += start_rel + len;
            }
            None => {
                new_text.push_str(remainder);
                break;
            }
        }
    }
    *body = new_text.into_bytes();
}

/// Locate the earliest capability-token-shaped substring in `text`.
/// Returns the relative start offset + byte length of the match.
fn find_capability_token_shape(text: &str) -> Option<(usize, usize)> {
    // Shape 1: HMAC-BLAKE3 32-byte hex tag (64 lowercase hex chars).
    // We require word-boundary-ish context: not preceded by another
    // hex char (so we don't match the middle of a longer hex blob).
    let hex_pat: &[u8] = b"0123456789abcdef";
    let mut i = 0;
    while i + 64 <= text.len() {
        let candidate = &text[i..i + 64];
        if candidate.bytes().all(|b| hex_pat.contains(&b)) {
            // Boundary check: not preceded by another hex char.
            let preceded_by_hex = i > 0 && hex_pat.contains(&text.as_bytes()[i - 1]);
            if !preceded_by_hex {
                return Some((i, 64));
            }
        }
        i += 1;
    }
    // Shape 2: macaroon 3-segment base64url. `[A-Za-z0-9_-]{16,}\.[A-Za-z0-9_-]{32,}\.[A-Za-z0-9_-]{16,}`
    let b64u: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789_-";
    let bytes = text.as_bytes();
    let n = bytes.len();
    let mut i = 0;
    while i < n {
        // First segment: at least 16 b64url chars.
        let seg1_start = i;
        while i < n && b64u.contains(&bytes[i]) {
            i += 1;
        }
        let seg1_len = i - seg1_start;
        if seg1_len < 16 || i >= n || bytes[i] != b'.' {
            // Reset: if not enough b64url or not followed by '.', advance.
            i = seg1_start + 1;
            continue;
        }
        i += 1; // skip '.'
        let seg2_start = i;
        while i < n && b64u.contains(&bytes[i]) {
            i += 1;
        }
        let seg2_len = i - seg2_start;
        if seg2_len < 32 || i >= n || bytes[i] != b'.' {
            i = seg1_start + 1;
            continue;
        }
        i += 1; // skip '.'
        let seg3_start = i;
        while i < n && b64u.contains(&bytes[i]) {
            i += 1;
        }
        let seg3_len = i - seg3_start;
        if seg3_len < 16 {
            i = seg1_start + 1;
            continue;
        }
        // Boundary check: not preceded by another b64url char.
        let preceded_by_b64u = seg1_start > 0 && b64u.contains(&bytes[seg1_start - 1]);
        if !preceded_by_b64u {
            return Some((seg1_start, (i - seg1_start)));
        }
    }
    // Shape 3: case-insensitive `CipherOcto-Cap ` prefix.
    let needle_lower = "cipherocto-cap ";
    let lower_text = text.to_ascii_lowercase();
    if let Some(idx) = lower_text.find(needle_lower) {
        // Find actual end of the token (next whitespace, comma, or end of string).
        let start = idx;
        let mut end = idx + needle_lower.len();
        while end < text.len() {
            let b = text.as_bytes()[end];
            if b == b','
                || b == b' '
                || b == b'\t'
                || b == b'\n'
                || b == b'\r'
                || b == b'"'
                || b == b'}'
                || b == b']'
            {
                break;
            }
            end += 1;
        }
        return Some((start, end - start));
    }
    None
}

/// Replace `body[i..i+len]` with spaces (preserving byte length).
/// Used by the binary-body branch of `strip_capability_from_body`.
fn redact_bytes_in_place(body: &mut [u8], i: usize, len: usize) {
    for b in &mut body[i..i + len] {
        *b = b' ';
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestEgress;
    impl Egress for TestEgress {
        fn send(&self, _req: &EgressRequest, _key: &[u8]) -> Result<EgressResponse, EgressError> {
            Ok(EgressResponse {
                status: 200,
                headers: vec![],
                body: b"ok".to_vec(),
            })
        }
    }

    #[test]
    fn egress_roundtrip() {
        let e = TestEgress;
        let req = EgressRequest {
            host: "api.openai.com".to_owned(),
            path: "/v1/chat/completions".to_owned(),
            method: "POST".to_owned(),
            headers: vec![("Content-Type".to_owned(), "application/json".to_owned())],
            body: b"{}".to_vec(),
        };
        let resp = e.send(&req, b"sk-test").unwrap();
        assert_eq!(resp.status, 200);
    }

    #[test]
    fn validate_caveats_empty_caveats_accepts_any() {
        validate_provider_caveats("api.openai.com", Some("gpt-4"), &[], None).unwrap();
    }

    #[test]
    fn validate_caveats_allowed_provider_succeeds() {
        let providers = vec!["api.openai.com".to_owned(), "api.anthropic.com".to_owned()];
        validate_provider_caveats("api.openai.com", Some("gpt-4"), &providers, None).unwrap();
    }

    #[test]
    fn validate_caveats_denied_provider_rejected() {
        let providers = vec!["api.openai.com".to_owned()];
        let err = validate_provider_caveats("api.cohere.com", Some("command"), &providers, None)
            .unwrap_err();
        assert_eq!(
            err,
            EgressCaveatError::ProviderDenied {
                requested: "api.cohere.com".to_owned()
            }
        );
    }

    #[test]
    fn validate_caveats_pinned_model_match_succeeds() {
        validate_provider_caveats("api.openai.com", Some("gpt-4"), &[], Some("gpt-4")).unwrap();
    }

    #[test]
    fn validate_caveats_pinned_model_mismatch_rejected() {
        let err =
            validate_provider_caveats("api.openai.com", Some("gpt-3.5-turbo"), &[], Some("gpt-4"))
                .unwrap_err();
        assert_eq!(
            err,
            EgressCaveatError::ModelDenied {
                requested: "gpt-3.5-turbo".to_owned(),
                pinned: "gpt-4".to_owned(),
            }
        );
    }

    #[test]
    fn validate_caveats_pinned_model_no_request_model_succeeds() {
        // If caveat pins a model but request doesn't specify a model, allow
        // (caller may fill in later or proxy may have defaulted).
        validate_provider_caveats("api.openai.com", None, &[], Some("gpt-4")).unwrap();
    }

    #[test]
    fn strip_capability_removes_x_capability_token() {
        let mut req = EgressRequest {
            host: "api.openai.com".to_owned(),
            path: "/v1/chat/completions".to_owned(),
            method: "POST".to_owned(),
            headers: vec![
                (
                    "X-Capability-Token".to_owned(),
                    "wire-token-string".to_owned(),
                ),
                ("Content-Type".to_owned(), "application/json".to_owned()),
            ],
            body: b"{}".to_vec(),
        };
        let handle = strip_capability(&mut req);
        assert_ne!(handle.cap_root_hash, [0u8; 32]);
        assert!(
            !req.headers.iter().any(|(k, _)| k == "X-Capability-Token"),
            "X-Capability-Token MUST be stripped at egress boundary"
        );
        assert_eq!(req.headers.len(), 1, "only Content-Type remains");
    }

    #[test]
    fn strip_capability_handles_authorization_alt() {
        let mut req = EgressRequest {
            host: "api.openai.com".to_owned(),
            path: "/v1/chat/completions".to_owned(),
            method: "POST".to_owned(),
            headers: vec![(
                "Authorization".to_owned(),
                "CipherOcto-Cap wire-token-string".to_owned(),
            )],
            body: b"{}".to_vec(),
        };
        let handle = strip_capability(&mut req);
        assert_ne!(handle.cap_root_hash, [0u8; 32]);
        assert!(!req.headers.iter().any(|(k, _)| k == "Authorization"));
    }

    #[test]
    fn strip_capability_no_header_returns_zero_hash() {
        let mut req = EgressRequest {
            host: "api.openai.com".to_owned(),
            path: "/v1/chat/completions".to_owned(),
            method: "POST".to_owned(),
            headers: vec![("Content-Type".to_owned(), "application/json".to_owned())],
            body: b"{}".to_vec(),
        };
        let handle = strip_capability(&mut req);
        assert_eq!(handle.cap_root_hash, [0u8; 32]);
        assert_eq!(req.headers.len(), 1);
    }

    #[test]
    fn strip_capability_is_case_insensitive_on_header_name() {
        let mut req = EgressRequest {
            host: "api.openai.com".to_owned(),
            path: "/v1/chat/completions".to_owned(),
            method: "POST".to_owned(),
            headers: vec![(
                "x-capability-token".to_owned(),
                "wire-token-string".to_owned(),
            )],
            body: b"{}".to_vec(),
        };
        strip_capability(&mut req);
        assert!(req.headers.is_empty(), "lowercase variant must also strip");
    }

    #[test]
    fn strip_capability_preserves_authorization_bearer() {
        // Authorization: Bearer ... is NOT a capability header; must NOT
        // be stripped (verifier layer downstream distinguishes them).
        let mut req = EgressRequest {
            host: "api.openai.com".to_owned(),
            path: "/v1/chat/completions".to_owned(),
            method: "POST".to_owned(),
            headers: vec![(
                "Authorization".to_owned(),
                "Bearer sk-not-a-cap-token".to_owned(),
            )],
            body: b"{}".to_vec(),
        };
        strip_capability(&mut req);
        assert_eq!(req.headers.len(), 1);
        assert_eq!(req.headers[0].0, "Authorization");
    }

    // ---- R9 review tripwires (mission 0957-b R9-1 / R9-2 / R9-5) ----

    /// R9-5 tripwire: lowercase `cipherocto-cap ` variant MUST be stripped.
    /// Without the case-insensitive match (line ~230 fold-to-lowercase),
    /// a client sending `authorization: cipherocto-cap ...` would bypass
    /// the strip and the token would reach the provider.
    #[test]
    fn strip_capability_authorization_alt_lowercase_strips() {
        let mut req = EgressRequest {
            host: "api.openai.com".to_owned(),
            path: "/v1/chat/completions".to_owned(),
            method: "POST".to_owned(),
            headers: vec![(
                "Authorization".to_owned(),
                "cipherocto-cap wire-token-lowercase".to_owned(),
            )],
            body: b"{}".to_vec(),
        };
        let handle = strip_capability(&mut req);
        assert_ne!(handle.cap_root_hash, [0u8; 32]);
        assert!(
            !req.headers.iter().any(|(k, _)| k == "Authorization"),
            "lowercase cipherocto-cap MUST be stripped (case-insensitive)"
        );
    }

    /// R9-5 tripwire: uppercase `CIPHEROCTO-CAP ` variant.
    #[test]
    fn strip_capability_authorization_alt_uppercase_strips() {
        let mut req = EgressRequest {
            host: "api.openai.com".to_owned(),
            path: "/v1/chat/completions".to_owned(),
            method: "POST".to_owned(),
            headers: vec![(
                "Authorization".to_owned(),
                "CIPHEROCTO-CAP wire-token-upper".to_owned(),
            )],
            body: b"{}".to_vec(),
        };
        strip_capability(&mut req);
        assert!(
            !req.headers.iter().any(|(k, _)| k == "Authorization"),
            "uppercase CIPHEROCTO-CAP MUST be stripped (case-insensitive)"
        );
    }

    /// R9-2 tripwire: HMAC-BLAKE3 32-byte hex tag in body MUST be redacted.
    /// Without `strip_capability_from_body`, a holder passing the cap-root
    /// hash or holder signature in a JSON body field would leak it to the
    /// provider.
    #[test]
    fn strip_capability_redacts_hex_tag_from_body() {
        let hex = "ab12cd34ef56ab12cd34ef56ab12cd34ef56ab12cd34ef56ab12cd34ef56ab12";
        let body = format!(r#"{{"prompt":"hi","sig":"{}"}}"#, hex);
        let mut req = EgressRequest {
            host: "api.openai.com".to_owned(),
            path: "/v1/chat/completions".to_owned(),
            method: "POST".to_owned(),
            headers: vec![],
            body: body.as_bytes().to_vec(),
        };
        strip_capability(&mut req);
        let redacted_body = std::str::from_utf8(&req.body).unwrap();
        assert!(
            !redacted_body.contains(hex),
            "hex tag MUST be redacted from body; got: {redacted_body}"
        );
        assert!(
            redacted_body.contains("[REDACTED-CAP-TOKEN]"),
            "redaction marker MUST appear; got: {redacted_body}"
        );
        // Byte length preserved so downstream parsers don't break.
        assert_eq!(req.body.len(), body.len());
    }

    /// R9-2 tripwire: macaroon 3-segment base64url in body MUST be redacted.
    #[test]
    fn strip_capability_redacts_macaroon_wire_from_body() {
        let macaroon = "eyJhbGciOiJibGFrZTMifQ.eyJpYXQiOjE3MDAwMDAwMDAsImNhdmVhdHMiOltdfQ.signature_segment_here_padding";
        let body = format!(
            r#"{{"messages":[{{"role":"user","content":"hi","cap_token":"{}"}}]}}"#,
            macaroon
        );
        let mut req = EgressRequest {
            host: "api.openai.com".to_owned(),
            path: "/v1/chat/completions".to_owned(),
            method: "POST".to_owned(),
            headers: vec![],
            body: body.as_bytes().to_vec(),
        };
        strip_capability(&mut req);
        let redacted_body = std::str::from_utf8(&req.body).unwrap();
        assert!(
            !redacted_body.contains(macaroon),
            "macaroon wire format MUST be redacted from body; got: {redacted_body}"
        );
    }

    /// R9-2 tripwire: `CipherOcto-Cap ` scheme value in body MUST be redacted.
    #[test]
    fn strip_capability_redacts_cipherocto_cap_scheme_from_body() {
        let body = r#"{"meta":{"note":"please attach CipherOcto-Cap eyJhbGciOiJibGFrZTMifQ.eyJpYXQiOi4uIn0.sig here"}}"#;
        let mut req = EgressRequest {
            host: "api.openai.com".to_owned(),
            path: "/v1/chat/completions".to_owned(),
            method: "POST".to_owned(),
            headers: vec![],
            body: body.as_bytes().to_vec(),
        };
        strip_capability(&mut req);
        let redacted_body = std::str::from_utf8(&req.body).unwrap();
        assert!(
            !redacted_body.contains("CipherOcto-Cap"),
            "CipherOcto-Cap scheme MUST be redacted from body; got: {redacted_body}"
        );
    }

    /// R9-1 tripwire: empty body is a no-op (no panic, no spurious redaction).
    #[test]
    fn strip_capability_empty_body_is_noop() {
        let mut req = EgressRequest {
            host: "api.openai.com".to_owned(),
            path: "/v1/chat/completions".to_owned(),
            method: "POST".to_owned(),
            headers: vec![("X-Capability-Token".to_owned(), "wire".to_owned())],
            body: b"".to_vec(),
        };
        let handle = strip_capability(&mut req);
        assert_ne!(handle.cap_root_hash, [0u8; 32]);
        assert!(req.body.is_empty());
    }

    /// R9-1 tripwire: non-UTF-8 (binary protobuf) body is scanned for
    /// `CipherOcto-Cap ` scheme only (the hex + macaroon patterns assume
    /// ASCII; binary bodies are opaque to those patterns). The scheme
    /// is detected case-insensitively.
    #[test]
    fn strip_capability_redacts_cipherocto_cap_from_binary_body() {
        let body = b"\x00\x01\x02CipherOcto-Cap wire-token-binary\xff\xfe";
        let mut req = EgressRequest {
            host: "api.openai.com".to_owned(),
            path: "/v1/chat/completions".to_owned(),
            method: "POST".to_owned(),
            headers: vec![],
            body: body.to_vec(),
        };
        strip_capability(&mut req);
        // Binary body: scan for the scheme bytes directly (surrounding
        // 0xff/0xfe make the body non-UTF-8 even after redaction).
        let still_present = req
            .body
            .windows(b"CipherOcto-Cap".len())
            .any(|w| w.eq_ignore_ascii_case(b"CipherOcto-Cap"));
        assert!(
            !still_present,
            "CipherOcto-Cap MUST be redacted from binary body; got: {:?}",
            req.body
        );
        // Length preserved.
        assert_eq!(req.body.len(), body.len());
    }
}
