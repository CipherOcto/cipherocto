//! Peer and group JID normalization. Every CLI/RPC entry point that takes a
//! peer or group MUST route through these helpers.
//!
//! Both functions are pure and deterministic: same input -> same output,
//! no I/O, safe to use as canonicalization keys.

use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum JidError {
    #[error("expected E.164, <digits>@s.whatsapp.net, or <digits>@lid; got {0:?}")]
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

#[cfg(test)]
mod tests;
