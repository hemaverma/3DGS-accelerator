//! Backend registration and selection system
//!
//! This module provides a centralized registry for 3DGS backend implementations,
//! supporting both built-in backends and dynamically loaded plugins.
//!
//! # Registry Architecture
//!
//! The registry maintains a mapping of backend names to factory functions.
//! Backends can be registered in two ways:
//!
//! 1. **Built-in backends**: Statically compiled into the binary
//! 2. **Plugin backends**: Dynamically loaded from shared libraries
//!
//! # Backend Selection
//!
//! Backend selection follows this priority order:
//! 1. `BACKEND` environment variable (highest priority)
//! 2. `backend` field in YAML configuration
//! 3. Default backend ("gaussian-splatting")
//!
//! # Usage Example
//!
//! ```no_run
//! use three_dgs_processor::backends::BackendRegistry;
//!
//! # async fn example() -> anyhow::Result<()> {
//! let mut registry = BackendRegistry::new();
//! registry.register_defaults();
//!
//! // Load backend by name
//! let backend = registry.get_backend("gsplat").await?;
//! # Ok(())
//! # }
//! ```

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, info, instrument, warn};

use super::gpu_detect::{detect_gpu, gpu_status_string};
use super::plugin_loader::PluginLoader;
use super::{
    GaussianSplatBackend, GaussianSplattingBackend, GsplatBackend, MockBackend, ThreeDGSCppBackend,
};

/// Factory function type for creating backend instances
type BackendFactory = Box<dyn Fn() -> Arc<dyn GaussianSplatBackend> + Send + Sync>;

/// Central registry for backend implementations
///
/// Manages both built-in and plugin-based backends, providing unified
/// access to all available implementations.
pub struct BackendRegistry {
    /// Registered backend factories
    factories: HashMap<String, BackendFactory>,

    /// Plugin loader for dynamic backends
    plugin_loader: PluginLoader,

    /// Cache of loaded plugin backends
    plugin_cache: HashMap<String, Arc<dyn GaussianSplatBackend>>,
}

impl BackendRegistry {
    /// Create a new empty backend registry
    pub fn new() -> Self {
        BackendRegistry {
            factories: HashMap::new(),
            plugin_loader: PluginLoader::new(),
            plugin_cache: HashMap::new(),
        }
    }

    /// Register a backend with a factory function
    ///
    /// # Arguments
    ///
    /// * `name` - Backend identifier (e.g., "gaussian-splatting")
    /// * `factory` - Function that creates new backend instances
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use three_dgs_processor::backends::BackendRegistry;
    /// # use three_dgs_processor::backends::GaussianSplattingBackend;
    /// use std::sync::Arc;
    ///
    /// let mut registry = BackendRegistry::new();
    /// registry.register(
    ///     "gaussian-splatting",
    ///     Box::new(|| Arc::new(GaussianSplattingBackend::new()))
    /// );
    /// ```
    pub fn register<F>(&mut self, name: impl Into<String>, factory: F)
    where
        F: Fn() -> Arc<dyn GaussianSplatBackend> + Send + Sync + 'static,
    {
        let name = name.into();
        debug!(backend = %name, "Registering backend");
        self.factories.insert(name, Box::new(factory));
    }

    /// Register all built-in backends
    ///
    /// This includes:
    /// - `gaussian-splatting` - Reference implementation
    /// - `gsplat` - Optimized Python/CUDA implementation
    /// - `3dgs-cpp` - High-performance C++ implementation
    /// - `mock` - Testing backend (fast, no external dependencies)
    pub fn register_defaults(&mut self) {
        info!("Registering default built-in backends");

        self.register("gaussian-splatting", || {
            Arc::new(GaussianSplattingBackend::new())
        });

        self.register("gsplat", || Arc::new(GsplatBackend::new()));

        self.register("3dgs-cpp", || Arc::new(ThreeDGSCppBackend::new()));

        self.register("mock", || Arc::new(MockBackend::new()));

        debug!(count = self.factories.len(), "Registered built-in backends");
    }

    /// Get a backend by name
    ///
    /// Searches in this order:
    /// 1. Built-in registered backends
    /// 2. Cached plugin backends
    /// 3. Attempts to load as plugin
    ///
    /// # Arguments
    ///
    /// * `name` - Backend identifier
    ///
    /// # Returns
    ///
    /// An arc-wrapped backend implementation
    ///
    /// # Errors
    ///
    /// Returns an error if the backend is not found or fails to load
    #[instrument(skip(self), fields(backend = name))]
    pub async fn get_backend(&mut self, name: &str) -> Result<Arc<dyn GaussianSplatBackend>> {
        // 1. Check built-in backends
        if let Some(factory) = self.factories.get(name) {
            debug!(backend = name, "Using built-in backend");
            return Ok(factory());
        }

        // 2. Check plugin cache
        if let Some(backend) = self.plugin_cache.get(name) {
            debug!(backend = name, "Using cached plugin backend");
            return Ok(Arc::clone(backend));
        }

        // 3. Try to load as plugin
        info!(backend = name, "Attempting to load backend as plugin");

        let backend = self
            .plugin_loader
            .load_backend(name)
            .with_context(|| format!("Failed to load backend '{}'", name))?;

        // Cache the loaded plugin
        self.plugin_cache
            .insert(name.to_string(), Arc::clone(&backend));

        Ok(backend)
    }

    /// Get backend name from configuration with GPU-aware auto-selection
    ///
    /// Selection priority:
    /// 1. BACKEND environment variable (explicit override)
    /// 2. YAML configuration backend
    /// 3. Auto-detect based on GPU availability
    ///
    /// # Arguments
    ///
    /// * `config_backend` - Backend name from YAML config (if any)
    ///
    /// # Returns
    ///
    /// The backend name to use
    pub fn resolve_backend_name(config_backend: Option<&str>) -> String {
        // Priority 1: Environment variable
        if let Ok(backend) = std::env::var("BACKEND") {
            info!(
                backend = %backend,
                source = "environment",
                "Selected backend from BACKEND env var"
            );
            return backend;
        }

        // Priority 2: YAML configuration
        if let Some(backend) = config_backend {
            // Check for "auto" keyword triggering GPU detection
            if backend == "auto" {
                return Self::auto_select_backend();
            }
            
            info!(
                backend = %backend,
                source = "configuration",
                "Selected backend from YAML config"
            );
            return backend.to_string();
        }

        // Priority 3: Auto-detect based on GPU (recommended default)
        info!("No backend specified - auto-selecting based on GPU capabilities");
        Self::auto_select_backend()
    }

    /// Auto-select backend based on GPU availability
    ///
    /// Uses GPU detection to recommend the best backend:
    /// - CUDA GPU → gsplat (fastest)
    /// - Metal GPU → gaussian-splatting
    /// - ROCm GPU → gaussian-splatting
    /// - CPU only → mock (for testing)
    ///
    /// Can be overridden with `FORCE_CPU_BACKEND=1` env var
    fn auto_select_backend() -> String {
        // Check for CPU-only mode override
        if std::env::var("FORCE_CPU_BACKEND").is_ok() {
            warn!("FORCE_CPU_BACKEND is set - using mock backend (no real training)");
            return "mock".to_string();
        }

        let gpu_info = detect_gpu();
        let status = gpu_status_string(&gpu_info);
        info!(gpu_status = %status, "GPU detection complete");

        let backend = gpu_info.recommend_backend();
        
        if !gpu_info.has_gpu() {
            warn!(
                backend = backend,
                "No GPU detected! Training will fail or be extremely slow. \
                 Consider setting FORCE_CPU_BACKEND=1 to use mock backend for testing."
            );
        } else {
            info!(
                backend = backend,
                platform = ?gpu_info.platform,
                "Auto-selected backend based on GPU capabilities"
            );
        }

        backend.to_string()
    }

    /// List all available backends
    ///
    /// Returns names of both built-in and plugin backends
    pub fn list_backends(&self) -> Vec<String> {
        let mut backends: Vec<String> = self.factories.keys().cloned().collect();

        // Add available plugins
        let plugins = self.plugin_loader.list_available_plugins();
        for plugin in plugins {
            if !backends.contains(&plugin) {
                backends.push(plugin);
            }
        }

        backends.sort();
        backends
    }

    /// Check if a backend is available
    ///
    /// Returns `true` if the backend is either:
    /// - Registered as a built-in
    /// - Available as a plugin
    /// - Already loaded in the cache
    pub fn is_backend_available(&self, name: &str) -> bool {
        // Check built-in
        if self.factories.contains_key(name) {
            return true;
        }

        // Check cache
        if self.plugin_cache.contains_key(name) {
            return true;
        }

        // Check if plugin exists
        let plugins = self.plugin_loader.list_available_plugins();
        plugins.contains(&name.to_string())
    }

    /// Get the plugin loader for direct manipulation
    pub fn plugin_loader_mut(&mut self) -> &mut PluginLoader {
        &mut self.plugin_loader
    }
}

impl Default for BackendRegistry {
    fn default() -> Self {
        let mut registry = Self::new();
        registry.register_defaults();
        registry
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    #[test]
    fn test_registry_creation() {
        let registry = BackendRegistry::new();
        assert_eq!(registry.factories.len(), 0);
        assert_eq!(registry.plugin_cache.len(), 0);
    }

    #[test]
    fn test_register_defaults() {
        let mut registry = BackendRegistry::new();
        registry.register_defaults();

        // Should register all built-in backends
        assert!(registry.factories.contains_key("gaussian-splatting"));
        assert!(registry.factories.contains_key("gsplat"));
        assert!(registry.factories.contains_key("3dgs-cpp"));
        assert!(registry.factories.contains_key("mock"));
        assert_eq!(registry.factories.len(), 4);
    }

    #[tokio::test]
    async fn test_get_builtin_backend() {
        let mut registry = BackendRegistry::new();
        registry.register_defaults();

        let backend = registry.get_backend("gaussian-splatting").await;
        assert!(backend.is_ok());
    }

    #[tokio::test]
    async fn test_get_nonexistent_backend() {
        let mut registry = BackendRegistry::new();
        registry.register_defaults();

        let backend = registry.get_backend("nonexistent").await;
        assert!(backend.is_err());
    }

    #[test]
    #[serial]
    fn test_resolve_backend_name_default() {
        // Clear env vars for clean state
        std::env::remove_var("BACKEND");
        std::env::remove_var("FORCE_CPU_BACKEND");

        let name = BackendRegistry::resolve_backend_name(None);
        // Default depends on GPU auto-detection; verify it returns a valid backend
        let valid_backends = ["gsplat", "gaussian-splatting", "mock"];
        assert!(
            valid_backends.contains(&name.as_str()),
            "Expected one of {valid_backends:?}, got {name:?}",
        );
    }

    #[test]
    #[serial]
    fn test_resolve_backend_name_config() {
        std::env::remove_var("BACKEND");

        let name = BackendRegistry::resolve_backend_name(Some("gsplat"));
        assert_eq!(name, "gsplat");
    }

    #[test]
    #[serial]
    fn test_resolve_backend_name_env_override() {
        std::env::set_var("BACKEND", "test-backend");

        let name = BackendRegistry::resolve_backend_name(Some("gsplat"));
        assert_eq!(name, "test-backend");

        std::env::remove_var("BACKEND");
    }

    #[test]
    fn test_list_backends() {
        let mut registry = BackendRegistry::new();
        registry.register_defaults();

        let backends = registry.list_backends();
        assert!(backends.contains(&"gaussian-splatting".to_string()));
    }

    #[test]
    fn test_is_backend_available() {
        let mut registry = BackendRegistry::new();
        registry.register_defaults();

        assert!(registry.is_backend_available("gaussian-splatting"));
        assert!(!registry.is_backend_available("nonexistent"));
    }
}
