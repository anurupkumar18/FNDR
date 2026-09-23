//! Perceptual hashing for frame deduplication

use img_hash::{HasherConfig, ImageHash};
use std::collections::VecDeque;

const HASH_HISTORY: usize = 3;

// Ported from FNDR v2 crates/fndr-capture/src/dedup.rs (T-303).

/// Sample a 9x8 luma grid from RGBA pixels (nearest neighbour, no full-resolution decode).
pub fn luma_9x8_from_rgba(rgba: &[u8], width: usize, height: usize) -> [u8; 72] {
    let mut out = [0u8; 72];
    for row in 0..8 {
        for col in 0..9 {
            let x = (col * (width - 1)) / 8;
            let y = (row * (height - 1)) / 7;
            let i = (y * width + x) * 4;
            let (r, g, b) = (rgba[i] as f32, rgba[i + 1] as f32, rgba[i + 2] as f32);
            out[row * 9 + col] = (0.299 * r + 0.587 * g + 0.114 * b) as u8;
        }
    }
    out
}

/// Difference hash: one bit per horizontally adjacent pair of the 9x8 grid.
pub fn dhash_9x8(luma: &[u8; 72]) -> u64 {
    let mut hash = 0u64;
    for row in 0..8 {
        for col in 0..8 {
            if luma[row * 9 + col] > luma[row * 9 + col + 1] {
                hash |= 1u64 << (row * 8 + col);
            }
        }
    }
    hash
}

pub fn hamming(a: u64, b: u64) -> u32 {
    (a ^ b).count_ones()
}

/// True when `hash` matches the frame two steps back but not the previous frame (A-B-A flicker).
pub fn is_aba(recent: &VecDeque<u64>, hash: u64, threshold: u32) -> bool {
    if recent.len() < 2 {
        return false;
    }
    let previous = recent[recent.len() - 1];
    let two_back = recent[recent.len() - 2];
    hamming(hash, two_back) <= threshold && hamming(hash, previous) > threshold
}

/// Perceptual hasher for deduplication
pub struct PerceptualHasher {
    hasher: img_hash::Hasher,
    last_hash: Option<ImageHash>,
    last_average_rgb: Option<[u8; 3]>,
    recent_hashes: VecDeque<(ImageHash, [u8; 3])>,
    last_bytes_fingerprint: Option<u64>,
}

impl PerceptualHasher {
    pub fn new() -> Self {
        let hasher = HasherConfig::new().hash_size(8, 8).to_hasher();
        Self {
            hasher,
            last_hash: None,
            last_average_rgb: None,
            recent_hashes: VecDeque::with_capacity(HASH_HISTORY),
            last_bytes_fingerprint: None,
        }
    }

    /// Check if the image is a duplicate of the last one
    /// Returns true if duplicate (below threshold), false if new
    pub fn is_duplicate(&mut self, image_data: &[u8], threshold: u32) -> bool {
        let bytes_fingerprint = stable_bytes_fingerprint(image_data);
        if self
            .last_bytes_fingerprint
            .map(|previous| previous == bytes_fingerprint)
            .unwrap_or(false)
        {
            return true;
        }

        // Decode image
        let image = match image::load_from_memory(image_data) {
            Ok(img) => img,
            Err(_) => return false, // Can't decode = treat as new
        };

        // Compute hash
        let hash = self.hasher.hash_image(&image);
        let average_rgb = average_rgb(&image);

        // Compare with last hash
        let mut is_dup = if let (Some(ref last), Some(last_average_rgb)) =
            (&self.last_hash, self.last_average_rgb)
        {
            let distance = hash.dist(last);
            distance < threshold && rgb_distance(average_rgb, last_average_rgb) <= 24
        } else {
            false
        };

        // Detect short alternating loops (A -> B -> A), not just exact consecutive duplicates.
        if !is_dup {
            is_dup = self.recent_hashes.iter().any(|(prev_hash, prev_rgb)| {
                let distance = hash.dist(prev_hash);
                distance < threshold.saturating_sub(1).max(1)
                    && rgb_distance(average_rgb, *prev_rgb) <= 20
            });
        }

        // Update last hash
        self.last_hash = Some(hash.clone());
        self.last_average_rgb = Some(average_rgb);
        self.last_bytes_fingerprint = Some(bytes_fingerprint);
        self.recent_hashes.push_back((hash, average_rgb));
        if self.recent_hashes.len() > HASH_HISTORY {
            self.recent_hashes.pop_front();
        }

        is_dup
    }
}

fn stable_bytes_fingerprint(bytes: &[u8]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    bytes.len().hash(&mut hasher);
    bytes.hash(&mut hasher);
    hasher.finish()
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

#[cfg(test)]
mod dhash_tests {
    use super::*;

    #[test]
    fn dhash_identical_zero_and_reversed_gradient_is_max_distance() {
        let mut inc = [0u8; 72];
        let mut dec = [0u8; 72];
        for row in 0..8 {
            for col in 0..9 {
                inc[row * 9 + col] = (col * 10) as u8;
                dec[row * 9 + col] = (250 - col * 10) as u8;
            }
        }
        assert_eq!(hamming(dhash_9x8(&inc), dhash_9x8(&inc)), 0);
        assert_eq!(hamming(dhash_9x8(&inc), dhash_9x8(&dec)), 64);
    }

    #[test]
    fn solid_color_frames_hash_identically() {
        let rgba = vec![200u8; 64 * 48 * 4];
        assert_eq!(dhash_9x8(&luma_9x8_from_rgba(&rgba, 64, 48)), 0);
    }

    #[test]
    fn aba_detects_flicker_but_not_progress() {
        let (a, b) = (0u64, u64::MAX);
        let mut recent = VecDeque::new();
        recent.push_back(a);
        recent.push_back(b);
        assert!(is_aba(&recent, a, 4));
        assert!(!is_aba(&recent, b, 4));
        let mut single = VecDeque::new();
        single.push_back(a);
        assert!(!is_aba(&single, a, 4));
    }
}
