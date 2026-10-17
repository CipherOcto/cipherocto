# Mission: 0942-b — Passthrough Endpoints

## Status

Complete

Open

## RFC

RFC-0942 (Economics): Additional API Endpoints

> **Numbering note.** RFC-0942 was never authored. Git history contains no
> RFC file with this number under any category, so this mission was filed
> against a specification that does not exist. The closest current RFC by title is RFC-0951 (Economics): Extended API Endpoints at `rfcs/accepted/economics/0951-extended-api-endpoints.md`, which this
> mission did not cite and which is not recorded as a renumbering of
> RFC-0942. Treat the governing specification as unconfirmed and
> reconcile it against RFC-0951 (Economics): Extended API Endpoints before relying on it.

## Context

LiteLLM allows direct provider access for unsupported APIs via passthrough. This mission adds `/{provider}/...` catch-all routing.

## Acceptance Criteria

- [x] Add `/{provider}/...` path routing in handle_request
- [x] Forward request directly to provider API base
- [x] No transformation — raw passthrough
- [x] Support Authorization header forwarding

## Files to Modify

- `crates/quota-router-core/src/proxy.rs` — add passthrough routing

## Verification

```bash
cargo test -p quota-router-core --lib
cargo clippy -p quota-router-core -- -D warnings
```
