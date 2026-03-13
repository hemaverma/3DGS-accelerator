use anyhow::Result;
use std::time::Duration;
use three_dgs_processor::{
    azure::{mount_all_containers, unmount_all_containers, AzureAuth, MountConfig},
    cleanup::start_retention_scheduler,
    config::{Config, RunMode},
    health,
    logging,
    processor::{
        cleanup::{move_to_error, move_to_processed},
        dedup::{DuplicateDetector, DuplicateStatus},
        execute_with_retry, run_batch_job, JobExecutionParams, JobQueue, JobStatus, QueuedJob,
        RetryConfig,
    },
    shutdown::ShutdownCoordinator,
    storage::check_disk_space,
    validation::ValidationConfig,
    watcher::{detect_new_folder, wait_for_stability},
};
use tracing::{error, info, warn};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging first
    logging::init()?;

    info!("Starting 3DGS Video Processor");

    // Load configuration
    let config = Config::from_env()?;
    let current_path = std::env::current_dir()?;
    let current_env_path = std::env::var("PATH").unwrap_or_else(|_| "None".to_string());
    info!(
        input_path = %config.input_path.display(),
        output_path = %config.output_path.display(),
        backend = %config.backend,
        upload_stability_timeout_secs = config.upload_stability_timeout_secs,
        max_retries = config.max_retries,
        "Configuration loaded"
    );

    info!("Current working directory: {}", current_path.display());
    info!("Current PATH environment variable: {}", current_env_path);

    // Azure Blob Storage mounting (if configured AND in watch mode)
    // Batch mode uses the SDK directly, no mounts needed
    let mount_config = if config.azure.is_some() && config.run_mode == RunMode::Watch {
        info!("Azure configuration detected, initializing Blob Storage mounts");
        // Initialize Azure authentication
        let azure_auth = AzureAuth::from_env()?;
        
        // Load mount configuration
        let mount_cfg = MountConfig::from_env()?;
        
        // Mount all containers
        mount_all_containers(&azure_auth, &mount_cfg).await?;
        
        info!("Azure Blob Storage mounts initialized successfully");
        Some(mount_cfg)
    } else if config.run_mode == RunMode::Batch {
        info!("Batch mode: using Azure Blob Storage SDK (no FUSE mounts)");
        None
    } else {
        info!("No Azure configuration detected, using local filesystem");
        None
    };

    // ── Batch mode: download → process → upload → exit ──
    if config.run_mode == RunMode::Batch {
        let batch_config = config.batch.as_ref().expect("batch config required in batch mode");
        info!(
            prefix = %batch_config.input_prefix,
            "Running in BATCH mode"
        );

        // Create temp and output directories for batch
        std::fs::create_dir_all(&config.temp_path)?;
        std::fs::create_dir_all(&config.output_path)?;

        // Start health check server (optional, useful for liveness probes)
        let (_health_handle, health_state) = health::start_health_server(&config).await?;
        health_state
            .update_state(health::ProcessorState::Processing)
            .await;

        let shutdown_coordinator = ShutdownCoordinator::new();
        let shutdown_flag = shutdown_coordinator.shutdown_flag();

        let result = run_batch_job(&config, batch_config, &health_state, &shutdown_flag).await;

        match result {
            Ok(()) => {
                info!("Batch job completed successfully");
                std::process::exit(0);
            }
            Err(e) => {
                error!(error = %e, "Batch job failed");
                std::process::exit(1);
            }
        }
    }

    // ── Watch mode: existing continuous service behavior ──
    info!("Running in WATCH mode");

    // Validate directories exist (or are properly mounted)
    config.validate_paths()?;

    // Initialize shutdown coordinator
    let shutdown_coordinator = ShutdownCoordinator::new();
    let shutdown_flag = shutdown_coordinator.shutdown_flag();

    // Start health check server (optional)
    let (health_handle, health_state) = health::start_health_server(&config).await?;

    // Set initial health state to watching
    health_state
        .update_state(health::ProcessorState::Watching)
        .await;

    // Spawn shutdown signal handler
    let shutdown_signal_task = {
        let coordinator = shutdown_coordinator.clone();
        tokio::spawn(async move {
            coordinator.wait_for_shutdown_signal().await;
        })
    };

    // Initialize duplicate detector
    let dedup = DuplicateDetector::new(config.processed_path.clone(), config.error_path.clone());

    // Initialize job queue
    let queue = JobQueue::new(100); // Allow up to 100 queued jobs
    let (queue_tx, queue_rx) = queue.split();

    // Spawn retention cleanup scheduler
    let cleanup_handle = {
        let processed_path = config.processed_path.clone();
        let error_path = config.error_path.clone();
        let retention_days = config.retention_days;
        start_retention_scheduler(processed_path, error_path, retention_days)
    };

    // Spawn watcher task
    let watcher_handle = {
        let config = config.clone();
        let queue_tx = queue_tx.clone();
        let shutdown_flag = shutdown_flag.clone();
        let dedup = dedup.clone();

        tokio::spawn(async move {
            run_watcher_loop(config, queue_tx, shutdown_flag, dedup).await;
        })
    };

    // Spawn processor task
    let processor_handle = {
        let config = config.clone();
        let shutdown_flag = shutdown_flag.clone();
        let health_state = health_state.clone();

        tokio::spawn(async move {
            run_processor_loop(config, queue_rx, shutdown_flag, health_state).await;
        })
    };

    // Wait for shutdown signal
    info!("Main event loop started, waiting for shutdown signal...");
    shutdown_coordinator.wait_for_shutdown().await;

    // Perform graceful shutdown
    let cleanup_result = shutdown_coordinator
        .shutdown_with_timeout(|| async {
            info!("Performing graceful shutdown...");

            // Update health state to indicate shutdown
            health_state
                .update_state(health::ProcessorState::Idle)
                .await;

            // Wait for tasks to complete (they will check shutdown_flag)
            info!("Waiting for watcher task to complete...");
            let _ = watcher_handle.await;

            info!("Waiting for processor task to complete...");
            let _ = processor_handle.await;

            info!("Stopping cleanup scheduler...");
            cleanup_handle.abort();

            // Unmount Azure Blob Storage if it was mounted
            if let Some(ref mount_cfg) = mount_config {
                info!("Unmounting Azure Blob Storage containers...");
                if let Err(e) = unmount_all_containers(mount_cfg).await {
                    warn!(error = %e, "Failed to unmount Azure containers during shutdown");
                } else {
                    info!("Azure Blob Storage unmounted successfully");
                }
            }

            info!("Graceful shutdown complete");
            Ok(())
        })
        .await;

    // Abort health server
    health_handle.abort();
    let _ = shutdown_signal_task.await;

    // Check cleanup result
    if let Err(e) = cleanup_result {
        error!(error = %e, "Shutdown cleanup encountered errors");
        std::process::exit(1);
    }

    info!("Shutdown complete");
    Ok(())
}

/// Watcher loop: detect new folders and enqueue jobs
///
/// This loop continuously watches for new folders in INPUT_PATH, waits for upload
/// stability, checks for duplicates, and enqueues new jobs for processing.
async fn run_watcher_loop(
    config: Config,
    queue_tx: three_dgs_processor::processor::JobQueueSender,
    shutdown_flag: three_dgs_processor::shutdown::ShutdownFlag,
    dedup: DuplicateDetector,
) {
    let poll_interval = Duration::from_secs(config.poll_interval_secs);
    let stability_timeout = Duration::from_secs(config.upload_stability_timeout_secs);
    let cancel = shutdown_flag.cancellation_token();

    loop {
        // Check for shutdown
        if shutdown_flag.is_shutdown_requested() {
            info!("Watcher loop received shutdown signal");
            break;
        }

        // Detect new folder (cancellable via shutdown token)
        let folder = match detect_new_folder(&config.input_path, poll_interval, cancel.clone())
            .await
        {
            Ok(f) => f,
            Err(e) => {
                if shutdown_flag.is_shutdown_requested() {
                    info!("Watcher loop shutting down");
                    break;
                }
                error!(error = %e, "Failed to detect new folder");
                tokio::time::sleep(Duration::from_secs(5)).await;
                continue;
            }
        };

        info!(folder = %folder.display(), "New folder detected");

        // Check for duplicates
        match dedup.check(&folder) {
            DuplicateStatus::AlreadyProcessed { processed_path } => {
                warn!(
                    folder = %folder.display(),
                    processed_path = %processed_path.display(),
                    "Folder already processed, skipping"
                );
                continue;
            }
            DuplicateStatus::AlreadyFailed { error_path } => {
                warn!(
                    folder = %folder.display(),
                    error_path = %error_path.display(),
                    "Folder previously failed, skipping"
                );
                continue;
            }
            DuplicateStatus::New => {
                // Proceed with processing
            }
        }

        // Wait for upload stability (cancellable via shutdown token)
        info!(
            folder = %folder.display(),
            timeout_secs = stability_timeout.as_secs(),
            "Waiting for upload stability"
        );

        if let Err(e) = wait_for_stability(&folder, stability_timeout, cancel.clone()).await {
            if shutdown_flag.is_shutdown_requested() {
                info!("Watcher loop shutting down during stability wait");
                break;
            }
            error!(
                folder = %folder.display(),
                error = %e,
                "Failed to wait for stability, skipping folder"
            );
            continue;
        }

        info!(folder = %folder.display(), "Upload stable, enqueuing job");

        // Enqueue job
        let job = QueuedJob::new(folder.clone());
        if let Err(e) = queue_tx.enqueue(job).await {
            error!(
                folder = %folder.display(),
                error = %e,
                "Failed to enqueue job"
            );
        }
    }

    info!("Watcher loop terminated");
}

/// Processor loop: dequeue and process jobs sequentially
///
/// This loop dequeues jobs from the queue and processes them one at a time with
/// retry logic. After processing, it moves folders to processed/ or error/ directories.
async fn run_processor_loop(
    config: Config,
    mut queue_rx: three_dgs_processor::processor::JobQueueReceiver,
    shutdown_flag: three_dgs_processor::shutdown::ShutdownFlag,
    health_state: health::HealthCheckState,
) {
    loop {
        // Check for shutdown before trying to dequeue
        if shutdown_flag.is_shutdown_requested() {
            info!("Processor loop received shutdown signal");
            break;
        }

        // Dequeue next job with timeout to allow checking shutdown
        let job = match tokio::time::timeout(Duration::from_secs(1), queue_rx.dequeue()).await {
            Ok(Some(job)) => job,
            Ok(None) => {
                info!("Job queue closed, processor loop terminating");
                break;
            }
            Err(_) => {
                // Timeout, loop back to check shutdown flag
                continue;
            }
        };

        info!(
            job_id = %job.job_id,
            folder = %job.folder_path.display(),
            "Dequeued job for processing"
        );

        // Check disk space before processing
        let min_free_space_gb = 10.0;
        match check_disk_space(&config.temp_path) {
            Ok(status) => {
                if !status.meets_requirement(min_free_space_gb) {
                    error!(
                        job_id = %job.job_id,
                        available_gb = status.available_gb(),
                        required_gb = min_free_space_gb,
                        "Insufficient disk space, moving job to error"
                    );
                    if let Err(e) = move_to_error(&job.folder_path, &config.error_path).await {
                        error!(error = %e, "Failed to move job to error folder");
                    }
                    continue;
                }
            }
            Err(e) => {
                warn!(error = %e, "Failed to check disk space, continuing anyway");
            }
        }

        // Update health state to processing
        health_state
            .update_state(health::ProcessorState::Processing)
            .await;

        // Get training config from the loaded configuration
        let training_config = config.training.training.clone();

        // Process job with retry logic (backend will be auto-selected if not specified)
        let params = JobExecutionParams {
            job_id: job.job_id.clone(),
            input_folder: job.folder_path.clone(),
            output_folder: config.output_path.clone(),
            temp_folder: config.temp_path.clone(),
            training_config,
            backend_name: Some(config.backend.clone()),
            validation_config: ValidationConfig::from_env(),
        };

        // Create retry configuration from environment variables
        let retry_config = RetryConfig::from_env();

        let result = execute_with_retry(params, retry_config, Some(&health_state), &shutdown_flag).await;

        // Handle job result
        match result.status {
            JobStatus::Success => {
                info!(
                    job_id = %job.job_id,
                    duration_secs = result.duration_secs,
                    "Job completed successfully, moving to processed folder"
                );

                if let Err(e) = move_to_processed(&job.folder_path, &config.processed_path).await {
                    error!(
                        job_id = %job.job_id,
                        error = %e,
                        "Failed to move job to processed folder"
                    );
                }
            }
            JobStatus::Failed => {
                error!(
                    job_id = %job.job_id,
                    duration_secs = result.duration_secs,
                    error = result.error.as_deref().unwrap_or("Unknown error"),
                    "Job failed after retries, moving to error folder"
                );

                if let Err(e) = move_to_error(&job.folder_path, &config.error_path).await {
                    error!(
                        job_id = %job.job_id,
                        error = %e,
                        "Failed to move job to error folder"
                    );
                }
            }
        }

        // Update health state back to watching
        health_state
            .update_state(health::ProcessorState::Watching)
            .await;
    }

    info!("Processor loop terminated");
}
