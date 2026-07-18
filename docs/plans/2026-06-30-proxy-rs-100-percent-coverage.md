# proxy.rs 100% Coverage — Complete Execution Plan

**Goal:** 1294/1294 lines covered in `crates/quota-router-core/src/proxy.rs`. Currently 861/1294 (66.5%). Every line below must be covered.

---

## Prerequisites

### A. Create `FailingBody` type in test module
A custom `HttpBody` impl whose `poll_frame` returns `Err(io::Error)`. This allows constructing `Request<FailingBody>` to trigger every `body.collect().await` → `Err(_)` branch across all endpoints. Without this, those branches are unreachable because `Request<String>` always succeeds at collect.

### B. Create `HalfCloseBody` type in test module
A custom `HttpBody` that returns one `Ok(frame)` then hangs forever (never resolves poll). This covers the `Poll::Pending` path at line 154 in `SseBody::poll_frame`.

---

## Every uncovered line mapped to a test

Each test below is identified by its target lines and what it exercises.

---

### ProxyServer::run() — lines 254-334

**1.** `test_proxy_server_run_starts_and_serves_one_request` — Bind ProxyServer on port 0, spawn `server.run()`, connect via `TcpStream`, send `GET /health` over raw TCP, assert 200 OK response received. Covers lines 254-256 (bind), 258 (info log), 260-271 (field clones), 274-277 (init_providers), 279-282 (spawn), 284-296 (accept loop + clone), 298-299 (TokioIo wrap), 301-321 (http1::Builder + service_fn + handle_request), 325-327 (error logging), 332-334 (Ok return).

**2.** `test_proxy_server_run_multiple_connections` — Same setup but send 3 sequential requests to exercise the accept loop iterating. Covers lines 284-330 (loop body executed multiple times).

**3.** `test_proxy_server_run_connection_error` — Start server, connect, send malformed HTTP, close. Covers line 327 (eprintln error branch).

---

### SseBody::poll_frame Pending — line 154

**4.** `test_sse_body_pending_state` — Create `SseBody::new` with an empty channel receiver (no items sent yet), poll it, assert `Poll::Pending`. Covers line 154.

---

### handle_request metrics — line 547

**5.** `test_handle_request_records_metrics_on_success` — Call handle_request with metrics enabled for any successful path (e.g., `/health`). Verify `metrics.requests_total` incremented. Covers line 547.

**6.** `test_handle_request_records_request_duration` — Same but verify `metrics.request_duration` histogram has entries. Covers line 601 (start time) and the duration recording.

---

### RPM rate limited error — lines 646-650, 661-666, 669

**7.** `test_handle_request_rpm_rate_limited_returns_429_with_retry_after` — Create key with rpm_limit=1, exhaust bucket, verify response has `retry-after` header and 429 status with JSON body containing `retry_after` field. Covers lines 646-650 (body construction), 653-659 (response building).

**8.** `test_handle_request_rpm_internal_error_returns_500` — Force `check_rpm_only` to return a non-RateLimited error. Covers lines 661-666 (internal error body), 669 (return).

---

### Key validation error — lines 688-691, 693

**9.** `test_handle_request_key_lookup_error_returns_500` — Cause `lookup_by_hash` to return `Err(...)`. Requires corrupting the database or mocking storage. Covers lines 688-691 (error body), 693 (return).

---

### Team budget error — lines 707-708, 715-716

**10.** `test_handle_request_team_budget_lookup_error_logs_warning` — Cause `get_budget` to return `Err(...)`. Covers lines 715-716 (tracing::warn).

**11.** `test_handle_request_team_budget_lookup_returns_ok_none` — Team has no budget configured. Covers line 714 (Ok(None) arm — currently only tested via the Ok(Some) path).

---

### Metrics observation after models endpoint — line 773

**12.** `test_handle_request_models_endpoint_records_duration` — Call `/v1/models` with metrics, verify duration recorded. Covers line 773.

---

### Embeddings endpoint body-collect failure — lines 798-801, 804

**13.** `test_embeddings_body_collect_failure` — Send POST `/v1/embeddings` with `FailingBody`. Covers lines 798-801 (error body), 804 (return).

---

### Embeddings model extraction — lines 816-817

**14.** `test_embeddings_no_model_in_body` — Send `/v1/embeddings` with body `{"input":"hi"}` (no model field). Covers lines 816-817 (request_model extraction when model missing).

---

### Embeddings metrics — line 835

**15.** `test_embeddings_records_duration` — Call `/v1/embeddings` with metrics, verify duration. Covers line 835.

---

### Completions endpoint body-collect failure — lines 861-864, 867

**16.** `test_completions_body_collect_failure` — Send POST `/v1/completions` with `FailingBody`. Covers lines 861-864 (error), 867 (return).

---

### Completions metrics — line 875

**17.** `test_completions_records_duration` — Call `/v1/completions` with metrics. Covers line 875.

---

### Moderations endpoint body-collect failure — lines 897-899, 901

**18.** `test_moderations_body_collect_failure` — Send POST `/v1/moderations` with `FailingBody`. Covers lines 897-899 (error), 901 (return).

---

### Moderations API key — line 919

**19.** `test_moderations_with_api_key_from_dispatch` — Set `api_key` in DispatchInfo for the "openai" provider entry. Verify the Authorization header is sent upstream. Covers line 919.

---

### Moderations status conversion — line 926

**20.** `test_moderations_upstream_returns_non_standard_status` — Mock returns status 502. Verify conversion to `StatusCode::BAD_GATEWAY`. Covers line 926.

---

### Moderations upstream error — lines 937-940, 942

**21.** `test_moderations_upstream_network_error` — Mock server drops connection. Covers lines 937-940 (error body), 942 (return).

---

### Messages endpoint body-collect failure — lines 953-955, 957

**22.** `test_messages_body_collect_failure` — Send POST `/v1/messages` with `FailingBody`. Covers lines 953-955 (error), 957 (return).

---

### Messages API key — line 971

**23.** `test_messages_with_api_key_from_dispatch` — Set `api_key` in DispatchInfo for "anthropic". Verify header. Covers line 971.

---

### Messages status conversion — line 978

**24.** `test_messages_upstream_returns_non_standard_status` — Mock returns 502. Covers line 978.

---

### Messages upstream error — lines 989-992, 994

**25.** `test_messages_upstream_network_error` — Mock drops connection. Covers lines 989-992 (error body), 994 (return).

---

### Images endpoint — lines 1010-1084

**26.** `test_images_post_success` — POST `/v1/images/generations` with dall-e-3 model in dispatch_map, mock returns success. Covers lines 1010-1012 (body collect), 1014-1016 (utf8), 1018 (model extract), 1023-1024, 1026 (dispatch lookup), 1029-1033, 1037-1041 (api_key fallback to openai), 1043 (resolve_api_key), 1046-1052 (base_url fallback), 1054 (unwrap), 1056-1057, 1059-1061 (builder + auth header), 1063 (send), 1065-1071 (status conversion + body read), 1073-1074, 1077 (return).

**27.** `test_images_upstream_network_error` — Mock server unreachable. Covers lines 1079-1082 (error body), 1084 (return).

---

### Audio endpoint — lines 1095-1097, 1099, 1116, 1133-1136, 1138

**28.** `test_audio_transcription_success` — POST `/v1/audio/transcriptions` with whisper-1 in dispatch_map, mock returns success. Covers lines 1095-1097 (body collect error branch — need FailingBody variant), 1099 (return), 1116 (success path through audio handler).

**29.** `test_audio_speech_success` — POST `/v1/audio/speech` with tts-1, mock returns binary audio. Covers lines 1133-1136 (status + body), 1138 (return).

---

### Responses endpoint — lines 1149-1151, 1153, 1171, 1189-1192, 1194

**30.** `test_responses_post_success` — POST `/v1/responses` with gpt-4o, mock returns response object. Covers lines 1149-1151 (body collect), 1153 (utf8), 1171 (dispatch), 1189-1192 (forward + response), 1194 (return).

---

### Files endpoint — lines 1238-1243, 1246, 1255-1257, 1259, 1265-1268, 1271, 1276-1278, 1289-1292, 1295, 1298, 1310-1313, 1338-1339, 1341-1343, 1345, 1351, 1353-1357, 1359-1360, 1362, 1368, 1386-1389, 1391

**31.** `test_files_upload_success` — POST `/v1/files` with valid purpose + base64 content. Covers file upload parsing, validation, forwarding.

**32.** `test_files_upload_invalid_base64` — Upload with invalid base64 content. Covers validation error.

**33.** `test_files_upload_no_purpose` — Upload without purpose field. Covers missing-field validation.

**34.** `test_files_list_success` — GET `/v1/files`. Covers list forwarding.

**35.** `test_files_retrieve_success` — GET `/v1/files/file-id`. Covers retrieve forwarding with validate_resource_id.

**36.** `test_files_retrieve_invalid_id` — GET `/v1/files/../../etc`. Covers path traversal rejection.

**37.** `test_files_delete_success` — DELETE `/v1/files/file-id`. Covers delete forwarding.

**38.** `test_files_upstream_error` — Mock returns error for file operation. Covers upstream error paths.

---

### Batches endpoint — lines 1434-1436, 1438, 1448-1451, 1474, 1477, 1489-1490, 1492, 1496, 1514-1517, 1519

**39.** `test_batches_create_success` — POST `/v1/batches`. Covers create forwarding.

**40.** `test_batches_retrieve_success` — GET `/v1/batches/batch-123`. Covers retrieve with validate_resource_id.

**41.** `test_batches_cancel_success` — POST `/v1/batches/batch-123/cancel`. Covers cancel sub-path parsing.

**42.** `test_batches_list_success` — GET `/v1/batches`. Covers list forwarding.

**43.** `test_batches_invalid_id` — GET `/v1/batches/../../etc`. Covers path traversal.

**44.** `test_batches_upstream_error` — Mock returns error. Covers error path.

---

### Rerank endpoint — lines 1535-1537, 1539-1541, 1543, 1548-1549, 1551, 1554-1558, 1562-1566, 1568, 1571-1577, 1579, 1581-1582, 1584-1586, 1588, 1590-1592, 1594-1596, 1598-1599, 1602, 1604-1607, 1609

**45.** `test_rerank_success` — POST `/v1/rerank` with rerank model in dispatch_map. Covers full rerank forwarding path.

**46.** `test_rerank_with_api_key` — Set api_key in dispatch, verify header forwarded.

**47.** `test_rerank_upstream_error` — Mock returns error. Covers error paths.

---

### Provider passthrough — lines 1662-1667, 1680-1682, 1684, 1695-1696, 1698, 1710

**48.** `test_provider_passthrough_default_api_base` — Send request to `/anthropic/v1/messages` WITHOUT dispatch_map entry for anthropic. Verify default API base `"https://api.anthropic.com"` is used. Covers lines 1662-1667 (default api_base map).

**49.** `test_provider_passthrough_with_query_string` — Send GET to `/openai/models?limit=10`. Verify query string forwarded. Covers line 1648 (query extraction).

**50.** `test_provider_passthrough_with_api_key_from_env` — Set `OPENAI_API_KEY` env var, send passthrough request without dispatch api_key. Covers line 1710 (env var fallback).

---

### Balance check for chat completions — lines 1757-1760, 1763

**51.** `test_chat_completions_body_collect_failure` — Send POST `/v1/chat/completions` with `FailingBody`. Covers lines 1757-1760 (error), 1763 (return).

---

### Model extraction with model_group/deployment_id — lines 1777-1778

**52.** `test_dispatch_lookup_by_model_group` — Set `model_group: Some("gpt-group")` in DispatchInfo, send request with model matching the group. Covers lines 1777-1778.

**53.** `test_dispatch_lookup_by_deployment_id` — Set `deployment_id: "my-deploy"`, send request with model matching deployment_id. Covers same lines via different branch.

---

### Response cache hit — lines 1862, 1873

**54.** `test_cache_hit_returns_cached_response` — Pre-populate ResponseCache with a key matching the request, send request, verify `x-cache: HIT` header. Covers lines 1862 (cache hit), 1873 (metrics).

**55.** `test_cache_miss_increments_counter` — Send request with cache enabled but no match. Verify `metrics.cache_misses` incremented. Covers lines 1879-1880.

---

### Post-success cache storage — lines 2146, 2149, 2152

**56.** `test_successful_response_at_cache_storage` — Send successful request with cache enabled. Covers lines 2146-2152 (cache key generation + storage attempt).

---

### Context window check — lines 1897-1898, 1900-1902

**57.** `test_context_window_check_triggered` — Create FallbackExecutor with `context_window_fallbacks: {"gpt-4o": ["gpt-4o-mini"]}`. Set DispatchInfo metadata with `max_input_tokens: 10`. Send request with model "gpt-4o". Covers lines 1897-1902 (context window check setup + deployment info construction).

---

### Context window exceeded with fallbacks — lines 1935-1938, 1951-1954, 1958-1960, 1965

**58.** `test_context_window_exceeded_with_fallback_success` — Make context window check return Exceeded with fallback models. Fallback model succeeds. Covers lines 1935-1938 (completion_request default), 1951-1954 (ContextWindowResult::Exceeded arm), 1987-1988, 1994-1999 (try_fallback_models call), 2001-2002, 2004-2005 (success path).

**59.** `test_context_window_exceeded_all_fallbacks_fail` — Context window exceeded, all fallbacks fail. Covers lines 2008-2011 (failure body), 2013, 2016 (return).

**60.** `test_context_window_exceeded_no_fallback_models` — Context window exceeded, no fallback models configured. Covers lines 2020-2022 (body), 2024 (return).

---

### Context window exceeded no fallback — lines 1954-1957, 1965

**61.** `test_context_window_exceeded_no_fallback_returns_400` — Context window exceeded but no fallback models available (ExceededNoFallback variant). Covers lines 1954-1957 (input_tokens/max_tokens extraction), 1958-1965 (error body + return).

---

### Health blocked — lines 2028-2030, 2035, 2037, 2040-2045, 2047-2049, 2052-2055, 2058, 2061-2064, 2067, 2070-2072, 2074

**62.** `test_health_blocked_fallback_success` — Mark model unhealthy in executor. Send request. Fallback model succeeds. Covers lines 2028-2030 (health_blocked=true), 2035-2037 (get_fallback_models), 2040-2045 (try_fallback_models), 2047-2049 (success return).

**63.** `test_health_blocked_all_fallbacks_fail` — Model unhealthy, fallbacks all fail. Covers lines 2052-2055 (error body), 2058 (return).

**64.** `test_health_blocked_no_fallback_models` — Model unhealthy, no fallbacks configured. Covers lines 2061-2064 (error body), 2067 (return).

**65.** `test_health_blocked_no_executor` — Model unhealthy but no executor. Covers lines 2070-2072 (error body), 2074 (return).

---

### Post-dispatch fallback — lines 2092, 2115-2118, 2120

**66.** `test_primary_5xx_triggers_fallback_success` — Primary mock returns 503, fallback returns success. Verify fallback response returned. Covers lines 2092 (status check), 2115-2118 (record_success).

**67.** `test_primary_success_records_success` — Primary returns 200. Verify `executor.record_success()` called. Covers line 2117-2118, 2120.

---

### Structured logging — line 2174

**68.** `test_structured_logging_emitted` — Any successful request through handle_request. The tracing::info! macro at line 2174 is executed. Covers line 2174.

---

### handle_streaming structured chunk — lines 2497, 2503-2505, 2507

**69.** `test_handle_streaming_structured_chunk_skipped` — Create a mock that returns valid SSE, but the provider returns `StreamingChunk::Structured(...)` instead of `RawSSE`. Covers lines 2503-2505 (Structured variant match arm).

**70.** `test_handle_streaming_error_chunk` — Mock returns SSE error. Covers lines 2563-2566 (error chunk forwarding), 2574-2576 (error body construction), 2584 ([DONE] marker after error).

---

### handle_streaming no key — line 2497

**71.** `test_handle_streaming_no_api_key` — Call handle_streaming with `api_key: None`. Covers line 2497 (None path through provider call).

---

### handle_embedding_request provider not found — lines 2819-2821, 2826

**72.** `test_handle_embedding_provider_not_found` — Call handle_embedding_request with unknown provider name. Covers lines 2819-2821 (error body), 2826 (return).

---

### try_fallback_models empty key — lines 2911-2912, 2914

**73.** `test_fallback_empty_api_key_skipped` — Set DispatchInfo `api_key: Some("")`. Verify empty string is skipped. Covers lines 2911-2912 (empty check), 2914 (None fallback).

---

## Test infrastructure additions

**74.** Add `FailingBody` struct implementing `HttpBody` that returns `Err` on every poll.

**75.** Add helper `make_failing_request(method, uri)` that builds `Request<FailingBody>`.

**76.** Add helper `make_dispatch_with_metadata(base_url, model, provider, metadata)` that constructs DispatchInfo with arbitrary metadata JSON (for context window tests).

---

## Total: 76 items (73 tests + 3 infrastructure)

## Execution strategy

All 76 items written as test functions in the existing `#[cfg(test)] mod tests` block. No new files. No new modules. Tests execute in a single `cargo test` pass.

## Verification

After all tests are written:
1. `cargo test --features litellm-mode -p quota-router-core` — 0 failures
2. `cargo clippy --features litellm-mode -p quota-router-core --all-targets -- -D warnings` — 0 warnings
3. `cargo tarpaulin --features litellm-mode -p quota-router-core --lib` — proxy.rs shows 1294/1294 (100%) or ≤5 lines gap
