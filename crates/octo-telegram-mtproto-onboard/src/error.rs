//! CLI-side error type. Re-exports the core's
//! `OnboardError` so the binary entry point can use a single
//! error type that bridges to `ExitCode`. The `exit_code()`
//! method lives in the core crate (orphan-rule friendly);
//! the CLI just calls it.

pub use octo_telegram_mtproto_onboard_core::error::OnboardError;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exit_codes_are_stable() {
        // The numeric codes are part of the CLI's
        // contract. Operators script against them, so any
        // change here is a breaking change. The
        // implementation lives on `OnboardError` itself
        // (in the core crate) so this is a smoke test.
        assert_eq!(OnboardError::InvalidInput("x".into()).exit_code(), 2);
        assert_eq!(OnboardError::Config("x".into()).exit_code(), 3);
        assert_eq!(OnboardError::Lifecycle { state: "x".into() }.exit_code(), 4);
        assert_eq!(OnboardError::NoSessionFile("x".into()).exit_code(), 4);
        assert_eq!(OnboardError::ChannelClosed("x".into()).exit_code(), 5);
        assert_eq!(OnboardError::Timeout("x".into()).exit_code(), 6);
        assert_eq!(OnboardError::TelegramApi("x".into()).exit_code(), 7);
        assert_eq!(OnboardError::Network("x".into()).exit_code(), 8);
    }
}
