// SPDX-License-Identifier: MIT OR Apache-2.0
//
// Session 2 of the wacore-webauthn plan (RFC-0909): the integration seam
// between the WhatsApp Web adapter and a host-supplied WebAuthn authenticator
// for SHORTCAKE_PASSKEY.
//
// Trait surface mirrors upstream `whatsapp_rust::passkey::PasskeyAuthenticator`
// (in `src/passkey/mod.rs:115-117`) so a future re-export
// (`pub use whatsapp_rust::passkey::*;` + drop this module) is mechanical:
//   * supertrait `wacore::sync_marker::MaybeSendSync` (Send+Sync on native,
//     relaxed on wasm32 — same as the sibling extension points
//     `Transport` / `EventHandler`)
//   * `get_assertion(&&self, request: &&AssertionRequest)`
//   * `#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]` — so the
//     returned future is `Send` on native and `!Send` on wasm32 (a browser
//     authenticator may hold `!Send` JS handles)
//
// `CallbackAuthenticator` mirrors upstream (`src/passkey/mod.rs:135-159`):
// the closure takes **owned** `AssertionRequest` (it `.clone()`s internally
// for sync), and the supertrait bound on the closure is
// `wacore::sync_marker::MaybeSendSync`.

use super::assertion::{AssertionRequest, PasskeyError, UserVerification};
use async_trait::async_trait;
use std::future::Future;
use std::pin::Pin;

/// WebAuthn assertion result, packaged for the `<passkey_prologue>` IQ.
///
/// Field shape mirrors upstream `whatsapp_rust::passkey::Assertion`
/// (`src/passkey/mod.rs:88-99`). The standard
/// `PublicKeyCredential.authenticationResponseJson` is passed as raw UTF-8
/// bytes; the wacore flow packs the response into the protocol payload
/// verbatim.
#[derive(Debug, Clone)]
pub struct Assertion {
    /// UTF-8 JSON of `PublicKeyCredential.authenticationResponseJson`:
    /// `{id, rawId(b64url), type:"public-key", response:{clientDataJSON,
    /// authenticatorData, signature, userHandle}}`.
    pub assertion_json: Vec<u8>,
    /// Raw credential `rawId` bytes for `<credential_id>`.
    pub credential_id: Vec<u8>,
}

/// Future alias that mirrors upstream (`src/passkey/mod.rs:124-129`):
/// `Send` on native, relaxed on wasm32 where a browser authenticator's future
/// (e.g. awaiting `navigator.credentials.get`) is `!Send`.
#[cfg(not(target_arch = "wasm32"))]
pub type AssertionFuture =
    Pin<Box<dyn Future<Output = Result<Assertion, PasskeyError>> + Send + 'static>>;
#[cfg(target_arch = "wasm32")]
pub type AssertionFuture = Pin<Box<dyn Future<Output = Result<Assertion, PasskeyError>> + 'static>>;

// Mirror upstream's `AssertionCallback` alias (`src/passkey/mod.rs:131-134`):
// auto-trait `Send + Sync` on native (a closure that needs to cross threads),
// relaxed on wasm32 (a browser closure may capture `!Send` JS handles). Using
// `Send + Sync` directly — NOT `MaybeSendSync` — because the latter is a
// *named* trait, and named supertraits are not allowed in `dyn ... + ...`
// (E0225). The `MaybeSendSync` bound belongs on the constructor where-clause
// (see `CallbackAuthenticator::new` below), not on the `dyn` itself.
#[cfg(not(target_arch = "wasm32"))]
type AssertionCallback = dyn Fn(AssertionRequest) -> AssertionFuture + Send + Sync;
#[cfg(target_arch = "wasm32")]
type AssertionCallback = dyn Fn(AssertionRequest) -> AssertionFuture;

/// WebAuthn authenticator trait. The single pluggable point of the
/// SHORTCAKE_PASSKEY link flow.
///
/// Implementations:
///
/// * `CallbackAuthenticator` (below) — the host provides an async closure
///   (e.g. bridges to Android Credential Manager over JNI).
/// * (Future) `webauthn-authenticator-rs`-driven authenticator — see Session
///   5 of the plan. Marked OPTIONAL due to ban risk; not in this commit.
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
pub trait PasskeyAuthenticator: wacore::sync_marker::MaybeSendSync {
    async fn get_assertion(&self, request: &AssertionRequest) -> Result<Assertion, PasskeyError>;
}

/// Generic [`PasskeyAuthenticator`] that defers to a host-provided async
/// closure. Mirrors upstream `CallbackAuthenticator`
/// (`src/passkey/mod.rs:135-159`).
///
/// The closure takes **owned** `AssertionRequest` because the SDK may consume
/// the request twice (once for the SDK's auto-drive pass, once for any retry
/// path); a `&&AssertionRequest` would require the host to keep the request
/// alive across the await, which is awkward for an FFI bridge.
#[derive(Clone)]
pub struct CallbackAuthenticator {
    cb: Arc<AssertionCallback>,
}

impl CallbackAuthenticator {
    /// Build a `CallbackAuthenticator` from a host-supplied closure.
    ///
    /// The closure's `MaybeSendSync` bound (Send+Sync on native, relaxed on
    /// wasm32) mirrors upstream — necessary so the SDK can store it as
    /// `Arc<dyn PasskeyAuthenticator>` and drive it across threads.
    pub fn new<F>(f: F) -> Self
    where
        F: Fn(AssertionRequest) -> AssertionFuture + wacore::sync_marker::MaybeSendSync + 'static,
    {
        Self { cb: Arc::new(f) }
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
impl PasskeyAuthenticator for CallbackAuthenticator {
    async fn get_assertion(&self, request: &AssertionRequest) -> Result<Assertion, PasskeyError> {
        (self.cb)(request.clone()).await
    }
}

use std::sync::Arc;

// ── Upstream bridge ──────────────────────────────────────────────────
//
// The wacore SDK's `Client::set_passkey_authenticator` requires
// `Arc<dyn whatsapp_rust::passkey::PasskeyAuthenticator>`. Our public
// `PasskeyAuthenticator` trait is a downstream mirror (with the same shape)
// so call sites in `WhatsAppConfig` can hold an `Arc<dyn crate::passkey::…>`
// without forcing the host crate to import the upstream trait.
//
// `UpstreamBridge` is the newtype that adapts between the two. It's
// `pub(crate)` because the only consumer is `WhatsAppWebAdapter::start_bot`.
//
// Field mapping:
//   * `AssertionRequest` ↔ `whatsapp_rust::passkey::AssertionRequest`
//     (field shapes already match — see `assertion.rs` doc-comment)
//   * `Assertion` ↔ `whatsapp_rust::passkey::Assertion` (2 fields match)
//   * `PasskeyError` ↔ upstream's 5-variant `PasskeyError`: we collapse
//     `NoCredential` / `Cancelled` / `Backend` / `Flow` into `InvalidOptions`
//     (the closest downstream category) and propagate `InvalidOptions` /
//     `Upstream` verbatim. The exact variant doesn't matter for the SDK's
//     error path (`Flow(String)` accepts any message).
pub(crate) struct UpstreamBridge {
    inner: Arc<dyn PasskeyAuthenticator>,
}

impl UpstreamBridge {
    pub(crate) fn wrap(
        inner: Arc<dyn PasskeyAuthenticator>,
    ) -> Arc<dyn whatsapp_rust::passkey::PasskeyAuthenticator> {
        Arc::new(Self { inner })
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
impl whatsapp_rust::passkey::PasskeyAuthenticator for UpstreamBridge {
    async fn get_assertion(
        &self,
        request: &whatsapp_rust::passkey::AssertionRequest,
    ) -> Result<whatsapp_rust::passkey::Assertion, whatsapp_rust::passkey::PasskeyError> {
        // Bridge: convert the upstream request into our mirror, drive our
        // authenticator, then convert the result back. Field shapes match,
        // so the conversions are field-copy shims.
        let our_request = AssertionRequest {
            challenge: request.challenge.clone(),
            rp_id: request.rp_id.clone(),
            allow_credentials: request.allow_credentials.clone(),
            user_verification: match request.user_verification {
                whatsapp_rust::passkey::UserVerification::Required => UserVerification::Required,
                whatsapp_rust::passkey::UserVerification::Preferred => UserVerification::Preferred,
                whatsapp_rust::passkey::UserVerification::Discouraged => {
                    UserVerification::Discouraged
                }
            },
            timeout_ms: request.timeout_ms,
            raw_options_json: request.raw_options_json.clone(),
        };

        let our_result = self.inner.get_assertion(&our_request).await;

        our_result
            .map(|a| whatsapp_rust::passkey::Assertion {
                assertion_json: a.assertion_json,
                credential_id: a.credential_id,
            })
            .map_err(|e| match e {
                PasskeyError::InvalidOptions(s) => {
                    whatsapp_rust::passkey::PasskeyError::InvalidOptions(s)
                }
                // `Upstream(String)` is the catch-all for the SDK's perspective —
                // `Flow(String)` is the SDK's equivalent catch-all.
                PasskeyError::Upstream(s) => whatsapp_rust::passkey::PasskeyError::Flow(s),
                // Local-only variants. Map to the closest SDK category so the
                // SDK's error path doesn't blow up on an unmapped enum variant.
                PasskeyError::AssertionFailed(s) => {
                    whatsapp_rust::passkey::PasskeyError::Backend(s)
                }
                PasskeyError::NotRegistered => whatsapp_rust::passkey::PasskeyError::NoCredential,
                PasskeyError::Timeout(_d) => {
                    // The duration is dropped at the SDK boundary — upstream's
                    // `Cancelled` variant is a unit (no payload). The local
                    // `PasskeyError::Timeout(Duration)` surface preserves the
                    // deadline for host-side logs.
                    whatsapp_rust::passkey::PasskeyError::Cancelled
                }
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::passkey::assertion::{AssertionRequest, UserVerification};

    fn dummy_request() -> AssertionRequest {
        AssertionRequest {
            challenge: b"challenge".to_vec(),
            rp_id: Some("web.whatsapp.com".to_string()),
            allow_credentials: vec![],
            user_verification: UserVerification::Preferred,
            timeout_ms: Some(60_000),
            raw_options_json: "{}".to_string(),
        }
    }

    #[tokio::test]
    async fn callback_authenticator_drives_closure() {
        let auth = CallbackAuthenticator::new(|req: AssertionRequest| {
            Box::pin(async move {
                Ok(Assertion {
                    assertion_json: format!(r#"{{"rp_id":"{}"}}"#, req.rp_id.unwrap()).into_bytes(),
                    credential_id: req.challenge,
                })
            })
        });

        let req = dummy_request();
        let assertion = auth.get_assertion(&req).await.expect("must succeed");
        assert!(assertion.assertion_json.starts_with(b"{\"rp_id\":"));
        assert_eq!(assertion.credential_id, b"challenge");
    }

    #[tokio::test]
    async fn callback_authenticator_propagates_error() {
        let auth = CallbackAuthenticator::new(|_: AssertionRequest| {
            Box::pin(async { Err(PasskeyError::Upstream("simulated".to_string())) })
        });

        let err = auth
            .get_assertion(&dummy_request())
            .await
            .expect_err("must fail");
        assert!(matches!(err, PasskeyError::Upstream(_)));
    }
}
