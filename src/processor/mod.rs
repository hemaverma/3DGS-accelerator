//! Job orchestration and processing pipeline
//!
//! This module implements:
//! - Multi-video job processing
//! - Job queue management
//! - Pipeline coordination (frame extraction -> COLMAP -> 3DGS -> export)
//! - Retry logic with exponential backoff
//! - Post-processing folder management (move to processed/error)
//! - Temporary directory management with RAII
//! - Duplicate job detection
//! - Progress tracking and checkpointing

pub mod cleanup;
pub mod dedup;
pub mod job;
pub mod multi_video;
pub mod progress;
pub mod queue;
pub mod retry;
pub mod temp;
pub mod batch;

pub use cleanup::{move_to_error, move_to_processed};
pub use dedup::{DuplicateDetector, DuplicateStatus};
pub use job::{execute_job, JobExecutionParams, JobOutputs, JobResult, JobStatus};
pub use multi_video::process_videos;
pub use progress::{
    CompletedStages, JobCheckpoint, ProcessingStage, ProgressTracker,
};
pub use queue::{JobQueue, JobQueueReceiver, JobQueueSender, QueuedJob};
pub use retry::{execute_with_retry, RetryConfig};
pub use temp::{create_temp_subdir, JobTempDir};
pub use batch::run_batch_job;
