//! Median-cut color quantization: reduce an image to N representative
//! colors. Consumer: the sixel emitter (≤256 hardware registers, we
//! default to 64 — see docs/design/gfx-three.md §2.3); reusable for any
//! future low-color target.
//!
//! Classic algorithm: start with one box holding every opaque pixel,
//! repeatedly split the box with the largest color spread along its
//! widest channel at the median pixel, until N boxes exist; each box's
//! mean color becomes a palette entry. Median-cut beats k-means here on
//! determinism and bounded cost, and beats octree on code size; its
//! known weakness (crushing small-but-distinct color islands) is
//! exactly what the Floyd–Steinberg pass afterwards compensates for.

use crate::base::Rgba;

/// Pixels above this count are sampled with a deterministic stride
/// before box-splitting — quantization is a statistical fit, and past
/// ~64k samples more pixels stop changing the medians while the sort
/// cost keeps growing.
const SAMPLE_CAP: usize = 65_536;

/// Reduce `pixels` to at most `max_colors` representative colors.
/// Fully transparent pixels are ignored (the sixel emitter renders
/// them as P2=1 holes, not as palette entries). Returns an empty
/// palette only when there are no visible pixels or `max_colors == 0`.
pub fn median_cut(pixels: &[Rgba], max_colors: usize) -> Vec<Rgba> {
    if max_colors == 0 {
        return Vec::new();
    }
    // Gather visible pixels (sampled if huge).
    let visible: Vec<[u8; 3]> = {
        let vis_iter = pixels.iter().filter(|p| p.a != 0);
        let count = vis_iter.clone().count();
        let stride = (count / SAMPLE_CAP).max(1);
        vis_iter.step_by(stride).map(|p| [p.r, p.g, p.b]).collect()
    };
    if visible.is_empty() {
        return Vec::new();
    }

    // Exact palettes need no splitting (and must not be averaged away:
    // a 4-color logo should quantize to exactly its 4 colors).
    let mut uniq = visible.clone();
    uniq.sort_unstable();
    uniq.dedup();
    if uniq.len() <= max_colors {
        return uniq
            .into_iter()
            .map(|c| Rgba::rgb(c[0], c[1], c[2]))
            .collect();
    }

    // Boxes are index ranges over one shared, re-sorted sample buffer:
    // splitting sorts a sub-range by the widest channel and cuts at the
    // median index. No per-box Vec churn.
    let mut samples = visible;
    let mut boxes: Vec<(usize, usize)> = vec![(0, samples.len())];
    while boxes.len() < max_colors {
        // Split the box with the widest channel range; ties broken by
        // population so hot regions get more entries.
        let mut best: Option<(usize, usize, u8)> = None; // (box idx, range, channel)
        for (bi, &(lo, hi)) in boxes.iter().enumerate() {
            let (range, ch) = widest_channel(&samples[lo..hi]);
            if range == 0 {
                continue; // uniform box cannot split
            }
            let better = match best {
                None => true,
                Some((pbi, prange, _)) => {
                    range > prange || (range == prange && (hi - lo) > (boxes[pbi].1 - boxes[pbi].0))
                }
            };
            if better {
                best = Some((bi, range, ch));
            }
        }
        let Some((bi, _, ch)) = best else {
            break; // every box uniform: palette is exact already
        };
        let (lo, hi) = boxes[bi];
        let seg = &mut samples[lo..hi];
        seg.sort_unstable_by_key(|c| c[ch as usize]);
        let mid = lo + seg.len() / 2;
        boxes[bi] = (lo, mid);
        boxes.push((mid, hi));
    }

    boxes
        .into_iter()
        .map(|(lo, hi)| {
            let n = (hi - lo) as u64;
            let mut sum = [0u64; 3];
            for c in &samples[lo..hi] {
                for ch in 0..3 {
                    sum[ch] += c[ch] as u64;
                }
            }
            Rgba::rgb(
                ((sum[0] + n / 2) / n) as u8,
                ((sum[1] + n / 2) / n) as u8,
                ((sum[2] + n / 2) / n) as u8,
            )
        })
        .collect()
}

/// (max range, channel index) over a pixel slice.
fn widest_channel(px: &[[u8; 3]]) -> (usize, u8) {
    let mut min = [255u8; 3];
    let mut max = [0u8; 3];
    for c in px {
        for ch in 0..3 {
            min[ch] = min[ch].min(c[ch]);
            max[ch] = max[ch].max(c[ch]);
        }
    }
    let mut best = (0usize, 0u8);
    for ch in 0..3 {
        let range = (max[ch] - min[ch]) as usize;
        if range > best.0 {
            best = (range, ch as u8);
        }
    }
    best
}

/// Nearest palette index by squared sRGB distance (shared metric with
/// the dither module).
pub fn nearest_index(palette: &[Rgba], c: Rgba) -> usize {
    let mut best = 0usize;
    let mut best_d = i64::MAX;
    for (i, p) in palette.iter().enumerate() {
        let dr = (c.r as i32 - p.r as i32) as i64;
        let dg = (c.g as i32 - p.g as i32) as i64;
        let db = (c.b as i32 - p.b as i32) as i64;
        let d = dr * dr + dg * dg + db * db;
        if d < best_d {
            best_d = d;
            best = i;
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_small_palettes_pass_through() {
        let colors = [
            Rgba::rgb(255, 0, 0),
            Rgba::rgb(0, 255, 0),
            Rgba::rgb(0, 0, 255),
        ];
        let pixels: Vec<Rgba> = (0..300).map(|i| colors[i % 3]).collect();
        let mut pal = median_cut(&pixels, 16);
        pal.sort_by_key(|c| (c.r, c.g, c.b));
        assert_eq!(pal.len(), 3);
        assert!(pal.contains(&Rgba::rgb(255, 0, 0)));
        assert!(pal.contains(&Rgba::rgb(0, 255, 0)));
        assert!(pal.contains(&Rgba::rgb(0, 0, 255)));
    }

    #[test]
    fn splits_reduce_error_on_gradient() {
        // A red->blue gradient quantized to 8 colors: every pixel must
        // land within a plausible distance of some palette entry.
        let pixels: Vec<Rgba> = (0..=255)
            .map(|i| Rgba::rgb(255 - i as u8, 0, i as u8))
            .collect();
        let pal = median_cut(&pixels, 8);
        assert_eq!(pal.len(), 8);
        for p in &pixels {
            let q = pal[nearest_index(&pal, *p)];
            let d = (p.r as i32 - q.r as i32)
                .abs()
                .max((p.b as i32 - q.b as i32).abs());
            // 256 values / 8 buckets = 32 per bucket; mean is ≤16 away
            // in the split channel, allow slack for rounding.
            assert!(d <= 24, "pixel {p:?} -> {q:?} (d={d})");
        }
    }

    #[test]
    fn transparent_pixels_ignored() {
        let mut pixels = vec![Rgba::new(0, 255, 0, 0); 100];
        pixels.push(Rgba::rgb(200, 10, 10));
        let pal = median_cut(&pixels, 4);
        assert_eq!(pal, vec![Rgba::rgb(200, 10, 10)]);
        assert!(median_cut(&[Rgba::TRANSPARENT; 10], 4).is_empty());
        assert!(median_cut(&pixels, 0).is_empty());
    }

    #[test]
    fn respects_max_colors() {
        // 256 unique grays into 16 colors.
        let pixels: Vec<Rgba> = (0..=255)
            .map(|i| Rgba::rgb(i as u8, i as u8, i as u8))
            .collect();
        let pal = median_cut(&pixels, 16);
        assert_eq!(pal.len(), 16);
        // Monotone coverage: first and last gray must be near an entry.
        for probe in [0u8, 255] {
            let q = pal[nearest_index(&pal, Rgba::rgb(probe, probe, probe))];
            assert!((q.r as i32 - probe as i32).abs() <= 16, "{probe} -> {q:?}");
        }
    }

    #[test]
    fn deterministic() {
        let pixels: Vec<Rgba> = (0..1000)
            .map(|i| {
                Rgba::rgb(
                    (i * 7 % 256) as u8,
                    (i * 13 % 256) as u8,
                    (i * 29 % 256) as u8,
                )
            })
            .collect();
        assert_eq!(median_cut(&pixels, 32), median_cut(&pixels, 32));
    }
}
