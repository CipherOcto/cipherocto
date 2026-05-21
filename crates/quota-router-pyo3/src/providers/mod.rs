// Providers module for quota-router-pyo3
// Per RFC-0917 Phase 3: any-llm-mode replaces any-llm SDK
//
// ⚠️ CRITICAL INVARIANT (RFC-0917):
// Mode gate controls PROVIDER STRATEGY (reqwest vs PyO3), NOT interface availability.
// BOTH HTTP proxy AND Python SDK exist in ALL modes.

#[allow(dead_code)]
pub mod base;
#[allow(dead_code)]
pub mod factory;
