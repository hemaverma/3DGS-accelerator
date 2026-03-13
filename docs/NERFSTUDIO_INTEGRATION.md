# Nerfstudio Integration: Feasibility Analysis

> **Purpose:** Analyze whether [Nerfstudio](https://github.com/nerfstudio-project/nerfstudio/) (specifically its `splatfacto` 3D Gaussian Splatting method) can be integrated as a backend plugin, and what architectural changes are needed.  
> **See also:** [PLUGIN_ARCHITECTURE.md](PLUGIN_ARCHITECTURE.md) (plugin system deep-dive), [ARCHITECTURE.md](ARCHITECTURE.md) (system overview)

---

## Table of Contents

- [Executive Summary](#executive-summary)
- [Pipeline Overlap: This Project vs. Nerfstudio](#pipeline-overlap-this-project-vs-nerfstudio)
- [Current Architecture: Where the Gap Is](#current-architecture-where-the-gap-is)
- [Integration Options](#integration-options)
  - [Option A: Nerfstudio as Training-Only Backend](#option-a-nerfstudio-as-training-only-backend-minimal-change)
  - [Option B: Nerfstudio as Full-Pipeline Backend](#option-b-nerfstudio-as-full-pipeline-backend)
  - [Option C: Fix Implicit Coupling (Recommended)](#option-c-fix-implicit-coupling-recommended)
- [Options Comparison](#options-comparison)
- [Recommended Approach: Detailed Design](#recommended-approach-detailed-design)
- [Implementation Roadmap](#implementation-roadmap)
- [Key Insights](#key-insights)

---

## Executive Summary

**Can Nerfstudio be a plugin?** Yes — but not without addressing an architectural gap.

Nerfstudio's `splatfacto` method is a 3D Gaussian Splatting trainer, which maps directly to the `GaussianSplatBackend` trait. However, Nerfstudio also bundles its own COLMAP invocation, creating overlap with the project's reconstruction step.

**The core problem:** All training backends currently **hardcode** the path `workspace/colmap/sparse/0/` to find camera pose data. The `ReconstructionOutput` from Step 5 is never passed to Step 6. This means any non-COLMAP reconstruction (including Nerfstudio's own) will break the training backends.

**Good news:** The reconstruction step is **already pluggable** via the `ReconstructionBackend` trait. COLMAP does not need to become a plugin — it already is one. The fix is making the training backend *receive* reconstruction output instead of hardcoding where to find it.

---

## Pipeline Overlap: This Project vs. Nerfstudio

```
    This Project's Pipeline               Nerfstudio's Pipeline
    ──────────────────────                 ─────────────────────

    ┌─────────────────┐                    ┌─────────────────┐
    │ 1. File Watcher  │                    │                 │
    │    (notify crate)│                    │  (manual input) │
    └────────┬────────┘                    └────────┬────────┘
             │                                      │
             ▼                                      ▼
    ┌─────────────────┐                    ┌─────────────────┐
    │ 2. Frame Extract │                    │  ns-process-data│
    │    (FFmpeg)      │  ◄── SAME ──►     │  (FFmpeg inside) │
    └────────┬────────┘                    └────────┬────────┘
             │                                      │
             ▼                                      ▼
    ┌─────────────────┐                    ┌─────────────────┐
    │ 3. COLMAP Sparse │                    │  COLMAP (bundled│
    │  Reconstruction  │  ◄── SAME ──►     │  in ns-process  │
    │  (Step 5)        │                    │  -data)         │
    └────────┬────────┘                    └────────┬────────┘
             │                                      │
             ▼                                      ▼
    ┌─────────────────┐                    ┌─────────────────┐
    │ 4. 3DGS Training │                    │  ns-train       │
    │  (GaussianSplat  │  ◄── SAME ──►     │  splatfacto     │
    │   Backend trait)  │    PURPOSE        │                 │
    └────────┬────────┘                    └────────┬────────┘
             │                                      │
             ▼                                      ▼
    ┌─────────────────┐                    ┌─────────────────┐
    │ 5. PLY/SPLAT     │                    │  ns-export      │
    │    Export         │  ◄── SAME ──►     │                 │
    └─────────────────┘                    └─────────────────┘
```

**Key overlap points:**

| Stage | This Project | Nerfstudio | Overlap |
|-------|-------------|------------|---------|
| Frame extraction | FFmpeg via `spawn_blocking` | FFmpeg via `ns-process-data` | Identical purpose |
| Reconstruction | COLMAP (or alternative via `ReconstructionBackend`) | COLMAP bundled in `ns-process-data` (skippable with `--skip-colmap`) | Identical purpose; Nerfstudio can accept pre-computed COLMAP |
| Training | `GaussianSplatBackend::train()` | `ns-train splatfacto` | Same 3DGS training, different implementation |
| Export | `export_ply()` / `export_splat()` | `ns-export` | Same formats |

**Nerfstudio's `--skip-colmap` flag** is critical: it allows Nerfstudio to accept this project's COLMAP output without re-running reconstruction.

---

## Current Architecture: Where the Gap Is

The project has **two pluggable stages**, but they are connected by an **implicit filesystem convention** rather than an explicit data handoff:

### The Two Plugin Traits

| Stage | Trait | Location | Pluggable? |
|-------|-------|----------|-----------|
| **Step 5: Reconstruction** | `ReconstructionBackend` | `src/reconstruction/backend.rs` | ✅ Yes — COLMAP, Nerfstudio, Precalibrated |
| **Step 6: Training** | `GaussianSplatBackend` | `src/backends/mod.rs` | ✅ Yes — gaussian-splatting, gsplat, 3dgs-cpp, mock |

### The Implicit Coupling Problem

```
    Step 5 (Reconstruction)                     Step 6 (Training)
    ───────────────────────                     ─────────────────

    ReconstructionBackend                       GaussianSplatBackend
        .reconstruct()                              .train()
              │                                          │
              ▼                                          ▼
    ┌──────────────────────┐               ┌──────────────────────┐
    │  ReconstructionOutput │               │  Receives ONLY:      │
    │  ├── output_dir       │               │  ├── frames: &[&Path]│
    │  ├── cameras []       │──── NOT ────► │  └── config          │
    │  ├── sparse_point_cloud│   PASSED     │                      │
    │  └── stats            │               │  Must DISCOVER colmap│
    └──────────────────────┘               │  files on its own!   │
                                            └──────────────────────┘

    ✗ ReconstructionOutput is logged         Backend hardcodes:
      then effectively discarded.            workspace/colmap/sparse/0/
                                             ├── cameras.bin
                                             ├── images.bin
                                             └── points3D.bin
```

**Where the hardcoding happens** (all three training backends):

| Backend | File | Line | Hardcoded Path |
|---------|------|------|----------------|
| gaussian-splatting | `src/backends/gaussian_splatting.rs` | 167 | `workspace_dir.join("colmap").join("sparse").join("0")` |
| gsplat | `src/backends/gsplat.rs` | 274 | `workspace_dir.join("colmap").join("sparse").join("0")` |
| 3dgs-cpp | `src/backends/three_dgs_cpp.rs` | 207 | `workspace_dir.join("colmap").join("sparse").join("0")` |

**Consequence:** Any reconstruction backend that doesn't produce COLMAP binary files at exactly `colmap/sparse/0/` will cause training to fail. This blocks:
- Nerfstudio reconstruction (produces `transforms.json`, not COLMAP binaries)
- Precalibrated cameras (no COLMAP files at all)
- Any future non-COLMAP reconstruction method

---

## Integration Options

### Option A: Nerfstudio as Training-Only Backend (Minimal Change)

**Approach:** Create a `NerfstudioSplatfactoBackend` that implements `GaussianSplatBackend` and uses the **existing COLMAP output** from Step 5. Nerfstudio's `ns-train splatfacto` accepts COLMAP data directly.

**How it works:**
1. This project runs COLMAP as usual (Step 5)
2. The Nerfstudio backend finds COLMAP files at the hardcoded path (same as other backends)
3. Converts COLMAP data to Nerfstudio format (`ns-process-data --skip-colmap`)
4. Runs `ns-train splatfacto --data <processed_dir>`
5. Exports model to PLY/SPLAT

```rust
// src/backends/nerfstudio_splatfacto.rs
pub struct NerfstudioSplatfactoBackend;

#[async_trait]
impl GaussianSplatBackend for NerfstudioSplatfactoBackend {
    async fn train(&self, frames: &[&Path], config: &TrainingConfig) -> Result<BackendOutput> {
        // 1. Infer workspace from frame paths (same as other backends)
        let workspace_dir = frames[0].parent().unwrap().parent().unwrap();
        let colmap_dir = workspace_dir.join("colmap").join("sparse").join("0");

        // 2. Convert COLMAP to Nerfstudio format
        // ns-process-data images --data <images> --skip-colmap --colmap-model-path <colmap_dir>
        
        // 3. Train splatfacto
        // ns-train splatfacto --data <processed>
        
        // 4. Return BackendOutput with model path
    }
}
```

**Pros:**
- ✅ No architecture changes — works with existing trait signature
- ✅ Simple to implement — just a new backend file
- ✅ Can be a plugin (dynamic .so) or built-in

**Cons:**
- ❌ Doesn't fix the underlying implicit coupling
- ❌ Still hardcodes COLMAP path lookup
- ❌ Cannot leverage Nerfstudio's own reconstruction
- ❌ Requires `ns-process-data` conversion step (redundant work)

**Effort:** Low

---

### Option B: Nerfstudio as Full-Pipeline Backend

**Approach:** Let the Nerfstudio backend handle **both** reconstruction and training. Skip Step 5 entirely when using Nerfstudio.

**How it works:**
1. Nerfstudio backend receives frames
2. Runs `ns-process-data images --data <frames_dir>` (internally runs COLMAP)
3. Runs `ns-train splatfacto --data <processed_dir>`
4. Exports model to PLY/SPLAT

```rust
pub struct NerfstudioSplatfactoBackend {
    run_own_reconstruction: bool,  // true = skip Step 5
}

#[async_trait]
impl GaussianSplatBackend for NerfstudioSplatfactoBackend {
    async fn train(&self, frames: &[&Path], config: &TrainingConfig) -> Result<BackendOutput> {
        if self.run_own_reconstruction {
            // Run ns-process-data (includes COLMAP internally)
            // Then ns-train splatfacto
        } else {
            // Use pre-computed COLMAP (Option A behavior)
        }
    }
}
```

**Requires pipeline change:** The processor (job.rs) would need logic to skip Step 5 when the training backend declares it handles reconstruction:

```rust
// job.rs - Step 5
if !backend.needs_external_reconstruction() {
    // Skip COLMAP, let backend handle it
} else {
    // Run COLMAP as usual
}
```

**Pros:**
- ✅ Leverages Nerfstudio's full pipeline
- ✅ No redundant COLMAP runs
- ✅ Nerfstudio can use its own optimized data processing

**Cons:**
- ❌ Requires adding a method to the backend trait (`needs_external_reconstruction()`)
- ❌ Couples reconstruction logic into the training backend
- ❌ Harder for plugin authors to understand two modes
- ❌ Still doesn't fix the implicit coupling for other backends

**Effort:** Medium

---

### Option C: Fix Implicit Coupling (Recommended)

**Approach:** Fix the underlying architectural problem by passing reconstruction output explicitly to the training backend. Then Nerfstudio (or any backend) can work with any reconstruction method.

**How it works:**
1. Create a `ReconstructionContext` type that wraps reconstruction output
2. Add it as a parameter to `GaussianSplatBackend::train()`
3. Remove hardcoded COLMAP path discovery from all backends
4. Processor (job.rs) passes reconstruction output to training backend

```rust
// New type in src/backends/models.rs
pub struct ReconstructionContext {
    /// Path to reconstruction output directory
    pub reconstruction_dir: PathBuf,
    
    /// Format of the reconstruction data
    /// Values: "colmap", "nerfstudio", "precalibrated"
    pub format: String,
    
    /// Camera registration data (if available)
    pub cameras: Vec<ImageRegistration>,
    
    /// Path to sparse point cloud (if available)
    pub sparse_point_cloud: Option<PathBuf>,
}

// Updated trait
#[async_trait]
pub trait GaussianSplatBackend: Send + Sync {
    async fn train(
        &self,
        frames: &[&Path],
        config: &TrainingConfig,
        reconstruction: &ReconstructionContext,  // NEW parameter
    ) -> Result<BackendOutput>;

    async fn export_ply(&self, model: &BackendOutput, output_path: &Path) -> Result<()>;
    async fn export_splat(&self, model: &BackendOutput, output_path: &Path) -> Result<()>;
}
```

**Processor change** (job.rs):

```rust
// Step 5: Reconstruction (already works)
let reconstruction_output = reconstruction_backend
    .reconstruct(&image_dir, &output_dir, &config)
    .await?;

// Step 6: Training — NOW passes reconstruction context
let reconstruction_context = ReconstructionContext {
    reconstruction_dir: reconstruction_output.output_dir,
    format: reconstruction_output.stats.backend_name,
    cameras: reconstruction_output.cameras,
    sparse_point_cloud: reconstruction_output.sparse_point_cloud,
};

let model_output = backend
    .train(&frame_paths, &training_config, &reconstruction_context)  // PASSES CONTEXT
    .await?;
```

**Backend change** (e.g., gaussian_splatting.rs):

```rust
// BEFORE: hardcoded path discovery
let colmap_sparse_dir = workspace_dir.join("colmap").join("sparse").join("0");

// AFTER: uses passed context
let colmap_sparse_dir = reconstruction.reconstruction_dir.clone();
// Optionally validate format:
if reconstruction.format != "colmap" {
    anyhow::bail!("gaussian-splatting backend requires COLMAP reconstruction, got: {}", 
                  reconstruction.format);
}
```

**Nerfstudio backend** can then work in two modes:

```rust
pub struct NerfstudioSplatfactoBackend;

#[async_trait]
impl GaussianSplatBackend for NerfstudioSplatfactoBackend {
    async fn train(
        &self,
        frames: &[&Path],
        config: &TrainingConfig,
        reconstruction: &ReconstructionContext,
    ) -> Result<BackendOutput> {
        match reconstruction.format.as_str() {
            "colmap" => {
                // Convert COLMAP to Nerfstudio format, then train
                // ns-process-data --skip-colmap --colmap-model-path <dir>
                // ns-train splatfacto --data <processed>
            }
            "nerfstudio" => {
                // Already in Nerfstudio format, train directly
                // ns-train splatfacto --data <dir>
            }
            _ => {
                anyhow::bail!("Unsupported reconstruction format: {}", reconstruction.format);
            }
        }
    }
}
```

**Pros:**
- ✅ Fixes the fundamental architectural problem
- ✅ Enables ANY combination of reconstruction + training backend
- ✅ Nerfstudio reconstruction (transforms.json) can now feed into training backends
- ✅ Precalibrated cameras can now work with training backends
- ✅ Plugin authors get explicit data contracts (no hidden filesystem conventions)
- ✅ All in-tree backends can be updated in one coordinated change

**Cons:**
- ⚠️ Breaking trait change — all `GaussianSplatBackend` implementations must be updated
- ⚠️ External plugins (if any exist) would need to update
- ⚠️ Slightly more complex trait signature

**Effort:** Medium-High (but fixes the root cause)

---

## Options Comparison

| Criterion | Option A | Option B | Option C (Recommended) |
|-----------|----------|----------|----------------------|
| **Architecture change** | None | Small (add method) | Medium (new param) |
| **Fixes implicit coupling** | ❌ No | ❌ No | ✅ Yes |
| **Nerfstudio integration** | Partial (training only) | Full | Full |
| **Non-COLMAP reconstruction** | ❌ Broken | ❌ Still broken | ✅ Fixed |
| **Plugin author experience** | Hidden conventions | Two modes to understand | Explicit data contract |
| **Breaking change** | None | Minor | `train()` signature |
| **Effort** | Low | Medium | Medium-High |
| **Future extensibility** | Poor | OK | Excellent |

**Recommendation:** **Option C** is the right long-term choice. It fixes a real architectural issue that currently prevents non-COLMAP reconstruction backends from working. The breaking change is manageable since all implementations are in-tree.

If you need Nerfstudio working quickly without refactoring, **Option A** is a valid stopgap — it can be implemented in a single file with no changes to existing code.

---

## Recommended Approach: Detailed Design

### New Types

```rust
// src/backends/models.rs

/// Context from the reconstruction stage, passed to training backends.
///
/// Provides explicit access to reconstruction output instead of requiring
/// backends to discover COLMAP files by filesystem convention.
#[derive(Debug, Clone)]
pub struct ReconstructionContext {
    /// Path to reconstruction output directory
    /// - COLMAP: contains sparse/0/{cameras.bin, images.bin, points3D.bin}
    /// - Nerfstudio: contains transforms.json
    /// - Precalibrated: contains camera poses in project format
    pub reconstruction_dir: PathBuf,

    /// Format identifier for the reconstruction data
    /// Known values: "colmap", "nerfstudio", "precalibrated"
    pub format: String,

    /// Parsed camera registrations (if available)
    /// Empty if the reconstruction backend didn't parse cameras
    pub cameras: Vec<ImageRegistration>,

    /// Path to sparse point cloud file (if available)
    /// Typically only present for COLMAP reconstructions
    pub sparse_point_cloud: Option<PathBuf>,
}
```

### Updated Trait Signature

```rust
#[async_trait]
pub trait GaussianSplatBackend: Send + Sync {
    /// Train a 3DGS model from extracted frames and reconstruction data.
    async fn train(
        &self,
        frames: &[&Path],
        config: &TrainingConfig,
        reconstruction: &ReconstructionContext,
    ) -> Result<BackendOutput>;

    /// Export to PLY format (unchanged)
    async fn export_ply(&self, model: &BackendOutput, output_path: &Path) -> Result<()>;

    /// Export to SPLAT format (unchanged)
    async fn export_splat(&self, model: &BackendOutput, output_path: &Path) -> Result<()>;
}
```

### Files to Modify

| File | Change |
|------|--------|
| `src/backends/models.rs` | Add `ReconstructionContext` struct |
| `src/backends/mod.rs` | Update `train()` signature, re-export new type |
| `src/backends/gaussian_splatting.rs` | Use `reconstruction.reconstruction_dir` instead of hardcoded path |
| `src/backends/gsplat.rs` | Same — remove hardcoded COLMAP discovery |
| `src/backends/three_dgs_cpp.rs` | Same — remove hardcoded COLMAP discovery |
| `src/backends/mock.rs` | Accept (and ignore) reconstruction context |
| `src/backends/registry.rs` | No change (registry doesn't call `train()`) |
| `src/processor/job.rs` | Build `ReconstructionContext` from `ReconstructionOutput`, pass to `backend.train()` |
| `src/backends/nerfstudio_splatfacto.rs` | **New file** — Nerfstudio splatfacto backend |
| `docs/PLUGIN_ARCHITECTURE.md` | Update trait docs and plugin guide |
| `tests/` | Update all tests calling `backend.train()` |

---

## Implementation Roadmap

### Phase 1: Fix the Coupling (Foundation)
1. Add `ReconstructionContext` to `src/backends/models.rs`
2. Update `GaussianSplatBackend::train()` signature
3. Update all existing backends to use the new parameter
4. Update `src/processor/job.rs` to pass reconstruction context
5. Fix all tests

### Phase 2: Nerfstudio Backend
1. Create `src/backends/nerfstudio_splatfacto.rs`
2. Implement COLMAP-input mode (accept this project's COLMAP output)
3. Implement Nerfstudio-input mode (accept transforms.json)
4. Register in `BackendRegistry`
5. Add tests

### Phase 3: Documentation
1. Update `docs/PLUGIN_ARCHITECTURE.md` with new trait signature
2. Add Nerfstudio as example in plugin guide
3. Document reconstruction format negotiation

---

## Key Insights

1. **COLMAP is already pluggable.** The `ReconstructionBackend` trait in `src/reconstruction/` already supports COLMAP, Nerfstudio (transforms.json), and precalibrated cameras. No new plugin system needed for reconstruction.

2. **The real problem is the data handoff.** The `ReconstructionOutput` from Step 5 is logged but never forwarded to Step 6. Training backends discover COLMAP files by hardcoded filesystem convention.

3. **Nerfstudio maps to TWO existing traits:**
   - `ReconstructionBackend` — already exists as `NerfstudioBackend` in `src/reconstruction/nerfstudio.rs` (reads transforms.json)
   - `GaussianSplatBackend` — needs a new `NerfstudioSplatfactoBackend` (runs `ns-train splatfacto`)

4. **Nerfstudio supports `--skip-colmap`.** This means it can accept this project's COLMAP output without re-running reconstruction. This is the key enabler for a clean integration.

5. **The trait change is breaking but safe.** All `GaussianSplatBackend` implementations are in-tree (no external plugins in the wild yet), so the breaking change can be done as a single coordinated commit.
