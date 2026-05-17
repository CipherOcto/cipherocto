# Mission: 0907-a — Config Hot-Reload

## Status

Open

## RFC

RFC-0907 (Economics): Configuration Management

## Context

LiteLLM has /config/update and /config/get endpoints for hot-reloading configuration.

## Acceptance Criteria

- [ ] Add GET /config/get — return current configuration
- [ ] Add POST /config/update — hot-reload configuration
- [ ] Validate new config before applying
- [ ] Return old and new config on update

## Files to Modify

- `crates/quota-router-core/src/admin.rs` — add config endpoints
