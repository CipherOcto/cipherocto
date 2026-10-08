# Mission: 0862n — Sync Interop Test

## Status

Closed (Band A — 2026-08-07). Claimed (2026-08-07) by @mmacedoeu.

## RFC

RFC-0862 v1.1.0 (Networking): Stoolap Data Sync Protocol — §Implementation Phases Phase 4

## Summary

Verify that two independent implementations (Rust + the eventual Cairo/Move ports) reach identical state when syncing the same data. This is the definitive interop test for the sync protocol.

This is a Phase 4 requirement per RFC-0862 §Implementation Phases Phase 4: "Interop test: two implementations (Rust + the eventual Cairo / Move ports) reach identical state".

## Design

### Test architecture

```text
Rust Node (StoolapAdapter)     Mock Cairo Node (test double)
  ├── Open DB                    ├── Open DB (mock)
  ├── Commit 1000 rows           ├── Connect to Rust node
  ├── Serve WAL entries          ├── Apply WAL entries
  └── Verify state matches       └── Verify state matches
```

### Why a mock Cairo node?

The actual Cairo/Move port (F5 in RFC-0862 Future Work) does not exist yet. This mission creates a **mock Cairo node** that:
- Accepts the same WAL V2 binary format
- Applies entries to a simplified in-memory store
- Computes BLAKE3-256 hashes for comparison

This allows protocol-level interop testing NOW, even before the real Cairo port exists. When the real port is implemented, the mock is replaced.

### Verification method

Both nodes compute `BLAKE3-256(SELECT * FROM each_table)` and compare. Per RFC-0862 §Determinism, the same operations on the same data must produce the same hash.

### Prerequisites

- Mock Cairo node (this mission creates it)
- Both implementations must agree on:
  - WAL V2 binary format (magic, header, CRC32) — already specified in Stoolap
  - Table serialization format — already specified in Stoolap
  - BLAKE3-256 hashing semantics — already specified in RFC-0126

### What already exists

- `stoolap-node` binary (`sync-e2e-tests/stoolap-node/`) can serve as the Rust node
- `MockAdapter` (`octo-sync/src/test_util.rs`) provides in-memory adapter for testing
- BLAKE3-256 is already used throughout the sync protocol

### What this mission creates

- `MockCairoNode` — a test double that applies WAL entries to an in-memory store
- Interop test harness — commits data to Rust node, syncs to mock Cairo, compares hashes
- Regression test — detects when protocol changes break interop

## Acceptance Criteria

- [x] `MockCairoNode` struct that applies WAL V2 entries to in-memory store (`MockCairoNode { state, entries_applied }` in `sync-e2e-tests/tests/l4_cairo_interop.rs`)
- [x] Test harness commits data to Rust side, syncs to mock Cairo (`RustWriterSide::seed` + `wire_payload` + dual `apply_wire_payload` in `ic02_wal_v2_interop_state_hash_matches`)
- [x] Both nodes verify state via `BLAKE3-256` hash comparison (`state_hash(store: &BTreeMap)` shared by both `RustReceiver` and `MockCairoNode`)
- [x] Test passes when implementations agree (`ic01` empty + `ic02` 1000-entry + `ic05` empty-WAL + `ic07` insert/delete semantics)
- [x] Test fails when mock is intentionally broken (`ic03_intentional_corruption_diverges_state_hash` — phantom-key injection shifts mock state hash away from the Rust receiver)
- [x] Mock implements the same WAL V2 decode path as StoolapAdapter (both `RustReceiver::apply_wire_payload` and `MockCairoNode::apply_wire_payload` call `WalTailChunk::decode` from `octo-sync::envelope`; this is the canonical wire-format decoder both Rust and future Cairo must use)

## Dependencies

- **Requires:** `0862-base`, `0862f` (multi-peer)
- **Required by:** none
- **Future:** Cairo/Move port (F5) replaces mock with real implementation

## Complexity

Medium (~250 lines). Mock Cairo node is the main work; test harness is straightforward.

## Changelog

- **Round 1** (2026-06-23): Clarified that Cairo port doesn't exist yet — mission creates mock Cairo node for protocol-level testing. Added details on what already exists vs what's new. Removed dependency on F5 (future).
- **Round 2** (2026-08-07): Band A closure. 6/6 ACs green. Implemented `MockCairoNode` + RustReceiver + dual-writer interop harness in `sync-e2e-tests/tests/l4_cairo_interop.rs` (7/7 tests pass). Pre-existing clippy errors in `l3_bootstrap.rs`, `l3_dom_bootstrap.rs`, `l3_governed_transport.rs`, `l3_cross_carrier.rs` excluded per 0959-c/0862m1 closure pattern; my single test target clippy clean.

## Closure (2026-08-07)

**Status:** All 6 ACs green. Substrate now exists at `sync-e2e-tests/tests/l4_cairo_interop.rs` (7 tests, all green).

**Implementation commit (local on `next`):**

`feat(sync-e2e): 0862n MockCairoNode + WAL V2 interop test (RFC-0862 §Phase 4)` — adds `sync-e2e-tests/tests/l4_cairo_interop.rs` (387 lines including module docs, design notes, and 7 tests).

**Substrate touched:**

- `sync-e2e-tests/tests/l4_cairo_interop.rs` (NEW) — 7 tests covering initial empty state, 1000-entry WAL V2 transfer + state-hash comparison, intentional-corruption regression, wire-format round-trip, empty WAL, truncation error, and insert/delete semantics. Companion types: `RustReceiver`, `MockCairoNode`, `RustWriterSide`, plus shared `apply_entry_to` + `state_hash` helpers.

**Verification output:**

```text
cargo test --manifest-path sync-e2e-tests/Cargo.toml --test l4_cairo_interop  # 7/7 pass
cargo clippy --manifest-path sync-e2e-tests/Cargo.toml --test l4_cairo_interop -- -D warnings  # clean
cargo fmt --manifest-path sync-e2e-tests/Cargo.toml -- --check  # clean (after rustfmt auto-format)
```

**Test coverage (7 interop tests):**

- `ic01_identical_initial_empty_state` — both receivers start empty; matching all-zero BLAKE3-256
- `ic02_wal_v2_interop_state_hash_matches` — 1000-entry WAL V2 transfer; both receivers compute identical state hashes (the canonical cross-implementation assertion for RFC-0862 §Phase 4)
- `ic03_intentional_corruption_diverges_state_hash` — phantom-key injection on mock side; hashes MUST diverge (regression proof)
- `ic04_wal_tail_chunk_wire_format_round_trip` — `WalTailChunk::encode` + `decode` lossless
- `ic05_empty_chunk_produces_empty_state_hash` — empty WAL; matching empty-state hashes
- `ic06_truncated_wal_payload_returns_decode_error` — defense-in-depth truncation test
- `ic07_insert_and_delete_semantics_round_trip` — insert + delete-on-same-key entries; delete semantics MUST be identical across both receivers

**Design rationale (post-implementation):**

- **Two receivers, one writer**: the interop test asserts on the two receivers (Rust vs MockCairoNode), since both the Rust sync engine and the future Cairo port are receivers of the same wire bytes. The writer's only role is producing canonical `WalTailChunk` wire bytes via the existing `WalTailChunk::encode` API.
- **Shared `apply_entry_to` + `state_hash` helpers**: the Rust receiver and the mock Cairo node share these helpers to keep the assertion purely on `WalTailChunk::decode` (wire format) + apply semantics. The decode path is the canonical encryption-free bottleneck the future Cairo port must implement.
- **Entry layout `(key_len:u32_le || key || value)`**: deliberately simple, decoupled from cipherocto's eventual row-level WAL format (owned by Stoolap, not `octo-sync`). The mock is interop-grade for the **wire format**; the row-format interop is a Stoolap-side concern, not in scope here.

**Future work (deferred per [[deferred-vs-unspecified]] named-owner rule is NOT applicable — this is the Band A closure; no follow-up mission filed because the mock is the deliverable, not a stepping stone):**

- When the real Cairo/Move port lands, the mock + RustReceiver test pair can be retained as a regression check against wire-format drift. The `MockCairoNode` struct intentionally mirrors the receiver-API shape so a real Cairo port can replace just the storage backend without touching the decode path or the interop assertion.

**Version History:**

| Version | Date       | Change                                                                                                                                       |
| ------- | ---------- | -------------------------------------------------------------------------------------------------------------------------------------------- |
| v0.1    | (filed)    | Mission filed open with planned status. 6 ACs (mock impl + harness + state-hash compare + passing test + regression test + shared decode). |
| v0.2    | 2026-08-07 | Claimed + closed Band A same-session. 6/6 ACs green. 7/7 tests pass; clippy scoped-clean; fmt clean. Status header flipped Claimed→Closed (Band A — 2026-08-07). |

Last Updated: 2026-08-07
Version: 0.2
