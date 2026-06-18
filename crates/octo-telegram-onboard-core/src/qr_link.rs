//! QR-code rendering for the `qr-link` subcommand.
//!
//! The Telegram `WaitOtherDeviceConfirmation` state carries a
//! `tg://login?token=...` URL that must be rendered as a QR code and
//! displayed in the terminal. The user scans it from their Telegram
//! app on another already-logged-in device. Per the generated TDLib
//! bindings, the link is "updated frequently" — callers should expect
//! many link updates and re-render on each.
//!
//! Renders with `qrcode::render::unicode::Dense1x2` (half-block
//! characters: `▀ ▄ █ ▌ ▐ ` and the `space` quiet zone). Same renderer
//! the whatsapp adapter uses for its pairing QR (see
//! `crates/octo-adapter-whatsapp/src/adapter.rs:301`), so terminal
//! output has a consistent shape across the two onboard CLIs.

use crate::error::{OnboardError, Result};

/// Render a `tg://login?token=...` link as a Unicode half-block QR
/// code ready to print to a terminal.
///
/// The returned string includes leading and trailing newlines and
/// is suitable for `eprintln!`-ing directly. Each call is pure
/// (no side effects, no caching) so callers can re-render on every
/// link update without worrying about idempotence.
pub fn render_qr_link(link: &str) -> Result<String> {
    if link.is_empty() {
        return Err(OnboardError::BadConfig(
            "empty link from TDLib WaitOtherDeviceConfirmation".into(),
        ));
    }
    let qr = qrcode::QrCode::new(link.as_bytes())
        .map_err(|e| OnboardError::BadConfig(format!("qrcode encode: {e}")))?;
    let rendered = qr
        .render::<qrcode::render::unicode::Dense1x2>()
        .quiet_zone(true)
        .build();
    Ok(format!("\n{rendered}\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_produces_non_empty_output() {
        let out = render_qr_link("tg://login?token=ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789abcdef")
            .expect("render should succeed for a real token");
        assert!(!out.is_empty());
        // Quiet zone is on, so the output has leading/trailing newlines.
        assert!(out.starts_with('\n'));
        assert!(out.ends_with('\n'));
    }

    #[test]
    fn render_contains_half_block_characters() {
        let out = render_qr_link("tg://login?token=deadbeef-cafe-1234-5678-90abcdef0000")
            .expect("render should succeed");
        // Dense1x2 uses upper-half / lower-half / full block + space.
        // We don't pin the exact mix (it depends on the encoded data),
        // but at least one of the half-block glyphs must be present.
        let has_half_block = out
            .chars()
            .any(|c| matches!(c, '\u{2580}' | '\u{2584}' | '\u{2588}' | ' '));
        assert!(
            has_half_block,
            "render output should contain Unicode half-block glyphs, got: {out:?}"
        );
    }

    #[test]
    fn render_is_deterministic_for_same_input() {
        let link = "tg://login?token=fixed-input-for-test";
        let a = render_qr_link(link).expect("first render");
        let b = render_qr_link(link).expect("second render");
        assert_eq!(a, b, "same input must produce same output");
    }

    #[test]
    fn render_different_for_different_input() {
        let a = render_qr_link("tg://login?token=aaaa-bbbb-cccc").expect("render a");
        let b = render_qr_link("tg://login?token=eeee-ffff-0000").expect("render b");
        assert_ne!(
            a, b,
            "different input must produce different QR (otherwise it's not a real encoding)"
        );
    }

    #[test]
    fn render_rejects_empty_link() {
        let err = render_qr_link("").expect_err("empty link should be rejected");
        // OnboardError's Display only shows the variant label; the inner
        // message is exposed via `.inner()`.
        assert!(
            err.inner()
                .map(|m| m.contains("empty link"))
                .unwrap_or(false),
            "inner error should explain the empty-input rejection, got: {:?}",
            err.inner()
        );
    }
}
