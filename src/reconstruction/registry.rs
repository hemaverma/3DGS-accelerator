//! Reconstruction backend registry
//!
//! Central registry for all available reconstruction backends.
//! Handles backend selection, registration, and discovery.

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use tracing::{info, warn};

use super::backend::ReconstructionBackend;
use super::colmap_backend::ColmapBackend;
use super::nerfstudio::NerfstudioBackend;
use super::precalibrated::PrecalibratedBackend;

/// Backend information for discovery
#[derive(Debug, Clone)]
pub struct BackendInfo {
    pub name: String,
    pub description: String,
    pub available: bool,
}

/// Global backend registry
static REGISTRY: once_cell::sync::Lazy<BackendRegistry> =
    once_cell::sync::Lazy::new(BackendRegistry::new);

/// Backend registry implementation
struct BackendRegistry {
    backends: RwLock<HashMap<String, Arc<dyn ReconstructionBackend>>>,
}

impl BackendRegistry {
    fn new() -> Self {
        let registry = Self {
            backends: RwLock::new(HashMap::new()),
        };

        // Register built-in backends
        registry.register_builtin_backends();
        registry
    }

    fn register_builtin_backends(&self) {
        info!("Registering built-in reconstruction backends");

        // COLMAP (primary SfM backend)
        let colmap = Arc::new(ColmapBackend::new());
        self.register_backend(colmap.clone())
            .expect("Failed to register COLMAP backend");

        // Nerfstudio transforms.json
        let nerfstudio = Arc::new(NerfstudioBackend::new());
        self.register_backend(nerfstudio.clone())
            .expect("Failed to register Nerfstudio backend");

        // Precalibrated (known poses)
        let precalibrated = Arc::new(PrecalibratedBackend::new());
        self.register_backend(precalibrated.clone())
            .expect("Failed to register Precalibrated backend");

        info!("Registered {} reconstruction backends", self.count());
    }

    fn register_backend(&self, backend: Arc<dyn ReconstructionBackend>) -> Result<()> {
        let name = backend.name().to_string();
        let available = backend.is_available().unwrap_or(false);

        if available {
            info!(backend = %name, "Reconstruction backend available");
        } else {
            warn!(backend = %name, "Reconstruction backend not available");
        }

        let mut backends = self.backends.write().unwrap();
        backends.insert(name.clone(), backend);
        Ok(())
    }

    fn get(&self, name: &str) -> Option<Arc<dyn ReconstructionBackend>> {
        let backends = self.backends.read().unwrap();
        backends.get(name).cloned()
    }

    fn list(&self) -> Vec<BackendInfo> {
        let backends = self.backends.read().unwrap();
        backends
            .values()
            .map(|b| BackendInfo {
                name: b.name().to_string(),
                description: b.description().to_string(),
                available: b.is_available().unwrap_or(false),
            })
            .collect()
    }

    fn count(&self) -> usize {
        let backends = self.backends.read().unwrap();
        backends.len()
    }
}

/// Get reconstruction backend by name
///
/// # Environment Variables
/// * `RECONSTRUCTION_BACKEND` - Backend name (default: "colmap")
///
/// # Examples
/// ```no_run
/// use three_dgs_processor::reconstruction::get_reconstruction_backend;
///
/// let backend = get_reconstruction_backend(Some("colmap")).unwrap();
/// ```
pub fn get_reconstruction_backend(
    name: Option<&str>,
) -> Result<Arc<dyn ReconstructionBackend>> {
    // Get backend name from parameter or environment variable
    let backend_name = name
        .map(String::from)
        .or_else(|| std::env::var("RECONSTRUCTION_BACKEND").ok())
        .unwrap_or_else(|| "colmap".to_string());

    info!(backend = %backend_name, "Selecting reconstruction backend");

    let backend = REGISTRY
        .get(&backend_name)
        .with_context(|| format!("Reconstruction backend '{}' not found", backend_name))?;

    // Check if backend is available
    if !backend.is_available().unwrap_or(false) {
        warn!(
            backend = %backend_name,
            "Reconstruction backend not available, but will attempt to use it"
        );
    }

    Ok(backend)
}

/// List all registered reconstruction backends
pub fn list_reconstruction_backends() -> Vec<BackendInfo> {
    REGISTRY.list()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_has_backends() {
        let backends = list_reconstruction_backends();
        assert!(
            !backends.is_empty(),
            "Registry should have at least one backend"
        );
    }

    #[test]
    fn test_get_colmap_backend() {
        let backend = get_reconstruction_backend(Some("colmap"));
        assert!(backend.is_ok(), "COLMAP backend should be registered");
        assert_eq!(backend.unwrap().name(), "colmap");
    }

    #[test]
    fn test_get_nerfstudio_backend() {
        let backend = get_reconstruction_backend(Some("nerfstudio"));
        assert!(backend.is_ok(), "Nerfstudio backend should be registered");
        assert_eq!(backend.unwrap().name(), "nerfstudio");
    }

    #[test]
    fn test_get_precalibrated_backend() {
        let backend = get_reconstruction_backend(Some("precalibrated"));
        assert!(
            backend.is_ok(),
            "Precalibrated backend should be registered"
        );
        assert_eq!(backend.unwrap().name(), "precalibrated");
    }

    #[test]
    fn test_invalid_backend() {
        let backend = get_reconstruction_backend(Some("nonexistent"));
        assert!(backend.is_err(), "Should fail for invalid backend");
    }

    #[test]
    fn test_default_backend() {
        std::env::remove_var("RECONSTRUCTION_BACKEND");
        let backend = get_reconstruction_backend(None);
        assert!(backend.is_ok(), "Should use default backend");
        assert_eq!(backend.unwrap().name(), "colmap");
    }
}
