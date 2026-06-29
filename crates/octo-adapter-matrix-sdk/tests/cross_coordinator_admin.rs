//! Cross-adapter CoordinatorAdmin smoke test (mission 0850h-d Phase 3).
//!
//! Verifies that:
//!   1. The matrix adapter opts into `CoordinatorAdmin` via the
//!      `as_coordinator_admin()` bridge on `PlatformAdapter`.
//!   2. The truthful capability report returned through the bridge
//!      matches the mission spec (19 true, 2 false).
//!   3. A bare `PlatformAdapter` stub (no CoordinatorAdmin impl)
//!      returns `None` from the default `as_coordinator_admin()`.
//!   4. Both adapters can be registered in a `DotGateway` via
//!      `add_adapter` (compile-time check on the trait surface).
//!
//! No live matrix.org session is required — the test uses
//! `MatrixAdapter::from_config_bytes` (in-memory access_token path).
//!
//! Run:
//! ```
//! cargo test --test cross_coordinator_admin -p octo-adapter-matrix-sdk
//! ```

use async_trait::async_trait;
use matrix_sdk::ruma::OwnedUserId;
use octo_adapter_matrix_sdk::MatrixAdapter;
use octo_network::dot::adapters::coordinator_admin::AdminCapabilityReport;
use octo_network::dot::adapters::{CapabilityReport, PlatformAdapter, RawPlatformMessage};
use octo_network::dot::domain::{BroadcastDomainId, PlatformType};
use octo_network::dot::gateway::{GatewayClass, GatewayIdentity};
use octo_network::dot::DotGateway;

/// Build a MatrixAdapter on a dedicated thread (the constructor
/// builds an internal tokio runtime that cannot be nested with the
/// test runtime).
fn build_matrix_adapter() -> MatrixAdapter {
    let cfg_json = serde_json::json!({
        "homeserver_url": "https://matrix.example.com",
        "user_id": format!(
            "@bot-cross-{}:matrix.example.com",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ),
        "device_id": "DEV_CROSS",
        "access_token": "syt_cross_test_token",
        "use_session_store": false,
        "config_path": "",
        "passphrase": null,
        "force_writeback": false,
        "session_store_path": "",
        "rooms": ["!placeholder:matrix.example.com"]
    });
    let bytes = serde_json::to_vec(&cfg_json).unwrap();
    std::thread::spawn(move || MatrixAdapter::from_config_bytes(&bytes))
        .join()
        .expect("adapter thread panicked")
        .expect("adapter construction (in-memory access_token)")
}

/// A bare `PlatformAdapter` stub that does NOT implement
/// `CoordinatorAdmin`. Used to verify that the default
/// `as_coordinator_admin()` returns `None` for non-admin adapters.
struct NonAdminStubAdapter;

#[async_trait]
impl PlatformAdapter for NonAdminStubAdapter {
    fn platform_type(&self) -> PlatformType {
        PlatformType::Matrix
    }
    fn self_handle(&self) -> Option<String> {
        None
    }
    async fn send_message(
        &self,
        _domain: &BroadcastDomainId,
        _envelope: &octo_network::dot::envelope::DeterministicEnvelope,
    ) -> Result<
        octo_network::dot::adapters::DeliveryReceipt,
        octo_network::dot::error::PlatformAdapterError,
    > {
        Err(
            octo_network::dot::error::PlatformAdapterError::Unimplemented {
                platform: "stub".into(),
                action: "send_message".into(),
            },
        )
    }
    async fn receive_messages(
        &self,
        _domain: &BroadcastDomainId,
    ) -> Result<Vec<RawPlatformMessage>, octo_network::dot::error::PlatformAdapterError> {
        Ok(Vec::new())
    }
    fn canonicalize(
        &self,
        _msg: &RawPlatformMessage,
    ) -> Result<
        octo_network::dot::envelope::DeterministicEnvelope,
        octo_network::dot::error::PlatformAdapterError,
    > {
        Err(
            octo_network::dot::error::PlatformAdapterError::Unimplemented {
                platform: "stub".into(),
                action: "canonicalize".into(),
            },
        )
    }
    fn capabilities(&self) -> CapabilityReport {
        CapabilityReport::default()
    }
    fn domain_id(&self, platform_id: &str) -> BroadcastDomainId {
        BroadcastDomainId {
            platform_type: PlatformType::Matrix as u16,
            domain_hash: *blake3::hash(platform_id.as_bytes()).as_bytes(),
        }
    }
}

/// Build a minimal `GatewayIdentity` for the test's `DotGateway`. The
/// constructor takes 4 args; we use deterministic zeroed values for
/// test repeatability.
fn test_gateway_identity() -> GatewayIdentity {
    GatewayIdentity::new([0u8; 32], 0, GatewayClass::Edge, 0)
}

/// Smoke test: matrix adapter + non-admin stub both register in a
/// `DotGateway`. Matrix returns `Some(self)` from
/// `as_coordinator_admin()`; the stub returns `None` (trait default).
#[test]
fn mx_cross_coord_admin_smoke() {
    // Build the matrix adapter on a separate thread (its internal
    // runtime cannot be nested with any other tokio runtime).
    let matrix = build_matrix_adapter();
    let stub = NonAdminStubAdapter;

    // Register both into a DotGateway. The trait-object insertion is
    // the cross-adapter smoke itself: it proves the trait surface
    // is uniform across both adapter types.
    let mut gateway = DotGateway::new(test_gateway_identity(), Default::default());
    gateway.add_adapter(Box::new(matrix));
    gateway.add_adapter(Box::new(stub));
    // `DotGateway.adapters` is private (no public iterator/count
    // accessor at the time of writing); the `add_adapter` calls above
    // are the compile-time assertion that the trait surface accepts
    // both adapter types. The runtime assertions below exercise each
    // adapter's `as_coordinator_admin()` bridge directly via a fresh
    // pair of adapters (we can't reach the moved ones from the gateway).

    // Cross-adapter invariant via `&dyn PlatformAdapter`:
    // - matrix -> as_coordinator_admin() = Some(self)
    // - stub   -> as_coordinator_admin() = None (trait default)
    let stub2 = NonAdminStubAdapter;
    let stub_ref: &dyn PlatformAdapter = &stub2;
    assert!(
        stub_ref.as_coordinator_admin().is_none(),
        "non-admin stub as_coordinator_admin must be None (default)"
    );

    // Build a fresh matrix adapter to exercise the bridge (the one
    // moved into the gateway is unreachable from outside).
    let matrix2 = build_matrix_adapter();
    let matrix_ref2: &dyn PlatformAdapter = &matrix2;
    let matrix_admin = matrix_ref2
        .as_coordinator_admin()
        .expect("matrix as_coordinator_admin must be Some");

    // Truthful capability check (matrix):
    //   - 19 true, 2 false (can_destroy, can_transfer_ownership).
    let caps: AdminCapabilityReport = matrix_admin.admin_capabilities();
    assert!(caps.can_create, "matrix must report can_create=true");
    assert!(caps.can_ban, "matrix must report can_ban=true");
    assert!(caps.can_promote, "matrix must report can_promote=true");
    assert!(!caps.can_destroy, "matrix has no destroy primitive");
    assert!(
        !caps.can_transfer_ownership,
        "matrix has no atomic transfer primitive"
    );
    let true_count = [
        caps.can_create,
        caps.can_join_by_id,
        caps.can_join_by_invite,
        caps.can_leave,
        caps.can_add_member,
        caps.can_remove_member,
        caps.can_ban,
        caps.can_promote,
        caps.can_demote,
        caps.can_approve_join,
        caps.can_rename,
        caps.can_describe,
        caps.can_lock,
        caps.can_announce,
        caps.can_set_ephemeral,
        caps.can_require_approval,
        caps.can_list_own_groups,
        caps.can_get_metadata,
        caps.can_resolve_invite,
    ]
    .iter()
    .filter(|b| **b)
    .count();
    assert_eq!(
        true_count, 19,
        "matrix must report exactly 19 true capability flags, got {true_count}"
    );

    // Platform name sanity.
    assert_eq!(matrix_admin.platform_name(), "matrix");

    // Sanity: the OwnedUserId import path is reachable (used by the
    // coordinator_admin methods that parse peer IDs).
    let _uid = OwnedUserId::try_from("@sanity:matrix.example.com").expect("valid matrix user id");
}

/// Verify the trait default: a bare `PlatformAdapter` without a
/// `CoordinatorAdmin` impl returns `None` from
/// `as_coordinator_admin()`. The `NonAdminStubAdapter` above is the
/// test surface for this.
#[test]
fn as_coordinator_admin_default_is_none_for_non_admin_adapter() {
    let stub = NonAdminStubAdapter;
    let stub_ref: &dyn PlatformAdapter = &stub;
    assert!(
        stub_ref.as_coordinator_admin().is_none(),
        "PlatformAdapter::as_coordinator_admin default must be None"
    );
}
