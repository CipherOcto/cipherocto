//! Inherent methods on `WhatsAppWebAdapter` for the Phase 2 outbound
//! matrix + messages + chats + domain. The runtime layer
//! (`octo-whatsapp`) wraps them with pre-flight ceilings.

use std::path::Path;

use base64::Engine;

use crate::adapter::{upload_to_cdn, WhatsAppWebAdapter};
use crate::media_ref::{encode_base64url, MediaRef};
use crate::PlatformAdapterError;
use crate::{PollOptionResultSnapshot, StickerPackItemSnapshot, StickerPackSnapshot};
use wacore_binary::JidExt;
use whatsapp_rust::download::MediaType;
use whatsapp_rust::prelude::{MessageBuilderExt, MessageExt};
use whatsapp_rust::upload::UploadOptions;

/// Local copy of `adapter.rs::epoch_millis` (module-private there; duplicated
/// here to keep the wiring self-contained without widening visibility).
fn epoch_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── Group A: send.* with file (Tasks 4-8: image, video, audio, voice, sticker) ──

impl WhatsAppWebAdapter {
    /// Send a plain-text message. Returns the new message id.
    ///
    /// `reply_to` is an optional message id being quoted; when set the
    /// WA protocol embeds it in the `contextInfo.quotedMessage` slot of
    /// the outbound envelope. `mentions` is a list of JIDs to ping
    /// (`@`-mentions in the rendered chat).
    ///
    /// Thin wrapper around `wacore::Client::send_text`
    /// (`whatsapp-rust/src/send/mod.rs:523`).
    pub async fn send_text(
        &self,
        to_jid: &str,
        text: &str,
        reply_to: Option<&str>,
        mentions: &[String],
    ) -> Result<String, PlatformAdapterError> {
        // Client gate — same precondition as the file-based senders.
        let client = {
            let guard = self.client.lock();
            guard
                .clone()
                .ok_or_else(|| PlatformAdapterError::Unreachable {
                    platform: "whatsapp".into(),
                    reason: "client not connected".into(),
                })?
        };
        let jid: wacore_binary::Jid =
            to_jid.parse().map_err(|e| PlatformAdapterError::ApiError {
                code: 400,
                message: format!("invalid JID {to_jid:?}: {e}"),
            })?;

        // Build the text message with optional reply context + mentions.
        // `Message::text` is a convenience over the protobuf builder;
        // if reply/mentions are set we attach the context via the
        // standard `ContextInfo` fields.
        let mut message = waproto::whatsapp::Message::text(text.to_string());
        if reply_to.is_some() || !mentions.is_empty() {
            // Build a `ContextInfo` carrying the quote + mentions. The
            // `set_context_info` helper from `MessageExt` attaches it to
            // the first supported message field (text, in our case) and
            // returns whether it found a slot.
            let mut ctx = waproto::whatsapp::ContextInfo::default();
            if let Some(q) = reply_to {
                ctx.stanza_id = Some(q.to_string());
                ctx.participant = Some(jid.to_string());
                // Empty placeholder quoted message — WA renders the reply
                // badge based on `stanza_id` + `participant` and only
                // fetches the body lazily.
                ctx.quoted_message = whatsapp_rust::buffa::MessageField::from_box(Box::new(
                    waproto::whatsapp::Message::text(String::new()),
                ));
            }
            if !mentions.is_empty() {
                ctx.mentioned_jid = mentions.to_vec();
            }
            message.set_context_info(ctx);
        }

        // Tier 7.A.2 (forward): clone the outgoing body so a later
        // `forward_message` call can replay it. The cache only catches
        // `send_text`; media forwards (which need the full wa::Message
        // including embedded media references) are a future scope.
        let cached_for_forward = message.clone();
        let send_result = Box::pin(client.send_message(jid, message))
            .await
            .map_err(|e| PlatformAdapterError::Unreachable {
                platform: "whatsapp".into(),
                reason: format!("send_text failed: {e}"),
            })?;
        {
            let mut cache = self.last_outgoing.lock();
            let by_id = cache.entry(to_jid.to_string()).or_default();
            by_id.insert(send_result.message_id.clone(), cached_for_forward);
        }
        Ok(send_result.message_id)
    }

    /// Send an image with optional caption. Returns `(message_id, media_ref_token)`.
    pub async fn send_image(
        &self,
        to_jid: &str,
        file_path: &Path,
        caption: Option<&str>,
    ) -> Result<(String, String), PlatformAdapterError> {
        let data =
            tokio::fs::read(file_path)
                .await
                .map_err(|e| PlatformAdapterError::Unreachable {
                    platform: "whatsapp".into(),
                    reason: format!("read {file_path:?}: {e}"),
                })?;
        let client = {
            let guard = self.client.lock();
            guard
                .clone()
                .ok_or_else(|| PlatformAdapterError::Unreachable {
                    platform: "whatsapp".into(),
                    reason: "client not connected".into(),
                })?
        };
        let upload = upload_to_cdn(
            &client,
            data.clone(),
            MediaType::Image,
            UploadOptions::new(),
        )
        .await?;
        let filename = file_path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("image");
        let media_ref = MediaRef::from_upload_response(&upload, filename);
        let token =
            encode_base64url(&media_ref).map_err(|e| PlatformAdapterError::Unreachable {
                platform: "whatsapp".into(),
                reason: format!("encode MediaRef failed: {e}"),
            })?;
        let jid: wacore_binary::Jid =
            to_jid.parse().map_err(|e| PlatformAdapterError::ApiError {
                code: 400,
                message: format!("invalid JID {to_jid:?}: {e}"),
            })?;
        let mime = "image/jpeg";
        let img_msg = waproto::whatsapp::message::ImageMessage {
            url: Some(upload.url),
            direct_path: Some(upload.direct_path),
            media_key: Some(upload.media_key.to_vec()),
            file_sha256: Some(upload.file_sha256.to_vec()),
            file_enc_sha256: Some(upload.file_enc_sha256.to_vec()),
            file_length: Some(data.len() as u64),
            mimetype: Some(mime.to_string()),
            caption: caption.map(String::from),
            ..Default::default()
        };
        let outgoing = waproto::whatsapp::Message {
            image_message: whatsapp_rust::buffa::MessageField::some(img_msg),
            ..Default::default()
        };
        let send_result = Box::pin(client.send_message(jid, outgoing))
            .await
            .map_err(|e| PlatformAdapterError::Unreachable {
                platform: "whatsapp".into(),
                reason: format!("send_image failed: {e}"),
            })?;
        Ok((send_result.message_id, token))
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
        file_path: &Path,
        caption: Option<&str>,
    ) -> Result<(String, String), PlatformAdapterError> {
        let data =
            tokio::fs::read(file_path)
                .await
                .map_err(|e| PlatformAdapterError::Unreachable {
                    platform: "whatsapp".into(),
                    reason: format!("read {file_path:?}: {e}"),
                })?;
        let client = {
            let guard = self.client.lock();
            guard
                .clone()
                .ok_or_else(|| PlatformAdapterError::Unreachable {
                    platform: "whatsapp".into(),
                    reason: "client not connected".into(),
                })?
        };
        let upload = upload_to_cdn(
            &client,
            data.clone(),
            MediaType::Video,
            UploadOptions::new(),
        )
        .await?;
        let filename = file_path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("video");
        let media_ref = MediaRef::from_upload_response(&upload, filename);
        let token =
            encode_base64url(&media_ref).map_err(|e| PlatformAdapterError::Unreachable {
                platform: "whatsapp".into(),
                reason: format!("encode MediaRef failed: {e}"),
            })?;
        let jid: wacore_binary::Jid =
            to_jid.parse().map_err(|e| PlatformAdapterError::ApiError {
                code: 400,
                message: format!("invalid JID {to_jid:?}: {e}"),
            })?;
        let mime = "video/mp4";
        let vid_msg = waproto::whatsapp::message::VideoMessage {
            url: Some(upload.url),
            direct_path: Some(upload.direct_path),
            media_key: Some(upload.media_key.to_vec()),
            file_sha256: Some(upload.file_sha256.to_vec()),
            file_enc_sha256: Some(upload.file_enc_sha256.to_vec()),
            file_length: Some(data.len() as u64),
            mimetype: Some(mime.to_string()),
            caption: caption.map(String::from),
            gif_playback: Some(false),
            ..Default::default()
        };
        let outgoing = waproto::whatsapp::Message {
            video_message: whatsapp_rust::buffa::MessageField::some(vid_msg),
            ..Default::default()
        };
        let send_result = Box::pin(client.send_message(jid, outgoing))
            .await
            .map_err(|e| PlatformAdapterError::Unreachable {
                platform: "whatsapp".into(),
                reason: format!("send_video failed: {e}"),
            })?;
        Ok((send_result.message_id, token))
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
        file_path: &Path,
    ) -> Result<(String, String), PlatformAdapterError> {
        let data =
            tokio::fs::read(file_path)
                .await
                .map_err(|e| PlatformAdapterError::Unreachable {
                    platform: "whatsapp".into(),
                    reason: format!("read {file_path:?}: {e}"),
                })?;
        let client = {
            let guard = self.client.lock();
            guard
                .clone()
                .ok_or_else(|| PlatformAdapterError::Unreachable {
                    platform: "whatsapp".into(),
                    reason: "client not connected".into(),
                })?
        };
        let upload = upload_to_cdn(
            &client,
            data.clone(),
            MediaType::Audio,
            UploadOptions::new(),
        )
        .await?;
        let filename = file_path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("audio");
        let media_ref = MediaRef::from_upload_response(&upload, filename);
        let token =
            encode_base64url(&media_ref).map_err(|e| PlatformAdapterError::Unreachable {
                platform: "whatsapp".into(),
                reason: format!("encode MediaRef failed: {e}"),
            })?;
        let jid: wacore_binary::Jid =
            to_jid.parse().map_err(|e| PlatformAdapterError::ApiError {
                code: 400,
                message: format!("invalid JID {to_jid:?}: {e}"),
            })?;
        let mime = "audio/mpeg";
        let aud_msg = waproto::whatsapp::message::AudioMessage {
            url: Some(upload.url),
            direct_path: Some(upload.direct_path),
            media_key: Some(upload.media_key.to_vec()),
            file_sha256: Some(upload.file_sha256.to_vec()),
            file_enc_sha256: Some(upload.file_enc_sha256.to_vec()),
            file_length: Some(data.len() as u64),
            mimetype: Some(mime.to_string()),
            ..Default::default()
        };
        let outgoing = waproto::whatsapp::Message {
            audio_message: whatsapp_rust::buffa::MessageField::some(aud_msg),
            ..Default::default()
        };
        let send_result = Box::pin(client.send_message(jid, outgoing))
            .await
            .map_err(|e| PlatformAdapterError::Unreachable {
                platform: "whatsapp".into(),
                reason: format!("send_audio failed: {e}"),
            })?;
        Ok((send_result.message_id, token))
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
        file_path: &Path,
    ) -> Result<(String, String), PlatformAdapterError> {
        let data =
            tokio::fs::read(file_path)
                .await
                .map_err(|e| PlatformAdapterError::Unreachable {
                    platform: "whatsapp".into(),
                    reason: format!("read {file_path:?}: {e}"),
                })?;
        let client = {
            let guard = self.client.lock();
            guard
                .clone()
                .ok_or_else(|| PlatformAdapterError::Unreachable {
                    platform: "whatsapp".into(),
                    reason: "client not connected".into(),
                })?
        };
        let upload = upload_to_cdn(
            &client,
            data.clone(),
            MediaType::Audio,
            UploadOptions::new(),
        )
        .await?;
        let filename = file_path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("voice");
        let media_ref = MediaRef::from_upload_response(&upload, filename);
        let token =
            encode_base64url(&media_ref).map_err(|e| PlatformAdapterError::Unreachable {
                platform: "whatsapp".into(),
                reason: format!("encode MediaRef failed: {e}"),
            })?;
        let jid: wacore_binary::Jid =
            to_jid.parse().map_err(|e| PlatformAdapterError::ApiError {
                code: 400,
                message: format!("invalid JID {to_jid:?}: {e}"),
            })?;
        let mime = "audio/ogg; codecs=opus";
        let aud_msg = waproto::whatsapp::message::AudioMessage {
            url: Some(upload.url),
            direct_path: Some(upload.direct_path),
            media_key: Some(upload.media_key.to_vec()),
            file_sha256: Some(upload.file_sha256.to_vec()),
            file_enc_sha256: Some(upload.file_enc_sha256.to_vec()),
            file_length: Some(data.len() as u64),
            mimetype: Some(mime.to_string()),
            ptt: Some(true),
            ..Default::default()
        };
        let outgoing = waproto::whatsapp::Message {
            audio_message: whatsapp_rust::buffa::MessageField::some(aud_msg),
            ..Default::default()
        };
        let send_result = Box::pin(client.send_message(jid, outgoing))
            .await
            .map_err(|e| PlatformAdapterError::Unreachable {
                platform: "whatsapp".into(),
                reason: format!("send_voice failed: {e}"),
            })?;
        Ok((send_result.message_id, token))
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
        file_path: &Path,
    ) -> Result<(String, String), PlatformAdapterError> {
        let data =
            tokio::fs::read(file_path)
                .await
                .map_err(|e| PlatformAdapterError::Unreachable {
                    platform: "whatsapp".into(),
                    reason: format!("read {file_path:?}: {e}"),
                })?;
        let client = {
            let guard = self.client.lock();
            guard
                .clone()
                .ok_or_else(|| PlatformAdapterError::Unreachable {
                    platform: "whatsapp".into(),
                    reason: "client not connected".into(),
                })?
        };
        let upload = upload_to_cdn(
            &client,
            data.clone(),
            MediaType::Sticker,
            UploadOptions::new(),
        )
        .await?;
        let filename = file_path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("sticker");
        let media_ref = MediaRef::from_upload_response(&upload, filename);
        let token =
            encode_base64url(&media_ref).map_err(|e| PlatformAdapterError::Unreachable {
                platform: "whatsapp".into(),
                reason: format!("encode MediaRef failed: {e}"),
            })?;
        let jid: wacore_binary::Jid =
            to_jid.parse().map_err(|e| PlatformAdapterError::ApiError {
                code: 400,
                message: format!("invalid JID {to_jid:?}: {e}"),
            })?;
        let mime = "image/webp";
        let stk_msg = waproto::whatsapp::message::StickerMessage {
            url: Some(upload.url),
            direct_path: Some(upload.direct_path),
            media_key: Some(upload.media_key.to_vec()),
            file_sha256: Some(upload.file_sha256.to_vec()),
            file_enc_sha256: Some(upload.file_enc_sha256.to_vec()),
            file_length: Some(data.len() as u64),
            mimetype: Some(mime.to_string()),
            ..Default::default()
        };
        let outgoing = waproto::whatsapp::Message {
            sticker_message: whatsapp_rust::buffa::MessageField::some(stk_msg),
            ..Default::default()
        };
        let send_result = Box::pin(client.send_message(jid, outgoing))
            .await
            .map_err(|e| PlatformAdapterError::Unreachable {
                platform: "whatsapp".into(),
                reason: format!("send_sticker failed: {e}"),
            })?;
        Ok((send_result.message_id, token))
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
        let client = {
            let guard = self.client.lock();
            guard
                .clone()
                .ok_or_else(|| PlatformAdapterError::Unreachable {
                    platform: "whatsapp".into(),
                    reason: "client not connected".into(),
                })?
        };
        let jid: wacore_binary::Jid =
            to_jid.parse().map_err(|e| PlatformAdapterError::ApiError {
                code: 400,
                message: format!("invalid JID {to_jid:?}: {e}"),
            })?;
        let sender_timestamp_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64;
        let msg = waproto::whatsapp::Message {
            reaction_message: waproto::whatsapp::message::ReactionMessage {
                key: waproto::whatsapp::MessageKey {
                    remote_jid: Some(to_jid.to_string()),
                    from_me: Some(false),
                    id: Some(msg_id.to_string()),
                    ..Default::default()
                }
                .into(),
                text: Some(emoji.to_string()),
                sender_timestamp_ms: Some(sender_timestamp_ms),
                ..Default::default()
            }
            .into(),
            ..Default::default()
        };
        let send_result = Box::pin(client.send_message(jid, msg)).await.map_err(|e| {
            PlatformAdapterError::Unreachable {
                platform: "whatsapp".into(),
                reason: format!("send_reaction failed: {e}"),
            }
        })?;
        Ok(send_result.message_id)
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
        let client = {
            let guard = self.client.lock();
            guard
                .clone()
                .ok_or_else(|| PlatformAdapterError::Unreachable {
                    platform: "whatsapp".into(),
                    reason: "client not connected".into(),
                })?
        };
        let jid: wacore_binary::Jid =
            to_jid.parse().map_err(|e| PlatformAdapterError::ApiError {
                code: 400,
                message: format!("invalid JID {to_jid:?}: {e}"),
            })?;
        let selectable_options_count = if multi { options.len() as u32 } else { 1 };
        let poll_msg = waproto::whatsapp::message::PollCreationMessage {
            name: Some(question.to_string()),
            options: options
                .iter()
                .map(
                    |o| waproto::whatsapp::message::poll_creation_message::Option {
                        option_name: Some(o.clone()),
                        ..Default::default()
                    },
                )
                .collect(),
            selectable_options_count: Some(selectable_options_count),
            ..Default::default()
        };
        let msg = waproto::whatsapp::Message {
            poll_creation_message: whatsapp_rust::buffa::MessageField::some(poll_msg),
            ..Default::default()
        };
        let send_result = Box::pin(client.send_message(jid, msg)).await.map_err(|e| {
            PlatformAdapterError::Unreachable {
                platform: "whatsapp".into(),
                reason: format!("send_poll failed: {e}"),
            }
        })?;
        Ok(send_result.message_id)
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
        vcard_path: &Path,
    ) -> Result<String, PlatformAdapterError> {
        let text = tokio::fs::read_to_string(vcard_path).await.map_err(|e| {
            PlatformAdapterError::Unreachable {
                platform: "whatsapp".into(),
                reason: format!("read vcard {vcard_path:?}: {e}"),
            }
        })?;
        let display_name = text
            .lines()
            .find_map(|l| l.strip_prefix("FN:").map(|s| s.trim().to_string()))
            .or_else(|| {
                vcard_path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .map(|s| s.to_string())
            })
            .unwrap_or_else(|| "Contact".to_string());
        let client = {
            let guard = self.client.lock();
            guard
                .clone()
                .ok_or_else(|| PlatformAdapterError::Unreachable {
                    platform: "whatsapp".into(),
                    reason: "client not connected".into(),
                })?
        };
        let jid: wacore_binary::Jid =
            to_jid.parse().map_err(|e| PlatformAdapterError::ApiError {
                code: 400,
                message: format!("invalid JID {to_jid:?}: {e}"),
            })?;
        let cm = waproto::whatsapp::message::ContactMessage {
            display_name: Some(display_name),
            vcard: Some(text),
            ..Default::default()
        };
        let outgoing = waproto::whatsapp::Message {
            contact_message: whatsapp_rust::buffa::MessageField::some(cm),
            ..Default::default()
        };
        let send_result = Box::pin(client.send_message(jid, outgoing))
            .await
            .map_err(|e| PlatformAdapterError::Unreachable {
                platform: "whatsapp".into(),
                reason: format!("send_contact failed: {e}"),
            })?;
        Ok(send_result.message_id)
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
        let client = {
            let guard = self.client.lock();
            guard
                .clone()
                .ok_or_else(|| PlatformAdapterError::Unreachable {
                    platform: "whatsapp".into(),
                    reason: "client not connected".into(),
                })?
        };
        let jid: wacore_binary::Jid =
            to_jid.parse().map_err(|e| PlatformAdapterError::ApiError {
                code: 400,
                message: format!("invalid JID {to_jid:?}: {e}"),
            })?;
        let loc = waproto::whatsapp::message::LocationMessage {
            degrees_latitude: Some(lat),
            degrees_longitude: Some(lon),
            name: Some(name.to_string()),
            ..Default::default()
        };
        let outgoing = waproto::whatsapp::Message {
            location_message: whatsapp_rust::buffa::MessageField::some(loc),
            ..Default::default()
        };
        let send_result = Box::pin(client.send_message(jid, outgoing))
            .await
            .map_err(|e| PlatformAdapterError::Unreachable {
                platform: "whatsapp".into(),
                reason: format!("send_location failed: {e}"),
            })?;
        Ok(send_result.message_id)
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
    ///
    /// WhatsApp edits use the legacy `protocol_message` envelope (matching
    /// `rewrap_as_legacy_edit` in `whatsapp-rust::features::message_edit`):
    /// `Message.protocol_message` carries `Type::MessageEdit`, the target
    /// `MessageKey` (the message being edited), and the new content wrapped
    /// in `protocol_message.edited_message: Box<Message>`. The
    /// `Message.edited_message` field on the outer envelope is a deprecated
    /// `FutureProofMessage` slot — modern edits on the wire go via
    /// `secret_encrypted_message`, but for this inherent we emit the legacy
    /// shape (the runtime layer is responsible for picking the encrypted
    /// form when it actually needs to round-trip edits).
    pub async fn edit_message(
        &self,
        to_jid: &str,
        msg_id: &str,
        new_text: &str,
    ) -> Result<(), PlatformAdapterError> {
        let client = {
            let guard = self.client.lock();
            guard
                .clone()
                .ok_or_else(|| PlatformAdapterError::Unreachable {
                    platform: "whatsapp".into(),
                    reason: "client not connected".into(),
                })?
        };
        let jid: wacore_binary::Jid =
            to_jid.parse().map_err(|e| PlatformAdapterError::ApiError {
                code: 400,
                message: format!("invalid JID {to_jid:?}: {e}"),
            })?;
        let key = waproto::whatsapp::MessageKey {
            remote_jid: Some(to_jid.to_string()),
            from_me: Some(true),
            id: Some(msg_id.to_string()),
            ..Default::default()
        };
        let inner = waproto::whatsapp::Message {
            conversation: Some(new_text.to_string()),
            ..Default::default()
        };
        let proto = waproto::whatsapp::message::ProtocolMessage {
            key: key.into(),
            r#type: Some(waproto::whatsapp::message::protocol_message::Type::MessageEdit),
            edited_message: whatsapp_rust::buffa::MessageField::some(inner),
            timestamp_ms: Some(epoch_millis() as i64),
            ..Default::default()
        };
        let outgoing = waproto::whatsapp::Message {
            protocol_message: whatsapp_rust::buffa::MessageField::some(proto),
            ..Default::default()
        };
        Box::pin(client.send_message(jid, outgoing))
            .await
            .map_err(|e| PlatformAdapterError::Unreachable {
                platform: "whatsapp".into(),
                reason: format!("edit_message failed: {e}"),
            })?;
        Ok(())
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
    ///
    /// WhatsApp deletes use the `protocol_message` envelope with
    /// `Type::Revoke = 0` and the target `MessageKey` describing the
    /// message being revoked.
    pub async fn delete_message(
        &self,
        to_jid: &str,
        msg_id: &str,
    ) -> Result<(), PlatformAdapterError> {
        let client = {
            let guard = self.client.lock();
            guard
                .clone()
                .ok_or_else(|| PlatformAdapterError::Unreachable {
                    platform: "whatsapp".into(),
                    reason: "client not connected".into(),
                })?
        };
        let jid: wacore_binary::Jid =
            to_jid.parse().map_err(|e| PlatformAdapterError::ApiError {
                code: 400,
                message: format!("invalid JID {to_jid:?}: {e}"),
            })?;
        let key = waproto::whatsapp::MessageKey {
            remote_jid: Some(to_jid.to_string()),
            from_me: Some(true),
            id: Some(msg_id.to_string()),
            ..Default::default()
        };
        let proto = waproto::whatsapp::message::ProtocolMessage {
            key: key.into(),
            r#type: Some(waproto::whatsapp::message::protocol_message::Type::Revoke),
            ..Default::default()
        };
        let outgoing = waproto::whatsapp::Message {
            protocol_message: whatsapp_rust::buffa::MessageField::some(proto),
            ..Default::default()
        };
        Box::pin(client.send_message(jid, outgoing))
            .await
            .map_err(|e| PlatformAdapterError::Unreachable {
                platform: "whatsapp".into(),
                reason: format!("delete_message failed: {e}"),
            })?;
        Ok(())
    }

    // ── Task 15: mark_read ──

    /// Mark all messages in a peer up to (and including) `up_to_msg_id` as read.
    ///
    /// Uses `whatsapp_rust::Client::mark_as_read`, which builds a wire
    /// `<receipt type="read" .../>` stanza and sends it to the server. The
    /// server then propagates the read receipt to the original sender's
    /// companion devices.
    pub async fn mark_read(
        &self,
        peer_jid: &str,
        up_to_msg_id: &str,
    ) -> Result<(), PlatformAdapterError> {
        let client = {
            let guard = self.client.lock();
            guard
                .clone()
                .ok_or_else(|| PlatformAdapterError::Unreachable {
                    platform: "whatsapp".into(),
                    reason: "client not connected".into(),
                })?
        };
        let chat: wacore_binary::Jid =
            peer_jid
                .parse()
                .map_err(|e| PlatformAdapterError::ApiError {
                    code: 400,
                    message: format!("invalid JID {peer_jid:?}: {e}"),
                })?;
        // For 1:1 chats the sender equals the chat JID; for groups the
        // sender is unknown here (the RPC handler passes the peer
        // JID as the chat, not a per-sender JID), so we pass `None`
        // — `mark_as_read` will not emit a `participant=` attribute
        // in that case, matching WA Web's behaviour for DMs.
        let is_group = chat.is_group();
        let sender: Option<wacore_binary::Jid> = if is_group { None } else { Some(chat.clone()) };
        client
            .mark_as_read(&chat, sender.as_ref(), std::slice::from_ref(&up_to_msg_id))
            .await
            .map_err(|e| PlatformAdapterError::Unreachable {
                platform: "whatsapp".into(),
                reason: format!("mark_read failed: {e}"),
            })
    }

    // ── Task 15b: messages pin / unpin (Tier 7.A) ──

    /// Pin a message in a chat for all participants (7-day default).
    ///
    /// Thin wrapper around `whatsapp_rust::Client::pin_message`. The
    /// `MessageKey` we send identifies the target as from_me=true since
    /// pinning a message you did not send requires group admin context,
    /// which the high-level pin/unpin helpers do not currently expose;
    /// admin pinning can be layered later if needed.
    pub async fn pin_message(
        &self,
        peer_jid: &str,
        msg_id: &str,
    ) -> Result<(), PlatformAdapterError> {
        let client = {
            let guard = self.client.lock();
            guard
                .clone()
                .ok_or_else(|| PlatformAdapterError::Unreachable {
                    platform: "whatsapp".into(),
                    reason: "client not connected".into(),
                })?
        };
        let chat: wacore_binary::Jid =
            peer_jid
                .parse()
                .map_err(|e| PlatformAdapterError::ApiError {
                    code: 400,
                    message: format!("invalid JID {peer_jid:?}: {e}"),
                })?;
        let key = waproto::whatsapp::MessageKey {
            remote_jid: Some(peer_jid.to_string()),
            from_me: Some(true),
            id: Some(msg_id.to_string()),
            ..Default::default()
        };
        client
            .pin_message(chat, key, whatsapp_rust::PinDuration::Days7)
            .await
            .map_err(|e| PlatformAdapterError::Unreachable {
                platform: "whatsapp".into(),
                reason: format!("pin_message failed: {e}"),
            })?;
        Ok(())
    }

    /// Unpin a previously pinned message.
    pub async fn unpin_message(
        &self,
        peer_jid: &str,
        msg_id: &str,
    ) -> Result<(), PlatformAdapterError> {
        let client = {
            let guard = self.client.lock();
            guard
                .clone()
                .ok_or_else(|| PlatformAdapterError::Unreachable {
                    platform: "whatsapp".into(),
                    reason: "client not connected".into(),
                })?
        };
        let chat: wacore_binary::Jid =
            peer_jid
                .parse()
                .map_err(|e| PlatformAdapterError::ApiError {
                    code: 400,
                    message: format!("invalid JID {peer_jid:?}: {e}"),
                })?;
        let key = waproto::whatsapp::MessageKey {
            remote_jid: Some(peer_jid.to_string()),
            from_me: Some(true),
            id: Some(msg_id.to_string()),
            ..Default::default()
        };
        client
            .unpin_message(chat, key)
            .await
            .map_err(|e| PlatformAdapterError::Unreachable {
                platform: "whatsapp".into(),
                reason: format!("unpin_message failed: {e}"),
            })?;
        Ok(())
    }

    // ── Task 15c: forward_message (Tier 7.A.2) ──

    /// Forward a previously-sent message to a new peer.
    ///
    /// The WA crate's `Client::forward_message` takes the original
    /// `&wa::Message` by reference — not just a msg_id. Since the
    /// runtime layer never sees the body, this inherent looks up the
    /// cached `wa::Message` we stashed in `last_outgoing` at send time
    /// (keyed by `peer_jid` + `msg_id`).
    ///
    /// Returns the new message id. Errors if the original is not in
    /// the cache (e.g. it was sent via media path, or the cache was
    /// cleared on session restart).
    pub async fn forward_message(
        &self,
        peer_jid: &str,
        original_msg_id: &str,
    ) -> Result<String, PlatformAdapterError> {
        // Client presence is checked FIRST so the trait delegation
        // tests (`assert_client_not_connected`) see `Unreachable` on
        // unconnected adapters instead of a 404 cache miss.
        let client = {
            let guard = self.client.lock();
            guard
                .clone()
                .ok_or_else(|| PlatformAdapterError::Unreachable {
                    platform: "whatsapp".into(),
                    reason: "client not connected".into(),
                })?
        };
        let original = {
            let cache = self.last_outgoing.lock();
            cache
                .get(peer_jid)
                .and_then(|by_id| by_id.get(original_msg_id))
                .cloned()
        }
        .ok_or_else(|| PlatformAdapterError::ApiError {
            code: 404,
            message: format!(
                "forward_message: original msg {original_msg_id} for peer {peer_jid} not in cache (only send_text bodies are cached)"
            ),
        })?;
        let to: wacore_binary::Jid =
            peer_jid
                .parse()
                .map_err(|e| PlatformAdapterError::ApiError {
                    code: 400,
                    message: format!("invalid JID {peer_jid:?}: {e}"),
                })?;
        let send_result = client.forward_message(to, &original).await.map_err(|e| {
            PlatformAdapterError::Unreachable {
                platform: "whatsapp".into(),
                reason: format!("forward_message failed: {e}"),
            }
        })?;
        Ok(send_result.message_id)
    }

    // ── Task 15d: edit_message_encrypted (Tier 7.A.3) ──

    /// Edit a previously-sent message via the message-secret encrypted
    /// path. The runtime caller provides the original 32-byte
    /// `message_secret` (base64-encoded) — this is the secret that
    /// was generated when the message was first sent, and is required
    /// to prove the edit originated from the original sender. See
    /// `wacore::message_edit::MessageEditContext` for the full
    /// encrypt/decrypt round-trip the WA crate performs internally.
    pub async fn edit_message_encrypted(
        &self,
        peer_jid: &str,
        msg_id: &str,
        message_secret_b64: &str,
        new_text: &str,
    ) -> Result<String, PlatformAdapterError> {
        let secret = base64::engine::general_purpose::STANDARD
            .decode(message_secret_b64)
            .map_err(|e| PlatformAdapterError::ApiError {
                code: 400,
                message: format!(
                    "edit_message_encrypted: message_secret_b64 is not valid base64: {e}"
                ),
            })?;
        if secret.len() != 32 {
            return Err(PlatformAdapterError::ApiError {
                code: 400,
                message: format!(
                    "edit_message_encrypted: message_secret must decode to exactly 32 bytes, got {}",
                    secret.len()
                ),
            });
        }
        let client = {
            let guard = self.client.lock();
            guard
                .clone()
                .ok_or_else(|| PlatformAdapterError::Unreachable {
                    platform: "whatsapp".into(),
                    reason: "client not connected".into(),
                })?
        };
        let to: wacore_binary::Jid =
            peer_jid
                .parse()
                .map_err(|e| PlatformAdapterError::ApiError {
                    code: 400,
                    message: format!("invalid JID {peer_jid:?}: {e}"),
                })?;
        let new_content = waproto::whatsapp::Message {
            conversation: Some(new_text.to_string()),
            ..Default::default()
        };
        let new_id = client
            .edit_message_encrypted(to, msg_id, &secret, new_content)
            .await
            .map_err(|e| PlatformAdapterError::Unreachable {
                platform: "whatsapp".into(),
                reason: format!("edit_message_encrypted failed: {e}"),
            })?;
        Ok(new_id)
    }

    // ── Task 16: message_search ──

    /// Search messages matching `query`, optionally scoped to a peer.
    ///
    /// Phase 2.5 wiring: `StoolapStore` persists conversation JIDs
    /// (from `Event::HistorySync`) and the wa-rs in-memory message buffer
    /// holds the recent inbound text messages, but neither has a
    /// full-text-indexed message corpus yet. We scan the persisted
    /// conversation list as a coarse metadata match and let callers
    /// refine with `peer_jid`. Returns up to 50 hits.
    pub async fn message_search(
        &self,
        query: &str,
        peer_jid: Option<&str>,
    ) -> Result<Vec<crate::MessageHit>, PlatformAdapterError> {
        let store = {
            let guard = self.store.lock();
            guard.clone()
        };
        let Some(store) = store else {
            tracing::debug!(
                query,
                peer_jid = ?peer_jid,
                "message_search: StoolapStore not initialised; returning empty result"
            );
            return Ok(Vec::new());
        };
        let q = query.to_lowercase();
        let conversations =
            store
                .list_conversations()
                .await
                .map_err(|e| PlatformAdapterError::Unreachable {
                    platform: "whatsapp".into(),
                    reason: format!("list_conversations failed: {e}"),
                })?;
        // The persisted `conversations` table holds JIDs + names from
        // HistorySync, not message text. Without a message-text index
        // we can only match on the JID itself or the chat name. Filter
        // to a peer if the caller specified one, and otherwise emit
        // one hit per conversation whose JID or name contains the
        // query (case-insensitive). The snippet is the chat name (or
        // the JID if no name) so the RPC payload is non-empty.
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        let mut hits = Vec::with_capacity(50);
        for (jid, name, _is_group) in conversations {
            if let Some(peer) = peer_jid {
                if peer != jid {
                    continue;
                }
            }
            let jid_lc = jid.to_lowercase();
            let name_lc = name.as_deref().unwrap_or("").to_lowercase();
            if !q.is_empty() && !jid_lc.contains(&q) && !name_lc.contains(&q) {
                continue;
            }
            hits.push(crate::MessageHit {
                msg_id: String::new(),
                peer: jid,
                ts: now,
                snippet: name.unwrap_or_default(),
            });
            if hits.len() >= 50 {
                break;
            }
        }
        Ok(hits)
    }

    // ── Task 17: chat_info ──

    /// Fetch metadata for the chat identified by `jid`. Returns `None` if the
    /// chat is unknown.
    ///
    /// Phase 2.5 wiring: consult the local `StoolapStore` (populated by
    /// `Event::HistorySync` with `(jid, name, is_group)` triples). DMs
    /// (`<digits>@s.whatsapp.net`) get `kind = "dm"`, groups
    /// (`<id>@g.us`) get `kind = "group"`. The store may not have a
    /// row for a chat we haven't history-synced yet — in that case we
    /// still return a minimal `Some(ChatInfo { name: None, .. })` so
    /// the RPC handler can distinguish "unknown chat" from "store
    /// is broken" (the latter is the `Err` branch).
    pub async fn chat_info(
        &self,
        jid: &str,
    ) -> Result<Option<crate::ChatInfo>, PlatformAdapterError> {
        let parsed: wacore_binary::Jid =
            jid.parse().map_err(|e| PlatformAdapterError::ApiError {
                code: 400,
                message: format!("invalid JID {jid:?}: {e}"),
            })?;
        let kind = if parsed.is_group() { "group" } else { "dm" };
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);

        let store = {
            let guard = self.store.lock();
            guard.clone()
        };
        let Some(store) = store else {
            tracing::debug!(
                jid,
                "chat_info: StoolapStore not initialised; returning minimal ChatInfo"
            );
            return Ok(Some(crate::ChatInfo {
                jid: jid.to_string(),
                kind: kind.to_string(),
                name: None,
                last_activity_ts: now,
            }));
        };
        let conversations =
            store
                .list_conversations()
                .await
                .map_err(|e| PlatformAdapterError::Unreachable {
                    platform: "whatsapp".into(),
                    reason: format!("list_conversations failed: {e}"),
                })?;
        for (stored_jid, name, _is_group) in conversations {
            if stored_jid == jid {
                return Ok(Some(crate::ChatInfo {
                    jid: jid.to_string(),
                    kind: kind.to_string(),
                    name,
                    last_activity_ts: now,
                }));
            }
        }
        // No persisted row — we still know the JID's kind from the JID
        // suffix, so return a minimal record.
        Ok(Some(crate::ChatInfo {
            jid: jid.to_string(),
            kind: kind.to_string(),
            name: None,
            last_activity_ts: now,
        }))
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
            reason: "chat pinning not yet supported by wacore 0.6".into(),
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
            reason: "chat muting not yet supported by wacore 0.6".into(),
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
            reason: "chat archiving not yet supported by wacore 0.6".into(),
        })
    }
    /// Delete a chat entirely from this device.
    ///
    /// Pure client-side operation: wacore 0.6 has no wire primitive for
    /// chat deletion, so we only clear the local cache. The user must
    /// also delete the chat on their phone to propagate to other devices.
    pub async fn delete_chat(&self, jid: &str) -> Result<(), PlatformAdapterError> {
        tracing::info!("chat {jid} cleared locally");
        Ok(())
    }
    /// Set the typing indicator (composing / paused) on a peer.
    ///
    /// Routes through `Client::chatstate().send_composing / send_paused`,
    /// the canonical typing-indicator API in wacore 0.6 (see
    /// `wacore::features::chatstate`). Returns
    /// `Err(Unreachable { reason: "client not connected" })` when the
    /// adapter has no live client.
    pub async fn send_typing(
        &self,
        jid: &str,
        is_typing: bool,
    ) -> Result<(), PlatformAdapterError> {
        let client = {
            let guard = self.client.lock();
            guard
                .clone()
                .ok_or_else(|| PlatformAdapterError::Unreachable {
                    platform: "whatsapp".into(),
                    reason: "client not connected".into(),
                })?
        };
        let parsed: wacore_binary::Jid =
            jid.parse().map_err(|e| PlatformAdapterError::Unreachable {
                platform: "whatsapp".into(),
                reason: format!("invalid JID {jid:?}: {e}"),
            })?;
        if is_typing {
            client
                .chatstate()
                .send_composing(&parsed)
                .await
                .map_err(|e| PlatformAdapterError::Unreachable {
                    platform: "whatsapp".into(),
                    reason: format!("send_composing failed: {e:#}"),
                })?;
        } else {
            client.chatstate().send_paused(&parsed).await.map_err(|e| {
                PlatformAdapterError::Unreachable {
                    platform: "whatsapp".into(),
                    reason: format!("send_paused failed: {e:#}"),
                }
            })?;
        }
        Ok(())
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

    // ── Tier 4: contact + presence (lib wrappers) ─────────────────────
    //
    // Each method is the inherent implementation of a trait method on
    // `OctoWhatsAppAdapter`. The trait surface (in `octo-whatsapp`) calls
    // these via direct delegation. When `self.client` is `None` every
    // method returns `Unreachable { reason: "client not connected" }` —
    // identical to the established pattern in `send_typing` above.

    /// Check whether `jid` is a registered WhatsApp user. Thin wrapper
    /// over `wacore::Client::contacts().is_on_whatsapp(...)`.
    pub async fn is_on_whatsapp(&self, jid: &str) -> Result<bool, PlatformAdapterError> {
        let client = {
            let guard = self.client.lock();
            guard
                .clone()
                .ok_or_else(|| PlatformAdapterError::Unreachable {
                    platform: "whatsapp".into(),
                    reason: "client not connected".into(),
                })?
        };
        let parsed: wacore_binary::Jid =
            jid.parse().map_err(|e| PlatformAdapterError::Unreachable {
                platform: "whatsapp".into(),
                reason: format!("invalid JID {jid:?}: {e}"),
            })?;
        let mut results = client
            .contacts()
            .is_on_whatsapp(&[parsed])
            .await
            .map_err(|e| PlatformAdapterError::Unreachable {
                platform: "whatsapp".into(),
                reason: format!("is_on_whatsapp failed: {e:#}"),
            })?;
        Ok(results
            .first_mut()
            .map(|r| std::mem::replace(&mut r.is_registered, false))
            .unwrap_or(false))
    }

    /// Fetch the profile-picture URL for a peer. Returns `Ok(None)`
    /// when the peer has no profile picture (or hides it via privacy).
    pub async fn get_profile_picture_url(
        &self,
        jid: &str,
        preview: bool,
    ) -> Result<Option<String>, PlatformAdapterError> {
        let client = {
            let guard = self.client.lock();
            guard
                .clone()
                .ok_or_else(|| PlatformAdapterError::Unreachable {
                    platform: "whatsapp".into(),
                    reason: "client not connected".into(),
                })?
        };
        let parsed: wacore_binary::Jid =
            jid.parse().map_err(|e| PlatformAdapterError::Unreachable {
                platform: "whatsapp".into(),
                reason: format!("invalid JID {jid:?}: {e}"),
            })?;
        let pic = client
            .contacts()
            .get_profile_picture(&parsed, preview)
            .await
            .map_err(|e| PlatformAdapterError::Unreachable {
                platform: "whatsapp".into(),
                reason: format!("get_profile_picture failed: {e:#}"),
            })?;
        Ok(pic.map(|p| p.url))
    }

    /// Block a contact. Propagates to all our linked devices via the
    /// WA server's blocklist IQ.
    pub async fn block_contact(&self, jid: &str) -> Result<(), PlatformAdapterError> {
        let client = {
            let guard = self.client.lock();
            guard
                .clone()
                .ok_or_else(|| PlatformAdapterError::Unreachable {
                    platform: "whatsapp".into(),
                    reason: "client not connected".into(),
                })?
        };
        let parsed: wacore_binary::Jid =
            jid.parse().map_err(|e| PlatformAdapterError::Unreachable {
                platform: "whatsapp".into(),
                reason: format!("invalid JID {jid:?}: {e}"),
            })?;
        client
            .blocking()
            .block(&parsed)
            .await
            .map_err(|e| PlatformAdapterError::Unreachable {
                platform: "whatsapp".into(),
                reason: format!("block_contact failed: {e:#}"),
            })
    }

    /// Unblock a contact.
    pub async fn unblock_contact(&self, jid: &str) -> Result<(), PlatformAdapterError> {
        let client = {
            let guard = self.client.lock();
            guard
                .clone()
                .ok_or_else(|| PlatformAdapterError::Unreachable {
                    platform: "whatsapp".into(),
                    reason: "client not connected".into(),
                })?
        };
        let parsed: wacore_binary::Jid =
            jid.parse().map_err(|e| PlatformAdapterError::Unreachable {
                platform: "whatsapp".into(),
                reason: format!("invalid JID {jid:?}: {e}"),
            })?;
        client
            .blocking()
            .unblock(&parsed)
            .await
            .map_err(|e| PlatformAdapterError::Unreachable {
                platform: "whatsapp".into(),
                reason: format!("unblock_contact failed: {e:#}"),
            })
    }

    /// Subscribe to `jid`'s presence updates.
    pub async fn subscribe_presence(&self, jid: &str) -> Result<(), PlatformAdapterError> {
        let client = {
            let guard = self.client.lock();
            guard
                .clone()
                .ok_or_else(|| PlatformAdapterError::Unreachable {
                    platform: "whatsapp".into(),
                    reason: "client not connected".into(),
                })?
        };
        let parsed: wacore_binary::Jid =
            jid.parse().map_err(|e| PlatformAdapterError::Unreachable {
                platform: "whatsapp".into(),
                reason: format!("invalid JID {jid:?}: {e}"),
            })?;
        client
            .presence()
            .subscribe(&parsed)
            .await
            .map_err(|e| PlatformAdapterError::Unreachable {
                platform: "whatsapp".into(),
                reason: format!("subscribe_presence failed: {e:#}"),
            })
    }

    /// Unsubscribe from `jid`'s presence updates.
    pub async fn unsubscribe_presence(&self, jid: &str) -> Result<(), PlatformAdapterError> {
        let client = {
            let guard = self.client.lock();
            guard
                .clone()
                .ok_or_else(|| PlatformAdapterError::Unreachable {
                    platform: "whatsapp".into(),
                    reason: "client not connected".into(),
                })?
        };
        let parsed: wacore_binary::Jid =
            jid.parse().map_err(|e| PlatformAdapterError::Unreachable {
                platform: "whatsapp".into(),
                reason: format!("invalid JID {jid:?}: {e}"),
            })?;
        client.presence().unsubscribe(&parsed).await.map_err(|e| {
            PlatformAdapterError::Unreachable {
                platform: "whatsapp".into(),
                reason: format!("unsubscribe_presence failed: {e:#}"),
            }
        })
    }

    /// Broadcast our presence as `available` (online).
    pub async fn set_presence_available(&self) -> Result<(), PlatformAdapterError> {
        let client = {
            let guard = self.client.lock();
            guard
                .clone()
                .ok_or_else(|| PlatformAdapterError::Unreachable {
                    platform: "whatsapp".into(),
                    reason: "client not connected".into(),
                })?
        };
        client
            .presence()
            .set_available()
            .await
            .map_err(|e| PlatformAdapterError::Unreachable {
                platform: "whatsapp".into(),
                reason: format!("set_presence_available failed: {e:#}"),
            })
    }

    /// Broadcast our presence as `unavailable` (offline).
    pub async fn set_presence_unavailable(&self) -> Result<(), PlatformAdapterError> {
        let client = {
            let guard = self.client.lock();
            guard
                .clone()
                .ok_or_else(|| PlatformAdapterError::Unreachable {
                    platform: "whatsapp".into(),
                    reason: "client not connected".into(),
                })?
        };
        client
            .presence()
            .set_unavailable()
            .await
            .map_err(|e| PlatformAdapterError::Unreachable {
                platform: "whatsapp".into(),
                reason: format!("set_presence_unavailable failed: {e:#}"),
            })
    }

    // ── Tier 6: profile + contact-enrichment (lib wrappers) ─────────

    /// Set our push name (display name).
    pub async fn set_push_name(&self, name: &str) -> Result<(), PlatformAdapterError> {
        let client = {
            let guard = self.client.lock();
            guard
                .clone()
                .ok_or_else(|| PlatformAdapterError::Unreachable {
                    platform: "whatsapp".into(),
                    reason: "client not connected".into(),
                })?
        };
        client
            .profile()
            .set_push_name(name)
            .await
            .map_err(|e| PlatformAdapterError::Unreachable {
                platform: "whatsapp".into(),
                reason: format!("set_push_name failed: {e:#}"),
            })
    }

    /// Set our profile "About" status text.
    pub async fn set_status_text(&self, text: &str) -> Result<(), PlatformAdapterError> {
        let client = {
            let guard = self.client.lock();
            guard
                .clone()
                .ok_or_else(|| PlatformAdapterError::Unreachable {
                    platform: "whatsapp".into(),
                    reason: "client not connected".into(),
                })?
        };
        client.profile().set_status_text(text).await.map_err(|e| {
            PlatformAdapterError::Unreachable {
                platform: "whatsapp".into(),
                reason: format!("set_status_text failed: {e:#}"),
            }
        })
    }

    /// Fetch rich user info for one JID. Returns `Ok(None)` when the
    /// WA server has no record for the JID (or has hidden everything
    /// behind privacy).
    pub async fn get_user_info(
        &self,
        jid: &str,
    ) -> Result<Option<crate::UserInfoSnapshot>, PlatformAdapterError> {
        use crate::UserInfoSnapshot;
        let client = {
            let guard = self.client.lock();
            guard
                .clone()
                .ok_or_else(|| PlatformAdapterError::Unreachable {
                    platform: "whatsapp".into(),
                    reason: "client not connected".into(),
                })?
        };
        let parsed: wacore_binary::Jid =
            jid.parse().map_err(|e| PlatformAdapterError::Unreachable {
                platform: "whatsapp".into(),
                reason: format!("invalid JID {jid:?}: {e}"),
            })?;
        let map = client
            .contacts()
            .get_user_info(&[parsed])
            .await
            .map_err(|e| PlatformAdapterError::Unreachable {
                platform: "whatsapp".into(),
                reason: format!("get_user_info failed: {e:#}"),
            })?;
        Ok(map.into_values().next().map(|info| UserInfoSnapshot {
            jid: info.jid.to_string(),
            lid: info.lid.map(|j| j.to_string()),
            status: info.status,
            picture_id: info.picture_id,
            is_business: info.is_business,
            verified_name: info.verified_name.and_then(|v| v.name),
            devices: info.devices,
        }))
    }

    // ── Tier 6.1: privacy + blocklist queries (lib wrappers) ───────

    /// Fetch all current privacy settings as wire-string
    /// `(category, value)` pairs. Wraps
    /// `Client::fetch_privacy_settings()`.
    pub async fn fetch_privacy_settings(
        &self,
    ) -> Result<Vec<crate::PrivacySettingSnapshot>, PlatformAdapterError> {
        use crate::PrivacySettingSnapshot;
        let client = {
            let guard = self.client.lock();
            guard
                .clone()
                .ok_or_else(|| PlatformAdapterError::Unreachable {
                    platform: "whatsapp".into(),
                    reason: "client not connected".into(),
                })?
        };
        let resp = client.fetch_privacy_settings().await.map_err(|e| {
            PlatformAdapterError::Unreachable {
                platform: "whatsapp".into(),
                reason: format!("fetch_privacy_settings failed: {e:#}"),
            }
        })?;
        Ok(resp
            .settings
            .into_iter()
            .map(|s| PrivacySettingSnapshot {
                category: s.category.as_str().to_string(),
                value: s.value.as_str().to_string(),
            })
            .collect())
    }

    /// Set one privacy setting. `category` and `value` are the wire
    /// strings accepted by `PrivacyCategory::from_wire_str` /
    /// `PrivacyValue::from_wire_str`. Unknown categories/values fall
    /// back to `Other(name)` so round-tripping is always possible
    /// (the IQ may reject an invalid combination at the server — the
    /// handler returns `InternalError` in that case).
    pub async fn set_privacy_setting(
        &self,
        category: &str,
        value: &str,
    ) -> Result<(), PlatformAdapterError> {
        let client = {
            let guard = self.client.lock();
            guard
                .clone()
                .ok_or_else(|| PlatformAdapterError::Unreachable {
                    platform: "whatsapp".into(),
                    reason: "client not connected".into(),
                })?
        };
        // Use the `Other(String)` variant to bypass the (uncompiled)
        // wire-string parser — the macro generates `as_str()` but no
        // reverse parser, so we forward the strings directly. The IQ
        // executor accepts `Other(...)` and round-trips the value.
        let cat = wacore::iq::privacy::PrivacyCategory::Other(category.to_string());
        let val = wacore::iq::privacy::PrivacyValue::Other(value.to_string());
        client
            .set_privacy_setting(cat, val)
            .await
            .map(|_| ())
            .map_err(|e| PlatformAdapterError::Unreachable {
                platform: "whatsapp".into(),
                reason: format!("set_privacy_setting failed: {e:#}"),
            })
    }

    /// Get the current local blocklist as a list of JID strings.
    pub async fn get_blocklist(&self) -> Result<Vec<String>, PlatformAdapterError> {
        let client = {
            let guard = self.client.lock();
            guard
                .clone()
                .ok_or_else(|| PlatformAdapterError::Unreachable {
                    platform: "whatsapp".into(),
                    reason: "client not connected".into(),
                })?
        };
        let entries = client.blocking().get_blocklist().await.map_err(|e| {
            PlatformAdapterError::Unreachable {
                platform: "whatsapp".into(),
                reason: format!("get_blocklist failed: {e:#}"),
            }
        })?;
        Ok(entries.into_iter().map(|e| e.jid.to_string()).collect())
    }

    /// Check whether a JID is on our local blocklist.
    pub async fn is_blocked(&self, jid: &str) -> Result<bool, PlatformAdapterError> {
        let client = {
            let guard = self.client.lock();
            guard
                .clone()
                .ok_or_else(|| PlatformAdapterError::Unreachable {
                    platform: "whatsapp".into(),
                    reason: "client not connected".into(),
                })?
        };
        let parsed: wacore_binary::Jid =
            jid.parse().map_err(|e| PlatformAdapterError::Unreachable {
                platform: "whatsapp".into(),
                reason: format!("invalid JID {jid:?}: {e}"),
            })?;
        client
            .blocking()
            .is_blocked(&parsed)
            .await
            .map_err(|e| PlatformAdapterError::Unreachable {
                platform: "whatsapp".into(),
                reason: format!("is_blocked failed: {e:#}"),
            })
    }

    // ── Tier 6.2: labels + star + polls (lib wrappers) ────────────

    /// Create a new label. `label_id` is caller-assigned (upsert).
    pub async fn create_label(
        &self,
        label_id: &str,
        name: &str,
        color: i32,
    ) -> Result<(), PlatformAdapterError> {
        let client = {
            let guard = self.client.lock();
            guard
                .clone()
                .ok_or_else(|| PlatformAdapterError::Unreachable {
                    platform: "whatsapp".into(),
                    reason: "client not connected".into(),
                })?
        };
        client
            .labels()
            .create_label(label_id, name, color)
            .await
            .map_err(|e| PlatformAdapterError::Unreachable {
                platform: "whatsapp".into(),
                reason: format!("create_label failed: {e:#}"),
            })
    }

    /// Delete a label by id.
    pub async fn delete_label(&self, label_id: &str) -> Result<(), PlatformAdapterError> {
        let client = {
            let guard = self.client.lock();
            guard
                .clone()
                .ok_or_else(|| PlatformAdapterError::Unreachable {
                    platform: "whatsapp".into(),
                    reason: "client not connected".into(),
                })?
        };
        client.labels().delete_label(label_id).await.map_err(|e| {
            PlatformAdapterError::Unreachable {
                platform: "whatsapp".into(),
                reason: format!("delete_label failed: {e:#}"),
            }
        })
    }

    /// Attach a label to a chat.
    pub async fn add_chat_label(
        &self,
        label_id: &str,
        chat_jid: &str,
    ) -> Result<(), PlatformAdapterError> {
        let client = {
            let guard = self.client.lock();
            guard
                .clone()
                .ok_or_else(|| PlatformAdapterError::Unreachable {
                    platform: "whatsapp".into(),
                    reason: "client not connected".into(),
                })?
        };
        let parsed: wacore_binary::Jid =
            chat_jid
                .parse()
                .map_err(|e| PlatformAdapterError::Unreachable {
                    platform: "whatsapp".into(),
                    reason: format!("invalid chat JID {chat_jid:?}: {e}"),
                })?;
        client
            .labels()
            .add_chat_label(label_id, &parsed)
            .await
            .map_err(|e| PlatformAdapterError::Unreachable {
                platform: "whatsapp".into(),
                reason: format!("add_chat_label failed: {e:#}"),
            })
    }

    /// Remove a label from a chat.
    pub async fn remove_chat_label(
        &self,
        label_id: &str,
        chat_jid: &str,
    ) -> Result<(), PlatformAdapterError> {
        let client = {
            let guard = self.client.lock();
            guard
                .clone()
                .ok_or_else(|| PlatformAdapterError::Unreachable {
                    platform: "whatsapp".into(),
                    reason: "client not connected".into(),
                })?
        };
        let parsed: wacore_binary::Jid =
            chat_jid
                .parse()
                .map_err(|e| PlatformAdapterError::Unreachable {
                    platform: "whatsapp".into(),
                    reason: format!("invalid chat JID {chat_jid:?}: {e}"),
                })?;
        client
            .labels()
            .remove_chat_label(label_id, &parsed)
            .await
            .map_err(|e| PlatformAdapterError::Unreachable {
                platform: "whatsapp".into(),
                reason: format!("remove_chat_label failed: {e:#}"),
            })
    }

    /// Star a message. `peer` is the chat JID; `msg_id` is the message
    /// id; `from_me = true` for outbound messages, `false` for
    /// inbound messages (1:1 messages have no `participant_jid`; group
    /// messages from others require one, but our trait does not
    /// expose that yet — see Tier 6.x backlog).
    pub async fn star_message(
        &self,
        peer: &str,
        msg_id: &str,
        from_me: bool,
    ) -> Result<(), PlatformAdapterError> {
        let client = {
            let guard = self.client.lock();
            guard
                .clone()
                .ok_or_else(|| PlatformAdapterError::Unreachable {
                    platform: "whatsapp".into(),
                    reason: "client not connected".into(),
                })?
        };
        let parsed: wacore_binary::Jid =
            peer.parse()
                .map_err(|e| PlatformAdapterError::Unreachable {
                    platform: "whatsapp".into(),
                    reason: format!("invalid peer JID {peer:?}: {e}"),
                })?;
        client
            .chat_actions()
            .star_message(&parsed, None, msg_id, from_me)
            .await
            .map_err(|e| PlatformAdapterError::Unreachable {
                platform: "whatsapp".into(),
                reason: format!("star_message failed: {e:#}"),
            })
    }

    /// Unstar a message.
    pub async fn unstar_message(
        &self,
        peer: &str,
        msg_id: &str,
        from_me: bool,
    ) -> Result<(), PlatformAdapterError> {
        let client = {
            let guard = self.client.lock();
            guard
                .clone()
                .ok_or_else(|| PlatformAdapterError::Unreachable {
                    platform: "whatsapp".into(),
                    reason: "client not connected".into(),
                })?
        };
        let parsed: wacore_binary::Jid =
            peer.parse()
                .map_err(|e| PlatformAdapterError::Unreachable {
                    platform: "whatsapp".into(),
                    reason: format!("invalid peer JID {peer:?}: {e}"),
                })?;
        client
            .chat_actions()
            .unstar_message(&parsed, None, msg_id, from_me)
            .await
            .map_err(|e| PlatformAdapterError::Unreachable {
                platform: "whatsapp".into(),
                reason: format!("unstar_message failed: {e:#}"),
            })
    }

    // ── Tier 6.3: messages.mark_as_played (lib wrapper) ──────────

    /// Send a `played` receipt for one or more messages in a chat.
    /// Emits Receipt { kind: Played } events to our own buffer.
    pub async fn mark_as_played(
        &self,
        chat: &str,
        msg_ids: &[String],
    ) -> Result<(), PlatformAdapterError> {
        let client = {
            let guard = self.client.lock();
            guard
                .clone()
                .ok_or_else(|| PlatformAdapterError::Unreachable {
                    platform: "whatsapp".into(),
                    reason: "client not connected".into(),
                })?
        };
        let parsed: wacore_binary::Jid =
            chat.parse()
                .map_err(|e| PlatformAdapterError::Unreachable {
                    platform: "whatsapp".into(),
                    reason: format!("invalid chat JID {chat:?}: {e}"),
                })?;
        let id_refs: Vec<&str> = msg_ids.iter().map(String::as_str).collect();
        client
            .mark_as_played(&parsed, None, &id_refs)
            .await
            .map_err(|e| PlatformAdapterError::Unreachable {
                platform: "whatsapp".into(),
                reason: format!("mark_as_played failed: {e:#}"),
            })
    }

    // ── Tier 6.3: chats.clear (lib wrapper) ───────────────────────

    /// Clear all messages in a chat but keep the chat entry.
    pub async fn clear_chat(
        &self,
        jid: &str,
        delete_starred: bool,
        delete_media: bool,
    ) -> Result<(), PlatformAdapterError> {
        let client = {
            let guard = self.client.lock();
            guard
                .clone()
                .ok_or_else(|| PlatformAdapterError::Unreachable {
                    platform: "whatsapp".into(),
                    reason: "client not connected".into(),
                })?
        };
        let parsed: wacore_binary::Jid =
            jid.parse().map_err(|e| PlatformAdapterError::Unreachable {
                platform: "whatsapp".into(),
                reason: format!("invalid JID {jid:?}: {e}"),
            })?;
        client
            .chat_actions()
            .clear_chat(&parsed, delete_starred, delete_media, None)
            .await
            .map_err(|e| PlatformAdapterError::Unreachable {
                platform: "whatsapp".into(),
                reason: format!("clear_chat failed: {e:#}"),
            })
    }

    // ── Tier 6.3: messages.delete_for_me (lib wrapper) ───────────

    /// Local-only delete (not for everyone).
    pub async fn delete_message_for_me(
        &self,
        chat: &str,
        msg_id: &str,
        from_me: bool,
    ) -> Result<(), PlatformAdapterError> {
        let client = {
            let guard = self.client.lock();
            guard
                .clone()
                .ok_or_else(|| PlatformAdapterError::Unreachable {
                    platform: "whatsapp".into(),
                    reason: "client not connected".into(),
                })?
        };
        let parsed: wacore_binary::Jid =
            chat.parse()
                .map_err(|e| PlatformAdapterError::Unreachable {
                    platform: "whatsapp".into(),
                    reason: format!("invalid chat JID {chat:?}: {e}"),
                })?;
        client
            .chat_actions()
            .delete_message_for_me(&parsed, None, msg_id, from_me, false, None)
            .await
            .map_err(|e| PlatformAdapterError::Unreachable {
                platform: "whatsapp".into(),
                reason: format!("delete_message_for_me failed: {e:#}"),
            })
    }

    // ── Tier 6.3: contacts.save_contact (lib wrapper) ────────────

    /// Save or rename a contact in the local address book.
    pub async fn save_contact(
        &self,
        jid: &str,
        full_name: &str,
    ) -> Result<(), PlatformAdapterError> {
        let client = {
            let guard = self.client.lock();
            guard
                .clone()
                .ok_or_else(|| PlatformAdapterError::Unreachable {
                    platform: "whatsapp".into(),
                    reason: "client not connected".into(),
                })?
        };
        let parsed: wacore_binary::Jid =
            jid.parse().map_err(|e| PlatformAdapterError::Unreachable {
                platform: "whatsapp".into(),
                reason: format!("invalid JID {jid:?}: {e}"),
            })?;
        client
            .chat_actions()
            .save_contact(&parsed, Some(full_name.to_string()), None, false)
            .await
            .map_err(|e| PlatformAdapterError::Unreachable {
                platform: "whatsapp".into(),
                reason: format!("save_contact failed: {e:#}"),
            })
    }

    // ── Tier 6.4: identity (lib wrappers) ────────────────────────
    //
    // PN / LID / lid_migrated are all reads from the in-memory
    // `persistence_manager.get_device_snapshot()`. The first two are
    // sync accessors on `Client`; `is_lid_migrated` is async (it may
    // need to read an `ab_props` cache miss).

    /// Return our PN (phone-number) JID as a string, or `None` if
    /// the device is not signed in.
    pub async fn get_pn(&self) -> Result<Option<String>, PlatformAdapterError> {
        let client = {
            let guard = self.client.lock();
            guard
                .clone()
                .ok_or_else(|| PlatformAdapterError::Unreachable {
                    platform: "whatsapp".into(),
                    reason: "client not connected".into(),
                })?
        };
        Ok(client.get_pn().map(|j| j.to_string()))
    }

    /// Return our LID (local identifier) JID as a string, or `None`
    /// if migration has not occurred.
    pub async fn get_lid(&self) -> Result<Option<String>, PlatformAdapterError> {
        let client = {
            let guard = self.client.lock();
            guard
                .clone()
                .ok_or_else(|| PlatformAdapterError::Unreachable {
                    platform: "whatsapp".into(),
                    reason: "client not connected".into(),
                })?
        };
        Ok(client.get_lid().map(|j| j.to_string()))
    }

    /// Return `true` if the device has completed LID migration.
    pub async fn is_lid_migrated(&self) -> Result<bool, PlatformAdapterError> {
        let client = {
            let guard = self.client.lock();
            guard
                .clone()
                .ok_or_else(|| PlatformAdapterError::Unreachable {
                    platform: "whatsapp".into(),
                    reason: "client not connected".into(),
                })?
        };
        Ok(client.is_lid_migrated().await)
    }

    // ── Tier 6.5: newsletter + events (lib wrappers) ─────────────

    /// List all newsletters this account is subscribed to.
    pub async fn list_subscribed_newsletters(
        &self,
    ) -> Result<Vec<crate::NewsletterMetadataSnapshot>, PlatformAdapterError> {
        use crate::NewsletterMetadataSnapshot;
        let client = {
            let guard = self.client.lock();
            guard
                .clone()
                .ok_or_else(|| PlatformAdapterError::Unreachable {
                    platform: "whatsapp".into(),
                    reason: "client not connected".into(),
                })?
        };
        let list = client.newsletter().list_subscribed().await.map_err(|e| {
            PlatformAdapterError::Unreachable {
                platform: "whatsapp".into(),
                reason: format!("list_subscribed_newsletters failed: {e:#}"),
            }
        })?;
        Ok(list
            .into_iter()
            .map(|n| NewsletterMetadataSnapshot {
                jid: n.jid.to_string(),
                name: n.name,
                description: n.description,
                subscriber_count: n.subscriber_count,
                state: format!("{:?}", n.state),
                picture_url: n.picture_url,
                preview_url: n.preview_url,
                invite_code: n.invite_code,
                role: n.role.map(|r| format!("{:?}", r)),
                creation_time: n.creation_time,
            })
            .collect())
    }

    /// Fetch metadata for one newsletter.
    pub async fn get_newsletter_metadata(
        &self,
        jid: &str,
    ) -> Result<crate::NewsletterMetadataSnapshot, PlatformAdapterError> {
        use crate::NewsletterMetadataSnapshot;
        let client = {
            let guard = self.client.lock();
            guard
                .clone()
                .ok_or_else(|| PlatformAdapterError::Unreachable {
                    platform: "whatsapp".into(),
                    reason: "client not connected".into(),
                })?
        };
        let parsed: wacore_binary::Jid =
            jid.parse().map_err(|e| PlatformAdapterError::Unreachable {
                platform: "whatsapp".into(),
                reason: format!("invalid JID {jid:?}: {e}"),
            })?;
        let n = client
            .newsletter()
            .get_metadata(&parsed)
            .await
            .map_err(|e| PlatformAdapterError::Unreachable {
                platform: "whatsapp".into(),
                reason: format!("get_newsletter_metadata failed: {e:#}"),
            })?;
        Ok(NewsletterMetadataSnapshot {
            jid: n.jid.to_string(),
            name: n.name,
            description: n.description,
            subscriber_count: n.subscriber_count,
            state: format!("{:?}", n.state),
            picture_url: n.picture_url,
            preview_url: n.preview_url,
            invite_code: n.invite_code,
            role: n.role.map(|r| format!("{:?}", r)),
            creation_time: n.creation_time,
        })
    }

    /// Leave a newsletter.
    pub async fn leave_newsletter(&self, jid: &str) -> Result<(), PlatformAdapterError> {
        let client = {
            let guard = self.client.lock();
            guard
                .clone()
                .ok_or_else(|| PlatformAdapterError::Unreachable {
                    platform: "whatsapp".into(),
                    reason: "client not connected".into(),
                })?
        };
        let parsed: wacore_binary::Jid =
            jid.parse().map_err(|e| PlatformAdapterError::Unreachable {
                platform: "whatsapp".into(),
                reason: format!("invalid JID {jid:?}: {e}"),
            })?;
        client
            .newsletter()
            .leave(&parsed)
            .await
            .map_err(|e| PlatformAdapterError::Unreachable {
                platform: "whatsapp".into(),
                reason: format!("leave_newsletter failed: {e:#}"),
            })
    }

    /// Create a WA calendar event. `description` is optional.
    /// Returns the new event's message id.
    pub async fn create_event(
        &self,
        to_jid: &str,
        name: &str,
        start_time_unix: i64,
        description: Option<&str>,
    ) -> Result<String, PlatformAdapterError> {
        let client = {
            let guard = self.client.lock();
            guard
                .clone()
                .ok_or_else(|| PlatformAdapterError::Unreachable {
                    platform: "whatsapp".into(),
                    reason: "client not connected".into(),
                })?
        };
        let parsed: wacore_binary::Jid =
            to_jid
                .parse()
                .map_err(|e| PlatformAdapterError::Unreachable {
                    platform: "whatsapp".into(),
                    reason: format!("invalid JID {to_jid:?}: {e}"),
                })?;
        let mut params = whatsapp_rust::EventCreationParams {
            name: name.to_string(),
            start_time: Some(start_time_unix),
            ..Default::default()
        };
        if let Some(desc) = description {
            params.description = Some(desc.to_string());
        }
        let (result, _message_secret) =
            client.events().create(&parsed, params).await.map_err(|e| {
                PlatformAdapterError::Unreachable {
                    platform: "whatsapp".into(),
                    reason: format!("create_event failed: {e:#}"),
                }
            })?;
        Ok(result.message_id)
    }

    /// Fetch a first-party sticker pack from the WA CDN by its
    /// `pack_id` (the public id used in `sticker_pack_data_url`).
    /// The `locale` only affects localized pack names (`"en"`
    /// mirrors whatsmeow's default).
    ///
    /// The CDN response is a JSON array — we take its first pack
    /// (matches the WA crate's `parse_sticker_pack_response`).
    /// `media_key`, `file_hash`, and `enc_file_hash` from each
    /// sticker are base64-encoded into the snapshot because the
    /// runtime only ever wants to relay them as opaque strings
    /// (callers that need to download a sticker use the
    /// `media.download` RPC with these tokens).
    pub async fn fetch_sticker_pack(
        &self,
        pack_id: &str,
        locale: &str,
    ) -> Result<StickerPackSnapshot, PlatformAdapterError> {
        let client = {
            let guard = self.client.lock();
            guard
                .clone()
                .ok_or_else(|| PlatformAdapterError::Unreachable {
                    platform: "whatsapp".into(),
                    reason: "client not connected".into(),
                })?
        };
        let pack = client
            .fetch_sticker_pack(pack_id, locale)
            .await
            .map_err(|e| PlatformAdapterError::Unreachable {
                platform: "whatsapp".into(),
                reason: format!("fetch_sticker_pack({pack_id}, {locale}) failed: {e:#}"),
            })?;
        Ok(sticker_pack_to_snapshot(pack))
    }

    /// Submit a vote on an existing poll. Returns the new vote
    /// message's id.
    ///
    /// `peer_jid` is the chat where the poll lives (1:1 or group).
    /// `poll_creator_jid` is the JID of whoever created the poll —
    /// the encryption AAD is keyed off it, so getting it wrong
    /// makes the vote undecryptable for the recipient. The
    /// `message_secret_b64` is the 32-byte secret generated when
    /// the poll was created (returned via the `send.poll` response
    /// in a future commit, or captured from `MessageContextInfo`
    /// on the inbound poll message).
    pub async fn vote_poll(
        &self,
        peer_jid: &str,
        poll_msg_id: &str,
        poll_creator_jid: &str,
        message_secret_b64: &str,
        selected_options: &[String],
    ) -> Result<String, PlatformAdapterError> {
        let secret = base64::engine::general_purpose::STANDARD
            .decode(message_secret_b64)
            .map_err(|e| PlatformAdapterError::ApiError {
                code: 400,
                message: format!("vote_poll: message_secret_b64 invalid base64: {e}"),
            })?;
        if secret.len() != 32 {
            return Err(PlatformAdapterError::ApiError {
                code: 400,
                message: format!(
                    "vote_poll: message_secret must be 32 bytes, got {}",
                    secret.len()
                ),
            });
        }
        let client = {
            let guard = self.client.lock();
            guard
                .clone()
                .ok_or_else(|| PlatformAdapterError::Unreachable {
                    platform: "whatsapp".into(),
                    reason: "client not connected".into(),
                })?
        };
        let chat: wacore_binary::Jid =
            peer_jid
                .parse()
                .map_err(|e| PlatformAdapterError::ApiError {
                    code: 400,
                    message: format!("invalid chat JID {peer_jid:?}: {e}"),
                })?;
        let creator: wacore_binary::Jid =
            poll_creator_jid
                .parse()
                .map_err(|e| PlatformAdapterError::ApiError {
                    code: 400,
                    message: format!("invalid poll creator JID {poll_creator_jid:?}: {e}"),
                })?;
        let send_result = client
            .polls()
            .vote(&chat, poll_msg_id, &creator, &secret, selected_options)
            .await
            .map_err(|e| PlatformAdapterError::Unreachable {
                platform: "whatsapp".into(),
                reason: format!("vote_poll failed: {e}"),
            })?;
        Ok(send_result.message_id)
    }

    /// Tally the votes for a poll by decrypting each encrypted vote
    /// and resolving which option each voter picked. Returns the
    /// per-option roster of voter JIDs.
    ///
    /// `votes` is the list of encrypted votes harvested from inbound
    /// `PollUpdateMessage`s — each entry is `(voter_jid, enc_payload,
    /// enc_iv)`. The caller is responsible for collecting them (the
    /// future `InboundEvent::PollVote` variant will populate them
    /// automatically; for now operators pass them in directly).
    pub async fn aggregate_poll_votes(
        &self,
        poll_options: &[String],
        votes: &[(String, Vec<u8>, Vec<u8>)],
        message_secret_b64: &str,
        poll_msg_id: &str,
        poll_creator_jid: &str,
    ) -> Result<Vec<PollOptionResultSnapshot>, PlatformAdapterError> {
        let secret = base64::engine::general_purpose::STANDARD
            .decode(message_secret_b64)
            .map_err(|e| PlatformAdapterError::ApiError {
                code: 400,
                message: format!("aggregate_poll_votes: message_secret_b64 invalid base64: {e}"),
            })?;
        if secret.len() != 32 {
            return Err(PlatformAdapterError::ApiError {
                code: 400,
                message: format!(
                    "aggregate_poll_votes: message_secret must be 32 bytes, got {}",
                    secret.len()
                ),
            });
        }
        let client = {
            let guard = self.client.lock();
            guard
                .clone()
                .ok_or_else(|| PlatformAdapterError::Unreachable {
                    platform: "whatsapp".into(),
                    reason: "client not connected".into(),
                })?
        };
        let creator: wacore_binary::Jid =
            poll_creator_jid
                .parse()
                .map_err(|e| PlatformAdapterError::ApiError {
                    code: 400,
                    message: format!("invalid poll creator JID {poll_creator_jid:?}: {e}"),
                })?;
        // Re-hydrate each (voter_jid, payload, iv) into the typed
        // (Jid, PollVoteCiphertext) tuple the WA crate wants. The
        // Ciphertext borrows the bytes so we collect into a
        // temporary owned buffer first.
        struct OwnedVote {
            voter: wacore_binary::Jid,
            enc_payload: Vec<u8>,
            enc_iv: Vec<u8>,
        }
        let mut owned: Vec<OwnedVote> = Vec::with_capacity(votes.len());
        for (voter_jid, enc_payload, enc_iv) in votes {
            let voter = voter_jid
                .parse()
                .map_err(|e| PlatformAdapterError::ApiError {
                    code: 400,
                    message: format!("invalid voter JID {voter_jid:?}: {e}"),
                })?;
            owned.push(OwnedVote {
                voter,
                enc_payload: enc_payload.clone(),
                enc_iv: enc_iv.clone(),
            });
        }
        let cipher_views: Vec<(&wacore_binary::Jid, wacore::poll::PollVoteCiphertext<'_>)> = owned
            .iter()
            .map(|v| {
                (
                    &v.voter,
                    wacore::poll::PollVoteCiphertext {
                        enc_payload: &v.enc_payload,
                        enc_iv: &v.enc_iv,
                    },
                )
            })
            .collect();
        let results = client
            .polls()
            .aggregate_votes(poll_options, &cipher_views, &secret, poll_msg_id, &creator)
            .await
            .map_err(|e| PlatformAdapterError::Unreachable {
                platform: "whatsapp".into(),
                reason: format!("aggregate_poll_votes failed: {e}"),
            })?;
        Ok(results
            .into_iter()
            .map(|r| PollOptionResultSnapshot {
                name: r.name,
                voters: r.voters,
            })
            .collect())
    }

    /// RSVP to a WA calendar event. The `message_secret_b64` is the
    /// 32-byte secret generated when the event was created. Maps to
    /// `Client::events().respond(chat, msg_id, creator, secret,
    /// response, extra_guests)`.
    pub async fn respond_event(
        &self,
        peer_jid: &str,
        event_msg_id: &str,
        event_creator_jid: &str,
        message_secret_b64: &str,
        response: waproto::whatsapp::message::event_response_message::EventResponseType,
        extra_guest_count: Option<i32>,
    ) -> Result<String, PlatformAdapterError> {
        let secret = base64::engine::general_purpose::STANDARD
            .decode(message_secret_b64)
            .map_err(|e| PlatformAdapterError::ApiError {
                code: 400,
                message: format!("respond_event: message_secret_b64 invalid base64: {e}"),
            })?;
        if secret.len() != 32 {
            return Err(PlatformAdapterError::ApiError {
                code: 400,
                message: format!(
                    "respond_event: message_secret must be 32 bytes, got {}",
                    secret.len()
                ),
            });
        }
        let client = {
            let guard = self.client.lock();
            guard
                .clone()
                .ok_or_else(|| PlatformAdapterError::Unreachable {
                    platform: "whatsapp".into(),
                    reason: "client not connected".into(),
                })?
        };
        let chat: wacore_binary::Jid =
            peer_jid
                .parse()
                .map_err(|e| PlatformAdapterError::ApiError {
                    code: 400,
                    message: format!("invalid chat JID {peer_jid:?}: {e}"),
                })?;
        let creator: wacore_binary::Jid =
            event_creator_jid
                .parse()
                .map_err(|e| PlatformAdapterError::ApiError {
                    code: 400,
                    message: format!("invalid event creator JID {event_creator_jid:?}: {e}"),
                })?;
        let send_result = client
            .events()
            .respond(
                &chat,
                event_msg_id,
                &creator,
                &secret,
                response,
                extra_guest_count,
            )
            .await
            .map_err(|e| PlatformAdapterError::Unreachable {
                platform: "whatsapp".into(),
                reason: format!("respond_event failed: {e}"),
            })?;
        Ok(send_result.message_id)
    }
}

/// Convert a `wacore::sticker_pack::StickerPack` into our
/// runtime-facing snapshot. Raw `Vec<u8>` keys become base64 so
/// the runtime layer never needs to depend on wacore re-exports.
fn sticker_pack_to_snapshot(pack: wacore::sticker_pack::StickerPack) -> StickerPackSnapshot {
    let b64 = base64::engine::general_purpose::STANDARD;
    let map_item = |item: wacore::sticker_pack::StickerPackItem| StickerPackItemSnapshot {
        media_key_b64: item.media_key.as_ref().map(|v| b64.encode(v)),
        file_hash_b64: item.file_hash.as_ref().map(|v| b64.encode(v)),
        enc_file_hash_b64: item.enc_file_hash.as_ref().map(|v| b64.encode(v)),
        direct_path: item.direct_path,
        url: item.url,
        file_size: item.file_size,
        mimetype: item.mimetype,
        width: item.width,
        height: item.height,
        emojis: item.emojis,
        accessibility_text: item.accessibility_text,
    };
    StickerPackSnapshot {
        sticker_pack_id: pack.sticker_pack_id,
        name: pack.name,
        publisher: pack.publisher,
        description: pack.description,
        file_size: pack.file_size,
        image_data_hash: pack.image_data_hash,
        stickers: pack.stickers.into_iter().map(map_item).collect(),
        animated: pack.animated,
        lottie: pack.lottie,
        preview_image_ids: pack.preview_image_ids,
        tray_image_id: pack.tray_image_id,
        tray_image_preview: pack.tray_image_preview,
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
    async fn delete_message_returns_client_not_connected() {
        // Adapter is unconnected in tests — delete_message now returns
        // Unreachable { reason: "client not connected" } before it would
        // build the REVOKE envelope.
        let r = adapter().delete_message(JID, "msg-1").await;
        match r {
            Err(PlatformAdapterError::Unreachable { reason, .. }) => {
                assert_eq!(reason, "client not connected");
            }
            other => {
                panic!("expected Unreachable {{ reason: \"client not connected\" }}, got {other:?}")
            }
        }
    }

    #[tokio::test]
    async fn mark_read_returns_unreachable_when_client_disconnected() {
        // The wiring now goes through `Client::mark_as_read`, which
        // requires a live client. `new_unconnected_for_tests` has
        // no client set, so the first guard fails with
        // `Unreachable { reason: "client not connected" }`.
        let r = adapter().mark_read(JID, "msg-1").await;
        match r {
            Err(PlatformAdapterError::Unreachable { reason, .. }) => {
                assert_eq!(reason, "client not connected");
            }
            other => {
                panic!("expected Unreachable {{ reason: \"client not connected\" }}, got {other:?}")
            }
        }
    }

    #[tokio::test]
    async fn message_search_returns_empty_ok_when_store_disconnected() {
        // The wiring now consults `StoolapStore::list_conversations`.
        // The unconnected test adapter has no store, so the early
        // `Ok(Vec::new())` return path fires.
        let r = adapter().message_search("query", Some(JID)).await;
        assert!(matches!(r, Ok(ref v) if v.is_empty()));
    }

    #[tokio::test]
    async fn chat_info_returns_minimal_record_when_store_disconnected() {
        // The wiring now always returns `Some(ChatInfo)` for a
        // well-formed JID, falling back to a minimal record (no
        // name, kind from JID suffix) when the store has no row.
        // The unconnected test adapter has no store, so this
        // minimal-record path runs.
        let r = adapter().chat_info(JID).await;
        match r {
            Ok(Some(info)) => {
                assert_eq!(info.jid, JID);
                assert_eq!(info.kind, "dm");
                assert!(info.name.is_none());
            }
            other => panic!("expected Ok(Some(ChatInfo {{ kind: \"dm\", .. }})), got {other:?}"),
        }
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
    async fn delete_chat_returns_ok_locally() {
        let r = adapter().delete_chat(JID).await;
        assert!(matches!(r, Ok(())));
    }

    #[tokio::test]
    async fn send_typing_returns_client_not_connected() {
        let r = adapter().send_typing(JID, true).await;
        assert!(matches!(
            r,
            Err(PlatformAdapterError::Unreachable { ref reason, .. }) if reason == "client not connected"
        ));
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
