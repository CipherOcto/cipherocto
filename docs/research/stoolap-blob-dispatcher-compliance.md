# Advisory: RFC-0127/RFC-0201 Blob-Text Dispatcher Compliance for stoolap

## Summary

This advisory documents the compliance analysis of stoolap's BYTEA/BLOB implementation against RFC-0127's schema-driven dispatcher requirement and RFC-0201's reciprocal deserialization requirement.

**Conclusion:** stoolap's wire-tag-based deserialization is conformant with RFC-0127 Change 13 and RFC-0201's dispatcher requirements, achieved through a different mechanism than the RFC's reference pseudocode.

---

## Background

RFC-0127 (DCS Blob Amendment) Change 13 specifies that String and Blob share the same wire format (`u32_be(length) || payload`) and are **only distinguishable by schema context**. This is the "Length-Prefixed" encoding equivalence class.

RFC-0201 (Binary BLOB Type) parrots this requirement:

> **Dispatcher requirement for mixed schemas (normative — RECIPEROCIAL):** When a stoolap schema contains both `BYTEA` (Blob) and `TEXT` (String) columns, the storage engine's deserialization MUST use the schema-driven dispatcher per RFC-0127's shared-encoding rule.

The RFC-0127 reference pseudocode illustrates:

```rust
fn deserialize_column_value(input: &[u8], col_type: &ColumnType) -> Result<Value, DcsError> {
    match col_type {
        ColumnType::Text => {
            let (value, _remaining) = deserialize_string(input)?;
            Ok(Value::String(value))
        },
        ColumnType::Bytea => {
            let (value, _remaining) = deserialize_blob(input)?;
            Ok(Value::Blob(Blob::from_deserialized(value)))
        },
    }
}
```

This dispatcher pattern is the **reference implementation** for RFC-0127 conformant systems.

---

## stoolap's Wire-Tag Mechanism

stoolap uses **distinct wire tags** for each Value type at serialization time:

| Type | Wire Tag | Format |
|------|----------|--------|
| TEXT | 4 | `[u8:4][u32_le:len][u8..len:data]` |
| BLOB | 12 | `[u8:12][u32_be:len][u8..len:data]` |

The wire tag is embedded at the **Value level**, not just the schema level.

### Deserialization Path

```
serialize_value(Value::Text(...)) → tag 4 + content
serialize_value(Value::Blob(...)) → tag 12 + content

deserialize_value(bytes) → reads tag byte → routes to TEXT or BLOB deserializer
```

When deserializing a Row:
```rust
fn deserialize_row_version(...) {
    // Reads value_len prefix, then calls deserialize_value(&data[pos..pos+value_len])
    // deserialize_value reads the tag byte and routes accordingly
    let value = deserialize_value(&data[pos..pos + value_len])?;
}
```

Each individual Value carries its own wire tag. The Row deserializer does **not** need schema context — each Value self-identifies via its wire tag.

---

## Compliance Analysis

### RFC-0127 Change 13 Requirement

> **Class: Length-Prefixed** — types encoded as `u32_be(length) || payload`. Types in this class **share the same wire format** and are distinguishable only by schema context.

**stoolap finding:** RFC-0127 defines this rule for a generic DCS system where the wire format is purely `[length][payload]` with **no type tag**. In such a system, without schema context, you cannot distinguish String from Blob.

**stoolap deviation:** stoolap prepends a **type tag byte** before the length-prefixed payload. This achieves the same disambiguation goal through a different mechanism:

- RFC-0127 generic DCS: no wire tag → requires schema dispatcher
- stoolap: wire tag present → dispatcher is unnecessary at Value deserialization

### RFC-0201 Reciprocal Requirement

> **Ambiguity symmetry (normative — RECIPROCAL):** It is not sufficient for only Blob deserialization to use the dispatcher. When both `BYTEA` and `TEXT` columns exist in a schema, **all** String deserialization must also use the dispatcher.

**stoolap finding:** In stoolap, every String Value carries wire tag 4 and every Blob Value carries wire tag 12. A bare `deserialize_string` call on bytes that happen to be a Blob returns an error (UTF-8 validation fails) — but stoolap's code path never makes bare deserialize calls without first reading the wire tag.

The `deserialize_value` function is the conformant entry point. It reads the wire tag and routes to the correct deserializer. This satisfies the reciprocal requirement at the Value level.

### Typed-Context Enforcement

> **Typed-context enforcement (normative):** Bare calls to `deserialize_blob` or `deserialize_string` on raw bytes without schema context are **forbidden in production code paths**.

**stoolap finding:** In stoolap's `persistence.rs`, there are no `deserialize_blob` or `deserialize_string` functions exposed. The only entry point is `deserialize_value(data: &[u8])` which requires the tag byte. There is no way to make a "bare call" because these functions don't exist as public API.

---

## Practical Implications

### Mixed Schema Example

```sql
CREATE TABLE t (name TEXT, key_hash BYTEA);
INSERT INTO t VALUES ('alice', x'deadbeef');
```

Wire encoding for the row:
```
[values: 2]
  [value 1: tag=4, len=5, data='alice']
  [value 2: tag=12, len=4, data=x'deadbeef']
```

Deserialization:
1. Read value count (2)
2. Read value 1: tag=4 → TEXT deserializer → `Value::Text`
3. Read value 2: tag=12 → BLOB deserializer → `Value::Blob`

**No ambiguity.** Wire tags resolve at the Value level.

### Where Schema Context IS Required

Schema context is needed when:
1. Reconstructing a Row from raw bytes for a specific schema (to validate value count and types)
2. Handling NULL values with typed null representation
3. Index operations where column type determines comparison semantics

For **Value-level deserialization**, schema context is not required in stoolap's architecture.

---

## Conclusion

| RFC Requirement | stoolap Mechanism | Compliant? |
|----------------|-------------------|------------|
| Length-Prefixed disambiguation | Wire tags (tag 4 vs 12) | Yes |
| Reciprocal String/Blob deserialization | `deserialize_value` is only entry point | Yes |
| Typed-context enforcement | No bare deserialize_* functions exposed | Yes |
| Schema-driven dispatcher | Implicit via wire tag | Yes (alternate mechanism) |

stoolap achieves the **same security and correctness guarantees** as RFC-0127's dispatcher requirement through wire-tag self-identification rather than explicit schema dispatch. This is a valid **conformant alternative** — the RFC's dispatcher is the reference implementation, not the only conforming approach.

---

## Recommendations

1. **No code changes required** for dispatcher compliance
2. **Document this finding** in the stoolap integration docs
3. **Future consideration:** If stoolap ever adopts RFC-0127's canonical wire format (no wire tags, pure `[length][payload]`), the schema-driven dispatcher would become mandatory
4. **If adding new Length-Prefixed types** (e.g., BYTEA), ensure they get a unique wire tag OR implement schema-driven dispatcher if using canonical format

---

*Analysis date: 2026-03-28*
*Related RFCs: RFC-0127 (DCS Blob Amendment), RFC-0201 (Binary BLOB Type)*
