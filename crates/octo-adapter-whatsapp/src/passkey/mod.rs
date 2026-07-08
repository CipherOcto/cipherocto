// SPDX-License-Identifier: MIT OR Apache-2.0
//
// Session 2 of the wacore-webauthn plan (RFC-0909): the `passkey` module is the
// integration seam between the WhatsApp Web adapter and a host-supplied
// WebAuthn authenticator (SHORTCAKE_PASSKEY).
//
// Architecture: this module owns a thin downstream copy of upstream's
// `PasskeyAuthenticator` trait so the adapter doesn't have to take a direct
// dependency on `whatsapp_rust::passkey` for a stable type. A future migration
// to `pub use whatsapp_rust::passkey::*;` is one line; the field shapes already
// match upstream (see `assertion.rs` doc-comment and `authenticator.rs`
// doc-comment for the cross-reference).
//
// `AssertionRequest` / `PasskeyError` here intentionally diverge from upstream's
// 5-variant `PasskeyError` (`NoCredential` / `Cancelled` / `InvalidOptions` /
// `Backend` / `Flow`) — downstream callers don't need to distinguish the
// upstream categories, so we collapse them into a single `Upstream(String)` to
// keep the enum small. The mapping from downstream to upstream happens inline
// in `authenticator.rs::UpstreamBridge::get_assertion` (the bridge is the only
// site that talks to the SDK).
//
// Lifecycle:
//   * `assertion::AssertionRequest::parse(json)` — parse the server's
//     `<passkey_request_options>` payload.
//   * `authenticator::PasskeyAuthenticator::get_assertion` — produce an
//     `Assertion` (or surface an `Upstream` error).
//   * `WhatsAppWebAdapter::start_bot` calls
//     `bot.client().set_passkey_authenticator(auth).await` between
//     `builder.build()` and `bot.spawn()` so the SDK auto-drives the assertion
//     step (or, with `None`, emits `Event::PairPasskeyRequest` for the host).

pub mod assertion;
pub mod authenticator;

pub use assertion::{AssertionRequest, PasskeyError, UserVerification};
pub use authenticator::{Assertion, AssertionFuture, CallbackAuthenticator, PasskeyAuthenticator};
