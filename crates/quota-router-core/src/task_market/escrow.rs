//! Task escrow — task-scoped wrapper around `marketplace::escrow::Escrow`.
//!
//! Placeholder; full implementation lands in Task 6.3.
//!
//! **Not `Clone`** (Round 1 review fix): same reasoning as
//! `marketplace::escrow::Escrow` — cloning would enable a
//! double-settle vector on the wrapped `Escrow`. Use
//! `TaskEscrowSnapshot` (cloneable) for audit/log capture.

use crate::marketplace::escrow::{Escrow, EscrowError, EscrowSnapshot, EscrowState};

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
        amount_micro_octo_w: u128,
    ) -> Self {
        Self {
            base: Escrow::new(id, buyer, seller, amount_micro_octo_w),
            task_id,
            request_id,
        }
    }

    pub fn lock(&mut self) -> Result<EscrowState, TaskEscrowError> {
        Ok(self.base.lock()?)
    }

    pub fn settle(&mut self) -> Result<EscrowState, TaskEscrowError> {
        Ok(self.base.settle()?)
    }

    pub fn dispute(&mut self) -> Result<EscrowState, TaskEscrowError> {
        Ok(self.base.dispute()?)
    }

    pub fn resolve_valid(&mut self) -> Result<EscrowState, TaskEscrowError> {
        Ok(self.base.resolve_valid()?)
    }

    pub fn resolve_invalid(&mut self) -> Result<EscrowState, TaskEscrowError> {
        Ok(self.base.resolve_invalid()?)
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
