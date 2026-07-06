//! Triggers registry. Phase 4 of
//! `docs/plans/2026-07-04-whatsapp-runtime-cli-mcp-design.md`
//! §Triggers (stateful agent targets).
//!
//! Submodules:
//! - [`trigger`] — `Trigger` struct + `RunnerSpec` + `RunRecord`.
//! - [`registry`] — `TriggerStore` with `ArcSwap<Triggerset>`, CRUD,
//!   `run(id, event)` that records synthetic outcomes (full dispatch
//!   wired in Part C).

pub mod registry;
pub mod trigger;

// Public re-exports — keep the surface narrow.
pub use registry::{TriggerDraft, TriggerError, TriggerPatch, TriggerStore, Triggerset};
pub use trigger::{RateLimit, RunRecord, RunnerSpec, Trigger};

// Backwards-compat: the Phase 1 stub used `TriggersView` for the
// read-only `triggers.list|get` RPC. Phase 4 keeps the alias for old
// tests.
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct TriggersView {
    pub triggers: Vec<Trigger>,
}

impl TriggersView {
    pub fn empty() -> Self {
        Self {
            triggers: Vec::new(),
        }
    }
    pub fn list(&self) -> &[Trigger] {
        &self.triggers
    }
    pub fn get(&self, _id: &str) -> Option<&Trigger> {
        None
    }
}
