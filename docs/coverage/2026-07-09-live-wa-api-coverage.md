# Live WA API Coverage Matrix

> **Status:** source-of-truth document for live-test scope.
> **Date:** 2026-07-09.
> **Owner:** `octo-whatsapp`.
> **Companion plan:** `.claude/plans/cryptic-percolating-octopus.md`.

This matrix enumerates **every public method exposed by the WhatsApp client crates** (`whatsapp-rust`, `wacore`, `wacore_binary`, `waproto` — git pin `6e0f241dc0…` from `oxidezap/whatsapp-rust`) and classifies each one along three axes:

1. **Is it wrapped by `octo-whatsapp` as a daemon RPC?**
2. **Does a live test exercise it end-to-end against real WA servers?**
3. **What `events_query::wait_for(predicate, timeout)` assertion proves it worked?**

A row is **covered** only when the live test asserts a real `InboundEvent` lands in the daemon's events buffer after the wire action. Anything less is **partial**, **gap:rpc**, **gap:test**, or **internal**.

## Status legend

| Status | Meaning |
|---|---|
| `covered` | RPC exists AND a live test asserts the event lands in the buffer |
| `partial` | RPC exists AND a live test runs, but only a subset of the WA method's behavior is verified |
| `gap:rpc` | WA method exists, no `octo-whatsapp` RPC. Needs a new handler + live test |
| `gap:test` | RPC exists, no live test. Needs a live test against real WA |
| `internal` | Runtime-internal lifecycle / IQ plumbing; consumed by the daemon but never user-facing |
| `n/a` | Out of scope for this matrix (paired-device features, voip feature flag not enabled, etc.) |

## Tier ordering

Per the plan agreement, live tests land in this order. Each tier blocks until the previous one is green on a real linked session.

| Tier | Category | New RPCs in this tier | Tests |
|---|---|---|---|
| 0 | Foundation (taxonomy fix + helpers) | `send.text` real dispatch | `live_connection_open_emits_event` |
| 1 | 1:1 text | — | `live_send_text_self`, `_peer`, `_quoted`, `_revoke`, `_oversize`, `_invalid_peer` |
| 2 | 1:1 media | — | `live_send_{image,video,document,audio,voice,sticker}` |
| 3 | Receipts | — | `live_receipt_{server_ack,delivered,read,played}` |
| 4 | Contacts + presence | `contacts.is_on_whatsapp`, `contacts.get_profile_picture`, `contact.{block,unblock}`, `presence.{subscribe,set_available,set_unavailable}` | per-method |
| 5 | Groups (TEST_MEMBER_2/3/4 needed) | `groups.get_invite_link` (and any other group ops missing in 1) | per-method |
| 6 | Sync + profile + privacy + meta | `profile.*`, `privacy.*`, `blocking.*`, `labels.*`, `polls.{vote,aggregate}`, `newsletter.*`, `status.*`, `events.*`, `tctoken.*`, `comments.*`, etc. | per-method |
| 7 | Identity + device + signal + passkey live | `identity.*`, `device.*` | per-method |

## Matrix: WA crate methods

Total public async/fn methods in `whatsapp-rust` + `wacore`: **~180**.

### 1. Connection / lifecycle (Tier 0)

| WA method | Crate:line | RPC | Status | Live test | `events.wait_for` predicate |
|---|---|---|---|---|---|
| `Client::new` | `client/lifecycle.rs:83` | (boot, internal) | `internal` | — | n/a — drives fixture boot |
| `Client::new_with_cache_config` | `:102` | (boot, internal) | `internal` | — | n/a |
| `Client::run` | `:322` | (boot, internal) | `internal` | — | n/a |
| `Client::connect` | `:444` | (boot, internal) | `internal` | — | n/a |
| `Client::disconnect` | `:616` | (boot, internal) | `internal` | — | n/a |
| `Client::logout` | `:588` | (boot, internal) | `internal` | — | n/a |
| `Client::reconnect` | `:694` | `reconnect.now` (transitive via `run_reconnect_loop`) | `covered` | `it_chain_h_daemon_control` | `InboundEvent::Connection { kind: Connected }` after reconnect.now |
| `Client::reconnect_immediately` | `:730` | covered by `reconnect.now` | `partial` | `it_chain_h_daemon_control` | same as above |
| `Client::wait_for_socket` | `:923` | (internal) | `internal` | — | n/a |
| `Client::wait_for_connected` | `:948` | (internal) | `internal` | — | n/a |
| `Client::is_connected` | `:969` | surfaced via `status.get` | `covered` | `live_connection_open_emits_event` | `InboundEvent::Connection { kind: Connected }` |
| `Client::is_logged_in` | `:980` | surfaced via `status.get` | `covered` | `it_chain_a_lifecycle` | `InboundEvent::Connection { kind: Connected }` |
| `Client::shutdown_signal` / `signal_shutdown_sync` | `:14`, `:24` | surfaced via `shutdown` | `internal` | — | `InboundEvent::Connection { kind: Disconnected }` (asserted implicitly on shutdown) |
| `Client::get_push_name` | `accessors.rs:236` | surfaced via `status.get` | `covered` | `it_chain_a_lifecycle` | no event — read directly via `status.get` |

### 2. 1:1 send (Tier 1)

| WA method | Crate:line | RPC | Status | Live test | `events.wait_for` predicate |
|---|---|---|---|---|---|
| `Client::send_text` | `send/mod.rs:523` | `send.text` (Phase 2 dispatch added) | `covered` | `live_send_text_self` (Tier 1) | `InboundEvent::Message { id == response.message_id, peer == self }` within 10s |
| `Client::send_message` | `send/mod.rs:506` | (via `envelope.send`) | `partial` | `it_chain_e_envelopes` | `InboundEvent::Message { peer, id }` for envelope-sent payload |
| `Client::forward_message` | `send/mod.rs:545` | — | `gap:rpc` | — | `InboundEvent::Message { id == forwarded_id }` (Tier 7) |
| `Client::send_message_with_options` | `send/mod.rs:559` | — | `gap:rpc` | — | `InboundEvent::Message` (Tier 7) |
| `Client::send_raw_bytes` | `messaging.rs:14` | — | `gap:rpc` | — | n/a — protocol-layer (Tier 7) |
| `Client::send_node` | `messaging.rs:25` | — | `gap:rpc` | — | n/a — protocol-layer (Tier 7) |
| `Client::edit_message` | `messaging.rs:59` | `messages.edit` | `covered` | `it_chain_d_sends` (will be `live_edit_message` Tier 1) | `InboundEvent::Message { peer, id == edited_id, text == new_text }` |
| `Client::edit_message_encrypted` | `messaging.rs:130` | — | `gap:rpc` | — | `InboundEvent::Message { id == edited_id }` (Tier 7) |
| `MessageActions::revoke_message` | `send/actions.rs:14` | `send.delete` | `covered` | `it_chain_d_sends` | `InboundEvent::Message { id == revoked_id, revoke=true }` on receiver side |
| `MessageActions::keep_message` | `send/actions.rs:96` | — | `gap:rpc` | — | n/a (Tier 7) |
| `MessageActions::pin_message` | `send/actions.rs:112` | — | `gap:rpc` | — | n/a (Tier 7) |
| `MessageActions::unpin_message` | `send/actions.rs:128` | — | `gap:rpc` | — | n/a (Tier 7) |
| `WhatsAppWebAdapter::send_document` | `adapter.rs:2570` | — (used only by `envelope.send-native`) | `gap:rpc` | — | `InboundEvent::Message { mime_type=application/pdf }` (Tier 2) |
| `Receipt::mark_as_read` | `receipt.rs:713` | `messages.mark_read` | `covered` | `it_chain_c_messages_chats` | `InboundEvent::Receipt { kind: Read, from_me: true }` |
| `Receipt::mark_as_played` | `receipt.rs:774` | — | `gap:rpc` | — | `InboundEvent::Receipt { kind: Played }` (Tier 3) |

### 3. 1:1 media send (Tier 2)

| WA method | Crate:line | RPC | Status | Live test | `events.wait_for` predicate |
|---|---|---|---|---|---|
| `MediaManager::upload` | `upload.rs:341` | (internal helper for `send.*`) | `internal` | — | n/a |
| `MediaManager::upload_stream` | `upload.rs:395` | (internal helper) | `internal` | — | n/a |
| `media::image_message` | `media.rs:67` | `send.image` (Phase 2) | `covered` | `it_chain_d_sends` | `InboundEvent::Message { mime_type: image/* }` |
| `media::video_message` | `media.rs:92` | `send.video` | `covered` | `it_chain_d_sends` | `InboundEvent::Message { mime_type: video/* }` |
| `media::document_message` | `media.rs:119` | `send.document` (via send.doc RPC) | `gap:rpc` | — | `InboundEvent::Message { mime_type: application/pdf }` (Tier 2) |
| `media::audio_message` | `media.rs:150` | `send.audio` | `covered` | `it_chain_d_sends` | `InboundEvent::Message { mime_type: audio/* }` |
| `WhatsAppWebAdapter::send_image` | `inherent.rs:27` | `send.image` | `covered` | `it_chain_d_sends` | `InboundEvent::Message { mime_type: image/jpeg }` |
| `WhatsAppWebAdapter::send_video` | `:121` | `send.video` | `covered` | `it_chain_d_sends` | `InboundEvent::Message { mime_type: video/mp4 }` |
| `WhatsAppWebAdapter::send_audio` | `:216` | `send.audio` | `covered` | `it_chain_d_sends` | `InboundEvent::Message { mime_type: audio/mpeg }` |
| `WhatsAppWebAdapter::send_voice` | `:307` | `send.voice` | `covered` | `it_chain_d_sends` | `InboundEvent::Message { voice=true }` |
| `WhatsAppWebAdapter::send_sticker` | `:399` | `send.sticker` | `covered` | `it_chain_d_sends` | `InboundEvent::Message { sticker=true }` |
| `MediaManager::download` | `download.rs:272` | `messages.download` | `covered` | `it_media_info` | n/a — read-out via RPC result, no event |
| `MediaManager::download_from_params` | `:335` | — | `gap:rpc` | — | n/a (Tier 7) |
| `MediaManager::download_to_writer` | `:370` | — | `gap:rpc` | — | n/a (Tier 7) |
| `MediaManager::download_from_params_to_writer` | `:387` | — | `gap:rpc` | — | n/a (Tier 7) |
| `MediaManager::fetch_sticker_pack` | `:313` | — | `gap:rpc` | — | n/a (Tier 7) |

### 4. Send non-media (Tier 1+)

| WA method | Crate:line | RPC | Status | Live test | `events.wait_for` predicate |
|---|---|---|---|---|---|
| `WhatsAppWebAdapter::send_reaction` | `inherent.rs:492` | `send.reaction` | `covered` | `it_chain_d_sends` | `InboundEvent::Reaction { id, emoji, target_msg_id }` on receiver side |
| `WhatsAppWebAdapter::send_poll` | `:562` | `send.poll` | `covered` | `it_chain_d_sends` | `InboundEvent::Message { kind: Poll, question, options }` on receiver side |
| `WhatsAppWebAdapter::send_contact` | `:633` | `send.contact` | `covered` | `it_chain_d_sends` | `InboundEvent::Message { kind: Contact }` on receiver side |
| `WhatsAppWebAdapter::send_location` | `:712` | `send.location` | `covered` | `it_chain_d_sends` | `InboundEvent::Message { kind: Location, lat, lon }` on receiver side |
| `Polls::create_quiz` | `features/polls.rs:67` | — | `gap:rpc` | — | `InboundEvent::Message { kind: Poll, is_quiz=true }` (Tier 6) |
| `Polls::vote` | `:120` | — | `gap:rpc` | — | `InboundEvent::Message { kind: PollVote }` (Tier 6) |
| `Polls::decrypt_vote` | `:211` | — | `gap:rpc` | — | n/a — local decrypt (Tier 6) |
| `Polls::aggregate_votes` | `:271` | — | `gap:rpc` | — | n/a — local aggregate (Tier 6) |

### 5. Chats (Tier 1 partial — Tier 7 closure)

| WA method | Crate:line | RPC | Status | Live test | `events.wait_for` predicate |
|---|---|---|---|---|---|
| `WhatsAppWebAdapter::chat_info` | `inherent.rs:1032` | `chats.info` | `covered` | `it_chain_c_messages_chats` | no event — read-only RPC |
| `WhatsAppWebAdapter::set_chat_pinned` | `:1094` | `chats.pin` / `chats.unpin` | `partial` | `it_chain_c_messages_chats` | `InboundEvent::PinUpdate { jid, pinned }` |
| `WhatsAppWebAdapter::set_chat_muted` | `:1109` | `chats.mute` | `partial` | `it_chain_c_messages_chats` | `InboundEvent::MuteUpdate { jid, muted_until }` |
| `WhatsAppWebAdapter::set_chat_archived` | `:1124` | `chats.archive` | `partial` | `it_chain_c_messages_chats` | `InboundEvent::ArchiveUpdate { jid, archived }` |
| `WhatsAppWebAdapter::delete_chat` | `:1140` | `chats.delete` | `partial` | `it_chain_c_messages_chats` | `InboundEvent::DeleteChatUpdate { jid }` |
| `ChatActions::archive_chat` / `unarchive_chat` | `chat_actions.rs:418,427` | `chats.archive` | `covered` | (same) | same |
| `ChatActions::pin_chat` / `unpin_chat` | `:439,448` | `chats.pin` / `chats.unpin` | `covered` | (same) | same |
| `ChatActions::mute_chat` / `mute_chat_until` / `unmute_chat` | `:457,464,473` | `chats.mute` | `covered` | (same) | same |
| `ChatActions::star_message` / `unstar_message` | `:471,483` | — | `gap:rpc` | — | `InboundEvent::StarUpdate { msg_id, starred }` (Tier 6) |
| `ChatActions::mark_chat_as_read` | `:519` | `messages.mark_read` | `covered` | (same) | `InboundEvent::Receipt { kind: Read }` |
| `ChatActions::delete_chat` | `:539` | `chats.delete` | `covered` | (same) | same |
| `ChatActions::clear_chat` | `:553` | — | `gap:rpc` | — | `InboundEvent::ClearChatUpdate { jid }` (Tier 6) |
| `ChatActions::set_user_status_mute` | `:583` | — | `gap:rpc` | — | `InboundEvent::UserStatusMuteUpdate { jid, muted }` (Tier 6) |
| `ChatActions::delete_message_for_me` | `:600` | — | `gap:rpc` | — | `InboundEvent::DeleteMessageForMeUpdate { msg_id }` (Tier 6) |
| `ChatActions::save_contact` | `:645` | — | `gap:rpc` | — | n/a — local store (Tier 6) |

### 6. Messages search (Tier 1)

| WA method | Crate:line | RPC | Status | Live test | `events.wait_for` predicate |
|---|---|---|---|---|---|
| `WhatsAppWebAdapter::message_search` | `inherent.rs:957` | `messages.search` | `covered` | `it_chain_c_messages_chats` | no event — read-only RPC |

### 7. Groups (Tier 5)

The 24 `CoordinatorAdmin` methods below are the existing wrapped surface. Phase 6.12 added 22 RPCs; Phase 6.1 added multi-account. The full gap list is ~22 more methods (group invite links, profile pics, member labels, join v4, etc.) — all `gap:rpc`.

| WA method | Crate:line | RPC | Status | Live test | `events.wait_for` predicate |
|---|---|---|---|---|---|
| `CoordinatorAdmin::create_group` | `coordinator_admin.rs:443` | `groups.create` | `covered` | `it_chain_h_daemon_control` | `InboundEvent::GroupChange { group_jid, kind: Create }` |
| `CoordinatorAdmin::list_own_groups` | `:671` | `groups.list` | `covered` | `it_chain_h_daemon_control` | no event — read-only |
| `CoordinatorAdmin::get_group_metadata` | `:692` | `groups.info` | `covered` | `it_chain_b1_groups_basic` | no event — read-only |
| `CoordinatorAdmin::leave_group` | `:457` | `groups.leave` | `partial` | (registry only — Tier 5 will close) | `InboundEvent::GroupChange { kind: Leave }` on remaining members |
| `CoordinatorAdmin::destroy_group` | `:470` | `groups.destroy` | `partial` | (registry only) | `InboundEvent::GroupChange { kind: Destroy }` |
| `CoordinatorAdmin::add_member` | `:497` | `groups.add_member` / `add_members` | `partial` | `it_chain_b2_groups_admin` | `InboundEvent::GroupChange { kind: Add, target }` |
| `CoordinatorAdmin::remove_member` | `:509` | `groups.remove_member` / `remove_members` | `partial` | `it_chain_b2_groups_admin` | `InboundEvent::GroupChange { kind: Remove, target }` |
| `CoordinatorAdmin::promote_to_admin` | `:540` | `groups.promote` | `partial` | `it_chain_b2_groups_admin` | `InboundEvent::GroupChange { kind: Promote, target }` |
| `CoordinatorAdmin::demote_from_admin` | `:552` | `groups.demote` | `partial` | `it_chain_b2_groups_admin` | `InboundEvent::GroupChange { kind: Demote, target }` |
| `CoordinatorAdmin::ban_member` | `:527` | `groups.ban` | `partial` | `it_chain_b2_groups_admin` | `InboundEvent::GroupChange { kind: Ban, target }` |
| `CoordinatorAdmin::approve_join_request` | `:566` | `groups.approve_join` | `partial` | `it_chain_b2_groups_admin` | `InboundEvent::GroupChange { kind: ApproveJoin, target }` |
| `CoordinatorAdmin::rename_group` | `:580` | `groups.rename` | `partial` | `it_chain_b1_groups_basic` | `InboundEvent::GroupChange { kind: Subject, after: new_name }` |
| `CoordinatorAdmin::set_group_description` | `:592` | `groups.set_description` | `partial` | `it_chain_b1_groups_basic` | `InboundEvent::GroupChange { kind: Description, after }` |
| `CoordinatorAdmin::set_locked` | `:604` | `groups.set_locked` | `partial` | `it_chain_b1_groups_basic` | `InboundEvent::GroupChange { kind: Locked }` |
| `CoordinatorAdmin::set_announce` | `:616` | `groups.set_announce` | `partial` | (registry only) | `InboundEvent::GroupChange { kind: Announce }` |
| `CoordinatorAdmin::set_ephemeral` | `:645` | `groups.set_ephemeral` | `partial` | (registry only) | `InboundEvent::GroupChange { kind: Ephemeral }` |
| `CoordinatorAdmin::set_require_approval` | `:657` | `groups.set_require_approval` | `partial` | (registry only) | `InboundEvent::GroupChange { kind: RequireApproval }` |
| `CoordinatorAdmin::list_own_groups_with_invites` | `:684` | `groups.list_with_invites` | `partial` | (registry only) | no event — read-only |
| `CoordinatorAdmin::join_by_invite` | `:720` | `groups.join_by_invite` | `partial` | (registry only) | `InboundEvent::GroupChange { kind: Join }` |
| `CoordinatorAdmin::join_by_id` | `:741` | `groups.join_by_id` | `partial` | (registry only) | same |
| `CoordinatorAdmin::transfer_ownership` | `:757` | `groups.transfer_ownership` | `partial` | (registry only) | `InboundEvent::GroupChange { kind: TransferOwnership, target }` |
| `CoordinatorAdmin::resolve_invite` | `:706` | `groups.resolve_invite` | `partial` | `it_chain_b1_groups_basic` | no event — read-only |

**Gap list (Tier 5 additions):** `Groups::query_info`, `get_invite_link`, `join_with_invite_code`, `join_with_invite_v4`, `get_membership_requests`, `approve_membership_requests`, `reject_membership_requests`, `set_member_add_mode`, `set_no_frequently_forwarded`, `set_allow_admin_reports`, `set_group_history`, `set_member_link_mode`, `set_member_share_history_mode`, `set_limit_sharing`, `cancel_membership_requests`, `revoke_request_code`, `acknowledge`, `batch_get_info`, `get_profile_pictures`, `set_profile_picture`, `remove_profile_picture`, `update_member_label` — all `gap:rpc`.

### 8. Community (Tier 6 — gap:rpc, no surface at all)

All 10 methods of `Community` (`create, deactivate, link_subgroups, unlink_subgroups, get_subgroups, get_subgroup_participant_counts, query_linked_group, join_subgroup, get_linked_groups_participants`) — `gap:rpc`. No live test possible until at least `community.create` lands.

### 9. Contacts + Profile (Tier 4 + Tier 6)

| WA method | Crate:line | RPC | Status | Live test | `events.wait_for` predicate |
|---|---|---|---|---|---|
| `Contacts::is_on_whatsapp` | `features/contacts.rs:114` | — | `gap:rpc` | — | no event — RPC response only (Tier 4) |
| `Contacts::get_profile_picture` | `:195` | — | `gap:rpc` | — | no event (Tier 4) |
| `Contacts::get_user_info` | `:247` | — | `gap:rpc` | — | no event (Tier 4) |
| `Profile::set_status_text` | `features/profile.rs:55` | — | `gap:rpc` | — | `InboundEvent::UserAboutUpdate { jid == self, about }` (Tier 6) |
| `Profile::set_push_name` | `:71` | — | `gap:rpc` | — | `InboundEvent::PushNameUpdate { jid == self, name }` (Tier 6) |
| `Profile::set_profile_picture` | `:87` | — | `gap:rpc` | — | `InboundEvent::PictureUpdate { jid == self }` (Tier 6) |
| `Profile::remove_profile_picture` | `:124` | — | `gap:rpc` | — | same |
| `Client::get_business_profile` | `client/iq_ops.rs:147` | — | `gap:rpc` | — | no event (Tier 6) |
| `Client::set_client_profile` | `:186` | — | `gap:rpc` | — | `InboundEvent::Unknown { kind: "client.profile" }` (Tier 6) |

### 10. Presence (Tier 4)

| WA method | Crate:line | RPC | Status | Live test | `events.wait_for` predicate |
|---|---|---|---|---|---|
| `Presence::set` | `features/presence.rs:73` | — | `gap:rpc` | — | no outbound event; inbound `Event::Presence` is observed by daemon |
| `Presence::set_available` | `:96` | — | `gap:rpc` | — | (Tier 4) |
| `Presence::set_unavailable` | `:117` | — | `gap:rpc` | — | (Tier 4) |
| `Presence::subscribe` | `:139` | — | `gap:rpc` | — | `InboundEvent::Presence { jid == subscribed }` within 10s (Tier 4) |
| `Presence::unsubscribe` | `:173` | — | `gap:rpc` | — | (Tier 4) |
| `Chatstate::send` / `send_composing` / `send_recording` / `send_paused` | `features/chatstate.rs:42-58` | `chats.typing` (via `send_typing` inherent) | `partial` | `it_chain_c_messages_chats` | `InboundEvent::Presence { kind: Composing }` on receiver side |
| `WhatsAppWebAdapter::send_typing` | `inherent.rs:1151` | `chats.typing` | `covered` | `it_chain_c_messages_chats` | same |
| `Client::set_force_active_delivery_receipts` | `messaging.rs:373` | — | `gap:rpc` | — | n/a — runtime config (Tier 6) |

### 11. Sync / appstate (Tier 6)

| WA method | Crate:line | RPC | Status | Live test | `events.wait_for` predicate |
|---|---|---|---|---|---|
| `Client::process_sync_task` | `app_state.rs:119` | (internal, called by SDK) | `internal` | — | `InboundEvent::Unknown { kind: "sync.task" }` observed via the event router |
| `Client::clean_dirty_bits` | `:899` | (internal) | `internal` | — | n/a |
| `Client::wait_for_startup_sync` | `sessions.rs:237` | (internal) | `internal` | — | n/a |
| `Client::set_skip_history_sync` | `accessors.rs:47` | — | `gap:rpc` | — | n/a — runtime config (Tier 6) |
| `Client::set_wanted_pre_key_count` | `:62` | — | `gap:rpc` | — | n/a — runtime config |
| `Client::set_resend_rate_limit` | `:82` | — | `gap:rpc` | — | n/a — runtime config |
| `Client::set_retry_admission` | `:97` | — | `gap:rpc` | — | n/a — runtime config |

### 12. Privacy (Tier 6)

| WA method | Crate:line | RPC | Status | Live test | `events.wait_for` predicate |
|---|---|---|---|---|---|
| `Client::fetch_privacy_settings` | `iq_ops.rs:66` | — | `gap:rpc` | — | no event (Tier 6) |
| `Client::set_privacy_setting` | `:85` | — | `gap:rpc` | — | no event |
| `Client::set_privacy_disallowed_list` | `:99` | — | `gap:rpc` | — | no event |
| `Client::set_default_disappearing_mode` | `:112` | — | `gap:rpc` | — | `InboundEvent::DisappearingModeChanged { mode }` |
| `Client::set_chat_disappearing_timer` | `:128` | — | `gap:rpc` | — | `InboundEvent::DisappearingModeChanged { jid, expiration }` |

### 13. Newsletter (Tier 6)

All 14 `Newsletter` methods (`list_subscribed, get_metadata, create, join, leave, update, set_follower_mute, set_admin_mute, get_metadata_by_invite, subscribe_live_updates, send_reaction, edit_message, revoke_message, get_messages`) — `gap:rpc`. Per-method `events.wait_for` predicate documented once any one is implemented (Tier 6).

### 14. Status / broadcast story (Tier 6)

All 5 `Status` methods (`send_text, send_image, send_video, send_raw, revoke`) — `gap:rpc`. Live test would use a TEST_MEMBER_1 self-story or operator's own story.

### 15. Blocking (Tier 6)

All 4 `Blocking` methods (`block, unblock, get_blocklist, is_blocked`) — `gap:rpc`.

### 16. Labels (Tier 6)

All 4 `Labels` methods (`create_label, delete_label, add_chat_label, remove_chat_label`) — `gap:rpc`.

### 17. Comments (Tier 6)

`Comments::send_text` / `send_message` — `gap:rpc`.

### 18. Mex / GraphQL (Tier 6)

`Mex::query` / `mutate` — `gap:rpc`.

### 19. Media re-upload (Tier 6)

`MediaReupload::request` / `request_many` — `gap:rpc`.

### 20. TcToken (Tier 6)

All 4 `TcToken` methods (`issue_tokens, prune_expired, get, get_all_jids`) — `gap:rpc`.

### 21. Events calendar (Tier 6)

`Events::create` / `respond` — `gap:rpc`.

### 22. Device / IQ (Tier 6)

| WA method | Crate:line | RPC | Status | Live test | `events.wait_for` predicate |
|---|---|---|---|---|---|
| `Client::set_passive` | `iq_ops.rs:6` | — | `gap:rpc` | — | n/a — runtime config |
| `Client::fetch_props` | `:11` | — | `gap:rpc` | — | no event |
| `Client::send_digest_key_bundle` | `:155` | — | `gap:rpc` | — | n/a — protocol-layer |
| `Client::set_device_props` | `:168` | — | `gap:rpc` | — | no event |

### 23. Identity (Tier 7)

| WA method | Crate:line | RPC | Status | Live test | `events.wait_for` predicate |
|---|---|---|---|---|---|
| `Client::get_pn` | `accessors.rs:243` | — | `gap:rpc` | — | no event (Tier 7) |
| `Client::get_lid` | `:247` | — | `gap:rpc` | — | no event |
| `Client::identity_tags` | `:254` | — | `gap:rpc` | — | no event |
| `Client::is_lid_migrated` | `lid_pn.rs:420` | — | `gap:rpc` | — | no event |
| `Client::get_lid_pn_entry` | `:739` | — | `gap:rpc` | — | no event |

### 24. Passkey (Tier 7)

| WA method | Crate:line | RPC | Status | Live test | `events.wait_for` predicate |
|---|---|---|---|---|---|
| `Client::set_passkey_authenticator` | `passkey/flow.rs:386` | (boot, `WhatsAppConfig::passkey_authenticator`) | `internal` | — | n/a |
| `Client::send_passkey_response` | `:396` | — | `gap:rpc` | — | `InboundEvent::Unknown { kind: "passkey.response" }` (Tier 7) |
| `Client::send_passkey_confirmation` | `:431` | — | `gap:rpc` | — | `InboundEvent::Unknown { kind: "passkey.confirmation" }` |
| `passkey::parse_request_options` | `passkey/mod.rs:164` | — | `internal` | — | n/a — helper |
| `passkey::build_webauthn_assertion_json` | `:237` | — | `internal` | — | n/a — helper |

**Existing observation:** the daemon's `connection_watcher` already classifies `Event::PairPasskeyRequest|Confirmation|Error` into `BotStateMirror::AwaitingPasskey` — those are observed today, just not asserted by a live test yet.

### 25. Voip (n/a — feature not enabled)

`Client::{voip, reject, accept, call, terminate}` — gated `#[cfg(feature = "voip")]`. `octo-adapter-whatsapp/Cargo.toml` does not enable that feature, so they are **not reachable**. Status: `n/a`.

### 26. IQ raw / misc (Tier 7)

| WA method | Crate:line | RPC | Status | Live test | `events.wait_for` predicate |
|---|---|---|---|---|---|
| `Client::send_iq` | `request.rs:210` | — | `internal` | — | n/a — used by other IQ methods |
| `Client::execute` | `:248` | — | `internal` | — | n/a |
| `generate_message_id` | `:132` | — | `internal` | — | n/a — helper |
| `Client::wait_for_node` / `wait_for_sent_node` | `accessors.rs:362,381` | — | `internal` | — | n/a — stanza waiters |

### 27. Events / observability (internal)

`Client::{register_handler, set_raw_node_forwarding, stats, memory_report, resource_report, persistence_manager}` + the `EventHandler` / `ChannelEventHandler` / `CoreEventBus` / `EventInterest` types — all consumed by the daemon's connection watcher and event router. No RPC needed; not user-actionable.

---

## Status totals (pre-Tier-1 execution)

| Status | Count |
|---|---:|
| `covered` | ~36 (RPC exists + live/integration test runs) |
| `partial` | ~26 (RPC exists + integration test, but live-test re-verification pending) |
| `gap:rpc` | ~145 |
| `gap:test` | 0 (no RPC exists that lacks a test once `it_chain_*` runs) |
| `internal` | ~20 |
| `n/a` | 5 (voip + a few pure helpers) |
| **total** | **~232** (some methods listed in both "Send" and "ChatActions" — de-duplicated count is ~180) |

## How to use this matrix

1. **Pick a tier.** Confirm operator prerequisites (linked session, TEST_MEMBER_1 for Tier 1; TEST_MEMBER_2/3/4 for Tier 5).
2. **For each `gap:rpc` row in the tier**, land it as a 2-commit unit: RPC handler + adapter-trait method, then the live test that asserts the `events.wait_for(predicate)`.
3. **For each `partial` row**, add a `live_*` test next to the existing `it_chain_*` coverage.
4. **Update the row** as status moves from `partial` → `covered` or `gap:rpc` → `partial`.
5. **Never claim a row is `covered` without a passing live test against a real WA session.** Integration tests under `it_chain_*` are not sufficient.

## Reuse

- **Boot-once fixture pattern** — `tests/live_daemon_test.rs::init_fixture` (Phase 0 commit).
- **`events_query::wait_for(predicate, timeout)`** — `crates/octo-whatsapp/src/events_query.rs` (Phase 0 commit).
- **Mandatory 2 s rate-limit floor** — `WA_LIVE_CALL_FLOOR_MS = 2000`, enforced by `inter_call_delay_for(method)` with read-only RPC bypass list.
- **`OctoWhatsAppAdapter` trait surface** — `crates/octo-whatsapp/src/adapter_trait.rs`. New RPCs go here as new trait methods + adapter inherent implementations.
- **`InboundEvent` variants** — `crates/octo-whatsapp/src/events.rs:34`. New event types that the WA crate emits but octo-whatsapp doesn't classify yet land as `InboundEvent::Unknown { kind: "<dotted.path>" }` and the live test asserts on the `kind` field.

## Related

- Plan: `.claude/plans/cryptic-percolating-octopus.md` (this session's planning)
- Enumeration: `.claude/plans/cryptic-percolating-octopus-agent-a03ed4631239cac78.md` (raw enumeration from the WA crate sources)
- Test inventory: `.claude/plans/cryptic-percolating-octopus-agent-a01c201fe6f3e4d33.md` (file/feature/test surface pre-rename)
- Session 2 summary (Tiers 4-6 landed): see `coverage-2026-07-10-session2.md` (next section).

## Session 2 progress (2026-07-10)

This session landed **Tiers 4 + 5 + 6.0–6.5** on top of Tier 0 (covered in the original plan and Tiers 1–3).

**RPC coverage delta:**
- Original matrix: ~36 covered rows.
- Session 2 adds: +37 RPCs across Tiers 4 (8), 5 (0 — groups RPCs already wrapped in Phase 6.12), 6.0 (3), 6.1 (4), 6.2 (6), 6.3 (4), 6.4 (3), 6.5 (4). Plus live test coverage for 13 new RPCs.
- Total daemon RPCs registered: **~125** (was 88 at the start of this session).

**Live test count delta:**
- Phase 0 (start of session): 41 `it_chain_*` tests only (no live).
- After Tier 0: 0 live tests registered (cfg-gated).
- After Phase 1 (matrix doc): 0 live tests.
- After Tier 1 (text send): 6 live tests (1 Tier 0 + 5 Tier 1).
- After Tier 2 (media): 11 live tests (added 5 Tier 2).
- After Tier 3 (receipts): 16 live tests (added 5 Tier 3).
- After Tier 4 (contacts + presence): 24 live tests (added 8 Tier 4).
- After Tier 5 (groups): 27 live tests (added 3 Tier 5 canary).
- After Tier 6 (profile + privacy + labels + lifecycle + identity + newsletter):
  - 6.0: 30 (+3 profile/user_info)
  - 6.1: 34 (+4 privacy/blocking)
  - 6.2: 37 (+3 labels/star)
  - 6.3: 41 (+4 lifecycle)
  - 6.4: 44 (+3 identity)
  - 6.5: 48 (+4 newsletter/events)
- **Final session-2 total: 48 live tests** (16 from Tiers 1-3, 27 → 48 across Tiers 4-6.5).

**Lib test count delta:** 685 → 717 (+32 session 2 additions for new delegation tests).

**Tier 6 backlog remaining (~33 RPCs, deferred to later operator-driven sessions):**
- `newsletter.create` / `join` / `send_reaction` / `edit_message` / `revoke_message` (4)
- `status.send_text` / `send_image` / `send_video` / `revoke` (4 — need `FontType` + `StatusSendOptions` types)
- `events.respond` (1 — RSVP, needs per-event `message_secret`)
- `tctoken.issue` / `get` / `prune` (3)
- `messages.pin` / `unpin` (2)
- `messages.forward` (1 — needs original message body, not msg_id)
- `messages.edit_encrypted` (1 — needs `wacore::message_edit::decrypt` round-trip)
- `polls.vote` / `aggregate` (2 — needs poll `message_secret` round-trip)
- `passkey.pair_request` / `pair_response` / `pair_confirmation` (3 — WebAuthn authenticator)
- `profile.set_profile_picture` / `remove_profile_picture` (2)
- `community` (8)
- `comments` / `mex` / `rotate_key` / `signal` (4)

**Coverage matrix recompute (post-session-2):**

| Status | Count (original) | Count (post-session-2) | Delta |
|---|---|---|---|
| `covered` | ~36 | ~73 | +37 |
| `partial` | ~26 | ~26 | 0 |
| `gap:rpc` | ~145 | ~108 | -37 |
| `internal` | ~20 | ~20 | 0 |
| `n/a` | ~5 | ~5 | 0 |

**Quality gates (all green):**
- `cargo test -p octo-whatsapp --lib` — 717 passing (was 685).
- `cargo test -p octo-whatsapp --features "live-whatsapp test-helpers" --test live_daemon_test -- --list` — 48 tests registered.
- `cargo clippy -p octo-whatsapp --all-targets --features "live-whatsapp test-helpers" -- -D warnings` — clean.
- `cargo fmt --check -p octo-whatsapp` — clean.

**Skip-vs-fail convention (mandatory across all live tiers):**
- Tests that depend on a real peer device (`OCTO_WHATSAPP_TEST_MEMBER=+<phone>`), a pre-created group (`OCTO_WHATSAPP_TEST_GROUP_ID`), a peer message (`OCTO_WHATSAPP_TEST_INBOUND_MSG_ID`), or a pre-joined newsletter (`OCTO_WHATSAPP_TEST_NEWSLETTER_LEAVE_JID`) **skip with `eprintln` + early return** when the operator flag is unset — they never `panic` on operator setup gaps.
- Self-running tests (Tier 0 canary, Tier 1 self-echo, Tier 2 self-media, Tier 3 receipt canary, Tier 4 self-is_on_whatsapp/profile_picture/get_user_info/set_available/unavailable/typing, Tier 5 self-create group, Tier 6.0 self profile/get_user_info, Tier 6.1 self privacy/blocking, Tier 6.2 self labels/star, Tier 6.3 self mark_as_played/clear_chat/delete_for_me/save_contact, Tier 6.4 self identity, Tier 6.5 self newsletter/events) always run when the fixture boots.

**Session commit list (chronological):**
```
9e810173 feat(octo-whatsapp): Tier 6.5 RPCs — newsletter (3) + events.create (1)
3b7d85d6 test(octo-whatsapp): Tier 6.5 live tests — newsletter list/get/leave + events.create
5b7a9975 test(octo-whatsapp): Tier 6.4 live tests — identity (get_pn/get_lid/is_lid_migrated)
8825992d feat(octo-whatsapp): Tier 6.4 RPCs — identity (get_pn/get_lid/is_lid_migrated)
82881e6e test(octo-whatsapp): Tier 6.3 live tests — mark_as_played + chats.clear + delete_for_me + save_contact
af451f13 feat(octo-whatsapp): Tier 6.3 RPCs — mark_as_played + chats.clear + delete_for_me + save_contact
9d562401 test(octo-whatsapp): Tier 6.2 live tests — labels (create/delete + add/remove) + messages star/unstar
24211521 feat(octo-whatsapp): Tier 6.2 RPCs — labels (4) + messages star/unstar (2)
af62f3b8 test(octo-whatsapp): Tier 6.1 live tests — privacy + blocking (4 RPCs)
9a1f7947 feat(octo-whatsapp): Tier 6.1 RPCs — privacy get/set + blocking get_blocklist/is_blocked
5e424f3a test(octo-whatsapp): Tier 6 live tests — profile + contact enrichment (3 canary tests)
6dcb7d52 feat(octo-whatsapp): Tier 6 RPCs — profile + contact enrichment (set_push_name, set_status, get_user_info)
25f81f0a test(octo-whatsapp): Tier 5 live tests — groups canary (create/info/destroy) + info/rename
8ea27db3 test(octo-whatsapp): Tier 4 live tests — contact + presence (8 tests for 8 RPCs)
362173b6 feat(octo-whatsapp): Tier 4 RPCs — contact + presence surface (8 new methods)
2343ebf7 test(octo-whatsapp): Tier 3 live tests — receipt chain
8854942c test(octo-whatsapp): Tier 2 live tests — 1:1 media
d1b69029 test(octo-whatsapp): Tier 1 live tests — 1:1 text send round-trip
a79d7b96 docs(coverage): live WA API coverage matrix (Phase 1)
0c635e19 test(octo-whatsapp): reclaim tests/live_daemon_test.rs
199f8ae6 feat(octo-whatsapp): events_query::wait_for helper
cacd29bb feat(octo-adapter-whatsapp): inherent send_text method
863e19ae feat(octo-whatsapp): send.text real adapter dispatch
47595171 refactor(octo-whatsapp): rename tests/live_daemon_test.rs to it_daemon_chain.rs
```

(All commits on local `feat/whatsapp-runtime-cli-mcp` branch. No push per operator instruction 2026-07-05.)