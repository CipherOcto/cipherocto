//! Concrete RPC method handlers. One file per logical group; all wired into
//! `build_registry()` at the bottom of this module.

pub mod daemon_ops;
pub mod events;
pub mod groups;
pub mod health;
pub mod messages;
pub mod preflight;
pub mod rules;
pub mod send_audio;
pub mod send_image;
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
        .register(Arc::new(groups::GroupsCreate))
        .register(Arc::new(groups::GroupsList))
        .register(Arc::new(groups::GroupsInfo))
        .register(Arc::new(groups::GroupsLeave))
        .register(Arc::new(messages::MessagesList))
        .register(Arc::new(rules::RulesList))
        .register(Arc::new(rules::RulesGet))
        .register(Arc::new(triggers::TriggersList))
        .register(Arc::new(triggers::TriggersGet))
        .register(Arc::new(events::EventsList))
        .register(Arc::new(events::EventsShow))
        .register(Arc::new(daemon_ops::ReconnectNow))
        .register(Arc::new(daemon_ops::Shutdown))
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

/// RPC method names added in Phase 2 (outbound media matrix; tasks 26-30).
pub const PHASE2_MEDIA_METHODS: &[&str] = &[
    "send.image",
    "send.video",
    "send.audio",
    "send.voice",
    "send.sticker",
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
        assert_eq!(
            reg.methods().len(),
            PHASE1_METHODS.len() + PHASE2_MEDIA_METHODS.len(),
            "registry size must equal Phase 1 + Phase 2 (media) sets",
        );
    }
}
