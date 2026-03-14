// Placeholder - implementation in Task 4
use crate::keys::{ApiKey, KeyError, KeyUpdates};

pub trait KeyStorage: Send + Sync {
    fn create_key(&self, _key: &ApiKey) -> Result<(), KeyError> { todo!() }
    fn lookup_by_hash(&self, _key_hash: &[u8]) -> Result<Option<ApiKey>, KeyError> { todo!() }
    fn update_key(&self, _key_id: &str, _updates: &KeyUpdates) -> Result<(), KeyError> { todo!() }
    fn list_keys(&self, _team_id: Option<&str>) -> Result<Vec<ApiKey>, KeyError> { todo!() }
}

pub struct StoolapKeyStorage {
    db: stoolap::Database,
}

impl StoolapKeyStorage {
    pub fn new(db: stoolap::Database) -> Self {
        Self { db }
    }
}
