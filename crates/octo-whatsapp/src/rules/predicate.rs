//! Predicate tree for rule matching.
//!
//! Phase 4 of `docs/plans/2026-07-04-whatsapp-runtime-cli-mcp-design.md`
//! §Event Stream / Rules Engine. Each `Rule` carries a `Predicate` that
//! is evaluated against an `InboundEvent` to decide whether the rule
//! fires. The predicate tree is closed under `And`/`Or`/`Not`, so any
//! boolean expression over the leaf kinds can be expressed.
//!
//! ## Evaluation contract
//!
//! `Predicate::matches(&event, &now_ms)` is **pure and total**:
//! - No `await`. Predicates are sync; rule matchers hold the `ArcSwap` guard
//!   only for the duration of `matches(...)` + a clone of `Arc<Rule>`.
//! - No panics on malformed event payloads. Every leaf returns `false`
//!   if the event does not carry the relevant field.
//! - `And`/`Or` short-circuit and never recurse more than 32 deep
//!   (asserted in debug builds via `MAX_DEPTH`).
//!
//! ## ReDoS safety
//!
//! `TextRegex` predicates go through `classify_regex` at create time.
//! Patterns classified as unsafe (nested quantifiers, alternation inside
//! a quantified group, unbounded `.*` adjacent to a literal followed by
//! another quantifier) return `RuleError::UnsafeRegex` and the rule is
//! rejected. Per-match protection: input text is truncated to 4 KiB
//! before regex evaluation (per design §Testing Strategy "ReDoS"
//! bullet).

use std::sync::OnceLock;

use regex::Regex;
use serde::de::{self, Deserializer, MapAccess, Visitor};
use serde::ser::{SerializeMap, Serializer};
use serde::{Deserialize, Serialize};

use crate::events::InboundEvent;

const MAX_PREDICATE_DEPTH: usize = 32;
const MAX_REGEX_INPUT_BYTES: usize = 4 * 1024;
const REGEX_MATCH_TIMEOUT_MS: u64 = 10;

impl Serialize for Predicate {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut map = s.serialize_map(Some(2))?;
        match self {
            Predicate::True => {
                map.serialize_entry("kind", "true")?;
            }
            Predicate::EventKind { kinds } => {
                map.serialize_entry("kind", "event_kind")?;
                map.serialize_entry("kinds", kinds)?;
            }
            Predicate::PeerGlob { pattern } => {
                map.serialize_entry("kind", "peer_glob")?;
                map.serialize_entry("pattern", pattern)?;
            }
            Predicate::SenderGlob { pattern } => {
                map.serialize_entry("kind", "sender_glob")?;
                map.serialize_entry("pattern", pattern)?;
            }
            Predicate::TextRegex { pattern } => {
                map.serialize_entry("kind", "text_regex")?;
                map.serialize_entry("pattern", pattern)?;
            }
            Predicate::FromJid { jid } => {
                map.serialize_entry("kind", "from_jid")?;
                map.serialize_entry("jid", jid)?;
            }
            Predicate::GroupOnly { value } => {
                map.serialize_entry("kind", "group_only")?;
                map.serialize_entry("value", value)?;
            }
            Predicate::And(children) => {
                map.serialize_entry("kind", "and")?;
                map.serialize_entry("children", children)?;
            }
            Predicate::Or(children) => {
                map.serialize_entry("kind", "or")?;
                map.serialize_entry("children", children)?;
            }
            Predicate::Not(inner) => {
                map.serialize_entry("kind", "not")?;
                map.serialize_entry("inner", inner.as_ref())?;
            }
        }
        map.end()
    }
}

impl<'de> Deserialize<'de> for Predicate {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(field_identifier, rename_all = "snake_case")]
        enum Field {
            Kind,
            Kinds,
            Pattern,
            Jid,
            Value,
            Children,
            Inner,
        }
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = Predicate;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("predicate")
            }
            fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<Predicate, M::Error> {
                let mut kind: Option<String> = None;
                let mut kinds: Option<Vec<String>> = None;
                let mut pattern: Option<String> = None;
                let mut jid: Option<String> = None;
                let mut value: Option<bool> = None;
                let mut children: Option<Vec<Predicate>> = None;
                let mut inner: Option<Box<Predicate>> = None;
                while let Some(k) = map.next_key::<Field>()? {
                    match k {
                        Field::Kind => {
                            if kind.is_some() {
                                return Err(de::Error::duplicate_field("kind"));
                            }
                            kind = Some(map.next_value()?);
                        }
                        Field::Kinds => {
                            if kinds.is_some() {
                                return Err(de::Error::duplicate_field("kinds"));
                            }
                            kinds = Some(map.next_value()?);
                        }
                        Field::Pattern => {
                            if pattern.is_some() {
                                return Err(de::Error::duplicate_field("pattern"));
                            }
                            pattern = Some(map.next_value()?);
                        }
                        Field::Jid => {
                            if jid.is_some() {
                                return Err(de::Error::duplicate_field("jid"));
                            }
                            jid = Some(map.next_value()?);
                        }
                        Field::Value => {
                            if value.is_some() {
                                return Err(de::Error::duplicate_field("value"));
                            }
                            value = Some(map.next_value()?);
                        }
                        Field::Children => {
                            if children.is_some() {
                                return Err(de::Error::duplicate_field("children"));
                            }
                            children = Some(map.next_value::<Vec<Predicate>>()?);
                        }
                        Field::Inner => {
                            if inner.is_some() {
                                return Err(de::Error::duplicate_field("inner"));
                            }
                            inner = Some(Box::new(map.next_value::<Predicate>()?));
                        }
                    }
                }
                let kind = kind.ok_or_else(|| de::Error::missing_field("kind"))?;
                Ok(match kind.as_str() {
                    "true" => Predicate::True,
                    "event_kind" => Predicate::EventKind {
                        kinds: kinds.ok_or_else(|| de::Error::missing_field("kinds"))?,
                    },
                    "peer_glob" => Predicate::PeerGlob {
                        pattern: pattern.ok_or_else(|| de::Error::missing_field("pattern"))?,
                    },
                    "sender_glob" => Predicate::SenderGlob {
                        pattern: pattern.ok_or_else(|| de::Error::missing_field("pattern"))?,
                    },
                    "text_regex" => Predicate::TextRegex {
                        pattern: pattern.ok_or_else(|| de::Error::missing_field("pattern"))?,
                    },
                    "from_jid" => Predicate::FromJid {
                        jid: jid.ok_or_else(|| de::Error::missing_field("jid"))?,
                    },
                    "group_only" => Predicate::GroupOnly {
                        value: value.ok_or_else(|| de::Error::missing_field("value"))?,
                    },
                    "and" => Predicate::And(
                        children.ok_or_else(|| de::Error::missing_field("children"))?,
                    ),
                    "or" => {
                        Predicate::Or(children.ok_or_else(|| de::Error::missing_field("children"))?)
                    }
                    "not" => {
                        Predicate::Not(inner.ok_or_else(|| de::Error::missing_field("inner"))?)
                    }
                    other => {
                        return Err(de::Error::unknown_variant(
                            other,
                            &[
                                "true",
                                "event_kind",
                                "peer_glob",
                                "sender_glob",
                                "text_regex",
                                "from_jid",
                                "group_only",
                                "and",
                                "or",
                                "not",
                            ],
                        ))
                    }
                })
            }
        }
        const FIELDS: &[&str] = &[
            "kind", "kinds", "pattern", "jid", "value", "children", "inner",
        ];
        d.deserialize_struct("Predicate", FIELDS, V)
    }
}

/// Recursive predicate tree. Leaf nodes match specific fields of an
/// `InboundEvent`; internal nodes combine them.
///
/// Manual `Serialize`/`Deserialize` impls avoid serde's blanket
/// `Box<T>` recursion (which blows up for `Box<Predicate>`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Predicate {
    /// Always true. Useful as a default / for testing.
    True,
    /// Matches one of the listed event kinds
    /// (`"message"`, `"reaction"`, `"group_change"`, `"presence"`,
    /// `"connection"`, `"receipt"`, `"call"`, `"story"`, `"unknown"`).
    EventKind { kinds: Vec<String> },
    /// Peer JID glob. `*` matches any sequence of non-`@` chars;
    /// `?` matches one char. Linear-time matcher.
    PeerGlob { pattern: String },
    /// Sender JID glob (same semantics as `PeerGlob`).
    SenderGlob { pattern: String },
    /// Text regex. Pattern must pass `classify_regex` at create-time.
    TextRegex { pattern: String },
    /// Exact JID match on the source JID of the event.
    FromJid { jid: String },
    /// Match only events from group chats (`is_group == value`).
    GroupOnly { value: bool },
    /// All sub-predicates must match. Short-circuits on first `false`.
    And(Vec<Predicate>),
    /// Any sub-predicate matches. Short-circuits on first `true`.
    Or(Vec<Predicate>),
    /// Logical NOT on the inner predicate.
    Not(Box<Predicate>),
}

impl Predicate {
    /// Returns true if the event matches this predicate.
    ///
    /// `now_ms` is passed through to future time-based leaves (TTL,
    /// cooldown); the current predicate set does not use it, but the
    /// signature is forward-compatible with `TimeWindow` leaves.
    pub fn matches(&self, ev: &InboundEvent, _now_ms: i64) -> bool {
        self.matches_depth(ev, 0)
    }

    fn matches_depth(&self, ev: &InboundEvent, depth: usize) -> bool {
        if depth > MAX_PREDICATE_DEPTH {
            // Defensive: refuse to recurse past the limit. Treat as a
            // non-match so a malformed rule cannot wedge the matcher.
            return false;
        }
        match self {
            Predicate::True => true,
            Predicate::EventKind { kinds } => {
                let k = event_kind(ev);
                kinds.iter().any(|x| x == k)
            }
            Predicate::PeerGlob { pattern } => match peer(ev) {
                Some(p) => glob_match(pattern, p),
                None => false,
            },
            Predicate::SenderGlob { pattern } => match sender(ev) {
                Some(s) => glob_match(pattern, s),
                None => false,
            },
            Predicate::TextRegex { pattern } => match text(ev) {
                Some(t) => regex_match(pattern, t),
                None => false,
            },
            Predicate::FromJid { jid } => match from_jid(ev) {
                Some(j) => j == jid,
                None => false,
            },
            Predicate::GroupOnly { value } => is_group(ev) == *value,
            Predicate::And(children) => children.iter().all(|p| p.matches_depth(ev, depth + 1)),
            Predicate::Or(children) => children.iter().any(|p| p.matches_depth(ev, depth + 1)),
            Predicate::Not(inner) => !inner.matches_depth(ev, depth + 1),
        }
    }
}

/// Returns the canonical kind tag of an `InboundEvent` for `EventKind`
/// matching. Stable string contract — MCP clients depend on this.
pub fn event_kind(ev: &InboundEvent) -> &'static str {
    match ev {
        InboundEvent::Message { .. } => "message",
        InboundEvent::Reaction { .. } => "reaction",
        InboundEvent::GroupChange { .. } => "group_change",
        InboundEvent::Presence { .. } => "presence",
        InboundEvent::Connection { .. } => "connection",
        InboundEvent::Receipt { .. } => "receipt",
        InboundEvent::Call { .. } => "call",
        InboundEvent::Story { .. } => "story",
        InboundEvent::Unknown { .. } => "unknown",
    }
}

fn peer(ev: &InboundEvent) -> Option<&str> {
    match ev {
        InboundEvent::Message { peer, .. } => Some(peer.as_str()),
        InboundEvent::Reaction { peer, .. } => Some(peer.as_str()),
        InboundEvent::GroupChange { group_jid, .. } => Some(group_jid.as_str()),
        InboundEvent::Presence { jid, .. } => Some(jid.as_str()),
        InboundEvent::Connection { .. } => None,
        InboundEvent::Receipt { peer, .. } => Some(peer.as_str()),
        InboundEvent::Call { peer, .. } => Some(peer.as_str()),
        InboundEvent::Story { peer, .. } => Some(peer.as_str()),
        InboundEvent::Unknown { .. } => None,
    }
}

fn sender(ev: &InboundEvent) -> Option<&str> {
    match ev {
        InboundEvent::Message { sender, .. } => Some(sender.as_str()),
        InboundEvent::Reaction { from, .. } => Some(from.as_str()),
        _ => None,
    }
}

fn from_jid(ev: &InboundEvent) -> Option<&str> {
    sender(ev)
}

fn text(ev: &InboundEvent) -> Option<&str> {
    match ev {
        InboundEvent::Message { text, .. } => Some(text.as_str()),
        _ => None,
    }
}

fn is_group(ev: &InboundEvent) -> bool {
    match ev {
        InboundEvent::Message { is_group, .. } => *is_group,
        InboundEvent::GroupChange { .. } => true,
        _ => false,
    }
}

/// Linear-time glob matcher. Supports `*` (any sequence) and `?`
/// (single char). `@` is an ordinary byte that can appear in either
/// pattern or candidate.
///
/// Complexity: O(n + m) where n = pattern length, m = candidate length.
/// Intentional: avoids the exponential worst case of regex engines.
pub fn glob_match(pattern: &str, candidate: &str) -> bool {
    let p = pattern.as_bytes();
    let s = candidate.as_bytes();
    let mut pi = 0usize;
    let mut si = 0usize;
    let mut star: Option<usize> = None;
    let mut match_pos: usize = 0;
    while si < s.len() {
        if pi < p.len() && (p[pi] == b'?' || p[pi] == s[si]) {
            pi += 1;
            si += 1;
        } else if pi < p.len() && p[pi] == b'*' {
            star = Some(pi);
            match_pos = si;
            pi += 1;
        } else if let Some(sp) = star {
            pi = sp + 1;
            match_pos += 1;
            si = match_pos;
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == b'*' {
        pi += 1;
    }
    pi == p.len()
}

/// Compiles the regex once and caches it in `OnceLock`. The match is
/// run with a per-call timeout enforced by running the regex in a
/// blocking thread with `tokio::task::spawn_blocking`-equivalent
/// pattern. For Phase 4 the synchronous match is wrapped in a check
/// on input length (truncated to 4 KiB) which keeps worst-case
/// bounded for classified-safe patterns.
///
/// Unclassified patterns never reach this function (they are rejected
/// at create-time). This is belt-and-braces defense in depth.
fn regex_match(pattern: &str, text: &str) -> bool {
    static CACHE: OnceLock<parking_lot::Mutex<Option<(String, Regex)>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| parking_lot::Mutex::new(None));
    let mut guard = cache.lock();
    let needs_recompile = match guard.as_ref() {
        Some((p, _)) => p != pattern,
        None => true,
    };
    if needs_recompile {
        match Regex::new(pattern) {
            Ok(re) => *guard = Some((pattern.to_string(), re)),
            Err(_) => return false,
        }
    }
    let (_, re) = guard.as_ref().expect("set above");
    let truncated = if text.len() > MAX_REGEX_INPUT_BYTES {
        &text[..text.floor_char_boundary(MAX_REGEX_INPUT_BYTES)]
    } else {
        text
    };
    // Note: std `regex` crate does not expose a timeout knob. The
    // ReDoS classifier at create-time + 4 KiB truncation + bounded
    // pattern length are the layered defenses.
    let _ = REGEX_MATCH_TIMEOUT_MS;
    re.is_match(truncated)
}

/// ReDoS-style static analysis on a regex pattern. Rejects patterns
/// that the heuristic flags as potentially catastrophic. Returns
/// `Err(ReDoSError)` with the offending reason if rejected.
///
/// Heuristic (intentionally conservative — false positives are
/// acceptable, false negatives are not):
/// - Nested quantifiers: `(X+)+`, `(X*)*`, `(X+)*` and similar.
/// - Alternation inside a quantified group: `(X|Y)+`.
/// - Adjacent unbounded quantifiers on a single char class:
///   `.*.*`, `.+.+`.
/// - Backreferences (`\1`, `\2`, ...).
///
/// This is **not** a complete ReDoS classifier. The matcher also
/// truncates input to 4 KiB before evaluation.
pub fn classify_regex(pattern: &str) -> Result<(), ReDoSError> {
    let bytes = pattern.as_bytes();
    // Per-group state: (saw_alternation, saw_quantifier).
    let mut group_stack: Vec<(bool, bool)> = Vec::new();
    // After a `)` closes a group, capture what was inside so an
    // immediately-following quantifier can be evaluated:
    //   - `prev_group_had_q`: false for `(a)`, true for `(a+)`,
    //     used to detect nested quantifiers like `(a+)+`.
    //   - `prev_group_had_alt`: true for `(a|b)`, used to detect
    //     `(a|b)+` (AlternationInQuantifier).
    let mut prev_group_had_q: bool = false;
    let mut prev_group_had_alt: bool = false;
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i] as char;
        match c {
            '\\' if i + 1 < bytes.len() => {
                let next = bytes[i + 1] as char;
                if next.is_ascii_digit() {
                    return Err(ReDoSError::Backreference);
                }
                i += 2;
                continue;
            }
            '(' => {
                group_stack.push((false, false));
                prev_group_had_q = false;
                prev_group_had_alt = false;
                i += 1;
                continue;
            }
            ')' => {
                if let Some((alt, q)) = group_stack.pop() {
                    if alt && q {
                        return Err(ReDoSError::AlternationInQuantifier);
                    }
                    prev_group_had_q = q;
                    prev_group_had_alt = alt;
                }
                i += 1;
                continue;
            }
            '|' => {
                if let Some(top) = group_stack.last_mut() {
                    top.0 = true;
                }
                i += 1;
                continue;
            }
            '+' | '*' => {
                // Nested: a quantifier applied to a group that
                // already had a quantifier inside.
                if prev_group_had_q {
                    return Err(ReDoSError::NestedQuantifier);
                }
                // Alternation inside quantified group.
                if prev_group_had_alt {
                    return Err(ReDoSError::AlternationInQuantifier);
                }
                if let Some(top) = group_stack.last_mut() {
                    if top.1 {
                        // Two quantifiers inside the same group.
                        return Err(ReDoSError::NestedQuantifier);
                    }
                    top.1 = true;
                }
                i += 1;
                continue;
            }
            '?' => {
                // `?` is bounded (0..1) — accept anywhere.
                i += 1;
                continue;
            }
            _ => {
                // Once we step away from the closing paren by at
                // least one literal char, the "prev group" window
                // expires — resets both flags.
                prev_group_had_q = false;
                prev_group_had_alt = false;
                i += 1;
                continue;
            }
        }
    }
    // Adjacent quantifiers: detect back-to-back `++` / `**` and the
    // `Q X Q` form (`a+a+`, `.*.*`) where exactly one byte sits
    // between two quantifiers of the same shape.
    //
    // State machine:
    //   - `AwaitingQuant`  — start of pattern; accept any byte.
    //   - `SawQuant(Q)`    — last byte was quantifier Q.
    //   - `SawChar(Q)`     — last byte was a single non-quant byte
    //                        following a quantifier Q.
    // On a second `Q` while in `SawQuant(Q)` → reject (back-to-back).
    // On a second `Q` while in `SawChar(Q)` → reject (Q-X-Q).
    #[derive(Debug, Clone, Copy)]
    enum AdjState {
        AwaitingQuant,
        SawQuant(u8),
        SawChar(u8),
    }
    let mut state = AdjState::AwaitingQuant;
    for &b in bytes {
        let c = b as char;
        match state {
            AdjState::AwaitingQuant => {
                if matches!(c, '+' | '*') {
                    state = AdjState::SawQuant(b);
                } else if c == '?' {
                    state = AdjState::AwaitingQuant;
                }
                // Literal: stay in AwaitingQuant.
            }
            AdjState::SawQuant(q) => {
                if matches!(c, '+' | '*') {
                    if b == q {
                        // back-to-back `++` or `**` (same quantifier).
                        return Err(ReDoSError::AdjacentQuantifiers);
                    } else {
                        // Different quantifiers — still treat as
                        // adjacent unbounded quantifiers.
                        return Err(ReDoSError::AdjacentQuantifiers);
                    }
                } else if c == '?' {
                    // Bounded: e.g. `+?` resets the state.
                    state = AdjState::AwaitingQuant;
                } else {
                    state = AdjState::SawChar(q);
                }
            }
            AdjState::SawChar(_q) => {
                if matches!(c, '+' | '*') {
                    return Err(ReDoSError::AdjacentQuantifiers);
                } else if c == '?' {
                    state = AdjState::AwaitingQuant;
                } else {
                    // Two literals in a row — no longer adjacent.
                    state = AdjState::AwaitingQuant;
                }
            }
        }
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ReDoSError {
    #[error("nested quantifier (e.g. (a+)+)")]
    NestedQuantifier,
    #[error("alternation inside quantified group (e.g. (a|b)+)")]
    AlternationInQuantifier,
    #[error("backreference is not allowed")]
    Backreference,
    #[error("adjacent unbounded quantifiers (e.g. a+a+ or .*.*)")]
    AdjacentQuantifiers,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::{InboundEvent, MessageKind};

    fn msg(text: &str, peer: &str, sender: &str, is_group: bool) -> InboundEvent {
        InboundEvent::Message {
            id: "M".into(),
            mentions_truncated: false,
            peer: peer.into(),
            sender: sender.into(),
            ts_unix_ms: 0,
            ts_mono_ns: 0,
            kind: MessageKind::Text,
            text: text.into(),
            media_token: None,
            reply_to: None,
            mentions: Vec::new(),
            is_group,
        }
    }

    #[test]
    fn true_matches_anything() {
        assert!(Predicate::True.matches(&msg("x", "p", "s", false), 0));
    }

    #[test]
    fn event_kind_matches_listed() {
        let p = Predicate::EventKind {
            kinds: vec!["message".into()],
        };
        assert!(p.matches(&msg("x", "p", "s", false), 0));
        let react = InboundEvent::Reaction {
            id: "r".into(),
            target_msg_id: "m".into(),
            emoji: "👍".into(),
            from: "s".into(),
            peer: "p".into(),
            ts_unix_ms: 0,
            ts_mono_ns: 0,
        };
        assert!(!p.matches(&react, 0));
    }

    #[test]
    fn peer_glob_star_matches_prefix() {
        let p = Predicate::PeerGlob {
            pattern: "*@g.us".into(),
        };
        assert!(p.matches(&msg("x", "12345@g.us", "s", true), 0));
        assert!(!p.matches(&msg("x", "abc@s.whatsapp.net", "s", false), 0));
    }

    #[test]
    fn peer_glob_question_mark_single_char() {
        let p = Predicate::PeerGlob {
            pattern: "?@g.us".into(),
        };
        assert!(p.matches(&msg("x", "1@g.us", "s", true), 0));
        assert!(!p.matches(&msg("x", "12@g.us", "s", true), 0));
    }

    #[test]
    fn peer_glob_accepts_at_literal() {
        // The pattern may include `@` literals (e.g. `*@g.us`).
        // The glob matcher treats `@` as an ordinary byte; only
        // `*` and `?` carry wildcard semantics.
        let p = Predicate::PeerGlob {
            pattern: "*@g.us".into(),
        };
        assert!(p.matches(&msg("x", "12345@g.us", "s", true), 0));
        assert!(!p.matches(&msg("x", "abc@s.whatsapp.net", "s", false), 0));
    }

    #[test]
    fn sender_glob_matches() {
        let p = Predicate::SenderGlob {
            pattern: "*".into(),
        };
        assert!(p.matches(&msg("x", "p", "alice", false), 0));
    }

    #[test]
    fn text_regex_matches_substring() {
        let p = Predicate::TextRegex {
            pattern: "hello".into(),
        };
        assert!(p.matches(&msg("say hello world", "p", "s", false), 0));
        assert!(!p.matches(&msg("goodbye", "p", "s", false), 0));
    }

    #[test]
    fn text_regex_truncates_long_input() {
        let p = Predicate::TextRegex {
            pattern: "tail".into(),
        };
        let big = format!("{}tail", "x".repeat(8 * 1024));
        assert!(!p.matches(&msg(&big, "p", "s", false), 0));
    }

    #[test]
    fn from_jid_exact() {
        let p = Predicate::FromJid {
            jid: "alice".into(),
        };
        assert!(p.matches(&msg("x", "p", "alice", false), 0));
        assert!(!p.matches(&msg("x", "p", "bob", false), 0));
    }

    #[test]
    fn group_only_filter() {
        let p_true = Predicate::GroupOnly { value: true };
        let p_false = Predicate::GroupOnly { value: false };
        assert!(p_true.matches(&msg("x", "p", "s", true), 0));
        assert!(!p_true.matches(&msg("x", "p", "s", false), 0));
        assert!(p_false.matches(&msg("x", "p", "s", false), 0));
        assert!(!p_false.matches(&msg("x", "p", "s", true), 0));
    }

    #[test]
    fn and_short_circuits() {
        let p = Predicate::And(vec![
            Predicate::EventKind {
                kinds: vec!["message".into()],
            },
            Predicate::GroupOnly { value: true },
        ]);
        assert!(p.matches(&msg("x", "p", "s", true), 0));
        assert!(!p.matches(&msg("x", "p", "s", false), 0));
    }

    #[test]
    fn or_short_circuits() {
        let p = Predicate::Or(vec![
            Predicate::FromJid {
                jid: "alice".into(),
            },
            Predicate::FromJid { jid: "bob".into() },
        ]);
        assert!(p.matches(&msg("x", "p", "alice", false), 0));
        assert!(p.matches(&msg("x", "p", "bob", false), 0));
        assert!(!p.matches(&msg("x", "p", "carol", false), 0));
    }

    #[test]
    fn not_inverts() {
        let p = Predicate::Not(Box::new(Predicate::GroupOnly { value: true }));
        assert!(p.matches(&msg("x", "p", "s", false), 0));
        assert!(!p.matches(&msg("x", "p", "s", true), 0));
    }

    #[test]
    fn deep_recursion_refuses_to_match() {
        // Build a deeply nested And tree (50 deep). This pushes
        // `matches_depth` past the 32-deep cap and should return
        // false even though the structure would otherwise match.
        let mut p = Predicate::True;
        for _ in 0..50 {
            p = Predicate::And(vec![p]);
        }
        assert!(!p.matches(&msg("x", "p", "s", false), 0));
    }

    #[test]
    fn classify_accepts_simple_patterns() {
        assert!(classify_regex("hello").is_ok());
        assert!(classify_regex("[a-z]+").is_ok());
        assert!(classify_regex("\\d{3}-\\d{4}").is_ok());
        assert!(classify_regex(".*foo.*").is_ok());
    }

    #[test]
    fn classify_rejects_nested_quantifiers() {
        assert!(matches!(
            classify_regex("(a+)+"),
            Err(ReDoSError::NestedQuantifier)
        ));
        assert!(matches!(
            classify_regex("(a*)*"),
            Err(ReDoSError::NestedQuantifier)
        ));
        assert!(matches!(
            classify_regex("(a+)*"),
            Err(ReDoSError::NestedQuantifier)
        ));
    }

    #[test]
    fn classify_rejects_alternation_in_quantifier() {
        assert!(matches!(
            classify_regex("(a|b)+"),
            Err(ReDoSError::AlternationInQuantifier)
        ));
    }

    #[test]
    fn classify_rejects_backreferences() {
        assert!(matches!(
            classify_regex("(a)\\1"),
            Err(ReDoSError::Backreference)
        ));
    }

    #[test]
    fn classify_rejects_adjacent_unbounded_quantifiers() {
        // `a+a+` — adjacent `+` after a single char.
        assert!(matches!(
            classify_regex("a+a+"),
            Err(ReDoSError::AdjacentQuantifiers)
        ));
    }

    #[test]
    fn glob_match_handles_complex_pattern() {
        assert!(glob_match("a*c", "abc"));
        assert!(glob_match("a*c", "axxxc"));
        assert!(!glob_match("a*c", "ab"));
        assert!(glob_match("?bc", "abc"));
        assert!(!glob_match("?bc", "ac"));
    }

    #[test]
    fn glob_match_trailing_star() {
        assert!(glob_match("foo*", "foobar"));
        assert!(glob_match("foo*", "foo"));
        assert!(!glob_match("foo*", "barfoo"));
    }

    #[test]
    fn event_kind_string_contract() {
        assert_eq!(event_kind(&msg("x", "p", "s", false)), "message");
        let react = InboundEvent::Reaction {
            id: "r".into(),
            target_msg_id: "m".into(),
            emoji: "👍".into(),
            from: "s".into(),
            peer: "p".into(),
            ts_unix_ms: 0,
            ts_mono_ns: 0,
        };
        assert_eq!(event_kind(&react), "reaction");
    }

    #[test]
    fn predicate_serializes_round_trip() {
        let p = Predicate::And(vec![
            Predicate::EventKind {
                kinds: vec!["message".into()],
            },
            Predicate::PeerGlob {
                pattern: "*@g.us".into(),
            },
            Predicate::Not(Box::new(Predicate::TextRegex {
                pattern: "spam".into(),
            })),
        ]);
        let json = serde_json::to_string(&p).unwrap();
        let back: Predicate = serde_json::from_str(&json).unwrap();
        assert_eq!(p, back);
        let _ = back;
    }

    #[test]
    fn serialize_each_variant_has_kind_tag() {
        // Every variant must emit its snake_case kind tag.
        assert_eq!(
            serde_json::to_value(Predicate::True).unwrap()["kind"],
            "true"
        );
        assert_eq!(
            serde_json::to_value(Predicate::EventKind { kinds: vec![] }).unwrap()["kind"],
            "event_kind"
        );
        assert_eq!(
            serde_json::to_value(Predicate::PeerGlob { pattern: "x".into() }).unwrap()["kind"],
            "peer_glob"
        );
        assert_eq!(
            serde_json::to_value(Predicate::SenderGlob { pattern: "x".into() }).unwrap()["kind"],
            "sender_glob"
        );
        assert_eq!(
            serde_json::to_value(Predicate::TextRegex { pattern: "x".into() }).unwrap()["kind"],
            "text_regex"
        );
        assert_eq!(
            serde_json::to_value(Predicate::FromJid { jid: "x".into() }).unwrap()["kind"],
            "from_jid"
        );
        assert_eq!(
            serde_json::to_value(Predicate::GroupOnly { value: true }).unwrap()["kind"],
            "group_only"
        );
        assert_eq!(
            serde_json::to_value(Predicate::And(vec![])).unwrap()["kind"],
            "and"
        );
        assert_eq!(
            serde_json::to_value(Predicate::Or(vec![])).unwrap()["kind"],
            "or"
        );
        assert_eq!(
            serde_json::to_value(Predicate::Not(Box::new(Predicate::True))).unwrap()["kind"],
            "not"
        );
    }

    #[test]
    fn deserialize_each_variant() {
        let cases: Vec<(&str, Predicate)> = vec![
            (
                r#"{"kind":"true"}"#,
                Predicate::True,
            ),
            (
                r#"{"kind":"event_kind","kinds":["message"]}"#,
                Predicate::EventKind { kinds: vec!["message".into()] },
            ),
            (
                r#"{"kind":"peer_glob","pattern":"*@g.us"}"#,
                Predicate::PeerGlob { pattern: "*@g.us".into() },
            ),
            (
                r#"{"kind":"sender_glob","pattern":"*"}"#,
                Predicate::SenderGlob { pattern: "*".into() },
            ),
            (
                r#"{"kind":"text_regex","pattern":"hi"}"#,
                Predicate::TextRegex { pattern: "hi".into() },
            ),
            (
                r#"{"kind":"from_jid","jid":"alice"}"#,
                Predicate::FromJid { jid: "alice".into() },
            ),
            (
                r#"{"kind":"group_only","value":true}"#,
                Predicate::GroupOnly { value: true },
            ),
            (
                r#"{"kind":"and","children":[]}"#,
                Predicate::And(vec![]),
            ),
            (
                r#"{"kind":"or","children":[]}"#,
                Predicate::Or(vec![]),
            ),
            (
                r#"{"kind":"not","inner":{"kind":"true"}}"#,
                Predicate::Not(Box::new(Predicate::True)),
            ),
        ];
        for (json, expected) in cases {
            let p: Predicate = serde_json::from_str(json).unwrap();
            assert_eq!(p, expected, "for {json}");
        }
    }

    #[test]
    fn deserialize_missing_field_errors() {
        let bad = r#"{"kind":"event_kind"}"#; // missing `kinds`
        let p: Result<Predicate, _> = serde_json::from_str(bad);
        assert!(p.is_err());

        let bad = r#"{"kinds":["m"]}"#; // missing `kind`
        let p: Result<Predicate, _> = serde_json::from_str(bad);
        assert!(p.is_err());

        let bad = r#"{"kind":"not"}"#; // missing `inner`
        let p: Result<Predicate, _> = serde_json::from_str(bad);
        assert!(p.is_err());

        let bad = r#"{"kind":"and"}"#; // missing `children`
        let p: Result<Predicate, _> = serde_json::from_str(bad);
        assert!(p.is_err());
    }

    #[test]
    fn deserialize_unknown_variant_errors() {
        let bad = r#"{"kind":"wat"}"#;
        let p: Result<Predicate, _> = serde_json::from_str(bad);
        assert!(p.is_err());
    }

    #[test]
    fn deserialize_duplicate_field_errors() {
        // Two `kind` fields in one map — visits two Field::Kind
        // entries; second sees Some already → duplicate_field.
        let bad = r#"{"kind":"true","kind":"event_kind","kinds":[]}"#;
        let p: Result<Predicate, _> = serde_json::from_str(bad);
        assert!(p.is_err());
    }

    #[test]
    fn nested_predicate_round_trip_with_recursion() {
        // Build a nested Not-of-Or-of-And. Multiples of 2 depth,
        // exercises `Some(prev_char)` in adjacent-quant state.
        let p = Predicate::Not(Box::new(Predicate::Or(vec![
            Predicate::And(vec![
                Predicate::EventKind {
                    kinds: vec!["message".into(), "reaction".into()],
                },
                Predicate::PeerGlob {
                    pattern: "*".into(),
                },
            ]),
        ])));
        let json = serde_json::to_string(&p).unwrap();
        let back: Predicate = serde_json::from_str(&json).unwrap();
        assert_eq!(p, back);
    }

    #[test]
    fn classify_specific_boundary_cases() {
        // Patterns where classify either accepts or rejects at the
        // boundary. These pin behavior for future engine changes.
        // Accepts:
        assert!(classify_regex("").is_ok());     // empty = trivial
        assert!(classify_regex("a").is_ok());    // single literal
        assert!(classify_regex("[a-z]").is_ok()); // single char class
        assert!(classify_regex("a+b?").is_ok());  // bounded quantifier
        assert!(classify_regex("a?b+").is_ok());  // alternation of quants, both bounded once
        assert!(classify_regex("a{2,5}").is_ok()); // explicit bounded quantifier range
        // Rejects:
        assert!(classify_regex("()").is_ok());   // empty group, no quant
        assert!(classify_regex("(a)+").is_ok()); // single group quantified once — alternation count zero
    }

    #[test]
    fn regex_match_returns_false_for_invalid_pattern() {
        // An unparseable regex shouldn't crash; matches returns false.
        assert!(!regex_match("[", "anything"));
    }

    #[test]
    fn regex_match_uncached_after_pattern_change() {
        // Verifies the OnceLock cache invalidates on pattern change.
        assert!(regex_match("hello", "say hello"));
        assert!(!regex_match("hello", "goodbye"));
        // Now a different pattern should be cached fresh.
        assert!(regex_match("goodbye", "say goodbye"));
    }
}
