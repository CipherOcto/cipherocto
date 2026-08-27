use thiserror::Error;

#[derive(Error, Debug)]
pub enum BalanceError {
    #[error("Insufficient balance: have {0}, need {1}")]
    Insufficient(u64, u64),
}

#[deprecated(
    since = "0.2.0",
    note = "legacy OCTO-W-only key-keyed balance; superseded by VaultBalanceProjection (RFC-0960 §3.6). Cycle 1 of 3-cycle deprecation; deletion in Cycle 3 (1 release)."
)]
pub struct Balance {
    pub amount: u64,
}

#[allow(deprecated)]
impl Balance {
    #[allow(deprecated)]
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

    /// Deduct `amount` from this balance.
    ///
    /// Returns `Err(BalanceError::Insufficient)` if the balance is smaller than
    /// `amount` — i.e. the caller MUST prove sufficiency via `check()` before
    /// calling `deduct()`. Silent underflow (the previous `saturating_sub`
    /// behavior) is forbidden: it lets a vault's mutable balance drift below
    /// zero and silently lose accounting correctness. Per RFC-0960 §9 the
    /// mutable balance row is a projection of the event log, never the
    /// source of truth; an underflow here means an upstream invariant
    /// (capability constraint evaluation, reservation accounting) is
    /// inconsistent with the projection.
    pub fn deduct(&mut self, amount: u64) -> Result<(), BalanceError> {
        self.amount = self
            .amount
            .checked_sub(amount)
            .ok_or(BalanceError::Insufficient(self.amount, amount))?;
        Ok(())
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
        balance.deduct(cost).unwrap();
        assert_eq!(balance.amount, 90);
    }

    #[test]
    fn test_balance_add() {
        let mut balance = Balance::new(50);
        balance.add(30);
        assert_eq!(balance.amount, 80);
    }

    #[test]
    fn test_balance_deduct_insufficient_returns_err() {
        // Per RFC-0960 §9: silent underflow is forbidden. The mutable balance
        // is a projection of the event log; underflow means an upstream
        // invariant is inconsistent with the projection. deduct() must error.
        let mut balance = Balance::new(5);
        let result = balance.deduct(10);
        assert!(matches!(result, Err(BalanceError::Insufficient(5, 10))));
        // Balance is unchanged on error.
        assert_eq!(balance.amount, 5);
    }

    #[test]
    fn test_balance_error_display() {
        let err = BalanceError::Insufficient(50, 100);
        assert_eq!(
            format!("{}", err),
            "Insufficient balance: have 50, need 100"
        );
    }

    #[test]
    fn test_balance_check_exact() {
        let balance = Balance::new(100);
        assert!(balance.check(100).is_ok());
    }

    #[test]
    fn test_balance_deduct_zero() {
        let mut balance = Balance::new(100);
        balance.deduct(0).unwrap();
        assert_eq!(balance.amount, 100);
    }

    #[test]
    fn test_balance_add_zero() {
        let mut balance = Balance::new(100);
        balance.add(0);
        assert_eq!(balance.amount, 100);
    }

    #[test]
    fn test_get_octo_w_balance() {
        let storage = create_test_storage();
        let key_id = [1u8; 16];
        let result = get_octo_w_balance(&storage, &key_id);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 0); // default balance is 0
    }

    #[test]
    fn test_deduct_octo_w_insufficient() {
        let storage = create_test_storage();
        let key_id = [1u8; 16];
        let result = deduct_octo_w(&storage, &key_id, 100);
        assert!(result.is_err());
    }

    fn create_test_storage() -> crate::storage::StoolapKeyStorage {
        let db = octo_storage_core::Database::open_in_memory().unwrap();
        crate::schema::init_database(&db).unwrap();
        crate::storage::StoolapKeyStorage::new(db)
    }
}
