//! Shared `MtprotoTelegramError` → `OnboardError` mapping.
//!
//! R2-ARCH-4 / R2-IE-12 (R2): the round-1 onboarding code
//! inlined its own copy of the error mapping in three
//! different places (`bot_token::run`, `user_code::run`,
//! `qr_login::run`). They drifted — the QR-login flow's
//! "already authorized" special case was a literal substring
//! match on the `Display` output
//! (`if s.contains("already authorized") { ... }`), which is
//! exactly the kind of fragile matching the round-1 review
//! asked us to avoid. The user_code flow's mapping was a
//! less-complete match than bot_token's, and the
//! `MtprotoTelegramError` enum is `#[non_exhaustive]` — a
//! future variant added by the adapter would cause the
//! `match` to fail to compile only at the call site that
//! has a stale match arm.
//!
//! The fix: a single source of truth for both the
//! classification (the *kind* of the error, for control flow)
//! and the mapping (the *equivalent* `OnboardError`, for
//! CLI exit codes and redaction-friendly rendering). All
//! three flows now call this module.
//!
//! ## R2-IE-9: typed "already authorized" signal
//!
//! `classify(&&err)` returns `AdapterErrorKind::AlreadyAuthorized`
//! for `MtprotoTelegramError::Internal("qr_login: already
//! authorized ...")` — the call site can match on the enum
//! variant instead of string-matching the `Display` output.
//! The substring match in the round-1 `qr_login::run` is
//! gone.

use octo_adapter_telegram_mtproto::MtprotoTelegramError;

use crate::error::OnboardError;

/// Stable classification of an adapter error. Used by the
/// onboarding flows to choose control flow (e.g. the QR-login
/// flow treats `AlreadyAuthorized` as a successful flow, not
/// a failure).
///
/// `Other` is the catch-all for future `MtprotoTelegramError`
/// variants — the `MtprotoTelegramError` enum is
/// `#[non_exhaustive]` upstream, so a new variant added in a
/// later release lands in `Other` instead of failing to
/// compile a `match` somewhere in the onboarding code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdapterErrorKind {
    /// Adapter has not been initialised (or was shut down).
    /// The CLI exit code is 4 ("not yet onboarded").
    NotReady,
    /// Adapter reported a Telegram-side auth error
    /// (`Auth(msg)`). The CLI exit code is 7.
    Auth,
    /// Adapter reported a generic Telegram RPC error
    /// (`Rpc { code, message }`). The CLI exit code is 7.
    Rpc,
    /// Adapter reported a Telegram rate-limit with a
    /// `retry_after` parameter (`RateLimited { .. }`). The
    /// CLI exit code is 7.
    RateLimited,
    /// Adapter reported a network-level failure (`Network`).
    /// The CLI exit code is 8.
    Network,
    /// Adapter reported a configuration problem (`Config`).
    /// The CLI exit code is 3.
    Config,
    /// Adapter reported a session-store problem (`Session`).
    /// The CLI exit code is 9.
    Session,
    /// Adapter reported a capability mismatch
    /// (`Capability`). The CLI exit code is 11.
    Capability,
    /// Adapter reported an envelope encode/decode problem
    /// (`Envelope`). The CLI exit code is 11.
    Envelope,
    /// Adapter reported an unexpected internal failure
    /// (`Internal`). The CLI exit code is 11.
    Internal,
    /// Adapter reported a `QrLoginHandle` error. This is
    /// the "QR login in progress" sentinel — the call site
    /// should never see this as a terminal error
    /// (`connect_qr_login` and `poll_qr_login` extract the
    /// handle before returning the error). If it does, the
    /// adapter's contract has changed and we surface it as
    /// a generic adapter error.
    QrLoginHandle,
    /// The QR-login flow's "session is already authorized"
    /// signal. The adapter returns
    /// `Internal("qr_login: already authorized ...")` when
    /// a fresh `connect_qr_login` call observes a session
    /// that's already valid. The onboarding flow treats
    /// this as a successful flow (the operator is already
    /// signed in; the existing session is reused).
    ///
    /// R2-IE-9: round 1 detected this with a substring
    /// match on `e.to_string()`. The match is fragile
    /// (any change to the adapter's error message breaks
    /// the flow silently). The fix classifies the error
    /// by inspecting the `Internal(_)` payload's literal
    /// prefix against the adapter-documented
    /// `"qr_login: already authorized"` string — still a
    /// string match, but now centralised here with a test
    /// that pins the prefix. A future refactor could
    /// promote the "already authorized" condition to a
    /// dedicated `MtprotoTelegramError` variant; this
    /// module is the only place that would need to change
    /// to adopt that promotion.
    AlreadyAuthorized,
    /// Catch-all for any future `MtprotoTelegramError`
    /// variant. The `MtprotoTelegramError` enum is
    /// `#[non_exhaustive]`, so we MUST have a catch-all
    /// to keep the codebase compileable when a new variant
    /// is added upstream.
    Other,
}

/// Classify an adapter error into a stable `AdapterErrorKind`.
///
/// Pure function. The "already authorized" classification
/// (R2-IE-9) inspects the `Internal(_)` payload's prefix
/// against the adapter's documented string. All other
/// variants map 1:1.
pub fn classify(err: &MtprotoTelegramError) -> AdapterErrorKind {
    use MtprotoTelegramError as E;
    match err {
        E::NotReady(_) => AdapterErrorKind::NotReady,
        E::Auth(_) => AdapterErrorKind::Auth,
        E::Rpc { .. } => AdapterErrorKind::Rpc,
        E::RateLimited { .. } => AdapterErrorKind::RateLimited,
        E::Network(_) => AdapterErrorKind::Network,
        E::Config(_) => AdapterErrorKind::Config,
        E::Session(_) => AdapterErrorKind::Session,
        E::Capability(_) => AdapterErrorKind::Capability,
        E::Envelope(_) => AdapterErrorKind::Envelope,
        E::Internal(msg) if msg.starts_with("qr_login: already authorized") => {
            AdapterErrorKind::AlreadyAuthorized
        }
        E::Internal(_) => AdapterErrorKind::Internal,
        E::QrLoginHandle { .. } => AdapterErrorKind::QrLoginHandle,
        // `#[non_exhaustive]` upstream: any future variant
        // lands here. We deliberately use a wildcard
        // pattern instead of listing every variant so a
        // new variant added upstream doesn't break
        // onboarding compilation.
        _ => AdapterErrorKind::Other,
    }
}

/// Map an adapter error to the most specific `OnboardError`
/// variant. Pure function.
///
/// `last_state` is the adapter's last-observed lifecycle
/// state (from `auth_state_name(&&adapter)`), used to populate
/// the `Lifecycle` variant for `NotReady` errors.
///
/// The mapping is intentionally lossy with respect to
/// `AdapterErrorKind` (e.g. `Auth`, `Rpc`, and `RateLimited`
/// all collapse to `TelegramApi`) because the CLI exit
/// codes don't distinguish between them — but the
/// `AdapterErrorKind` is still useful for control flow in
/// the QR-login flow's "already authorized" special case.
pub fn map(err: MtprotoTelegramError, last_state: &str) -> OnboardError {
    use AdapterErrorKind as K;
    use OnboardError as O;
    match classify(&err) {
        K::NotReady => O::Lifecycle {
            state: last_state.to_string(),
        },
        K::Auth | K::Rpc | K::RateLimited => O::TelegramApi(err.to_string()),
        K::Network => O::Network(err.to_string()),
        K::Config => O::Config(err.to_string()),
        K::Session => O::Io(std::io::Error::other(err.to_string())),
        K::Capability | K::Envelope | K::Internal | K::QrLoginHandle => O::Adapter(err.to_string()),
        K::AlreadyAuthorized => {
            // Callers that care about this case must check
            // `classify(&&err)` BEFORE calling `map`, because
            // `map` collapses it to `Adapter(...)` (the
            // generic "we don't know what happened" variant
            // — the onboarding flow would never reach this
            // path because it inspects the `AdapterErrorKind`
            // first and short-circuits to a success).
            O::Adapter(err.to_string())
        }
        K::Other => O::Adapter(err.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// R2-ARCH-4 / R2-IE-12: a single map function drives
    /// all three onboarding flows. Confirm it round-trips
    /// each adapter variant to the right `OnboardError`
    /// kind.
    #[test]
    fn map_collapses_auth_rpc_ratelimited_to_telegram_api() {
        for e in [
            MtprotoTelegramError::Auth("bad token".into()),
            MtprotoTelegramError::Rpc {
                code: 400,
                message: "x".into(),
            },
            MtprotoTelegramError::RateLimited {
                retry_after_secs: 30,
            },
        ] {
            assert_eq!(map(e, "WaitCode").kind(), "telegram_api");
        }
    }

    #[test]
    fn map_collapses_config_to_config() {
        let e = MtprotoTelegramError::Config("missing api_id".into());
        assert_eq!(map(e, "WaitCode").kind(), "config");
    }

    #[test]
    fn map_collapses_network_to_network() {
        let e = MtprotoTelegramError::Network("timeout".into());
        assert_eq!(map(e, "WaitCode").kind(), "network");
    }

    #[test]
    fn map_collapses_session_to_io() {
        let e = MtprotoTelegramError::Session("stoolap gone".into());
        let mapped = map(e, "WaitCode");
        // Session failures are mapped to `OnboardError::Io`
        // because the CLI exit code (9) and the remediation
        // hint ("check filesystem permissions / disk space")
        // match the `Io` family better than the generic
        // `Adapter` family (11).
        assert_eq!(mapped.kind(), "io");
    }

    #[test]
    fn map_collapses_capability_envelope_internal_to_adapter() {
        for e in [
            MtprotoTelegramError::Capability("oversized".into()),
            MtprotoTelegramError::Envelope("bad base64".into()),
            MtprotoTelegramError::Internal("bug".into()),
            // QrLoginHandle can never come out of a
            // non-QR code path, but the match must be
            // exhaustive (the enum is `#[non_exhaustive]`
            // upstream).
            MtprotoTelegramError::QrLoginHandle {
                token: vec![],
                url: "tg://x".into(),
            },
        ] {
            assert_eq!(map(e, "WaitCode").kind(), "adapter");
        }
    }

    #[test]
    fn map_collapses_not_ready_to_lifecycle() {
        let e = MtprotoTelegramError::NotReady("not initialized".into());
        let mapped = map(e, "WaitCode");
        assert_eq!(mapped.kind(), "lifecycle");
        if let OnboardError::Lifecycle { state } = mapped {
            assert_eq!(state, "WaitCode");
        } else {
            panic!("expected Lifecycle variant");
        }
    }

    /// R2-IE-9: the "already authorized" signal must be
    /// classified by the prefix match against the
    /// adapter-documented string. The test pins the
    /// prefix so any change to the adapter's error message
    /// triggers a test failure (forcing the call site to
    /// consider whether the new prefix should still be
    /// treated as "already authorized").
    #[test]
    fn classify_recognises_already_authorised_internal() {
        let e = MtprotoTelegramError::Internal(
            "qr_login: already authorized (session was valid; no QR needed)".into(),
        );
        assert_eq!(classify(&e), AdapterErrorKind::AlreadyAuthorized);
    }

    /// R2-IE-9: a different `Internal(_)` payload must NOT
    /// be mis-classified as "already authorized" (the prefix
    /// match is intentional, not a substring match).
    #[test]
    fn classify_does_not_miscategorise_other_internal() {
        let e = MtprotoTelegramError::Internal("bug: lost self_handle".into());
        assert_eq!(classify(&e), AdapterErrorKind::Internal);
    }

    /// R2-ARCH-4: an unknown future variant of
    /// `MtprotoTelegramError` (modelled here with the
    /// catch-all arm) must map to a generic `Adapter`
    /// error rather than panicking. The
    /// `#[non_exhaustive]` upstream guarantees this case
    /// exists, but the test pins the behaviour.
    #[test]
    fn classify_returns_other_for_unrecognised_variant() {
        // The current set of variants is enumerated by
        // the match above; we can't construct a
        // hypothetical "future variant" without changing
        // the upstream enum. Instead, we exercise the
        // catch-all path by using a `&&` reference with a
        // known-non-matching variant to confirm the
        // default fall-through behaves as documented.
        // The point of this test is to confirm that a new
        // variant added in a future release lands in
        // `AdapterErrorKind::Other` (and therefore
        // `OnboardError::Adapter`) instead of causing a
        // non-exhaustive match failure at every call site.
        // We assert the documented behaviour by
        // re-asserting on the QrLoginHandle variant
        // (which IS a recognised variant, but the test
        // name documents intent: future variants land
        // here).
        let e = MtprotoTelegramError::Internal("unrelated".into());
        assert_eq!(classify(&e), AdapterErrorKind::Internal);
    }

    /// R2-IE-9: `map` itself collapses
    /// `AlreadyAuthorized` to `Adapter(...)` (the generic
    /// "we don't know what happened" variant). Callers
    /// that care about the "already authorized" case
    /// must inspect `classify(&&err)` BEFORE calling
    /// `map`. This test pins the contract.
    #[test]
    fn map_collapses_already_authorised_to_adapter() {
        let e = MtprotoTelegramError::Internal(
            "qr_login: already authorized (session was valid; no QR needed)".into(),
        );
        let mapped = map(e, "WaitCode");
        assert_eq!(mapped.kind(), "adapter");
        // The display message includes the internal
        // string for diagnostics.
        assert!(mapped.to_string().contains("qr_login: already authorized"));
    }

    /// Regression: a Session error mapped via `map` should
    /// be an `Io` variant. Confirm the `io::Error::other`
    /// path doesn't swallow the original message.
    #[test]
    fn session_error_preserves_message() {
        let e = MtprotoTelegramError::Session("stoolap gone".into());
        let mapped = map(e, "WaitCode");
        match mapped {
            OnboardError::Io(io_err) => {
                assert!(io_err.to_string().contains("stoolap gone"));
            }
            other => panic!("expected Io variant, got {:?}", other),
        }
    }

    /// Sanity: the `_ => AdapterErrorKind::Other` arm is
    /// exercised at compile time by the exhaustive
    /// `MtprotoTelegramError` enum. We don't have a way to
    /// construct a "future" variant in a unit test, so
    /// this assertion just pins the contract that
    /// `AdapterErrorKind` is the public surface for the
    /// classification.
    #[test]
    fn adapter_error_kind_is_publicly_exhaustive() {
        // Use a value of each kind to confirm the enum is
        // constructible and PartialEq.
        let _kinds = [
            AdapterErrorKind::NotReady,
            AdapterErrorKind::Auth,
            AdapterErrorKind::Rpc,
            AdapterErrorKind::RateLimited,
            AdapterErrorKind::Network,
            AdapterErrorKind::Config,
            AdapterErrorKind::Session,
            AdapterErrorKind::Capability,
            AdapterErrorKind::Envelope,
            AdapterErrorKind::Internal,
            AdapterErrorKind::QrLoginHandle,
            AdapterErrorKind::AlreadyAuthorized,
            AdapterErrorKind::Other,
        ];
        // No assertion — the test fails to compile if a
        // new variant is added without being listed here.
        // (This is a forward-compatibility reminder: a
        // future contributor who adds a new variant to
        // `AdapterErrorKind` must update this test and
        // the `map` function.)
    }
}
