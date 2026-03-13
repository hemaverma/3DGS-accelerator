# End-to-End Testing Guide

This guide explains how to run and understand the E2E test suite for the 3DGS Video Processor.

## Prerequisites

### Required

- Docker installed and running
- Rust toolchain (1.75+)
- FFmpeg installed (for test video generation)

### Optional

- Azure CLI (for Azure Blob Storage tests)
- Docker Buildx (for multi-arch builds)

## Setup

### 1. Generate Test Videos

Before running E2E tests, generate the test video files:

```bash
./scripts/generate-test-videos.sh
```text

This creates:

- `testdata/sample_scene/view1.mp4` (5s, 1280x720)
- `testdata/sample_scene/view2.mp4` (5s, 1280x720)
- `testdata/sample_scene/view3.mp4` (5s, 1280x720)
- `testdata/sample_scene/corrupted.mp4` (intentionally broken)

Total size: ~3-5MB

### 2. Build Docker Image

The E2E tests will automatically build the Docker image if needed, but you can pre-build:

```bash
docker build -t 3dgs-processor:test .
```text

## Running Tests

### Run All E2E Tests

```bash
cargo test --test e2e -- --test-threads=1
```text

**Note:** E2E tests run serially (`--test-threads=1`) to avoid Docker port/name conflicts.

### Run Specific Test

```bash
cargo test --test e2e test_e2e_single_job_processing -- --test-threads=1
```text

### Run with Verbose Output

```bash
cargo test --test e2e -- --test-threads=1 --nocapture
```text

## Test Structure

### Test Categories

#### 1. Infrastructure Tests

- `test_workspace_setup` - Validates test directory structure
- `test_docker_availability` - Checks Docker is available

#### 2. Single Job Processing

- `test_e2e_single_job_processing` - Complete pipeline with 2 videos
  - Creates test volumes
  - Starts container
  - Copies videos to input
  - Waits for processing
  - Verifies `.ply` and `.splat` outputs
  - Confirms job moved to `processed/`

#### 3. Multiple Jobs

- `test_e2e_multiple_jobs` - Sequential processing of 2+ jobs
  - Verifies job queue functionality
  - Confirms all jobs complete

#### 4. Error Handling

- `test_e2e_error_handling` - Corrupted video handling
  - Tests retry mechanism
  - Verifies job moved to `error/` folder after retries

#### 5. Watch Mode

- `test_e2e_watch_mode_dynamic_upload` - Dynamic file detection
  - Starts container with empty input
  - Adds videos while running
  - Tests stability timeout detection

#### 6. Duplicate Detection

- `test_e2e_duplicate_detection` - Prevents reprocessing
  - Processes job once
  - Attempts to reprocess same job
  - Verifies only one output created

#### 7. Azure Blob Storage

- `test_e2e_azure_blobstorage` - Azurite emulator integration
  - Starts Azurite container
  - Configures blobfuse2 mounting
  - Tests full pipeline with blob storage

#### 8. Operational Tests

- `test_e2e_disk_space_monitoring` - Disk space checks
- `test_e2e_retention_cleanup` - Old job cleanup (30+ days)
- `test_e2e_health_endpoint` - Health check HTTP endpoint
- `test_e2e_graceful_shutdown` - SIGTERM handling

## Test Utilities

### `tests/e2e/utils.rs`

Provides Docker orchestration helpers:

```rust
// Create test workspace
let volumes = create_docker_test_volumes().await?;

// Start container
let container = run_docker_container(&volumes, false).await?;

// Copy test videos
copy_testdata_videos(&job_dir, &["view1.mp4", "view2.mp4"])?;

// Wait for completion
wait_for_processing_complete(&volumes.processed_dir, "job_name", Duration::from_secs(180)).await?;

// Stop container
stop_docker_container(&container).await?;
```text

### Key Functions

- `check_docker_available()` - Verifies Docker is running
- `create_docker_test_volumes()` - Creates temp directory structure
- `ensure_docker_image_built()` - Builds Docker image if missing
- `run_docker_container()` - Starts container with test config
- `stop_docker_container()` - Gracefully stops container
- `copy_testdata_videos()` - Copies test videos to input
- `wait_for_processing_complete()` - Polls for job completion
- `verify_output_files()` - Validates output file existence/size
- `start_azurite()` - Starts Azurite emulator
- `get_container_logs()` - Retrieves container logs for debugging

## Test Environment

### Environment Variables (Auto-Configured)

E2E tests set these automatically:

```bash
INPUT_PATH=/input
OUTPUT_PATH=/output
PROCESSED_PATH=/processed
ERROR_PATH=/error
CONFIG_PATH=/config/config.yaml
BACKEND=mock                           # Mock backend for tests
LOG_LEVEL=debug
UPLOAD_STABILITY_TIMEOUT_SECS=5       # Faster for tests
MAX_RETRIES=2                          # Reduced for tests
POLL_INTERVAL_SECS=2                   # Faster polling
```text

### Volume Mounts

Test containers mount:

- `/input` - Job input directories
- `/output` - Generated `.ply` and `.splat` files
- `/processed` - Successfully completed jobs
- `/error` - Failed jobs (after retries)
- `/config` - Configuration file

## Debugging Failed Tests

### 1. Check Container Logs

```rust
let logs = get_container_logs(&container).await?;
println!("Container logs:\n{}", logs);
```text

### 2. Inspect Test Volumes

Test volumes are in temp directories that clean up automatically. To persist:

```rust
// In utils.rs, comment out TempDir drop or use:
let temp_path = volumes.base_path.clone();
println!("Test volumes at: {}", temp_path.display());
std::thread::sleep(Duration::from_secs(300)); // Keep alive
```text

Then inspect:

```bash
ls -la /tmp/test_volumes_xyz/
```text

### 3. Run Container Manually

```bash
docker run --rm \
  -v $(pwd)/testdata/sample_scene:/input/job1 \
  -v $(pwd)/test_output:/output \
  -v $(pwd)/test_processed:/processed \
  -v $(pwd)/test_error:/error \
  -e INPUT_PATH=/input \
  -e OUTPUT_PATH=/output \
  -e PROCESSED_PATH=/processed \
  -e ERROR_PATH=/error \
  -e BACKEND=mock \
  -e LOG_LEVEL=debug \
  3dgs-processor:test
```text

### 4. Common Issues

#### Test Videos Not Found

```text
Error: Test video not found: testdata/sample_scene/view1.mp4
```text

**Solution:** Run `./scripts/generate-test-videos.sh`

#### Docker Not Running

```text
Error: Docker is not running
```text

**Solution:** Start Docker Desktop or Docker daemon

#### Port Conflicts

```text
Error: port 8080 already in use
```text

**Solution:** Stop other services on port 8080, or run tests serially

#### Image Build Failures

```text
Error: Failed to build Docker image
```text

**Solution:** Check Dockerfile, ensure all dependencies available

## CI/CD Integration

### GitHub Actions Example

```yaml
name: E2E Tests

on: [push, pull_request]

jobs:
  e2e:
    runs-on: ubuntu-latest
    
    steps:
      - uses: actions/checkout@v3
      
      - name: Set up Rust
        uses: actions-rs/toolchain@v1
        with:
          toolchain: stable
      
      - name: Install FFmpeg
        run: sudo apt-get install -y ffmpeg
      
      - name: Generate test videos
        run: ./scripts/generate-test-videos.sh
      
      - name: Run E2E tests
        run: cargo test --test e2e -- --test-threads=1
        env:
          RUST_LOG: debug
```text

## Performance

### Test Duration

| Test | Typical Duration | Timeout |
|------|------------------|---------|
| Single job processing | 30-60s | 180s |
| Multiple jobs | 60-120s | 180s per job |
| Error handling | 10-30s | 60s |
| Watch mode | 40-90s | 180s |
| Azure mode | 60-90s | 180s |

**Total suite:** ~5-10 minutes (serial execution)

### Optimization Tips

1. **Pre-build Docker image** before running tests
2. **Use mock backend** (faster than real 3DGS training)
3. **Reduce stability timeout** in test environment
4. **Parallelize** non-Docker-dependent unit tests separately

## Test Data

### Size Constraints

Test videos are intentionally small:

- 5 seconds each
- 1280x720 resolution
- 30 fps
- Total: ~3-5MB

This keeps CI/CD fast and repository size small.

### Adding Custom Test Videos

To add new test scenarios:

1. Add video to `testdata/sample_scene/`
2. Update `scripts/generate-test-videos.sh` if procedural
3. Reference in E2E test with `copy_testdata_videos()`

## Azure Blob Storage Testing

### Azurite Emulator

Tests use [Azurite](https://github.com/Azure/Azurite) for Azure Blob Storage emulation:

```rust
let azurite = start_azurite().await?;
// ... run test ...
stop_docker_container(&azurite).await?;
```text

### Connection String

Default Azurite connection string (hardcoded in tests):

```text
DefaultEndpointsProtocol=http;
AccountName=devstoreaccount1;
AccountKey=Eby8vdM02xNOcqFlqUwJPLlmEtlCDXJ1OUzFT50uSRZ6IFsuFq2UVErCz4I6tq/K1SZFPTOtr/KBHBeksoGMGw==;
BlobEndpoint=http://host.docker.internal:10000/devstoreaccount1;
```text

### Testing with Real Azure

To test against real Azure Blob Storage (not recommended for automated tests):

1. Set environment variables before test
2. Modify `run_docker_container()` to use real connection string
3. Ensure cleanup after test

## Troubleshooting

### Logs Not Showing

Add `--nocapture` to see println/eprintln output:

```bash
cargo test --test e2e -- --test-threads=1 --nocapture
```text

### Container Not Stopping

Check running containers:

```bash
docker ps | grep 3dgs-e2e-test
```text

Manual cleanup:

```bash
docker stop $(docker ps -q --filter name=3dgs-e2e-test)
```text

### Azurite Port Conflicts

Check port 10000/10001:

```bash
lsof -i :10000
```text

Stop conflicting services or change Azurite ports in `utils.rs`.

## Best Practices

1. **Always run E2E tests serially** (`--test-threads=1`)
2. **Generate test videos once** before test runs
3. **Clean up containers** even on test failure (use `--rm` flag)
4. **Use appropriate timeouts** (allow 3-5x expected duration)
5. **Log debugging info** when tests fail
6. **Test both success and failure paths**
7. **Verify file contents**, not just existence

## Contributing

When adding new E2E tests:

1. Follow naming convention: `test_e2e_<scenario>`
2. Add `#[serial]` attribute for Docker tests
3. Use provided utilities instead of raw Docker commands
4. Clean up resources in all code paths (success/failure)
5. Add descriptive comments explaining test scenario
6. Update this documentation with new test descriptions

## Related Documentation

- [ARCHITECTURE.md](../docs/ARCHITECTURE.md) - System architecture
- [DEPLOYMENT.md](../docs/DEPLOYMENT.md) - Deployment guide
- [TROUBLESHOOTING.md](../docs/TROUBLESHOOTING.md) - Operational issues
- [docs/3dgs-video-processor-prd.md](../docs/3dgs-video-processor-prd.md) - Requirements
