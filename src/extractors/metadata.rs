//! Metadata extraction from video files
//!
//! This module implements:
//! - Video metadata extraction via FFmpeg probe
//! - EXIF/GPS data extraction
//! - Structured metadata output per video

use anyhow::{Context, Result};
use serde_json::Value;
use std::process::Command;
use tracing::{debug, info, warn};

use super::ffmpeg::VideoInput;
use super::models::{CameraInfo, GpsCoordinates, Orientation, Resolution, VideoMetadata};

/// Extract metadata from a video file asynchronously
///
/// This function wraps the synchronous metadata extraction in spawn_blocking
pub async fn extract_metadata(video: &VideoInput) -> Result<VideoMetadata> {
    let video = video.clone();
    tokio::task::spawn_blocking(move || extract_metadata_sync(&video))
        .await
        .context("Metadata extraction task panicked")?
}

/// Extract metadata from a video file synchronously using FFprobe
///
/// This function should be called via spawn_blocking to avoid blocking the async runtime
pub fn extract_metadata_sync(video: &VideoInput) -> Result<VideoMetadata> {
    info!(
        video = %video.path.display(),
        "Extracting metadata with FFprobe"
    );

    // Run ffprobe to get video metadata in JSON format
    let output = Command::new("ffprobe")
        .arg("-v")
        .arg("quiet")
        .arg("-print_format")
        .arg("json")
        .arg("-show_format")
        .arg("-show_streams")
        .arg(&video.path)
        .output()
        .context("Failed to execute ffprobe command")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("FFprobe failed: {}", stderr);
    }

    let json_output =
        String::from_utf8(output.stdout).context("FFprobe output is not valid UTF-8")?;

    let probe_data: Value =
        serde_json::from_str(&json_output).context("Failed to parse FFprobe JSON output")?;

    // Extract video stream information
    let video_stream = find_video_stream(&probe_data).context("No video stream found in file")?;

    // Extract required metadata
    let width = video_stream["width"]
        .as_u64()
        .context("Missing or invalid width")? as u32;

    let height = video_stream["height"]
        .as_u64()
        .context("Missing or invalid height")? as u32;

    let resolution = Resolution::new(width, height);

    // Extract frame rate
    let fps = parse_frame_rate(video_stream)?;

    // Extract duration and frame count
    let format = &probe_data["format"];
    let duration_secs = format["duration"]
        .as_str()
        .and_then(|s| s.parse::<f64>().ok())
        .context("Missing or invalid duration")?;

    // Calculate frame count from duration and fps
    let frame_count = (duration_secs * fps).round() as u32;

    // Extract optional camera/EXIF metadata
    let camera_info = extract_camera_info(&probe_data);

    let metadata = VideoMetadata {
        filename: video.filename.clone(),
        path: video.path.clone(),
        frame_count,
        resolution,
        fps,
        duration_secs,
        camera_info: if camera_info.has_data() {
            Some(camera_info)
        } else {
            None
        },
    };

    debug!(
        video = %video.path.display(),
        resolution = %resolution,
        fps = fps,
        duration = duration_secs,
        frames = frame_count,
        "Extracted metadata"
    );

    Ok(metadata)
}

/// Find the first video stream in the probe data
fn find_video_stream(probe_data: &Value) -> Option<&Value> {
    probe_data["streams"]
        .as_array()?
        .iter()
        .find(|stream| stream["codec_type"] == "video")
}

/// Parse frame rate from video stream data
fn parse_frame_rate(stream: &Value) -> Result<f64> {
    // Try r_frame_rate first (real frame rate)
    if let Some(fps_str) = stream["r_frame_rate"].as_str() {
        if let Some(fps) = parse_rational(fps_str) {
            if fps > 0.0 {
                return Ok(fps);
            }
        }
    }

    // Fall back to avg_frame_rate
    if let Some(fps_str) = stream["avg_frame_rate"].as_str() {
        if let Some(fps) = parse_rational(fps_str) {
            if fps > 0.0 {
                return Ok(fps);
            }
        }
    }

    anyhow::bail!("Could not determine frame rate")
}

/// Parse a rational number string like "30000/1001" to a float
fn parse_rational(s: &str) -> Option<f64> {
    let parts: Vec<&str> = s.split('/').collect();
    if parts.len() == 2 {
        let num: f64 = parts[0].parse().ok()?;
        let den: f64 = parts[1].parse().ok()?;
        if den != 0.0 {
            return Some(num / den);
        }
    }
    None
}

/// Extract camera/EXIF metadata from probe data
fn extract_camera_info(probe_data: &Value) -> CameraInfo {
    let format = &probe_data["format"];
    let tags = &format["tags"];

    // Extract recording timestamp
    let recording_timestamp = extract_timestamp(tags);

    // Extract GPS coordinates
    let gps_coordinates = extract_gps_coordinates(tags);

    // Extract camera model
    let camera_model = tags["com.android.version"]
        .as_str()
        .or_else(|| tags["make"].as_str())
        .or_else(|| tags["model"].as_str())
        .or_else(|| tags["encoder"].as_str())
        .map(|s| s.to_string());

    // Extract orientation
    let orientation = extract_orientation(&probe_data["streams"]);

    // Extract focal length from EXIF
    let focal_length_mm = extract_focal_length(tags);
    let focal_length_35mm = extract_focal_length_35mm(tags);

    // Estimate sensor size from camera model or focal length
    let (sensor_width_mm, sensor_height_mm) = estimate_sensor_size(
        camera_model.as_deref(),
        focal_length_mm,
        focal_length_35mm,
    );

    CameraInfo {
        recording_timestamp,
        gps_coordinates,
        camera_model,
        orientation,
        focal_length_mm,
        focal_length_35mm,
        sensor_width_mm,
        sensor_height_mm,
    }
}

/// Extract timestamp from metadata tags
fn extract_timestamp(tags: &Value) -> Option<String> {
    // Try various timestamp fields
    tags["creation_time"]
        .as_str()
        .or_else(|| tags["date"].as_str())
        .or_else(|| tags["datetime"].as_str())
        .map(|s| s.to_string())
}

/// Extract GPS coordinates from metadata tags
fn extract_gps_coordinates(tags: &Value) -> Option<GpsCoordinates> {
    // Try to extract GPS location (format varies by device)
    let location_str = tags["location"].as_str()?;

    // Parse location string like "+47.6062-122.3321/" or similar formats
    parse_gps_string(location_str)
}

/// Parse GPS string in various formats
fn parse_gps_string(location: &str) -> Option<GpsCoordinates> {
    // Try ISO 6709 format: +47.6062-122.3321/ or +47.6062-122.3321+100/
    if let Some(coords) = parse_iso6709(location) {
        return Some(coords);
    }

    warn!(location = location, "Could not parse GPS coordinates");
    None
}

/// Parse ISO 6709 location string format
fn parse_iso6709(s: &str) -> Option<GpsCoordinates> {
    // Format: +XX.XXXX+YYY.YYYY/ or +XX.XXXX+YYY.YYYY+ZZZ/
    let trimmed = s.trim_end_matches('/');

    // Split into components
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut in_number = false;

    for ch in trimmed.chars() {
        if ch == '+' || ch == '-' {
            if in_number && !current.is_empty() {
                parts.push(current.clone());
                current.clear();
            }
            current.push(ch);
            in_number = true;
        } else {
            current.push(ch);
        }
    }

    if !current.is_empty() {
        parts.push(current);
    }

    // Need at least latitude and longitude
    if parts.len() < 2 {
        return None;
    }

    let latitude: f64 = parts[0].parse().ok()?;
    let longitude: f64 = parts[1].parse().ok()?;
    let altitude: Option<f64> = parts.get(2).and_then(|s| s.parse().ok());

    Some(GpsCoordinates::new(latitude, longitude, altitude))
}

/// Extract orientation from video stream metadata
fn extract_orientation(streams: &Value) -> Option<Orientation> {
    let video_stream = streams
        .as_array()?
        .iter()
        .find(|stream| stream["codec_type"] == "video")?;

    // Check for rotation tag in stream or side_data
    let rotation = video_stream["tags"]["rotate"]
        .as_str()
        .and_then(|s| s.parse::<i32>().ok())
        .or_else(|| {
            video_stream["side_data_list"].as_array().and_then(|arr| {
                arr.iter().find_map(|data| {
                    if data["side_data_type"] == "Display Matrix" {
                        data["rotation"].as_i64().map(|r| r as i32)
                    } else {
                        None
                    }
                })
            })
        })?;

    Some(Orientation::from_rotation(rotation))
}

/// Extract focal length from metadata tags (in millimeters)
fn extract_focal_length(tags: &Value) -> Option<f64> {
    // Try QuickTime focal length tag first (common in iPhone/Android)
    if let Some(focal) = tags["com.apple.quicktime.camera.focal_length"]
        .as_str()
        .and_then(|s| s.parse::<f64>().ok())
    {
        debug!(focal_length_mm = focal, "Extracted focal length from QuickTime tag");
        return Some(focal);
    }

    // Try standard EXIF FocalLength tag
    if let Some(focal) = tags["FocalLength"]
        .as_str()
        .and_then(parse_exif_rational)
    {
        debug!(focal_length_mm = focal, "Extracted focal length from EXIF tag");
        return Some(focal);
    }

    // Try focal_length tag
    if let Some(focal) = tags["focal_length"]
        .as_str()
        .and_then(|s| s.parse::<f64>().ok())
    {
        debug!(focal_length_mm = focal, "Extracted focal length from generic tag");
        return Some(focal);
    }

    None
}

/// Extract 35mm equivalent focal length from metadata tags
fn extract_focal_length_35mm(tags: &Value) -> Option<f64> {
    // Try standard EXIF FocalLengthIn35mmFilm tag
    if let Some(focal) = tags["FocalLengthIn35mmFilm"]
        .as_str()
        .and_then(|s| s.parse::<f64>().ok())
    {
        debug!(focal_length_35mm = focal, "Extracted 35mm equivalent focal length");
        return Some(focal);
    }

    // Try QuickTime equivalent
    if let Some(focal) = tags["com.apple.quicktime.camera.focal_length_in_35mm"]
        .as_str()
        .and_then(|s| s.parse::<f64>().ok())
    {
        debug!(focal_length_35mm = focal, "Extracted 35mm equivalent from QuickTime");
        return Some(focal);
    }

    None
}

/// Parse EXIF rational number format (e.g., "4250/1000" = 4.25)
fn parse_exif_rational(s: &str) -> Option<f64> {
    if let Some(pos) = s.find('/') {
        let num: f64 = s[..pos].parse().ok()?;
        let den: f64 = s[pos + 1..].parse().ok()?;
        if den != 0.0 {
            return Some(num / den);
        }
    }
    // Try as plain number if not rational
    s.parse().ok()
}

/// Estimate sensor size from camera model or focal length data
///
/// Returns (sensor_width_mm, sensor_height_mm) if estimation is possible
fn estimate_sensor_size(
    camera_model: Option<&str>,
    focal_length_mm: Option<f64>,
    focal_length_35mm: Option<f64>,
) -> (Option<f64>, Option<f64>) {
    // If we have both actual and 35mm equivalent focal lengths, we can calculate sensor size
    if let (Some(focal_mm), Some(focal_35mm)) = (focal_length_mm, focal_length_35mm) {
        if focal_35mm > 0.0 && focal_mm > 0.0 {
            // Crop factor = focal_35mm / focal_mm
            // Full-frame sensor is 36mm x 24mm
            let crop_factor = focal_35mm / focal_mm;
            let sensor_width = 36.0 / crop_factor;
            let sensor_height = 24.0 / crop_factor;
            debug!(
                crop_factor = crop_factor,
                sensor_width_mm = sensor_width,
                sensor_height_mm = sensor_height,
                "Computed sensor size from focal length ratio"
            );
            return (Some(sensor_width), Some(sensor_height));
        }
    }

    // Try known camera models
    if let Some(model) = camera_model {
        if let Some((width, height)) = lookup_sensor_size(model) {
            debug!(
                camera_model = model,
                sensor_width_mm = width,
                sensor_height_mm = height,
                "Found sensor size from camera model database"
            );
            return (Some(width), Some(height));
        }
    }

    // No sensor size information available
    (None, None)
}

/// Lookup sensor size for known camera models
///
/// This is a simplified database of common camera sensor sizes
fn lookup_sensor_size(model: &str) -> Option<(f64, f64)> {
    let model_lower = model.to_lowercase();

    // iPhone models (various sensor sizes over generations)
    if model_lower.contains("iphone") {
        // Modern iPhones typically use 1/1.28" to 1/1.9" sensors
        // Approximate: 1/1.7" sensor = ~7.6mm x 5.7mm
        return Some((7.6, 5.7));
    }

    // Android flagships (typically 1/1.3" to 1/2.55" sensors)
    if model_lower.contains("pixel")
        || model_lower.contains("galaxy")
        || model_lower.contains("oneplus")
    {
        // Approximate: 1/2.0" sensor = ~6.4mm x 4.8mm
        return Some((6.4, 4.8));
    }

    // GoPro (1/2.3" sensor common)
    if model_lower.contains("gopro") {
        return Some((6.17, 4.55)); // 1/2.3" sensor
    }

    // DJI drones
    if model_lower.contains("dji") || model_lower.contains("mavic") {
        return Some((6.17, 4.55)); // Typical 1/2.3" sensor
    }

    // Default smartphone sensor: 1/2.55" (~5.76mm x 4.29mm)
    // This is conservative and widely used
    None // Return None if unknown, caller can use defaults
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_rational() {
        assert_eq!(parse_rational("30000/1001"), Some(29.97002997002997));
        assert_eq!(parse_rational("30/1"), Some(30.0));
        assert_eq!(parse_rational("24000/1001"), Some(23.976023976023978));
        assert_eq!(parse_rational("invalid"), None);
        assert_eq!(parse_rational("30/0"), None);
    }

    #[test]
    fn test_parse_iso6709() {
        let coords = parse_iso6709("+47.6062-122.3321/").unwrap();
        assert!((coords.latitude - 47.6062).abs() < 0.0001);
        assert!((coords.longitude - (-122.3321)).abs() < 0.0001);
        assert_eq!(coords.altitude, None);

        let coords_alt = parse_iso6709("+47.6062-122.3321+100.5/").unwrap();
        assert!((coords_alt.latitude - 47.6062).abs() < 0.0001);
        assert!((coords_alt.longitude - (-122.3321)).abs() < 0.0001);
        assert_eq!(coords_alt.altitude, Some(100.5));

        assert!(parse_iso6709("invalid").is_none());
    }

    #[test]
    fn test_parse_exif_rational() {
        assert_eq!(parse_exif_rational("4250/1000"), Some(4.25));
        assert_eq!(parse_exif_rational("50/1"), Some(50.0));
        assert_eq!(parse_exif_rational("285/10"), Some(28.5));
        assert_eq!(parse_exif_rational("42"), Some(42.0)); // Plain number
        assert_eq!(parse_exif_rational("10/0"), None); // Division by zero
        assert_eq!(parse_exif_rational("invalid"), None);
    }

    #[test]
    fn test_estimate_sensor_size_from_focal_lengths() {
        // Test with 35mm equivalent calculation
        // Example: 4.25mm focal length with 26mm equivalent
        // Crop factor = 26 / 4.25 ≈ 6.12
        // Sensor size = 36mm / 6.12 ≈ 5.88mm x 3.92mm
        let (width, height) = estimate_sensor_size(None, Some(4.25), Some(26.0));
        assert!(width.is_some());
        assert!(height.is_some());
        assert!((width.unwrap() - 5.88).abs() < 0.1);
        assert!((height.unwrap() - 3.92).abs() < 0.1);
    }

    #[test]
    fn test_lookup_sensor_size() {
        // Test iPhone
        let (w, h) = lookup_sensor_size("iPhone 15 Pro").unwrap();
        assert_eq!(w, 7.6);
        assert_eq!(h, 5.7);

        // Test Pixel
        let (w, h) = lookup_sensor_size("Google Pixel 8").unwrap();
        assert_eq!(w, 6.4);
        assert_eq!(h, 4.8);

        // Test GoPro
        let (w, h) = lookup_sensor_size("GoPro HERO11").unwrap();
        assert_eq!(w, 6.17);
        assert_eq!(h, 4.55);

        // Test unknown model
        assert!(lookup_sensor_size("Unknown Camera").is_none());
    }

    #[test]
    fn test_estimate_sensor_size_no_data() {
        let (width, height) = estimate_sensor_size(None, None, None);
        assert!(width.is_none());
        assert!(height.is_none());
    }
}
