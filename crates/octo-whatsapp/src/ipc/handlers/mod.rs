//! Concrete RPC method handlers. One file per logical group; all wired into
//! `build_registry()` at the bottom of this module.

pub mod capabilities;
pub mod chats_archive;
pub mod chats_delete;
pub mod chats_info;
pub mod chats_list;
pub mod chats_mute;
pub mod chats_pin;
pub mod chats_typing;
pub mod chats_unpin;
pub mod clients;
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
pub mod media_info;
pub mod messages_download;
pub mod messages_edit;
pub mod messages_get;
pub mod messages_list;
pub mod messages_mark_read;
pub mod messages_search;
pub mod preflight;
pub mod rules;
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
        .register(Arc::new(messages_list::MessagesList))
        .register(Arc::new(messages_search::MessagesSearch))
        .register(Arc::new(messages_edit::MessagesEdit))
        .register(Arc::new(messages_mark_read::MessagesMarkRead))
        .register(Arc::new(messages_download::MessagesDownload))
        .register(Arc::new(messages_get::MessagesGet))
        .register(Arc::new(rules::RulesList))
        .register(Arc::new(rules::RulesGet))
        .register(Arc::new(triggers::TriggersList))
        .register(Arc::new(triggers::TriggersGet))
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
            .collect::<std::collections::BTreeSet<_>>()
            .len();
        assert_eq!(reg.methods().len(), dedup);
    }
}
