# Mission: DOT LoRa Adapter

## Status

Implemented (16 tests, serial bridge, 256B limit, fragmentation)

## RFC

RFC-0850: Deterministic Overlay Transport (DOT) — §8.1

## Summary

Implement a LoRa (Long Range) adapter for ultra-long-range, low-bandwidth communication. LoRa can reach 10-15km line-of-sight — useful for rural, maritime, and satellite-free scenarios.

## Acceptance Criteria

- [ ] `crates/octo-adapter-lora/` manages LoRa hardware via serial/bridge
- [ ] Implements `PlatformAdapter` trait with all methods (6 required + 3 optional: replay_protection, health_check, shutdown)
- [ ] `send_envelope()` transmits envelope fragment via LoRa radio
- [ ] `receive_messages()` listens for LoRa packets
- [ ] `CapabilityReport`: max_payload=256 (LoRa max), rate_limit=0.1/sec (duty cycle)
- [ ] `domain_id()`: `BroadcastDomainId(0x000C, BLAKE3(network_id))`
- [ ] Config: `serial_port`, `frequency`, `spreading_factor`, `network_id`
- [ ] Fragmentation: mandatory — all envelopes must be fragmented
- [ ] Duty cycle compliance: EU 1% duty cycle on 868MHz
- [ ] Unit tests with mock serial interface

## Location

`crates/octo-adapter-lora/`

## Complexity

High

## Prerequisites

- Mission 0850e: DOT Adapter Registry & Plugin ABI

## Implementation Notes

- LoRa hardware: SX1276/SX1262 modules connected via SPI/UART
- Bridge: use `lora_gateway` or custom Rust serial driver
- Payload: 51-256 bytes depending on spreading factor
- Duty cycle: EU regulations limit transmit time to 1% per hour per frequency
- Spreading factor: higher SF = longer range but lower bandwidth
- Frequency: 868MHz (EU), 915MHz (US), 433MHz (Asia)
- Mesh: LoRa mesh protocols (Meshtastic) can extend range via relay nodes
