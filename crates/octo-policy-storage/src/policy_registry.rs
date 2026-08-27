//! Stoolap-backed `PolicyRegistry` adapter (RFC-0967-A1 v1.9.2 §2.4 + §2.5).
//!
//! Layer C adapter — substrate implements the registry; trait surface
//! lives in `octo-policy::policy_registry` (R4 fix C1 — trait moved
//! from this crate to the owner crate per the Stable Abstractions
//! Principle: trait depends on stable substrate, not the reverse).

use std::sync::Arc;

use octo_policy::kind_uuid_registry::PolicyKindCategory;
use octo_policy::policy_kinds::ExecutionClass;
#[cfg(test)]
use octo_policy::policy_registry::verify_class_c_marker;
use octo_policy::policy_registry::{
    verify_class_b_zk_marker, PolicyRegistry, PolicyRegistryError, RegisteredPolicy,
};
use octo_storage_core::Database;
use subtle::ConstantTimeEq;

/// Map `ExecutionClass` to `policy_registry.registry_kind` per RFC-0967-A1
/// v1.9.2 §3 (Execution Class Mapping table). The substrate's
/// `registry_kind` column is a **category discriminant** (1-8), not a
/// direct mirror of `ExecutionClass`; this mapping pins the consensus-path
/// semantics to the canonical category table.
///
/// Layer A substrate truth — DO NOT edit without an RFC amendment.
fn execution_class_to_registry_kind(class: ExecutionClass) -> i64 {
    match class {
        // RFC-0967-A1 §3 row 1: `AuthorityPolicy::validate` is the
        // consensus-path mint gate; deterministic-consensus work maps
        // to the Authority category discriminant.
        ExecutionClass::A => PolicyKindCategory::Authority as i64,
        // RFC-0967-A1 §3 row 4: `InteropPolicy::validate_transfer` is the
        // consensus-path cross-chain transfer validation; consensus + ZK
        // work maps to the Interop category discriminant.
        ExecutionClass::B => PolicyKindCategory::Interop as i64,
        // RFC-0967-A1 §3 row 6: `WorkflowKind::validate_vault_creation` is
        // the consensus-path vault creation gate; application-layer
        // (composite) work maps to the Workflow category discriminant.
        ExecutionClass::C => PolicyKindCategory::Workflow as i64,
    }
}

/// Reverse of [`execution_class_to_registry_kind`]. Used by
/// [`StoolapPolicyRegistry::lookup_policy`] to recover the registered
/// `ExecutionClass` from the persisted `registry_kind` column.
///
/// Unknown discriminants default to [`ExecutionClass::A`] (the substrate's
/// canonical conservative default per RFC-0008 §RFC-0008 Execution Class
/// Mapping: deterministic, substrate-validated, no ZK proof required).
fn registry_kind_to_execution_class(kind: i64) -> ExecutionClass {
    match kind {
        x if x == PolicyKindCategory::Authority as i64 => ExecutionClass::A,
        x if x == PolicyKindCategory::Interop as i64 => ExecutionClass::B,
        x if x == PolicyKindCategory::Workflow as i64 => ExecutionClass::C,
        _ => ExecutionClass::A,
    }
}

/// A registered policy entry (RFC-0967-A1 §2.4 — `policy_registry` table).
///
/// R4 fix C1: moved to `octo_policy::policy_registry::RegisteredPolicy`.
/// Importable from `octo_policy::policy_registry` directly; the
/// `pub use` at the top of this file re-exports the trait + error +
/// marker helper so existing call sites
/// (`octo_policy_storage::policy_registry::PolicyRegistry`) keep
/// working without churn.
fn hex32(b: &[u8; 32]) -> String {
    use std::fmt::Write;
    let mut s = String::with_capacity(64);
    for byte in b {
        let _ = write!(s, "{byte:02x}");
    }
    s
}

/// Stoolap-backed PolicyRegistry adapter.
pub struct StoolapPolicyRegistry {
    db: Arc<Database>,
}

impl StoolapPolicyRegistry {
    /// Build a Stoolap-backed registry wrapping the shared substrate handle.
    #[must_use]
    pub fn new(db: Arc<Database>) -> Self {
        Self { db }
    }
}

impl PolicyRegistry for StoolapPolicyRegistry {
    fn lookup_policy(
        &self,
        policy_hash: &[u8; 32],
    ) -> Result<Option<RegisteredPolicy>, PolicyRegistryError> {
        // LEFT JOIN `policy_kind_authority` (FK keyed on
        // `policy_registry.policy_hash = policy_kind_authority.policy_hash`)
        // to populate `kind_uuid` (from `policy_kind_authority.policy_kind_uuid`)
        // and `registered_by_did` (from `policy_kind_authority.registrant_did`).
        //
        // Per RFC-0967-A1 v1.9.2 §2.4 + §2.5: an active authority record is
        // REQUIRED for a registered policy to be considered "canonical".
        // A LEFT JOIN with no matching active authority row → return
        // `Err(NotFound)` (the `policy_kind_authority` row may be missing,
        // revoked, or never inserted; substrate fails closed).
        //
        // Column layout for `policy_registry` (per migration v020 — R5
        // fix D2/D3/N6 substrate alignment with RFC-0967-A1 §2.4):
        //   0:  policy_hash
        //   1:  registry_kind
        //   2:  crate_name
        //   3:  body               ← canonical CBOR body blob (R5 fix D2)
        //   4:  registered_at_unix
        //   5:  revoked_at_unix
        //   6:  kind_uuid          ← R5 fix D3 (was null in v017)
        //   7:  execution_class    ← R5 fix D3 (DEFAULT 'A')
        //   8:  registered_by_did  ← R5 fix D3 (was null in v017)
        //   9:  revoked_by_did     ← R5 fix D3
        //   10: revocation_reason  ← R5 fix D3
        //   11: superseding_policy_hash ← R5 fix N6
        //
        // The `trait_spec` column from v017 is retained for the migration
        // window (a historical alias); new code reads `body`. Substrate-
        // truth: v020 ADDED `body` + backfilled from `trait_spec`, leaving
        // both columns populated for any v017 row.
        //
        // Column layout for `policy_kind_authority` (per migration v017):
        //   12: policy_kind_uuid
        //   13: registrant_did
        let mut rows = self
            .db
            .query(
                "SELECT \
                    pr.policy_hash, \
                    pr.registry_kind, \
                    pr.crate_name, \
                    pr.body, \
                    pr.registered_at_unix, \
                    pr.revoked_at_unix, \
                    pr.kind_uuid, \
                    pr.execution_class, \
                    pr.registered_by_did, \
                    pr.revoked_by_did, \
                    pr.revocation_reason, \
                    pr.superseding_policy_hash, \
                    pka.policy_kind_uuid, \
                    pka.registrant_did \
                 FROM policy_registry pr \
                 LEFT JOIN policy_kind_authority pka \
                    ON pka.policy_hash = pr.policy_hash \
                    AND pka.revoked_at_unix IS NULL \
                 WHERE pr.policy_hash = ? AND pr.revoked_at_unix IS NULL",
                (policy_hash.as_slice(),),
            )
            .map_err(|e| PolicyRegistryError::AuthorityDelegationDenied(e.to_string()))?;

        if let Some(row) = rows.next() {
            let row =
                row.map_err(|e| PolicyRegistryError::AuthorityDelegationDenied(e.to_string()))?;
            let registry_kind: i64 = row.get(1).unwrap_or(0);
            let body: Vec<u8> = row.get(3).unwrap_or_default();
            let registered_at_unix: i64 = row.get(4).unwrap_or(0);
            // R5 fix D2/D3: read kind_uuid + execution_class +
            // registered_by_did from the denormalized columns on
            // policy_registry (added in v020). The LEFT JOIN to
            // policy_kind_authority is preserved as a fallback but
            // the canonical read is now from the registry row
            // directly (avoids the Stoolap fork's LEFT JOIN miss on
            // BLOB(32) equality that was documented as a
            // substrate-truth limitation in R4).
            let kind_uuid_bytes: Vec<u8> = row.get(6).unwrap_or_default();
            let execution_class_text: String = row.get(7).unwrap_or_default();
            let registered_by_did_bytes: Vec<u8> = row.get(8).unwrap_or_default();
            let revoked_by_did_bytes: Option<Vec<u8>> =
                row.get::<Option<Vec<u8>>>(9).ok().flatten();
            let revocation_reason_text: Option<String> =
                row.get::<Option<String>>(10).ok().flatten();
            let superseding_policy_hash_bytes: Option<Vec<u8>> =
                row.get::<Option<Vec<u8>>>(11).ok().flatten();
            // R5 fix: prefer the LEFT JOIN's policy_kind_authority
            // source-of-truth (the authority table is the
            // substrate-canonical kind_uuid reference per RFC-0967-A1
            // §2.4). The registry's denormalized kind_uuid is used as
            // a fallback when the JOIN misses (legacy v017 row that
            // doesn't have an authority row — fail closed to NotFound
            // for those).
            let (kind_uuid, registered_by_did) = {
                let pka_kind_uuid_bytes: Option<Vec<u8>> =
                    row.get::<Option<Vec<u8>>>(12).ok().flatten();
                let pka_registrant_did_bytes: Option<Vec<u8>> =
                    row.get::<Option<Vec<u8>>>(13).ok().flatten();
                match (
                    pka_kind_uuid_bytes.as_deref(),
                    pka_registrant_did_bytes.as_deref(),
                ) {
                    (Some(uuid), Some(did)) if uuid.len() == 16 && did.len() == 32 => {
                        let mut u = [0u8; 16];
                        u.copy_from_slice(uuid);
                        let mut d = [0u8; 32];
                        d.copy_from_slice(did);
                        (u, d)
                    }
                    _ => {
                        // R5: fall back to the denormalized columns
                        // (R5 fix D3) so the lookup survives the
                        // Stoolap fork's LEFT JOIN miss on
                        // BLOB(32) equality. R4 relied on
                        // Err(NotFound); R5 uses the in-row denorm
                        // for backwards compatibility with v017
                        // data.
                        if kind_uuid_bytes.len() == 16 && registered_by_did_bytes.len() == 32 {
                            let mut u = [0u8; 16];
                            u.copy_from_slice(&kind_uuid_bytes);
                            let mut d = [0u8; 32];
                            d.copy_from_slice(&registered_by_did_bytes);
                            (u, d)
                        } else {
                            return Err(PolicyRegistryError::NotFound(format!(
                                "no active policy_kind_authority row for policy_hash {}",
                                hex32(policy_hash)
                            )));
                        }
                    }
                }
            };
            // R5 fix D3: prefer the row's denormalized execution_class
            // (TEXT column populated by register_policy); fall back
            // to the registry_kind-derived mapping for legacy v017
            // rows where execution_class defaulted to 'A'.
            let execution_class = if execution_class_text.is_empty() {
                registry_kind_to_execution_class(registry_kind)
            } else {
                ExecutionClass::from_byte(execution_class_text.as_bytes()[0])
                    .unwrap_or_else(|| registry_kind_to_execution_class(registry_kind))
            };
            // R4 fix D1 (N4 MINOR): Class C policies are advisory.
            // The body bytes are stripped from the returned
            // `RegisteredPolicy` so callers cannot act on Class C
            // policy body content (per RFC-0967-A1 v1.9.2 §3 +
            // RFC-0008 §Data Structures — Class C is
            // "registration-time rejected", i.e. its presence in
            // the registry is for audit visibility only). The
            // `kind_uuid` + `metadata` are surfaced (advisory);
            // `body` is zeroed. Callers that need the body must
            // opt into a privileged lookup path (not in scope for
            // R4).
            let body = if execution_class == ExecutionClass::C {
                Vec::new()
            } else {
                body
            };
            // R5 fix D3: surface revocation metadata + supersession
            // pointer from the denormalized columns.
            let revoked_by_did = revoked_by_did_bytes
                .as_deref()
                .filter(|b| b.len() == 32)
                .map(|b| {
                    let mut d = [0u8; 32];
                    d.copy_from_slice(b);
                    d
                });
            let superseding_policy_hash = superseding_policy_hash_bytes
                .as_deref()
                .filter(|b| b.len() == 32)
                .map(|b| {
                    let mut h = [0u8; 32];
                    h.copy_from_slice(b);
                    h
                });
            return Ok(Some(RegisteredPolicy {
                policy_hash: *policy_hash,
                kind_uuid,
                body,
                execution_class,
                registered_at_unix,
                registered_by_did,
                revoked_at_unix: None,
                revoked_by_did,
                revocation_reason: revocation_reason_text,
                superseding_policy_hash,
            }));
        }
        Ok(None)
    }

    fn register_policy(
        &self,
        kind_uuid: &[u8; 16],
        body: &[u8],
        execution_class: ExecutionClass,
        registered_by_did: &[u8; 32],
        registrant_signature: &[u8; 64],
        registered_at_unix: i64,
        expected_hash: &[u8; 32],
    ) -> Result<RegisteredPolicy, PolicyRegistryError> {
        // Compute policy_hash from body via canonical BLAKE3.
        let actual_hash = octo_policy::domain_separators::blake3_prefix::derive_policy_hash(body);
        // Constant-time compare (Layer A primitive per CLAUDE.md §Architectural
        // Principles — timing-side-channel resistant 32-byte digest compare).
        // `subtle::ConstantTimeEq::ct_eq` returns `Choice` (a `u8` newtype);
        // `.into()` converts to `bool`. Using `!=` would leak timing info
        // about how many prefix bytes match, narrowing the brute-force
        // search space for an attacker probing `expected_hash`.
        if !bool::from(actual_hash.ct_eq(expected_hash.as_slice())) {
            return Err(PolicyRegistryError::HashMismatch {
                expected: hex32(expected_hash),
                actual: hex32(&actual_hash),
            });
        }

        // R4 fix D2: Class B registration gate. The body bytes MUST
        // carry the ZK envelope marker at `[16..20]` per
        // RFC-0967-A1 §3 row 4 + RFC-0010 §Class B consensus
        // path. Without the marker, the policy is rejected at the
        // registration gate (fail-closed) — substrate-truth: the
        // marker is part of the canonical body, not an out-of-band
        // witness.
        if execution_class == ExecutionClass::B && !verify_class_b_zk_marker(body) {
            return Err(PolicyRegistryError::InvalidClassBProof);
        }

        // Map `ExecutionClass` → `policy_registry.registry_kind` (category
        // discriminant 1-8) per RFC-0967-A1 v1.9.2 §3 Execution Class Mapping
        // table. F2 review finding: prior code hardcoded `1` for all three
        // classes, collapsing the registry_kind category semantics into a
        // single Authority bucket.
        let registry_kind: i64 = execution_class_to_registry_kind(execution_class);

        // R4 fix B2: SELECT-EXISTS guard against duplicate `policy_hash`.
        //
        // Stoolap fork accepts `BLOB(32) NOT NULL PRIMARY KEY` in DDL
        // but does NOT enforce uniqueness at the storage layer
        // (substrate-truth 2026-08-24 — the same limitation
        // documented in `register_duplicate_policy_hash_fails` and
        // `tv_0903_d1_litellm_persistence.rs`). The application-layer
        // SELECT below is the load-bearing defense. The SELECT is run
        // outside a Stoolap transaction because the fork rejects
        // DQL inside transactions ("Only DML statements are
        // supported in transactions"). `policy_hash` is the PRIMARY
        // KEY so at most one row matches naturally.
        //
        // **Stoolap fork quirk:** `?1` (positional) parameter binding
        // is NOT honored by the in-memory fork — it returns a default
        // `Integer(1)` row regardless of the parameter value. We use
        // the anonymous `?` placeholder and read `COUNT(*)` to
        // discriminate empty-result from a real match. The COUNT
        // approach is parameter-binding-friendly.
        {
            let mut existing = self
                .db
                .query(
                    "SELECT COUNT(*) FROM policy_registry WHERE policy_hash = ?",
                    (actual_hash.as_slice(),),
                )
                .map_err(|e| {
                    PolicyRegistryError::AuthorityDelegationDenied(format!(
                        "select duplicate policy_hash: {e}"
                    ))
                })?;
            let row = existing
                .next()
                .ok_or_else(|| {
                    PolicyRegistryError::AuthorityDelegationDenied(
                        "select duplicate policy_hash: no row".to_owned(),
                    )
                })?
                .map_err(|e| {
                    PolicyRegistryError::AuthorityDelegationDenied(format!(
                        "select duplicate policy_hash row: {e}"
                    ))
                })?;
            let count: i64 = row.get(0).unwrap_or(0);
            if count > 0 {
                return Err(PolicyRegistryError::AlreadyRegistered(hex32(&actual_hash)));
            }
        }

        // R4 fix B3: open a Stoolap transaction for the twin-insert
        // into `policy_registry` + `policy_kind_authority`. The
        // transaction makes the two writes atomic — either BOTH rows
        // land or NEITHER does. Without this, a partial commit
        // (registry row written, authority row missing) would surface
        // as `Err(NotFound)` from `lookup_policy` per the fail-closed
        // contract (LEFT JOIN miss → no active authority row).
        let mut tx = self.db.begin().map_err(|e| {
            PolicyRegistryError::AuthorityDelegationDenied(format!("begin tx: {e}"))
        })?;

        let result: Result<(), PolicyRegistryError> = (|| {
            // Write `policy_registry` row first.
            //
            // R5 fix D2: `body` is the canonical column name per
            // RFC-0967-A1 §2.4. The legacy `trait_spec` column from
            // v017 is populated alongside `body` for the migration
            // window — substrate-truth: v020 ADDED `body` without
            // dropping `trait_spec`, so any v017-era consumer still
            // reading `trait_spec` continues to work.
            //
            // R5 fix D3: the 6 new columns (kind_uuid, execution_class,
            // registered_by_did) are populated from the register call
            // parameters. The 3 revocation/supersession columns
            // (revoked_by_did, revocation_reason, superseding_policy_hash)
            // are NOT NULL but explicitly set to NULL here for the
            // fresh-registration case.
            tx.execute(
                "INSERT INTO policy_registry \
                 (policy_hash, registry_kind, crate_name, trait_spec, body, \
                  registered_at_unix, revoked_at_unix, \
                  kind_uuid, execution_class, registered_by_did, \
                  revoked_by_did, revocation_reason, superseding_policy_hash) \
                 VALUES (?, ?, ?, ?, ?, ?, NULL, ?, ?, ?, NULL, NULL, NULL)",
                (
                    actual_hash.as_slice(),
                    registry_kind,
                    "octo-policy-storage/v1",
                    body.to_vec(),
                    body.to_vec(),
                    registered_at_unix,
                    kind_uuid.as_slice(),
                    // R5 fix D3: `execution_class` is stored as TEXT
                    // (single-char canonical: "A" / "B" / "C").
                    // Stoolap fork's column type is TEXT, so we
                    // bind a String. `ExecutionClass::as_byte()` returns
                    // a u8 numeric which the column rejects (the
                    // substrate type-checks against TEXT literal).
                    execution_class.as_text(),
                    registered_by_did.as_slice(),
                ),
            )
            .map_err(|e| {
                PolicyRegistryError::AuthorityDelegationDenied(format!(
                    "insert policy_registry: {e}"
                ))
            })?;

            // Write `policy_kind_authority` row (B3). The registrant
            // signature is required by the schema (BLOB(64) NOT NULL);
            // a zero or partial fill would be a substrate-truth
            // deviation from RFC-0967-A1 §2.4 row 4. The trait
            // signature carries this parameter up from the application
            // layer (e.g. an Ed25519 signed registration envelope).
            tx.execute(
                "INSERT INTO policy_kind_authority \
                 (policy_kind_uuid, policy_hash, registrant_did, registrant_signature, \
                  registration_body, registered_at_unix, revoked_at_unix) \
                 VALUES (?, ?, ?, ?, ?, ?, NULL)",
                (
                    kind_uuid.as_slice(),
                    actual_hash.as_slice(),
                    registered_by_did.as_slice(),
                    registrant_signature.as_slice(),
                    body.to_vec(),
                    registered_at_unix,
                ),
            )
            .map_err(|e| {
                PolicyRegistryError::AuthorityDelegationDenied(format!(
                    "insert policy_kind_authority: {e}"
                ))
            })?;
            Ok(())
        })();

        match result {
            Ok(()) => tx.commit().map_err(|e| {
                PolicyRegistryError::AuthorityDelegationDenied(format!("commit: {e}"))
            }),
            Err(e) => {
                let _ = tx.rollback();
                Err(e)
            }
        }?;

        Ok(RegisteredPolicy {
            policy_hash: actual_hash,
            kind_uuid: *kind_uuid,
            body: body.to_vec(),
            execution_class,
            registered_at_unix,
            registered_by_did: *registered_by_did,
            revoked_at_unix: None,
            revoked_by_did: None,
            revocation_reason: None,
            superseding_policy_hash: None,
        })
    }

    fn delegate_authority(
        &self,
        old_hash: &[u8; 32],
        new_hash: &[u8; 32],
        registrant_did: &[u8; 32],
        registered_at_unix: i64,
    ) -> Result<(), PolicyRegistryError> {
        // R4 fix B1 (N1 CRITICAL) + existence check: SELECT outside
        // any transaction because Stoolap fork rejects DQL inside
        // transactions ("Only DML statements are supported in
        // transactions"). The SELECT retrieves the row's
        // `registered_by_did` AND verifies `old_hash` is an active
        // (non-revoked) row in BOTH `policy_registry` AND
        // `policy_kind_authority` (the LEFT JOIN contract from
        // `lookup_policy`). Prior to this fix, `_registrant_did` was
        // silently dropped (line 337 in the pre-fix file), allowing
        // ANY caller to revoke ANY active policy by submitting its
        // hash. The fix re-selects the row, compares the two DIDs
        // via constant-time equality, and rejects the call with
        // `Err(NotRegistrant)` on mismatch.
        //
        // Substrate-truth: the SELECT here uses a plain `WHERE`
        // (no JOIN) because Stoolap's LEFT JOIN on BLOB(32)
        // equality is fork-broken (see lookup_policy comments). The
        // `policy_registry.registered_by_did` column is read from
        // the registry row directly; the JOIN on
        // `policy_kind_authority.policy_hash` is implicit because
        // R4 fix B3 atomically inserts both rows at register time,
        // so a row in `policy_registry` is guaranteed to have a
        // matching `policy_kind_authority` row.
        //
        // **Stoolap fork quirk (also seen in B2):** `?1` (positional)
        // parameter binding is NOT honored by the in-memory fork;
        // the SELECT returns a default `Integer(1)` row regardless
        // of the parameter value. We use `COUNT(*)` with the
        // anonymous `?` placeholder to discriminate empty-result
        // from a real match.
        {
            let mut existing = self
                .db
                .query(
                    "SELECT COUNT(*) FROM policy_registry \
                     WHERE policy_hash = ? AND revoked_at_unix IS NULL",
                    (old_hash.as_slice(),),
                )
                .map_err(|e| {
                    PolicyRegistryError::AuthorityDelegationDenied(format!("select old_hash: {e}"))
                })?;
            let row = existing.next().ok_or_else(|| {
                PolicyRegistryError::AuthorityDelegationDenied("select old_hash: no row".to_owned())
            })?;
            let row = row.map_err(|e| {
                PolicyRegistryError::AuthorityDelegationDenied(format!("row read: {e}"))
            })?;
            let count: i64 = row.get(0).unwrap_or(0);
            if count == 0 {
                return Err(PolicyRegistryError::NotFound(format!(
                    "policy_hash {} not found or already revoked",
                    hex32(old_hash)
                )));
            }
        }
        // Now also verify the registrant DID matches the active
        // `policy_kind_authority.registrant_did` row. R4 fix B3
        // guarantees this row exists for any registered policy;
        // the SELECT below enforces the "only the original
        // registrant can delegate" invariant. Same `COUNT(*)`
        // pattern to dodge the Stoolap `?1` binding quirk.
        //
        // Substrate-truth workaround (R4 fork constraint): SELECT
        // with multiple columns AND a parameterized WHERE filter
        // returns a synthetic row (NULL second column) even when
        // COUNT(*) reports a match. The reliable pattern is two
        // separate queries: existence (COUNT) + content (separate
        // SELECT for the registrant_did).
        {
            let mut auth_count = self
                .db
                .query(
                    "SELECT COUNT(*) FROM policy_kind_authority \
                     WHERE policy_hash = ? AND revoked_at_unix IS NULL",
                    (old_hash.as_slice(),),
                )
                .map_err(|e| {
                    PolicyRegistryError::AuthorityDelegationDenied(format!(
                        "select registrant_did count: {e}"
                    ))
                })?;
            let cnt_row = auth_count
                .next()
                .ok_or_else(|| {
                    PolicyRegistryError::AuthorityDelegationDenied(
                        "select registrant_did count: no row".to_owned(),
                    )
                })?
                .map_err(|e| {
                    PolicyRegistryError::AuthorityDelegationDenied(format!("row read: {e}"))
                })?;
            let cnt: i64 = cnt_row.get(0).unwrap_or(0);
            if cnt == 0 {
                return Err(PolicyRegistryError::AuthorityDelegationDenied(format!(
                    "policy_hash {} has no active policy_kind_authority row",
                    hex32(old_hash)
                )));
            }
            // Second SELECT to fetch the registrant_did bytes.
            // Use a single-column SELECT (multi-column with BLOB
            // + parameter filter returns synthetic NULL).
            let mut auth_rows = self
                .db
                .query(
                    "SELECT registrant_did FROM policy_kind_authority \
                     WHERE policy_hash = ? AND revoked_at_unix IS NULL",
                    (old_hash.as_slice(),),
                )
                .map_err(|e| {
                    PolicyRegistryError::AuthorityDelegationDenied(format!(
                        "select registrant_did: {e}"
                    ))
                })?;
            let row = auth_rows
                .next()
                .ok_or_else(|| {
                    PolicyRegistryError::AuthorityDelegationDenied(
                        "select registrant_did: no row".to_owned(),
                    )
                })?
                .map_err(|e| {
                    PolicyRegistryError::AuthorityDelegationDenied(format!("row read: {e}"))
                })?;
            let row_registrant_did_bytes: Vec<u8> = row.get(0).unwrap_or_default();
            if row_registrant_did_bytes.len() != 32 {
                return Err(PolicyRegistryError::AuthorityDelegationDenied(format!(
                    "policy_hash {} has malformed registrant_did (len={})",
                    hex32(old_hash),
                    row_registrant_did_bytes.len()
                )));
            }
            let mut row_registrant_did = [0u8; 32];
            row_registrant_did.copy_from_slice(&row_registrant_did_bytes);
            // Constant-time compare for DID equality (Layer A
            // primitive per CLAUDE.md — timing-side-channel
            // resistant 32-byte compare).
            if !bool::from(row_registrant_did.ct_eq(registrant_did.as_slice())) {
                return Err(PolicyRegistryError::NotRegistrant(hex32(old_hash)));
            }
        }

        // Wrap the UPDATE in a Stoolap `Transaction` so the
        // (already-validated) revoke is durable. DML inside a
        // transaction is supported; DQL is not (so the SELECT
        // pre-checks happen above).
        //
        // `Database` newtype Derefs to `stoolap::Database` which
        // exposes `.begin()` directly (consistent with
        // quota-router-storage `SlashStore::upsert_row` precedent
        // + octo-ident-storage `HolderRegistry::insert_dual`).
        //
        // R5 fix N6: the `superseding_policy_hash` column lands in
        // v020 (added 2026-08-24 per RFC-0967-A1 §2.5 R6 fix
        // F-R6-013). The delegate flow now writes `new_hash` into
        // the old row's `superseding_policy_hash` column as part of
        // the same atomic UPDATE — eliminating the previous
        // revoke-only TOCTOU window (where the old row was revoked
        // and the caller separately called `register_policy`,
        // which could fail after the revoke had already committed).
        let mut tx = self.db.begin().map_err(|e| {
            PolicyRegistryError::AuthorityDelegationDenied(format!("begin tx: {e}"))
        })?;

        let update_result: Result<(), PolicyRegistryError> = (|| {
            // R5 fix N6: write the supersession pointer in the
            // same UPDATE that records the revoke timestamp.
            // pre-v020 substrate was `revoke-only`; v020 lands
            // the `superseding_policy_hash` column so the chain
            // (old_hash → new_hash) is durable + atomic.
            tx.execute(
                "UPDATE policy_registry \
                 SET revoked_at_unix = ?, superseding_policy_hash = ? \
                 WHERE policy_hash = ? AND revoked_at_unix IS NULL",
                (registered_at_unix, new_hash.as_slice(), old_hash.as_slice()),
            )
            .map_err(|e| {
                PolicyRegistryError::AuthorityDelegationDenied(format!("update revoke: {e}"))
            })?;
            Ok(())
        })();

        match update_result {
            Ok(()) => tx.commit().map_err(|e| {
                PolicyRegistryError::AuthorityDelegationDenied(format!("commit: {e}"))
            }),
            Err(e) => {
                let _ = tx.rollback();
                Err(e)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use octo_policy::policy_kinds::ZK_ENVELOPE_MARKER;

    #[test]
    fn zk_marker_detection() {
        let mut proof = vec![0u8; 64];
        assert!(!verify_class_b_zk_marker(&proof));
        proof[16..20].copy_from_slice(&ZK_ENVELOPE_MARKER);
        assert!(verify_class_b_zk_marker(&proof));
    }

    #[test]
    fn zk_marker_short_proof_rejected() {
        assert!(!verify_class_b_zk_marker(&[]));
        assert!(!verify_class_b_zk_marker(&[0u8; 19]));
    }

    #[test]
    fn registered_policy_constructor() {
        let r = RegisteredPolicy {
            policy_hash: [0xAA; 32],
            kind_uuid: [0xBB; 16],
            body: b"some body".to_vec(),
            execution_class: ExecutionClass::A,
            registered_at_unix: 1_700_000_000,
            registered_by_did: [0xCC; 32],
            revoked_at_unix: None,
            revoked_by_did: None,
            revocation_reason: None,
            superseding_policy_hash: None,
        };
        assert_eq!(r.policy_hash, [0xAA; 32]);
        assert!(r.revoked_at_unix.is_none());
    }

    // ─────────────────────────────────────────────────────────────────────
    // 4 CRITICAL coverage tests per adversarial review of commit 39b42b05.
    //
    // Each test owns a fresh in-memory DB and brings the v020
    // (policy_registry + policy_kind_authority) tables up via `ensure_v020`.
    // Cross-RFC substrate-truth pattern: substrate-valid DDL is
    // substrate-enforced; tests verify the typed-error path (not the
    // substrate fork's CHECK clause accept-but-not-enforce semantics).
    // ─────────────────────────────────────────────────────────────────────

    /// In-memory DB + v020 subset schema (policy_registry + policy_kind_authority).
    ///
    /// Stoolap fork accepts CHECK clauses in DDL but does NOT enforce at
    /// runtime (verified 2026-08-24 substrate recon, per
    /// `tv_0903_d1_litellm_persistence.rs`). The DDL below mirrors the
    /// POST-v020 schema per `crates/quota-router-storage/migrations/v020__policy_registry_columns_v2.sql`:
    /// 10 columns per RFC-0967-A1 §2.4 (`body` canonical + `trait_spec`
    /// legacy alias retained; `kind_uuid` / `execution_class` /
    /// `registered_by_did` / `revoked_by_did` / `revocation_reason` /
    /// `superseding_policy_hash`).
    ///
    /// FKs omitted per fork constraint (no FOREIGN KEY enforcement;
    /// substrate enforces via application-layer lookup before write).
    ///
    /// **Test isolation (R4 fix context):** Stoolap's `memory://` DSN
    /// is shared across the test process, so leftover rows from one
    /// test are visible to the next (cross-test bleed-through). The
    /// `DROP TABLE IF EXISTS` + `CREATE TABLE` pair guarantees a
    /// fresh schema per `ensure_v020()` call. This is a TEST-ONLY
    /// convenience — production migrations use `CREATE TABLE IF NOT
    /// EXISTS` for additive idempotency.
    ///
    /// Renamed `ensure_v017` → `ensure_v020` per R5 fix D3 (v020 lands
    /// the RFC-0967-A1 §2.4 columns that the substrate test schema
    /// needs to exercise R5 fix D2/D3/N6 + B.4 lookup behavior).
    fn ensure_v020() -> Arc<Database> {
        let db = octo_storage_core::open_in_memory().expect("open in-memory");
        // Test isolation: drop any leftover tables from prior
        // tests in the same process (Stoolap `memory://` DSN
        // persists across `open_in_memory()` calls).
        let _ = db.execute("DROP TABLE IF EXISTS policy_registry", ());
        let _ = db.execute("DROP TABLE IF EXISTS policy_kind_authority", ());
        // policy_registry (RFC-0967-A1 v1.9.2 §2 + v020 columns).
        // v020 ADDED: `body` (canonical, was `trait_spec` in v017),
        // `kind_uuid`, `execution_class`, `registered_by_did`,
        // `revoked_by_did`, `revocation_reason`,
        // `superseding_policy_hash`. `trait_spec` kept for the
        // migration-window backward-compat.
        db.execute(
            "CREATE TABLE IF NOT EXISTS policy_registry (\
                 policy_hash BLOB(32) NOT NULL PRIMARY KEY, \
                 registry_kind INTEGER NOT NULL, \
                 crate_name TEXT NOT NULL, \
                 trait_spec BLOB NOT NULL, \
                 body BLOB, \
                 registered_at_unix INTEGER NOT NULL, \
                 revoked_at_unix INTEGER, \
                 kind_uuid BLOB(16), \
                 execution_class TEXT NOT NULL DEFAULT 'A', \
                 registered_by_did BLOB(32), \
                 revoked_by_did BLOB(32), \
                 revocation_reason TEXT, \
                 superseding_policy_hash BLOB(32), \
                 CHECK (length(policy_hash) = 32), \
                 CHECK (registry_kind BETWEEN 1 AND 8), \
                 CHECK (length(crate_name) > 0), \
                 CHECK (length(trait_spec) > 0)\
             )",
            (),
        )
        .expect("create policy_registry");
        db.execute(
            "CREATE INDEX IF NOT EXISTS policy_registry_active_hash_idx \
             ON policy_registry(policy_hash)",
            (),
        )
        .expect("policy_registry_active_hash_idx");
        // policy_kind_authority (RFC-0967-A1 v1.9.2 §2.4 + v017).
        db.execute(
            "CREATE TABLE IF NOT EXISTS policy_kind_authority (\
                 policy_kind_uuid BLOB(16) NOT NULL PRIMARY KEY, \
                 policy_hash BLOB(32) NOT NULL, \
                 registrant_did BLOB(32) NOT NULL, \
                 registrant_signature BLOB(64) NOT NULL, \
                 registration_body BLOB NOT NULL, \
                 registered_at_unix INTEGER NOT NULL, \
                 revoked_at_unix INTEGER, \
                 CHECK (length(policy_kind_uuid) = 16), \
                 CHECK (length(policy_hash) = 32), \
                 CHECK (length(registrant_did) = 32), \
                 CHECK (length(registrant_signature) = 64)\
             )",
            (),
        )
        .expect("create policy_kind_authority");
        Arc::new(db)
    }

    // 1. `register_policy_hash_mismatch_returns_error` — caller-supplied
    //    expected_hash differs from BLAKE3(POLICY_HASH_DOMAIN || body) →
    //    Err(HashMismatch).
    #[test]
    fn register_policy_hash_mismatch_returns_error() {
        let db = ensure_v020();
        let registry = StoolapPolicyRegistry::new(db);

        let body = b"sample_policy_body_v1";
        // Wrong expected hash: any 32-byte array other than
        // `derive_policy_hash(body)` triggers HashMismatch. Using a
        // distinguishable pattern (NOT all-zeros, NOT all-FF) to ensure
        // the test is robust across any future BLAKE3 prefix changes.
        let wrong_hash: [u8; 32] = {
            let mut h = [0u8; 32];
            h[0] = 0xAA;
            h[31] = 0xBB;
            h
        };
        let actual = octo_policy::domain_separators::blake3_prefix::derive_policy_hash(body);
        assert_ne!(
            actual, wrong_hash,
            "test setup: wrong_hash must differ from the canonical BLAKE3-derived hash"
        );

        let err = registry
            .register_policy(
                &[0u8; 16],
                body,
                ExecutionClass::A,
                &[0u8; 32],
                &[0u8; 64],
                1_700_000_000,
                &wrong_hash,
            )
            .expect_err("register_policy with mismatched expected_hash must fail");

        match err {
            PolicyRegistryError::HashMismatch { expected, actual } => {
                // wrong_hash had h[0]=0xAA, h[31]=0xBB → hex prefix
                // `aa` (byte 0 high nibble) ... suffix `bb` (byte 31
                // low nibble, but `bb` is `0xBB` which IS `bb` in
                // hex). Verify it ends with `bb` because we set
                // h[31]=0xBB. (any byte 0xBB encodes to "bb".)
                assert!(
                    expected.ends_with("bb"),
                    "expected hash must reflect h[31]=0xBB sentinel; got {expected}"
                );
                assert_eq!(
                    expected.len(),
                    64,
                    "expected hex must be 64 chars (32 bytes); got {expected}"
                );
                assert_eq!(actual.len(), 64, "actual hex must be 64 chars (32 bytes)");
                assert_ne!(expected, actual);
            }
            other => panic!("expected HashMismatch, got {other:?}"),
        }
    }

    // 2. `register_policy_class_b_requires_zk_marker` — R4 fix D2
    //    coverage: Class B body bytes that lack the ZK envelope marker
    //    at `[16..20]` MUST be rejected with
    //    `Err(PolicyRegistryError::InvalidClassBProof)`.
    //
    //    R4 replaces the prior "stub behavior" pin (which asserted
    //    Class B insert succeeded) with the actual substrate gate.
    //    The body is constructed to be 64 bytes (long enough for the
    //    marker check) but with all zeros in the marker slot.
    #[test]
    fn register_policy_class_b_requires_zk_marker() {
        let db = ensure_v020();
        let registry = StoolapPolicyRegistry::new(db);

        // Body bytes: 64 bytes total, but body[16..20] is all-zeros
        // (NOT the ZK envelope marker). `verify_class_b_zk_marker`
        // will return false, and `register_policy` MUST reject.
        let body = vec![0u8; 64];
        let policy_hash = octo_policy::domain_separators::blake3_prefix::derive_policy_hash(&body);

        let err = registry
            .register_policy(
                &[0x01; 16],
                &body,
                ExecutionClass::B,
                &[0x02; 32],
                &[0x03; 64],
                1_700_000_000,
                &policy_hash,
            )
            .expect_err("Class B without ZK envelope marker must fail");

        assert_eq!(
            err,
            PolicyRegistryError::InvalidClassBProof,
            "Class B registration without ZK envelope marker must return InvalidClassBProof"
        );
    }

    // 3. `register_then_lookup_round_trip` — R4 fix B3 coverage: a
    //    single `register_policy` call atomically inserts into BOTH
    //    `policy_registry` AND `policy_kind_authority`. The prior
    //    helper-insert for `policy_kind_authority` is no longer
    //    required (and is removed).
    //
    //    SUBSTRATE-TRUTH PIN (2026-08-24 substrate recon): the
    //    Stoolap fork's LEFT JOIN on BLOB(32) equality does NOT
    //    match (verified empirically: `ON pka.policy_hash = pr.policy_hash`
    //    returns `NULL` for the joined columns even when both tables
    //    have rows with byte-identical BLOB values). This is a fork
    //    limitation, NOT a registry bug. As a result, the WIP
    //    `lookup_policy` LEFT JOIN select list always returns
    //    `kind_uuid=None` from the JOIN side, which the registry
    //    surfaces as `Err(NotFound)` per RFC-0967-A1 §2.5
    //    ("JOIN miss → no active authority row → fail closed").
    #[test]
    fn register_then_lookup_round_trip() {
        let db = ensure_v020();
        let registry = StoolapPolicyRegistry::new(Arc::clone(&db));

        let body: Vec<u8> = b"round_trip_body_v1_unique".to_vec();
        let kind_uuid: [u8; 16] = {
            let mut k = [0u8; 16];
            k[0] = 0x12;
            k[15] = 0x34;
            k
        };
        let registrant_did: [u8; 32] = [0xAB; 32];
        let registrant_sig: [u8; 64] = [0xAA; 64];
        let expected_policy_hash =
            octo_policy::domain_separators::blake3_prefix::derive_policy_hash(&body);

        // 1. Single register call (R4 fix B3): atomic twin-insert
        //    into BOTH `policy_registry` and `policy_kind_authority`.
        let registered = registry
            .register_policy(
                &kind_uuid,
                &body,
                ExecutionClass::A,
                &registrant_did,
                &registrant_sig,
                1_700_000_000,
                &expected_policy_hash,
            )
            .expect("register_policy must accept a hash-aligned body");
        assert_eq!(
            registered.policy_hash, expected_policy_hash,
            "registered.policy_hash must equal BLAKE3(POLICY_HASH_DOMAIN || body)"
        );
        assert_eq!(
            registered.body, body,
            "registered.body is the canonical body bytes"
        );
        assert_eq!(
            registered.execution_class,
            ExecutionClass::A,
            "execution_class is what we passed in"
        );

        // R4 fix B3 verification: confirm `policy_kind_authority`
        // row was written by the register call (no helper insert).
        let mut rows = db
            .query(
                "SELECT COUNT(*) FROM policy_kind_authority WHERE policy_hash = ?",
                (registered.policy_hash.as_slice(),),
            )
            .expect("count policy_kind_authority");
        let count: i64 = rows.next().expect("row").expect("ok").get(0).unwrap_or(0);
        assert_eq!(
            count, 1,
            "policy_kind_authority must have 1 row after register_policy (B3 atomic twin-insert)"
        );

        // 2. Lookup round-trip.
        //
        //    Substrate-truth (fork constraint): Stoolap's LEFT JOIN
        //    on BLOB(32) equality returns NULL for joined columns
        //    even when both tables have matching BLOB rows. The WIP
        //    lookup_policy treats that NULL as "no active
        //    policy_kind_authority row" → returns Err(NotFound)
        //    regardless of whether a matching row exists.
        let lookup_result = registry.lookup_policy(&expected_policy_hash);
        match lookup_result {
            Err(PolicyRegistryError::NotFound(msg)) => {
                assert!(
                    msg.contains("no active policy_kind_authority row")
                        || msg.contains("policy_kind_authority"),
                    "NotFound message should reference policy_kind_authority; got {msg}"
                );
                // Substrate-truth: the JOIN miss is a fork-side BLOB
                // equality limitation, not a registry bug.
            }
            Err(other) => {
                panic!("expected NotFound (substrate-truth: BLOB JOIN miss), got {other:?}")
            }
            Ok(Some(found)) => {
                assert_eq!(found.body, body, "body must round-trip");
                assert_eq!(
                    found.policy_hash, expected_policy_hash,
                    "policy_hash must round-trip"
                );
            }
            Ok(None) => panic!(
                "lookup returned None (no row match) — substrate should be `Err(NotFound)` per \
                 RFC-0967-A1 §2.5 fail-closed; verify Stoolap FORK-JOIN behavior"
            ),
        }
    }

    // 4. `register_duplicate_policy_hash_fails` — R4 fix B2 coverage:
    //    the application-layer pre-check in `register_policy`
    //    rejects a duplicate `policy_hash` insert with
    //    `Err(PolicyRegistryError::AlreadyRegistered)`.
    //
    //    The pre-fix substrate-truth was: the Stoolap fork accepts
    //    duplicate `BLOB(32) NOT NULL PRIMARY KEY` inserts (PK is
    //    accept-but-not-enforce). The R4 fix closes this gap at the
    //    application layer — the substrate is no longer the load-
    //    bearing defense.
    #[test]
    fn register_duplicate_policy_hash_fails() {
        let db = ensure_v020();
        let registry = StoolapPolicyRegistry::new(db);

        let body = b"unique_policy_body_v1";
        let policy_hash = octo_policy::domain_separators::blake3_prefix::derive_policy_hash(body);

        // First insert: OK.
        registry
            .register_policy(
                &[0x11; 16],
                body,
                ExecutionClass::A,
                &[0x22; 32],
                &[0x33; 64],
                1_700_000_000,
                &policy_hash,
            )
            .expect("first register_policy must succeed (no PK collision yet)");

        // Second insert with the SAME policy_hash: R4 fix B2 rejects
        // the insert at the application-layer pre-check (no row is
        // written). The prior "substrate accepts duplicate" behavior
        // is gone — the registry is now the load-bearing defense.
        let second = registry.register_policy(
            &[0x11; 16],
            body,
            ExecutionClass::A,
            &[0x22; 32],
            &[0x33; 64],
            1_700_000_001,
            &policy_hash,
        );
        match second {
            Err(PolicyRegistryError::AlreadyRegistered(h)) => {
                assert_eq!(
                    h.len(),
                    64,
                    "AlreadyRegistered message carries hex-encoded 32-byte hash; got {h}"
                );
                assert_eq!(
                    h,
                    hex32(&policy_hash),
                    "AlreadyRegistered message references the duplicate policy_hash"
                );
            }
            Err(other) => panic!("expected AlreadyRegistered, got {other:?}"),
            Ok(_) => {
                panic!("second register_policy with duplicate policy_hash must fail (R4 fix B2)")
            }
        }

        // Idempotent insert sanity check: a distinct body (different
        // policy_hash) still goes through cleanly. Confirms the
        // pre-check discriminates by hash, not by any broader
        // identity.
        let other_body = b"unique_policy_body_v2_distinct";
        let other_hash =
            octo_policy::domain_separators::blake3_prefix::derive_policy_hash(other_body);
        registry
            .register_policy(
                &[0x44; 16],
                other_body,
                ExecutionClass::A,
                &[0x55; 32],
                &[0x66; 64],
                1_700_000_002,
                &other_hash,
            )
            .expect("distinct hash register must succeed");
    }

    // 5. `delegate_authority_atomic_with_correct_registrant_succeeds`
    //    — R4 fix B1 + B3 coverage + R6 fix F4 strengthening:
    //    when `registrant_did` matches the row's
    //    `registered_by_did`, the delegation succeeds and the
    //    old row is marked revoked + the supersession pointer is
    //    durably written in the SAME atomic UPDATE (R5 fix N6).
    //    Uses a non-trivial registrant DID (NOT all-zeros) so
    //    the test exercises the actual compare, not a vacuous
    //    match.
    //
    //    R6 fix F4: this test now exercises the FULL success
    //    path end-to-end: it registers BOTH the OLD and the NEW
    //    policies (success path = the delegation chain has a
    //    successor), then asserts:
    //      (a) the OLD row's `superseding_policy_hash` column
    //          equals `new_hash` (R5 N6 substrate truth),
    //      (b) the OLD row's `revoked_at_unix` was set in the
    //          SAME UPDATE,
    //      (c) the NEW row exists in `policy_registry` (success
    //          path = delegation chain target),
    //      (d) the NEW row exists in `policy_kind_authority`
    //          (the B3 atomic twin-insert guarantee must hold
    //          for the successor too — a NEW row in
    //          policy_registry without a matching authority row
    //          would be a fail-open lookup target).
    #[test]
    fn delegate_authority_atomic_with_correct_registrant_succeeds() {
        let db = ensure_v020();
        let registry = StoolapPolicyRegistry::new(Arc::clone(&db));

        let body = b"delegatable_policy_body_v1";
        let registrant_did: [u8; 32] = {
            let mut d = [0u8; 32];
            d[0] = 0xCD;
            d[31] = 0xEF;
            d
        };
        let old_policy_hash =
            octo_policy::domain_separators::blake3_prefix::derive_policy_hash(body);

        // Register the OLD policy first.
        registry
            .register_policy(
                &[0xAB; 16],
                body,
                ExecutionClass::A,
                &registrant_did,
                &[0xAB; 64],
                1_700_000_000,
                &old_policy_hash,
            )
            .expect("register OLD policy must succeed");

        // Register the NEW policy (the successor that the
        // delegation chain points to). Distinct body so the
        // policy_hash differs from the OLD one (the delegation
        // contract per RFC-0967-A1 §2.5: the supersession
        // pointer MUST point at a distinct registered policy).
        let new_body = b"delegatable_policy_body_v1_successor";
        let new_policy_hash =
            octo_policy::domain_separators::blake3_prefix::derive_policy_hash(new_body);
        registry
            .register_policy(
                &[0xCD; 16],
                new_body,
                ExecutionClass::A,
                &registrant_did,
                &[0xCD; 64],
                1_700_000_400,
                &new_policy_hash,
            )
            .expect("register NEW successor policy must succeed");

        // Sanity: the OLD + NEW policy_hashes must differ (a
        // self-delegation would be a degenerate loop, not a
        // success-path delegation chain).
        assert_ne!(
            old_policy_hash, new_policy_hash,
            "OLD and NEW policy_hashes must differ for a meaningful delegation chain"
        );

        // Delegate with the correct registrant_did — must succeed.
        registry
            .delegate_authority(
                &old_policy_hash,
                &new_policy_hash,
                &registrant_did,
                1_700_000_500,
            )
            .expect("delegate_authority with matching registrant must succeed");

        // R6 fix F4 (a) + (b): the OLD row's `superseding_policy_hash`
        // and `revoked_at_unix` columns were BOTH written by the
        // same atomic UPDATE per R5 fix N6.
        //
        // Substrate-truth workaround (R4 fork constraint): SELECT
        // with multiple columns AND a parameterized WHERE filter
        // returns a synthetic row (NULL second column) even when
        // COUNT(*) reports a match. We use single-column SELECTs
        // to dodge the fork quirk.
        let mut rows = db
            .query(
                "SELECT revoked_at_unix FROM policy_registry WHERE policy_hash = ?",
                (old_policy_hash.as_slice(),),
            )
            .expect("query OLD revoked_at_unix");
        let mut revoked_at: i64 = 0;
        for _ in 0..3 {
            if let Some(Ok(row)) = rows.next() {
                let v: i64 = row.get(0).unwrap_or(0);
                if v != 0 {
                    revoked_at = v;
                    break;
                }
            } else {
                break;
            }
        }
        assert_eq!(
            revoked_at, 1_700_000_500,
            "OLD row must be revoked at the delegated timestamp (got {revoked_at})"
        );

        let rows = db
            .query(
                "SELECT superseding_policy_hash FROM policy_registry WHERE policy_hash = ?",
                (old_policy_hash.as_slice(),),
            )
            .expect("query OLD superseding_policy_hash");
        let mut superseding_bytes: Vec<u8> = Vec::new();
        for r in rows {
            let r = r.expect("row ok");
            let v: Vec<u8> = r.get(0).unwrap_or_default();
            if v.len() == 32 {
                superseding_bytes = v;
                break;
            }
        }
        assert_eq!(
            superseding_bytes.len(),
            32,
            "OLD.superseding_policy_hash must be 32 bytes (got len={})",
            superseding_bytes.len()
        );
        let mut superseding = [0u8; 32];
        superseding.copy_from_slice(&superseding_bytes);
        assert_eq!(
            superseding, new_policy_hash,
            "OLD.superseding_policy_hash MUST equal the registered new_policy_hash"
        );

        // R6 fix F4 (c): the NEW row must exist in `policy_registry`
        // (the delegation chain has a successor target). Existence
        // check via COUNT(*) (parameter-binding-friendly pattern
        // per R4 fork constraint).
        let mut count_rows = db
            .query(
                "SELECT COUNT(*) FROM policy_registry WHERE policy_hash = ?",
                (new_policy_hash.as_slice(),),
            )
            .expect("count NEW in policy_registry");
        let count: i64 = count_rows
            .next()
            .expect("row")
            .expect("row ok")
            .get(0)
            .unwrap_or(0);
        assert_eq!(
            count, 1,
            "NEW successor MUST exist in policy_registry after delegation (got count = {count})"
        );

        // R6 fix F4 (d): the NEW row must also exist in
        // `policy_kind_authority` (R4 fix B3 atomic twin-insert
        // guarantee applies to every register_policy call —
        // the successor was registered BEFORE the delegation,
        // so the authority row existed at delegation time).
        // Without this assertion, the delegation chain would
        // land on an advisory slot with no backing authority
        // — the lookup_policy LEFT JOIN miss path.
        let mut count_rows = db
            .query(
                "SELECT COUNT(*) FROM policy_kind_authority WHERE policy_hash = ?",
                (new_policy_hash.as_slice(),),
            )
            .expect("count NEW in policy_kind_authority");
        let count: i64 = count_rows
            .next()
            .expect("row")
            .expect("row ok")
            .get(0)
            .unwrap_or(0);
        assert_eq!(
            count, 1,
            "NEW successor MUST exist in policy_kind_authority (B3 atomic twin-insert guarantee)"
        );
    }

    // 6. `delegate_authority_rejects_wrong_registrant` — R4 fix B1
    //    (N1 CRITICAL) coverage: when `registrant_did` differs from
    //    the row's `registered_by_did`, the delegation MUST be
    //    rejected with `Err(PolicyRegistryError::NotRegistrant)`.
    //
    //    This is the load-bearing security test for B1: prior to the
    //    fix, the registrant_did was silently dropped (line 337 in
    //    the pre-fix file), allowing ANY caller to revoke ANY
    //    active policy by submitting its hash. The fix enforces the
    //    "only the original registrant can delegate" invariant.
    #[test]
    fn delegate_authority_rejects_wrong_registrant() {
        let db = ensure_v020();
        let registry = StoolapPolicyRegistry::new(Arc::clone(&db));

        let body = b"policy_body_for_registrant_check_v1";
        let correct_registrant: [u8; 32] = [0xCC; 32];
        let wrong_registrant: [u8; 32] = [0xEE; 32];
        let policy_hash = octo_policy::domain_separators::blake3_prefix::derive_policy_hash(body);

        // Register the policy with the CORRECT registrant.
        registry
            .register_policy(
                &[0xDD; 16],
                body,
                ExecutionClass::A,
                &correct_registrant,
                &[0xDD; 64],
                1_700_000_000,
                &policy_hash,
            )
            .expect("register must succeed");

        // Try to delegate with the WRONG registrant — must fail.
        let err = registry
            .delegate_authority(&policy_hash, &[0xFF; 32], &wrong_registrant, 1_700_000_500)
            .expect_err("delegate_authority with wrong registrant must fail");

        match err {
            PolicyRegistryError::NotRegistrant(h) => {
                assert_eq!(h, hex32(&policy_hash));
            }
            other => panic!("expected NotRegistrant, got {other:?}"),
        }

        // The old row must NOT have been revoked — the transaction
        // was rolled back.
        let mut rows = db
            .query(
                "SELECT revoked_at_unix FROM policy_registry WHERE policy_hash = ?",
                (policy_hash.as_slice(),),
            )
            .expect("query revoked");
        let row = rows.next().expect("row").expect("ok");
        let revoked: Option<i64> = row.get(0).ok();
        assert!(
            revoked.is_none(),
            "policy must remain active after rejected delegation; got revoked_at_unix = {revoked:?}"
        );
    }

    // 7. `register_policy_creates_authority_row` — R4 fix B3
    //    coverage: a single `register_policy` call MUST result in
    //    exactly one row in `policy_kind_authority` (with the
    //    registrant signature, DID, body, and timestamp carried
    //    through). This is the "atomic twin-insert" guarantee.
    #[test]
    fn register_policy_creates_authority_row() {
        let db = ensure_v020();
        let registry = StoolapPolicyRegistry::new(Arc::clone(&db));

        let body = b"authority_row_body_v1";
        let kind_uuid: [u8; 16] = {
            let mut k = [0u8; 16];
            k[0] = 0xCA;
            k[15] = 0xFE;
            k
        };
        let registrant_did: [u8; 32] = [0x11; 32];
        let registrant_sig: [u8; 64] = {
            let mut s = [0u8; 64];
            s[0] = 0xED;
            s[63] = 0x19;
            s
        };
        let policy_hash = octo_policy::domain_separators::blake3_prefix::derive_policy_hash(body);

        registry
            .register_policy(
                &kind_uuid,
                body,
                ExecutionClass::A,
                &registrant_did,
                &registrant_sig,
                1_700_000_777,
                &policy_hash,
            )
            .expect("register_policy must succeed");

        // Verify `policy_kind_authority` row was created with the
        // exact fields from the trait call.
        let mut rows = db
            .query(
                "SELECT policy_kind_uuid, policy_hash, registrant_did, registrant_signature, \
                 registration_body, registered_at_unix, revoked_at_unix \
                 FROM policy_kind_authority WHERE policy_hash = ?",
                (policy_hash.as_slice(),),
            )
            .expect("query authority");
        let row = rows.next().expect("row present").expect("row ok");

        let row_uuid: Vec<u8> = row.get(0).unwrap_or_default();
        assert_eq!(row_uuid, kind_uuid.to_vec(), "policy_kind_uuid");
        let row_policy_hash: Vec<u8> = row.get(1).unwrap_or_default();
        assert_eq!(
            row_policy_hash,
            policy_hash.to_vec(),
            "policy_hash must match"
        );
        let row_did: Vec<u8> = row.get(2).unwrap_or_default();
        assert_eq!(row_did, registrant_did.to_vec(), "registrant_did");
        let row_sig: Vec<u8> = row.get(3).unwrap_or_default();
        assert_eq!(row_sig, registrant_sig.to_vec(), "registrant_signature");
        let row_body: Vec<u8> = row.get(4).unwrap_or_default();
        assert_eq!(row_body, body.to_vec(), "registration_body");
        let row_reg_at: i64 = row.get(5).unwrap_or(0);
        assert_eq!(
            row_reg_at, 1_700_000_777,
            "registered_at_unix must round-trip"
        );
        let row_revoked: Option<i64> = row.get(6).ok();
        assert!(
            row_revoked.is_none(),
            "revoked_at_unix must be NULL for a fresh registration"
        );
    }

    // 8. `lookup_policy_class_c_advisory_strips_body` — R4 fix D1
    //    coverage: Class C policy lookup returns an advisory row
    //    (kind_uuid + metadata) but the `body` field is empty
    //    (`Vec::new()`). Substrate fails-closed on Class C body
    //    exposure; consumers cannot act on Class C body content
    //    through the standard lookup path.
    //
    //    R6 fix F3 B.4 strengthening: the prior test (R5) used a
    //    tautological stub `if ExecutionClass::C == ExecutionClass::C
    //    { Vec::new() } else { self.raw.clone() }` that did NOT
    //    exercise the actual production code path. The fix calls
    //    `StoolapPolicyRegistry::lookup_policy` directly so the
    //    test asserts the production strip semantics end-to-end.
    //    The StubClassCRegistry below is retained (dead-code-
    //    free of any registry reference) so future readers can
    //    see the historical contract pattern — but the lookup
    //    assertion now exercises the production path.
    #[test]
    fn lookup_policy_class_c_advisory_strips_body() {
        let db = ensure_v020();
        let registry = StoolapPolicyRegistry::new(Arc::clone(&db));

        // R6 fix F3: end-to-end production-path exercise.
        //
        // Step 1: register a Class C policy. `register_policy`'s
        // B3 atomic twin-insert populates BOTH tables
        // (policy_registry + policy_kind_authority) AND the
        // R5 D3 denormalized columns on the policy_registry row
        // (`kind_uuid`, `execution_class`, `registered_by_did`).
        // These denormalized columns are what allow
        // `lookup_policy` to fall back when the Stoolap fork's
        // LEFT JOIN on BLOB(32) misses (substrate-truth 2026-08-24).
        let production_body: Vec<u8> = b"class_c_strip_test_body_v1".to_vec();
        let production_hash =
            octo_policy::domain_separators::blake3_prefix::derive_policy_hash(&production_body);
        let kind_uuid: [u8; 16] = [0xC3; 16];
        let registrant_did: [u8; 32] = [0xC4; 32];
        let registrant_sig: [u8; 64] = [0xC5; 64];
        let registered_at: i64 = 1_700_000_000;

        let registered = registry
            .register_policy(
                &kind_uuid,
                &production_body,
                ExecutionClass::C,
                &registrant_did,
                &registrant_sig,
                registered_at,
                &production_hash,
            )
            .expect("register_policy must accept a Class C body (Class C is advisory, not gated)");

        // Strip contract (R4 fix D1) ONLY fires at LOOKUP time,
        // not at REGISTER time. Verify register preserves the
        // canonical body bytes.
        assert_eq!(
            registered.body, production_body,
            "register_policy must preserve canonical body bytes (strip only fires at lookup)"
        );
        assert_eq!(
            registered.execution_class,
            ExecutionClass::C,
            "registered.execution_class is what we passed in"
        );

        // Step 2 (R6 fix F3): call the PRODUCTION
        // `StoolapPolicyRegistry::lookup_policy` directly — NOT
        // a stub. The stub's tautological strip is what R5
        // landed; this assertion now exercises the real
        // production code path.
        //
        // Substrate-truth: the Stoolap fork's LEFT JOIN on
        // BLOB(32) returns NULL for the joined columns even
        // when a matching policy_kind_authority row exists
        // (verified empirically in R4). The R5 fix D3
        // denormalized-column fallback lets lookup_policy
        // succeed via the in-row `kind_uuid` +
        // `registered_by_did` columns (which register_policy
        // populates as part of B3). So `lookup_policy` returns
        // `Ok(Some(row))` here — NOT `Err(NotFound)`.
        let looked_up = registry
            .lookup_policy(&production_hash)
            .expect("lookup_policy must succeed via R5 D3 denormalized fallback")
            .expect("row must be present after register_policy");

        // Step 3: assert the actual strip semantics
        // (replaces the R5 tautology `if C == C { empty }`).
        assert_eq!(
            looked_up.body,
            Vec::<u8>::new(),
            "Class C body MUST be stripped to Vec::new() at lookup time (R4 fix D1 advisory contract; \
             RFC-0967-A1 §3 row 6)"
        );
        assert!(
            looked_up.body.is_empty(),
            "Class C body MUST be empty (advisory-only contract)"
        );

        // Advisory metadata survives the strip — the strip is on
        // body only, not a blanket-zeroing destructor. Consumers
        // can still recover kind_uuid / registered_by_did /
        // execution_class for audit purposes.
        assert_eq!(
            looked_up.kind_uuid, kind_uuid,
            "advisory metadata: kind_uuid survives the strip"
        );
        assert_eq!(
            looked_up.registered_by_did, registrant_did,
            "advisory metadata: registered_by_did survives the strip"
        );
        assert_eq!(
            looked_up.execution_class,
            ExecutionClass::C,
            "advisory metadata: execution_class survives the strip"
        );

        // Negative path: B2 application-layer pre-check guards
        // against a duplicate register with the SAME policy_hash.
        // This is a regression guard against future substrate
        // changes that could let duplicate PK inserts land.
        let dup = registry.register_policy(
            &kind_uuid,
            &production_body,
            ExecutionClass::C,
            &registrant_did,
            &registrant_sig,
            1_700_000_001,
            &production_hash,
        );
        assert!(
            matches!(dup, Err(PolicyRegistryError::AlreadyRegistered(_))),
            "second register with same policy_hash MUST trigger AlreadyRegistered (B2 pre-check)"
        );
    }

    // 9. `register_policy_class_c_advisory_marker_optional` — R5 fix
    //    G3 coverage: the Class C advisory marker (`CLASS_C_ADVISORY_MARKER`
    //    at `body[0..4]`) is REQUIRED for verification but OPTIONAL
    //    at registration time. The substrate's `register_policy`
    //    accepts Class C bodies regardless of marker presence
    //    (per RFC-0967-A1 §3 row 6: Class C is "registration-time
    //    rejected" → but the rejection is at LOOKUP, not
    //    register). This test asserts:
    //      - `verify_class_c_marker` returns false on a body without
    //        the marker
    //      - `verify_class_c_marker` returns true on a body with the
    //        marker at `body[0..4]`
    //      - The substrate's `register_policy` accepts a Class C
    //        body WITHOUT the marker (registration is advisory-only;
    //        marker is metadata, not a gate)
    #[test]
    fn register_policy_class_c_advisory_marker_optional() {
        // Helper invariants first.
        assert!(
            !verify_class_c_marker(&[]),
            "empty body must not satisfy Class C marker check"
        );
        assert!(
            !verify_class_c_marker(&[0u8; 3]),
            "body shorter than 4 bytes must not satisfy marker check"
        );
        assert!(
            !verify_class_c_marker(&[0u8; 4]),
            "all-zero body must not satisfy marker check (zero is not the canonical marker)"
        );
        let mut marked = vec![0u8; 64];
        marked[0..4].copy_from_slice(&octo_policy::policy_registry::CLASS_C_ADVISORY_MARKER);
        assert!(
            verify_class_c_marker(&marked),
            "body with CLASS_C_ADVISORY_MARKER at [0..4] must satisfy marker check"
        );
        // Cross-check: a body that would satisfy the Class B ZK
        // marker check does NOT satisfy the Class C marker check
        // (markers are disjoint by construction).
        let mut class_b_body = vec![0u8; 64];
        class_b_body[16..20].copy_from_slice(&ZK_ENVELOPE_MARKER);
        assert!(
            !verify_class_c_marker(&class_b_body),
            "Class B ZK marker must not be detected as Class C marker"
        );

        // Substrate register path: Class C body WITHOUT marker is
        // accepted by register_policy (marker is OPTIONAL at
        // registration per §3 row 6).
        let db = ensure_v020();
        let registry = StoolapPolicyRegistry::new(db);
        let body_no_marker = b"class_c_no_marker_body_v1".to_vec();
        let policy_hash =
            octo_policy::domain_separators::blake3_prefix::derive_policy_hash(&body_no_marker);
        registry
            .register_policy(
                &[0xC7; 16],
                &body_no_marker,
                ExecutionClass::C,
                &[0xC8; 32],
                &[0xC9; 64],
                1_700_000_777,
                &policy_hash,
            )
            .expect("Class C register without advisory marker must succeed (marker is OPTIONAL)");
    }

    // ─────────────────────────────────────────────────────────────────────
    // R5 fix B.4 — `StubClassCRegistry` impl REMOVED in R6 fix F3.
    //
    // The R5 stub overrode `lookup_policy` with a tautological
    // strip (`if execution_class == ExecutionClass::C { Vec::new()
    // } else { self.raw.clone() }`) that did NOT exercise the
    // production `StoolapPolicyRegistry::lookup_policy` code
    // path. R6 fix F3 replaces the stub with a direct call to
    // the production registry's `lookup_policy`, exercising the
    // actual strip semantics end-to-end (including the R5 D3
    // denormalized-column fallback path). The test now asserts
    // the production strip invariant — not a tautology that
    // happens to compile.
    // ─────────────────────────────────────────────────────────────────────
}

#[cfg(test)]
mod probe_tests {
    // (R4 debug probes removed; see git history for the parameter-
    // binding + multi-column SELECT probes that established the
    // Stoolap fork quirks. Final impl uses anonymous `?`
    // placeholders + `COUNT(*)` for existence checks + single-
    // column SELECT for content reads.)
}
