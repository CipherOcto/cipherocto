# Mission: marketplace-caller-await-migration

## Status

Closed 2026-08-13 (@claude). Vacuously satisfied — no remaining
production callers to migrate. Search evidence + deferral rationale
recorded.

## RFC

RFC-0968 retirement gate. Mission
`marketplace-facade-reputation-async-migration` v0.2 added the async
read/write surface (`record_outcome_async`, `read_reputation_async`).
The intended follow-up was to migrate every production caller of the
legacy `record_outcome` over to the async surface before the gate
flip.

## Search evidence (2026-08-13)

```
rg "record_outcome\(" --type rust crates/quota-router-core/src/

crates/quota-router-core/src/marketplace/mod.rs:1097-1106  (tests, mod tests block)
crates/quota-router-core/src/node/metrics.rs:145-149       (Metrics::record_outcome, different type)
crates/quota-router-core/src/node/mod.rs:483-610           (Metrics::record_outcome, different type)
crates/quota-router-core/src/guardrails/engine.rs:59,71    (GuardrailEngine::record_outcome, different type)
```

All `Marketplace::record_outcome` references live INSIDE the
`marketplace` module's own unit-test block. The `Metrics` and
`GuardrailEngine` `record_outcome` methods are on different types
with different semantics (counter increments, not reputation
writes) and are out of scope.

Production call sites:
- Task market settlement (`crates/quota-router-core/src/task_market/`) — does NOT call `Marketplace::record_outcome` today. Settlement goes through `escrow` + `orderbook`; the reputation write would happen as a separate step that the task market does not yet perform.
- Inference mesh scorer (`crates/quota-router-core/src/node/`) — records to `Metrics`, not `Marketplace`.
- Admin RPC handlers — no current write site.

## Conclusion

There are ZERO production callers of `Marketplace::record_outcome` to
migrate. The async reputation surface was added preemptively in
mission `marketplace-facade-reputation-async-migration` v0.2 to be
ready when the first caller materializes. Once a real caller exists
(settlement path, scorer, etc.) the migration is a mechanical
`record_outcome` → `record_outcome_async(..., controller_id, now)`
swap; that caller materialization is OUT OF SCOPE for this mission.

## Acceptance Criteria

- [x] Confirm there are zero production callers of
      `Marketplace::record_outcome` (search evidence above)
- [x] Document why no migration is needed today (preemptive async
      surface; settlement path does not yet call the reputation
      surface)
- [x] Record the deferred trigger: the first production caller of
      `record_outcome` (or, equivalently, the first production
      caller of `record_outcome_async`) must be logged in this
      mission's Notes section, after which a new follow-on mission
      must be filed to migrate it.

## Claimant

(@claude)

## Pull Request

(in progress)

## Notes

**Deferred trigger:** the moment a production code path records an
inference outcome via `Marketplace::record_outcome`, a follow-on
mission `marketplace-caller-await-migration-real` (TBD) must be
filed to convert that caller to `record_outcome_async`. Until then,
this mission stays closed-vacuously.

**Production caller discovery checklist (for whoever finds the first
one):**
1. Identify the caller (settlement / scorer / admin / CLI).
2. Add `Marketplace::record_outcome_async(...).await?` with the
   `controller_id` derived from the operator's governance pubkey via
   `reputation_compat::controller_id_from_governance_pubkey(...)`
   (RFC-0968-A1 amendment 44).
3. The caller becomes async — propagate `.await` up the stack.
4. Add a test that pins the dual-read parity invariant under the
   new caller.

## Cross-references

- Mission `marketplace-facade-reputation-async-migration` v0.2
- Mission `marketplace-cheapest-with-ranking-async` v0.2
- RFC-0968 §retirement gate
- RFC-0968-A1 amendment 44 (controller_id derivation)

## Version History

| Version | Date       | Status  | Change                                                                                                                       |
| ------- | ---------- | ------- | ---------------------------------------------------------------------------------------------------------------------------- |
| v0.1    | 2026-08-13 | closed  | Vacuously satisfied — search evidence shows zero production callers of `Marketplace::record_outcome`. Preemptive async surface ready for the first real caller. |
