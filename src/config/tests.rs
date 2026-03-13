#[cfg(test)]
mod config_tests {
    use crate::config::{Config, EnvConfig, YamlConfig};
    use serial_test::serial;
    use std::env;
    use tempfile::TempDir;

    /// Helper to set up test environment
    fn setup_test_env() -> TempDir {
        // Clean up any existing env vars first
        cleanup_test_env();

        let temp_dir = TempDir::new().unwrap();
        let base = temp_dir.path();

        // Set required environment variables
        env::set_var("INPUT_PATH", base.join("input"));
        env::set_var("OUTPUT_PATH", base.join("output"));
        env::set_var("PROCESSED_PATH", base.join("processed"));
        env::set_var("ERROR_PATH", base.join("error"));

        temp_dir
    }

    /// Clean up test environment
    fn cleanup_test_env() {
        env::remove_var("INPUT_PATH");
        env::remove_var("OUTPUT_PATH");
        env::remove_var("PROCESSED_PATH");
        env::remove_var("ERROR_PATH");
        env::remove_var("TEMP_PATH");
        env::remove_var("CONFIG_PATH");
        env::remove_var("UPLOAD_STABILITY_TIMEOUT_SECS");
        env::remove_var("MAX_RETRIES");
        env::remove_var("POLL_INTERVAL_SECS");
        env::remove_var("FRAME_RATE");
        env::remove_var("FRAME_COUNT");
        env::remove_var("BACKEND");
        env::remove_var("LOG_LEVEL");
        env::remove_var("RETENTION_DAYS");
        env::remove_var("AZURE_STORAGE_CONNECTION_STRING");
        env::remove_var("AZURE_STORAGE_ACCOUNT");
        env::remove_var("AZURE_STORAGE_SAS_TOKEN");
        env::remove_var("AZURE_USE_MANAGED_IDENTITY");
    }

    #[test]
    #[serial]
    fn test_env_config_with_required_vars_only() {
        let _temp = setup_test_env();

        let config = EnvConfig::load().expect("Failed to load config");

        // Check required paths are set
        assert!(config.input_path.to_string_lossy().contains("input"));
        assert!(config.output_path.to_string_lossy().contains("output"));
        assert!(config
            .processed_path
            .to_string_lossy()
            .contains("processed"));
        assert!(config.error_path.to_string_lossy().contains("error"));

        // Check defaults
        assert_eq!(config.temp_path.to_string_lossy(), "/tmp/3dgs-work");
        assert_eq!(config.config_path.to_string_lossy(), "/config/config.yaml");
        assert_eq!(config.upload_stability_timeout_secs, 60);
        assert_eq!(config.max_retries, 3);
        assert_eq!(config.poll_interval_secs, 10);
        assert_eq!(config.backend, "gaussian-splatting");
        assert_eq!(config.log_level, "info");
        assert_eq!(config.retention_days, 30);

        cleanup_test_env();
    }

    #[test]
    #[serial]
    fn test_env_config_missing_required_var() {
        cleanup_test_env();

        // Don't set any required vars
        let result = EnvConfig::load();

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("INPUT_PATH"));
    }

    #[test]
    #[serial]
    fn test_env_config_with_custom_values() {
        let _temp = setup_test_env();

        env::set_var("TEMP_PATH", "/custom/temp");
        env::set_var("CONFIG_PATH", "/custom/config.yaml");
        env::set_var("UPLOAD_STABILITY_TIMEOUT_SECS", "120");
        env::set_var("MAX_RETRIES", "5");
        env::set_var("POLL_INTERVAL_SECS", "20");
        env::set_var("FRAME_RATE", "2.5");
        env::set_var("FRAME_COUNT", "100");
        env::set_var("BACKEND", "gsplat");
        env::set_var("LOG_LEVEL", "debug");
        env::set_var("RETENTION_DAYS", "60");

        let config = EnvConfig::load().expect("Failed to load config");

        assert_eq!(config.temp_path.to_string_lossy(), "/custom/temp");
        assert_eq!(config.config_path.to_string_lossy(), "/custom/config.yaml");
        assert_eq!(config.upload_stability_timeout_secs, 120);
        assert_eq!(config.max_retries, 5);
        assert_eq!(config.poll_interval_secs, 20);
        assert_eq!(config.frame_rate, Some(2.5));
        assert_eq!(config.frame_count, Some(100));
        assert_eq!(config.backend, "gsplat");
        assert_eq!(config.log_level, "debug");
        assert_eq!(config.retention_days, 60);

        cleanup_test_env();
    }

    #[test]
    #[serial]
    fn test_env_config_invalid_numeric_value() {
        let _temp = setup_test_env();
        env::set_var("FRAME_RATE", "not-a-number");

        let result = EnvConfig::load();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("FRAME_RATE"));

        cleanup_test_env();
    }

    #[test]
    #[serial]
    fn test_azure_config_connection_string() {
        let _temp = setup_test_env();
        env::set_var(
            "AZURE_STORAGE_CONNECTION_STRING",
            "DefaultEndpointsProtocol=https;...",
        );

        let env_config = EnvConfig::load().expect("Failed to load config");
        let azure = Config::build_azure_config(&env_config);

        assert!(azure.is_some());
        let azure = azure.unwrap();
        assert_eq!(
            azure.connection_string.as_deref(),
            Some("DefaultEndpointsProtocol=https;...")
        );

        cleanup_test_env();
    }

    #[test]
    #[serial]
    fn test_azure_config_sas_token() {
        let _temp = setup_test_env();
        env::set_var("AZURE_STORAGE_ACCOUNT", "myaccount");
        env::set_var("AZURE_STORAGE_SAS_TOKEN", "sv=2021-06-08&...");

        let env_config = EnvConfig::load().expect("Failed to load config");
        let azure = Config::build_azure_config(&env_config);

        assert!(azure.is_some());
        let azure = azure.unwrap();
        assert_eq!(azure.storage_account.as_deref(), Some("myaccount"));
        assert_eq!(azure.sas_token.as_deref(), Some("sv=2021-06-08&..."));

        cleanup_test_env();
    }

    #[test]
    #[serial]
    fn test_azure_config_managed_identity() {
        let _temp = setup_test_env();
        env::set_var("AZURE_STORAGE_ACCOUNT", "myaccount");
        env::set_var("AZURE_USE_MANAGED_IDENTITY", "true");

        let env_config = EnvConfig::load().expect("Failed to load config");
        let azure = Config::build_azure_config(&env_config);

        assert!(azure.is_some());
        let azure = azure.unwrap();
        assert_eq!(azure.storage_account.as_deref(), Some("myaccount"));
        assert!(azure.use_managed_identity);

        cleanup_test_env();
    }

    #[test]
    #[serial]
    fn test_azure_config_none_when_no_vars() {
        let _temp = setup_test_env();

        let env_config = EnvConfig::load().expect("Failed to load config");
        let azure = Config::build_azure_config(&env_config);

        assert!(azure.is_none());

        cleanup_test_env();
    }

    #[test]
    fn test_yaml_config_defaults() {
        let config = YamlConfig::default();

        assert_eq!(config.backend, "gaussian-splatting");
        assert_eq!(config.frame_extraction.rate, 1.0);
        assert_eq!(config.frame_extraction.count, None);
        assert_eq!(config.colmap.matcher, "exhaustive");
        assert_eq!(config.colmap.camera_model, "OPENCV");
        assert_eq!(config.training.iterations, 30000);
        assert_eq!(config.training.learning_rate, 0.0025);
        assert_eq!(config.training.densification_interval, 100);
        assert_eq!(config.training.densification_start, 500);
        assert_eq!(config.training.densification_stop, 15000);
        assert_eq!(config.training.opacity_reset_interval, 3000);
        assert_eq!(config.export.formats, vec!["ply", "splat"]);
        assert_eq!(config.export.ply_precision, "float32");
    }

    #[test]
    fn test_yaml_config_load_from_file() {
        use std::fs;

        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("test_config.yaml");

        let yaml_content = r#"
backend: gsplat
frame_extraction:
  rate: 2.0
  count: 200
colmap:
  matcher: sequential
  camera_model: SIMPLE_PINHOLE
training:
  iterations: 50000
  learning_rate: 0.005
  densification_interval: 200
  densification_start: 1000
  densification_stop: 20000
  opacity_reset_interval: 5000
export:
  formats:
    - ply
  ply_precision: float16
"#;

        fs::write(&config_path, yaml_content).expect("Failed to write test config");

        let config = YamlConfig::load(&config_path).expect("Failed to load YAML config");

        assert_eq!(config.backend, "gsplat");
        assert_eq!(config.frame_extraction.rate, 2.0);
        assert_eq!(config.frame_extraction.count, Some(200));
        assert_eq!(config.colmap.matcher, "sequential");
        assert_eq!(config.colmap.camera_model, "SIMPLE_PINHOLE");
        assert_eq!(config.training.iterations, 50000);
        assert_eq!(config.training.learning_rate, 0.005);
        assert_eq!(config.training.densification_interval, 200);
        assert_eq!(config.training.densification_start, 1000);
        assert_eq!(config.training.densification_stop, 20000);
        assert_eq!(config.training.opacity_reset_interval, 5000);
        assert_eq!(config.export.formats, vec!["ply"]);
        assert_eq!(config.export.ply_precision, "float16");
    }

    #[test]
    fn test_yaml_config_partial_file() {
        use std::fs;

        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("partial_config.yaml");

        // Only provide some fields, others should use defaults
        let yaml_content = r#"
training:
  iterations: 10000
"#;

        fs::write(&config_path, yaml_content).expect("Failed to write test config");

        let config = YamlConfig::load(&config_path).expect("Failed to load YAML config");

        assert_eq!(config.training.iterations, 10000);
        // These should be defaults
        assert_eq!(config.backend, "gaussian-splatting");
        assert_eq!(config.training.learning_rate, 0.0025);
        assert_eq!(config.colmap.matcher, "exhaustive");
    }

    #[test]
    #[serial]
    fn test_config_from_env_with_yaml() {
        let temp_dir = setup_test_env();

        // Create a YAML config file
        let config_path = temp_dir.path().join("config.yaml");
        let yaml_content = r#"
training:
  iterations: 25000
"#;
        std::fs::write(&config_path, yaml_content).expect("Failed to write test config");
        env::set_var("CONFIG_PATH", &config_path);

        let config = Config::from_env().expect("Failed to load complete config");

        // Check env vars are loaded
        assert!(config.input_path.to_string_lossy().contains("input"));
        assert_eq!(config.backend, "gaussian-splatting");

        // Check YAML is loaded
        assert_eq!(config.training.training.iterations, 25000);

        cleanup_test_env();
    }

    #[test]
    #[serial]
    fn test_config_from_env_without_yaml() {
        let _temp = setup_test_env();
        env::set_var("CONFIG_PATH", "/nonexistent/config.yaml");

        let config = Config::from_env().expect("Failed to load config without YAML");

        // Should use defaults when YAML file doesn't exist
        assert_eq!(config.training.training.iterations, 30000);

        cleanup_test_env();
    }

    #[test]
    #[serial]
    fn test_validate_paths_creates_directories() {
        let _temp_dir = setup_test_env();

        let config = Config::from_env().expect("Failed to load config");

        // Paths shouldn't exist yet
        assert!(!config.input_path.exists());

        // validate_paths should create them
        config.validate_paths().expect("Failed to validate paths");

        assert!(config.input_path.exists());
        assert!(config.output_path.exists());
        assert!(config.processed_path.exists());
        assert!(config.error_path.exists());
        assert!(config.temp_path.exists());

        cleanup_test_env();
    }
}
