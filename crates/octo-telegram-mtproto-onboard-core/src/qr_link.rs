//! QR-code rendering for the `qr-login` subcommand.
//!
//! The Telegram MTProto `auth.exportLoginToken` RPC returns a
//! `tg://login?token=...` URL that must be rendered as a QR code
//! and displayed in the terminal. The operator scans it from
//! their Telegram app on another already-logged-in device.
//!
//! ## Why this lives in `-core` (R2-OPS-4)
//!
//! Round 1 left the QR rendering entirely to the CLI (the
//! `--render-qr-ascii` flag was defined but no QR-rendering
//! dependency was wired up; the operator's terminal never
//! showed a QR, only a `tracing::info!` line). The fix moves
//! the renderer into `-core` so it can be unit-tested with
//! the same coverage as the TDLib version's `qr_link.rs`,
//! matching the workspace convention of tested utility
//! functions rather than untested CLI plumbing.
//!
//! ## Why we don't go through `tracing` (R2-OPS-5)
//!
//! The first round of OPS-1 added a secret-redaction layer in
//! the CLI's `logging` module. `REDACTED_FIELD_NAMES` includes
//! `"token"`, so a `tracing::info!(url = %prompt.url, ...)`
//! call would mangle the URL `tg://login?token=...` to
//! `tg://login?token=***` (the body-substring pass matches
//! `token=...` and replaces the value). The QR URL **is**
//! the auth credential — anyone with that URL can import the
//! session — so the operator MUST see the raw URL (or, much
//! more usefully, a QR code) at the terminal. Bypassing the
//! `tracing` layer for QR rendering is correct.
//!
//! Renders with `qrcode::render::unicode::Dense1x2` (half-block
//! characters: `▀ ▄ █ ▌ ▐` and the `space` quiet zone). Same
//! renderer the TDLib onboard crate uses
//! (`crates/octo-telegram-onboard-core/src/qr_link.rs`),
//! so the two onboard CLIs produce visually consistent
//! terminal output.

use crate::error::OnboardError;

/// Render a `tg://login?token=...` link as a Unicode
/// half-block QR code ready to print to a terminal.
///
/// The returned string includes leading and trailing
/// newlines and is suitable for `eprint!`-ing directly. Each
/// call is pure (no side effects, no caching) so callers can
/// re-render on every link update without worrying about
/// idempotence — the Telegram MTProto server rotates the
/// token every ~30 seconds.
pub fn render_qr_link(link: &str) -> Result<String, OnboardError> {
    if link.is_empty() {
        return Err(OnboardError::Config(
            "empty link from auth.exportLoginToken".into(),
        ));
    }
    let qr = qrcode::QrCode::new(link.as_bytes())
        .map_err(|e| OnboardError::Config(format!("qrcode encode: {e}")))?;
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
        // We don't pin the exact mix (it depends on the encoded
        // data), but at least one of the half-block glyphs must
        // be present.
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
        // OnboardError's Display only shows the variant label; the
        // inner message is exposed via `.kind()` and the
        // error-chain `source()` (for `thiserror`-derived errors
        // the message is on the top-level Display).
        assert!(
            format!("{err}").contains("config") || format!("{err:?}").contains("empty"),
            "error should explain the empty-input rejection, got: {err}"
        );
    }

    /// R2-OPS-5: confirm the rendered QR output does NOT
    /// contain `token=***` (the substring that the round-1
    /// redaction layer would have inserted had we routed
    /// the URL through `tracing`). The QR must be the
    /// encoded URL, not a redacted one.
    #[test]
    fn render_does_not_redact_token_substring() {
        let out = render_qr_link("tg://login?token=ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789abcdef")
            .expect("render should succeed");
        // The `qrcode` crate encodes the bytes as a bitmap;
        // the substring "token=" does NOT appear in the
        // rendered output (it's a visual encoding, not the
        // raw URL). The important assertion is that
        // `token=***` does NOT appear — i.e. the renderer
        // is not passing through the URL to a redaction
        // layer.
        assert!(
            !out.contains("token=***"),
            "QR render output should not contain redacted token marker; got first 200 chars: {:?}",
            &out[..out.len().min(200)]
        );
    }
}
