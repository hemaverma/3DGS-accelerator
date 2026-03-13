//! E2E test utilities for Docker container orchestration
//!
//! Provides helpers for:
//! - Starting/stopping Docker containers
//! - Managing test volumes
//! - Copying test data
//! - Waiting for processing completion
//! - Azurite emulator management

#![allow(dead_code)] // E2E test helpers may not all be used yet

use anyhow::{anyhow, Result};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;
use tempfile::TempDir;
use tokio::fs;
use tokio::process::Command;
use tokio::time::{sleep, timeout};

/// Docker container handle
pub struct DockerContainer {
    pub container_id: String,
    pub name: String,
    pub health_port: Option<u16>,
}

/// Test volume paths
pub struct TestVolumes {
    pub _temp_dir: TempDir, // Keep alive for cleanup
    pub base_path: PathBuf,
    pub input_dir: PathBuf,
    pub output_dir: PathBuf,
    pub processed_dir: PathBuf,
    pub error_dir: PathBuf,
    pub config_dir: PathBuf,
}

/// Check if Docker is available and running
pub async fn check_docker_available() -> Result<()> {
    let output = Command::new("docker")
        .arg("version")
        .output()
        .await
        .map_err(|e| anyhow!("Docker not found: {}", e))?;

    if !output.status.success() {
        return Err(anyhow!("Docker is not running"));
    }

    Ok(())
}

/// Create test volumes directory structure
pub async fn create_docker_test_volumes() -> Result<TestVolumes> {
    let temp_dir = TempDir::new()?;
    let base_path = temp_dir.path().to_path_buf();

    let input_dir = base_path.join("input");
    let output_dir = base_path.join("output");
    let processed_dir = base_path.join("processed");
    let error_dir = base_path.join("error");
    let config_dir = base_path.join("config");

    // Create all directories
    for dir in [&input_dir, &output_dir, &processed_dir, &error_dir, &config_dir] {
        fs::create_dir_all(dir).await?;
    }

    // Create minimal config file
    let config_content = r#"
training:
  iterations: 100
  densification_interval: 100
  densify_until_iter: 50
  
export:
  formats:
    - ply
    - splat
"#;
    fs::write(config_dir.join("config.yaml"), config_content).await?;

    Ok(TestVolumes {
        _temp_dir: temp_dir,
        base_path,
        input_dir,
        output_dir,
        processed_dir,
        error_dir,
        config_dir,
    })
}

/// Build Docker image if not already built
pub async fn ensure_docker_image_built() -> Result<()> {
    // Check if image exists
    let check_output = Command::new("docker")
        .args(["images", "-q", "3dgs-processor:test"])
        .output()
        .await?;

    if check_output.stdout.is_empty() {
        // Image doesn't exist, build it
        eprintln!("📦 Building Docker image (this may take a few minutes)...");

        let build_output = Command::new("docker")
            .args([
                "build",
                "-t",
                "3dgs-processor:test",
                "-f",
                "Dockerfile",
                ".",
            ])
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .output()
            .await?;

        if !build_output.status.success() {
            return Err(anyhow!("Failed to build Docker image"));
        }

        eprintln!("✅ Docker image built successfully");
    }

    Ok(())
}

/// Find an available port on localhost
pub async fn find_available_port() -> Result<u16> {
    use std::net::TcpListener;
    
    // Let OS assign available port
    let listener = TcpListener::bind("127.0.0.1:0")
        .map_err(|e| anyhow!("Failed to find available port: {}", e))?;
    let port = listener.local_addr()?.port();
    drop(listener); // Release the port
    Ok(port)
}

/// Start Docker container with test volumes
pub async fn run_docker_container(
    volumes: &TestVolumes,
    azure_mode: bool,
) -> Result<DockerContainer> {
    run_docker_container_with_port(volumes, azure_mode, None).await
}

/// Start Docker container with optional health check port mapping
pub async fn run_docker_container_with_port(
    volumes: &TestVolumes,
    azure_mode: bool,
    health_port: Option<u16>,
) -> Result<DockerContainer> {
    // Ensure image is built
    ensure_docker_image_built().await?;

    let container_name = format!("3dgs-e2e-test-{}", uuid::Uuid::new_v4());

    let mut args = vec![
        "run",
        "-d",
        "--name",
        &container_name,
        "--rm",
    ];

    // Mount volumes
    let input_mount = format!("{}:/input", volumes.input_dir.display());
    let output_mount = format!("{}:/output", volumes.output_dir.display());
    let processed_mount = format!("{}:/processed", volumes.processed_dir.display());
    let error_mount = format!("{}:/error", volumes.error_dir.display());
    let config_mount = format!("{}:/config", volumes.config_dir.display());

    args.extend(&[
        "-v", &input_mount,
        "-v", &output_mount,
        "-v", &processed_mount,
        "-v", &error_mount,
        "-v", &config_mount,
    ]);

    // Environment variables
    let mut env_vars = vec![
        "-e", "INPUT_PATH=/input",
        "-e", "OUTPUT_PATH=/output",
        "-e", "PROCESSED_PATH=/processed",
        "-e", "ERROR_PATH=/error",
        "-e", "CONFIG_PATH=/config/config.yaml",
        "-e", "BACKEND=mock", // Use mock backend for tests
        "-e", "LOG_LEVEL=debug",
        "-e", "UPLOAD_STABILITY_TIMEOUT_SECS=5",
        "-e", "MAX_RETRIES=2",
        "-e", "POLL_INTERVAL_SECS=2",
    ];

    // Only enable health check if port mapping requested
    if health_port.is_some() {
        env_vars.extend(&["-e", "HEALTH_CHECK_ENABLED=true"]);
    }

    args.extend(env_vars);

    // Azure mode configuration
    // NOTE: the connection string is not a secret 
    // the well-known emulator keys, and is required for blobfuse2 to work in the container with Azurite
    if azure_mode {
        args.extend(&[
            "--privileged", // Required for blobfuse2
            "-e",
            "AZURE_STORAGE_CONNECTION_STRING=DefaultEndpointsProtocol=http;AccountName=devstoreaccount1;AccountKey=Eby8vdM02xNOcqFlqUwJPLlmEtlCDXJ1OUzFT50uSRZ6IFsuFq2UVErCz4I6tq/K1SZFPTOtr/KBHBeksoGMGw==;BlobEndpoint=http://host.docker.internal:10000/devstoreaccount1;",
            "-e",
            "AZURE_ENABLED=true",
        ]);
    }

    // Health check port (only if requested)
    let port_mapping: String;
    if let Some(port) = health_port {
        port_mapping = format!("{}:8080", port);
        args.extend(&["-p", &port_mapping]);
    }

    // Image name
    args.push("3dgs-processor:test");

    let output = Command::new("docker")
        .args(&args)
        .output()
        .await?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("Failed to start container: {}", stderr));
    }

    let container_id = String::from_utf8(output.stdout)?
        .trim()
        .to_string();

    Ok(DockerContainer {
        container_id,
        name: container_name,
        health_port,
    })
}

/// Stop and remove Docker container
pub async fn stop_docker_container(container: &DockerContainer) -> Result<()> {
    // Container is started with --rm, so stopping will automatically remove it
    let output = Command::new("docker")
        .args(["stop", &container.container_id])
        .output()
        .await?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("Failed to stop container: {}", stderr));
    }

    Ok(())
}

/// Get container logs
pub async fn get_container_logs(container: &DockerContainer) -> Result<String> {
    let output = Command::new("docker")
        .args(["logs", &container.container_id])
        .output()
        .await?;

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Copy test videos to a directory
pub fn copy_testdata_videos(dest_dir: &Path, video_files: &[&str]) -> Result<()> {
    let testdata_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("testdata")
        .join("sample_scene");

    for video_file in video_files {
        let src = testdata_dir.join(video_file);
        let dst = dest_dir.join(video_file);

        if !src.exists() {
            return Err(anyhow!(
                "Test video not found: {}. Run ./scripts/generate-test-videos.sh first",
                src.display()
            ));
        }

        std::fs::copy(&src, &dst)?;
    }

    Ok(())
}

/// Wait for a file to be created
pub async fn wait_for_file_creation(
    file_path: &Path,
    timeout_duration: Duration,
) -> Result<()> {
    timeout(timeout_duration, async {
        loop {
            if file_path.exists() {
                return Ok(());
            }
            sleep(Duration::from_millis(500)).await;
        }
    })
    .await?
}

/// Wait for processing to complete (job moved to processed folder)
pub async fn wait_for_processing_complete(
    processed_dir: &Path,
    job_name: &str,
    timeout_duration: Duration,
) -> Result<()> {
    let job_path = processed_dir.join(job_name);
    wait_for_file_creation(&job_path, timeout_duration).await
}

/// Verify output files exist and are valid
pub fn verify_output_files(output_dir: &Path) -> Result<()> {
    let entries = std::fs::read_dir(output_dir)?;

    let mut ply_found = false;
    let mut splat_found = false;

    for entry in entries {
        let entry = entry?;
        let path = entry.path();

        if let Some(ext) = path.extension() {
            match ext.to_str() {
                Some("ply") => {
                    ply_found = true;
                    // Verify PLY is not empty
                    let metadata = std::fs::metadata(&path)?;
                    if metadata.len() == 0 {
                        return Err(anyhow!("PLY file is empty: {}", path.display()));
                    }
                }
                Some("splat") => {
                    splat_found = true;
                    // Verify SPLAT is not empty
                    let metadata = std::fs::metadata(&path)?;
                    if metadata.len() == 0 {
                        return Err(anyhow!("SPLAT file is empty: {}", path.display()));
                    }
                }
                _ => {}
            }
        }
    }

    if !ply_found {
        return Err(anyhow!("No .ply file found in output"));
    }

    if !splat_found {
        return Err(anyhow!("No .splat file found in output"));
    }

    Ok(())
}

/// Start Azurite emulator container
pub async fn start_azurite() -> Result<DockerContainer> {
    let container_name = format!("azurite-test-{}", uuid::Uuid::new_v4());

    let args = vec![
        "run",
        "-d",
        "--name",
        &container_name,
        "--rm",
        "-p",
        "10000:10000",
        "-p",
        "10001:10001",
        "mcr.microsoft.com/azure-storage/azurite:latest",
        "azurite-blob",
        "--blobHost",
        "0.0.0.0",
    ];

    let output = Command::new("docker")
        .args(&args)
        .output()
        .await?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("Failed to start Azurite: {}", stderr));
    }

    let container_id = String::from_utf8(output.stdout)?
        .trim()
        .to_string();

    Ok(DockerContainer {
        container_id,
        name: container_name,
        health_port: None, // Azurite doesn't use health port
    })
}

/// Wait for Azurite to be ready
pub async fn wait_for_azurite_ready(timeout_duration: Duration) -> Result<()> {
    timeout(timeout_duration, async {
        loop {
            // Try to connect to Azurite health endpoint
            if reqwest::get("http://localhost:10000")
                .await
                .is_ok()
            {
                return Ok(());
            }
            sleep(Duration::from_millis(500)).await;
        }
    })
    .await?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_create_test_volumes() {
        let volumes = create_docker_test_volumes().await.unwrap();

        assert!(volumes.input_dir.exists());
        assert!(volumes.output_dir.exists());
        assert!(volumes.processed_dir.exists());
        assert!(volumes.error_dir.exists());
        assert!(volumes.config_dir.exists());
        assert!(volumes.config_dir.join("config.yaml").exists());
    }

    #[test]
    fn test_copy_testdata_videos_error_handling() {
        let temp = TempDir::new().unwrap();
        let result = copy_testdata_videos(temp.path(), &["nonexistent.mp4"]);

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Test video not found"));
    }
}
