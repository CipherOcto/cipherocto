//! WASM Plugin Runtime for DOT adapters (RFC-0850 §8.3)
//!
//! Loads community-contributed platform adapters from `.wasm` files
//! with sandboxed execution via `wasmtime`.
//!
//! Feature gate: `wasm` — enable with `cargo build -p octo-network --features wasm`

use wasmtime::*;

/// Maximum WASM linear memory per adapter instance (16 MiB).
const MAX_WASM_MEMORY_BYTES: usize = 16 * 1024 * 1024;

/// Maximum execution time per WASM call (5 seconds).
const MAX_CALL_TIMEOUT_SECS: u64 = 5;

/// WASM adapter ABI version.
pub const WASM_ABI_VERSION: u32 = 1;

/// Resource limits for WASM execution.
#[derive(Debug, Clone)]
pub struct WasmResourceLimits {
    pub max_memory_bytes: usize,
    pub call_timeout: std::time::Duration,
    pub allowed_domains: Vec<String>,
}

impl Default for WasmResourceLimits {
    fn default() -> Self {
        Self {
            max_memory_bytes: MAX_WASM_MEMORY_BYTES,
            call_timeout: std::time::Duration::from_secs(MAX_CALL_TIMEOUT_SECS),
            allowed_domains: Vec::new(),
        }
    }
}

/// WASM plugin runtime that manages adapter instances.
pub struct WasmAdapterRuntime {
    engine: Engine,
    limits: WasmResourceLimits,
}

impl WasmAdapterRuntime {
    pub fn new() -> Result<Self, String> {
        Self::with_limits(WasmResourceLimits::default())
    }

    pub fn with_limits(limits: WasmResourceLimits) -> Result<Self, String> {
        let mut config = Config::new();
        config.wasm_multi_memory(false);
        config.max_wasm_stack(1 << 20);
        config.consume_fuel(true);
        let engine = Engine::new(&config).map_err(|e| format!("WASM engine: {e}"))?;
        Ok(Self { engine, limits })
    }

    pub fn engine(&self) -> &Engine {
        &self.engine
    }
    pub fn limits(&self) -> &WasmResourceLimits {
        &self.limits
    }

    pub fn load_module(&self, wasm_bytes: &[u8]) -> Result<Module, String> {
        Module::new(&self.engine, wasm_bytes).map_err(|e| format!("WASM module: {e}"))
    }

    pub fn load_module_from_path(&self, path: &std::path::Path) -> Result<Module, String> {
        Module::from_file(&self.engine, path).map_err(|e| format!("WASM file: {e}"))
    }

    pub fn create_store(&self) -> Result<Store<HostState>, String> {
        let mut store = Store::new(&self.engine, HostState::new(self.limits.clone()));
        store
            .set_fuel(1_000_000_000)
            .map_err(|e| format!("Fuel: {e}"))?;
        Ok(store)
    }

    pub fn create_linker(&self) -> Result<Linker<HostState>, String> {
        Ok(Linker::new(&self.engine))
    }

    /// Register standard host functions on a linker.
    pub fn register_host_functions(&self, linker: &mut Linker<HostState>) -> Result<(), String> {
        linker
            .func_wrap(
                "env",
                "http_request",
                |_caller: Caller<'_, HostState>, _ptr: i32, _len: i32| -> i32 { -1 },
            )
            .map_err(|e| format!("http_request: {e}"))?;

        linker
            .func_wrap(
                "env",
                "log",
                |_caller: Caller<'_, HostState>, _level: i32, _ptr: i32, _len: i32| {},
            )
            .map_err(|e| format!("log: {e}"))?;

        linker
            .func_wrap("env", "current_epoch", || -> i64 {
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs() as i64
            })
            .map_err(|e| format!("current_epoch: {e}"))?;

        Ok(())
    }
}

/// Internal host state for WASM store.
#[derive(Debug)]
pub struct HostState {
    pub limits: WasmResourceLimits,
}

impl HostState {
    fn new(limits: WasmResourceLimits) -> Self {
        Self { limits }
    }
}

/// WASM adapter module metadata extracted from exports.
#[derive(Debug, Clone)]
pub struct WasmAdapterMeta {
    pub abi_version: u32,
    pub platform_type: u16,
}

impl WasmAdapterMeta {
    pub fn from_instance(
        store: &mut Store<HostState>,
        instance: &Instance,
    ) -> Result<Self, String> {
        let version_fn = instance
            .get_typed_func::<(), i32>(&mut *store, "adapter_version")
            .map_err(|e| format!("adapter_version: {e}"))?;
        let version = version_fn
            .call(&mut *store, ())
            .map_err(|e| format!("adapter_version call: {e}"))?;

        let ptype_fn = instance
            .get_typed_func::<(), i32>(&mut *store, "platform_type")
            .map_err(|e| format!("platform_type: {e}"))?;
        let ptype = ptype_fn
            .call(&mut *store, ())
            .map_err(|e| format!("platform_type call: {e}"))?;

        Ok(Self {
            abi_version: version as u32,
            platform_type: ptype as u16,
        })
    }

    pub fn verify_abi(&self) -> Result<(), String> {
        if self.abi_version != WASM_ABI_VERSION {
            return Err(format!(
                "ABI version mismatch: expected {}, got {}",
                WASM_ABI_VERSION, self.abi_version
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_runtime_creation() {
        assert!(WasmAdapterRuntime::new().is_ok());
    }
    #[test]
    fn test_runtime_custom_limits() {
        let limits = WasmResourceLimits {
            max_memory_bytes: 8 * 1024 * 1024,
            call_timeout: std::time::Duration::from_secs(10),
            allowed_domains: vec!["example.com".into()],
        };
        assert!(WasmAdapterRuntime::with_limits(limits).is_ok());
    }
    #[test]
    fn test_limits_default() {
        let l = WasmResourceLimits::default();
        assert_eq!(l.max_memory_bytes, 16 * 1024 * 1024);
        assert_eq!(l.call_timeout, std::time::Duration::from_secs(5));
        assert!(l.allowed_domains.is_empty());
    }
    #[test]
    fn test_abi_version() {
        assert_eq!(WASM_ABI_VERSION, 1);
    }
    #[test]
    fn test_max_memory() {
        assert_eq!(MAX_WASM_MEMORY_BYTES, 16 * 1024 * 1024);
    }
    #[test]
    fn test_load_invalid_module() {
        assert!(WasmAdapterRuntime::new()
            .unwrap()
            .load_module(b"not wasm")
            .is_err());
    }
    #[test]
    fn test_create_store() {
        assert!(WasmAdapterRuntime::new().unwrap().create_store().is_ok());
    }
    #[test]
    fn test_create_linker() {
        assert!(WasmAdapterRuntime::new().unwrap().create_linker().is_ok());
    }
    #[test]
    fn test_meta_verify_abi_ok() {
        assert!(WasmAdapterMeta {
            abi_version: 1,
            platform_type: 0x0009
        }
        .verify_abi()
        .is_ok());
    }
    #[test]
    fn test_meta_verify_abi_fail() {
        assert!(WasmAdapterMeta {
            abi_version: 99,
            platform_type: 0x0009
        }
        .verify_abi()
        .is_err());
    }
}
