# Mission: 0948-b — Prompt API Endpoints

## Status

Closed 2026-08-13 (@claude). LANDED + drift-closed.

**Substrate (from prior sessions):** 11/12 ACs PASS — code shipped in
commit `9ae79d1d` ("feat(0948-b): fix prompt endpoints compilation and
test issues"). End-to-end wiring verified by 0948-c-prompt-integration
tests (commit `2b45a949`).

**Drift closure:** 1 AC DEFERRED — rate limiting on CRUD endpoints
(see Follow-ons below). Drift pattern matches 0947-c / 0947-b.

## RFC

RFC-0948 (Economics): Prompt Management

## Dependencies

- Mission-0948-a: Prompt Registry (commit `e983bd0b`)

## Acceptance Criteria

- [x] Implement `POST /prompts` — create prompt (admin.rs:722)
- [x] Implement `GET /prompts` — list prompts (with PromptFilter, pagination) (admin.rs:745)
- [x] Implement `GET /prompts/:id` — get prompt with active version (admin.rs:852)
- [x] Implement `PUT /prompts/:id` — update prompt (admin.rs:860)
- [x] Implement `DELETE /prompts/:id` — delete prompt (admin.rs:898)
- [x] Implement `GET /prompts/:id/versions` — list versions (sorted by creation order) (admin.rs:759)
- [x] Implement `POST /prompts/:id/versions` — create version (admin.rs:798)
- [x] Implement `POST /prompts/:id/rollback` — rollback to version (admin.rs:769)
- [x] Implement `POST /prompts/:id/versions/:v/activate` — activate version (admin.rs:839)
- [ ] **DEFERRED** Rate limiting on CRUD endpoints (per RFC-0933) — see follow-on
- [x] Clippy passes with zero warnings (verified by 0948-c PR)
- [x] All existing tests pass (verified by 0948-c PR: 1571/1571 lib)

## Claimant

(unclaimed)

## Pull Request

#

## Notes

**Drift pattern** — code landed in commit `9ae79d1d` (compilation/test
fixes for prompt endpoints) + `e983bd0b` (mission `0948-a` prompt
registry + `0947-a` callback), but mission file remained `open/`. The
endpoints were already exercised by 0948-c integration tests at
commit `2b45a949` ("atomic A/B metrics + priority chain + SDK parity").

**Rate limiting deferred** — admin.rs has no rate limiting on *any*
endpoint (auth/sso, prompts, providers CRUD). The rate-limiting
substrate exists at `crates/quota-router-core/src/rate_limit/` +
`key_rate_limiter.rs` but is wired only at the proxy layer (chat
completion request path), not at the admin CRUD layer. This is a
cross-mission gap, not a 0948-b-specific miss. Filed as follow-on
mission `0948-b1-admin-rate-limiting` to scope the work.

## Follow-ons

- `0948-b1-admin-rate-limiting` — apply quota-router-core rate_limit
  middleware to all admin.rs CRUD endpoints (auth/sso, prompts,
  providers). RFC-0933 wiring. Per-endpoint policy (10/min login, 20/min
  callback, 30/min refresh/revoke, 60/min CRUD).

## Cross-references

- RFC-0948 (Economics): Prompt Management
- Mission `0948-a` (commit `e983bd0b`) — registry substrate
- Mission `0948-c` (commit `2b45a949`) — integration + closure witness
- Mission `0949-b` (file `0949-b-sso-oauth2-oidc.md`) — also DEFERRED
  rate limiting; will close under `0948-b1`.

## Version History

| Version | Date       | Status   | Changes |
| ------- | ---------- | -------- | ------- |
| v0.1    | 2026-07-23 | claimed  | Original mission |
| v0.2    | 2026-08-13 | closed   | 11/12 ACs PASS; 1 DEFERRED (rate limiting). Follow-on `0948-b1` filed. |
