//! Peer and group JID normalization. Every CLI/RPC entry point that takes a
//! peer or group MUST route through these helpers.
//!
//! Both functions are pure and deterministic: same input -> same output,
//! no I/O, safe to use as canonicalization keys.

use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum JidError {
    #[error("expected E.164, <digits>@s.whatsapp.net, <digits>@lid, or <digits>@g.us; got {0:?}")]
    InvalidPeerFormat(String),
    #[error("expected <digits>@g.us; got {0:?}")]
    InvalidGroupFormat(String),
    #[error("phone number invalid: {0}")]
    InvalidPhone(String),
}

pub fn peer_to_jid(input: &str) -> Result<String, JidError> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(JidError::InvalidPeerFormat(trimmed.to_string()));
    }
    // Group JIDs (`<digits>@g.us`) are valid `peer` inputs for
    // send.text / envelope.send so that an agent can post to a group
    // the same way it posts to a 1:1 chat. The shape rules below
    // match `group_to_jid`: digits-only local part, >= 10 digits.
    if trimmed.ends_with("@g.us") {
        let digits = trimmed.trim_end_matches("@g.us");
        if digits.chars().all(|c| c.is_ascii_digit()) && !digits.is_empty() && digits.len() >= 10 {
            return Ok(trimmed.to_string());
        }
        return Err(JidError::InvalidPeerFormat(trimmed.to_string()));
    }
    if trimmed.ends_with("@lid") {
        let digits = trimmed.trim_end_matches("@lid");
        if digits.chars().all(|c| c.is_ascii_digit()) && !digits.is_empty() {
            return Ok(format!("{digits}@lid"));
        }
        return Err(JidError::InvalidPeerFormat(trimmed.to_string()));
    }
    if trimmed.ends_with("@s.whatsapp.net") {
        return Ok(trimmed.to_string());
    }
    if trimmed.contains('@') || trimmed.contains(' ') {
        return Err(JidError::InvalidPeerFormat(trimmed.to_string()));
    }
    let digits = trimmed.trim_start_matches('+');
    // ASCII-only (not Unicode numeric): WhatsApp uses ASCII E.164 internally;
    // accepting Arabic-Indic or full-width digits here would create a JID the
    // server rejects.
    if !digits.chars().all(|c| c.is_ascii_digit()) || digits.is_empty() {
        return Err(JidError::InvalidPeerFormat(trimmed.to_string()));
    }
    // Light validation: 7-15 digits (E.164 max length).
    if digits.len() < 7 || digits.len() > 15 {
        return Err(JidError::InvalidPhone(trimmed.to_string()));
    }
    Ok(format!("{digits}@s.whatsapp.net"))
}

pub fn group_to_jid(input: &str) -> Result<String, JidError> {
    let trimmed = input.trim();
    if !trimmed.ends_with("@g.us") {
        return Err(JidError::InvalidGroupFormat(trimmed.to_string()));
    }
    let digits = trimmed.trim_end_matches("@g.us");
    if digits.chars().all(|c| c.is_ascii_digit()) && !digits.is_empty() && digits.len() >= 10 {
        Ok(trimmed.to_string())
    } else {
        Err(JidError::InvalidGroupFormat(trimmed.to_string()))
    }
}

/// Leading ASCII digits of a JID (the E.164 portion). Used to detect
/// self-sends regardless of the JID envelope shape (`<digits>@s.whatsapp.net`,
/// `<digits>@lid`, `<digits>@g.us`, or bare `<digits>` from `peer_to_jid`).
pub fn jid_digit_prefix(jid: &str) -> String {
    jid.chars().take_while(|c| c.is_ascii_digit()).collect()
}

/// Self-send routing. When the user-supplied peer resolves to the same
/// E.164 digits as the session's own canonical JID, the canonical JID
/// (with device suffix) is returned so the dispatch lands on the
/// operator's linked WA client. Otherwise the input JID is returned
/// unchanged. A `None` `self_jid_full` (operator's identity hasn't
/// resolved yet, typical for early boot) means the routing decision
/// defaults to the caller-supplied peer.
///
/// Group JIDs (`@g.us`) never match because `jid_digit_prefix` returns
/// the same leading digits for both sides, and we additionally require
/// the post-`@` domain to match the session JID's domain — a self-send
/// on a group address would only make sense if the operator joined the
/// group on the linked device, in which case the dispatch is still
/// correctly addressed (same JID either way). To be conservative we
/// only swap when the envelopes are domain-equivalent
/// (`s.whatsapp.net` ↔ `s.whatsapp.net`, `lid` ↔ `lid`).
pub fn apply_self_routing(peer_jid: &str, self_jid_full: Option<&str>) -> String {
    let Some(self_jid) = self_jid_full else {
        return peer_jid.to_string();
    };
    let peer_digits = jid_digit_prefix(peer_jid);
    let self_digits = jid_digit_prefix(self_jid);
    if peer_digits.is_empty() || peer_digits != self_digits {
        return peer_jid.to_string();
    }
    // Domain check: only swap when the envelopes share a domain shape.
    let peer_domain = peer_jid.find('@').map(|i| &peer_jid[i..]);
    let self_domain = self_jid.find('@').map(|i| &self_jid[i..]);
    match (peer_domain, self_domain) {
        (Some(p), Some(s)) if p == s => self_jid.to_string(),
        _ => peer_jid.to_string(),
    }
}

#[cfg(test)]
mod tests;
