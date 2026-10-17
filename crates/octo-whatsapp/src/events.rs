//! Re-export of the typed `InboundEvent` enum that the adapter produces.
//!
//! Events first-class overhaul (plan
//! `docs/plans/2026-07-18-whatsapp-events-first-class-overhaul.md`).
//!
//! The actual enum + sub-enums + Debug-string parser helpers +
//! `discriminant_label` + `known_kinds` + `now_mono_ns` all live in
//! `octo-adapter-whatsapp::events` so the adapter can construct typed
//! events without a dependency cycle. This module re-exports every
//! public item so consumers continue to write
//! `crate::events::InboundEvent`, `crate::events::MessageKind`, etc.
//!
//! New wacore `Event` variants surface as [`InboundEvent::Unknown`] —
//! graceful observability, never a compile error. The catch-all path in
//! `octo-adapter-whatsapp/src/adapter.rs` emits `Unknown` for any
//! wacore variant we haven't yet projected.

pub use octo_adapter_whatsapp::events::*;
