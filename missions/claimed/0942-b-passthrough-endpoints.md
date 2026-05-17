# Mission: 0942-b — Passthrough Endpoints

## Status

Open

## RFC

RFC-0942 (Economics): Additional API Endpoints

## Context

LiteLLM allows direct provider access for unsupported APIs via passthrough. This mission adds `/{provider}/...` catch-all routing.

## Acceptance Criteria

- [ ] Add `/{provider}/...` path routing in handle_request
- [ ] Forward request directly to provider API base
- [ ] No transformation — raw passthrough
- [ ] Support Authorization header forwarding

## Files to Modify

- `crates/quota-router-core/src/proxy.rs` — add passthrough routing

## Verification

```bash
cargo test -p quota-router-core --lib
cargo clippy -p quota-router-core -- -D warnings
```
