//! Onboarding passthrough. The runtime does NOT auto-onboard; operators
//! always invoke `octo-whatsapp onboard qr-link|pair-link|...` themselves.
//! Phase 1: thin re-exports + command builders. No daemon is involved.

pub use octo_whatsapp_onboard_core::{
    wait_for_connected, CoreError, PairLinkArgs as CorePairLinkArgs, QrLinkArgs as CoreQrLinkArgs,
};

#[derive(Debug, Clone)]
pub enum OnboardCommand {
    QrLink { timeout_secs: u64 },
    PairLink { phone: String },
    Whoami,
    SessionList,
    SessionVerify { name: String },
    SessionRemove { name: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_construction() {
        let c = OnboardCommand::QrLink { timeout_secs: 120 };
        assert!(matches!(c, OnboardCommand::QrLink { timeout_secs: 120 }));
    }
}
