# Multi-stage Dockerfile for 3DGS Video Processor
# Optimized for small image size using Docker best practices
# Supports multi-arch builds (linux/amd64 and linux/arm64)
# Build with: docker buildx build --platform linux/amd64,linux/arm64 -t 3dgs-processor:latest .

# Use Rust 1.93 on Bookworm for glibc compatibility with runtime stage
FROM rust:1.93-bookworm AS builder

WORKDIR /build

# Install build dependencies
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Copy source and build
COPY Cargo.toml Cargo.lock ./
COPY src ./src

# Build with locked dependencies and strip binary for smaller size  
RUN cargo build --release --locked && \
    strip target/release/3dgs-processor

# ============================================================================
# Stage 2: Build COLMAP (separate to leverage caching)
# ============================================================================
FROM debian:bookworm-slim AS colmap-builder

# Install all dependencies in a single layer
RUN apt-get update && apt-get install -y --no-install-recommends \
    git \
    cmake \
    ninja-build \
    build-essential \
    libboost-program-options-dev \
    libboost-filesystem-dev \
    libboost-graph-dev \
    libboost-system-dev \
    libeigen3-dev \
    libflann-dev \
    libfreeimage-dev \
    libmetis-dev \
    libgoogle-glog-dev \
    libsqlite3-dev \
    libglew-dev \
    qtbase5-dev \
    libqt5opengl5-dev \
    libcgal-dev \
    libceres-dev \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Build COLMAP from source
ARG COLMAP_VERSION=3.9
RUN git clone --branch ${COLMAP_VERSION} --depth 1 https://github.com/colmap/colmap.git /tmp/colmap && \
    cd /tmp/colmap && \
    mkdir build && cd build && \
    cmake .. -GNinja \
        -DCMAKE_BUILD_TYPE=Release \
        -DCMAKE_INSTALL_PREFIX=/usr/local \
        -DBUILD_TESTING=OFF \
        -DCMAKE_CXX_FLAGS="-O3" && \
    ninja && \
    ninja install && \
    rm -rf /tmp/colmap

# ============================================================================
# Stage 3: Python environment with gsplat (optional backend)
# ============================================================================
FROM python:3.11-slim-bookworm AS python-builder

# Install build dependencies for Python packages
RUN apt-get update && apt-get install -y --no-install-recommends \
    build-essential \
    git \
    && rm -rf /var/lib/apt/lists/*

# Create virtual environment
RUN python -m venv /opt/venv
ENV PATH="/opt/venv/bin:$PATH"

# Install PyTorch CPU-only (smaller image, works without GPU)
# For GPU support, change to: torch torchvision --index-url https://download.pytorch.org/whl/cu121
RUN pip install --no-cache-dir \
    torch torchvision --index-url https://download.pytorch.org/whl/cpu && \
    pip install --no-cache-dir \
    gsplat \
    numpy

# ============================================================================
# Stage 4: Final runtime image (minimal)
# ============================================================================
FROM debian:bookworm-slim

WORKDIR /app

# Install runtime dependencies + FFmpeg + Blobfuse2 in a single layer
RUN apt-get update && \
    # Detect architecture
    ARCH=$(dpkg --print-architecture) && \
    # Add Microsoft repository for Blobfuse2
    apt-get install -y --no-install-recommends \
        ca-certificates \
        curl \
        gnupg \
        lsb-release && \
    curl -fsSL https://packages.microsoft.com/keys/microsoft.asc | \
        gpg --dearmor -o /usr/share/keyrings/microsoft-prod.gpg && \
    DISTRO=$(lsb_release -is | tr '[:upper:]' '[:lower:]') && \
    CODENAME=$(lsb_release -cs) && \
    echo "deb [arch=${ARCH} signed-by=/usr/share/keyrings/microsoft-prod.gpg] https://packages.microsoft.com/repos/microsoft-${DISTRO}-${CODENAME}-prod ${CODENAME} main" | \
        tee /etc/apt/sources.list.d/microsoft.list && \
    apt-get update && \
    # Install runtime dependencies
    apt-get install -y --no-install-recommends \
        # Core runtime
        fuse3 \
        libfuse3-3 \
        # Python runtime for gsplat backend
        python3.11 \
        python3.11-venv \
        libpython3.11 \
        # FFmpeg and codecs
        ffmpeg \
        libavcodec59 \
        libavformat59 \
        libavutil57 \
        libswscale6 \
        libavfilter8 \
        # COLMAP runtime dependencies
        libboost-filesystem1.74.0 \
        libboost-program-options1.74.0 \
        libboost-system1.74.0 \
        libboost-graph1.74.0 \
        libfreeimage3 \
        libgoogle-glog0v6 \
        libsqlite3-0 \
        libglew2.2 \
        libopengl0 \
        libglx0 \
        libqt5core5a \
        libqt5gui5 \
        libqt5widgets5 \
        libqt5opengl5 \
        libceres3 \
        libmetis5 \
        libflann1.9 && \
    # Install blobfuse2 only on amd64 (not available for arm64)
    if [ "$ARCH" = "amd64" ]; then \
        apt-get install -y --no-install-recommends blobfuse2 && \
        blobfuse2 --version > /dev/null; \
    else \
        echo "WARNING: blobfuse2 not available for ${ARCH} - Azure Blob mounting will not work" >&2; \
    fi && \
    # Cleanup in the same layer to reduce image size
    apt-get clean && \
    rm -rf /var/lib/apt/lists/* /tmp/* /var/tmp/* && \
    # Verify installations
    ffmpeg -version > /dev/null

# Copy COLMAP binary from builder stage (shared libs already in runtime image)
COPY --from=colmap-builder /usr/local/bin/colmap /usr/local/bin/colmap

# Copy Python virtual environment with gsplat from python-builder
COPY --from=python-builder /opt/venv /opt/venv

# Fix venv Python symlinks to point to system Python
RUN cd /opt/venv/bin && \
    rm -f python python3 python3.11 && \
    ln -s /usr/bin/python3.11 python3.11 && \
    ln -s python3.11 python3 && \
    ln -s python3 python && \
    /opt/venv/bin/python --version

# Copy compiled Rust binary from builder
COPY --from=builder /build/target/release/3dgs-processor /usr/local/bin/3dgs-processor

# Copy gsplat training script
COPY scripts/gsplat_train.py /app/scripts/gsplat_train.py

# Copy Azure mounting helper script
COPY scripts/mount-azure.sh /usr/local/bin/mount-azure.sh

# Create directories and set permissions in a single layer
RUN mkdir -p /config /tmp/3dgs-work /tmp/blobfuse-cache /tmp/blobfuse-configs && \
    chmod +x /usr/local/bin/mount-azure.sh /usr/local/bin/3dgs-processor

# Copy example config
COPY config.example.yaml /config/config.example.yaml

# Set environment variables with defaults
ENV PATH="/opt/venv/bin:$PATH" \
    PYTHONUNBUFFERED=1 \
    LOG_LEVEL=info \
    TEMP_PATH=/tmp/3dgs-work \
    CONFIG_PATH=/config/config.yaml \
    UPLOAD_STABILITY_TIMEOUT_SECS=60 \
    MAX_RETRIES=3 \
    POLL_INTERVAL_SECS=10 \
    BACKEND=gsplat \
    GSPLAT_PYTHON=/opt/venv/bin/python \
    GSPLAT_BIN=/app/scripts/gsplat_train.py \
    RETENTION_DAYS=30

# Required environment variables (must be provided at runtime)
# INPUT_PATH, OUTPUT_PATH, PROCESSED_PATH, ERROR_PATH

# Azure Blob Storage support via Blobfuse2
# Note: Running with Azure Blob Storage requires:
#   - Privileged mode: --privileged flag
#   - Device access: --device /dev/fuse --cap-add SYS_ADMIN
#   - One of the following authentication methods:
#     1. AZURE_STORAGE_CONNECTION_STRING
#     2. AZURE_STORAGE_ACCOUNT + AZURE_STORAGE_SAS_TOKEN
#     3. AZURE_STORAGE_ACCOUNT + AZURE_USE_MANAGED_IDENTITY=true

# Health check (optional)
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
    CMD pgrep -f 3dgs-processor || exit 1

ENTRYPOINT ["/usr/local/bin/3dgs-processor"]
