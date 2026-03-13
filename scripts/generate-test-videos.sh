#!/usr/bin/env bash
# Generate minimal test videos for integration and E2E testing
#
# This script creates small sample videos suitable for testing 3DGS processing
# without requiring large real-world video files.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TESTDATA_DIR="$SCRIPT_DIR/../testdata/sample_scene"

echo "🎬 Generating test videos for 3DGS processor..."

# Check for FFmpeg
if ! command -v ffmpeg &> /dev/null; then
    echo "❌ Error: ffmpeg is required but not installed"
    echo "Install with: brew install ffmpeg (macOS) or apt install ffmpeg (Linux)"
    exit 1
fi

# Create testdata directory
mkdir -p "$TESTDATA_DIR"

# Generate 3 simple test videos (5 seconds each, 1280x720, 30fps)
# These simulate a simple scene from different camera perspectives
# Using different color tints to differentiate views

echo "📹 Creating view1.mp4..."
ffmpeg -f lavfi -i testsrc=duration=5:size=1280x720:rate=30 \
    -c:v libx264 -preset fast -crf 23 \
    -y "$TESTDATA_DIR/view1.mp4" 2>/dev/null

echo "📹 Creating view2.mp4..."
ffmpeg -f lavfi -i testsrc=duration=5:size=1280x720:rate=30 \
    -vf "hue=h=60" \
    -c:v libx264 -preset fast -crf 23 \
    -y "$TESTDATA_DIR/view2.mp4" 2>/dev/null

echo "📹 Creating view3.mp4..."
ffmpeg -f lavfi -i testsrc=duration=5:size=1280x720:rate=30 \
    -vf "hue=h=120" \
    -c:v libx264 -preset fast -crf 23 \
    -y "$TESTDATA_DIR/view3.mp4" 2>/dev/null

# Create a corrupted video for error testing
echo "❌ Creating corrupted.mp4 (truncated file)..."
head -c 1024 "$TESTDATA_DIR/view1.mp4" > "$TESTDATA_DIR/corrupted.mp4"

# Create expected manifest template
cat > "$TESTDATA_DIR/expected_manifest.json" <<'EOF'
{
  "job_id": "sample-test-job",
  "processing_timestamp": "2024-01-01T00:00:00Z",
  "videos": [
    {
      "filename": "view1.mp4",
      "frame_count": 150,
      "resolution": "1280x720",
      "fps": 30.0,
      "duration_secs": 5.0,
      "frames": []
    },
    {
      "filename": "view2.mp4",
      "frame_count": 150,
      "resolution": "1280x720",
      "fps": 30.0,
      "duration_secs": 5.0,
      "frames": []
    },
    {
      "filename": "view3.mp4",
      "frame_count": 150,
      "resolution": "1280x720",
      "fps": 30.0,
      "duration_secs": 5.0,
      "frames": []
    }
  ]
}
EOF

# Print summary
echo ""
echo "✅ Test data generated successfully!"
echo "📁 Location: $TESTDATA_DIR"
echo ""
echo "Files created:"
ls -lh "$TESTDATA_DIR"/*.mp4 | awk '{print "  - " $9 " (" $5 ")"}'
echo ""
echo "Total size: $(du -sh "$TESTDATA_DIR" | cut -f1)"
