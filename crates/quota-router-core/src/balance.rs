use thiserror::Error;

#[derive(Error, Debug)]
pub enum BalanceError {
    #[error("Insufficient balance: have {0}, need {1}")]
    Insufficient(u64, u64),
}

pub struct Balance {
    pub amount: u64,
}

impl Balance {
    pub fn new(amount: u64) -> Self {
        Self { amount }
    }

    pub fn check(&self, required: u64) -> Result<(), BalanceError> {
        if self.amount >= required {
            Ok(())
        } else {
            Err(BalanceError::Insufficient(self.amount, required))
        }
    }

    pub fn deduct(&mut self, amount: u64) {
        self.amount = self.amount.saturating_sub(amount);
    }

    pub fn add(&mut self, amount: u64) {
        self.amount += amount;
    }
}

// =============================================================================
// OCTO-W Balance Functions (RFC-0904 F3)
// =============================================================================

use crate::keys::KeyError;

/// Get the current OCTO-W balance for a key.
/// Returns Ok(u64) with balance in micro-units, or storage error.
pub fn get_octo_w_balance(
    storage: &dyn crate::storage::KeyStorage,
    key_id: &[u8; 16],
) -> Result<u64, KeyError> {
    let key_id_str = hex::encode(key_id);
    storage.get_octo_w_balance(&key_id_str)
}

/// Deduct cost_amount from OCTO-W balance atomically.
/// Returns Ok(new_balance) or error if insufficient or storage failure.
pub fn deduct_octo_w(
    storage: &dyn crate::storage::KeyStorage,
    key_id: &[u8; 16],
    cost_amount: u64,
) -> Result<u64, KeyError> {
    let key_id_str = hex::encode(key_id);
    storage.deduct_octo_w(&key_id_str, cost_amount)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_balance_check_sufficient() {
        let balance = Balance::new(100);
        let required = 10;
        assert!(balance.check(required).is_ok());
    }

    #[test]
    fn test_balance_check_insufficient() {
        let balance = Balance::new(5);
        let required = 10;
        assert!(balance.check(required).is_err());
    }

    #[test]
    fn test_balance_decrement() {
        let mut balance = Balance::new(100);
        let cost = 10;
        balance.deduct(cost);
        assert_eq!(balance.amount, 90);
    }

    #[test]
    fn test_balance_add() {
        let mut balance = Balance::new(50);
        balance.add(30);
        assert_eq!(balance.amount, 80);
    }

    #[test]
    fn test_balance_saturating_sub() {
        let mut balance = Balance::new(5);
        balance.deduct(10);
        assert_eq!(balance.amount, 0); // Should saturate, not underflow
    }
}