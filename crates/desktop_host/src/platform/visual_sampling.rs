use glam::Vec2;

use crate::{
    DesktopVisualFrame, DesktopVisualSample, VISUAL_GRID_CELLS, VISUAL_GRID_HEIGHT,
    VISUAL_GRID_WIDTH, VisualCell,
};

pub(super) const CAPTURE_SAMPLES_PER_AXIS: usize = 4;
pub(super) const CAPTURE_WIDTH: usize = VISUAL_GRID_WIDTH * CAPTURE_SAMPLES_PER_AXIS;
pub(super) const CAPTURE_HEIGHT: usize = VISUAL_GRID_HEIGHT * CAPTURE_SAMPLES_PER_AXIS;

/// Turns the same small, tightly packed BGRA image into Pet's platform-neutral
/// visual feature grid. Native backends only own capture and coordinate APIs.
pub(super) fn frame_from_bgra(
    pixels: &[u8],
    pet_position: Vec2,
    previous: &mut Vec<[f32; 3]>,
    sequence: u64,
    timestamp: f64,
) -> Option<DesktopVisualFrame> {
    let (mut cells, mut means) = sample_visual_cells(pixels, previous)?;
    // The Windows capture can include the topmost organism. The Mac capture is
    // taken below its overlay, but applying the same mask keeps the perception
    // contract identical and protects both backends from self-attention lock.
    let self_mask = visual_self_mask(pet_position);
    let neutral_rgb = mean_unmasked_rgb(&means, &self_mask);
    for (index, masked) in self_mask.iter().copied().enumerate() {
        if !masked {
            continue;
        }
        means[index] = neutral_rgb;
        cells[index].luminance = luminance(neutral_rgb);
        cells[index].contrast = 0.0;
        cells[index].colorfulness = 0.0;
        cells[index].motion = 0.0;
        cells[index].edge_density = 0.0;
        cells[index].sudden_change = 0.0;
    }
    let unmasked_means = means
        .iter()
        .copied()
        .enumerate()
        .filter_map(|(index, sample)| (!self_mask[index]).then_some(sample))
        .collect::<Vec<_>>();
    let (mean_luminance, contrast, colorfulness, warmth, dominant_hue, edge_density) =
        summarize_samples(&unmasked_means);
    let motion_energy = cells.iter().map(|cell| cell.motion).sum::<f32>() / cells.len() as f32;
    let sudden_change = cells
        .iter()
        .map(|cell| cell.sudden_change)
        .fold(0.0_f32, f32::max);
    let column = (pet_position.x.clamp(0.0, 0.999_999) * VISUAL_GRID_WIDTH as f32) as usize;
    let row = (pet_position.y.clamp(0.0, 0.999_999) * VISUAL_GRID_HEIGHT as f32) as usize;
    let summary = DesktopVisualSample {
        mean_luminance,
        local_luminance: cells[row * VISUAL_GRID_WIDTH + column].luminance,
        contrast,
        colorfulness,
        warmth,
        dominant_hue,
        motion_energy,
        edge_density,
        sudden_change,
    };
    *previous = means;
    let frame = DesktopVisualFrame {
        summary,
        cells,
        sequence,
        timestamp,
    };
    frame.is_finite().then_some(frame)
}

#[cfg(target_os = "windows")]
pub(super) fn background_luminance_stats(
    bgra8: &[u8],
    width: u32,
    height: u32,
    bytes_per_row: u32,
) -> (f32, f32) {
    let stride = (width.min(height) / 24).max(1) as usize;
    let mut sum = 0.0_f32;
    let mut sum_squared = 0.0_f32;
    let mut count = 0.0_f32;
    for y in (0..height as usize).step_by(stride) {
        for x in (0..width as usize).step_by(stride) {
            let index = y * bytes_per_row as usize + x * 4;
            let blue = bgra8[index] as f32 / 255.0;
            let green = bgra8[index + 1] as f32 / 255.0;
            let red = bgra8[index + 2] as f32 / 255.0;
            let value = red * 0.2126 + green * 0.7152 + blue * 0.0722;
            sum += value;
            sum_squared += value * value;
            count += 1.0;
        }
    }
    let mean = sum / count.max(1.0);
    let contrast = (sum_squared / count.max(1.0) - mean * mean).max(0.0).sqrt();
    (mean, contrast)
}

fn sample_visual_cells(
    pixels: &[u8],
    previous: &[[f32; 3]],
) -> Option<([VisualCell; VISUAL_GRID_CELLS], Vec<[f32; 3]>)> {
    if pixels.len() < CAPTURE_WIDTH * CAPTURE_HEIGHT * 4 {
        return None;
    }
    let mut cells = [VisualCell::default(); VISUAL_GRID_CELLS];
    let mut means = Vec::with_capacity(VISUAL_GRID_CELLS);
    for row in 0..VISUAL_GRID_HEIGHT {
        for column in 0..VISUAL_GRID_WIDTH {
            let mut samples = [[0.0_f32; 3]; 16];
            let mut count = 0_usize;
            for sample_row in 0..CAPTURE_SAMPLES_PER_AXIS {
                for sample_column in 0..CAPTURE_SAMPLES_PER_AXIS {
                    let pixel_x = column * CAPTURE_SAMPLES_PER_AXIS + sample_column;
                    let pixel_y = row * CAPTURE_SAMPLES_PER_AXIS + sample_row;
                    let pixel = (pixel_y * CAPTURE_WIDTH + pixel_x) * 4;
                    samples[count] = [
                        pixels[pixel + 2] as f32 / 255.0,
                        pixels[pixel + 1] as f32 / 255.0,
                        pixels[pixel] as f32 / 255.0,
                    ];
                    count += 1;
                }
            }
            let valid_samples = &samples[..count];
            let mut mean_rgb = [0.0_f32; 3];
            for sample in valid_samples {
                for channel in 0..3 {
                    mean_rgb[channel] += sample[channel] / count as f32;
                }
            }
            let (luma, contrast, colorfulness, warmth, hue, fallback_edge_density) =
                summarize_samples(valid_samples);
            let edge_density = if count == 16 {
                grid_edge_density_4x4(valid_samples)
            } else {
                fallback_edge_density
            };
            let index = row * VISUAL_GRID_WIDTH + column;
            let motion = previous.get(index).map_or(0.0, |old| {
                (((mean_rgb[0] - old[0]).abs()
                    + (mean_rgb[1] - old[1]).abs()
                    + (mean_rgb[2] - old[2]).abs())
                    / 3.0
                    * 2.6)
                    .clamp(0.0, 1.0)
            });
            let previous_luma = previous.get(index).map_or(luma, |old| luminance(*old));
            cells[index] = VisualCell {
                luminance: luma,
                contrast,
                colorfulness,
                warmth,
                hue,
                motion,
                edge_density,
                sudden_change: ((luma - previous_luma).abs() * 1.8 + motion * 1.25).clamp(0.0, 1.0),
            };
            means.push(mean_rgb);
        }
    }
    Some((cells, means))
}

fn visual_self_mask(pet_position: Vec2) -> [bool; VISUAL_GRID_CELLS] {
    let column = (pet_position.x.clamp(0.0, 0.999_999) * VISUAL_GRID_WIDTH as f32) as isize;
    let row = (pet_position.y.clamp(0.0, 0.999_999) * VISUAL_GRID_HEIGHT as f32) as isize;
    std::array::from_fn(|index| {
        let candidate_row = (index / VISUAL_GRID_WIDTH) as isize;
        let candidate_column = (index % VISUAL_GRID_WIDTH) as isize;
        (candidate_row - row).abs() <= 1 && (candidate_column - column).abs() <= 1
    })
}

fn mean_unmasked_rgb(means: &[[f32; 3]], mask: &[bool; VISUAL_GRID_CELLS]) -> [f32; 3] {
    let mut sum = [0.0_f32; 3];
    let mut count = 0_u32;
    for (index, sample) in means.iter().copied().enumerate() {
        if mask.get(index).copied().unwrap_or(true) {
            continue;
        }
        for channel in 0..3 {
            sum[channel] += sample[channel];
        }
        count += 1;
    }
    if count == 0 {
        return [0.5; 3];
    }
    for channel in &mut sum {
        *channel /= count as f32;
    }
    sum
}

fn grid_edge_density_4x4(samples: &[[f32; 3]]) -> f32 {
    if samples.len() != 16 {
        return 0.0;
    }
    let luminances = samples.iter().copied().map(luminance).collect::<Vec<_>>();
    let mut gradient = 0.0_f32;
    let mut pairs = 0_u32;
    for row in 0..4 {
        for column in 0..4 {
            let index = row * 4 + column;
            if column + 1 < 4 {
                gradient += (luminances[index + 1] - luminances[index]).abs();
                pairs += 1;
            }
            if row + 1 < 4 {
                gradient += (luminances[index + 4] - luminances[index]).abs();
                pairs += 1;
            }
        }
    }
    (gradient / pairs.max(1) as f32 * 3.2).clamp(0.0, 1.0)
}

fn summarize_samples(samples: &[[f32; 3]]) -> (f32, f32, f32, f32, f32, f32) {
    if samples.is_empty() {
        return (0.5, 0.0, 0.0, 0.5, 0.0, 0.0);
    }
    let luminances = samples.iter().copied().map(luminance).collect::<Vec<_>>();
    let mean = luminances.iter().sum::<f32>() / luminances.len() as f32;
    let contrast = (luminances
        .iter()
        .map(|value| (*value - mean).powi(2))
        .sum::<f32>()
        / luminances.len() as f32)
        .sqrt()
        .mul_add(2.2, 0.0)
        .clamp(0.0, 1.0);
    let colorfulness = samples
        .iter()
        .map(|sample| {
            sample.iter().copied().fold(f32::NEG_INFINITY, f32::max)
                - sample.iter().copied().fold(f32::INFINITY, f32::min)
        })
        .sum::<f32>()
        / samples.len() as f32;
    let warmth = (0.5
        + samples
            .iter()
            .map(|sample| sample[0] - sample[2])
            .sum::<f32>()
            / samples.len() as f32
            * 0.5)
        .clamp(0.0, 1.0);
    let mut hue_vector = Vec2::ZERO;
    for sample in samples {
        let (hue, saturation) = hue_and_saturation(*sample);
        let angle = hue * std::f32::consts::TAU;
        hue_vector += Vec2::new(angle.cos(), angle.sin()) * saturation;
    }
    let dominant_hue = if hue_vector.length_squared() > 1.0e-6 {
        hue_vector
            .y
            .atan2(hue_vector.x)
            .rem_euclid(std::f32::consts::TAU)
            / std::f32::consts::TAU
    } else {
        0.0
    };
    let edge_density = if luminances.len() < 2 {
        0.0
    } else {
        (luminances
            .windows(2)
            .map(|pair| (pair[1] - pair[0]).abs())
            .sum::<f32>()
            / (luminances.len() - 1) as f32
            * 3.2)
            .clamp(0.0, 1.0)
    };
    (
        mean.clamp(0.0, 1.0),
        contrast,
        colorfulness.clamp(0.0, 1.0),
        warmth,
        dominant_hue,
        edge_density,
    )
}

fn luminance(sample: [f32; 3]) -> f32 {
    sample[0] * 0.2126 + sample[1] * 0.7152 + sample[2] * 0.0722
}

fn hue_and_saturation(sample: [f32; 3]) -> (f32, f32) {
    let maximum = sample.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let minimum = sample.iter().copied().fold(f32::INFINITY, f32::min);
    let delta = maximum - minimum;
    if delta <= 1.0e-6 || maximum <= 1.0e-6 {
        return (0.0, 0.0);
    }
    let hue = if maximum == sample[0] {
        ((sample[1] - sample[2]) / delta).rem_euclid(6.0)
    } else if maximum == sample[1] {
        (sample[2] - sample[0]) / delta + 2.0
    } else {
        (sample[0] - sample[1]) / delta + 4.0
    } / 6.0;
    (hue.rem_euclid(1.0), (delta / maximum).clamp(0.0, 1.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn self_mask_covers_the_body_cell_and_its_immediate_neighbors() {
        let centered = visual_self_mask(Vec2::splat(0.5));
        assert_eq!(centered.into_iter().filter(|masked| *masked).count(), 9);
        let corner = visual_self_mask(Vec2::ZERO);
        assert_eq!(corner.into_iter().filter(|masked| *masked).count(), 4);
    }

    #[test]
    fn four_by_four_sampling_distinguishes_structure_from_flat_color() {
        let flat = [[0.5_f32; 3]; 16];
        assert_eq!(grid_edge_density_4x4(&flat), 0.0);
        let checker: [[f32; 3]; 16] = std::array::from_fn(|index| {
            let row = index / 4;
            let column = index % 4;
            if (row + column) % 2 == 0 {
                [0.0; 3]
            } else {
                [1.0; 3]
            }
        });
        assert!(grid_edge_density_4x4(&checker) > 0.95);
    }
}
