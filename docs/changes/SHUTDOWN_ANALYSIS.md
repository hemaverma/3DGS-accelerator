# Shutdown and Signal Handling Flow Analysis

## 1. SHUTDOWN COORDINATOR & SIGNAL HANDLING
**Location**: `src/shutdown/signal_handler.rs` and `src/shutdown/mod.rs`

### ShutdownCoordinator Structure
```rust
pub struct ShutdownCoordinator {
    shutdown_flag: ShutdownFlag,           // Arc<AtomicBool>
    shutdown_notify: Arc<Notify>,          // tokio::sync::Notify
}
```

### ShutdownFlag (atomic)
```rust
pub struct ShutdownFlag {
    inner: Arc<AtomicBool>,
}
```
- **Load** (read): Uses `Ordering::Relaxed` for quick checks
- **Store** (write): Uses `Ordering::Relaxed` - no synchronization guarantees

### Key Methods

#### wait_for_shutdown_signal()
- Uses `tokio::select!` to await SIGTERM or SIGINT
- Sets `shutdown_flag` to true when signal received
- Notifies all waiters via `shutdown_notify.notify_waiters()`

#### shutdown_with_timeout()
- Wraps cleanup function with `tokio::time::timeout(5 minutes)`
- **CRITICAL**: If cleanup exceeds 5 minutes, returns error (but main still exits)

### Shutdown Flow
```
SIGTERM/SIGINT
    ↓
tokio::select! in wait_for_shutdown_signal()
    ↓
shutdown_flag.request_shutdown() [sets AtomicBool = true]
shutdown_notify.notify_waiters()
    ↓
main() waits via shutdown_coordinator.wait_for_shutdown()
    ↓
shutdown_coordinator.shutdown_with_timeout(cleanup_fn)
    ↓
Tasks check shutdown_flag.is_shutdown_requested() periodically
```

---

## 2. FILE WATCHING - POTENTIAL HANG POINTS
**Location**: `src/watcher/mod.rs`, `src/watcher/notify_watcher.rs`, `src/watcher/poll_watcher.rs`

### detect_new_folder() - HIGH RISK ⚠️
**File**: `src/watcher/mod.rs:33-62`

```rust
pub async fn detect_new_folder(watch_path: &Path, poll_interval: Duration) -> Result<PathBuf> {
    // Try inotify-based watcher first
    match notify_watcher::watch_for_new_folder(watch_path).await {
        Ok(folder) => Ok(folder),
        Err(e) => {
            // Fall back to polling
            poll_watcher::watch_for_new_folder(watch_path, poll_interval).await
        }
    }
}
```

#### **CRITICAL ISSUE 1: NO SHUTDOWN CHECKS**
- `detect_new_folder()` does NOT check `shutdown_flag` during blocking wait
- It can wait indefinitely until a new folder is created
- **When SIGTERM arrives while waiting for new folder → HANGS unless folder created**

#### notify_watcher (inotify) Implementation
**File**: `src/watcher/notify_watcher.rs:20-90`

```rust
pub async fn watch_for_new_folder(path: &Path) -> Result<PathBuf> {
    // ... watcher setup ...
    while let Some(event_result) = rx.recv().await {  // ← BLOCKS HERE
        // Process events until folder detected
    }
}
```
- Blocks on `mpsc::recv()` waiting for file system events
- **No timeout, no shutdown flag check**
- Will block until either:
  1. A new folder is created
  2. Channel closes (watcher dropped)

#### poll_watcher (Fallback) Implementation
**File**: `src/watcher/poll_watcher.rs:22-62`

```rust
pub async fn watch_for_new_folder(path: &Path, poll_interval: Duration) -> Result<PathBuf> {
    let mut known_folders = list_folders(&watch_path).await?;
    loop {
        sleep(poll_interval).await;  // ← CAN BLOCK FOR poll_interval
        let current_folders = list_folders(&watch_path).await?;
        // Check for new folders
    }
}
```
- **No shutdown check** during the sleep loop
- If SIGTERM arrives, must wait up to `poll_interval` before checking

---

## 3. UPLOAD STABILITY WAITING - POTENTIAL HANG POINTS
**Location**: `src/watcher/stability.rs`

### wait_for_stability() - HIGH RISK ⚠️
**File**: `src/watcher/stability.rs:33-60`

```rust
pub async fn wait_for_stability(folder_path: &Path, stability_timeout: Duration) -> Result<()> {
    match wait_for_stability_with_notify(folder_path, stability_timeout).await {
        Ok(()) => Ok(()),
        Err(e) => {
            warn!("Inotify-based stability detection failed, falling back to polling");
            wait_for_stability_with_polling(folder_path, stability_timeout).await
        }
    }
}
```

#### **CRITICAL ISSUE 2: NO SHUTDOWN CHECKS**
- Neither the notify nor polling implementation checks `shutdown_flag`
- Can wait indefinitely for upload to stabilize
- **When SIGTERM arrives while waiting for stability → HANGS**

#### notify-based stability (inotify)
**File**: `src/watcher/stability.rs:63-146`

```rust
async fn wait_for_stability_with_notify(folder_path: &Path, stability_timeout: Duration) -> Result<()> {
    loop {
        tokio::select! {
            Some(event_result) = rx.recv() => {
                // Process file events, reset timer if new files
            }
            _ = sleep(Duration::from_secs(1)) => {
                let elapsed = last_event.elapsed();
                if elapsed >= stability_timeout {
                    return Ok(());
                }
            }
        }
    }
}
```

**Better** than folder detection: Uses `tokio::select!` with timeout checks
**Problem**: Still no shutdown flag check in the select
- **Can still block up to `stability_timeout` duration (default configurable)**

#### polling-based stability (fallback)
**File**: `src/watcher/stability.rs:149-194`

```rust
async fn wait_for_stability_with_polling(folder_path: &Path, stability_timeout: Duration) -> Result<()> {
    let mut last_file_count = count_files(folder_path)?;
    let mut last_change = Instant::now();
    let poll_interval = Duration::from_secs(1);
    
    loop {
        sleep(poll_interval).await;  // ← Can block for 1 second
        let current_file_count = count_files(folder_path)?;
        // Check if file count changed, reset timer if so
        // Check if we've reached stability
    }
}
```

- Sleeps 1 second per iteration
- **No shutdown flag check**
- Can accumulate to `stability_timeout` seconds of uninterruptible sleep

---

## 4. JOB QUEUE - DEQUEUE BEHAVIOR
**Location**: `src/processor/queue.rs`

### JobQueueReceiver::dequeue()
**File**: `src/processor/queue.rs:191-206`

```rust
pub async fn dequeue(&mut self) -> Option<QueuedJob> {
    match self.rx.recv().await {
        Some(job) => {
            debug!(...);
            Some(job)
        }
        None => {
            info!("Job queue closed, no more jobs");
            None
        }
    }
}
```

### How Dequeue Works
- Uses `tokio::sync::mpsc::Receiver::recv()`
- **BLOCKS indefinitely** until a job arrives or sender is dropped
- When sender dropped → returns `None` (shutdown signal)
- **NO TIMEOUT** in the dequeue itself

### Shutdown Handling in Processor
**File**: `src/main.rs:268-292`

```rust
async fn run_processor_loop(
    config: Config,
    mut queue_rx: three_dgs_processor::processor::JobQueueReceiver,
    shutdown_flag: three_dgs_processor::shutdown::ShutdownFlag,
    health_state: health::HealthCheckState,
) {
    loop {
        // Check for shutdown BEFORE trying to dequeue
        if shutdown_flag.is_shutdown_requested() {
            info!("Processor loop received shutdown signal");
            break;
        }

        // **CRITICAL**: Dequeue with timeout to allow shutdown checks
        let job = match tokio::time::timeout(Duration::from_secs(1), queue_rx.dequeue()).await {
            Ok(Some(job)) => job,
            Ok(None) => {
                info!("Job queue closed, processor loop terminating");
                break;
            }
            Err(_) => {
                // Timeout, loop back to check shutdown flag
                continue;
            }
        };
        // ... process job ...
    }
}
```

**GOOD**: Uses `tokio::time::timeout(1 second)` to allow periodic shutdown checks
- Checks shutdown flag before attempting dequeue
- Timeouts every 1 second, allowing shutdown detection

---

## 5. HEALTH SERVER - SHUTDOWN RISK
**Location**: `src/health/mod.rs`

### start_health_server()
**File**: `src/health/mod.rs:98-142`

```rust
pub async fn start_health_server(
    _config: &Config,
) -> Result<(tokio::task::JoinHandle<Result<()>>, HealthCheckState)> {
    let enabled = std::env::var("HEALTH_CHECK_ENABLED")
        .unwrap_or_else(|_| "false".to_string())
        .parse::<bool>()
        .unwrap_or(false);

    if !enabled {
        info!("Health check endpoint is disabled");
        let state = HealthCheckState::new();
        let handle = tokio::spawn(async { Ok(()) });
        return Ok((handle, state));
    }

    let listener = tokio::net::TcpListener::bind(addr).await?;

    let handle = tokio::spawn(async move {
        axum::serve(listener, app).await?;  // ← Runs indefinitely
        Ok(())
    });

    Ok((handle, state))
}
```

### Shutdown Handling
**File**: `src/main.rs:163-165`

```rust
// Abort health server
health_handle.abort();
let _ = shutdown_signal_task.await;
```

**GOOD**: Server handle is aborted, not waited for
- Won't block shutdown
- However, abrupt termination

---

## 6. RETRY LOGIC - POTENTIAL HANG POINTS
**Location**: `src/processor/retry.rs`

### execute_with_retry()
**File**: `src/processor/retry.rs:139-236`

```rust
pub async fn execute_with_retry(
    params: JobExecutionParams,
    config: RetryConfig,
    health_state: Option<&HealthCheckState>,
) -> JobResult {
    let mut attempt = 0;
    let max_attempts = config.max_retries + 1;

    loop {
        attempt += 1;
        let result = execute_job(params.clone(), health_state).await;

        if result.status == JobStatus::Success {
            return result;
        }

        // ... check if retryable ...

        let delay = config.delay_for_attempt(attempt);
        
        sleep(delay).await;  // ← Can sleep for minutes
    }
}
```

### **CRITICAL ISSUE 3: Retry Sleeps Don't Check Shutdown**
- Retry delays can be up to 60 seconds per attempt (configurable)
- Uses `tokio::time::sleep()` with **NO shutdown check**
- **SIGTERM during retry sleep → HANGS for up to 60 seconds**
- Can have multiple retries with cumulative delays

### Environment Variables
- `MAX_RETRIES`: Default 3
- `RETRY_BASE_DELAY_SECS`: Default 2
- `RETRY_MAX_DELAY_SECS`: Default 60
- Total max wait: 3 retries × 60 seconds = 3 minutes (before processing even starts)

---

## 7. MAIN SHUTDOWN FLOW
**Location**: `src/main.rs:21-175`

### Key Spawned Tasks
1. **Watcher loop** (line 102-111): Detects folders, waits for stability, enqueues jobs
2. **Processor loop** (line 114-122): Dequeues and processes jobs
3. **Health server** (line 71): Optional HTTP endpoint
4. **Cleanup scheduler** (line 94-99): Retention cleanup
5. **Shutdown signal handler** (line 79-84): Listens for SIGTERM/SIGINT

### Main Shutdown Sequence
```rust
// 1. Wait for shutdown signal
shutdown_coordinator.wait_for_shutdown().await;

// 2. Graceful shutdown with 5-minute timeout
shutdown_coordinator.shutdown_with_timeout(|| async {
    health_state.update_state(ProcessorState::Idle).await;
    
    // 3. Wait for watcher to exit
    let _ = watcher_handle.await;
    
    // 4. Wait for processor to exit
    let _ = processor_handle.await;
    
    // 5. Abort cleanup scheduler
    cleanup_handle.abort();
    
    // 6. Unmount Azure containers
    if let Some(ref mount_cfg) = mount_config { ... }
}).await;

// 7. Abort health server
health_handle.abort();
```

### Shutdown Timeout: **5 MINUTES** (300 seconds)
**File**: `src/shutdown/mod.rs:18`
```rust
pub const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(5 * 60);
```

If any component doesn't exit within 5 minutes, shutdown times out.

---

## 8. WATCHER LOOP - SHUTDOWN CHECK
**Location**: `src/main.rs:181-262`

```rust
async fn run_watcher_loop(
    config: Config,
    queue_tx: JobQueueSender,
    shutdown_flag: ShutdownFlag,
    dedup: DuplicateDetector,
) {
    loop {
        // **GOOD**: Check shutdown flag at loop start
        if shutdown_flag.is_shutdown_requested() {
            info!("Watcher loop received shutdown signal");
            break;
        }

        // **CRITICAL**: Blocks until new folder detected (no timeout!)
        let folder = match detect_new_folder(&config.input_path, poll_interval).await {
            Ok(f) => f,
            Err(e) => {
                error!(...);
                tokio::time::sleep(Duration::from_secs(5)).await;
                continue;
            }
        };

        // **CRITICAL**: Blocks until stability achieved (no timeout!)
        if let Err(e) = wait_for_stability(&folder, stability_timeout).await {
            error!(...);
            continue;
        }
    }
}
```

**Summary**: 
- ✅ Checks shutdown at loop start
- ❌ No timeout on `detect_new_folder()`
- ❌ No timeout on `wait_for_stability()`
- ❌ If signal arrives while waiting → can take several minutes to exit

---

## SUMMARY OF BLOCKING POINTS

| Component | Function | Blocking Behavior | Shutdown Check | Timeout | Risk |
|-----------|----------|-------------------|----------------|---------|------|
| **Watcher** | `detect_new_folder()` | Wait for inotify or poll loop | ❌ NO | ❌ NO | 🔴 CRITICAL |
| **Stability** | `wait_for_stability()` | Wait for file stability | ❌ NO | ❌ NO | 🔴 CRITICAL |
| **Stability** | Polling loop | 1-sec sleep per iteration | ❌ NO | ❌ NO | 🔴 CRITICAL |
| **Retry** | `execute_with_retry()` | Sleep between retries | ❌ NO | ❌ NO | 🔴 CRITICAL |
| **Queue** | `dequeue()` | Wait for job | ✅ YES (1s timeout) | ✅ YES | 🟢 GOOD |
| **Health** | `axum::serve()` | Listen for requests | N/A | N/A | 🟡 ABORTED |
| **Shutdown** | Overall timeout | Wait for cleanup | ✅ YES | ✅ 5 min | 🟡 CRITICAL |

---

## WHAT COULD HANG ON SIGTERM/SIGINT

1. **Most Likely**: Watcher waiting for new folder detection
   - If no folder created, can wait indefinitely
   - Signal handlers cannot interrupt `wait_for_stability()`

2. **Very Likely**: Waiting for upload stability
   - After folder detected, waits for stability (configurable timeout)
   - Uses `select!` but no shutdown check inside

3. **Possible**: Retry logic sleeping
   - If job fails and retries with delay
   - Can sleep up to 60 seconds per retry
   - Up to 3 retries = 180+ seconds total

4. **Unlikely**: Processor awaiting jobs
   - Protected by 1-second timeout and shutdown check

5. **Unlikely**: Health server
   - Gets aborted, not awaited

---

## RECOMMENDED FIXES

### Fix 1: Add Shutdown Check to Watcher Loop
```rust
async fn detect_new_folder_with_shutdown(
    watch_path: &Path, 
    poll_interval: Duration,
    shutdown_flag: &ShutdownFlag,
) -> Result<Option<PathBuf>> {
    tokio::select! {
        result = detect_new_folder(watch_path, poll_interval) => {
            Ok(Some(result?))
        }
        _ = watch_for_shutdown(shutdown_flag) => {
            Ok(None)  // Shutdown requested
        }
    }
}
```

### Fix 2: Add Shutdown Check to Stability Wait
```rust
tokio::select! {
    result = wait_for_stability(folder_path, stability_timeout) => {
        result
    }
    _ = watch_for_shutdown(shutdown_flag) => {
        Err(anyhow::anyhow!("Shutdown requested"))
    }
}
```

### Fix 3: Make Retry Sleep Cancellable
```rust
tokio::select! {
    _ = sleep(delay) => {}
    _ = watch_for_shutdown(shutdown_flag) => {
        return Err(anyhow::anyhow!("Shutdown requested"));
    }
}
```

