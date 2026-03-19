#!/usr/bin/env bash
# Build a matrix of 3DGS Processor Docker images (CPU and/or GPU variants).
#
# Usage:
#   ./scripts/docker-build-matrix.sh                   # Build CPU + GPU (default)
#   ./scripts/docker-build-matrix.sh --cpu-only         # CPU image only
#   ./scripts/docker-build-matrix.sh --gpu-only         # GPU image only
#   ./scripts/docker-build-matrix.sh --gpu-only --cuda-arch "75"   # T4 only
#   REGISTRY=myacr.azurecr.io ./scripts/docker-build-matrix.sh --push
#
# Environment variables:
#   IMAGE_NAME       — Base image name (default: 3dgs-processor)
#   IMAGE_TAG        — Tag suffix (default: latest)
#   REGISTRY         — Registry prefix, e.g. myacr.azurecr.io (default: none)
#   CUDA_ARCHITECTURES — Semicolon-separated SM codes (default: "75;80;86;89;90")
#   CUDA_VERSION     — CUDA toolkit version (default: 12.6.3)
#   PYTORCH_CUDA_TAG — PyTorch wheel suffix (default: cu126)
#   COLMAP_VERSION   — COLMAP git tag (default: 3.11.1)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
cd "${REPO_ROOT}"

# ── Defaults ────────────────────────────────────────────────────────────────
IMAGE_NAME="${IMAGE_NAME:-3dgs-processor}"
IMAGE_TAG="${IMAGE_TAG:-latest}"
REGISTRY="${REGISTRY:-}"
CUDA_ARCHITECTURES="${CUDA_ARCHITECTURES:-75;80;86;89;90}"
CUDA_VERSION="${CUDA_VERSION:-12.6.3}"
PYTORCH_CUDA_TAG="${PYTORCH_CUDA_TAG:-cu126}"
COLMAP_VERSION="${COLMAP_VERSION:-3.11.1}"

BUILD_CPU=true
BUILD_GPU=true
PUSH=false
LOAD=true   # --load by default for local use

usage() {
    cat <<'EOF'
3DGS Processor Docker build matrix

Usage:
  ./scripts/docker-build-matrix.sh                         # Build CPU + GPU (default)
  ./scripts/docker-build-matrix.sh --cpu-only              # CPU image only
  ./scripts/docker-build-matrix.sh --gpu-only              # GPU image only
  ./scripts/docker-build-matrix.sh --gpu-only --cuda-arch "75"   # T4 only
  REGISTRY=myacr.azurecr.io ./scripts/docker-build-matrix.sh --push

Options:
  --cpu-only            Build CPU-only image
  --gpu-only            Build GPU-enabled image
  --push                Push images to the configured registry (no local --load)
  --no-load             Do not load images into the local Docker daemon
  --cuda-arch <list>    Semicolon-separated SM codes (overrides CUDA_ARCHITECTURES)
  -h, --help            Show this help message and exit

Environment variables:
  IMAGE_NAME            Base image name (default: 3dgs-processor)
  IMAGE_TAG             Tag suffix (default: latest)
  REGISTRY              Registry prefix, e.g. myacr.azurecr.io (default: none)
  CUDA_ARCHITECTURES    Semicolon-separated SM codes (default: "75;80;86;89;90")
  CUDA_VERSION          CUDA toolkit version (default: 12.6.3)
  PYTORCH_CUDA_TAG      PyTorch wheel suffix (default: cu126)
  COLMAP_VERSION        COLMAP git tag (default: 3.11.1)
EOF
}

# ── Parse arguments ─────────────────────────────────────────────────────────
while [[ $# -gt 0 ]]; do
    case $1 in
        --cpu-only)  BUILD_GPU=false; shift ;;
        --gpu-only)  BUILD_CPU=false; shift ;;
        --push)      PUSH=true; LOAD=false; shift ;;
        --no-load)   LOAD=false; shift ;;
        --cuda-arch)
            if [[ $# -lt 2 || "${2:-}" == -* ]]; then
                echo "Error: --cuda-arch requires a value." >&2
                usage
                exit 1
            fi
            CUDA_ARCHITECTURES="$2"
            shift 2
            ;;
        --help|-h)
            usage
            exit 0
            ;;
        *)
            echo "Unknown option: $1 (try --help)"
            exit 1
            ;;
    esac
done

# ── Helpers ─────────────────────────────────────────────────────────────────
tag_name() {
    local variant="$1"
    local full="${IMAGE_NAME}:${variant}"
    if [[ -n "${REGISTRY}" ]]; then
        full="${REGISTRY}/${full}"
    fi
    echo "${full}"
}

build_image() {
    local target="$1"
    local tag="$2"
    shift 2
    local extra_args=("$@")

    echo ""
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    echo "Building: ${tag}  (--target ${target})"
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

    local cmd=(docker buildx build --target "${target}" -t "${tag}")

    if [[ "${PUSH}" == "true" ]]; then
        cmd+=(--push)
    elif [[ "${LOAD}" == "true" ]]; then
        cmd+=(--load)
    fi

    cmd+=("${extra_args[@]}" .)

    echo "→ ${cmd[*]}"
    "${cmd[@]}"

    echo "✅ ${tag} built successfully"
}

# ── Ensure buildx is available ──────────────────────────────────────────────
if ! docker buildx version &> /dev/null; then
    echo "ERROR: Docker Buildx is required. Install it or use Docker Desktop."
    exit 1
fi

echo "========================================="
echo "3DGS Processor — Docker Build Matrix"
echo "========================================="
echo "CPU build:          ${BUILD_CPU}"
echo "GPU build:          ${BUILD_GPU}"
echo "CUDA architectures: ${CUDA_ARCHITECTURES}"
echo "CUDA version:       ${CUDA_VERSION}"
echo "Push:               ${PUSH}"
[[ -n "${REGISTRY}" ]] && echo "Registry:           ${REGISTRY}"
echo "========================================="

BUILT_TAGS=()

# ── CPU image ───────────────────────────────────────────────────────────────
if [[ "${BUILD_CPU}" == "true" ]]; then
    CPU_TAG=$(tag_name "cpu-${IMAGE_TAG}")
    build_image "cpu" "${CPU_TAG}" \
        --platform linux/amd64
    BUILT_TAGS+=("${CPU_TAG}")
fi

# ── GPU image ───────────────────────────────────────────────────────────────
if [[ "${BUILD_GPU}" == "true" ]]; then
    GPU_TAG=$(tag_name "gpu-${IMAGE_TAG}")
    build_image "gpu" "${GPU_TAG}" \
        --platform linux/amd64 \
        --build-arg "CUDA_VERSION=${CUDA_VERSION}" \
        --build-arg "CUDA_ARCHITECTURES=${CUDA_ARCHITECTURES}" \
        --build-arg "PYTORCH_CUDA_TAG=${PYTORCH_CUDA_TAG}" \
        --build-arg "COLMAP_VERSION=${COLMAP_VERSION}"
    BUILT_TAGS+=("${GPU_TAG}")
fi

# ── Preflight image (separate Dockerfile, uses NVIDIA base for GPU detection) ─
PREFLIGHT_TAG=$(tag_name "preflight-${IMAGE_TAG}")

echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "Building: ${PREFLIGHT_TAG}  (Dockerfile.preflight)"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

PF_CMD=(docker buildx build -f Dockerfile.preflight -t "${PREFLIGHT_TAG}" --platform linux/amd64 --build-arg "CUDA_VERSION=${CUDA_VERSION}")
if [[ "${PUSH}" == "true" ]]; then PF_CMD+=(--push); elif [[ "${LOAD}" == "true" ]]; then PF_CMD+=(--load); fi
PF_CMD+=(.)
echo "→ ${PF_CMD[*]}"
"${PF_CMD[@]}"
echo "✅ ${PREFLIGHT_TAG} built successfully"
BUILT_TAGS+=("${PREFLIGHT_TAG}")

# ── Summary ─────────────────────────────────────────────────────────────────
echo ""
echo "========================================="
echo "Build Matrix Complete"
echo "========================================="
for t in "${BUILT_TAGS[@]}"; do
    echo "  • ${t}"
done
echo ""
if [[ "${PUSH}" == "true" ]]; then
    echo "Images pushed to registry."
else
    echo "Images loaded locally. Run with:"
    echo "  docker run --rm <tag> --help"
    echo ""
    echo "To push, re-run with --push and set REGISTRY:"
    echo "  REGISTRY=myacr.azurecr.io $0 --push"
fi
