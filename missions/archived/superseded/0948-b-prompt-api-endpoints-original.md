# Mission: 0948-b — Prompt API Endpoints

> **Superseded.** This is the original unclaimed stub, retained for history.
> The worked mission closed at `missions/archived/completed/0948-b-prompt-api-endpoints.md`
> (Closed 2026-08-13, 11 of 12 ACs green, rate limiting deferred).

## Status

Superseded — see completed mission.

## RFC

RFC-0948 (Economics): Prompt Management

## Dependencies

- Mission-0948-a: Prompt Registry

## Acceptance Criteria

- [ ] Implement `POST /prompts` — create prompt
- [ ] Implement `GET /prompts` — list prompts (with PromptFilter, pagination)
- [ ] Implement `GET /prompts/:id` — get prompt with active version
- [ ] Implement `PUT /prompts/:id` — update prompt
- [ ] Implement `DELETE /prompts/:id` — delete prompt
- [ ] Implement `GET /prompts/:id/versions` — list versions (sorted by creation order)
- [ ] Implement `POST /prompts/:id/versions` — create version
- [ ] Implement `POST /prompts/:id/rollback` — rollback to version
- [ ] Implement `POST /prompts/:id/versions/:v/activate` — activate version
- [ ] Rate limiting on CRUD endpoints (per RFC-0933)
- [ ] Clippy passes with zero warnings
- [ ] All existing tests pass

## Claimant

(unclaimed)

## Pull Request

#

## Notes

Key files:

- `crates/quota-router-core/src/admin.rs` — Add prompt CRUD endpoints
- `crates/quota-router-core/src/prompts/mod.rs` — Registry methods
