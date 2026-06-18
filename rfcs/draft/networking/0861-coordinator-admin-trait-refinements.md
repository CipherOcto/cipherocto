# RFC-0861 (Networking): CoordinatorAdmin Adapter Contract Refinements

## Status

Draft (2026-06-18)

## Authors

- @mmacedoeu

## Maintainers

- @mmacedoeu

## Summary

Refines the `CoordinatorAdmin` trait and its platform-specific
implementations (WhatsApp, IRC) to close the 17 findings from
[`docs/reviews/coordinator-admin-impl-adversarial-review-r1.md`](../reviews/coordinator-admin-impl-adversarial-review-r1.md)
that were deferred from the R20/R21 implementation. Closes 3 HIGH
(H1, H2, H6) and 14 MEDIUM (M1, M2, M3, M4, M5, M7, M8, M10, M11,
M12, M13, M14, M15, M16) by tightening capability-report honesty,
adding input validation, defining error and partial-success
semantics, and bounding the doc-vs-code contract on each platform.

## Dependencies

**Required (must be Accepted or in Active Implementation):**

- RFC-0850 (Networking): Deterministic Overlay Transport — DOT
  envelope format
- RFC-0850p-a (Networking): WhatsApp Auth Onboarding — `GroupConfig`
  and the `WhatsAppWebAdapter` surface where the WhatsApp `CoordinatorAdmin`
  impl lives
- RFC-0850p-c (Networking): Transport Group Binding Ceremony — the
  `GroupBinding` / `GroupState` machinery that consumes
  `CoordinatorAdmin::create_group` and `join_by_invite` outputs
- RFC-0850p-d (Networking): DC-Initiated Transport Group Creation &
  Invite — uses `CoordinatorAdmin::create_group` and `add_member`

**Refines / Extends:** the `CoordinatorAdmin` trait defined in
`crates/octo-network/src/dot/adapters/coordinator_admin.rs` (R20, no
RFC of its own — this RFC also serves as the trait's defining spec,
filling the gap).

## Motivation

The R20 `CoordinatorAdmin` trait + R21 implementations were audited
across five rounds (R23c → R23d → R23e → R23f → R23g) of adversarial
review. The IRC implementation is now defensible (50 tests passing,
zero open findings). The 17 R1 findings listed below are **still
unaddressed**; per the project's "Deferred ≠ Unspecified" rule (see
memory `mem_1781647176929_4539827401334513900`), they cannot be
deferred without one of:

1. A full spec in this RFC (chosen approach), or
2. A new mission that implements them

This RFC is the chosen (1): a complete spec for all 17 findings,
referenced by one master mission.

### Use Case Link

- `docs/use-cases/mission-coordinator-lifecycle.md` — DC authority
  relies on the `CoordinatorAdmin` surface being honest about what it
  can do
- `docs/research/coordinator-admin-actions.md` — the per-platform
  matrix that motivates the trait

## Roles and Authorities

This RFC defines **no new roles or authorities**. It refines the
**contract** of the existing `CoordinatorAdmin` trait that the
`PlatformAdapter` implementations expose. The platform operators
(WhatsApp, IRC, etc.) and the DomainCoordinator role are unchanged
from RFC-0855p-c.

**Out-of-scope roles:**

- The platform operator (e.g. WhatsApp server admin) — out of scope
  for any DOT RFC
- Third-party group members who receive invites — out of scope
  beyond what's in RFC-0850p-d

## Specification

### 1. Capability-report honesty (closes H1, M10)

**Rule:** every bit in `AdminCapabilityReport` MUST truthfully
reflect whether the corresponding method is overridden with a
non-`Unimplemented` implementation. If the bit is set, calling the
method MUST NOT return `PlatformAdapterError::Unimplemented`. If the
bit is clear, the method MAY be overridden to return
`Unimplemented` (e.g. unimplemented even though the adapter opted in
to the trait) — but the bit MUST be clear.

**Per-adapter fixes:**

| Adapter | Bit | Old | New | Reason |
|---|---|---|---|---|
| WhatsApp | `can_join_by_invite` | `true` | `false` (or implement) | H1: bit was true, method was `Unimplemented` |
| WhatsApp | `can_join_by_invite` | (alternative) implement via `client.groups().join_with_invite_code(...)` | (alternative) | H1: SDK call exists per the existing comment |
| IRC | `can_join_by_id` | `false` | `true` (and add `join_by_id` method that wraps `join_by_invite`) | M10: `JOIN #chan` IS join-by-id; the bit is conservative-but-wrong |

**Decision policy:** for H1, choose the **alternative** (implement
`join_by_invite` on WhatsApp) — this is more useful and matches the
doc's claim. For M10, choose the bit-flip — IRC's `join_by_invite`
and `join_by_id` would be aliases of the same underlying `JOIN` IRC
command, and the bit should reflect that.

### 2. Input validation (closes M2, M15, M16)

**Rule:** every newtype constructor (`GroupId::new`, `PeerId::new`,
`InviteRef::new`) MUST add a `try_new` variant that rejects empty
strings. The existing `new` constructors remain infallible for
backward compatibility but SHOULD `debug_assert!` non-emptiness in
test builds.

**Per-adapter fixes:**

- **M15 (IRC):** `IrcConfig::validate()` (introduced for H7 in R23d)
  MUST also reject channel names that don't start with `#`, `&`, `+`,
  or `!`, and that contain CR/LF/NUL/space/comma/colon.
- **M16 (WhatsApp):** `WhatsAppConfig::validate()` MUST reject
  `groups` entries that contain `@` but don't end with `@g.us`
  (newsletter JID misuse) or that contain `:` (user JID misuse). The
  helper `group_to_jid` MUST be tightened to refuse non-numeric
  inputs without the `@g.us` suffix.
- **M2:** add `try_new` constructors; the existing `new` methods get
  `debug_assert!(!s.is_empty())`.

### 3. Error and partial-success semantics (closes H2, H6, M1, M4, M5, M13)

**H2 — WhatsApp `create_group` trait/inherent disambiguation.**
Rename the inherent `create_group` on `WhatsAppWebAdapter` to
`create_group_str` (mirroring the `leave_group_str` precedent at
`crates/octo-adapter-whatsapp/src/adapter.rs:1767-1796`). The trait
impl calls the unambiguous inherent. This removes the
infinite-recursion footgun that would silently activate if anyone
loosens the inherent's signature.

**H6 — WhatsApp `add_member` partial-success.** The trait method
signature MUST change from `Result<(), PlatformAdapterError>` to
`Result<AddMemberOutput, PlatformAdapterError>`:

```rust
pub struct AddMemberOutput {
    /// True if the platform confirmed the add.
    pub added: bool,
    /// The result of the optional promote (None if `is_admin`
    /// was false at the call site). `Err` means the add
    /// succeeded but the promote failed.
    pub promoted: Result<(), PlatformAdapterError>,
}
```

**M1 — WhatsApp `set_ephemeral` u64→u32 truncation.** Document on
the trait that adapters MAY clamp `ttl` to a platform-specific
maximum and SHOULD return
`PlatformAdapterError::ApiError { code: 400, message: ... }` on
overflow. The WhatsApp impl currently silently rounds; this fix adds
the explicit error path.

**M4 — WhatsApp `create_group` initial-admin promotion failure is
silent.** Add an `initial_admins_promoted: bool` field to
`GroupHandle` (defaulted via `#[serde(default)]` for backward
compatibility). The WhatsApp impl populates it from the
`promote_participants` call's result. The trait's
`is_admin: true` claim is documented to mean "the calling bot is
admin", not "all initial members are admin".

**M5 — WhatsApp `get_group_metadata` and `get_invite_link` errors
swallowed in `create_group`.** Replace `.ok()` with
`tracing::debug!` + continue; add a doc-comment on
`GroupHandle::invite_url` / `member_count` / `mode_flags` saying
"`None` means the platform did not surface it (either doesn't
support it or transient failure); callers needing strong guarantees
should call `get_group_metadata` separately".

**M13 — N+1 invite-URL materialization in `list_own_groups`.** Add
a new method `list_own_groups_with_invites(&self) ->
Result<Vec<GroupHandle>, PlatformAdapterError>` that materializes
the invite URLs in parallel using
`futures::future::join_all` (already a workspace dep). Keep the
existing `list_own_groups` for callers that don't need URLs. Document
the N+1 cost in the trait.

### 4. IRC `is_admin` / `add_member` honesty (closes M7, M8)

**M7 — IRC `add_member` (INVITE) doesn't require op status.** The
trait's doc-comment for `add_member` MUST be updated to add
`PlatformAdapterError::ApiError` semantics: adapters that receive
`ERR_CHANOPRIVSNEEDED` (or equivalent) from the platform MUST return
`ApiError { code: 403, message: "not a channel operator" }` rather
than `Ok(())`. The IRC impl must wire `ERR_CHANOPRIVSNEEDED` to this
return path. This requires a small "pending reply" buffer in the
listener keyed by the command timestamp (see R1 H5 fix for the
`shutdown_tx` infrastructure; the same channel can carry reply
codes).

**M8 — IRC `health_check` doesn't validate the authenticated
session.** The IRC impl MUST add an `is_authenticated: AtomicBool`
field on `IrcAdapter`, set it on `RPL_WELCOME` (001), clear it on
disconnect. `health_check` MUST return
`ApiError { code: 503, message: "IRC session not authenticated" }`
when `is_authenticated` is false, even if the TCP path is up.

### 5. WhatsApp optimization (closes M11)

**M11 — `list_own_groups` does O(N) lookup per group.** Replace the
per-group participant scan with a `HashSet<String>` of the bot's
possible phone forms (with/without country code, with/without
`@s.whatsapp.net`) computed once before the iter. Reduces complexity
from O(groups × participants) to O(groups × participants) constant
factor improvement (the per-group work is still O(participants), but
the inner comparison is a hash lookup instead of a string
comparison).

### 6. Trait doc clarifications (closes M12, M14)

**M12 — `GroupModeFlags` semantics in `set_ephemeral`.** Add a
doc-comment example: "for `ttl = None`, the adapter MUST disable
ephemeral mode (equivalent to a TTL of 0)". Add a unit test in the
trait's test module that the noop admin's
`set_ephemeral(..., None)` returns `Unimplemented` (covered) and
that the docs say "implementations should interpret `None` as
disable".

**M14 — `GroupHandle::is_admin` semantics.** Update the doc to:
"`is_admin: true` means the calling adapter can perform admin
actions (e.g. `set_locked`, `promote_to_admin`) on this group at
this moment. `false` means either the adapter is not an admin, or
the platform doesn't expose admin status for the bot". The IRC
impl currently always returns `false` for `join_by_invite` (line
1036) and `list_own_groups` (line 976) — this is correct per the
new doc, since IRC doesn't track op status at the adapter level.

### 7. M3 — IRC `health_check` ignores `use_tls`

Once R21's `connect_tls` no-op is fixed (R23d C1), `health_check`
MUST attempt the TLS handshake (or at least validate the cert
chain) when `use_tls = true`. This is blocked on C1's fix; tracked
under the same mission.

## Implementation Phases

### Phase 1: Trait surface (low risk, no behavior change)

- Add `try_new` constructors to `GroupId`, `PeerId`, `InviteRef`
- Add `debug_assert!(!s.is_empty())` to existing `new` methods
- Update doc-comments per §6 (M12, M14)
- Add `AddMemberOutput` struct and update the `add_member` trait
  signature per §3 (H6)
- Add `initial_admins_promoted: bool` to `GroupHandle` per §3 (M4)
- Add `list_own_groups_with_invites` per §3 (M13)

### Phase 2: WhatsApp-side behavior changes

- Implement `join_by_invite` via `client.groups().join_with_invite_code(...)` per §1 (H1)
- Rename inherent `create_group` to `create_group_str` per §3 (H2)
- Add `set_ephemeral` overflow error per §3 (M1)
- Add `M5` debug logging in `create_group` (already documented)
- Add `M11` HashSet optimization
- Tighten `WhatsAppConfig::validate()` and `group_to_jid` per §2 (M16)

### Phase 3: IRC-side behavior changes

- Tighten `IrcConfig::validate()` channel-name rules per §2 (M15)
- Add `is_authenticated: AtomicBool` and `health_check` upgrade per §4 (M8)
- Wire `ERR_CHANOPRIVSNEEDED` to `add_member` `ApiError` per §4 (M7)
- Flip `can_join_by_id: true` and add `join_by_id` wrapper per §1 (M10)
- Update `health_check` to use TLS per §7 (M3) — blocked on R23d C1's fix

### Phase 4: Update M3 once C1 is fixed

(no separate mission; rolls into the master mission as a sub-task)

## Key Files to Modify

| File | Change |
|---|---|
| `crates/octo-network/src/dot/adapters/coordinator_admin.rs` | `try_new` ctors (M2), `AddMemberOutput` (H6), `initial_admins_promoted` (M4), `list_own_groups_with_invites` (M13), doc updates (M12, M14) |
| `crates/octo-adapter-whatsapp/src/adapter.rs` | `create_group_str` rename (H2), `join_by_invite` impl (H1), `set_ephemeral` error (M1), M5 logging, M11 HashSet, M16 JID validation |
| `crates/octo-adapter-irc/src/lib.rs` | `IrcConfig::validate` channel rules (M15), `is_authenticated` (M8), `add_member` `ERR_CHANOPRIVSNEEDED` (M7), `can_join_by_id` flip + `join_by_id` (M10) |
| `crates/octo-adapter-irc/src/lib.rs` (listener) | M3 (TLS health check) — blocked on R23d C1 |
| `docs/research/coordinator-admin-actions.md` | Update M10's claim that IRC doesn't support join-by-id |

## Future Work

- **F1:** Add `CoordinatorAdmin` impl for Telegram TDLib, Matrix,
  matrix-sdk. Currently WhatsApp and IRC are the only implementations.
  Will be triggered by the corresponding adapter-mission R-passes.
- **F2:** Add a `list_own_groups_paginated` method for adapters
  (WhatsApp) that have large inventories. WhatsApp's `get_participating`
  returns all groups in one call; for a bot in 1000+ groups this
  matters.

## Rationale

Why a single RFC for 17 findings rather than 17 separate ones?

- All 17 findings touch a single trait surface (`CoordinatorAdmin`)
  with a single maintainer (`@mmacedoeu`) and a single test
  boundary (`crates/octo-network/src/dot/adapters/`).
- Splitting into 17 RFCs would create 17 review threads, 17 missions,
  17 PRs, and 17 merge coordination points for what is mechanically
  a small surface.
- The findings cluster naturally into 7 spec sections (§1–§7),
  each of which is reviewable in isolation.
- A single RFC makes the "deferred to future rounds" promise in
  R5 actionable: the user can review one spec, accept it, and
  implement all 11 fixes in one mission (or in three phase-aligned
  PRs per the implementation phases).

## Version History

| Version | Date       | Changes |
| ------- | ---------- | ------- |
| 1.0     | 2026-06-18 | Initial. Fills the "deferred without spec" gap from R5 review; specifies 11 R1 findings (H1, H2, H6, M1, M4, M5, M10, M11, M12, M13, M14, M15, M16, M3, M7, M8). |

## Related RFCs

- RFC-0850 (Networking): Deterministic Overlay Transport (DOT)
- RFC-0850p-a (Networking): WhatsApp Auth Onboarding
- RFC-0850p-c (Networking): Transport Group Binding Ceremony
- RFC-0850p-d (Networking): DC-Initiated Transport Group Creation & Invite
- RFC-0855p-c (Networking): DomainCoordinator Role

## Related Use Cases

- [`docs/use-cases/mission-coordinator-lifecycle.md`](../use-cases/mission-coordinator-lifecycle.md)
- [`docs/research/coordinator-admin-actions.md`](../research/coordinator-admin-actions.md)

## Related Review Docs

- [`docs/reviews/coordinator-admin-impl-adversarial-review-r1.md`](../reviews/coordinator-admin-impl-adversarial-review-r1.md) — source of all 17 findings
- [`docs/reviews/coordinator-admin-impl-adversarial-review-r5.md`](../reviews/coordinator-admin-impl-adversarial-review-r5.md) — R5 closure summary; references this RFC

## Appendices

### A. Finding-to-spec mapping

| R1 finding | RFC § | Severity | Adapter | Phase |
|---|---|---|---|---|
| H1 | §1 | HIGH | WhatsApp | 2 |
| H2 | §3 | HIGH | WhatsApp | 2 |
| H6 | §3 | HIGH | WhatsApp | 1 |
| M1 | §3 | MEDIUM | WhatsApp | 2 |
| M2 | §2 | MEDIUM | trait | 1 |
| M3 | §7 | MEDIUM | IRC | 4 (blocked on C1) |
| M4 | §3 | MEDIUM | WhatsApp | 1 |
| M5 | §3 | MEDIUM | WhatsApp | 2 |
| M7 | §4 | MEDIUM | IRC | 3 |
| M8 | §4 | MEDIUM | IRC | 3 |
| M10 | §1 | MEDIUM | IRC | 3 |
| M11 | §5 | MEDIUM | WhatsApp | 2 |
| M12 | §6 | MEDIUM | trait | 1 |
| M13 | §3 | MEDIUM | WhatsApp | 1 |
| M14 | §6 | MEDIUM | trait | 1 |
| M15 | §2 | MEDIUM | IRC | 3 |
| M16 | §2 | MEDIUM | WhatsApp | 2 |

17 entries. The R5 review listed 16 of them (H1, H2, H6, M1, M4,
M5, M10–M16, M3, M7, M8) as "deferred"; M2 (try_new constructors) was
missed by the R5 review's enumeration but is the same kind of
trait-level input-validation fix as M15/M16 and is included here for
completeness. This RFC is the canonical spec for all 17 items.
