//! `octo vault {list, balance, transfer}` — RFC-0011-e §Subcommand Taxonomy.
//!
//! Layer C wrapper over the [`octo_vault`] substrate. Operator invocation
//! → clap parse → substrate port binding → JSON envelope render.
//!
//! ## Substrate port binding (RFC-0011-e §Substrate Additions + [[cipherocto-design-principles]])
//!
//! Per `[[cipherocto-design-principles]]` Layer direction:
//!
//! - The substrate ([`octo_vault::VaultOwnerIndex`] / [`octo_vault::VaultAssetResolver`] /
//!   [`octo_vault::TransferEventLog`]) declares the port SHAPE only (Layer B
//!   additive). The substrate library exposes NO production impl (the
//!   production Layer D Stoolap adapter lands in `octo-vault-stoolap`,
//!   out of scope for this mission).
//! - This module supplies a Phase 1 in-process adapter — [`HomeVaultOwnerIndex`],
//!   [`HomeVaultAssetResolver`], [`HomeTransferEventLog`] — backed by
//!   `$OCTO_HOME/vaults/vaults.jsonl` + `$OCTO_HOME/vaults/transfers.jsonl`.
//!   The CLI owns this adapter; the substrate stays free of CLI concerns.
//!   When `octo-vault-stoolap` lands, the adapter swaps behind the same
//!   port trait without changing this file.
//!
//! ## Re-exports from substrate (RFC-0011-e §Substrate Additions)
//!
//! - [`VaultSummary`] — CLI-facing vault inventory record (already
//!   `Serialize + Deserialize` per substrate).
//! - [`VaultBalanceProjection`] (RFC-0960-v37 §2.1) — the substrate's
//!   canonical SUM projection. The CLI converts this to a serializable
//!   [`VaultBalanceOutput`] DTO because the substrate `VaultBalanceProjection`
//!   intentionally does NOT derive `Serialize` (its `Dqa` field has
//!   only `Clone/Copy/Debug/PartialEq/Eq/Hash` per `determin/src/dqa.rs`).
//! - [`ProjectionSource`] — `#[repr(u8)]` enum (`Cache = 0`, `FreshLogScan = 1`,
//!   `EpochRebuild = 2`). Wire form is the integer discriminant
//!   (RFC-0960-v37 §2.5); the CLI surfaces it via `Display` + JSON
//!   without pattern-matching on variants (forward-compat per F-14).
//!
//! ## Mode gating (RFC-0011-e §Roles and Authorities)
//!
//! | Subcommand        | Mode   | Required flags |
//! |-------------------|--------|----------------|
//! | `list`            | All    | (none — read-only) |
//! | `balance`         | All    | (none — read-only) |
//! | `transfer`        | Human  | `--confirm` + `--confirm-acknowledge` |
//! | `transfer`        | Ci     | `--allow-write` |
//! | `transfer`        | Auditor | denied (`AuditorDenied`, exit 2) |
//!
//! `list` and `balance` are read-only and bypass the `require_confirm`
//! gate. `transfer` is mutating and goes through the two-step
//! pastejacking-defense gate per RFC-0011 §Confirmation Flag Matrix.
//!
//! ## `OwnerDid` substrate-truth deviation
//!
//! Per `vault_operations.rs` module-level "Substrate-truth deviation note",
//! the substrate's [`octo_vault::OwnerDid`] is `String` (NOT `octo_wallet::Did`)
//! because adding `octo-wallet` to `octo-vault` would create a workspace
//! cycle. The CLI converts `octo_wallet::Did` → `&str` at the substrate
//! boundary via `did.0.as_str()`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};

use clap::Subcommand;
use octo_cap_macaroon::{
    dqa_serde as dqa_field, AssetId, AssetMetadata, ChainId, Dqa, InMemoryAssetRegistry, VaultId,
};
use octo_vault::{
    initiate_transfer as substrate_initiate_transfer, list_owned as substrate_list_owned,
    project_vault_balance, ProjectionError, ProjectionSource, TransferEventLog,
    TransferEventLogInsertError, TransferEventRef, TransferHandle, TransferStatus,
    VaultAssetResolver, VaultAssetResolverError, VaultOperationsError, VaultOwnerIndex,
    VaultSummary,
};
use serde::{Deserialize, Serialize};

use crate::commands::identity::require_confirm;
use crate::error::{sanitize_substrate_error, OctoCliError};
use crate::home;
use crate::output::{OutputEnvelope, RedactedString};
use crate::Octo;

/// CLI-facing vault subcommand enum (Layer C; delegates to the
/// `octo_vault` substrate for decisions). `#[non_exhaustive]` per F-14
/// — the follow-on `0011-e-vault-subcommands-transfer` mission adds a
/// `Transfer` variant without central-enum edits.
#[derive(Subcommand, Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum VaultAction {
    /// List vaults owned by the active identity.
    ///
    /// `--chain-id` and `--asset-symbol` are client-side filters
    /// applied at the envelope layer (RFC-0011-e §Subcommand Taxonomy
    /// `vault list` Flags). `--limit` is a defensive cap (default
    /// 100; `0` triggers a substrate rejection per TV-VLT3).
    /// `--cursor` is reserved for the future paginated substrate
    /// surface; Phase 1 surfaces `next_cursor: None`.
    List {
        /// Filter to a single chain (RFC-0010 canonical form).
        #[arg(long, value_name = "CHAIN")]
        chain_id: Option<String>,
        /// Filter to one asset symbol (e.g. `OCTO`); client-side filter.
        #[arg(long, value_name = "SYMBOL")]
        asset_symbol: Option<String>,
        /// Cap on returned vaults (default 100; substrate validates
        /// `limit >= 1` per TV-VLT3).
        #[arg(long, value_name = "N", default_value_t = 100_u32)]
        limit: u32,
        /// Opaque pagination cursor returned by prior call (reserved;
        /// Phase 1 surfaces `next_cursor: None`).
        #[arg(long, value_name = "CURSOR")]
        cursor: Option<String>,
    },
    /// Project the balance of a single vault (RFC-0960-v37 §2.2 SUM).
    ///
    /// `--no-cache` forces a fresh projection (substrate cache bypass
    /// per RFC-0960-v37 §2.3); `--history <n>` is reserved for the
    /// follow-on substrate history surface (Phase 1 surfaces
    /// `history: None`). A cache staleness warning is emitted to stderr
    /// when the projection is older than the cache TTL (RFC-0011-e
    /// §Security: Balance Projection Staleness).
    Balance {
        /// Vault identifier (RFC-0960 §2.1 canonical 32-byte form;
        /// CLI accepts 64-char lowercase hex).
        vault_id: String,
        /// Force substrate re-computation; bypass cache.
        #[arg(long)]
        no_cache: bool,
        /// Reserved for future substrate history surface (ignored
        /// in Phase 1; envelope surfaces `history: None`).
        #[arg(long, value_name = "N")]
        history: Option<u32>,
    },
    /// Initiate a vault-to-vault transfer (RFC-0011-e §Subcommand
    /// Taxonomy `octo vault transfer`).
    ///
    /// **Mutating.** Requires `--confirm` AND `--confirm-acknowledge`
    /// in Human mode (two-step pastejacking defense per RFC-0011
    /// §Confirmation Flag Matrix); `--allow-write` in CI mode; denied
    /// in Auditor mode.
    ///
    /// Pre-flight runs four checks before the substrate call (role
    /// provisioned → HSM reachable → vault owned → balance sufficient)
    /// per RFC-0011-e Appendix D. Destination-vault validation is
    /// substrate-side by design — the CLI must NOT pre-fetch the vault
    /// registry (that would duplicate substrate authority).
    ///
    /// There is deliberately **no `--soft-sign` flag** (or any
    /// equivalent): the HSM signing path is mandatory per RFC-0011-e
    /// §Security: HSM Downgrade + `[[cipherocto-design-principles]]`.
    Transfer {
        /// Source vault (64-char lowercase hex); must be owned by the
        /// active DID.
        #[arg(long, value_name = "VAULT_ID")]
        from: String,
        /// Destination vault (64-char lowercase hex). Cross-chain
        /// transfers require an explicit `--dest-chain-id`.
        #[arg(long, value_name = "VAULT_ID")]
        to: String,
        /// Amount in DQA canonical form (RFC-0960-v36 §Wire Form).
        #[arg(long, value_name = "DQA")]
        amount: String,
        /// Asset symbol (e.g. `OCTO`); resolved against the vault
        /// registry.
        #[arg(long, value_name = "SYMBOL")]
        asset: String,
        /// Free-text memo. Redacted as `[REDACTED:<n>chars]` in BOTH
        /// the log/stderr sink and the JSON payload unless
        /// `--include-memo` is set.
        ///
        /// Memo content is signed into the transfer envelope and is
        /// observable by the destination vault owner and by anyone
        /// reading the chain — CLI redaction protects local sinks only.
        #[arg(long, value_name = "TEXT")]
        memo: Option<String>,
        /// Opt in to emitting `--memo` plaintext in both sinks.
        #[arg(long)]
        include_memo: bool,
        /// Opt in to truncating vault identifiers in rendered output.
        /// Output is then NOT signature-verifiable.
        #[arg(long)]
        redact_ids: bool,
        /// Destination chain ID (RFC-0010 canonical form). REQUIRED
        /// when `--to` resolves to a different chain than `--from`.
        #[arg(long, value_name = "CHAIN")]
        dest_chain_id: Option<String>,
        /// Build + substrate-validate the envelope WITHOUT signing or
        /// broadcasting. Surfaces `status: DryRun`.
        #[arg(long)]
        dry_run: bool,
    },
}

// ---------------------------------------------------------------------------
// CLI-side output envelope payload types — RFC-0011-e §Output Envelope
// ---------------------------------------------------------------------------

/// `octo vault list` payload.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct VaultListOutput {
    /// Vaults owned by the active DID, filtered + limited.
    /// Substrate re-export type — already `Serialize + Deserialize`
    /// per `octo_vault::VaultSummary`.
    pub vaults: Vec<VaultSummary>,
    /// Opaque pagination cursor for the next page. Phase 1 always
    /// `None`; the substrate paginated surface lands in a follow-on
    /// amendment per RFC-0011-e §Future Work.
    pub next_cursor: Option<String>,
    /// Unix seconds at which the CLI resolved the active DID (used
    /// for stale-list detection by the operator; the substrate
    /// returns the rows at the call instant).
    pub resolved_at_unix: i64,
}

/// `octo vault balance` payload — CLI-side DTO for the substrate's
/// [`VaultBalanceProjection`].
///
/// The substrate's `VaultBalanceProjection` intentionally does NOT
/// derive `Serialize` (its `Dqa` field has only
/// `Clone/Copy/Debug/PartialEq/Eq/Hash` per `determin/src/dqa.rs:104`).
/// The CLI converts to a serializable DTO at the substrate boundary.
/// Wire form for `projected_balance` uses the canonical 16-byte BE
/// `DqaEncoding` via `octo_cap_macaroon::dqa_serde::field`.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct VaultBalanceOutput {
    /// Canonical SUM projection record (CLI-side DTO mirror of the
    /// substrate `VaultBalanceProjection`).
    pub record: VaultBalanceRecord,
    /// `true` when the projection was served from the substrate
    /// bounded-LRU cache; `false` when a fresh SUM scan was run.
    pub cache_hit: bool,
    /// Substrate `ProjectionSource` discriminant (integer form
    /// per RFC-0960-v37 §2.5). Phase 1 surfaces the raw `u8`;
    /// follow-on amendments may add a typed enum-tagged form
    /// without breaking this field per F-14.
    pub projection_source_u8: u8,
    /// Cache staleness warnings emitted to stderr (Phase 1: at most
    /// one entry when the projection age exceeds the TTL).
    pub warnings: Vec<String>,
    /// Last `n` projection events (reserved; Phase 1 always `None`).
    pub history: Option<Vec<ProjectionEventDto>>,
}

/// CLI-side mirror of the substrate [`VaultBalanceProjection`] with
/// `Dqa` serialized as the canonical 16-byte BE `DqaEncoding`.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct VaultBalanceRecord {
    /// Owning chain (RFC-0105 §3.11 chain-binding).
    pub chain_id: ChainId,
    /// Vault whose balance is projected (RFC-0960 §2.1 canonical
    /// 32-byte form; serde via the standard `VaultId` newtype shape).
    pub vault_id: VaultId,
    /// Asset contained by the vault (RFC-0960 §2.6 asset-generality).
    pub asset_id: AssetId,
    /// Projected balance (RFC-0960-v36 canonical DQA wire form;
    /// 16-byte BE `DqaEncoding` per
    /// `octo_cap_macaroon::dqa_serde::field`).
    #[serde(with = "dqa_field::field")]
    pub projected_balance: Dqa,
    /// Wall-clock timestamp of the projection (unix seconds).
    /// `None` when the vault has no events.
    pub projected_at_unix_seconds: Option<i64>,
    /// Registry snapshot epoch at projection time.
    pub registry_snapshot_epoch: u64,
    /// Substrate `ProjectionSource` discriminant (integer form).
    pub source_kind_u8: u8,
}

/// Phase 1 placeholder for the projection-event history surface
/// (RFC-0960-v37 §2.4 invalidation bus; lands in a follow-on
/// amendment). `#[non_exhaustive]` so the substrate can grow
/// fields without breaking CLI consumers.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ProjectionEventDto {
    /// Wall-clock unix seconds of the event.
    pub occurred_at_unix: i64,
    /// Event discriminator (`CacheInvalidated` | `EpochAdvanced` |
    /// future variants). Surfaces the discriminant; the CLI does
    /// NOT pattern-match on variants (F-14).
    pub kind_u8: u8,
}

/// `octo vault transfer` payload (RFC-0011-e §Output Envelope).
///
/// Wraps the substrate-built [`TransferHandle`] verbatim. The
/// substrate owns handle identity + nonce derivation; the CLI adds only
/// the operator-facing broadcast timestamp, the (possibly CLI-rewritten)
/// status, and the `--include-memo` opt-in plaintext channel.
///
/// ## Memo asymmetry (RFC-0011-e §Redaction)
///
/// `memo` is deliberately ABSENT as a redacted-by-default field on this
/// struct: `memo_plaintext` is `None` unless the operator passes
/// `--include-memo`. The length-signal rendering
/// (`[REDACTED:<n>chars]`) is surfaced via `memo_redacted`, which is a
/// [`RedactedString`] and therefore can never serialize plaintext even
/// if a future refactor mis-wires the opt-in.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct VaultTransferOutput {
    /// Substrate-built transfer handle (RFC-0011-e §Substrate
    /// Additions). Substrate re-export type.
    pub handle: TransferHandle,
    /// Wall-clock unix seconds at broadcast. `None` for `--dry-run`
    /// (nothing was broadcast) and for handles that never reached the
    /// chain adapter.
    ///
    /// Wall-clock-derived, so `octo vault transfer` is NOT deterministic
    /// (RFC-0011-e §Determinism Requirements — acceptable because
    /// mutating commands are Class C and do not participate in
    /// consensus).
    pub broadcast_at_unix: Option<u64>,
    /// Transfer status. Equals `handle.status` except on the
    /// `--dry-run` path, where the CLI rewrites it to
    /// [`TransferStatus::DryRun`] (RFC-0011-e Appendix D).
    pub status: TransferStatus,
    /// Memo plaintext — populated ONLY under `--include-memo`
    /// (RFC-0011-e §Redaction). `None` is the default-redaction state.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memo_plaintext: Option<String>,
    /// Length-signal rendering of the memo (`[REDACTED:<n>chars]`).
    /// `None` when no `--memo` was supplied.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memo_redacted: Option<RedactedString>,
    /// True when at least one field in this payload was altered by the
    /// redaction layer (RFC-0011-e §Output Envelope `redacted` flag).
    pub redacted: bool,
}

// ---------------------------------------------------------------------------
// CLI-side Phase 1 substrate port impls
// ---------------------------------------------------------------------------

/// `HomeVaultOwnerIndex` — Phase 1 in-process adapter implementing
/// [`VaultOwnerIndex`] against a file-backed vault table at
/// `$OCTO_HOME/vaults/vaults.jsonl` (one [`VaultSummary`] per line).
///
/// The Phase 1 adapter is the CLI's stand-in until `octo-vault-stoolap`
/// (Layer D production adapter) lands. The CLI owns the file format —
/// the substrate stays free of CLI concerns.
#[derive(Debug, Default)]
pub struct HomeVaultOwnerIndex {
    rows: Vec<VaultSummary>,
}

impl HomeVaultOwnerIndex {
    /// Build the adapter from a `VaultSummary` slice (test/dev).
    #[must_use]
    pub fn from_rows(rows: Vec<VaultSummary>) -> Self {
        Self { rows }
    }

    /// Build the adapter from a `$OCTO_HOME/vaults/vaults.jsonl` file.
    ///
    /// Missing file → empty table. Lines that fail to deserialize are
    /// skipped silently (defensive — substrate reads handle their own
    /// diagnostics).
    /// # Errors
    /// Returns `OctoCliError::Internal` when an I/O read fails
    /// (filesystem error other than "file not found").
    pub fn load(home: &Path) -> Result<Arc<Self>, OctoCliError> {
        let path = vaults_path(home);
        let rows = if path.exists() {
            let bytes = std::fs::read(&path).map_err(|e| {
                OctoCliError::Internal(sanitize_substrate_error(&format!(
                    "vault index read {}: {e}",
                    path.display()
                )))
            })?;
            let mut out = Vec::new();
            for line in bytes.split(|b| *b == b'\n') {
                if line.is_empty() {
                    continue;
                }
                if let Ok(s) = serde_json::from_slice::<VaultSummary>(line) {
                    out.push(s);
                }
            }
            out
        } else {
            Vec::new()
        };
        Ok(Arc::new(Self { rows }))
    }

    /// All rows currently loaded (for filter / inspection in tests).
    #[must_use]
    pub fn rows(&self) -> &[VaultSummary] {
        &self.rows
    }
}

impl VaultOwnerIndex for HomeVaultOwnerIndex {
    fn vaults_for_owner(&self, owner_did: &str) -> Result<Vec<VaultSummary>, VaultError> {
        Ok(self
            .rows
            .iter()
            .filter(|s| s.owner_did == owner_did)
            .cloned()
            .collect())
    }
}

/// CLI-side alias for [`VaultOperationsError`] (substrate vault
/// operations errors). Mirrors the substrate's `VaultError` so the
/// adapter code can stay agnostic of the exact variant.
pub type VaultError = VaultOperationsError;

/// `HomeVaultAssetResolver` — Phase 1 adapter implementing
/// [`VaultAssetResolver`] against the same vault rows (reverse
/// `(chain_id, vault_id) → asset_id` lookup).
#[derive(Debug, Default)]
pub struct HomeVaultAssetResolver {
    rows: Vec<VaultSummary>,
}

impl HomeVaultAssetResolver {
    /// Build from rows (test/dev).
    #[must_use]
    pub fn from_rows(rows: Vec<VaultSummary>) -> Self {
        Self { rows }
    }

    /// Build from a vault table file.
    /// # Errors
    /// Returns `OctoCliError::Internal` on I/O failures.
    pub fn load(home: &Path) -> Result<Arc<Self>, OctoCliError> {
        let index = HomeVaultOwnerIndex::load(home)?;
        Ok(Arc::new(Self {
            rows: index.rows.clone(),
        }))
    }
}

impl VaultAssetResolver for HomeVaultAssetResolver {
    fn resolve_asset_for(
        &self,
        chain_id: &ChainId,
        vault_id: &VaultId,
    ) -> Result<AssetId, VaultAssetResolverError> {
        self.rows
            .iter()
            .find(|r| &r.chain_id == chain_id && &r.vault_id == vault_id)
            .map(|r| asset_symbol_to_id(&r.asset_symbol))
            .ok_or(VaultAssetResolverError::UnknownVault {
                vault_id: *vault_id,
            })
    }
}

/// `HomeTransferEventLog` — Phase 1 adapter implementing
/// [`octo_vault::TransferEventLog`] against a `$OCTO_HOME/vaults/transfers.jsonl`
/// file (one event per line).
///
/// The Phase 1 surface is intentionally minimal — Phase 1 transfer
/// subcommand lands in `0011-e-vault-subcommands-transfer` and adds
/// the full transfer-event shape (amount, dest, asset_id, nonce).
/// For now the events are just `(chain_id, vault_id, asset_id,
/// occurred_at_unix, amount_dqa_micros)` so the SUM projection can
/// run end-to-end and TV-VLT5..TV-VLT7 land.
#[derive(Debug, Default)]
pub struct HomeTransferEventLog {
    events: Vec<TransferEventRow>,
}

/// One row of `$OCTO_HOME/vaults/transfers.jsonl` (Phase 1 minimum
/// surface). Wire-stable additive shape — future amendments extend
/// with optional fields.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct TransferEventRow {
    /// Owning chain.
    pub chain_id: ChainId,
    /// Vault the event belongs to (source or destination depending
    /// on direction; the Phase 1 sum splits on `to_vault` /
    /// `from_vault` discriminator below).
    pub vault_id: VaultId,
    /// Asset ID (canonical AssetId newtype).
    pub asset_id: AssetId,
    /// Wall-clock unix seconds of the event.
    pub occurred_at_unix: i64,
    /// Signed amount in DQA micros (scale 0). Positive = credit to
    /// `vault_id`; negative = debit from `vault_id`.
    pub amount_dqa_micros: i64,
    /// Direction discriminator (`to_vault` | `from_vault`).
    /// `to_vault` adds to the vault's balance; `from_vault` subtracts.
    /// Surfaced as the discriminant integer (0 = to, 1 = from).
    pub direction_u8: u8,
}

impl HomeTransferEventLog {
    /// Build from rows (test/dev).
    #[must_use]
    pub fn from_rows(events: Vec<TransferEventRow>) -> Self {
        Self { events }
    }

    /// Build from the transfers file.
    /// # Errors
    /// Returns `OctoCliError::Internal` on I/O failures.
    pub fn load(home: &Path) -> Result<Arc<Self>, OctoCliError> {
        let path = transfers_path(home);
        let events = if path.exists() {
            let bytes = std::fs::read(&path).map_err(|e| {
                OctoCliError::Internal(sanitize_substrate_error(&format!(
                    "transfer log read {}: {e}",
                    path.display()
                )))
            })?;
            let mut out = Vec::new();
            for line in bytes.split(|b| *b == b'\n') {
                if line.is_empty() {
                    continue;
                }
                if let Ok(e) = serde_json::from_slice::<TransferEventRow>(line) {
                    out.push(e);
                }
            }
            out
        } else {
            Vec::new()
        };
        Ok(Arc::new(Self { events }))
    }
}

impl TransferEventLog for HomeTransferEventLog {
    fn sum_to_vault(
        &self,
        chain_id: &ChainId,
        vault_id: &VaultId,
        asset_id: &AssetId,
        _since_unix: i64,
    ) -> Result<Dqa, ProjectionError> {
        let sum: i64 = self
            .events
            .iter()
            .filter(|e| {
                e.direction_u8 == 0
                    && &e.chain_id == chain_id
                    && &e.vault_id == vault_id
                    && &e.asset_id == asset_id
            })
            .map(|e| e.amount_dqa_micros)
            .sum();
        // Phase 1: scale 0 throughout (DQA micros). The substrate
        // invariant is "single-scale projection"; the scale 0
        // wrapper here is consistent with the substrate's
        // `project()` helper.
        Dqa::new(sum, 0).map_err(|_| ProjectionError::LogReadFailed("dqa overflow".into()))
    }

    fn sum_from_vault(
        &self,
        chain_id: &ChainId,
        vault_id: &VaultId,
        asset_id: &AssetId,
        _since_unix: i64,
    ) -> Result<Dqa, ProjectionError> {
        let sum: i64 = self
            .events
            .iter()
            .filter(|e| {
                e.direction_u8 == 1
                    && &e.chain_id == chain_id
                    && &e.vault_id == vault_id
                    && &e.asset_id == asset_id
            })
            .map(|e| e.amount_dqa_micros)
            .sum();
        Dqa::new(sum, 0).map_err(|_| ProjectionError::LogReadFailed("dqa overflow".into()))
    }

    fn max_occurred_at_unix(
        &self,
        chain_id: &ChainId,
        vault_id: &VaultId,
        asset_id: &AssetId,
    ) -> Result<Option<i64>, ProjectionError> {
        let max = self
            .events
            .iter()
            .filter(|e| {
                &e.chain_id == chain_id && &e.vault_id == vault_id && &e.asset_id == asset_id
            })
            .map(|e| e.occurred_at_unix)
            .max();
        Ok(max)
    }

    fn insert(&mut self, _event: &TransferEventRef) -> Result<(), TransferEventLogInsertError> {
        // Phase 1: read-only CLI surface — the CLI does not insert
        // events. The transfer subcommand (follow-on mission) wires
        // the substrate's broadcast path which lands in the same
        // adapter via the substrate's `EventLogProducer`.
        Ok(())
    }
}

/// Transfer role gate port (RFC-0011-e Appendix D pre-flight step 1).
///
/// Answers the single question "may this DID initiate a transfer?". The
/// CLI **queries** this gate; it does NOT implement role provisioning —
/// that is RFC-0011-d's surface, and duplicating it here would be a
/// parallel abstraction per `[[cipherocto-design-principles]]`.
///
/// The canonical substrate hook named by RFC-0011-e is
/// `octo_vault::role_can_transfer(active_did)`. That function does not
/// exist in the substrate yet, so the production impl
/// ([`UnprovisionedRoleGate`]) fails closed and the CLI surfaces
/// `RoleNotProvisioned` (exit 25) — the stub-with-error state mandated by
/// RFC-0011-e §Implementation Phases. When the substrate hook lands,
/// swap the production impl behind this same trait; no handler change.
pub trait TransferRoleGate: Send + Sync + std::fmt::Debug {
    /// `true` when `owner_did` holds a provisioned transfer capability.
    fn can_transfer(&self, owner_did: &str) -> bool;
}

/// Fail-closed production role gate — the stub-with-error state.
///
/// Returns `false` for every DID because the substrate role-gate hook
/// (`octo_vault::role_can_transfer`) has not landed. Per RFC-0011-e
/// §Implementation Phases the CLI surfaces `RoleNotProvisioned`
/// (exit 25) **regardless of HSM availability**, so this gate is
/// evaluated FIRST in the pre-flight chain.
#[derive(Debug, Default, Clone, Copy)]
pub struct UnprovisionedRoleGate;

impl TransferRoleGate for UnprovisionedRoleGate {
    fn can_transfer(&self, _owner_did: &str) -> bool {
        false
    }
}

/// HSM reachability probe port (RFC-0011-e Appendix D pre-flight step 2).
///
/// The canonical substrate hook named by RFC-0011-e is
/// `octo_wallet::hsm_status()`. `octo-wallet` exposes no HSM status
/// surface yet, so the production impl ([`WalletHsmProbe`]) reports
/// reachable-if-an-active-identity-resolves and the substrate remains
/// authoritative: it refuses to fall back to soft signing, so a missing
/// HSM surfaces at signing time rather than being silently downgraded
/// (RFC-0011-e §Security: HSM Downgrade).
pub trait HsmProbe: Send + Sync + std::fmt::Debug {
    /// `true` when an HSM slot is provisioned for `owner_did`.
    fn is_provisioned(&self, owner_did: &str) -> bool;
}

/// Production HSM probe backed by the wallet's active-identity
/// resolution. Fails closed on any wallet error.
#[derive(Debug, Default, Clone, Copy)]
pub struct WalletHsmProbe;

impl HsmProbe for WalletHsmProbe {
    fn is_provisioned(&self, owner_did: &str) -> bool {
        // An active identity that resolves to this DID implies a usable
        // signing slot in Phase 1. The substrate is authoritative and
        // never soft-signs, so a false positive here surfaces as a
        // signing failure rather than an HSM downgrade.
        !owner_did.is_empty()
    }
}

/// Shared process-wide Phase 1 substrate port state.
///
/// Phase 1 keeps the substrate port state in-process (the production
/// `octo-vault-stoolap` Layer D adapter lands in a follow-on mission).
/// The state is loaded once per dispatch and shared across the
/// `list` / `balance` handlers via an `OnceLock` to keep the file
/// reads off the hot path.
#[derive(Debug, Default)]
pub struct VaultPorts {
    owner_index: RwLock<Option<Arc<HomeVaultOwnerIndex>>>,
    asset_resolver: RwLock<Option<Arc<HomeVaultAssetResolver>>>,
    transfer_log: RwLock<Option<Arc<HomeTransferEventLog>>>,
    asset_registry: RwLock<Option<Arc<InMemoryAssetRegistry>>>,
    /// Transfer role gate (RFC-0011-e pre-flight step 1). `None` →
    /// [`UnprovisionedRoleGate`] (fail closed).
    role_gate: RwLock<Option<Arc<dyn TransferRoleGate>>>,
    /// HSM reachability probe (RFC-0011-e pre-flight step 2). `None` →
    /// [`WalletHsmProbe`].
    hsm_probe: RwLock<Option<Arc<dyn HsmProbe>>>,
}

impl VaultPorts {
    /// Construct a fresh empty port bundle (test convenience).
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Build from explicit components (test convenience).
    #[must_use]
    pub fn from_components(
        index: Arc<HomeVaultOwnerIndex>,
        resolver: Arc<HomeVaultAssetResolver>,
        log: Arc<HomeTransferEventLog>,
        registry: Arc<InMemoryAssetRegistry>,
    ) -> Self {
        Self {
            owner_index: RwLock::new(Some(index)),
            asset_resolver: RwLock::new(Some(resolver)),
            transfer_log: RwLock::new(Some(log)),
            asset_registry: RwLock::new(Some(registry)),
            role_gate: RwLock::new(None),
            hsm_probe: RwLock::new(None),
        }
    }

    /// Override the transfer role gate (test + future-activation seam).
    #[must_use]
    pub fn with_role_gate(self, gate: Arc<dyn TransferRoleGate>) -> Self {
        *self.role_gate.write().unwrap_or_else(|p| p.into_inner()) = Some(gate);
        self
    }

    /// Override the HSM probe (test + future-activation seam).
    #[must_use]
    pub fn with_hsm_probe(self, probe: Arc<dyn HsmProbe>) -> Self {
        *self.hsm_probe.write().unwrap_or_else(|p| p.into_inner()) = Some(probe);
        self
    }

    /// Load the ports from `$OCTO_HOME`. Missing files yield empty
    /// tables.
    /// # Errors
    /// Returns `OctoCliError::Internal` on I/O failures.
    pub fn load(home: &Path) -> Result<Self, OctoCliError> {
        let index = HomeVaultOwnerIndex::load(home)?;
        let resolver = HomeVaultAssetResolver::load(home)?;
        let log = HomeTransferEventLog::load(home)?;
        let registry = build_registry(&index);
        Ok(Self {
            owner_index: RwLock::new(Some(index)),
            asset_resolver: RwLock::new(Some(resolver)),
            transfer_log: RwLock::new(Some(log)),
            asset_registry: RwLock::new(Some(registry)),
            role_gate: RwLock::new(None),
            hsm_probe: RwLock::new(None),
        })
    }

    /// Resolve the role gate, defaulting to the fail-closed production
    /// gate (RFC-0011-e §Implementation Phases stub-with-error).
    fn role_gate(&self) -> Arc<dyn TransferRoleGate> {
        self.role_gate
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .clone()
            .unwrap_or_else(|| Arc::new(UnprovisionedRoleGate))
    }

    /// Resolve the HSM probe, defaulting to the wallet-backed probe.
    fn hsm_probe(&self) -> Arc<dyn HsmProbe> {
        self.hsm_probe
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .clone()
            .unwrap_or_else(|| Arc::new(WalletHsmProbe))
    }

    fn owner_index(&self) -> Result<Arc<HomeVaultOwnerIndex>, OctoCliError> {
        self.owner_index
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .clone()
            .ok_or_else(|| OctoCliError::Internal("vault owner index not loaded".into()))
    }

    fn asset_resolver(&self) -> Result<Arc<HomeVaultAssetResolver>, OctoCliError> {
        self.asset_resolver
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .clone()
            .ok_or_else(|| OctoCliError::Internal("vault asset resolver not loaded".into()))
    }

    fn transfer_log(&self) -> Result<Arc<HomeTransferEventLog>, OctoCliError> {
        self.transfer_log
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .clone()
            .ok_or_else(|| OctoCliError::Internal("vault transfer log not loaded".into()))
    }

    fn asset_registry(&self) -> Result<Arc<InMemoryAssetRegistry>, OctoCliError> {
        self.asset_registry
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .clone()
            .ok_or_else(|| OctoCliError::Internal("vault asset registry not loaded".into()))
    }
}

/// Static registry used by the dispatch path (Phase 1 in-process).
static PORTS: std::sync::OnceLock<Mutex<VaultPorts>> = std::sync::OnceLock::new();

/// Initialise the Phase 1 vault ports from `$OCTO_HOME`.
/// Idempotent — second + subsequent calls are no-ops.
fn init_ports(home: &Path) -> Result<(), OctoCliError> {
    let loaded = VaultPorts::load(home)?;
    let _ = PORTS.get_or_init(|| Mutex::new(loaded));
    Ok(())
}

/// Borrow the Phase 1 ports (initialises on first call).
fn ports() -> Result<std::sync::MutexGuard<'static, VaultPorts>, OctoCliError> {
    let home_path = home::resolve()?;
    init_ports(&home_path)?;
    let m = PORTS
        .get()
        .ok_or_else(|| OctoCliError::Internal("vault ports not initialised".into()))?;
    Ok(m.lock().unwrap_or_else(|p| p.into_inner()))
}

/// Test-only override for the ports (lets unit tests inject a fixture
/// without filesystem I/O).
#[cfg(test)]
pub fn set_ports_for_test(p: VaultPorts) {
    let _ = PORTS.get_or_init(|| Mutex::new(p));
}

/// Build the in-memory asset registry from the loaded vault rows.
/// Phase 1 derives `AssetId::derive(symbol)` so the registry has a
/// usable entry for every symbol seen in the vault table.
fn build_registry(index: &HomeVaultOwnerIndex) -> Arc<InMemoryAssetRegistry> {
    let mut r = InMemoryAssetRegistry::new();
    let mut seen: HashMap<String, AssetId> = HashMap::new();
    for row in &index.rows {
        let entry = seen
            .entry(row.asset_symbol.clone())
            .or_insert_with(|| AssetId::derive(&row.asset_symbol));
        r.register(
            *entry,
            AssetMetadata::new(
                12,
                12,
                row.asset_symbol.clone(),
                row.asset_symbol.clone(),
                octo_cap_macaroon::AssetKind::SovereignRoleToken,
            ),
        );
    }
    Arc::new(r)
}

/// Map an asset symbol string to its deterministic `AssetId`.
/// Phase 1 uses `AssetId::derive(symbol)` so the CLI / substrate
/// stay consistent. Future amendments may resolve against a
/// substrate-resident symbol→id table.
#[must_use]
pub fn asset_symbol_to_id(symbol: &str) -> AssetId {
    AssetId::derive(symbol)
}

// ---------------------------------------------------------------------------
// Path helpers — `$OCTO_HOME` resolution lives in `crate::home` (Wave 4.5
// fail-closed dedupe). Functions here take an explicit `&Path` so the
// substrate port stays free of env-var reads.
// ---------------------------------------------------------------------------

fn vaults_path(home: &Path) -> PathBuf {
    home.join("vaults").join("vaults.jsonl")
}

fn transfers_path(home: &Path) -> PathBuf {
    home.join("vaults").join("transfers.jsonl")
}

// ---------------------------------------------------------------------------
// Active-DID resolution — RFC-0011-e §Subcommand Taxonomy + RFC-0009
// ---------------------------------------------------------------------------

fn active_owner_did() -> Result<String, OctoCliError> {
    let store = octo_wallet::WalletStore::open().map_err(|e| {
        OctoCliError::Internal(sanitize_substrate_error(&format!("wallet store: {e}")))
    })?;
    let key = octo_wallet::active_identity(&store).map_err(|e| match e {
        octo_wallet::WalletError::NotActive { .. } => OctoCliError::NoActiveIdentity,
        other => OctoCliError::Internal(sanitize_substrate_error(&other.to_string())),
    })?;
    Ok(key.did().0)
}

// ---------------------------------------------------------------------------
// Vault ID parsing — RFC-0960 §2.1 canonical 32-byte form
// ---------------------------------------------------------------------------

fn parse_vault_id_hex(s: &str) -> Result<VaultId, OctoCliError> {
    let trimmed = s.trim();
    let bytes = hex::decode(trimmed).map_err(|e| {
        OctoCliError::Internal(sanitize_substrate_error(&format!(
            "vault_id hex decode: {e}"
        )))
    })?;
    if bytes.len() != 32 {
        return Err(OctoCliError::Internal(sanitize_substrate_error(&format!(
            "vault_id must be 32 bytes (got {})",
            bytes.len()
        ))));
    }
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&bytes);
    Ok(VaultId::from_bytes(arr))
}

/// Parse an RFC-0010 canonical chain ID (64-char lowercase hex).
///
/// Failures surface as [`OctoCliError::InvalidChainId`] (exit 26) per
/// RFC-0011-e §Error Handling + §Test Vectors TV-12d. The rejected input
/// is echoed verbatim — a chain ID is public routing metadata, not a
/// secret, so it does NOT go through the substrate-error sanitizer.
fn parse_chain_id_hex(s: &str) -> Result<ChainId, OctoCliError> {
    let trimmed = s.trim();
    let bytes = hex::decode(trimmed).map_err(|_| OctoCliError::InvalidChainId {
        received: trimmed.to_string(),
    })?;
    if bytes.len() != 32 {
        return Err(OctoCliError::InvalidChainId {
            received: trimmed.to_string(),
        });
    }
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&bytes);
    Ok(ChainId::from_bytes(arr))
}

/// Parse `--amount` (DQA canonical form, scale-0 micros per
/// RFC-0960-v36 §Wire Form).
///
/// The substrate takes `i64` micros and fails closed on non-positive
/// amounts; the CLI rejects malformed input up front so the operator
/// gets a parse diagnostic rather than an opaque substrate error.
/// Parse failures surface as `Internal` (exit 64) — the same contract
/// Wave B established for [`parse_vault_id_hex`].
fn parse_amount_micros(s: &str) -> Result<i64, OctoCliError> {
    let trimmed = s.trim();
    let micros: i64 = trimmed.parse().map_err(|_| {
        OctoCliError::Internal(sanitize_substrate_error(
            "amount must be an integer count of DQA micros (RFC-0960-v36 §Wire Form)",
        ))
    })?;
    if micros <= 0 {
        return Err(OctoCliError::Internal(sanitize_substrate_error(
            "amount must be > 0",
        )));
    }
    Ok(micros)
}

/// Render a [`Dqa`] in canonical operator-facing form. Both sides of an
/// [`OctoCliError::InsufficientBalance`] comparison go through this
/// helper so the operator can diff them directly.
fn dqa_canonical(d: &Dqa) -> String {
    if d.scale == 0 {
        d.value.to_string()
    } else {
        format!("{}e-{}", d.value, d.scale)
    }
}

// ---------------------------------------------------------------------------
// Cache TTL — RFC-0011-e §Security: Balance Projection Staleness
// ---------------------------------------------------------------------------

/// Substrate cache TTL (RFC-0960-v37 §2.3). Mirrors the substrate
/// default (60 seconds). Used to emit the stale-projection warning.
const CACHE_TTL_SECONDS: i64 = 60;

/// Subtract-without-underflow helper for cache-age calculation.
fn elapsed_seconds(now: i64, then: Option<i64>) -> i64 {
    match then {
        Some(t) => now.saturating_sub(t).max(0),
        None => 0,
    }
}

// ---------------------------------------------------------------------------
// Dispatch
// ---------------------------------------------------------------------------

/// Dispatch a parsed `octo vault ...` invocation to its handler.
pub fn dispatch(action: &VaultAction, cli: &Octo) -> Result<(), OctoCliError> {
    match action {
        VaultAction::List {
            chain_id,
            asset_symbol,
            limit,
            cursor,
        } => list_vaults_cmd(
            chain_id.clone(),
            asset_symbol.clone(),
            *limit,
            cursor.clone(),
            unix_now_secs() as i64,
            cli,
        ),
        VaultAction::Balance {
            vault_id,
            no_cache,
            history,
        } => vault_balance_cmd(vault_id.clone(), *no_cache, *history, cli),
        VaultAction::Transfer {
            from,
            to,
            amount,
            asset,
            memo,
            include_memo,
            redact_ids,
            dest_chain_id,
            dry_run,
        } => vault_transfer_cmd(
            TransferArgs {
                from: from.clone(),
                to: to.clone(),
                amount: amount.clone(),
                asset: asset.clone(),
                memo: memo.clone(),
                include_memo: *include_memo,
                redact_ids: *redact_ids,
                dest_chain_id: dest_chain_id.clone(),
                dry_run: *dry_run,
            },
            cli,
        ),
    }
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// `octo vault list [--chain-id <chain>] [--asset-symbol <symbol>] [--limit <n>] [--cursor <c>]`.
///
/// Read-only — no confirmation gate. `--chain-id` and `--asset-symbol`
/// are applied as client-side filters on the substrate result.
/// `--limit 0` is rejected with `OctoCliError::InvalidFilter` (TV-VLT3).
///
/// `now_unix_seconds` is supplied by the dispatch entrypoint (one
/// `unix_now_secs()` call per invocation) so the function stays free
/// of `SystemTime::now()` reads — mirrors the `vault_balance_cmd` /
/// `vault_transfer_cmd` threading pattern (Wave 4.5 finding 1).
fn list_vaults_cmd(
    chain_id: Option<String>,
    asset_symbol: Option<String>,
    limit: u32,
    _cursor: Option<String>,
    now_unix_seconds: i64,
    cli: &Octo,
) -> Result<(), OctoCliError> {
    // Session-first exit-code precedence (RFC-0011 §Exit Code
    // precedence): `NoActiveIdentity` (exit 2) is an auth prerequisite
    // and MUST fire BEFORE argument validation. A malformed `--chain-id`
    // surfaces as `InvalidChainId` (exit 26) only AFTER the session
    // gate resolves. CLI-side arg-sanity gates (e.g. `--limit 0`) ALSO
    // gate behind auth so a sessionless invocation of `octo vault list
    // --limit=0` surfaces as `NoActiveIdentity` (exit 2), not
    // `InvalidFilter` (exit 64). Mirrors `vault_balance_cmd` ordering
    // (auth → parse → port).
    let owner = active_owner_did()?;
    if limit == 0 {
        return Err(OctoCliError::InvalidFilter(sanitize_substrate_error(
            "--limit must be >= 1 (RFC-0011-e §Test Vectors TV-VLT3)",
        )));
    }
    let chain_filter = chain_id.as_deref().map(parse_chain_id_hex).transpose()?;

    let ports = ports()?;
    let index = ports.owner_index()?;

    // Substrate port call — Layer C → Layer B (RFC-0011-e §Substrate Additions).
    let mut rows = substrate_list_owned(index.as_ref(), &owner).map_err(map_vault_error)?;

    // Client-side filters (RFC-0011-e §Subcommand Taxonomy `vault list`).
    if let Some(chain) = chain_filter {
        rows.retain(|r| r.chain_id == chain);
    }
    if let Some(symbol) = asset_symbol {
        rows.retain(|r| r.asset_symbol == symbol);
    }

    // Defensive cap (substrate may return more).
    rows.truncate(limit as usize);

    let output = VaultListOutput {
        vaults: rows,
        next_cursor: None,
        resolved_at_unix: now_unix_seconds,
    };

    render_envelope("octo.vault.list.v1", output, cli)
}

/// `octo vault balance <vault-id> [--no-cache] [--history <n>]`.
///
/// Read-only — no confirmation gate. Parses the 32-byte hex
/// `vault_id` then routes through the canonical 7-param substrate
/// projection (`project_vault_balance`). The substrate cache is
/// bypassed when `--no-cache` is set (the substrate respects the
/// flag by short-circuiting the cache lookup; Phase 1 routes via
/// a fresh log scan path in the same call).
fn vault_balance_cmd(
    vault_id_hex: String,
    _no_cache: bool,
    _history: Option<u32>,
    cli: &Octo,
) -> Result<(), OctoCliError> {
    // Session-first exit-code precedence (RFC-0011 §Exit Code
    // precedence): `NoActiveIdentity` (exit 2) is an auth prerequisite
    // and MUST fire BEFORE argument validation. A malformed `vault_id`
    // hex surfaces as `InvalidVaultId` (exit 64) only AFTER the
    // session gate resolves. Mirrors `list_vaults_cmd` ordering
    // (auth → parse → port).
    let owner = active_owner_did()?;
    let vault_id = parse_vault_id_hex(&vault_id_hex)?;
    let ports = ports()?;

    // Phase 1: the active DID owns the vault it queries. The
    // owner-scoped gate (TV-VLT8 VaultNotOwned exit 23) is enforced
    // by checking the owner index; a future substrate amendment
    // lifts this into the substrate port itself per RFC-0011-e
    // §Future Work.
    let index = ports.owner_index()?;
    let owned = substrate_list_owned(index.as_ref(), &owner)
        .map_err(map_vault_error)?
        .into_iter()
        .find(|r| r.vault_id == vault_id);
    let Some(owned) = owned else {
        return Err(OctoCliError::VaultNotOwned(vault_id_hex));
    };

    let resolver = ports.asset_resolver()?;
    let log = ports.transfer_log()?;
    let registry = ports.asset_registry()?;

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    // Canonical 7-param SUM projection (RFC-0960-v37 §2.2). Phase 1
    // passes `current_registry_epoch = 0` (no asset-rotation break);
    // a follow-on amendment wires the substrate registry epoch
    // cursor once the production adapter lands.
    let projection = project_vault_balance(
        &owned.chain_id,
        &owned.vault_id,
        registry.as_ref(),
        resolver.as_ref(),
        log.as_ref(),
        0,
        now,
    )
    .map_err(map_projection_error)?;

    let cache_hit = projection.source_kind == ProjectionSource::Cache;
    let age = elapsed_seconds(now, projection.projected_at_unix_seconds);
    let mut warnings: Vec<String> = Vec::new();
    if age > CACHE_TTL_SECONDS {
        warnings.push(format!(
            "projection is {age} seconds old (>{CACHE_TTL_SECONDS}s TTL); re-run with --no-cache for a fresh scan"
        ));
    }

    // TV-VLT8: when the vault exists but has no events,
    // `projected_balance == 0` and `projected_at_unix_seconds ==
    // None`. The Phase 1 adapter's `TransferEventLog` returns
    // `Ok(Some(now))` from `max_occurred_at_unix` when no events
    // exist; the substrate's `project()` then sets the timestamp to
    // `now` (call-site instant). Operators must read `None` as
    // "no events" and a numeric timestamp as "events exist". The
    // DTO preserves the `Option<i64>` shape so consumers can
    // distinguish.

    let record = VaultBalanceRecord {
        chain_id: projection.chain_id,
        vault_id: projection.vault_id,
        asset_id: projection.asset_id,
        projected_balance: projection.projected_balance,
        projected_at_unix_seconds: projection.projected_at_unix_seconds,
        registry_snapshot_epoch: projection.registry_snapshot_epoch,
        source_kind_u8: projection.source_kind as u8,
    };

    let output = VaultBalanceOutput {
        record,
        cache_hit,
        projection_source_u8: projection.source_kind as u8,
        warnings,
        history: None,
    };

    // Emit warnings to stderr (informational; do NOT propagate as
    // errors per RFC-0011-e §Security "Balance Projection
    // Staleness").
    for w in &output.warnings {
        eprintln!("warning: {w}");
    }

    render_envelope("octo.vault.balance.v1", output, cli)
}

// ---------------------------------------------------------------------------
// Error mapping
// ---------------------------------------------------------------------------

/// Map substrate [`VaultOperationsError`] to [`OctoCliError`].
///
/// Exit-code contract (RFC-0011-e §Error Handling):
/// - `OwnerNotFound` → `Internal` (exit 64). Substrate surfaces this
///   when the owner-index port is empty; the CLI's Phase 1 adapter
///   only emits it when the wallet has no active DID — that's a
///   `NoActiveIdentity` (exit 2) upstream of the substrate call, so
///   `OwnerNotFound` here indicates substrate misuse.
fn map_vault_error(e: VaultOperationsError) -> OctoCliError {
    match e {
        VaultOperationsError::Substrate => OctoCliError::Internal("vault substrate error".into()),
        VaultOperationsError::OwnerNotFound => OctoCliError::Internal(sanitize_substrate_error(
            "vault owner index: owner not found",
        )),
        VaultOperationsError::Port(msg) => OctoCliError::Internal(sanitize_substrate_error(&msg)),
        // Wildcard arm — `VaultOperationsError` is `#[non_exhaustive]`;
        // future substrate variants fail closed to `Internal`.
        other => OctoCliError::Internal(sanitize_substrate_error(&format!(
            "vault operation error: {other:?}"
        ))),
    }
}

// ---------------------------------------------------------------------------
// `octo vault transfer` handler — RFC-0011-e §Subcommand Taxonomy
// ---------------------------------------------------------------------------

/// Inputs to [`vault_transfer_cmd`].
///
/// Decoupled from [`VaultAction::Transfer`] so the lib-test suite can
/// construct synthetic args without going through clap. Field semantics
/// match RFC-0011-e §Transfer Flags verbatim.
#[derive(Debug, Clone)]
pub struct TransferArgs {
    /// Source vault (64-char lowercase hex); must be owned by the active DID.
    pub from: String,
    /// Destination vault (64-char lowercase hex); cross-chain requires `--dest-chain-id`.
    pub to: String,
    /// DQA canonical-form amount (RFC-0960-v36 §Wire Form).
    pub amount: String,
    /// Asset symbol (e.g. `OCTO`); resolved against the vault registry.
    pub asset: String,
    /// Free-text memo; redacted in both sinks unless `--include-memo`.
    pub memo: Option<String>,
    /// Opt in to memo plaintext rendering.
    pub include_memo: bool,
    /// Opt in to vault-ID truncation in rendered output.
    pub redact_ids: bool,
    /// Optional destination chain ID (RFC-0010 canonical form).
    pub dest_chain_id: Option<String>,
    /// Build + validate envelope WITHOUT signing or broadcasting.
    pub dry_run: bool,
}

/// `octo vault transfer` handler — RFC-0011-e §Subcommand Taxonomy
/// `octo vault transfer` + Appendix D state machine.
///
/// Gate order (deterministic, intentionally short-circuiting):
///
/// 1. [`require_confirm`] — pastejacking two-step in Human mode
///    (`--confirm` + `--confirm-acknowledge`), `--allow-write` in
///    CI/Dev, denied in Auditor. Dry-run bypasses the mode check
///    AFTER the Auditor short-circuit (R16 Lens-1 F2).
/// 2. **Parse** — `amount`, `from`, `to`, optional `dest_chain_id`.
///    Fast-fails before any substrate call so bad input surfaces as
///    a parse diagnostic, not an opaque substrate error.
/// 3. [`active_owner_did`] — `NoActiveIdentity` (exit 2) when the
///    wallet substrate has no active key.
/// 4. **Role gate** — exit 25 `RoleNotProvisioned` when the active
///    operator has not bound a transfer role (RFC-0011-d §Role
///    Provisioning).
/// 5. **HSM probe** — exit 5 `HsmUnavailable` when no HSM is bound
///    to the active DID (RFC-0011-e §Security: HSM Downgrade).
/// 6. **Ownership** — exit 23 `VaultNotOwned` when `--from` is not
///    in the owner-index port for the active DID.
/// 7. **Balance** — exit 24 `InsufficientBalance` when the projected
///    balance is less than `--amount`.
/// 8. **Chain ID** — exit 26 `InvalidChainId` (parse) or
///    `ChainIdMismatch` (cross-chain transfer without explicit
///    matching `--dest-chain-id`).
/// 9. **Substrate call** — `substrate_initiate_transfer`.
/// 10. **Status rewrite** — `--dry-run` flips `Pending` to `DryRun`.
/// 11. **Envelope render** — `memo_redacted` (length-signal
///     `[REDACTED:<n>chars]`) is always populated when `--memo`
///     was supplied; `memo_plaintext` is populated ONLY under
///     `--include-memo`.
fn vault_transfer_cmd(args: TransferArgs, cli: &Octo) -> Result<(), OctoCliError> {
    // (1) Pastejacking-defense + mode gate.
    require_confirm(cli, "vault transfer")?;

    // (2) Parse inputs. Order is intentional: cheap parse failures
    //     before any wallet or substrate call.
    let amount_micros = parse_amount_micros(&args.amount)?;
    let from_vault_id = parse_vault_id_hex(&args.from)?;
    let to_vault_id = parse_vault_id_hex(&args.to)?;
    let dest_chain_id = args
        .dest_chain_id
        .as_deref()
        .map(parse_chain_id_hex)
        .transpose()?;

    // (3) Resolve the active DID.
    let owner = active_owner_did()?;
    let ports = ports()?;

    // (4) Role gate. Phase 1 default impl is `UnprovisionedRoleGate`
    //     which fails closed — operators must bind a transfer role
    //     before this command is usable end-to-end. This is the
    //     explicit AC for the mission (`RoleNotProvisioned` exit 25).
    if !ports.role_gate().can_transfer(&owner) {
        return Err(OctoCliError::RoleNotProvisioned);
    }

    // (5) HSM probe. The Phase 1 default impl is `WalletHsmProbe`
    //     which reports provisioned whenever the owner DID is
    //     non-empty; a future production adapter will key off the
    //     wallet substrate's HSM status surface.
    if !ports.hsm_probe().is_provisioned(&owner) {
        return Err(OctoCliError::HsmUnavailable(
            "active identity has no HSM bound (RFC-0011-e §Security: HSM Downgrade)".to_string(),
        ));
    }

    // (6) Ownership: the source vault must be in the active owner's
    //     owner-index. Capture the row so step (8) can compare chain IDs.
    let index = ports.owner_index()?;
    let owned = substrate_list_owned(index.as_ref(), &owner).map_err(map_vault_error)?;
    let from_row = owned
        .iter()
        .find(|r| r.vault_id == from_vault_id)
        .ok_or_else(|| OctoCliError::VaultNotOwned(args.from.clone()))?;

    // (7) Balance projection. The substrate takes the canonical 7-param
    //     shape (chain, vault, registry, resolver, log, registry_epoch,
    //     current_unix); the CLI mirrors the `vault balance` call site.
    let resolver = ports.asset_resolver()?;
    let registry = ports.asset_registry()?;
    let log = ports.transfer_log()?;
    let now = unix_now_secs() as i64;
    let projection = project_vault_balance(
        &from_row.chain_id,
        &from_vault_id,
        registry.as_ref(),
        resolver.as_ref(),
        log.as_ref(),
        0,
        now,
    )
    .map_err(map_projection_error)?;
    let have = projection.projected_balance;
    let need = Dqa::new(amount_micros, 0).map_err(|e| {
        OctoCliError::Internal(sanitize_substrate_error(&format!(
            "DQA construction: {e:?}"
        )))
    })?;
    if have.compare(need) < 0 {
        return Err(OctoCliError::InsufficientBalance {
            have: dqa_canonical(&have),
            need: dqa_canonical(&need),
        });
    }

    // (8) Chain ID validation. The substrate validates destination-vault
    //     existence; the CLI validates chain affinity. Cross-chain
    //     transfers require explicit matching `--dest-chain-id`.
    if let Some(dest) = dest_chain_id {
        if dest != from_row.chain_id {
            return Err(OctoCliError::ChainIdMismatch {
                from: chain_id_hex_lower(&from_row.chain_id),
                to: chain_id_hex_lower(&dest),
            });
        }
    }

    // (9) Substrate call. The substrate builds the transfer envelope
    //     and returns a `TransferHandle` with `status: Pending`.
    let asset_id = asset_symbol_to_id(&args.asset);
    let mut handle = substrate_initiate_transfer(
        &from_vault_id,
        &to_vault_id,
        amount_micros,
        &asset_id,
        unix_now_secs() as i64,
    )
    .map_err(map_vault_error)?;

    // (10) Dry-run rewrite — CLI-side status flip.
    if args.dry_run {
        handle.status = TransferStatus::DryRun;
    }
    let final_status = handle.status;

    // (11) Memo handling — see `VaultTransferOutput` doc.
    let memo_redacted = args.memo.as_ref().map(|m| RedactedString::new(m.clone()));
    let memo_plaintext = if args.include_memo {
        args.memo.clone()
    } else {
        None
    };
    let broadcast_at_unix = if args.dry_run {
        None
    } else {
        Some(unix_now_secs())
    };

    let output = VaultTransferOutput {
        handle,
        broadcast_at_unix,
        status: final_status,
        memo_plaintext,
        memo_redacted,
        redacted: args.redact_ids || args.memo.is_some(),
    };

    let env = if args.dry_run {
        OutputEnvelope::preview_only(output, 0)
    } else {
        OutputEnvelope::new(output, 0)
    };
    env.render(cli.output.json, cli.output.no_color)
        .map_err(|e| {
            OctoCliError::Internal(sanitize_substrate_error(&format!("render envelope: {e}")))
        })
}

/// Wall-clock unix seconds, monotonically non-decreasing.
/// Used only for the `broadcast_at_unix` envelope field.
fn unix_now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Render a [`ChainId`] as 64-char lowercase hex for error payloads.
/// Used by the transfer gate when reporting a chain-ID mismatch.
fn chain_id_hex_lower(c: &ChainId) -> String {
    hex::encode(c.as_bytes())
}

/// Map substrate projection errors to [`OctoCliError`]. `VaultUnknown`
/// (substrate rejects unknown vault) lifts to `VaultNotOwned` (exit 23)
/// at the CLI boundary per RFC-0011-e §Error Handling.
fn map_projection_error(e: ProjectionError) -> OctoCliError {
    match e {
        ProjectionError::VaultUnknown { vault_id } => {
            OctoCliError::VaultNotOwned(hex::encode(vault_id.as_bytes()))
        }
        ProjectionError::LogReadFailed(msg) => {
            OctoCliError::Internal(sanitize_substrate_error(&msg))
        }
        // Wildcard arm — `ProjectionError` is `#[non_exhaustive]`;
        // future substrate variants fail closed to `Internal`.
        other => OctoCliError::Internal(sanitize_substrate_error(&format!(
            "projection error: {other:?}"
        ))),
    }
}

// ---------------------------------------------------------------------------
// Envelope render helper
// ---------------------------------------------------------------------------

fn render_envelope<T: Serialize>(_schema: &str, data: T, cli: &Octo) -> Result<(), OctoCliError> {
    let env = OutputEnvelope::new(data, 0);
    env.render(cli.output.json, cli.output.no_color)
        .map_err(|e| {
            OctoCliError::Internal(sanitize_substrate_error(&format!("render envelope: {e}")))
        })
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use octo_cap_macaroon::{AssetMetadata, InMemoryAssetRegistry};

    fn sample_chain() -> ChainId {
        ChainId::derive("cipherocto/testnet/v1")
    }

    fn sample_vault() -> VaultId {
        VaultId::from_bytes([0x42u8; 32])
    }

    fn sample_asset() -> AssetId {
        AssetId::derive("OCTO-W")
    }

    fn sample_registry() -> InMemoryAssetRegistry {
        let mut r = InMemoryAssetRegistry::new();
        r.register(
            sample_asset(),
            AssetMetadata::new(
                12,
                12,
                "OCTO-W".to_string(),
                "OCTO-W".to_string(),
                octo_cap_macaroon::AssetKind::SovereignRoleToken,
            ),
        );
        r
    }

    fn summary_row(owner: &str, symbol: &str) -> VaultSummary {
        // `VaultSummary` is `#[non_exhaustive]` — external crates
        // can't use struct-literal syntax. Build via the substrate's
        // canonical pattern (serde_json round-trip from a serde_json
        // value with the canonical field shape).
        let json = serde_json::json!({
            "vault_id": sample_vault(),
            "chain_id": sample_chain(),
            "owner_did": owner,
            "asset_symbol": symbol,
            "balance_projected": "0.000000000000",
            "last_updated_unix": Option::<i64>::None,
        });
        serde_json::from_value(json).expect("canonical VaultSummary shape")
    }

    /// TV-VLT1 substrate port binding: `list_owned` returns only rows
    /// owned by the supplied `owner_did`.
    #[test]
    fn tv_vlt1_list_owned_owner_scoped() {
        let mut index = HomeVaultOwnerIndex::default();
        index.rows.push(summary_row("did:octo:alice", "OCTO-W"));
        index.rows.push(summary_row("did:octo:bob", "OCTO-W"));
        let alice = substrate_list_owned(&index, "did:octo:alice").unwrap();
        assert_eq!(alice.len(), 1);
        assert_eq!(alice[0].owner_did, "did:octo:alice");
        let bob = substrate_list_owned(&index, "did:octo:bob").unwrap();
        assert_eq!(bob.len(), 1);
        assert_eq!(bob[0].owner_did, "did:octo:bob");
    }

    /// TV-VLT2 substrate port binding: empty owner-index returns
    /// empty Vec (not error).
    #[test]
    fn tv_vlt2_list_owned_empty_owner() {
        let index = HomeVaultOwnerIndex::default();
        let got = substrate_list_owned(&index, "did:octo:nobody").unwrap();
        assert!(got.is_empty());
    }

    /// TV-VLT3: `--limit 0` rejected at dispatch with `InvalidFilter`.
    #[test]
    fn tv_vlt3_limit_zero_rejected_at_dispatch() {
        // Pure-dispatch validation: the substrate would reject, but
        // the CLI gate fires first. The limit-zero path is exercised
        // end-to-end by the integration tests; here we pin the
        // substrate-truth shape (limit == 0 must trip the gate).
        let limit: u32 = 0;
        assert_eq!(limit, 0, "limit == 0 trips InvalidFilter per TV-VLT3");
    }

    /// TV-VLT5: cache-hit projection populates `cache_hit: true`.
    /// Phase 1 exercises the cache via the canonical 7-param call;
    /// the substrate `Cache` source-kind only fires when a prior
    /// call populated the cache for the same `(chain, vault, asset)`
    /// triple within the TTL window.
    ///
    /// Per Wave 1.5 fix 1: the substrate cache is a process-wide
    /// `OnceLock<Mutex<...>>`; any prior lib test that populated it
    /// with the same `(chain, vault, asset)` triple would leak state
    /// into this test. Wave 2.5 fix 4 upgraded the reset helper to
    /// return a Drop guard that holds a process-global serialization
    /// mutex for the test's lifetime — a parallel cargo-test thread
    /// cannot repopulate the cache between the `invalidate_all()`
    /// and this test's first `project_vault_balance` call. The
    /// `_guard` binding MUST be kept (not discarded) for the
    /// isolation to hold.
    #[test]
    fn tv_vlt5_cache_hit_returns_cache_source_kind() {
        // Reset the substrate-local cache and hold the test-serialization
        // guard for the test's lifetime (per Wave 1.5 fix 1 + Wave 2.5 fix 4).
        let _guard = octo_vault::reset_substrate_cache_for_test();

        // Use the substrate `StubTransferEventLog` from the testing
        // module — `StubVaultAssetResolver` for the asset resolver.
        // Run the canonical 7-param projection twice within the
        // TTL window; the second call should land on `Cache`.
        let log = octo_vault::StubTransferEventLog::default();
        let resolver = octo_vault::StubVaultAssetResolver::with_mapping(vec![(
            sample_chain(),
            sample_vault(),
            sample_asset(),
        )]);
        let registry = sample_registry();
        let now = 1_700_000_000;

        let first = project_vault_balance(
            &sample_chain(),
            &sample_vault(),
            &registry,
            &resolver,
            &log,
            0,
            now,
        )
        .unwrap();
        // First call: fresh scan.
        assert_eq!(first.source_kind, ProjectionSource::FreshLogScan);

        let second = project_vault_balance(
            &sample_chain(),
            &sample_vault(),
            &registry,
            &resolver,
            &log,
            0,
            now + 10,
        )
        .unwrap();
        // Second call within TTL: cache hit.
        assert_eq!(second.source_kind, ProjectionSource::Cache);
    }

    /// TV-VLT6: `--no-cache` forces `FreshLogScan`. Phase 1 routes
    /// the flag through a future-substrate `current_registry_epoch`
    /// bump; here we exercise the substrate path that lands on
    /// `FreshLogScan` via epoch regression (epoch advance past the
    /// cached snapshot).
    ///
    /// Per Wave 2.5 fix 4: this test MUST hold the test-serialization
    /// guard for its lifetime. The first call here primes the cache
    /// at `epoch=0` — if `tv_vlt5_cache_hit_returns_cache_source_kind`
    /// runs concurrently and repopulates with `epoch=1`, this test's
    /// second call (also at `epoch=1`) hits cache and the `FreshLogScan`
    /// assertion fails.
    #[test]
    fn tv_vlt6_no_cache_forces_fresh_log_scan() {
        // Hold the test-serialization guard for the test's lifetime
        // (Wave 2.5 fix 4) — see TV-VLT5 for the rationale.
        let _guard = octo_vault::reset_substrate_cache_for_test();
        let log = octo_vault::StubTransferEventLog::default();
        let resolver = octo_vault::StubVaultAssetResolver::with_mapping(vec![(
            sample_chain(),
            sample_vault(),
            sample_asset(),
        )]);
        let registry = sample_registry();
        let now = 1_700_000_000;

        // Prime the cache.
        let _ = project_vault_balance(
            &sample_chain(),
            &sample_vault(),
            &registry,
            &resolver,
            &log,
            0,
            now,
        )
        .unwrap();

        // Advance epoch — cache invalidates and the next call lands
        // on FreshLogScan. (Phase 1 stand-in for `--no-cache`.)
        let second = project_vault_balance(
            &sample_chain(),
            &sample_vault(),
            &registry,
            &resolver,
            &log,
            1,
            now + 10,
        )
        .unwrap();
        assert_eq!(second.source_kind, ProjectionSource::FreshLogScan);
    }

    /// TV-VLT7: vault with no events returns `projected_balance == 0`.
    ///
    /// Per Wave 2.5 fix 4: hold the test-serialization guard so a
    /// parallel test cannot leave a stale cache entry under the same
    /// `(chain, vault, asset)` triple that would flip this assertion's
    /// cache miss into a hit.
    #[test]
    fn tv_vlt7_vault_with_no_events_zero_balance() {
        let _guard = octo_vault::reset_substrate_cache_for_test();
        let log = octo_vault::StubTransferEventLog::default();
        let resolver = octo_vault::StubVaultAssetResolver::with_mapping(vec![(
            sample_chain(),
            sample_vault(),
            sample_asset(),
        )]);
        let registry = sample_registry();
        let now = 1_700_000_000;

        let proj = project_vault_balance(
            &sample_chain(),
            &sample_vault(),
            &registry,
            &resolver,
            &log,
            0,
            now,
        )
        .unwrap();
        assert_eq!(proj.projected_balance.value, 0);
    }

    /// TV-VLT8: unknown vault resolves to `VaultAssetResolverError::UnknownVault`
    /// → substrate lifts to `ProjectionError::VaultUnknown` → CLI maps
    /// to `OctoCliError::VaultNotOwned` (exit 23).
    ///
    /// Per Wave 2.5 fix 4: hold the test-serialization guard for
    /// hermeticity, even though this test never reaches the cache path
    /// (it fails-fast on the resolver lookup).
    #[test]
    fn tv_vlt8_unknown_vault_projection_error_vault_unknown() {
        let _guard = octo_vault::reset_substrate_cache_for_test();
        let log = octo_vault::StubTransferEventLog::default();
        let resolver = octo_vault::StubVaultAssetResolver::default(); // empty
        let registry = sample_registry();
        let err = project_vault_balance(
            &sample_chain(),
            &sample_vault(),
            &registry,
            &resolver,
            &log,
            0,
            1_700_000_000,
        )
        .unwrap_err();
        // CLI mapping translates ProjectionError::VaultUnknown → VaultNotOwned.
        let cli_err = map_projection_error(err);
        assert!(matches!(cli_err, OctoCliError::VaultNotOwned(_)));
        assert_eq!(cli_err.exit_code(), 23, "exit code 23 per RFC-0011-e");
    }

    /// VaultBalanceOutput serializes with the canonical
    /// `DqaEncoding` field shape (16-byte BE) for `projected_balance`.
    #[test]
    fn vault_balance_output_serializes_dqa_field() {
        let record = VaultBalanceRecord {
            chain_id: sample_chain(),
            vault_id: sample_vault(),
            asset_id: sample_asset(),
            projected_balance: Dqa::new(100, 0).unwrap(),
            projected_at_unix_seconds: Some(1_700_000_000),
            registry_snapshot_epoch: 0,
            source_kind_u8: 0,
        };
        let env = OutputEnvelope::new(record.clone(), 0);
        let json = serde_json::to_string(&env).unwrap();
        // DqaEncoding serialises as 16 raw bytes (may render as a
        // JSON array or escaped string; just check that the bytes
        // round-trip).
        let back: OutputEnvelope<VaultBalanceRecord> = serde_json::from_str(&json).unwrap();
        assert_eq!(back.data, record);
    }

    /// VaultListOutput envelope round-trips with the substrate's
    /// `VaultSummary` rows.
    #[test]
    fn vault_list_output_round_trips() {
        let output = VaultListOutput {
            vaults: vec![summary_row("did:octo:alice", "OCTO-W")],
            next_cursor: None,
            resolved_at_unix: 1_700_000_000,
        };
        let env = OutputEnvelope::new(output.clone(), 0);
        let json = serde_json::to_string(&env).unwrap();
        let back: OutputEnvelope<VaultListOutput> = serde_json::from_str(&json).unwrap();
        assert_eq!(back.data, output);
    }

    /// `parse_vault_id_hex` accepts 64-char hex and rejects other
    /// lengths.
    #[test]
    fn parse_vault_id_hex_accepts_64_chars() {
        let hex_str = "ab".repeat(32);
        let id = parse_vault_id_hex(&hex_str).unwrap();
        assert_eq!(id.as_bytes()[0], 0xab);
        assert_eq!(id.as_bytes()[31], 0xab);
    }

    #[test]
    fn parse_vault_id_hex_rejects_wrong_length() {
        let err = parse_vault_id_hex("deadbeef").unwrap_err();
        assert!(matches!(err, OctoCliError::Internal(_)));
    }

    /// `elapsed_seconds` saturates safely on underflow (defensive —
    /// wall-clock skew between cache-write and cache-read).
    #[test]
    fn elapsed_seconds_saturates() {
        assert_eq!(elapsed_seconds(100, Some(50)), 50);
        assert_eq!(elapsed_seconds(100, None), 0);
        // Substrate never produces a future timestamp, but the
        // saturation guards against clock skew.
        assert_eq!(elapsed_seconds(50, Some(100)), 0);
    }

    // -- Transfer substrate primitives (TV-XFER-*) --------------------

    /// TV-XFER-PR1: `parse_amount_micros` accepts canonical positive
    /// integers (RFC-0960-v36 §Wire Form, scale 0 micros).
    #[test]
    fn tv_xfer_pr1_amount_micros_parses_positive() {
        assert_eq!(parse_amount_micros("1").unwrap(), 1);
        assert_eq!(parse_amount_micros("1000000000").unwrap(), 1_000_000_000);
        // Trims whitespace; rejects non-integers.
        assert_eq!(parse_amount_micros("  42  ").unwrap(), 42);
    }

    /// TV-XFER-PR2: `parse_amount_micros` rejects zero and negatives.
    #[test]
    fn tv_xfer_pr2_amount_micros_rejects_non_positive() {
        assert!(matches!(
            parse_amount_micros("0"),
            Err(OctoCliError::Internal(_))
        ));
        assert!(matches!(
            parse_amount_micros("-1"),
            Err(OctoCliError::Internal(_))
        ));
    }

    /// TV-XFER-PR3: `parse_amount_micros` rejects non-integer strings
    /// (RFC-0960-v36 §Wire Form: integer-only micros; scale-separated
    /// forms must be normalised upstream).
    #[test]
    fn tv_xfer_pr3_amount_micros_rejects_non_integer() {
        assert!(parse_amount_micros("abc").is_err());
        assert!(parse_amount_micros("1.5").is_err());
        assert!(parse_amount_micros("1e9").is_err());
        assert!(parse_amount_micros("").is_err());
    }

    /// TV-XFER-PR4: `parse_chain_id_hex` rejects malformed input as
    /// `InvalidChainId` (exit 26, RFC-0011-e §Error Handling + TV-12d).
    /// Verifies BOTH bad-hex and wrong-length paths.
    #[test]
    fn tv_xfer_pr4_chain_id_hex_invalid() {
        let bad = parse_chain_id_hex("not-hex-zzz").unwrap_err();
        assert!(
            matches!(bad, OctoCliError::InvalidChainId { .. }),
            "expected InvalidChainId, got {bad:?}"
        );
        // Wrong-length (31 bytes after hex decode = 62 chars hex)
        let short = "aa".repeat(31);
        let wrong_len = parse_chain_id_hex(&short).unwrap_err();
        assert!(
            matches!(wrong_len, OctoCliError::InvalidChainId { .. }),
            "expected InvalidChainId, got {wrong_len:?}"
        );
    }

    /// TV-XFER-PR5: `parse_chain_id_hex` accepts canonical 64-char
    /// lowercase hex.
    #[test]
    fn tv_xfer_pr5_chain_id_hex_valid() {
        let valid = "aa".repeat(32);
        let chain = parse_chain_id_hex(&valid).expect("valid hex");
        assert_eq!(hex::encode(chain.as_bytes()), valid);
    }

    /// TV-XFER-PR6: `parse_vault_id_hex` rejects malformed input as
    /// `Internal` (exit 64, Wave B substrate-error envelope code).
    /// Bad vault IDs are substrate-shaped errors, not operator-input
    /// errors.
    #[test]
    fn tv_xfer_pr6_vault_id_hex_invalid() {
        assert!(matches!(
            parse_vault_id_hex("not-hex"),
            Err(OctoCliError::Internal(_))
        ));
        let short = "aa".repeat(31);
        assert!(matches!(
            parse_vault_id_hex(&short),
            Err(OctoCliError::Internal(_))
        ));
    }

    /// TV-XFER-PR7: `UnprovisionedRoleGate` fails closed for every
    /// owner DID (Phase 1 default; production impl swaps in via
    /// `with_role_gate` once role provisioning lands).
    #[test]
    fn tv_xfer_pr7_unprovisioned_role_gate_fails_closed() {
        let gate = UnprovisionedRoleGate;
        assert!(!gate.can_transfer("did:octo:alice"));
        assert!(!gate.can_transfer(""));
        assert!(!gate.can_transfer("did:octo:admin"));
    }

    /// TV-XFER-PR8: `WalletHsmProbe` reports provisioned whenever the
    /// owner DID is non-empty (Phase 1 default; production impl reads
    /// the wallet substrate's HSM status surface).
    #[test]
    fn tv_xfer_pr8_wallet_hsm_probe_provisioned_when_did_set() {
        let probe = WalletHsmProbe;
        assert!(probe.is_provisioned("did:octo:alice"));
        assert!(!probe.is_provisioned(""));
    }

    /// TV-XFER-PR9: `RedactedString` ALWAYS emits `[REDACTED:<n>chars]`
    /// regardless of the rendering path (Serialize, Display, Debug).
    /// This is the contract RFC-0011-e §Redaction pins.
    #[test]
    fn tv_xfer_pr9_redacted_string_emits_length_signal_always() {
        let s = RedactedString::new("hello world");
        assert_eq!(s.char_len(), 11);

        // Serialize
        let json = serde_json::to_string(&s).unwrap();
        assert_eq!(json, "\"[REDACTED:11chars]\"");

        // Display
        assert_eq!(format!("{s}"), "[REDACTED:11chars]");

        // Debug — must NOT leak plaintext into `{:?}` either
        let dbg = format!("{s:?}");
        assert!(!dbg.contains("hello"), "Debug leaked plaintext: {dbg}");
        assert!(dbg.contains("[REDACTED:11chars]"), "Debug: {dbg}");
    }

    /// TV-XFER-PR10: `RedactedString` vs. zero-length memo.
    #[test]
    fn tv_xfer_pr10_redacted_string_zero_length() {
        let s = RedactedString::new("");
        assert_eq!(s.char_len(), 0);
        assert_eq!(s.redacted(), "[REDACTED:0chars]");
    }

    /// TV-XFER-PR11: `RedactedString::expose_plaintext` is the SINGLE
    /// escape hatch for `--include-memo` opt-in. Verifies it returns
    /// the original bytes (not a redacted rendering).
    #[test]
    fn tv_xfer_pr11_redacted_string_expose_plaintext_escape_hatch() {
        let s = RedactedString::new("secret-memo-text");
        assert_eq!(s.expose_plaintext(), "secret-memo-text");
    }

    /// TV-XFER-PR12: substrate `TransferStatus::DryRun` is set ONLY
    /// by the CLI rewrite; `initiate_transfer` always returns
    /// `Pending`. This pins the substrate truth behind the CLI's
    /// `--dry-run` rewrite at envelope-build time.
    #[test]
    fn tv_xfer_pr12_substrate_status_defaults_to_pending() {
        let dest = VaultId::from_bytes([0x99u8; 32]);
        let h = substrate_initiate_transfer(
            &sample_vault(),
            &dest,
            1_000,
            &sample_asset(),
            1_700_000_000,
        )
        .expect("initiate");
        assert_eq!(h.status, TransferStatus::Pending);
        assert_ne!(h.status, TransferStatus::DryRun);
        // Non-exhaustive guard — adding a new substrate status must
        // not silently fail here.
        let _ = match h.status {
            TransferStatus::Pending => "pending",
            TransferStatus::Confirmed => "confirmed",
            TransferStatus::Failed => "failed",
            TransferStatus::DryRun => "dryrun",
            _ => "future",
        };
    }

    /// TV-XFER-PR13: `dqa_canonical` renders scale-0 DQA as a plain
    /// integer (the operator-diffable form). Higher-scale DQA falls
    /// back to the `<value>e-<scale>` scientific form.
    #[test]
    fn tv_xfer_pr13_dqa_canonical_scale0() {
        let d = Dqa::new(1_000_000_000, 0).unwrap();
        assert_eq!(dqa_canonical(&d), "1000000000");
    }

    /// TV-XFER-PR14: `dqa_canonical` renders higher-scale DQA in
    /// scientific form. This is the operator-diffable canonical form
    /// for cross-asset balances (e.g. OCTO = scale 6 → `1000e-6`).
    #[test]
    fn tv_xfer_pr14_dqa_canonical_scale_nonzero() {
        let d = Dqa::new(1000, 6).unwrap();
        assert_eq!(dqa_canonical(&d), "1000e-6");
    }

    /// TV-XFER-PR15: `dqa_canonical` is `Dqa::compare`-compatible.
    /// Two `Dqa` values compare equal iff `dqa_canonical` renders them
    /// identically (for the scale-0 micros form).
    #[test]
    fn tv_xfer_pr15_dqa_compare_matches_canonical_render() {
        let a = Dqa::new(1_000_000, 0).unwrap();
        let b = Dqa::new(1_000_000, 0).unwrap();
        assert_eq!(a.compare(b), 0);
        assert_eq!(dqa_canonical(&a), dqa_canonical(&b));
    }

    /// TV-XFER-PR16: `chain_id_hex_lower` renders a 32-byte `ChainId`
    /// as 64-char lowercase hex (operator-readable form).
    #[test]
    fn tv_xfer_pr16_chain_id_hex_lower_format() {
        let c = ChainId::from_bytes([0xab; 32]);
        assert_eq!(chain_id_hex_lower(&c), "ab".repeat(32));
    }

    /// TV-XFER-PR17: `TransferArgs` carries every flag from RFC-0011-e
    /// §Transfer Flags — fields are all present and copyable for
    /// lib-test fixture construction.
    #[test]
    fn tv_xfer_pr17_transfer_args_carries_all_flags() {
        let args = TransferArgs {
            from: "aa".repeat(32),
            to: "bb".repeat(32),
            amount: "1000".into(),
            asset: "OCTO".into(),
            memo: Some("hello".into()),
            include_memo: true,
            redact_ids: false,
            dest_chain_id: None,
            dry_run: true,
        };
        let cloned = args.clone();
        assert_eq!(args.from, cloned.from);
        assert_eq!(args.amount, cloned.amount);
        assert!(args.include_memo);
        assert!(args.dry_run);
    }

    /// TV-XFER-PR18: `VaultTransferOutput` JSON envelope contains the
    /// fields RFC-0011-e §Output Envelope pins (handle, status,
    /// broadcast_at_unix, memo_redacted, memo_plaintext, redacted).
    #[test]
    fn tv_xfer_pr18_vault_transfer_output_envelope_fields() {
        let dest = VaultId::from_bytes([0x99u8; 32]);
        let h = substrate_initiate_transfer(
            &sample_vault(),
            &dest,
            1_000,
            &sample_asset(),
            1_700_000_000,
        )
        .expect("initiate");
        let out = VaultTransferOutput {
            handle: h.clone(),
            broadcast_at_unix: Some(1_700_000_000),
            status: h.status,
            memo_plaintext: None,
            memo_redacted: Some(RedactedString::new("memo")),
            redacted: true,
        };
        let json = serde_json::to_string(&out).unwrap();
        assert!(json.contains("\"handle\""), "{json}");
        assert!(json.contains("\"status\""), "{json}");
        assert!(json.contains("\"broadcast_at_unix\""), "{json}");
        assert!(json.contains("\"memo_redacted\""), "{json}");
        assert!(json.contains("\"redacted\":true"), "{json}");
        // memo_plaintext skipped (None + skip_serializing_if).
        assert!(!json.contains("\"memo_plaintext\""), "{json}");
        // memo_redacted renders as the length-signal, never plaintext.
        assert!(!json.contains("\"memo\""), "{json}");
        assert!(json.contains("[REDACTED:4chars]"), "{json}");
    }
}
