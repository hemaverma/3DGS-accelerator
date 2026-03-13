//! Blobfuse2 mounting logic
//!
//! This module handles mounting Azure Blob containers using Blobfuse2.
//! It supports mounting multiple containers (input, output, processed, error)
//! and verifies mount success before proceeding with processing.

use super::auth::AzureAuth;
use anyhow::{bail, Context, Result};
use std::env;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use tracing::{debug, info, instrument, warn};

/// Configuration for mounting Azure Blob containers
#[derive(Debug, Clone)]
pub struct MountConfig {
    /// Container name for input videos
    pub input_container: String,
    /// Container name for output 3DGS files
    pub output_container: String,
    /// Container name for processed inputs
    pub processed_container: String,
    /// Container name for failed inputs
    pub error_container: String,
    /// Local mount paths
    pub input_path: PathBuf,
    pub output_path: PathBuf,
    pub processed_path: PathBuf,
    pub error_path: PathBuf,
}

impl MountConfig {
    /// Load mount configuration from environment variables
    pub fn from_env() -> Result<Self> {
        Ok(MountConfig {
            input_container: env::var("AZURE_BLOB_CONTAINER_INPUT")
                .unwrap_or_else(|_| "input".to_string()),
            output_container: env::var("AZURE_BLOB_CONTAINER_OUTPUT")
                .unwrap_or_else(|_| "output".to_string()),
            processed_container: env::var("AZURE_BLOB_CONTAINER_PROCESSED")
                .unwrap_or_else(|_| "processed".to_string()),
            error_container: env::var("AZURE_BLOB_CONTAINER_ERROR")
                .unwrap_or_else(|_| "error".to_string()),
            input_path: PathBuf::from(env::var("INPUT_PATH").context("INPUT_PATH not set")?),
            output_path: PathBuf::from(env::var("OUTPUT_PATH").context("OUTPUT_PATH not set")?),
            processed_path: PathBuf::from(
                env::var("PROCESSED_PATH").context("PROCESSED_PATH not set")?,
            ),
            error_path: PathBuf::from(env::var("ERROR_PATH").context("ERROR_PATH not set")?),
        })
    }
}

/// Mount all required Azure Blob containers using Blobfuse2
///
/// This function mounts all four required containers (input, output, processed, error)
/// to their respective local filesystem paths. It requires privileged container mode
/// for FUSE mounting.
///
/// # Security
/// The Blobfuse2 config files contain credentials and are created with 0600 permissions.
/// They are stored in /tmp/blobfuse-configs/ and should be cleaned up on exit.
#[instrument(skip(auth))]
pub async fn mount_all_containers(auth: &AzureAuth, config: &MountConfig) -> Result<()> {
    info!("Starting Azure Blob container mounting via Blobfuse2");

    // Create config directory
    let config_dir = PathBuf::from("/tmp/blobfuse-configs");
    fs::create_dir_all(&config_dir).context("Failed to create blobfuse config directory")?;

    // Create cache directories
    for container in &[
        &config.input_container,
        &config.output_container,
        &config.processed_container,
        &config.error_container,
    ] {
        let cache_dir = PathBuf::from(format!("/tmp/blobfuse-cache/{}", container));
        fs::create_dir_all(&cache_dir)
            .with_context(|| format!("Failed to create cache directory for {}", container))?;
    }

    // Mount each container
    mount_container(
        auth,
        &config.input_container,
        &config.input_path,
        &config_dir,
    )
    .await
    .context("Failed to mount input container")?;

    mount_container(
        auth,
        &config.output_container,
        &config.output_path,
        &config_dir,
    )
    .await
    .context("Failed to mount output container")?;

    mount_container(
        auth,
        &config.processed_container,
        &config.processed_path,
        &config_dir,
    )
    .await
    .context("Failed to mount processed container")?;

    mount_container(
        auth,
        &config.error_container,
        &config.error_path,
        &config_dir,
    )
    .await
    .context("Failed to mount error container")?;

    info!("All Azure Blob containers mounted successfully");
    Ok(())
}

/// Mount a single Azure Blob container
#[instrument(skip(auth))]
async fn mount_container(
    auth: &AzureAuth,
    container_name: &str,
    mount_path: &Path,
    config_dir: &Path,
) -> Result<()> {
    info!(
        container = %container_name,
        mount_path = %mount_path.display(),
        "Mounting container"
    );

    // Create mount point if it doesn't exist
    fs::create_dir_all(mount_path)
        .with_context(|| format!("Failed to create mount point {}", mount_path.display()))?;

    // Generate blobfuse2 config
    let config_content = auth.to_blobfuse_config(container_name, mount_path.to_str().unwrap());
    let config_file = config_dir.join(format!("{}.yaml", container_name));

    // Write config with restricted permissions
    fs::write(&config_file, config_content)
        .with_context(|| format!("Failed to write config file {}", config_file.display()))?;

    // Set permissions to 0600 (owner read/write only)
    fs::set_permissions(&config_file, fs::Permissions::from_mode(0o600))
        .with_context(|| format!("Failed to set permissions on {}", config_file.display()))?;

    debug!(
        config_file = %config_file.display(),
        "Created blobfuse2 config with restricted permissions"
    );

    // Execute blobfuse2 mount command
    let output = Command::new("blobfuse2")
        .arg("mount")
        .arg(mount_path)
        .arg("--config-file")
        .arg(&config_file)
        .arg("--foreground=false")
        .output()
        .context("Failed to execute blobfuse2 command")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!(
            "Blobfuse2 mount failed for container '{}': {}",
            container_name,
            stderr
        );
    }

    // Verify mount succeeded
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
    if !is_mounted(mount_path)? {
        bail!(
            "Mount verification failed for container '{}' at {}",
            container_name,
            mount_path.display()
        );
    }

    info!(
        container = %container_name,
        mount_path = %mount_path.display(),
        "Container mounted successfully"
    );

    Ok(())
}

/// Verify that all required mounts are active
#[instrument]
pub fn verify_mounts(config: &MountConfig) -> Result<()> {
    info!("Verifying all Azure Blob mounts");

    let mounts = [
        (&config.input_path, "input"),
        (&config.output_path, "output"),
        (&config.processed_path, "processed"),
        (&config.error_path, "error"),
    ];

    for (path, name) in &mounts {
        if !is_mounted(path)? {
            bail!(
                "Mount verification failed for {} at {}",
                name,
                path.display()
            );
        }
        debug!(mount = %name, path = %path.display(), "Mount verified");
    }

    info!("All mounts verified successfully");
    Ok(())
}

/// Check if a path is mounted via Blobfuse2
fn is_mounted(path: &Path) -> Result<bool> {
    // Read /proc/mounts to check if path is a mount point
    let mounts = fs::read_to_string("/proc/mounts").context("Failed to read /proc/mounts")?;

    let canonical_path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());

    for line in mounts.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 2 {
            continue;
        }

        let mount_point = PathBuf::from(parts[1]);
        if mount_point == canonical_path {
            debug!(
                path = %path.display(),
                filesystem = %parts[0],
                "Mount point found"
            );
            return Ok(true);
        }
    }

    warn!(
        path = %path.display(),
        "Path is not mounted"
    );
    Ok(false)
}

/// Unmount all Azure Blob containers (for graceful shutdown)
#[instrument]
pub async fn unmount_all_containers(config: &MountConfig) -> Result<()> {
    info!("Unmounting all Azure Blob containers");

    let paths = [
        &config.input_path,
        &config.output_path,
        &config.processed_path,
        &config.error_path,
    ];

    for path in &paths {
        if is_mounted(path)? {
            unmount_container(path).await?;
        }
    }

    // Clean up config files
    let config_dir = PathBuf::from("/tmp/blobfuse-configs");
    if config_dir.exists() {
        fs::remove_dir_all(&config_dir).context("Failed to clean up blobfuse config directory")?;
    }

    info!("All containers unmounted and configs cleaned up");
    Ok(())
}

/// Unmount a single container
#[instrument]
async fn unmount_container(mount_path: &Path) -> Result<()> {
    info!(mount_path = %mount_path.display(), "Unmounting container");

    let output = Command::new("fusermount")
        .arg("-u")
        .arg(mount_path)
        .output()
        .context("Failed to execute fusermount command")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        warn!(
            mount_path = %mount_path.display(),
            error = %stderr,
            "Failed to unmount (may already be unmounted)"
        );
    } else {
        info!(mount_path = %mount_path.display(), "Container unmounted successfully");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    // Mutex to ensure tests don't run in parallel and interfere with each other's env vars
    static TEST_MUTEX: Mutex<()> = Mutex::new(());

    #[test]
    fn test_mount_config_defaults() {
        let _lock = TEST_MUTEX.lock().unwrap();

        // Set required env vars for test
        env::set_var("INPUT_PATH", "/mnt/input");
        env::set_var("OUTPUT_PATH", "/mnt/output");
        env::set_var("PROCESSED_PATH", "/mnt/processed");
        env::set_var("ERROR_PATH", "/mnt/error");

        let config = MountConfig::from_env().unwrap();
        assert_eq!(config.input_container, "input");
        assert_eq!(config.output_container, "output");
        assert_eq!(config.processed_container, "processed");
        assert_eq!(config.error_container, "error");

        // Clean up
        env::remove_var("INPUT_PATH");
        env::remove_var("OUTPUT_PATH");
        env::remove_var("PROCESSED_PATH");
        env::remove_var("ERROR_PATH");
    }

    #[test]
    fn test_mount_config_custom_containers() {
        let _lock = TEST_MUTEX.lock().unwrap();

        // Set required env vars for test
        env::set_var("INPUT_PATH", "/mnt/input");
        env::set_var("OUTPUT_PATH", "/mnt/output");
        env::set_var("PROCESSED_PATH", "/mnt/processed");
        env::set_var("ERROR_PATH", "/mnt/error");
        env::set_var("AZURE_BLOB_CONTAINER_INPUT", "custom-input");
        env::set_var("AZURE_BLOB_CONTAINER_OUTPUT", "custom-output");

        let config = MountConfig::from_env().unwrap();
        assert_eq!(config.input_container, "custom-input");
        assert_eq!(config.output_container, "custom-output");
        assert_eq!(config.processed_container, "processed");
        assert_eq!(config.error_container, "error");

        // Clean up
        env::remove_var("INPUT_PATH");
        env::remove_var("OUTPUT_PATH");
        env::remove_var("PROCESSED_PATH");
        env::remove_var("ERROR_PATH");
        env::remove_var("AZURE_BLOB_CONTAINER_INPUT");
        env::remove_var("AZURE_BLOB_CONTAINER_OUTPUT");
    }
}
