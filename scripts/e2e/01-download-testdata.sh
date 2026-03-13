#!/usr/bin/env bash
# 01-download-testdata.sh — Download COLMAP South Building dataset and create test videos
#
# Downloads the South Building dataset (~20MB, 128 images) from the COLMAP project,
# then splits the images into 3 video files for E2E pipeline testing.
#
# The South Building dataset is a standard benchmark for Structure-from-Motion:
# 128 images of UNC Chapel Hill's South Building from multiple viewpoints.
# COLMAP reliably produces 10,000+ 3D points from this dataset.
#
# Output:
#   testdata/south_building/         — raw extracted images
#   testdata/south_building_videos/  — view1.mp4, view2.mp4, view3.mp4

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$SCRIPT_DIR/../.."
DATASET_URL="https://github.com/colmap/colmap/releases/download/3.11.1/south-building.zip"
DATASET_DIR="$PROJECT_ROOT/testdata/south_building"
VIDEO_DIR="$PROJECT_ROOT/testdata/south_building_videos"
ZIP_FILE="$PROJECT_ROOT/testdata/south-building.zip"

echo "============================================"
echo "  01 — Download Test Data & Create Videos"
echo "============================================"
echo ""

# Check prerequisites
if ! command -v ffmpeg &>/dev/null; then
    echo "❌ ffmpeg is required. Run 00-install-deps.sh first."
    exit 1
fi

# Download dataset if not already present
if [ -d "$DATASET_DIR" ] && [ "$(find "$DATASET_DIR" -name '*.jpg' -o -name '*.JPG' -o -name '*.png' -o -name '*.PNG' 2>/dev/null | wc -l)" -gt 50 ]; then
    echo "✅ South Building dataset already downloaded"
else
    echo "📥 Downloading COLMAP South Building dataset (~20MB)..."
    mkdir -p "$PROJECT_ROOT/testdata"
    curl -L -o "$ZIP_FILE" "$DATASET_URL"
    
    echo "📦 Extracting dataset..."
    mkdir -p "$DATASET_DIR"
    unzip -o -q "$ZIP_FILE" -d "$PROJECT_ROOT/testdata/"
    
    # The zip extracts to south-building/images/ — move images up if needed
    if [ -d "$PROJECT_ROOT/testdata/south-building/images" ]; then
        mv "$PROJECT_ROOT/testdata/south-building/images" "$DATASET_DIR/images" 2>/dev/null || true
        # If the zip extracts directly with a different structure, handle it
    fi
    if [ -d "$PROJECT_ROOT/testdata/south-building" ] && [ "$PROJECT_ROOT/testdata/south-building" != "$DATASET_DIR" ]; then
        # If the extracted dir is different from our target, move it
        if [ ! -d "$DATASET_DIR/images" ]; then
            mv "$PROJECT_ROOT/testdata/south-building" "$DATASET_DIR" 2>/dev/null || true
        fi
    fi
    
    rm -f "$ZIP_FILE"
    echo "✅ Dataset extracted"
fi

# Find image directory
IMAGE_DIR=""
for candidate in "$DATASET_DIR/images" "$DATASET_DIR" "$PROJECT_ROOT/testdata/south-building/images" "$PROJECT_ROOT/testdata/south-building"; do
    count=$(find "$candidate" -maxdepth 1 \( -name '*.jpg' -o -name '*.JPG' -o -name '*.png' -o -name '*.PNG' \) 2>/dev/null | wc -l)
    if [ "$count" -gt 50 ]; then
        IMAGE_DIR="$candidate"
        break
    fi
done

if [ -z "$IMAGE_DIR" ]; then
    echo "❌ Could not find extracted images. Listing testdata contents:"
    find "$PROJECT_ROOT/testdata" -maxdepth 4 -type f | head -30
    exit 1
fi

echo "📁 Found images in: $IMAGE_DIR"
IMAGE_COUNT=$(find "$IMAGE_DIR" -maxdepth 1 \( -name '*.jpg' -o -name '*.JPG' -o -name '*.png' -o -name '*.PNG' \) | wc -l)
echo "📷 Total images: $IMAGE_COUNT"

# Create video directory
mkdir -p "$VIDEO_DIR"

# Sort images and split into 3 groups
mapfile -t ALL_IMAGES < <(find "$IMAGE_DIR" -maxdepth 1 \( -name '*.jpg' -o -name '*.JPG' -o -name '*.png' -o -name '*.PNG' \) | sort)

TOTAL=${#ALL_IMAGES[@]}
THIRD=$(( (TOTAL + 2) / 3 ))

echo ""
echo "🎬 Creating 3 test videos from $TOTAL images (split ~$THIRD each)..."

for VIDEO_IDX in 1 2 3; do
    START=$(( (VIDEO_IDX - 1) * THIRD ))
    END=$(( START + THIRD ))
    if [ "$END" -gt "$TOTAL" ]; then
        END=$TOTAL
    fi
    
    COUNT=$(( END - START ))
    if [ "$COUNT" -lt 1 ]; then
        echo "⚠️  Skipping view${VIDEO_IDX}.mp4 (no images)"
        continue
    fi
    
    # Create temp directory with sequential naming for ffmpeg
    TEMP_FRAMES=$(mktemp -d)
    IDX=0
    for (( i=START; i<END; i++ )); do
        cp "${ALL_IMAGES[$i]}" "$TEMP_FRAMES/$(printf 'frame_%06d.jpg' $IDX)"
        IDX=$((IDX + 1))
    done
    
    OUTPUT_VIDEO="$VIDEO_DIR/view${VIDEO_IDX}.mp4"
    
    echo "  📹 Creating view${VIDEO_IDX}.mp4 ($COUNT frames)..."
    ffmpeg -y -framerate 5 \
        -i "$TEMP_FRAMES/frame_%06d.jpg" \
        -vf "scale=1280:-2" \
        -c:v libx264 -preset fast -crf 18 \
        -pix_fmt yuv420p \
        -r 5 \
        "$OUTPUT_VIDEO" 2>/dev/null
    
    rm -rf "$TEMP_FRAMES"
    
    # Show video info
    DURATION=$(ffprobe -v error -show_entries format=duration -of csv=p=0 "$OUTPUT_VIDEO" 2>/dev/null || echo "?")
    FRAMES=$(ffprobe -v error -count_frames -select_streams v:0 -show_entries stream=nb_read_frames -of csv=p=0 "$OUTPUT_VIDEO" 2>/dev/null || echo "?")
    RES=$(ffprobe -v error -select_streams v:0 -show_entries stream=width,height -of csv=p=0 "$OUTPUT_VIDEO" 2>/dev/null || echo "?")
    echo "    → ${FRAMES} frames, ${RES} resolution, ${DURATION}s duration"
done

echo ""
echo "✅ Test videos created in: $VIDEO_DIR"
ls -lh "$VIDEO_DIR"/*.mp4 2>/dev/null
echo ""
echo "These videos contain real multi-view imagery of the South Building"
echo "and will produce 1000+ 3D points when processed with COLMAP."
