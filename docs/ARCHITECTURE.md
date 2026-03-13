# Architecture: 3DGS Video Processor

System design and architecture documentation.

## Table of Contents

* [System Overview](#system-overview)
* [Component Architecture](#component-architecture)
* [Pipeline Flow](#pipeline-flow)
* [Module Organization](#module-organization)
* [Design Decisions](#design-decisions)

## System Overview

```
┌─────────────────────────────────────────────────────────────┐
│                    3DGS Video Processor                      │
│            (Long-Running Watch Service)                      │
└─────────────────────────────────────────────────────────────┘
                              │
        ┌─────────────────────┼─────────────────────┐
        │                     │                     │
        ▼                     ▼                     ▼
┌──────────────┐    ┌─────────────────┐    ┌──────────────┐
│ File Watcher │    │  Job Processor  │    │  Cleanup &   │
│   (notify)   │───▶│   (tokio async) │    │  Retention   │
└──────────────┘    └─────────────────┘    └──────────────┘
        │                     │
        │           ┌─────────┴─────────┐
        │           │                   │
        ▼           ▼                   ▼
┌──────────────────────┐    ┌────────────────────┐
│ Stability Detection  │    │ Multi-Video Extract│
│  (no new files for   │    │   (FFmpeg parallel)│
│   30s = complete)    │    └────────────────────┘
└──────────────────────┘                │
                                        ▼
                            ┌────────────────────┐
                            │ Reconstruction     │
                            │ Backend Selector   │
                            │ (colmap/nerfstudio)│
                            └────────────────────┘
                                        │
                                        ▼
                            ┌────────────────────┐
                            │  Backend Selector  │
                            │  (Plugin Loader)   │
                            └────────────────────┘
                                        │
                ┌───────────────────────┼───────────────────────┐
                ▼                       ▼                       ▼
        ┌───────────────┐    ┌───────────────┐    ┌───────────────┐
        │  gaussian-    │    │    gsplat     │    │   3DGS.cpp    │
        │  splatting    │    │   (optimized) │    │  (CPU-only)   │
        └───────────────┘    └───────────────┘    └───────────────┘
                │                       │                       │
                └───────────────────────┴───────────────────────┘
                                        │
                                        ▼
                            ┌────────────────────┐
                            │  Export to PLY &   │
                            │      SPLAT         │
                            └────────────────────┘
```

> **Deep dive on the plugin/backend system:** See [PLUGIN_ARCHITECTURE.md](PLUGIN_ARCHITECTURE.md) for the full plugin loading model, binary interface contract, and how to build a custom backend plugin.

## Component Architecture

### 1. File Watcher (`src/watcher/`)

Monitors input directory for new job folders containing videos.

**Components:**

* `notify_watcher.rs` - Inotify-based watcher (local filesystems)
* `poll_watcher.rs` - Polling watcher (network mounts, Blobfuse2)
* `stability.rs` - Upload completion detection

**Key Behavior:**

* Hybrid approach: Use inotify when available, fall back to polling for FUSE mounts
* Stability timeout: Wait for no new file events for 30 seconds before triggering processing
* Restart-resilient: Check `processed/` and `error/` to avoid reprocessing

**Implementation Pattern:**

```rust
async fn watch_with_stability(path: &Path, timeout: Duration) -> Result<Vec<PathBuf>> {
    let (tx, mut rx) = mpsc::channel(100);
    let mut watcher = RecommendedWatcher::new(/* */)?;
    
    let mut last_event = Instant::now();
    loop {
        select! {
            Some(event) = rx.recv() => last_event = Instant::now(),
            _ = sleep_until(last_event + timeout) => break,
        }
    }
}
```

### 2. Job Processor (`src/processor/`)

Orchestrates end-to-end processing pipeline.

**Components:**

* `queue.rs` - FIFO job queue (sequential processing)
* `job.rs` - Job execution orchestration
* `multi_video.rs` - Concurrent frame extraction
* `progress.rs` - Progress tracking and checkpointing
* `dedup.rs` - Duplicate job detection
* `retry.rs` - Exponential backoff retry logic
* `cleanup.rs` - Post-processing folder management
* `temp.rs` - Temporary file RAII cleanup

**Processing Flow:**

1. Validate input folder contains videos
2. Extract frames from all videos concurrently
3. Generate combined manifest JSON
4. Run COLMAP for camera pose estimation
5. Train 3DGS model via selected backend
6. Export to PLY and SPLAT formats
7. Move folder to `processed/` or `error/`

**Progress Tracking:**

Each job progresses through 8 stages with automatic checkpoint persistence:

```rust
pub enum ProcessingStage {
    Validation,           // 0%
    FrameExtraction,      // 12.5%
    MetadataExtraction,   // 25%
    ManifestGeneration,   // 37.5%
    ColmapReconstruction, // 50%
    Training,             // 62.5%
    PlyExport,            // 75%
    SplatExport,          // 87.5%
    Completed,            // 100%
}
```

Checkpoints are saved to `{output_folder}/.checkpoint.json` after each stage completes, enabling:

* **Restart resilience**: Resume from last completed stage after failures
* **Progress monitoring**: Query current stage and percentage via health endpoint
* **Status visibility**: Track video count, frame count, Gaussian count, etc.

Checkpoints persist for 24 hours and are cleaned up by the retention scheduler.

See [PROGRESS_TRACKING.md](PROGRESS_TRACKING.md) for detailed documentation.

### 3. Frame Extraction (`src/extractors/`)

Concurrent frame extraction using FFmpeg.

**Components:**

* `ffmpeg.rs` - FFmpeg command wrapper (spawn_blocking)
* `frames.rs` - Frame extraction orchestration
* `metadata.rs` - EXIF/GPS metadata extraction
* `models.rs` - Metadata type definitions

**Concurrency Pattern:**

```rust
async fn extract_all_frames(videos: Vec<VideoInput>) -> Result<Vec<FrameSet>> {
    stream::iter(videos)
        .map(|video| tokio::task::spawn_blocking(move || {
            ffmpeg_extract_sync(&video)  // Blocking I/O
        }))
        .buffer_unordered(num_cpus::get())
        .collect::<Vec<_>>()
        .await
}
```

### 4. Reconstruction Backend System (`src/reconstruction/`)

Pluggable reconstruction backend architecture for camera pose estimation.

**Trait Definition:**

```rust
#[async_trait]
pub trait ReconstructionBackend: Send + Sync {
    fn name(&self) -> &str;
    fn is_available(&self) -> Result<bool>;
    async fn reconstruct(
        &self,
        image_dir: &Path,
        output_dir: &Path,
        config: &serde_json::Value,
    ) -> Result<ReconstructionOutput>;
    fn validate_output(&self, output: &ReconstructionOutput) -> Result<()>;
}
```

**Built-in Backends:**

* **`colmap`** (default) - Gold-standard Structure-from-Motion pipeline
  - Feature extraction (SIFT)
  - Feature matching (exhaustive or sequential)
  - Sparse bundle adjustment
  - Output: Camera poses + sparse 3D point cloud
  - Implementation: `src/reconstruction/colmap_backend.rs`

* **`nerfstudio`** - Load pre-computed `transforms.json`
  - No actual reconstruction performed
  - Reads camera poses from Nerfstudio format
  - Compatible with Polycam, Luma AI, Record3D exports
  - Implementation: `src/reconstruction/nerfstudio.rs`

* **`precalibrated`** - Use known camera poses
  - Reads poses from custom calibration file
  - Bypasses SfM entirely for pre-calibrated rigs
  - Implementation: `src/reconstruction/precalibrated.rs`

**Registry and Selection:**

```rust
// Get backend by name from RECONSTRUCTION_BACKEND env var
let backend = get_reconstruction_backend("colmap")?;

// Check if backend is available (binaries installed, etc.)
if !backend.is_available()? {
    return Err(ProcessorError::BackendUnavailable);
}

// Run reconstruction
let output = backend.reconstruct(image_dir, output_dir, config).await?;
```

**COLMAP Implementation:**

The COLMAP backend wraps the traditional COLMAP pipeline:

* `runner.rs` - COLMAP command execution  
* `parser.rs` - Sparse reconstruction output parsing
* `models.rs` - COLMAP data structures

See `src/colmap/` for low-level COLMAP integration.

### 5. Backend System (`src/backends/`)

Plugin-based 3DGS backend architecture.

**Components:**

* `mod.rs` - `GaussianSplatBackend` trait definition
* `registry.rs` - Backend registration and discovery
* `plugin_loader.rs` - Dynamic library loading
* `gaussian_splatting.rs` - Reference backend
* `gsplat.rs` - Optimized backend
* `three_dgs_cpp.rs` - CPU backend
* `models.rs` - Shared backend types

**Trait Definition:**

```rust
#[async_trait]
pub trait GaussianSplatBackend: Send + Sync {
    async fn train(&self, frames: &[&Path], config: &TrainingConfig) -> Result<BackendOutput>;
    async fn export_ply(&self, output: &BackendOutput, path: &Path) -> Result<()>;
    async fn export_splat(&self, output: &BackendOutput, path: &Path) -> Result<()>;
}
```

**Plugin Loading:**

Backends are loaded as shared libraries (`.so`/`.dylib`) based on `BACKEND` env var:

```rust
fn load_backend(name: &str) -> Result<Box<dyn GaussianSplatBackend>> {
    unsafe {
        let lib = Library::new(format!("plugins/lib{}_backend.so", name))?;
        let constructor: Symbol<fn() -> Box<dyn GaussianSplatBackend>> = 
            lib.get(b"create_backend")?;
        Ok(constructor())
    }
}
```

### 6. Storage Management (`src/storage/` & `src/cleanup/`)

Resource cleanup and retention enforcement.

**Components:**

* `space.rs` - Disk space monitoring
* `retention.rs` - Age-based cleanup policies
* `scheduler.rs` - Periodic cleanup tasks

**Cleanup Strategy:**

* Temporary files: RAII with `TempDir` (auto-cleanup on drop)
* Processed data: Delete after `RETENTION_DAYS`
* Error data: Manual review required (no auto-delete)
* Disk full: Pause processing, delete oldest processed data

### 7. Configuration (`src/config/`)

Layered configuration system.

**Components:**

* `env.rs` - Environment variable loading
* `yaml.rs` - YAML file parsing
* `mod.rs` - Merged configuration

**Priority Order:**

1. Environment variables (highest priority)
2. YAML file (3DGS training params only)
3. Default values

### 8. Health & Monitoring (`src/health/` & `src/logging/`)

Observability and diagnostics.

**Components:**

* `status.rs` - Optional health check endpoint
* `metrics.rs` - Structured logging with tracing
* `redact.rs` - Azure credential redaction

**Metrics Logged:**

* Job processing duration
* Frame extraction time
* COLMAP reconstruction stats
* Training iterations and loss
* Export file sizes
* Retry attempts
* Disk space usage

## Pipeline Flow

```
┌────────────────────────────────────────────────────────────────────┐
│ 1. File Watcher Detects New Folder                                │
│    - Check for .mp4 files                                          │
│    - Wait for stability (no new files for 30s)                     │
└────────────────────────────────────────────────────────────────────┘
                            │
                            ▼
┌────────────────────────────────────────────────────────────────────┐
│ 2. Duplicate Detection                                             │
│    - Check if folder exists in processed/ or error/                │
│    - Skip if already processed                                     │
└────────────────────────────────────────────────────────────────────┘
                            │
                            ▼
┌────────────────────────────────────────────────────────────────────┐
│ 3. Multi-Video Frame Extraction (Concurrent)                       │
│    - FFmpeg extracts frames from all videos in parallel            │
│    - Metadata extraction (EXIF, GPS, camera intrinsics)            │
│    - Creates frame_NNNNNN.jpg files                                │
└────────────────────────────────────────────────────────────────────┘
                            │
                            ▼
┌────────────────────────────────────────────────────────────────────┐
│ 4. Manifest Generation                                             │
│    - Combine metadata from all videos                              │
│    - List all extracted frames                                     │
│    - Compute or extract camera intrinsics                          │
└────────────────────────────────────────────────────────────────────┘
                            │
                            ▼
┌────────────────────────────────────────────────────────────────────┐
│ 5. Reconstruction Backend (Pluggable)                              │
│    - Select backend: COLMAP, Nerfstudio, or Precalibrated         │
│    - COLMAP: Feature extraction, matching, bundle adjustment       │
│    - Nerfstudio: Load transforms.json from mobile captures         │
│    - Precalibrated: Use known camera poses                         │
│    - Output: Camera poses + sparse 3D points (or empty)            │
└────────────────────────────────────────────────────────────────────┘
                            │
                            ▼
┌────────────────────────────────────────────────────────────────────┐
│ 6. 3DGS Training (Backend-Dependent)                               │
│    - Load selected backend plugin                                  │
│    - Train Gaussian primitives (iterative optimization)            │
│    - Save model checkpoint                                         │
└────────────────────────────────────────────────────────────────────┘
                            │
                            ▼
┌────────────────────────────────────────────────────────────────────┐
│ 7. Export to Standard Formats                                      │
│    - PLY: Point cloud with Gaussian attributes                     │
│    - SPLAT: Web-optimized binary format                            │
│    - UTC timestamp naming: model-20240101120000.ply                │
└────────────────────────────────────────────────────────────────────┘
                            │
                            ▼
┌────────────────────────────────────────────────────────────────────┐
│ 8. Post-Processing Cleanup                                         │
│    - Move job folder to processed/ (success) or error/ (failure)   │
│    - Delete temporary files                                        │
│    - Log metrics                                                   │
└────────────────────────────────────────────────────────────────────┘
```

## Module Organization

```
src/
├── main.rs                 # Tokio runtime, signal handling
├── lib.rs                  # Public API
├── error.rs                # Unified error types
│
├── config/                 # Configuration loading
│   ├── env.rs             # Environment variables
│   ├── yaml.rs            # YAML file parsing
│   └── tests.rs           # Config tests
│
├── watcher/               # File watching
│   ├── notify_watcher.rs  # Inotify-based
│   ├── poll_watcher.rs    # Polling-based
│   └── stability.rs       # Upload completion detection
│
├── processor/             # Job orchestration
│   ├── job.rs            # Job execution
│   ├── queue.rs          # FIFO queue
│   ├── multi_video.rs    # Concurrent frame extraction
│   ├── progress.rs       # Progress tracking & checkpoints
│   ├── dedup.rs          # Duplicate detection
│   ├── retry.rs          # Retry logic
│   ├── cleanup.rs        # Folder management
│   └── temp.rs           # Temporary file RAII
│
├── extractors/            # Frame extraction
│   ├── ffmpeg.rs         # FFmpeg wrapper
│   ├── frames.rs         #Extraction orchestration
│   ├── metadata.rs       # EXIF/GPS parsing
│   └── models.rs         # Metadata types
│
├── manifest/              # Manifest generation
│   ├── models.rs         # JSON structures
│   └── intrinsics.rs     # Camera intrinsics computation
│
├── reconstruction/        # Pluggable reconstruction backends
│   ├── backend.rs        # ReconstructionBackend trait
│   ├── registry.rs       # Backend registration/discovery
│   ├── colmap_backend.rs # COLMAP implementation
│   ├── nerfstudio.rs     # Nerfstudio transforms.json
│   ├── precalibrated.rs  # Pre-calibrated camera poses
│   └── models.rs         # Shared reconstruction types
│
├── colmap/                # Low-level COLMAP integration
│   ├── runner.rs         # Command execution
│   ├── parser.rs         # Output parsing
│   └── models.rs         # Data structures
│
├── backends/              # 3DGS backend system
│   ├── mod.rs            # Trait definition
│   ├── registry.rs       # Backend registration
│   ├── plugin_loader.rs  # Dynamic loading
│   ├── gaussian_splatting.rs  # Reference backend
│   ├── gsplat.rs         # Optimized backend
│   ├── three_dgs_cpp.rs  # CPU backend
│   └── models.rs         # Shared types
│
├── exporters/             # Format exporters
│   ├── ply.rs            # PLY export
│   └── splat.rs          # SPLAT export
│
├── cleanup/               # Resource management
│   ├── retention.rs      # Age-based cleanup
│   └── scheduler.rs      # Periodic tasks
│
├── storage/               # Disk management
│   └── space.rs          # Space monitoring
│
├── health/                # Health checks
│   └── status.rs         # HTTP endpoint
│
├── logging/               # Observability
│   ├── metrics.rs        # Structured logging
│   └── redact.rs         # Credential sanitization
│
├── shutdown/              # Graceful shutdown
│   └── signal_handler.rs # SIGTERM handler
│
├── azure/                 # Azure Blob Storage
│   ├── auth.rs           # Authentication
│   └── mount.rs          # Blobfuse2 mounting
│
└── validation/            # Input validation
    └── video.rs          # Video file checks
```

## Design Decisions

### Why Rust?

* **Concurrency**: Tokio async runtime for efficient multi-video processing
* **Safety**: Memory safety without garbage collection overhead
* **Performance**: Near-C performance for video processing pipelines
* **Ecosystem**: Excellent FFmpeg bindings, COLMAP integration support

### Why Watch Mode vs. Batch Processing?

* **Automation**: No manual triggering required
* **Low Latency**: Process as soon as uploads complete
* **Restart Resilience**: Check processed/error folders on startup
* **Cloud Native**: Works seamlessly with blob storage file notifications

### Why Plugin Architecture for Backends?

* **Flexibility**: Swap 3DGS implementations without recompiling
* **Isolation**: Backend crashes don't take down the service
* **Extensibility**: Add new backends without modifying core code
* **Version Management**: Run multiple backend versions side-by-side

### Why Pluggable Reconstruction Backends?

* **Multi-Source Support**: Handle COLMAP, Nerfstudio, Polycam, and custom formats
* **Flexibility**: Skip expensive SfM when poses are pre-computed
* **Mobile Capture**: Seamlessly process exports from Record3D, Polycam, Luma AI
* **Research Compatibility**: Works with existing NeRF/3DGS datasets
* **Future-Proof**: Easy to add new reconstruction methods (DROID-SLAM, etc.)

### Why Spawn Blocking for FFmpeg/COLMAP?

* **Blocking I/O**: These tools use synchronous system calls
* **Async Runtime**: Tokio threadpool handles blocking work efficiently
* **Cancellation**: Tasks can be cancelled during shutdown
* **Resource Control**: Limits concurrent blocking operations

### Why Separate Processed and Error Folders?

* **Idempotency**: Avoid reprocessing completed jobs
* **Debugging**: Inspect failed inputs without losing data
* **Retry Logic**: Failed jobs moved to error/ after max retries
* **Cleanup**: Different retention policies for success vs. failure

### Why UTC Timestamps in Filenames?

* **Uniqueness**: Avoid filename collisions
* **Traceability**: Correlate outputs with logs
* **Sorting**: Chronological ordering by filename
* **Timezone Independence**: No ambiguity in distributed systems
