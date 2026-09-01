//! `octo-runtime` substrate error envelope.
//!
//! Per RFC-0011-c §9.8 (Error Codes) + §9.10 (Substrate `[ADD]` Signatures).
//! Every variant is `#[non_exhaustive]` — new substrate additions land as
//! additive variants without breaking downstream consumers (per
//! [[cipherocto-design-principles]] §Extension over enumeration).

use thiserror::Error;

/// Substrate error envelope for `spawn_agent` + `attach` (RFC-0011-c §9.10).
///
/// # Layer discipline
///
/// Layer B substrate. Surface area consumed by Layer C/D `octo-cli`
/// per RFC-0011-c §9.8 (exit codes 44/48/49/51). The CLI maps each
/// variant to a stable exit code verbatim; substrate does not invent
/// parallel variants for CLI-side concerns.
#[derive(Error, Debug)]
#[non_exhaustive]
pub enum RuntimeError {
    /// `agent_id` is unknown to the substrate.
    ///
    /// Surfaces verbatim from `agent run` / `agent list` /
    /// `agent destroy` / `agent attach`. CLI exit code 42 per
    /// RFC-0011-c §9.8.
    #[error("agent not found: {0}")]
    AgentNotFound(uuid::Uuid),

    /// `agent_id` already has a live runtime handle.
    ///
    /// Surfaces from `agent run` against an agent that is already
    /// in `Busy` state. CLI exit code 43 (invalid state transition)
    /// per RFC-0011-c §9.8 — `Active → Busy` is invalid from `Busy`.
    #[error("agent already running: {0}")]
    AgentAlreadyRunning(uuid::Uuid),

    /// `agent_id` is not in a state that permits attach.
    ///
    /// Surfaces from `agent attach` against a `Terminated` /
    /// `Retired` / `Rejected` agent. CLI exit code 48 per
    /// RFC-0011-c §9.8.
    #[error("agent not running: {0}")]
    AgentNotRunning(uuid::Uuid),

    /// State-machine rejected the requested transition (RFC-0002
    /// §Agent State Machine substrate-authoritative).
    ///
    /// CLI exit code 43 per RFC-0011-c §9.8.
    #[error("invalid state transition: {from} -> {to}")]
    InvalidStateTransition {
        /// Current state the substrate observed.
        from: String,
        /// Target state the CLI requested.
        to: String,
    },

    /// Runtime container spawn failed.
    ///
    /// CLI exit code 44 per RFC-0011-c §9.8. The `reason` string is
    /// substrate-internal; the CLI sanitizes it via
    /// `OctoCliError::sanitize_substrate_error` before display.
    #[error("runtime spawn failed: {reason}")]
    RuntimeSpawnFailed {
        /// Substrate-internal failure reason (sanitized by CLI).
        reason: String,
    },

    /// Runtime attach failed (handle revoked, channel closed, etc.).
    ///
    /// CLI exit code 49 per RFC-0011-c §9.8.
    #[error("runtime attach failed: {reason}")]
    RuntimeAttachFailed {
        /// Substrate-internal failure reason (sanitized by CLI).
        reason: String,
    },

    /// `RuntimeHandle` was dropped and the underlying pub-sub
    /// subscription was revoked.
    ///
    /// CLI exit code 49 (runtime attach failed) per RFC-0011-c §9.8.
    #[error("runtime handle revoked for agent {0}")]
    HandleRevoked(uuid::Uuid),

    /// `AttachHandle` does not match the requested `agent_id`.
    ///
    /// CLI exit code 49 per RFC-0011-c §9.8.
    #[error("invalid attach handle: {0}")]
    InvalidAttachHandle(String),

    /// Underlying event-stream channel closed unexpectedly.
    ///
    /// CLI exit code 49 per RFC-0011-c §9.8.
    #[error("event stream closed")]
    EventStreamClosed,
}

impl RuntimeError {
    /// Stable exit-code mapping per RFC-0011-c §9.8.
    ///
    /// Returned values mirror the CLI's `OctoCliError::exit_code()`
    /// mapping for the runtime-related variants (44/48/49). The CLI
    /// is the canonical surface for these codes; the substrate
    /// provides the mapping for tests + adapter-layer use.
    #[must_use]
    pub const fn exit_code(&self) -> i32 {
        match self {
            Self::AgentNotFound(_) => 42,
            Self::AgentAlreadyRunning(_) | Self::InvalidStateTransition { .. } => 43,
            Self::RuntimeSpawnFailed { .. } => 44,
            Self::AgentNotRunning(_) => 48,
            Self::RuntimeAttachFailed { .. }
            | Self::HandleRevoked(_)
            | Self::InvalidAttachHandle(_)
            | Self::EventStreamClosed => 49,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exit_code_matches_rfc_0011_c_section_9_8() {
        // Substrate-authoritative exit codes per RFC-0011-c §9.8.
        let cases: Vec<(RuntimeError, i32)> = vec![
            (RuntimeError::AgentNotFound(uuid::Uuid::nil()), 42),
            (RuntimeError::AgentAlreadyRunning(uuid::Uuid::nil()), 43),
            (
                RuntimeError::InvalidStateTransition {
                    from: "TERMINATED".into(),
                    to: "ACTIVE".into(),
                },
                43,
            ),
            (RuntimeError::RuntimeSpawnFailed { reason: "x".into() }, 44),
            (RuntimeError::AgentNotRunning(uuid::Uuid::nil()), 48),
            (RuntimeError::RuntimeAttachFailed { reason: "x".into() }, 49),
            (RuntimeError::HandleRevoked(uuid::Uuid::nil()), 49),
            (RuntimeError::InvalidAttachHandle("x".into()), 49),
            (RuntimeError::EventStreamClosed, 49),
        ];
        for (err, code) in cases {
            assert_eq!(err.exit_code(), code, "{err:?}");
        }
    }

    #[test]
    fn display_messages_match_rfc_0011_c_section_9_10() {
        // Display strings match the RFC-0011-c §9.10 substrate signature
        // verbatim (CLI sanitizes but preserves the leading tag).
        let e = RuntimeError::AgentNotFound(uuid::Uuid::nil());
        assert!(e.to_string().starts_with("agent not found:"), "{}", e);
        let e = RuntimeError::RuntimeSpawnFailed {
            reason: "no container runtime".into(),
        };
        assert!(e.to_string().contains("runtime spawn failed"), "{}", e);
        let e = RuntimeError::InvalidStateTransition {
            from: "REGISTERED".into(),
            to: "BUSY".into(),
        };
        assert!(e.to_string().contains("REGISTERED -> BUSY"), "{}", e);
    }
}
