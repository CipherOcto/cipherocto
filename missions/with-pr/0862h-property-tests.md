# Mission: 0862h — Property Tests for Sync Protocol

## Status

In Review (PR submitted 2026-06-22)

## RFC

RFC-0862 v1.1.0 (Networking): Stoolap Data Sync Protocol — §Test Vectors, §Performance Targets, §Implementation Phases, §DatabaseSyncAdapter Trait (v1.1.0)

## Summary

Implement comprehensive property-based tests for the Sync protocol. Property tests verify invariants that must hold for all inputs, not just specific examples. This mission covers:

1. **Envelope round-trip** — every envelope type DCS-encodes and decodes losslessly
2. **LSN monotonicity** — `entry.lsn == previous_lsn + 1` for all entries
3. **Merkle tree determinism** — same segments → same root
4. **HMAC binding** — different `transport_key` or different `node_id` → different HMAC
5. **AEAD round-trip** — `decrypt(encrypt(p, aad), aad) == p`
6. **State machine coverage** — every transition in the table is exercised

## Design

### New module: `octo-sync/tests/property_tests.rs` (in the `octo-sync/` leaf workspace at `cipherocto/octo-sync/tests/`)

Use the `proptest` crate (already in cipherocto's dev-dependencies) for property-based testing. The tests run against `MockAdapter` (per mission 0862-base Phase 0) — the property tests do NOT require a real Stoolap database; the adapter boundary means tests are runnable on a plain `cargo test -p octo-sync` invocation.

```rust
use proptest::prelude::*;

proptest! {
    /// Every envelope type must DCS-encode and decode losslessly.
    #[test]
    fn envelope_roundtrip(envelope in any_envelope()) {
        let encoded = envelope.encode();
        let decoded = Envelope::decode(&encoded).unwrap();
        prop_assert_eq!(envelope, decoded);
    }

    /// LSN monotonicity: any sequence of LSNs starting from 1 must satisfy
    /// `entry.lsn == previous.lsn + 1` after sorting by LSN.
    #[test]
    fn lsn_monotonicity(lsns in proptest::collection::vec(1u64..1_000_000, 1..1000)) {
        let mut sorted = lsns.clone();
        sorted.sort();
        sorted.dedup();
        // After dedup, must be strictly monotonic
        for w in sorted.windows(2) {
            prop_assert_eq!(w[1], w[0] + 1);
        }
    }

    /// Merkle tree determinism: same segments → same root.
    #[test]
    fn merkle_tree_deterministic(segments in proptest::collection::vec(any_segment(), 1..1000)) {
        let tree1 = MerkleSegmentTree::from_segments(&segments);
        let tree2 = MerkleSegmentTree::from_segments(&segments);
        prop_assert_eq!(tree1.root(), tree2.root());
    }

    /// HMAC binding: different `transport_key` or different `node_id` → different HMAC.
    #[test]
    fn hmac_binding(
        transport_key in any_32_bytes(),
        node_id in any_32_bytes(),
        summary_body in any_bytes()
    ) {
        let h1 = hmac_blake3(&transport_key, &summary_body, &node_id);
        let mut tk2 = transport_key;
        tk2[0] ^= 1;
        let h2 = hmac_blake3(&tk2, &summary_body, &node_id);
        prop_assert_ne!(h1, h2);
    }

    /// AEAD round-trip.
    #[test]
    fn aead_roundtrip(
        key in any_32_bytes(),
        nonce in any_12_bytes(),
        plaintext in any_bytes(),
        aad in any_bytes(),
    ) {
        let ct = encrypt(&key, &nonce, &plaintext, &aad);
        let pt = decrypt(&key, &nonce, &ct, &aad).unwrap();
        prop_assert_eq!(plaintext, pt);
    }

    /// State machine: every transition in the table is reachable.
    #[test]
    fn state_machine_coverage(sequence in proptest::collection::vec(any_event(), 1..100)) {
        let mut sm = SyncStateMachine::new();
        for event in sequence {
            sm = sm.apply(event);
            // Every state must be one of the 7 valid states
            prop_assert!(matches!(sm.state(),
                SyncLifecycle::Init
                | SyncLifecycle::Connecting
                | SyncLifecycle::Authenticating
                | SyncLifecycle::Streaming
                | SyncLifecycle::Suspect
                | SyncLifecycle::Reconnecting
                | SyncLifecycle::Terminated
            ));
        }
    }
}
```

### Test categories

- **Unit property tests** (in `tests/property_tests.rs`): the 6 above
- **Integration property tests** (in `tests/property_integration_tests.rs`): end-to-end scenarios
  - Two-node sync with random operation sequences
  - Random partitions and heals
  - Random reader/writer crashes and restarts
  - Random schema migrations
  - Random peer failures

## Acceptance Criteria

- [ ] `octo-sync/tests/property_tests.rs` (in the `octo-sync/` leaf workspace) exists with 6 property tests
- [ ] `octo-sync/tests/property_integration_tests.rs` exists with 5 integration property tests
- [ ] Each property test runs 1000+ iterations (`PROPTEST_CASES=1000` env var)
- [ ] All property tests pass in CI on Linux x86_64 and macOS arm64
- [ ] Cross-implementation determinism: property tests produce the same counterexamples on both platforms
- [ ] No false positives: each found counterexample is a real bug, not a test bug
- [ ] `cargo test -p octo-sync --features proptest` passes
- [ ] The test runner reports the number of cases run for each property test
- [ ] Property tests use `MockAdapter` from `octo-sync/src/test_util.rs`; no real Stoolap DB is required (per RFC-0862 v1.1.0)

## Tests

- **Property tests (the 6 listed above)**
- **Integration property tests (the 5 listed above)**

## Dependencies

- **Requires:**
  - `0862-base` — Sync engine (the unit under test), **`MockAdapter`** (per RFC-0862 v1.1.0)
  - `0862a` — WAL-tail streamer
  - `0862b` — Merkle summary
  - `0862c` — snapshot segment indexer
  - `0862d` — OCrypt key ring
  - `0862e` — ReplayCache persistence
  - `0862f` — multi-peer
  - `0862g` — cross-carrier
  - `proptest` crate (already in dev-dependencies)

- **Required by:** none (this is the test-coverage mission)

## Blockers / Dependencies

- **Blocked by:** all other 0862 missions (this mission tests them)
- **Blocks:** none

## Description

Property-based tests verify invariants that must hold for all inputs, not just specific examples. This mission is the test-coverage mission for the entire Sync protocol. It exercises the 6 core invariants (envelope round-trip, LSN monotonicity, Merkle tree determinism, HMAC binding, AEAD round-trip, state machine coverage) and 5 end-to-end scenarios (two-node sync, partitions, crashes, schema migrations, peer failures).

## Technical Details

### Performance

- **Property test runtime:** < 5 minutes total for all 11 tests × 1000 cases each
- **Memory:** < 200 MB peak (proptest shrinking can use a lot of memory)
- **CI runtime:** adds < 5 minutes to the test suite

### Why property tests (not just example tests)?

Example tests cover specific scenarios; they can miss edge cases. Property tests verify invariants for all inputs, including edge cases the author didn't think of. Per the BLUEPRINT, property tests are a hard requirement for consensus-relevant code (RFC-0862 is not consensus, but it carries application state that downstream ZK proofs reference, so the same standard applies).

### Why 1000 cases?

Empirically, 1000 cases finds 95% of bugs in a single run; 10000 cases finds 99%. The remaining 1% is typically found by `cargo fuzz` (out of scope for this mission). 1000 cases is the sweet spot for CI runtime vs bug coverage.

### Pitfalls

- **Don't generate too-large inputs.** `proptest` will generate up to 1 MB inputs by default; for Sync, limit to 16 KB per envelope (matches the DOT MTU for one segment).
- **Don't use `proptest!` for stateful tests.** Use `proptest_state_machine` (separate crate) for state machine testing.
- **Don't ignore shrinking.** When a property test fails, `proptest` shrinks the failing input to the minimal counterexample. Always fix the bug, not the test.
- **Don't run property tests in release mode.** They need to instrument the code; release optimizations can hide bugs.

---

**Mission Type:** Testing
**Priority:** High
**Phase:** 2 (Catch-up via snapshot segments)
**RFC Section Coverage:** §Test Vectors, §Performance Targets

## Type Coverage

This mission is a **testing mission** that exercises the types defined by other missions, not a new-type mission. The 6 property tests cover the following RFC-0862 types:

| Test | Types Exercised |
|------|-----------------|
| `envelope_roundtrip` | All 13 envelope types from `envelope.rs` (mission 0862-base) |
| `lsn_monotonicity` | LSN monotonicity enforcement (mission 0862-base `lsn.rs`) |
| `merkle_tree_deterministic` | `MerkleSegmentTree` (mission 0862b) |
| `hmac_binding` | `KeyRing::summary_hmac` (mission 0862d) |
| `aead_roundtrip` | `KeyRing::encrypt`/`decrypt` (mission 0862d) |
| `state_machine_coverage` | `SyncLifecycle` 7-state enum (mission 0862-base) |

The 5 integration property tests exercise the end-to-end Sync flow (writer + reader + WAL-tail + Merkle + segments + state machine).
