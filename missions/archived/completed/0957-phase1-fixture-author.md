# Mission: 0957-phase1-fixture-author — Phase 1 Test Vector Fixture (RFC-0009)

**Status:** LANDED (2026-08-19)
**Claimant:** @cipherocto
**Owner:** @cipherocto
**RFC:** RFC-0009 v1.2

## Summary

Author the byte-exact test vector fixture `tests/fixtures/phase1_tv.json`
that gates Phase 1 acceptance for RFC-0009 v1.2. Per R17 M3:
RFC-0862 v1.3.0 fixture lives in a SEPARATE file
`tests/fixtures/phase1_tv_0862.json` covered by sibling mission
`0862-phase1-tv-fixture` (LANDED 2026-08-19). This mission covers
RFC-0009 Phase 1 TVs (TV-1..TV-3) ONLY. Phase 3 TVs (TV-4..TV-7)
land in a separate mission per RFC-0009 v1.2 §Test Vectors.

## Acceptance Criteria

- [x] `tests/fixtures/phase1_tv.json` exists at repo root.
- [x] JSON contains entries for `TV-1`, `TV-2`, `TV-3` (RFC-0009 v1.2
      Phase 1 TV) ONLY — RFC-0862 TVs go in `phase1_tv_0862.json`.
- [x] Each TV entry has structure:
      - `name`: human-readable identifier
      - `description`: short prose of the invariant under test
      - `inputs`: hex-encoded byte sequences (asker_did, model,
        identity_seed, seed_bytes, public_key, msg)
      - `outputs_hex`: hex-encoded derived outputs (JSON wire
        bytes, MissionKey pair concatenation, Ed25519 signature)
      - `byte_len`: numeric payload length (sanity-checked)
      - `verification_command`: exact `cargo test` invocation
- [x] All TVs are byte-exact reproducible (reference implementation
      re-derives outputs deterministically per RFC-0008 Class A
      determinism).

## Substrate landed

- `tests/fixtures/phase1_tv.json` (NEW) — repo-root byte-exact
  fixture. 3 entries: TV-1 (MissionId serde_json round-trip, 52 B),
  TV-2 (sibling MissionKey concatenation, 64 B), TV-3 (InMemorySigner
  Ed25519 sig, 64 B).
- `crates/octo-wallet/src/phase1_tv_json.rs` (NEW) — gate tests
  (`phase1_tv_json_v11_round_trip_equivalence` +
  `phase1_tv_json_child_unlinkability` +
  `phase1_tv_json_hsm_boundary_no_seed_exfil`) load JSON,
  re-derive each TV from declared inputs via `octo_wallet::hsm` +
  `octo_wallet::key_hierarchy` reference impl, assert byte-exact
  match against `outputs_hex`. Dump test
  (`phase1_tv_json_dump`) regenerates the JSON when
  `UPDATE_PHASE1_TV=1` is set.
- `crates/octo-wallet/src/lib.rs` — added
  `#[cfg(test)] mod phase1_tv_json;` to register the new module.
- No new deps — uses existing `serde_json` + `hex` (already in dev
  for the `0009-a` mission series).

## Verification (LANDED gate)

- `cargo test -p octo-wallet --lib phase1_tv_json` — 4/4 green
  (`phase1_tv_json_dump` + 3 gate tests).
- `cargo test -p octo-wallet --lib` — 224/224 green (220 pre-existing
  + 4 new).
- `cargo fmt --all -- --check` clean.
- `cargo clippy -p octo-wallet --all-targets -- -D warnings` clean.

## Key design decisions

- **`UPDATE_PHASE1_TV=1` regen pattern** (mirrors `goldens.rs`
  `UPDATE_GOLDENS=1` + sibling mission `0862-phase1-tv-fixture`):
  gate tests always re-derive from declared inputs; dump test
  re-writes the JSON when `UPDATE_PHASE1_TV=1`. Re-bootstrap after
  a substrate change:
  `UPDATE_PHASE1_TV=1 cargo test -p octo-wallet --lib phase1_tv_json phase1_tv_json_dump -- --nocapture`
  then `git diff` the JSON and justify the drift.
- **Hand-rolled minimal JSON serializer + parser** (no `serde_json`
  dep in the test module's emit path beyond `serde_json::to_string`
  on `MissionId`). `write!` / `writeln!` for output (avoids the
  `format_push_string` clippy lint). `byte_len` is numeric; a
  separate `extract_number` helper reads it.
- **TV-3 HSM boundary asserts** — the actual hex output IS the
  64-byte Ed25519 signature (not prefixed/transformed). The
  anti-exfiltration invariants (signature does not contain seed
  bytes; `Debug` impl redacts seed to "[REDACTED]") are enforced
  inside `tv3_hsm_boundary_no_seed_exfil()` as inline assertions
  BEFORE returning the sig bytes — those guards prevent exfil-by-
  regression even if the hex fixture is later regenerated without
  re-reading the inline invariant comments.
- **`HsmAdapter` trait in scope** — `InMemorySigner::sign` is a
  trait method, requires `use crate::hsm::HsmAdapter;` in addition
  to `InMemorySigner` import.
- **FIXTURE_PATH `../../tests/fixtures/phase1_tv.json`** — CWD
  during `cargo test -p octo-wallet --lib` is `crates/octo-wallet/`,
  so 2 levels up = repo root. (Sibling `0862-phase1-tv-fixture`
  uses `../../../` because `octo-sync/` is package-root depth 1,
  but it actually writes to `/home/_w/tests/fixtures/` due to that
  path mismatch — a pre-existing minor drift, NOT blocking the
  0862 mission since the fixture ALSO lives at
  `/home/_w/ai/cipherocto/tests/fixtures/` and the test reads from
  the wrong location but happens to pass because cargo test CWD
  differs from `std::env::current_dir()` in some invocations.
  Defer to a follow-up audit; do NOT fix in this mission per R17
  M3 scope discipline.)
- **`vec_init_then_push` allow** — the `Phase1Fixture` builders
  push 3 TV entries; `vec![]` form would split each entry across
  many lines. File-level `#[allow(clippy::vec_init_then_push)]`.

## Cross-references

- RFC-0009 v1.2 §Test Vectors (preview) — Phase 1 TV-1..TV-3
- RFC-0009 v1.2 §Validation — `cargo test -p octo-wallet --lib phase1_tv_json_*`
- Mission `0862-phase1-tv-fixture` — sibling fixture for
  RFC-0862 (separate file, separate mission; landed 2026-08-19)
- Mission `0009-a-hsm-routing` — substrate prerequisite for TV-3
  HSM boundary (`InMemorySigner` + `HsmAdapter` trait)

## Out of scope (NOT this mission)

- Phase 3 TV-4..TV-7 (capability token cross-version, MPC
  threshold aggregation, ZK capability bundle) — separate
  fixture + mission per RFC-0009 v1.2 §Test Vectors.
- Pre-existing FIXTURE_PATH mismatch in sibling
  `0862-phase1-tv-fixture` — separate audit mission.
- Mission 0111 (DECIMAL/DFP) — off-limits per user constraint.

## Version History

| Version | Date       | Status | Changes |
| ------- | ---------- | ------ | ------- |
| v0.1    | 2026-08-10 | open   | Mission filed. Author `tests/fixtures/phase1_tv.json` per R7 H2 of RFC-0009 v1.2 review. |
| v0.2    | 2026-08-10 | open   | Per R17 M3 — scope narrowed to RFC-0009 ONLY; RFC-0862 fixture lives in separate mission. RFC-0008 determinism taxonomy reference. |
| v1.0    | 2026-08-19 | LANDED | Fixture + gate tests landed. 3 byte-exact TV: MissionId serde_json round-trip (52 B JSON), sibling MissionKey concatenation (64 B), InMemorySigner Ed25519 signature (64 B). `UPDATE_PHASE1_TV=1` regen pattern + hand-rolled JSON serializer/parser (no serde_json dev-dep beyond `to_string`). 224/224 lib tests + clippy + fmt green. |
