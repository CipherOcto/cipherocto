//! `TrustLevel` typed-discriminator newtype (RFC-0011-f §Trust Level
//! Canonical UUIDs).
//!
//! Per [[cipherocto-design-principles]] "Extension over enumeration (no
//! central enums)", `TrustLevel` is a `String` newtype wrapping a
//! canonical UUID discriminator (RFC-0871 typed-discriminator pattern).
//! New trust signals extend the canonical UUID table without substrate
//! or CLI enum edits; old code fails-closed on unknown discriminators
//! per [[cipherocto-design-principles]] "Open/Closed".

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Canonical UUIDs for the 3 trust levels defined by RFC-0011-f
/// §Trust Level Canonical UUIDs.
///
/// New trust signals extend the canonical UUID table WITHOUT
/// substrate or CLI enum edits (RFC-0011-f §Rationale "Why TrustLevel
/// is a typed-discriminator newtype (not a central enum)").
pub mod trust_level_uuids {
    /// Peer is trusted (RFC-0855p-c DomainCoordinator active for the
    /// peer — `GroupBinding::state = Bound` + `CoordinatorRecord.state =
    /// Active` — plus RFC-0871 envelope handshake history successful).
    /// No `DomainCoordinatorRecord` wrapper in substrate per RFC-0855p-c §2.
    pub const TRUSTED: &str = "urn:octo:trust-level:00000000-0000-0000-0000-000000000001";
    /// Peer is verified (RFC-0871 envelope handshake history successful
    /// without RFC-0855p-c DomainCoordinator presence).
    pub const VERIFIED: &str = "urn:octo:trust-level:00000000-0000-0000-0000-000000000002";
    /// Peer is untrusted (initial state on add — promotion to
    /// `VERIFIED` / `TRUSTED` happens via subsequent substrate signals,
    /// NOT via the CLI per RFC-0011-f §Rationale).
    pub const UNTRUSTED: &str = "urn:octo:trust-level:00000000-0000-0000-0000-000000000003";
}

/// Typed-discriminator wrapper around the canonical TrustLevel UUID
/// string.
///
/// Per RFC-0011-f §Rationale "Why TrustLevel enum is in the CLI (not
/// substrate)", the substrate carries the underlying signals (RFC-0855p-c
/// `GroupBinding` + `CoordinatorRecord` state + RFC-0871 envelope
/// handshake history) separately and the CLI is the canonical place to
/// aggregate them into an operator-friendly discriminator string. The
/// substrate exports this newtype so the CLI can pass UUIDs through
/// without inventing parallel enums. (RFC-0855p-c §2: no
/// `DomainCoordinatorRecord` wrapper in substrate.)
///
/// **Wave 4.5 finding 9 (Lens-4):** `Hash` was dropped from the
/// derive list — the derive is redundant noise for a `String`
/// newtype. The struct already has `PartialEq`/`Eq` via the
/// `String` field, and equality on `String` already implies equality
/// on the wrapper; callers that genuinely need a `HashSet<TrustLevel>`
/// can hash via `.as_uuid_str()` (or re-derive explicitly with a
/// documented intent).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct TrustLevel(pub String);

impl TrustLevel {
    /// Wrap a canonical UUID string.
    #[must_use]
    pub fn new(uuid: &'static str) -> Self {
        Self(uuid.to_string())
    }

    /// `Untrusted` — initial state on `peer add`.
    #[must_use]
    pub fn untrusted() -> Self {
        Self(trust_level_uuids::UNTRUSTED.to_string())
    }

    /// Return the canonical UUID string.
    #[must_use]
    pub fn as_uuid_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for TrustLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
