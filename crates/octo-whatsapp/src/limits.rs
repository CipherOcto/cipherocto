//! Per-kind payload ceilings and `MediaKind` enum for the outbound
//! matrix. See design §Raw vs DOT Protocol Paths.

pub const MAX_TEXT_BYTES: usize = 65_536;
pub const MAX_IMAGE_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_VIDEO_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_AUDIO_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_VOICE_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_STICKER_BYTES: usize = 1024 * 1024;
pub const MAX_DOC_BYTES: usize = 100 * 1024 * 1024;
pub const MAX_VCARD_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MediaKind {
    Text,
    Image,
    Video,
    Audio,
    Voice,
    Sticker,
    Document,
    Contact,
    Reaction,
    Poll,
    Location,
}
impl MediaKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Image => "image",
            Self::Video => "video",
            Self::Audio => "audio",
            Self::Voice => "voice",
            Self::Sticker => "sticker",
            Self::Document => "document",
            Self::Contact => "contact",
            Self::Reaction => "reaction",
            Self::Poll => "poll",
            Self::Location => "location",
        }
    }
    pub fn max_bytes(self) -> usize {
        match self {
            Self::Text => MAX_TEXT_BYTES,
            Self::Image => MAX_IMAGE_BYTES,
            Self::Video => MAX_VIDEO_BYTES,
            Self::Audio => MAX_AUDIO_BYTES,
            Self::Voice => MAX_VOICE_BYTES,
            Self::Sticker => MAX_STICKER_BYTES,
            Self::Document => MAX_DOC_BYTES,
            Self::Contact => MAX_VCARD_BYTES,
            Self::Reaction => 1024,
            Self::Poll => 4096,
            Self::Location => 1024,
        }
    }
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "text" => Some(Self::Text),
            "image" => Some(Self::Image),
            "video" => Some(Self::Video),
            "audio" => Some(Self::Audio),
            "voice" => Some(Self::Voice),
            "sticker" => Some(Self::Sticker),
            "document" => Some(Self::Document),
            "contact" => Some(Self::Contact),
            "reaction" => Some(Self::Reaction),
            "poll" => Some(Self::Poll),
            "location" => Some(Self::Location),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ceilings_match_whatsapp_web_quotas() {
        assert_eq!(MAX_TEXT_BYTES, 65_536);
        assert_eq!(MAX_IMAGE_BYTES, 16 * 1024 * 1024);
        assert_eq!(MAX_VIDEO_BYTES, 16 * 1024 * 1024);
        assert_eq!(MAX_AUDIO_BYTES, 16 * 1024 * 1024);
        assert_eq!(MAX_VOICE_BYTES, 16 * 1024 * 1024);
        assert_eq!(MAX_STICKER_BYTES, 1024 * 1024);
        assert_eq!(MAX_DOC_BYTES, 100 * 1024 * 1024);
        assert_eq!(MAX_VCARD_BYTES, 1024 * 1024);
    }
    #[test]
    fn media_kind_round_trip() {
        for k in [
            MediaKind::Image,
            MediaKind::Video,
            MediaKind::Audio,
            MediaKind::Voice,
            MediaKind::Sticker,
            MediaKind::Document,
            MediaKind::Contact,
            MediaKind::Reaction,
            MediaKind::Poll,
            MediaKind::Location,
        ] {
            assert_eq!(MediaKind::from_str(k.as_str()).unwrap(), k);
        }
    }
}
