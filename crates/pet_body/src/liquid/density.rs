use super::{
    kernels::density_kernel,
    particles::{LiquidParticle, MAX_LIQUID_PARTICLES},
};

#[must_use]
pub fn calibrate_rest_density(
    particles: &[LiquidParticle; MAX_LIQUID_PARTICLES],
    count: usize,
    kernel_radius: f32,
) -> f32 {
    let count = count.min(MAX_LIQUID_PARTICLES);
    if count == 0 {
        return 0.01;
    }
    // A free surface is intentionally under-dense. Averaging it into rho0 made
    // the interior start 15% "compressed", so the very first PBF projection
    // blasted the calibrated pack apart. Use the well-supported interior's 90th
    // percentile instead; the unilateral constraint then leaves the initial
    // surface alone and activates only under genuine compression.
    let mut samples = [0.0_f32; MAX_LIQUID_PARTICLES];
    for (index, sample) in samples[..count].iter_mut().enumerate() {
        *sample = density_at(particles, count, index, kernel_radius, false);
    }
    samples[..count].sort_by(f32::total_cmp);
    let percentile_index = ((count - 1) as f32 * 0.90).round() as usize;
    samples[percentile_index].max(0.01)
}

pub fn update_density_and_surface(
    particles: &mut [LiquidParticle; MAX_LIQUID_PARTICLES],
    count: usize,
    kernel_radius: f32,
) {
    let snapshot = *particles;
    for index in 0..count {
        let mut neighbors = 0_u32;
        let mut density = 0.0;
        for other in 0..count {
            let delta = snapshot[index].predicted_position - snapshot[other].predicted_position;
            let weight = density_kernel(delta, kernel_radius);
            density += weight;
            if index != other && weight > 0.0 {
                neighbors += 1;
            }
        }
        particles[index].density = density;
        particles[index].surface_score = (1.0 - neighbors as f32 / 10.0).clamp(0.0, 1.0);
    }
}

#[must_use]
pub fn mean_density_error(
    particles: &[LiquidParticle; MAX_LIQUID_PARTICLES],
    count: usize,
    rest_density: f32,
) -> f32 {
    particles[..count]
        .iter()
        .map(|particle| (particle.density / rest_density.max(0.01) - 1.0).abs())
        .sum::<f32>()
        / count.max(1) as f32
}

fn density_at(
    particles: &[LiquidParticle; MAX_LIQUID_PARTICLES],
    count: usize,
    index: usize,
    kernel_radius: f32,
    predicted: bool,
) -> f32 {
    let position = if predicted {
        particles[index].predicted_position
    } else {
        particles[index].position
    };
    particles[..count]
        .iter()
        .map(|other| {
            let other_position = if predicted {
                other.predicted_position
            } else {
                other.position
            };
            density_kernel(position - other_position, kernel_radius)
        })
        .sum()
}
