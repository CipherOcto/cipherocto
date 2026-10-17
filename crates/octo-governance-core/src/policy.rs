//! Canonical `GovernancePolicy` struct + `GovernanceModel` +
//! `EmergencyAuthority` enums per RFC-0013 §Module Layout `policy` +
//! RFC-0855 §11.1.

use serde::{Deserialize, Serialize};

/// Governance policy struct. The combination of `(issuer, model,
/// emergency_authority)` uniquely identifies a governance regime per
/// RFC-0013 §Specification.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GovernancePolicy {
    /// Issuer node DID (raw canonical wire form; conversion lives at
    /// domain call boundary).
    pub issuer: String,
    /// Governance model in effect.
    pub model: GovernanceModel,
    /// Emergency authority override (RFC-0855 §11.2).
    pub emergency_authority: EmergencyAuthority,
    /// Quorum threshold (basis points; e.g. 5000 = 50%).
    pub quorum_bps: u32,
    /// Approval threshold (basis points; e.g. 6000 = 60%).
    pub approval_bps: u32,
}

/// Governance model in effect. `#[non_exhaustive]` per CLAUDE.md
/// §Extension over enumeration — new models land via amendment +
/// extension-registry slot, not central enum edits.
///
/// # Discriminants
///
/// `#[repr(u16)]` for byte-identical storage per RFC-0855 §11.1:
/// Centralized=0, Dao=1, Federated=2, AiAssisted=3, Autonomous=4.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u16)]
pub enum GovernanceModel {
    /// Single issuer has unilateral decision authority.
    Centralized = 0,
    /// Token-weighted DAO vote (default CipherOcto governance model).
    Dao = 1,
    /// Multi-org federation with weighted composite votes.
    Federated = 2,
    /// AI-assisted proposal evaluation + human final vote.
    AiAssisted = 3,
    /// Fully autonomous AI decisioning (subject to override).
    Autonomous = 4,
}

/// Emergency authority override applied during crisis conditions
/// (RFC-0855 §11.2). `#[non_exhaustive]` per CLAUDE.md
/// §Extension over enumeration.
///
/// # Discriminants
///
/// `#[repr(u16)]` for byte-identical storage per RFC-0855 §11.2:
/// None=0, GovernanceCouncil=1, DesignatedRecovery=2.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u16)]
pub enum EmergencyAuthority {
    /// No emergency authority; emergency rekey requires full quorum.
    None = 0,
    /// Pre-defined governance council can override.
    GovernanceCouncil = 1,
    /// Designated recovery key holder can override.
    DesignatedRecovery = 2,
}
