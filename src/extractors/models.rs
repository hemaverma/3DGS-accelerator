//! Metadata type definitions for video processing
//!
//! This module defines structured types for video metadata extraction,
//! including camera information and EXIF data.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Complete metadata for a video file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoMetadata {
    /// Original video filename
    pub filename: String,

    /// Path to the video file
    pub path: PathBuf,

    /// Total number of frames in the video
    pub frame_count: u32,

    /// Frame resolution (width x height)
    pub resolution: Resolution,

    /// Frame rate (frames per second)
    pub fps: f64,

    /// Video duration in seconds
    pub duration_secs: f64,

    /// Camera/EXIF metadata (optional)
    pub camera_info: Option<CameraInfo>,
}

/// Video resolution
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct Resolution {
    /// Width in pixels
    pub width: u32,

    /// Height in pixels
    pub height: u32,
}

impl Resolution {
    /// Create a new Resolution
    pub fn new(width: u32, height: u32) -> Self {
        Resolution { width, height }
    }
}

impl std::fmt::Display for Resolution {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}x{}", self.width, self.height)
    }
}

/// Camera and EXIF metadata extracted from video
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CameraInfo {
    /// Recording timestamp (UTC)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recording_timestamp: Option<String>,

    /// GPS coordinates (latitude, longitude)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gps_coordinates: Option<GpsCoordinates>,

    /// Camera/device model identifier
    #[serde(skip_serializing_if = "Option::is_none")]
    pub camera_model: Option<String>,

    /// Device orientation (degrees)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub orientation: Option<Orientation>,

    /// Focal length in millimeters
    #[serde(skip_serializing_if = "Option::is_none")]
    pub focal_length_mm: Option<f64>,

    /// Focal length in 35mm equivalent
    #[serde(skip_serializing_if = "Option::is_none")]
    pub focal_length_35mm: Option<f64>,

    /// Physical sensor width in millimeters
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sensor_width_mm: Option<f64>,

    /// Physical sensor height in millimeters
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sensor_height_mm: Option<f64>,
}

impl CameraInfo {
    /// Create an empty CameraInfo
    pub fn empty() -> Self {
        CameraInfo {
            recording_timestamp: None,
            gps_coordinates: None,
            camera_model: None,
            orientation: None,
            focal_length_mm: None,
            focal_length_35mm: None,
            sensor_width_mm: None,
            sensor_height_mm: None,
        }
    }

    /// Check if any camera info is available
    pub fn has_data(&self) -> bool {
        self.recording_timestamp.is_some()
            || self.gps_coordinates.is_some()
            || self.camera_model.is_some()
            || self.orientation.is_some()
            || self.focal_length_mm.is_some()
            || self.focal_length_35mm.is_some()
    }
}

/// GPS coordinates
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct GpsCoordinates {
    /// Latitude in decimal degrees
    pub latitude: f64,

    /// Longitude in decimal degrees
    pub longitude: f64,

    /// Altitude in meters (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub altitude: Option<f64>,
}

impl GpsCoordinates {
    /// Create new GPS coordinates
    pub fn new(latitude: f64, longitude: f64, altitude: Option<f64>) -> Self {
        GpsCoordinates {
            latitude,
            longitude,
            altitude,
        }
    }
}

/// Device orientation
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum Orientation {
    /// Landscape orientation (0 degrees)
    Landscape,

    /// Portrait orientation (90 degrees)
    Portrait,

    /// Landscape upside down (180 degrees)
    LandscapeUpsideDown,

    /// Portrait upside down (270 degrees)
    PortraitUpsideDown,

    /// Unknown orientation
    #[default]
    Unknown,
}

impl Orientation {
    /// Parse orientation from rotation degrees
    pub fn from_rotation(degrees: i32) -> Self {
        match degrees {
            0 => Orientation::Landscape,
            90 => Orientation::Portrait,
            180 => Orientation::LandscapeUpsideDown,
            270 => Orientation::PortraitUpsideDown,
            _ => Orientation::Unknown,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolution_display() {
        let res = Resolution::new(1920, 1080);
        assert_eq!(res.to_string(), "1920x1080");
    }

    #[test]
    fn test_orientation_from_rotation() {
        assert_eq!(Orientation::from_rotation(0), Orientation::Landscape);
        assert_eq!(Orientation::from_rotation(90), Orientation::Portrait);
        assert_eq!(
            Orientation::from_rotation(180),
            Orientation::LandscapeUpsideDown
        );
        assert_eq!(
            Orientation::from_rotation(270),
            Orientation::PortraitUpsideDown
        );
        assert_eq!(Orientation::from_rotation(45), Orientation::Unknown);
    }

    #[test]
    fn test_camera_info_has_data() {
        let empty = CameraInfo::empty();
        assert!(!empty.has_data());

        let with_model = CameraInfo {
            camera_model: Some("iPhone 15 Pro".to_string()),
            ..CameraInfo::empty()
        };
        assert!(with_model.has_data());

        let with_gps = CameraInfo {
            gps_coordinates: Some(GpsCoordinates::new(47.6062, -122.3321, None)),
            ..CameraInfo::empty()
        };
        assert!(with_gps.has_data());
    }

    #[test]
    fn test_gps_coordinates_serialization() {
        let gps = GpsCoordinates::new(47.6062, -122.3321, Some(100.0));
        let json = serde_json::to_string(&gps).unwrap();
        assert!(json.contains("47.6062"));
        assert!(json.contains("-122.3321"));
        assert!(json.contains("100.0"));
    }
}
