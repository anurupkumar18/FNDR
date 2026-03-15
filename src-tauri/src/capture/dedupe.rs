//! Perceptual hashing for frame deduplication

use img_hash::{HasherConfig, ImageHash};

/// Perceptual hasher for deduplication
pub struct PerceptualHasher {
    hasher: img_hash::Hasher,
    last_hash: Option<ImageHash>,
}

impl PerceptualHasher {
    pub fn new() -> Self {
        let hasher = HasherConfig::new().hash_size(8, 8).to_hasher();
        Self {
            hasher,
            last_hash: None,
        }
    }

    /// Check if the image is a duplicate of the last one
    /// Returns true if duplicate (below threshold), false if new
    pub fn is_duplicate(&mut self, image_data: &[u8], threshold: u32) -> bool {
        // Decode image
        let image = match image::load_from_memory(image_data) {
            Ok(img) => img,
            Err(_) => return false, // Can't decode = treat as new
        };

        // Compute hash
        let hash = self.hasher.hash_image(&image);

        // Compare with last hash
        let is_dup = if let Some(ref last) = self.last_hash {
            let distance = hash.dist(last);
            distance < threshold
        } else {
            false
        };

        // Update last hash
        self.last_hash = Some(hash);

        is_dup
    }
}

impl Default for PerceptualHasher {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_identical_images_duplicate() {
        let mut hasher = PerceptualHasher::new();

        // Create a simple test image
        let img = image::DynamicImage::new_rgb8(100, 100);
        let mut buf = Vec::new();
        img.write_to(&mut std::io::Cursor::new(&mut buf), image::ImageFormat::Png)
            .unwrap();

        // First image is never duplicate
        assert!(!hasher.is_duplicate(&buf, 5));
        // Second identical image should be duplicate
        assert!(hasher.is_duplicate(&buf, 5));
    }

    #[test]
    fn test_different_images_not_duplicate() {
        let mut hasher = PerceptualHasher::new();

        // Create two different images
        let img1 = image::DynamicImage::new_rgb8(100, 100);
        let mut buf1 = Vec::new();
        img1.write_to(
            &mut std::io::Cursor::new(&mut buf1),
            image::ImageFormat::Png,
        )
        .unwrap();

        let mut img2 = image::RgbImage::new(100, 100);
        for pixel in img2.pixels_mut() {
            *pixel = image::Rgb([255, 0, 0]);
        }
        let img2 = image::DynamicImage::ImageRgb8(img2);
        let mut buf2 = Vec::new();
        img2.write_to(
            &mut std::io::Cursor::new(&mut buf2),
            image::ImageFormat::Png,
        )
        .unwrap();

        assert!(!hasher.is_duplicate(&buf1, 5));
        assert!(!hasher.is_duplicate(&buf2, 5));
    }
}
