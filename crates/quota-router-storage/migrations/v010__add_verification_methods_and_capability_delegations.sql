-- Mission 0010-f8-rich-did-storage: persist RFC-0010
-- §VerificationMethod + §CapabilityDelegation on the `did_registry`
-- table.
--
-- Encoding: borsh-serialized `Vec<VerificationMethod>` /
-- `Vec<CapabilityDelegation>` as BLOB (same pattern as v009).
-- NULL = legacy row (no rich fields populated); `resolve()` decodes
-- via `unwrap_or_default()` → empty `Vec`.
--
-- Total `did_registry` schema after v010: 7 columns (4 legacy +
-- 3 rich BLOBs). All rich columns nullable.

ALTER TABLE did_registry ADD COLUMN verification_methods BLOB;
ALTER TABLE did_registry ADD COLUMN capability_delegations BLOB;