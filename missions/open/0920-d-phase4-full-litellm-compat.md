# Mission: RFC-0920 Phase 4 — Full LiteLLM Compatibility

## Status

Open — depends on Phase 3

## RFC

RFC-0920 (Economics): Unified Python SDK — Dual-Mode LiteLLM/any-llm Compatibility (Accepted v1.58)

## Dependencies

- Mission: 0920-c-phase3-enterprise-features (Phase 3 must complete first)

## Context

Phase 4 covers remaining parameters marked Phase 4 in RFC-0920 (lines 3735-3751). **Note:** modalities, audio, prediction were moved to Phase 3 per RFC-0920 version history line 4716. **Critical:** web_search_options, shared_session, enable_json_schema_validation are already specced in completion signatures — NOT Phase 4 pending (RFC-0920 Phase 4 table is stale at lines 3746, 3750, 3751).

## Phase 4 Checklist (RFC-0920 lines 4641-4645)

**F1/F2 CRITICAL:** Mission previously claimed web_search_options, shared_session, enable_json_schema_validation as Phase 4 per RFC-0920 lines 1153-1155. **These are WRONG** — RFC-0920 Phase 4 table (lines 3746, 3750, 3751) is **stale/inconsistent** with actual signatures:
- `web_search_options` — already specced at lines 416, 1331
- `shared_session` — already specced at lines 415, 1330
- `enable_json_schema_validation` — already specced at lines 417, 1332

**Actual Phase 4 items** (per RFC-0920 lines 3735-3751, excluding already-specced):

- [ ] **`truncation` parameter** — Phase 4 per RFC-0920 line 3735, "not yet specced" at line 1159 **F6 fix: added**
- [ ] **`top_k` parameter** — Phase 4 per line 3735
- [ ] **`service_tier` parameter** — Phase 4 per line 3735
- [ ] **`background` parameter** — Phase 4 per line 3735
- [ ] **`prompt_cache_key` parameter** — Phase 4 per line 3735
- [ ] **`prompt_cache_retention` parameter** — Phase 4 per line 3735
- [ ] **`conversation` parameter** — Phase 4 per line 3735
- [ ] LiteLLM test suite compatibility — `pytest tests/test_litellm_compat.py -v` (target: 100% API surface coverage)

**F4 fix:** F3 (SSE normalization) is already Phase 3 done (lines 2088-2154) — removed from Future Work.

**Phase 4 scope clarification:** RFC-0920 lines 4641-4645 claim modalities/audio/prediction + 8 routing strategies, but:
- modalities/audio/prediction are Phase 3 per lines 3747-3749
- 8 routing strategies are Phase 3 per Router class
- Actual Phase 4 pending items are the 7 truncation/top_k/etc items above

## Future Work Items (RFC-0920 lines 4657-4662)

These are NOT Phase 4 but represent future integration:

- [ ] **F1: LangChain integration** — LCEL compatibility
- [ ] **F2: LlamaIndex integration** — callback-based
- [ ] **F4: Response caching** — RFC-0913 **D6 FIX: was RFC-0906 which is wrong reference**
  - **K3 fix:** Note — `cache_responses` is Phase 3 (stoolap semantic cache). F4 "Response caching" may refer to a different feature (e.g., HTTP-level caching) or be stale.

**F4 note:** F3 (Streaming SSE normalization) is **already Phase 3** per RFC-0920 lines 2088-2154 — removed from Future Work.

## Acceptance Criteria

**Note:** F1/F2 fix — web_search_options/shared_session/enable_json_schema_validation are already specced, NOT Phase 4. Updated acceptance criteria to reflect actual Phase 4 scope:

- [ ] `truncation` param in messages() — per RFC-0920 line 3735/1159
- [ ] `top_k` param in messages() — per RFC-0920 line 3735
- [ ] `service_tier` param — per RFC-0920 line 3735
- [ ] `background` param — per RFC-0920 line 3735
- [ ] `prompt_cache_key` param — per RFC-0920 line 3735
- [ ] `prompt_cache_retention` param — per RFC-0920 line 3735
- [ ] `conversation` param — per RFC-0920 line 3735
- [ ] LiteLLM test suite compatibility — `pytest tests/test_litellm_compat.py -v` (target: 100% API surface coverage) — per RFC-0920 lines 4455-4464
- [ ] `cargo clippy -D warnings` passes
- [ ] `cargo test --lib` passes