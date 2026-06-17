# R15 R3 Adversarial Review

**Date:** 2026-06-17
**Reviewer:** Self (autonomous)
**Scope:** Full R14 + R15 R1+R2 surface area
**Prior rounds:** R1 (8 issues, all fixed), R2 (5 issues, all fixed)

## Issues Found

### R3-1 (HIGH): Slash envelopes with unrecognized sub-codes are accepted
**File:** `crates/octo-network/src/dc/slash.rs::process_dc_slash`
**Problem:** `process_dc_slash` validates `slash_reason == 0x000F` but
NOT `slash_reason_data` against the `DcMisbehavior` enum. An
attacker who controls 2/3 of the witness quorum can submit an
envelope with `slash_reason=0x000F, slash_reason_data=0x0099` and
the slash is processed (counted, cooldown applied, possibly
permanent ban at 5th), but `envelope.misbehavior()` returns
`None` — operators have no record of what the slash was for.

**Fix:** Reject `slash_reason_data` that doesn't map to a
`DcMisbehavior` variant. Added `InvalidSlashReasonData(u32)` to
`DcSlashError`. Test `invalid_slash_reason_data_rejected`.

### R3-2 (MEDIUM): `cool_down_epochs` saturates one epoch too early
**File:** `crates/octo-network/src/dc/slash.rs::cool_down_epochs`
**Problem:** The old code had `if slash_count >= 63 { return
u64::MAX; }`. But `1u64 << 63` is `0x8000_0000_0000_0000` and does
NOT overflow — only `1u64 << 64` would. The threshold was one
position early, so `cool_down_epochs(63)` returned `u64::MAX`
(2^64 - 1) instead of 2^63. Also the existing
`cool_down_overflow_safe` test asserted the wrong value.

**Fix:** Changed threshold to `>= 64`. Updated test
`cool_down_overflow_safe` to assert `cool_down_epochs(64) ==
u64::MAX`. Added `cool_down_at_63_is_2_pow_63` boundary test.

### R3-3 (MEDIUM): `BindEnvelope::new` accepts empty fields
**File:** `crates/octo-network/src/mon/bind_envelope.rs::BindEnvelope::new`
**Problem:** An empty `domain_id` would create the degenerate
gossip topic `/dot/bind/` (line 124 of `gossip/bind.rs`). Empty
`platform` or `group_id` produce a BIND that names nothing.

**Fix:** Added `assert!` guards for all three string fields.
Tests `new_rejects_empty_domain_id`, `new_rejects_empty_platform`,
`new_rejects_empty_group_id` (using `#[should_panic]`).

### R3-4 (MEDIUM): No stress test for `BindGossipState` eviction
**File:** `crates/octo-network/src/gossip/bind.rs::record_received`
**Problem:** R1 added `MAX_RECEIVED_BINDS=1024` with FIFO
eviction, but the test coverage was limited to dedup
(`record_received_dedupes`). A regression that broke eviction
would not be caught.

**Fix:** Added `fifo_eviction_at_max_received_binds` (insert
MAX+10, verify oldest 10 evicted, most recent retained, count
correct) and `eviction_does_not_evict_duplicates` (re-insert of
duplicate must not trigger eviction).

### R3-5 (LOW): `multi_account::export` does not validate `account_id`
**File:** `crates/octo-whatsapp-onboard-core/src/multi_account.rs::export`
**Problem:** R2 added `validate_account_id` to `import`,
`use_account`, and `import_bundle`, but missed `export`. While
`export` doesn't write to a path derived from `account_id` (it
writes to a caller-supplied `out` path), the `account_id` is
embedded in `manifest.json` and an empty/invalid id produces a
malformed manifest.

**Fix:** Added `validate_account_id(account_id)?;` at top of
`export`. Test `export_rejects_path_traversal_account_id`.

### R3-6 (LOW): `multi_account::remove` does not validate `account_id`
**File:** `crates/octo-whatsapp-onboard-core/src/multi_account.rs::remove`
**Problem:** Same as R3-5. The function only removes from the
in-memory index, so the security impact is low, but the
inconsistency makes the API surprising.

**Fix:** Added `validate_account_id(account_id)?;` at top of
`remove`. Test `remove_rejects_path_traversal_account_id` also
verifies the original account survives a rejected remove.

## Out of Scope (Already Verified in R1/R2)

- VDF `verify` is properly tested (R1)
- `BindGossipState` bounded by `MAX_RECEIVED_BINDS=1024` (R1)
- `member_count_at_bind` in `canonical_bytes` (R1)
- 5-of-7 recovery multi-sig doc (R1)
- `0x000C, 0x000E-0xFFFF` reserved range doc (R1)
- Variable shadowing in `trust_graph::celebrities` (R1)
- `domain_id_base58` doc (R1)
- `#[cfg(unix)]` symlink fix in `multi_account` (R1)
- `account_id` path traversal in import/use_account/import_bundle (R2)
- `ConsensusOutcome::InProgress` (R2)
- Consensus N=0 guard (R2)
- `isqrt` doc note (R2)
- `verify_attest` MAX boundary (R2)

## Test Counts

| Crate                                | R0   | After R1 | After R2 | After R3 |
|--------------------------------------|------|----------|----------|----------|
| octo-network                         | 1044 | 1052     | 1052     | 1059     |
| octo-whatsapp-onboard-core           | 34   | 39       | 39       | 41       |
| octo-whatsapp-onboard                | 7    | 21       | 21       | 21       |

R3 added 7 tests: 2 in `dc::slash`, 2 in `gossip::bind`, 3 in
`mon::bind_envelope` (all `#[should_panic]`).

## Summary

6 issues found, 6 fixed. No new Critical issues. No remaining
unresolved issues from prior rounds.
