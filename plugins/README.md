# Backend Plugins

This directory contains dynamically loadable backend plugins for the 3DGS Video Processor.

> **For a deep-dive on plugin internals** — discovery, ABI contract, how to build a plugin from scratch — see [docs/PLUGIN_ARCHITECTURE.md](../docs/PLUGIN_ARCHITECTURE.md).

## Overview

The 3DGS Video Processor supports multiple backend implementations through a plugin system. Backends can be:

1. **Built-in**: Compiled directly into the binary (default: `gaussian-splatting`)
2. **Plugin-based**: Dynamically loaded from shared libraries at runtime

## Available Backends

### Built-in Backends

- **gaussian-splatting** - Reference implementation from the original paper
  - Source: `src/backends/gaussian_splatting.rs`
  - Always available

### Plugin Backends

- **gsplat** - Optimized Python/CUDA implementation
  - Source: `src/backends/gsplat.rs`
  - Library: `libgsplat_backend.so` (Linux) / `libgsplat_backend.dylib` (macOS)
  - Requirements: Python 3.8+, CUDA, gsplat package

- **3dgs-cpp** - High-performance C++ implementation
  - Source: `src/backends/three_dgs_cpp.rs`
  - Library: `lib3dgs_cpp_backend.so` (Linux) / `lib3dgs_cpp_backend.dylib` (macOS)
  - Requirements: CUDA Toolkit, 3dgs-cpp binary

## Building Plugins

### gsplat Backend

To build the gsplat backend as a plugin:

```bash
cargo build --release --features gsplat-plugin --lib
cp target/release/libthree_dgs_processor.so plugins/libgsplat_backend.so
```text

**Note**: In production, you would build a separate crate for each plugin to avoid symbol conflicts.

### 3DGS.cpp Backend

To build the 3dgs-cpp backend as a plugin:

```bash
cargo build --release --features 3dgs-cpp-plugin --lib
cp target/release/libthree_dgs_processor.so plugins/lib3dgs_cpp_backend.so
```text

## Plugin Structure

Each plugin must export a C-ABI function named `create_backend`:

```rust
#[no_mangle]
pub extern "C" fn create_backend() -> Box<dyn GaussianSplatBackend> {
    Box::new(MyBackend::new())
}
```text

## Plugin Search Paths

Plugins are searched in the following locations (in order):

1. `$PLUGIN_PATH` environment variable directory
2. `./plugins/` relative to executable
3. `/usr/local/lib/3dgs-processor/plugins/`

## Using Backends

### Selecting a Backend

Set the `BACKEND` environment variable or configure in YAML:

```bash
# Via environment variable (highest priority)
export BACKEND=gsplat
./3dgs-processor

# Via YAML config
# config.yaml:
# backend: "3dgs-cpp"
```text

### Backend Priority

1. `BACKEND` environment variable (highest)
2. `backend` field in YAML config
3. Default: `gaussian-splatting`

## Current Status

**gsplat Backend Implementation**: Complete (Production-Ready)

The gsplat backend is now **fully implemented** and ready for production use:

- ✅ Complete gsplat CLI argument integration
- ✅ COLMAP data validation
- ✅ PLY export support
- ✅ SPLAT format conversion
- ✅ Multi-format log parsing
- ✅ Comprehensive error handling
- ✅ Unit tests (100% pass rate)

### gsplat Setup

To use the gsplat backend:

1. **Install gsplat Python package**:

   ```bash
   pip install gsplat torch torchvision
   ```

2. **Set up the training script**:
   - Use the reference script: `scripts/gsplat_train.py`
   - Or install gsplat's official training tools
   - Or use the gsplat module directly: `python -m gsplat.train`

3. **Configure environment variables**:

   ```bash
   export BACKEND=gsplat
   export GSPLAT_BIN="scripts/gsplat_train.py"  # or "gsplat.train"
   export GSPLAT_PYTHON="python3"
   ```

4. **For SPLAT export** (optional):

   ```bash
   # If using a custom PLY-to-SPLAT converter
   export PLY_TO_SPLAT_BIN="ply-to-splat"
   ```

### 3DGS.cpp Backend

The 3dgs-cpp backend stub implementation demonstrates the plugin pattern but requires:

- Installation of 3dgs-cpp binary
- Command-line interface integration

**Note**: The gaussian-splatting (reference) backend is always available and fully functional.

## Current Status (Legacy)

**Phase 8 Implementation Status**: Complete (Stub Implementations Replaced)

The plugin system infrastructure is fully implemented:

- ✅ Plugin loader with dynamic library loading
- ✅ Backend registry for managing backends
- ✅ gsplat backend stub implementation
- ✅ 3DGS.cpp backend stub implementation

**Note**: The gsplat and 3dgs-cpp implementations are currently **stubs** that demonstrate the plugin pattern but do not integrate with actual external tools. Full integration requires:

- **gsplat**: Installation of gsplat Python package and proper CLI/Python script integration
- **3dgs-cpp**: Installation of 3dgs-cpp binary and proper command-line interface

## Creating Custom Backends

To create a custom backend:

1. Implement the `GaussianSplatBackend` trait
2. Export a `create_backend` function
3. Compile as a shared library with platform-specific naming
4. Place in a plugin search path

Example minimal backend:

```rust
use async_trait::async_trait;
use three_dgs_processor::backends::{GaussianSplatBackend, BackendOutput, TrainingConfig};

struct MyBackend;

#[async_trait]
impl GaussianSplatBackend for MyBackend {
    async fn train(&self, frames: &[&Path], config: &TrainingConfig) -> Result<BackendOutput> {
        // Your training implementation
    }
    
    async fn export_ply(&self, model: &BackendOutput, output_path: &Path) -> Result<()> {
        // PLY export implementation
    }
    
    async fn export_splat(&self, model: &BackendOutput, output_path: &Path) -> Result<()> {
        // SPLAT export implementation
    }
}

#[no_mangle]
pub extern "C" fn create_backend() -> Box<dyn GaussianSplatBackend> {
    Box::new(MyBackend)
}
```text

## Troubleshooting

### Plugin Not Found

```text
Error: Backend plugin 'gsplat' not found
```text

**Solution**: Ensure the plugin library exists in one of the search paths:

- Check `$PLUGIN_PATH` is set correctly
- Verify library naming: `libgsplat_backend.so` (Linux) or `libgsplat_backend.dylib` (macOS)
- Check file permissions

### Symbol Not Found

```text
Error: Plugin 'gsplat' does not export 'create_backend' function
```text

**Solution**: Verify the plugin exports the required function with correct signature:

- Must use `#[no_mangle]` attribute
- Must use `extern "C"` linkage
- Must return `Box<dyn GaussianSplatBackend>`

### Library Loading Errors

```text
Error: Failed to load plugin library
```text

**Solution**: Check system dependencies:

- Linux: Use `ldd libgsplat_backend.so` to check shared library dependencies
- macOS: Use `otool -L libgsplat_backend.dylib`
- Ensure all required libraries are installed and in library search paths

## Testing

Run backend tests:

```bash
# All backend tests
cargo test --lib backends

# Specific backend tests
cargo test --lib backends::gsplat::tests
cargo test --lib backends::three_dgs_cpp::tests
cargo test --lib backends::registry::tests
```text

## Security Considerations

⚠️ **Plugin loading is inherently unsafe** as it executes arbitrary code from dynamic libraries.

**Best Practices**:

- Only load plugins from trusted sources
- Verify plugin checksums before loading
- Use file permissions to restrict plugin directory write access
- Consider code signing for production plugins

## References

- [Plugin Loader Implementation](../src/backends/plugin_loader.rs)
- [Backend Registry](../src/backends/registry.rs)
- [Backend Trait Definition](../src/backends/mod.rs)
- [3DGS Video Processor PRD](../docs/3dgs-video-processor-prd.md)
