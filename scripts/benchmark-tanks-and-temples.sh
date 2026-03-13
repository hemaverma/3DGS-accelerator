#!/usr/bin/env bash
# Benchmark 3DGS processor with Tanks and Temples scenes
#
# Usage: ./scripts/benchmark-tanks-and-temples.sh [scene_name...]
# Example: ./scripts/benchmark-tanks-and-temples.sh barn truck

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
DATA_DIR="$PROJECT_ROOT/testdata/tanks-and-temples"
OUTPUT_DIR="$PROJECT_ROOT/outputs/benchmarks"
RESULTS_FILE="$OUTPUT_DIR/benchmark-results.json"

# Color output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

log_info() { echo -e "${BLUE}ℹ${NC} $*"; }
log_success() { echo -e "${GREEN}✓${NC} $*"; }
log_warning() { echo -e "${YELLOW}⚠${NC} $*"; }
log_error() { echo -e "${RED}✗${NC} $*"; }

# Get system info
get_system_info() {
    local os=$(uname -s)
    local arch=$(uname -m)
    local cpu=""
    local gpu=""
    
    if [[ "$os" == "Darwin" ]]; then
        cpu=$(sysctl -n machdep.cpu.brand_string)
        if [[ "$arch" == "arm64" ]]; then
            gpu="Apple Silicon GPU"
        fi
    elif [[ "$os" == "Linux" ]]; then
        cpu=$(grep "model name" /proc/cpuinfo | head -1 | cut -d: -f2 | xargs)
        if command -v nvidia-smi &>/dev/null; then
            gpu=$(nvidia-smi --query-gpu=name --format=csv,noheader | head -1)
        fi
    fi
    
    echo "{\"os\":\"$os\",\"arch\":\"$arch\",\"cpu\":\"$cpu\",\"gpu\":\"$gpu\"}"
}

# Benchmark a single scene
benchmark_scene() {
    local scene="$1"
    local scene_dir="$DATA_DIR/$scene"
    
    # Check if scene exists
    if [[ ! -d "$scene_dir" ]]; then
        log_error "Scene not found: $scene_dir"
        log_info "Download with: ./scripts/download-tanks-and-temples.sh $scene"
        return 1
    fi
    
    log_info "============================================"
    log_info "Benchmarking scene: $scene"
    log_info "============================================"
    
    # Count images
    local num_images=$(find "$scene_dir/images" -type f \( -name "*.jpg" -o -name "*.png" \) | wc -l | xargs)
    log_info "Images: $num_images"
    
    # Setup output directory
    local output_path="$OUTPUT_DIR/$scene/$(date +%Y%m%d_%H%M%S)"
    mkdir -p "$output_path"
    
    # Detect backend
    local backend="${BACKEND:-auto}"
    log_info "Backend: $backend"
    
    # Start timer
    local start_time=$(date +%s)
    
    # Run training with gsplat script directly (faster than full pipeline)
    log_info "Starting training..."
    
    if python3 "$SCRIPT_DIR/gsplat_train.py" \
        --data "$scene_dir/images" \
        --colmap-dir "$scene_dir/colmap/sparse/0" \
        --model-dir "$output_path" \
        --iterations "${ITERATIONS:-7000}" \
        --save-ply \
        --save-splat \
        > "$output_path/training.log" 2>&1; then
        
        local end_time=$(date +%s)
        local duration=$((end_time - start_time))
        
        log_success "Training completed in ${duration}s"
        
        # Collect metrics
        local ply_size=0
        local splat_size=0
        
        if [[ -f "$output_path/model.ply" ]]; then
            ply_size=$(stat -f%z "$output_path/model.ply" 2>/dev/null || stat -c%s "$output_path/model.ply")
        fi
        if [[ -f "$output_path/model.splat" ]]; then
            splat_size=$(stat -f%z "$output_path/model.splat" 2>/dev/null || stat -c%s "$output_path/model.splat")
        fi
        
        # Extract peak memory from logs (if available)
        local peak_memory=$(grep -i "peak memory" "$output_path/training.log" | tail -1 | grep -oE "[0-9.]+" || echo "0")
        
        # Create result JSON
        cat > "$output_path/benchmark.json" <<EOF
{
  "scene": "$scene",
  "timestamp": "$(date -u +"%Y-%m-%dT%H:%M:%SZ")",
  "system": $(get_system_info),
  "config": {
    "backend": "$backend",
    "iterations": ${ITERATIONS:-7000}
  },
  "metrics": {
    "num_images": $num_images,
    "training_time_seconds": $duration,
    "ply_size_bytes": $ply_size,
    "splat_size_bytes": $splat_size,
    "peak_memory_gb": $peak_memory
  },
  "status": "success"
}
EOF
        
        log_success "Results saved to: $output_path/benchmark.json"
        
        # Display summary
        echo ""
        log_info "Benchmark Summary:"
        echo "  Scene: $scene"
        echo "  Images: $num_images"
        echo "  Duration: ${duration}s ($(printf "%.1f" $(echo "$duration / 60" | bc -l))min)"
        echo "  PLY size: $(numfmt --to=iec-i --suffix=B $ply_size 2>/dev/null || echo "${ply_size} bytes")"
        if [[ $splat_size -gt 0 ]]; then
            echo "  SPLAT size: $(numfmt --to=iec-i --suffix=B $splat_size 2>/dev/null || echo "${splat_size} bytes")"
        fi
        echo ""
        
        return 0
    else
        log_error "Training failed"
        
        # Create failure result
        cat > "$output_path/benchmark.json" <<EOF
{
  "scene": "$scene",
  "timestamp": "$(date -u +"%Y-%m-%dT%H:%M:%SZ")",
  "system": $(get_system_info),
  "status": "failed",
  "error": "Training process failed - see training.log"
}
EOF
        
        log_info "Check logs at: $output_path/training.log"
        return 1
    fi
}

# Aggregate results
aggregate_results() {
    log_info "Aggregating benchmark results..."
    
    local results_json="["
    local first=true
    
    # Find all benchmark.json files
    while IFS= read -r -d '' result_file; do
        if [[ "$first" == true ]]; then
            first=false
        else
            results_json+=","
        fi
        results_json+=$(cat "$result_file")
    done < <(find "$OUTPUT_DIR" -name "benchmark.json" -print0 | sort -z)
    
    results_json+="]"
    
    # Save aggregated results
    echo "$results_json" | python3 -m json.tool > "$RESULTS_FILE" 2>/dev/null || echo "$results_json" > "$RESULTS_FILE"
    
    log_success "Aggregated results saved to: $RESULTS_FILE"
}

# Generate markdown report
generate_report() {
    local report_file="$OUTPUT_DIR/BENCHMARK_REPORT.md"
    
    log_info "Generating benchmark report..."
    
    cat > "$report_file" <<'EOF'
# Tanks and Temples Benchmark Results

## System Information

EOF
    
    # Add system info
    get_system_info | python3 -c "
import sys, json
data = json.load(sys.stdin)
print(f\"- **OS**: {data['os']} ({data['arch']})\")
print(f\"- **CPU**: {data['cpu']}\")
print(f\"- **GPU**: {data['gpu']}\")
" >> "$report_file"
    
    cat >> "$report_file" <<EOF

- **Backend**: ${BACKEND:-auto}
- **Iterations**: ${ITERATIONS:-7000}
- **Date**: $(date -u +"%Y-%m-%d %H:%M:%S UTC")

## Results

| Scene | Images | Training Time | PLY Size | Status |
|-------|--------|---------------|----------|--------|
EOF
    
    # Add results from benchmark.json files
    find "$OUTPUT_DIR" -name "benchmark.json" | sort | while read -r result_file; do
        python3 -c "
import sys, json
with open('$result_file') as f:
    data = json.load(f)
    scene = data['scene']
    status = data['status']
    
    if status == 'success':
        metrics = data['metrics']
        num_images = metrics['num_images']
        duration = metrics['training_time_seconds']
        ply_size = metrics['ply_size_bytes']
        
        duration_min = duration / 60
        ply_mb = ply_size / (1024 * 1024)
        
        print(f\"| {scene} | {num_images} | {duration}s ({duration_min:.1f}min) | {ply_mb:.0f}MB | ✅ Success |\")
    else:
        print(f\"| {scene} | - | - | - | ❌ Failed |\")
" >> "$report_file"
    done
    
    cat >> "$report_file" <<EOF

## Performance Metrics

### Training Time vs. Image Count

EOF
    
    # Generate simple scatter plot data (can be visualized externally)
    echo '```' >> "$report_file"
    find "$OUTPUT_DIR" -name "benchmark.json" | while read -r result_file; do
        python3 -c "
import json
with open('$result_file') as f:
    data = json.load(f)
    if data['status'] == 'success':
        m = data['metrics']
        print(f\"{m['num_images']},{m['training_time_seconds']}\")
" 
    done | sort -n >> "$report_file"
    echo '```' >> "$report_file"
    
    cat >> "$report_file" <<EOF

### Output Sizes

Average PLY file size: $(find "$OUTPUT_DIR" -name "model.ply" -exec stat -f%z {} \; 2>/dev/null | awk '{s+=$1} END {print s/NR/1024/1024 "MB"}')

## Notes

- Training iterations: ${ITERATIONS:-7000}
- Backend: ${BACKEND:-auto} (GPU-accelerated if available)
- Timing includes COLMAP loading, training, and export
- Does not include frame extraction (images pre-extracted)

## Raw Data

Full results available in: \`$RESULTS_FILE\`

---

*Generated: $(date -u +"%Y-%m-%d %H:%M:%S UTC")*
EOF
    
    log_success "Report generated: $report_file"
    
    # Display report
    if command -v bat &>/dev/null; then
        bat "$report_file"
    elif command -v less &>/dev/null; then
        less "$report_file"
    else
        cat "$report_file"
    fi
}

# Usage
usage() {
    cat <<EOF
Benchmark 3DGS processor with Tanks and Temples scenes

Usage: $0 [options] [scene...]

Options:
  --iterations N    Number of training iterations (default: 7000)
  --backend NAME    Backend to use: auto, gsplat, gaussian-splatting (default: auto)
  --report          Generate benchmark report after completion
  --help            Show this help

Examples:
  # Benchmark single scene
  $0 truck
  
  # Benchmark multiple scenes
  $0 barn truck church
  
  # Custom iterations
  $0 --iterations 15000 barn
  
  # Generate report from existing results
  $0 --report

Environment Variables:
  BACKEND          Override backend selection (auto, gsplat, gaussian-splatting)
  ITERATIONS       Override training iterations
  FORCE_CPU_BACKEND=1  Force CPU backend (slower)

EOF
}

# Main
main() {
    local scenes=()
    local generate_report_only=false
    
    # Parse arguments
    while [[ $# -gt 0 ]]; do
        case "$1" in
            --help|-h)
                usage
                exit 0
                ;;
            --report)
                generate_report_only=true
                shift
                ;;
            --iterations)
                export ITERATIONS="$2"
                shift 2
                ;;
            --backend)
                export BACKEND="$2"
                shift 2
                ;;
            -*)
                log_error "Unknown option: $1"
                usage
                exit 1
                ;;
            *)
                scenes+=("$1")
                shift
                ;;
        esac
    done
    
    # Create output directory
    mkdir -p "$OUTPUT_DIR"
    
    # Generate report only?
    if [[ "$generate_report_only" == true ]]; then
        generate_report
        exit 0
    fi
    
    # Check if any scenes specified
    if [[ ${#scenes[@]} -eq 0 ]]; then
        log_error "No scenes specified"
        usage
        exit 1
    fi
    
    # Benchmark each scene
    local failed=()
    for scene in "${scenes[@]}"; do
        if ! benchmark_scene "$scene"; then
            failed+=("$scene")
        fi
        echo ""
    done
    
    # Aggregate results
    aggregate_results
    
    # Generate report
    generate_report
    
    # Summary
    echo ""
    echo "============================================"
    if [[ ${#failed[@]} -eq 0 ]]; then
        log_success "All benchmarks completed successfully!"
    else
        log_error "Some benchmarks failed: ${failed[*]}"
        exit 1
    fi
    echo "============================================"
}

main "$@"
