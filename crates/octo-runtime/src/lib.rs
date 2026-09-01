//! CipherOcto Runtime — agent spawn / attach substrate.
//!
//! Mission `0011-c-octo-runtime-substrate` (RFC-0011-c §9.10
//! Substrate `[ADD]` Signatures; Layer B substrate-authoritative).
//!
//! ## Architecture
//!
//! ```text
//! CLI → octo-runtime → RuntimeHandle → broadcast::Sender<RuntimeEvent>
//!                          ↓
//!                  AgentStateDispatcher (octo-wallet substrate)
//! ```
//!
//! ## Substrate surface
//!
//! - [`spawn_agent`] — mint a `RuntimeHandle` for an `agent_id`
//!   (RFC-0011-c §9.3.2 `agent run`).
//! - [`attach`] — open an `EventStream` against a live `RuntimeHandle`
//!   (RFC-0011-c §9.3.5 `agent attach`).
//! - [`AgentStateDispatcher`] — shared state-machine trait consumed
//!   from `octo-wallet` (RFC-0011-c §Implementation Phases Phase 1).
//!
//! ## Layer discipline
//!
//! Layer B substrate (RFC-0011-c §9.1 Architecture). Types are
//! years-stable per [[cipherocto-design-principles]] §Stable
//! Abstractions Principle. The `octo-cli` (Layer C/D) consumes
//! this crate; the dependency direction is one-way (`octo-cli` →
//! `octo-runtime`). New variants on `AgentState` / `RuntimeEvent`
//! land additively via `#[non_exhaustive]`
//! ([[cipherocto-design-principles]] §Extension over enumeration).
//!
//! ## State semantics
//!
//! `octo-runtime` does **not** mutate agent lifecycle state. State
//! transitions are the `AgentStateDispatcher`'s responsibility
//! (RFC-0011-c §Implementation Phases Phase 1). The runtime
//! substrate allocates the handle, opens the pub-sub bus, and
//! surfaces events — it does not own the state machine.

#![deny(unsafe_code)]
#![warn(missing_debug_implementations)]
#![allow(clippy::module_name_repetitions)]

pub mod attach;
pub mod error;
pub mod handle;
pub mod spawn;

// Public re-exports — substrate surface per RFC-0011-c §9.10.
pub use attach::{attach, clamp_since};
pub use error::RuntimeError;
pub use handle::{
    AgentState, AgentStateDispatcher, AttachHandle, EventStream, RuntimeEvent, RuntimeHandle,
    RuntimeHandleId, EVENT_CHANNEL_CAPACITY,
};
pub use spawn::{spawn_agent, EXPECTED_PRE_SPAWN_STATE};

/// Legacy `execute_agent` shim — kept for callers predating
/// mission `0011-c-octo-runtime-substrate` (the `octo-cli` v0.1.0
/// registry stub still calls it). New code MUST use [`spawn_agent`]
/// + [`attach`] per RFC-0011-c §9.10.
#[deprecated(
    since = "0.2.0",
    note = "use spawn_agent + attach per RFC-0011-c §9.10; execute_agent is a pre-substrate MVP shim"
)]
pub async fn execute_agent(name: &str) -> Result<String, String> {
    println!("🚀 Executing agent: {}", name);
    // MVP behavior preserved verbatim from the 0.1.0 skeleton.
    println!("✓ Agent completed execution");
    println!("✓ Results persisted to registry");
    Ok(format!("Agent {name} executed successfully"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use uuid::Uuid;

    #[test]
    fn substrate_re_exports_compile() {
        // Smoke test: every re-exported type is constructible in its
        // canonical form. Guards against accidental visibility or
        // name drift during future refactors.
        let agent_id = Uuid::new_v4();
        let handle = spawn_agent(agent_id, None).expect("spawn");
        let handle_id = handle.handle_id;
        let _stream = attach(handle, None).expect("attach");
        let _token = AttachHandle {
            agent_id,
            handle_id,
            issued_at_unix: Utc::now().timestamp(),
        };
        // RuntimeError variants construct.
        let _e = RuntimeError::AgentNotFound(agent_id);
        let _s = AgentState::Active;
        let _ev = RuntimeEvent::Spawned {
            agent_id,
            at: Utc::now(),
        };
    }

    #[test]
    fn event_channel_capacity_is_power_of_two() {
        // Sanity check: capacity must be a positive power of two (the
        // tokio `broadcast` channel uses a ring buffer; non-power-of-
        // two capacities still work but signal non-substrate-quality
        // sizing). 1024 is the documented default.
        const { assert!(EVENT_CHANNEL_CAPACITY > 0) };
        assert_eq!(EVENT_CHANNEL_CAPACITY, 1024);
    }
}
