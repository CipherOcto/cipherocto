# RFC-0205 (Storage): Stoolap Fork Stability Certification

## Status

**Version:** 1.0 (2026-08-19)
**Status:** Draft

## Authors

- Author: @mmacedoeu

## Maintainers

- Maintainer: Stoolap steward team (per plan §3 A.2)
- Co-maintainer: octo-determin owner

## Summary

Formalizes a two-tier split of the Stoolap fork substrate: a **Layer A frozen snapshot** (`octo-stoolap-frozen`, commit-pinned, years-stable) consumed only by `octo-determin`, and a **Layer B active fork** (branch `feat/blockchain-sql`) consumed by `octo-storage` and downstream storage crates. Defines release-tag pin policy, monthly + quarterly re-certification schedule, and Cargo.toml pinning discipline. Closes the §8.1.7 HIGH blocker from the storage restructure review.

## Dependencies

**Requires:**

- RFC-0105 (Numeric): Deterministic Quant Arithmetic — DQA substrate consumed by the frozen fork
- RFC-0010 (Process): Canonical DID Codec — chain_id namespace depends on fork wire form
- RFC-0870 (Process): Node envelope version_tag — pins the on-wire tag version that the frozen fork must match

**Optional:**

- RFC-0900 (Economics): Chain-aware slash ledger — depends on fork's DQA(scale) codec

> **Dependency Validation Rules:** All upstream RFCs are Accepted. This RFC introduces no new layer-A dependency; it constrains how an existing Layer B dependency (the Stoolap fork) is layered.

## Design Goals

| Goal | Target                 | Metric                                                                                |
| ---- | ---------------------- | ------------------------------------------------------------------------------------- |
| G1   | Zero silent fork drift | `octo-stoolap-frozen` rev matches `Cargo.toml:stoolap.rev` byte-for-byte in CI        |
| G2   | ≤ 7-day CVE response   | Time from upstream CRITICAL CVE announcement to patched `octo-stoolap-frozen` release |
| G3   | Monthly re-cert green  | Re-cert checklist passes every 30 days                                                |
| G4   | Quarterly split review | Layer A vs Layer B boundary review every 90 days                                      |

## Motivation

`docs/reviews/2026-08-15-storage-layer-restructuring-analysis.md` §8.1.7 audit identified that the Stoolap fork (workspace `Cargo.toml:156`, `branch = "feat/blockchain-sql"`) is **NOT CERTIFIED** for Layer A stability:

- No release tag, no commit SHA pin
- Active fork maintained by CipherOcto team; merge-to-upstream plan not started
- Layer A requires years-stable primitives; an un-pinned moving fork is a layering contradiction

The Stoolap fork hosts critical substrate features consumed across the chain: `DQA(scale)`, `Value::quant`, `as_dqa`, `encode_decimal_lexicographic`, 16-byte MVCC DQA extension. Until certified, DQA-as-SQL columns are additive Layer B (RFC-driven), not Layer A (RFC-frozen).

**Solution:** Adopt a two-tier split per §8.1.7. `octo-determin` (Layer A) consumes a frozen commit-pinned snapshot. `octo-storage` + downstream crates (Layer B) consume the active fork. The split is enforced by Cargo.toml dependency direction, not by convention.

## Roles and Authorities

1. **Stoolap steward team** — owns the fork; performs freeze + re-cert; files RFCs to bump the frozen rev.
2. **octo-determin owner** — sole Layer A consumer of `octo-stoolap-frozen`; flags fork-side changes that need an upstream merge.
3. **RFC reviewer** — signs off on rev bumps and emergency CVE bypasses.
4. **On-call security** — co-signs emergency CVE bypasses per release-tag pin policy table.

| Role                | Identifier                          | Authority Scope                                | Lifecycle                 | Source/Ref                       |
| ------------------- | ----------------------------------- | ---------------------------------------------- | ------------------------- | -------------------------------- |
| Stoolap steward     | GitHub team `@stoolap-stewards`     | Fork maintenance, freeze, re-cert              | Active until role revoked | RFC-0205 §Specification          |
| octo-determin owner | GitHub team `@octo-determin-owners` | Layer A consumption, fork-side change flagging | Active until role revoked | RFC-0205 §Specification          |
| RFC reviewer        | RFC process role                    | Rev bump approval, CVE bypass co-sign          | Per-RFC                   | RFC-0001 §Mission Lifecycle      |
| On-call security    | Rotation role                       | 24-hour CVE bypass co-sign                     | Per-incident              | RFC-0205 §Release-Tag Pin Policy |

## Specification

### Two-Tier Architecture

```text
Layer A (years-stable, RFC-frozen):
  - crates/octo-determin: DQA substrate (already certified per review §8.1.1)
  - Stoolap fork pinned to commit SHA; frozen as octo-stoolap-frozen
    crate dependency; cannot be upgraded without RFC + major version bump

Layer B (RFC-driven, additive):
  - Active Stoolap fork at feat/blockchain-sql
  - Used by crates/octo-storage and consumer crates via Cargo.toml dep
```

**Dependency direction rule (enforced by `cargo metadata` audit + CI gate):**

- `octo-determin` MAY depend on `octo-stoolap-frozen`.
- `octo-determin` MUST NOT depend on the active fork (`stoolap`).
- `octo-storage` and downstream crates MAY depend on the active fork (`stoolap`).
- `octo-storage` and downstream crates MUST NOT depend on `octo-stoolap-frozen`.
- Cross-tier dependency = layering violation; rejected at CI.

### Cargo.toml Pinning

**Layer A (workspace root or `octo-determin/Cargo.toml`):**

```toml
# Layer A — frozen snapshot
[dependencies.octo-stoolap-frozen]
git = "https://github.com/CipherOcto/stoolap"
rev = "<commit-sha-when-frozen>"
# Used by octo-determin only; never by consumer crates directly
# Bump policy: see §Release-Tag Pin Policy
```

**Layer B (`octo-storage/Cargo.toml`, `quota-router-storage/Cargo.toml`, etc.):**

```toml
# Layer B — active fork
[dependencies.stoolap]
git = "https://github.com/CipherOcto/stoolap"
branch = "feat/blockchain-sql"
# Used by octo-storage, quota-router-storage
```

### Release-Tag Pin Policy

| Trigger                        | Action                                                                                                                             | Owner                              | SLA                                      |
| ------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------- | ---------------------------------------- |
| Initial freeze                 | Pin `rev = "<sha>"` of `feat/blockchain-sql` HEAD; tag `octo-stoolap-frozen-v0`                                                    | Stoolap steward team               | one-time                                 |
| Upstream Stoolap major release | File RFC to bump `octo-stoolap-frozen` rev; security audit + RFC-major bump required                                               | Stoolap steward + RFC reviewer     | 7 days for CRITICAL CVE; 90 days for LOW |
| For-Layer-A consumer request   | `octo-determin` only; never directly consumed by other crates                                                                      | octo-determin owner                | 30 days                                  |
| Emergency CVE bypass           | `octo-determin` may consume active fork temporarily with audit log entry per consumption                                           | on-call security + Stoolap steward | 24 hours for CRITICAL CVE                |
| Monthly re-cert                | Re-certify the frozen commit-applies checklist (CI green, security audit clean, no upstream regressions in the dependency surface) | Stoolap steward                    | 30 days                                  |
| Quarterly re-cert              | Full review of the two-tier split, cargo-pinning health, merge-to-upstream progress                                                | Stoolap steward + RFC reviewer     | 90 days                                  |

### Determinism Requirements

- `octo-stoolap-frozen` MUST be byte-for-byte reproducible at `rev = "<sha>"`.
- DQA(scale) wire form (16-byte BE) MUST NOT change across re-cert without an RFC bump.
- `encode_decimal_lexicographic` byte output MUST be pinned across re-cert.

### RFC-0008 Execution Class Mapping

| Operation              | Class | Rationale                                 |
| ---------------------- | ----- | ----------------------------------------- |
| Freeze + rev-pin       | A     | Layer A substrate; years-stable           |
| Monthly re-cert        | C     | Operational; no consensus impact          |
| Emergency CVE bypass   | C     | Operational; gated by audit log + co-sign |
| Quarterly split review | C     | Governance; no consensus impact           |

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

| Assumption                                 | Where Relied Upon                         | Blast Radius if False                                      | Mitigation / Status                                                                                                                         |
| ------------------------------------------ | ----------------------------------------- | ---------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------- |
| Fork maintainer availability               | §Release-Tag Pin Policy (monthly re-cert) | Frozen rev falls behind upstream; CVE unpatched            | Backup steward + monthly SLA escalation; ACCEPTED RISK                                                                                      |
| Upstream Stoolap does not delete fork repo | §Cargo.toml Pinning                       | `Cargo.toml` rev points to 404                             | Mirror to internal registry; quarterly verify                                                                                               |
| `octo-determin` is sole Layer A consumer   | §Two-Tier Architecture                    | If another crate consumes frozen fork → layering violation | CI gate: `cargo metadata --format-version 1 \| jq '.packages[] \| select(.name == "octo-stoolap-frozen") \| .dependencies'` reverse-DB scan |
| DQA wire form stable across re-cert        | §Determinism Requirements                 | Settlement replay diverges                                 | Pinned at RFC-0105; bump = RFC-major                                                                                                        |

### Categories to Audit

- **Operator trust** — Stoolap steward team is trusted; compromise → MITM via poisoned SHA. Mitigation: RFC reviewer co-sign on rev bump; CI verifies `Cargo.toml` rev matches git tag.
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
- **Layer A drift** — `octo-determin` accidentally consumes active fork. Mitigation: CI graph audit.
- **Replay attacks** — fork replay of old DQA wire form across re-cert. Mitigation: wire form pinned at RFC-0105; bump = RFC-major.

## Adversary Analysis

| Decision               | Q1 Beneficiary                          | Q2 Cost to Attacker        | Q3 Gain if Successful                        | Q4 Defense (cost to legit op)                                        | Q5 Residual Risk                   |
| ---------------------- | --------------------------------------- | -------------------------- | -------------------------------------------- | -------------------------------------------------------------------- | ---------------------------------- |
| Pin fork to commit SHA | Compromised steward                     | Steward account compromise | Inject malicious DQA codec → consensus split | RFC reviewer co-sign + git tag signature + CI byte-verify (low cost) | LOW — multi-party co-sign required |
| Two-tier split         | Fork maintainer pushing breaking change | Reputation cost            | Force Layer A to upgrade early               | CI rejects cross-tier deps (low cost)                                | LOW — automatic gate               |
| Monthly re-cert        | Lazy steward                            | None (passive)             | CVE goes unpatched                           | 30-day SLA; escalation to RFC reviewer (low cost)                    | MED — depends on steward vigilance |

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

- **Backward:** DQA wire form unchanged (pinned at RFC-0105); pre-RFC-0205 forks resolve identically to post-RFC-0205 `octo-stoolap-frozen`.
- **Forward:** RFC bump of `rev` is the only change vector; new RFCs may amend §Release-Tag Pin Policy table.

## Test Vectors

No byte-exact TV — this is a governance RFC. Verification is structural:

1. **TV-0205-01:** `Cargo.toml` workspace root has `octo-stoolap-frozen` with `rev = "<sha>"` (not `branch`).
2. **TV-0205-02:** `octo-determin/Cargo.toml` depends on `octo-stoolap-frozen`, not `stoolap`.
3. **TV-0205-03:** `octo-storage/Cargo.toml` depends on `stoolap` (active fork), not `octo-stoolap-frozen`.
4. **TV-0205-04:** CI graph audit rejects any other crate depending on `octo-stoolap-frozen`.
5. **TV-0205-05:** CI verifies frozen `rev` resolves to a commit reachable from `feat/blockchain-sql` branch tip.
6. **TV-0205-06:** Monthly re-cert checklist passes (script: `scripts/stoolap_recert.sh`).

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
- [ ] Task 2: Update workspace `Cargo.toml` + `octo-determin/Cargo.toml` per §Cargo.toml Pinning
- [ ] Task 3: Add CI graph audit gate (rejects cross-tier deps)
- [ ] Task 4: Add `scripts/stoolap_recert.sh` + monthly cron

### Phase 2: Merge-to-Upstream Plan

- [ ] Task 5: File sub-mission `stoolap-fork-merge-to-upstream.md` (out of scope for this RFC; tracked separately)
- [ ] Task 6: Quarterly review of fork divergence metric

## Key Files to Modify

| File                               | Change                                                       |
| ---------------------------------- | ------------------------------------------------------------ |
| `Cargo.toml` (workspace)           | Add `octo-stoolap-frozen` workspace dep with `rev = "<sha>"` |
| `crates/octo-determin/Cargo.toml`  | Replace `stoolap` dep with `octo-stoolap-frozen`             |
| `.github/workflows/ci.yml`         | Add graph audit gate + frozen-rev byte-verify                |
| `scripts/stoolap_recert.sh`        | NEW — monthly re-cert checklist                              |
| `docs/runbooks/stoolap-steward.md` | NEW — steward operating manual                               |

## Future Work

- Sub-mission `stoolap-fork-merge-to-upstream.md` for upstream contribution strategy.
- Sub-mission `octo-stoolap-frozen-release-process.md` for tagging + signing convention.
- Per-Phase-2 quarterly split review audits.

## Version History

| Version | Date       | Author     | Changes                                                                                            |
| ------- | ---------- | ---------- | -------------------------------------------------------------------------------------------------- |
| 1.0     | 2026-08-19 | @mmacedoeu | Initial draft. Two-tier split per review §8.1.7; release-tag pin policy table; Cargo.toml pinning. |
