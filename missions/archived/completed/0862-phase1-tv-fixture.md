# Mission: 0862-phase1-tv-fixture — Phase 1 Test Vector Fixture (RFC-0862)

**Status:** LANDED (2026-08-19)
**Claimant:** @cipherocto
**Owner:** @cipherocto
**RFC:** RFC-0862 v1.3.0 (Draft 2026-08-10)

## Summary

Author the byte-exact test vector fixture
`tests/fixtures/phase1_tv_0862.json` that gates Phase 1 acceptance
for RFC-0862 v1.3.0. Per R17 M3: separate file from RFC-0009's
`phase1_tv.json` (each RFC owns its own fixture file). Mission
covers RFC-0862 Phase 1 TVs (TV-1..TV-4: HLC monotonicity + HLC
logical increment + WriterIdentity caching + bootstrap peer
acquisition). Phase 3 TVs (TV-5..TV-8: election latency, drain
throughput, failover pause, WAL fan-out lag) land in a separate
Phase 3 mission per RFC-0862 §Test Vectors.

## Acceptance Criteria

- [x] `tests/fixtures/phase1_tv_0862.json` exists at repo root.
- [x] JSON contains entries for `TV-1`, `TV-2`, `TV-3`, `TV-4`
      (RFC-0862 v1.3.0 Phase 1 TV) ONLY.
- [x] Each TV entry has structure:
      - `name`: human-readable identifier
      - `description`: short prose of the invariant under test
      - `inputs`: hex-encoded byte sequences (WriterNodeId,
        ShardMissionId, ShardKey, ChainId, HLC observed
        physical_ms, bootstrap peer identities, etc.)
      - `outputs_hex`: hex-encoded derived outputs (HlcTimestamp
        borsh bytes, WriterIdentity borsh bytes, BLAKE3
        fingerprint of acquired peer list, etc.)
      - `byte_len`: sanity-checked length of the hex-decoded
        output payload
      - `verification_command`: exact `cargo test` invocation
        that reproduces the expected output
- [x] All TVs are byte-exact reproducible (reference implementation
      re-derives outputs deterministically per RFC-0008 Class A
      determinism).

## Substrate landed

- `tests/fixtures/phase1_tv_0862.json` (NEW) — repo-root byte-exact
  fixture. 4 entries: TV-1 (HLC monotonicity, 132 bytes), TV-2
  (HLC logical increment, 132 bytes), TV-3 (WriterIdentity
  caching, 148 bytes), TV-4 (bootstrap peer acquisition, 32-byte
  BLAKE3 fingerprint).
- `octo-sync/tests/phase1_tv_0862.rs` (NEW) — gate test
  (`phase1_tv_0862_match`) loads JSON, re-derives each TV from
  declared inputs via `octo_sync::substrate` reference impl, asserts
  byte-exact match against `outputs_hex`. Dump test
  (`phase1_tv_0862_dump`) regenerates the JSON when
  `UPDATE_PHASE1_TV=1` is set.
- `octo-sync/Cargo.toml` — added `hex = "0.4"` to `[dev-dependencies]`
  (gate test uses `hex::encode` / `hex::decode` for fixture
  round-trip).

## Verification (LANDED gate)

- `cargo test -p octo-sync --test phase1_tv_0862` — 2/2 green
  (`phase1_tv_0862_dump` + `phase1_tv_0862_match`).
- `cargo test -p octo-sync --tests` — all green (lib + integration
  tests across `cross_instance_drain_tv`, `cross_instance_tv`,
  `governance_relinquish_tv`, `property_tests`, `phase1_tv_0862`).
- `cargo fmt --all -- --check` clean.
- `cargo clippy -p octo-sync --all-targets -- -D warnings` clean.

## Key design decisions

- **`UPDATE_PHASE1_TV=1` regen pattern** (mirrors quota-router-core
  `goldens.rs` `UPDATE_GOLDENS=1`): the gate test always re-derives
  from declared inputs. The dump test re-writes the JSON when
  `UPDATE_PHASE1_TV=1`. Re-bootstrap after a substrate change:
  `UPDATE_PHASE1_TV=1 cargo test -p octo-sync --test phase1_tv_0862 phase1_tv_0862_dump -- --nocapture`
  then `git diff` the JSON and justify the drift.
- **Hand-rolled minimal JSON parser** (no `serde_json` dev-dep) —
  the fixture is shallow (entries / inputs / outputs_hex / byte_len
  / verification_command) so a 60-line recursive parser suffices.
  `byte_len` is numeric (not string); a separate `extract_number`
  helper handles that single non-string field.
- **TV-4 BLAKE3 fingerprint** — `PeerIdentity` does NOT derive
  `BorshSerialize` (only the substrate newtypes do, per Layer A
  contract). The "acquired peer list" wire form is canonicalized
  via BLAKE3 of `(node_id || overlay_id || mission_id)` triples
  in peer-return order. Downstream consumers hash-verify against
  this 32-byte fingerprint.
- **`vec_init_then_push` allow** — the `Phase1Fixture` builders
  push 4 TV entries; `vec![]` form would split each entry across
  many lines. File-level `#[allow(clippy::vec_init_then_push)]`
  with rationale comment.

## Cross-references

- RFC-0862 v1.3.0 §Test Vectors (preview) — Phase 1 TV-1..TV-4
- RFC-0862 v1.3.0 §Performance Targets — full TV list
  (TV-1..TV-8; Phase 3 TVs covered by separate mission)
- Mission `0957-phase1-fixture-author` — sibling fixture for
  RFC-0009 (separate file, separate mission)

## Version History

| Version | Date       | Status   | Changes |
| ------- | ---------- | -------- | ------- |
| v0.1    | 2026-08-10 | open     | Mission filed per R17 M3 — RFC-0862 Phase 1 fixture scope (phase1_tv_0862.json; 4 TVs). Phase 3 TVs deferred to separate mission. |
| v1.0    | 2026-08-19 | LANDED   | Fixture + gate test + dump test landed. 4 byte-exact TV: HLC monotonicity (132 B), HLC logical increment (132 B), WriterIdentity caching (148 B), bootstrap peer acquisition (32 B BLAKE3). `UPDATE_PHASE1_TV=1` regen pattern + hand-rolled JSON parser (no serde_json dev-dep). |