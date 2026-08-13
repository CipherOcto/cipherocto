---
name: marketplace-round-1-review-status
description: Marketplace 3-pass code review status — 3 CRITICAL + 2 HIGH fixes landed (commit 264e2665), 6 architectural follow-ons filed (commit caa1cbfa)
metadata:
  type: project
---

Multi-pass marketplace review (RFC-0900 Marketplace + RFC-0901 Task Market + RFC-0968 Reputation).

**Round 1 landed (commit `264e2665`):**
- C1 orderbook seq-key collision: `(price, ts_unix)` → `(price, next_seq)` per-book counter
- C2 orderbook partial-fill residual re-insert after partial fill
- C3 Escrow + TaskEscrow: dropped `#[derive(Clone)]`; added `EscrowSnapshot`/`TaskEscrowSnapshot` view types
- H2 escrow cancel(): removed; state diagram corrected
- H3 slashing f64 → u128 fixed-point penalty math
- 3 reputation_compat tests (RFC-0968 retirement gate coverage)

Verified: 73 marketplace lib tests + 76 E2E tests = 149 pass. Clippy clean.

**6 architectural follow-ons filed (commit `caa1cbfa`):**
- `marketplace-escrow-caller-authorization` — Party enum + auth-gated transitions (12 ACs)
- `marketplace-repo-trait-decouple` — AskRepository → trait (12 ACs)
- `marketplace-facade-reputation-async-migration` — async migration + retirement gate (11 ACs)
- `marketplace-book-load-on-open` — hydrate book from repo on restart (8 ACs)
- `marketplace-slash-reason-typed-discriminator` — enum → type_id + registry (12 ACs)
- `marketplace-slashing-persistence` — SlashStore trait + stoolap-backed impl (9 ACs)

**Why:** Write boundary enforce caller identity [Party]; stable abstractions have
trait boundary (not concrete struct); extension-bearing types use typed-discriminator
not enum (CLAUDE.md §Extension over enumeration); production state survives restart.

**How to apply:** When picking next marketplace mission, follow-on family is the
priority queue. Pair H1+H2 (escrow auth + facade migration) so consumer API
churn is one event. Pair H4+M2 (typed-discriminator + persistence) since both
touch `slashing.rs`.

Related: [[mission-0871b-cross-domain-resolution-impl-status]] (recent LANDED pattern),
[[cipherocto-design-principles]] (extension-over-enumeration, stable abstractions).
