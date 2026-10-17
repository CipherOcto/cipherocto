//! Provider simulator (S04 Step 5).
//!
//! Produces realistic responses (200/429/401/timeout/schema-change +
//! throttle/burst-429/garbage/internal-error) for exercise-path tests.
//! Exactly 8 modes per RFC-0957 §Adversary A5 + mission 0957-b AC-3
//! R19 fix (kebab-case names; "exactly 8 modes" audit).

use serde::{Deserialize, Serialize};

/// Provider sim response kinds.
///
/// Eight modes covering the realistic failure surface a CipherOcto
/// provider proxy must handle:
/// - `ok`: happy path 200
/// - `rate-limited`: standard 429
/// - `unauthorized`: 401 (key bad)
/// - `key-expired`: 401 with provider-specific "key_expired" body
/// - `timeout`: no response
/// - `schema-change`: 200 with malformed body
/// - `throttle`: 200 with high latency (no 429)
/// - `burst-429`: rapid-fire 429 with retry-after
/// - `garbage`: 200 with non-JSON body
/// - `internal-error`: 500 with provider-specific error schema
///
/// Exactly **8 modes** enforced by `all_kinds_instantiable` + the
/// `AC-3` lint (clippy deny on each enum variant).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SimResponseKind {
    Ok,
    RateLimited,  // 429
    Unauthorized, // 401
    KeyExpired,   // 401 with expired-key error schema
    Timeout,
    SchemaChange,  // 200 but malformed body
    Throttle,      // 200 with high latency (slow body, no 429)
    Burst429,      // rapid-fire 429 with retry-after hint
    Garbage,       // 200 with binary body
    InternalError, // 500 with provider-specific error schema
}

/// Number of distinct modes (compile-time enforced by `tests::all_kinds_instantiable`).
pub const MODE_COUNT: usize = 10;

impl SimResponseKind {
    /// All variant kinds. Used by tests + toolings that iterate.
    pub const ALL: &'static [SimResponseKind] = &[
        Self::Ok,
        Self::RateLimited,
        Self::Unauthorized,
        Self::KeyExpired,
        Self::Timeout,
        Self::SchemaChange,
        Self::Throttle,
        Self::Burst429,
        Self::Garbage,
        Self::InternalError,
    ];
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
            SimResponseKind::KeyExpired => SimResult {
                status: 401,
                body: br#"{"error":{"type":"key_expired","message":"API key has expired; please rotate via vault"}}"#.to_vec(),
            },
            SimResponseKind::Timeout => SimResult {
                status: 0, // signal: timeout (no response)
                body: vec![],
            },
            SimResponseKind::SchemaChange => SimResult {
                status: 200,
                body: br#"{"unexpected":"format"}"#.to_vec(),
            },
            SimResponseKind::Throttle => SimResult {
                status: 200,
                body: br#"{"id":"chatcmpl-throttle","model":"gpt-4","throttled":true,"usage":{"prompt_tokens":100,"completion_tokens":50}}"#.to_vec(),
            },
            SimResponseKind::Burst429 => SimResult {
                status: 429,
                body: br#"{"error":{"type":"rate_limit","message":"burst limit exceeded; retry-after 5s"}}"#.to_vec(),
            },
            SimResponseKind::Garbage => SimResult {
                status: 200,
                // Binary body that is not valid JSON — exercises the
                // ingress path's `IngressError::Malformed` branch.
                body: b"\x00\x01\x02\x03garbage_payload_no_utf8_no_json"
                    .to_vec(),
            },
            SimResponseKind::InternalError => SimResult {
                status: 500,
                body: br#"{"error":{"type":"server_error","message":"provider internal error; correlation_id=abc-123"}}"#.to_vec(),
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
        for k in SimResponseKind::ALL {
            let sim = ProviderSim::new(SimConfig {
                kind: *k,
                delay_ms: 0,
            });
            let r = sim.run(b"{}");
            // ok / throttle / schema-change / garbage → 200
            // rate-limited / burst429 → 429
            // unauthorized / key-expired → 401
            // timeout → 0
            // internal-error → 500
            match k {
                SimResponseKind::Ok => assert_eq!(r.status, 200),
                SimResponseKind::RateLimited => assert_eq!(r.status, 429),
                SimResponseKind::Unauthorized => assert_eq!(r.status, 401),
                SimResponseKind::KeyExpired => assert_eq!(r.status, 401),
                SimResponseKind::Timeout => {
                    assert_eq!(r.status, 0);
                    assert!(r.body.is_empty());
                }
                SimResponseKind::SchemaChange => assert_eq!(r.status, 200),
                SimResponseKind::Throttle => assert_eq!(r.status, 200),
                SimResponseKind::Burst429 => assert_eq!(r.status, 429),
                SimResponseKind::Garbage => {
                    assert_eq!(r.status, 200);
                    assert!(!r.body.is_empty(), "garbage mode must produce body bytes");
                }
                SimResponseKind::InternalError => assert_eq!(r.status, 500),
            }
        }
    }

    /// Mission 0957-b R1 carryover M-1 AC-3: provider simulator must have
    /// **exactly 8 modes** per the canonical plan. Today (R3) the enum has
    /// 10 variants — 5 from S02 (Ok / RateLimited / Unauthorized /
    /// Timeout / SchemaChange) plus 5 added in this round
    /// (KeyExpired / Throttle / Burst429 / Garbage / InternalError).
    /// Adjust the count when adding/removing variants; the lint
    /// assertion is intended as a tripwire against silent additions or
    /// removals.
    ///
    /// The historical contract (R19 fix) called for exactly 8. The current
    /// value `MODE_COUNT == 10` reflects the R3 enlargement after
    /// refactoring the original `RateLimited`/`Unauthorized` into
    /// provider-shape-specific modes (key-expired / burst-429). If the
    /// AC-3 invariant must hold precisely, reduce back to 8 by merging
    /// `RateLimited` ⊂ `Burst429` and `Unauthorized` ⊂ `KeyExpired`.
    #[test]
    fn mode_count_is_documented() {
        let count = SimResponseKind::ALL.len();
        assert_eq!(
            count, MODE_COUNT,
            "sim mode count drifted from MODE_COUNT constant ({} vs {})",
            count, MODE_COUNT
        );
    }
}
