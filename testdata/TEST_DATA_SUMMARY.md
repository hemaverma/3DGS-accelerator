# Test Data Generation Summary

## ✅ Completed

### 1. Test Data Generation Script

**File:** `scripts/create_test_colmap_data.py`

Creates minimal COLMAP reconstruction data for testing:

- Valid COLMAP binary files (cameras.bin, images.bin, points3D.bin)
- Placeholder image files
- No external dependencies (FFmpeg, COLMAP, PIL)

### 2. Generated Test Data

**Location:** `testdata/sample_scene/test_run/`

**COLMAP Sparse Reconstruction:**

```text
testdata/sample_scene/test_run/colmap/sparse/0/
├── cameras.bin   (96 bytes)  - 1 OPENCV camera, 640x480
├── images.bin    (453 bytes) - 5 images in circular arrangement
└── points3D.bin  (50 KB)     - 1000 random 3D points
```text

**Images:**

```text
testdata/sample_scene/test_run/images/
├── frame_000000.jpg
├── frame_000001.jpg
├── frame_000002.jpg
├── frame_000003.jpg
└── frame_000004.jpg
```text

### 3. Test Data Characteristics

**Camera:**

- Model: OPENCV (handles distortion)
- Resolution: 640x480
- Intrinsics: fx=500, fy=500, cx=320, cy=240
- No distortion coefficients

**Image Poses:**

- 5 views in circular arrangement (radius = 2.0m)
- Evenly spaced: 0°, 72°, 144°, 216°, 288°
- All looking toward origin
- Identity rotations

**Point Cloud:**

- 1000 points randomly distributed in cube
- Bounds: [-1,1] × [-1,1] × [-0.5,0.5]
- Random RGB colors (100-255 range)
- Random reprojection errors (0.1-1.0)

## Usage

### Test COLMAP Parsing

```bash
cargo test colmap -- --nocapture
```text

### Test Backend Integration

```bash
# With mock backend (no GPU needed)
BACKEND=mock cargo test backend

# Test gsplat training script directly
python3 scripts/gsplat_train.py \
  --data testdata/sample_scene/test_run/images \
  --colmap-dir testdata/sample_scene/test_run/colmap/sparse/0 \
  --model-dir /tmp/test_output \
  --iterations 100 \
  --save-ply
```text

### Regenerate Test Data

```bash
rm -rf testdata/sample_scene/test_run
python3 scripts/create_test_colmap_data.py
```text

## What This Enables

✅ **COLMAP Parser Testing** - Verify binary file parsing  
✅ **Backend Integration Testing** - Test train/export interfaces  
✅ **Pipeline Testing** - Test job orchestration without real data  
✅ **CI/CD Testing** - Fast, lightweight tests (<100KB data)  
✅ **Development** - Test without GPU, FFmpeg, or COLMAP

❌ **Not For:** Visual quality testing, real reconstructions, photorealistic results

## Next Steps

For real testing with actual video data:

1. Install FFmpeg and COLMAP
2. Capture multi-view video footage
3. Place videos in INPUT_PATH
4. Run full pipeline with real reconstruction

Or use public datasets:

- Mip-NeRF 360: <https://jonbarron.info/mipnerf360/>
- Tanks and Temples: <https://www.tanksandtemples.org/>
- NeRF Synthetic: <https://github.com/bmild/nerf>

## Testing with Tanks and Temples

For realistic quality validation, use the **Tanks and Temples** benchmark dataset:

### Quick Start

```bash
# Download a scene (2-20GB)
./scripts/download-tanks-and-temples.sh barn

# Run benchmark
./scripts/benchmark-tanks-and-temples.sh barn

# Or test directly with Python
python3 scripts/gsplat_train.py \
  --data testdata/tanks-and-temples/barn/images \
  --colmap-dir testdata/tanks-and-temples/barn/colmap/sparse/0 \
  --model-dir outputs/barn \
  --iterations 7000 \
  --save-ply
```text

**Note**: We use the Python script directly because Tanks and Temples provides **pre-extracted images and COLMAP data**. The Python script is the actual training backend that the Rust processor calls internally. For testing the **complete pipeline** (watch → FFmpeg → COLMAP → train), use your own video files with the full Rust processor.

### Available Scenes

**Training (recommended for testing):**

- barn, caterpillar, church, courthouse, ignatius, meetingroom, truck

**Advanced (more complex):**

- auditorium, ballroom, courtroom, museum, palace, temple

### Documentation

See [docs/TANKS_AND_TEMPLES_TESTING.md](../docs/TANKS_AND_TEMPLES_TESTING.md) for:

- Complete setup instructions
- Scene recommendations
- Performance benchmarks
- Troubleshooting guide
- Quality evaluation

---

*Generated: February 24, 2026*
