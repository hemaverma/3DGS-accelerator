---
applyTo: '**/*.rs, **/Cargo.toml, **/Cargo.lock'
description: 'Idiomatic Rust best practices and conventions'
maturity: stable
---

# Rust Best Practices

Conventions for writing idiomatic Rust code following the [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/).

## Naming Conventions

| Element | Convention | Example |
|---------|------------|---------|
| Crates | `kebab-case` | `video-processor` |
| Modules | `snake_case` | `file_watcher` |
| Types | `PascalCase` | `VideoProcessor` |
| Traits | `PascalCase` | `Processable` |
| Enums | `PascalCase` | `ProcessingState` |
| Enum variants | `PascalCase` | `State::Processing` |
| Functions | `snake_case` | `process_video` |
| Methods | `snake_case` | `extract_frames` |
| Local variables | `snake_case` | `frame_count` |
| Static variables | `SCREAMING_SNAKE_CASE` | `MAX_RETRIES` |
| Constant values | `SCREAMING_SNAKE_CASE` | `DEFAULT_TIMEOUT` |
| Type parameters | `PascalCase` | `T`, `TResult` |
| Lifetimes | `'lowercase` | `'a`, `'input` |

## Project Structure

```text
my-project/
├── Cargo.toml
├── Cargo.lock
├── src/
│   ├── main.rs           # Binary entry point
│   ├── lib.rs            # Library root (if dual binary/library)
│   ├── bin/              # Additional binaries
│   │   └── worker.rs
│   └── module_name/      # Submodules
│       ├── mod.rs
│       ├── types.rs
│       └── utils.rs
├── tests/                # Integration tests
│   └── integration_test.rs
├── benches/              # Benchmarks
│   └── benchmark.rs
└── examples/             # Example usage
    └── simple.rs
```text

## Error Handling

### Library Crates: Use `thiserror`

```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum VideoError {
    #[error("Failed to open video file: {path}")]
    OpenFailed {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("Invalid frame rate: {0}")]
    InvalidFrameRate(f32),

    #[error("Processing failed")]
    ProcessingFailed(#[from] ProcessorError),
}

pub type Result<T> = std::result::Result<T, VideoError>;
```text

### Application Crates: Use `anyhow`

```rust
use anyhow::{Context, Result, bail, ensure};

fn process_file(path: &Path) -> Result<()> {
    let file = File::open(path)
        .context("Failed to open input file")?;
    
    ensure!(file.metadata()?.len() > 0, "File is empty");
    
    if !path.exists() {
        bail!("Path does not exist: {}", path.display());
    }
    
    Ok(())
}
```text

### Error Handling Best Practices

- Use `?` operator for propagation
- Add context with `.context()` for debugging
- Don't use `unwrap()` or `expect()` in library code
- Use `expect()` with descriptive messages in application code when invariant is guaranteed
- Implement `std::error::Error` for custom error types (via `thiserror`)

## Async Programming

### Runtime Setup

```rust
// Binary with tokio macro
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Application code
    Ok(())
}

// Or manual runtime
fn main() -> anyhow::Result<()> {
    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async_main())
}
```text

### Blocking Operations

Always use `spawn_blocking` for CPU-bound or blocking I/O:

```rust
async fn process_data(data: Vec<u8>) -> Result<Output> {
    tokio::task::spawn_blocking(move || {
        // CPU-intensive work or blocking I/O
        expensive_computation(data)
    })
    .await
    .context("Task panicked")?
}
```text

### Concurrent Operations

```rust
use futures::stream::{self, StreamExt};
use tokio::task::JoinSet;

// Process multiple items with bounded concurrency
async fn process_many(items: Vec<Item>) -> Vec<Result<Output>> {
    stream::iter(items)
        .map(|item| async move { process_item(item).await })
        .buffer_unordered(10)  // Max 10 concurrent
        .collect()
        .await
}

// Or use JoinSet for dynamic task spawning
async fn spawn_tasks(items: Vec<Item>) -> Result<Vec<Output>> {
    let mut set = JoinSet::new();
    
    for item in items {
        set.spawn(async move { process_item(item).await });
    }
    
    let mut results = Vec::new();
    while let Some(result) = set.join_next().await {
        results.push(result??);
    }
    Ok(results)
}
```text

### Select and Timeouts

```rust
use tokio::{select, time::{timeout, Duration}};

async fn with_timeout(input: Input) -> Result<Output> {
    timeout(Duration::from_secs(30), process(input))
        .await
        .context("Operation timed out")?
}

async fn process_with_cancellation(rx: Receiver<()>) -> Result<Output> {
    select! {
        result = do_work() => result,
        _ = rx.recv() => {
            Err(anyhow!("Operation cancelled"))
        }
    }
}
```text

## Type Design

### Use NewType Pattern

```rust
// Strong typing to prevent mistakes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UserId(u64);

impl UserId {
    pub fn new(id: u64) -> Self {
        Self(id)
    }
    
    pub fn as_u64(&self) -> u64 {
        self.0
    }
}
```text

### Builder Pattern

```rust
pub struct Config {
    pub host: String,
    pub port: u16,
    pub timeout: Duration,
}

pub struct ConfigBuilder {
    host: String,
    port: u16,
    timeout: Duration,
}

impl ConfigBuilder {
    pub fn new(host: impl Into<String>) -> Self {
        Self {
            host: host.into(),
            port: 8080,
            timeout: Duration::from_secs(30),
        }
    }
    
    pub fn port(mut self, port: u16) -> Self {
        self.port = port;
        self
    }
    
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }
    
    pub fn build(self) -> Config {
        Config {
            host: self.host,
            port: self.port,
            timeout: self.timeout,
        }
    }
}
```text

### Enums for State

```rust
pub enum ProcessingState {
    Pending,
    InProgress { started_at: Instant },
    Completed { result: Output },
    Failed { error: String },
}

// Use pattern matching
match state {
    ProcessingState::Pending => { /* ... */ }
    ProcessingState::InProgress { started_at } => { /* ... */ }
    ProcessingState::Completed { result } => { /* ... */ }
    ProcessingState::Failed { error } => { /* ... */ }
}
```text

## Ownership and Borrowing

### Accept Borrowed Types

```rust
// Good: Accept &str and &Path
fn process_name(name: &str) -> Result<()> { /* ... */ }
fn read_file(path: &Path) -> Result<String> { /* ... */ }

// Not ideal: Forces caller to allocate
fn process_name(name: String) -> Result<()> { /* ... */ }
fn read_file(path: PathBuf) -> Result<String> { /* ... */ }
```text

### Return Owned Types

```rust
// Good: Return owned types
fn load_data() -> Result<Vec<Item>> { /* ... */ }
fn get_name() -> String { /* ... */ }

// Let caller decide what to do with the data
```text

### Use Cow for Flexible Ownership

```rust
use std::borrow::Cow;

fn process_text(text: &str) -> Cow<str> {
    if needs_modification(text) {
        Cow::Owned(modify(text))
    } else {
        Cow::Borrowed(text)
    }
}
```text

## Traits

### Standard Trait Implementations

```rust
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MyType {
    // fields
}

// Implement Display for user-facing output
impl std::fmt::Display for MyType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "MyType {{ ... }}")
    }
}

// Implement From/Into for conversions
impl From<String> for MyType {
    fn from(s: String) -> Self {
        // conversion logic
    }
}
```text

### Async Traits

```rust
use async_trait::async_trait;

#[async_trait]
pub trait DataStore: Send + Sync {
    async fn save(&self, key: &str, value: &[u8]) -> Result<()>;
    async fn load(&self, key: &str) -> Result<Vec<u8>>;
}
```text

## Testing

### Unit Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_functionality() {
        let result = process_data(input);
        assert_eq!(result, expected);
    }

    #[test]
    fn test_error_case() {
        let result = process_invalid_data();
        assert!(result.is_err());
    }

    // Async tests
    #[tokio::test]
    async fn test_async_operation() {
        let result = async_process().await;
        assert!(result.is_ok());
    }
}
```text

### Integration Tests

```rust
// tests/integration_test.rs
use my_crate::*;

#[test]
fn test_full_workflow() {
    let processor = Processor::new();
    let result = processor.run_pipeline(input);
    assert_eq!(result, expected_output);
}
```text

### Property-Based Testing

```rust
use proptest::prelude::*;

proptest! {
    #[test]
    fn test_reversible(s in "\\PC*") {
        let encoded = encode(&s);
        let decoded = decode(&encoded)?;
        prop_assert_eq!(s, decoded);
    }
}
```text

### Test Mocking

```rust
#[cfg(test)]
use mockall::automock;

#[cfg_attr(test, automock)]
pub trait FileSystem {
    fn read_file(&self, path: &Path) -> Result<String>;
    fn write_file(&self, path: &Path, content: &str) -> Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_with_mock() {
        let mut mock = MockFileSystem::new();
        mock.expect_read_file()
            .returning(|_| Ok("test content".to_string()));
        
        let result = process_with_fs(&mock);
        assert!(result.is_ok());
    }
}
```text

## Documentation

### Doc Comments

```rust
/// Processes a video file and extracts frames.
///
/// This function reads the video at the specified path and extracts
/// individual frames based on the provided configuration.
///
/// # Arguments
///
/// * `path` - Path to the video file
/// * `config` - Configuration for frame extraction
///
/// # Returns
///
/// Returns a `Vec<Frame>` containing all extracted frames.
///
/// # Errors
///
/// Returns an error if:
/// - The video file cannot be opened
/// - The video format is unsupported
/// - Frame extraction fails
///
/// # Examples
///
/// ```
/// use my_crate::{extract_frames, Config};
/// use std::path::Path;
///
/// let path = Path::new("video.mp4");
/// let config = Config::default();
/// let frames = extract_frames(path, &config)?;
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn extract_frames(path: &Path, config: &Config) -> Result<Vec<Frame>> {
    // Implementation
}
```text

### Module Documentation

```rust
//! Video processing utilities.
//!
//! This module provides functions and types for processing video files,
//! including frame extraction, metadata parsing, and format conversion.
//!
//! # Examples
//!
//! ```
//! use video_processor::extract_frames;
//! ```

pub mod extraction;
pub mod metadata;
```text

## Performance Best Practices

### Use References to Avoid Cloning

```rust
// Avoid unnecessary clones
fn process_items(items: &[Item]) -> Result<()> {
    for item in items {
        process_item(item)?;
    }
    Ok(())
}
```text

### Pre-allocate Collections

```rust
// Good: pre-allocate with known capacity
let mut results = Vec::with_capacity(items.len());
for item in items {
    results.push(process(item));
}

// Use collect when transforming iterators
let results: Vec<_> = items.iter()
    .map(|item| process(item))
    .collect();
```text

### Use Iterators Instead of Loops

```rust
// Prefer iterator chains
let total: u32 = data.iter()
    .filter(|x| x.is_valid())
    .map(|x| x.value())
    .sum();

// Instead of manual loops
let mut total = 0;
for item in &data {
    if item.is_valid() {
        total += item.value();
    }
}
```text

### Avoid String Allocations

```rust
// Good: work with string slices
fn count_words(text: &str) -> usize {
    text.split_whitespace().count()
}

// Avoid format! in hot paths
let path = format!("{}/{}", dir, file);  // Allocates

// Consider using a buffer or write! macro instead
use std::fmt::Write;
let mut path = String::new();
write!(&mut path, "{}/{}", dir, file)?;
```text

## Common Patterns

### RAII for Resource Management

```rust
pub struct FileGuard {
    path: PathBuf,
}

impl FileGuard {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }
}

impl Drop for FileGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}
```text

### Extension Traits

```rust
pub trait PathExt {
    fn is_video_file(&self) -> bool;
}

impl PathExt for Path {
    fn is_video_file(&self) -> bool {
        matches!(
            self.extension().and_then(|s| s.to_str()),
            Some("mp4" | "mov" | "avi")
        )
    }
}
```text

### Type State Pattern

```rust
pub struct Connection<State> {
    addr: String,
    state: State,
}

pub struct Disconnected;
pub struct Connected { socket: TcpStream }

impl Connection<Disconnected> {
    pub fn new(addr: String) -> Self {
        Self { addr, state: Disconnected }
    }
    
    pub async fn connect(self) -> Result<Connection<Connected>> {
        let socket = TcpStream::connect(&self.addr).await?;
        Ok(Connection {
            addr: self.addr,
            state: Connected { socket },
        })
    }
}

impl Connection<Connected> {
    pub async fn send(&mut self, data: &[u8]) -> Result<()> {
        self.state.socket.write_all(data).await?;
        Ok(())
    }
}
```text

## Cargo Best Practices

### Dependency Management

```toml
[dependencies]
# Prefer caret requirements (default)
serde = "1.0"
tokio = { version = "1.35", features = ["full"] }

# Use workspace dependencies for multi-crate projects
[workspace.dependencies]
anyhow = "1.0"
tokio = { version = "1.35", features = ["full"] }

[dependencies]
anyhow.workspace = true
tokio.workspace = true
```text

### Feature Flags

```toml
[features]
default = ["json"]
json = ["dep:serde_json"]
xml = ["dep:quick-xml"]
full = ["json", "xml"]
```text

```rust
#[cfg(feature = "json")]
pub fn to_json(data: &Data) -> Result<String> {
    serde_json::to_string(data)
}
```text

### Profile Configuration

```toml
[profile.release]
opt-level = 3
lto = "thin"
codegen-units = 1
strip = true

[profile.dev]
opt-level = 0
debug = true

[profile.dev.package."*"]
opt-level = 2  # Optimize dependencies in dev
```text

## Common Anti-Patterns to Avoid

❌ **Unnecessary String allocations**
```rust
// Bad
fn get_name() -> String {
    String::from("default")
}

// Good - return &str when possible
fn get_name() -> &'static str {
    "default"
}
```text

❌ **Using `.clone()` everywhere**
```rust
// Bad - unnecessary clone
fn process(data: Vec<Item>) {
    for item in data.clone() {  // Clone entire vec!
        // ...
    }
}

// Good - iterate by reference
fn process(data: &[Item]) {
    for item in data {
        // ...
    }
}
```text

❌ **Ignoring errors with `unwrap()`**
```rust
// Bad - will panic in production
let file = File::open(path).unwrap();

// Good - handle errors
let file = File::open(path)
    .context("Failed to open file")?;
```text

❌ **Not using iterators**
```rust
// Bad - explicit indexing is error-prone
for i in 0..vec.len() {
    process(&vec[i]);
}

// Good - use iterators
for item in &vec {
    process(item);
}
```text

## Tools

```bash
# Format code
cargo fmt

# Lint code
cargo clippy -- -D warnings

# Run tests
cargo test
cargo test --all-features

# Check without building
cargo check

# Build documentation
cargo doc --open

# Audit dependencies
cargo audit

# Check for outdated dependencies
cargo outdated
```text

## Resources

- [Rust Book](https://doc.rust-lang.org/book/)
- [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/)
- [Effective Rust](https://www.lurklurk.org/effective-rust/)
- [Rust Performance Book](https://nnethercote.github.io/perf-book/)
- [Tokio Tutorial](https://tokio.rs/tokio/tutorial)
- [Async Book](https://rust-lang.github.io/async-book/)
