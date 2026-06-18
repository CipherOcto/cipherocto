//! WhatsApp Web adapter for DOT (RFC-0850 §8.1, PlatformType::WhatsApp)
//!
//! Bridges DOT envelopes to WhatsApp groups via the native WhatsApp Web protocol
//! using whatsapp-rust. No Meta Business verification required.
//!
//! ## Configuration
//!
//! ```json
//! {
//!   "session_path": "~/.cipherocto/whatsapp-session.db",
//!   "pair_phone": "15551234567",
//!   "groups": ["120363012345678901"]
//! }
//! ```

pub mod adapter;
pub mod state;
pub mod store;

pub use adapter::{CreateGroupOutput, WhatsAppConfig, WhatsAppWebAdapter};
pub use state::{BotState, LoggedOutCause};
pub use store::StoolapStore;
// Re-export the whatsapp-rust types that the e2e group-setup test references
// directly. Keeping the re-exports centralised here means callers (and the
// test) don't need a direct `whatsapp-rust` dependency on their dev-deps
// just to spell out a `CreateGroupOutput.metadata.participants: Vec<GroupParticipant>`.
pub use whatsapp_rust::{GroupMetadata, GroupParticipant, ParticipantChangeResponse};

// ── Plugin ABI ─────────────────────────────────────────────────────

#[no_mangle]
pub extern "C" fn adapter_version() -> u32 {
    1
}

#[no_mangle]
pub extern "C" fn platform_type() -> u16 {
    0x0008
}

/// Create a new adapter from JSON config bytes.
///
/// # Safety
///
/// `config` must point to a valid buffer of at least `config_len` bytes.
/// Returns null on invalid config. Caller must call `destroy_adapter` to free.
#[no_mangle]
pub unsafe extern "C" fn create_adapter(config: *const u8, config_len: usize) -> *mut () {
    if config.is_null() || config_len == 0 {
        return std::ptr::null_mut();
    }
    let bytes = std::slice::from_raw_parts(config, config_len);
    match WhatsAppWebAdapter::from_config_bytes(bytes) {
        Ok(a) => Box::into_raw(Box::new(a)) as *mut (),
        Err(_) => std::ptr::null_mut(),
    }
}

/// Destroy an adapter created by `create_adapter`.
///
/// # Safety
///
/// `adapter` must be a pointer previously returned by `create_adapter`.
/// Must not be called more than once for the same pointer.
#[no_mangle]
pub unsafe extern "C" fn destroy_adapter(adapter: *mut ()) {
    if !adapter.is_null() {
        let _ = Box::from_raw(adapter as *mut WhatsAppWebAdapter);
    }
}
