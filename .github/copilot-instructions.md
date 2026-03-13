# 3DGS Video Processor - Project-Specific Patterns

**Requirements:** [docs/3dgs-video-processor-prd.md](../docs/3dgs-video-processor-prd.md)  
**Rust Best Practices:** [.github/instructions/rust/rust.instructions.md](instructions/rust/rust.instructions.md)  
**This file:** Architecture patterns unique to THIS project

## Architecture Patterns

### File Watcher Implementation
Use `notify` crate with hybrid approach:
```rust
use notify::{RecommendedWatcher, RecursiveMode, Event, EventKind};
use tokio::sync::mpsc;
use tokio::time::{sleep, Duration, Instant};

async fn watch_with_stability(
    path: &Path, 
    stability_timeout: Duration
) -> Result<Vec<PathBuf>> {
    let (tx, mut rx) = mpsc::channel(100);
    let mut watcher = RecommendedWatcher::new(/* */)?;
    watcher.watch(path, RecursiveMode::Recursive)?;
    
    let mut last_event = Instant::now();
    // Wait for stability_timeout with no new events
    // If new files arrive during processing, return Err to restart
}
```text

### Multi-Video Processing
Always use `Vec<VideoInput>` for job input. Process frame extraction concurrently:
```rust
use futures::stream::{self, StreamExt};

async fn extract_all_frames(videos: Vec<VideoInput>) -> Result<Vec<FrameSet>> {
    stream::iter(videos)
        .map(|video| extract_frames_from_video(video))
        .buffer_unordered(num_cpus::get())
        .collect::<Vec<_>>()
        .await
}
```text

### Plugin-Based Backend Architecture
Define trait first, load backends as dynamic libraries:
```rust
use libloading::{Library, Symbol};

pub trait GaussianSplatBackend: Send + Sync {
    fn train(&self, frames: &[PathBuf], config: &TrainingConfig) -> Result<BackendOutput>;
    fn export_ply(&self, model: &BackendOutput, path: &Path) -> Result<()>;
    fn export_splat(&self, model: &BackendOutput, path: &Path) -> Result<()>;
}

// Load plugin at runtime based on BACKEND env var
fn load_backend(name: &str) -> Result<Box<dyn GaussianSplatBackend>> {
    let lib_path = format!("plugins/lib{}_backend.so", name);
    unsafe {
        let lib = Library::new(lib_path)?;
        let constructor: Symbol<fn() -> Box<dyn GaussianSplatBackend>> = 
            lib.get(b"create_backend")?;
        Ok(constructor())
    }
}
```text

### Configuration Loading Order
Environment variables are PRIMARY. YAML file contains ONLY 3DGS training hyperparameters:
```rust
struct Config {
    // Always from env vars: INPUT_PATH, OUTPUT_PATH, BACKEND, etc.
    input_path: PathBuf,
    backend: String,
    
    // Only from config.yaml: training iterations, densification settings
    training: TrainingConfig,
}

// Env vars override everything. YAML cannot override env vars.
```text

### FFmpeg/COLMAP Blocking Operations
These tools do synchronous I/O. Wrap in `spawn_blocking`:
```rust
async fn extract_frames(video: VideoInput) -> Result<FrameSet> {
    let video_path = video.path.clone();
    tokio::task::spawn_blocking(move || {
        ffmpeg_extract_sync(&video_path)  // Blocking FFmpeg call
    }).await?
}
```text

## Testing Strategy

### Mock FFmpeg/COLMAP/3DGS Outputs
Don't run actual tools in unit tests. Create fixtures:
```rust
// Generate minimal test frames instead of calling FFmpeg
fn create_test_frames(dir: &Path, count: usize) {
    for i in 0..count {
        image::DynamicImage::new_rgb8(1, 1)
            .save(dir.join(format!("frame_{:06}.jpg", i))).unwrap();
    }
}

// Mock the backend trait
struct MockBackend;
impl GaussianSplatBackend for MockBackend {
    fn train(&self, _: &[PathBuf], _: &TrainingConfig) -> Result<BackendOutput> {
        Ok(BackendOutput::mock())
    }
}
```text

### Test Data Organization
```text
testdata/
  sample_scene/           # Multi-view capture scenario
    ├── view1.mp4         # 5 frames, 1920x1080
    ├── view2.mp4         # Different angle
    └── expected_manifest.json
```text

Use these in integration tests. E2E tests run full container with testdata mounted.

### Cleanup Pattern
Always use RAII with `Drop` or explicit cleanup:
```rust
struct JobContext {
    temp_dir: TempDir,  // Auto-cleanup on drop
}

impl Drop for JobContext {
    fn drop(&mut self) {
        info!(path = %self.temp_dir.path().display(), "Cleaning temp files");
    }
}
```text

## Common Pitfalls

❌ **Don't:** Process single video per job  
✅ **Do:** Always handle `Vec<VideoInput>` with concurrent extraction

❌ **Don't:** Call FFmpeg directly in async code  
✅ **Do:** Wrap FFmpeg calls in `tokio::task::spawn_blocking`

❌ **Don't:** Store job state in memory  
✅ **Do:** Check `PROCESSED_PATH`/`ERROR_PATH` for duplicate detection

❌ **Don't:** Leave temp files after failures  
✅ **Do:** Use RAII (`TempDir`) or explicit cleanup in error paths

❌ **Don't:** Log Azure credentials  
✅ **Do:** Redact connection strings in logs with `#[instrument(skip(conn_str))]`

## Project Structure

```text
src/
  main.rs              # Tokio runtime, signal handling
  lib.rs               # Public API
  watcher/            
    mod.rs             # File watching with stability detection
  processor/
    mod.rs             # Multi-video job orchestration
  extractors/
    frames.rs          # FFmpeg frame extraction (spawn_blocking)
    metadata.rs        # EXIF/GPS metadata extraction
  backends/
    mod.rs             # Backend trait definition
    plugin_loader.rs   # Dynamic library loading
    gaussian_splatting.rs  # Reference implementation
tests/
  integration/         # Tests with testdata/ videos
  e2e/                 # Containerized full-pipeline tests
```text

## Build and Deploy

```bash
# Local development
cargo build --release
cargo test
cargo clippy -- -D warnings

# Multi-arch container (bundle FFmpeg, COLMAP, plugins)
docker buildx build --platform linux/amd64,linux/arm64 \
  -t 3dgs-processor:latest .
```text

## Security

- **Privileged Container:** Required for Blobfuse2 FUSE mounting. Document security implications.
- **Azure Auth:** Support 3 methods (connection string, SAS token, Managed Identity). Never log credentials.
- **Input Validation:** Check video file integrity, minimum resolution, sufficient frames before processing.

## Key Files

- [docs/3dgs-video-processor-prd.md](../docs/3dgs-video-processor-prd.md) - Complete requirements specification
- [.github/instructions/rust/rust.instructions.md](instructions/rust/rust.instructions.md) - Idiomatic Rust best practices
- `config.yaml` - 3DGS-specific video processing parameters ONLY (training iterations, densification, export formats)
- `testdata/` - Sample test videos for integration/E2E tests (will be created)

## Avoid

- Batch processing patterns (this is a **watch service**)
- Single-video processing (always handle **multiple videos** per job)
- Stateful in-memory job tracking (restart-resilient: check processed/error folders)
- Leaving temp files after job completion
- Processing folders already in processed/ or error/ (check duplicates)
