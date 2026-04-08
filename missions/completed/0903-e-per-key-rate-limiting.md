# Mission: Token Bucket Rate Limiter

## Status
Completed (2026-03-14)

## RFC
RFC-0903: Virtual API Key System

## Summary
Implement token bucket rate limiting per key: RPM (requests per minute) and TPM (tokens per minute).

## Acceptance Criteria
- [ ] Create rate_limiter.rs with TokenBucket
- [ ] Implement RPM limiter per key
- [ ] Implement TPM limiter per key
- [ ] Implement rate limit exceeded error
- [ ] Integrate with key cache for fast lookups
- [ ] Unit tests for token bucket algorithm

## Complexity
Medium

## Prerequisites
- Mission 0903-a (Key Core) - per-key limits
- Mission 0903-d (Key Cache) - for fast key lookup

## Implementation Notes
- Token bucket algorithm: bucket_size, refill_rate
- RPM: requests per minute
- TPM: tokens per minute (for LLM usage)
- Check rate limits before proxying request

## Location
`/home/mmacedoeu/_w/ai/cipherocto/crates/quota-router-core/src/`

---
**Mission Type:** Implementation
**Priority:** High
**Phase:** RFC-0903 Virtual API Key
