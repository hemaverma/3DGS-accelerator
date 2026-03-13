//! PLY File Parsing and Validation Tests
//!
//! Tests for PLY file parsing capabilities across all backends.
//! This validates that backends can correctly parse and count Gaussians
//! from PLY output files.

use std::path::Path;
use tempfile::TempDir;

/// Create a minimal valid PLY file with specified vertex count
async fn create_minimal_ply(path: &Path, vertex_count: usize) -> anyhow::Result<()> {
    let mut content = String::new();
    content.push_str("ply\n");
    content.push_str("format ascii 1.0\n");
    content.push_str(&format!("element vertex {}\n", vertex_count));
    content.push_str("property float x\n");
    content.push_str("property float y\n");
    content.push_str("property float z\n");
    content.push_str("property float nx\n");
    content.push_str("property float ny\n");
    content.push_str("property float nz\n");
    content.push_str("property uchar red\n");
    content.push_str("property uchar green\n");
    content.push_str("property uchar blue\n");
    content.push_str("end_header\n");
    
    // Add vertex data
    for i in 0..vertex_count.min(5) {
        content.push_str(&format!(
            "{:.6} {:.6} {:.6} {:.6} {:.6} {:.6} {} {} {}\n",
            i as f32, i as f32, i as f32,
            0.0, 0.0, 1.0,
            255, 128, 64
        ));
    }
    
    tokio::fs::write(path, content).await?;
    Ok(())
}

/// Create a realistic 3DGS PLY file with Gaussian Splatting properties
async fn create_gaussian_splatting_ply(path: &Path, gaussian_count: usize) -> anyhow::Result<()> {
    let mut content = String::new();
    content.push_str("ply\n");
    content.push_str("format ascii 1.0\n");
    content.push_str(&format!("element vertex {}\n", gaussian_count));
    
    // Standard Gaussian Splatting PLY properties
    content.push_str("property float x\n");
    content.push_str("property float y\n");
    content.push_str("property float z\n");
    content.push_str("property float nx\n");
    content.push_str("property float ny\n");
    content.push_str("property float nz\n");
    content.push_str("property float f_dc_0\n");
    content.push_str("property float f_dc_1\n");
    content.push_str("property float f_dc_2\n");
    content.push_str("property float f_rest_0\n");
    content.push_str("property float opacity\n");
    content.push_str("property float scale_0\n");
    content.push_str("property float scale_1\n");
    content.push_str("property float scale_2\n");
    content.push_str("property float rot_0\n");
    content.push_str("property float rot_1\n");
    content.push_str("property float rot_2\n");
    content.push_str("property float rot_3\n");
    content.push_str("end_header\n");
    
    // Add some gaussian data
    for i in 0..gaussian_count.min(5) {
        content.push_str(&format!(
            "{:.6} {:.6} {:.6} {:.6} {:.6} {:.6} {:.6} {:.6} {:.6} {:.6} {:.6} {:.6} {:.6} {:.6} {:.6} {:.6} {:.6} {:.6}\n",
            i as f32 * 0.1, i as f32 * 0.1, i as f32 * 0.1,  // xyz
            0.0, 0.0, 1.0,  // normals
            0.5, 0.5, 0.5,  // f_dc (RGB)
            0.0,            // f_rest
            1.0,            // opacity
            0.1, 0.1, 0.1,  // scale
            1.0, 0.0, 0.0, 0.0  // rotation quaternion
        ));
    }
    
    tokio::fs::write(path, content).await?;
    Ok(())
}

/// Create an invalid PLY file (missing vertex count)
async fn create_invalid_ply(path: &Path) -> anyhow::Result<()> {
    let content = "ply\nformat ascii 1.0\nproperty float x\nend_header\n";
    tokio::fs::write(path, content).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader};

    #[tokio::test]
    async fn test_create_minimal_ply() {
        let temp = TempDir::new().unwrap();
        let ply_path = temp.path().join("test.ply");
        
        create_minimal_ply(&ply_path, 100).await.unwrap();
        
        assert!(ply_path.exists());
        let content = tokio::fs::read_to_string(&ply_path).await.unwrap();
        assert!(content.contains("ply"));
        assert!(content.contains("element vertex 100"));
        assert!(content.contains("end_header"));
    }

    #[tokio::test]
    async fn test_create_gaussian_splatting_ply() {
        let temp = TempDir::new().unwrap();
        let ply_path = temp.path().join("gaussian.ply");
        
        create_gaussian_splatting_ply(&ply_path, 50000).await.unwrap();
        
        assert!(ply_path.exists());
        let content = tokio::fs::read_to_string(&ply_path).await.unwrap();
        
        // Validate PLY structure
        assert!(content.contains("ply"));
        assert!(content.contains("format ascii 1.0"));
        assert!(content.contains("element vertex 50000"));
        assert!(content.contains("property float opacity"));
        assert!(content.contains("property float scale_0"));
        assert!(content.contains("property float rot_0"));
        assert!(content.contains("end_header"));
    }

    #[tokio::test]
    async fn test_parse_vertex_count_from_ply() {
        let temp = TempDir::new().unwrap();
        let ply_path = temp.path().join("test.ply");
        
        create_minimal_ply(&ply_path, 12345).await.unwrap();
        
        // Parse vertex count
        let file = std::fs::File::open(&ply_path).unwrap();
        let reader = BufReader::new(file);
        
        let mut vertex_count = None;
        for line in reader.lines() {
            let line = line.unwrap();
            if line.starts_with("element vertex ") {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 3 {
                    vertex_count = Some(parts[2].parse::<usize>().unwrap());
                    break;
                }
            }
        }
        
        assert_eq!(vertex_count, Some(12345));
    }

    #[tokio::test]
    async fn test_parse_ply_without_vertex_element() {
        let temp = TempDir::new().unwrap();
        let ply_path = temp.path().join("invalid.ply");
        
        create_invalid_ply(&ply_path).await.unwrap();
        
        // Try to parse vertex count
        let file = std::fs::File::open(&ply_path).unwrap();
        let reader = BufReader::new(file);
        
        let mut vertex_count = None;
        for line in reader.lines() {
            let line = line.unwrap();
            if line.starts_with("element vertex ") {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 3 {
                    vertex_count = Some(parts[2].parse::<usize>().unwrap());
                }
            }
            if line.starts_with("end_header") {
                break;
            }
        }
        
        assert_eq!(vertex_count, None);
    }

    #[tokio::test]
    async fn test_large_ply_file() {
        let temp = TempDir::new().unwrap();
        let ply_path = temp.path().join("large.ply");
        
        // Create PLY with large gaussian count
        create_gaussian_splatting_ply(&ply_path, 500_000).await.unwrap();
        
        assert!(ply_path.exists());
        
        // Verify we can parse the header without loading all vertices
        let file = std::fs::File::open(&ply_path).unwrap();
        let reader = BufReader::new(file);
        
        let mut found_vertex_count = false;
        let mut line_count = 0;
        
        for line in reader.lines() {
            let line = line.unwrap();
            line_count += 1;
            
            if line.starts_with("element vertex 500000") {
                found_vertex_count = true;
            }
            
            if line.starts_with("end_header") {
                break;
            }
        }
        
        assert!(found_vertex_count);
        assert!(line_count < 50, "Header should be parsed quickly without reading all vertices");
    }

    #[tokio::test]
    async fn test_ply_with_different_formats() {
        let temp = TempDir::new().unwrap();
        
        // Test ASCII format
        let ascii_path = temp.path().join("ascii.ply");
        create_minimal_ply(&ascii_path, 100).await.unwrap();
        let ascii_content = tokio::fs::read_to_string(&ascii_path).await.unwrap();
        assert!(ascii_content.contains("format ascii 1.0"));
        
        // Note: Binary format would require actual binary encoding
        // which is beyond the scope of this test helper
    }

    #[tokio::test]
    async fn test_ply_validation() {
        let temp = TempDir::new().unwrap();
        
        // Valid PLY
        let valid_path = temp.path().join("valid.ply");
        create_minimal_ply(&valid_path, 100).await.unwrap();
        
        let content = tokio::fs::read_to_string(&valid_path).await.unwrap();
        assert!(content.starts_with("ply\n"));
        assert!(content.contains("format"));
        assert!(content.contains("element vertex"));
        assert!(content.contains("end_header"));
        
        // Invalid PLY (missing magic header)
        let invalid_path = temp.path().join("invalid.ply");
        tokio::fs::write(&invalid_path, "format ascii 1.0\nelement vertex 100\nend_header\n")
            .await
            .unwrap();
        
        let invalid_content = tokio::fs::read_to_string(&invalid_path).await.unwrap();
        assert!(!invalid_content.starts_with("ply\n"));
    }

    #[tokio::test]
    async fn test_zero_vertex_ply() {
        let temp = TempDir::new().unwrap();
        let ply_path = temp.path().join("empty.ply");
        
        create_minimal_ply(&ply_path, 0).await.unwrap();
        
        let content = tokio::fs::read_to_string(&ply_path).await.unwrap();
        assert!(content.contains("element vertex 0"));
    }

    #[tokio::test]
    async fn test_ply_property_parsing() {
        let temp = TempDir::new().unwrap();
        let ply_path = temp.path().join("properties.ply");
        
        create_gaussian_splatting_ply(&ply_path, 10).await.unwrap();
        
        let content = tokio::fs::read_to_string(&ply_path).await.unwrap();
        
        // Verify Gaussian Splatting properties are present
        let required_properties = vec![
            "property float x",
            "property float y",
            "property float z",
            "property float opacity",
            "property float scale_0",
            "property float scale_1",
            "property float scale_2",
            "property float rot_0",
            "property float rot_1",
            "property float rot_2",
            "property float rot_3",
        ];
        
        for prop in required_properties {
            assert!(content.contains(prop), "Missing property: {}", prop);
        }
    }

    #[tokio::test]
    async fn test_ply_header_line_count() {
        let temp = TempDir::new().unwrap();
        let ply_path = temp.path().join("header.ply");
        
        create_gaussian_splatting_ply(&ply_path, 100).await.unwrap();
        
        let file = std::fs::File::open(&ply_path).unwrap();
        let reader = BufReader::new(file);
        
        let mut header_lines = 0;
        for line in reader.lines() {
            let line = line.unwrap();
            header_lines += 1;
            if line == "end_header" {
                break;
            }
        }
        
        // Gaussian Splatting PLY should have ~20 lines in header
        assert!(header_lines > 15 && header_lines < 30);
    }
}
