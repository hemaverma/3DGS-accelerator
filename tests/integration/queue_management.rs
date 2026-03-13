//! Integration tests for job queue management  
//!
//! Tests the FIFO job queue for sequential processing.

use std::path::PathBuf;
use std::time::Duration;
use tokio::time::timeout;

use three_dgs_processor::processor::queue::{JobQueue, QueuedJob};

#[tokio::test]
async fn test_basic_enqueue_dequeue() {
    let queue = JobQueue::new(10);
    let (tx, mut rx) = queue.split();
    
    // Enqueue a job
    let job = QueuedJob::new(PathBuf::from("/input/scene1"));
    tx.enqueue(job.clone()).await.unwrap();
    
    // Dequeue the job
    let dequeued = rx.dequeue().await.unwrap();
    assert_eq!(dequeued.job_id, job.job_id);
}

#[tokio::test]
async fn test_fifo_ordering() {
    let queue = JobQueue::new(10);
    let (tx, mut rx) = queue.split();
    
    // Enqueue multiple jobs
    let jobs = vec![
        QueuedJob::with_id(PathBuf::from("/input/job1"), "job-1".to_string()),
        QueuedJob::with_id(PathBuf::from("/input/job2"), "job-2".to_string()),
        QueuedJob::with_id(PathBuf::from("/input/job3"), "job-3".to_string()),
    ];
    
    for job in &jobs {
        tx.enqueue(job.clone()).await.unwrap();
    }
    
    // Dequeue in FIFO order
    for expected_job in &jobs {
        let dequeued = rx.dequeue().await.unwrap();
        assert_eq!(dequeued.job_id, expected_job.job_id);
    }
}

#[tokio::test]
async fn test_empty_queue() {
    let queue = JobQueue::new(10);
    let (_tx, mut rx) = queue.split();
    
    // Dequeue from empty queue should block (or return None if channel closed)
    // Test with timeout to avoid hanging
    let result = timeout(Duration::from_millis(100), rx.dequeue()).await;
    assert!(result.is_err()); // Timeout indicates blocking behavior
}

#[tokio::test]
async fn test_queue_capacity() {
    let capacity = 5;
    let queue = JobQueue::new(capacity);
    let (tx, mut rx) = queue.split();
    
    // Fill queue to capacity
    for i in 0..capacity {
        let job = QueuedJob::with_id(
            PathBuf::from(format!("/input/job{}", i)),
            format!("job-{}", i),
        );
        tx.enqueue(job).await.unwrap();
    }
    
    // Verify we can dequeue
    let first = rx.dequeue().await.unwrap();
    assert_eq!(first.job_id, "job-0");
}

#[tokio::test]
async fn test_concurrent_producers() {
    use tokio::task;
    
    let queue = JobQueue::new(100);
    let (tx, mut rx) = queue.split();
    
    // Spawn multiple producers
    let mut handles = vec![];
    for producer_id in 0..5 {
        let tx_clone = tx.clone();
        let handle = task::spawn(async move {
            for job_num in 0..10 {
                let job = QueuedJob::with_id(
                    PathBuf::from(format!("/input/p{}_j{}", producer_id, job_num)),
                    format!("p{}-job{}", producer_id, job_num),
                );
                tx_clone.enqueue(job).await.unwrap();
            }
        });
        handles.push(handle);
    }
    
    // Wait for all producers to finish
    for handle in handles {
        handle.await.unwrap();
    }
    
    // Dequeue all jobs (should be 50 total)
    let mut _count = 0;
    let rx_handle = task::spawn(async move {
        let mut jobs = vec![];
        while let Some(job) = timeout(Duration::from_millis(500), rx.dequeue()).await.ok().flatten() {
            jobs.push(job);
            if jobs.len() == 50 {
                break;
            }
        }
        jobs
    });
    
    let jobs = rx_handle.await.unwrap();
    assert_eq!(jobs.len(), 50);
}

#[tokio::test]
async fn test_job_id_generation() {
    // Test automatic job ID generation from folder name
    let job1 = QueuedJob::new(PathBuf::from("/input/scene-2024-01-15"));
    assert_eq!(job1.job_id, "scene-2024-01-15");
    
    let job2 = QueuedJob::new(PathBuf::from("/input/complex/nested/path/myscene"));
    assert_eq!(job2.job_id, "myscene");
    
    // Test custom job ID
    let job3 = QueuedJob::with_id(
        PathBuf::from("/input/scene"),
        "custom-id-123".to_string(),
    );
    assert_eq!(job3.job_id, "custom-id-123");
}

#[tokio::test]
async fn test_queue_drop_behavior() {
    let queue = JobQueue::new(10);
    let (tx, mut rx) = queue.split();
    
    // Enqueue a job
    tx.enqueue(QueuedJob::new(PathBuf::from("/input/test"))).await.unwrap();
    
    // Drop the sender
    drop(tx);
    
    // Should still be able to dequeue existing job
    let job = rx.dequeue().await.unwrap();
    assert_eq!(job.job_id, "test");
    
    // Next dequeue should return None (channel closed)
    let empty = rx.dequeue().await;
    assert!(empty.is_none());
}

#[tokio::test]
async fn test_sequential_processing_pattern() {
    // Simulate the main processing loop pattern
    let queue = JobQueue::new(10);
    let (tx, mut rx) = queue.split();
    
    // Producer: watch and enqueue
    let producer = tokio::spawn(async move {
        for i in 0..5 {
            let job = QueuedJob::with_id(
                PathBuf::from(format!("/input/scene{}", i)),
                format!("scene-{}", i),
            );
            tx.enqueue(job).await.unwrap();
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    });
    
    // Consumer: process sequentially
    let consumer = tokio::spawn(async move {
        let mut processed = vec![];
        while let Some(job) = timeout(Duration::from_secs(1), rx.dequeue()).await.ok().flatten() {
            // Simulate processing
            tokio::time::sleep(Duration::from_millis(20)).await;
            processed.push(job.job_id);
            
            if processed.len() == 5 {
                break;
            }
        }
        processed
    });
    
    producer.await.unwrap();
    let processed = consumer.await.unwrap();
    
    assert_eq!(processed.len(), 5);
    assert_eq!(processed[0], "scene-0");
    assert_eq!(processed[4], "scene-4");
}

#[tokio::test]
async fn test_queue_backpressure() {
    // Test that queue applies backpressure when full
    let capacity = 2;
    let queue = JobQueue::new(capacity);
    let (tx, _rx) = queue.split();
    
    // Fill queue
    tx.enqueue(QueuedJob::new(PathBuf::from("/input/job1"))).await.unwrap();
    tx.enqueue(QueuedJob::new(PathBuf::from("/input/job2"))).await.unwrap();
    
    // Next enqueue should block (test with timeout)
    let enqueue_result = timeout(
        Duration::from_millis(100),
        tx.enqueue(QueuedJob::new(PathBuf::from("/input/job3"))),
    ).await;
    
    // Should timeout because queue is full and no consumer is dequeuing
    assert!(enqueue_result.is_err());
}

#[tokio::test]
async fn test_multiple_queues_isolation() {
    // Test that multiple queues don't interfere with each other
    let queue1 = JobQueue::new(10);
    let queue2 = JobQueue::new(10);
    
    let (tx1, mut rx1) = queue1.split();
    let (tx2, mut rx2) = queue2.split();
    
    // Enqueue to both
    tx1.enqueue(QueuedJob::with_id(PathBuf::from("/q1/job"), "q1-job".to_string())).await.unwrap();
    tx2.enqueue(QueuedJob::with_id(PathBuf::from("/q2/job"), "q2-job".to_string())).await.unwrap();
    
    // Dequeue from each
    let job1 = rx1.dequeue().await.unwrap();
    let job2 = rx2.dequeue().await.unwrap();
    
    assert_eq!(job1.job_id, "q1-job");
    assert_eq!(job2.job_id, "q2-job");
}

#[tokio::test]
async fn test_job_path_preservation() {
    // Test that folder paths are preserved through queue
    let original_path = PathBuf::from("/mnt/input/scene-abc-123");
    let job = QueuedJob::new(original_path.clone());
    
    let queue = JobQueue::new(10);
    let (tx, mut rx) = queue.split();
    
    tx.enqueue(job).await.unwrap();
    let dequeued = rx.dequeue().await.unwrap();
    
    assert_eq!(dequeued.folder_path, original_path);
}

#[test]
fn test_queued_job_clone() {
    // Test that QueuedJob is cloneable
    let job = QueuedJob::with_id(
        PathBuf::from("/input/test"),
        "test-job".to_string(),
    );
    
    let cloned = job.clone();
    assert_eq!(job.job_id, cloned.job_id);
    assert_eq!(job.folder_path, cloned.folder_path);
}
