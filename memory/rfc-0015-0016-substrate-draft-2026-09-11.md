---
name: rfc-0015-0016-substrate-draft-2026-09-11
description: 2 substrate RFC drafts landed as follow-on chain to RFC-0011-c + RFC-0011-a; substrate unblocks 5 RFC-0011 follow-on claimed missions
metadata:
  type: project
---

`next` HEAD `35c1e166`. 1 commit. 2 new draft RFCs drafted per user pick "Both RFCs back-to-back".

- `rfcs/draft/process/0015-wallet-agent-operations.md` (~410 lines) — additive `list_owned_agents` + `transition_agent` + `WalletError::AgentNotFound` on `octo-wallet` Layer B façade per RFC-0011-c follow-on chain.
- `rfcs/draft/process/0016-audit-receipt-api.md` (~535 lines) — additive `list_receipts` + `get_receipt` + `append_audit_event` + `audit_home` + 4 supporting types on `octo-audit` Layer B façade per RFC-0011-a follow-on chain + audit-append consumer requirement from RFC-0011-c `0011-c-agent-destroy-subcommand`.

Substrate-faithful drift notes documented in both RFCs:
- RFC-0015: 3-state `AgentState { Registered, Running, Terminated }` vs RFC-0002 spec 5-state (ACTIVE/BUSY collapsed into Running).
- RFC-0016: `ReceiptStatus` 3-variant form (Ok/Partial/Reject) vs RFC-0011-a v1.5 "Unknown" Raw escape hatch.

Cite sweep clean: RFC-0015 77/77, RFC-0016 155/155 after 15 cite repairs (12 §section anchors matched to verified headings + 3 multi-occurrence `replace_all` fixes).

Prettier clean on both.

How to apply: Both Draft RFCs unlock 5 RFC-0011 follow-on claimed missions:
- `0011-c-agent-list-subcommand` — unblocked by RFC-0015 `list_owned_agents`.
- `0011-c-agent-{run,attach}-subcommand` — unblocked by RFC-0015 `transition_agent`.
- `0011-c-agent-destroy-subcommand` — unblocked by RFC-0015 `transition_agent` + RFC-0016 `append_audit_event` + `ReceiptStatus`.
- `0011-a-audit-commands` — unblocked by RFC-0016 `list_receipts` + `get_receipt` + `AuditFilter` + `ReceiptId` + `ReceiptStatus` + `ReceiptSummary`.

Next draft-→Accepted gates:
1. DRY review (5-len parallel) per [[feedback-5-len-dry-pattern]].
2. User owns push to `origin/next` + PR per [[git-workflow]] + [[feedback_initiation_user_only]].
3. RFC-0015 + RFC-0016 promote Draft → Accepted same cycle.
4. Mission YAMLs `0011-c-agent-{list,run,attach,destroy}` and `0011-a-audit-commands` become implementable (were blocked on substrate-gap annotations per tasks #626/#633).

Related: [[no-phantom-mission-pointers]] [[memory-is-never-status-ground-truth]] [[cipherocto-design-principles]] [[no-line-refs-anywhere]]
