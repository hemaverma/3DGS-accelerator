# Test Data for 3DGS Video Processor

This directory contains minimal test videos for integration and end-to-end testing.

## Files

* `view1.mp4` - Test video from perspective 1 (5 seconds, 1280x720, 30fps)
* `view2.mp4` - Test video from perspective 2 (5 seconds, 1280x720, 30fps)
* `view3.mp4` - Test video from perspective 3 (5 seconds, 1280x720, 30fps)
* `corrupted.mp4` - Intentionally corrupted video for error handling tests
* `expected_manifest.json` - Expected manifest structure for validation

## Generating Test Videos

Run the generation script:

```bash
./scripts/generate-test-videos.sh
```text

This script requires FFmpeg to be installed.

## Usage in Tests

Integration tests use these videos to verify:

* Frame extraction from multiple videos
* Manifest generation
* Multi-video processing pipeline
* Error handling with corrupted inputs

## Size

Total size: ~3-5MB (small enough for fast CI/CD execution)

## Regeneration

To regenerate test videos (e.g., after changing parameters):

```bash
rm -rf testdata/sample_scene/*.mp4
./scripts/generate-test-videos.sh
```text
