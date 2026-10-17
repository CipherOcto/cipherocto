-- Migration tracking table.
-- Records which migration versions have been applied to this database.
CREATE TABLE IF NOT EXISTS cipherocto_migrations (
    version INTEGER PRIMARY KEY,
    applied_at INTEGER NOT NULL
);
