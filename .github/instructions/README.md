# Copilot Instructions

AI agent guidance files for the 3DGS Video Processor project.

## File Organization

### [/.github/copilot-instructions.md](../copilot-instructions.md)
**Project-specific architecture patterns**
- Multi-video processing implementation
- Plugin-based backend loading
- File watching with stability detection
- Testing strategy for this project
- Project structure and conventions unique to 3DGS processor

### [rust/rust.instructions.md](rust/rust.instructions.md)
**Idiomatic Rust best practices**
- Naming conventions (snake_case, PascalCase, etc.)
- Error handling (anyhow vs thiserror)
- Async programming patterns
- Testing and documentation
- Performance optimization
- Common Rust idioms and anti-patterns

### [e2e-testing.instructions.md](e2e-testing.instructions.md)
**End-to-end testing setup and troubleshooting**
- E2E shell scripts (numbered 00–04)
- Test data sourcing (COLMAP South Building dataset)
- Pipeline env vars for CPU-only testing
- COLMAP performance tuning (matcher, features, resolution)
- Common failure modes and fixes

## How They Work Together

```text
┌─────────────────────────────────────────────────┐
│ copilot-instructions.md                         │
│ "Use Vec<VideoInput> for multi-video jobs"     │
│ "Load backends as plugins with libloading"     │
└────────────────┬────────────────────────────────┘
                 │ References
                 ▼
┌─────────────────────────────────────────────────┐
│ rust/rust.instructions.md                       │
│ "Use spawn_blocking for CPU-bound work"        │
│ "Accept &str parameters, return owned String"  │
└─────────────────────────────────────────────────┘
```text

**When coding:**
1. Check **copilot-instructions.md** for project-specific patterns
2. Reference **rust.instructions.md** for general Rust best practices
3. See **[docs/3dgs-video-processor-prd.md](../../docs/3dgs-video-processor-prd.md)** for requirements

## Frontmatter

Files use YAML frontmatter to specify where they apply:

```yaml
---
applyTo: '**/*.rs, **/Cargo.toml'
description: 'Instructions for Rust development'
maturity: stable
---
```text

AI agents automatically load applicable instruction files based on the `applyTo` pattern.
