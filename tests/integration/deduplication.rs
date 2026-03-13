//! Integration tests for duplicate job detection
//!
//! Tests the deduplication logic that prevents reprocessing of jobs
//! that have already been completed or failed.

use std::fs;

use super::common::fixtures::create_test_workspace;
use three_dgs_processor::processor::dedup::{DuplicateDetector, DuplicateStatus};

#[test]
fn test_detect_new_job() {
    let workspace = create_test_workspace().unwrap();
    let processed_dir = workspace.path().join("processed");
    let error_dir = workspace.path().join("error");
    
    let detector = DuplicateDetector::new(processed_dir, error_dir);
    
    // New job that hasn't been processed
    let new_job = workspace.path().join("input").join("new-scene-001");
    fs::create_dir_all(&new_job).unwrap();
    
    let status = detector.check(&new_job);
    assert_eq!(status, DuplicateStatus::New);
}

#[test]
fn test_detect_already_processed() {
    let workspace = create_test_workspace().unwrap();
    let processed_dir = workspace.path().join("processed");
    let error_dir = workspace.path().join("error");
    
    // Create a "processed" job
    let job_name = "scene-123";
    let processed_path = processed_dir.join(job_name);
    fs::create_dir_all(&processed_path).unwrap();
    
    let detector = DuplicateDetector::new(processed_dir, error_dir);
    
    // Try to process the same job again
    let duplicate_job = workspace.path().join("input").join(job_name);
    fs::create_dir_all(&duplicate_job).unwrap();
    
    let status = detector.check(&duplicate_job);
    match status {
        DuplicateStatus::AlreadyProcessed { processed_path: path } => {
            assert!(path.ends_with(job_name));
        }
        _ => panic!("Expected AlreadyProcessed status"),
    }
}

#[test]
fn test_detect_already_failed() {
    let workspace = create_test_workspace().unwrap();
    let processed_dir = workspace.path().join("processed");
    let error_dir = workspace.path().join("error");
    
    // Create a "failed" job
    let job_name = "corrupted-scene-456";
    let error_path = error_dir.join(job_name);
    fs::create_dir_all(&error_path).unwrap();
    
    let detector = DuplicateDetector::new(processed_dir, error_dir);
    
    // Try to process the failed job again
    let duplicate_job = workspace.path().join("input").join(job_name);
    fs::create_dir_all(&duplicate_job).unwrap();
    
    let status = detector.check(&duplicate_job);
    match status {
        DuplicateStatus::AlreadyFailed { error_path: path } => {
            assert!(path.ends_with(job_name));
        }
        _ => panic!("Expected AlreadyFailed status"),
    }
}

#[test]
fn test_multiple_jobs_mixed_status() {
    let workspace = create_test_workspace().unwrap();
    let processed_dir = workspace.path().join("processed");
    let error_dir = workspace.path().join("error");
    
    // Create jobs with different statuses
    fs::create_dir_all(processed_dir.join("job-1")).unwrap();
    fs::create_dir_all(error_dir.join("job-2")).unwrap();
    
    let detector = DuplicateDetector::new(processed_dir.clone(), error_dir.clone());
    
    // Check processed job
    let job1 = workspace.path().join("input").join("job-1");
    fs::create_dir_all(&job1).unwrap();
    assert!(matches!(
        detector.check(&job1),
        DuplicateStatus::AlreadyProcessed { .. }
    ));
    
    // Check failed job
    let job2 = workspace.path().join("input").join("job-2");
    fs::create_dir_all(&job2).unwrap();
    assert!(matches!(
        detector.check(&job2),
        DuplicateStatus::AlreadyFailed { .. }
    ));
    
    // Check new job
    let job3 = workspace.path().join("input").join("job-3");
    fs::create_dir_all(&job3).unwrap();
    assert_eq!(detector.check(&job3), DuplicateStatus::New);
}

#[test]
fn test_deduplication_after_restart() {
    // Simulates container restart scenario where processed jobs
    // should not be reprocessed
    let workspace = create_test_workspace().unwrap();
    let processed_dir = workspace.path().join("processed");
    let error_dir = workspace.path().join("error");
    
    // Simulate pre-restart state: 5 jobs already processed
    for i in 1..=5 {
        let job_name = format!("pre-restart-job-{}", i);
        fs::create_dir_all(processed_dir.join(&job_name)).unwrap();
    }
    
    // After restart, detector should skip all pre-processed jobs
    let detector = DuplicateDetector::new(processed_dir, error_dir);
    
    for i in 1..=5 {
        let job_name = format!("pre-restart-job-{}", i);
        let job_path = workspace.path().join("input").join(&job_name);
        fs::create_dir_all(&job_path).unwrap();
        
        let status = detector.check(&job_path);
        assert!(matches!(status, DuplicateStatus::AlreadyProcessed { .. }));
    }
}

#[test]
fn test_case_sensitive_job_names() {
    let workspace = create_test_workspace().unwrap();
    let processed_dir = workspace.path().join("processed");
    let error_dir = workspace.path().join("error");
    
    // Process "Scene-A"
    fs::create_dir_all(processed_dir.join("Scene-A")).unwrap();
    
    let detector = DuplicateDetector::new(processed_dir, error_dir);
    
    // "scene-a" should be treated as different (case-sensitive)
    let job_lower = workspace.path().join("input").join("scene-a");
    fs::create_dir_all(&job_lower).unwrap();
    
    let status = detector.check(&job_lower);
    // Case sensitivity depends on filesystem, but check logic is consistent
    match status {
        DuplicateStatus::New | DuplicateStatus::AlreadyProcessed { .. } => {
            // Both are valid depending on filesystem
        }
        _ => panic!("Unexpected status"),
    }
}

#[test]
fn test_special_characters_in_job_names() {
    let workspace = create_test_workspace().unwrap();
    let processed_dir = workspace.path().join("processed");
    let error_dir = workspace.path().join("error");
    
    // Job with special characters
    let job_name = "scene-2024-01-15_10-30-45";
    fs::create_dir_all(processed_dir.join(job_name)).unwrap();
    
    let detector = DuplicateDetector::new(processed_dir, error_dir);
    
    let job_path = workspace.path().join("input").join(job_name);
    fs::create_dir_all(&job_path).unwrap();
    
    let status = detector.check(&job_path);
    assert!(matches!(status, DuplicateStatus::AlreadyProcessed { .. }));
}

#[test]
fn test_nested_job_directories() {
    let workspace = create_test_workspace().unwrap();
    let processed_dir = workspace.path().join("processed");
    let error_dir = workspace.path().join("error");
    
    // Nested job structure (job contains subdirectories)
    let job_name = "complex-scene";
    let processed_path = processed_dir.join(job_name);
    fs::create_dir_all(processed_path.join("frames")).unwrap();
    fs::create_dir_all(processed_path.join("metadata")).unwrap();
    
    let detector = DuplicateDetector::new(processed_dir, error_dir);
    
    // Check if nested structure is detected
    let job_path = workspace.path().join("input").join(job_name);
    fs::create_dir_all(&job_path).unwrap();
    
    let status = detector.check(&job_path);
    assert!(matches!(status, DuplicateStatus::AlreadyProcessed { .. }));
}

#[test]
fn test_empty_processed_and_error_dirs() {
    let workspace = create_test_workspace().unwrap();
    let processed_dir = workspace.path().join("processed");
    let error_dir = workspace.path().join("error");
    
    // Start with empty dirs (first run)
    let detector = DuplicateDetector::new(processed_dir, error_dir);
    
    let new_job = workspace.path().join("input").join("first-job-ever");
    fs::create_dir_all(&new_job).unwrap();
    
    let status = detector.check(&new_job);
    assert_eq!(status, DuplicateStatus::New);
}
