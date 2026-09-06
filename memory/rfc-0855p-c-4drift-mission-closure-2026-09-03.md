---
name: rfc-0855p-c-4drift-mission-closure-2026-09-03
description: RFC-0855p-c 4-drift-mission closure + 0x000F collision resolved INLINE 2026-09-03
metadata:
  node_type: memory
  type: project
  originSessionId: d23cf564-d553-4e7d-be82-070883125eed
  modified: 2026-09-03T22:15:00.000Z
---

RFC-0855p-c 4 drift missions ALL CLOSED 2026-09-03 at `next` HEAD. Substrate totals: **992 LoC / 38 tests** across `dc/{slash,rejoin,discipline,sub_admin}.rs`. Plus `dc/slash_bridge.rs` orphan (RFC-0862 sibling, 239 LoC / 9 tests).

| Mission            | Substrate          | Tests | Verdict                |
| ------------------ | ------------------ | ----- | ---------------------- |
| auto-rejoin        | `dc/rejoin.rs`     | 9     | ✅ CLOSED              |
| slash-small-groups | `dc/discipline.rs` | 7     | ✅ CLOSED              |
| sub-admins         | `dc/sub_admin.rs`  | 9     | ✅ CLOSED              |
| cross-domain-slash | `dc/slash.rs`      | 13    | ✅ CLOSED (0x0100 fix) |

## 0x000F collision resolution (0x000F → 0x0100)

**Prior state:** `dc/slash.rs:23` claimed `DC_SLASH_REASON_DOMAIN_COORDINATOR_MISBEHAVIOR = 0x000F`. But `SlashReasonCode::CgGroupSpam = 0x000F` is canonical per RFC-0855p-b §Appendix B + RFC-0850p-d §"Slash Reason Codes Added" (Layer B RFC-frozen).

**Resolution:** Reassigned to `0x0100` (first user-extension-registry slot). Per CLAUDE.md §Architectural Principles §"Extension over enumeration", the extension namespace `0x0100-0xFFFF` is the canonical home for RFC-allocated slash reasons that lack typed variants; `SlashReasonCode::Extension(0x0100)` returns from `try_from_reason_id` and `from_reason_id`. Typed semantics live in `octo_network::dc::slash::DcMisbehavior` (substrate adapter).

**Why NOT 0x0013 (R5-OOS-4 reviewer recommendation):** F-7 (RFC-0855p-e) already allocates `0x0013-0x0016` to `FalseAttestation` / `QuorumTimeout` / `TallyTamper` / `LateDelivery`. Shifting F-7 to free 0x0013 would cascade through RFC-0855p-e + multiple test vectors — out of scope per user direction "doc updates, RFCs and missions, should be in place, not separated amendments".

**Files updated (inline, single coherent change set):**

- `crates/octo-network/src/dc/slash.rs` — const + module doc + 13 unit test sites + error doc string (0x000F → 0x0100)
- `crates/octo-network/src/dc/mod.rs:9` — module doc reference
- `crates/octo-network/src/mon/slash.rs:11` — module doc reserved range + new allocation + extension registry
- `crates/octo-coordinator-types/src/lib.rs` — slash reason code doc table row + Extension safety note
- `rfcs/accepted/networking/0855p-c-domain-coordinator-role.md` — §9b allocation block + NEW §9c "Cross-Domain Slash" + §Adversary Analysis + VH row v0.1.3 (closes R5-OOS-4)
- `rfcs/accepted/networking/0855p-b-coordinator-lifecycle.md:966` — §B table 5-row split (F-7 + rejected + DC misbehavior + extension registry)
- `rfcs/draft/networking/0850p-d-dc-initiated-group-creation.md` — preamble + new "Note on 0x0100" paragraph
- `missions/archived/completed/0855p-c-cross-domain-slash.md` — §Design §1 rewrite + AC #1 + Type Coverage table + §Notes "Why 0x0100?" rationale

**Verification:**

- `cargo test -p octo-network --lib dc::slash::tests` — 13/13 PASS
- `cargo test -p octo-network --lib dc::` — 93/93 PASS
- `grep -rn "0x000F" crates/octo-network/src/dc/` — 0 matches; remaining 0x000F residue in `dot/{slash,domain,handover,dc_envelopes}.rs` is CgGroupSpam/PlatformType::Twitter (separate namespaces, unrelated)
- ⚠ Pre-existing clippy error in `dot/subgroup_state.rs:230` — UNRELATED, user-owned per `feedback_initiation_user_only`

## Bookkeeping landed (single commit)

`git mv` all 4 missions `missions/open/ → missions/archived/completed/`:

- 0855p-c-cross-domain-slash.md (rewritten + collision-fixed)
- 0855p-c-auto-rejoin.md
- 0855p-c-slash-small-groups.md
- 0855p-c-sub-admins.md

Combined with the previous 3-mission closure (`de9bc5f7`), all 7 RFC-0855p-c missions are now in `archived/completed/`.

## Cross-RFC invariants closed

- [x] `RecorderDid` canonical keying (RFC-0968 §28.4 amendment 22)
- [x] `HARD_THRESHOLD = 5` byte-identical shared between `slash_store.rs:36` (0855p-b) + `dc_store.rs:40` (0855p-c)
- [x] `BindEnvelope::canonical_bytes()` includes `member_count_at_bind` — security property
- [x] `bitflags` serde feature-gate avoided via manual serde impl
- [x] Defensive guards: empty peer_id, 3rd+ strike → UNBIND, 2/3 quorum
- [x] `SlashReasonCode` canonical enum preserves 0x000F = `CgGroupSpam`; DC misbehavior now at 0x0100 extension namespace
- [x] Sync slash code range `0x0020..=0x0023` separate (RFC-0862 namespace)

Closure audit at `docs/audits/2026-09-03-0855p-c-4drift-mission-hard-audit.md` (scratchpad per [[docs-audits-scratchpad]]).

**Why:** 7/7 RFC-0855p-c missions closed; 0x000F collision resolved INLINE (per user direction "in place, not separated amendments"); cross-RFC invariants preserved; extension-registry placement honors CLAUDE.md §Extension over enumeration.

**How to apply:** User owns `git push origin next` + PR `next → main` for the fix + bookkeeping commit. NO PUSH from agent.

Related: [[cipherocto-design-principles]] (§Layer model + §Extension over enumeration + §No central enums), [[feedback-no-fabricated-commit-rule]], [[never-guess]], [[docs-audits-scratchpad]], [[git-workflow]], [[feedback_initiation_user_only]], [[rfc-0855p-b-substrate-closure-2026-09-03]] (collided canonical enum), [[rfc-0855p-c-3mission-closure-2026-09-03]] (3 prior closed missions).
