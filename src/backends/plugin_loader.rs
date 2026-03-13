//! Dynamic backend plugin loader
//!
//! This module provides runtime loading of 3DGS backend implementations as
//! dynamic libraries (.so on Linux, .dylib on macOS, .dll on Windows).
//!
//! # Plugin Location
//!
//! Plugins are searched in the following locations (in order):
//! 1. `PLUGIN_PATH` environment variable directory
//! 2. `./plugins/` relative to executable
//! 3. `/usr/local/lib/3dgs-processor/plugins/`
//!
//! # Plugin Naming Convention
//!
//! Plugins must follow the naming pattern:
//! - Linux: `lib{backend}_backend.so`
//! - macOS: `lib{backend}_backend.dylib`
//! - Windows: `{backend}_backend.dll`
//!
//! Where `{backend}` is the backend identifier (e.g., "gsplat", "3dgs_cpp").
//!
//! # Plugin Interface
//!
//! Each plugin must export a C-ABI function named `create_backend` that
//! returns a boxed trait object implementing `GaussianSplatBackend`:
//!
//! ```ignore
//! #[no_mangle]
//! pub extern "C" fn create_backend() -> Box<dyn GaussianSplatBackend> {
//!     Box::new(MyBackend::new())
//! }
//! ```
//!
//! # Safety
//!
//! Plugin loading is inherently unsafe as it executes arbitrary code from
//! dynamic libraries. Only load plugins from trusted sources.

use anyhow::{Context, Result};
use libloading::{Library, Symbol};
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{debug, info, instrument, warn};

use super::GaussianSplatBackend;

/// Type alias for the plugin constructor function
///
/// Plugins must export a function with this signature named `create_backend`
///
/// # Safety
/// This is not FFI-safe but is the standard pattern for plugin systems.
/// The Box<dyn GaussianSplatBackend> is stable across the plugin boundary
/// because both the host and plugin use the same Rust ABI.
#[allow(improper_ctypes_definitions)]
type CreateBackendFn = unsafe extern "C" fn() -> Box<dyn GaussianSplatBackend>;

/// Plugin loader for dynamic backend libraries
///
/// Handles discovery, loading, and initialization of backend plugins.
pub struct PluginLoader {
    /// Search paths for plugin libraries
    plugin_paths: Vec<PathBuf>,

    /// Loaded libraries (kept alive to prevent unloading)
    #[allow(dead_code)]
    libraries: Vec<Library>,
}

impl PluginLoader {
    /// Create a new plugin loader with default search paths
    pub fn new() -> Self {
        let mut plugin_paths = Vec::new();

        // 1. Environment variable path (highest priority)
        if let Ok(plugin_path) = std::env::var("PLUGIN_PATH") {
            let path = PathBuf::from(plugin_path);
            if path.exists() {
                debug!(path = %path.display(), "Added plugin path from PLUGIN_PATH env var");
                plugin_paths.push(path);
            } else {
                warn!(path = %path.display(), "PLUGIN_PATH set but directory does not exist");
            }
        }

        // 2. Relative to executable
        if let Ok(exe_path) = std::env::current_exe() {
            if let Some(exe_dir) = exe_path.parent() {
                let plugins_dir = exe_dir.join("plugins");
                if plugins_dir.exists() {
                    debug!(path = %plugins_dir.display(), "Added plugin path relative to executable");
                    plugin_paths.push(plugins_dir);
                }
            }
        }

        // 3. System-wide installation path
        let system_path = PathBuf::from("/usr/local/lib/3dgs-processor/plugins");
        if system_path.exists() {
            debug!(path = %system_path.display(), "Added system-wide plugin path");
            plugin_paths.push(system_path);
        }

        // Always add current working directory ./plugins
        let local_plugins = PathBuf::from("./plugins");
        if local_plugins.exists() {
            debug!(path = %local_plugins.display(), "Added local ./plugins directory");
            plugin_paths.push(local_plugins);
        }

        if plugin_paths.is_empty() {
            warn!("No plugin search paths found - plugin loading may fail");
        } else {
            info!(
                count = plugin_paths.len(),
                "Initialized plugin search paths"
            );
        }

        PluginLoader {
            plugin_paths,
            libraries: Vec::new(),
        }
    }

    /// Create a plugin loader with custom search paths
    pub fn with_paths(paths: Vec<PathBuf>) -> Self {
        PluginLoader {
            plugin_paths: paths,
            libraries: Vec::new(),
        }
    }

    /// Get the platform-specific library filename for a backend
    ///
    /// # Examples
    ///
    /// - Linux: `gsplat` -> `libgsplat_backend.so`
    /// - macOS: `gsplat` -> `libgsplat_backend.dylib`
    /// - Windows: `gsplat` -> `gsplat_backend.dll`
    fn get_library_name(backend_name: &str) -> String {
        #[cfg(target_os = "linux")]
        return format!("lib{}_backend.so", backend_name);

        #[cfg(target_os = "macos")]
        return format!("lib{}_backend.dylib", backend_name);

        #[cfg(target_os = "windows")]
        return format!("{}_backend.dll", backend_name);

        #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
        compile_error!("Unsupported platform for plugin loading");
    }

    /// Find a plugin library in the search paths
    fn find_plugin(&self, backend_name: &str) -> Option<PathBuf> {
        let lib_name = Self::get_library_name(backend_name);

        for search_path in &self.plugin_paths {
            let plugin_path = search_path.join(&lib_name);
            if plugin_path.exists() {
                debug!(
                    backend = backend_name,
                    path = %plugin_path.display(),
                    "Found plugin library"
                );
                return Some(plugin_path);
            }
        }

        None
    }

    /// Load a backend plugin by name
    ///
    /// # Arguments
    ///
    /// * `backend_name` - Backend identifier (e.g., "gsplat", "3dgs_cpp")
    ///
    /// # Returns
    ///
    /// An arc-wrapped trait object implementing `GaussianSplatBackend`
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Plugin library not found in search paths
    /// - Library failed to load (linking errors, missing dependencies)
    /// - Plugin does not export `create_backend` function
    /// - Plugin initialization failed
    ///
    /// # Safety
    ///
    /// This function loads and executes code from a dynamic library, which is
    /// inherently unsafe. Only load plugins from trusted sources.
    #[instrument(skip(self), fields(backend = backend_name))]
    pub fn load_backend(&mut self, backend_name: &str) -> Result<Arc<dyn GaussianSplatBackend>> {
        info!(backend = backend_name, "Loading backend plugin");

        // Find the plugin library
        let plugin_path = self.find_plugin(backend_name).ok_or_else(|| {
            let lib_name = Self::get_library_name(backend_name);
            anyhow::anyhow!(
                "Backend plugin '{}' not found. Searched for '{}' in: {:?}",
                backend_name,
                lib_name,
                self.plugin_paths
            )
        })?;

        // Load the dynamic library
        let library = unsafe {
            Library::new(&plugin_path).with_context(|| {
                format!("Failed to load plugin library: {}", plugin_path.display())
            })?
        };

        debug!(path = %plugin_path.display(), "Loaded plugin library");

        // Get the constructor function
        let create_backend: Symbol<CreateBackendFn> = unsafe {
            library.get(b"create_backend").with_context(|| {
                format!(
                    "Plugin '{}' does not export 'create_backend' function",
                    plugin_path.display()
                )
            })?
        };

        // Call the constructor to create the backend instance
        let backend = unsafe { create_backend() };

        info!(
            backend = backend_name,
            path = %plugin_path.display(),
            "Successfully loaded and initialized backend plugin"
        );

        // Keep library alive to prevent unloading
        self.libraries.push(library);

        Ok(Arc::from(backend))
    }

    /// List all available plugins in search paths
    pub fn list_available_plugins(&self) -> Vec<String> {
        let mut plugins = Vec::new();

        for search_path in &self.plugin_paths {
            if let Ok(entries) = std::fs::read_dir(search_path) {
                for entry in entries.flatten() {
                    if let Some(filename) = entry.file_name().to_str() {
                        // Extract backend name from library filename
                        if let Some(backend_name) = Self::extract_backend_name(filename) {
                            if !plugins.contains(&backend_name) {
                                plugins.push(backend_name);
                            }
                        }
                    }
                }
            }
        }

        plugins.sort();
        plugins
    }

    /// Extract backend name from library filename
    ///
    /// Examples:
    /// - `libgsplat_backend.so` -> Some("gsplat")
    /// - `lib3dgs_cpp_backend.dylib` -> Some("3dgs_cpp")
    /// - `random_file.txt` -> None
    fn extract_backend_name(filename: &str) -> Option<String> {
        // Linux/macOS: lib{name}_backend.so/dylib
        if let Some(stripped) = filename.strip_prefix("lib") {
            if let Some(name) = stripped.strip_suffix("_backend.so") {
                return Some(name.to_string());
            }
            if let Some(name) = stripped.strip_suffix("_backend.dylib") {
                return Some(name.to_string());
            }
        }

        // Windows: {name}_backend.dll
        if let Some(name) = filename.strip_suffix("_backend.dll") {
            return Some(name.to_string());
        }

        None
    }
}

impl Default for PluginLoader {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_library_name_format() {
        #[cfg(target_os = "linux")]
        assert_eq!(
            PluginLoader::get_library_name("gsplat"),
            "libgsplat_backend.so"
        );

        #[cfg(target_os = "macos")]
        assert_eq!(
            PluginLoader::get_library_name("gsplat"),
            "libgsplat_backend.dylib"
        );

        #[cfg(target_os = "windows")]
        assert_eq!(
            PluginLoader::get_library_name("gsplat"),
            "gsplat_backend.dll"
        );
    }

    #[test]
    fn test_extract_backend_name() {
        assert_eq!(
            PluginLoader::extract_backend_name("libgsplat_backend.so"),
            Some("gsplat".to_string())
        );
        assert_eq!(
            PluginLoader::extract_backend_name("libgsplat_backend.dylib"),
            Some("gsplat".to_string())
        );
        assert_eq!(
            PluginLoader::extract_backend_name("gsplat_backend.dll"),
            Some("gsplat".to_string())
        );
        assert_eq!(
            PluginLoader::extract_backend_name("lib3dgs_cpp_backend.so"),
            Some("3dgs_cpp".to_string())
        );
        assert_eq!(PluginLoader::extract_backend_name("random_file.txt"), None);
        assert_eq!(PluginLoader::extract_backend_name("libwrong.so"), None);
    }

    #[test]
    fn test_plugin_loader_creation() {
        let loader = PluginLoader::new();
        assert!(!loader.plugin_paths.is_empty() || std::env::var("PLUGIN_PATH").is_err());
    }

    #[test]
    fn test_plugin_loader_with_custom_paths() {
        let paths = vec![PathBuf::from("/custom/path")];
        let loader = PluginLoader::with_paths(paths.clone());
        assert_eq!(loader.plugin_paths, paths);
    }
}
