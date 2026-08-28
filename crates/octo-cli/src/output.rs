//! Machine-readable output envelope — RFC-0011 §Output Envelope.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::io::{self, IsTerminal, Write};

/// Versioned envelope wrapping every successful command payload.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct OutputEnvelope<T> {
    /// Envelope schema version.
    pub schema_version: u32,
    /// Generation timestamp (RFC 3339, UTC, `Z` suffix).
    pub generated_at: DateTime<Utc>,
    /// Command payload.
    pub data: T,
    /// Process exit code the caller will use.
    pub exit_code: i32,
    /// True when the command ran in `--dry-run` preview mode.
    pub preview_only: bool,
}

impl<T> OutputEnvelope<T> {
    /// Current envelope schema version.
    pub const SCHEMA_VERSION: u32 = 2;

    /// Build an applied-result envelope.
    pub fn new(data: T, exit_code: i32) -> Self {
        Self {
            schema_version: Self::SCHEMA_VERSION,
            generated_at: Utc::now(),
            data,
            exit_code,
            preview_only: false,
        }
    }

    /// Build a preview-only (`--dry-run`) envelope.
    pub fn preview_only(data: T, exit_code: i32) -> Self {
        Self {
            schema_version: Self::SCHEMA_VERSION,
            generated_at: Utc::now(),
            data,
            exit_code,
            preview_only: true,
        }
    }
}

impl<T: Serialize> OutputEnvelope<T> {
    /// Render to stdout — JSON when forced or when stdout is not a TTY,
    /// otherwise a colourised pretty form.
    pub fn render(&self, force_json: bool, no_color: bool) -> io::Result<()> {
        let stdout = io::stdout();
        let tty = stdout.is_terminal();
        let mut w = stdout.lock();
        if force_json || !tty {
            let json = serde_json::to_string(self)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
            writeln!(w, "{json}")
        } else {
            self.render_pretty(&mut w, no_color)
        }
    }

    fn render_pretty<W: Write>(&self, w: &mut W, no_color: bool) -> io::Result<()> {
        let value = serde_json::to_value(self)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        let colored = !no_color && io::stdout().is_terminal();
        write_value(w, &value, 0, colored)?;
        writeln!(w)
    }
}

const CYAN: &str = "\x1b[36m";
const YELLOW: &str = "\x1b[33m";
const GREEN: &str = "\x1b[32m";
const MAGENTA: &str = "\x1b[35m";
const RESET: &str = "\x1b[0m";

fn paint(s: &str, code: &str, colored: bool) -> String {
    if colored {
        format!("{code}{s}{RESET}")
    } else {
        s.to_string()
    }
}

fn write_value<W: Write>(
    w: &mut W,
    v: &serde_json::Value,
    indent: usize,
    colored: bool,
) -> io::Result<()> {
    let pad = " ".repeat(indent);
    match v {
        serde_json::Value::Object(map) => {
            for (k, val) in map {
                let key = paint(k, CYAN, colored);
                match val {
                    serde_json::Value::Object(_) | serde_json::Value::Array(_) => {
                        writeln!(w, "{pad}{key}:")?;
                        write_value(w, val, indent + 2, colored)?;
                    }
                    _ => {
                        write!(w, "{pad}{key}: ")?;
                        write_scalar(w, val, colored)?;
                        writeln!(w)?;
                    }
                }
            }
            Ok(())
        }
        serde_json::Value::Array(items) => {
            for item in items {
                match item {
                    serde_json::Value::Object(_) | serde_json::Value::Array(_) => {
                        writeln!(w, "{pad}-")?;
                        write_value(w, item, indent + 2, colored)?;
                    }
                    _ => {
                        write!(w, "{pad}- ")?;
                        write_scalar(w, item, colored)?;
                        writeln!(w)?;
                    }
                }
            }
            Ok(())
        }
        _ => write_scalar(w, v, colored),
    }
}

fn write_scalar<W: Write>(w: &mut W, v: &serde_json::Value, colored: bool) -> io::Result<()> {
    let s = match v {
        serde_json::Value::String(s) => paint(s, YELLOW, colored),
        serde_json::Value::Bool(b) => paint(&b.to_string(), GREEN, colored),
        serde_json::Value::Number(n) => paint(&n.to_string(), MAGENTA, colored),
        serde_json::Value::Null => paint("null", GREEN, colored),
        _ => unreachable!("composite handled by write_value"),
    };
    write!(w, "{s}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::SecondsFormat;

    #[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
    struct Payload {
        name: String,
        count: u32,
    }

    fn payload() -> Payload {
        Payload {
            name: "did:octo:abc".into(),
            count: 3,
        }
    }

    #[test]
    fn tv_env1_schema_version_present() {
        let env = OutputEnvelope::new(payload(), 0);
        let json = serde_json::to_string(&env).unwrap();
        assert!(json.contains("\"schema_version\":2"), "{json}");
        assert!(json.contains("\"preview_only\":false"), "{json}");
    }

    #[test]
    fn tv_env2_generated_at_rfc3339_utc() {
        let env = OutputEnvelope::new(payload(), 0);
        let s = env.generated_at.to_rfc3339_opts(SecondsFormat::Secs, true);
        assert!(s.ends_with('Z'), "{s}");
    }

    #[test]
    fn tv_env3_preview_only_true() {
        let env = OutputEnvelope::preview_only(payload(), 0);
        let json = serde_json::to_string(&env).unwrap();
        assert!(json.contains("\"preview_only\":true"), "{json}");
    }

    #[test]
    fn tv_env4_json_roundtrip() {
        let env = OutputEnvelope::new(payload(), 0);
        let json = serde_json::to_string(&env).unwrap();
        let back: OutputEnvelope<Payload> = serde_json::from_str(&json).unwrap();
        assert_eq!(
            back.schema_version,
            OutputEnvelope::<Payload>::SCHEMA_VERSION
        );
        assert_eq!(back.data, env.data);
        assert_eq!(back.exit_code, env.exit_code);
        assert_eq!(back.preview_only, env.preview_only);
    }
}
