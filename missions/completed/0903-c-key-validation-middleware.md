# Mission: Auth Middleware

## Status
Pending

## RFC
RFC-0903: Virtual API Key System

## Summary
Implement authentication middleware: API key extraction, validation, request routing based on key permissions.

## Acceptance Criteria
- [ ] Create auth.rs with ApiKeyAuth middleware
- [ ] Extract API key from Authorization header
- [ ] Validate key (exists, not expired, not revoked)
- [ ] Route requests based on key type (LLM_API, MANAGEMENT, READ_ONLY)
- [ ] Implement route normalization for security
- [ ] Unit tests for auth middleware

## Complexity
Medium

## Prerequisites
- Mission 0903-a (Key Core) - need key validation

## Implementation Notes
- Key types: LLM_API, MANAGEMENT, READ_ONLY, DEFAULT
- Authorization header format: Bearer {key}
- Reject path traversal (e.g., /v1/chat/../admin)

## Location
`/home/mmacedoeu/_w/ai/cipherocto/crates/quota-router-core/src/`

---
**Mission Type:** Implementation
**Priority:** High
**Phase:** RFC-0903 Virtual API Key
