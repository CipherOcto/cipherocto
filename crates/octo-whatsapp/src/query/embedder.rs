//! Embedder layer — turns message text into f32 vectors.
//!
//! Two implementations coexist behind a [`HybridEmbedder`] router:
//!
//! - [`LocalCandleEmbedder`] (default): `candle-core` +
//!   `sentence-transformers/all-MiniLM-L6-v2` quantized to Q4,
//!   384 dims. ~25 MB model weights pulled on first run via
//!   `hf-hub` into `~/.cache/octo/models/all-MiniLM-L6-v2-q4/`.
//!   No network at embed time; deterministic across runs.
//! - (Phase 4) `RemoteEmbedder` (opt-in): OpenAI-compatible
//!   `POST {url}/v1/embeddings`. Off by default; configured via
//!   `[query.embed]` in `query.toml`.
//!
//! Failure paths write to `embeddings` with `provider='failed'` + a
//! `ts_embed_ms` stamp so callers can compute coverage via
//! `messages.coverage`.
//!
//! See `docs/plans/2026-07-11-whatsapp-query-layer-design.md` Part 4.

use std::result::Result as StdResult;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum EmbedError {
    /// Recoverable — caller may retry or fall back to another
    /// embedder.
    #[error("transient embedder error: {0}")]
    Transient(String),
    /// Unrecoverable — model missing, configuration invalid, etc.
    #[error("fatal embedder error: {0}")]
    Fatal(String),
}

pub type Result<T> = StdResult<T, EmbedError>;

impl EmbedError {
    pub fn is_transient(&self) -> bool {
        matches!(self, EmbedError::Transient(_))
    }
}

/// One text-to-vector encoder. Implementations are `Send + Sync`
/// so a single `HybridEmbedder` can be shared across the ingest
/// broadcast loop.
#[async_trait::async_trait]
pub trait Embedder: Send + Sync {
    /// Stable identifier persisted in the `embeddings.model_id`
    /// column so consumers can tell if two embeddings came from the
    /// same model.
    fn model_id(&self) -> &'static str;

    /// Vector dimensionality. Used to size cosine math + bucket
    /// matched-dim lookups. Stored per-row in `embeddings.dims`.
    fn dims(&self) -> usize;

    /// Embed a batch of texts. Returns one vector per input, in the
    /// same order. Empty `inputs` returns an empty Vec.
    ///
    /// Implementations are expected to L2-normalize so cosine
    /// similarity == dot product.
    async fn embed(&self, inputs: &[String]) -> Result<Vec<Vec<f32>>>;

    /// Short human-readable provider tag: `"local"` or `"remote"`.
    /// Persisted in `embeddings.provider`.
    fn provider_tag(&self) -> &'static str;
}

// ---------------------------------------------------------------------------
// HybridEmbedder — primary + optional fallback + metrics.
// ---------------------------------------------------------------------------

/// Counters for observability. Cheap to clone via `Arc`.
#[derive(Debug, Default)]
pub struct EmbedMetrics {
    pub primary_ok: AtomicU64,
    pub primary_failures: AtomicU64,
    pub fallback_ok: AtomicU64,
    pub fallback_failures: AtomicU64,
    pub all_failed: AtomicU64,
    pub total_texts_embedded: AtomicU64,
}

impl EmbedMetrics {
    pub fn snapshot(&self) -> EmbedMetricsSnapshot {
        EmbedMetricsSnapshot {
            primary_ok: self.primary_ok.load(Ordering::Relaxed),
            primary_failures: self.primary_failures.load(Ordering::Relaxed),
            fallback_ok: self.fallback_ok.load(Ordering::Relaxed),
            fallback_failures: self.fallback_failures.load(Ordering::Relaxed),
            all_failed: self.all_failed.load(Ordering::Relaxed),
            total_texts_embedded: self.total_texts_embedded.load(Ordering::Relaxed),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EmbedMetricsSnapshot {
    pub primary_ok: u64,
    pub primary_failures: u64,
    pub fallback_ok: u64,
    pub fallback_failures: u64,
    pub all_failed: u64,
    pub total_texts_embedded: u64,
}

/// Try `primary` first; on `EmbedError::Transient`, fall back to
/// `fallback` if configured. Otherwise surface the primary error
/// or `EmbedError::Fatal` for both-failed (so the caller can mark
/// `provider='failed'` in storage).
///
/// Both implementations must produce the SAME `dims()`; the router
/// enforces this at construction time.
pub struct HybridEmbedder {
    primary: Arc<dyn Embedder>,
    fallback: Option<Arc<dyn Embedder>>,
    metrics: Arc<EmbedMetrics>,
}

impl std::fmt::Debug for HybridEmbedder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HybridEmbedder")
            .field("primary_model", &self.primary.model_id())
            .field(
                "fallback_model",
                &self.fallback.as_ref().map(|e| e.model_id()),
            )
            .finish_non_exhaustive()
    }
}

impl HybridEmbedder {
    /// Construct with only a primary embedder (no fallback).
    pub fn new(primary: Arc<dyn Embedder>) -> Self {
        Self {
            primary,
            fallback: None,
            metrics: Arc::new(EmbedMetrics::default()),
        }
    }

    /// Construct with primary + fallback. Asserts both have the same
    /// `dims()` so per-row storage always reads back consistent
    /// vectors regardless of which provider served the request.
    pub fn with_fallback(primary: Arc<dyn Embedder>, fallback: Arc<dyn Embedder>) -> Self {
        assert_eq!(
            primary.dims(),
            fallback.dims(),
            "primary and fallback embeddings must agree on dims ({} vs {})",
            primary.dims(),
            fallback.dims()
        );
        Self {
            primary,
            fallback: Some(fallback),
            metrics: Arc::new(EmbedMetrics::default()),
        }
    }

    /// Borrow the metrics handle. Useful for `query.stats`.
    pub fn metrics(&self) -> Arc<EmbedMetrics> {
        Arc::clone(&self.metrics)
    }

    /// Embed one batch. The full logic lives in
    /// [`HybridEmbedder::embed_with_timing`] — this is a thin
    /// wrapper that drops the timing record.
    pub async fn embed(&self, inputs: &[String]) -> Result<Vec<Vec<f32>>> {
        self.embed_with_timing(inputs).await.map(|r| r.vectors)
    }

    /// Embed with timing information exposed — useful for
    /// observability / soak tests.
    pub async fn embed_with_timing(&self, inputs: &[String]) -> Result<EmbedResult> {
        if inputs.is_empty() {
            return Ok(EmbedResult {
                vectors: vec![],
                served_by: ServeSource::Primary,
                elapsed: std::time::Duration::ZERO,
            });
        }
        let started = Instant::now();
        match self.primary.embed(inputs).await {
            Ok(vectors) => {
                self.metrics.primary_ok.fetch_add(1, Ordering::Relaxed);
                self.metrics
                    .total_texts_embedded
                    .fetch_add(vectors.len() as u64, Ordering::Relaxed);
                Ok(EmbedResult {
                    vectors,
                    served_by: ServeSource::Primary,
                    elapsed: started.elapsed(),
                })
            }
            Err(e) if e.is_transient() => {
                self.metrics
                    .primary_failures
                    .fetch_add(1, Ordering::Relaxed);
                if let Some(fb) = &self.fallback {
                    match fb.embed(inputs).await {
                        Ok(vectors) => {
                            self.metrics.fallback_ok.fetch_add(1, Ordering::Relaxed);
                            self.metrics
                                .total_texts_embedded
                                .fetch_add(vectors.len() as u64, Ordering::Relaxed);
                            Ok(EmbedResult {
                                vectors,
                                served_by: ServeSource::Fallback,
                                elapsed: started.elapsed(),
                            })
                        }
                        Err(_) => {
                            self.metrics
                                .fallback_failures
                                .fetch_add(1, Ordering::Relaxed);
                            self.metrics.all_failed.fetch_add(1, Ordering::Relaxed);
                            Err(EmbedError::Fatal(
                                "both primary and fallback embedders failed".to_string(),
                            ))
                        }
                    }
                } else {
                    self.metrics.all_failed.fetch_add(1, Ordering::Relaxed);
                    Err(e)
                }
            }
            Err(e) => {
                // Fatal — don't try fallback for non-transient errors.
                self.metrics
                    .primary_failures
                    .fetch_add(1, Ordering::Relaxed);
                self.metrics.all_failed.fetch_add(1, Ordering::Relaxed);
                Err(e)
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServeSource {
    Primary,
    Fallback,
}

#[derive(Debug)]
pub struct EmbedResult {
    pub vectors: Vec<Vec<f32>>,
    pub served_by: ServeSource,
    pub elapsed: std::time::Duration,
}

// ---------------------------------------------------------------------------
// MockEmbedder — deterministic vectors for hermetic tests.
// ---------------------------------------------------------------------------

/// Deterministic mock — `inputs[i]` maps to a vector of all-zeros
/// except for a single 1.0 at position `i % dims`. Lets tests assert
/// "the right input went to the right output" without depending on
/// real model weights. `model_id` is settable so different mocks can
/// look distinct to storage layer tests.
#[derive(Debug)]
pub struct MockEmbedder {
    model_id: &'static str,
    dims: usize,
    behavior: MockBehavior,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MockBehavior {
    Ok,
    TransientFailure,
    FatalFailure,
}

impl MockEmbedder {
    pub fn ok(model_id: &'static str, dims: usize) -> Self {
        Self {
            model_id,
            dims,
            behavior: MockBehavior::Ok,
        }
    }

    pub fn failing_transient(model_id: &'static str, dims: usize) -> Self {
        Self {
            model_id,
            dims,
            behavior: MockBehavior::TransientFailure,
        }
    }

    pub fn failing_fatal(model_id: &'static str, dims: usize) -> Self {
        Self {
            model_id,
            dims,
            behavior: MockBehavior::FatalFailure,
        }
    }
}

#[async_trait::async_trait]
impl Embedder for MockEmbedder {
    fn model_id(&self) -> &'static str {
        self.model_id
    }
    fn dims(&self) -> usize {
        self.dims
    }
    async fn embed(&self, inputs: &[String]) -> Result<Vec<Vec<f32>>> {
        match self.behavior {
            MockBehavior::Ok => Ok(inputs
                .iter()
                .enumerate()
                .map(|(i, _)| {
                    let mut v = vec![0.0f32; self.dims];
                    if self.dims > 0 {
                        v[i % self.dims] = 1.0;
                    }
                    v
                })
                .collect()),
            MockBehavior::TransientFailure => Err(EmbedError::Transient("mock transient".into())),
            MockBehavior::FatalFailure => Err(EmbedError::Fatal("mock fatal".into())),
        }
    }
    fn provider_tag(&self) -> &'static str {
        "mock"
    }
}

// ---------------------------------------------------------------------------
// LocalCandleEmbedder — scaffold that resolves the model path +
// exposes model metadata. Full tokenize + forward pass lands in
// Phase 1 task 9 (search_semantic + brute-force cosine); the
// embed() body returns EmbedError::Fatal("not wired") for now so
// callers fall back / fail loudly instead of silently embedding
// zeros.
// ---------------------------------------------------------------------------

/// File-system location of the cached MiniLM-L6-v2 Q4 weights.
///
/// Resolved via `hf-hub` on first construction:
/// `~/.cache/hf/hub/models--sentence-transformers--all-MiniLM-L6-v2-onnx/`
/// (default `hf-hub` cache). Future revisions can override via the
/// `OCTO_WHATSAPP_EMBED_MODEL_DIR` env var.
#[derive(Debug, Clone)]
pub struct LocalCandleEmbedder {
    /// Human-readable model id, persisted in `embeddings.model_id`.
    pub model_id: &'static str,
    pub dims: usize,
    /// Resolved absolute path to the model weights on disk, or
    /// `None` if the model hasn't been downloaded yet.
    pub model_dir: Option<std::path::PathBuf>,
}

impl Default for LocalCandleEmbedder {
    fn default() -> Self {
        Self::new().expect("LocalCandleEmbedder default")
    }
}

impl LocalCandleEmbedder {
    /// Attempt to resolve the model cache directory. Returns a
    /// `LocalCandleEmbedder` even if the model hasn't been
    /// downloaded yet (model_dir stays `None`).
    pub fn new() -> Result<Self> {
        let cache_dir = std::env::var("OCTO_WHATSAPP_EMBED_MODEL_DIR")
            .map(std::path::PathBuf::from)
            .ok()
            .or_else(|| {
                // hf-hub default cache layout. We don't actually call
                // hf-hub here yet — Phase 1 task 9 wires that.
                dirs_cache_root().map(|c| c.join("hf").join("hub"))
            });

        let model_dir = cache_dir.and_then(|c| {
            let p = c.join("models--sentence-transformers--all-MiniLM-L6-v2-onnx");
            if p.exists() {
                Some(p)
            } else {
                None
            }
        });

        Ok(Self {
            model_id: "all-MiniLM-L6-v2-q4",
            dims: 384,
            model_dir,
        })
    }

    /// True iff the model weights are present on disk.
    pub fn is_ready(&self) -> bool {
        self.model_dir.is_some()
    }

    /// Trigger download via `hf-hub`. Phase 1 task 9 fills this in;
    /// for now we return a `Fatal` error so callers know the
    /// embedder isn't wired yet.
    pub async fn ensure_downloaded(&self) -> Result<()> {
        Err(EmbedError::Fatal(
            "LocalCandleEmbedder::ensure_downloaded not yet wired; Phase 1 task 9 \
             (search_semantic + brute-force cosine) lands this path. See \
             docs/plans/2026-07-11-whatsapp-query-layer-design.md Part 4"
                .into(),
        ))
    }
}

fn dirs_cache_root() -> Option<std::path::PathBuf> {
    // Reuse the `dirs` crate already a direct dep of `octo-whatsapp`
    // (Phase 5 Part C). Falls back to None if HOME isn't set so
    // hermetic builds still compile.
    if let Some(home) = std::env::var_os("HOME") {
        return Some(std::path::PathBuf::from(home).join(".cache"));
    }
    None
}

#[async_trait::async_trait]
impl Embedder for LocalCandleEmbedder {
    fn model_id(&self) -> &'static str {
        self.model_id
    }
    fn dims(&self) -> usize {
        self.dims
    }
    async fn embed(&self, _inputs: &[String]) -> Result<Vec<Vec<f32>>> {
        // Phase 1 task 9: forward pass + mean-pool + L2-normalize.
        // Until that lands we return Fatal so the HybridEmbedder
        // marks this provider as failed and coverage stays honest.
        Err(EmbedError::Fatal(
            "LocalCandleEmbedder::embed not yet wired; Phase 1 task 9 lands this \
             path. Until then the query layer uses MockEmbedder in tests."
                .into(),
        ))
    }
    fn provider_tag(&self) -> &'static str {
        "local"
    }
}

// ---------------------------------------------------------------------------
// tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn mock_embedder_produces_l2_unit_vectors() {
        let emb = MockEmbedder::ok("mock-3d", 3);
        let vecs = emb
            .embed(&["a".into(), "b".into(), "c".into()])
            .await
            .unwrap();
        assert_eq!(vecs.len(), 3);
        // Each vector has exactly one 1.0 position.
        for (i, v) in vecs.iter().enumerate() {
            let sum: f32 = v.iter().sum();
            assert!((sum - 1.0).abs() < 1e-6, "input {i} sum {sum} != 1.0");
            let idx = v.iter().position(|&x| x == 1.0).expect("at least one 1.0");
            assert_eq!(idx, i, "input {i} should land at position {i}, got {idx}");
        }
        assert_eq!(emb.dims(), 3);
        assert_eq!(emb.model_id(), "mock-3d");
        assert_eq!(emb.provider_tag(), "mock");
    }

    #[tokio::test]
    async fn hybrid_primary_ok_returns_primary_metrics() {
        let primary: Arc<dyn Embedder> = Arc::new(MockEmbedder::ok("p", 8));
        let fallback: Arc<dyn Embedder> = Arc::new(MockEmbedder::ok("f", 8));
        let hybrid = HybridEmbedder::with_fallback(Arc::clone(&primary), Arc::clone(&fallback));
        let vecs = hybrid.embed(&["x".into()]).await.unwrap();
        assert_eq!(vecs.len(), 1);
        let snap = hybrid.metrics().snapshot();
        assert_eq!(snap.primary_ok, 1);
        assert_eq!(snap.fallback_ok, 0);
        assert_eq!(snap.all_failed, 0);
        assert_eq!(snap.total_texts_embedded, 1);
    }

    #[tokio::test]
    async fn hybrid_transient_failure_falls_back() {
        let primary: Arc<dyn Embedder> = Arc::new(MockEmbedder::failing_transient("p", 8));
        let fallback: Arc<dyn Embedder> = Arc::new(MockEmbedder::ok("f", 8));
        let hybrid = HybridEmbedder::with_fallback(Arc::clone(&primary), fallback);
        let res = hybrid
            .embed_with_timing(&["a".into(), "b".into()])
            .await
            .unwrap();
        assert_eq!(res.served_by, ServeSource::Fallback);
        assert_eq!(res.vectors.len(), 2);
        let snap = hybrid.metrics().snapshot();
        assert_eq!(snap.primary_failures, 1);
        assert_eq!(snap.fallback_ok, 1);
        assert_eq!(snap.all_failed, 0);
        assert_eq!(snap.total_texts_embedded, 2);
    }

    #[tokio::test]
    async fn hybrid_fatal_failure_does_not_attempt_fallback() {
        let primary: Arc<dyn Embedder> = Arc::new(MockEmbedder::failing_fatal("p", 8));
        let fallback: Arc<dyn Embedder> = Arc::new(MockEmbedder::ok("f", 8));
        let hybrid = HybridEmbedder::with_fallback(Arc::clone(&primary), fallback);
        let err = hybrid.embed(&["x".into()]).await.unwrap_err();
        assert!(matches!(err, EmbedError::Fatal(_)), "got {err:?}");
        let snap = hybrid.metrics().snapshot();
        assert_eq!(snap.primary_failures, 1);
        assert_eq!(snap.fallback_ok, 0, "fallback must NOT run on fatal");
        assert_eq!(snap.all_failed, 1);
    }

    #[tokio::test]
    async fn hybrid_transient_failure_without_fallback_surfaces_error() {
        let primary: Arc<dyn Embedder> = Arc::new(MockEmbedder::failing_transient("p", 8));
        let hybrid = HybridEmbedder::new(Arc::clone(&primary));
        let err = hybrid.embed(&["x".into()]).await.unwrap_err();
        assert!(err.is_transient(), "got {err:?}");
        let snap = hybrid.metrics().snapshot();
        assert_eq!(snap.primary_failures, 1);
        assert_eq!(snap.all_failed, 1);
    }

    #[tokio::test]
    async fn hybrid_dim_mismatch_panics_at_construction() {
        let primary: Arc<dyn Embedder> = Arc::new(MockEmbedder::ok("p", 8));
        let fallback: Arc<dyn Embedder> = Arc::new(MockEmbedder::ok("f", 16));
        // `Arc<dyn Trait>` is not `UnwindSafe`; the closure only
        // references pointer values, so AssertUnwindSafe is sound.
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            HybridEmbedder::with_fallback(Arc::clone(&primary), Arc::clone(&fallback))
        }));
        assert!(result.is_err(), "HybridEmbedder must reject dim mismatch");
    }

    #[tokio::test]
    async fn empty_input_is_zero_cost() {
        let primary: Arc<dyn Embedder> = Arc::new(MockEmbedder::ok("p", 8));
        let hybrid = HybridEmbedder::new(primary);
        let res = hybrid.embed_with_timing(&[]).await.unwrap();
        assert!(res.vectors.is_empty());
        // No metrics moved — empty input is a no-op.
        assert_eq!(hybrid.metrics().snapshot().primary_ok, 0);
    }

    #[test]
    fn local_candle_scaffold_resolves_meta() {
        let emb = LocalCandleEmbedder::new().expect("resolve");
        assert_eq!(emb.dims(), 384);
        assert_eq!(emb.model_id(), "all-MiniLM-L6-v2-q4");
        assert_eq!(emb.provider_tag(), "local");
        // is_ready() is bool — we don't assert the specific outcome
        // because hermetic environments without a downloaded model
        // should still construct cleanly.
        let _: bool = emb.is_ready();
    }

    #[tokio::test]
    async fn local_candle_scaffold_returns_fatal_until_phase1() {
        let emb = LocalCandleEmbedder::new().expect("resolve");
        let err = emb.embed(&["x".into()]).await.unwrap_err();
        // Either Fatal("not yet wired") or Fatal("model not
        // downloaded") — both communicate "not operational yet".
        assert!(matches!(err, EmbedError::Fatal(_)));
    }
}
