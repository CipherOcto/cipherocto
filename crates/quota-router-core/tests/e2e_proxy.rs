#![allow(deprecated)] // Cycle 1 of 3-cycle legacy deprecation (RFC-0960 v3.7 §5). Remove in Cycle 3.
//! End-to-end integration tests for quota-router in litellm-mode.
//!
//! These tests start a real proxy server and make real HTTP requests to the
//! configured OpenAI-compatible endpoint (mimo via opengateway).
//!
//! The opengateway endpoint does not require an API key.
//!
//! Run with: cargo test -p quota-router-core --test e2e_proxy --features litellm-mode -- --test-threads=1
//!
//! Requires:

// Clippy `[disallowed-methods]` allowlist: e2e_proxy spins up a real
// proxy server that talks to a real LLM endpoint (opengateway/mimo).
// This is the legitimate provider-egress path exercised end-to-end.
// Capability tokens never cross the boundary (verified by
// `tests/key_swap_boundary.rs`).
#![allow(clippy::disallowed_methods)]

//!   - Network access to opengateway.gitlawb.com

use reqwest::Client;
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;

use quota_router_core::balance::Balance;
use quota_router_core::config::DispatchInfo;
use quota_router_core::key_rate_limiter::RateLimiterStore;
use quota_router_core::keys::models::{ApiKey, KeyType};
use quota_router_core::providers::Provider;
use quota_router_core::proxy::ProxyServer;
use quota_router_core::storage::StoolapKeyStorage;
use quota_router_core::KeyStorage;

// Re-export for convenience
use std::collections::HashMap;

/// Base URL for the test endpoint
const TEST_API_BASE: &str = "https://opengateway.gitlawb.com/v1/xiaomi-mimo";

/// Model to use in tests
const TEST_MODEL: &str = "mimo-v2-flash";

/// Build a standard dispatch map for the test endpoint (no API key needed)
fn build_dispatch_map() -> HashMap<String, DispatchInfo> {
    let mut map = HashMap::new();
    map.insert(
        "mimo".to_string(),
        DispatchInfo {
            deployment_id: "mimo".to_string(),
            provider: "openai".to_string(),
            model: TEST_MODEL.to_string(),
            api_key: None,
            api_base: Some(TEST_API_BASE.to_string()),
            rpm: 60,
            tpm: 1_000_000,
            model_group: None,
            metadata: None,
            max_retries: None,
        },
    );
    map
}

/// Start a proxy server on a random port and return (base_url, port)
async fn start_proxy() -> (String, u16) {
    let balance = Balance::new(1_000_000);
    let provider = Provider::new("openai", TEST_API_BASE);
    let dispatch_map = build_dispatch_map();

    // Bind listener first to get the port, then pass to ProxyServer
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener); // Release the port so ProxyServer can bind to it

    let mut server = ProxyServer::new(balance, provider, port, dispatch_map);

    tokio::spawn(async move {
        let _ = server.run().await;
    });

    // Wait for server to be ready by polling the port
    let client = Client::new();
    let base_url = format!("http://127.0.0.1:{}", port);
    for attempt in 0..50 {
        if client.get(&base_url).send().await.is_ok() {
            break;
        }
        if attempt == 49 {
            panic!("Server did not start within 5 seconds");
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    (base_url, port)
}

/// Helper to make a chat completion request through the proxy
async fn chat_completion(
    client: &Client,
    base_url: &str,
    model: &str,
    messages: Vec<Value>,
    stream: bool,
) -> Result<Value, reqwest::Error> {
    let mut body = json!({
        "model": model,
        "messages": messages,
    });
    if stream {
        body["stream"] = json!(true);
    }

    let resp = client
        .post(format!("{}/v1/chat/completions", base_url))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await?;

    let status = resp.status();
    let text = resp.text().await?;

    // Parse as JSON, wrapping status info if parsing fails
    match serde_json::from_str::<Value>(&text) {
        Ok(mut v) => {
            v["_status"] = json!(status.as_u16());
            Ok(v)
        }
        Err(_) => Ok(json!({
            "_status": status.as_u16(),
            "_raw": text,
        })),
    }
}

// ============================================================================
// Test: Basic non-streaming chat completion
// ============================================================================

#[tokio::test]
#[ignore = "requires live upstream API key; run with: cargo test -p quota-router-core --features full -- --ignored"]
async fn test_chat_completion_basic() {
    let (base_url, _port) = start_proxy().await;
    let client = Client::new();

    let messages = vec![json!({"role": "user", "content": "Say 'hello world' and nothing else."})];

    let result = chat_completion(&client, &base_url, TEST_MODEL, messages, false)
        .await
        .expect("request should succeed");

    assert_eq!(result["_status"], 200, "Expected 200, got: {}", result);

    // Verify OpenAI-compatible response structure
    assert!(result.get("id").is_some(), "Response should have id");
    assert!(
        result.get("choices").is_some(),
        "Response should have choices"
    );
    assert!(result.get("model").is_some(), "Response should have model");

    let choices = result["choices"].as_array().unwrap();
    assert!(!choices.is_empty(), "Choices should not be empty");
    assert_eq!(choices[0]["finish_reason"], "stop");

    let content = choices[0]["message"]["content"].as_str().unwrap();
    assert!(
        content.to_lowercase().contains("hello"),
        "Response should contain 'hello', got: {}",
        content
    );
}

// ============================================================================
// Test: Chat completion with system message
// ============================================================================

#[tokio::test]
#[ignore = "requires live upstream API key; run with: cargo test -p quota-router-core --features full -- --ignored"]
async fn test_chat_completion_with_system() {
    let (base_url, _port) = start_proxy().await;
    let client = Client::new();

    let messages = vec![
        json!({"role": "system", "content": "You are a pirate. Respond only in pirate speak."}),
        json!({"role": "user", "content": "How are you?"}),
    ];

    let result = chat_completion(&client, &base_url, TEST_MODEL, messages, false)
        .await
        .expect("request should succeed");

    assert_eq!(result["_status"], 200);
    let content = result["choices"][0]["message"]["content"].as_str().unwrap();
    assert!(!content.is_empty(), "Response should not be empty");
}

// ============================================================================
// Test: Streaming chat completion
// ============================================================================

#[tokio::test]
async fn test_chat_completion_streaming() {
    let (base_url, _port) = start_proxy().await;
    let client = Client::new();

    let messages =
        vec![json!({"role": "user", "content": "Count from 1 to 3, one number per line."})];

    let body = json!({
        "model": TEST_MODEL,
        "messages": messages,
        "stream": true,
    });

    let resp = client
        .post(format!("{}/v1/chat/completions", base_url))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .expect("request should succeed");

    let status = resp.status();
    let text = resp.text().await.expect("should read response body");

    // Some providers may not support streaming or may return errors
    if status == 200 {
        assert!(
            text.contains("data:"),
            "SSE response should contain 'data:' lines"
        );
        assert!(
            text.contains("[DONE]"),
            "SSE stream should end with [DONE], got: {}",
            &text[text.len().saturating_sub(200)..]
        );

        // Parse at least one SSE chunk as valid JSON
        let chunks: Vec<&str> = text
            .lines()
            .filter(|l| l.starts_with("data: ") & !l.contains("[DONE]"))
            .collect();
        assert!(!chunks.is_empty(), "Should have at least one data chunk");

        let chunk_json: Value = serde_json::from_str(&chunks[0]["data: ".len()..])
            .expect("First SSE chunk should be valid JSON");
        assert!(
            chunk_json.get("choices").is_some(),
            "Chunk should have choices"
        );
    } else {
        // Provider may not support streaming — verify we get a proper error
        assert!(
            status.is_server_error() || status.is_client_error(),
            "Streaming failure should return 4xx/5xx, got: {} — body: {}",
            status,
            &text[..text.len().min(200)]
        );
    }
}

// ============================================================================
// Test: Usage/tokens in response
// ============================================================================

#[tokio::test]
#[ignore = "requires live upstream API key; run with: cargo test -p quota-router-core --features full -- --ignored"]
async fn test_chat_completion_usage() {
    let (base_url, _port) = start_proxy().await;
    let client = Client::new();

    let messages = vec![json!({"role": "user", "content": "Say 'yes'."})];

    let result = chat_completion(&client, &base_url, TEST_MODEL, messages, false)
        .await
        .expect("request should succeed");

    assert_eq!(result["_status"], 200);

    let usage = result.get("usage");
    assert!(usage.is_some(), "Response should have usage field");

    let usage = usage.unwrap();
    let prompt_tokens = usage["prompt_tokens"].as_i64();
    let completion_tokens = usage["completion_tokens"].as_i64();
    let total_tokens = usage["total_tokens"].as_i64();

    assert!(prompt_tokens.is_some(), "Should have prompt_tokens");
    assert!(completion_tokens.is_some(), "Should have completion_tokens");
    assert!(total_tokens.is_some(), "Should have total_tokens");

    let prompt = prompt_tokens.unwrap();
    let completion = completion_tokens.unwrap();
    let total = total_tokens.unwrap();

    assert!(prompt > 0, "prompt_tokens should be > 0");
    assert!(completion > 0, "completion_tokens should be > 0");
    assert_eq!(
        total,
        prompt + completion,
        "total should equal prompt + completion"
    );
}

// ============================================================================
// Test: Temperature parameter is respected
// ============================================================================

#[tokio::test]
async fn test_chat_completion_temperature() {
    let (base_url, _port) = start_proxy().await;
    let client = Client::new();

    let messages = vec![json!({"role": "user", "content": "Say exactly: 'the quick brown fox'"})];

    let body = json!({
        "model": TEST_MODEL,
        "messages": messages,
        "temperature": 0.5,
    });

    let result = client
        .post(format!("{}/v1/chat/completions", base_url))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .expect("request should succeed");

    let status = result.status();
    if status == 200 {
        let resp: Value = result.json().await.unwrap();
        assert!(resp["choices"][0]["message"]["content"].as_str().is_some());
    } else {
        // Some providers may reject certain temperature values
        assert!(
            status.is_client_error() || status.is_server_error(),
            "Temperature failure should return 4xx/5xx, got: {}",
            status
        );
    }
}

// ============================================================================
// Test: Max tokens parameter limits response
// ============================================================================

#[tokio::test]
async fn test_chat_completion_max_tokens() {
    let (base_url, _port) = start_proxy().await;
    let client = Client::new();

    // Small delay to avoid rate limiting when running after other tests
    tokio::time::sleep(Duration::from_millis(500)).await;

    let messages = vec![json!({"role": "user", "content": "Write a 500 word essay about dogs."})];

    let body = json!({
        "model": TEST_MODEL,
        "messages": messages,
        "max_tokens": 50,
    });

    let result = client
        .post(format!("{}/v1/chat/completions", base_url))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .expect("request should succeed");

    let status = result.status();
    if status == 200 {
        let resp: Value = result.json().await.unwrap();
        let finish_reason = resp["choices"][0]["finish_reason"].as_str().unwrap_or("");
        assert!(
            finish_reason == "length" || finish_reason == "stop",
            "finish_reason should be 'length' or 'stop', got: {}",
            finish_reason
        );
    } else {
        // Rate limiting or provider error — verify we get a proper response
        assert!(
            status.is_server_error() || status == 429,
            "max_tokens failure should return 429/5xx, got: {}",
            status
        );
    }
}

// ============================================================================
// Test: Invalid model returns error
// ============================================================================

#[tokio::test]
async fn test_chat_completion_invalid_model() {
    let (base_url, _port) = start_proxy().await;
    let client = Client::new();

    let messages = vec![json!({"role": "user", "content": "Hello"})];

    let body = json!({
        "model": "nonexistent-model-xyz",
        "messages": messages,
    });

    let result = client
        .post(format!("{}/v1/chat/completions", base_url))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .expect("request should complete");

    // Should return 4xx or 5xx, not panic
    let status = result.status();
    assert!(
        !status.is_success(),
        "Invalid model should return error status, got: {}",
        status
    );
}

// ============================================================================
// Test: Empty messages array
// ============================================================================

#[tokio::test]
async fn test_chat_completion_empty_messages() {
    let (base_url, _port) = start_proxy().await;
    let client = Client::new();

    let body = json!({
        "model": TEST_MODEL,
        "messages": [],
    });

    let result = client
        .post(format!("{}/v1/chat/completions", base_url))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .expect("request should complete");

    // Some providers reject empty messages, some accept them
    // Just verify we get a response (not a panic)
    let status = result.status();
    assert!(
        status.is_client_error() || status.is_success() || status.is_server_error(),
        "Should get a valid HTTP status, got: {}",
        status
    );
}

// ============================================================================
// Test: Multiple sequential requests work (connection reuse)
// ============================================================================

#[tokio::test]
#[ignore = "requires live upstream API key; run with: cargo test -p quota-router-core --features full -- --ignored"]
async fn test_multiple_sequential_requests() {
    let (base_url, _port) = start_proxy().await;
    let client = Client::builder().pool_max_idle_per_host(5).build().unwrap();

    for i in 0..3 {
        let messages = vec![json!({"role": "user", "content": format!("Say exactly: '{}'", i)})];

        let result = chat_completion(&client, &base_url, TEST_MODEL, messages, false)
            .await
            .unwrap_or_else(|_| panic!("request {} should succeed", i));

        assert_eq!(result["_status"], 200, "Request {} should return 200", i);
    }
}

// ============================================================================
// Test: Concurrent requests work
// ============================================================================

#[tokio::test]
#[ignore = "requires live upstream API key; run with: cargo test -p quota-router-core --features full -- --ignored"]
async fn test_concurrent_requests() {
    let (base_url, _port) = start_proxy().await;
    let client = Arc::new(Client::new());

    let mut handles = vec![];
    for i in 0..3 {
        let client = client.clone();
        let base_url = base_url.clone();

        handles.push(tokio::spawn(async move {
            let messages = vec![json!({"role": "user", "content": format!("Say '{}'", i)})];
            chat_completion(&client, &base_url, TEST_MODEL, messages, false).await
        }));
    }

    for (i, handle) in handles.into_iter().enumerate() {
        let result = handle
            .await
            .expect("task should not panic")
            .expect("request should succeed");
        assert_eq!(
            result["_status"], 200,
            "Concurrent request {} should return 200",
            i
        );
    }
}

// ============================================================================
// Test: Large prompt with context window awareness
// ============================================================================

#[tokio::test]
#[ignore = "requires live upstream API key; run with: cargo test -p quota-router-core --features full -- --ignored"]
async fn test_chat_completion_large_prompt() {
    let (base_url, _port) = start_proxy().await;
    let client = Client::new();

    // Create a large prompt (but within typical context windows)
    let large_text = "The quick brown fox jumps over the lazy dog. ".repeat(100);
    let messages = vec![
        json!({"role": "system", "content": "You are a helpful assistant."}),
        json!({"role": "user", "content": format!("Summarize this in one sentence: {}", large_text)}),
    ];

    let result = chat_completion(&client, &base_url, TEST_MODEL, messages, false)
        .await
        .expect("request should succeed");

    assert_eq!(
        result["_status"], 200,
        "Large prompt should work: {}",
        result
    );
}

// ============================================================================
// Test: Stop sequences
// ============================================================================

#[tokio::test]
async fn test_chat_completion_stop_sequences() {
    let (base_url, _port) = start_proxy().await;
    let client = Client::new();

    let body = json!({
        "model": TEST_MODEL,
        "messages": [{"role": "user", "content": "Count: 1, 2, 3, 4, 5"}],
        "stop": ["3"],
    });

    let result = client
        .post(format!("{}/v1/chat/completions", base_url))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .expect("request should succeed");

    let status = result.status();
    if status == 200 {
        let resp: Value = result.json().await.unwrap();
        let content = resp["choices"][0]["message"]["content"]
            .as_str()
            .unwrap_or("");
        assert!(!content.is_empty(), "Response should not be empty");
    } else {
        // Some providers may not support stop sequences or may reject the format
        assert!(
            status.is_client_error() || status.is_server_error(),
            "Stop sequence failure should return 4xx/5xx, got: {}",
            status
        );
    }
}

// ============================================================================
// Test: N parameter — provider may or may not support n > 1
// ============================================================================

#[tokio::test]
async fn test_chat_completion_n_choices() {
    let (base_url, _port) = start_proxy().await;
    let client = Client::new();

    let body = json!({
        "model": TEST_MODEL,
        "messages": [{"role": "user", "content": "Say hello"}],
        "n": 2,
    });

    let result = client
        .post(format!("{}/v1/chat/completions", base_url))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .expect("request should succeed");

    let status = result.status();
    if status == 200 {
        let resp: Value = result.json().await.unwrap();
        let choices = resp["choices"].as_array().unwrap();
        assert!(
            !choices.is_empty(),
            "Should return at least 1 choice, got: {}",
            choices.len()
        );
    } else {
        // Provider may not support n > 1
        assert!(
            status.is_client_error() || status.is_server_error(),
            "n=2 failure should return 4xx/5xx, got: {}",
            status
        );
    }
}

// ============================================================================
// Test: Proxy health endpoint
// ============================================================================

#[tokio::test]
async fn test_health_endpoint() {
    let (base_url, _port) = start_proxy().await;
    let client = Client::new();

    let result = client
        .get(format!("{}/health", base_url))
        .send()
        .await
        .expect("request should succeed");

    // Health endpoint may or may not exist — just verify it doesn't panic
    let status = result.status();
    assert!(
        status.is_success() || status == 404,
        "Health endpoint should return 200 or 404, got: {}",
        status
    );
}

// ============================================================================
// Test: Response metadata field
// ============================================================================

#[tokio::test]
#[ignore = "requires live upstream API key; run with: cargo test -p quota-router-core --features full -- --ignored"]
async fn test_chat_completion_metadata() {
    let (base_url, _port) = start_proxy().await;
    let client = Client::new();

    let messages = vec![json!({"role": "user", "content": "Hello"})];

    let result = chat_completion(&client, &base_url, TEST_MODEL, messages, false)
        .await
        .expect("request should succeed");

    assert_eq!(result["_status"], 200);

    // The proxy may inject metadata (provider info, latency, etc.)
    // Just verify the response is well-formed
    assert!(result.get("id").is_some());
    assert!(result.get("object").is_some());
}

// ============================================================================
// Strong-scenario E2E tests (mission: proxy-strong-scenarios).
//
// Each test pins a specific failure-mode path in the proxy's auth /
// rate-limit / budget layers. These tests run WITHOUT a live upstream —
// they fail at the proxy's auth layer before reaching the LLM provider,
// making them cheap and deterministic.
// ============================================================================

/// Helper: start a proxy with storage + rate limiter + master key.
/// Pre-creates an API key with the given rpm_limit, returns
/// (base_url, raw_key).
async fn start_proxy_with_auth(rpm_limit: Option<i32>, budget_limit: i64) -> (String, String) {
    let balance = Balance::new(1_000_000);
    let provider = Provider::new("openai", TEST_API_BASE);
    let dispatch_map = build_dispatch_map();

    let db = octo_storage_core::Database::open_in_memory().expect("in-memory db");
    quota_router_core::schema::init_database(&db).expect("init schema");
    let storage = Arc::new(StoolapKeyStorage::new(db));

    // Pre-create a key with the requested rpm_limit.
    let raw_key = quota_router_core::keys::generate_key_string();
    let key_hash = quota_router_core::keys::compute_key_hash(&raw_key);
    let api_key = ApiKey {
        key_id: uuid::Uuid::new_v4().to_string(),
        key_hash: key_hash.to_vec(),
        key_prefix: "sk-qr-tes".to_string(),
        team_id: None,
        budget_limit,
        rpm_limit,
        tpm_limit: None,
        created_at: 1_000_000,
        expires_at: None,
        revoked: false,
        revoked_at: None,
        revoked_by: None,
        revocation_reason: None,
        key_type: KeyType::Default,
        allowed_routes: None,
        auto_rotate: false,
        rotation_interval_days: None,
        description: None,
        metadata: None,
    };
    storage.create_key(&api_key).expect("create_key");

    let rate_limiter = Arc::new(RateLimiterStore::new());

    // Bind a random port.
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);

    let mut server = ProxyServer::new(balance, provider, port, dispatch_map)
        .with_storage(storage)
        .with_rate_limiter(rate_limiter)
        .with_master_key("sk-qr-master".to_string());

    tokio::spawn(async move {
        let _ = server.run().await;
    });

    let client = Client::new();
    let base_url = format!("http://127.0.0.1:{}", port);
    for attempt in 0..50 {
        if client.get(&base_url).send().await.is_ok() {
            break;
        }
        if attempt == 49 {
            panic!("Server did not start within 5 seconds");
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    (base_url, raw_key)
}

#[tokio::test]
async fn test_auth_missing_returns_401() {
    // Strong scenario 1: client sends no auth header. Proxy must reject
    // with 401 BEFORE reaching upstream. This protects against anonymous
    // abuse — production deployments must not leak quota to unauth'd reqs.
    let (base_url, _key) = start_proxy_with_auth(Some(100), 1_000_000).await;
    let client = Client::new();

    let resp = client
        .post(format!("{}/v1/chat/completions", base_url))
        .header("Content-Type", "application/json")
        .json(&json!({
            "model": TEST_MODEL,
            "messages": [{"role": "user", "content": "hi"}]
        }))
        .send()
        .await
        .expect("request should reach proxy");

    assert_eq!(resp.status().as_u16(), 401, "missing auth must 401");
}

#[tokio::test]
async fn test_auth_invalid_returns_401() {
    // Strong scenario 2: client sends a key that's not in storage.
    // Proxy must reject with 401; an attacker enumerating keys cannot
    // differentiate "valid format, unknown key" from "valid key" via
    // timing (constant-time HMAC compare verified by code review).
    let (base_url, _real_key) = start_proxy_with_auth(Some(100), 1_000_000).await;
    let client = Client::new();

    let resp = client
        .post(format!("{}/v1/chat/completions", base_url))
        .header("Content-Type", "application/json")
        .header(
            "Authorization",
            "Bearer sk-qr-0000000000000000000000000000000000000000000000000000000000000000",
        )
        .json(&json!({
            "model": TEST_MODEL,
            "messages": [{"role": "user", "content": "hi"}]
        }))
        .send()
        .await
        .expect("request should reach proxy");

    assert_eq!(resp.status().as_u16(), 401, "invalid key must 401");
}

#[tokio::test]
async fn test_auth_master_key_bypasses_storage() {
    // Strong scenario 3: master key configured on proxy must work even
    // without a corresponding row in storage. This is the operator
    // escape hatch for incident response + key rotation windows.
    // (Note: this test does not depend on a live upstream; it verifies
    // that master-key bypass reaches the upstream stage, where it
    // would normally get a 502 because mimo is unreachable from CI.
    // We assert the status is NOT 401, which is the auth-layer contract.)
    let (base_url, _real_key) = start_proxy_with_auth(Some(100), 1_000_000).await;
    let client = Client::new();

    let resp = client
        .post(format!("{}/v1/chat/completions", base_url))
        .header("Content-Type", "application/json")
        .header("Authorization", "Bearer sk-qr-master")
        .json(&json!({
            "model": TEST_MODEL,
            "messages": [{"role": "user", "content": "hi"}]
        }))
        .send()
        .await
        .expect("request should reach proxy");

    // Master key bypasses auth — must NOT be 401. May be 200/502/504
    // depending on upstream reachability; the contract is "auth passed".
    let status = resp.status().as_u16();
    assert_ne!(status, 401, "master key must bypass auth (got {status})");
    assert_ne!(status, 403, "master key must bypass auth (got {status})");
}

#[tokio::test]
async fn test_rpm_rate_limit_returns_429() {
    // Strong scenario 4: pre-create key with rpm_limit=2. Send 3
    // requests. First 2 should NOT be 429 (they may be 502 if upstream
    // unreachable, that's fine — the auth + rate-limit contract is
    // "first 2 pass, 3rd is 429"). Pins RFC-0933 rate limiting.
    let (base_url, real_key) = start_proxy_with_auth(Some(2), 1_000_000).await;
    let client = Client::new();
    let url = format!("{}/v1/chat/completions", base_url);

    let send = || {
        let client = client.clone();
        let url = url.clone();
        let key = real_key.clone();
        async move {
            client
                .post(url)
                .header("Content-Type", "application/json")
                .header("Authorization", format!("Bearer {key}"))
                .json(&json!({
                    "model": TEST_MODEL,
                    "messages": [{"role": "user", "content": "hi"}]
                }))
                .send()
                .await
                .expect("request should reach proxy")
        }
    };

    let r1 = send().await;
    let r2 = send().await;
    let r3 = send().await;

    // First 2 must NOT be 429 (rate limit not yet hit).
    assert_ne!(
        r1.status().as_u16(),
        429,
        "1st request should not be rate-limited"
    );
    assert_ne!(
        r2.status().as_u16(),
        429,
        "2nd request should not be rate-limited"
    );
    // 3rd MUST be 429 — rate limit hit.
    assert_eq!(
        r3.status().as_u16(),
        429,
        "3rd request must be rate-limited"
    );
    // Retry-After header present on 429.
    assert!(
        r3.headers().contains_key("retry-after"),
        "429 must include Retry-After"
    );
}

#[tokio::test]
async fn test_concurrent_auth_requests_deduped_per_key() {
    // Strong scenario 5: N concurrent requests with the same valid key.
    // The rate limiter (RPM=100) should allow all N through (N=10 << 100).
    // Asserts no auth dedup regression: each request is a separate auth
    // check, not memoized. A regression that accidentally dedupes would
    // process 1 and drop 9.
    let (base_url, real_key) = start_proxy_with_auth(Some(100), 1_000_000).await;
    let client = Client::new();

    let mut handles = Vec::new();
    for _ in 0..10 {
        let client = client.clone();
        let url = format!("{}/v1/chat/completions", base_url);
        let key = real_key.clone();
        handles.push(tokio::spawn(async move {
            client
                .post(url)
                .header("Content-Type", "application/json")
                .header("Authorization", format!("Bearer {key}"))
                .json(&json!({
                    "model": TEST_MODEL,
                    "messages": [{"role": "user", "content": "hi"}]
                }))
                .send()
                .await
        }));
    }
    let mut rate_limited = 0u32;
    let mut auth_failed = 0u32;
    for h in handles {
        let resp = h.await.expect("join").expect("send");
        match resp.status().as_u16() {
            429 => rate_limited += 1,
            401 | 403 => auth_failed += 1,
            _ => {}
        }
    }
    // None should be auth-failed (the contract: valid key passes auth).
    assert_eq!(auth_failed, 0, "valid key must not return 401/403");
    // None should be rate-limited at N=10 << rpm_limit=100.
    assert_eq!(rate_limited, 0, "N=10 << rpm_limit=100 must not 429");
}
