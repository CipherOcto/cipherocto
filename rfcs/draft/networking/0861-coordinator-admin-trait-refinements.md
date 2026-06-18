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
| WhatsApp | `can_join_by_invite` | `true` (impl returns `Unimplemented`) | implement via `client.groups().join_with_invite_code(...)` | H1: SDK call exists per the existing comment; bit is honest only when impl is real (see §3 H1 for the variant mapping) |
| IRC | `can_join_by_id` | `false` | `true` (and add `join_by_id` method that wraps `join_by_invite`) | M10: `JOIN #chan` IS join-by-id; the bit is conservative-but-wrong |

**Decision policy:** for H1, implement `join_by_invite` (variant
mapping in §3 H1 below). For M10, choose the bit-flip — IRC's
`join_by_invite` and `join_by_id` would be aliases of the same
underlying `JOIN` IRC command, and the bit should reflect that.

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

### 3. Error and partial-success semantics (closes H1, H2, H6, M1, M4, M5, M13)

**H1 — WhatsApp `join_by_invite` implementation.** Implement the
method via `client.groups().join_with_invite_code(invite.0.as_str())`.
The SDK returns `Result<JoinGroupResult, anyhow::Error>` where
`JoinGroupResult` is an enum defined in the `wacore` SDK at
`wacore/src/iq/groups.rs:2319`:

```rust
pub enum JoinGroupResult {
    Joined(Jid),
    PendingApproval(Jid),
}
```

Map both variants to
`Ok(GroupHandle { id: GroupId::new(jid.to_string()), is_admin: false,
subject: None, invite_url: None, member_count: None, mode_flags: None })`.
The implementer MAY distinguish `PendingApproval` from `Joined` by
leaving `subject: None` and returning `Ok(GroupHandle)`; callers that
need to know "pending vs joined" can call `get_group_metadata` after
a backoff. Map the `anyhow::Error` to
`PlatformAdapterError::ApiError` with a clear message. Keep
`can_join_by_invite: true` in the capability report (now honest
because the impl is real).

**H2 — WhatsApp `create_group` trait/inherent disambiguation.**
Rename the inherent `create_group` on `WhatsAppWebAdapter` to
`create_group_str` (mirroring the `leave_group_str` precedent: inherent
method at `crates/octo-adapter-whatsapp/src/adapter.rs:1769`, comment
block at lines 1763-1764, trait impl at lines 1467-1479). The trait
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
`GroupHandle` (defaulted via `#[serde(default)]` for wire
backward-compatibility: pre-RFC-0861 serializations deserialize
with `initial_admins_promoted: false`). The WhatsApp impl populates
it from the `promote_participants` call's result. The trait's
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
the invite URLs in parallel. The implementer MAY use
`futures::future::join_all` (will require adding `futures = "0.3"` to
`crates/octo-adapter-whatsapp/Cargo.toml`; not currently a dep) OR
`tokio::task::JoinSet` (already available via `tokio`). Keep the
existing `list_own_groups` for callers that don't need URLs. Document
the N+1 cost in the trait.

### 4. IRC `is_admin` / `add_member` honesty (closes M7, M8)

**M7 — IRC `add_member` (INVITE) doesn't require op status.** The
trait's doc-comment for `add_member` MUST be updated to add
`PlatformAdapterError::ApiError` semantics: adapters that receive
`ERR_CHANOPRIVSNEEDED` (or equivalent) from the platform MUST return
`ApiError { code: 403, message: "not a channel operator" }` rather
than `Ok(())`. The IRC impl must wire `ERR_CHANOPRIVSNEEDED` to this
return path. This requires a NEW `pending_replies` correlation
buffer — neither `out_tx` (mpsc::Sender<String> for outbound lines
to the server, `crates/octo-adapter-irc/src/lib.rs:222`) nor
`shutdown_tx` (watch::Sender<bool> for shutdown signaling,
`crates/octo-adapter-irc/src/lib.rs:232`) can carry reply codes.
The implementer MUST add a `pending_replies: Mutex<HashMap<CommandId,
oneshot::Sender<NumericResult>>>` on `IrcAdapter` (and the matching
state inside the `irc_session` listener task), keyed by a per-command
nonce that `add_member` inserts on send and that the listener
resolves when the matching numeric reply arrives.

**M8 — IRC `health_check` doesn't validate the authenticated
session.** The IRC impl MUST add an `is_authenticated: AtomicBool`
field on `IrcAdapter`, set it on the first RPL_ENDOFMOTD (376) or
ERR_NOMOTD (422) received, and clear it on disconnect or session
restart. Using 376/422 (not 001 / RPL_WELCOME) is intentional: the
listener's existing 376/422 handler at
`crates/octo-adapter-irc/src/lib.rs:838-849` triggers the channel
JOINs, and 376/422 are only sent AFTER the NICK/USER handshake
completes, so they are the canonical "we are authenticated and the
session is usable" signal. (The listener has no 001/RPL_WELCOME
parsing; using 376/422 reuses existing code instead of adding new
parsing.) `health_check` MUST return
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
1565) and `list_own_groups` (line 1469) — this is correct per the
new doc, since IRC doesn't track op status at the adapter level.

### 7. M3 — IRC `health_check` ignores `use_tls`

The current `health_check` does a plain `TcpStream::connect` even
when `use_tls = true`. Once `connect_tls` is real (it is now, at
`crates/octo-adapter-irc/src/lib.rs:713-723` — uses `tokio-rustls`
via `TlsConnector::from(tls_client_config())`), `health_check` MUST
attempt the same TLS handshake when `use_tls = true`. If the
handshake fails, return `ApiError { code: 525, message: "TLS
handshake failed" }` so the caller can distinguish "TCP up, TLS
broken" from "TCP down".

The R23d C1 fix is already in `next` (commit `4b0f5e0`), so this
is unblocked. Tracked in the same mission, no separate sub-task.

## Implementation Phases

### Phase 1: Trait surface (additive; no breakage for existing callers)

- Add `try_new` constructors to `GroupId`, `PeerId`, `InviteRef`
- Add `debug_assert!(!s.is_empty())` to existing `new` methods
- Update doc-comments per §6 (M12, M14)
- Add `AddMemberOutput` struct and update the `add_member` trait
  signature per §3 (H6)
- Add `initial_admins_promoted: bool` to `GroupHandle` per §3 (M4)
- Add `list_own_groups_with_invites` per §3 (M13)

### Phase 2: WhatsApp-side behavior changes

- Implement `join_by_invite` via `client.groups().join_with_invite_code(...)` per §1 (H1)
- Rename inherent `create_group` to `create_group_str` per §3 (H2) — **do this FIRST**, all other Phase 2 edits land on the renamed function
- Add `set_ephemeral` overflow error per §3 (M1) — in the renamed `create_group_str`'s sibling method, not in `create_group` itself
- Add `M5` debug logging in `create_group_str` (already documented) — note H2 rename
- Add `M11` HashSet optimization
- Tighten `WhatsAppConfig::validate()` and `group_to_jid` per §2 (M16)

### Phase 3: IRC-side behavior changes

- Tighten `IrcConfig::validate()` channel-name rules per §2 (M15)
- Add `is_authenticated: AtomicBool` and `health_check` upgrade per §4 (M8) — set on 376/422, clear on disconnect
- Wire `ERR_CHANOPRIVSNEEDED` to `add_member` `ApiError` per §4 (M7)
- Flip `can_join_by_id: true` and add `join_by_id` wrapper per §1 (M10)
- Update `health_check` to use TLS per §7 (M3) — was previously "Phase 4" but is unblocked since R23d C1 is already fixed

## Key Files to Modify

| File | Change |
|---|---|
| `crates/octo-network/src/dot/adapters/coordinator_admin.rs` | `try_new` ctors (M2), `AddMemberOutput` (H6), `initial_admins_promoted` (M4), `list_own_groups_with_invites` (M13), doc updates (M12, M14) |
| `crates/octo-adapter-whatsapp/src/adapter.rs` | `create_group_str` rename (H2; inherent `leave_group_str` precedent at line 1769, trait impl at line 1467-1479), `join_by_invite` impl (H1, line 1728-1742), `set_ephemeral` error (M1), M5 logging, M11 HashSet, M16 JID validation |
| `crates/octo-adapter-irc/src/lib.rs` | `IrcConfig::validate` channel rules (M15, validate at line 95; struct at line 58, impl at line 82), `is_authenticated: AtomicBool` field on `IrcAdapter` (struct ~line 225, next to `out_tx`/`shutdown_tx`); SET it true in the existing 376/422 branch in `irc_session` at line 838, CLEAR it in `disconnect` next to the existing `shutdown_tx` clear (M8), `add_member` `ERR_CHANOPRIVSNEEDED` (M7, `add_member` trait impl at line 1261-1273; requires NEW `pending_replies: Mutex<HashMap<CommandId, oneshot::Sender<NumericResult>>>` on `IrcAdapter` per RFC §4 M7), `can_join_by_id` flip + `join_by_id` (M10, capability report at line 1189) |
| `crates/octo-adapter-irc/src/lib.rs` (listener / `irc_session`) | M3 (TLS health check at line 1128) |
| `docs/research/coordinator-admin-actions.md` | Update M10's claim that IRC doesn't support join-by-id |

## Future Work

- **F1:** Add `CoordinatorAdmin` impl for Telegram TDLib and for
  Matrix (via the `matrix-sdk` Rust client library). Currently
  WhatsApp and IRC are the only implementations. Will be triggered
  by the corresponding adapter-mission R-passes (Telegram TDLib and
  Matrix adapter RFCs do not yet exist; this RFC will be
  cross-referenced when they do).
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
  implement all 17 fixes in one mission (or in three phase-aligned
  PRs per the implementation phases).

## Version History

| Version | Date       | Changes |
| ------- | ---------- | ------- |
| 1.0     | 2026-06-18 | Initial. Fills the "deferred without spec" gap from R5 review; specifies 17 R1 findings (H1, H2, H6, M1, M2, M3, M4, M5, M7, M8, M10, M11, M12, M13, M14, M15, M16). |
| 1.1     | 2026-06-18 | R24a fixes: H1 detailed with `JoinGroupResult` mapping; M8 trigger changed from 001 (RPL_WELCOME) to 376/422 (RPL_ENDOFMOTD/ERR_NOMOTD) since the listener has no 001 parsing; M3 unblocked (R23d C1 is fixed); stale line numbers corrected; `futures` dep note added for M13; test count corrections. |
| 1.3     | 2026-06-18 | R24c fixes: 8 LOW accuracy gaps. Key Files row for IrcConfig::validate line ~140 → 95 (actual); M8 row extended to name the IrcAdapter field decl (~line 225) and the SET site (irc_session line 838); capability report line 1190 → 1189 (can_join_by_id is at 1189); wacore JoinGroupResult line 2318 → 2319 (verified at the SDK checkout); Phase 1 title "(low risk, no behavior change)" → "(additive; no breakage for existing callers)" matching the mission; Phase 2 plan M5 line cross-references H2's create_group_str rename and reorders H2 first. |

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
| H6 | §3 | HIGH | WhatsApp | 2 |
| M1 | §3 | MEDIUM | WhatsApp | 2 |
| M2 | §2 | MEDIUM | trait | 1 |
| M3 | §7 | MEDIUM | IRC | 3 (unblocked since R23d C1) |
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
