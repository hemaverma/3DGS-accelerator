//! Batch mode processor for Azure Blob Storage SDK
//!
//! Downloads input videos from Azure Blob Storage, runs the full processing pipeline
//! locally, uploads outputs back to blob storage, and exits.
//!
//! Flow:
//! 1. Download input blobs (BATCH_INPUT_PREFIX) from input container to local temp dir
//! 2. Run existing pipeline: validation → ffmpeg → COLMAP → training → export
//! 3. Upload PLY, SPLAT, manifest to output container
//! 4. Move input blobs to processed (or error) container
//! 5. Exit with code 0 (success) or 1 (failure)

use anyhow::{Context, Result};
use std::path::Path;
use std::time::Instant;
use tracing::{error, info, instrument};

use crate::azure::BlobStorageClient;
use crate::config::{BatchConfig, Config};
use crate::health::HealthCheckState;
use crate::processor::{execute_job, JobExecutionParams, JobStatus};
use crate::shutdown::ShutdownFlag;
use crate::validation::ValidationConfig;

/// Run a single batch job: download → process → upload → exit
///
/// Returns `Ok(())` on success, `Err` on failure.
/// The caller should exit with code 0 or 1 accordingly.
#[instrument(skip_all, fields(prefix = %batch_config.input_prefix))]
pub async fn run_batch_job(
    config: &Config,
    batch_config: &BatchConfig,
    health_state: &HealthCheckState,
    _shutdown_flag: &ShutdownFlag,
) -> Result<()> {
    let start_time = Instant::now();

    let job_id = batch_config
        .job_id
        .clone()
        .unwrap_or_else(|| {
            // Derive job ID from prefix (strip trailing slash)
            batch_config
                .input_prefix
                .trim_end_matches('/')
                .rsplit('/')
                .next()
                .unwrap_or(&batch_config.input_prefix)
                .to_string()
        });

    info!(
        job_id = %job_id,
        prefix = %batch_config.input_prefix,
        "Starting batch job"
    );

    // Step 1: Initialize Azure Blob Storage SDK client
    let blob_client = BlobStorageClient::from_env()
        .context("Failed to initialize Azure Blob Storage SDK client")?;

    // Step 2: Download input blobs to local temp directory
    let input_dir = config.temp_path.join("batch-input").join(&job_id);
    tokio::fs::create_dir_all(&input_dir)
        .await
        .with_context(|| format!("Failed to create input dir: {}", input_dir.display()))?;

    info!(
        container = %blob_client.container_input,
        prefix = %batch_config.input_prefix,
        local_dir = %input_dir.display(),
        "Downloading input blobs"
    );

    let downloaded = blob_client
        .download_prefix(
            &blob_client.container_input,
            &batch_config.input_prefix,
            &input_dir,
        )
        .await
        .context("Failed to download input blobs from Azure Blob Storage")?;

    info!(
        count = downloaded.len(),
        "Downloaded input files, starting pipeline"
    );

    // Step 3: Create output directory
    let output_dir = config.output_path.join(&job_id);
    tokio::fs::create_dir_all(&output_dir)
        .await
        .with_context(|| format!("Failed to create output dir: {}", output_dir.display()))?;

    // Step 4: Run the existing processing pipeline
    let training_config = config.training.training.clone();
    let params = JobExecutionParams {
        job_id: job_id.clone(),
        input_folder: input_dir.clone(),
        output_folder: output_dir.clone(),
        temp_folder: config.temp_path.clone(),
        training_config,
        backend_name: Some(config.backend.clone()),
        validation_config: ValidationConfig::from_env(),
    };

    let result = execute_job(params, Some(health_state)).await;

    let duration_secs = start_time.elapsed().as_secs_f64();

    match result.status {
        JobStatus::Success => {
            info!(
                job_id = %job_id,
                duration_secs = duration_secs,
                "Pipeline completed successfully, uploading outputs"
            );

            // Step 5: Upload outputs to blob storage
            let output_prefix = format!("{}/", job_id);
            if let Err(e) = upload_outputs(&blob_client, &output_dir, &output_prefix).await {
                error!(error = %e, "Failed to upload outputs to blob storage");
                move_to_error(&blob_client, &batch_config.input_prefix).await;
                anyhow::bail!("Output upload failed: {}", e);
            }

            // Step 6: Move input blobs to processed container
            info!("Moving input blobs to processed container");
            if let Err(e) = blob_client
                .move_blobs(
                    &blob_client.container_input,
                    &batch_config.input_prefix,
                    &blob_client.container_processed,
                )
                .await
            {
                error!(error = %e, "Failed to move input blobs to processed (non-fatal)");
            }

            // Step 7: Clean up local temp files
            cleanup_local(&input_dir, &output_dir).await;

            info!(
                job_id = %job_id,
                duration_secs = duration_secs,
                "Batch job completed successfully"
            );
            Ok(())
        }
        JobStatus::Failed => {
            let err_msg = result.error.unwrap_or_else(|| "Unknown error".to_string());
            error!(
                job_id = %job_id,
                duration_secs = duration_secs,
                error = %err_msg,
                "Pipeline failed"
            );

            // Move input blobs to error container
            move_to_error(&blob_client, &batch_config.input_prefix).await;

            // Clean up local temp files
            cleanup_local(&input_dir, &output_dir).await;

            anyhow::bail!("Batch job failed: {}", err_msg)
        }
    }
}

/// Upload output files (PLY, SPLAT, manifest) to the output blob container
async fn upload_outputs(
    blob_client: &BlobStorageClient,
    output_dir: &Path,
    prefix: &str,
) -> Result<()> {
    let uploaded = blob_client
        .upload_directory(&blob_client.container_output, prefix, output_dir)
        .await
        .context("Failed to upload output directory")?;

    info!(count = uploaded.len(), "Uploaded output files to blob storage");
    Ok(())
}

/// Move input blobs to the error container (best-effort)
async fn move_to_error(blob_client: &BlobStorageClient, prefix: &str) {
    info!("Moving input blobs to error container");
    if let Err(e) = blob_client
        .move_blobs(
            &blob_client.container_input,
            prefix,
            &blob_client.container_error,
        )
        .await
    {
        error!(error = %e, "Failed to move input blobs to error container");
    }
}

/// Clean up local temporary directories (best-effort)
async fn cleanup_local(input_dir: &Path, output_dir: &Path) {
    for dir in [input_dir, output_dir] {
        if dir.exists() {
            if let Err(e) = tokio::fs::remove_dir_all(dir).await {
                error!(
                    dir = %dir.display(),
                    error = %e,
                    "Failed to clean up temp directory"
                );
            }
        }
    }
}
