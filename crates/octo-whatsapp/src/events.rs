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
        InboundEvent::Unknown {
            raw: env.raw,
            ts_unix_ms: env.ts_unix_ms,
            ts_mono_ns: env.ts_mono_ns,
        }
    }
}

#[cfg(test)]
mod tests;
