//! Transport selection for the MTProto Telegram adapter.
//!
//! The adapter can use one of two transports to reach Telegram:
//!
//! - **MTProto** (primary): pure-Rust via the `grammers` family
//!   of crates. Implements the full MTProto protocol over TCP
//!   with AES-IGE + auth_key. This is the default and supports
//!   both bot and user accounts.
//!
//! - **Bot-API HTTP** (fallback, Phase 3 / sub-mission
//!   `0850ab-c-http`): HTTPS + JSON against
//!   `https://api.telegram.org/bot<token>/<method>`. Bot-only
//!   (no user accounts), opt-in. Implemented in
//!   `crate::http_fallback` (gated on the `bot-api` feature).
//!
//! The transport is **per-`Adapter` instance** — there is no
//! global mode flag. A deployment with two `Adapter` instances
//! can run one in `Mtproto` mode and the other in `BotApiHttp`
//! mode if it has, e.g., two bot accounts and one is in a
//! region-blocked network while the other is not.
//!
//! This module is unconditional (no Cargo feature gate) so
//! `MtprotoTelegramConfig` can reference the `Transport` enum
//! from the default build. The `BotApiClient` and method
//! implementations that actually use the `BotApiHttp` variant
//! live in `crate::http_fallback` and are feature-gated.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// Bot selection (per-`Adapter` instance). Default is `Mtproto`.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Transport {
    /// Primary transport: pure-Rust MTProto via grammers
    /// (Phase 1 + Phase 2.5). Supports both bot and user
    /// accounts.
    #[default]
    Mtproto,
    /// Fallback transport: Bot API at `api.telegram.org` over
    /// HTTPS. Bot-only, opt-in. Implemented in
    /// `crate::http_fallback` (requires the `bot-api` feature
    /// to actually use).
    ///
    /// Serde: the canonical wire form is `"http"` (matching
    /// the CLI flag and the research doc); the longer
    /// `"bot-api-http"` form is accepted as an alias for
    /// clarity in config files.
    #[serde(rename = "http", alias = "bot-api-http")]
    BotApiHttp,
}

impl fmt::Display for Transport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Mtproto => f.write_str("mtproto"),
            Self::BotApiHttp => f.write_str("http"),
        }
    }
}

impl FromStr for Transport {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "mtproto" | "tcp" => Ok(Self::Mtproto),
            "http" | "bot-api" | "bot_api" | "botapi" => Ok(Self::BotApiHttp),
            other => Err(format!(
                "unknown transport: '{}' (expected 'mtproto' or 'http')",
                other
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_mtproto() {
        assert_eq!(Transport::default(), Transport::Mtproto);
    }

    #[test]
    fn from_str_accepts_aliases() {
        assert_eq!("mtproto".parse::<Transport>().unwrap(), Transport::Mtproto);
        assert_eq!("MTPROTO".parse::<Transport>().unwrap(), Transport::Mtproto);
        assert_eq!("tcp".parse::<Transport>().unwrap(), Transport::Mtproto);
        assert_eq!("http".parse::<Transport>().unwrap(), Transport::BotApiHttp);
        assert_eq!(
            "bot-api".parse::<Transport>().unwrap(),
            Transport::BotApiHttp
        );
        assert_eq!(
            "bot_api".parse::<Transport>().unwrap(),
            Transport::BotApiHttp
        );
        assert!("unknown".parse::<Transport>().is_err());
    }

    #[test]
    fn display_is_kebab() {
        assert_eq!(Transport::Mtproto.to_string(), "mtproto");
        assert_eq!(Transport::BotApiHttp.to_string(), "http");
    }

    #[test]
    fn serde_round_trip() {
        // The Mtproto variant is the kebab-case default.
        let s = serde_json::to_string(&Transport::Mtproto).unwrap();
        assert_eq!(s, "\"mtproto\"");
        // The BotApiHttp variant has an explicit rename
        // (`http`) so its canonical wire form is short and
        // matches the CLI flag.
        let s = serde_json::to_string(&Transport::BotApiHttp).unwrap();
        assert_eq!(s, "\"http\"");
        // Both `http` and `bot-api-http` are accepted on
        // deserialization.
        let t: Transport = serde_json::from_str("\"http\"").unwrap();
        assert_eq!(t, Transport::BotApiHttp);
        let t: Transport = serde_json::from_str("\"bot-api-http\"").unwrap();
        assert_eq!(t, Transport::BotApiHttp);
    }

    #[test]
    fn serde_rejects_unknown_transport() {
        let r: Result<Transport, _> = serde_json::from_str("\"tcp\"");
        assert!(r.is_err());
    }
}
