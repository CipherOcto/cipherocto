# RFC-0011 Caveat Form Amendment

| Field        | Value                                                       |
| ------------ | ----------------------------------------------------------- |
| Status       | Draft                                                       |
| Version      | (v1.0 lands at Phase 4 promotion)                           |
| Layer        | A (substrate-frozen `octo-cap-macaroon`)                    |
| Parent RFC   | RFC-0011                                                    |
| Companion    | `missions/open/0011-caveat-form-amendment.md` (Layer A)      |
|              | `missions/open/0011-vault-asset-chain-id-hex.md` (Layer B)  |

## Version History

| Version | Date | Change |
| ------- | ---- | ------ |
|         |      | (v1.0 lands at Phase 4 promotion — all 14 `tv_cf_*` vectors green + the inverted guide vector green) |

## Status

**Draft (2026-10-02).** Created as Phase 0 of the Caveat Form Alignment plan.
Promoted to `Accepted` after Phase 1 (`visit_str` + `hex_id_32` on `Caveat::Vault`)
and Phase 2 (`hex-ids` feature on `AssetId` / `ChainId` / `VaultId`) land and the
gate vectors `tv_cf_07..09` + `tv_cf_14` pass on every crate touched.

## Summary

RFC-0011 §Caveat Catalog is reaffirmed and extended to close the canonical-form
asymmetry between `Caveat::canonical_ser` (Layer A) and the `--caveats` parser
(Layer C). The canonical hex form emitted by `canonical_ser` for `AmountMax`,
`Payment.budget`, and `Vault` MUST be accepted by the same `Caveat` enum's
`Deserialize` impl. The asymmetry is a conformance drift; the corrected form
is documented as normative in this amendment.

## Context

The drift audit `docs/audits/2026-09-30-open-limitations-drift-audit.md` §9
records that the `AmountMax` / `Payment` scale-dropping defect is closed at
the canonical-form level; the remaining piece is the round-trip.

## Caveat Form Amendment

The following is normative for any `Caveat` enum whose canonical form is
emitted by `Caveat::canonical_ser`.

1. **Round-trip MUST hold.** The canonical hex form emitted by
   `Caveat::canonical_ser` for the `AmountMax` arm, the `Payment.budget` arm,
   and the `Vault` arm MUST be accepted by the same `Caveat` enum's
   `Deserialize` impl. The asymmetry — emitter produces hex, input form accepts
   only a byte-array — is a conformance drift; the corrected form is the
   canonical-hex form, not the byte-array form.

2. **Substrate-owned serialization paths are normative.**
   - `#[serde(with = "dqa_serde::field")]` is the substrate-owned serialization
     path for the 16-byte `DqaEncoding` payload carried by `AmountMax` and
     `Payment.budget`.
   - `#[serde(with = "hex_id_32")]` is the substrate-owned serialization path
     for any 32-byte id-bearing field. The first field to adopt the adapter is
     `Caveat::Vault([u8; 32])`. Any future caveat payload that is 32-byte
     id-bearing inherits the hex form.

3. **`Caveat::Vault` accepts both forms on input.**
   - Legacy 32-element array form: preserved for migration. A serialized
     `{ "type": "vault", "value": [u8; 32] }` envelope continues to parse.
   - Canonical 64-hex form: preferred. A serialized
     `{ "type": "vault", "value": "<64-char lowercase hex>" }` envelope parses
     to the same value.

4. **Default-feature stability for Layer B newtypes.** `AssetId`, `ChainId`,
   and `VaultId` keep their derived 32-element byte-array `Serialize` form
   under default features. The hex form is gated behind the `hex-ids` feature
   flag (off by default) so any external consumer that already pins the
   byte-array form is not broken by this amendment.

## Caveat::Vault hex form

The `hex_id_32` adapter's `deserialize_any` semantics are the substrate-owned
path for accepting both forms transparently. The adapter accepts:

- A 64-char lowercase hex string (canonical, preferred), via `visit_str`.
- A 32-element byte array (legacy, preserved for migration), via `visit_seq`.

A `deserialize_str` / `deserialize_seq` dispatch would force the consumer to
commit to one form via a tag; the cleaner substrate-owned solution is to
accept both forms transparently. The migration-window contract is that the
adapter's `deserialize_any` is the documented public surface of the path.

## Compatibility

1. **`CaveatSummaryView` wire-format break.** The operator-visible
   `CaveatSummaryView { kind, body }` projects the canonical form using
   pre-amendment field names. This amendment renames the wire form to
   `{ type, value }` to mirror the canonical `Caveat` envelope. The struct's
   Rust field identifiers (`kind`, `body`) are unchanged — only the
   `#[serde(rename)]` wire form changes.

   Any external consumer of `CaveatSummaryView` that reads
   `{ kind, body }` MUST migrate to `{ type, value }`. The known in-repo
   consumers are the operator-guide `jq` filters and the test fixtures that
   construct `CaveatSummaryView` literals in JSON; both are updated in
   Phase 3. Any external Rust consumer that destructures the struct is
   unaffected (struct field identifiers unchanged).

2. **`Caveat::Vault` input-form acceptance is a positive change.** Consumers
   that currently pass the 32-element array form continue to parse; consumers
   that pass the 64-hex form now also parse. There is no consumer that breaks
   from the `hex_id_32` adapter landing.

3. **`hex-ids` feature flag is off by default.** Consumers that did not opt
   into the feature see no wire-format change for `AssetId`, `ChainId`,
   `VaultId`. Consumers that opt in accept the 64-hex form on those types.

## Acceptance Criteria

The amendment is promoted to `Accepted` once the following vectors all pass
on the substrate crate (`octo-cap-macaroon`):

| Vector | Property locked |
| ------ | --------------- |
| `tv_cf_01_dqa_hex_round_trip` | `dqa_serde::field::deserialize` accepts a 64-hex string and decodes to the same `Dqa` as the 16-byte byte-array form. |
| `tv_cf_02_dqa_hex_rejects_odd_length` | odd-length hex strings reject. |
| `tv_cf_03_dqa_hex_rejects_non_hex_chars` | non-hex characters reject. |
| `tv_cf_04_hex_id_32_round_trip_string` | `hex_id_32` adapter round-trips through a 64-char hex string. |
| `tv_cf_05_hex_id_32_accepts_legacy_array_form` | `hex_id_32` adapter parses the legacy 32-element array form. |
| `tv_cf_06_hex_id_32_rejects_short_hex` | short hex strings reject. |
| `tv_cf_07_canonical_amount_max_reparses_through_input_form` | `Caveat::AmountMax`'s `canonical_ser` output re-parses through `#[serde(with = "dqa_serde::field")]`. |
| `tv_cf_08_canonical_vault_reparses_through_input_form` | `Caveat::Vault`'s `canonical_ser` output re-parses through `#[serde(with = "hex_id_32")]`. |
| `tv_cf_09_legacy_vault_array_form_still_parses` | legacy 32-element array form on `Caveat::Vault` continues to parse. |
| `tv_cf_10_asset_id_hex_round_trip` | under `--features hex-ids`, `AssetId` round-trips through 64-hex. |
| `tv_cf_11_chain_id_hex_round_trip` | under `--features hex-ids`, `ChainId` round-trips through 64-hex. |
| `tv_cf_12_vault_id_hex_round_trip` | under `--features hex-ids`, `VaultId` round-trips through 64-hex. |
| `tv_cf_13_default_newtype_serde_is_byte_array` | under default features, `AssetId` / `ChainId` / `VaultId` serialize as 32-element byte arrays. |
| `tv_cf_14_caveat_summary_view_serialises_as_type_value` | `CaveatSummaryView` serialises as `{ type, value }`, not `{ kind, body }`. |

Inverted guide vector:

- `guide_canonical_form_is_reparseable` — pre-amendment this was
  `guide_canonical_form_is_not_reparseable` and pinned the asymmetry. After
  this amendment, the canonical form MUST round-trip through the input form.
  The vector's doc comment records the inversion; the pre-amendment name is
  removed.

## References

- RFC-0011 §Caveat Catalog — defines the canonical envelope `{ type, value }`
  for every caveat arm, including the 16-byte `DqaEncoding` hex form for
  `AmountMax` and the 32-byte id form for `Vault`.
- RFC-0011 §Hex32 newtype — defines the 32-byte hex newtype for the
  operator-visible view; this amendment extends the same hex discipline to
  substrate-owned serialization paths via `hex_id_32`.