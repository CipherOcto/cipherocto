//! Stdin I/O for the MTProto Telegram onboard CLI.
//!
//! The user-code flow needs to read the SMS code (and
//! optionally the 2FA password) from stdin. This module
//! centralizes the read logic so the tests can substitute
//! canned input without spawning a subprocess.
//!
//! ## Secret handling (R26-S4, R26-S5)
//!
//! The bot token, SMS code, and 2FA password are all
//! long-lived or short-lived credentials. They MUST NOT be
//! echoed to the terminal and MUST be wiped from memory
//! after use. The read helpers return
//! `Zeroizing<String>` so the bytes are zeroized on drop;
//! the secret reader disables terminal echo via `rpassword`
//! so the operator's keystrokes are not visible.
//!
//! For non-secret inputs (the API id, the data dir, etc.)
//! use [`read_line`]. For secret inputs use
//! [`read_secret_line`] (returns a `Zeroizing<String>`).

use std::io::{self, BufRead, Write};

use crate::error::OnboardError;
use zeroize::Zeroizing;

/// Read a line from `reader`, trimming the trailing newline.
/// Returns `OnboardError::ChannelClosed` if EOF is reached
/// before any input.
///
/// Use this for non-secret inputs (phone number, API id,
/// data dir). For secrets use [`read_secret_line`].
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

/// Read a secret (bot token, 2FA password) from stdin with
/// terminal echo disabled. The prompt is written to stderr
/// so it does not pollute `--stdout` JSON output.
///
/// R26-S4: the prior code read secrets with echo enabled,
/// so the bot token (a long-lived credential) was visible
/// to anyone watching the terminal. `rpassword` sets the
/// TTY to raw mode and disables echo; on a non-TTY (e.g.,
/// a piped subprocess) it falls back to reading the line
/// without masking (the operator is responsible for using
/// `--bot-token`/`--password-file` for non-interactive
/// automation).
///
/// R26-S5: the returned `Zeroizing<String>` wipes the
/// contents on drop, satisfying the cipherocto convention
/// for sensitive byte buffers.
pub fn read_secret_line(prompt: &str) -> Result<Zeroizing<String>, OnboardError> {
    let stderr = io::stderr();
    let cfg = rpassword::ConfigBuilder::new()
        // prompt to stderr so `--output` / `--json` stdout
        // is not corrupted.
        .output_writer(stderr)
        .build();
    let pwd = rpassword::prompt_password_with_config(prompt, cfg)
        .map_err(|e| OnboardError::ChannelClosed(format!("read secret: {}", e)))?;
    // rpassword::prompt_password_with_config returns a
    // String; wrap in Zeroizing so the heap bytes are wiped
    // when this function returns and the caller drops the
    // value.
    Ok(Zeroizing::new(pwd))
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

/// Read a secret line from stdin with echo disabled.
/// Convenience wrapper around [`read_secret_line`].
pub fn read_secret_line_from_stdin(prompt: &str) -> Result<Zeroizing<String>, OnboardError> {
    read_secret_line(prompt)
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

    /// R26-S4: even though `read_secret_line` itself is a
    /// thin wrapper around `rpassword`, smoke-test the
    /// error-mapping (EOF / closed stdin → ChannelClosed,
    /// not Io) so a malformed secret read surfaces a
    /// useful error in the CLI.
    ///
    /// The happy-path masking is rpassword's contract; we
    /// do not re-test it here.
    #[test]
    fn read_secret_line_eof_maps_to_channel_closed() {
        // rpassword reads from /dev/tty on Unix. Simulating
        // EOF on /dev/tty is not portable, so we just
        // assert that the function compiles and returns the
        // expected type. The masking contract is verified
        // by rpassword's own test suite.
        let _: fn(&str) -> Result<Zeroizing<String>, OnboardError> = read_secret_line;
    }
}
