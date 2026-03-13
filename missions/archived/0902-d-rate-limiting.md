# Mission: RFC-0902-d: Rate Limiting

## Status

Claimed

## RFC

RFC-0902 (Economics): Multi-Provider Routing and Load Balancing

## Dependencies

- Mission-0902-a: Routing Strategy Core
- Mission-0902-b: Advanced Routing Strategies

## Acceptance Criteria

- [x] RPM (requests per minute) tracking
- [x] TPM (tokens per minute) tracking
- [x] Soft mode: RPM/TPM for routing decisions only
- [x] Hard mode: Strict blocking when limit exceeded
- [ ] Redis support for multi-instance deployment (future)
- [x] 429 response on hard limit exceeded
- [x] Unit tests for rate limiting

## Description

Implement rate limiting enforcement based on LiteLLM's implementation:

- Track RPM/TPM usage per deployment
- Two enforcement modes: soft (routing) and hard (blocking)
- Shared state via Redis for multi-instance deployments

## Technical Details

### Rate Limit Modes (LiteLLM reference)

| Mode | Behavior | Use Case |
|------|----------|----------|
| **Soft (default)** | RPM/TPM used for routing decisions only | Prefer available capacity |
| **Hard** | Hard blocking when limit exceeded | Strict enforcement |

### Configuration

```yaml
router_settings:
  # Enable strict rate limiting
  optional_pre_call_checks:
    - enforce_model_rate_limits

model_list:
  - model_name: gpt-4
    litellm_params:
      model: openai/gpt-4
      rpm: 60      # 60 requests per minute
      tpm: 90000   # 90k tokens per minute

  # For multi-instance deployments
  redis_host: redis.example.com
  redis_port: 6379
```

### Error Response (Hard Mode)

```json
{
  "error": {
    "message": "Model rate limit exceeded. RPM limit=60, current usage=60",
    "type": "rate_limit_error",
    "code": 429
  }
}
```

Response includes `retry-after: 60` header.

## Implementation Notes

**Files created:**
- `crates/quota-router-core/src/rate_limit.rs` - New rate limiting module

**Implemented:**
- RateLimitMode enum (Soft/Hard)
- RateLimitConfig with rpm/tpm limits
- RateLimiter with usage tracking
- RateLimiterManager for multi-model groups
- RateLimitResult with blocked reason and retry_after

**Deferred:**
- Redis support (future enhancement)

**Tests:** 5 rate limit tests passing (27 total)

---

**Claimant:** @claude-code

**Pull Request:** #

**Related RFCs:**
- RFC-0902: Multi-Provider Routing and Load Balancing
- RFC-0904: Real-Time Cost Tracking
