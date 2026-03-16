# Mission: Virtual API Key Core

## Status
Pending

## RFC
RFC-0903: Virtual API Key System

## Summary
Implement core key management in quota-router-core: key generation, validation, storage schema, and basic CRUD operations.

## Acceptance Criteria
- [ ] Create keys.rs with key generation (UUID, sk-qr- prefix)
- [ ] Create storage.rs with stoolap table definitions for api_keys
- [ ] Implement key validation middleware
- [ ] Implement key creation API
- [ ] Implement key lookup by hash
- [ ] Unit tests for key generation and validation

## Complexity
High

## Prerequisites
- RFC-0913 (WAL Pub/Sub) - for cache invalidation events
- Mission 0913-c (Cache Integration) - for key cache invalidation

## Implementation Notes
- Key format: sk-qr-{uuid} for LiteLLM compatibility
- Store key_hash (SHA-256) not plaintext
- Use deterministic serialization (RFC-0126)

## Location
`/home/mmacedoeu/_w/ai/cipherocto/crates/quota-router-core/src/`

---
**Mission Type:** Implementation
**Priority:** High
**Phase:** RFC-0903 Virtual API Key
