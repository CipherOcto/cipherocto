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
`crates/octo-adapter-matrix-sdk`. The matrix-sdk 0.18 API exposes
most of the primitives the trait needs (`room.ban_user`,
`room.kick_user`, `room.update_power_levels`, `client.create_room`,
`room.invite_user_by_id`, `room.leave`, plus state-event helpers
`room.send_state_event` / `send_state_event_raw` for the rest) but the
adapter currently doesn't bind any of them to the cipherocto
admin-trait surface, so `as_coordinator_admin()` returns the default
`None` and all 24 trait methods return `Err(Unimplemented)`.

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

Every mapping below is a real matrix-sdk API verified against the
vendored 0.18 sources at
`~/.cargo/registry/src/.../matrix-sdk-0.18.0/src/room/mod.rs`,
`matrix-sdk-base-0.18.0/src/room/mod.rs`, and the ruma-* 0.18
crates. The third column lists the cipherocto-side translation.
The matrix adapter operates against the unified `Room` type
post-0.18 (`Joined`/`Invited`/`Left` were merged). `Room` derefs
to `BaseRoom`, so getters like `name()`, `topic()`,
`canonical_alias()`, `join_rule()`, `power_levels()` resolve
through that deref.

Where the SDK has no first-class method (join rules, member counts,
topic read), the adapter drops to
`room.send_state_event(...)` / `send_state_event_raw(...)` with
the relevant `m.room.*` state event content, or to
`room.members(RoomMemberships::JOIN).await?.len()` for counts.

| Trait method                   | matrix-sdk 0.18 API                                                                                                                                                                   | Translation notes                                                                                                                                                                                                                                                                                                                                  |
| ------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `admin_capabilities`           | (static, no SDK call)                                                                                                                                                                 | Returns truthful 21-flag report per the table below (one flag per trait method, matching the `Default` impl's all-false shape verified in `coordinator_admin.rs:826-849`)                                                                                                                                                                                       |
| `platform_name`                | (static)                                                                                                                                                                              | `"matrix"`                                                                                                                                                                                                                                                                                                                                         |
| `create_group`                 | `client.create_room(req)` with `preset: Some(RoomPreset::PrivateChat)` + `visibility: Visibility::Private` — both re-exported from `matrix_sdk::ruma::api::client::room::create_room::v3`                                                          | Initial members invited via `room.invite_user_by_id(user_id)`; `is_admin = true` honored via post-create `room.update_power_levels(vec![(user_id, 100)])` (see M4 note below)                                                                                                                                                                            |
| `leave_group`                  | `room.leave().await`                                                                                                                                                                  | Idempotent — SDK returns `Err(WrongRoomState)` if already left; treat as `Ok(())` per the trait's `leave_group` doc-comment                                                                                                                                                                                                                        |
| `destroy_group`                | `room.leave().await` (matrix has no "destroy room" primitive and no `disable_encryption` API — only `enable_encryption` exists at `matrix-sdk-0.18.0/src/room/mod.rs:2344`)              | Follow the trait's `destroy_group` doc-comment: "leave the group and revoke the invite link; the group ID may still be queryable after `destroy_group` returns `Ok(())`". `can_destroy: false` in the capability report — the SDK provides no tear-down primitive.                                                                                                                                                |
| `add_member`                   | `room.invite_user_by_id(user_id).await` then conditional `room.update_power_levels(vec![(user_id, 100)])` for `is_admin = true`                                                       | `AddMemberOutput { added, promoted }` partial-success per RFC-0861 H6: `added` from invite result, `promoted` from optional update_power_levels                                                                                                                                                                                                                                                                    |
| `remove_member`                | `room.kick_user(user_id, reason)`                                                                                                                                                     | SDK requires the caller to have power level ≥ kick threshold                                                                                                                                                                                                                                                                                       |
| `ban_member`                   | `room.ban_user(user_id, reason)`                                                                                                                                                      | SDK signature is `Room::ban_user(&UserId, Option<&str>) -> Result<()>` -- no `duration` parameter (matrix-sdk-0.18.0/src/room/mod.rs:1973). The indefinite-only check is enforced at the adapter layer: if `duration: Some(_)`, return `Err(ApiError { code: 400, message: "matrix ban is indefinite-only" })` without calling the SDK (RFC-0861 §3 M1). The wire-format reason is that `m.room.banned` has no expiry field.                                                                                                                                                                                                                                                                       |
| `promote_to_admin`             | `room.update_power_levels(vec![(user_id, 100)])`                                                                                                                                      | Power level `100` is the matrix "admin" level (matches `users_default` for admin-only rooms). The SDK handles reading the current `RoomPowerLevels`, mutating `users`, and sending `m.room.power_levels` atomically (matrix-sdk-0.18.0/src/room/mod.rs:2884-2894).                                                                                                                                          |
| `demote_from_admin`            | `room.update_power_levels(vec![(user_id, users_default)])`                                                                                                                            | The SDK auto-removes the user's per-user override when the new level equals `users_default` (matrix-sdk-0.18.0/src/room/mod.rs:2887-2893). Caller must read `users_default` first via `room.power_levels().await?.users_default`.                                                                                                                                                                                  |
| `approve_join_request`         | `room.invite_user_by_id(user_id)` — this IS the SDK's accept path. `KnockRequest::accept` (matrix-sdk-0.18.0/src/room/knock_requests.rs:65-68) delegates to `room.invite_user_by_id(&self.member_info.user_id)` internally. For richer per-request semantics (event id, timestamp, reason), use `room.subscribe_to_knock_requests()` and call `.accept()` on the matching `KnockRequest`. | The SDK has no `Room::accept_invite` method, but `KnockRequest::accept` is the canonical accept path. The trait's `approve_join_request` doc-comment says "Only meaningful on groups with `requires_approval = true`" — i.e., `JoinRule::Knock`. |
| `rename_group`                 | `room.set_name(name).await`                                                                                                                                                           | Sends `m.room.name` state event                                                                                                                                                                                                                                                                                                                    |
| `set_group_description`        | `room.set_room_topic(topic).await`                                                                                                                                                    | Sends `m.room.topic` state event                                                                                                                                                                                                                                                                                                                   |
| `set_locked`                   | `room.send_state_event(JoinRulesEventContent::new(JoinRule::Invite))` (true) / `JoinRule::Public` (false). **No `set_join_rule` method exists in 0.18.**                                              | "Locked" = invite-only on matrix; sets `m.room.join_rules` state event. Use `ruma::events::room::join_rules::{JoinRule, JoinRulesEventContent}` (re-exported via `matrix_sdk::ruma::events::room::join_rules`).                                                                                                                                                  |
| `set_announce`                 | Read `room.power_levels().await?`, set `pl.events_default = 100` (true) / `0` (false), then `room.send_state_event(RoomPowerLevelsEventContent::try_from(pl)?).await?`. **Do not use `update_power_levels`** (which is per-user only).            | Sends `m.room.power_levels` state event with mutated `events_default` field. Field name on the `RoomPowerLevels` wrapper struct is `events_default`, not `events.default`.                                                                                                                                                                                |
| `set_ephemeral`                | `room.send_state_event_raw(...)` for `m.room.retention` state event                                                                                                                   | TTL is `state_default` (ms); `None` = clear the state event entirely (matrix can disable retention once enabled, so the trait contract's "soft disable" precedent on `set_ephemeral` holds). Clamp `as_millis() > i64::MAX` to `ApiError { code: 400, ... }` per RFC-0861 §3 M1                                                                    |
| `set_require_approval`         | `room.send_state_event(JoinRulesEventContent::new(JoinRule::Knock))` (true) / `JoinRule::Invite` or `Public` (false)                                                                 | Matrix's "knock" join rule is the closest mapping -- joiners send `m.room.member` with `membership: knock`, admins accept them. **Truthful capability caveat:** `can_require_approval` is `true` on rooms where the homeserver supports `m.room.join_rules: knock`; report `false` on homeservers that don't (e.g., older Synapse configs) |
| `list_own_groups`              | `client.joined_rooms()` + per-room `room.name()` and `room.members(RoomMemberships::JOIN).await?.len()` (no `member_count` / `joined_members_count` method exists)                       | Returns `Vec<GroupHandle>` with `is_admin = (own power_level >= 100)`. **No `own_user_power_level` method exists** -- read own power via `room.get_user_power_level(&client.session_meta().expect("authenticated").user_id).await?` (matrix-sdk-0.18.0/src/room/mod.rs:2939) and unwrap the `UserPowerLevel::Int(_)` arm. (For room-creator accounts with room v12+, the result is `UserPowerLevel::Infinite` -- treat as ≥100.)                                                                                                                                                |
| `list_own_groups_with_invites` | Default impl delegates to `list_own_groups` + per-group `room.canonical_alias()` (most useful invite ref on matrix)                                                                   | Override the trait's default `list_own_groups_with_invites` to populate `invite_url` with `#alias:server`                                                                                                                                                                                                                                          |
| `get_group_metadata`           | `room.name()`, `room.topic()`, `room.power_levels()`, `room.members(RoomMemberships::JOIN).await?.len()` (no `joined_members_count` method), `room.canonical_alias()`                       | Map `room.power_levels().await?` to `admins: Vec<PeerId>` (filter members where power level >= 100); full member list via `room.members(RoomMemberships::JOIN)`                                                                                                                                                                                                  |
| `resolve_invite`               | `client.resolve_room_alias(alias).await`                                                                                                                                              | For `#alias:server`; for `mxc://` or `matrix.to` URLs, parse first                                                                                                                                                                                                                                                                                 |
| `join_by_invite`               | `client.join_room_by_id(room_id).await` (room_id from alias resolution)                                                                                                               | Distinct from `join_by_id` because the SDK has a first-class join path; the alias-resolution + join is the matrix "join via invite" flow                                                                                                                                                                                                           |
| `join_by_id`                   | `client.join_room_by_id(room_id).await`                                                                                                                                               | Matrix aliases are first-class — `!roomid:server` is joinable by ID; `#alias:server` resolves to a room_id and joins. Distinct from `join_by_invite` (which uses `JoinRule::Knock` semantics)                                                                                                                                                      |
| `transfer_ownership`           | Multi-step dance (matrix has no atomic transfer): `room.update_power_levels(vec![(new_owner, 100)])`, then `room.update_power_levels(vec![(self, users_default)])`, then `room.leave()` | `can_transfer_ownership = false` (matrix has no first-class transfer, per the trait's `transfer_ownership` doc-comment)                                                                                                                                                                                                                            |

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
    can_approve_join: true,      // KnockRequest::accept (matrix-sdk 0.18) -- delegates to invite_user_by_id
    // C. Mode
    can_rename: true,
    can_describe: true,
    can_lock: true,              // JoinRule::Invite
    can_announce: true,          // events_default power level
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
`docs/research/coordinator-admin-actions.md` §3 currently has this
row (verbatim, line 200):

> `octo-adapter-matrix-sdk` | matrix-sdk Rust crate | ❌ nothing (the SDK exposes the calls; the adapter just doesn't use them) | Same as matrix HTTP, but already wired through SDK — the upgrade is small

After this mission lands, that row's "Real admin surface today"
column updates to a truthful ✅ row enumerating the methods this
mission implements. The exact new cell text is left to the
implementer (it's a research-doc prose update, not a code-affecting
contract), but it must explicitly note:

- `can_create`, `can_join_by_id`, `can_join_by_invite`, `can_leave`,
  `can_add_member`, `can_remove_member`, `can_ban`, `can_promote`,
  `can_demote`, `can_rename`, `can_describe`, `can_lock`,
  `can_announce`, `can_set_ephemeral`, `can_require_approval`,
  `can_list_own_groups`, `can_list_own_groups_with_invites`,
  `can_get_metadata`, `can_resolve_invite` are `true`.
- `can_destroy` and `can_transfer_ownership` are `false` (matrix
  has no first-class primitive for either).
- `can_approve_join` is `true` **with a caveat**: the adapter
  implements `approve_join_request` via `room.invite_user_by_id(user_id)`,
  which matches what `KnockRequest::accept`
  (`matrix-sdk-0.18.0/src/room/knock_requests.rs:65-68`) does
  internally. The SDK has no `Room::accept_invite` method, but it
  does expose the event-driven `room.subscribe_to_knock_requests()`
  stream for richer per-request semantics (event id, timestamp,
  reason) — the impl chooses the simpler path.
- `can_require_approval` is `true` **with a caveat**:
  homeserver-dependent (`m.room.join_rules: knock` support varies —
  e.g., older Synapse configs may lack it).

## Acceptance Criteria

### Phase 1 — impl + unit tests

- [ ] `octo-adapter-matrix-sdk/src/lib.rs` gains an `impl
    CoordinatorAdmin for MatrixAdapter` block with all 24 trait
      methods overridden (22 `async fn` + 2 sync `fn`
      `admin_capabilities` and `platform_name`; per the trait body in
      `crates/octo-network/src/dot/adapters/coordinator_admin.rs:428-776`,
      using the per-method table above as the authoritative spec)
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
      19 unit tests still pass (18 `#[test]` + 1 `#[tokio::test]` in
      `mod tests` at `lib.rs:1406`); new admin unit tests pass

### Phase 2 — live tests

> **Coverage gap to call out before scoping.** The trait has 24
> methods across 5 sections (A. Lifecycle = 3, B. Membership = 6, C.
> Mode = 6, D. Discovery = 6, E. Handoff = 1). The six new live
> tests below cover at most 14 of those 24 methods (C is fully
> covered; A covers 1/3; B covers 4/6 -- missing `add_member` and
> `approve_join_request`; D covers 2/6 -- missing
> `list_own_groups_with_invites`, `resolve_invite`, `join_by_invite`,
> `join_by_id`; E covers 0/1). The full-coverage live suite would
> need **12 tests, not 6** -- mx09–mx14 are the **first 6** of that
> 12-test plan. The remaining 6 (mx15–mx20) are explicitly listed
> below as a follow-on mission, NOT part of this mission's
> acceptance gate. Phase 1's unit tests in `mod tests` cover the
> uncovered-method error paths at the adapter layer.

- [ ] `tests/live_matrix_test.rs` gains six new tests:
      `mx09_create_group`, `mx10_ban_kick`, `mx11_promote_demote`,
      `mx12_set_modes`, `mx13_list_and_metadata`,
      `mx14_set_require_approval`. Each follows the same pre-scan
      guard + room-create + cleanup pattern as `mx04_05_06`; each
      uses the `octo-test-mx-mx{nn}-{ts}` room-naming convention so
      the pre-scan guard sweeps stale rooms on the next run
- [ ] Each live test exercises one section of the trait (mx09 → A.
      Lifecycle [partial: `create_group` only]; mx10 → B. Membership
      [partial: `remove_member` + `ban_member`]; mx11 → B. Membership
      continues [partial: `promote_to_admin` + `demote_from_admin`];
      mx12 → C. Mode [full: rename, describe, lock, announce,
      ephemeral]; mx13 → D. Discovery [partial: `list_own_groups` +
      `get_group_metadata`]; mx14 → C. Mode continues [full:
      `set_require_approval`]). Section coverage is NOT 1:1 -- see
      the coverage-gap note above for the full 24-method breakdown
- [ ] `cargo test -p octo-adapter-matrix-sdk --features live-matrix
      --test live_matrix_test -- --ignored --nocapture` — all 14
      tests pass when run with `--test-threads=1` (live tests must
      run serially; matrix.org session is shared). Acceptance: mx00
      through mx14 all green; no flake across 3 consecutive full-suite
      runs. **Sync timeouts**: new mx09–mx14 tests use the 60s cold-sync
      budget (per commit `9c5c4ee1`'s production fix), not the 5s
      budget the pre-scan guard still uses -- the pre-scan is a
      best-effort warm-up only
- [ ] `docs/research/coordinator-admin-actions.md` §3 table updated
      per the M10 follow-on doc above
- [ ] **Follow-on mission `0850h-e` is filed** at
      `missions/open/0850h-e-matrix-coordinator-admin-coverage.md`
      (or similar) for the remaining 6 live tests:
      `mx15_add_member`, `mx16_approve_join_request`,
      `mx17_list_own_groups_with_invites`, `mx18_resolve_invite`,
      `mx19_join_by_invite_and_id`, `mx20_transfer_ownership`. This
      mission creates that follow-on file as a stub with the same
      pre-scan + naming-convention pattern, but does NOT block on
      implementing it

### Phase 3 — cross-adapter integration

- [ ] Add `mx-cross-coord-admin` smoke test in
      `crates/octo-adapter-matrix-sdk/tests/cross_coordinator_admin.rs`
      that loads both the matrix adapter and a `MockPlatformAdapter`
      (from `crates/octo-network/tests/common/mock_adapter.rs`) via
      `DotGateway::add_adapter` (`crates/octo-network/src/dot/mod.rs:114`),
      exercises the same admin operation through both adapters, and
      verifies `as_coordinator_admin()` returns `Some(self)` for
      matrix and the mock, and `None` for the non-admin platforms in
      the registry
- [ ] `cargo test --lib -p octo-network` — existing tests still pass
      (this is the cross-adapter smoke; it lives in the matrix
      adapter crate because the matrix side is what this mission
      brings online, so the test asserts matrix-from-the-outside)

## Location

- `crates/octo-adapter-matrix-sdk/src/lib.rs` — `impl
CoordinatorAdmin for MatrixAdapter` + `as_coordinator_admin`
  override
- `crates/octo-adapter-matrix-sdk/tests/live_matrix_test.rs` — six
  new live tests (mx09-mx14)
- `crates/octo-adapter-matrix-sdk/tests/cross_coordinator_admin.rs`
  — `mx-cross-coord-admin` smoke test (new file; Phase 3)
- `docs/research/coordinator-admin-actions.md` — §3 table update
- `missions/open/0850h-e-matrix-coordinator-admin-coverage.md` —
  follow-on mission stub for mx15-mx20 live tests

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
- **Power-level read pattern:** `room.power_levels().await?`
  returns `RoomPowerLevels` (the wrapper struct in
  `ruma_events::room::power_levels`, NOT `RoomPowerLevelsEventContent`).
  To read per-user overrides directly, use
  `room.get_user_power_level(user_id).await?` which returns
  `UserPowerLevel` (the `Int(_)` arm is the level; `Infinite` means
  room creator). For per-user overrides use the SDK helper
  ```rust
  room.update_power_levels(vec![(user_id, 100)]).await?;
  ```
  which handles reading the current state, mutating the `users`
  map, and sending the `m.room.power_levels` state event atomically
  (matrix-sdk-0.18.0/src/room/mod.rs:2884-2894). For
  `events_default` mutations (used by `set_announce`), do NOT use
  `update_power_levels` -- instead mutate the wrapper struct
  directly and send via `send_state_event`:
  ```rust
  let mut pl = room.power_levels().await?;
  pl.events_default = 100; // announce_only = true
  room.send_state_event(RoomPowerLevelsEventContent::try_from(pl)?).await?;
  ```
- **Power-level write gate:** matrix enforces that the caller can
  only set power levels ≤ their own. The matrix adapter needs to
  read the caller's own power level first via
  `room.get_user_power_level(&client.session_meta().expect("authenticated").user_id).await?`
  (`session_meta()` returns `Option<&SessionMeta>` — see
  `matrix-sdk-0.18.0/src/client/mod.rs:683`; **not** `own_user_power_level`,
  which does not exist as a 0.18 method) and fail with
  `ApiError { code: 403, message: "caller power level too low" }` if
  the requested level exceeds the caller's. This mirrors the IRC
  `ERR_CHANOPRIVSNEEDED` handling at RFC-0861 M7.
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
  naming convention `octo-test-mx-mx{nn}-{ts}` (per-test `mx{nn}`
  prefix + Unix-ms `ts` suffix) keeps the sweep scoped to each
  test's own rooms.

## Cross-references

- `crates/octo-network/src/dot/adapters/coordinator_admin.rs` —
  trait definition and module-level doc-block listing the platforms
  expected to support the trait; this mission closes the matrix
  gap in that doc-block
- `crates/octo-adapter-whatsapp/src/adapter.rs` — `impl
CoordinatorAdmin for WhatsAppWebAdapter` block; pattern to mirror
  (full impl inline in adapter file)
- `crates/octo-adapter-telegram-mtproto/src/adapter.rs` —
  `as_coordinator_admin` override pattern (4 lines)
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
- Unblocks `rfcs/draft/networking/0850p-d-dc-initiated-group-creation.md` Matrix section
- Unblocks `rfcs/draft/networking/0850p-e-kick-detection.md` Matrix event mapping (needs `remove_member` / `ban_member` on the trait)

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
in the same `0850h-*` matrix-adapter series (a/b/c/d) so the
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
100" — this is matrix's "admin" threshold and matches the
`GroupModeFlags.announce_only` semantics on other platforms
(`GroupModeFlags` has `locked`, `announce_only`, `ephemeral_ttl`,
`requires_approval` per
`crates/octo-network/src/dot/adapters/coordinator_admin.rs:325-340`).

### Power level defaults (matrix spec v1.13)

The matrix spec defaults that the impl needs to honor (per
[spec §4.6](https://spec.matrix.org/v1.13/client-server-api/#mroompower_levels)):

- `ban`: 50 (default threshold)
- `invite`: 0 (default threshold per matrix spec and ruma-events
  `RoomPowerLevels::default()`)
- `kick`: 50 (default threshold)
- `events_default`: 0 (message events follow this unless overridden)
- `state_default`: 50 (rename/topic/`m.room.join_rules` follow
  this unless overridden per-event)
- `users_default`: 0 (default power for non-explicit users)
- `m.room.power_levels` itself is NOT covered by `state_default`;
  it must be explicitly overridden in the `events` map. Typical
  room creators set this to `100`, which is what makes
  "promote to admin = power level 100" semantically meaningful.
- `m.room.retention` is also a state event and follows
  `state_default: 50` by default; rooms that want retention
  restricted to admins add an `events` override of `100`.

## Deadline

Pre-public-launch (this is part of the "six Tier-1 broadcast
platforms" claim in RFC-0863p-a; the documentation already commits
to it).
