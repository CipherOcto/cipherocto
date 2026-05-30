# Mission: DOT Bluetooth Adapter

## Status

Implemented (11 tests, BLE bridge, DOT/1/ encoding)

## RFC

RFC-0850: Deterministic Overlay Transport (DOT) — §8.1

## Summary

Implement a Bluetooth Low Energy (BLE) mesh adapter using a bridge daemon. Enables local mesh networking without internet — useful for disaster recovery, protests, and air-gapped environments.

## Acceptance Criteria

- [ ] `crates/octo-adapter-bluetooth/` manages BLE daemon subprocess
- [ ] Implements `PlatformAdapter` trait with all methods (6 required + 3 optional: replay_protection, health_check, shutdown)
- [ ] `send_envelope()` broadcasts BLE advertisement with envelope fragment
- [ ] `receive_messages()` listens for BLE advertisements from peers
- [ ] `CapabilityReport`: max_payload=244 (BLE advertisement limit), rate_limit=1/sec
- [ ] `domain_id()`: `BroadcastDomainId(0x000B, BLAKE3(mesh_network_id))`
- [ ] Config: `adapter_name`, `mesh_network_id`, `scan_interval_ms`
- [ ] Fragmentation: multi-advertisement reassembly for larger envelopes
- [ ] Unit tests with mock BLE stack

## Location

`crates/octo-adapter-bluetooth/`

## Complexity

High

## Prerequisites

- Mission 0850e: DOT Adapter Registry & Plugin ABI

## Implementation Notes

- BLE advertisements: 244 bytes max payload (31 bytes legacy, 244 bytes extended)
- Bridge daemon: separate process managing BLE hardware (e.g., `bluetoothctl` or custom daemon)
- Mesh networking: BLE mesh (Bluetooth SIG Mesh) or custom flood-fill protocol
- Range: ~100m outdoor, ~30m indoor — useful for local coordination only
- Power: BLE is low-power, suitable for battery-operated devices
- Platform: Linux BlueZ API, macOS CoreBluetooth, Windows BLE API
