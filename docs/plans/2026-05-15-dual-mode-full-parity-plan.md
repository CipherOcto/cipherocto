# Dual-Mode Full Parity Implementation Plan

**Date:** 2026-05-15
**Goal:** quota-router as drop-in replacement for LiteLLM (litellm-mode) and any-llm (any-llm-mode)
**Persistence:** stoolap fork for all storage (replaces Redis, PostgreSQL, SQLite)

---

## Executive Summary

This plan achieves dual-mode full parity in 4 phases over 8-10 weeks. Each phase is independently testable and deployable.

**Total RFCs:** 18 accepted + 7 new planned + 3 to advance from draft
**Total work items:** 32

---

## Phase 1: Core Gateway (Weeks 1-3)

**Goal:** Wire existing infrastructure into proxy, enable auth and rate limiting.

### 1.1 Gateway Auth (RFC-0932)

| Task | Description | RFC | Dependencies |
|------|-------------|-----|--------------|
| 1.1.1 | Create auth middleware for proxy | RFC-0932 | None |
| 1.1.2 | Support both header formats (Authorization, X-AnyLLM-Key) | RFC-0932 | 1.1.1 |
| 1.1.3 | Wire key_storage lookup to auth | RFC-0932, RFC-0903 | 1.1.1 |
| 1.1.4 | Implement key type permissions (LlmApi, Management, ReadOnly) | RFC-0932 | 1.1.3 |
| 1.1.5 | Create management endpoints (/v1/keys/*, /v1/users/*) | RFC-0932 | 1.1.4 |
| 1.1.6 | Implement error responses (401, 403) | RFC-0932 | 1.1.1 |

**Acceptance criteria:**
- [ ] Valid LlmApi key → 200 on /v1/chat/completions
- [ ] ReadOnly key on POST → 403
- [ ] Revoked key → 401
- [ ] Master key bypasses all checks
- [ ] Both header formats work
- [ ] Management endpoints require Management key type

### 1.2 Rate Limiting (RFC-0933)

| Task | Description | RFC | Dependencies |
|------|-------------|-----|--------------|
| 1.2.1 | Create rate limit middleware | RFC-0933 | 1.1.1 (auth) |
| 1.2.2 | Wire TokenBucket to middleware | RFC-0933 | 1.2.1 |
| 1.2.3 | Add rate limit headers to responses | RFC-0933 | 1.2.1 |
| 1.2.4 | Implement per-key RPM/TPM limits | RFC-0933 | 1.2.2 |
| 1.2.5 | Create rate limit error responses (429) | RFC-0933 | 1.2.1 |
| 1.2.6 | Add stoolap persistence for rate limit state | RFC-0933, RFC-0914 | 1.2.2 |

**Acceptance criteria:**
- [ ] Request within RPM limit → 200
- [ ] Request exceeding RPM limit → 429 with retry_after
- [ ] Rate limit headers present in response
- [ ] Multiple keys have independent limits
- [ ] Rate limit resets after window
- [ ] stoolap persistence survives restart

### 1.3 Fallback Chains (UPDATE RFC-0902)

| Task | Description | RFC | Dependencies |
|------|-------------|-----|--------------|
| 1.3.1 | Wire FallbackConfig into Router | RFC-0902 | None |
| 1.3.2 | Implement default_fallbacks | RFC-0902 | 1.3.1 |
| 1.3.3 | Implement context_window_fallbacks | RFC-0902 | 1.3.1 |
| 1.3.4 | Implement content_policy_fallbacks | RFC-0902 | 1.3.1 |
| 1.3.5 | Add fallback activation metrics | RFC-0902, RFC-0937 | 1.3.1 |

**Acceptance criteria:**
- [ ] Primary provider failure triggers fallback
- [ ] Context window exceeded triggers context_window_fallbacks
- [ ] Content policy violation triggers content_policy_fallbacks
- [ ] Max fallbacks limit respected
- [ ] Fallback activations counted in metrics

---

## Phase 2: Config & API Parity (Weeks 3-5)

**Goal:** Full config compatibility with LiteLLM and any-llm.

### 2.1 Env Var Syntax (RFC-0931, RFC-0938)

| Task | Description | RFC | Dependencies |
|------|-------------|-----|--------------|
| 2.1.1 | Implement os.environ["KEY"] syntax | RFC-0931 | None |
| 2.1.2 | Implement ${VAR} YAML interpolation | RFC-0938 | None |
| 2.1.3 | Implement ANY_LLM_KEY universal key | RFC-0938 | None |
| 2.1.4 | Implement os.environ/ prefix (LiteLLM compat) | RFC-0935 | None |

**Acceptance criteria:**
- [ ] os.environ["KEY"] resolves from environment
- [ ] ${VAR} interpolation works in YAML
- [ ] ANY_LLM_KEY works as fallback for any provider
- [ ] os.environ/ prefix stripped for LiteLLM compat

### 2.2 Provider Registry (UPDATE RFC-0930)

| Task | Description | RFC | Dependencies |
|------|-------------|-----|--------------|
| 2.2.1 | Expand provider default registry from 6 to 41 | RFC-0930 | None |
| 2.2.2 | Add api_base defaults for all py_bridge providers | RFC-0930 | 2.2.1 |
| 2.2.3 | Implement ConfigError::MissingProvider variant | RFC-0930 | None |

**Acceptance criteria:**
- [ ] All 41 providers have default api_base in registry
- [ ] MissingProvider error returned when provider unknown
- [ ] Provider inference works for all providers

### 2.3 Exception Mapping (UPDATE RFC-0920)

| Task | Description | RFC | Dependencies |
|------|-------------|-----|--------------|
| 2.3.1 | Map ProviderError to LiteLLM exceptions | RFC-0920 | None |
| 2.3.2 | Map PyBridgeError to any-llm exceptions | RFC-0920 | None |
| 2.3.3 | Create exception types for both modes | RFC-0920 | 2.3.1, 2.3.2 |

**Acceptance criteria:**
- [ ] LiteLLM exceptions match Python SDK
- [ ] any-llm exceptions match Python SDK
- [ ] Error messages are compatible

---

## Phase 3: Advanced Features (Weeks 5-7)

**Goal:** Response caching, secret management, pre-call checks.

### 3.1 Response Caching (RFC-0906)

| Task | Description | RFC | Dependencies |
|------|-------------|-----|--------------|
| 3.1.1 | Design stoolap cache schema | RFC-0906, RFC-0914 | None |
| 3.1.2 | Implement cache key generation | RFC-0906 | 3.1.1 |
| 3.1.3 | Implement cache lookup and storage | RFC-0906 | 3.1.1 |
| 3.1.4 | Add cache TTL and eviction | RFC-0906 | 3.1.3 |
| 3.1.5 | Add cache hit/miss metrics | RFC-0906, RFC-0937 | 3.1.3 |

**Acceptance criteria:**
- [ ] Cache hit returns cached response
- [ ] Cache miss calls provider and caches result
- [ ] TTL expiration works
- [ ] Cache survives restart (stoolap)
- [ ] Cache hit/miss metrics available

### 3.2 Secret Manager (RFC-0935)

| Task | Description | RFC | Dependencies |
|------|-------------|-----|--------------|
| 3.2.1 | Implement SecretManager trait | RFC-0935 | None |
| 3.2.2 | Implement EnvSecretManager | RFC-0935 | 3.2.1 |
| 3.2.3 | Implement VaultSecretManager | RFC-0935 | 3.2.1 |
| 3.2.4 | Implement AwsSecretManager | RFC-0935 | 3.2.1 |
| 3.2.5 | Implement OIDC token resolution | RFC-0935 | 3.2.1 |
| 3.2.6 | Add secret caching in stoolap | RFC-0935, RFC-0914 | 3.2.1 |

**Acceptance criteria:**
- [ ] Env var lookup works
- [ ] HashiCorp Vault integration works
- [ ] AWS Secrets Manager integration works
- [ ] OIDC token resolution works
- [ ] Secret caching in stoolap works

### 3.3 Pre-call Checks (RFC-0936)

| Task | Description | RFC | Dependencies |
|------|-------------|-----|--------------|
| 3.3.1 | Implement ContextWindowCheck | RFC-0936 | None |
| 3.3.2 | Implement TagFilterCheck | RFC-0936 | None |
| 3.3.3 | Implement HealthCheck | RFC-0936 | None |
| 3.3.4 | Wire checks into Router | RFC-0936 | 3.3.1, 3.3.2, 3.3.3 |
| 3.3.5 | Add check failure metrics | RFC-0936, RFC-0937 | 3.3.4 |

**Acceptance criteria:**
- [ ] Context window check filters deployments
- [ ] Tag filter check passes/blocks correctly
- [ ] Health check marks unhealthy deployments
- [ ] Router only routes to deployments passing all checks
- [ ] Check failures counted in metrics

---

## Phase 4: Polish & Compatibility (Weeks 7-10)

**Goal:** Metrics, budget management, Python SDK, CLI.

### 4.1 Prometheus Metrics (RFC-0937)

| Task | Description | RFC | Dependencies |
|------|-------------|-----|--------------|
| 4.1.1 | Create Metrics struct with all counters | RFC-0937 | None |
| 4.1.2 | Implement metrics middleware | RFC-0937 | 4.1.1 |
| 4.1.3 | Create /metrics endpoint | RFC-0937 | 4.1.1 |
| 4.1.4 | Integrate metrics with all components | RFC-0937 | 4.1.1 |
| 4.1.5 | Add push gateway support (optional) | RFC-0937 | 4.1.1 |

**Acceptance criteria:**
- [ ] /metrics endpoint returns Prometheus format
- [ ] All metrics categories populated
- [ ] Metrics accurate and consistent

### 4.2 Budget Management (RFC-0934)

| Task | Description | RFC | Dependencies |
|------|-------------|-----|--------------|
| 4.2.1 | Design stoolap budget schema | RFC-0934, RFC-0914 | None |
| 4.2.2 | Implement spend tracking | RFC-0934 | 4.2.1 |
| 4.2.3 | Implement budget enforcement | RFC-0934 | 4.2.2 |
| 4.2.4 | Implement cost calculation | RFC-0934, RFC-0910 | None |
| 4.2.5 | Implement alert webhooks | RFC-0934 | 4.2.2 |
| 4.2.6 | Create budget management API | RFC-0934 | 4.2.1 |

**Acceptance criteria:**
- [ ] Spend tracking updates current_spend
- [ ] Hard limit blocks requests when exceeded
- [ ] Soft limit triggers alert webhook
- [ ] Budget reset on period boundary
- [ ] Per-key, per-user, per-team budgets work
- [ ] Cost calculation matches expected values

### 4.3 Python SDK Final Polish (RFC-0908, RFC-0920)

| Task | Description | RFC | Dependencies |
|------|-------------|-----|--------------|
| 4.3.1 | Test litellm-mode SDK compatibility | RFC-0908, RFC-0920 | All Phase 1-3 |
| 4.3.2 | Test any-llm-mode SDK compatibility | RFC-0908, RFC-0920 | All Phase 1-3 |
| 4.3.3 | Fix any API surface differences | RFC-0920 | 4.3.1, 4.3.2 |
| 4.3.4 | Fix any type mismatches | RFC-0920 | 4.3.1, 4.3.2 |
| 4.3.5 | Fix any exception mismatches | RFC-0920 | 4.3.1, 4.3.2 |

**Acceptance criteria:**
- [ ] litellm SDK tests pass with quota-router backend
- [ ] any-llm SDK tests pass with quota-router backend
- [ ] No API surface differences
- [ ] No type mismatches
- [ ] Exception messages match

---

## RFC Summary

### Existing RFCs (18 accepted)

| RFC | Title | Status |
|-----|-------|--------|
| 0902 | Multi-Provider Routing & Load Balancing | Accepted (needs update for fallbacks) |
| 0903 | Virtual API Key System | Accepted (needs wire to proxy) |
| 0904 | Real-Time Cost Tracking | Accepted |
| 0908 | Python SDK PyO3 Bindings | Accepted |
| 0910 | Pricing Table Registry | Accepted |
| 0912 | stoolap Row Locking | Accepted |
| 0913 | stoolap Pub/Sub | Accepted |
| 0917 | Dual-Mode Query Router | Accepted (needs update for endpoints) |
| 0920 | Unified Python SDK | Accepted (needs update for exceptions) |
| 0924 | Provider Metrics Bucket Tracking | Accepted |
| 0925 | Latency-Based Routing Extensions | Accepted |
| 0926 | Penalty Latency Scoring | Accepted |
| 0927 | RouterConfig Extension | Accepted |
| 0928 | Deployment Configuration Schema | Accepted |
| 0929 | GatewayConfig Provider Dispatch | Accepted |
| 0930 | Provider Inference | Accepted (needs update for registry) |
| 0931 | Env Var Parity | Accepted (needs implementation) |

### New RFCs (7 planned)

| RFC | Title | Priority |
|-----|-------|----------|
| 0932 | Gateway Auth & API Key Management | P0 |
| 0933 | Rate Limiting Integration | P0 |
| 0934 | Budget Management & Spend Tracking | P0 |
| 0935 | Secret Manager Integration | P1 |
| 0936 | Pre-call Checks | P1 |
| 0937 | Prometheus Metrics Endpoint | P1 |
| 0938 | YAML Interpolation & Universal Key | P2 |

### Draft RFCs to Advance (3)

| RFC | Title | Action |
|-----|-------|--------|
| 0906 | Response Caching | Review & accept |
| 0914 | stoolap-only Persistence | Review & accept |
| 0905 | Observability & Logging | Draft |

---

## Dependencies

```
Phase 1 (Core Gateway)
  ├── 1.1 Gateway Auth (RFC-0932)
  │   └── 1.2 Rate Limiting (RFC-0933) depends on auth
  └── 1.3 Fallback Chains (RFC-0902 update)

Phase 2 (Config & API)
  ├── 2.1 Env Var Syntax (RFC-0931, RFC-0938)
  ├── 2.2 Provider Registry (RFC-0930 update)
  └── 2.3 Exception Mapping (RFC-0920 update)

Phase 3 (Advanced Features)
  ├── 3.1 Response Caching (RFC-0906)
  ├── 3.2 Secret Manager (RFC-0935)
  └── 3.3 Pre-call Checks (RFC-0936)

Phase 4 (Polish)
  ├── 4.1 Prometheus Metrics (RFC-0937)
  ├── 4.2 Budget Management (RFC-0934) depends on Phase 1 auth
  └── 4.3 Python SDK Polish depends on all phases
```

---

## Success Criteria

### litellm-mode Parity
- [ ] All LiteLLM config files work without modification
- [ ] All LiteLLM API calls work without modification
- [ ] All LiteLLM exceptions match
- [ ] All LiteLLM routing strategies work
- [ ] All LiteLLM fallback chains work
- [ ] All LiteLLM secret managers work

### any-llm-mode Parity
- [ ] All any-llm config files work without modification
- [ ] All any-llm API calls work without modification
- [ ] All any-llm exceptions match
- [ ] All any-llm providers work
- [ ] any-llm gateway features work (auth, rate limiting, budgets)

### Persistence
- [ ] All state stored in stoolap (no Redis, PostgreSQL, SQLite)
- [ ] State survives restart
- [ ] Performance comparable to Redis/PostgreSQL
