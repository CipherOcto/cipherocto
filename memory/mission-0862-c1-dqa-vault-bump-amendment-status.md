---
name: mission-0862-c1-dqa-vault-bump-amendment-status
description: S6c RFC-0862 v2.0 (Dqa + vault bump) + TV-0862-01..08 byte-exact spend_ledger fixtures — CLAIMED 2026-08-17 (pending LANDED)
metadata:
  type: project
---

# S6c — RFC-0862 v2.0 Dqa + vault bump (CLAIMED 2026-08-17)

Mission `0862-c1-dqa-vault-bump-amendment` claimed. Third S6
sub-session per user split-by-RFC decision (after S6a RFC-0870 +
S6b RFC-0957).

## Scope

- **RFC-0862 v1.4.0 → v2.0** amendment
  (`rfcs/accepted/networking/0862-writer-election-bootstrap-v130.md`):
  - §Version History v2.0 row added
  - §SpendLedger Substrate subsection added (after §DrainCoordinator)
  - `StoolapSpendLedger` impl surface
  - Dqa wire-format bump (16-byte BE DqaEncoding per RFC-0105 v1.9)
  - Vault substrate integration (per RFC-0960 §20.3 vault_id derivation)
- **TV-0862-01..08** byte-exact fixtures in
  `crates/quota-router-storage/tests/tv_0862_spend_ledger.rs` (NEW)

## Pre-reqs (all LANDED 2026-08-17)

- S3 (octo-vault crate) — LANDED
- S4 (Dqa codemod) — LANDED (`19faf380` + `4ab400bd` + `18edbe0d`)
- S5 (verify-time invariant) — LANDED (`d007de54`)
- S6a (RFC-0870 + 1 TV) — LANDED (`c7f99a47` + `ab2b57b4` + Round 2)
- S6b (RFC-0957 + 22 TV) — LANDED (`c9149128` + `4ec9779f` +
  `e5138420` + `57676533`)

## Plan reference

- `docs/plans/2026-08-16-storage-layer-restructuring-execution-plan.md`
  §3 row 6 (Stream A.1 S6c) + §4 S6 verify gate (8 spend_ledger TV)

## Next steps

1. Read RFC-0862 v1.4.0 §DrainCoordinator + line 1629 + 1730 + 1801
   to confirm v2.0 amendment context
2. Read `crates/octo-determin/src/dqa_encoding.rs` to confirm Dqa
   wire-form before drafting amendment text
3. Read `crates/quota-router-storage/src/stoolap_spend_ledger.rs`
   schema to confirm TV-0862-01..08 surface
4. Draft RFC-0862 v2.0 row + §SpendLedger Substrate subsection
5. Write `crates/quota-router-storage/tests/tv_0862_spend_ledger.rs`
   (8 byte-exact TV)
6. Run verify gate per plan §4 S6
7. Run prettier + cargo fmt + cargo clippy
8. Commit
9. Multi-round adversarial review (loop-until-dry)
10. Land mission status

## Push authorization

Commits queued on `next` await user go-ahead per
[[feedback_initiative_user_only]] + [[git-workflow]].

## Cross-reference

- Plan: `docs/plans/2026-08-16-storage-layer-restructuring-execution-plan.md`
  §3 row 6
- Mission: `missions/open/0862-c1-dqa-vault-bump-amendment.md`
  (status: CLAIMED)
- Pre-reqs: `memory/mission-0957-g-verify-time-invariant-status.md` (S5)
  - `memory/mission-0870-c1-version-tag-amendment-status.md` (S6a)
  - `memory/mission-0957-c1-verify-time-amendment-status.md` (S6b)
- Review source: `docs/reviews/2026-08-15-storage-layer-restructuring-analysis.md`
  §14.1 + §20.6.1 + §20.3
