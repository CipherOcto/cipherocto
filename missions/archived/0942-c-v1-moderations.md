# Mission: 0942-c — /v1/moderations Endpoint

## Status

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

LiteLLM supports the /v1/moderations endpoint for content moderation. This mission adds path routing.

## Acceptance Criteria

- [ ] Add /v1/moderations path routing in handle_request
- [ ] Forward to provider's moderations API
- [ ] Return OpenAI-compatible response format

## Files to Modify

- `crates/quota-router-core/src/proxy.rs` — add path routing
