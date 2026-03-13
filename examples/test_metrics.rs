use three_dgs_processor::logging;

fn main() -> anyhow::Result<()> {
    logging::init()?;

    // Test all metric functions
    logging::log_job_started("job-001", 3);
    logging::log_frame_extraction_complete("job-001", 30, 360);
    logging::log_colmap_complete("job-001", 195, 125430);
    logging::log_training_complete("job-001", 1587, 30000);
    logging::log_export_complete("job-001", 33, 45.2, 38.7);
    logging::log_job_complete("job-001", 1845, "success");

    Ok(())
}
