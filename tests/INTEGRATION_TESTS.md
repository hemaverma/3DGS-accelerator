# Integration Test Suite

Comprehensive integration tests for the 3DGS Video Processor covering all major functional areas.

## Overview

The integration test suite validates the complete processing pipeline from video input through frame extraction, COLMAP reconstruction, 3DGS training, and output generation. Tests use mock backends to avoid requiring GPU resources while still validating the full orchestration logic.

## Test Statistics

**Total Tests:** 151+  
**Test Modules:** 7  
**Coverage Areas:** Multi-video processing, deduplication, retry logic, error handling, queue management, validation, stability detection

## Test Modules

### 1. Multi-Video Processing (`multi_video_processing.rs`)

Tests concurrent processing of multiple videos from different capture angles/times.

**Key Test Cases:**

- `test_process_multiple_videos_sequential` - Process 3 videos with 90 total frames
- `test_concurrent_frame_extraction_simulation` - Concurrent extraction from 5 videos
- `test_video_metadata_aggregation` - Metadata handling for multi-video jobs
- `test_video_input_creation` - VideoInput struct creation
- `test_empty_video_list` - Graceful handling of empty input
- `test_single_video_processing` - Edge case of single video in multi-video pipeline
- `test_mixed_frame_count_videos` - Videos with different frame counts (10, 50, 200 frames)

**Validates:**

- ✅ Concurrent frame extraction pattern
- ✅ Multi-video job orchestration
- ✅ Frame aggregation from multiple sources
- ✅ Export to both PLY and SPLAT formats

### 2. Deduplication (`deduplication.rs`)

Tests duplicate job detection to prevent reprocessing after container restarts.

**Key Test Cases:**

- `test_detect_new_job` - New job should be processed
- `test_detect_already_processed` - Skip jobs in `processed/` folder
- `test_detect_already_failed` - Skip jobs in `error/` folder
- `test_multiple_jobs_mixed_status` - Mixed new/processed/failed jobs
- `test_deduplication_after_restart` - Simulates container restart with pre-existing jobs
- `test_case_sensitive_job_names` - Case sensitivity behavior
- `test_special_characters_in_job_names` - Job names with timestamps and special chars
- `test_nested_job_directories` - Jobs with subdirectories
- `test_empty_processed_and_error_dirs` - First run scenario

**Validates:**

- ✅ DuplicateDetector::check() for all status types
- ✅ Restart-resilient state detection
- ✅ Filesystem case sensitivity handling
- ✅ Special character and nested directory support

### 3. Retry Logic (`retry_logic.rs`)

Tests exponential backoff retry behavior for failed jobs.

**Key Test Cases:**

- `test_retry_succeeds_on_first_attempt` - No retries needed for successful jobs
- `test_retry_exhaustion` - MAX_RETRIES behavior after persistent failures
- `test_exponential_backoff_timing` - Validates 2^n delay pattern (0s, 2s, 4s, 8s)
- `test_retry_counter_tracking` - Attempt counting (initial + retries)
- `test_max_retries_config` - Different MAX_RETRIES values
- `test_retry_with_transient_failure` - Job succeeds on retry after transient error
- `test_no_retry_on_success` - Successful jobs don't trigger retries
- `test_concurrent_retries` - Multiple jobs retry independently
- `test_backoff_duration_calculation` - Formula: 2^attempt seconds
- `test_retry_preserves_job_context` - Job ID and paths preserved across retries

**Validates:**

- ✅ execute_with_retry() function
- ✅ Exponential backoff pattern
- ✅ Retry exhaustion after MAX_RETRIES
- ✅ Context preservation across retries

### 4. Error Scenarios (`error_scenarios.rs`)

Tests various error conditions and edge cases.

**Key Test Cases:**

- `test_empty_input_folder` - Job with no videos
- `test_missing_input_folder` - Non-existent folder path
- `test_insufficient_frames` - Video with too few frames (5 < min 30)
- `test_low_resolution_video` - Video below minimum resolution (320x240)
- `test_corrupted_video_extension` - File with .mp4 extension but invalid content
- `test_training_with_corrupted_frames` - Empty/invalid frame files
- `test_disk_full_simulation` - Insufficient disk space scenario
- `test_permission_denied_output` - Write permission errors
- `test_video_without_extension` - Video file without extension
- `test_mixed_valid_invalid_videos` - Folder with both valid and invalid videos
- `test_zero_duration_video` - Video with 0 duration
- `test_backend_failure_simulation` - Backend training failure
- `test_invalid_job_id` - Path traversal, command injection, null bytes
- `test_extremely_large_frame_count` - 1M frames, 8K resolution
- `test_negative_fps_handling` - Invalid FPS values

**Validates:**

- ✅ Graceful handling of invalid input
- ✅ Validation logic for video constraints
- ✅ Error propagation from backends
- ✅ Security: input sanitization

### 5. Queue Management (`queue_management.rs`)

Tests the FIFO job queue for sequential processing.

**Key Test Cases:**

- `test_basic_enqueue_dequeue` - Single job enqueue/dequeue
- `test_fifo_ordering` - Jobs processed in submission order
- `test_empty_queue` - Dequeue blocks when queue is empty
- `test_queue_capacity` - Bounded channel behavior
- `test_concurrent_producers` - 5 producers × 10 jobs each = 50 jobs
- `test_job_id_generation` - Automatic ID from folder name
- `test_queue_drop_behavior` - Channel closure when sender drops
- `test_sequential_processing_pattern` - Main processing loop pattern
- `test_queue_backpressure` - Backpressure when queue is full
- `test_multiple_queues_isolation` - Independent queue instances
- `test_job_path_preservation` - Folder paths preserved through queue
- `test_queued_job_clone` - QueuedJob cloneable for dispatching

**Validates:**

- ✅ JobQueue FIFO ordering
- ✅ Thread-safe enqueue/dequeue
- ✅ Backpressure with bounded channel
- ✅ Graceful shutdown behavior

### 6. Validation (`validation.rs`)

Tests input validation for videos before processing.

**Key Test Cases:**

- `test_default_validation_config` - Default thresholds (640×480, 30 frames, 1s)
- `test_validation_config_from_env` - Load config from environment variables
- `test_valid_hd_video` - 1920×1080 @ 30fps passes validation
- `test_valid_4k_video` - 3840×2160 @ 60fps passes validation
- `test_invalid_too_few_frames` - 15 frames fails (min 30)
- `test_invalid_low_width` - 320px width fails (min 640)
- `test_invalid_low_height` - 240px height fails (min 480)
- `test_invalid_short_duration` - 0.5s duration fails (min 1.0s)
- `test_boundary_conditions` - Exactly at minimum (valid), just below (invalid)
- `test_high_fps_video` - 120fps video validation
- `test_low_fps_video` - 24fps cinematic video validation
- `test_ultra_wide_resolution` - 21:9 aspect ratio (2560×1080)
- `test_vertical_video` - Portrait 9:16 (1080×1920)
- `test_square_resolution` - 1:1 aspect ratio (1080×1080)
- `test_validation_with_custom_thresholds` - Stricter requirements
- `test_batch_validation` - Validate multiple videos, filter valid ones

**Validates:**

- ✅ ValidationConfig with defaults and from env
- ✅ Resolution, frame count, duration checks
- ✅ Boundary condition handling
- ✅ Aspect ratio support (wide, portrait, square)

### 7. Stability / Edge Cases (`stability_edge_cases.rs`)

Tests file watching stability detection and edge cases.

**Key Test Cases:**

- `test_file_stability_detection` - Wait for file to stop being modified
- `test_folder_stability_with_multiple_files` - All files in folder stabilize
- `test_new_files_during_processing` - Detect files arriving during job processing
- `test_concurrent_folder_creation` - Multiple folders created simultaneously
- `test_symlink_handling` - Symbolic link detection
- `test_hidden_files_and_folders` - Filter .hidden and .DS_Store files
- `test_large_number_of_files` - Process 100 video files
- `test_empty_video_files` - 0-byte files
- `test_filesystem_case_sensitivity` - Case-sensitive vs case-insensitive filesystems
- `test_special_filenames` - Spaces, dashes, parentheses, brackets
- `test_rapid_sequential_uploads` - 10 jobs uploaded in rapid succession
- `test_partially_uploaded_folder` - Detect .part files, wait for completion
- `test_folder_permissions` - Read-only file handling
- `test_nested_directory_structure` - Deeply nested paths (level1/.../level4)
- `test_workspace_cleanup` - Temporary directory cleanup
- `test_file_size_changes` - Detect upload progress via file size

**Validates:**

- ✅ File stability detection (no modifications for N seconds)
- ✅ Hidden file filtering
- ✅ Symlink handling
- ✅ Large batch processing
- ✅ Upload-in-progress detection
- ✅ Filesystem compatibility

## Running Tests

### Run All Integration Tests

```bash
cargo test --test integration
```text

### Run Specific Module

```bash
cargo test --test integration multi_video_processing::
cargo test --test integration deduplication::
cargo test --test integration retry_logic::
cargo test --test integration error_scenarios::
cargo test --test integration queue_management::
cargo test --test integration validation::
cargo test --test integration stability_edge_cases::
```text

### Run Single Test

```bash
cargo test --test integration multi_video_processing::test_process_multiple_videos_sequential
```text

### Verbose Output

```bash
cargo test --test integration -- --nocapture --test-threads=1
```text

## Test Architecture

### Fixtures (`tests/common_fixtures/`)

- `create_test_workspace()` - Create temp workspace with input/output/processed/error dirs
- `create_test_frames(dir, count)` - Generate test JPEG frames for backend testing

### Mocks (`tests/common_mocks/`)

- `MockBackend` - Fast 3DGS training simulator (no GPU required)
  - `MockBackend::new()` - Succeeds after simulated delay
  - `MockBackend::failing()` - Always fails for error testing
  - `MockBackend::with_duration(ms)` - Custom processing time

### Patterns

**Async Tests:**

```rust
#[tokio::test]
async fn test_name() {
    // Test async code
}
```text

**Workspace Setup:**

```rust
let workspace = create_test_workspace().unwrap();
let input = workspace.path().join("input");
let output = workspace.path().join("output");
// Workspace auto-cleans on drop
```text

**Mock Backend Usage:**

```rust
let backend = MockBackend::new();
let frames = create_test_frames(&temp_dir, 50).unwrap();
let frame_refs: Vec<&Path> = frames.iter().map(|p| p.as_path()).collect();
let result = backend.train(&frame_refs, &config).await.unwrap();
assert_eq!(result.iterations_completed, 1000);
```text

## Test Data

Tests use generated test data to avoid dependencies on external video files:

- **Frames:** 1×1 pixel JPEG files with sequential naming (`frame_000001.jpg`)
- **Videos:** Simulated via VideoMetadata structs (no actual video encoding)
- **Workspace:** Temporary directories auto-cleaned after tests

For E2E tests with real videos, see `tests/e2e/` and `testdata/`.

## CI/CD Integration

Integration tests run in CI via `.github/workflows/`:

```yaml
- name: Run integration tests
  run: cargo test --test integration --verbose
```text

**Requirements:**

- No FFmpeg/COLMAP needed (uses mocks)
- No GPU needed (MockBackend)
- Runs on amd64/arm64

## Contributing New Tests

1. **Choose the right module:** Place test in appropriate file based on functionality
2. **Use existing fixtures:** Leverage `create_test_workspace()` and `create_test_frames()`
3. **Use MockBackend:** Avoid real FFmpeg/COLMAP/GPU in integration tests
4. **Name tests descriptively:** `test_<what>_<scenario>` pattern
5. **Add assertions:** Verify both success and error conditions
6. **Document edge cases:** Add comments for non-obvious behaviors

### Example Test Template

```rust
#[tokio::test]
async fn test_my_new_feature() {
    // Arrange
    let workspace = create_test_workspace().unwrap();
    let backend = MockBackend::new();
    
    // Act
    let result = some_function(&workspace.path()).await;
    
    // Assert
    assert!(result.is_ok());
}
```text

## Coverage Goals

- **Multi-video processing:** ✅ Covered
- **Deduplication:** ✅ Covered
- **Retry logic:** ✅ Covered
- **Error scenarios:** ✅ Covered
- **Queue management:** ✅ Covered
- **Input validation:** ✅ Covered
- **Stability detection:** ✅ Covered
- **COLMAP integration:** ⚠️ Partial (integration tests use mocks)
- **Real FFmpeg extraction:** ⚠️ Partial (E2E tests only)
- **Azure Blob Storage:** ⚠️ E2E tests only (Azurite emulator)

## Related Documentation

- **E2E Tests:** `tests/e2e/` - Full containerized pipeline tests
- **E2E Guide:** `tests/E2E_TESTING.md` - E2E test documentation
- **Main README:** `tests/README.md` - Test suite overview
- **PRD:** `docs/3dgs-video-processor-prd.md` - Requirements specification

## Troubleshooting

### Tests Hang

- Check for deadlocks in queue tests
- Verify timeout durations are sufficient
- Use `--test-threads=1` to isolate issues

### Mock Backend Issues

- Ensure `create_test_frames()` generates valid JPEGs
- Check temp directory cleanup (`TempDir::drop`)
- Verify frame references are valid paths

### Flaky Tests

- Stability tests may be timing-sensitive
- Use larger timeouts for file operations
- Consider filesystem-specific behaviors (macOS vs Linux)

## Performance

**Test Execution Time:** ~31 seconds for 151 tests  
**Parallelization:** Default (uses all CPU cores)  
**Memory Usage:** Minimal (mocks don't allocate large buffers)

To run tests sequentially (slower but easier to debug):

```bash
cargo test --test integration -- --test-threads=1
```text

## Future Enhancements

- [ ] Add performance benchmarks for queue throughput
- [ ] Test COLMAP with real sparse reconstruction fixtures
- [ ] Add tests for health endpoint monitoring
- [ ] Test retention policy cleanup logic
- [ ] Add tests for metrics collection and reporting
- [ ] Test graceful shutdown signal handling
