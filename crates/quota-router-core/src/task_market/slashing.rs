//! Task market slashing — wrapper around `SlashingLedger`.
//!
//! Placeholder; full integration lands in Task 6.4.

use crate::marketplace::slashing::{
    SlashError, SlashOutcome, SlashReason, SlashingLedger, SlashingRules,
};

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum TaskSlashError {
    #[error(transparent)]
    Slash(#[from] SlashError),
}

#[derive(Debug, Default, Clone)]
pub struct TaskMarketSlashing {
    ledger: SlashingLedger,
}

impl TaskMarketSlashing {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn with_rules(rules: SlashingRules) -> Self {
        Self {
            ledger: SlashingLedger::with_rules(rules),
        }
    }

    pub fn register(&mut self, provider_id: impl Into<String>, initial_stake_micro_octo_w: u128) {
        self.ledger
            .register(provider_id, initial_stake_micro_octo_w);
    }

    pub fn slash(
        &mut self,
        provider_id: &str,
        reason: SlashReason,
        miss_rate: f64,
    ) -> Result<SlashOutcome, TaskSlashError> {
        Ok(self.ledger.slash(provider_id, reason, miss_rate)?)
    }

    /// Current remaining stake for `provider_id`, or `None` if unregistered.
    #[must_use]
    pub fn ledger_stake(&self, provider_id: &str) -> Option<u128> {
        self.ledger.stake(provider_id).map(|s| s.stake_micro_octo_w)
    }

    #[must_use]
    pub fn rules(&self) -> &SlashingRules {
        self.ledger.rules()
    }
}
