//! Peer and group JID normalization. Every CLI/RPC entry point that takes a
//! peer or group MUST route through these helpers.

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
    todo!("Phase 1 Task 6")
}

pub fn group_to_jid(input: &str) -> Result<String, JidError> {
    todo!("Phase 1 Task 9")
}

#[cfg(test)]
mod tests;