//! Stdin I/O for the MTProto Telegram onboard CLI.
//!
//! The user-code flow needs to read the SMS code (and
//! optionally the 2FA password) from stdin. This module
//! centralizes the read logic so the tests can substitute
//! canned input without spawning a subprocess.
//!
//! Notes on masking: SMS codes are short-lived (5-minute
//! expiry) and shown verbatim in the Telegram mobile app, so
//! we do not mask the input. 2FA passwords, by contrast, are
//! long-lived secrets and SHOULD be masked on input. We do
//! not yet have a `rpassword`/`zeroize` story in Phase B; the
//! 2FA path is best-effort and operators who care should use
//! `--password-file` (a future feature).
//!
//! For Phase B, we read a single line from stdin and trim
//! trailing whitespace. Tests can substitute `read_code` /
//! `read_password` with values that bypass stdin entirely
//! (see `--code-file` / `--password-file`).

use std::io::{self, BufRead, Write};

use crate::error::OnboardError;

/// Read a line from `reader`, trimming the trailing newline.
/// Returns `OnboardError::ChannelClosed` if EOF is reached
/// before any input.
pub fn read_line<R: BufRead, W: Write>(
    prompt: &str,
    reader: &mut R,
    writer: &mut W,
) -> Result<String, OnboardError> {
    write!(writer, "{}", prompt).map_err(OnboardError::Io)?;
    writer.flush().map_err(OnboardError::Io)?;
    let mut line = String::new();
    let n = reader.read_line(&mut line).map_err(OnboardError::Io)?;
    if n == 0 {
        return Err(OnboardError::ChannelClosed("stdin".to_string()));
    }
    Ok(line.trim().to_string())
}

/// Read a line from stdin. Convenience wrapper around
/// [`read_line`] for the production CLI path.
pub fn read_line_from_stdin(prompt: &str) -> Result<String, OnboardError> {
    let stdin = io::stdin();
    let mut handle = stdin.lock();
    let stdout = io::stdout();
    let mut handle_out = stdout.lock();
    read_line(prompt, &mut handle, &mut handle_out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn read_line_returns_trimmed_input() {
        let mut input = Cursor::new(b"12345\n".to_vec());
        let mut output = Vec::new();
        let got = read_line("code> ", &mut input, &mut output).unwrap();
        assert_eq!(got, "12345");
        assert_eq!(output, b"code> ");
    }

    #[test]
    fn read_line_trims_trailing_whitespace() {
        let mut input = Cursor::new(b"  abc  \n".to_vec());
        let mut output = Vec::new();
        let got = read_line("> ", &mut input, &mut output).unwrap();
        assert_eq!(got, "abc");
    }

    #[test]
    fn read_line_eof_returns_channel_closed() {
        let mut input = Cursor::new(Vec::<u8>::new());
        let mut output = Vec::new();
        let e = read_line("> ", &mut input, &mut output).unwrap_err();
        assert_eq!(e.kind(), "channel_closed");
    }
}
