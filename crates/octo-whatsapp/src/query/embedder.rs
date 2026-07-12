//! Embedder layer — turns message text into f32 vectors.
//!
//! Phase 0 only ships the trait surface + a mock so hermetic tests
//! don't need the real model. Phase 0 task 3 (next commit) fills in
//! `LocalCandleEmbedder` with the actual `all-MiniLM-L6-v2-q4`
//! forward pass + `HybridEmbedder` fallback routing.
//!
//! See `docs/plans/2026-07-11-whatsapp-query-layer-design.md` Part 4.

use std::result::Result as StdResult;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum EmbedError {
    #[error("transient embedder error: {0}")]
    Transient(String),
    #[error("fatal embedder error: {0}")]
    Fatal(String),
}

pub type Result<T> = StdResult<T, EmbedError>;

#[async_trait::async_trait]
pub trait Embedder: Send + Sync {
    fn model_id(&self) -> &'static str;
    fn dims(&self) -> usize;
    async fn embed(&self, inputs: &[String]) -> Result<Vec<Vec<f32>>>;
}

/// `HybridEmbedder` — Phase 0 placeholder. The full router with
/// remote fallback lands in Phase 0 task 3.
#[derive(Debug, Default)]
pub struct HybridEmbedder;

impl HybridEmbedder {
    pub fn new() -> Self {
        Self
    }
}

/// `LocalCandleEmbedder` — Phase 0 placeholder. The real implementation
/// (model load, tokenize, forward pass) lands in Phase 0 task 3.
#[derive(Debug)]
pub struct LocalCandleEmbedder;

impl LocalCandleEmbedder {
    pub fn new() -> StdResult<Self, EmbedError> {
        Ok(Self)
    }
}

impl Default for LocalCandleEmbedder {
    fn default() -> Self {
        Self::new().expect("LocalCandleEmbedder default")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placeholder_compiles() {
        // Real assertions land once task 3 wires the model.
        assert_eq!(std::mem::size_of::<HybridEmbedder>(), 0);
        assert!(LocalCandleEmbedder::new().is_ok());
    }
}
