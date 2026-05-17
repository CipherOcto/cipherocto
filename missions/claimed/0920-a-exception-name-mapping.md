# Mission: 0920-a — Exception Name Mapping

## Status

Open

## RFC

RFC-0920 (Economics): Unified Python SDK Dual-Mode Compatibility

## Context

The Python SDK exceptions need to map to both quota-router names and LiteLLM-compatible names.

## Acceptance Criteria

- [ ] Map quota-router exceptions to HTTP status codes
- [ ] Map LiteLLM exception names to quota-router exceptions
- [ ] Ensure Python SDK raises correct exception types

## Files to Modify

- `crates/quota-router-pyo3/src/exceptions.rs` — add exception mapping
