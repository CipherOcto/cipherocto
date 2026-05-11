# Mission: RFC-0920 Phase 4 — Full LiteLLM Compatibility

## Status

In Review — 2026-05-10

## RFC

RFC-0920 (Economics): Unified Python SDK — Dual-Mode LiteLLM/any-llm Compatibility (Accepted v1.70)

**Note:** Phase 4 was removed from RFC-0920 in v1.66 (2026-05-09) per deferred-vs-unspecified rule. All items previously in Phase 4 are either:
- Moved to Phase 3 (if specced)
- Removed as unspecced (if no spec existed)

This mission tracks **Phase 4 parameters that remain specced** in RFC-0920 lines 3735-3751.

## Dependencies

- Mission: 0920-c-phase3-enterprise-features (Phase 3 completed)

---

## Phase 4 Parameters (per RFC-0920 lines 3735-3751)

These parameters appear in completion/messages signatures per RFC-0920 spec:

- [x] **`truncation` parameter** — Phase 4 per RFC-0920 line 3735
- [x] **`top_k` parameter** — Phase 4 per line 3735
- [x] **`service_tier` parameter** — Phase 4 per line 3735
- [x] **`background` parameter** — Phase 4 per line 3735
- [x] **`prompt_cache_key` parameter** — Phase 4 per line 3735
- [x] **`prompt_cache_retention` parameter** — Phase 4 per line 3735
- [x] **`conversation` parameter** — Phase 4 per line 3735

### Items Already Specced (NOT Phase 4 pending)

These are already in completion signatures per RFC-0920:
- `web_search_options` — already specced at lines 416, 1331
- `shared_session` — already specced at lines 415, 1330
- `enable_json_schema_validation` — already specced at lines 417, 1332

---

## Future Work

- [ ] **LangChain integration** — LCEL compatibility
- [ ] **LlamaIndex integration** — callback-based
- [ ] **Response caching** — RFC-0913 via stoolap

---

## Acceptance Criteria

- [x] `truncation` param in messages() — per RFC-0920 line 3735/1159
- [x] `top_k` param in messages() — per RFC-0920 line 3735
- [x] `service_tier` param — per RFC-0920 line 3735
- [x] `background` param — per RFC-0920 line 3735
- [x] `prompt_cache_key` param — per RFC-0920 line 3735
- [x] `prompt_cache_retention` param — per RFC-0920 line 3735
- [x] `conversation` param — per RFC-0920 line 3735
- [x] `cargo clippy -D warnings` passes
- [x] `cargo test --lib` passes