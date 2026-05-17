# Mission: 0948-a — Prompt Registry

## Status

Open

## RFC

RFC-0948 (Economics): Prompt Management

## Dependencies

None

## Acceptance Criteria

- [ ] Define `PromptTemplate` struct (id, name, team_id, tags, default_version_id, created_at, created_by)
- [ ] Define `PromptVersion` struct (id, prompt_id, template, variables, defaults, created_at, created_by)
- [ ] Define `PromptFilter` struct (team_id, tags, name, limit, offset)
- [ ] Implement `PromptRegistry` with stoolap-backed storage (RFC-0914 Required)
- [ ] Implement `TemplateEngine` with variable substitution (`{{var}}` syntax)
- [ ] TemplateEngine filters: Default, Truncate, Upper, Lower, Strip
- [ ] Single-pass rendering (values containing `{{` rendered literally)
- [ ] Implement LRU prompt caching
- [ ] Concurrent access via `Arc<RwLock<PromptRegistry>>`
- [ ] Add `PromptConfig` to `config.rs` (storage, cache_size)
- [ ] Error handling: PromptNotFound(404), PromptVersionNotFound(404), TemplateRenderError(400), VariableMissing(400), AbTestNotFound(404), AbTestEnded(410), StorageError(500), PermissionDenied(403)
- [ ] Clippy passes with zero warnings
- [ ] All existing tests pass

## Claimant

(unclaimed)

## Pull Request

#

## Notes

Key files:
- `crates/quota-router-core/src/prompts/mod.rs` — New
- `crates/quota-router-core/src/prompts/storage.rs` — New
- `crates/quota-router-core/src/prompts/template.rs` — New
- `crates/quota-router-core/src/config.rs` — Add PromptConfig
