//! Guardrail Engine (RFC-0946 §Execution Model + Mission 0946-c).
//!
//! Wraps `GuardrailExecutor` with Prometheus metrics emission. The engine
//! is the public surface used by the request-path layer (proxy.rs server-fn
//! wrapper) so individual handlers do not need to know about the executor
//! directly. Conversion from the `GuardrailConfig` enum-based config to
//! the trait-object `Arc<dyn GuardrailChecker>` form expected by the
//! executor lives in `adapter.rs`.
//!
//! Per RFC-0946:
//! - Pre-call hook runs input guardrails against the request prompt.
//! - Post-call hook runs output guardrails against the provider response.
//! - Failure mode: Block → 4xx response, Warn → log + allow, Transform →
//!   log + emit redacted body, Log → log + allow, Allow → no-op.
//!
//! Per RFC-0937: 4 Prometheus metrics — `guardrail_checks_total`,
//! `guardrail_blocks_total`, `guardrail_errors_total`,
//! `guardrail_latency_seconds`.

use std::sync::Arc;

use crate::guardrails::{GuardrailChecker, GuardrailExecutionResult, GuardrailExecutor};
use crate::metrics::Metrics;

/// Engine that runs guardrails in the request path with Prometheus hooks.
///
/// Owned by the server-fn layer; receives `Arc<Metrics>` for metric emission
/// and `Arc<GuardrailExecutor>` for the actual guardrail chain. Methods are
/// infallible at the engine level — guardrail errors are converted into
/// `GuardrailResult::Error` upstream and resolved by the executor's
/// `resolve_error` (fail-open or fail-closed per guardrail fallback).
pub struct GuardrailEngine {
    executor: Arc<GuardrailExecutor>,
    metrics: Arc<Metrics>,
}

impl GuardrailEngine {
    /// Construct a new engine. The caller owns the executor and metrics.
    pub fn new(executor: Arc<GuardrailExecutor>, metrics: Arc<Metrics>) -> Self {
        Self { executor, metrics }
    }

    /// Run pre-call input guardrails against the request prompt.
    ///
    /// `key_id` is the virtual key identifier (per-key overrides); `model`
    /// is the resolved model name (per-model overrides). Either may be
    /// `None`/empty to skip the corresponding override layer.
    ///
    /// Emits `guardrail_checks_total` (always) and `guardrail_blocks_total`
    /// (when the final result is Block). Latency is measured and recorded
    /// regardless of outcome.
    pub async fn check_input(
        &self,
        input: &str,
        key_id: Option<&str>,
        model: &str,
    ) -> GuardrailExecutionResult {
        let result = self.executor.check_input(input, key_id, model).await;
        self.record_outcome(&result);
        result
    }

    /// Run post-call output guardrails against the provider response.
    pub async fn check_output(
        &self,
        output: &str,
        key_id: Option<&str>,
        model: &str,
    ) -> GuardrailExecutionResult {
        let result = self.executor.check_output(output, key_id, model).await;
        self.record_outcome(&result);
        result
    }

    /// Build a `GuardrailExecutor` from a `GuardrailConfig`. Used at startup
    /// wiring time. Returns `None` if the config is unconfigured or has no
    /// usable guardrails. Conversion happens via the adapter (see `adapter.rs`).
    /// Per RFC-0946: model_overrides and key_overrides alone without global
    /// input/output are NOT rejected — operators may rely entirely on
    /// per-model or per-key policies.
    pub fn executor_from_config(
        cfg: &crate::config::GuardrailConfig,
    ) -> Option<Arc<GuardrailExecutor>> {
        if !cfg.enabled {
            return None;
        }
        let input = super::adapter::to_checkers(&cfg.input);
        let output = super::adapter::to_checkers(&cfg.output);
        let model_overrides = cfg
            .model_overrides
            .iter()
            .map(|(k, v)| (k.clone(), super::adapter::to_checkers(v)))
            .filter(|(_, v)| !v.is_empty())
            .collect::<std::collections::HashMap<_, _>>();
        let key_overrides = cfg
            .key_overrides
            .iter()
            .map(|(k, v)| (k.clone(), super::adapter::to_checkers(v)))
            .filter(|(_, v)| !v.is_empty())
            .collect::<std::collections::HashMap<_, _>>();
        if input.is_empty()
            && output.is_empty()
            && model_overrides.is_empty()
            && key_overrides.is_empty()
        {
            return None;
        }
        Some(Arc::new(GuardrailExecutor::new(
            input,
            output,
            model_overrides,
            key_overrides,
        )))
    }

    /// Emit metrics for a single guardrail execution outcome.
    fn record_outcome(&self, result: &GuardrailExecutionResult) {
        self.metrics.guardrail_checks_total.inc();
        let latency_seconds = result.execution_time_us as f64 / 1_000_000.0;
        self.metrics
            .guardrail_latency_seconds
            .observe(latency_seconds);
        if matches!(
            result.result,
            crate::guardrails::GuardrailResult::Block { .. }
        ) {
            self.metrics.guardrail_blocks_total.inc();
        }
        if result.triggered.is_empty() {
            // Allow: no guardrail fired. Done.
            return;
        }
        // Error fallbacks surfaced through resolve_error are still Allow
        // (fail-open) or Block (fail-closed) — counted as their target result.
        // We don't separately emit guardrail_errors_total at the engine
        // boundary because the executor resolves Fallback before returning.
        // Per-guardrail error metrics are emitted by the per-checker
        // adapter at the source. This keeps the metric surface honest.
        let _ = latency_seconds; // suppress unused
    }
}

/// Helper: extract the human-readable block reason from an
/// `GuardrailExecutionResult` for use in error responses.
pub fn block_reason(result: &GuardrailExecutionResult) -> String {
    match &result.result {
        crate::guardrails::GuardrailResult::Block { reason, guardrail } => {
            format!("[{}] {}", guardrail, reason)
        }
        crate::guardrails::GuardrailResult::Warn { warnings } => warnings.join("; "),
        crate::guardrails::GuardrailResult::Transform { transformed } => {
            format!("transformed={}", transformed)
        }
        _ => String::new(),
    }
}

/// Trait alias re-export for callers that don't want to import
/// `guardrails::GuardrailChecker` directly.
pub trait CheckInput: GuardrailChecker {}
impl<T: GuardrailChecker> CheckInput for T {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::guardrails::{GuardrailResult, GuardrailType};
    use std::sync::Arc;

    /// Stub checker that always returns the configured result.
    struct StubChecker {
        name: String,
        gt: GuardrailType,
        result_on_input: GuardrailResult,
        result_on_output: GuardrailResult,
    }

    #[async_trait::async_trait]
    impl GuardrailChecker for StubChecker {
        fn name(&self) -> &str {
            &self.name
        }
        fn guardrail_type(&self) -> GuardrailType {
            self.gt.clone()
        }
        async fn check_input(&self, _input: &str) -> GuardrailResult {
            self.result_on_input.clone()
        }
        async fn check_output(&self, _output: &str) -> GuardrailResult {
            self.result_on_output.clone()
        }
    }

    fn empty_metrics() -> Arc<Metrics> {
        Arc::new(Metrics::new())
    }

    #[tokio::test]
    async fn engine_check_input_emits_metrics() {
        let metrics = empty_metrics();
        let checker: Arc<dyn GuardrailChecker> = Arc::new(StubChecker {
            name: "stub_allow".into(),
            gt: GuardrailType::Both,
            result_on_input: GuardrailResult::Allow,
            result_on_output: GuardrailResult::Allow,
        });
        let executor = Arc::new(GuardrailExecutor::new(
            vec![checker],
            vec![],
            Default::default(),
            Default::default(),
        ));
        let engine = GuardrailEngine::new(executor, metrics.clone());

        let result = engine.check_input("hello", None, "gpt-4").await;
        assert!(matches!(result.result, GuardrailResult::Allow));
        assert_eq!(metrics.guardrail_checks_total.get(), 1);
        assert_eq!(metrics.guardrail_blocks_total.get(), 0);
    }

    #[tokio::test]
    async fn engine_check_input_block_increments_blocks() {
        let metrics = empty_metrics();
        let checker: Arc<dyn GuardrailChecker> = Arc::new(StubChecker {
            name: "stub_block".into(),
            gt: GuardrailType::Input,
            result_on_input: GuardrailResult::Block {
                reason: "forbidden".into(),
                guardrail: "stub_block".into(),
            },
            result_on_output: GuardrailResult::Allow,
        });
        let executor = Arc::new(GuardrailExecutor::new(
            vec![checker],
            vec![],
            Default::default(),
            Default::default(),
        ));
        let engine = GuardrailEngine::new(executor, metrics.clone());

        let result = engine.check_input("bad", None, "gpt-4").await;
        assert!(matches!(result.result, GuardrailResult::Block { .. }));
        assert_eq!(metrics.guardrail_checks_total.get(), 1);
        assert_eq!(metrics.guardrail_blocks_total.get(), 1);
    }

    #[tokio::test]
    async fn engine_check_output_runs_output_guardrails() {
        let metrics = empty_metrics();
        let checker: Arc<dyn GuardrailChecker> = Arc::new(StubChecker {
            name: "stub_out".into(),
            gt: GuardrailType::Output,
            result_on_input: GuardrailResult::Allow,
            result_on_output: GuardrailResult::Warn {
                warnings: vec!["profanity".into()],
            },
        });
        let executor = Arc::new(GuardrailExecutor::new(
            vec![],
            vec![checker],
            Default::default(),
            Default::default(),
        ));
        let engine = GuardrailEngine::new(executor, metrics.clone());

        let result = engine.check_output("output text", None, "gpt-4").await;
        assert!(matches!(result.result, GuardrailResult::Warn { .. }));
        assert_eq!(metrics.guardrail_checks_total.get(), 1);
    }

    #[test]
    fn block_reason_formats_block_correctly() {
        let r = GuardrailExecutionResult {
            result: GuardrailResult::Block {
                reason: "PII leak".into(),
                guardrail: "pii_detection".into(),
            },
            triggered: vec!["pii_detection".into()],
            execution_time_us: 42,
        };
        let s = block_reason(&r);
        assert!(s.contains("pii_detection"));
        assert!(s.contains("PII leak"));
    }

    #[test]
    fn block_reason_empty_for_allow() {
        let r = GuardrailExecutionResult {
            result: GuardrailResult::Allow,
            triggered: vec![],
            execution_time_us: 0,
        };
        assert_eq!(block_reason(&r), "");
    }

    #[test]
    fn executor_from_config_disabled_returns_none() {
        let cfg = crate::config::GuardrailConfig::default();
        assert!(GuardrailEngine::executor_from_config(&cfg).is_none());
    }

    #[test]
    fn executor_from_config_enabled_empty_lists_returns_none() {
        let cfg = crate::config::GuardrailConfig {
            enabled: true,
            input: vec![],
            output: vec![],
            model_overrides: Default::default(),
            key_overrides: Default::default(),
        };
        assert!(GuardrailEngine::executor_from_config(&cfg).is_none());
    }

    #[test]
    fn executor_from_config_pii_builds_input_checker() {
        let cfg = crate::config::GuardrailConfig {
            enabled: true,
            input: vec![crate::guardrails::Guardrail::PiiDetection {
                action: crate::guardrails::GuardrailAction::Block,
                entities: vec![crate::guardrails::PiiEntity::Email],
            }],
            output: vec![],
            model_overrides: Default::default(),
            key_overrides: Default::default(),
        };
        let exec = GuardrailEngine::executor_from_config(&cfg).expect("executor");
        assert_eq!(exec.input_guardrails.len(), 1);
        assert_eq!(exec.input_guardrails[0].name(), "pii_detection");
    }

    #[test]
    fn executor_from_config_model_overrides_resolved() {
        let mut model_overrides = std::collections::HashMap::new();
        model_overrides.insert(
            "gpt-4".to_string(),
            vec![crate::guardrails::Guardrail::PiiDetection {
                action: crate::guardrails::GuardrailAction::Block,
                entities: vec![crate::guardrails::PiiEntity::SSN],
            }],
        );
        let cfg = crate::config::GuardrailConfig {
            enabled: true,
            input: vec![],
            output: vec![],
            model_overrides,
            key_overrides: Default::default(),
        };
        let exec = GuardrailEngine::executor_from_config(&cfg).expect("executor");
        let g = exec
            .model_overrides
            .get("gpt-4")
            .expect("model override populated");
        assert_eq!(g.len(), 1);
    }

    #[test]
    fn config_serializes_litellm_alias_format() {
        // Operators using LiteLLM-style YAML should be able to drop
        // `input_guardrails` / `output_guardrails` keys directly.
        let yaml = r#"
            enabled: true
            input_guardrails:
              - type: pii_detection
                action: block
                entities: [email]
            output_guardrails: []
        "#;
        let cfg: crate::config::GuardrailConfig =
            serde_yaml::from_str(yaml).expect("LiteLLM-compatible YAML deserializes");
        assert!(cfg.enabled);
        assert_eq!(cfg.input.len(), 1);
        assert!(matches!(
            cfg.input[0],
            crate::guardrails::Guardrail::PiiDetection { .. }
        ));
    }

    #[test]
    fn config_canonical_keys_still_work() {
        // The canonical `input` / `output` keys must continue to work
        // (no regression on existing operator configs).
        let yaml = r#"
            enabled: true
            input:
              - type: pii_detection
                action: block
                entities: [email]
            output: []
        "#;
        let cfg: crate::config::GuardrailConfig =
            serde_yaml::from_str(yaml).expect("canonical YAML deserializes");
        assert!(cfg.enabled);
        assert_eq!(cfg.input.len(), 1);
    }
}
