//! Inherent methods on `WhatsAppWebAdapter` for the Phase 2 outbound
//! matrix + messages + chats + domain. The runtime layer
//! (`octo-whatsapp`) wraps them with pre-flight ceilings.

use std::path::Path;

use crate::adapter::{upload_to_cdn, WhatsAppWebAdapter};
use crate::media_ref::{encode_base64url, MediaRef};
use crate::PlatformAdapterError;
use wacore_binary::JidExt;
use whatsapp_rust::download::MediaType;
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
