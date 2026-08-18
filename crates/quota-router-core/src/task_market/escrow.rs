//! Task escrow — task-scoped wrapper around `marketplace::escrow::Escrow`.
//!
//! Placeholder; full implementation lands in Task 6.3.
//!
//! **Not `Clone`** (Round 1 review fix): same reasoning as
//! `marketplace::escrow::Escrow` — cloning would enable a
//! double-settle vector on the wrapped `Escrow`. Use
//! `TaskEscrowSnapshot` (cloneable) for audit/log capture.

use crate::marketplace::escrow::{Escrow, EscrowError, EscrowSnapshot, EscrowState, Party};
use octo_determin::Dqa;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum TaskEscrowError {
    #[error(transparent)]
    Escrow(#[from] EscrowError),
}

#[derive(Debug, PartialEq, Eq)]
pub struct TaskEscrow {
    pub base: Escrow,
    pub task_id: [u8; 32],
    pub request_id: [u8; 32],
}

/// Immutable, cloneable snapshot of a `TaskEscrow`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskEscrowSnapshot {
    pub base: EscrowSnapshot,
    pub task_id: [u8; 32],
    pub request_id: [u8; 32],
}

impl From<&TaskEscrow> for TaskEscrowSnapshot {
    fn from(t: &TaskEscrow) -> Self {
        Self {
            base: EscrowSnapshot::from(&t.base),
            task_id: t.task_id,
            request_id: t.request_id,
        }
    }
}

impl TaskEscrow {
    #[must_use]
    pub fn new(
        id: [u8; 32],
        task_id: [u8; 32],
        request_id: [u8; 32],
        buyer: impl Into<String>,
        seller: impl Into<String>,
        amount_micro_octo_w: Dqa,
    ) -> Self {
        Self {
            base: Escrow::new(id, buyer, seller, amount_micro_octo_w),
            task_id,
            request_id,
        }
    }

    #[must_use]
    pub fn with_arbitrator(
        id: [u8; 32],
        task_id: [u8; 32],
        request_id: [u8; 32],
        buyer: impl Into<String>,
        seller: impl Into<String>,
        arbitrator: impl Into<String>,
        amount_micro_octo_w: Dqa,
    ) -> Self {
        Self {
            base: Escrow::with_arbitrator(id, buyer, seller, arbitrator, amount_micro_octo_w),
            task_id,
            request_id,
        }
    }

    pub fn lock(&mut self, caller: &Party) -> Result<EscrowState, TaskEscrowError> {
        Ok(self.base.lock(caller)?)
    }

    pub fn settle(&mut self, caller: &Party) -> Result<EscrowState, TaskEscrowError> {
        Ok(self.base.settle(caller)?)
    }

    pub fn dispute(&mut self, caller: &Party) -> Result<EscrowState, TaskEscrowError> {
        Ok(self.base.dispute(caller)?)
    }

    pub fn resolve_valid(&mut self, caller: &Party) -> Result<EscrowState, TaskEscrowError> {
        Ok(self.base.resolve_valid(caller)?)
    }

    pub fn resolve_invalid(&mut self, caller: &Party) -> Result<EscrowState, TaskEscrowError> {
        Ok(self.base.resolve_invalid(caller)?)
    }

    #[must_use]
    pub fn state(&self) -> EscrowState {
        self.base.state
    }

    #[must_use]
    pub fn is_terminal(&self) -> bool {
        self.base.is_terminal()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> TaskEscrow {
        TaskEscrow::with_arbitrator(
            [0xaa; 32],
            [0xbb; 32],
            [0xcc; 32],
            octo_ident::test_helpers::sample_did(130),
            octo_ident::test_helpers::sample_did(95),
            octo_ident::test_helpers::sample_did(50),
            Dqa::new(100_000, 0).expect("scale=0 always valid"),
        )
    }

    fn buyer() -> Party {
        Party::Buyer(octo_ident::test_helpers::sample_did(130))
    }
    fn seller() -> Party {
        Party::Seller(octo_ident::test_helpers::sample_did(95))
    }
    fn arbiter() -> Party {
        Party::Arbitrator(octo_ident::test_helpers::sample_did(50))
    }

    #[test]
    fn task_escrow_full_happy_path() {
        let mut t = sample();
        t.lock(&buyer()).unwrap();
        t.dispute(&buyer()).unwrap();
        t.resolve_invalid(&arbiter()).unwrap();
        assert_eq!(t.state(), EscrowState::Settled);
        assert!(t.is_terminal());
    }

    #[test]
    fn task_escrow_rejects_unauthorized_caller() {
        // Seller trying to lock — must reject.
        let mut t = sample();
        assert!(matches!(
            t.lock(&seller()).unwrap_err(),
            TaskEscrowError::Escrow(EscrowError::UnauthorizedCaller {
                required: "buyer",
                ..
            })
        ));
    }

    #[test]
    fn task_escrow_snapshot_carries_arbitrator() {
        let t = sample();
        let snap = TaskEscrowSnapshot::from(&t);
        assert_eq!(
            snap.base.arbitrator,
            octo_ident::test_helpers::sample_did(50)
        );
        assert_eq!(snap.base.buyer, octo_ident::test_helpers::sample_did(130));
        assert_eq!(snap.base.seller, octo_ident::test_helpers::sample_did(95));
    }
}
