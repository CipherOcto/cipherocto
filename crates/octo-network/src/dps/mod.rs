//! Deterministic Proof Substrate (DPS) — RFC-0854
//!
//! Provides deterministic proof generation, verification, and aggregation
//! with protocol-level proof attachment and canonical proof boundaries.

pub mod envelope;
pub mod error;
pub mod recursive;
pub mod suite;
pub mod verifier;
pub mod witness;

pub use envelope::ProofCarryingEnvelope;
pub use error::DpsError;
pub use recursive::{AggregatedProof, AggregationMethod};
pub use suite::{ProofCircuitModel, ProofExecutionClass, ProofSuite, ProofSystemId};
pub use verifier::{VerifierEntry, VerifierRegistry};
pub use witness::{Witness, WitnessInput};
