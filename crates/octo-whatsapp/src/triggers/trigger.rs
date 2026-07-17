//! Trigger + RunnerSpec definitions. Phase 4 of
//! `docs/plans/2026-07-04-whatsapp-runtime-cli-mcp-design.md` §Triggers.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RunnerSpec {
    /// Shell process. Args as argv (no `sh -c`). `env_passthrough`
    /// is the allowlist of `OCTO_*` / `HOME` / etc. that the runner
    /// retains (default: empty + base shell env).
    Shell {
        argv: Vec<String>,
        cwd: Option<String>,
        env_passthrough: Vec<String>,
    },
    /// HTTP POST to a URL with optional HMAC signing.
    Http {
        url: String,
        method: String,
        headers: BTreeMap<String, String>,
        signing_secret_env: Option<String>,
    },
    /// Agent invocation. `input_template` is rendered with the
    /// event payload substituted into `{{event}}` placeholders.
    Agent {
        agent_id: String,
        input_template: String,
    },
}

impl RunnerSpec {
    pub fn kind_str(&self) -> &'static str {
        match self {
            RunnerSpec::Shell { .. } => "shell",
            RunnerSpec::Http { .. } => "http",
            RunnerSpec::Agent { .. } => "agent",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RateLimit {
    pub per_second: u32,
    pub burst: u32,
}

/// Outcome of a single trigger run. Stored on `Trigger::last_run`
/// and appended to the audit log with sha256 digests.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RunRecord {
    pub started_at: i64,
    pub finished_at: i64,
    pub exit_code: i32,
    pub stdout_sha256: String,
    pub stderr_sha256: String,
    pub truncated: bool,
    pub bytes_stdout: u64,
    pub bytes_stderr: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Trigger {
    pub id: String,
    pub version: u64,
    pub enabled: bool,
    pub runner: RunnerSpec,
    pub rate_limit: Option<RateLimit>,
    pub timeout_ms: u64,
    pub retries: u32,
    pub last_run: Option<RunRecord>,
    pub history_cap: u32,
    pub created_by: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub etag: String,
}

impl Trigger {
    pub fn is_fireable(&self) -> bool {
        self.enabled
    }

    pub fn etag_payload(&self) -> ETagPayload<'_> {
        ETagPayload {
            version: self.version,
            runner: &self.runner,
            rate_limit: &self.rate_limit,
            timeout_ms: self.timeout_ms,
            retries: self.retries,
            history_cap: self.history_cap,
            enabled: self.enabled,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ETagPayload<'a> {
    pub version: u64,
    pub runner: &'a RunnerSpec,
    pub rate_limit: &'a Option<RateLimit>,
    pub timeout_ms: u64,
    pub retries: u32,
    pub history_cap: u32,
    pub enabled: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runner_spec_serde_round_trip() {
        for r in [
            RunnerSpec::Shell {
                argv: vec!["echo".into(), "hi".into()],
                cwd: None,
                env_passthrough: vec!["HOME".into()],
            },
            RunnerSpec::Http {
                url: "https://example.com/h".into(),
                method: "POST".into(),
                headers: BTreeMap::from([("X-Test".into(), "v".into())]),
                signing_secret_env: Some("S".into()),
            },
            RunnerSpec::Agent {
                agent_id: "a1".into(),
                input_template: "{{event}}".into(),
            },
        ] {
            let j = serde_json::to_string(&r).unwrap();
            let back: RunnerSpec = serde_json::from_str(&j).unwrap();
            assert_eq!(r, back);
        }
    }

    #[test]
    fn runner_kind_str() {
        assert_eq!(
            RunnerSpec::Shell {
                argv: vec![],
                cwd: None,
                env_passthrough: vec![]
            }
            .kind_str(),
            "shell"
        );
        assert_eq!(
            RunnerSpec::Http {
                url: "x".into(),
                method: "GET".into(),
                headers: BTreeMap::new(),
                signing_secret_env: None
            }
            .kind_str(),
            "http"
        );
        assert_eq!(
            RunnerSpec::Agent {
                agent_id: "a".into(),
                input_template: "".into()
            }
            .kind_str(),
            "agent"
        );
    }
}
