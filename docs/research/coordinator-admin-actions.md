# Research: Coordinator / Admin Actions on DOT Transport Adapters

**Date:** 2026-06-18
**Status:** Research
**Related:** [`docs/research/group-coordination-transport-adapters.md`](group-coordination-transport-adapters.md) — the prior doc maps
_group primitives_ (does the platform have a group at all?). This doc maps
_group admin actions_ (what can a creator/admin _do_ with the group, and how
should the DOT trait model it?).
**Scope:** 20 platform adapters in `crates/octo-adapter-*` and the
`PlatformAdapter` trait in `crates/octo-network/src/dot/adapters/mod.rs`.
**Trigger:** R19 (live WhatsApp E2E test for coordinator group setup)
shipped five coordinator-only methods on `WhatsAppWebAdapter` —
`create_group`, `add_members`, `get_invite_link`, `leave_group`,
`group_metadata` — and the question came up: should these (and their
natural extensions) be promoted to a shared abstraction, or stay
per-adapter? Per-adapter keeps WhatsApp isolated; shared abstraction
helps every other Tier-1 adapter adopt the same coordinator surface
uniformly.

---

## Executive Summary

The `PlatformAdapter` trait today models only **envelope transport**:
`send_envelope`, `receive_messages`, `canonicalize`. Group _lifecycle_
(create, leave, delete), group _membership_ (add, remove, promote, ban),
group _mode_ (lock, announce, ephemeral, approve-required), group
_discovery_ (list, lookup by invite), and group _handoff_ (transfer
ownership, demote-self) are all absent from the trait and from every
adapter except WhatsApp (R19). They live in the _platform's own SDK_
and are platform-shaped: a WhatsApp `GroupParticipant` is not a
Telegram `ChatMember` is not a Matrix `RoomMember` is not a Discord
`Member` is not a Nostr `Event`.

But the **use cases** (the _why_) generalize cleanly. There are five
distinct categories of coordinator action, and every Tier-1 platform
(those with native group support) has a way to express each one, even
if the names differ:

| Category          | Common shape                                                                   | Example (Telegram TDLib)                                               | Example (Matrix)                                        | Example (WhatsApp)                                                                  |
| ----------------- | ------------------------------------------------------------------------------ | ---------------------------------------------------------------------- | ------------------------------------------------------- | ----------------------------------------------------------------------------------- |
| **A. Lifecycle**  | `create / join / leave / delete`                                               | `createNewSupergroupChat` / `deleteChatHistory`                        | `create_room` / `leave_room`                            | `create_group` (R19) / `leave`                                                      |
| **B. Membership** | `add / remove / promote / demote / ban`                                        | `addChatMember` / `banChatMember` / `setChatMemberStatus`              | `invite_user` / `kick` / `ban`                          | `add_members` (R19) / `promote_participants` / `remove_participants`                |
| **C. Mode**       | `set_topic / set_description / lock / announce / ephemeral / approve_required` | `setChatTitle` / `setChatPermissions` / `setChatMessageAutoDeleteTime` | `set_room_name` / `set_room_topic` / `redact_event`     | `set_subject` / `set_description` / `set_locked` / `set_announce` / `set_ephemeral` |
| **D. Discovery**  | `list_my_groups / get_group / resolve_invite`                                  | `getChats` / `searchPublicChat` / `checkChatInviteLink`                | `joined_rooms` / `get_room_state` / `preview_by_invite` | `get_participating` / `get_metadata` / `get_invite_info`                            |
| **E. Handoff**    | `transfer / demote_self / approve_handoff`                                     | `transferChatOwnership` (built-in!)                                    | `set_user_power_level` then leave                       | `promote_participants` + `demote_participants` + `leave`                            |

The **honest finding**: the abstraction works for A, B, D, E across
all Tier-1 platforms, but C is messy (each platform has a different
set of modes; the trait would need a "modes" capability flag). And
**all five categories are inapplicable to Tier-2/3/4 platforms** —
Nostr, Bluesky, Twitter, BLE, LoRa, QUIC, WebRTC, Webhook, nativep2p —
which either have no group concept or have one but the local
adapter is webhook/bot-restricted and can't administer it.

The proposed design is **a separate `CoordinatorAdmin` trait, parallel
to `PlatformAdapter`, with default-`Unimplemented` methods and a
`AdminCapabilityReport` companion struct**. Adapters opt in by
implementing the trait; callers do `as_any().downcast_ref()` or a
new `dyn-compatible` upcast pattern to call admin methods on
adapters that support them. This mirrors the `upload_media` /
`download_media` default-`Unimplemented` pattern already in
`PlatformAdapter`.

---

## 1. Why a separate trait, not a method on `PlatformAdapter`

`PlatformAdapter` has 12 methods today. Every one of them is on the
"hot path" of envelope delivery. Adding 10+ more admin methods to
it would:

1. **Bloat the trait** — 22+ methods, mostly `Unimplemented` for
   adapters that don't support them.
2. **Confuse the contract** — `PlatformAdapter` says "I carry
   envelopes in and out of a domain". Admin says "I manage the
   domain itself". These are different responsibilities.
3. **Block FFI/wasm** — every admin method would have to be exposed
   through the C ABI even for adapters that don't support it,
   turning `extern "C"` exports into a swiss-army-knife.

The `upload_media` / `download_media` pattern is the right precedent:
optional capability, default `Unimplemented`, adapter opts in by
overriding. We do the same for admin: a **new trait**,
`CoordinatorAdmin`, with a **new capability report**,
`AdminCapabilityReport`, and the caller does a downcast-or-feature-
detection step before invoking.

---

## 2. The five categories in detail

### A. Lifecycle (create, join, leave, destroy)

| Use case                | WhatsApp                                                         | Telegram                                                         | Matrix                                           | Discord         | Slack           | Signal                        | IRC                                                   | Nostr       | Webhook |
| ----------------------- | ---------------------------------------------------------------- | ---------------------------------------------------------------- | ------------------------------------------------ | --------------- | --------------- | ----------------------------- | ----------------------------------------------------- | ----------- | ------- |
| Create new group        | ✅ `create_group`                                                | ✅ `createNewSupergroupChat`                                     | ✅ `create_room`                                 | ❌ webhook-only | ❌ webhook-only | ⚠️ via signal-cli (new-group) | ⚠️ via raw `/JOIN` + `/TOPIC`                         | ❌ no group | ❌ N/A  |
| Join existing (by ID)   | ❌ (member must be added)                                        | ❌ (member must be added)                                        | ✅ `join_room`                                   | ❌              | ❌              | ❌                            | ✅ `JOIN`                                             | ❌          | ❌      |
| Join via invite code    | ❌ (link is for humans)                                          | ✅ `addChatMember` by invite hash                                | ✅ `join_room_by_id_or_alias`                    | ❌              | ❌              | ❌                            | ✅ (raw `JOIN #chan`)                                 | ❌          | ❌      |
| Leave                   | ✅ `leave` (R19)                                                 | ✅ `leaveChat`                                                   | ✅ `leave_room`                                  | ❌              | ❌              | ✅                            | ✅ `PART`                                             | ❌          | ❌      |
| Destroy (group is gone) | ⚠️ no native protocol op; "leave + revoke invite" is the closest | ⚠️ `deleteChatHistory` deletes messages but not the group itself | ⚠️ `leave_room` + tombstone event; rooms persist | ❌              | ❌              | ❌                            | ⚠️ no protocol op; group dies when last member leaves | ❌          | ❌      |

**Insight on "destroy":** WhatsApp, Telegram, Matrix, IRC, and
Signal all lack a "group is gone for good" operation. You can leave
and revoke the invite, but the group still exists server-side. This
is a real divergence from how a typical user thinks about "deleting"
a group, and the trait must reflect that. The proposed
`destroy_group` method should be **optional and best-effort**:
"leave the group and revoke the invite link; the platform may still
retain a tombstone".

### B. Membership (add, remove, promote, demote, ban)

| Use case                     | WhatsApp                                  | Telegram                                | Matrix                       | Discord      | Slack | Signal                | IRC                  |
| ---------------------------- | ----------------------------------------- | --------------------------------------- | ---------------------------- | ------------ | ----- | --------------------- | -------------------- |
| Add member                   | ✅ `add_members` (R19)                    | ✅ `addChatMember`                      | ✅ `invite_user` (3rd party) | ❌ (webhook) | ❌    | ❌                    | ❌                   |
| Remove member (kick)         | ⚠️ `remove_participants` (not yet wired)  | ✅ `setChatMemberStatus(Left)`          | ✅ `kick`                    | ❌           | ❌    | ❌                    | ✅ `KICK`            |
| Ban (can't rejoin)           | ❌ no native ban                          | ✅ `banChatMember`                      | ✅ `ban`                     | ❌           | ❌    | ❌                    | ✅ `KICK` + ban-list |
| Promote to admin             | ⚠️ `promote_participants` (not yet wired) | ✅ `setChatMemberStatus(Administrator)` | ✅ `set_user_power_level`    | ❌           | ❌    | ⚠️ only owner can add | ❌                   |
| Demote from admin            | ⚠️ `demote_participants` (not yet wired)  | ✅ same as above                        | ✅ same as above             | ❌           | ❌    | ❌                    | ❌                   |
| Approve pending join request | ❌                                        | ✅ `processChatJoinRequest`             | ✅ accept invite event       | ❌           | ❌    | ⚠️ via signal-cli     | ⚠️ INVITE-list only  |
| Get current members          | ✅ `get_metadata` (R19)                   | ✅ `getChatMembers`                     | ✅ `joined_members`          | ❌           | ❌    | ⚠️                    | ✅ `NAMES` / `WHO`   |

**Insight on "ban":** WhatsApp has no ban — once removed, the user
can rejoin if they have the invite. Matrix's ban is enforced by
`m.room.ban` state event. IRC has operator-set ban lists. The trait
should expose `ban_member(peer, duration)` with `duration: None`
meaning "indefinite". Adapters that can't implement indefinite
(WhatsApp) return `Err(Unsupported)` and callers fall back to
`remove_member` + a local deny-list (the typical "coordinator-level
ban" pattern).

### C. Mode (lock, announce, ephemeral, approve-required, description)

| Mode flag                                            | WhatsApp | Telegram                          | Matrix              | Discord      | Slack | Signal | IRC                      |
| ---------------------------------------------------- | -------- | --------------------------------- | ------------------- | ------------ | ----- | ------ | ------------------------ |
| `set_subject` (rename)                               | ✅       | ✅ `setChatTitle`                 | ✅ `set_room_name`  | ❌ (webhook) | ❌    | ❌     | ✅ `TOPIC`               |
| `set_description`                                    | ✅       | ⚠️ only at create                 | ✅ `set_room_topic` | ❌           | ❌    | ❌     | ⚠️ `TOPIC` doubles       |
| `set_locked` (only admins can add)                   | ✅       | ⚠️ via `setChatPermissions`       | ✅ power levels     | ❌           | ❌    | ❌     | ✅ `MODE +l`             |
| `set_announce` (only admins can post)                | ✅       | ⚠️ via `setChatPermissions`       | ✅ power levels     | ❌           | ❌    | ❌     | ✅ `MODE +m` (moderated) |
| `set_ephemeral` (disappearing messages)              | ✅       | ✅ `setChatMessageAutoDeleteTime` | ✅ state event      | ❌           | ❌    | ⚠️     | ❌                       |
| `set_membership_approval` (require approval to join) | ✅       | ⚠️ via invite link flag           | ✅ state event      | ❌           | ❌    | ❌     | ❌                       |

**Insight on "modes":** this is the messiest category. Each platform
has a different set of toggles, and some (Telegram) require composing
a `chatPermissions` struct to express what WhatsApp exposes as a
single boolean. The proposed design: each mode is a separate method
on the trait, defaulting to `Err(Unsupported)`. Adapters that
implement a mode override that one method. The capability report
exposes which modes each adapter supports, so callers can do
"set announce mode on this group; fall back gracefully if the adapter
doesn't support it".

### D. Discovery (list, lookup, resolve invite)

| Use case                           | WhatsApp                                      | Telegram                 | Matrix                 | IRC                                     |
| ---------------------------------- | --------------------------------------------- | ------------------------ | ---------------------- | --------------------------------------- |
| List groups I'm in                 | ✅ `get_participating`                        | ✅ `getChats`            | ✅ `joined_rooms`      | ❌ (no protocol op; usually `LIST` raw) |
| Get metadata for a group           | ✅ `get_metadata` (R19)                       | ✅ `getChat`             | ✅ `get_room_state`    | ⚠️ `TOPIC` (limited)                    |
| Resolve invite code/URL → group_id | ✅ `get_invite_info` (chat.whatsapp.com code) | ✅ `checkChatInviteLink` | ✅ `preview_by_invite` | ❌                                      |

**Insight:** discovery is the _prerequisite_ for the sidecar-persisted
`created_groups` pattern from the R19 follow-up discussion: at
startup, an adapter can call `get_participating`, intersect with
its persisted `created_groups` list, and `leave_group` any orphans
(scenario B from the prior conversation). WhatsApp already has the
primitives for this; the missing piece is wiring them up.

### E. Handoff (transfer ownership, atomic handoff)

| Use case                      | WhatsApp                          | Telegram                   | Matrix                               |
| ----------------------------- | --------------------------------- | -------------------------- | ------------------------------------ |
| Transfer ownership (built-in) | ❌ (use promote + demote + leave) | ✅ `transferChatOwnership` | ⚠️ set power_level to 100 then leave |
| Atomic promote-and-demote     | ⚠️ two-step (no transaction)      | ✅ via status set          | ✅ via two state events              |
| Quorum-gated handoff          | ❌                                | ❌                         | ⚠️ custom logic on top               |

**Insight:** Telegram's `transferChatOwnership` is the only
first-class "give this group to someone else" primitive in the
survey. WhatsApp and Matrix require a two-step dance. The trait
exposes `transfer_ownership(peer) -> Result<()>` and adapters
implement it as best they can (one-step on Telegram, two-step on
WhatsApp, the same two-step on Matrix). The `Err(Unsupported)`
fallback is for adapters where the dance isn't possible (Discord,
Slack, Signal, IRC).

---

## 3. Per-platform capability matrix (the "what can the local adapter actually do today?" table)

This is the critical nuance: even when a _platform_ supports an
admin action, the _adapter_ might not be able to use it (because
the adapter is webhook-only, or because the upstream library
doesn't expose it, or because the feature is gated behind a flag).

| Adapter                                                       | Mode                  | Real admin surface today                                                                                                                                                                                                                                                                                                                                                                                                        | What it could plausibly grow into                                                                                                                                                                                                  |
| ------------------------------------------------------------- | --------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `octo-adapter-whatsapp`                                       | Bot (whatsapp-rust)   | ✅ R19: create_group, add_members, get_invite_link, leave_group, group_metadata                                                                                                                                                                                                                                                                                                                                                 | + remove_members, promote/demote, get_participating, set_subject/description, set_announce/locked, set_ephemeral, get_invite_info, set_membership_approval                                                                         |
| `octo-adapter-telegram`                                       | TDLib (user mode)     | ⚠️ read-only `ChatResolver` (resolve by name/username/invite)                                                                                                                                                                                                                                                                                                                                                                   | + createNewSupergroupChat, addChatMember, banChatMember, setChatMemberStatus, transferChatOwnership, setChatTitle, setChatPermissions, setChatMessageAutoDeleteTime, createChatInviteLink, getChats, getChat                       |
| `octo-adapter-matrix` (HTTP)                                  | App service token     | ❌ nothing                                                                                                                                                                                                                                                                                                                                                                                                                      | + create_room, join_room, leave_room, kick, ban, invite_user, set_room_name, set_room_topic, joined_rooms, get_room_state                                                                                                          |
| `octo-adapter-matrix-sdk`                                     | matrix-sdk Rust crate | ✅ mission 0850h-d: create_group, add_member, remove_member, ban_member, promote_to_admin, demote_from_admin, approve_join_request, rename_group, set_group_description, set_locked, set_announce, set_ephemeral, set_require_approval, list_own_groups, get_group_metadata, resolve_invite, join_by_invite, join_by_id (18 of 24; can_destroy=false, can_transfer_ownership=false; see `AdminCapabilityReport` for full flags) | Covered via `CoordinatorAdmin` trait (RFC-0861). Remaining 6 methods (leave_group, destroy_group, list_own_groups_with_invites, transfer_ownership, leave_group_with_invites — deferred to mission 0850h-e for live test coverage) |
| `octo-adapter-discord`                                        | Webhook only          | ❌                                                                                                                                                                                                                                                                                                                                                                                                                              | ❌ (webhook URLs can't create/manage channels; a "Discord bot mode" adapter would unlock this)                                                                                                                                     |
| `octo-adapter-slack`                                          | Webhook only          | ❌                                                                                                                                                                                                                                                                                                                                                                                                                              | ❌ (same as Discord)                                                                                                                                                                                                               |
| `octo-adapter-irc`                                            | Raw TCP               | ✅ mission 0861: create_group, leave_group, add_member (INVITE), remove_member (KICK), ban_member, promote_to_admin, demote_from_admin, rename_group, set_group_description, set_locked, set_announce, set_ephemeral, set_require_approval, list_own_groups, get_group_metadata, join_by_id, health_check (TLS-aware); can_destroy=false, can_transfer_ownership=false                                                          | Covered via `CoordinatorAdmin` trait (RFC-0861). Remaining: resolve_invite, join_by_invite, approve_join_request, list_own_groups_with_invites                                                                                     |
| `octo-adapter-signal`                                         | signal-cli daemon     | ⚠️ receive only (the adapter reads via signal-cli)                                                                                                                                                                                                                                                                                                                                                                              | + send (for admin actions), group create via signal-cli                                                                                                                                                                            |
| `octo-adapter-nostr`                                          | NIP-01 (synthetic)    | ❌ no group concept                                                                                                                                                                                                                                                                                                                                                                                                             | N/A — but the adapter could expose "create a long-lived kind:40 group metadata event" as a synthetic group                                                                                                                         |
| `octo-adapter-bluesky`                                        | AT Protocol           | ❌ no group concept                                                                                                                                                                                                                                                                                                                                                                                                             | N/A                                                                                                                                                                                                                                |
| `octo-adapter-twitter`                                        | X API                 | ❌ no group concept                                                                                                                                                                                                                                                                                                                                                                                                             | N/A (DMs are 1:1 only)                                                                                                                                                                                                             |
| `octo-adapter-bluetooth`                                      | BLE GATT              | ❌ 1:1 transport                                                                                                                                                                                                                                                                                                                                                                                                                | N/A                                                                                                                                                                                                                                |
| `octo-adapter-lora`                                           | LoRa radio            | ❌ 1:1 transport                                                                                                                                                                                                                                                                                                                                                                                                                | N/A                                                                                                                                                                                                                                |
| `octo-adapter-quic`                                           | QUIC stream           | ❌ 1:1 transport                                                                                                                                                                                                                                                                                                                                                                                                                | N/A                                                                                                                                                                                                                                |
| `octo-adapter-webrtc`                                         | WebRTC data channel   | ❌ 1:1 transport                                                                                                                                                                                                                                                                                                                                                                                                                | N/A                                                                                                                                                                                                                                |
| `octo-adapter-webhook`                                        | HTTP POST             | ❌ 1:1 transport (1 URL)                                                                                                                                                                                                                                                                                                                                                                                                        | N/A                                                                                                                                                                                                                                |
| `octo-adapter-p2p` (gossipsub)                                | libp2p                | ⚠️ topics are implicit (subscribe publishes a topic; no admin)                                                                                                                                                                                                                                                                                                                                                                  | ⚠️ gossipsub has no admin — but the adapter could expose "create namespace", "set peer allowlist per topic"                                                                                                                        |
| `octo-adapter-wechat` / `dingtalk` / `lark` / `qq` / `reddit` | Webhook stubs         | ❌                                                                                                                                                                                                                                                                                                                                                                                                                              | TBD — depends on the platform's bot API                                                                                                                                                                                            |

**Key takeaway:** of 20 adapters, **2** (WhatsApp R19, partially
IRC) currently expose any group admin action. **6** could plausibly
add it (Telegram TDLib, matrix-sdk, the upgraded matrix HTTP, IRC,
Signal, NativeP2P for synthetic namespaces). **12** never will, either
because the transport has no group concept (Tier 3) or the adapter's
mode doesn't support it (Discord/Slack webhooks).

---

## 4. Proposed design: `CoordinatorAdmin` trait

```rust
// In crates/octo-network/src/dot/adapters/coordinator_admin.rs

use async_trait::async_trait;
use crate::dot::adapters::PlatformAdapterError;
use crate::dot::domain::BroadcastDomainId;

/// Bit-flags describing which admin actions an adapter supports.
#[derive(Clone, Debug, Default)]
pub struct AdminCapabilityReport {
    /// Can create new groups on this platform.
    pub can_create: bool,
    /// Can join existing groups (by ID or invite code).
    pub can_join: bool,
    /// Can leave groups the adapter is a member of.
    pub can_leave: bool,
    /// Can delete/destroy groups (best-effort: leave + revoke invite).
    pub can_destroy: bool,
    /// Can add members.
    pub can_add_member: bool,
    /// Can remove members (kick).
    pub can_remove_member: bool,
    /// Can ban members (kick + prevent rejoin).
    pub can_ban: bool,
    /// Can promote a member to admin.
    pub can_promote: bool,
    /// Can demote an admin to regular member.
    pub can_demote: bool,
    /// Can approve a pending join request.
    pub can_approve_join: bool,
    /// Can rename the group.
    pub can_rename: bool,
    /// Can set/change the group description / topic.
    pub can_describe: bool,
    /// Can lock membership (only admins add).
    pub can_lock: bool,
    /// Can enable announce-only mode.
    pub can_announce: bool,
    /// Can enable ephemeral / disappearing messages.
    pub can_set_ephemeral: bool,
    /// Can require approval for new joiners.
    pub can_require_approval: bool,
    /// Can list groups the adapter is a member of.
    pub can_list_own_groups: bool,
    /// Can fetch metadata for a specific group.
    pub can_get_metadata: bool,
    /// Can resolve an invite code/URL to a group ID.
    pub can_resolve_invite: bool,
    /// Can transfer ownership atomically (true only on Telegram).
    pub can_transfer_ownership: bool,
}

/// Coordinator / admin actions on a group. Optional capability;
/// adapters that support any of these implement this trait and
/// override only the methods they support. The default for every
/// method is `Err(Unimplemented)`.
#[async_trait]
pub trait CoordinatorAdmin: Send + Sync {
    /// Report which admin actions this adapter supports.
    fn admin_capabilities(&self) -> AdminCapabilityReport {
        AdminCapabilityReport::default()
    }

    // ── A. Lifecycle ──────────────────────────────────────────

    /// Create a new group with `subject`. Returns the new group ID
    /// (platform-native shape — adapter-specific). The adapter
    /// becomes the creator/admin by default.
    async fn create_group(
        &self,
        subject: &str,
        initial_members: &[GroupMemberSpec],
    ) -> Result<GroupHandle, PlatformAdapterError> {
        Err(PlatformAdapterError::Unimplemented { /* … */ })
    }

    /// Leave a group the adapter is a member of. Idempotent.
    async fn leave_group(
        &self,
        group_id: &GroupId,
    ) -> Result<(), PlatformAdapterError> {
        Err(PlatformAdapterError::Unimplemented { /* … */ })
    }

    /// Best-effort destroy: leave the group and revoke the invite
    /// link. May not fully remove the group server-side; the caller
    /// should not assume the group ID becomes invalid.
    async fn destroy_group(
        &self,
        group_id: &GroupId,
    ) -> Result<(), PlatformAdapterError> {
        Err(PlatformAdapterError::Unimplemented { /* … */ })
    }

    // ── B. Membership ─────────────────────────────────────────

    async fn add_member(
        &self,
        group_id: &GroupId,
        member: &GroupMemberSpec,
    ) -> Result<(), PlatformAdapterError> { … }
    async fn remove_member(
        &self,
        group_id: &GroupId,
        member: &PeerId,
    ) -> Result<(), PlatformAdapterError> { … }
    async fn ban_member(
        &self,
        group_id: &GroupId,
        member: &PeerId,
        duration: Option<std::time::Duration>,
    ) -> Result<(), PlatformAdapterError> { … }
    async fn promote_to_admin(
        &self,
        group_id: &GroupId,
        member: &PeerId,
    ) -> Result<(), PlatformAdapterError> { … }
    async fn demote_from_admin(
        &self,
        group_id: &GroupId,
        member: &PeerId,
    ) -> Result<(), PlatformAdapterError> { … }
    async fn approve_join_request(
        &self,
        group_id: &GroupId,
        requester: &PeerId,
    ) -> Result<(), PlatformAdapterError> { … }

    // ── C. Mode ───────────────────────────────────────────────

    async fn rename_group(/* … */) -> Result<(), PlatformAdapterError> { … }
    async fn set_group_description(/* … */) -> Result<(), PlatformAdapterError> { … }
    async fn set_locked(/* … */, locked: bool) -> Result<(), PlatformAdapterError> { … }
    async fn set_announce(/* … */, announce_only: bool) -> Result<(), PlatformAdapterError> { … }
    async fn set_ephemeral(/* … */, ttl: std::time::Duration) -> Result<(), PlatformAdapterError> { … }
    async fn set_require_approval(/* … */, require: bool) -> Result<(), PlatformAdapterError> { … }

    // ── D. Discovery ──────────────────────────────────────────

    async fn list_own_groups(&self) -> Result<Vec<GroupHandle>, PlatformAdapterError> { … }
    async fn get_group_metadata(&self, group_id: &GroupId) -> Result<GroupMetadata, PlatformAdapterError> { … }
    async fn resolve_invite(&self, invite: &InviteRef) -> Result<GroupHandle, PlatformAdapterError> { … }

    // ── E. Handoff ────────────────────────────────────────────

    async fn transfer_ownership(
        &self,
        group_id: &GroupId,
        new_owner: &PeerId,
    ) -> Result<(), PlatformAdapterError> { … }
}
```

Supporting types:

```rust
/// Platform-agnostic reference to a group. Adapters translate this
/// to/from their native group ID format (JID, chat_id, room_id, etc.).
#[derive(Clone, Debug)]
pub struct GroupId(pub String);

/// Platform-agnostic reference to a peer. Adapters translate to/from
/// their native handle (phone, @username, mxid, did, pubkey, etc.).
#[derive(Clone, Debug)]
pub struct PeerId(pub String);

/// What a caller specifies when creating a group / adding a member.
/// "Just give me a name and the initial roster; I don't care which
/// platform-native format each member is identified in."
#[derive(Clone, Debug)]
pub struct GroupMemberSpec {
    pub handle: String, // phone / @user / mxid / pubkey
    pub display_name: Option<String>,
    pub is_admin: bool,
}

/// Opaque handle returned by create / resolve_invite. Includes the
/// `GroupId` plus adapter-specific metadata (subject, member count,
/// invite URL, etc.) so callers don't have to call get_metadata
/// immediately after create.
#[derive(Clone, Debug)]
pub struct GroupHandle {
    pub id: GroupId,
    pub subject: Option<String>,
    pub invite_url: Option<String>,
    pub is_admin: bool,
    pub member_count: Option<u32>,
    /// Platform-native handle (e.g. raw JID, chat_id, room_id). Useful
    /// for adapter-specific paths that don't go through the trait.
    pub platform_native: Option<String>,
}

/// Opaque reference to an invite code or URL.
#[derive(Clone, Debug)]
pub struct InviteRef(pub String);

#[derive(Clone, Debug, Default)]
pub struct GroupMetadata {
    pub id: GroupId,
    pub subject: Option<String>,
    pub description: Option<String>,
    pub members: Vec<PeerId>,
    pub admins: Vec<PeerId>,
    pub invite_url: Option<String>,
    pub mode_flags: GroupModeFlags,
}

#[derive(Clone, Debug, Default)]
pub struct GroupModeFlags {
    pub locked: bool,
    pub announce_only: bool,
    pub ephemeral_ttl: Option<std::time::Duration>,
    pub requires_approval: bool,
}
```

### How callers use it

```rust
use octo_network::dot::adapters::coordinator_admin::CoordinatorAdmin;

async fn maybe_destroy(
    adapter: &(dyn PlatformAdapter + '_),
    group_id: &GroupId,
) -> Result<(), PlatformAdapterError> {
    // Pattern A: downcast (if the adapter type is statically known)
    if let Some(whatsapp) = (adapter as &dyn std::any::Any)
        .downcast_ref::<WhatsAppWebAdapter>()
    {
        whatsapp.destroy_group(group_id).await?;
        return Ok(());
    }

    // Pattern B: feature-detect via the capability report
    if let Some(admin) = adapter.as_coordinator_admin() {
        if admin.admin_capabilities().can_destroy {
            return admin.destroy_group(group_id).await;
        }
    }

    // Fallback: best-effort leave
    if let Some(admin) = adapter.as_coordinator_admin() {
        if admin.admin_capabilities().can_leave {
            return admin.leave_group(group_id).await;
        }
    }

    Err(PlatformAdapterError::Unimplemented { /* … */ })
}
```

The `as_coordinator_admin()` method on `PlatformAdapter` is a
default `None` impl; adapters that support admin override it to
return `Some(self)`. This is a clean opt-in pattern that doesn't
require `dyn` to be compatible with upcasting (a Rust nightly feature
at time of writing).

### Why not just put it all on `PlatformAdapter`?

Three reasons (recapping §1):

1. **Trait bloat.** `PlatformAdapter` is the hot path. Adding 20+
   methods to it makes the trait 3x larger for adapters that
   implement 0 of them.
2. **FFI cost.** Every `extern "C"` export on a plugin adapter
   would have to expose all 20+ admin methods, even though most
   return `Unimplemented` for that platform. The plugin ABI explodes.
3. **Conceptual cleanness.** `PlatformAdapter` says "I carry envelopes
   in a domain". `CoordinatorAdmin` says "I manage the domain itself".
   Different responsibilities, different trait, same file, same module.

### Why not a single `dyn PlatformAdmin`?

We need both. A `CoordinatorAdmin` can exist on an adapter that is
_not_ an envelope transport (e.g. a "test admin shim" that creates
groups but never carries envelopes). Conversely, an adapter can be
an envelope transport with no admin powers (most of the 20 today).
Two separate traits, each with their own capability report, models
the real world.

---

## 5. Backwards-compatibility plan

If we add this trait now:

- **`PlatformAdapter` is unchanged.** No method added, no method
  removed. Existing adapters compile and run identically.
- **`CoordinatorAdmin` is a new trait.** Adapters that don't
  implement it simply aren't usable for admin actions; callers
  get `Unimplemented` and fall back. No breaking change.
- **WhatsApp R19 methods stay on `WhatsAppWebAdapter`.** We add
  a `impl CoordinatorAdmin for WhatsAppWebAdapter` block that
  delegates to the existing R19 methods. No rename, no deprecation,
  no migration. (The R19 methods can stay as the _adapter-specific_
  API; the trait methods become the _uniform_ API. Both work.)
- **Other adapters opt in incrementally.** Telegram, matrix-sdk,
  and IRC are the natural next adopters; the trait gives them a
  target to aim at without forcing immediate work.

---

## 6. Migration order (recommended)

1. **Land the trait** (`coordinator_admin.rs`) and the supporting
   types in `octo-network`. No callers, no adapter changes.
   ~150 LOC + ~20 doc tests for the type definitions.
2. **Implement for WhatsApp** in `octo-adapter-whatsapp` as a
   delegating wrapper around the R19 methods. ~80 LOC. Also wire
   up the missing primitives (`remove_members`, `promote/demote`,
   `get_participating`, `set_subject`, `set_description`,
   `set_announce`, `set_locked`, `set_ephemeral`, `get_invite_info`)
   — these are the natural next batch from the prior conversation.
3. **Implement for IRC** by exposing the raw protocol ops
   (`JOIN`, `PART`, `TOPIC`, `MODE`, `KICK`) as adapter methods
   and wrapping them in the trait. ~120 LOC. _This is the
   missing-in-action adapter for many "coordinator bot in a
   public IRC channel" use cases._
4. **Implement for matrix-sdk** (the SDK already exposes
   `create_room`, `join_room`, `leave_room`, `kick`, `ban`,
   `set_room_name`, etc. — the upgrade is small).
5. **Implement for Telegram** by adding new methods to
   `TelegramClient` trait that wrap the TDLib admin calls
   (`createNewSupergroupChat`, `addChatMember`, `banChatMember`,
   `setChatMemberStatus`, `transferChatOwnership`, etc.). Then
   `impl CoordinatorAdmin for TelegramAdapter` delegating to the
   client. ~200 LOC + offline unit tests + a live E2E test
   (mirroring the R19 WhatsApp pattern).
6. **Stop here.** Discord, Slack, Bluesky, Twitter, Webhook,
   BLE, LoRa, QUIC, WebRTC, nativep2p never get admin — they're
   either webhook-only or have no group concept. Document this
   explicitly in each adapter's `//!` module doc.

---

## 7. Summary table

| Adapter                                    | Admin surface today         | After migration step 6             |
| ------------------------------------------ | --------------------------- | ---------------------------------- |
| whatsapp                                   | ✅ R19 (5 methods)          | ✅ full set (~20 methods)          |
| telegram                                   | ⚠️ ChatResolver (read-only) | ✅ full set via TDLib              |
| matrix                                     | ❌                          | ✅ full set via matrix-sdk         |
| matrix-sdk                                 | ❌                          | ✅ full set via SDK                |
| irc                                        | ⚠️ raw protocol ops         | ✅ full set via raw protocol       |
| signal                                     | ❌                          | ⚠️ partial (depends on signal-cli) |
| discord                                    | ❌ (webhook)                | ❌ (would need bot-mode adapter)   |
| slack                                      | ❌ (webhook)                | ❌ (same)                          |
| bluesky                                    | ❌                          | ❌ (no group concept)              |
| twitter                                    | ❌                          | ❌ (no group concept)              |
| wechat / dingtalk / lark / qq / reddit     | ❌ (stubs)                  | TBD per platform                   |
| bluetooth / lora / quic / webrtc / webhook | ❌ (1:1)                    | ❌ (no group concept)              |
| p2p (gossipsub)                            | ❌                          | ⚠️ partial (synthetic namespaces)  |
| nativep2p                                  | ❌                          | ⚠️ partial                         |

**Net assessment:** the abstraction works for **5 of 20** adapters
(WhatsApp, Telegram, matrix, matrix-sdk, IRC) immediately, **2 of 20**
(Signal, nativep2p) partially, and never for the other **13**. That's
fine — the trait is a `default = Unimplemented` opt-in, and the
13 adapters that don't implement it are honest about it via
`admin_capabilities()` returning all-`false`. The trait makes the
coordinator surface uniform across the platforms that _can_ support
it, and explicit about the platforms that _can't_.

---

## Appendix A: Cross-references

- Prior research: [`docs/research/group-coordination-transport-adapters.md`](group-coordination-transport-adapters.md)
  (the 4-tier model and the existing 12-adapter survey).
- R19 commit: `f86c580` "live WhatsApp E2E test for coordinator group
  setup + runtime_groups fix" — the 5 WhatsApp admin methods that
  motivate this doc.
- R18 commits: per-platform `domain_id` / `send_envelope` fixes
  (the per-adapter concerns this doc doesn't repeat).
- E2E test plan: `docs/e2e/2026-06-16-e2e-test-plan.md` — the
  scenario-1 cold-start flow this doc extends.

## Appendix B: Open questions

1. **Should `create_group`'s `initial_members` be `GroupMemberSpec`
   (uniform shape) or `&[&str]` (per-adapter format)?** My take:
   `GroupMemberSpec` (uniform). Callers don't have to know the
   adapter's native format. The adapter translates internally
   (phone → `<digits>@s.whatsapp.net`, handle → JID, pubkey → NIP-19, etc.).
2. **What about `create_group` returning a `GroupId` that the caller
   can use immediately, or requiring a follow-up `get_metadata`?**
   My take: `create_group` returns a `GroupHandle` with subject +
   invite_url already populated. `get_metadata` is for re-querying
   later, not for the just-created snapshot.
3. **Should the trait be `Send + Sync`?** My take: yes, mirror
   `PlatformAdapter`. Admin operations may take seconds (network
   round-trips) and must be cancellable.
4. **Naming: `CoordinatorAdmin` vs `GroupAdmin` vs `BroadcastGroupAdmin`?**
   My take: `CoordinatorAdmin` — the coordinator is the role; the
   group is the object. The doc-string on the trait makes the
   relationship explicit.
5. **Should `GroupId` carry a `PlatformType`?** My take: yes, a
   `GroupId(platform: PlatformType, native: String)`. This makes
   cross-platform handoff safer (you can't accidentally hand a
   WhatsApp JID to a Telegram adapter).

---

## 7. Implementation status (appended 2026-06-18+)

The research above was the _plan_. This section tracks the
_execution_ — what's been built, what's pending. Updated as each
R-series lands.

### Done

- **R20 (commit `03315ae`):** `coordinator_admin.rs` trait +
  type newtypes + WhatsApp adapter impl. 20/22 actions
  implemented on WhatsApp; honest `Unimplemented` for the rest.
- **R21 (commit `48056b9`):** `octo-adapter-irc` impl.
  Truthful capability report (10 supported / 11 unsupported);
  raw IRC protocol ops (`KICK`, `MODE +o/-o/+i/-i/+m/-m`,
  `TOPIC`, `INVITE`, `JOIN`, `PART`) wrapped in the trait;
  admin command channel added to the listener task with
  `tokio::select!` so commands make forward progress even
  when the listener is blocked on `read_line`.

### In progress / pending

- **R22:** `octo-adapter-matrix` impl (hand-rolled reqwest
  HTTP; rich power-level / join-rules / state-event model).
- **R23:** `octo-adapter-telegram` impl (gated `real-tdlib`).
- **R-toolchain + R24:** bump rustc 1.92 → 1.93 to unblock
  `matrix-sdk 0.17`, then `octo-adapter-matrix-sdk` impl.
- **R25:** wire a `CoordinatorAdmin` consumer in the gateway /
  coordinator daemon (the "step 4" of the original migration
  order).
