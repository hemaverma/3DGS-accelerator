#!/usr/bin/env bash
# Download and prepare Tanks and Temples dataset scenes
#
# Usage: ./scripts/download-tanks-and-temples.sh [scene_name...]
# Example: ./scripts/download-tanks-and-temples.sh barn truck

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
OUTPUT_DIR="$PROJECT_ROOT/testdata/tanks-and-temples"

# Color output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

log_info() { echo -e "${BLUE}ℹ${NC} $*"; }
log_success() { echo -e "${GREEN}✓${NC} $*"; }
log_warning() { echo -e "${YELLOW}⚠${NC} $*"; }
log_error() { echo -e "${RED}✗${NC} $*"; }

# Available scenes and their download URLs
declare -A SCENE_URLS=(
    # Training scenes (recommended)
    ["barn"]="https://storage.googleapis.com/tanks-and-temples/barn.tar.gz"
    ["caterpillar"]="https://storage.googleapis.com/tanks-and-temples/caterpillar.tar.gz"
    ["church"]="https://storage.googleapis.com/tanks-and-temples/church.tar.gz"
    ["courthouse"]="https://storage.googleapis.com/tanks-and-temples/courthouse.tar.gz"
    ["ignatius"]="https://storage.googleapis.com/tanks-and-temples/ignatius.tar.gz"
    ["meetingroom"]="https://storage.googleapis.com/tanks-and-temples/meetingroom.tar.gz"
    ["truck"]="https://storage.googleapis.com/tanks-and-temples/truck.tar.gz"
    
    # Advanced scenes (more complex)
    ["auditorium"]="https://storage.googleapis.com/tanks-and-temples/auditorium.tar.gz"
    ["ballroom"]="https://storage.googleapis.com/tanks-and-temples/ballroom.tar.gz"
    ["courtroom"]="https://storage.googleapis.com/tanks-and-temples/courtroom.tar.gz"
    ["museum"]="https://storage.googleapis.com/tanks-and-temples/museum.tar.gz"
    ["palace"]="https://storage.googleapis.com/tanks-and-temples/palace.tar.gz"
    ["temple"]="https://storage.googleapis.com/tanks-and-temples/temple.tar.gz"
)

# Display usage
usage() {
    cat <<EOF
Download and prepare Tanks and Temples dataset scenes

Usage: $0 [scene_name...]

Available scenes:
  Training (recommended for testing):
    barn, caterpillar, church, courthouse, ignatius, meetingroom, truck
  
  Advanced (more complex):
    auditorium, ballroom, courtroom, museum, palace, temple

Examples:
  $0 barn                  # Download barn scene
  $0 barn truck church     # Download multiple scenes
  $0 --list                # List all available scenes
  $0 --help                # Show this help

Notes:
  - Each scene is 2-20GB (download may take 10-60 minutes)
  - Requires curl or wget
  - COLMAP must be installed if format conversion is needed
  - Data saved to: testdata/tanks-and-temples/

EOF
}

# List available scenes
list_scenes() {
    echo "Training Scenes:"
    for scene in barn caterpillar church courthouse ignatius meetingroom truck; do
        echo "  - $scene"
    done
    echo ""
    echo "Advanced Scenes:"
    for scene in auditorium ballroom courtroom museum palace temple; do
        echo "  - $scene"
    done
}

# Check dependencies
check_dependencies() {
    local missing=()
    
    if ! command -v curl &>/dev/null && ! command -v wget &>/dev/null; then
        missing+=("curl or wget")
    fi
    
    if [[ ${#missing[@]} -gt 0 ]]; then
        log_error "Missing required dependencies: ${missing[*]}"
        echo "Install with:"
        echo "  macOS: brew install curl"
        echo "  Ubuntu: apt install curl"
        exit 1
    fi
    
    # COLMAP is optional but recommended
    if ! command -v colmap &>/dev/null; then
        log_warning "COLMAP not found - format conversion may not work"
        log_info "Install with: brew install colmap (macOS) or apt install colmap (Ubuntu)"
    fi
}

# Download file with progress
download_file() {
    local url="$1"
    local output="$2"
    
    log_info "Downloading from $url..."
    
    if command -v curl &>/dev/null; then
        curl -L --progress-bar -o "$output" "$url"
    elif command -v wget &>/dev/null; then
        wget --progress=bar -O "$output" "$url"
    else
        log_error "Neither curl nor wget found"
        return 1
    fi
}

# Extract archive
extract_archive() {
    local archive="$1"
    local output_dir="$2"
    
    log_info "Extracting $(basename "$archive")..."
    
    mkdir -p "$output_dir"
    tar -xzf "$archive" -C "$output_dir" --strip-components=1
    
    log_success "Extracted to $output_dir"
}

# Organize COLMAP data into expected structure
organize_colmap_data() {
    local scene_dir="$1"
    local scene_name="$2"
    
    log_info "Organizing COLMAP data for $scene_name..."
    
    # Expected structure after extraction varies by scene
    # Common patterns:
    # 1. images/ and colmap/ at root
    # 2. images/ and sparse/ at root
    # 3. everything in subdirectory
    
    local colmap_target="$scene_dir/colmap/sparse/0"
    mkdir -p "$colmap_target"
    
    # Find COLMAP binary files
    local cameras_bin=$(find "$scene_dir" -name "cameras.bin" -type f | head -1)
    local images_bin=$(find "$scene_dir" -name "images.bin" -type f | head -1)
    local points3d_bin=$(find "$scene_dir" -name "points3D.bin" -type f | head -1)
    
    if [[ -n "$cameras_bin" ]]; then
        cp "$cameras_bin" "$colmap_target/"
        cp "$images_bin" "$colmap_target/"
        cp "$points3d_bin" "$colmap_target/"
        log_success "COLMAP binary files copied"
    else
        log_warning "COLMAP binary files not found - may need manual conversion"
        
        # Check for text format
        local cameras_txt=$(find "$scene_dir" -name "cameras.txt" -type f | head -1)
        if [[ -n "$cameras_txt" && -n "$(command -v colmap)" ]]; then
            log_info "Found text format, converting to binary..."
            local txt_dir=$(dirname "$cameras_txt")
            colmap model_converter \
                --input_path "$txt_dir" \
                --input_type txt \
                --output_path "$colmap_target" \
                --output_type bin
            log_success "Converted text to binary format"
        fi
    fi
    
    # Ensure images directory exists
    if [[ ! -d "$scene_dir/images" ]]; then
        local images_dir=$(find "$scene_dir" -type d -name "images" | head -1)
        if [[ -n "$images_dir" ]]; then
            ln -sf "$(realpath --relative-to="$scene_dir" "$images_dir")" "$scene_dir/images"
        else
            log_warning "Images directory not found at expected location"
        fi
    fi
}

# Create manifest.json for the processor
create_manifest() {
    local scene_dir="$1"
    local scene_name="$2"
    
    log_info "Creating manifest.json..."
    
    local num_images=$(find "$scene_dir/images" -type f \( -name "*.jpg" -o -name "*.png" \) 2>/dev/null | wc -l)
    
    cat > "$scene_dir/manifest.json" <<EOF
{
  "version": "1.0",
  "dataset": "tanks-and-temples",
  "scene": "${scene_name}",
  "source": "https://www.tanksandtemples.org/",
  "images": {
    "count": ${num_images},
    "format": "jpg",
    "path": "images/"
  },
  "reconstruction": {
    "method": "colmap",
    "sparse_path": "colmap/sparse/0/"
  },
  "metadata": {
    "downloaded": "$(date -u +"%Y-%m-%dT%H:%M:%SZ")",
    "processor_version": "1.0.0"
  }
}
EOF
    
    log_success "Created manifest.json"
}

# Download and prepare a single scene
download_scene() {
    local scene="$1"
    
    # Normalize scene name to lowercase
    scene=$(echo "$scene" | tr '[:upper:]' '[:lower:]')
    
    # Check if scene exists
    if [[ ! -v "SCENE_URLS[$scene]" ]]; then
        log_error "Unknown scene: $scene"
        echo "Run '$0 --list' to see available scenes"
        return 1
    fi
    
    local url="${SCENE_URLS[$scene]}"
    local scene_dir="$OUTPUT_DIR/$scene"
    
    # Check if already downloaded
    if [[ -d "$scene_dir/images" ]] && [[ -f "$scene_dir/colmap/sparse/0/cameras.bin" ]]; then
        log_warning "Scene '$scene' already exists at $scene_dir"
        read -p "Re-download? [y/N] " -n 1 -r
        echo
        if [[ ! $REPLY =~ ^[Yy]$ ]]; then
            log_info "Skipping download"
            return 0
        fi
        rm -rf "$scene_dir"
    fi
    
    log_info "============================================"
    log_info "Downloading scene: $scene"
    log_info "============================================"
    
    # Create temporary directory for download
    local temp_dir=$(mktemp -d)
    trap "rm -rf $temp_dir" EXIT
    
    local archive="$temp_dir/$scene.tar.gz"
    
    # Download
    if ! download_file "$url" "$archive"; then
        log_error "Failed to download $scene"
        return 1
    fi
    
    # Extract
    if ! extract_archive "$archive" "$scene_dir"; then
        log_error "Failed to extract $scene"
        return 1
    fi
    
    # Organize data
    organize_colmap_data "$scene_dir" "$scene"
    
    # Create manifest
    create_manifest "$scene_dir" "$scene"
    
    # Verify structure
    log_info "Verifying data structure..."
    
    local status_ok=true
    if [[ ! -d "$scene_dir/images" ]]; then
        log_error "Missing images/ directory"
        status_ok=false
    fi
    if [[ ! -f "$scene_dir/colmap/sparse/0/cameras.bin" ]]; then
        log_error "Missing COLMAP cameras.bin"
        status_ok=false
    fi
    if [[ ! -f "$scene_dir/colmap/sparse/0/images.bin" ]]; then
        log_error "Missing COLMAP images.bin"
        status_ok=false
    fi
    if [[ ! -f "$scene_dir/colmap/sparse/0/points3D.bin" ]]; then
        log_error "Missing COLMAP points3D.bin"
        status_ok=false
    fi
    
    if [[ "$status_ok" == true ]]; then
        log_success "============================================"
        log_success "Scene '$scene' ready at:"
        log_success "  $scene_dir"
        log_success "============================================"
        
        # Show usage hint
        echo ""
        log_info "Test with:"
        echo "  python3 scripts/gsplat_train.py \\"
        echo "    --data $scene_dir/images \\"
        echo "    --colmap-dir $scene_dir/colmap/sparse/0 \\"
        echo "    --model-dir outputs/$scene \\"
        echo "    --iterations 7000 \\"
        echo "    --save-ply"
        echo ""
    else
        log_error "Scene '$scene' incomplete - manual fixes needed"
        return 1
    fi
}

# Main
main() {
    # Parse arguments
    if [[ $# -eq 0 ]]; then
        usage
        exit 1
    fi
    
    case "$1" in
        --help|-h)
            usage
            exit 0
            ;;
        --list|-l)
            list_scenes
            exit 0
            ;;
    esac
    
    # Check dependencies
    check_dependencies
    
    # Create output directory
    mkdir -p "$OUTPUT_DIR"
    
    # Download requested scenes
    local failed=()
    for scene in "$@"; do
        if ! download_scene "$scene"; then
            failed+=("$scene")
        fi
    done
    
    # Summary
    echo ""
    echo "============================================"
    if [[ ${#failed[@]} -eq 0 ]]; then
        log_success "All scenes downloaded successfully!"
    else
        log_error "Some scenes failed: ${failed[*]}"
        exit 1
    fi
    echo "============================================"
    
    log_info "Next steps:"
    echo "  1. Review downloaded data in: $OUTPUT_DIR"
    echo "  2. Run processor with: BACKEND=auto INPUT_PATH=$OUTPUT_DIR/[scene] ./target/release/3dgs-processor"
    echo "  3. Or test directly: python3 scripts/gsplat_train.py --data $OUTPUT_DIR/[scene]/images ..."
}

main "$@"
