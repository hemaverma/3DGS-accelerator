use tracing::info;

/// Log when a job starts processing
/// Format: [2026-02-11T20:15:00Z] INFO job_started job_id=job-001 videos=3
pub fn log_job_started(job_id: &str, video_count: usize) {
    info!(job_id = job_id, videos = video_count, "job_started");
}

/// Log when frame extraction completes
/// Format: [2026-02-11T20:15:30Z] INFO frame_extraction_complete job_id=job-001 duration_secs=30 total_frames=360
pub fn log_frame_extraction_complete(job_id: &str, duration_secs: u64, total_frames: usize) {
    info!(
        job_id = job_id,
        duration_secs = duration_secs,
        total_frames = total_frames,
        "frame_extraction_complete"
    );
}

/// Log when COLMAP processing completes
/// Format: [2026-02-11T20:18:45Z] INFO colmap_complete job_id=job-001 duration_secs=195 points=125430
pub fn log_colmap_complete(job_id: &str, duration_secs: u64, points: usize) {
    info!(
        job_id = job_id,
        duration_secs = duration_secs,
        points = points,
        "colmap_complete"
    );
}

/// Log when 3DGS training completes
/// Format: [2026-02-11T20:45:12Z] INFO training_complete job_id=job-001 duration_secs=1587 iterations=30000
pub fn log_training_complete(job_id: &str, duration_secs: u64, iterations: u32) {
    info!(
        job_id = job_id,
        duration_secs = duration_secs,
        iterations = iterations,
        "training_complete"
    );
}

/// Log when model export completes
/// Format: [2026-02-11T20:45:45Z] INFO export_complete job_id=job-001 duration_secs=33 ply_size_mb=45.2 splat_size_mb=38.7
pub fn log_export_complete(job_id: &str, duration_secs: u64, ply_size_mb: f64, splat_size_mb: f64) {
    info!(
        job_id = job_id,
        duration_secs = duration_secs,
        ply_size_mb = ply_size_mb,
        splat_size_mb = splat_size_mb,
        "export_complete"
    );
}

/// Log when a complete job finishes (success or failure)
/// Format: [2026-02-11T20:45:45Z] INFO job_complete job_id=job-001 total_duration_secs=1845 status=success
pub fn log_job_complete(job_id: &str, total_duration_secs: u64, status: &str) {
    info!(
        job_id = job_id,
        total_duration_secs = total_duration_secs,
        status = status,
        "job_complete"
    );
}
