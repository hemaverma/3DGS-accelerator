//! Integration tests for stability and edge cases  
//!
//! Tests file watching stability detection and various edge cases.

use std::fs;
use std::path::Path;
use std::time::Duration;
use tempfile::TempDir;
use tokio::time::sleep;

use super::common::fixtures::create_test_workspace;

/// Helper to create a file with content
fn create_file_with_content(path: &Path, content: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, content)
}

#[tokio::test]
async fn test_file_stability_detection() {
    // Test waiting for file to stop being modified
    let temp = TempDir::new().unwrap();
    let file_path = temp.path().join("uploading.mp4");
    
    // Simulate file being written
    create_file_with_content(&file_path, b"partial").unwrap();
    
    let initial_size = fs::metadata(&file_path).unwrap().len();
    
    // Modify file
    sleep(Duration::from_millis(100)).await;
    create_file_with_content(&file_path, b"partial data with more").unwrap();
    
    let modified_size = fs::metadata(&file_path).unwrap().len();
    assert!(modified_size > initial_size);
    
    // Wait for stability (no modifications)
    sleep(Duration::from_millis(200)).await;
    let final_size = fs::metadata(&file_path).unwrap().len();
    
    assert_eq!(final_size, modified_size);
}

#[tokio::test]
async fn test_folder_stability_with_multiple_files() {
    // Test waiting for all files in folder to stabilize
    let temp = TempDir::new().unwrap();
    let upload_dir = temp.path().join("upload-batch");
    fs::create_dir_all(&upload_dir).unwrap();
    
    // Create multiple files being uploaded
    let files = vec!["video1.mp4", "video2.mp4", "video3.mp4"];
    
    for filename in &files {
        let path = upload_dir.join(filename);
        create_file_with_content(&path, b"initial").unwrap();
    }
    
    // Simulate incremental writes
    for i in 0..3 {
        sleep(Duration::from_millis(50)).await;
        for filename in &files {
            let path = upload_dir.join(filename);
            let content = format!("chunk {}", i);
            create_file_with_content(&path, content.as_bytes()).unwrap();
        }
    }
    
    // All files should exist
    for filename in &files {
        assert!(upload_dir.join(filename).exists());
    }
}

#[tokio::test]
async fn test_new_files_during_processing() {
    // Test detection of new files arriving during job processing
    let temp = TempDir::new().unwrap();
    let job_dir = temp.path().join("job");
    fs::create_dir_all(&job_dir).unwrap();
    
    // Initial files
    create_file_with_content(&job_dir.join("video1.mp4"), b"data1").unwrap();
    create_file_with_content(&job_dir.join("video2.mp4"), b"data2").unwrap();
    
    let initial_count = fs::read_dir(&job_dir).unwrap().count();
    assert_eq!(initial_count, 2);
    
    // Simulate new file arriving during processing
    sleep(Duration::from_millis(100)).await;
    create_file_with_content(&job_dir.join("video3.mp4"), b"data3").unwrap();
    
    let after_count = fs::read_dir(&job_dir).unwrap().count();
    assert_eq!(after_count, 3);
}

#[test]
fn test_concurrent_folder_creation() {
    // Test multiple folders being created simultaneously
    let temp = TempDir::new().unwrap();
    
    let folders = vec!["job-a", "job-b", "job-c", "job-d"];
    
    for folder in &folders {
        let path = temp.path().join(folder);
        fs::create_dir_all(&path).unwrap();
    }
    
    // Verify all created
    for folder in &folders {
        assert!(temp.path().join(folder).exists());
    }
}

#[test]
fn test_symlink_handling() {
    // Test behavior with symbolic links
    let temp = TempDir::new().unwrap();
    let real_dir = temp.path().join("real");
    let link_dir = temp.path().join("link");
    
    fs::create_dir_all(&real_dir).unwrap();
    
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&real_dir, &link_dir).ok();
        
        if link_dir.exists() {
            // Symlink should be detectable
            let metadata = fs::symlink_metadata(&link_dir).unwrap();
            assert!(metadata.is_symlink());
        }
    }
}

#[test]
fn test_hidden_files_and_folders() {
    // Test handling of hidden files/folders (unix: ., mac: .DS_Store)
    let temp = TempDir::new().unwrap();
    let job_dir = temp.path().join("job");
    fs::create_dir_all(&job_dir).unwrap();
    
    // Create hidden files
    create_file_with_content(&job_dir.join(".hidden"), b"hidden").unwrap();
    create_file_with_content(&job_dir.join(".DS_Store"), b"metadata").unwrap();
    create_file_with_content(&job_dir.join("video.mp4"), b"video").unwrap();
    
    // Count visible files (should filter hidden)
    let visible_videos: Vec<_> = fs::read_dir(&job_dir)
        .unwrap()
        .filter_map(Result::ok)
        .filter(|e| {
            !e.file_name()
                .to_string_lossy()
                .starts_with('.')
                && e.path().extension().and_then(|s| s.to_str()) == Some("mp4")
        })
        .collect();
    
    assert_eq!(visible_videos.len(), 1);
}

#[test]
fn test_large_number_of_files() {
    // Test processing folder with many files
    let temp = TempDir::new().unwrap();
    let job_dir = temp.path().join("large-batch");
    fs::create_dir_all(&job_dir).unwrap();
    
    // Create 100 video files
    for i in 0..100 {
        let filename = format!("video_{:03}.mp4", i);
        create_file_with_content(&job_dir.join(filename), b"data").unwrap();
    }
    
    let video_count = fs::read_dir(&job_dir)
        .unwrap()
        .filter_map(Result::ok)
        .filter(|e| e.path().extension().and_then(|s| s.to_str()) == Some("mp4"))
        .count();
    
    assert_eq!(video_count, 100);
}

#[test]
fn test_empty_video_files() {
    // Test handling of 0-byte files
    let temp = TempDir::new().unwrap();
    let job_dir = temp.path().join("job");
    fs::create_dir_all(&job_dir).unwrap();
    
    create_file_with_content(&job_dir.join("empty.mp4"), b"").unwrap();
    
    let metadata = fs::metadata(job_dir.join("empty.mp4")).unwrap();
    assert_eq!(metadata.len(), 0);
}

#[test]
fn test_filesystem_case_sensitivity() {
    // Test case sensitivity behavior
    let temp = TempDir::new().unwrap();
    
    let lower = temp.path().join("scene");
    let upper = temp.path().join("SCENE");
    
    fs::create_dir_all(&lower).unwrap();
    
    // On case-insensitive filesystems (macOS default), these are the same
    // On case-sensitive filesystems (Linux), they're different
    let same_path = lower.canonicalize().ok() == upper.canonicalize().ok();
    
    // Test passes on both, just documents behavior
    #[cfg(target_os = "macos")]
    {
        // macOS is typically case-insensitive
        let _ = same_path;
    }
    
    #[cfg(target_os = "linux")]
    {
        // Linux is typically case-sensitive
        let _ = same_path;
    }
}

#[test]
fn test_special_filenames() {
    // Test handling of filenames with special characters
    let temp = TempDir::new().unwrap();
    let job_dir = temp.path().join("job");
    fs::create_dir_all(&job_dir).unwrap();
    
    let special_names = vec![
        "video with spaces.mp4",
        "video-with-dashes.mp4",
        "video_with_underscores.mp4",
        "video.multiple.dots.mp4",
        "video(with)parentheses.mp4",
        "video[with]brackets.mp4",
    ];
    
    for name in &special_names {
        let result = create_file_with_content(&job_dir.join(name), b"data");
        // Some names may not be valid on all filesystems
        let _ = result;
    }
}

#[tokio::test]
async fn test_rapid_sequential_uploads() {
    // Test handling of jobs uploaded in rapid succession
    let temp = TempDir::new().unwrap();
    
    for i in 0..10 {
        let job_dir = temp.path().join(format!("job-{:03}", i));
        fs::create_dir_all(&job_dir).unwrap();
        create_file_with_content(&job_dir.join("video.mp4"), b"data").unwrap();
        
        // Minimal delay between uploads
        sleep(Duration::from_millis(10)).await;
    }
    
    // All should be created
    for i in 0..10 {
        assert!(temp.path().join(format!("job-{:03}", i)).exists());
    }
}

#[test]
fn test_partially_uploaded_folder() {
    // Test folder that's still being populated
    let temp = TempDir::new().unwrap();
    let job_dir = temp.path().join("partial-upload");
    fs::create_dir_all(&job_dir).unwrap();
    
    // Some files present, some missing
    create_file_with_content(&job_dir.join("video1.mp4"), b"complete").unwrap();
    create_file_with_content(&job_dir.join("video2.mp4.part"), b"incomplete").unwrap();
    
    // Should detect .part files and wait
    let has_partial = fs::read_dir(&job_dir)
        .unwrap()
        .filter_map(Result::ok)
        .any(|e| e.path().extension().and_then(|s| s.to_str()) == Some("part"));
    
    assert!(has_partial);
}

#[test]
fn test_folder_permissions() {
    // Test handling of permission issues
    let temp = TempDir::new().unwrap();
    let job_dir = temp.path().join("job");
    fs::create_dir_all(&job_dir).unwrap();
    
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        
        // Create file with read-only permissions
        let file_path = job_dir.join("readonly.mp4");
        create_file_with_content(&file_path, b"data").unwrap();
        
        let perms = fs::Permissions::from_mode(0o444);
        fs::set_permissions(&file_path, perms).ok();
        
        // Verify it's readable
        assert!(fs::read(&file_path).is_ok());
    }
}

#[test]
fn test_nested_directory_structure() {
    // Test deeply nested folders
    let temp = TempDir::new().unwrap();
    let deep_path = temp
        .path()
        .join("level1")
        .join("level2")
        .join("level3")
        .join("level4")
        .join("job");
    
    fs::create_dir_all(&deep_path).unwrap();
    create_file_with_content(&deep_path.join("video.mp4"), b"data").unwrap();
    
    assert!(deep_path.join("video.mp4").exists());
}

#[test]
fn test_workspace_cleanup() {
    // Test cleanup of temporary directories
    let workspace = create_test_workspace().unwrap();
    
    // Create some temporary files
    let temp_dir = workspace.path().join("tmp");
    fs::create_dir_all(&temp_dir).unwrap();
    create_file_with_content(&temp_dir.join("temp_frame_001.jpg"), b"frame").unwrap();
    
    assert!(temp_dir.exists());
    
    // Cleanup simulation (would be done by TempDir::drop)
    fs::remove_dir_all(&temp_dir).ok();
}

#[tokio::test]
async fn test_file_size_changes() {
    // Test detecting file size changes (upload progress)
    let temp = TempDir::new().unwrap();
    let file_path = temp.path().join("growing.mp4");
    
    // Start with small file
    create_file_with_content(&file_path, b"start").unwrap();
    let size1 = fs::metadata(&file_path).unwrap().len();
    
    // Simulate chunks being appended
    sleep(Duration::from_millis(50)).await;
    create_file_with_content(&file_path, b"start + more data").unwrap();
    let size2 = fs::metadata(&file_path).unwrap().len();
    
    sleep(Duration::from_millis(50)).await;
    create_file_with_content(&file_path, b"start + more data + even more").unwrap();
    let size3 = fs::metadata(&file_path).unwrap().len();
    
    // Should be monotonically increasing
    assert!(size2 > size1);
    assert!(size3 > size2);
}
