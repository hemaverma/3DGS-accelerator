#!/usr/bin/env python3
"""
Create minimal COLMAP reconstruction data for testing.

This script generates valid COLMAP binary files (cameras.bin, images.bin, points3D.bin)
with minimal data suitable for integration testing.
"""

import struct
import sys
from pathlib import Path


def write_cameras_bin(output_path):
    """Write minimal cameras.bin file."""
    with open(output_path, "wb") as f:
        # Number of cameras
        f.write(struct.pack("Q", 1))
        
        # Camera 0: OPENCV model
        camera_id = 0
        model_id = 4  # OPENCV model
        width = 640
        height = 480
        params = [500.0, 500.0, 320.0, 240.0, 0.0, 0.0, 0.0, 0.0]  # fx, fy, cx, cy, k1, k2, p1, p2
        
        f.write(struct.pack("I", camera_id))
        f.write(struct.pack("i", model_id))
        f.write(struct.pack("Q", width))
        f.write(struct.pack("Q", height))
        f.write(struct.pack(f"{len(params)}d", *params))
    
    print(f"Created cameras.bin with 1 camera")


def write_images_bin(output_path, num_images=5):
    """Write minimal images.bin file."""
    with open(output_path, "wb") as f:
        # Number of images
        f.write(struct.pack("Q", num_images))
        
        for i in range(num_images):
            image_id = i
            # Quaternion (identity rotation with slight variation)
            qw, qx, qy, qz = 1.0, 0.0, 0.0, 0.0
            # Translation (camera positions in a circle)
            import math
            angle = (i / num_images) * 2 * math.pi
            tx = math.cos(angle) * 2.0
            ty = math.sin(angle) * 2.0
            tz = 0.0
            
            camera_id = 0
            image_name = f"frame_{i:06d}.jpg"
            
            f.write(struct.pack("I", image_id))
            f.write(struct.pack("4d", qw, qx, qy, qz))
            f.write(struct.pack("3d", tx, ty, tz))
            f.write(struct.pack("I", camera_id))
            
            # Write image name (null-terminated)
            f.write(image_name.encode("utf-8") + b"\x00")
            
            # Number of 2D points (0 for simplicity)
            f.write(struct.pack("Q", 0))
    
    print(f"Created images.bin with {num_images} images")


def write_points3d_bin(output_path, num_points=1000):
    """Write minimal points3D.bin file."""
    import random
    
    with open(output_path, "wb") as f:
        # Number of points
        f.write(struct.pack("Q", num_points))
        
        for i in range(num_points):
            point_id = i
            # Random 3D position in a cube
            x = random.uniform(-1.0, 1.0)
            y = random.uniform(-1.0, 1.0)
            z = random.uniform(-0.5, 0.5)
            
            # Random RGB color
            r = random.randint(100, 255)
            g = random.randint(100, 255)
            b = random.randint(100, 255)
            
            # Random error
            error = random.uniform(0.1, 1.0)
            
            f.write(struct.pack("Q", point_id))
            f.write(struct.pack("3d", x, y, z))
            f.write(struct.pack("3B", r, g, b))
            f.write(struct.pack("d", error))
            
            # Track length (0 for simplicity)
            f.write(struct.pack("Q", 0))
    
    print(f"Created points3D.bin with {num_points} points")


def create_placeholder_images(image_dir, num_images=5):
    """Create minimal placeholder image files."""
    try:
        from PIL import Image
        import numpy as np
        
        for i in range(num_images):
            # Create a simple colored image
            color = ((i * 50) % 256, (i * 100) % 256, (i * 150) % 256)
            img_array = np.ones((480, 640, 3), dtype=np.uint8)
            img_array[:, :] = color
            
            img = Image.fromarray(img_array, 'RGB')
            img.save(image_dir / f"frame_{i:06d}.jpg", quality=85)
        
        print(f"Created {num_images} placeholder images")
        return True
    except ImportError:
        print("PIL not available, skipping image creation (COLMAP data still usable)")
        # Create empty files as placeholders
        for i in range(num_images):
            (image_dir / f"frame_{i:06d}.jpg").touch()
        print(f"Created {num_images} empty placeholder files")
        return False


def main():
    # Determine output directory
    if len(sys.argv) > 1:
        output_dir = Path(sys.argv[1])
    else:
        output_dir = Path(__file__).parent.parent / "testdata" / "sample_scene" / "test_run"
    
    colmap_dir = output_dir / "colmap" / "sparse" / "0"
    image_dir = output_dir / "images"
    
    # Create directories
    colmap_dir.mkdir(parents=True, exist_ok=True)
    image_dir.mkdir(parents=True, exist_ok=True)
    
    print(f"Creating test COLMAP data in: {colmap_dir}")
    print(f"Creating test images in: {image_dir}")
    print()
    
    # Create COLMAP binary files
    write_cameras_bin(colmap_dir / "cameras.bin")
    write_images_bin(colmap_dir / "images.bin", num_images=5)
    write_points3d_bin(colmap_dir / "points3D.bin", num_points=1000)
    
    print()
    
    # Create placeholder images
    create_placeholder_images(image_dir, num_images=5)
    
    print()
    print("✓ Test COLMAP data created successfully!")
    print(f"  Camera: OPENCV model, 640x480")
    print(f"  Images: 5 views in circular arrangement")
    print(f"  Points: 1000 3D points")
    print()
    print("You can now test backends with this data:")
    print(f"  Images dir: {image_dir}")
    print(f"  COLMAP sparse: {colmap_dir}")


if __name__ == "__main__":
    main()
