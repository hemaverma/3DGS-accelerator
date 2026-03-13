//! Job queue implementation for sequential processing
//!
//! This module provides a thread-safe FIFO job queue using tokio channels.
//! Jobs are queued when folders are detected and processed sequentially
//! (one at a time) to avoid resource contention.

use anyhow::Result;
use std::path::PathBuf;
use tokio::sync::mpsc;
use tracing::{debug, info, instrument};

/// A queued job representing a folder to process
#[derive(Debug, Clone)]
pub struct QueuedJob {
    /// Absolute path to the folder containing videos
    pub folder_path: PathBuf,
    /// Unique job identifier (typically folder name or UUID)
    pub job_id: String,
}

impl QueuedJob {
    /// Create a new queued job from a folder path
    ///
    /// The job ID is automatically generated from the folder name
    pub fn new(folder_path: PathBuf) -> Self {
        let job_id = folder_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();

        Self {
            folder_path,
            job_id,
        }
    }

    /// Create a new queued job with a custom job ID
    pub fn with_id(folder_path: PathBuf, job_id: String) -> Self {
        Self {
            folder_path,
            job_id,
        }
    }
}

/// FIFO job queue for sequential processing
///
/// This queue uses tokio mpsc channels to provide thread-safe enqueueing
/// and dequeueing of jobs. Jobs are processed in the order they are received.
///
/// # Design
///
/// - **Thread-safe**: Uses tokio::sync::mpsc for safe concurrent access
/// - **FIFO ordering**: Jobs are processed in submission order
/// - **Backpressure**: Bounded channel prevents memory exhaustion
/// - **Non-blocking**: Async send/receive operations
///
/// # Example
///
/// ```no_run
/// use three_dgs_processor::processor::queue::{JobQueue, QueuedJob};
/// use std::path::PathBuf;
///
/// #[tokio::main]
/// async fn main() -> anyhow::Result<()> {
///     let queue = JobQueue::new(100);
///     let (tx, rx) = queue.split();
///     
///     // Producer: enqueue jobs
///     tokio::spawn(async move {
///         tx.enqueue(QueuedJob::new(PathBuf::from("/input/scene1"))).await.ok();
///         tx.enqueue(QueuedJob::new(PathBuf::from("/input/scene2"))).await.ok();
///     });
///     
///     // Consumer: process jobs sequentially
///     while let Some(job) = rx.dequeue().await {
///         println!("Processing job: {:?}", job.job_id);
///         // ... process job ...
///     }
///     
///     Ok(())
/// }
/// ```
pub struct JobQueue {
    tx: mpsc::Sender<QueuedJob>,
    rx: mpsc::Receiver<QueuedJob>,
}

impl JobQueue {
    /// Create a new job queue with the specified capacity
    ///
    /// # Arguments
    ///
    /// * `capacity` - Maximum number of queued jobs before backpressure applies
    ///                Recommended: 100 for typical workloads
    #[instrument]
    pub fn new(capacity: usize) -> Self {
        info!(capacity = capacity, "Creating new job queue");
        let (tx, rx) = mpsc::channel(capacity);
        Self { tx, rx }
    }

    /// Split the queue into sender and receiver halves
    ///
    /// This allows the sender to be moved to the watcher task/thread
    /// while the receiver remains in the processing loop.
    ///
    /// # Returns
    ///
    /// A tuple of (JobQueueSender, JobQueueReceiver)
    pub fn split(self) -> (JobQueueSender, JobQueueReceiver) {
        (
            JobQueueSender { tx: self.tx },
            JobQueueReceiver { rx: self.rx },
        )
    }
}

/// Sender half of the job queue
///
/// Can be cloned and shared across multiple watcher tasks
#[derive(Clone)]
pub struct JobQueueSender {
    tx: mpsc::Sender<QueuedJob>,
}

impl JobQueueSender {
    /// Enqueue a new job for processing
    ///
    /// # Arguments
    ///
    /// * `job` - The job to enqueue
    ///
    /// # Returns
    ///
    /// Ok(()) if the job was enqueued successfully
    /// Err if the queue is closed (receiver dropped)
    ///
    /// # Errors
    ///
    /// Returns an error if the receiver has been dropped, indicating
    /// the processing loop has shut down.
    #[instrument(skip(self), fields(job_id = %job.job_id, folder = %job.folder_path.display()))]
    pub async fn enqueue(&self, job: QueuedJob) -> Result<()> {
        let job_id = job.job_id.clone();

        debug!(
            job_id = %job.job_id,
            folder = %job.folder_path.display(),
            "Enqueuing job"
        );

        self.tx
            .send(job)
            .await
            .map_err(|_| anyhow::anyhow!("Job queue closed"))?;

        info!(job_id = %job_id, "Job enqueued successfully");
        Ok(())
    }

    /// Get the number of queued jobs (approximate)
    ///
    /// Note: This is a lower bound estimate and may not be exact in concurrent scenarios
    pub fn queued_count(&self) -> usize {
        // Note: mpsc doesn't expose queue length directly
        // This is a limitation we accept for this design
        // Alternative would be to use a more complex queue with Arc<Mutex<VecDeque>>
        0
    }
}

/// Receiver half of the job queue
///
/// Must remain in the processing loop to dequeue jobs
pub struct JobQueueReceiver {
    rx: mpsc::Receiver<QueuedJob>,
}

impl JobQueueReceiver {
    /// Dequeue the next job for processing
    ///
    /// This method blocks until a job is available or the sender is dropped.
    ///
    /// # Returns
    ///
    /// Some(QueuedJob) if a job is available
    /// None if all senders have been dropped (shutdown)
    #[instrument(skip(self))]
    pub async fn dequeue(&mut self) -> Option<QueuedJob> {
        match self.rx.recv().await {
            Some(job) => {
                debug!(
                    job_id = %job.job_id,
                    folder = %job.folder_path.display(),
                    "Dequeued job"
                );
                Some(job)
            }
            None => {
                info!("Job queue closed, no more jobs");
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_queue_fifo_ordering() {
        let queue = JobQueue::new(10);
        let (tx, mut rx) = queue.split();

        // Enqueue jobs in order
        let job1 = QueuedJob::new(PathBuf::from("/input/scene1"));
        let job2 = QueuedJob::new(PathBuf::from("/input/scene2"));
        let job3 = QueuedJob::new(PathBuf::from("/input/scene3"));

        tx.enqueue(job1.clone()).await.unwrap();
        tx.enqueue(job2.clone()).await.unwrap();
        tx.enqueue(job3.clone()).await.unwrap();

        // Dequeue should return jobs in FIFO order
        let dequeued1 = rx.dequeue().await.unwrap();
        assert_eq!(dequeued1.job_id, job1.job_id);

        let dequeued2 = rx.dequeue().await.unwrap();
        assert_eq!(dequeued2.job_id, job2.job_id);

        let dequeued3 = rx.dequeue().await.unwrap();
        assert_eq!(dequeued3.job_id, job3.job_id);
    }

    #[tokio::test]
    async fn test_queue_shutdown() {
        let queue = JobQueue::new(10);
        let (tx, mut rx) = queue.split();

        // Enqueue one job
        tx.enqueue(QueuedJob::new(PathBuf::from("/input/scene1")))
            .await
            .unwrap();

        // Drop sender to signal shutdown
        drop(tx);

        // Should dequeue the one job
        assert!(rx.dequeue().await.is_some());

        // Should return None after sender dropped
        assert!(rx.dequeue().await.is_none());
    }

    #[tokio::test]
    async fn test_queued_job_creation() {
        let path = PathBuf::from("/input/my-scene");
        let job = QueuedJob::new(path.clone());

        assert_eq!(job.folder_path, path);
        assert_eq!(job.job_id, "my-scene");
    }

    #[tokio::test]
    async fn test_queued_job_with_custom_id() {
        let path = PathBuf::from("/input/my-scene");
        let custom_id = "custom-job-123".to_string();
        let job = QueuedJob::with_id(path.clone(), custom_id.clone());

        assert_eq!(job.folder_path, path);
        assert_eq!(job.job_id, custom_id);
    }

    #[tokio::test]
    async fn test_queue_sender_cloning() {
        let queue = JobQueue::new(10);
        let (tx1, mut rx) = queue.split();
        let tx2 = tx1.clone();

        // Both senders should work
        tx1.enqueue(QueuedJob::new(PathBuf::from("/input/scene1")))
            .await
            .unwrap();
        tx2.enqueue(QueuedJob::new(PathBuf::from("/input/scene2")))
            .await
            .unwrap();

        // Should receive both jobs
        assert!(rx.dequeue().await.is_some());
        assert!(rx.dequeue().await.is_some());
    }
}
