//! Task escrow — task-scoped wrapper around `marketplace::escrow::Escrow`.
//!
//! Placeholder; full implementation lands in Task 6.3.

use crate::marketplace::escrow::{Escrow, EscrowError, EscrowState};

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum TaskEscrowError {
    #[error(transparent)]
    Escrow(#[from] EscrowError),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskEscrow {
    pub base: Escrow,
    pub task_id: [u8; 32],
    pub request_id: [u8; 32],
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
