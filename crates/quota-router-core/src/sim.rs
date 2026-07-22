//! Provider simulator (S04 Step 5).
//!
//! Produces realistic responses (200/429/401/timeout/schema-change) for
//! exercise-path tests.

use serde::{Deserialize, Serialize};

/// Provider sim response kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SimResponseKind {
    Ok,
    RateLimited,  // 429
    Unauthorized, // 401
    Timeout,
    SchemaChange, // 200 but malformed body
}

/// Provider sim config.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimConfig {
    pub kind: SimResponseKind,
    pub delay_ms: u64,
}

/// Provider sim — produces a deterministic response given a config.
pub struct ProviderSim {
    config: SimConfig,
}

impl ProviderSim {
    #[must_use]
    pub fn new(config: SimConfig) -> Self {
        Self { config }
    }

    /// Run the sim. Returns status + body.
    #[must_use]
    pub fn run(&self, _request_body: &[u8]) -> SimResult {
        match self.config.kind {
            SimResponseKind::Ok => SimResult {
                status: 200,
                body: br#"{"id":"chatcmpl-1","model":"gpt-4","usage":{"prompt_tokens":100,"completion_tokens":50}}"#.to_vec(),
            },
            SimResponseKind::RateLimited => SimResult {
                status: 429,
                body: br#"{"error":{"type":"rate_limit"}}"#.to_vec(),
            },
            SimResponseKind::Unauthorized => SimResult {
                status: 401,
                body: br#"{"error":{"type":"invalid_api_key"}}"#.to_vec(),
            },
            SimResponseKind::Timeout => SimResult {
                status: 0, // signal: timeout (no response)
                body: vec![],
            },
            SimResponseKind::SchemaChange => SimResult {
                status: 200,
                body: br#"{"unexpected":"format"}"#.to_vec(),
            },
        }
    }
}

#[derive(Debug, Clone)]
pub struct SimResult {
    pub status: u16,
    pub body: Vec<u8>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ok_response() {
        let sim = ProviderSim::new(SimConfig {
            kind: SimResponseKind::Ok,
            delay_ms: 0,
        });
        let r = sim.run(b"{}");
        assert_eq!(r.status, 200);
        assert!(r.body.starts_with(br#"{"id""#));
    }

    #[test]
    fn rate_limited_response() {
        let sim = ProviderSim::new(SimConfig {
            kind: SimResponseKind::RateLimited,
            delay_ms: 0,
        });
        let r = sim.run(b"{}");
        assert_eq!(r.status, 429);
    }

    #[test]
    fn timeout_signal() {
        let sim = ProviderSim::new(SimConfig {
            kind: SimResponseKind::Timeout,
            delay_ms: 0,
        });
        let r = sim.run(b"{}");
        assert_eq!(r.status, 0);
        assert!(r.body.is_empty());
    }

    #[test]
    fn all_kinds_instantiable() {
        for k in [
            SimResponseKind::Ok,
            SimResponseKind::RateLimited,
            SimResponseKind::Unauthorized,
            SimResponseKind::Timeout,
            SimResponseKind::SchemaChange,
        ] {
            let sim = ProviderSim::new(SimConfig {
                kind: k,
                delay_ms: 0,
            });
            let _ = sim.run(b"{}");
        }
    }
}
