//! Progress tracking and checkpointing for job execution
//!
//! This module provides:
//! - Progress tracking through pipeline stages
//! - Checkpoint persistence to disk (restart-resilient)
//! - Resume capability from saved checkpoints
//! - Progress reporting for monitoring/health endpoints
//!
//! # Checkpoint Storage
//!
//! Checkpoints are stored as JSON files in the output directory:
//! `{output_folder}/.checkpoint.json`
//!
//! This allows jobs to resume after process restarts or failures.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use tracing::{debug, info, warn};

use crate::backends::BackendOutput;
use crate::extractors::FrameSet;

/// Processing stage in the pipeline
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProcessingStage {
    /// Initial validation and video discovery
    Validation,
    /// Frame extraction from videos
    FrameExtraction,
    /// Metadata extraction from videos
    MetadataExtraction,
    /// Manifest generation
    ManifestGeneration,
    /// COLMAP sparse reconstruction
    ColmapReconstruction,
    /// 3DGS model training
    Training,
    /// Export to PLY format
    PlyExport,
    /// Export to SPLAT format
    SplatExport,
    /// Job completed successfully
    Completed,
}

impl ProcessingStage {
    /// Get the stage number (0-based)
    pub fn stage_number(&self) -> usize {
        match self {
            Self::Validation => 0,
            Self::FrameExtraction => 1,
            Self::MetadataExtraction => 2,
            Self::ManifestGeneration => 3,
            Self::ColmapReconstruction => 4,
            Self::Training => 5,
            Self::PlyExport => 6,
            Self::SplatExport => 7,
            Self::Completed => 8,
        }
    }

    /// Get the total number of stages
    pub fn total_stages() -> usize {
        9
    }

    /// Calculate progress percentage (0-100)
    pub fn progress_percentage(&self) -> f64 {
        (self.stage_number() as f64 / (Self::total_stages() - 1) as f64) * 100.0
    }

    /// Get human-readable stage name
    pub fn name(&self) -> &'static str {
        match self {
            Self::Validation => "Validation",
            Self::FrameExtraction => "Frame Extraction",
            Self::MetadataExtraction => "Metadata Extraction",
            Self::ManifestGeneration => "Manifest Generation",
            Self::ColmapReconstruction => "COLMAP Reconstruction",
            Self::Training => "3DGS Training",
            Self::PlyExport => "PLY Export",
            Self::SplatExport => "SPLAT Export",
            Self::Completed => "Completed",
        }
    }

    /// Get the next stage in the pipeline
    pub fn next(&self) -> Option<Self> {
        match self {
            Self::Validation => Some(Self::FrameExtraction),
            Self::FrameExtraction => Some(Self::MetadataExtraction),
            Self::MetadataExtraction => Some(Self::ManifestGeneration),
            Self::ManifestGeneration => Some(Self::ColmapReconstruction),
            Self::ColmapReconstruction => Some(Self::Training),
            Self::Training => Some(Self::PlyExport),
            Self::PlyExport => Some(Self::SplatExport),
            Self::SplatExport => Some(Self::Completed),
            Self::Completed => None,
        }
    }
}

/// Job checkpoint data
///
/// Stores the current state of job execution to enable resumption after
/// failures or process restarts. Persisted to disk as JSON.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobCheckpoint {
    /// Job identifier
    pub job_id: String,
    /// Current processing stage
    pub stage: ProcessingStage,
    /// Input folder path
    pub input_folder: PathBuf,
    /// Output folder path
    pub output_folder: PathBuf,
    /// Temporary folder path
    pub temp_folder: PathBuf,
    /// Checkpoint creation timestamp (Unix seconds)
    pub timestamp: u64,
    /// Completed stage data
    pub completed_stages: CompletedStages,
}

/// Data from completed pipeline stages
///
/// Stores paths and metadata from stages that have already completed,
/// allowing the job to skip these steps on resume.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CompletedStages {
    /// Number of videos discovered
    pub video_count: Option<usize>,
    /// Total frames extracted
    pub total_frames: Option<usize>,
    /// Cached frame sets with validation metadata
    pub frame_cache: Option<FrameCache>,
    /// Path to generated manifest
    pub manifest_path: Option<PathBuf>,
    /// Path to COLMAP sparse output
    pub colmap_sparse_path: Option<PathBuf>,
    /// Number of COLMAP reconstruction points
    pub colmap_points: Option<usize>,
    /// Cached trained model output
    pub model_output: Option<BackendOutput>,
    /// Number of Gaussians in trained model
    pub gaussian_count: Option<usize>,
    /// Path to exported PLY file
    pub ply_path: Option<PathBuf>,
    /// Path to exported SPLAT file
    pub splat_path: Option<PathBuf>,
}

/// Frame extraction cache with validation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrameCache {
    /// Extracted frame sets
    pub frame_sets: Vec<FrameSet>,
    /// Frame file validation data
    pub validation: Vec<FrameValidation>,
}

/// Validation metadata for a single frame file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrameValidation {
    /// Path to frame file
    pub path: PathBuf,
    /// File size in bytes
    pub size: u64,
    /// Last modified timestamp (Unix seconds)
    pub modified: u64,
}

impl FrameCache {
    /// Create a new frame cache from frame sets
    pub fn new(frame_sets: Vec<FrameSet>) -> Result<Self> {
        let mut validation = Vec::new();
        
        for frame_set in &frame_sets {
            for frame_path in &frame_set.frame_paths {
                let metadata = std::fs::metadata(frame_path)
                    .context("Failed to read frame file metadata")?;
                
                let modified = metadata
                    .modified()
                    .context("Failed to get modified time")?
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .context("Invalid modified time")?
                    .as_secs();
                
                validation.push(FrameValidation {
                    path: frame_path.clone(),
                    size: metadata.len(),
                    modified,
                });
            }
        }
        
        Ok(Self {
            frame_sets,
            validation,
        })
    }
    
    /// Validate that all cached frames still exist and match stored metadata
    pub fn validate(&self) -> Result<()> {
        for val in &self.validation {
            if !val.path.exists() {
                anyhow::bail!(
                    "Cached frame file missing: {}",
                    val.path.display()
                );
            }
            
            let metadata = std::fs::metadata(&val.path)
                .context("Failed to read frame file metadata")?;
            
            if metadata.len() != val.size {
                anyhow::bail!(
                    "Frame file size mismatch: {} (expected {}, got {})",
                    val.path.display(),
                    val.size,
                    metadata.len()
                );
            }
            
            let modified = metadata
                .modified()
                .context("Failed to get modified time")?
                .duration_since(SystemTime::UNIX_EPOCH)
                .context("Invalid modified time")?
                .as_secs();
            
            if modified != val.modified {
                anyhow::bail!(
                    "Frame file modified: {} (expected {}, got {})",
                    val.path.display(),
                    val.modified,
                    modified
                );
            }
        }
        
        info!(
            frame_count = self.validation.len(),
            "Frame cache validation passed"
        );
        
        Ok(())
    }
}

impl JobCheckpoint {
    /// Create a new checkpoint for a job
    pub fn new(
        job_id: String,
        input_folder: PathBuf,
        output_folder: PathBuf,
        temp_folder: PathBuf,
    ) -> Self {
        Self {
            job_id,
            stage: ProcessingStage::Validation,
            input_folder,
            output_folder,
            temp_folder,
            timestamp: SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            completed_stages: CompletedStages::default(),
        }
    }

    /// Save checkpoint to disk
    pub fn save(&self) -> Result<()> {
        let checkpoint_path = self.checkpoint_path();
        
        // Ensure parent directory exists
        if let Some(parent) = checkpoint_path.parent() {
            std::fs::create_dir_all(parent)
                .context("Failed to create checkpoint directory")?;
        }

        let json = serde_json::to_string_pretty(&self)
            .context("Failed to serialize checkpoint")?;
        
        std::fs::write(&checkpoint_path, json)
            .context("Failed to write checkpoint file")?;

        debug!(
            job_id = %self.job_id,
            stage = ?self.stage,
            path = %checkpoint_path.display(),
            "Checkpoint saved"
        );

        Ok(())
    }

    /// Load checkpoint from disk
    pub fn load(output_folder: &Path) -> Result<Option<Self>> {
        let checkpoint_path = Self::checkpoint_path_for_folder(output_folder);

        if !checkpoint_path.exists() {
            return Ok(None);
        }

        let json = std::fs::read_to_string(&checkpoint_path)
            .context("Failed to read checkpoint file")?;

        let checkpoint: Self = serde_json::from_str(&json)
            .context("Failed to deserialize checkpoint")?;

        info!(
            job_id = %checkpoint.job_id,
            stage = ?checkpoint.stage,
            progress = checkpoint.stage.progress_percentage(),
            "Checkpoint loaded"
        );

        Ok(Some(checkpoint))
    }

    /// Delete checkpoint file
    pub fn delete(&self) -> Result<()> {
        let checkpoint_path = self.checkpoint_path();

        if checkpoint_path.exists() {
            std::fs::remove_file(&checkpoint_path)
                .context("Failed to delete checkpoint file")?;
            debug!(
                job_id = %self.job_id,
                "Checkpoint deleted"
            );
        }

        Ok(())
    }

    /// Advance to next stage and save checkpoint
    pub fn advance_stage(&mut self) -> Result<()> {
        if let Some(next_stage) = self.stage.next() {
            self.stage = next_stage;
            self.timestamp = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_secs();
            self.save()?;
            
            info!(
                job_id = %self.job_id,
                stage = ?self.stage,
                progress = self.stage.progress_percentage(),
                "Advanced to next stage"
            );
        } else {
            warn!(
                job_id = %self.job_id,
                "Attempted to advance from final stage"
            );
        }

        Ok(())
    }

    /// Get checkpoint file path for this job
    fn checkpoint_path(&self) -> PathBuf {
        Self::checkpoint_path_for_folder(&self.output_folder)
    }

    /// Get checkpoint file path for a folder
    fn checkpoint_path_for_folder(output_folder: &Path) -> PathBuf {
        output_folder.join(".checkpoint.json")
    }

    /// Get progress percentage
    pub fn progress_percentage(&self) -> f64 {
        self.stage.progress_percentage()
    }

    /// Check if job can be resumed
    pub fn can_resume(&self) -> bool {
        // Can resume if not completed and checkpoint is recent (< 24 hours)
        if self.stage == ProcessingStage::Completed {
            return false;
        }

        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let age_hours = (now - self.timestamp) / 3600;
        age_hours < 24
    }
}

/// Progress tracker for active job execution
///
/// Maintains in-memory progress state and checkpoint persistence
pub struct ProgressTracker {
    checkpoint: JobCheckpoint,
}

impl ProgressTracker {
    /// Create a new progress tracker
    pub fn new(
        job_id: String,
        input_folder: PathBuf,
        output_folder: PathBuf,
        temp_folder: PathBuf,
    ) -> Self {
        Self {
            checkpoint: JobCheckpoint::new(job_id, input_folder, output_folder, temp_folder),
        }
    }

    /// Try to resume from existing checkpoint
    pub fn try_resume(output_folder: &Path) -> Result<Option<Self>> {
        if let Some(checkpoint) = JobCheckpoint::load(output_folder)? {
            if checkpoint.can_resume() {
                info!(
                    job_id = %checkpoint.job_id,
                    stage = ?checkpoint.stage,
                    "Resuming from checkpoint"
                );
                return Ok(Some(Self { checkpoint }));
            } else {
                warn!(
                    job_id = %checkpoint.job_id,
                    "Checkpoint too old or completed, starting fresh"
                );
            }
        }
        Ok(None)
    }

    /// Get current stage
    pub fn stage(&self) -> ProcessingStage {
        self.checkpoint.stage
    }

    /// Get progress percentage
    pub fn progress_percentage(&self) -> f64 {
        self.checkpoint.progress_percentage()
    }

    /// Get job ID
    pub fn job_id(&self) -> &str {
        &self.checkpoint.job_id
    }

    /// Get completed stages data
    pub fn completed_stages(&self) -> &CompletedStages {
        &self.checkpoint.completed_stages
    }

    /// Mark stage as completed and advance
    pub fn complete_stage(&mut self) -> Result<()> {
        self.checkpoint.advance_stage()
    }

    /// Update video count
    pub fn set_video_count(&mut self, count: usize) -> Result<()> {
        self.checkpoint.completed_stages.video_count = Some(count);
        self.checkpoint.save()
    }

    /// Update total frames
    pub fn set_total_frames(&mut self, count: usize) -> Result<()> {
        self.checkpoint.completed_stages.total_frames = Some(count);
        self.checkpoint.save()
    }

    /// Update manifest path
    pub fn set_manifest_path(&mut self, path: PathBuf) -> Result<()> {
        self.checkpoint.completed_stages.manifest_path = Some(path);
        self.checkpoint.save()
    }

    /// Update COLMAP sparse path
    pub fn set_colmap_sparse_path(&mut self, path: PathBuf, points: usize) -> Result<()> {
        self.checkpoint.completed_stages.colmap_sparse_path = Some(path);
        self.checkpoint.completed_stages.colmap_points = Some(points);
        self.checkpoint.save()
    }

    /// Update Gaussian count
    pub fn set_gaussian_count(&mut self, count: usize) -> Result<()> {
        self.checkpoint.completed_stages.gaussian_count = Some(count);
        self.checkpoint.save()
    }

    /// Update PLY path
    pub fn set_ply_path(&mut self, path: PathBuf) -> Result<()> {
        self.checkpoint.completed_stages.ply_path = Some(path);
        self.checkpoint.save()
    }

    /// Update SPLAT path
    pub fn set_splat_path(&mut self, path: PathBuf) -> Result<()> {
        self.checkpoint.completed_stages.splat_path = Some(path);
        self.checkpoint.save()
    }

    /// Cache frame extraction results
    pub fn set_frame_cache(&mut self, frame_sets: Vec<FrameSet>) -> Result<()> {
        let cache = FrameCache::new(frame_sets)
            .context("Failed to create frame cache")?;
        self.checkpoint.completed_stages.frame_cache = Some(cache);
        self.checkpoint.save()
    }

    /// Get cached frame sets if available and valid
    pub fn get_cached_frames(&self) -> Result<Option<Vec<FrameSet>>> {
        if let Some(cache) = &self.checkpoint.completed_stages.frame_cache {
            // Validate cache integrity
            match cache.validate() {
                Ok(()) => {
                    info!(
                        frame_count = cache.validation.len(),
                        "Using cached frame extraction results"
                    );
                    Ok(Some(cache.frame_sets.clone()))
                }
                Err(e) => {
                    warn!(
                        error = %e,
                        "Frame cache validation failed, will re-extract"
                    );
                    Ok(None)
                }
            }
        } else {
            Ok(None)
        }
    }

    /// Cache trained model output
    pub fn set_model_output(&mut self, output: BackendOutput) -> Result<()> {
        self.checkpoint.completed_stages.model_output = Some(output);
        self.checkpoint.save()
    }

    /// Get cached model output if available
    pub fn get_cached_model(&self) -> Option<&BackendOutput> {
        if let Some(model_output) = &self.checkpoint.completed_stages.model_output {
            // Verify model file still exists
            if model_output.model_path.exists() {
                info!(
                    model_path = %model_output.model_path.display(),
                    "Using cached trained model"
                );
                return Some(model_output);
            } else {
                warn!(
                    model_path = %model_output.model_path.display(),
                    "Cached model file missing, will re-train"
                );
            }
        }
        None
    }

    /// Finalize job (mark completed and clean up checkpoint)
    pub fn finalize(&mut self) -> Result<()> {
        self.checkpoint.stage = ProcessingStage::Completed;
        self.checkpoint.save()?;
        // Don't delete checkpoint immediately - keep for status queries
        // Cleanup will be handled by retention policy
        Ok(())
    }

    /// Get immutable checkpoint reference
    pub fn checkpoint(&self) -> &JobCheckpoint {
        &self.checkpoint
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_processing_stage_progress() {
        assert_eq!(ProcessingStage::Validation.progress_percentage(), 0.0);
        assert_eq!(ProcessingStage::Completed.progress_percentage(), 100.0);
        assert!(ProcessingStage::Training.progress_percentage() > 50.0);
    }

    #[test]
    fn test_checkpoint_save_load() {
        let temp_dir = TempDir::new().unwrap();
        let output_folder = temp_dir.path().to_path_buf();

        let checkpoint = JobCheckpoint::new(
            "test_job".to_string(),
            temp_dir.path().to_path_buf(),
            output_folder.clone(),
            temp_dir.path().join("temp"),
        );

        checkpoint.save().unwrap();

        let loaded = JobCheckpoint::load(&output_folder).unwrap();
        assert!(loaded.is_some());
        let loaded = loaded.unwrap();
        assert_eq!(loaded.job_id, "test_job");
        assert_eq!(loaded.stage, ProcessingStage::Validation);
    }

    #[test]
    fn test_progress_tracker() {
        let temp_dir = TempDir::new().unwrap();
        let mut tracker = ProgressTracker::new(
            "test_job".to_string(),
            temp_dir.path().to_path_buf(),
            temp_dir.path().to_path_buf(),
            temp_dir.path().join("temp"),
        );

        assert_eq!(tracker.stage(), ProcessingStage::Validation);
        assert_eq!(tracker.progress_percentage(), 0.0);

        tracker.complete_stage().unwrap();
        assert_eq!(tracker.stage(), ProcessingStage::FrameExtraction);

        tracker.set_video_count(3).unwrap();
        assert_eq!(tracker.completed_stages().video_count, Some(3));
    }

    #[test]
    fn test_resume_from_checkpoint() {
        let temp_dir = TempDir::new().unwrap();
        let output_folder = temp_dir.path().to_path_buf();

        let mut tracker = ProgressTracker::new(
            "test_job".to_string(),
            temp_dir.path().to_path_buf(),
            output_folder.clone(),
            temp_dir.path().join("temp"),
        );

        tracker.complete_stage().unwrap();
        tracker.set_video_count(5).unwrap();
        drop(tracker);

        let resumed = ProgressTracker::try_resume(&output_folder).unwrap();
        assert!(resumed.is_some());
        let resumed = resumed.unwrap();
        assert_eq!(resumed.stage(), ProcessingStage::FrameExtraction);
        assert_eq!(resumed.completed_stages().video_count, Some(5));
    }

    #[test]
    fn test_frame_cache() {
        let temp_dir = TempDir::new().unwrap();
        let output_folder = temp_dir.path().to_path_buf();

        let mut tracker = ProgressTracker::new(
            "test_job".to_string(),
            temp_dir.path().to_path_buf(),
            output_folder.clone(),
            temp_dir.path().join("temp"),
        );

        // Create test frame files
        let frames_dir = temp_dir.path().join("frames");
        std::fs::create_dir_all(&frames_dir).unwrap();
        
        let frame_paths: Vec<PathBuf> = (0..3)
            .map(|i| {
                let path = frames_dir.join(format!("frame_{:06}.jpg", i));
                std::fs::write(&path, b"test frame data").unwrap();
                path
            })
            .collect();

        let video_input = crate::extractors::VideoInput {
            path: temp_dir.path().join("test.mp4"),
            filename: "test.mp4".to_string(),
        };

        let frame_set = crate::extractors::FrameSet::new(
            frame_paths.clone(),
            frames_dir.clone(),
            video_input,
        );

        // Cache frame sets
        tracker.set_frame_cache(vec![frame_set.clone()]).unwrap();

        // Verify cache is stored
        assert!(tracker.completed_stages().frame_cache.is_some());

        // Verify we can retrieve cached frames
        let cached = tracker.get_cached_frames().unwrap();
        assert!(cached.is_some());
        let cached_sets = cached.unwrap();
        assert_eq!(cached_sets.len(), 1);
        assert_eq!(cached_sets[0].frame_count, 3);
    }

    #[test]
    fn test_frame_cache_invalidation() {
        let temp_dir = TempDir::new().unwrap();
        let output_folder = temp_dir.path().to_path_buf();

        let mut tracker = ProgressTracker::new(
            "test_job".to_string(),
            temp_dir.path().to_path_buf(),
            output_folder.clone(),
            temp_dir.path().join("temp"),
        );

        // Create and cache frame files
        let frames_dir = temp_dir.path().join("frames");
        std::fs::create_dir_all(&frames_dir).unwrap();
        
        let frame_path = frames_dir.join("frame_000001.jpg");
        std::fs::write(&frame_path, b"test data").unwrap();

        let video_input = crate::extractors::VideoInput {
            path: temp_dir.path().join("test.mp4"),
            filename: "test.mp4".to_string(),
        };

        let frame_set = crate::extractors::FrameSet::new(
            vec![frame_path.clone()],
            frames_dir.clone(),
            video_input,
        );

        tracker.set_frame_cache(vec![frame_set]).unwrap();

        // Delete frame file to invalidate cache
        std::fs::remove_file(&frame_path).unwrap();

        // Cache should be invalid now
        let cached = tracker.get_cached_frames().unwrap();
        assert!(cached.is_none());
    }

    #[test]
    fn test_model_output_cache() {
        use crate::backends::BackendOutput;
        
        let temp_dir = TempDir::new().unwrap();
        let output_folder = temp_dir.path().to_path_buf();

        let mut tracker = ProgressTracker::new(
            "test_job".to_string(),
            temp_dir.path().to_path_buf(),
            output_folder.clone(),
            temp_dir.path().join("temp"),
        );

        // Create a test model file
        let model_path = temp_dir.path().join("model.pth");
        std::fs::write(&model_path, b"model data").unwrap();

        let model_output = BackendOutput::new(
            model_path.clone(),
            30000,
            "test-backend".to_string(),
        );

        // Cache model output
        tracker.set_model_output(model_output.clone()).unwrap();

        // Verify cache is stored
        assert!(tracker.completed_stages().model_output.is_some());

        // Verify we can retrieve cached model
        let cached = tracker.get_cached_model();
        assert!(cached.is_some());
        assert_eq!(cached.unwrap().model_path, model_path);
    }

    #[test]
    fn test_model_cache_invalidation() {
        use crate::backends::BackendOutput;
        
        let temp_dir = TempDir::new().unwrap();
        let output_folder = temp_dir.path().to_path_buf();

        let mut tracker = ProgressTracker::new(
            "test_job".to_string(),
            temp_dir.path().to_path_buf(),
            output_folder.clone(),
            temp_dir.path().join("temp"),
        );

        // Create and cache model file
        let model_path = temp_dir.path().join("model.pth");
        std::fs::write(&model_path, b"model data").unwrap();

        let model_output = BackendOutput::new(
            model_path.clone(),
            30000,
            "test-backend".to_string(),
        );

        tracker.set_model_output(model_output).unwrap();

        // Delete model file to invalidate cache
        std::fs::remove_file(&model_path).unwrap();

        // Cache should be invalid now
        let cached = tracker.get_cached_model();
        assert!(cached.is_none());
    }

    #[test]
    fn test_checkpoint_resume_with_cache() {
        use crate::backends::BackendOutput;
        
        let temp_dir = TempDir::new().unwrap();
        let output_folder = temp_dir.path().to_path_buf();

        // Create initial tracker and populate with cache
        let mut tracker = ProgressTracker::new(
            "test_job".to_string(),
            temp_dir.path().to_path_buf(),
            output_folder.clone(),
            temp_dir.path().join("temp"),
        );

        // Advance to training stage
        tracker.complete_stage().unwrap(); // Validation -> FrameExtraction
        tracker.complete_stage().unwrap(); // FrameExtraction -> MetadataExtraction
        tracker.complete_stage().unwrap(); // MetadataExtraction -> ManifestGeneration
        tracker.complete_stage().unwrap(); // ManifestGeneration -> ColmapReconstruction
        tracker.complete_stage().unwrap(); // ColmapReconstruction -> Training

        // Create and cache model
        let model_path = temp_dir.path().join("model.pth");
        std::fs::write(&model_path, b"model data").unwrap();

        let model_output = BackendOutput::new(
            model_path.clone(),
            30000,
            "test-backend".to_string(),
        );

        tracker.set_model_output(model_output).unwrap();
        drop(tracker);

        // Resume from checkpoint
        let resumed = ProgressTracker::try_resume(&output_folder).unwrap();
        assert!(resumed.is_some());
        let resumed = resumed.unwrap();
        
        // Verify we're at Training stage
        assert_eq!(resumed.stage(), ProcessingStage::Training);

        // Verify cached model is available
        let cached_model = resumed.get_cached_model();
        assert!(cached_model.is_some());
        assert_eq!(cached_model.unwrap().model_path, model_path);
    }
}
