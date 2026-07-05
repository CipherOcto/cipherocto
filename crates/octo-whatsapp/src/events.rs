//! Typed inbound event model + parser. Phase 1: only the `Unknown` fallback
//! is exercised; the full parser arrives in Phase 3 alongside the event
//! router and `events.tail`.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EventEnvelope {
    pub raw: String,
    pub ts_unix_ms: i64,
    pub ts_mono_ns: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum InboundEvent {
    Unknown {
        raw: String,
        ts_unix_ms: i64,
        ts_mono_ns: u64,
    },
}

impl InboundEvent {
    pub fn parse(env: EventEnvelope) -> Self {
        // Phase 1: every event is `Unknown`. Phase 3 will introduce a real
        // parser that classifies by `format!("{:?}", ev)` output shape.
        let _ = env;
        todo!("Phase 1 Task 13")
    }
}

#[cfg(test)]
mod tests;
