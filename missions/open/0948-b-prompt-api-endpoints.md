# Mission: 0948-b — Prompt API Endpoints

## Status

Open

## RFC

RFC-0948 (Economics): Prompt Management

## Dependencies

- Mission-0948-a: Prompt Registry

## Acceptance Criteria

- [ ] Add `prompt_id: Option<String>` and `prompt_variables: Option<HashMap<String, String>>` to `ChatCompletionRequest`
- [ ] Implement `POST /prompts` — create prompt
- [ ] Implement `GET /prompts` — list prompts (with PromptFilter, pagination)
- [ ] Implement `GET /prompts/:id` — get prompt with active version
- [ ] Implement `PUT /prompts/:id` — update prompt
- [ ] Implement `DELETE /prompts/:id` — delete prompt
- [ ] Implement `GET /prompts/:id/versions` — list versions (sorted by creation order)
- [ ] Implement `POST /prompts/:id/versions` — create version
- [ ] Rate limiting on CRUD endpoints
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
