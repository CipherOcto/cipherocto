# Mission: Key Management Routes

## Status
Pending

## RFC
RFC-0903: Virtual API Key System

## Summary
Add key management HTTP routes to quota-router-cli: create, revoke, rotate, update, list.

## Acceptance Criteria
- [ ] POST /api/keys - create new key
- [ ] GET /api/keys - list keys (with filters)
- [ ] PUT /api/keys/:id - update key
- [ ] POST /api/keys/:id/revoke - revoke key
- [ ] POST /api/keys/:id/rotate - rotate key
- [ ] DELETE /api/keys/:id - delete key
- [ ] Integration tests for all routes

## Complexity
Medium

## Prerequisites
- Mission 0903-a (Key Core) - key CRUD
- Mission 0903-b (Team Management) - team assignment
- Mission 0903-c (Auth Middleware) - route protection

## Implementation Notes
- Routes protected by auth middleware
- Key operations emit invalidation events
- Admin routes require MANAGEMENT key type

## Location
`/home/mmacedoeu/_w/ai/cipherocto/crates/quota-router-cli/src/`

---
**Mission Type:** Implementation
**Priority:** High
**Phase:** RFC-0903 Virtual API Key
