-- v003 — schema_migrations tracking table.
--
-- One row per applied migration. `apply_migrations` reads this table on
-- store open and runs only migrations whose version is absent. The runner
-- is idempotent — calling it on an open `Database` is a no-op once every
-- entry in `BUILTIN_MIGRATIONS` is present.
--
-- Version strings are the canonical file name without the `.sql` suffix
-- (e.g., `v001__reputation_events`). They MUST be unique, monotonically
-- sortable as text, and never edited after publication.
--
-- PK note: stoolap-fork supports only `INTEGER PRIMARY KEY` (rowid
-- aliasing). We track the canonical file name in `name TEXT UNIQUE`
-- instead; the synthetic `id INTEGER PRIMARY KEY AUTOINCREMENT`
-- satisfies the rowid constraint without semantic cost.

CREATE TABLE IF NOT EXISTS schema_migrations (
    id              INTEGER PRIMARY KEY,
    name            TEXT NOT NULL UNIQUE,
    applied_at_unix INTEGER NOT NULL
);
