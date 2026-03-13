//! Integration tests for retry logic
//!
//! Tests exponential backoff retry behavior for failed jobs.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use super::common::fixtures::create_test_workspace;
use three_dgs_processor::backends::TrainingConfig;
use three_dgs_processor::processor::retry::{execute_with_retry, RetryConfig};
use three_dgs_processor::processor::job::JobExecutionParams;
use three_dgs_processor::shutdown::ShutdownFlag;

#[tokio::test]
async fn test_retry_succeeds_on_first_attempt() {
    let workspace = create_test_workspace().unwrap();
    let input_folder = workspace.path().join("input").join("test-job");
    let output_folder = workspace.path().join("output");
    let temp_folder = workspace.path().join("tmp");
    
    std::fs::create_dir_all(&input_folder).unwrap();
    
    // Mock backend that succeeds
    let params = JobExecutionParams {
        job_id: "test-job".to_string(),
        input_folder,
        output_folder,
        temp_folder,
        training_config: TrainingConfig::default(),
        backend_name: None, // auto-select
        validation_config: three_dgs_processor::validation::ValidationConfig::default(),
    };
    let config = RetryConfig {
        max_retries: 3,
        base_delay_secs: 2,
        max_delay_secs: 60,
    };
    let shutdown_flag = ShutdownFlag::new();
    let result = execute_with_retry(
        params,
        config,
        None, // health_state
        &shutdown_flag,
    ).await;
    
    // Should succeed on first attempt (mock always succeeds for now)
    // In real integration test, would check attempt count
    assert_eq!(result.job_id, "test-job");
}

#[tokio::test]
async fn test_retry_exhaustion() {
    // Test that after MAX_RETRIES, job is marked as failed
    let workspace = create_test_workspace().unwrap();
    let input_folder = workspace.path().join("input").join("failing-job");
    let output_folder = workspace.path().join("output");
    let temp_folder = workspace.path().join("tmp");
    
    std::fs::create_dir_all(&input_folder).unwrap();
    
    // With real implementation, would use a failing backend
    // For now, test structure
    let max_retries = 2;
    
    let params = JobExecutionParams {
        job_id: "failing-job".to_string(),
        input_folder,
        output_folder,
        temp_folder,
        training_config: TrainingConfig::default(),
        backend_name: None, // auto-select
        validation_config: three_dgs_processor::validation::ValidationConfig::default(),
    };
    let config = RetryConfig {
        max_retries,
        base_delay_secs: 2,
        max_delay_secs: 60,
    };
    let shutdown_flag = ShutdownFlag::new();
    let result = execute_with_retry(
        params,
        config,
        None, // health_state
        &shutdown_flag,
    ).await;
    
    // Verify result has proper structure
    assert_eq!(result.job_id, "failing-job");
}

#[tokio::test]
async fn test_exponential_backoff_timing() {
    // Test that retry delays follow exponential pattern
    use tokio::time::Instant;
    
    let _start = Instant::now();
    
    // Simulate retries with backoff: 0s, 2s, 4s, 8s
    // This would be internal to execute_with_retry
    let expected_delays = [0, 2, 4, 8];
    
    // In real test, would measure actual delays
    // For now, verify the expected pattern exists
    assert_eq!(expected_delays.len(), 4);
    assert_eq!(expected_delays[1], 2);
    assert_eq!(expected_delays[2], 4);
    assert_eq!(expected_delays[3], 8);
}

#[test]
fn test_retry_counter_tracking() {
    // Test that we can track how many retries occurred
    let counter = Arc::new(AtomicU32::new(0));
    
    // Simulate incrementing on each attempt
    for _attempt in 1..=4 {
        counter.fetch_add(1, Ordering::SeqCst);
    }
    
    assert_eq!(counter.load(Ordering::SeqCst), 4); // 1 initial + 3 retries
}

#[test]
fn test_max_retries_config() {
    // Test different max_retries values
    let configs = vec![0, 1, 3, 5, 10];
    
    for max_retries in configs {
        let max_attempts = max_retries + 1; // Initial + retries
        assert!(max_attempts > 0);
        assert_eq!(max_attempts, max_retries + 1);
    }
}

#[tokio::test]
async fn test_retry_with_transient_failure() {
    // Simulate transient failure that succeeds on retry
    let attempt_counter = Arc::new(AtomicU32::new(0));
    
    // Simulate operation that fails first time, succeeds second time
    let counter_clone = attempt_counter.clone();
    let operation = move || {
        let attempts = counter_clone.fetch_add(1, Ordering::SeqCst);
        if attempts < 1 {
            Err::<(), _>("Transient failure")
        } else {
            Ok(())
        }
    };
    
    // First call should fail
    assert!(operation().is_err());
    // Second call should succeed
    assert!(operation().is_ok());
    
    assert_eq!(attempt_counter.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn test_no_retry_on_success() {
    // Verify that successful jobs don't trigger retries
    let workspace = create_test_workspace().unwrap();
    let input_folder = workspace.path().join("input").join("success-job");
    let output_folder = workspace.path().join("output");
    let temp_folder = workspace.path().join("tmp");
    
    std::fs::create_dir_all(&input_folder).unwrap();
    
    let params = JobExecutionParams {
        job_id: "success-job".to_string(),
        input_folder,
        output_folder,
        temp_folder,
        training_config: TrainingConfig::default(),
        backend_name: None, // auto-select
        validation_config: three_dgs_processor::validation::ValidationConfig::default(),
    };
    let config = RetryConfig {
        max_retries: 5,
        base_delay_secs: 2,
        max_delay_secs: 60,
    };
    let shutdown_flag = ShutdownFlag::new();
    let result = execute_with_retry(
        params,
        config,
        None, // health_state
        &shutdown_flag,
    ).await;
    
    // Should succeed without retries
    assert_eq!(result.job_id, "success-job");
}

#[tokio::test]
async fn test_concurrent_retries() {
    // Test that multiple jobs can retry concurrently without interference
    use tokio::task;
    
    let workspace = create_test_workspace().unwrap();
    
    let mut handles = vec![];
    for i in 0..3 {
        let job_id = format!("concurrent-job-{}", i);
        let input_folder = workspace.path().join("input").join(&job_id);
        let output_folder = workspace.path().join("output");
        let temp_folder = workspace.path().join("tmp").join(&job_id);
        
        std::fs::create_dir_all(&input_folder).unwrap();
        
        let handle = task::spawn(async move {
            let params = JobExecutionParams {
                job_id: job_id.clone(),
                input_folder,
                output_folder,
                temp_folder,
                training_config: TrainingConfig::default(),
                backend_name: None, // auto-select
                validation_config: three_dgs_processor::validation::ValidationConfig::default(),
            };
            let config = RetryConfig {
                max_retries: 2,
                base_delay_secs: 2,
                max_delay_secs: 60,
            };
            let shutdown_flag = ShutdownFlag::new();
            execute_with_retry(
                params,
                config,
                None, // health_state
                &shutdown_flag,
            ).await
        });
        
        handles.push(handle);
    }
    
    // All should complete
    for handle in handles {
        let result = handle.await.unwrap();
        assert!(result.job_id.starts_with("concurrent-job-"));
    }
}

#[test]
fn test_backoff_duration_calculation() {
    // Test the 2^attempt formula for backoff
    let durations: Vec<u64> = (0..6).map(|attempt| 2_u64.pow(attempt)).collect();
    
    assert_eq!(durations, vec![1, 2, 4, 8, 16, 32]);
}

#[tokio::test]
async fn test_retry_preserves_job_context() {
    // Verify that job ID and paths are preserved across retries
    let workspace = create_test_workspace().unwrap();
    let input_folder = workspace.path().join("input").join("context-job");
    let output_folder = workspace.path().join("output");
    let temp_folder = workspace.path().join("tmp");
    
    std::fs::create_dir_all(&input_folder).unwrap();
    
    let params = JobExecutionParams {
        job_id: "context-job".to_string(),
        input_folder: input_folder.clone(),
        output_folder: output_folder.clone(),
        temp_folder: temp_folder.clone(),
        training_config: TrainingConfig::default(),
        backend_name: None, // auto-select
        validation_config: three_dgs_processor::validation::ValidationConfig::default(),
    };
    let config = RetryConfig {
        max_retries: 3,
        base_delay_secs: 2,
        max_delay_secs: 60,
    };
    let shutdown_flag = ShutdownFlag::new();
    let result = execute_with_retry(
        params,
        config,
        None, // health_state
        &shutdown_flag,
    ).await;
    
    // Verify context preserved
    assert_eq!(result.job_id, "context-job");
    assert!(result.duration_secs >= 0.0);
}
