// Re-export of the canonical BearerCapsule from `quota-router-storage`.
// 0959-b places `BearerCapsule` near the delivery types that use it; the
// canonical type lives in storage per [[stoolap-general-purpose-db]] red line
// (cipherocto-side persistence in the storage crate).

pub use quota_router_storage::bearer_capsule_stub::BearerCapsule;
