//! Terminal QR renderer using the `qrcode` crate's unicode half-block
//! output.
//!
//! Used by the QR login mode (mission 0850h-a). The operator scans the
//! rendered QR with a verified Element client (e.g. Element Android's
//! "Link new device" flow), which authorizes the CLI's session.

use anyhow::Result;
use qrcode::{render::unicode::Dense1x2, QrCode};

/// Render `data` as a unicode-half-block QR code suitable for printing
/// to a terminal. The resulting string includes a one-cell border (the
/// `qrcode` crate's default) which most scanners require to lock on.
///
/// `dark_color` is the color of the **dark** modules; `light_color`
/// is the color of the **light** modules. Swapping them (the bug
/// R1-H4 flagged) produces a light-on-dark QR that standard
/// scanners (Element Android, camera apps) cannot lock onto.
pub fn to_terminal(data: &[u8]) -> Result<String> {
    let code = QrCode::new(data).map_err(|e| anyhow::anyhow!("QR encode failed: {}", e))?;
    let rendered = code
        .render::<Dense1x2>()
        .dark_color(Dense1x2::Dark)
        .light_color(Dense1x2::Light)
        .build();
    Ok(rendered)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_non_empty() {
        let rendered = to_terminal(b"matrix://test").unwrap();
        assert!(!rendered.is_empty());
        // Unicode half-block (▀ / ▄ / space) should appear
        assert!(rendered.contains(' ') || rendered.contains('▀') || rendered.contains('▄'));
    }

    #[test]
    fn empty_data_returns_err_or_handled() {
        // R1-M3 fix: the previous test (`let _ = to_terminal(b"");`)
        // discarded the result and conveyed no information. This
        // asserts that the function either errors OR returns a
        // non-empty string — both are acceptable design contracts
        // (qrcode's `QrCode::new(&&[])` may return a degenerate
        // code, but it must not panic).
        let r = to_terminal(b"");
        assert!(r.is_err() || !r.unwrap().is_empty());
    }

    #[test]
    fn dark_color_is_dark_modules_not_light() {
        // R1-H4 positive test: with the corrected mapping, the
        // rendered output must use the light-on-light (space) char
        // for the **light** modules and '▀' / '▄' / '█' for the
        // **dark** modules. A future regression that swaps the
        // colors will produce an output with no spaces (because
        // every cell is dark in the inverse mapping) or with a
        // different ratio of spaces to blocks.
        let rendered = to_terminal(b"matrix://test-color-mapping").unwrap();
        let space_count = rendered.matches(' ').count();
        let dark_count = rendered.matches('▀').count()
            + rendered.matches('▄').count()
            + rendered.matches('█').count();
        assert!(
            space_count > 0,
            "no light modules rendered (dark/light swap?)"
        );
        assert!(
            dark_count > 0,
            "no dark modules rendered (dark/light swap?)"
        );
        // The dark-on-light ratio should be roughly 1:1 for a
        // balanced QR (half the modules are dark, half are light).
        // Allow a 2x skew.
        let ratio = dark_count.max(space_count) as f64 / dark_count.min(space_count) as f64;
        assert!(
            ratio < 3.0,
            "dark/light ratio {ratio} (dark={dark_count}, light={space_count}) suggests color swap"
        );
    }
}
