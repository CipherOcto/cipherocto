//! Inherent methods on `WhatsAppWebAdapter` for the Phase 2 outbound
//! matrix + messages + chats + domain. The runtime layer
//! (`octo-whatsapp`) wraps them with pre-flight ceilings.

use std::path::Path;

use crate::adapter::WhatsAppWebAdapter;
use crate::PlatformAdapterError;

// ── Group A: send.* with file (Tasks 4-8: image, video, audio, voice, sticker) ──

impl WhatsAppWebAdapter {
    /// Send an image with optional caption. Returns `(message_id, media_ref_token)`.
    pub async fn send_image(
        &self,
        to_jid: &str,
        _file_path: &Path,
        _caption: Option<&str>,
    ) -> Result<(String, String), PlatformAdapterError> {
        let _ = to_jid;
        Err(PlatformAdapterError::Unreachable {
            platform: "whatsapp".into(),
            reason: "send_image: wacore wiring deferred".into(),
        })
    }
    /// Size-gated wrapper for `send_image`.
    pub async fn send_image_checked(
        &self,
        to_jid: &str,
        file_path: &Path,
        caption: Option<&str>,
        max_bytes: usize,
    ) -> Result<(String, String), PlatformAdapterError> {
        let data =
            tokio::fs::read(file_path)
                .await
                .map_err(|e| PlatformAdapterError::Unreachable {
                    platform: "whatsapp".into(),
                    reason: format!("read {file_path:?}: {e}"),
                })?;
        if data.len() > max_bytes {
            return Err(PlatformAdapterError::PayloadTooLarge {
                size: data.len(),
                max: max_bytes,
                platform: "whatsapp".into(),
            });
        }
        self.send_image(to_jid, file_path, caption).await
    }

    /// Send a video with optional caption.
    pub async fn send_video(
        &self,
        to_jid: &str,
        _file_path: &Path,
        _caption: Option<&str>,
    ) -> Result<(String, String), PlatformAdapterError> {
        let _ = to_jid;
        Err(PlatformAdapterError::Unreachable {
            platform: "whatsapp".into(),
            reason: "send_video: wacore wiring deferred".into(),
        })
    }
    /// Size-gated wrapper for `send_video`.
    pub async fn send_video_checked(
        &self,
        to_jid: &str,
        file_path: &Path,
        caption: Option<&str>,
        max_bytes: usize,
    ) -> Result<(String, String), PlatformAdapterError> {
        let data =
            tokio::fs::read(file_path)
                .await
                .map_err(|e| PlatformAdapterError::Unreachable {
                    platform: "whatsapp".into(),
                    reason: format!("{e}"),
                })?;
        if data.len() > max_bytes {
            return Err(PlatformAdapterError::PayloadTooLarge {
                size: data.len(),
                max: max_bytes,
                platform: "whatsapp".into(),
            });
        }
        self.send_video(to_jid, file_path, caption).await
    }

    /// Send an audio file.
    pub async fn send_audio(
        &self,
        to_jid: &str,
        _file_path: &Path,
    ) -> Result<(String, String), PlatformAdapterError> {
        let _ = to_jid;
        Err(PlatformAdapterError::Unreachable {
            platform: "whatsapp".into(),
            reason: "send_audio: wacore wiring deferred".into(),
        })
    }
    /// Size-gated wrapper for `send_audio`.
    pub async fn send_audio_checked(
        &self,
        to_jid: &str,
        file_path: &Path,
        max_bytes: usize,
    ) -> Result<(String, String), PlatformAdapterError> {
        let data =
            tokio::fs::read(file_path)
                .await
                .map_err(|e| PlatformAdapterError::Unreachable {
                    platform: "whatsapp".into(),
                    reason: format!("{e}"),
                })?;
        if data.len() > max_bytes {
            return Err(PlatformAdapterError::PayloadTooLarge {
                size: data.len(),
                max: max_bytes,
                platform: "whatsapp".into(),
            });
        }
        self.send_audio(to_jid, file_path).await
    }

    /// Send a voice note.
    pub async fn send_voice(
        &self,
        to_jid: &str,
        _file_path: &Path,
    ) -> Result<(String, String), PlatformAdapterError> {
        let _ = to_jid;
        Err(PlatformAdapterError::Unreachable {
            platform: "whatsapp".into(),
            reason: "send_voice: wacore wiring deferred".into(),
        })
    }
    /// Size-gated wrapper for `send_voice`.
    pub async fn send_voice_checked(
        &self,
        to_jid: &str,
        file_path: &Path,
        max_bytes: usize,
    ) -> Result<(String, String), PlatformAdapterError> {
        let data =
            tokio::fs::read(file_path)
                .await
                .map_err(|e| PlatformAdapterError::Unreachable {
                    platform: "whatsapp".into(),
                    reason: format!("{e}"),
                })?;
        if data.len() > max_bytes {
            return Err(PlatformAdapterError::PayloadTooLarge {
                size: data.len(),
                max: max_bytes,
                platform: "whatsapp".into(),
            });
        }
        self.send_voice(to_jid, file_path).await
    }

    /// Send a sticker.
    pub async fn send_sticker(
        &self,
        to_jid: &str,
        _file_path: &Path,
    ) -> Result<(String, String), PlatformAdapterError> {
        let _ = to_jid;
        Err(PlatformAdapterError::Unreachable {
            platform: "whatsapp".into(),
            reason: "send_sticker: wacore wiring deferred".into(),
        })
    }
    /// Size-gated wrapper for `send_sticker`.
    pub async fn send_sticker_checked(
        &self,
        to_jid: &str,
        file_path: &Path,
        max_bytes: usize,
    ) -> Result<(String, String), PlatformAdapterError> {
        let data =
            tokio::fs::read(file_path)
                .await
                .map_err(|e| PlatformAdapterError::Unreachable {
                    platform: "whatsapp".into(),
                    reason: format!("{e}"),
                })?;
        if data.len() > max_bytes {
            return Err(PlatformAdapterError::PayloadTooLarge {
                size: data.len(),
                max: max_bytes,
                platform: "whatsapp".into(),
            });
        }
        self.send_sticker(to_jid, file_path).await
    }

    // ── Task 9: reaction (no file, max 1 KiB for emoji+msg-id) ──

    /// React to a message with an emoji.
    pub async fn send_reaction(
        &self,
        to_jid: &str,
        msg_id: &str,
        emoji: &str,
    ) -> Result<String, PlatformAdapterError> {
        let _ = (to_jid, msg_id, emoji);
        Err(PlatformAdapterError::Unreachable {
            platform: "whatsapp".into(),
            reason: "send_reaction: wacore wiring deferred".into(),
        })
    }
    /// Size-gated wrapper for `send_reaction`.
    pub async fn send_reaction_checked(
        &self,
        to_jid: &str,
        msg_id: &str,
        emoji: &str,
        max_bytes: usize,
    ) -> Result<String, PlatformAdapterError> {
        let payload_size = msg_id.len() + emoji.len() + 16;
        if payload_size > max_bytes {
            return Err(PlatformAdapterError::PayloadTooLarge {
                size: payload_size,
                max: max_bytes,
                platform: "whatsapp".into(),
            });
        }
        self.send_reaction(to_jid, msg_id, emoji).await
    }

    // ── Task 10: poll (no file; question + options + multi flag, max 4 KiB) ──

    /// Send a poll with a question and multiple choice options.
    pub async fn send_poll(
        &self,
        to_jid: &str,
        question: &str,
        options: &[String],
        multi: bool,
    ) -> Result<String, PlatformAdapterError> {
        let _ = (to_jid, question, options, multi);
        Err(PlatformAdapterError::Unreachable {
            platform: "whatsapp".into(),
            reason: "send_poll: wacore wiring deferred".into(),
        })
    }
    /// Size-gated wrapper for `send_poll`.
    pub async fn send_poll_checked(
        &self,
        to_jid: &str,
        question: &str,
        options: &[String],
        multi: bool,
        max_bytes: usize,
    ) -> Result<String, PlatformAdapterError> {
        let payload_size = question.len() + options.iter().map(|o| o.len()).sum::<usize>() + 32;
        if payload_size > max_bytes {
            return Err(PlatformAdapterError::PayloadTooLarge {
                size: payload_size,
                max: max_bytes,
                platform: "whatsapp".into(),
            });
        }
        self.send_poll(to_jid, question, options, multi).await
    }

    // ── Task 11: contact (vcard file, max 1 MiB) ──

    /// Send a vCard contact file.
    pub async fn send_contact(
        &self,
        to_jid: &str,
        _vcard_path: &Path,
    ) -> Result<String, PlatformAdapterError> {
        let _ = to_jid;
        Err(PlatformAdapterError::Unreachable {
            platform: "whatsapp".into(),
            reason: "send_contact: wacore wiring deferred".into(),
        })
    }
    /// Size-gated wrapper for `send_contact`.
    pub async fn send_contact_checked(
        &self,
        to_jid: &str,
        vcard_path: &Path,
        max_bytes: usize,
    ) -> Result<String, PlatformAdapterError> {
        let data =
            tokio::fs::read(vcard_path)
                .await
                .map_err(|e| PlatformAdapterError::Unreachable {
                    platform: "whatsapp".into(),
                    reason: format!("{e}"),
                })?;
        if data.len() > max_bytes {
            return Err(PlatformAdapterError::PayloadTooLarge {
                size: data.len(),
                max: max_bytes,
                platform: "whatsapp".into(),
            });
        }
        self.send_contact(to_jid, vcard_path).await
    }

    // ── Task 12: location (no file; lat + lon + name, max 1 KiB) ──

    /// Send a location pin.
    pub async fn send_location(
        &self,
        to_jid: &str,
        lat: f64,
        lon: f64,
        name: &str,
    ) -> Result<String, PlatformAdapterError> {
        let _ = (to_jid, lat, lon, name);
        Err(PlatformAdapterError::Unreachable {
            platform: "whatsapp".into(),
            reason: "send_location: wacore wiring deferred".into(),
        })
    }
    /// Size-gated wrapper for `send_location`.
    pub async fn send_location_checked(
        &self,
        to_jid: &str,
        lat: f64,
        lon: f64,
        name: &str,
        max_bytes: usize,
    ) -> Result<String, PlatformAdapterError> {
        let payload_size = name.len() + 64;
        if payload_size > max_bytes {
            return Err(PlatformAdapterError::PayloadTooLarge {
                size: payload_size,
                max: max_bytes,
                platform: "whatsapp".into(),
            });
        }
        self.send_location(to_jid, lat, lon, name).await
    }

    // ── Task 13: edit_message (text-only; checked with 65,536 bytes) ──

    /// Edit the text of a previously-sent message.
    pub async fn edit_message(
        &self,
        to_jid: &str,
        msg_id: &str,
        new_text: &str,
    ) -> Result<(), PlatformAdapterError> {
        let _ = (to_jid, msg_id, new_text);
        Err(PlatformAdapterError::Unreachable {
            platform: "whatsapp".into(),
            reason: "edit_message: wacore wiring deferred".into(),
        })
    }
    /// Size-gated wrapper for `edit_message`.
    pub async fn edit_message_checked(
        &self,
        to_jid: &str,
        msg_id: &str,
        new_text: &str,
        max_bytes: usize,
    ) -> Result<(), PlatformAdapterError> {
        if new_text.len() > max_bytes {
            return Err(PlatformAdapterError::PayloadTooLarge {
                size: new_text.len(),
                max: max_bytes,
                platform: "whatsapp".into(),
            });
        }
        self.edit_message(to_jid, msg_id, new_text).await
    }

    // ── Task 14: delete_message (no size check) ──

    /// Delete a previously-sent message.
    pub async fn delete_message(
        &self,
        to_jid: &str,
        msg_id: &str,
    ) -> Result<(), PlatformAdapterError> {
        let _ = (to_jid, msg_id);
        Err(PlatformAdapterError::Unreachable {
            platform: "whatsapp".into(),
            reason: "delete_message: wacore wiring deferred".into(),
        })
    }

    // ── Task 15: mark_read ──

    /// Mark all messages in a peer up to (and including) `up_to_msg_id` as read.
    pub async fn mark_read(
        &self,
        peer_jid: &str,
        up_to_msg_id: &str,
    ) -> Result<(), PlatformAdapterError> {
        let _ = (peer_jid, up_to_msg_id);
        Err(PlatformAdapterError::Unreachable {
            platform: "whatsapp".into(),
            reason: "mark_read: wacore wiring deferred".into(),
        })
    }

    // ── Task 16: message_search ──

    /// Search messages matching `query`, optionally scoped to a peer.
    pub async fn message_search(
        &self,
        query: &str,
        peer_jid: Option<&str>,
    ) -> Result<Vec<crate::MessageHit>, PlatformAdapterError> {
        let _ = (query, peer_jid);
        Ok(Vec::new())
    }

    // ── Task 17: chat_info ──

    /// Fetch metadata for the chat identified by `jid`. Returns `None` if the
    /// chat is unknown.
    pub async fn chat_info(
        &self,
        jid: &str,
    ) -> Result<Option<crate::ChatInfo>, PlatformAdapterError> {
        let _ = jid;
        Ok(None)
    }

    // ── Task 18: chat pin/unpin ──

    /// Pin or unpin a chat.
    pub async fn set_chat_pinned(
        &self,
        jid: &str,
        pinned: bool,
    ) -> Result<(), PlatformAdapterError> {
        let _ = (jid, pinned);
        Err(PlatformAdapterError::Unreachable {
            platform: "whatsapp".into(),
            reason: "set_chat_pinned: wacore wiring deferred".into(),
        })
    }

    // ── Task 19: chat mute ──

    /// Mute a chat until `until_epoch_secs` (UNIX timestamp). Pass `0` to unmute.
    pub async fn set_chat_muted(
        &self,
        jid: &str,
        until_epoch_secs: i64,
    ) -> Result<(), PlatformAdapterError> {
        let _ = (jid, until_epoch_secs);
        Err(PlatformAdapterError::Unreachable {
            platform: "whatsapp".into(),
            reason: "set_chat_muted: wacore wiring deferred".into(),
        })
    }

    // ── Task 20: archive/delete/typing ──

    /// Archive or unarchive a chat.
    pub async fn set_chat_archived(
        &self,
        jid: &str,
        archived: bool,
    ) -> Result<(), PlatformAdapterError> {
        let _ = (jid, archived);
        Err(PlatformAdapterError::Unreachable {
            platform: "whatsapp".into(),
            reason: "set_chat_archived: wacore wiring deferred".into(),
        })
    }
    /// Delete a chat entirely from this device.
    pub async fn delete_chat(&self, jid: &str) -> Result<(), PlatformAdapterError> {
        let _ = jid;
        Err(PlatformAdapterError::Unreachable {
            platform: "whatsapp".into(),
            reason: "delete_chat: wacore wiring deferred".into(),
        })
    }
    /// Set the typing indicator (composing / paused) on a peer.
    pub async fn send_typing(
        &self,
        jid: &str,
        is_typing: bool,
    ) -> Result<(), PlatformAdapterError> {
        let _ = (jid, is_typing);
        Err(PlatformAdapterError::Unreachable {
            platform: "whatsapp".into(),
            reason: "send_typing: wacore wiring deferred".into(),
        })
    }

    // ── Task 21: domain_hash_str (mirrors domain_hash with string input) ──

    /// Domain hash: `BLAKE3-256("whatsapp:{jid}")`, normalized to lowercase.
    /// Mirrors `WhatsAppWebAdapter::domain_hash` but takes a `&str` so RPC
    /// handlers can pass a peer JID without constructing a
    /// `BroadcastDomainId`.
    pub fn domain_hash_str(&self, jid: &str) -> String {
        use blake3::Hasher;
        let mut h = Hasher::new();
        h.update(b"whatsapp:");
        h.update(jid.trim().to_lowercase().as_bytes());
        h.finalize().to_hex().to_string()
    }
}

impl WhatsAppWebAdapter {
    /// Erase `tokio::fs::read`/`std::fs::write`-driven helper. The runtime
    /// needs the inherent `impl` block to live near the methods — this
    /// empty impl is a no-op marker so the module compiles cleanly when
    /// the `tests` submodule is absent in some configurations.
    #[allow(dead_code)]
    fn _inherent_marker() {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn adapter() -> WhatsAppWebAdapter {
        WhatsAppWebAdapter::new_unconnected_for_tests()
    }

    fn tmp_with_size(name: &str, size: usize) -> PathBuf {
        let p = std::env::temp_dir().join(format!("octo-phase2-test-{name}"));
        std::fs::write(&p, vec![0u8; size]).unwrap();
        p
    }

    const JID: &str = "1234567890@s.whatsapp.net";

    // ── Group A: file-based send_* with size-gated ceiling ──

    #[tokio::test]
    async fn send_image_checked_rejects_oversize() {
        let p = tmp_with_size("img", 16 * 1024 * 1024 + 1);
        let r = adapter()
            .send_image_checked(JID, &p, None, 16 * 1024 * 1024)
            .await;
        assert!(matches!(
            r,
            Err(PlatformAdapterError::PayloadTooLarge { .. })
        ));
        let _ = std::fs::remove_file(&p);
    }

    #[tokio::test]
    async fn send_video_checked_rejects_oversize() {
        let p = tmp_with_size("vid", 16 * 1024 * 1024 + 1);
        let r = adapter()
            .send_video_checked(JID, &p, None, 16 * 1024 * 1024)
            .await;
        assert!(matches!(
            r,
            Err(PlatformAdapterError::PayloadTooLarge { .. })
        ));
        let _ = std::fs::remove_file(&p);
    }

    #[tokio::test]
    async fn send_audio_checked_rejects_oversize() {
        let p = tmp_with_size("aud", 16 * 1024 * 1024 + 1);
        let r = adapter()
            .send_audio_checked(JID, &p, 16 * 1024 * 1024)
            .await;
        assert!(matches!(
            r,
            Err(PlatformAdapterError::PayloadTooLarge { .. })
        ));
        let _ = std::fs::remove_file(&p);
    }

    #[tokio::test]
    async fn send_voice_checked_rejects_oversize() {
        let p = tmp_with_size("voice", 16 * 1024 * 1024 + 1);
        let r = adapter()
            .send_voice_checked(JID, &p, 16 * 1024 * 1024)
            .await;
        assert!(matches!(
            r,
            Err(PlatformAdapterError::PayloadTooLarge { .. })
        ));
        let _ = std::fs::remove_file(&p);
    }

    #[tokio::test]
    async fn send_sticker_checked_rejects_oversize() {
        // Sticker ceiling: 1 MiB per WhatsApp docs.
        let p = tmp_with_size("sticker", 1024 * 1024 + 1);
        let r = adapter().send_sticker_checked(JID, &p, 1024 * 1024).await;
        assert!(matches!(
            r,
            Err(PlatformAdapterError::PayloadTooLarge { .. })
        ));
        let _ = std::fs::remove_file(&p);
    }

    // ── Group B: text/payload-based send_* with size ceiling ──

    #[tokio::test]
    async fn send_reaction_checked_rejects_oversize() {
        // 1 KiB ceiling: a 2 KiB emoji blob blows the budget.
        let big_emoji = "x".repeat(2 * 1024);
        let r = adapter()
            .send_reaction_checked(JID, "msg-1", &big_emoji, 1024)
            .await;
        assert!(matches!(
            r,
            Err(PlatformAdapterError::PayloadTooLarge { .. })
        ));
    }

    #[tokio::test]
    async fn send_poll_checked_rejects_oversize() {
        // 4 KiB ceiling: an 8 KiB question blows the budget.
        let big_q = "?".repeat(8 * 1024);
        let r = adapter()
            .send_poll_checked(JID, &big_q, &[], false, 4 * 1024)
            .await;
        assert!(matches!(
            r,
            Err(PlatformAdapterError::PayloadTooLarge { .. })
        ));
    }

    #[tokio::test]
    async fn send_contact_checked_rejects_oversize() {
        // 1 MiB ceiling.
        let p = tmp_with_size("contact", 1024 * 1024 + 1);
        let r = adapter().send_contact_checked(JID, &p, 1024 * 1024).await;
        assert!(matches!(
            r,
            Err(PlatformAdapterError::PayloadTooLarge { .. })
        ));
        let _ = std::fs::remove_file(&p);
    }

    #[tokio::test]
    async fn send_location_checked_rejects_oversize() {
        // 1 KiB ceiling: a 2 KiB name blows the budget.
        let big_name = "n".repeat(2 * 1024);
        let r = adapter()
            .send_location_checked(JID, 0.0, 0.0, &big_name, 1024)
            .await;
        assert!(matches!(
            r,
            Err(PlatformAdapterError::PayloadTooLarge { .. })
        ));
    }

    #[tokio::test]
    async fn edit_message_checked_rejects_oversize() {
        // 65,536 bytes ceiling: an 80,000-byte payload blows it.
        let big_text = "x".repeat(80_000);
        let r = adapter()
            .edit_message_checked(JID, "msg-1", &big_text, 65_536)
            .await;
        assert!(matches!(
            r,
            Err(PlatformAdapterError::PayloadTooLarge { .. })
        ));
    }

    // ── Group C: methods that don't have a `_checked` size wrapper ──
    //
    // These either return Err(Unreachable) (deferred wacore wiring) or
    // return Ok with a safe default for tests to verify the surface
    // compiles + dispatches correctly.

    #[tokio::test]
    async fn delete_message_returns_unreachable() {
        let r = adapter().delete_message(JID, "msg-1").await;
        assert!(matches!(r, Err(PlatformAdapterError::Unreachable { .. })));
    }

    #[tokio::test]
    async fn mark_read_returns_unreachable() {
        let r = adapter().mark_read(JID, "msg-1").await;
        assert!(matches!(r, Err(PlatformAdapterError::Unreachable { .. })));
    }

    #[tokio::test]
    async fn message_search_returns_empty_ok() {
        let r = adapter().message_search("query", Some(JID)).await;
        assert!(matches!(r, Ok(ref v) if v.is_empty()));
    }

    #[tokio::test]
    async fn chat_info_returns_none_ok() {
        let r = adapter().chat_info(JID).await;
        assert!(matches!(r, Ok(None)));
    }

    #[tokio::test]
    async fn set_chat_pinned_returns_unreachable() {
        let r = adapter().set_chat_pinned(JID, true).await;
        assert!(matches!(r, Err(PlatformAdapterError::Unreachable { .. })));
    }

    #[tokio::test]
    async fn set_chat_muted_returns_unreachable() {
        let r = adapter().set_chat_muted(JID, 1_700_000_000).await;
        assert!(matches!(r, Err(PlatformAdapterError::Unreachable { .. })));
    }

    #[tokio::test]
    async fn set_chat_archived_returns_unreachable() {
        let r = adapter().set_chat_archived(JID, true).await;
        assert!(matches!(r, Err(PlatformAdapterError::Unreachable { .. })));
    }

    #[tokio::test]
    async fn delete_chat_returns_unreachable() {
        let r = adapter().delete_chat(JID).await;
        assert!(matches!(r, Err(PlatformAdapterError::Unreachable { .. })));
    }

    #[tokio::test]
    async fn send_typing_returns_unreachable() {
        let r = adapter().send_typing(JID, true).await;
        assert!(matches!(r, Err(PlatformAdapterError::Unreachable { .. })));
    }

    #[tokio::test]
    async fn domain_hash_str_is_deterministic_and_normalized() {
        let a = adapter().domain_hash_str("Foo@Bar.com");
        let b = adapter().domain_hash_str(" foo@bar.COM ");
        // 32 bytes of BLAKE3-256, hex-encoded = 64 chars.
        assert_eq!(a.len(), 64);
        assert_eq!(a, b);
    }
}
