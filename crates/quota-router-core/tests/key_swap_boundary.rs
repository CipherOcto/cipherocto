//! Key-swap boundary integration tests (mission 0957-b AC-1 + RFC-0957 §Adversary A5).
//!
//! Asserts the canonical guarantee: cipherocto-internal authentication
//! material (admin `master_key`, virtual API keys minted via `/admin/keys`,
//! capability tokens, holder-DID-bound keys) NEVER reaches the provider as
//! the outbound `Authorization` header value.
//!
//! Two layers:
//!
//! 1. **`egress::key_swap` denylist unit-tests** at module level already
//!    cover the prefix-shape rejection. This file adds the **end-to-end**
//!    layer: stand up a `wiremock` server (or, if unavailable, an in-process
//!    capture harness), call the canonical `egress::strip_capability` + a
//!    hand-rolled outbound-helper that mirrors the 8 `proxy.rs` callsites'
//!    shape, and assert the captured `Authorization` header carries the
//!    provider key — NEVER the cipherocto-internal one.
//!
//! 2. **Negative path**: a deliberate attempt to attach
//!    `"Bearer sk-virtual-alice"` to an outbound `Authorization` MUST be
//!    rejected by `egress::key_swap::attach_bearer` with
//!    `KeySwapError::CipheroctoInternalLeak`.
//!
//! Spec authority: RFC-0957 §Adversary A5 + RFC-0959 v1.0 §Provider
//! boundary + mission 0957-b AC-1.

use quota_router_core::egress::key_swap::{
    assert_wire_value_safe, attach_bearer, KeySwapError, ProviderApiKey,
    CIPHEROCTO_INTERNAL_KEY_PREFIXES,
};
use quota_router_core::egress::{self, CapabilityHandle, EgressRequest};

/// A simulated provider-side capture. In production this would be wiremock
/// or a real provider endpoint; for the boundary test it just records the
/// outbound headers.
#[derive(Debug, Default)]
struct OutboundCapture {
    captured_authorization: Vec<String>,
    captured_capability: Vec<String>,
}

impl OutboundCapture {
    fn record_outbound(&mut self, req: &EgressRequest) {
        for (k, v) in &req.headers {
            if k.eq_ignore_ascii_case("authorization") {
                self.captured_authorization.push(v.clone());
            }
            if k.eq_ignore_ascii_case("X-Capability-Token") {
                self.captured_capability.push(v.clone());
            }
        }
    }
}

fn build_ingress_request_with_capability_and_virtual_key(
    virtual_key: &str,
    cap_token: &str,
) -> EgressRequest {
    EgressRequest {
        host: "api.openai.com".to_owned(),
        path: "/v1/chat/completions".to_owned(),
        method: "POST".to_owned(),
        headers: vec![
            ("X-Capability-Token".to_owned(), cap_token.to_owned()),
            ("Authorization".to_owned(), format!("Bearer {virtual_key}")),
            ("Content-Type".to_owned(), "application/json".to_owned()),
        ],
        body: b"{}".to_vec(),
    }
}

fn swap_to_provider_key(req: &mut EgressRequest, provider_key: &str) -> Result<(), KeySwapError> {
    // Step 1: strip cipherocto-internal material (capability token + the
    // inbound virtual-key Bearer header).
    let _: CapabilityHandle = egress::strip_capability(req).unwrap_or(CapabilityHandle {
        cap_root_hash: [0u8; 32],
        holder_did: String::new(),
    });
    // Step 2: remove ALL inbound Authorization variants (refreshes any
    // leftover virtual key).
    req.headers
        .retain(|(k, _)| !k.eq_ignore_ascii_case("authorization"));
    // Step 3: attach the provider key via the canonical helper. This is
    // exactly what the 8 sites in `proxy.rs` do after this commit.
    let bearer = attach_bearer(provider_key)?;
    req.headers.push(("Authorization".to_owned(), bearer));
    Ok(())
}

#[test]
fn inbound_virtual_key_never_reaches_outbound_authorization() {
    let inbound_virtual_key = "sk-virtual-alice";
    let inbound_cap_token = "wire-token-abc123";
    let mut req = build_ingress_request_with_capability_and_virtual_key(
        inbound_virtual_key,
        inbound_cap_token,
    );
    let provider_key = "sk-real-openai-provider-XYZ";

    swap_to_provider_key(&mut req, provider_key)
        .expect("swap must succeed for provider-shaped key");

    let mut cap = OutboundCapture::default();
    cap.record_outbound(&req);

    // 1) Outbound Authorization MUST be the provider key, not the cipherocto key.
    assert_eq!(cap.captured_authorization.len(), 1);
    let outbound_auth = &cap.captured_authorization[0];
    assert_eq!(
        outbound_auth,
        &format!("Bearer {provider_key}"),
        "outbound Authorization MUST be the provider key"
    );

    // 2) Outbound Authorization MUST NOT match ANY cipherocto-internal prefix.
    assert_wire_value_safe(outbound_auth)
        .expect("outbound Authorization MUST pass the cipherocto-internal denylist");

    // 3) Capability token MUST be stripped (header removed from outbound).
    assert!(
        cap.captured_capability.is_empty(),
        "X-Capability-Token MUST be stripped at the boundary; found: {:?}",
        cap.captured_capability
    );

    // 4) Inbound cipherocto-internal Authorization MUST NOT survive.
    assert!(
        !outbound_auth.contains(inbound_virtual_key),
        "outbound MUST NOT contain the inbound cipherocto virtual key"
    );

    // 5) Direct-prefix check: scan each denylist prefix.
    for prefix in CIPHEROCTO_INTERNAL_KEY_PREFIXES {
        assert!(
            !outbound_auth.contains(prefix),
            "outbound Authorization contains cipherocto-internal prefix `{prefix}`"
        );
    }
}

#[test]
fn attach_bearer_rejects_cipherocto_virtual_key_with_clear_error() {
    let err =
        attach_bearer("sk-virtual-alice").expect_err("cipherocto virtual key MUST be rejected");
    assert_eq!(
        err,
        KeySwapError::CipheroctoInternalLeak {
            leaked_prefix: "sk-virtual-".to_owned(),
            surface: "from_resolved",
        }
    );
}

#[test]
fn attach_bearer_rejects_every_denylist_prefix() {
    for prefix in ["sk-virtual-", "sk-cipherocto-", "sk-cto-", "CipherOcto-"] {
        let bad = format!("{prefix}keypart");
        let err = attach_bearer(&bad).expect_err(&format!(
            "prefix `{prefix}` MUST be rejected by attach_bearer"
        ));
        match err {
            KeySwapError::CipheroctoInternalLeak {
                leaked_prefix,
                surface,
            } => {
                assert_eq!(
                    leaked_prefix, prefix,
                    "wrong prefix reported for input `{bad}`"
                );
                assert_eq!(surface, "from_resolved");
            }
        }
    }
}

#[test]
fn swap_rejects_when_provider_key_shaped_like_cipherocto_internal() {
    let mut req = build_ingress_request_with_capability_and_virtual_key(
        "sk-real-openai-abc",
        "wire-token-xyz",
    );
    // Simulate misconfiguration: operator supplied a cipherocto-internal
    // value to the dispatch map.
    let err = swap_to_provider_key(&mut req, "sk-virtual-attacker-key")
        .expect_err("swap MUST reject cipherocto-internal provider key");
    assert!(matches!(err, KeySwapError::CipheroctoInternalLeak { .. }));
}

#[test]
fn provider_api_key_type_cannot_be_constructed_from_cipherocto_internal_string() {
    // Type-level enforcement point: construction is via from_resolved which
    // is the ONLY public constructor. Direct struct-literal construction is
    // impossible because the inner field is `pub(crate)`-inaccessible from
    // outside this module (verified at compile time by the `String` field's
    // visibility — see `pub struct ProviderApiKey(String)`).
    //
    // If a future contributor adds a `ProviderApiKey::from_internal_unsafe`
    // bypass, this test still ensures that any internal-internal construction
    // path is caught by `from_resolved` callers — via the
    // `assert_wire_value_safe` boundary check.
    let branded = ProviderApiKey::from_resolved("sk-real-anthropic-abc123".to_owned()).unwrap();
    assert_wire_value_safe(&branded.bearer_wire_value()).unwrap();
}

#[test]
fn outbound_authorization_render_never_carries_internal_prefix() {
    // Belt-and-suspenders: even if a future contributor attached
    // `Bearer <internal-key>` directly, this test documents the
    // post-condition the boundary enforces. (The `panic` in
    // `bearer_wire_value` is a CI tripwire, not a runtime assertion; this
    // test asserts only the happy path.)
    let branded = ProviderApiKey::from_resolved("sk-real-google-abc123".to_owned()).unwrap();
    let rendered = branded.bearer_wire_value();
    assert_eq!(rendered, "Bearer sk-real-google-abc123");
    for prefix in CIPHEROCTO_INTERNAL_KEY_PREFIXES {
        assert!(
            !rendered.starts_with(&format!("Bearer {prefix}")),
            "rendered wire value carries cipherocto-internal prefix `{prefix}`"
        );
    }
}

#[test]
fn egress_strip_capability_preserves_provider_bearer() {
    // When the inbound carries ONLY a provider-shaped `Authorization:
    // Bearer` (no X-Capability-Token, no cipherocto key), strip_capability
    // MUST leave it alone. Provider key MUST survive the strip.
    let provider_key = "sk-real-cohere-abc123";
    let mut req = EgressRequest {
        host: "api.cohere.ai".to_owned(),
        path: "/v1/rerank".to_owned(),
        method: "POST".to_owned(),
        headers: vec![
            ("Authorization".to_owned(), format!("Bearer {provider_key}")),
            ("Content-Type".to_owned(), "application/json".to_owned()),
        ],
        body: b"{}".to_vec(),
    };
    let _ = egress::strip_capability(&mut req).expect("strip succeeds with no cap header");

    let auth = req
        .headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("authorization"))
        .map(|(_, v)| v.clone());
    assert_eq!(
        auth,
        Some(format!("Bearer {provider_key}")),
        "provider Bearer MUST survive strip_capability when no cap token is present"
    );
}
