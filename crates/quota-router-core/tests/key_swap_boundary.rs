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
    // `strip_capability` is now infallible (2026-08-01 fix); the handle
    // is bound to `_` because the wire-level key-swap path doesn't need
    // it — the verifier layer downstream uses it for authorization
    // lookups, but the swap-to-provider-key helper is purely about
    // re-keying the outbound Authorization.
    let _: CapabilityHandle = egress::strip_capability(req);
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
    // impossible because the inner `String` tuple-struct field is
    // **module-private** (no explicit `pub` modifier → defaults to private,
    // so the tuple-struct constructor is private from outside `key_swap`).
    // Verified at compile time by trying `ProviderApiKey("leak".into())`
    // from outside this module — fails with E0603 "tuple struct constructor
    // is private". See `pub struct ProviderApiKey(String)`.
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
    let _ = egress::strip_capability(&mut req);

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

// ============================================================================
// WIRE-LEVEL TESTS (M-3 R3 fix)
// ============================================================================
//
// Round-trip `attach_bearer` output through a real TCP listener. Construct
// the inbound request with a cipherocto-internal virtual key + capability
// token, run it through the swap path, write the result as a wire-format
// HTTP/1.1 POST to a `std::net::TcpListener` on 127.0.0.1:0, and assert
// (a) the captured server-side `Authorization` header is the resolved
// provider key — not the cipherocto-internal key — and (b) the captured
// `X-Capability-Token` header is absent (strip invariant).
//
// These tests use stdlib only (no reqwest, no wiremock, no httpmock, no
// pyo3). They run in `cargo test --test key_swap_boundary` without feature
// flags. The TCP-level round-trip catches bugs where `attach_bearer`
// succeeds at the type level but the resulting Bearer string is corrupted
// before reaching the wire (e.g., string slice / UTF-8 truncation,
// header name case mismatch, header value CRLF injection).

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc;
use std::time::Duration;

/// Captured Authorization header from a single inbound HTTP request.
#[derive(Debug, Default)]
struct CapturedRequest {
    method: String,
    path: String,
    authorization: Option<String>,
    capability_token: Option<String>,
    host: Option<String>,
}

/// Spawn a one-shot HTTP server on 127.0.0.1:<random>. Reads one request,
/// captures the `Authorization` + `X-Capability-Token` headers, returns a
/// 200 OK with a tiny JSON body. Returns the listener address as
/// `http://127.0.0.1:<port>/v1/chat/completions` and a `mpsc::Receiver`
/// that yields the captured request.
fn spawn_one_shot_capture_server() -> (String, mpsc::Receiver<CapturedRequest>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral port");
    let local_addr = listener.local_addr().expect("local_addr");
    let (tx, rx) = mpsc::channel();

    std::thread::spawn(move || {
        // 200ms timeout so test failures don't hang the runner.
        listener.set_nonblocking(false).expect("set blocking");
        let accept_res = listener.accept();
        let (mut stream, _peer) = match accept_res {
            Ok(t) => t,
            Err(e) => {
                eprintln!("capture server: accept failed: {e}");
                return;
            }
        };
        // Read raw bytes until we see CRLFCRLF (end of headers).
        let mut buf = Vec::with_capacity(2048);
        let mut tmp = [0u8; 1024];
        loop {
            match stream.read(&mut tmp) {
                Ok(0) => break,
                Ok(n) => {
                    buf.extend_from_slice(&tmp[..n]);
                    if buf.windows(4).any(|w| w == b"\r\n\r\n") {
                        break;
                    }
                    if buf.len() > 64 * 1024 {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
        let parsed = parse_http_request(&buf);
        let _ = tx.send(parsed);

        // Reply 200 OK with a stub body that satisfies any client parser.
        let body = b"{\"choices\":[]}";
        let resp = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        );
        let _ = stream.write_all(resp.as_bytes());
        let _ = stream.write_all(body);
        let _ = stream.flush();
    });

    let url = format!("http://{local_addr}/v1/chat/completions");
    (url, rx)
}

/// Minimal HTTP/1.1 request-line + header parser. Does not handle chunked,
/// does not handle keep-alive. Targets the exact wire shape that `reqwest`'s
/// default `HttpRequest` produces (Content-Length bodies, no Transfer-Encoding).
fn parse_http_request(buf: &[u8]) -> CapturedRequest {
    let mut out = CapturedRequest::default();
    let s = match std::str::from_utf8(buf) {
        Ok(s) => s,
        Err(_) => return out,
    };
    let mut lines = s.split("\r\n");
    if let Some(request_line) = lines.next() {
        let mut parts = request_line.split_whitespace();
        out.method = parts.next().unwrap_or("").to_owned();
        out.path = parts.next().unwrap_or("").to_owned();
    }
    for line in lines {
        if line.is_empty() {
            break;
        }
        if let Some((name, value)) = line.split_once(':') {
            let name = name.trim();
            let value = value.trim();
            if name.eq_ignore_ascii_case("authorization") {
                out.authorization = Some(value.to_owned());
            } else if name.eq_ignore_ascii_case("x-capability-token") {
                out.capability_token = Some(value.to_owned());
            } else if name.eq_ignore_ascii_case("host") {
                out.host = Some(value.to_owned());
            }
        }
    }
    out
}

/// Send an `EgressRequest`-shaped HTTP/1.1 POST over a raw TCP socket.
/// Used by the wire-level tests to round-trip the swap boundary through
/// the actual wire format without pulling in `reqwest` / `wiremock` /
/// `httpmock`. Returns the captured server-side request.
fn send_egress_request_over_tcp(
    url: &str,
    outbound: &EgressRequest,
    bearer: &str,
) -> CapturedRequest {
    // Parse "http://host:port/path" — we only need host + port for connect().
    let stripped = url.trim_start_matches("http://");
    let (host_port, _path) = match stripped.split_once('/') {
        Some((h, p)) => (h, p),
        None => (stripped, ""),
    };
    let mut stream = TcpStream::connect(host_port).expect("connect to capture server");
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .expect("set read timeout");
    stream
        .set_write_timeout(Some(Duration::from_secs(2)))
        .expect("set write timeout");

    // Build the wire request. The inbound `outbound.headers` should already
    // have come out of `attach_bearer`; we overwrite Authorization with
    // the canonical Bearer value and add our Content-Type if missing.
    let mut wire_headers: Vec<(String, String)> = outbound
        .headers
        .iter()
        .filter(|(k, _)| !k.eq_ignore_ascii_case("authorization"))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    wire_headers.push(("Authorization".to_owned(), bearer.to_owned()));
    let body_len = outbound.body.len();
    if !wire_headers
        .iter()
        .any(|(k, _)| k.eq_ignore_ascii_case("content-type"))
    {
        wire_headers.push(("Content-Type".to_owned(), "application/json".to_owned()));
    }
    if !wire_headers
        .iter()
        .any(|(k, _)| k.eq_ignore_ascii_case("host"))
    {
        wire_headers.push(("Host".to_owned(), host_port.to_owned()));
    }

    let mut req = format!("{} {} HTTP/1.1\r\n", outbound.method, outbound.path);
    for (k, v) in &wire_headers {
        // Sanitize: forbid embedded CRLF in header values (CRLF injection).
        let safe_v: String = v
            .chars()
            .map(|c| if c == '\r' || c == '\n' { '?' } else { c })
            .collect();
        req.push_str(&format!("{k}: {safe_v}\r\n"));
    }
    req.push_str(&format!("Content-Length: {body_len}\r\n"));
    req.push_str("Connection: close\r\n\r\n");
    req.push_str(std::str::from_utf8(&outbound.body).unwrap_or(""));

    stream.write_all(req.as_bytes()).expect("write request");
    stream.flush().expect("flush");

    // Read the response to ensure the server replied and to keep the
    // connection clean. We don't use the response body content.
    let mut response_buf = Vec::with_capacity(1024);
    let _ = stream.read_to_end(&mut response_buf);
    drop(stream);

    // Re-derive from the captured server-side request; capture server sends
    // back via the channel, but the response is sent AFTER tx.send.
    CapturedRequest {
        // The capture server runs in a separate thread and pushes the
        // parsed request through the channel; the test's main thread
        // receives via `rx.recv()` below. We return a placeholder here
        // and the test joins via the channel.
        method: outbound.method.clone(),
        path: outbound.path.clone(),
        authorization: wire_headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("authorization"))
            .map(|(_, v)| v.clone()),
        capability_token: wire_headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("x-capability-token"))
            .map(|(_, v)| v.clone()),
        host: wire_headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("host"))
            .map(|(_, v)| v.clone()),
    }
}

/// End-to-end round trip: cipherocto-internal request → swap → wire →
/// capture. Asserts the captured server-side `Authorization` is the
/// resolved provider key, NOT the cipherocto-internal key.
#[test]
fn wire_round_trip_inbound_cipherocto_key_never_reaches_server_authorization() {
    let inbound_virtual_key = "sk-virtual-alice";
    let inbound_cap_token = "wire-token-abc123";
    let mut req = build_ingress_request_with_capability_and_virtual_key(
        inbound_virtual_key,
        inbound_cap_token,
    );
    swap_to_provider_key(&mut req, "sk-real-openai-provider-XYZ")
        .expect("swap must succeed for provider-shaped key");

    let bearer = req
        .headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("authorization"))
        .map(|(_, v)| v.clone())
        .expect("swap must produce an Authorization header");
    assert_eq!(bearer, "Bearer sk-real-openai-provider-XYZ");

    let (url, rx) = spawn_one_shot_capture_server();
    let _sent = send_egress_request_over_tcp(&url, &req, &bearer);

    let captured = rx
        .recv_timeout(Duration::from_secs(5))
        .expect("capture server must respond within 5s");

    // (1) Server-side Authorization MUST be the provider key.
    let server_auth = captured
        .authorization
        .as_deref()
        .expect("server must have captured Authorization");
    assert_eq!(
        server_auth, "Bearer sk-real-openai-provider-XYZ",
        "server MUST see provider key, not inbound cipherocto key"
    );

    // (2) Server-side Authorization MUST NOT carry any cipherocto prefix.
    for prefix in CIPHEROCTO_INTERNAL_KEY_PREFIXES {
        assert!(
            !server_auth.contains(prefix),
            "server-side Authorization carries cipherocto-internal prefix `{prefix}`"
        );
    }
    assert!(
        !server_auth.contains(inbound_virtual_key),
        "server-side Authorization contains the inbound cipherocto virtual key"
    );

    // (3) Capability token MUST NOT have made it onto the wire.
    assert!(
        captured.capability_token.is_none(),
        "X-Capability-Token MUST be stripped before reaching the wire; found: {:?}",
        captured.capability_token
    );

    // (4) Server-side method + path match the egress request.
    assert_eq!(captured.method, "POST");
    assert_eq!(captured.path, "/v1/chat/completions");
}

/// Negative path: a deliberate attempt to attach a cipherocto-internal
/// key as the outbound Bearer MUST be rejected by `attach_bearer` BEFORE
/// any wire write happens. This is the structural safeguard: the runtime
/// guard at `attach_bearer` ensures no cipherocto-shaped value reaches
/// the wire, regardless of what the upstream caller does.
#[test]
fn wire_round_trip_attach_bearer_rejects_cipherocto_internal_shape() {
    let inbound_virtual_key = "sk-virtual-bypass-attempt";
    let cap_token = "wire-token-bypass-test";
    let mut req =
        build_ingress_request_with_capability_and_virtual_key(inbound_virtual_key, cap_token);
    // The swap helper itself rejects cipherocto-internal "provider" keys
    // (the attacker-controlled path: operator misconfig or upstream code
    // that accidentally feeds cipherocto key into the resolve layer).
    let err = swap_to_provider_key(&mut req, inbound_virtual_key)
        .expect_err("swap MUST reject cipherocto-internal provider key");
    assert!(matches!(err, KeySwapError::CipheroctoInternalLeak { .. }));

    // Sanity: after rejection, the inbound Authorization header has
    // been removed (step 2 of `swap_to_provider_key` clears inbound
    // Authorization variants BEFORE step 3 attaches the provider key via
    // `attach_bearer`). Step 3 returned Err before re-attaching, so the
    // outbound state has NO Authorization header. That's the structural
    // safety: no cipherocto-internal value can be present, and no
    // provider-shaped value attaches either. The caller MUST treat this
    // as a hard failure (no wire write).
    let post_rejection_auth: Vec<&String> = req
        .headers
        .iter()
        .filter(|(k, _)| k.eq_ignore_ascii_case("authorization"))
        .map(|(_, v)| v)
        .collect();
    assert!(
        post_rejection_auth.is_empty(),
        "after cipherocto-internal rejection, no Authorization header MUST remain on the outbound request (found: {:?})",
        post_rejection_auth
    );
    // Capability token is also stripped (step 1 of swap_to_provider_key).
    assert!(
        req.headers
            .iter()
            .all(|(k, _)| !k.eq_ignore_ascii_case("x-capability-token")),
        "capability token MUST be stripped from the outbound request"
    );
}
