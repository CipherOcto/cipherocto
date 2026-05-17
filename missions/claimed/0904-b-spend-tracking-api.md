# Mission: 0904-b — Spend Tracking API

## Status

Open

## RFC

RFC-0904 (Economics): Real-Time Cost Tracking

## Context

LiteLLM has /spend/logs, /global/spend, /spend/calculate endpoints. This mission adds spend query API endpoints.

## Acceptance Criteria

- [ ] Add GET /spend/logs — query spend logs with filters
- [ ] Add GET /global/spend — aggregate spend across all keys
- [ ] Add POST /spend/calculate — estimate cost for a request
- [ ] All endpoints require Management key type

## Files to Modify

- `crates/quota-router-core/src/admin.rs` — add spend endpoints
