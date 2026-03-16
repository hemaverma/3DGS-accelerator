# COLMAP Package Comparison: Ubuntu Noble apt vs Dockerfile Source Build

## Problem Statement

Determine whether the pre-built `colmap` package from Ubuntu Noble 24.04 (`apt install colmap`) provides the same level of support as the COLMAP built from source in the Dockerfile's `colmap-builder` stage.

## Findings

### Version Comparison

| Aspect | Dockerfile (source build) | Ubuntu Noble (apt) |
|--------|--------------------------|-------------------|
| **Version** | 3.9 | 3.9.1-2build2 |
| **Base OS** | Debian Bookworm | Ubuntu 24.04 (Noble) |
| **Build type** | From source, Release, -O3 | Pre-built binary |
| **CUDA/GPU** | ❌ Not compiled in (no CUDA toolkit in build stage) | ❌ Not included |
| **Install path** | `/usr/local/bin/colmap` | `/usr/bin/colmap` |

### Version Difference: 3.9 vs 3.9.1

The difference is **negligible**. COLMAP 3.9.1 is a patch release over 3.9 with one meaningful change:
- "Fully encapsulate freeimage in bitmap library" (internal refactor, no user-facing impact)

The Ubuntu package (3.9.1) is actually **slightly newer** than the Dockerfile build (3.9).

### GPU/CUDA Support

**Neither version has CUDA support.**

- **Dockerfile:** The `colmap-builder` stage uses `debian:bookworm-slim` which has no CUDA toolkit installed. Without CUDA libraries, CMake's auto-detection will disable GPU features. No `-DCUDA_ENABLED=ON` flag is passed.
- **Ubuntu Noble:** The apt package is explicitly built without CUDA support (standard for distro packages due to licensing/compatibility).

**Conclusion:** Both are CPU-only. Equivalent.

### COLMAP Commands Used by This Codebase

The codebase uses exactly 4 COLMAP commands (from `src/colmap/runner.rs` and `src/colmap/parser.rs`):

| Command | Available in apt? | Available in source build? |
|---------|-------------------|---------------------------|
| `colmap feature_extractor` | ✅ | ✅ |
| `colmap exhaustive_matcher` | ✅ | ✅ |
| `colmap sequential_matcher` | ✅ | ✅ |
| `colmap vocab_tree_matcher` | ✅ | ✅ |
| `colmap mapper` | ✅ | ✅ |
| `colmap model_converter` | ✅ | ✅ |

All commands are core COLMAP functionality, present in every build variant since well before 3.9.

### Dependency Library Versions

| Library | Dockerfile (Bookworm) | Ubuntu Noble |
|---------|----------------------|--------------|
| Boost | 1.74.x | 1.83.x |
| Ceres Solver | libceres3 | libceres4t64 (2.2.0+) |
| FreeImage | libfreeimage3 | libfreeimage3 (3.18+) |
| GLEW | 2.2 | 2.2 |
| glog | 0v6 | 0v6t64 |
| SQLite3 | 3.x | 3.x |
| Qt5 | 5.15.x | 5.15.x |
| FLANN | 1.9 | 1.9 |
| Metis | 5 | 5 |

The Ubuntu Noble packages use newer library versions (especially Boost and Ceres) but these are backwards-compatible for COLMAP's usage.

### Build Optimization

- **Dockerfile:** `-DCMAKE_BUILD_TYPE=Release -DCMAKE_CXX_FLAGS="-O3"` — explicit high optimization
- **Ubuntu apt:** Built with Debian/Ubuntu default release flags (typically `-O2` with hardening flags like `-fstack-protector-strong`, `-D_FORTIFY_SOURCE=2`)

The Dockerfile build may be **marginally faster** due to -O3 vs -O2, but the difference is negligible for this project's use case (CPU-bound COLMAP runs are dominated by algorithmic complexity, not compiler optimization level).

## Conclusion

**The Ubuntu Noble apt package is functionally equivalent (or slightly better) for this project's needs.**

| Criterion | Verdict |
|-----------|---------|
| All required commands available | ✅ Equivalent |
| CPU-only (no CUDA) | ✅ Equivalent (neither has CUDA) |
| Version | ✅ Ubuntu is newer (3.9.1 vs 3.9) |
| Core SfM pipeline | ✅ Equivalent |
| Performance | ≈ Negligible difference (-O3 vs -O2) |
| Library compatibility | ✅ Ubuntu has newer libs |

### Recommendation

**No changes needed.** The E2E test script (`scripts/e2e/00-install-deps.sh`) using `apt install colmap` on Ubuntu Noble provides the same COLMAP capabilities as the Dockerfile source build. The apt package is:
- Same feature set (all commands used by the codebase are present)
- Same CUDA status (neither has GPU support)
- Slightly newer version (3.9.1 vs 3.9)
- Easier to maintain (no source compilation)

### Optional Improvements (Out of Scope)

If CUDA/GPU support is ever needed:
- **Both** the Dockerfile and E2E setup would need changes
- Dockerfile: Add NVIDIA CUDA base image + `-DCUDA_ENABLED=ON`
- E2E: Cannot use apt package; would need source build or NVIDIA PPA

## Todos

No code changes required — this is a research/analysis task confirming compatibility.
