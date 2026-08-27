# l4-parallel-transfer-event-log-elimination — consolidate parallel TransferEventLog trait (Layer B invariant)

**Status:** claimed (2026-08-27)
**Substrate:** RFC-0960 §2.5 (EventLogProducer trait) + `cipherocto-design-principles` §No parallel abstractions
**Parent:** RFC-0960 §6 Mission B follow-on (R3 review L4 CRITICAL #2)
**Depends on:**

- Mission B (`0960-v37-b-event-log-producer-wiring.md`) — `TransferEventLog` (octo-vault) trait + `produce_burn` call site must exist with the parallel abstraction in place before refactor
- Mission F (`0960-v36-burn-event-dqa-migration-substrate.md`) — `crate::burn_event::TransferEventRef::consume` parallel trait source

## Motivation

R3 adversarial review (Lens-4 Layer-direction) flagged that `produce_burn` carries two parallel `TransferEventLog` traits at the function signature:

1. `octo_vault::TransferEventLog` — Layer B substrate port (production impl lives in `octo-vault-stoolap`)
2. `crate::burn_event::TransferEventLog` — Layer C parallel trait owned by octo-policy

Both expose an `insert(&TransferEventRef)` shape but exist as two distinct trait declarations. Per `cipherocto-design-principles` §No parallel abstractions, this duplicates adapter code, creates two health-check surfaces, and violates the Layer B invariant (one source of truth for substrate ports).

## Scope

Eliminate the parallel trait. The Layer B `octo_vault::TransferEventLog` (defined at the `TransferEventLog` trait symbol in `crates/octo-vault/src/vault_balance_projection.rs`) is the canonical substrate port. `crate::burn_event::TransferEventLog` MUST be removed and replaced by direct use of `octo_vault::TransferEventLog`.

### Sub-steps

1. **Audit parallel trait surface** — `grep -rn "TransferEventLog" crates/octo-policy/src/burn_event.rs` returns: trait decl (1 method `insert`) + impls in tests. The 4-method shape (`insert`, `sum_to_vault`, `sum_from_vault`, `max_occurred_at_unix`) belongs to the canonical `octo_vault::TransferEventLog`; only `insert` is duplicated on the parallel trait.

2. **Delete parallel trait** — `crate::burn_event::TransferEventLog` removed from `crates/octo-policy/src/burn_event.rs`. Test impls in same file replaced by single `impl TransferEventLog for &mut TestLog` against `octo_vault::TransferEventLog`.

3. **Refactor `produce_burn` signature** — drop the `consume_log: &mut dyn crate::burn_event::TransferEventLog` parameter. `BurnEventRef::consume` is rewritten to accept `&mut dyn octo_vault::TransferEventLog` (Layer B port). Result: signature shrinks from 8 to 7 params, mirroring `produce_payment` (6 params) / `produce_settlement` (6 params).

4. **Audit downstream call sites** — only the 15 TV-BE tests in `burn_event.rs` use `crate::burn_event::TransferEventLog`. Each must rewrite to use `octo_vault::TransferEventLog`. The `pub use crate::burn_event::TransferEventLog as BurnTransferEventLog` re-export in `crates/octo-policy/src/event_log_producer.rs` is REMOVED.

5. **Verify no other consumers** — `grep -rn "burn_event::TransferEventLog\|BurnTransferEventLog" crates/ agents/ use-cases/ docs/` returns 0 matches after step 4.

## Out of Scope

- Moving `octo_vault::TransferEventLog` trait to a different layer (Layer B hosting is canonical per `cipherocto-design-principles`)
- Adding new methods to `TransferEventLog` (no surface change)
- Changing `TransferEventLogInsertError` (re-exported from `vault_balance_projection`, already canonical)

## Test Vectors

- TV-L4-1: `produce_burn` signature now has 7 params (not 8); `consume_log` parameter gone
- TV-L4-2: `crate::burn_event::TransferEventLog` decl removed; `cargo doc --no-deps` lists zero entries for that path
- TV-L4-3: All 15 TV-BE tests still pass after rewrite; `cargo test --workspace --lib` green
- TV-L4-4: `pub use ... BurnTransferEventLog` re-export removed; downstream crate compiles without it
- TV-L4-5: `grep -rn "TransferEventLog" crates/octo-policy/src/burn_event.rs` returns ONLY references to the imported `octo_vault::TransferEventLog` (zero decl / impl references to a local trait)

## Layer direction (per `cipherocto-design-principles`)

- `octo-vault` (Layer B) — retains canonical `TransferEventLog` port (unchanged)
- `octo-policy` (Layer C) — `crate::burn_event::TransferEventLog` deleted; `BurnEventRef::consume` rewritten to consume `octo_vault::TransferEventLog`
- Layer C → Layer B (allowed per design principles)
- Layer B → Layer C inversion remains forbidden (verified — `octo_vault` does not gain `octo-policy` dep)

## Validation

```bash
# Pre-merge
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --features full -- -D warnings
cargo test --workspace --lib

# Grep gates
grep -rn "burn_event::TransferEventLog\|BurnTransferEventLog" crates/ agents/ use-cases/ docs/
# expect: zero matches

grep -rn "TransferEventLog" crates/octo-policy/src/burn_event.rs
# expect: only `use octo_vault::TransferEventLog` + `&mut dyn octo_vault::TransferEventLog` references
```

## Backward compat

- `produce_burn` signature BREAKING (8 params → 7 params). Justified per `cipherocto-design-principles` §No parallel abstractions + R3 review L4 CRITICAL #2 verdict.
- `BurnEventRef::consume` signature BREAKING (Layer C → Layer B trait swap). Mitigated by `octo-policy` being the sole consumer.
- All TV-BE tests require update — green at landing time.

## Risk

- HIGH: mid-migration compile failure — splitting this trait-elimination refactor across commits leaves the workspace non-compiling (parallel trait gone, single trait not yet wired at all consumer sites). Mitigation: atomic commit alongside `producer-wrapper-consumer-wiring` sibling mission per `caveat-central-enum-non-exhaustive` Cycle ordering rule; the two land together.
- MEDIUM: orphan-impl drift after parallel-trait deletion — `impl TransferEventLog for X` blocks left at deleted sites block the next `cargo build`. Mitigation: TV-L4-2 + TV-L4-5 grep gates catch orphans at landing time.
- LOW: doc-link staleness — `cargo doc --no-deps --workspace` surfaces intra-doc broken refs to the deleted parallel trait. Mitigation: `cargo doc` post-merge gate catches stale links.

## Cross-references

- `cipherocto-design-principles` §No parallel abstractions — canonical rule
- RFC-0960 §2.2 — `TransferEventLog` port (Layer B substrate)
- RFC-0960 §2 — `BurnEventRef::consume` (Mission F substrate that consumes the trait)
- Mission B (`0960-v37-b-event-log-producer-wiring.md`) — `produce_burn` call site
- Mission F (`0960-v36-burn-event-dqa-migration-substrate.md`) — source of the parallel trait
- R3 review L4 CRITICAL #2 — finding source

## Claimant

@mmacedoeu

## Pull Request

#