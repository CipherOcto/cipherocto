# 0015-b-substrate-defect-amendment — RFC-0015-b substrate-defect paired amendment

**Status:** Open
**Substrate:** RFC-0015 §Pre-existing Substrate + RFC-0015-a §6.1 + §6.4 + RFC-0016 §Pre-existing Substrate
**Parent:** RFC-0015 (read-path) + RFC-0015-a (write-path) + RFC-0016 (audit read-path) — all Accepted 2026-09-14 per `docs/audits/2026-09-14-rfc-0015-0016-plateau-declaration.md`
**Depends on:** RFC-0015 Accepted; RFC-0015-a Accepted; RFC-0016 Accepted

## Scope

Draft RFC-0015-b as the **paired-acceptance amendment** that addresses
the 7 substrate defects documented in
`docs/audits/2026-09-14-rfc-0015-0016-plateau-declaration.md` §Deferred
substrate defects. Each defect is restated as an RFC requirement
(not as a defect) so the amendment can DRY-review as a normal
additive amendment to the parent RFC chain.

Per the R48 precedent (`RFC-0012-v2` + `RFC-0014-v2`), the amendment
follows the paired-acceptance pattern: amendment RFC + paired
substrate-implementation mission land as a unit. RFC-0015-b is the
RFC surface; mission `0015-b-substrate-defect-impl` is the paired
implementation surface.

## Why this exists

The R12 + R13 DRY plateau (R12 = 30 findings, R13 = 50 findings,
non-converging) on RFC-0015 + RFC-0015-a + RFC-0016 surfaced 7
substrate defects that were **out of scope** for the RFC DRY loop
(they require substrate code amendments, not RFC text changes).
Per Option B in the plateau declaration, the defects become a
formal paired-acceptance amendment backlog rather than continuing
the DRY loop with diminishing returns.

## 7 defects as RFC-0015-b requirements

### Defect 1 — `WalletError::AlreadyInTransition(Uuid)` activation

**Current state:** variant declared at `crates/octo-wallet/src/error.rs:184-190`
but never constructed by `transition_agent`. Dead surface at parent
RFC-0015-a acceptance.

**RFC-0015-b requirement §X.1:** `transition_agent` MUST construct
`WalletError::AlreadyInTransition(uuid)` on `parking_lot::Mutex::try_lock()`
failure (contention path) AND on observed concurrent-call detection
(post-acceptance verification via `inspect_concurrent_call_log`).

**Activation surface:** the `try_lock` path replaces the current
`std::sync::Mutex::lock` blocking path; `try_lock` failure is the
canonical signal for in-flight transitions. CLI mirrors the variant
via `OctoCliError::AlreadyInTransition(Uuid)` (already declared at
parent RFC-0015-a acceptance; re-verified at amendment landing).

**Layer model:** Layer B substrate mutation. No CLI behavior change
(exit code 43 mapping pre-existing).

### Defect 2 — doc-comment drift on `AlreadyInTransition`

**Current state:** `error.rs:184-190` doc-comment references
`parking_lot::Mutex::try_lock` but substrate implements
`std::sync::Mutex::lock` at `crates/octo-wallet/src/agent.rs:488`.

**RFC-0015-b requirement §X.2:** once §X.1 lands, the doc-comment
becomes accurate (matches implementation). Pre-amendment landing:
amendment cites the `try_lock` form as the operative intent; the
substrate implementation is upgraded to match.

**Layer model:** doc-comment hygiene, no code change beyond §X.1.

### Defect 3 — `lookup_agent` existence-leak

**Current state:** `agent.rs:486-500` returns `WalletError::AgentNotFound`
for unknown Uuid but `WalletError::ForbiddenHolderMismatch` for
caller-DID mismatch. Existence leak enables enumeration: caller can
probe for Uuids belonging to other DIDs.

**RFC-0015-b requirement §X.3:** `lookup_agent(caller_did, uuid)` MUST
normalize both unknown + not-owned cases to `WalletError::AgentNotFound`.
The multi-DID enumeration side-channel closes. The
`ForbiddenHolderMismatch` variant remains reserved (used by other
substrate paths that DO need to distinguish; not by `lookup_agent`).

**Layer model:** Layer B substrate mutation. CLI behavior unchanged
(`OctoCliError::AgentNotFound(Uuid)` exit 42 mapping pre-existing).
Security posture improves (no information leak on probe).

### Defect 4 — `transition_agent` TOCTOU window

**Current state:** `validate_reason` runs BEFORE
`registry().lock()`. Concurrent caller could mutate registry between
reason validation and lock acquisition.

**RFC-0015-b requirement §X.4:** `transition_agent` MUST acquire
the registry lock FIRST, then run `validate_reason` INSIDE the lock.
Lock-then-validate ordering closes the TOCTOU window.

**Layer model:** Layer B substrate mutation. CLI behavior unchanged
(signature preserved). Test vectors MUST add TOCTOU regression
coverage (concurrent caller race test).

### Defect 5 — phantom-event window on audit-append + rollback

**Current state:** on audit-append failure + rollback, the registry
state is reverted but the audit event chain may have been partially
appended. Phantom-event window: appears as "transition committed" but
chain-hash references stale state.

**RFC-0015-b requirement §X.5:** `transition_agent` MUST add:

- **§X.5.a** monotonic `state_version: u64` field on `AgentRecord`,
  bumped before each successful transition.
- **§X.5.b** post-rollback verification pass that asserts
  `audit_chain.tip.state_version == current_state_version - 1`.
  Mismatch → `WalletError::AuditChainInconsistent(version_observed,
version_expected)`.

**Layer model:** Layer B substrate mutation. CLI behavior unchanged;
substrate adds an internal reconciliation step before
`transition_agent` returns. New error variant `AuditChainInconsistent`
added to `WalletError` (additive).

### Defect 6 — missing state-machine tests

**Current state:** `crates/octo-wallet/src/agent.rs` lacks tests
for `Registered → Running`, `Running → Terminated`, `Terminated → *`
(reject), `Running → Running` (idempotent).

**RFC-0015-b requirement §X.6:** substrate addition must include
test coverage for ALL canonical state-machine edges per RFC-0002
§Agent State Machine:

| Edge                                                 | Test expected outcome                                                           |
| ---------------------------------------------------- | ------------------------------------------------------------------------------- |
| `Registered → Running`                               | `Ok(TransitionReceipt { current_state: Running, ... })`                         |
| `Running → Terminated`                               | `Ok(TransitionReceipt { current_state: Terminated, ... })`                      |
| `Terminated → Running`                               | `Err(WalletError::InvalidStateTransition { from: Terminated, to: Running })`    |
| `Registered → Terminated`                            | `Err(WalletError::InvalidStateTransition { from: Registered, to: Terminated })` |
| `Running → Running`                                  | `Ok(TransitionReceipt)` idempotent (no audit append)                            |
| `Running → Terminated` (post-§X.4 TOCTOU regression) | Race test: 2 concurrent callers; one wins, one gets `AlreadyInTransition`       |

**Layer model:** test addition, no behavior change.

### Defect 7 — RFC-0015 §Pre-existing Substrate parity gap

**Current state:** RFC-0015 §Pre-existing Substrate lists substrate
items, but several substrate items landed in Phase 2 unblock work
(R13.5 + earlier). Substrate parity gap.

**RFC-0015-b requirement §X.7:** RFC-0015 §Pre-existing Substrate
table refresh. Add 3 rows for `transition_agent`,
`TransitionReceipt` projection, `AgentState::default = Registered`
field on `AgentRecord`. Note "Phase 2 unblock landing (commit
`next e09f3e3a` 2026-09-13)" in the §Pre-existing Substrate
timestamp column.

**Layer model:** RFC text refresh, no code change.

## Amendment structure (RFC-0015-b shape)

RFC-0015-b follows the parent RFC structure (mirrors RFC-0015-a §X.x
section numbering). New sections:

- §X.1 Activation: `AlreadyInTransition` path
- §X.2 doc-comment alignment
- §X.3 `lookup_agent` normalization
- §X.4 TOCTOU window closure
- §X.5 phantom-event detection
- §X.6 test-vector additions
- §X.7 §Pre-existing Substrate refresh

Version History entry: "v3.2 — substrate-defect paired amendment (7
defects from plateau declaration 2026-09-14)".

## Acceptance Criteria

- [ ] RFC-0015-b drafted at `rfcs/draft/process/0015-b-substrate-defects.md`
- [ ] Each of 7 defects restated as numbered §X.1-§X.7 requirements
- [ ] Layer-model annotations on each §X.x (Layer B substrate mutation; CLI behavior unchanged for §X.1, §X.2, §X.4, §X.5, §X.6, §X.7; CLI security posture improvement for §X.3)
- [ ] Cross-references to parent RFC-0015 + RFC-0015-a + RFC-0016 sections
- [ ] Cross-reference to plateau declaration audit doc
- [ ] Cross-reference to paired implementation mission `0015-b-substrate-defect-impl`
- [ ] Multi-round DRY review loop on RFC-0015-b (per parent RFC pattern)
- [ ] Promoted Draft → Accepted (paired with implementation mission landing per `[[no-phantom-mission-pointer]]` rule)

## DRY review pattern (parent RFC mirror)

R1 spawn 5-len reviewers on RFC-0015-b. Aggregate → R1.5 fix → R2 (DRY
verification round 1) → R2.5 fix → R3 (DRY verification round 2 = DRY
CLOSED) → closure artifacts (audit doc + memory card).

5 reviewers per round:

1. correctness
2. layer-model
3. simplification
4. hygiene
5. substrate-faithfulness (critical: each §X.x matches the operative
   defect description from plateau declaration)

## Cross-references

- `docs/audits/2026-09-14-rfc-0015-0016-plateau-declaration.md` §Deferred substrate defects
- RFC-0015 (read-path) §Pre-existing Substrate
- RFC-0015-a (write-path) §6.1 + §6.4 (operative intent — substrate implementation references §6.1, amendment brings §6.1 into alignment)
- RFC-0016 (audit read-path) §Pre-existing Substrate
- `[[substrate-faithfulness-verification]]` — verify reviewer substrate claims against actual code BEFORE acting on briefs
- `[[no-phantom-mission-pointer]]` — paired RFC + implementation mission must both be valid

## Why gate

Release-gated on parent RFC-0015 + RFC-0015-a + RFC-0016 Accepted
landing. Per parent RFC-0015-a §6.4 paired-invariance rule, the
amendment RFC and the paired implementation mission form a unit;
the amendment lands Accepted FIRST, then the implementation mission
lands (which references the Accepted amendment as substrate contract).
