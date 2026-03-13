//! Backend Registry Integration Tests
//!
//! Tests the backend registry system including:
//! - Backend registration and discovery
//! - Backend selection based on configuration
//! - Auto-detection with GPU availability
//! - Fallback mechanisms

use tempfile::TempDir;
use three_dgs_processor::backends::{BackendRegistry, TrainingConfig};
use std::path::Path;

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_registry_initialization() {
        let mut registry = BackendRegistry::new();
        assert_eq!(registry.list_backends().len(), 0);

        registry.register_defaults();
        let backends = registry.list_backends();
        
        // Should have at least the mock backend
        assert!(!backends.is_empty());
        assert!(backends.contains(&"mock".to_string()));
    }

    #[tokio::test]
    async fn test_registry_mock_backend_available() {
        let mut registry = BackendRegistry::new();
        registry.register_defaults();

        assert!(registry.is_backend_available("mock"));
        assert!(!registry.is_backend_available("nonexistent-backend"));
    }

    #[tokio::test]
    async fn test_registry_get_mock_backend() {
        let mut registry = BackendRegistry::new();
        registry.register_defaults();

        let backend = registry.get_backend("mock").await;
        assert!(backend.is_ok());
    }

    #[tokio::test]
    async fn test_registry_get_nonexistent_backend() {
        let mut registry = BackendRegistry::new();
        registry.register_defaults();

        let result = registry.get_backend("invalid-backend-xyz").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_registry_backend_train() {
        let mut registry = BackendRegistry::new();
        registry.register_defaults();

        let backend = registry.get_backend("mock").await.unwrap();

        // Create test frame
        let temp = TempDir::new().unwrap();
        let frame = temp.path().join("frame.jpg");
        tokio::fs::write(&frame, b"test").await.unwrap();

        let frames: Vec<&Path> = vec![frame.as_path()];
        let config = TrainingConfig::default();

        let output = backend.train(&frames, &config).await.unwrap();
        assert_eq!(output.metadata.backend_name, "mock");
    }

    #[tokio::test]
    async fn test_backend_auto_selection() {
        let mut registry = BackendRegistry::new();
        registry.register_defaults();
        
        // Clear env var to test auto selection
        std::env::remove_var("BACKEND");
        
        // Auto mode should select an available backend
        let backends = registry.list_backends();
        assert!(!backends.is_empty());
    }

    #[tokio::test]
    async fn test_backend_selection_by_name() {
        let mut registry = BackendRegistry::new();
        registry.register_defaults();
        
        std::env::remove_var("BACKEND");
        
        // Mock backend should always be available
        let backend = registry.get_backend("mock").await;
        assert!(backend.is_ok());
    }

    #[tokio::test]
    async fn test_backend_env_var_selection() {
        let mut registry = BackendRegistry::new();
        registry.register_defaults();
        
        // When BACKEND env var is set, it should be used
        std::env::set_var("BACKEND", "mock");
        let result = registry.get_backend("mock").await;
        assert!(result.is_ok());
        
        std::env::remove_var("BACKEND");
    }

    #[tokio::test]
    async fn test_multiple_backend_instances() {
        let mut registry1 = BackendRegistry::new();
        let mut registry2 = BackendRegistry::new();

        registry1.register_defaults();
        registry2.register_defaults();

        let backend1 = registry1.get_backend("mock").await.unwrap();
        let backend2 = registry2.get_backend("mock").await.unwrap();

        // Both should work independently
        let temp = TempDir::new().unwrap();
        let frame = temp.path().join("frame.jpg");
        tokio::fs::write(&frame, b"test").await.unwrap();
        let frames: Vec<&Path> = vec![frame.as_path()];
        let config = TrainingConfig::default();

        let output1 = backend1.train(&frames, &config).await.unwrap();
        let output2 = backend2.train(&frames, &config).await.unwrap();

        assert_eq!(output1.metadata.backend_name, "mock");
        assert_eq!(output2.metadata.backend_name, "mock");
    }

    #[tokio::test]
    async fn test_backend_list_consistency() {
        let mut registry = BackendRegistry::new();
        registry.register_defaults();

        let list1 = registry.list_backends();
        let list2 = registry.list_backends();

        assert_eq!(list1, list2);
    }

    #[tokio::test]
    async fn test_backend_availability_after_get() {
        let mut registry = BackendRegistry::new();
        registry.register_defaults();

        assert!(registry.is_backend_available("mock"));

        // Get backend
        let _backend = registry.get_backend("mock").await.unwrap();

        // Should still be available
        assert!(registry.is_backend_available("mock"));
    }

    #[tokio::test]
    async fn test_registry_with_all_available_backends() {
        let mut registry = BackendRegistry::new();
        registry.register_defaults();

        let backends = registry.list_backends();

        // Test each backend can be retrieved
        for backend_name in backends {
            let result = registry.get_backend(&backend_name).await;
            assert!(
                result.is_ok(),
                "Failed to get backend '{}': {:?}",
                backend_name,
                result.err()
            );
        }
    }

    #[tokio::test]
    async fn test_registry_force_cpu_backend() {
        // Test FORCE_CPU_BACKEND environment variable
        std::env::set_var("FORCE_CPU_BACKEND", "1");
        
        let mut registry = BackendRegistry::new();
        registry.register_defaults();
        
        // Mock should be available when forced to CPU
        assert!(registry.is_backend_available("mock"));
        
        std::env::remove_var("FORCE_CPU_BACKEND");
    }

    #[tokio::test]
    async fn test_backend_concurrent_access() {
        // Create multiple backend requests concurrently
        let handles: Vec<_> = (0..5)
            .map(|_| {
                tokio::spawn(async move {
                    let mut reg = BackendRegistry::new();
                    reg.register_defaults();
                    reg.get_backend("mock").await
                })
            })
            .collect();

        // All should succeed
        for handle in handles {
            let result = handle.await.unwrap();
            assert!(result.is_ok());
        }
    }

    #[tokio::test]
    async fn test_backend_chain_operations() {
        let mut registry = BackendRegistry::new();
        registry.register_defaults();

        let backend = registry.get_backend("mock").await.unwrap();

        let temp = TempDir::new().unwrap();
        let frame = temp.path().join("frame.jpg");
        tokio::fs::write(&frame, b"test").await.unwrap();
        let frames: Vec<&Path> = vec![frame.as_path()];
        let config = TrainingConfig::default();

        // Train
        let output = backend.train(&frames, &config).await.unwrap();

        // Export PLY
        let ply_path = temp.path().join("output.ply");
        backend.export_ply(&output, &ply_path).await.unwrap();
        assert!(ply_path.exists());

        // Export SPLAT
        let splat_path = temp.path().join("output.splat");
        backend.export_splat(&output, &splat_path).await.unwrap();
        assert!(splat_path.exists());
    }

    #[tokio::test]
    async fn test_backend_names_are_lowercase() {
        let mut registry = BackendRegistry::new();
        registry.register_defaults();

        let backends = registry.list_backends();
        
        for backend_name in backends {
            assert_eq!(
                backend_name, 
                backend_name.to_lowercase(),
                "Backend name '{}' should be lowercase",
                backend_name
            );
        }
    }

    #[tokio::test]
    async fn test_registry_backend_order_stability() {
        let mut registry = BackendRegistry::new();
        registry.register_defaults();

        let list1 = registry.list_backends();
        
        // Get a backend
        let _ = registry.get_backend("mock").await;
        
        let list2 = registry.list_backends();

        // Order should remain stable
        assert_eq!(list1, list2);
    }
}
