//! Concrete RPC method handlers. One file per logical group; all wired into
//! `build_registry()` at the bottom of this module.

pub mod accounts;
pub mod actions_escalate;
pub mod audit;
pub mod blocking_get_blocklist;
pub mod blocking_is_blocked;
pub mod capabilities;
pub mod chats_archive;
pub mod chats_clear;
pub mod chats_delete;
pub mod chats_info;
pub mod chats_list;
pub mod chats_mute;
pub mod chats_pin;
pub mod chats_typing;
pub mod chats_unpin;
pub mod clients;
pub mod contact_block;
pub mod contact_unblock;
pub mod contacts_get_profile_picture;
pub mod contacts_get_user_info;
pub mod contacts_is_on_whatsapp;
pub mod contacts_save_contact;
pub mod daemon_methods;
pub mod daemon_ops;
pub mod domain_compute_hash;
pub mod envelope_decode;
pub mod envelope_encode;
pub mod envelope_send;
pub mod envelope_send_native;
pub mod events;
pub mod groups;
pub mod health;
pub mod identity_get_lid;
pub mod identity_get_pn;
pub mod identity_is_lid_migrated;
pub mod labels_add_chat_label;
pub mod labels_create;
pub mod labels_delete;
pub mod labels_remove_chat_label;
pub mod media_info;
pub mod messages_delete_for_me;
pub mod messages_download;
pub mod messages_edit;
pub mod messages_get;
pub mod messages_list;
pub mod messages_mark_as_played;
pub mod messages_mark_read;
pub mod messages_search;
pub mod messages_star;
pub mod messages_unstar;
pub mod preflight;
pub mod presence_set_available;
pub mod presence_set_unavailable;
pub mod presence_subscribe;
pub mod presence_unsubscribe;
pub mod privacy_get;
pub mod privacy_set;
pub mod profile_set_push_name;
pub mod profile_set_status;
pub mod rules;
pub mod security_tokens;
pub mod send_audio;
pub mod send_contact;
pub mod send_delete;
pub mod send_image;
pub mod send_location;
pub mod send_poll;
pub mod send_reaction;
pub mod send_sticker;
pub mod send_text;
pub mod send_video;
pub mod send_voice;
pub mod status;
pub mod triggers;
pub mod util;
pub mod version;

use super::server::HandlerRegistry;
use std::sync::Arc;

/// Build the Phase 1 handler registry. Registering is order-independent;
/// `HandlerRegistry::register` is the builder-style API.
pub fn build_registry() -> HandlerRegistry {
    HandlerRegistry::new()
        .register(Arc::new(version::VersionGet))
        .register(Arc::new(status::StatusGet))
        .register(Arc::new(health::HealthGet))
        .register(Arc::new(send_text::SendText))
        .register(Arc::new(send_image::SendImage))
        .register(Arc::new(send_video::SendVideo))
        .register(Arc::new(send_audio::SendAudio))
        .register(Arc::new(send_voice::SendVoice))
        .register(Arc::new(send_sticker::SendSticker))
        .register(Arc::new(send_reaction::SendReaction))
        .register(Arc::new(send_poll::SendPoll))
        .register(Arc::new(send_contact::SendContact))
        .register(Arc::new(send_location::SendLocation))
        .register(Arc::new(send_delete::SendDelete))
        .register(Arc::new(groups::GroupsCreate))
        .register(Arc::new(groups::GroupsList))
        .register(Arc::new(groups::GroupsInfo))
        .register(Arc::new(groups::GroupsLeave))
        .register(Arc::new(groups::GroupsDestroy))
        .register(Arc::new(groups::GroupsResolveInvite))
        .register(Arc::new(groups::GroupsAddMember))
        .register(Arc::new(groups::GroupsAddMembers))
        .register(Arc::new(groups::GroupsRemoveMember))
        .register(Arc::new(groups::GroupsRemoveMembers))
        .register(Arc::new(groups::GroupsPromote))
        .register(Arc::new(groups::GroupsDemote))
        .register(Arc::new(groups::GroupsBan))
        .register(Arc::new(groups::GroupsApproveJoin))
        .register(Arc::new(groups::GroupsRename))
        .register(Arc::new(groups::GroupsSetDescription))
        .register(Arc::new(groups::GroupsSetLocked))
        .register(Arc::new(groups::GroupsTransferOwnership))
        .register(Arc::new(groups::GroupsSetAnnounce))
        .register(Arc::new(groups::GroupsSetEphemeral))
        .register(Arc::new(groups::GroupsSetRequireApproval))
        .register(Arc::new(groups::GroupsListWithInvites))
        .register(Arc::new(groups::GroupsJoinByInvite))
        .register(Arc::new(groups::GroupsJoinById))
        .register(Arc::new(messages_list::MessagesList))
        .register(Arc::new(messages_search::MessagesSearch))
        .register(Arc::new(messages_edit::MessagesEdit))
        .register(Arc::new(messages_mark_read::MessagesMarkRead))
        .register(Arc::new(messages_download::MessagesDownload))
        .register(Arc::new(messages_get::MessagesGet))
        .register(Arc::new(rules::RulesList))
        .register(Arc::new(rules::RulesGet))
        .register(Arc::new(rules::RulesCreate))
        .register(Arc::new(rules::RulesUpdate))
        .register(Arc::new(rules::RulesPatch))
        .register(Arc::new(rules::RulesDelete))
        .register(Arc::new(rules::RulesEnable))
        .register(Arc::new(rules::RulesDisable))
        .register(Arc::new(rules::RulesApprove))
        .register(Arc::new(rules::RulesReload))
        .register(Arc::new(rules::RulesFlush))
        .register(Arc::new(rules::RulesTest))
        .register(Arc::new(triggers::TriggersList))
        .register(Arc::new(triggers::TriggersGet))
        .register(Arc::new(triggers::TriggersCreate))
        .register(Arc::new(triggers::TriggersUpdate))
        .register(Arc::new(triggers::TriggersDelete))
        .register(Arc::new(triggers::TriggersRun))
        .register(Arc::new(events::EventsList))
        .register(Arc::new(events::EventsShow))
        .register(Arc::new(events::EventsReplay))
        .register(Arc::new(events::EventsTail))
        .register(Arc::new(clients::ClientsList))
        .register(Arc::new(daemon_methods::DaemonMethodsList))
        .register(Arc::new(daemon_methods::DaemonMethodsHelp))
        .register(Arc::new(daemon_ops::ReconnectNow))
        .register(Arc::new(daemon_ops::Shutdown))
        .register(Arc::new(chats_list::ChatsList))
        .register(Arc::new(chats_info::ChatsInfo))
        .register(Arc::new(chats_pin::ChatsPin))
        .register(Arc::new(chats_unpin::ChatsUnpin))
        .register(Arc::new(chats_mute::ChatsMute))
        .register(Arc::new(chats_archive::ChatsArchive))
        .register(Arc::new(chats_delete::ChatsDelete))
        .register(Arc::new(chats_typing::ChatsTyping))
        .register(Arc::new(media_info::MediaInfo))
        .register(Arc::new(envelope_encode::EnvelopeEncode))
        .register(Arc::new(envelope_decode::EnvelopeDecode))
        .register(Arc::new(envelope_send::EnvelopeSend))
        .register(Arc::new(envelope_send_native::EnvelopeSendNative))
        .register(Arc::new(capabilities::Capabilities))
        .register(Arc::new(domain_compute_hash::DomainComputeHash))
        .register(Arc::new(audit::AuditTail))
        .register(Arc::new(audit::AuditVerify))
        .register(Arc::new(actions_escalate::ActionsEscalate))
        .register(Arc::new(security_tokens::SecurityRotateToken))
        .register(Arc::new(security_tokens::SecurityRevokeAllTokens))
        .register(Arc::new(security_tokens::SecurityListTokens))
        .register(Arc::new(accounts::AccountsList))
        .register(Arc::new(accounts::AccountsUse))
        .register(Arc::new(accounts::AccountsInfo))
        // Tier 4: contacts + presence
        .register(Arc::new(contacts_is_on_whatsapp::ContactsIsOnWhatsApp))
        .register(Arc::new(
            contacts_get_profile_picture::ContactsGetProfilePicture,
        ))
        .register(Arc::new(contact_block::ContactBlock))
        .register(Arc::new(contact_unblock::ContactUnblock))
        .register(Arc::new(presence_subscribe::PresenceSubscribe))
        .register(Arc::new(presence_unsubscribe::PresenceUnsubscribe))
        .register(Arc::new(presence_set_available::PresenceSetAvailable))
        .register(Arc::new(presence_set_unavailable::PresenceSetUnavailable))
        // Tier 6: profile + contact enrichment
        .register(Arc::new(profile_set_push_name::ProfileSetPushName))
        .register(Arc::new(profile_set_status::ProfileSetStatus))
        .register(Arc::new(contacts_get_user_info::ContactsGetUserInfo))
        // Tier 6.1: privacy + blocklist queries
        .register(Arc::new(privacy_get::PrivacyGet))
        .register(Arc::new(privacy_set::PrivacySet))
        .register(Arc::new(blocking_get_blocklist::BlockingGetBlocklist))
        .register(Arc::new(blocking_is_blocked::BlockingIsBlocked))
        // Tier 6.2: labels + star
        .register(Arc::new(labels_create::LabelsCreate))
        .register(Arc::new(labels_delete::LabelsDelete))
        .register(Arc::new(labels_add_chat_label::LabelsAddChatLabel))
        .register(Arc::new(labels_remove_chat_label::LabelsRemoveChatLabel))
        .register(Arc::new(messages_star::MessagesStar))
        .register(Arc::new(messages_unstar::MessagesUnstar))
        // Tier 6.3: messages.mark_as_played + chats.clear + messages.delete_for_me + contacts.save_contact
        .register(Arc::new(messages_mark_as_played::MessagesMarkAsPlayed))
        .register(Arc::new(chats_clear::ChatsClear))
        .register(Arc::new(messages_delete_for_me::MessagesDeleteForMe))
        .register(Arc::new(contacts_save_contact::ContactsSaveContact))
        // Tier 6.4: identity (local-state reads)
        .register(Arc::new(identity_get_pn::IdentityGetPn))
        .register(Arc::new(identity_get_lid::IdentityGetLid))
        .register(Arc::new(identity_is_lid_migrated::IdentityIsLidMigrated))
}

/// Every RPC method name exposed in Phase 1 (used by tests + CLI/MCP surface).
pub const PHASE1_METHODS: &[&str] = &[
    "version.get",
    "status.get",
    "health.get",
    "send.text",
    "groups.create",
    "groups.list",
    "groups.info",
    "groups.leave",
    "messages.list",
    "rules.list",
    "rules.get",
    "triggers.list",
    "triggers.get",
    "events.list",
    "events.show",
    "reconnect.now",
    "shutdown",
];

/// RPC method names added in Phase 2 outbound media matrix (Tasks 26-30).
pub const PHASE2_MEDIA_METHODS: &[&str] = &[
    "send.image",
    "send.video",
    "send.audio",
    "send.voice",
    "send.sticker",
];

/// RPC method names added in Phase 2 send/message control plane
/// (Tasks 31-40): reactions, polls, contacts, location, delete,
/// search, edit, mark_read, download, messages.list (re-stub),
/// messages.get.
pub const PHASE2_SEND_MESSAGE_METHODS: &[&str] = &[
    "send.reaction",
    "send.poll",
    "send.contact",
    "send.location",
    "send.delete",
    "messages.search",
    "messages.edit",
    "messages.mark_read",
    "messages.download",
    "messages.list",
    "messages.get",
];

/// RPC method names added in Phase 2 chat-control plane (Tasks 41-45):
/// chat list/info/pin/unpin/mute/archive/delete/typing + media.info.
pub const PHASE2_CHATS_METHODS: &[&str] = &[
    "chats.list",
    "chats.info",
    "chats.pin",
    "chats.unpin",
    "chats.mute",
    "chats.archive",
    "chats.delete",
    "chats.typing",
    "media.info",
];

/// RPC method names added in Phase 2 envelope + capabilities plane
/// (Tasks 46-50): DOT/1 encode/decode/send, native transport,
/// platform capabilities, and deterministic domain-id hashing.
pub const PHASE2_ENVELOPE_METHODS: &[&str] = &[
    "envelope.encode",
    "envelope.decode",
    "envelope.send",
    "envelope.send-native",
    "capabilities",
    "domain.compute-hash",
];

/// RPC method names added in Phase 3 (events): list, show, replay, tail.
pub const PHASE3_EVENTS_METHODS: &[&str] =
    &["events.list", "events.show", "events.replay", "events.tail"];

/// RPC method names added in Phase 3 (agent discovery): clients.list,
/// daemon.methods.list, daemon.methods.help.
pub const PHASE3_DISCOVERY_METHODS: &[&str] =
    &["clients.list", "daemon.methods.list", "daemon.methods.help"];

/// RPC method names added in Phase 4 (rules + triggers + audit + escalate).
pub const PHASE4_RULES_METHODS: &[&str] = &[
    "rules.list",
    "rules.get",
    "rules.create",
    "rules.update",
    "rules.patch",
    "rules.delete",
    "rules.enable",
    "rules.disable",
    "rules.approve",
    "rules.reload",
    "rules.flush",
    "rules.test",
];

pub const PHASE4_TRIGGERS_METHODS: &[&str] = &[
    "triggers.list",
    "triggers.get",
    "triggers.create",
    "triggers.update",
    "triggers.delete",
    "triggers.run",
];

pub const PHASE4_AUDIT_METHODS: &[&str] = &["audit.tail", "audit.verify"];

pub const PHASE4_ACTIONS_METHODS: &[&str] = &["actions.escalate"];

/// RPC method names added in Phase 5 Part A (security).
pub const PHASE5_SECURITY_METHODS: &[&str] = &[
    "security.rotate_token",
    "security.revoke_all_tokens",
    "security.list_tokens",
];

/// RPC method names added in Phase 6.12 (groups member-management
/// + invite resolution).
pub const PHASE6_12_GROUPS_METHODS: &[&str] = &[
    // T6.12-3: membership
    "groups.add_member",
    "groups.add_members",
    "groups.remove_member",
    "groups.remove_members",
    "groups.promote",
    "groups.demote",
    // T6.12-4: mode / admin
    "groups.destroy",
    "groups.ban",
    "groups.approve_join",
    "groups.rename",
    "groups.set_description",
    "groups.set_locked",
    // T6.12-5: invite
    "groups.resolve_invite",
    // ownership transfer (added with mode/admin batch)
    "groups.transfer_ownership",
    // T6.12.1-1: completion surface (TTL/announce/approval + joins)
    "groups.set_announce",
    "groups.set_ephemeral",
    "groups.set_require_approval",
    "groups.list_with_invites",
    "groups.join_by_invite",
    "groups.join_by_id",
];

/// RPC method names added in Phase 6.1 (multi-account).
pub const PHASE6_1_ACCOUNTS_METHODS: &[&str] = &[
    "daemon.accounts.list",
    "daemon.accounts.use",
    "daemon.accounts.info",
];

/// RPC method names added in Tier 4 (live coverage matrix): contact
/// existence / profile picture / blocklist queries + presence
/// subscription + outbound presence broadcasts.
pub const TIER4_CONTACT_PRESENCE_METHODS: &[&str] = &[
    "contacts.is_on_whatsapp",
    "contacts.get_profile_picture",
    "contact.block",
    "contact.unblock",
    "presence.subscribe",
    "presence.unsubscribe",
    "presence.set_available",
    "presence.set_unavailable",
];

/// RPC method names added in Tier 6 (live coverage matrix): profile
/// updates (push name, About status) + rich user-info enrichment.
pub const TIER6_PROFILE_METHODS: &[&str] = &[
    "profile.set_push_name",
    "profile.set_status",
    "contacts.get_user_info",
];

/// RPC method names added in Tier 6.1 (live coverage matrix):
/// privacy settings (get/set) + blocklist queries (get_blocklist /
/// is_blocked).
pub const TIER6_1_PRIVACY_METHODS: &[&str] = &[
    "privacy.get",
    "privacy.set",
    "blocking.get_blocklist",
    "blocking.is_blocked",
];

/// RPC method names added in Tier 6.2 (live coverage matrix):
/// labels (create / delete / add-chat / remove-chat) + message
/// star / unstar.
///
/// **Deferred:** `polls.vote` and `polls.aggregate` require the
/// `message_secret` + `poll_creator_jid` + per-vote ciphertext
/// round-trip that wacore's `polls::vote` API exposes; they are
/// tracked in the coverage matrix as `gap:rpc`.
pub const TIER6_2_LABELS_STAR_METHODS: &[&str] = &[
    "labels.create",
    "labels.delete",
    "labels.add_chat_label",
    "labels.remove_chat_label",
    "messages.star",
    "messages.unstar",
];

/// RPC method names added in Tier 6.3 (live coverage matrix):
/// `messages.mark_as_played` (Played receipt), `chats.clear`
/// (clear all messages), `messages.delete_for_me` (local-only
/// delete), `contacts.save_contact` (sync contact metadata).
///
/// **Deferred:** `messages.forward` requires the original
/// message body (not just msg_id); `messages.edit_message_encrypted`
/// requires the wacore `message_edit::decrypt` round-trip with the
/// per-message HKDF secret. Both tracked as `gap:rpc`.
pub const TIER6_3_LIFECYCLE_METHODS: &[&str] = &[
    "messages.mark_as_played",
    "chats.clear",
    "messages.delete_for_me",
    "contacts.save_contact",
];

/// RPC method names added in Tier 6.4 (live coverage matrix):
/// identity (PN / LID / LID-migration status) — all read from the
/// in-memory device snapshot, no WA server roundtrip.
///
/// **Deferred:** passkey pair RPCs (pair_passkey_request /
/// pair_passkey_response / pair_passkey_confirmation) require a
/// WebAuthn authenticator and are tracked as `gap:rpc` for a
/// future session.
pub const TIER6_4_IDENTITY_METHODS: &[&str] = &[
    "identity.get_pn",
    "identity.get_lid",
    "identity.is_lid_migrated",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phase1_methods_all_registered() {
        let reg = build_registry();
        for m in PHASE1_METHODS {
            assert!(
                reg.contains(m),
                "method {m:?} not registered in build_registry()"
            );
        }
    }

    #[test]
    fn phase2_media_methods_all_registered() {
        let reg = build_registry();
        for m in PHASE2_MEDIA_METHODS {
            assert!(
                reg.contains(m),
                "method {m:?} not registered in build_registry()"
            );
        }
    }

    #[test]
    fn phase2_send_message_methods_all_registered() {
        let reg = build_registry();
        for m in PHASE2_SEND_MESSAGE_METHODS {
            assert!(
                reg.contains(m),
                "method {m:?} not registered in build_registry()"
            );
        }
    }

    #[test]
    fn phase2_envelope_methods_all_registered() {
        let reg = build_registry();
        for m in PHASE2_ENVELOPE_METHODS {
            assert!(
                reg.contains(m),
                "method {m:?} not registered in build_registry()"
            );
        }
    }

    #[test]
    fn registry_size_matches_phase1_phase2() {
        let reg = build_registry();
        // `messages.list` is in both PHASE1_METHODS and
        // PHASE2_SEND_MESSAGE_METHODS; we only register it once.
        let dedup = PHASE1_METHODS
            .iter()
            .chain(PHASE2_MEDIA_METHODS.iter())
            .chain(PHASE2_SEND_MESSAGE_METHODS.iter())
            .chain(PHASE2_CHATS_METHODS.iter())
            .chain(PHASE2_ENVELOPE_METHODS.iter())
            .chain(PHASE3_EVENTS_METHODS.iter())
            .chain(PHASE3_DISCOVERY_METHODS.iter())
            .chain(PHASE4_RULES_METHODS.iter())
            .chain(PHASE4_TRIGGERS_METHODS.iter())
            .chain(PHASE4_AUDIT_METHODS.iter())
            .chain(PHASE4_ACTIONS_METHODS.iter())
            .chain(PHASE5_SECURITY_METHODS.iter())
            .chain(PHASE6_12_GROUPS_METHODS.iter())
            .chain(PHASE6_1_ACCOUNTS_METHODS.iter())
            .chain(TIER4_CONTACT_PRESENCE_METHODS.iter())
            .chain(TIER6_PROFILE_METHODS.iter())
            .chain(TIER6_1_PRIVACY_METHODS.iter())
            .chain(TIER6_2_LABELS_STAR_METHODS.iter())
            .chain(TIER6_3_LIFECYCLE_METHODS.iter())
            .chain(TIER6_4_IDENTITY_METHODS.iter())
            .collect::<std::collections::BTreeSet<_>>()
            .len();
        assert_eq!(reg.methods().len(), dedup);
    }
}
