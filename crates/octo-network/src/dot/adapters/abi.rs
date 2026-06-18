//! Plugin ABI types for dynamically loaded platform adapters (RFC-0850 §8.3)

/// Current ABI version. Increment when the `PlatformAdapter` trait adds methods.
/// Old plugins (lower version) still load but report `UnsupportedOperation` for new methods.
pub const ADAPTER_ABI_VERSION: u32 = 1;

/// Function signature for `adapter_version()` exported by cdylib adapters.
pub type AdapterVersionFn = unsafe extern "C" fn() -> u32;

/// Function signature for `platform_type()` exported by cdylib adapters.
pub type PlatformTypeFn = unsafe extern "C" fn() -> u16;

/// Function signature for `create_adapter()` exported by cdylib adapters.
/// Returns an opaque pointer to a boxed `PlatformAdapter` impl.
/// The caller owns the pointer and must drop it via `destroy_adapter`.
pub type CreateAdapterFn = unsafe extern "C" fn(config: *const u8, config_len: usize) -> *mut ();

/// Function signature for `destroy_adapter()` exported by cdylib adapters.
/// Takes ownership of the pointer and drops the adapter.
pub type DestroyAdapterFn = unsafe extern "C" fn(adapter: *mut ());

/// Handle to a loaded cdylib adapter plugin.
pub struct AdapterPlugin {
    /// ABI version reported by the plugin
    pub abi_version: u32,
    /// Platform type this adapter handles
    pub platform_type: u16,
    /// Opaque pointer to the adapter instance
    pub instance: *mut (),
    /// Function to destroy the adapter instance
    pub destroy_fn: DestroyAdapterFn,
    /// Keep the library loaded
    pub _library: libloading::Library,
}

impl AdapterPlugin {
    /// Get the raw adapter pointer for use with `PlatformAdapter`.
    pub fn adapter_ptr(&self) -> *mut () {
        self.instance
    }
}

impl Drop for AdapterPlugin {
    fn drop(&mut self) {
        if !self.instance.is_null() {
            unsafe {
                (self.destroy_fn)(self.instance);
            }
        }
    }
}

// SAFETY: The loaded adapter is expected to implement Send + Sync via the PlatformAdapter trait.
// Plugin authors MUST ensure their adapter instances are thread-safe. The opaque pointer
// (`*mut ()`) is assumed to point to a heap-allocated object that can be safely shared across
// threads. If a plugin's adapter is not thread-safe, it must use internal synchronization
// (e.g., Mutex, RwLock) to satisfy this contract. Violating this invariant causes undefined
// behavior when the host accesses the adapter from multiple threads.
unsafe impl Send for AdapterPlugin {}
unsafe impl Sync for AdapterPlugin {}
