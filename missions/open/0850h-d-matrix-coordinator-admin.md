# Mission: 0850h-d Matrix CoordinatorAdmin trait implementation

## Status

Open (2026-06-27)

## RFC

RFC-0850: Deterministic Overlay Transport (DOT) — §8 (CoordinatorAdmin
extension); RFC-0861 (CoordinatorAdmin Adapter Contract Refinements,
Accepted 2026-06-19) — refines the trait surface this mission
implements against.

## Summary

Implement `CoordinatorAdmin` for `MatrixAdapter` in
`crates/octo-adapter-matrix-sdk`. The matrix-sdk 0.18 API exposes every
primitive the trait needs (`room.ban_user`, `room.kick_user`,
`room.set_power_levels`, `room.redact`, `client.create_room`,
`room.invite_user`, `room.leave`, `room.set_join_rule`, etc.) but the
adapter currently doesn't bind any of them to the cipherocto
admin-trait surface, so `as_coordinator_admin()` returns the default
`None` and all 25 methods return `Err(Unimplemented)`.

Today only `WhatsAppWebAdapter`, `MtprotoTelegramAdapter`, and
`IrcAdapter` implement `CoordinatorAdmin`. RFC-0863p-a, RFC-0851p-b,
and RFC-0855p-c (all Accepted) all list Matrix among the
broadcast-capable platforms with group management via
`CoordinatorAdmin`, so the documentation has outrun the
implementation. Closing this gap is the prerequisite for
RFC-0855p-c-admin-attestation (mission `0855p-c-admin-attestation.md`,
Open) to use Matrix in its `PlatformAdminAttest` envelope source set,
and for RFC-0850p-d (DC-initiated group creation, Draft) to rely on
Matrix as one of the six Tier-1 platforms that can self-bootstrap a
DOT group.

## Design

### High-level shape

- `crates/octo-adapter-matrix-sdk/src/lib.rs` — add the
  `CoordinatorAdmin` impl block (~300-450 lines), and override
  `as_coordinator_admin` in the existing `PlatformAdapter` impl to
  return `Some(self)`. Pattern mirrors the 4-line
  `as_coordinator_admin` override on `MtprotoTelegramAdapter`
  (in `crates/octo-adapter-telegram-mtproto/src/adapter.rs`) and the
  per-method impls in
  `crates/octo-adapter-telegram-mtproto/src/coordinator_admin.rs`.
  The matrix version keeps the impls inline in `lib.rs` because the
  matrix adapter is a single file — the same pattern used by the
  `impl CoordinatorAdmin for WhatsAppWebAdapter` block in
  `crates/octo-adapter-whatsapp/src/adapter.rs`.
- `crates/octo-adapter-matrix-sdk/tests/live_matrix_test.rs` — extend
  the live suite with new tests `mx09_create_group`,
  `mx10_ban_kick`, `mx11_promote_demote`, `mx12_set_modes`,
  `mx13_list_and_metadata`, `mx14_set_require_approval`. Each test
  uses the existing `pre-scan guard` + `room.create` + cleanup pattern
  from `mx04_05_06_envelope_round_trip`. The pre-scan guard at
  `tests/live_matrix_test.rs` cleans up `octo-test-mx-*` rooms
  before each new admin test, so admin tests compose safely with the
  envelope tests in the same run.
- `docs/research/coordinator-admin-actions.md` — update the
  `octo-adapter-matrix-sdk` row in the per-platform capability
  matrix (§3, "Real admin surface today" column) from `❌ nothing`
  to the truthful list of supported methods; the row currently reads
  "the SDK exposes the calls; the adapter just doesn't use them" — that
  note becomes historical context for the upgrade.

### Per-method mapping (Matrix SDK 0.18 → trait method)

Every mapping below is a real matrix-sdk API confirmed against the
0.18 docs; the third column lists the cipherocto-side translation.
The matrix adapter operates against `Room` (the unified `Room` type
post-0.18 — `Joined`/`Invited`/`Left` were merged).

| Trait method                   | matrix-sdk 0.18 API                                                                                                                                                                   | Translation notes                                                                                                                                                                                                                                                                                                                                  |
| ------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `admin_capabilities`           | (static, no SDK call)                                                                                                                                                                 | Returns truthful 22-flag report per the table below                                                                                                                                                                                                                                                                                                |
| `platform_name`                | (static)                                                                                                                                                                              | `"matrix"`                                                                                                                                                                                                                                                                                                                                         |
| `create_group`                 | `client.create_room(req)` with `RoomPreset::PrivateChat` + `Visibility::Private`                                                                                                      | Initial members invited via `room.invite_user_by_id`; `is_admin = true` honored via post-create `room.set_power_levels` per-user override (see M4 note below)                                                                                                                                                                                      |
| `leave_group`                  | `room.leave().await`                                                                                                                                                                  | Idempotent — SDK returns `Err(WrongRoomState)` if already left; treat as `Ok(())` per the trait's `leave_group` doc-comment                                                                                                                                                                                                                        |
| `destroy_group`                | `room.leave().await` then `room.disable_encryption().await` (matrix best-effort "destroy" — rooms persist server-side)                                                                | Follow the trait's `destroy_group` doc-comment: "leave the group and revoke the invite link; the group ID may still be queryable after `destroy_group` returns `Ok(())`"                                                                                                                                                                           |
| `add_member`                   | `room.invite_user_by_id(user_id).await` then conditional `room.set_power_levels` for `is_admin = true`                                                                                | `AddMemberOutput { added, promoted }` partial-success per RFC-0861 H6: `added` from invite result, `promoted` from optional set_power_levels                                                                                                                                                                                                       |
| `remove_member`                | `room.kick_user(user_id, reason)`                                                                                                                                                     | SDK requires the caller to have power level ≥ kick threshold                                                                                                                                                                                                                                                                                       |
| `ban_member`                   | `room.ban_user(user_id, reason)`                                                                                                                                                      | Indefinite (`duration: None`) is the matrix default — `m.room.banned` state event has no expiry field; `duration: Some(_)` returns `Err(ApiError { code: 400, message: "matrix ban is indefinite-only" })` per the trait's "TTL bounds" precedent (RFC-0861 §3 M1)                                                                                 |
| `promote_to_admin`             | `room.set_power_levels` with per-user override                                                                                                                                        | Power level `100` is the matrix "admin" level (matches `users_default` for admin-only rooms); get current `PowerLevelsEventContent` via `room.power_levels().await?`, mutate, send                                                                                                                                                                 |
| `demote_from_admin`            | `room.set_power_levels` with per-user override removed                                                                                                                                | Drop the user's specific entry from `users` map; fall back to `users_default`                                                                                                                                                                                                                                                                      |
| `approve_join_request`         | `room.invite_user_by_id` for invite-only rooms; `room.accept_invite` for `m.room.member` with `membership: invite` events                                                             | For invite-only rooms, the SDK's invite flow auto-accepts; for restricted rooms, use `room.accept_invite()` on the pending `m.room.member` event                                                                                                                                                                                                   |
| `rename_group`                 | `room.set_name(name).await`                                                                                                                                                           | Sends `m.room.name` state event                                                                                                                                                                                                                                                                                                                    |
| `set_group_description`        | `room.set_topic(topic).await`                                                                                                                                                         | Sends `m.room.topic` state event                                                                                                                                                                                                                                                                                                                   |
| `set_locked`                   | `room.set_join_rule(JoinRule::Invite)` (true) / `JoinRule::Public` (false)                                                                                                            | "Locked" = invite-only on matrix; sets `m.room.join_rules` state event                                                                                                                                                                                                                                                                             |
| `set_announce`                 | `room.set_power_levels` with `events.default = 100` (true) / `0` (false)                                                                                                              | Sends `m.room.power_levels` state event with mutated `events_default`                                                                                                                                                                                                                                                                              |
| `set_ephemeral`                | `room.send_state_event_raw(...)` for `m.room.retention` state event                                                                                                                   | TTL is `state_default` (ms); `None` = clear the state event entirely (matrix can disable retention once enabled, so the trait contract's "soft disable" precedent on `set_ephemeral` holds). Clamp `as_millis() > i64::MAX` to `ApiError { code: 400, ... }` per RFC-0861 §3 M1                                                                    |
| `set_require_approval`         | `room.set_join_rule(JoinRule::Knock)` (true) / `JoinRule::Invite` or `Public` (false)                                                                                                 | Matrix's "knock" join rule is the closest mapping — joiners send `m.room.member` with `membership: knock`, admins `accept_invite` them. **Truthful capability caveat:** `can_require_approval` is `true` on rooms where the homeserver supports `m.room.join_rules: knock`; report `false` on homeservers that don't (e.g., older Synapse configs) |
| `list_own_groups`              | `client.joined_rooms()` + per-room `room.name()` and `room.member_count()`                                                                                                            | Returns `Vec<GroupHandle>` with `is_admin = (own power_level >= 100)` per `room.own_user_power_level().await`                                                                                                                                                                                                                                      |
| `list_own_groups_with_invites` | Default impl delegates to `list_own_groups` + per-group `room.canonical_alias()` (most useful invite ref on matrix)                                                                   | Override the trait's default `list_own_groups_with_invites` to populate `invite_url` with `#alias:server`                                                                                                                                                                                                                                          |
| `get_group_metadata`           | `room.name()`, `room.topic()`, `room.power_levels()`, `room.joined_members_count()`, `room.canonical_alias()`                                                                         | Map `room.power_levels().await?` → `admins: Vec<PeerId>` (filter members where power level ≥ 100); full member list via `room.members(...)`                                                                                                                                                                                                        |
| `resolve_invite`               | `client.resolve_room_alias(alias).await`                                                                                                                                              | For `#alias:server`; for `mxc://` or `matrix.to` URLs, parse first                                                                                                                                                                                                                                                                                 |
| `join_by_invite`               | `client.join_room_by_id(room_id).await` (room_id from alias resolution)                                                                                                               | Distinct from `join_by_id` because the SDK has a first-class join path; the alias-resolution + join is the matrix "join via invite" flow                                                                                                                                                                                                           |
| `join_by_id`                   | `client.join_room_by_id(room_id).await`                                                                                                                                               | Matrix aliases are first-class — `!roomid:server` is joinable by ID; `#alias:server` resolves to a room_id and joins. Distinct from `join_by_invite` (which uses `JoinRule::Knock` semantics)                                                                                                                                                      |
| `transfer_ownership`           | Multi-step dance (matrix has no atomic transfer): `room.set_power_levels` setting new owner to 100, then `room.set_power_levels` setting self to `users_default`, then `room.leave()` | `can_transfer_ownership = false` (matrix has no first-class transfer, per the trait's `transfer_ownership` doc-comment)                                                                                                                                                                                                                            |

### Truthful `admin_capabilities()` report

Every flag's value is determined by what matrix-sdk 0.18 actually
exposes (not by what the homeserver happens to support — that's a
runtime concern, surfaced via `set_require_approval` per the caveat
above).

```rust
AdminCapabilityReport {
    // A. Lifecycle
    can_create: true,
    can_join_by_id: true,        // matrix aliases are first-class
    can_join_by_invite: true,    // matrix-rust-sdk has join_room_by_id
    can_leave: true,
    can_destroy: false,          // matrix rooms persist server-side (see destroy_group doc)
    // B. Membership
    can_add_member: true,
    can_remove_member: true,
    can_ban: true,               // m.room.banned state event
    can_promote: true,           // via per-user power level override
    can_demote: true,
    can_approve_join: true,      // KnockRule + accept_invite
    // C. Mode
    can_rename: true,
    can_describe: true,
    can_lock: true,              // JoinRule::Invite
    can_announce: true,          // events.default power level
    can_set_ephemeral: true,     // m.room.retention state event
    can_require_approval: true,  // JoinRule::Knock (caveat: homeserver-dependent)
    // D. Discovery
    can_list_own_groups: true,
    can_get_metadata: true,
    can_resolve_invite: true,
    // E. Handoff
    can_transfer_ownership: false, // matrix has no atomic transfer primitive
}
```

### M4 caveat — `initial_admins_promoted` for matrix

The `GroupHandle.initial_admins_promoted` field (added by RFC-0861
M4) tracks whether the platform-side admin promotion step has
completed for this group. For matrix, the creator is automatically
power level 100 in rooms they create — so `initial_admins_promoted`
is `true` at create time with no post-create dance. The WhatsApp
`create_group` impl at `crates/octo-adapter-whatsapp/src/adapter.rs`
does a post-create `promote_participants` step and reflects this in
the field; matrix's create flow has no such step.

### E2EE dependency

The matrix adapter requires E2EE bootstrap (mission `0850h-b-matrix-
adapter-e2ee.md`, Claimed 2026-06-02, SDK 0.18 extension landed)
before any power-level or room-state operations work end-to-end. The
admin tests depend on `client.session()` returning a valid session
via the OIDC flow — they fail with `SessionMissing` if the bootstrap
is missing. The CI gate is therefore: `cargo test -p
octo-adapter-matrix-sdk --features live-matrix --test live_matrix_test
-- --ignored --nocapture` with a session at
`~/.config/octo/matrix.json` (from `octo-matrix-onboard login oidc`).

### Documentation update (RFC-0861 §1 M10 follow-on)

The per-platform capability matrix in
`docs/research/coordinator-admin-actions.md` §3 currently lists
`octo-adapter-matrix-sdk` as "the SDK exposes the calls; the adapter
just doesn't use them — the upgrade is small". That row updates to:

> ❌ nothing (the SDK exposes the calls; the adapter just doesn't use them) | Same as matrix HTTP, but already wired through SDK — the upgrade is small

to a truthful list of supported methods, with a note that
`can_destroy: false`, `can_transfer_ownership: false`, and the
`set_require_approval` homeserver caveat are matrix-specific.

## Acceptance Criteria

### Phase 1 — impl + unit tests

- [ ] `octo-adapter-matrix-sdk/src/lib.rs` gains an `impl
    CoordinatorAdmin for MatrixAdapter` block with all 25 methods
      overridden (using the per-method table above as the
      authoritative spec)
- [ ] `as_coordinator_admin` is overridden in the existing
      `PlatformAdapter` impl to return `Some(self)`; the override
      mirrors the 4-line pattern on `MtprotoTelegramAdapter`
      (in `crates/octo-adapter-telegram-mtproto/src/adapter.rs`)
- [ ] `admin_capabilities()` returns the truthful report documented
      above (with `can_destroy: false`, `can_transfer_ownership: false`,
      `can_require_approval: true` with the homeserver caveat noted
      in the impl doc-comment)
- [ ] Unit tests in `octo-adapter-matrix-sdk/src/lib.rs` `mod tests`
      cover the partial-success `AddMemberOutput` discriminator (the
      three variants per RFC-0861 H6: `None` / `Some(Ok(()))` /
      `Some(Err(_))`), the `initial_admins_promoted` matrix semantics
      (always `true` at create time, no post-create dance), the
      `set_ephemeral` i64-overflow `ApiError { code: 400 }` clamp,
      and the `ban_member(duration: Some(_))` indefinite-only
      rejection
- [ ] `cargo build --all-targets -p octo-adapter-matrix-sdk` — zero
      errors
- [ ] `cargo clippy --all-targets --all-features -- -D warnings -p
    octo-adapter-matrix-sdk` — zero warnings
- [ ] `cargo fmt --check` — clean
- [ ] `cargo test --lib -p octo-adapter-matrix-sdk` — all existing
      34 unit tests still pass; new admin unit tests pass

### Phase 2 — live tests

- [ ] `tests/live_matrix_test.rs` gains six new tests:
      `mx09_create_group`, `mx10_ban_kick`, `mx11_promote_demote`,
      `mx12_set_modes`, `mx13_list_and_metadata`,
      `mx14_set_require_approval`. Each follows the same pre-scan
      guard + room-create + cleanup pattern as `mx04_05_06`; each
      uses the `octo-test-mx-mx{nn}-{ts}` room-naming convention so
      the pre-scan guard sweeps stale rooms on the next run
- [ ] Each live test exercises one section of the trait (mx09 → A.
      Lifecycle; mx10 → B. Membership; mx11 → B. Membership continues;
      mx12 → C. Mode; mx13 → D. Discovery; mx14 → C. Mode continues).
      The trait doc-block on `CoordinatorAdmin`'s module header lists
      the trait's section coverage, and these tests map 1:1 to those
      sections
- [ ] `cargo test -p octo-adapter-matrix-sdk --features live-matrix
    --test live_matrix_test -- --ignored --nocapture` — all 14
      tests pass when run with `--test-threads=1` (live tests must
      run serially; matrix.org session is shared). Acceptance: mx01
      through mx14 all green; no flake across 3 consecutive full-suite
      runs
- [ ] `docs/research/coordinator-admin-actions.md` §3 table updated
      per the M10 follow-on doc above

### Phase 3 — cross-adapter integration

- [ ] Add `mx-cross-coord-admin` smoke test that loads both the
      matrix adapter and a `MockPlatformAdapter` (from
      `crates/octo-network/tests/common/mock_adapter.rs`) via
      `DotGateway`, exercises the same admin operation through both
      adapters, and verifies `as_coordinator_admin()` returns
      `Some(self)` for matrix and the mock, and `None` for the
      non-admin platforms in the registry
- [ ] `cargo test --lib -p octo-network` — existing tests still pass
      (this is the cross-adapter smoke; it lives in the matrix
      adapter crate because it's a Matrix-side acceptance check)

## Location

- `crates/octo-adapter-matrix-sdk/src/lib.rs` — `impl
CoordinatorAdmin for MatrixAdapter` + `as_coordinator_admin`
  override
- `crates/octo-adapter-matrix-sdk/tests/live_matrix_test.rs` — six
  new live tests (mx09-mx14)
- `docs/research/coordinator-admin-actions.md` — §3 table update

## Complexity

Medium (~400 lines of impl in `lib.rs` + 6 live tests + 1 cross-
adapter smoke). Smaller than the WhatsApp R19/R20 set (which is
~280 lines of impl but on a more platform-restricted surface) and
larger than the IRC set (which has fewer true platform primitives
to map). Phase 1 is the critical path; Phase 2-3 are verification.

## Prerequisites

- Mission `0850h-b-matrix-adapter-e2ee.md` (Claimed 2026-06-02) — the
  matrix adapter must have E2EE enabled for the SDK's room state
  calls to work end-to-end. This mission depends on the SDK 0.18
  upgrade (extension of 0850h-b) having landed.
- `octo-matrix-onboard login oidc --homeserver https://matrix.org` —
  live tests require an OIDC-authenticated session at
  `~/.config/octo/matrix.json`.
- RFC-0850 and RFC-0861 (both Accepted) — these define the trait
  surface this mission implements against.
- (Optional) Mission `0855p-c-admin-attestation.md` (Open) — once
  this mission lands, that attestation mission can include Matrix
  in its `PlatformAdminAttest` envelope source set. The dependency
  is `0850h-d` → `0855p-c`, not the reverse.

## Implementation Notes

- **Pattern to mirror:** the `impl CoordinatorAdmin for
WhatsAppWebAdapter` block in
  `crates/octo-adapter-whatsapp/src/adapter.rs`, which is a full
  impl inline in the adapter file. The matrix adapter is also a
  single-file crate, so the WhatsApp pattern is the right precedent
  (the telegram-mtproto split into a separate `coordinator_admin.rs`
  module was specific to that crate's organization; the matrix
  adapter doesn't have that split).
- **Room lookup pattern:** every method needs to convert the trait's
  `GroupId(String)` to a `matrix_sdk::ruma::OwnedRoomId` via
  `OwnedRoomId::try_from(group_id.as_str())` and then look up
  `client.get_room(&room_id)`. If `get_room` returns `None`, return
  `PlatformAdapterError::Unreachable { platform: "matrix", reason:
"room not in joined_rooms" }`. This pattern is already used in the
  live tests' cleanup blocks.
- **Power-level read pattern:** `room.power_levels().await?` returns
  the current `PowerLevelsEventContent`. To mutate, clone and edit:
  ```rust
  let mut pl = room.power_levels().await?;
  pl.users.insert(user_id, 100);
  room.set_power_levels(pl).await?;
  ```
  The `room.set_power_levels` call takes the new content and sends
  the state event.
- **Power-level write gate:** matrix enforces that the caller can
  only set power levels ≤ their own. The matrix adapter needs to
  read the caller's own power level first (`room.own_user_power_level
.await`) and fail with `ApiError { code: 403, message: "caller power
level too low" }` if the requested level exceeds the caller's.
  This mirrors the IRC `ERR_CHANOPRIVSNEEDED` handling at RFC-0861
  M7.
- **H4 — redaction is not in the trait.** Note: `room.redact()` is
  a matrix-sdk API but is NOT a `CoordinatorAdmin` method — it's
  per-message operation, not group-management. Any caller needing
  message redaction must call `room.redact(event_id, reason)` via
  the platform-specific API path, not via the trait. If a future
  trait revision adds a `redact_message` method, this mission's
  pattern maps directly to it.
- **No-op update for the existing live tests.** The pre-scan guard
  in `tests/live_matrix_test.rs` already sweeps `octo-test-mx-*`
  rooms, so the new mx09-mx14 tests compose with the existing
  mx04-mx07 tests without changes to the test harness. The room
  naming convention `octo-test-mx-mx{nn}-{ts}` (per-test prefix
  suffix) keeps the sweep scoped to each test's own rooms.

## Cross-references

- `crates/octo-network/src/dot/adapters/coordinator_admin.rs` —
  trait definition (the surface this mission implements against)
- `crates/octo-adapter-whatsapp/src/adapter.rs` — `impl
CoordinatorAdmin for WhatsAppWebAdapter` block; pattern to mirror
  (full impl inline in adapter file)
- `crates/octo-adapter-telegram-mtproto/src/adapter.rs` —
  `as_coordinator_admin` override pattern (4 lines)
- `crates/octo-network/src/dot/adapters/coordinator_admin.rs` —
  module-level doc-block listing the platforms expected to support
  the trait; this mission closes the matrix gap in that doc-block
- `docs/research/coordinator-admin-actions.md` §3 — per-platform
  capability matrix that this mission updates
- `missions/claimed/0850h-b-matrix-adapter-e2ee.md` — parent mission
  that enabled the SDK 0.18 upgrade
- `missions/open/0855p-c-admin-attestation.md` — downstream mission
  that will use Matrix admin via the trait once this mission lands
- `rfcs/draft/networking/0850p-d-dc-initiated-group-creation.md` —
  downstream RFC that lists matrix as one of the six Tier-1
  platforms that should self-bootstrap a DOT group via the trait
- `rfcs/draft/networking/0850p-e-kick-detection.md` — downstream RFC
  that maps matrix `m.room.member` ban/leave events; depends on
  `remove_member` and `ban_member` being available on the trait

## Mitigates

- Documentation drift: closes the gap where RFC-0863p-a, RFC-0851p-b,
  and RFC-0855p-c (all Accepted) claim matrix implements
  `CoordinatorAdmin` but the adapter doesn't bind the trait
- Unblocks `missions/open/0855p-c-admin-attestation.md` for Matrix
- Unblocks `rfcs/draft/networking/0850p-d-dc-initiated-group-
creation.md` Matrix section
- Unblocks `rfcs/draft/networking/0850p-e-kick-detection.md` Matrix
  event mapping (needs `remove_member` / `ban_member` on the trait)

## Notes

### Why a separate mission, not an extension of 0850h-b

Mission `0850h-b-matrix-adapter-e2ee.md` is about E2EE bootstrap,
recovery key, and SAS verification — the crypto-side of the matrix
adapter. `CoordinatorAdmin` is the group-management-side. The two
share the matrix-sdk crate and the same `MatrixAdapter` struct but
they're orthogonal features: E2EE is required for `send_envelope`
end-to-end; `CoordinatorAdmin` is required for the
domain-coordinator role to manage the room that envelopes flow
through. Combining them would force a single PR covering crypto,
room management, and the live-test matrix — too large to review
cleanly.

### Why 0850h-d, not 0850h-c

`0850h-c-file-based-refresh-rotation.md` (Claimed) is taken by
the file-based OAuth refresh-token rotation work. This mission is
in the same `0850h-*` matrix-adapter series (a/d/b/c/d) so the
ordering matches: a (auth) → b (E2EE) → c (refresh rotation) → d
(admin trait). Using a number from a different series would break
the alphabet ordering readers expect from the directory listing.

### Reference for power-level semantics

The Matrix Spec, Section 4.6 ("Power level events"):
<https://spec.matrix.org/v1.13/client-server-api/#mroompower_levels>
The matrix-sdk 0.18 docs:
<https://docs.rs/matrix-sdk/0.18.0/matrix_sdk/struct.Room.html>

The power levels for the relevant actions on matrix are:

- Kick: 50 (default)
- Ban: 50 (default)
- Invite: 50 (default)
- Send state events (rename, topic, power levels themselves): 100
- Set join rules: 50 (default)
- Send m.room.retention: 100 (typically restricted to admins)

The adapter's `promote_to_admin` maps to "set user's power level to
100" — this is matrix's "admin" threshold and matches
`GroupModeFlags.only_admins_change_info` semantics on other platforms.

## Deadline

Pre-public-launch (this is part of the "six Tier-1 broadcast
platforms" claim in RFC-0863p-a; the documentation already commits
to it).
