//! How the reference sizes an image for EmbeddingGemma 2's vision encoder, so
//! llama.cpp sees the same pixels. mtmd fixes the token budget per context and
//! rescales only images outside its range, while the reference always fills
//! the budget; so images are resized here and mtmd takes them as they are.
//! Ported from DigUp's `ImageSizing`, which was checked against Pillow bit
//! for bit.

/// The vision encoder's soft-token budgets.
pub const BUDGETS: [u32; 5] = [70, 140, 280, 560, 1120];

/// transformers' Gemma 4 image processor: scale to fill `budget` soft tokens
/// (each 3 x 3 patches of 16 px) keeping the aspect ratio, each side rounded
/// down to a multiple of 48 px. Returns (height, width), or `None` for an
/// image too thin to embed.
pub fn target_size(height: usize, width: usize, budget: u32) -> Option<(usize, usize)> {
    let (patch, pooling) = (16_usize, 3_usize);
    let max_patches = budget as usize * pooling * pooling;
    let factor = ((max_patches * patch * patch) as f64 / (height * width) as f64).sqrt();
    let side = pooling * patch;
    let mut target_height = (factor * height as f64 / side as f64).floor() as usize * side;
    let mut target_width = (factor * width as f64 / side as f64).floor() as usize * side;
    if target_height == 0 && target_width == 0 {
        return None;
    }
    let max_side = (max_patches / (pooling * pooling)) * side;
    if target_height == 0 {
        target_height = side;
        target_width = (width / height * side).min(max_side);
    } else if target_width == 0 {
        target_width = side;
        target_height = (height / width * side).min(max_side);
    }
    Some((target_height, target_width))
}

const PRECISION: i32 = 22;

fn filter(value: f64) -> f64 {
    let x = value.abs();
    let a = -0.5;
    if x < 1.0 {
        ((a + 2.0) * x - (a + 3.0)) * x * x + 1.0
    } else if x < 2.0 {
        (((x - 5.0) * x + 8.0) * x - 4.0) * a
    } else {
        0.0
    }
}

/// For each output pixel: the first contributing input pixel, how many contribute, and their weights.
fn coefficients(in_size: usize, out_size: usize) -> (Vec<usize>, Vec<i32>, usize) {
    let scale = in_size as f64 / out_size as f64;
    let filter_scale = scale.max(1.0);
    let support = 2.0 * filter_scale;
    let size = support.ceil() as usize * 2 + 1;
    let mut bounds = vec![0_usize; out_size * 2];
    let mut weights = vec![0_i32; out_size * size];
    let mut row = vec![0.0_f64; size];
    for out in 0..out_size {
        let center = (out as f64 + 0.5) * scale;
        let first = ((center - support + 0.5) as i64).max(0) as usize;
        let count = ((center + support + 0.5) as usize).min(in_size) - first;
        let mut total = 0.0;
        for (x, slot) in row.iter_mut().enumerate() {
            *slot = if x < count {
                filter(((x + first) as f64 - center + 0.5) / filter_scale)
            } else {
                0.0
            };
            total += *slot;
        }
        for x in 0..size {
            let weight = if x < count && total != 0.0 {
                row[x] / total
            } else {
                row[x]
            };
            let rounding = if weight < 0.0 { -0.5 } else { 0.5 };
            weights[out * size + x] = (weight * f64::from(1_u32 << PRECISION) + rounding) as i32;
        }
        bounds[out * 2] = first;
        bounds[out * 2 + 1] = count;
    }
    (bounds, weights, size)
}

fn clip(value: i32) -> u8 {
    (value >> PRECISION).clamp(0, 255) as u8
}

/// Pillow's bicubic resampling of packed RGB (a = -0.5, the kernel widened
/// when shrinking, 22-bit fixed point): what the reference gets from
/// `PIL.Image.resize`.
pub fn resize_bicubic(
    source: &[u8],
    width: usize,
    height: usize,
    to_width: usize,
    to_height: usize,
) -> Vec<u8> {
    match (to_width != width, to_height != height) {
        (true, true) => {
            let pass = horizontal(source, width, height, to_width);
            vertical(&pass, height, to_width, to_height)
        }
        (true, false) => horizontal(source, width, height, to_width),
        (false, true) => vertical(source, height, width, to_height),
        (false, false) => source[..width * height * 3].to_vec(),
    }
}

fn horizontal(source: &[u8], in_width: usize, rows: usize, out_width: usize) -> Vec<u8> {
    let (bounds, weights, size) = coefficients(in_width, out_width);
    let mut out = vec![0_u8; out_width * rows * 3];
    for y in 0..rows {
        for x in 0..out_width {
            let mut p = (y * in_width + bounds[x * 2]) * 3;
            let (mut r, mut g, mut b) = (
                1_i32 << (PRECISION - 1),
                1_i32 << (PRECISION - 1),
                1_i32 << (PRECISION - 1),
            );
            for k in 0..bounds[x * 2 + 1] {
                let w = weights[x * size + k];
                r = r.wrapping_add(i32::from(source[p]).wrapping_mul(w));
                g = g.wrapping_add(i32::from(source[p + 1]).wrapping_mul(w));
                b = b.wrapping_add(i32::from(source[p + 2]).wrapping_mul(w));
                p += 3;
            }
            let o = (y * out_width + x) * 3;
            out[o] = clip(r);
            out[o + 1] = clip(g);
            out[o + 2] = clip(b);
        }
    }
    out
}

fn vertical(source: &[u8], in_height: usize, row_width: usize, out_height: usize) -> Vec<u8> {
    let (bounds, weights, size) = coefficients(in_height, out_height);
    let stride = row_width * 3;
    let mut out = vec![0_u8; stride * out_height];
    let mut sums = vec![0_i32; stride];
    for y in 0..out_height {
        sums.iter_mut().for_each(|sum| *sum = 1 << (PRECISION - 1));
        for k in 0..bounds[y * 2 + 1] {
            let row = &source[(bounds[y * 2] + k) * stride..][..stride];
            let w = weights[y * size + k];
            for (sum, value) in sums.iter_mut().zip(row) {
                *sum = sum.wrapping_add(i32::from(*value).wrapping_mul(w));
            }
        }
        for (slot, sum) in out[y * stride..][..stride].iter_mut().zip(&sums) {
            *slot = clip(*sum);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_fill_the_budget_in_multiples_of_48() {
        // A 1920 x 1080 screenshot at 280 tokens: 2520 patches of 16 px.
        let (height, width) = target_size(1080, 1920, 280).unwrap();
        assert_eq!((height % 48, width % 48), (0, 0));
        assert!(height * width <= 280 * 9 * 256);
        assert!(height * width > 280 * 9 * 256 / 2);
        // Aspect ratio is kept.
        assert!((width as f64 / height as f64 - 1920.0 / 1080.0).abs() < 0.2);
    }

    #[test]
    fn thin_images_get_one_row_and_dots_get_nothing() {
        // A strip: one 48 px row, as wide as the budget allows (70 tokens x 48 px).
        assert_eq!(target_size(10, 40_000, 70), Some((48, 3360)));
        assert_eq!(
            target_size(1, 1, 70).map(|(h, w)| h > 0 && w > 0),
            Some(true)
        );
    }

    #[test]
    fn a_flat_color_stays_flat_and_size_is_right() {
        let source = vec![200_u8; 100 * 60 * 3];
        let out = resize_bicubic(&source, 100, 60, 48, 96);
        assert_eq!(out.len(), 48 * 96 * 3);
        assert!(out.iter().all(|value| *value == 200));
        assert_eq!(resize_bicubic(&source, 100, 60, 100, 60), source);
    }

    #[test]
    fn coefficients_sum_to_one() {
        let (bounds, weights, size) = coefficients(100, 48);
        for out in 0..48 {
            let sum: i64 = weights[out * size..out * size + bounds[out * 2 + 1]]
                .iter()
                .map(|w| i64::from(*w))
                .sum();
            assert!((sum - (1 << PRECISION)).abs() < 64, "{out}: {sum}");
        }
    }
}
