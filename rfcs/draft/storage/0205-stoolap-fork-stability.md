# RFC-0205 (Storage): Stoolap Fork Stability Certification

## Status

**Version:** 1.3 (2026-08-19)
**Status:** Draft
**Layer:** B (governance; introduces no Layer A boundary change — constrains how an existing Layer B dependency is layered)

## Authors

- Author: @mmacedoeu

## Maintainers

- Maintainer: Stoolap steward team (per plan §3 A.2)
- Co-maintainer: octo-storage-core owner

## Summary

Formalizes a two-tier split of the Stoolap fork substrate: a **Layer A frozen snapshot** (workspace `[patch.crates-io]` redirect pointing the upstream `stoolap` crate at a commit-pinned rev, years-stable) consumed only by `octo-storage-core` (the new Layer A substrate per RFC-0206), and a **Layer B active fork** (branch `feat/blockchain-sql`) consumed by the Layer B facade `octo-storage` and downstream storage crates. Defines release-tag pin policy, monthly + quarterly re-certification schedule, and Cargo.toml pinning discipline. Closes the §8.1.7 HIGH blocker from the storage restructure review.

## Dependencies

**Requires:**

- RFC-0105 (Numeric): Deterministic Quant Arithmetic — DQA substrate consumed by the frozen fork
- RFC-0010 (Process): Canonical DID Codec — chain_id namespace depends on fork wire form
- RFC-0870 (Process): Node envelope version_tag — pins the on-wire tag version that the frozen fork must match

**Optional:**

- RFC-0900 (Economics): Chain-aware slash ledger — depends on fork's DQA(scale) codec

> **Dependency Validation Rules:** All upstream RFCs are Accepted. This RFC introduces no new layer-A dependency; it constrains how an existing Layer B dependency (the Stoolap fork) is layered.

## Design Goals

| Goal | Target                 | Metric                                                                                                                                     |
| ---- | ---------------------- | ------------------------------------------------------------------------------------------------------------------------------------------ |
| G1   | Zero silent fork drift | workspace `Cargo.toml` `[patch.crates-io]` `stoolap.rev` equals `git rev-parse octo-stoolap-frozen-vN`; byte-equal in CI                   |
| G2   | ≤ 7-day CVE response   | Time from upstream CRITICAL CVE announcement to patched frozen fork release (see §Release-Tag Pin Policy "CRITICAL CVE mid-cycle" trigger) |
| G3   | Monthly re-cert green  | Re-cert checklist passes every 30 days                                                                                                     |
| G4   | Quarterly split review | Layer A vs Layer B boundary review every 90 days                                                                                           |

## Motivation

`docs/reviews/2026-08-15-storage-layer-restructuring-analysis.md` §8.1.7 audit identified that the Stoolap fork (workspace root `Cargo.toml` `[patch.crates-io]` block, `branch = "feat/blockchain-sql"`) is **NOT CERTIFIED** for Layer A stability:

- No release tag, no commit SHA pin
- Active fork maintained by CipherOcto team; merge-to-upstream plan not started
- Layer A requires years-stable primitives; an un-pinned moving fork is a layering contradiction

The Stoolap fork hosts critical substrate features consumed across the chain: `DQA(scale)`, `Value::quant`, `as_dqa`, `encode_decimal_lexicographic`, 16-byte MVCC DQA extension. Until certified, DQA-as-SQL columns are additive Layer B (RFC-driven), not Layer A (RFC-frozen).

**Solution:** Adopt a two-tier split per §8.1.7. `octo-storage-core` (Layer A substrate per RFC-0206) consumes a frozen commit-pinned snapshot via workspace `[patch.crates-io]`. `octo-storage` + downstream crates (Layer B) consume the active fork. `octo-determin` is a Layer A crate but does NOT consume the fork directly (per `determin/Cargo.toml` — zero Stoolap dep); it provides the DQA substrate (RFC-0105) that the frozen fork is expected to encode. The split is enforced by Cargo.toml dependency direction, not by convention.

## Roles and Authorities

1. **Stoolap steward team** — owns the fork; performs freeze + re-cert; files RFCs to bump the frozen rev.
2. **octo-storage-core owner** — sole Layer A consumer of the frozen fork via workspace `[patch.crates-io]` redirect (per `crates/octo-storage-core/Cargo.toml`); flags fork-side changes that need an upstream merge.
3. **RFC reviewer** — signs off on rev bumps and emergency CVE bypasses.
4. **On-call security** — co-signs emergency CVE bypasses per release-tag pin policy table.

| Role                    | Identifier                              | Authority Scope                                                        | Lifecycle                 | Source/Ref                       |
| ----------------------- | --------------------------------------- | ---------------------------------------------------------------------- | ------------------------- | -------------------------------- |
| Stoolap steward         | GitHub team `@stoolap-stewards`         | Fork maintenance, freeze, re-cert                                      | Active until role revoked | RFC-0205 §Two-Tier Architecture  |
| octo-storage-core owner | GitHub team `@octo-storage-core-owners` | Layer A consumption via `[patch.crates-io]`; fork-side change flagging | Active until role revoked | RFC-0205 §Two-Tier Architecture  |
| RFC reviewer            | RFC process role                        | Rev bump approval, CVE bypass co-sign                                  | Per-RFC                   | RFC-0205 §Release-Tag Pin Policy |
| On-call security        | Rotation role                           | 24-hour CVE bypass co-sign                                             | Per-incident              | RFC-0205 §Release-Tag Pin Policy |

## Specification

### Two-Tier Architecture

```mermaid
graph TD
    subgraph LayerA["Layer A (years-stable, RFC-frozen)"]
        Core["crates/octo-storage-core<br/>Layer A substrate per RFC-0206<br/>consumes fork via [patch.crates-io]"]
        Determin["determin/<br/>DQA substrate (RFC-0105)<br/>zero Stoolap dep per Cargo.toml"]
    end
    subgraph Frozen["Frozen fork (consumed by Layer A only)"]
        Frozen["workspace [patch.crates-io]<br/>stoolap.rev = '&lt;sha&gt;'<br/>tagged octo-stoolap-frozen-vN"]
    end
    Core --> Frozen
    subgraph LayerB["Layer B (RFC-driven, additive)"]
        Fork["Active Stoolap fork<br/>branch = feat/blockchain-sql"]
        Facade["crates/octo-storage<br/>(Layer B re-export facade per RFC-0206)"]
        Consumers["downstream crates<br/>(quota-router-storage,<br/>octo-vault, etc.)"]
        Facade --> Fork
        Consumers --> Facade
    end
    Core -. MUST NOT .-> Fork
    Facade -. MUST NOT .-> Frozen
    Consumers -. MUST NOT .-> Frozen
```

> **Note:** The fork repo's `Cargo.toml` declares `name = "stoolap"` (the upstream crate name); the workspace does NOT consume a separately-published `octo-stoolap-frozen` crate. The frozen pin mechanism is `[patch.crates-io] stoolap = { git = "...", rev = "<sha>" }`, which redirects the upstream `stoolap` crate to the fork at a frozen rev. Tagging is performed via `octo-stoolap-frozen-vN` for visibility, but the actual cargo dependency is the upstream-named `stoolap` crate, not a separately-published crate.

**Dependency direction rule (enforced by `cargo metadata` audit + CI gate):**

- `octo-storage-core` MAY consume the fork via workspace `[patch.crates-io]` redirect (the only Layer A consumer).
- `octo-storage-core` MUST NOT consume the active `feat/blockchain-sql` branch directly (bypassing the patch).
- `octo-storage` and downstream crates MAY depend on the active fork (`stoolap` branch pin per their Cargo.toml).
- `octo-storage` and downstream crates MUST NOT carry their own `[patch.crates-io]` redirect of `stoolap` (would override the workspace frozen pin).
- Cross-tier dependency = layering violation; rejected at CI.

### Cargo.toml Pinning

**Layer A (workspace root `Cargo.toml` `[patch.crates-io]` block):**

```toml
# Layer A — frozen snapshot redirect
# The fork repo's package name is "stoolap" (upstream); we redirect the
# crates-io stoolap to the fork at a frozen rev. Layer A consumer is
# octo-storage-core (per RFC-0206); Layer B crates get the same redirect
# transitively but are expected to depend on `stoolap` by branch directly.
[patch.crates-io]
stoolap = { git = "https://github.com/CipherOcto/stoolap", rev = "<commit-sha-when-frozen>" }
# Tagging convention: octo-stoolap-frozen-v{N} (N monotonic from 0) points
# to the same commit as the frozen rev. Tag-mismatch = layering violation.
# Bump policy: see §Release-Tag Pin Policy
```

**Layer B (`crates/octo-storage/Cargo.toml`, `crates/octo-vault/Cargo.toml`, etc.):**

```toml
# Layer B — active fork by branch (the workspace [patch.crates-io] block
# is in effect when building, but Layer B crates MAY also declare a direct
# branch pin for documentation; in practice they get the patched stoolap)
[dependencies]
stoolap = { git = "https://github.com/CipherOcto/stoolap", branch = "feat/blockchain-sql" }
# Used by octo-storage, quota-router-storage, etc.
```

### Release-Tag Pin Policy

| Trigger                                                         | Action                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                    | Owner                              | SLA                                      |
| --------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------- | ---------------------------------------- |
| Initial freeze                                                  | Pin `rev = "<sha>"` of `feat/blockchain-sql` HEAD; tag `octo-stoolap-frozen-v0`                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                           | Stoolap steward team               | one-time                                 |
| Upstream Stoolap major release                                  | File RFC to bump `octo-stoolap-frozen` rev; security audit + RFC-major bump required                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                      | Stoolap steward + RFC reviewer     | 7 days for CRITICAL CVE; 90 days for LOW |
| For-Layer-A consumer request                                    | `octo-determin` only; never directly consumed by other crates                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                             | octo-determin owner                | 30 days                                  |
| Emergency CVE bypass                                            | `octo-determin` may consume active fork temporarily with audit log entry per consumption                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                  | on-call security + Stoolap steward | 24 hours for CRITICAL CVE                |
| Monthly re-cert                                                 | Re-certify the frozen commit-applies checklist (CI green, security audit clean, no upstream regressions in the dependency surface). Calendar trigger: 30 days after last freeze tag (not calendar-month 1st — deterministic sliding window from freeze event). Bootstrap clause: the first re-cert cycle starts at the v0 freeze tag; pre-v0 (before Phase 1 Task 1 freezes) no re-cert obligation exists, but the steward MUST hold a daily diff-watch on `feat/blockchain-sql` and surface any breaking change to RFC reviewer within 24 h. **Transition:** First 30-day cycle starts at the `octo-stoolap-frozen-v0` tag-creation timestamp (sliding window from tag creation); daily-watch obligation auto-terminates once the v0 tag exists on the workspace branch. | Stoolap steward                    | 30 days (post-v0); daily-watch (pre-v0)  |
| Quarterly re-cert                                               | Full review of the two-tier split, cargo-pinning health, merge-to-upstream progress                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                       | Stoolap steward + RFC reviewer     | 90 days                                  |
| CRITICAL CVE disclosed mid-cycle (active fork, ice-until-patch) | Resume daily-watch on `feat/blockchain-sql`; file `octo-stoolap-frozen-v{N+1}` bump RFC; freeze v{N+1} from `feat/blockchain-sql` HEAD; CI byte-verify (TV-0205-05) gates the bump. Frozen consumer (`octo-storage-core`) auto-picks up via workspace `[patch.crates-io]` redirect on next `cargo build`; no consumer migration required.                                                                                                                                                                                                                                                                                                                                                                                                                                 | Stoolap steward + RFC reviewer     | 7 days per SLA                           |

### Determinism Requirements

- `octo-stoolap-frozen` MUST be byte-for-byte reproducible at `rev = "<sha>"`.
- DQA(scale) wire form (16-byte BE) MUST NOT change across re-cert without an RFC bump.
- `encode_decimal_lexicographic` byte output MUST be pinned across re-cert.

### Operation Class Mapping

| Operation                   | Class | Rationale                                                                                                                                  |
| --------------------------- | ----- | ------------------------------------------------------------------------------------------------------------------------------------------ |
| Freeze + rev-pin            | A     | Layer A substrate; years-stable; sole consumer is `octo-storage-core` (per §Two-Tier Architecture)                                         |
| Monthly re-cert             | C     | Operational; no consensus impact                                                                                                           |
| Emergency CVE bypass        | C     | Operational; gated by audit log + co-sign                                                                                                  |
| Quarterly split review      | C     | Governance; no consensus impact                                                                                                            |
| CRITICAL CVE mid-cycle bump | A     | Same-layer substrate bump; requires RFC, no separate wiring (`octo-storage-core` auto-picks up via workspace `[patch.crates-io]` redirect) |

> **Note:** Operation Class A/B/C taxonomy per `docs/BLUEPRINT.md` §RFC Process. No separate RFC-NNNN anchors this taxonomy; it is defined inline in the process doc.

### Error Handling

| Error                                        | Detection                            | Recovery                                               |
| -------------------------------------------- | ------------------------------------ | ------------------------------------------------------ |
| `octo-stoolap-frozen` rev mismatch           | CI `cargo metadata` audit gate       | Re-pin to declared SHA; reject merge if drift detected |
| Cross-tier dependency                        | CI graph audit                       | Reject merge; route to RFC reviewer                    |
| Fork diverges from upstream by > 100 commits | Quarterly review trigger             | File merge-to-upstream sub-mission                     |
| CVE in frozen fork                           | Monthly re-cert or upstream advisory | Per release-tag pin policy table                       |

## Performance Targets

| Metric               | Target   | Notes                 |
| -------------------- | -------- | --------------------- |
| Re-cert cycle time   | < 1 day  | Manual checklist + CI |
| Initial freeze       | one-time | Stamp v0              |
| CVE patch turnaround | ≤ 7 days | CRITICAL; 90 days LOW |

## Implicit Assumptions Audit

| Assumption                                   | Where Relied Upon                         | Blast Radius if False                                         | Mitigation / Status                                                                                                                                                                                                                                                                               |
| -------------------------------------------- | ----------------------------------------- | ------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Fork maintainer availability                 | §Release-Tag Pin Policy (monthly re-cert) | Frozen rev falls behind upstream; CVE unpatched               | Backup steward + monthly SLA escalation; ACCEPTED RISK                                                                                                                                                                                                                                            |
| Upstream Stoolap does not delete fork repo   | §Cargo.toml Pinning                       | `Cargo.toml` rev points to 404                                | Mirror to internal registry; quarterly verify                                                                                                                                                                                                                                                     |
| `octo-storage-core` is sole Layer A consumer | §Two-Tier Architecture                    | If another crate consumes the frozen pin → layering violation | CI gate: `cargo metadata --format-version 1 \| jq '.packages[] \| select(.dependencies[]?.name == "stoolap" and .dependencies[]?.source == "git+https://github.com/CipherOcto/stoolap?rev=<sha>") \| .name'` returns the list of consumers-of-frozen-rev (rejects any name ≠ `octo-storage-core`) |
| DQA wire form stable across re-cert          | §Determinism Requirements                 | Settlement replay diverges                                    | Pinned at RFC-0105; bump = RFC-major                                                                                                                                                                                                                                                              |

### Categories to Audit

- **Operator trust** — Stoolap steward team is trusted; compromise → MITM via poisoned SHA. Mitigation: RFC reviewer co-sign on rev bump (separate GitHub team; co-signer key NOT co-located with steward account); CI verifies `Cargo.toml` rev matches git tag.
- **Platform trust** — GitHub is trusted as fork host; outage → `Cargo.toml` resolution fails. Mitigation: mirror to internal registry; quarterly failover drill.
- **Time source** — re-cert cadence is calendar-based (30/90 days); clock skew does not affect.
- **Network partition** — fork fetch requires network access; offline CI fails. Mitigation: vendor fork tarball in offline mode.
- **Upgrade safety** — `octo-stoolap-frozen` bumps require RFC; no silent upgrade. CI gate enforces.
- **Configuration** — `Cargo.toml` is the configuration source of truth; no env vars.
- **Identity stability** — steward GitHub team membership must be stable; quarterly audit.
- **Resource availability** — fork repo availability; same as platform trust.

## Security Considerations

- **Fork poisoning** — attacker compromises steward account, pushes malicious commit, rev-pin includes it. Mitigation: RFC reviewer co-sign + git tag signed with maintainer key.
- **CVE in fork** — frozen snapshot lacks patch. Mitigation: 7-day CVE SLA + emergency bypass.
- **Layer A drift** — `octo-storage-core` accidentally consumes active fork directly (bypassing `[patch.crates-io]` redirect). Mitigation: CI graph audit.
- **Replay attacks** — fork replay of old DQA wire form across re-cert. Mitigation: wire form pinned at RFC-0105; bump = RFC-major.

## Adversary Analysis

| Decision               | Q1 Beneficiary                          | Q2 Cost to Attacker        | Q3 Gain if Successful                        | Q4 Defense (cost to legit op)                                                                                                                                                                                                                                    | Q5 Residual Risk                                |
| ---------------------- | --------------------------------------- | -------------------------- | -------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------- |
| Pin fork to commit SHA | Compromised steward                     | Steward account compromise | Inject malicious DQA codec → consensus split | RFC reviewer co-sign via out-of-band channel (HW security key held by separate person, not GitHub team membership) + git tag signature + CI byte-verify gate (TV-0205-05 — `git merge-base --is-ancestor` + `git rev-parse <tag> == <rev>` byte-equal; low cost) | LOW — multi-party co-sign + automated rejection |
| Two-tier split         | Fork maintainer pushing breaking change | Reputation cost            | Force Layer A to upgrade early               | CI rejects cross-tier deps (low cost)                                                                                                                                                                                                                            | LOW — automatic gate                            |
| Monthly re-cert        | Lazy steward                            | None (passive)             | CVE goes unpatched                           | 30-day SLA; escalation to RFC reviewer (low cost)                                                                                                                                                                                                                | MED — depends on steward vigilance              |

### Severity Classification

| Severity     | Definition                              | Action                                             |
| ------------ | --------------------------------------- | -------------------------------------------------- |
| **CRITICAL** | Fork poisoning accepted into frozen rev | MUST mitigate before Accept (RFC reviewer co-sign) |
| **HIGH**     | CVE unpatched > 7 days                  | SHOULD mitigate; ACCEPTED RISK with deadline       |
| **MEDIUM**   | Layer A consumes active fork            | SHOULD mitigate (CI gate)                          |
| **LOW**      | Re-cert skipped one cycle               | MAY accept; document residual                      |

### Multi-Round Review

This RFC touches the Layer A substrate boundary. Multi-round review with severity classification is REQUIRED per `docs/BLUEPRINT.md` §Adversarial Review Process.

## Economic Analysis

No new tokens or stake implications. Cost: ~0.5 FTE/month for steward re-cert + quarterly RFC reviewer time. Mitigated by monthly cadence + automation.

## Compatibility

- **Backward:** DQA wire form unchanged (pinned at RFC-0105); pre-RFC-0205 forks resolve identically to post-RFC-0205 workspace `[patch.crates-io]` frozen redirect (the upstream `stoolap` crate is patched to the fork rev at build time).
- **Forward:** RFC bump of the workspace `[patch.crates-io]` `stoolap.rev` is the only change vector; new RFCs may amend §Release-Tag Pin Policy table. Drop-fork scenario (e.g., upstream Stoolap merges DQA features and fork is retired) requires migration of every Layer B crate's `stoolap` branch pin to crates-io semver — see §Future Work.

## Test Vectors

No byte-exact TV — this is a governance RFC. Verification is structural. TV-0205-01..05 are **forward requirements** — they gate the CI gate install once Phase 1 Task 1 freeze lands (today the workspace carries `branch = "feat/blockchain-sql"`, no `octo-stoolap-frozen` dep exists); the CI script MUST be installed with skip-check enabled on `main` until v0 freeze tag exists.

1. **TV-0205-01:** workspace `Cargo.toml` `[patch.crates-io]` block has `stoolap = { git = "https://github.com/CipherOcto/stoolap", rev = "<sha>" }` (not `branch = "feat/blockchain-sql"`). **(forward requirement — gates once Phase 1 Task 1 freeze lands)**
2. **TV-0205-02:** `crates/octo-storage-core/Cargo.toml` depends on `stoolap` (the upstream-named crate, resolved to the fork via workspace `[patch.crates-io]`); does NOT depend on `stoolap` by branch directly. **(forward requirement)**
3. **TV-0205-03:** `crates/octo-storage/Cargo.toml` depends on `stoolap` by branch (Layer B; the workspace `[patch.crates-io]` is in effect at build time, but the declared branch pin documents the active-fork intent). **(forward requirement)**
4. **TV-0205-04:** CI graph audit rejects any crate other than `octo-storage-core` resolving `stoolap` to the frozen rev. Implemented as `cargo metadata --format-version 1 | jq '.packages[] | select(.dependencies[]?.name == "stoolap" and .dependencies[]?.source == "git+https://github.com/CipherOcto/stoolap?rev=<sha>") | .name'` — returns the list of consumers-of-frozen-rev (rejects any name ≠ `octo-storage-core`). **(forward requirement)**
5. **TV-0205-05:** CI verifies frozen `rev` is an **ancestor of** `feat/blockchain-sql` branch tip AND that `octo-stoolap-frozen-vN` tag points to exactly the same commit as the `rev = "<sha>"` in workspace `Cargo.toml`. Implemented as: (a) `git merge-base --is-ancestor <rev> origin/feat/blockchain-sql` (rebase history-rewrites drop the frozen rev → fail), and (b) `git rev-parse <tag>` exits 0 AND equals `<rev>` byte-equal (tag-deleted-not-recreated → fail with `frozen-tag-missing`; force-push retargeting the tag to a non-frozen commit → fail with `frozen-tag-mismatch`). Both checks must pass. Tag-numbering convention: `octo-stoolap-frozen-v{N}` where N is monotonically increasing integer beginning at 0; the mapping `rev → tag` is recorded in `Cargo.toml` and consumed by the CI script. **(forward requirement)**
6. **TV-0205-06:** Monthly re-cert checklist passes (script: `scripts/stoolap_recert.sh` — **NEW** per §Implementation Phases Phase 1 Task 4; not yet present on disk; TV gates once script exists). Required script steps: (i) frozen-rev byte-verify vs `git rev-parse octo-stoolap-frozen-vN`; (ii) `cargo tree -p octo-storage-core` shows the frozen rev; (iii) CVE scan of fork rev via `gh advisory` cross-ref; (iv) CI green check; (v) audit log entry.

## Alternatives Considered

| Approach                                        | Pros                           | Cons                                                           |
| ----------------------------------------------- | ------------------------------ | -------------------------------------------------------------- |
| Option A: Single fork, no split                 | Simpler dependency graph       | Layer A substrate inherits fork volatility; layering violation |
| Option B: Vendor fork as octo-stoolap fork copy | No upstream merge pressure     | Maintenance burden; divergence from upstream features          |
| Option C: Adopt upstream Stoolap directly       | Zero fork maintenance          | Lose DQA(scale), MVCC DQA, lexicographic codec; major rewrite  |
| **Option D: Two-tier split (adopted)**          | Layer A frozen; Layer B active | Requires steward + re-cert discipline                          |

## Implementation Phases

### Phase 1: Initial Freeze

- [ ] Task 1: Pin `feat/blockchain-sql` HEAD to `<sha-0>`; tag `octo-stoolap-frozen-v0`
- [ ] Task 2: Update workspace `Cargo.toml` `[patch.crates-io]` block per §Cargo.toml Pinning (replace `branch = "feat/blockchain-sql"` with `rev = "<sha-0>"`)
- [ ] Task 3: Add CI graph audit gate per TV-0205-04 (rejects any consumer of frozen rev other than `octo-storage-core`)
- [ ] Task 4: Add `scripts/stoolap_recert.sh` (per TV-0205-06 step list) + monthly cron + `docs/runbooks/stoolap-steward.md` (per §Key Files checklist)

### Phase 2: Merge-to-Upstream Plan

- [ ] Task 5: File sub-mission `stoolap-fork-merge-to-upstream.md` (out of scope for this RFC; tracked separately)
- [ ] Task 6: Quarterly review of fork divergence metric

## Key Files to Modify

| File                                  | Change                                                                                                                                                                                                                             |
| ------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `Cargo.toml` (workspace)              | Replace `[patch.crates-io] stoolap = { ..., branch = "feat/blockchain-sql" }` with `rev = "<sha-0>"`; the upstream `stoolap` crate name is unchanged (the fork repo's `Cargo.toml` package name stays `stoolap`).                  |
| `crates/octo-storage-core/Cargo.toml` | No change — already depends on `stoolap` (active fork); the workspace `[patch.crates-io]` redirects at build time.                                                                                                                 |
| `.github/workflows/ci.yml`            | Add graph audit gate (TV-0205-04) + frozen-rev byte-verify (TV-0205-05) + skip-check on `main` until v0 freeze exists (forward-requirement handling)                                                                               |
| `scripts/stoolap_recert.sh`           | NEW — monthly re-cert checklist (5 steps per TV-0205-06)                                                                                                                                                                           |
| `docs/runbooks/stoolap-steward.md`    | NEW — steward operating manual (content checklist: pre-v0 daily-watch protocol; freeze procedure with `git tag -s` signing; CVE bypass 24 h audit-log format; merge-base / tag-match CI gate operation; quarterly review template) |

## Future Work

- Sub-mission `stoolap-fork-merge-to-upstream.md` (`to be filed`) for upstream contribution strategy.
- Sub-mission `octo-stoolap-frozen-release-process.md` (`to be filed`) for tagging + signing convention.
- Per-Phase-2 quarterly split review audits.

## Version History

| Version | Date       | Author     | Changes                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                            |
| ------- | ---------- | ---------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1.0     | 2026-08-19 | @mmacedoeu | Initial draft. Two-tier split per review §8.1.7; release-tag pin policy table; Cargo.toml pinning.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                 |
| 1.1     | 2026-08-19 | @mmacedoeu | Round 1 review fixes: `Cargo.toml:156` line ref → `Cargo.toml` `[patch.crates-io]` block; phantom `RFC-0001`/`RFC-0008` → `BLUEPRINT.md` ref + inline Operation Class Mapping; Two-Tier ASCII → Mermaid graph; "monthly re-cert" trigger clarified (30 days from last freeze, not calendar-month 1st); TV-0205-05 "reachable from" → "ancestor of" + `git merge-base --is-ancestor`; TV-0205-06 marked as **NEW** (script not yet on disk); Roles Source/Ref column updated to precise §names; Layer self-declaration added to Status; Adversary Q1 mitigation clarified (HW security key held by separate person, not GitHub team); Future Work phantom mission pointers marked **to be filed**. Doc accuracy only — no spec change.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                              |
| 1.2     | 2026-08-19 | @mmacedoeu | Round 2 review fixes: Monthly re-cert row gained bootstrap clause (pre-v0 daily-watch on `feat/blockchain-sql` with 24 h RFC-reviewer escalation; SLA column split to "30 days post-v0 / daily-watch pre-v0"); TV-0205-05 expanded with tag-match check (`git rev-parse <tag> == <rev>` byte-equal — defends against force-push retargeting the tag to a non-frozen commit); `docs/runbooks/stoolap-steward.md` content checklist enumerated (pre-v0 daily-watch protocol; freeze procedure with `git tag -s` signing; CVE bypass 24 h audit-log format; merge-base / tag-match CI gate operation; quarterly review template); Future Work `to be filed` markers backticked. Doc accuracy only — no spec change.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                   |
| 1.3     | 2026-08-19 | @mmacedoeu | Round 3 deep-dive reviewer fixes: §Summary, §Motivation, §Roles, §Two-Tier Architecture Mermaid, §Implicit Assumptions row 3, §Security Considerations Layer-A-drift row, §Operation Class Mapping row 1 — all `octo-determin` → `octo-storage-core` (verified via `crates/octo-storage-core/Cargo.toml` is the actual sole direct consumer of the fork; `octo-determin` has zero stoolap deps); §Cargo.toml Pinning Layer A template rewritten from nonexistent `[dependencies.octo-stoolap-frozen]` to the actual mechanism — workspace `[patch.crates-io] stoolap = { git = "...", rev = "<sha>" }` redirect (the fork repo's `Cargo.toml` declares `name = "stoolap"`, no `octo-stoolap-frozen` crate exists); §Design Goals G1 metric rewritten to match mechanism; §Adversary Row 1 Q4 cites TV-0205-05 explicitly (was implicit); §Release-Tag Pin Policy gained "CRITICAL CVE disclosed mid-cycle" row (ice-until-patch semantics, v{N+1} bump RFC + TV-0205-05 gate, no consumer migration); §Operation Class Mapping gained mid-cycle CVE bump row (Class A, same-layer); §Test Vectors §General rule reframed (was "No byte-exact TV — this is a governance RFC. Verification is structural. TV-0205-01..05 are forward requirements"; all 6 entries updated to reference `[patch.crates-io]` redirect + `octo-storage-core` + backward slug path); §Implementation Phases Phase 1 Task 2 rephrased to `[patch.crates-io]` block edit; §Key Files to Modify — `crates/octo-storage-core/Cargo.toml` row added (no change; documents why); §Compatibility Backward reframed to "pre/post-RFC-0205 workspace `[patch.crates-io]` frozen redirect" (the upstream `stoolap` crate is patched to the fork rev at build time); §Compatibility Forward gained drop-fork scenario clause (Layer B branch pin → crates-io semver migration). Doc accuracy only — no spec change. |
