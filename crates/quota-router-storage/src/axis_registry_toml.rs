//! TOML parser for the PricingAxis registry (RFC-0959 §Data Structures).
//!
//! Loads `crates/octo-core/config/pricing-axes.toml` at boot, validates that
//! every axis ID is snake_case (RFC-0959 §Data Structures: kebab-case and
//! mixed-case rejected fail-closed), and exposes the parsed
//! [`PricingAxisRegistry`].
//!
//! ## TOML schema
//!
//! ```toml
//! [[axes]]
//! id = "input_tokens_per_1k"
//! name = "Input tokens per 1K"
//! default_rate_per_1k = 30000
//!
//! [[axes]]
//! id = "output_tokens_per_1k"
//! name = "Output tokens per 1K"
//! default_rate_per_1k = 60000
//!
//! [[axes]]
//! id = "cached_input_tokens_per_1k"
//! name = "Cached input tokens per 1K"
//! default_rate_per_1k = 3000
//! ```

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::ask::{AxisRegistryError, PricingAxis, PricingAxisRegistry};

/// Errors from TOML registry loading / validation.
#[derive(Debug, thiserror::Error)]
pub enum AxisRegistryTomlError {
    #[error("failed to read TOML file `{path}`: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to parse TOML: {0}")]
    Toml(#[from] toml::de::Error),
    #[error(
        "axis `{0}` is not snake_case (snake_case required; kebab-case + mixed-case rejected)"
    )]
    NotSnakeCase(String),
    #[error("duplicate axis id `{0}` in TOML")]
    Duplicate(String),
    #[error(
        "registry must contain at least the 3 MVP axes (input/output/cached_input); missing `{0}`"
    )]
    MissingMvpAxis(String),
    #[error("axis `{axis}` has negative rate `{rate}` (must be >= 0)")]
    NegativeRate { axis: String, rate: i64 },
    #[error("registry insert failed: {0}")]
    Registry(#[from] AxisRegistryError),
}

/// Raw TOML representation.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct TomlFile {
    axes: Vec<TomlAxis>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TomlAxis {
    id: String,
    name: String,
    // `toml` crate has no native u128 support as of 0.8; decode as i64 and
    // validate before converting. RFC-0959 §Data Structures per-axis rate
    // cap is well within i64 range for any realistic deployment.
    default_rate_per_1k: i64,
}

/// Validate snake_case: lowercase letters + digits + underscores; must not
/// start or end with underscore; no consecutive underscores; only ASCII.
#[must_use]
pub fn is_snake_case(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    if s.starts_with('_') || s.ends_with('_') {
        return false;
    }
    let mut prev_underscore = false;
    for c in s.chars() {
        if c == '_' {
            if prev_underscore {
                return false; // consecutive
            }
            prev_underscore = true;
            continue;
        }
        prev_underscore = false;
        if !c.is_ascii_lowercase() && !c.is_ascii_digit() {
            return false;
        }
    }
    true
}

/// Load the registry from a TOML string. Rejects non-snake-case IDs and
/// duplicate IDs fail-closed. The MVP axes (`input_tokens_per_1k`,
/// `output_tokens_per_1k`, `cached_input_tokens_per_1k`) are required to
/// be present per RFC-0959 §Data Structures.
/// # Errors
/// Returns `AxisRegistryTomlError::Toml` on parse failure,
/// `AxisRegistryTomlError::NotSnakeCase` on invalid ID, etc.
pub fn load_from_str(src: &str) -> Result<PricingAxisRegistry, AxisRegistryTomlError> {
    let parsed: TomlFile = toml::from_str(src)?;
    let mut seen_ids = std::collections::HashSet::new();
    // Start from an EMPTY registry (NOT `PricingAxisRegistry::default()` which
    // pre-populates the 3 MVP axes); the TOML file is the source of truth.
    let mut registry = PricingAxisRegistry { axes: Vec::new() };
    for ax in parsed.axes {
        if !is_snake_case(&ax.id) {
            return Err(AxisRegistryTomlError::NotSnakeCase(ax.id));
        }
        if !seen_ids.insert(ax.id.clone()) {
            return Err(AxisRegistryTomlError::Duplicate(ax.id));
        }
        if ax.default_rate_per_1k < 0 {
            return Err(AxisRegistryTomlError::NegativeRate {
                axis: ax.id,
                rate: ax.default_rate_per_1k,
            });
        }
        let rate = octo_determin::Dqa::new(ax.default_rate_per_1k, 0).map_err(|_| {
            AxisRegistryTomlError::NegativeRate {
                axis: ax.id.clone(),
                rate: ax.default_rate_per_1k,
            }
        })?;
        registry.register(PricingAxis {
            id: ax.id,
            name: ax.name,
            default_rate_per_1k: rate,
        })?;
    }
    // MVP axis check (RFC-0959 §Data Structures: registry ships with the
    // 3 MVP axes; custom additions are allowed but the MVP set is required).
    for required in &[
        "input_tokens_per_1k",
        "output_tokens_per_1k",
        "cached_input_tokens_per_1k",
    ] {
        if registry.get(required).is_none() {
            return Err(AxisRegistryTomlError::MissingMvpAxis(
                (*required).to_owned(),
            ));
        }
    }
    Ok(registry)
}

/// Load from a TOML file path.
/// # Errors
/// Returns `AxisRegistryTomlError::Io` on read failure.
pub fn load_from_path(
    path: impl AsRef<Path>,
) -> Result<PricingAxisRegistry, AxisRegistryTomlError> {
    let path_ref = path.as_ref();
    let src = std::fs::read_to_string(path_ref).map_err(|e| AxisRegistryTomlError::Io {
        path: path_ref.display().to_string(),
        source: e,
    })?;
    load_from_str(&src)
}

/// Default MVP registry as an inline string (used when no TOML file ships
/// with the deployment; the file is preferred for production).
pub const DEFAULT_MVP_TOML: &str = r#"
[[axes]]
id = "input_tokens_per_1k"
name = "Input tokens per 1K"
default_rate_per_1k = 30000

[[axes]]
id = "output_tokens_per_1k"
name = "Output tokens per 1K"
default_rate_per_1k = 60000

[[axes]]
id = "cached_input_tokens_per_1k"
name = "Cached input tokens per 1K"
default_rate_per_1k = 3000
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_snake_case_accepts_valid() {
        assert!(is_snake_case("input_tokens_per_1k"));
        assert!(is_snake_case("a"));
        assert!(is_snake_case("a1"));
        assert!(is_snake_case("abc_def_ghi"));
    }

    #[test]
    fn is_snake_case_rejects_kebab() {
        assert!(!is_snake_case("input-tokens-per-1k"));
    }

    #[test]
    fn is_snake_case_rejects_mixed_case() {
        assert!(!is_snake_case("Input_tokens"));
        assert!(!is_snake_case("inputTokens"));
        assert!(!is_snake_case("InputTokens"));
    }

    #[test]
    fn is_snake_case_rejects_leading_trailing_underscore() {
        assert!(!is_snake_case("_foo"));
        assert!(!is_snake_case("foo_"));
    }

    #[test]
    fn is_snake_case_rejects_consecutive_underscores() {
        assert!(!is_snake_case("foo__bar"));
    }

    #[test]
    fn is_snake_case_rejects_empty() {
        assert!(!is_snake_case(""));
    }

    #[test]
    fn is_snake_case_rejects_non_ascii() {
        assert!(!is_snake_case("café"));
        assert!(!is_snake_case("foo_baré"));
    }

    #[test]
    fn default_mvp_toml_loads() {
        let reg = load_from_str(DEFAULT_MVP_TOML).unwrap();
        assert_eq!(reg.axes.len(), 3);
        assert!(reg.get("input_tokens_per_1k").is_some());
        assert!(reg.get("output_tokens_per_1k").is_some());
        assert!(reg.get("cached_input_tokens_per_1k").is_some());
    }

    #[test]
    fn rejects_kebab_case_axis_id() {
        let src = r#"
[[axes]]
id = "input-tokens-per-1k"
name = "bad"
default_rate_per_1k = 1000

[[axes]]
id = "output_tokens_per_1k"
name = "Output"
default_rate_per_1k = 1000

[[axes]]
id = "cached_input_tokens_per_1k"
name = "Cached"
default_rate_per_1k = 1000
"#;
        let err = load_from_str(src).unwrap_err();
        assert!(
            matches!(err, AxisRegistryTomlError::NotSnakeCase(ref s) if s == "input-tokens-per-1k")
        );
    }

    #[test]
    fn rejects_mixed_case_axis_id() {
        let src = r#"
[[axes]]
id = "InputTokens"
name = "bad"
default_rate_per_1k = 1000

[[axes]]
id = "output_tokens_per_1k"
name = "Output"
default_rate_per_1k = 1000

[[axes]]
id = "cached_input_tokens_per_1k"
name = "Cached"
default_rate_per_1k = 1000
"#;
        let err = load_from_str(src).unwrap_err();
        assert!(matches!(err, AxisRegistryTomlError::NotSnakeCase(ref s) if s == "InputTokens"));
    }

    #[test]
    fn rejects_duplicate_ids() {
        let src = r#"
[[axes]]
id = "input_tokens_per_1k"
name = "First"
default_rate_per_1k = 1000

[[axes]]
id = "input_tokens_per_1k"
name = "Dup"
default_rate_per_1k = 2000

[[axes]]
id = "output_tokens_per_1k"
name = "Output"
default_rate_per_1k = 1000

[[axes]]
id = "cached_input_tokens_per_1k"
name = "Cached"
default_rate_per_1k = 1000
"#;
        let err = load_from_str(src).unwrap_err();
        assert!(
            matches!(err, AxisRegistryTomlError::Duplicate(ref s) if s == "input_tokens_per_1k")
        );
    }

    #[test]
    fn rejects_missing_mvp_axis() {
        let src = r#"
[[axes]]
id = "input_tokens_per_1k"
name = "Input"
default_rate_per_1k = 1000

[[axes]]
id = "cached_input_tokens_per_1k"
name = "Cached"
default_rate_per_1k = 1000
"#;
        let err = load_from_str(src).unwrap_err();
        assert!(
            matches!(err, AxisRegistryTomlError::MissingMvpAxis(ref s) if s == "output_tokens_per_1k")
        );
    }

    #[test]
    fn accepts_runtime_additions_beyond_mvp() {
        // Per RFC-0959 §Data Structures: registry leaves room for additions
        // (axis-class additions require RFC revision; axis-instance additions
        // are registry-only).
        let src = r#"
[[axes]]
id = "input_tokens_per_1k"
name = "Input"
default_rate_per_1k = 1000

[[axes]]
id = "output_tokens_per_1k"
name = "Output"
default_rate_per_1k = 1000

[[axes]]
id = "cached_input_tokens_per_1k"
name = "Cached"
default_rate_per_1k = 1000

[[axes]]
id = "image_tokens_per_1k"
name = "Image tokens per 1K"
default_rate_per_1k = 5000
"#;
        let reg = load_from_str(src).unwrap();
        assert_eq!(reg.axes.len(), 4);
        assert!(reg.get("image_tokens_per_1k").is_some());
    }

    #[test]
    fn load_from_path_roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("pricing-axes.toml");
        std::fs::write(&path, DEFAULT_MVP_TOML).unwrap();
        let reg = load_from_path(&path).unwrap();
        assert_eq!(reg.axes.len(), 3);
    }
}
