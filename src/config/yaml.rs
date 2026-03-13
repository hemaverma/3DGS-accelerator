use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

/// 3DGS training hyperparameters loaded from YAML config file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YamlConfig {
    #[serde(default = "default_backend")]
    pub backend: String,

    #[serde(default)]
    pub frame_extraction: FrameExtractionConfig,

    #[serde(default)]
    pub colmap: ColmapConfig,

    #[serde(default)]
    pub training: TrainingConfig,

    #[serde(default)]
    pub export: ExportConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrameExtractionConfig {
    #[serde(default = "default_frame_rate")]
    pub rate: f32,

    #[serde(default)]
    pub count: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColmapConfig {
    #[serde(default = "default_matcher")]
    pub matcher: String,

    #[serde(default = "default_camera_model")]
    pub camera_model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrainingConfig {
    #[serde(default = "default_iterations")]
    pub iterations: u32,

    #[serde(default = "default_learning_rate")]
    pub learning_rate: f64,

    #[serde(default = "default_densification_interval")]
    pub densification_interval: u32,

    #[serde(default = "default_densification_start")]
    pub densification_start: u32,

    #[serde(default = "default_densification_stop")]
    pub densification_stop: u32,

    #[serde(default = "default_opacity_reset_interval")]
    pub opacity_reset_interval: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportConfig {
    #[serde(default = "default_export_formats")]
    pub formats: Vec<String>,

    #[serde(default = "default_ply_precision")]
    pub ply_precision: String,
}

impl Default for YamlConfig {
    fn default() -> Self {
        Self {
            backend: default_backend(),
            frame_extraction: FrameExtractionConfig::default(),
            colmap: ColmapConfig::default(),
            training: TrainingConfig::default(),
            export: ExportConfig::default(),
        }
    }
}

impl Default for FrameExtractionConfig {
    fn default() -> Self {
        Self {
            rate: default_frame_rate(),
            count: None,
        }
    }
}

impl Default for ColmapConfig {
    fn default() -> Self {
        Self {
            matcher: default_matcher(),
            camera_model: default_camera_model(),
        }
    }
}

impl Default for TrainingConfig {
    fn default() -> Self {
        Self {
            iterations: default_iterations(),
            learning_rate: default_learning_rate(),
            densification_interval: default_densification_interval(),
            densification_start: default_densification_start(),
            densification_stop: default_densification_stop(),
            opacity_reset_interval: default_opacity_reset_interval(),
        }
    }
}

impl Default for ExportConfig {
    fn default() -> Self {
        Self {
            formats: default_export_formats(),
            ply_precision: default_ply_precision(),
        }
    }
}

// Default value functions
fn default_backend() -> String {
    "gaussian-splatting".to_string()
}

fn default_frame_rate() -> f32 {
    1.0
}

fn default_matcher() -> String {
    "exhaustive".to_string()
}

fn default_camera_model() -> String {
    "OPENCV".to_string()
}

fn default_iterations() -> u32 {
    30000
}

fn default_learning_rate() -> f64 {
    0.0025
}

fn default_densification_interval() -> u32 {
    100
}

fn default_densification_start() -> u32 {
    500
}

fn default_densification_stop() -> u32 {
    15000
}

fn default_opacity_reset_interval() -> u32 {
    3000
}

fn default_export_formats() -> Vec<String> {
    vec!["ply".to_string(), "splat".to_string()]
}

fn default_ply_precision() -> String {
    "float32".to_string()
}

impl YamlConfig {
    pub fn load(path: &Path) -> Result<Self> {
        let contents = fs::read_to_string(path)
            .with_context(|| format!("Failed to read config file: {}", path.display()))?;

        serde_yaml::from_str(&contents)
            .with_context(|| format!("Failed to parse YAML config: {}", path.display()))
    }
}
