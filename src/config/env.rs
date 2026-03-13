use anyhow::{Context, Result};
use std::env;
use std::path::PathBuf;

/// Configuration loaded from environment variables
#[derive(Debug, Clone)]
pub struct EnvConfig {
    // Required paths
    pub input_path: PathBuf,
    pub output_path: PathBuf,
    pub processed_path: PathBuf,
    pub error_path: PathBuf,

    // Optional paths with defaults
    pub temp_path: PathBuf,
    pub config_path: PathBuf,

    // Processing settings
    pub upload_stability_timeout_secs: u64,
    pub max_retries: u32,
    pub poll_interval_secs: u64,
    pub frame_rate: Option<f32>,
    pub frame_count: Option<u32>,
    pub backend: String,
    pub reconstruction_backend: String,
    pub log_level: String,
    pub retention_days: u32,

    // Azure settings
    pub azure_connection_string: Option<String>,
    pub azure_storage_account: Option<String>,
    pub azure_sas_token: Option<String>,
    pub azure_use_managed_identity: bool,
    pub azure_container_input: Option<String>,
    pub azure_container_output: Option<String>,
    pub azure_container_processed: Option<String>,
    pub azure_container_error: Option<String>,
}

impl EnvConfig {
    pub fn load() -> Result<Self> {
        let is_batch = std::env::var("RUN_MODE")
            .unwrap_or_default()
            .to_lowercase()
            == "batch";

        Ok(EnvConfig {
            // Paths: required in watch mode, default to temp dirs in batch mode
            input_path: if is_batch {
                get_path_or_default("INPUT_PATH", "/tmp/3dgs-work/batch-input")
            } else {
                get_required_path("INPUT_PATH")?
            },
            output_path: if is_batch {
                get_path_or_default("OUTPUT_PATH", "/tmp/3dgs-work/batch-output")
            } else {
                get_required_path("OUTPUT_PATH")?
            },
            processed_path: if is_batch {
                get_path_or_default("PROCESSED_PATH", "/tmp/3dgs-work/batch-processed")
            } else {
                get_required_path("PROCESSED_PATH")?
            },
            error_path: if is_batch {
                get_path_or_default("ERROR_PATH", "/tmp/3dgs-work/batch-error")
            } else {
                get_required_path("ERROR_PATH")?
            },

            // Optional paths with defaults
            temp_path: get_path_or_default("TEMP_PATH", "/tmp/3dgs-work"),
            config_path: get_path_or_default("CONFIG_PATH", "/config/config.yaml"),

            // Processing settings with defaults
            upload_stability_timeout_secs: get_u64_or_default("UPLOAD_STABILITY_TIMEOUT_SECS", 60),
            max_retries: get_u32_or_default("MAX_RETRIES", 3),
            poll_interval_secs: get_u64_or_default("POLL_INTERVAL_SECS", 10),
            frame_rate: get_optional_f32("FRAME_RATE")?,
            frame_count: get_optional_u32("FRAME_COUNT")?,
            backend: get_string_or_default("BACKEND", "gaussian-splatting"),
            reconstruction_backend: get_string_or_default("RECONSTRUCTION_BACKEND", "colmap"),
            log_level: get_string_or_default("LOG_LEVEL", "info"),
            retention_days: get_u32_or_default("RETENTION_DAYS", 30),

            // Azure settings
            azure_connection_string: env::var("AZURE_STORAGE_CONNECTION_STRING").ok(),
            azure_storage_account: env::var("AZURE_STORAGE_ACCOUNT").ok(),
            azure_sas_token: env::var("AZURE_STORAGE_SAS_TOKEN").ok(),
            azure_use_managed_identity: env::var("AZURE_USE_MANAGED_IDENTITY")
                .unwrap_or_default()
                .parse()
                .unwrap_or(false),
            azure_container_input: env::var("AZURE_BLOB_CONTAINER_INPUT").ok(),
            azure_container_output: env::var("AZURE_BLOB_CONTAINER_OUTPUT").ok(),
            azure_container_processed: env::var("AZURE_BLOB_CONTAINER_PROCESSED").ok(),
            azure_container_error: env::var("AZURE_BLOB_CONTAINER_ERROR").ok(),
        })
    }
}

fn get_required_path(var: &str) -> Result<PathBuf> {
    env::var(var)
        .with_context(|| format!("Missing required environment variable: {}", var))
        .map(PathBuf::from)
}

fn get_path_or_default(var: &str, default: &str) -> PathBuf {
    env::var(var).unwrap_or_else(|_| default.to_string()).into()
}

fn get_u64_or_default(var: &str, default: u64) -> u64 {
    env::var(var)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn get_u32_or_default(var: &str, default: u32) -> u32 {
    env::var(var)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn get_string_or_default(var: &str, default: &str) -> String {
    env::var(var).unwrap_or_else(|_| default.to_string())
}

fn get_optional_f32(var: &str) -> Result<Option<f32>> {
    match env::var(var) {
        Ok(val) => val
            .parse()
            .with_context(|| format!("Invalid float value for {}: {}", var, val))
            .map(Some),
        Err(_) => Ok(None),
    }
}

fn get_optional_u32(var: &str) -> Result<Option<u32>> {
    match env::var(var) {
        Ok(val) => val
            .parse()
            .with_context(|| format!("Invalid integer value for {}: {}", var, val))
            .map(Some),
        Err(_) => Ok(None),
    }
}
