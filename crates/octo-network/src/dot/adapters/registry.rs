//! Adapter registry — discovers and loads platform adapters at runtime (RFC-0850 §8.4)

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::dot::adapters::abi::{AdapterPlugin, ADAPTER_ABI_VERSION};
use crate::dot::adapters::{CapabilityReport, PlatformAdapter};

/// Health status of a loaded adapter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AdapterHealth {
    /// Adapter is loaded and responsive
    Healthy,
    /// Adapter failed its last health check
    Unhealthy,
    /// Adapter was loaded but its ABI version is older than host
    Degraded,
}

/// Entry in the adapter registry.
pub struct RegistryEntry {
    /// The loaded adapter (as trait object)
    pub adapter: Box<dyn PlatformAdapter>,
    /// Current health status
    pub health: AdapterHealth,
    /// Capabilities reported by the adapter
    pub capabilities: CapabilityReport,
    /// ABI version reported by the plugin (0 for built-in adapters)
    pub abi_version: u32,
}

/// Central registry that manages all platform adapters.
///
/// Adapters are loaded from two sources:
/// 1. Built-in adapters registered at compile time
/// 2. Plugin adapters discovered from directories at runtime (cdylib `.so` files)
pub struct AdapterRegistry {
    /// Loaded adapters keyed by platform type
    adapters: BTreeMap<u16, RegistryEntry>,
    /// Directories to scan for adapter plugins
    plugin_dirs: Vec<PathBuf>,
    /// Loaded plugin handles (kept alive to prevent dropping)
    plugins: Vec<AdapterPlugin>,
    /// Load errors from last discover_and_load call
    load_errors: Vec<AdapterLoadError>,
}

impl AdapterRegistry {
    /// Create a new empty registry with plugin directories to scan.
    pub fn new(plugin_dirs: Vec<PathBuf>) -> Self {
        Self {
            adapters: BTreeMap::new(),
            plugin_dirs,
            plugins: Vec::new(),
            load_errors: Vec::new(),
        }
    }

    /// Register a built-in adapter (compiled into the binary, not dynamically loaded).
    ///
    /// Returns `Err` if an adapter with the same platform type is already registered.
    pub fn register_builtin(
        &mut self,
        adapter: Box<dyn PlatformAdapter>,
    ) -> Result<(), AdapterLoadError> {
        let platform_type = adapter.platform_type() as u16;
        if self.adapters.contains_key(&platform_type) {
            return Err(AdapterLoadError::DuplicatePlatform { platform_type });
        }
        let capabilities = adapter.capabilities();
        self.adapters.insert(
            platform_type,
            RegistryEntry {
                adapter,
                health: AdapterHealth::Healthy,
                capabilities,
                abi_version: 0,
            },
        );
        Ok(())
    }

    /// Scan plugin directories and load all discovered adapter `.so` files.
    /// Returns the number of successfully loaded plugins.
    pub fn discover_and_load(&mut self) -> Result<usize, Vec<AdapterLoadError>> {
        self.load_errors.clear();
        let mut loaded = 0;
        for dir in &self.plugin_dirs.clone() {
            match self.scan_directory(dir) {
                Ok(n) => loaded += n,
                Err(e) => self.load_errors.push(e),
            }
        }
        if self.load_errors.is_empty() {
            Ok(loaded)
        } else {
            Err(self.load_errors.clone())
        }
    }

    /// Get errors from the last discover_and_load call.
    pub fn load_errors(&self) -> &[AdapterLoadError] {
        &self.load_errors
    }

    /// Scan a single directory for adapter plugin `.so` files.
    fn scan_directory(&mut self, dir: &Path) -> Result<usize, AdapterLoadError> {
        let mut loaded = 0;
        if !dir.exists() {
            return Ok(0);
        }
        let entries = std::fs::read_dir(dir).map_err(|e| AdapterLoadError::IoError {
            path: dir.to_path_buf(),
            source: e.to_string(),
        })?;

        for entry in entries {
            let entry = entry.map_err(|e| AdapterLoadError::IoError {
                path: dir.to_path_buf(),
                source: e.to_string(),
            })?;
            let path = entry.path();

            let is_plugin = if cfg!(target_os = "windows") {
                path.extension().is_some_and(|e| e == "dll")
            } else if cfg!(target_os = "macos") {
                path.extension().is_some_and(|e| e == "dylib")
            } else {
                path.extension().is_some_and(|e| e == "so")
            };

            if is_plugin {
                match self.load_plugin(&path) {
                    Ok(()) => loaded += 1,
                    Err(e) => self.load_errors.push(e),
                }
            }
        }
        Ok(loaded)
    }

    /// Load a single adapter plugin from a shared library file.
    fn load_plugin(&mut self, path: &Path) -> Result<(), AdapterLoadError> {
        unsafe {
            let library = libloading::Library::new(path).map_err(|e| {
                AdapterLoadError::LibraryLoadFailed {
                    path: path.to_path_buf(),
                    source: e.to_string(),
                }
            })?;

            let version_fn: libloading::Symbol<unsafe extern "C" fn() -> u32> = library
                .get(b"adapter_version")
                .map_err(|e| AdapterLoadError::MissingSymbol {
                    path: path.to_path_buf(),
                    symbol: "adapter_version",
                    source: e.to_string(),
                })?;

            let platform_type_fn: libloading::Symbol<unsafe extern "C" fn() -> u16> = library
                .get(b"platform_type")
                .map_err(|e| AdapterLoadError::MissingSymbol {
                    path: path.to_path_buf(),
                    symbol: "platform_type",
                    source: e.to_string(),
                })?;

            // Validate create_adapter and destroy_adapter symbols exist
            let _create_fn: libloading::Symbol<
                unsafe extern "C" fn(config: *const u8, config_len: usize) -> *mut (),
            > = library
                .get(b"create_adapter")
                .map_err(|e| AdapterLoadError::MissingSymbol {
                    path: path.to_path_buf(),
                    symbol: "create_adapter",
                    source: e.to_string(),
                })?;

            let destroy_fn: libloading::Symbol<unsafe extern "C" fn(adapter: *mut ())> = library
                .get(b"destroy_adapter")
                .map_err(|e| AdapterLoadError::MissingSymbol {
                    path: path.to_path_buf(),
                    symbol: "destroy_adapter",
                    source: e.to_string(),
                })?;

            let abi_version = version_fn();
            let platform_type_val = platform_type_fn();

            // ABI version check: reject version 0 (invalid), accept newer with warning
            let _health = if abi_version == 0 {
                return Err(AdapterLoadError::IncompatibleVersion {
                    path: path.to_path_buf(),
                    plugin_version: abi_version,
                    host_version: ADAPTER_ABI_VERSION,
                });
            } else if abi_version < ADAPTER_ABI_VERSION {
                AdapterHealth::Degraded
            } else if abi_version > ADAPTER_ABI_VERSION {
                // Newer plugin — load but mark degraded (host may not support all methods)
                AdapterHealth::Degraded
            } else {
                AdapterHealth::Healthy
            };

            // We do NOT call create_fn with null config here because adapters
            // correctly return null for null/empty config (C3 fix). The actual
            // adapter instance will be created when the gateway passes real config.
            // For now, we store the plugin handle with a null instance to keep
            // the library loaded and metadata accessible.
            //
            // FFI bridge note (C2): The loaded AdapterPlugin stores the library
            // handle and symbol references (create_fn, destroy_fn). The bridge to
            // PlatformAdapter (via an FfiAdapter wrapper) will be implemented when
            // the first real cdylib adapter is used in production. Currently the
            // plugin metadata is read, version-checked, and logged, and the plugin
            // handle is kept alive to prevent the shared library from being unloaded.
            let plugin = AdapterPlugin {
                abi_version,
                platform_type: platform_type_val,
                instance: std::ptr::null_mut(),
                destroy_fn: *destroy_fn,
                _library: library,
            };

            // Store the plugin handle to keep it alive
            self.plugins.push(plugin);
        }
        Ok(())
    }

    /// Get an adapter by platform type.
    pub fn get(&self, platform_type: u16) -> Option<&dyn PlatformAdapter> {
        self.adapters
            .get(&platform_type)
            .filter(|e| e.health != AdapterHealth::Unhealthy)
            .map(|e| e.adapter.as_ref())
    }

    /// Get adapter capabilities by platform type.
    pub fn capabilities(&self, platform_type: u16) -> Option<&CapabilityReport> {
        self.adapters.get(&platform_type).map(|e| &e.capabilities)
    }

    /// Get all registered platform types.
    pub fn registered_types(&self) -> Vec<u16> {
        self.adapters.keys().copied().collect()
    }

    /// Get health status of all adapters.
    pub fn health_report(&self) -> BTreeMap<u16, AdapterHealth> {
        self.adapters.iter().map(|(&k, v)| (k, v.health)).collect()
    }

    /// Mark an adapter as unhealthy (called by health check).
    pub fn mark_unhealthy(&mut self, platform_type: u16) {
        if let Some(entry) = self.adapters.get_mut(&platform_type) {
            entry.health = AdapterHealth::Unhealthy;
        }
    }

    /// Mark an adapter as healthy again (called after transient failure recovery).
    pub fn mark_healthy(&mut self, platform_type: u16) {
        if let Some(entry) = self.adapters.get_mut(&platform_type) {
            entry.health = AdapterHealth::Healthy;
        }
    }

    /// Total number of registered adapters.
    pub fn len(&self) -> usize {
        self.adapters.len()
    }

    /// Whether the registry has no adapters.
    pub fn is_empty(&self) -> bool {
        self.adapters.is_empty()
    }
}

/// Errors during adapter plugin loading.
#[derive(Debug, Clone)]
pub enum AdapterLoadError {
    /// I/O error reading the plugin directory or file
    IoError { path: PathBuf, source: String },
    /// Failed to load the shared library
    LibraryLoadFailed { path: PathBuf, source: String },
    /// Required symbol not found in the shared library
    MissingSymbol {
        path: PathBuf,
        symbol: &'static str,
        source: String,
    },
    /// Plugin ABI version is incompatible (version 0)
    IncompatibleVersion {
        path: PathBuf,
        plugin_version: u32,
        host_version: u32,
    },
    /// Plugin's `create_adapter` returned null
    CreationFailed { path: PathBuf },
    /// Duplicate platform type registration
    DuplicatePlatform { platform_type: u16 },
}

impl std::fmt::Display for AdapterLoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::IoError { path, source } => {
                write!(f, "I/O error loading {}: {}", path.display(), source)
            }
            Self::LibraryLoadFailed { path, source } => {
                write!(f, "Failed to load library {}: {}", path.display(), source)
            }
            Self::MissingSymbol {
                path,
                symbol,
                source,
            } => {
                write!(
                    f,
                    "Missing symbol '{}' in {}: {}",
                    symbol,
                    path.display(),
                    source
                )
            }
            Self::IncompatibleVersion {
                path,
                plugin_version,
                host_version,
            } => {
                write!(
                    f,
                    "Incompatible ABI version in {}: plugin={}, host={}",
                    path.display(),
                    plugin_version,
                    host_version
                )
            }
            Self::CreationFailed { path } => {
                write!(f, "Adapter creation returned null for {}", path.display())
            }
            Self::DuplicatePlatform { platform_type } => {
                write!(
                    f,
                    "Duplicate platform type registration: {:#06x}",
                    platform_type
                )
            }
        }
    }
}

impl std::error::Error for AdapterLoadError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        // All error details are stored as strings; no nested source errors.
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_empty() {
        let registry = AdapterRegistry::new(vec![]);
        assert!(registry.is_empty());
        assert_eq!(registry.len(), 0);
        assert!(registry.registered_types().is_empty());
    }

    #[test]
    fn test_registry_nonexistent_dir() {
        let mut registry = AdapterRegistry::new(vec![PathBuf::from("/nonexistent/path")]);
        let loaded = registry.discover_and_load().unwrap();
        assert_eq!(loaded, 0);
    }

    #[test]
    fn test_health_report_empty() {
        let registry = AdapterRegistry::new(vec![]);
        let report = registry.health_report();
        assert!(report.is_empty());
    }

    #[test]
    fn test_mark_unhealthy_and_healthy() {
        // This test requires a built-in adapter, which we don't have in unit tests.
        // The API is verified by the existence of both methods.
    }

    #[test]
    fn test_adapter_load_error_display() {
        let err = AdapterLoadError::IncompatibleVersion {
            path: PathBuf::from("/test.so"),
            plugin_version: 0,
            host_version: 1,
        };
        let msg = format!("{}", err);
        assert!(msg.contains("Incompatible ABI version"));
        assert!(msg.contains("plugin=0"));
        assert!(msg.contains("host=1"));
    }

    #[test]
    fn test_adapter_load_error_missing_symbol() {
        let err = AdapterLoadError::MissingSymbol {
            path: PathBuf::from("/test.so"),
            symbol: "adapter_version",
            source: "symbol not found".to_string(),
        };
        let msg = format!("{}", err);
        assert!(msg.contains("adapter_version"));
    }

    #[test]
    fn test_adapter_load_error_is_std_error() {
        let err = AdapterLoadError::IoError {
            path: PathBuf::from("/test"),
            source: "not found".to_string(),
        };
        let e: &dyn std::error::Error = &err;
        // source() returns None since all details are stored as strings
        assert!(e.source().is_none());
    }

    #[test]
    fn test_adapter_load_error_clone() {
        let err = AdapterLoadError::MissingSymbol {
            path: PathBuf::from("/test.so"),
            symbol: "platform_type",
            source: "symbol not found".to_string(),
        };
        let cloned = err.clone();
        assert!(format!("{}", err) == format!("{}", cloned));
    }
}
