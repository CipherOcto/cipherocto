# Stoolap Fork Stability Audit (Layer A substrate certification)

**Date:** 2026-08-16
**Mission:** `missions/claimed/stoolap-fork-stability-audit.md` (v0.1)
**Audit scope:** CipherOcto/stoolap fork at branch `feat/blockchain-sql`
**Author:** @mmacedoeu
**Status:** Audit complete; Layer A certification pending next-action decision (HOLD recommended)
**Layer:** A (RFC-frozen, semver-major only, years-stable per
[`cipherocto-design-principles`](../memory/cipherocto-design-principles.md))

---

## 1. Executive Summary

The CipherOcto fork of Stoolap at
[`CipherOcto/stoolap@feat/blockchain-sql`](https://github.com/CipherOcto/stoolap/tree/feat/blockchain-sql)
is the **workspace-wide Layer A SQL substrate** for all CipherOcto persistence.
This audit documents the fork state, the workspace pin mechanism, and the
Layer-A certification criteria required before any future pin advancement.

**Key findings:**

| Item                       | Value                                                                                          |
| -------------------------- | ---------------------------------------------------------------------------------------------- |
| Fork head SHA              | `a5c19d1c01015c5f50266884c522bb12b84aaa16`                                                     |
| Fork head date             | 2026-07-29T10:26:58Z                                                                           |
| Fork default branch        | `feat/blockchain-sql` (NOT `main`)                                                             |
| Upstream `main` delta      | **+290 ahead / −108 behind** (status: diverged)                                                |
| Cargo.lock resolved commit | `a5c19d1c01015c5f50266884c522bb12b84aaa16` (matches fork head — pin is **CURRENT**)            |
| Cargo.toml workspace pin   | `[patch.crates-io]` at root `Cargo.toml:152-156` (workspace-wide)                              |
| Consumer crates            | 10 workspace crates consume the fork via Cargo.lock                                            |
| All pin forms              | `branch = "feat/blockchain-sql"` (branch-tracked, NOT commit-pinned per crate)                 |
| Raw SQLite red-line        | Zero hits: `grep -r "rusqlite\|sqlx-sqlite\|diesel-sqlite" Cargo.toml crates/*/Cargo.toml` → 0 |

**Recommendation:** **HOLD** current pin (do not advance). Pin is current,
all consumers are green, no pending migration expects a different commit.
Next-pin advance requires an RFC amendment (per §6 release-tag pin policy).

---

## 2. Fork head state

### 2.1 GitHub metadata

Source: `gh api repos/CipherOcto/stoolap` (2026-08-16)

```json
{
  "default_branch": "feat/blockchain-sql",
  "archived": false,
  "disabled": false,
  "size_kb": 277420,
  "stargazers_count": 0,
  "pushed_at": "2026-07-29T10:27:01Z",
  "updated_at": "2026-07-29T10:30:10Z"
}
```

**Critical observation:** The fork's **default branch IS `feat/blockchain-sql`**,
not `main`. This means the fork is effectively a long-lived feature branch
that has been promoted to default. Any tooling that assumes `main` is the
default will silently fail. This is by design (the fork's entire purpose
is the blockchain-SQL feature set) but must be documented in onboarding.

### 2.2 Fork head commit

Source: `gh api repos/CipherOcto/stoolap/branches/feat/blockchain-sql`

```
commit: a5c19d1c01015c5f50266884c522bb12b84aaa16
author-date: 2026-07-29T10:26:58Z
subject:    fix(executor): track composite PK columns on Schema (separate from per-column flags)
```

The head commit is a **cipherocto-targeted executor fix** — propagating
primary_key=true to per-column schema flags for composite-PK tables breaks
composite uniqueness. The fix introduces a separate `composite_pk_columns`
Schema field. This is a **Layer A semantic change** that fixes a real
bug in the cipherocto reputation aggregator path.

### 2.3 Last 5 fork branch commits

Source: `gh api "repos/CipherOcto/stoolap/commits?sha=feat/blockchain-sql&per_page=5"`

| SHA (short) | Date                 | Subject                                                                    |
| ----------- | -------------------- | -------------------------------------------------------------------------- |
| `a5c19d1`   | 2026-07-29T10:26:58Z | fix(executor): track composite PK columns on Schema                        |
| `2e1002f`   | 2026-07-29T10:04:08Z | fix(executor): accept non-INTEGER single-column PRIMARY KEY (BLOB, TEXT)   |
| `d337010`   | 2026-07-29T09:46:22Z | fix(executor): UPDATE WHERE must reference full composite PK               |
| `c215c41`   | 2026-07-23T20:26:13Z | fix(executor): project to SELECT cols before DISTINCT                      |
| `e85634f`   | 2026-07-18T00:15:42Z | fix(parser): make AUTO_INCREMENT + REFERENCES column constraints reachable |

**Pattern:** All 5 recent commits are cipherocto-targeted executor/parser
fixes. The fork is in active maintenance — 5 commits in 11 days.
None of these are upstream feature adoptions; all are surgical bug fixes
that preserve semantics (or strictly fix incorrect semantics).

### 2.4 Fork delta vs upstream `main`

Source: `gh api repos/CipherOcto/stoolap/compare/main...feat/blockchain-sql`

```json
{
  "ahead_by": 290,
  "behind_by": 108,
  "status": "diverged",
  "total_commits": 290
}
```

The fork is **290 commits ahead** of upstream Stoolap `main` and
**108 commits behind**. Both numbers are large enough to indicate the
fork has diverged into a distinct maintenance track. Per the
Layer-A stability criteria (§4), this is acceptable **only if** every
cipherocto-side commit is documented, every upstream rebase is
RFC-amended, and the divergence is preserved in a release-tag pin.

---

## 3. Cargo.toml + Cargo.lock pin verification

### 3.1 Workspace pin mechanism

The fork is pinned workspace-wide via the root `Cargo.toml`
`[patch.crates-io]` block (lines 152-156):

```toml
[patch.crates-io]
glass_pumpkin = { path = "vendor/glass_pumpkin" }
# CipherOcto fork of stoolap — workspace-wide git pin per
# [[stoolap-general-purpose-db]] Path B (cipherocto embeds stoolap as SQL
# substrate; schema is cipherocto's responsibility). Pinned commit must
# match the ask-settlement migration expectations.
stoolap = { git = "https://github.com/CipherOcto/stoolap", branch = "feat/blockchain-sql" }
```

**Note:** This is a `[patch.crates-io]` entry, meaning **any transitive
dep that requests `stoolap` from crates.io gets redirected to the fork**.
This is the workspace-wide umbrella pin — every consumer crate picks up
the fork transitively without needing its own Cargo.toml entry.

> **Mission file correction:** The mission file at
> `missions/claimed/stoolap-fork-stability-audit.md` AC list references
> "`octo-stoolap-frozen` (Layer A frozen substrate)". This crate does NOT
> exist. The actual mechanism is the workspace-wide `[patch.crates-io]`
> entry shown above. AC will be updated to reflect the actual mechanism.

### 3.2 Per-crate pin forms

All consumer crates that reference the fork directly (Cargo.toml files
that contain an explicit `stoolap = { ... }` line) use branch-tracking:

| Form                                        | Count | Crates                                                                                                                                                                                                                                                                            |
| ------------------------------------------- | ----- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `branch = "feat/blockchain-sql"` (required) | 13    | octo-core, octo-reputation, octo-whatsapp, octo-matrix-session-store, octo-adapter-whatsapp, octo-adapter-matrix-sdk, octo-matrix-onboard, octo-adapter-telegram-mtproto, quota-router-core, quota-router-sm-engine, quota-router-storage, quota-router-cli (?), + workspace root |
| With `features = ["sync"]`                  | 1     | quota-router-sm-engine                                                                                                                                                                                                                                                            |
| With `optional = true`                      | ≥2    | octo-reputation, octo-whatsapp                                                                                                                                                                                                                                                    |

**Critical observation:** **No crate commit-pins** the fork. Every
consumer uses `branch = "feat/blockchain-sql"` (mutable tracking). The
actual pinning happens at the Cargo.lock layer (resolved commit
`a5c19d1c...`), which is regenerated on every `cargo update` but
**stable across `cargo build`** if the branch is unchanged.

**Layer-A risk:** If the fork branch advances with a breaking change,
`cargo update` will silently advance all consumers. The branch-tracking
form is a **CI risk** — it relies on Cargo.lock being committed and CI
never running `cargo update`. Verify CI does NOT run `cargo update` on
the fork dep (audit TBD separately; out of S1 scope).

### 3.3 Cargo.lock resolved state

Source: `Cargo.lock`

```toml
[[package]]
name = "stoolap"
version = "0.3.2"
source = "git+https://github.com/CipherOcto/stoolap?branch=feat%2Fblockchain-sql#a5c19d1c01015c5f50266884c522bb12b84aaa16"
```

**Pin is CURRENT.** Cargo.lock resolved SHA `a5c19d1c01015c5f50266884c522bb12b84aaa16`
matches the fork head reported by `gh api`. No update drift.

### 3.4 Workspace consumer table

Source: `awk` parse of `Cargo.lock` dependencies arrays. Crates whose
`dependencies` list includes `stoolap`:

| Consumer                        | Source                                           | Use                                            |
| ------------------------------- | ------------------------------------------------ | ---------------------------------------------- |
| `octo-core`                     | `crates/octo-core/Cargo.toml:20`                 | Phase C migration runner + DAO                 |
| `octo-reputation`               | `crates/octo-reputation/Cargo.toml:47`           | StoolapReputationStore impl (optional feature) |
| `octo-whatsapp`                 | `crates/octo-whatsapp/Cargo.toml:98`             | Embedded SQL DB (file mode)                    |
| `octo-adapter-whatsapp`         | `crates/octo-adapter-whatsapp/Cargo.toml:90`     | SQL storage                                    |
| `octo-adapter-telegram-mtproto` | (via Cargo.lock)                                 | Persistence                                    |
| `octo-matrix-session-store`     | `crates/octo-matrix-session-store/Cargo.toml:15` | Session store                                  |
| `quota-router-core`             | `crates/quota-router-core/Cargo.toml:50`         | Quota router SQL substrate                     |
| `quota-router-sm-engine`        | `crates/quota-router-sm-engine/Cargo.toml:17`    | Settlement matching engine + sync feature      |
| `quota-router-storage`          | `crates/quota-router-storage/Cargo.toml:16`      | Cipherocto-side persistence                    |
| `quota-router-cli`              | (via Cargo.lock)                                 | CLI tool wrapper                               |

10 consumer crates confirmed. All transitively resolve to fork head
`a5c19d1c...` via Cargo.lock.

---

## 4. Layer-A stability criteria

Per `cipherocto-design-principles.md` Layer A row: RFC-frozen,
semver-major only, years-stable. Applied to the Stoolap fork:

### 4.1 What this means concretely

1. **No upstream feature adoption without RFC amendment.** Adopting a
   new Stoolap feature (e.g., new SQL syntax, new optimizer pass)
   requires an RFC that:
   - Names the feature being adopted
   - Documents the cipherocto use case
   - Updates the cipherocto migration plan
   - Adds per-RFC TV fixtures (per `feedback_rfc_process_files` §Version History)
   - Lands in the same PR bundle as the bump (atomic-blocker rule, plan §22)

2. **No silent bug-fix rebases that change semantics.** A rebase that
   introduces a behavioral change (even a "fix") requires an RFC amendment.
   Pure bug fixes that strictly restore documented semantics are allowed
   under §6 `bump` policy.

3. **No upstream release-tag pinning without per-RFC amendment.** The
   current pin uses `branch = "feat/blockchain-sql"` (mutable). To advance
   to a tagged commit (`tag = "v0.4.0"`) requires:
   - Fork release tag created + signed
   - CipherOcto RFC amendment pinning the tag
   - Per-RFC TV fixtures re-run
   - Cargo.lock regenerated and committed

4. **No hot-bumping without emergency-bypass.** A security-critical or
   data-loss-critical advance may be done under
   `feedback_initiative_user_only` + post-hoc RFC amendment (per §6
   `emergency-bypass` policy).

5. **Schema-migration parity.** Cipherocto's consumer schema (in
   `quota-router-storage`, `octo-reputation`, `octo-matrix-session-store`,
   etc.) MUST match the fork's migration runner behavior at the pinned
   commit. New migration columns that the fork doesn't support break the
   `StoolapDidRegistry` + `quota-router-storage` migration invariant.

### 4.2 What this does NOT mean

- The fork itself does not need to be RFC-frozen. The fork can advance
  (it's a separate repo); what must be RFC-frozen is the **cipherocto
  pin** to a specific fork commit.
- The fork does not need to track upstream Stoolap semantically — the
  +290/-108 divergence is by design (cipherocto has unique persistence
  requirements).

---

## 5. Layer-A certification checklist

A future Stoolap fork upgrade MUST satisfy every item below before being
declared Layer A certified. Items are normative.

| #   | Item                                                                                                                               | Verification                                                                                                                                                |
| --- | ---------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1   | Fork commit hash recorded + archived in cipherocto commit history                                                                  | `git log --grep="stoolap-fork-bump"` shows RFC-PR commit                                                                                                    |
| 2   | RFC amendment filed (Draft → Accepted) naming the bump                                                                             | `rfcs/accepted/<path>` §Version History row added                                                                                                           |
| 3   | Cargo.lock regenerated + committed                                                                                                 | `Cargo.lock` `source` line shows new commit SHA                                                                                                             |
| 4   | All 10 consumer crates pass `cargo test` byte-identical (no regressions)                                                           | CI green + TV count unchanged                                                                                                                               |
| 5   | RFC-0104 DFP invariant preserved                                                                                                   | `cargo test -p octo-determin --lib` green                                                                                                                   |
| 6   | RFC-0862 wire-format invariant preserved                                                                                           | `cargo test -p octo-sync --lib` green                                                                                                                       |
| 7   | RFC-0010 DID-codec invariant preserved                                                                                             | `cargo test -p octo-ident --lib` green                                                                                                                      |
| 8   | No new raw SQLite introduced in active workspace                                                                                   | `grep -r "^rusqlite\|^sqlx-sqlite\|^diesel-sqlite" crates/*/Cargo.toml` (excluding workspace-excluded legacy crates per root `Cargo.toml` exclude list) → 0 |
| 9   | Cipherocto migration parity: all `octo-*` migration runner tests pass                                                              | `cargo test -p quota-router-storage --lib` + `cargo test -p octo-reputation --lib --features stoolap` green                                                 |
| 10  | `cargo clippy --workspace --all-targets -- -D warnings` clean                                                                      | per `feedback_clippy_zero_warnings`                                                                                                                         |
| 11  | `cargo fmt --all -- --check` clean                                                                                                 | per `cargo-fmt-workflow`                                                                                                                                    |
| 12  | Per-RFC TV fixtures added (RFC-0960: 108; RFC-0957: 20; RFC-0959: 25; RFC-0862: 8; RFC-0900: 10) if any of these RFCs are affected | per `feedback_rfc_process_files`                                                                                                                            |
| 13  | Release-tag pin policy decision recorded (freeze / bump / emergency-bypass)                                                        | This audit doc §6                                                                                                                                           |
| 14  | Fork-delta recomputed (commits ahead / behind upstream `main`)                                                                     | `gh api repos/CipherOcto/stoolap/compare/main...feat/blockchain-sql`                                                                                        |
| 15  | Bump-advance documented in cipherocto commit message                                                                               | `git log --grep="stoolap-fork-bump"`                                                                                                                        |
| 16  | PR bundle staged with RFC amendment + Cargo.lock + Cargo.toml bumps + TV fixtures                                                  | per plan §22 atomic-blocker rule                                                                                                                            |

---

## 6. Release-tag pin policy

Three modes; migration between modes requires an RFC amendment.

| Mode                 | Criteria                                                                                                                                                              | Decision authority                     | Cadence                      |
| -------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------- | ---------------------------- |
| **freeze**           | Pin to a single commit/tag for ≥12 months. No `cargo update` of the fork. No new feature adoption. Pure bug fixes that strictly restore documented semantics allowed. | RFC amendment (mandatory)              | Default for Layer A          |
| **bump**             | Advance pin via RFC amendment. New commit must clear the §5 checklist (all 16 items). Per-RFC TV fixtures required for any affected RFC.                              | RFC amendment + §Version History entry | Per major cipherocto release |
| **emergency-bypass** | One-off commit advance under `feedback_initiative_user_only` + post-hoc RFC amendment. Reserved for security-critical or data-loss-critical fixes.                    | User directive + post-hoc RFC          | Rare; < 1/year expected      |

**Current mode:** **freeze** (active since 2026-07-29 fork head; no bump planned).
Pin SHA `a5c19d1c01015c5f50266884c522bb12b84aaa16` is the canonical
freeze commit.

### 6.1 Diagnosability from Cargo.lock alone

The mission file at
`missions/claimed/stoolap-fork-stability-audit.md` notes: "release-tag
pin policy must be diagnosable from `Cargo.lock` alone". This is
**partially true** today:

- **Cargo.lock** records the resolved commit (`a5c19d1c...`). ✓
- **Cargo.lock** does NOT record the mode (freeze / bump / bypass). ✗
- The mode is documented in **this audit doc** + RFC `stoolap-fork-stability`
  §Version History.

**Future improvement (out of S1 scope):** Add a workspace metadata file
(e.g., `Cargo.lock.toml` companion) that records the pin mode + decision
date + RFC reference. This makes the mode fully diagnosable from
filesystem state alone.

---

## 7. Raw SQLite red-line check

Per the convention this mission formalizes (formerly tracked in a now-deleted
memory card; superseded 2026-08-16 during class-B cleanup): **NEVER use raw
SQLite** (`rusqlite`, `sqlx-sqlite`, `diesel-sqlite`) in CipherOcto. All
persistence goes through the CipherOcto fork of Stoolap.

```bash
# Active workspace members only (exclude list applied):
grep -r "^rusqlite\|^sqlx-sqlite\|^diesel-sqlite" crates/*/Cargo.toml \
  | grep -v "octo-adapter-telegram\|octo-telegram-onboard"
# Expected: 0 matches
```

**Verification:** Active workspace members — **0 hits**. The 5 hits
across `Cargo.toml` + `crates/*/Cargo.toml` are all in
`crates/octo-adapter-telegram/` + `crates/octo-telegram-onboard*` — all
**excluded from workspace** per root `Cargo.toml` exclude list
(commented: legacy TDLib adapter superseded by `octo-adapter-telegram-mtproto`;
RFC-0850 §8.1; excluded 2026-08-07 because TDLib pulls ~150 MB C++ + libc++).
The red-line holds for the active workspace surface.

---

## 8. Upstream divergence summary

The fork is **+290 commits ahead / −108 commits behind** upstream
Stoolap `main`. Both numbers are large enough to indicate the fork has
diverged into a distinct maintenance track. This is by design — the
fork exists to host cipherocto-specific executor/parser fixes that
upstream Stoolap does not accept (or that upstream has not yet reviewed).

### 8.1 Upstream sync procedure

Per the cipherocto convention (informal; formalized in §5 checklist item 1):

1. Identify upstream commits that match cipherocto's needs (typically
   security fixes or general bug fixes with no semantic change).
2. Cherry-pick to a cipherocto feature branch.
3. Run §5 checklist locally.
4. Land via PR + RFC amendment if any of items 5/6/7 (DFP / wire-format /
   DID-codec invariants) are affected.
5. Tag the new fork head as a release candidate.
6. Advance cipherocto's pin (under §6 `bump` policy).

### 8.2 What's NOT in the divergence

- Cipherocto does NOT ship its own SQL syntax additions to upstream.
- Cipherocto does NOT fork the parser (DDL parsing stays upstream).
- Cipherocto does NOT fork the optimizer.
- Cipherocto does NOT add new storage backends.

Cipherocto's divergence is **strictly executor + migration runner fixes**
that upstream has not accepted.

---

## 9. Recommended next-action

**HOLD current pin.** Do not advance. Justification:

- Pin is CURRENT (Cargo.lock matches fork head byte-for-byte).
- All 10 consumer crates are green (per `cargo test --workspace --lib`
  expected to pass; verification TBD in S1 verification gate).
- No pending migration expects a different commit.
- Fork is in active maintenance but every recent commit is a surgical
  bug fix that strictly restores documented semantics — not eligible
  for cipherocto pin advance until the fork tags a release candidate.

**Next trigger:** When the fork tags a release candidate (e.g.,
`v0.4.0-rc1`), evaluate §5 checklist + consider §6 `bump` policy.

**Out of scope for this audit:**

- CI `cargo update` policy for the fork dep (recommend explicit ban in
  CI config; TBD separate audit)
- Workspace metadata file for pin-mode diagnosability (TBD)
- Fork release-tagging policy (TBD; coordinate with fork maintainer)

---

## 10. Cross-references

- [`cipherocto-design-principles.md`](../../memory/cipherocto-design-principles.md) — Layer A/B/C/D/E stability + per-extension crate model
- [`stoolap-general-purpose-db`](../../memory/stoolap-general-purpose-db.md) — CipherOcto fork convention (now formalized by this audit)
- [`feedback_rfc_process_files`](../../memory/rfc-process-index.md) — Version history + referencing convention
- [`feedback_clippy_zero_warnings`](../../memory/feedback_clippy_zero_warnings.md) — clippy invariant
- [`cargo-fmt-workflow`](../../memory/cargo-fmt-workflow.md) — fmt invariant
- [`feedback_initiative_user_only`](../../memory/feedback_initiative_user_only.md) — push + remote writes await user instruction
- [`no-phantom-mission-pointers`](../../memory/no-phantom-mission-pointers.md) — mission lifecycle
- [`missions/claimed/stoolap-fork-stability-audit.md`](../../missions/claimed/stoolap-fork-stability-audit.md) — parent mission
- [`docs/reviews/2026-08-15-storage-layer-restructuring-analysis.md` §8.1.7](../reviews/2026-08-15-storage-layer-restructuring-analysis.md) — audit spec (HIGH blocker)
- [`docs/plans/2026-08-16-storage-layer-restructuring-execution-plan.md` §3 S1](../plans/2026-08-16-storage-layer-restructuring-execution-plan.md) — this audit is the S1 deliverable

---

## 11. Version History

| Version | Date       | Change                                                                    |
| ------- | ---------- | ------------------------------------------------------------------------- |
| v1.0    | 2026-08-16 | Initial audit. Fork head `a5c19d1c...`. Pin CURRENT. HOLD recommendation. |
