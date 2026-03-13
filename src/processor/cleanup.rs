//! Post-processing folder management
//!
//! This module handles moving completed jobs to processed/ or error/ folders
//! based on job outcome. It preserves the original folder structure and handles
//! naming collisions by appending timestamps when needed.

use anyhow::{Context, Result};
use chrono::Utc;
use std::path::{Path, PathBuf};
use tokio::fs;
use tracing::{error, info, instrument, warn};

/// Move a successfully processed job folder to the processed directory
///
/// # Arguments
///
/// * `source_folder` - Path to the input folder that was processed
/// * `processed_path` - Root path of the processed directory
///
/// # Returns
///
/// The final destination path where the folder was moved
///
/// # Behavior
///
/// 1. Extracts the folder name from source path
/// 2. Constructs destination path under processed_path
/// 3. Handles naming collisions by appending timestamps
/// 4. Moves the entire folder atomically
/// 5. Logs the movement with timestamps
///
/// # Example
///
/// ```no_run
/// # use std::path::PathBuf;
/// # async fn example() -> anyhow::Result<()> {
/// let source = PathBuf::from("/input/scene_001");
/// let processed = PathBuf::from("/processed");
/// let dest = move_to_processed(&source, &processed).await?;
/// // dest will be /processed/scene_001 or /processed/scene_001_20240211_143022
/// # Ok(())
/// # }
/// ```
#[instrument(skip_all, fields(source = %source_folder.display(), processed_root = %processed_path.display()))]
pub async fn move_to_processed(source_folder: &Path, processed_path: &Path) -> Result<PathBuf> {
    // Extract folder name
    let folder_name = source_folder
        .file_name()
        .context("Source folder has no file name")?
        .to_str()
        .context("Folder name is not valid UTF-8")?;

    // Construct destination path
    let mut dest = processed_path.join(folder_name);

    // Handle naming collisions by appending timestamp
    if dest.exists() {
        let timestamp = Utc::now().format("%Y%m%d_%H%M%S");
        let new_name = format!("{}_{}", folder_name, timestamp);
        dest = processed_path.join(&new_name);

        warn!(
            original_dest = %processed_path.join(folder_name).display(),
            new_dest = %dest.display(),
            "Destination already exists, appending timestamp"
        );
    }

    // Ensure processed directory exists
    fs::create_dir_all(processed_path).await.with_context(|| {
        format!(
            "Failed to create processed directory: {}",
            processed_path.display()
        )
    })?;

    // Move the folder
    fs::rename(source_folder, &dest).await.with_context(|| {
        format!(
            "Failed to move folder from {} to {}",
            source_folder.display(),
            dest.display()
        )
    })?;

    info!(
        source = %source_folder.display(),
        destination = %dest.display(),
        timestamp = %Utc::now().to_rfc3339(),
        "Successfully moved folder to processed"
    );

    Ok(dest)
}

/// Move a failed job folder to the error directory
///
/// # Arguments
///
/// * `source_folder` - Path to the input folder that failed processing
/// * `error_path` - Root path of the error directory
///
/// # Returns
///
/// The final destination path where the folder was moved
///
/// # Behavior
///
/// 1. Extracts the folder name from source path
/// 2. Constructs destination path under error_path
/// 3. Handles naming collisions by appending timestamps
/// 4. Moves the entire folder atomically
/// 5. Logs the movement with timestamps and error context
///
/// # Example
///
/// ```no_run
/// # use std::path::PathBuf;
/// # async fn example() -> anyhow::Result<()> {
/// let source = PathBuf::from("/input/scene_failed");
/// let error = PathBuf::from("/error");
/// let dest = move_to_error(&source, &error).await?;
/// // dest will be /error/scene_failed or /error/scene_failed_20240211_143022
/// # Ok(())
/// # }
/// ```
#[instrument(skip_all, fields(source = %source_folder.display(), error_root = %error_path.display()))]
pub async fn move_to_error(source_folder: &Path, error_path: &Path) -> Result<PathBuf> {
    // Extract folder name
    let folder_name = source_folder
        .file_name()
        .context("Source folder has no file name")?
        .to_str()
        .context("Folder name is not valid UTF-8")?;

    // Construct destination path
    let mut dest = error_path.join(folder_name);

    // Handle naming collisions by appending timestamp
    if dest.exists() {
        let timestamp = Utc::now().format("%Y%m%d_%H%M%S");
        let new_name = format!("{}_{}", folder_name, timestamp);
        dest = error_path.join(&new_name);

        warn!(
            original_dest = %error_path.join(folder_name).display(),
            new_dest = %dest.display(),
            "Destination already exists, appending timestamp"
        );
    }

    // Ensure error directory exists
    fs::create_dir_all(error_path)
        .await
        .with_context(|| format!("Failed to create error directory: {}", error_path.display()))?;

    // Move the folder
    fs::rename(source_folder, &dest).await.with_context(|| {
        format!(
            "Failed to move folder from {} to {}",
            source_folder.display(),
            dest.display()
        )
    })?;

    error!(
        source = %source_folder.display(),
        destination = %dest.display(),
        timestamp = %Utc::now().to_rfc3339(),
        "Moved failed job folder to error directory"
    );

    Ok(dest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use tokio::fs;

    #[tokio::test]
    async fn test_move_to_processed_success() -> Result<()> {
        // Setup
        let test_root = TempDir::new()?;
        let input_dir = test_root.path().join("input");
        let processed_dir = test_root.path().join("processed");
        let source_folder = input_dir.join("test_scene");

        fs::create_dir_all(&source_folder).await?;
        fs::write(source_folder.join("test.txt"), "test content").await?;

        // Execute
        let dest = move_to_processed(&source_folder, &processed_dir).await?;

        // Verify
        assert!(dest.exists(), "Destination folder should exist");
        assert!(!source_folder.exists(), "Source folder should be moved");
        assert!(dest.join("test.txt").exists(), "File should be preserved");
        assert_eq!(
            dest,
            processed_dir.join("test_scene"),
            "Should be moved to correct location"
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_move_to_processed_collision() -> Result<()> {
        // Setup
        let test_root = TempDir::new()?;
        let input_dir = test_root.path().join("input");
        let processed_dir = test_root.path().join("processed");
        let source_folder = input_dir.join("test_scene");
        let existing_folder = processed_dir.join("test_scene");

        fs::create_dir_all(&source_folder).await?;
        fs::create_dir_all(&existing_folder).await?;
        fs::write(source_folder.join("test.txt"), "new content").await?;
        fs::write(existing_folder.join("old.txt"), "old content").await?;

        // Execute
        let dest = move_to_processed(&source_folder, &processed_dir).await?;

        // Verify
        assert!(dest.exists(), "Destination folder should exist");
        assert!(!source_folder.exists(), "Source folder should be moved");
        assert!(
            existing_folder.exists(),
            "Original folder should be preserved"
        );
        assert!(dest.join("test.txt").exists(), "New file should exist");
        assert!(!dest.join("old.txt").exists(), "Old file should not exist");
        assert_ne!(
            dest, existing_folder,
            "Should create new folder with timestamp"
        );

        // Destination should have timestamp suffix
        let dest_name = dest.file_name().unwrap().to_str().unwrap();
        assert!(
            dest_name.starts_with("test_scene_"),
            "Should have timestamp suffix"
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_move_to_error_success() -> Result<()> {
        // Setup
        let test_root = TempDir::new()?;
        let input_dir = test_root.path().join("input");
        let error_dir = test_root.path().join("error");
        let source_folder = input_dir.join("failed_scene");

        fs::create_dir_all(&source_folder).await?;
        fs::write(source_folder.join("error.log"), "error details").await?;

        // Execute
        let dest = move_to_error(&source_folder, &error_dir).await?;

        // Verify
        assert!(dest.exists(), "Destination folder should exist");
        assert!(!source_folder.exists(), "Source folder should be moved");
        assert!(dest.join("error.log").exists(), "File should be preserved");
        assert_eq!(
            dest,
            error_dir.join("failed_scene"),
            "Should be moved to correct location"
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_move_to_error_collision() -> Result<()> {
        // Setup
        let test_root = TempDir::new()?;
        let input_dir = test_root.path().join("input");
        let error_dir = test_root.path().join("error");
        let source_folder = input_dir.join("failed_scene");
        let existing_folder = error_dir.join("failed_scene");

        fs::create_dir_all(&source_folder).await?;
        fs::create_dir_all(&existing_folder).await?;
        fs::write(source_folder.join("new_error.log"), "new error").await?;
        fs::write(existing_folder.join("old_error.log"), "old error").await?;

        // Execute
        let dest = move_to_error(&source_folder, &error_dir).await?;

        // Verify
        assert!(dest.exists(), "Destination folder should exist");
        assert!(!source_folder.exists(), "Source folder should be moved");
        assert!(
            existing_folder.exists(),
            "Original folder should be preserved"
        );
        assert!(dest.join("new_error.log").exists(), "New file should exist");
        assert!(
            !dest.join("old_error.log").exists(),
            "Old file should not exist"
        );
        assert_ne!(
            dest, existing_folder,
            "Should create new folder with timestamp"
        );

        // Destination should have timestamp suffix
        let dest_name = dest.file_name().unwrap().to_str().unwrap();
        assert!(
            dest_name.starts_with("failed_scene_"),
            "Should have timestamp suffix"
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_preserves_folder_structure() -> Result<()> {
        // Setup
        let test_root = TempDir::new()?;
        let input_dir = test_root.path().join("input");
        let processed_dir = test_root.path().join("processed");
        let source_folder = input_dir.join("complex_scene");

        // Create nested structure
        fs::create_dir_all(&source_folder.join("subfolder/nested")).await?;
        fs::write(source_folder.join("root.txt"), "root").await?;
        fs::write(source_folder.join("subfolder/mid.txt"), "mid").await?;
        fs::write(source_folder.join("subfolder/nested/deep.txt"), "deep").await?;

        // Execute
        let dest = move_to_processed(&source_folder, &processed_dir).await?;

        // Verify structure is preserved
        assert!(dest.join("root.txt").exists());
        assert!(dest.join("subfolder/mid.txt").exists());
        assert!(dest.join("subfolder/nested/deep.txt").exists());

        // Verify content
        let content = fs::read_to_string(dest.join("subfolder/nested/deep.txt")).await?;
        assert_eq!(content, "deep");

        Ok(())
    }
}
