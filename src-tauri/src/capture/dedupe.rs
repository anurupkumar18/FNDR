//! Perceptual hashing for frame deduplication

use img_hash::{HasherConfig, ImageHash};

/// Perceptual hasher for deduplication
pub struct PerceptualHasher {
    hasher: img_hash::Hasher,
    last_hash: Option<ImageHash>,
    last_average_rgb: Option<[u8; 3]>,
}

impl PerceptualHasher {
    pub fn new() -> Self {
        let hasher = HasherConfig::new().hash_size(8, 8).to_hasher();
        Self {
            hasher,
            last_hash: None,
            last_average_rgb: None,
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
        let average_rgb = average_rgb(&image);

        // Compare with last hash
        let is_dup = if let (Some(ref last), Some(last_average_rgb)) =
            (&self.last_hash, self.last_average_rgb)
        {
            let distance = hash.dist(last);
            distance < threshold && rgb_distance(average_rgb, last_average_rgb) <= 24
        } else {
            false
        };

        // Update last hash
        self.last_hash = Some(hash);
        self.last_average_rgb = Some(average_rgb);

        is_dup
    }
}

fn average_rgb(image: &image::DynamicImage) -> [u8; 3] {
    let rgb = image.to_rgb8();
    let mut sums = [0_u64; 3];
    let mut count = 0_u64;

    for pixel in rgb.pixels() {
        sums[0] += pixel[0] as u64;
        sums[1] += pixel[1] as u64;
        sums[2] += pixel[2] as u64;
        count += 1;
    }

    if count == 0 {
        return [0, 0, 0];
    }

    [
        (sums[0] / count) as u8,
        (sums[1] / count) as u8,
        (sums[2] / count) as u8,
    ]
}

fn rgb_distance(a: [u8; 3], b: [u8; 3]) -> u32 {
    a.iter()
        .zip(b)
        .map(|(left, right)| (*left as i32 - right as i32).unsigned_abs())
        .sum()
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
        // Hamming distance can be small between flat fields; only distance 0 counts as dup at threshold 1.
        assert!(!hasher.is_duplicate(&buf2, 1));
    }
}
