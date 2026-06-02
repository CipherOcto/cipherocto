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
pub fn to_terminal(data: &[u8]) -> Result<String> {
    let code = QrCode::new(data).map_err(|e| anyhow::anyhow!("QR encode failed: {}", e))?;
    let rendered = code
        .render::<Dense1x2>()
        .dark_color(Dense1x2::Light)
        .light_color(Dense1x2::Dark)
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
    fn empty_data_is_an_error_or_handled() {
        // qrcode requires at least 1 byte; empty input should produce
        // a code with minimal content, not panic.
        let _ = to_terminal(b"");
    }
}
