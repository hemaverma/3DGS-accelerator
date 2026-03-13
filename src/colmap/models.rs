//! COLMAP data structures and types

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::str::FromStr;

/// COLMAP sparse reconstruction output
#[derive(Debug, Clone)]
pub struct ColmapOutput {
    /// Path to sparse reconstruction directory
    pub sparse_dir: PathBuf,

    /// Path to COLMAP database file
    pub database_path: PathBuf,

    /// Reconstruction statistics
    pub stats: ColmapStats,
}

/// Statistics from COLMAP sparse reconstruction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColmapStats {
    /// Total number of 3D points reconstructed
    pub total_points: usize,

    /// Number of cameras/images registered
    pub camera_count: usize,

    /// Average reprojection error (if available)
    pub avg_reprojection_error: Option<f64>,
}

/// Feature matching strategy
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MatcherType {
    /// Match all image pairs (best quality, slowest)
    Exhaustive,

    /// Match sequential image pairs (faster, assumes ordered sequence)
    Sequential,

    /// Use vocabulary tree for matching (fastest for large datasets)
    VocabTree,
}

impl FromStr for MatcherType {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "exhaustive" => Ok(MatcherType::Exhaustive),
            "sequential" => Ok(MatcherType::Sequential),
            "vocab_tree" => Ok(MatcherType::VocabTree),
            _ => Err(anyhow!(
                "Invalid matcher type '{}'. Valid options: exhaustive, sequential, vocab_tree",
                s
            )),
        }
    }
}

impl std::fmt::Display for MatcherType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MatcherType::Exhaustive => write!(f, "exhaustive"),
            MatcherType::Sequential => write!(f, "sequential"),
            MatcherType::VocabTree => write!(f, "vocab_tree"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_matcher_type_roundtrip() {
        let types = vec![
            MatcherType::Exhaustive,
            MatcherType::Sequential,
            MatcherType::VocabTree,
        ];

        for matcher in types {
            let s = matcher.to_string();
            let parsed = s.parse::<MatcherType>().unwrap();
            assert_eq!(matcher, parsed);
        }
    }

    #[test]
    fn test_matcher_type_invalid() {
        assert!("invalid".parse::<MatcherType>().is_err());
    }
}
