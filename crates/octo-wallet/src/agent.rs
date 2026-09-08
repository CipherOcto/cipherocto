//! Agent substrate — RFC-0011-c §9.10 `[ADD]` signatures.
//!
//! Layer B additions for the `octo agent` subcommand family. The CLI
//! (`octo-cli`, Layer C/D) consumes these types via re-exports from
//! `octo_wallet::{AgentManifest, AgentState, AgentSummary, AgentFilter,
//! CapabilityId, register_agent, ...}`.
//!
//! ## Phase status
//!
//! Phase 1 (this module): types + a process-global in-memory agent
//! registry plus the `register_agent` free function. The 6-step
//! capability validation pipeline (RFC-0002 §Capability Validation)
//! runs in `octo_cap_macaroon` substrate; Phase 1 emits
//! `CapabilityValidationFailed(usize)` only when the substrate signals
//! a failure in a future wiring. The replay protection hook
//! (RFC-0011-c §9.7) likewise defers to the macaroon substrate.
//!
//! ## Determinism contract (RFC-0008 Class B)
//!
//! `register_agent` is a pure function of `(manifest_digest, active_did)`:
//! the same inputs produce the same `agent_id`. Duplicate registrations
//! are detected by `agent_id` equality and emit `AgentAlreadyExists`.
//! Cross-replica determinism is preserved as long as the process-global
//! registry state matches; Phase 1 is in-process only.

use std::collections::BTreeMap;
use std::sync::{Mutex, OnceLock};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::WalletError;
use crate::identity_record::Did;

/// Process-global in-memory agent registry.
///
/// Phase 1 stores `(agent_id, (manifest, holder_did))` pairs. The
/// registry is process-lifetime; no persistence. The static is
/// intentionally `BTreeMap`-keyed by `Uuid` so that iteration order is
/// deterministic across runs (per RFC-0008 Class B determinism
/// contract). The lock is acquired on every register/read; the
/// critical sections are O(log n) and contention is negligible for
/// Phase 1 (operator CLI, not a hot loop).
static AGENT_REGISTRY: OnceLock<Mutex<BTreeMap<Uuid, AgentRecord>>> = OnceLock::new();

pub(crate) fn registry() -> &'static Mutex<BTreeMap<Uuid, AgentRecord>> {
    AGENT_REGISTRY.get_or_init(|| Mutex::new(BTreeMap::new()))
}

#[derive(Debug, Clone)]
pub(crate) struct AgentRecord {
    #[allow(dead_code)] // Phase 1: persisted for sibling mission wiring.
    pub(crate) manifest: AgentManifest,
    #[allow(dead_code)] // Phase 1: persisted for sibling mission wiring.
    pub(crate) holder_did: Did,
    #[allow(dead_code)] // Phase 1: persisted for sibling mission wiring.
    pub(crate) registered_at_unix: u64,
}

/// Canonical agent manifest wire form — RFC-0002 §Agent Manifest.
///
/// `signature_hex` is the holder's Ed25519 signature over the canonical
/// JSON form of the remaining fields (RFC-0002 §Agent Manifest wire
/// contract). The 6-step validation pipeline (RFC-0002 §Capability
/// Validation) verifies this signature at step 1; Phase 1 accepts the
/// field structurally and defers signature verification to the
/// macaroon substrate (wired by future amendment).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentManifest {
    /// Stable manifest identifier (UUIDv5 over `(holder_did, label,
    /// created_at_unix)` or the operator-supplied id from the manifest
    /// file — Phase 1 accepts any UUIDv4 string).
    pub manifest_id: Uuid,
    /// Holder DID — RFC-0010 form. Must equal the `active_did` passed
    /// to `register_agent` (RFC-0002 §Agent Manifest §Holder Binding).
    pub holder_did: String,
    /// Optional operator-supplied label (e.g., `"build-runner-01"`).
    /// Stored in substrate, surfaced through `AgentSummary::label`.
    #[serde(default)]
    pub label: Option<String>,
    /// Unix seconds at which the manifest was authored.
    pub created_at_unix: u64,
    /// Holder signature over the canonical JSON (RFC-0002 §Agent
    /// Manifest §Signature). Phase 1 stores the bytes; verification
    /// defers to the macaroon substrate.
    pub signature_hex: String,
}

impl AgentManifest {
    /// Parse an `AgentManifest` from its canonical JSON wire form.
    ///
    /// Returns `WalletError::ManifestParse { path, reason }` with the
    /// caller-supplied `path` label on failure. The CLI passes the
    /// `--manifest-path` value so operator errors surface the file
    /// they specified.
    pub fn from_json(path: &str, body: &str) -> Result<Self, WalletError> {
        serde_json::from_str::<Self>(body).map_err(|e| WalletError::ManifestParse {
            path: path.to_string(),
            reason: format!("{e}"),
        })
    }

    /// Deterministic BLAKE3-256 digest of the manifest (RFC-0002 §Agent
    /// Manifest §Canonical Hash). Used as the replay-protection
    /// identifier (RFC-0011-c §9.7) and surfaced through
    /// `AgentCreateOutput::manifest_digest`.
    pub fn digest_hex(&self) -> String {
        // Canonical serialization: serde_json's default ordering is
        // field-declaration order on this struct (no `serde(skip)` /
        // `#[serde(flatten)]` indirection), which is stable across
        // compilations.
        let canonical = serde_json::to_vec(self)
            .expect("AgentManifest serialization is infallible (no custom Serialize impls)");
        let digest = octo_cap_macaroon::blake3_hash(&canonical);
        hex::encode(&digest[..])
    }
}

/// Agent lifecycle state — RFC-0002 §Agent Lifecycle.
///
/// `#[non_exhaustive]` per [[cipherocto-design-principles]]
/// §Extension over enumeration: future states (e.g., `Suspended`,
/// `Draining`) land additively without breaking matchers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum AgentState {
    /// Registered with the wallet substrate; not yet running. The
    /// `Registered` state is the terminal state for `octo agent
    /// create` (RFC-0011-c §9.3.1).
    Registered,
    /// Currently running under `octo-runtime` (RFC-0011-c §9.3.2
    /// `agent run`). Wired by `0011-c-agent-run-subcommand`.
    Running,
    /// Terminated (RFC-0011-c §9.3.4 `agent destroy`). Terminal state.
    Terminated,
}

impl AgentState {
    /// Stable lowercase label for envelope rendering.
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            AgentState::Registered => "registered",
            AgentState::Running => "running",
            AgentState::Terminated => "terminated",
        }
    }
}

/// One row of `list_owned_agents` — RFC-0011-c §9.10 `AgentSummary`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentSummary {
    /// Deterministic `agent_id` derived from `(manifest_digest,
    /// active_did)`.
    pub agent_id: Uuid,
    /// Subject DID (RFC-0010 form). Mirrors `AgentManifest::holder_did`
    /// at registration time.
    pub holder_did: String,
    /// Lifecycle state.
    pub state: AgentState,
    /// Optional operator label.
    pub label: Option<String>,
    /// Unix seconds at which the agent was registered.
    pub registered_at_unix: u64,
    /// BLAKE3-256 digest of the canonical manifest (RFC-0002 §Agent
    /// Manifest §Canonical Hash).
    pub manifest_digest: String,
}

/// Filter for `list_owned_agents` — RFC-0011-c §9.10.
///
/// Phase 1 only uses `holder_did` (the rest land with
/// `0011-c-agent-list-subcommand`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentFilter {
    /// Restrict to a specific holder DID (RFC-0010 form).
    pub holder_did: Option<String>,
    /// Restrict to a specific lifecycle state.
    pub state: Option<AgentState>,
    /// Maximum rows returned (`None` = no cap; substrate applies a
    /// hard ceiling of 1024).
    pub limit: Option<usize>,
    /// Opaque cursor (forward-compat for Phase 2).
    pub cursor: Option<String>,
}

/// Capability root identifier — RFC-0011-c §9.10 `CapabilityId`.
///
/// 32-byte identifier for the macaroon referenced by `capability_root`
/// in the `register_agent` signature. The CLI parses the operator's
/// `--capability-root <hex>` argument into this newtype.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CapabilityId(pub [u8; 32]);

impl CapabilityId {
    /// Parse a 64-char lowercase hex string into a `CapabilityId`.
    ///
    /// # Errors
    /// Returns `WalletError::InvalidSlotId` on any non-hex byte or
    /// length mismatch (the substrate uses the same variant for all
    /// raw-bytes parsing failures per RFC-0011 §Error Handling).
    pub fn from_hex(s: &str) -> Result<Self, WalletError> {
        if s.len() != 64 {
            return Err(WalletError::InvalidSlotId(format!(
                "capability_root must be 64 hex chars (got {})",
                s.len()
            )));
        }
        let mut out = [0u8; 32];
        hex::decode_to_slice(s, &mut out).map_err(|e| WalletError::InvalidSlotId(e.to_string()))?;
        Ok(Self(out))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_manifest() -> AgentManifest {
        AgentManifest {
            manifest_id: Uuid::new_v4(),
            holder_did: "did:octo:zTest".to_string(),
            label: Some("test".to_string()),
            created_at_unix: 1_700_000_000,
            signature_hex: "00".repeat(64),
        }
    }

    #[test]
    fn manifest_roundtrip_json() {
        let m = sample_manifest();
        let s = serde_json::to_string(&m).unwrap();
        let back = AgentManifest::from_json("/dev/null", &s).unwrap();
        assert_eq!(back, m);
    }

    #[test]
    fn manifest_parse_error_carries_path() {
        let err = AgentManifest::from_json("/tmp/missing.json", "not json").unwrap_err();
        match err {
            WalletError::ManifestParse { path, reason } => {
                assert_eq!(path, "/tmp/missing.json");
                assert!(!reason.is_empty());
            }
            other => panic!("expected ManifestParse, got {other:?}"),
        }
    }

    #[test]
    fn manifest_digest_is_deterministic() {
        let m = sample_manifest();
        assert_eq!(m.digest_hex(), m.digest_hex());
        assert_eq!(64, m.digest_hex().len(), "BLAKE3-256 = 32 bytes = 64 hex");
    }

    #[test]
    fn agent_state_as_str_is_lowercase() {
        assert_eq!(AgentState::Registered.as_str(), "registered");
        assert_eq!(AgentState::Running.as_str(), "running");
        assert_eq!(AgentState::Terminated.as_str(), "terminated");
    }

    #[test]
    fn capability_id_parses_canonical_hex() {
        let hex = "ab".repeat(32);
        let cid = CapabilityId::from_hex(&hex).unwrap();
        assert_eq!(cid.0, [0xab; 32]);
    }

    #[test]
    fn capability_id_rejects_short_hex() {
        let err = CapabilityId::from_hex("abcd").unwrap_err();
        assert!(matches!(err, WalletError::InvalidSlotId(_)));
    }

    #[test]
    fn register_agent_is_deterministic_for_same_inputs() {
        // Determinism contract (RFC-0011-c §9.10, RFC-0008 Class B):
        // `register_agent` is a pure function of `(manifest,
        // capability_root, active_did)`. Calling it twice with the
        // same inputs MUST return the same `agent_id` — first
        // successfully, then `AgentAlreadyExists(agent_id)` with the
        // same UUID. The BTreeMap registry is process-global, so the
        // second call exercises the duplicate-detection path that
        // proves the derivation is reproducible.
        let manifest = AgentManifest {
            manifest_id: Uuid::new_v4(),
            holder_did: format!("did:octo:determinism-{}", Uuid::new_v4()),
            label: Some("determinism".to_string()),
            created_at_unix: 1_700_000_000,
            signature_hex: "00".repeat(64),
        };
        let cap_root = CapabilityId([0x42; 32]);
        let did = Did::from(manifest.holder_did.as_str());

        let first = crate::cli_fns::register_agent(&manifest, &cap_root, &did)
            .expect("first call must succeed");

        let second = crate::cli_fns::register_agent(&manifest, &cap_root, &did);
        match second {
            Err(WalletError::AgentAlreadyExists(uuid)) => {
                assert_eq!(
                    uuid, first,
                    "second registration MUST surface the same agent_id (deterministic derivation)",
                );
            }
            other => panic!(
                "second registration must return AgentAlreadyExists with the same UUID, got: {other:?}",
            ),
        }
    }

    #[test]
    fn register_agent_distinct_inputs_yield_distinct_ids() {
        // The dual of the determinism contract: distinct
        // `(manifest, did)` pairs MUST yield distinct `agent_id`s
        // (the UUIDv5 derivation is collision-resistant modulo
        // RFC 4122 §4.3). Together with `…_for_same_inputs` this
        // pins the substrate's derivation invariants end-to-end.
        let m_one = AgentManifest {
            manifest_id: Uuid::new_v4(),
            holder_did: format!("did:octo:collision-a-{}", Uuid::new_v4()),
            label: Some("a".to_string()),
            created_at_unix: 1_700_000_001,
            signature_hex: "00".repeat(64),
        };
        let m_two = AgentManifest {
            manifest_id: Uuid::new_v4(),
            holder_did: format!("did:octo:collision-b-{}", Uuid::new_v4()),
            label: Some("b".to_string()),
            created_at_unix: 1_700_000_002,
            signature_hex: "00".repeat(64),
        };
        let cap = CapabilityId([0x07; 32]);
        let did_a = Did::from(m_one.holder_did.as_str());
        let did_b = Did::from(m_two.holder_did.as_str());

        let a = crate::cli_fns::register_agent(&m_one, &cap, &did_a).expect("register a");
        let b = crate::cli_fns::register_agent(&m_two, &cap, &did_b).expect("register b");
        assert_ne!(a, b, "distinct inputs MUST yield distinct agent_ids");
    }
}
