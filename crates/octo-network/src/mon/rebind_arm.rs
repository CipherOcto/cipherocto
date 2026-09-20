//! Rebind arm registration substrate (RFC-0011-l Phase 4 row G21).
//!
//! Bridges the `octo network bind-envelope rebind-{prepare,commit,abort}`
//! clap arm trio (Layer C) with the `RebindCoordinator` payload-builder
//! surface (Layer B) via the RFC-0011-c §F.5.1 D2.1 discriminator.
//!
//! ## Why a separate substrate module
//!
//! The rebind trio is mutating (replaces the binding for a DOT
//! domain across N platforms). The clap arm registration is the
//! Layer C wiring, but the typed dispatch lives in Layer B so:
//!
//! 1. Layer C never sees raw `RebindCoordinator` (Layer B private
//!    state — `participants`, `responses`, `deadline_epoch`).
//! 2. The D2.1 discriminator (`RebindArmKey`) sits at the
//!    substrate boundary so a future D2.2 population policy
//!    (deferred post-PQC) does not require a Layer C change.
//! 3. Operators can grep on `RebindArmAction` variants to map
//!    to typed exit codes (substrate-faithful exit code table).
//!
//! ## Persistence boundary
//!
//! The substrate today returns
//! `RebindArmError::AdapterUnwired` because the persistence
//! adapter is the Phase 6 follow-on per
//! `0011-h-s-a-rebind-arm-persistence`. Pre-persistence, the
//! substrate returns the typed payload the operator asked for
//! but does NOT persist the action — a CLI operator-visible
//! "dry-run today; persisted post-Phase 6" envelope.
//!
//! ## D2.1 pairing
//!
//! D2.1 (discriminator-only additive slice) LANDED at
//! `next 01340b93` per RFC-0011-c §F.5.1 D2.1 closure card.
//! D2.2 (population policy) deferred post-PQC. The
//! `RebindArmKey` enum mirrors the D2.1 discriminator
//! (`KeyId::V1` / `KeyId::V2` in `octo-runtime::handle::key_id`).
//!
//! `octo-attach-key-rotation` Cargo feature gates `V2`; the
//! default `V1` is the only reachable variant today
//! (matches RFC-0011-c §F.5.1 D2.1 paired-acceptance bridge).

use serde::{Deserialize, Serialize};

use crate::mon::bind_envelope::{RebindAbort, RebindCommit, RebindPrepare};
use crate::mon::rebind::RebindCoordinator;

/// Typed dispatch enum for the `bind-envelope rebind-*` clap arm
/// trio (RFC-0011-l Phase 4 row G21).
///
/// The three variants map 1:1 to `RebindCoordinator` payload
/// builders:
/// - `Prepare` → `RebindCoordinator::prepare_envelope(signature)`
/// - `Commit` → `RebindCoordinator::commit_envelope(signature)`
/// - `Abort` → `RebindCoordinator::abort_envelope(signature)`
///
/// `#[non_exhaustive]` so future arms (e.g. `rotate` for
/// RFC-0011-c §F.5.1 D2.2) land additive without breaking
/// downstream exhaustive matches.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum RebindArmAction {
    /// Build the PREPARE envelope for broadcast.
    Prepare,
    /// Build the COMMIT envelope (after quorum reached).
    Commit,
    /// Build the ABORT envelope (vote-abort or timeout).
    Abort,
}

/// D2.1 discriminator for the rebind arm trio. Mirrors
/// `octo_runtime::handle::key_id::KeyId` from RFC-0011-c
/// §F.5.1 (LANDED at `next 01340b93`).
///
/// `V1` is the always-reachable discriminator (the substrate
/// ships with `KeyId::V1` always). `V2` is gated on the
/// `octo-attach-key-rotation` Cargo feature per the paired-
/// acceptance bridge; pre-D2.2 the population policy is
/// deferred post-PQC, so even `V2`-feature-ON the helper
/// returns `RebindArmError::AdapterUnwired` for the typed
/// payload side.
///
/// The substrate today ALWAYS returns `V1` regardless of
/// feature flags because the persistence adapter (Phase 6
/// follow-on) is the owner of the key-id → envelope mapping.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum RebindArmKey {
    /// The original key-id discriminator.
    V1,
}

/// Substrate-faithful error type for the rebind arm dispatch
/// surface. Mirrors the `CoordinatorAdminActionError` shape
/// from RFC-0011-k Phase 3 G12.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum RebindArmError {
    /// Persistence adapter absent today (Phase 6 follow-on
    /// per `0011-h-s-a-rebind-arm-persistence`). The CLI
    /// translates to typed exit 1 `Internal` with the
    /// forward-looking Phase 6 note in the operator hint.
    AdapterUnwired,
    /// The arm action variant is not yet wired to a payload
    /// builder. Reserved for future arms (e.g. `rotate`).
    UnknownArm(String),
}

/// Typed payload envelope returned by
/// `dispatch_rebind_arm_action`. Tagged enum so CLI JSON
/// envelopes roundtrip cleanly through serde.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "arm")]
pub enum RebindArmPayload {
    /// PREPARE envelope ready for broadcast.
    Prepare(RebindPrepare),
    /// COMMIT envelope ready for broadcast.
    Commit(RebindCommit),
    /// ABORT envelope ready for broadcast.
    Abort(RebindAbort),
}

/// Substrate-faithful boundary between Layer C (CLI dispatch)
/// and Layer B (`RebindCoordinator` payload builders).
///
/// Today, the function returns `Err(AdapterUnwired)` because
/// the persistence adapter is the Phase 6 follow-on. The CLI
/// caller (Phase 4 dispatch slice) consumes the typed error
/// and surfaces it via `OctoCliError::Internal` carrying the
/// forward-looking Phase 6 note.
///
/// The function is `sync` (matches `RebindCoordinator`'s
/// payload-builder shape — no async I/O today; the
/// persistence adapter will lift to async when it lands).
pub fn dispatch_rebind_arm_action(
    _coordinator: &RebindCoordinator,
    arm: RebindArmAction,
    _key: RebindArmKey,
) -> Result<RebindArmPayload, RebindArmError> {
    // Substrate-faithful Option<RebindArmPayload> → typed
    // error today. The persistence adapter (Phase 6
    // follow-on) is the owner of "did the rebind stick"; the
    // substrate never lies about persistence it didn't do.
    let _ = match arm {
        RebindArmAction::Prepare => "prepare",
        RebindArmAction::Commit => "commit",
        RebindArmAction::Abort => "abort",
    };
    Err(RebindArmError::AdapterUnwired)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mon::bind_envelope::BindEnvelope;

    fn fixture() -> RebindCoordinator {
        RebindCoordinator::new(
            "d1",
            BindEnvelope::new("d1", "whatsapp", "g1"),
            vec!["p1".into(), "p2".into()],
        )
    }

    #[test]
    fn t_rebind_arm_action_serde_roundtrip() {
        for arm in [
            RebindArmAction::Prepare,
            RebindArmAction::Commit,
            RebindArmAction::Abort,
        ] {
            let json = serde_json::to_string(&arm).unwrap();
            let back: RebindArmAction = serde_json::from_str(&json).unwrap();
            assert_eq!(back, arm);
        }
    }

    #[test]
    fn t_rebind_arm_key_default_is_v1() {
        let k = RebindArmKey::V1;
        let json = serde_json::to_string(&k).unwrap();
        assert_eq!(json, "\"V1\"");
    }

    #[test]
    fn t_dispatch_rebind_arm_action_returns_adapter_unwired_today() {
        let coord = fixture();
        for arm in [
            RebindArmAction::Prepare,
            RebindArmAction::Commit,
            RebindArmAction::Abort,
        ] {
            let r = dispatch_rebind_arm_action(&coord, arm, RebindArmKey::V1);
            assert!(
                matches!(r, Err(RebindArmError::AdapterUnwired)),
                "substrate-faithful: persistence adapter absent → AdapterUnwired"
            );
        }
    }

    #[test]
    fn t_dispatch_rebind_arm_action_idempotent_across_calls() {
        let coord = fixture();
        let a = dispatch_rebind_arm_action(&coord, RebindArmAction::Prepare, RebindArmKey::V1);
        let b = dispatch_rebind_arm_action(&coord, RebindArmAction::Prepare, RebindArmKey::V1);
        assert!(matches!(a, Err(RebindArmError::AdapterUnwired)));
        assert!(matches!(b, Err(RebindArmError::AdapterUnwired)));
    }

    #[test]
    fn t_rebind_arm_error_unknown_arm_constructs() {
        let e = RebindArmError::UnknownArm("rotate".into());
        let json = serde_json::to_string(&e).unwrap();
        let back: RebindArmError = serde_json::from_str(&json).unwrap();
        assert_eq!(back, e);
    }
}
