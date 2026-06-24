use std::sync::Arc;

use octo_network::dot::adapters::registry::AdapterRegistry;
use octo_network::dot::BroadcastDomainId;

use crate::adapter_bridge::PlatformAdapterBridge;
use crate::sender::NetworkSender;

/// Factory that creates `NetworkSender`s from the platform adapter registry.
///
/// Iterates registered adapters, wraps each one in a `PlatformAdapterBridge`,
/// and returns a list of general-purpose senders.
pub struct AdapterFactory;

impl AdapterFactory {
    /// Create `NetworkSender`s from all **healthy** adapters in the registry.
    ///
    /// Consumes the registry (via `drain`) since `dyn PlatformAdapter`
    /// cannot be cloned. Filters out unhealthy adapters. Each adapter is
    /// wrapped in a `PlatformAdapterBridge` with the given default domain.
    pub fn from_registry(
        mut registry: AdapterRegistry,
        default_domain: BroadcastDomainId,
    ) -> Vec<Arc<dyn NetworkSender>> {
        registry
            .drain()
            .into_iter()
            .filter(|(_, entry)| {
                entry.health != octo_network::dot::adapters::registry::AdapterHealth::Unhealthy
            })
            .map(|(_platform_type, entry)| {
                let adapter: Arc<dyn octo_network::dot::adapters::PlatformAdapter> =
                    Arc::from(entry.adapter);
                let bridge = PlatformAdapterBridge::new(adapter, default_domain);
                Arc::new(bridge) as Arc<dyn NetworkSender>
            })
            .collect()
    }
}
