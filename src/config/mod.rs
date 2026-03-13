mod env;
mod yaml;

#[cfg(test)]
mod tests;

pub use env::EnvConfig;
pub use yaml::{ColmapConfig, ExportConfig, FrameExtractionConfig, TrainingConfig, YamlConfig};

use anyhow::{Context, Result};
use std::path::PathBuf;

/// Complete application configuration combining environment variables and YAML
#[derive(Debug, Clone)]
pub struct Config {
    // Deployment settings (from environment variables)
    pub input_path: PathBuf,
    pub output_path: PathBuf,
    pub processed_path: PathBuf,
    pub error_path: PathBuf,
    pub temp_path: PathBuf,
    pub upload_stability_timeout_secs: u64,
    pub max_retries: u32,
    pub poll_interval_secs: u64,
    pub backend: String,
    pub log_level: String,
    pub retention_days: u32,

    // Frame extraction settings
    pub frame_rate: Option<f32>,
    pub frame_count: Option<u32>,

    // Azure settings (optional)
    pub azure: Option<AzureConfig>,

    // Run mode: "watch" (default) or "batch"
    pub run_mode: RunMode,

    // Batch-specific settings (only used when run_mode == Batch)
    pub batch: Option<BatchConfig>,

    // Training parameters (from YAML)
    pub training: YamlConfig,
}

/// Run mode selection
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunMode {
    /// Continuous file-watching service (default, existing behavior)
    Watch,
    /// Single batch job: download from blob → process → upload → exit
    Batch,
}

/// Batch-mode specific configuration
#[derive(Debug, Clone)]
pub struct BatchConfig {
    /// Blob prefix identifying the input folder (e.g., "scene_001/")
    pub input_prefix: String,
    /// Optional job ID for logging/tracking
    pub job_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AzureConfig {
    pub connection_string: Option<String>,
    pub storage_account: Option<String>,
    pub sas_token: Option<String>,
    pub use_managed_identity: bool,
    pub container_input: Option<String>,
    pub container_output: Option<String>,
    pub container_processed: Option<String>,
    pub container_error: Option<String>,
}

impl Config {
    /// Load configuration from environment variables and optional YAML file
    pub fn from_env() -> Result<Self> {
        let env_config = EnvConfig::load()?;

        // Load YAML config if path is specified and exists
        let yaml_config = if env_config.config_path.exists() {
            YamlConfig::load(&env_config.config_path)
                .context("Failed to load YAML configuration")?
        } else {
            YamlConfig::default()
        };

        // Build Azure config if any Azure env vars are present
        let azure = Self::build_azure_config(&env_config);

        // Determine run mode
        let run_mode = match std::env::var("RUN_MODE").unwrap_or_default().to_lowercase().as_str() {
            "batch" => RunMode::Batch,
            _ => RunMode::Watch,
        };

        // Build batch config if in batch mode
        let batch = if run_mode == RunMode::Batch {
            let input_prefix = std::env::var("BATCH_INPUT_PREFIX")
                .context("BATCH_INPUT_PREFIX is required when RUN_MODE=batch")?;
            if input_prefix.is_empty() {
                anyhow::bail!("BATCH_INPUT_PREFIX is set but empty");
            }
            Some(BatchConfig {
                input_prefix,
                job_id: std::env::var("BATCH_JOB_ID").ok(),
            })
        } else {
            None
        };

        Ok(Config {
            input_path: env_config.input_path,
            output_path: env_config.output_path,
            processed_path: env_config.processed_path,
            error_path: env_config.error_path,
            temp_path: env_config.temp_path,
            upload_stability_timeout_secs: env_config.upload_stability_timeout_secs,
            max_retries: env_config.max_retries,
            poll_interval_secs: env_config.poll_interval_secs,
            backend: env_config.backend,
            log_level: env_config.log_level,
            retention_days: env_config.retention_days,
            frame_rate: env_config.frame_rate,
            frame_count: env_config.frame_count,
            azure,
            run_mode,
            batch,
            training: yaml_config,
        })
    }

    /// Validate that required paths exist or can be created
    pub fn validate_paths(&self) -> Result<()> {
        let paths = [
            &self.input_path,
            &self.output_path,
            &self.processed_path,
            &self.error_path,
            &self.temp_path,
        ];

        for path in paths {
            if !path.exists() {
                std::fs::create_dir_all(path)
                    .with_context(|| format!("Failed to create directory: {}", path.display()))?;
            }
        }

        Ok(())
    }

    fn build_azure_config(env: &EnvConfig) -> Option<AzureConfig> {
        // Only create Azure config if at least one Azure setting is present
        if env.azure_connection_string.is_none()
            && env.azure_storage_account.is_none()
            && !env.azure_use_managed_identity
        {
            return None;
        }

        Some(AzureConfig {
            connection_string: env.azure_connection_string.clone(),
            storage_account: env.azure_storage_account.clone(),
            sas_token: env.azure_sas_token.clone(),
            use_managed_identity: env.azure_use_managed_identity,
            container_input: env.azure_container_input.clone(),
            container_output: env.azure_container_output.clone(),
            container_processed: env.azure_container_processed.clone(),
            container_error: env.azure_container_error.clone(),
        })
    }
}
