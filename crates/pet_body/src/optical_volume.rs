//! Silhouette inflation, independent of the face and of particle optical slopes.
//!
//! Solve -Laplacian(u)=1, u=0 on the actual density isocontour; then z=2*sqrt(u).
//! For a disk u=(R^2-r^2)/4, so the cap is exactly a hemisphere. The square-root
//! cross section follows the Poisson inflation construction in Ink-and-Ray
//! (Sykora et al., TOG 2014), rather than interpolating a front-facing plate rim.
//! Cut-cell boundary distances retain subpixel silhouette motion. Neither this
//! optical approximation nor its height changes simulation geometry or alpha.

use crate::liquid_render::DensityInstance;

pub(crate) const OPTICAL_VOLUME_SIZE: usize = 64;
const MAX_SWEEPS: usize = 192;

pub(crate) struct OpticalVolumeGrid {
    /// Signed, unit, Y-up normals in XYZ; inflated world-unit height in W.
    pub texels: Vec<[f32; 4]>,
    /// World point -> texture UV: (point - bounds.xy) * bounds.zw.
    /// Row zero is MINIMUM world Y, including the half-texel sampling offset.
    pub bounds: [f32; 4],
}

#[derive(Clone, Copy)]
struct Kernel {
    center: [f32; 2],
    axis: [f32; 2],
    radii: [f32; 2],
    extent: [f32; 2],
    density: f32,
}

impl Kernel {
    fn from_instance(instance: &DensityInstance) -> Option<Self> {
        let a = instance.geometry_a;
        let b = instance.geometry_b;
        if !a.iter().chain(b.iter()).all(|v| v.is_finite())
            || a[2] <= 0.0
            || a[3] <= 0.0
            || b[2] <= 0.0
        {
            return None;
        }
        // Identical to liquid_density.wgsl::particle_vertex, including fallback.
        let axis_length_squared = b[0] * b[0] + b[1] * b[1];
        let axis = if axis_length_squared < 0.25 {
            [1.0, 0.0]
        } else {
            let inverse = axis_length_squared.sqrt().recip();
            [b[0] * inverse, b[1] * inverse]
        };
        Some(Self {
            center: [a[0], a[1]],
            axis,
            radii: [a[2], a[3]],
            density: b[2],
            extent: [
                ((axis[0] * a[2]).powi(2) + (axis[1] * a[3]).powi(2)).sqrt(),
                ((axis[1] * a[2]).powi(2) + (axis[0] * a[3]).powi(2)).sqrt(),
            ],
        })
    }

    fn at(self, x: f32, y: f32) -> f32 {
        let dx = x - self.center[0];
        let dy = y - self.center[1];
        let qx = (dx * self.axis[0] + dy * self.axis[1]) / self.radii[0];
        let qy = (-dx * self.axis[1] + dy * self.axis[0]) / self.radii[1];
        let support = (1.0 - qx * qx - qy * qy).max(0.0);
        self.density * support * support * support
    }
}

#[derive(Clone, Copy)]
struct Cell {
    index: usize,
    neighbors: [usize; 4],
    /// Distances to neighboring interior nodes or the interpolated boundary.
    distances: [f32; 4],
    weights: [f32; 4],
    rhs: f32,
    relaxation: f32,
}

impl OpticalVolumeGrid {
    pub fn reconstruct(instances: &[DensityInstance], iso: f32) -> Self {
        let kernels: Vec<_> = instances.iter().filter_map(Kernel::from_instance).collect();
        if kernels.is_empty() || !iso.is_finite() || iso <= 0.0 {
            return Self::empty();
        }
        let mut min = [f32::INFINITY; 2];
        let mut max = [f32::NEG_INFINITY; 2];
        for kernel in &kernels {
            for axis in 0..2 {
                min[axis] = min[axis].min(kernel.center[axis] - kernel.extent[axis]);
                max[axis] = max[axis].max(kernel.center[axis] + kernel.extent[axis]);
            }
        }
        let n = OPTICAL_VOLUME_SIZE;
        let h = [
            (max[0] - min[0]) / (n - 5) as f32,
            (max[1] - min[1]) / (n - 5) as f32,
        ];
        if !h.iter().all(|v| v.is_finite() && *v > 1.0e-8) {
            return Self::empty();
        }
        let start = [min[0] - h[0] * 2.0, min[1] - h[1] * 2.0];
        let bounds = [
            start[0] - h[0] * 0.5,
            start[1] - h[1] * 0.5,
            1.0 / (n as f32 * h[0]),
            1.0 / (n as f32 * h[1]),
        ];
        let mut field = vec![0.0; n * n];
        // Splat only each rotated ellipse's AABB, never particles x full grid.
        for kernel in kernels {
            let low = [
                ((kernel.center[0] - kernel.extent[0] - start[0]) / h[0])
                    .floor()
                    .max(0.0) as usize,
                ((kernel.center[1] - kernel.extent[1] - start[1]) / h[1])
                    .floor()
                    .max(0.0) as usize,
            ];
            let high = [
                (((kernel.center[0] + kernel.extent[0] - start[0]) / h[0]).ceil() as usize)
                    .min(n - 1),
                (((kernel.center[1] + kernel.extent[1] - start[1]) / h[1]).ceil() as usize)
                    .min(n - 1),
            ];
            for y in low[1]..=high[1] {
                for x in low[0]..=high[0] {
                    field[y * n + x] +=
                        kernel.at(start[0] + x as f32 * h[0], start[1] + y as f32 * h[1]);
                }
            }
        }
        let (texels, _, _) = inflate(&field, n, h, iso);
        Self { texels, bounds }
    }

    fn empty() -> Self {
        Self {
            texels: vec![[0.0, 0.0, 1.0, 0.0]; OPTICAL_VOLUME_SIZE * OPTICAL_VOLUME_SIZE],
            bounds: [-1.0, -1.0, 0.5, 0.5],
        }
    }
}

/// Shortley-Weller stencil: unequal edge distances impose u=0 at the continuous
/// isocontour, rather than at the center of the first exterior texel.
fn inflate(field: &[f32], n: usize, h: [f32; 2], iso: f32) -> (Vec<[f32; 4]>, Vec<f32>, f32) {
    let mut cells = Vec::with_capacity(n * n);
    for y in 1..n - 1 {
        for x in 1..n - 1 {
            let index = y * n + x;
            if field[index] <= iso {
                continue;
            }
            let neighbors = [index - 1, index + 1, index - n, index + n];
            let mut distances = [h[0], h[0], h[1], h[1]];
            for direction in 0..4 {
                if field[neighbors[direction]] <= iso {
                    let fraction =
                        (field[index] - iso) / (field[index] - field[neighbors[direction]]);
                    // A vanishing cut cell would unnecessarily amplify f32 roundoff.
                    distances[direction] *= fraction.clamp(0.02, 1.0);
                }
            }
            let mut weights = [0.0; 4];
            let mut diagonal = 0.0;
            for direction in 0..4 {
                let pair = direction ^ 1;
                let coefficient =
                    2.0 / (distances[direction] * (distances[direction] + distances[pair]));
                diagonal += coefficient;
                if field[neighbors[direction]] > iso {
                    weights[direction] = coefficient;
                }
            }
            let rhs = diagonal.recip();
            for weight in &mut weights {
                *weight *= rhs;
            }
            cells.push(Cell {
                index,
                neighbors,
                distances,
                weights,
                rhs,
                relaxation: if distances[0] < h[0]
                    || distances[1] < h[0]
                    || distances[2] < h[1]
                    || distances[3] < h[1]
                {
                    1.0
                } else {
                    1.82
                },
            });
        }
    }
    // Nested iteration supplies the long-wavelength volume on a cheap coarse
    // domain. Fine SOR only removes interpolation/boundary error, instead of
    // spending hundreds of sweeps propagating the rim all the way to the center.
    let mut u = if n >= 32 {
        let coarse_n = n.div_ceil(2);
        let ratio = (n - 1) as f32 / (coarse_n - 1) as f32;
        let mut coarse_field = vec![0.0; coarse_n * coarse_n];
        for y in 0..coarse_n {
            for x in 0..coarse_n {
                coarse_field[y * coarse_n + x] =
                    bilinear(field, n, x as f32 * ratio, y as f32 * ratio);
            }
        }
        let (_, coarse_u, _) = inflate(&coarse_field, coarse_n, [h[0] * ratio, h[1] * ratio], iso);
        let mut initial = vec![0.0; n * n];
        for cell in &cells {
            let x = cell.index % n;
            let y = cell.index / n;
            initial[cell.index] = bilinear(&coarse_u, coarse_n, x as f32 / ratio, y as f32 / ratio);
        }
        initial
    } else {
        vec![0.0; n * n]
    };
    // Fixed upper bound; lexicographic SOR is deterministic and all components
    // solve independently. No temporal filter introduces lag into deformation.
    let sweep_limit = if n >= 48 {
        80
    } else if n >= 32 {
        48
    } else {
        MAX_SWEEPS
    };
    for sweep in 0..sweep_limit {
        let mut max_change = 0.0_f32;
        let mut max_u = 0.0_f32;
        for cell in &cells {
            let target = cell.rhs
                + cell.weights[0] * u[cell.neighbors[0]]
                + cell.weights[1] * u[cell.neighbors[1]]
                + cell.weights[2] * u[cell.neighbors[2]]
                + cell.weights[3] * u[cell.neighbors[3]];
            let old = u[cell.index];
            let next = (old + cell.relaxation * (target - old)).max(0.0);
            u[cell.index] = next;
            max_change = max_change.max((next - old).abs());
            max_u = max_u.max(next);
        }
        if sweep >= 32 && max_change <= max_u * 2.0e-6 {
            break;
        }
    }
    let mut texels = vec![[0.0, 0.0, 1.0, 0.0]; n * n];
    // Extend boundary orientation into exterior texels so bilinear interpolation
    // tends toward the true grazing normal, not a front-facing outside padding.
    for y in 1..n - 1 {
        for x in 1..n - 1 {
            let i = y * n + x;
            if field[i] <= iso {
                let nx = (field[i - 1] - field[i + 1]) / (2.0 * h[0]);
                let ny = (field[i - n] - field[i + n]) / (2.0 * h[1]);
                let length = (nx * nx + ny * ny).sqrt();
                if length > 1.0e-12 {
                    texels[i] = [nx / length, ny / length, 0.0, 0.0];
                }
            }
        }
    }
    let mut residual = 0.0_f32;
    for cell in &cells {
        let i = cell.index;
        let mut neighbor_u = [0.0; 4];
        for (direction, value) in neighbor_u.iter_mut().enumerate() {
            if cell.weights[direction] > 0.0 {
                *value = u[cell.neighbors[direction]];
            }
        }
        let dx = derivative(
            u[i],
            neighbor_u[0],
            neighbor_u[1],
            cell.distances[0],
            cell.distances[1],
        );
        let dy = derivative(
            u[i],
            neighbor_u[2],
            neighbor_u[3],
            cell.distances[2],
            cell.distances[3],
        );
        let z = u[i].sqrt();
        let inverse_length = (dx * dx + dy * dy + z * z).max(1.0e-30).sqrt().recip();
        texels[i] = [
            -dx * inverse_length,
            -dy * inverse_length,
            z * inverse_length,
            z * 2.0,
        ];
        let target = cell.rhs
            + cell.weights[0] * neighbor_u[0]
            + cell.weights[1] * neighbor_u[1]
            + cell.weights[2] * neighbor_u[2]
            + cell.weights[3] * neighbor_u[3];
        residual = residual.max((u[i] - target).abs() / cell.rhs);
    }
    (texels, u, residual)
}

fn bilinear(field: &[f32], n: usize, x: f32, y: f32) -> f32 {
    let ix = (x as usize).min(n - 2);
    let iy = (y as usize).min(n - 2);
    let tx = (x - ix as f32).clamp(0.0, 1.0);
    let ty = (y - iy as f32).clamp(0.0, 1.0);
    let a = field[iy * n + ix] * (1.0 - tx) + field[iy * n + ix + 1] * tx;
    let b = field[(iy + 1) * n + ix] * (1.0 - tx) + field[(iy + 1) * n + ix + 1] * tx;
    a * (1.0 - ty) + b * ty
}

fn derivative(center: f32, left: f32, right: f32, dl: f32, dr: f32) -> f32 {
    (dl * dl * (right - center) + dr * dr * (center - left)) / (dl * dr * (dl + dr))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ellipse_field(n: usize, a: f32, b: f32) -> (Vec<f32>, [f32; 2]) {
        let h = [a * 2.4 / (n - 1) as f32, b * 2.4 / (n - 1) as f32];
        let mut field = Vec::with_capacity(n * n);
        for y in 0..n {
            for x in 0..n {
                let px = x as f32 * h[0] - a * 1.2;
                let py = y as f32 * h[1] - b * 1.2;
                field.push(1.0 - (px / a).powi(2) - (py / b).powi(2));
            }
        }
        (field, h)
    }

    #[test]
    fn disk_and_ellipse_match_analytic_caps_throughout_interior() {
        for (a, b) in [(0.4, 0.4), (0.52, 0.19)] {
            let n = OPTICAL_VOLUME_SIZE;
            let (field, h) = ellipse_field(n, a, b);
            let (normals, u, residual) = inflate(&field, n, h, 0.0);
            let c = 1.0 / (2.0 * (1.0 / (a * a) + 1.0 / (b * b)));
            let mut maximum_angle = 0.0_f32;
            let mut maximum_height_error = 0.0_f32;
            for y in 1..n - 1 {
                for x in 1..n - 1 {
                    let i = y * n + x;
                    if field[i] < 0.04 {
                        continue;
                    }
                    let px = x as f32 * h[0] - a * 1.2;
                    let py = y as f32 * h[1] - b * 1.2;
                    let expected_u = c * field[i];
                    let normal = glam::Vec3::new(
                        2.0 * c * px / (a * a),
                        2.0 * c * py / (b * b),
                        expected_u.sqrt(),
                    )
                    .normalize();
                    let actual =
                        glam::Vec3::from_array([normals[i][0], normals[i][1], normals[i][2]]);
                    maximum_angle =
                        maximum_angle.max(normal.dot(actual).clamp(-1.0, 1.0).acos().to_degrees());
                    maximum_height_error = maximum_height_error.max((u[i] - expected_u).abs() / c);
                }
            }
            eprintln!(
                "cap a={a} b={b}: angle={maximum_angle:.4}deg height_u_error={maximum_height_error:.6} residual={residual:.6}"
            );
            assert!(maximum_angle < 2.0);
            assert!(maximum_height_error < 0.003);
            assert!(residual < 0.005);
            // At half radius a hemisphere already turns by 30 degrees: the
            // desired surface is curved throughout, not flat with a beveled rim.
            let x = ((a * 1.2 + a * 0.5) / h[0]).round() as usize;
            let y = ((b * 1.2) / h[1]).round() as usize;
            assert!(normals[y * n + x][0] > if a == b { 0.45 } else { 0.15 });
        }
    }

    fn instance(x: f32, y: f32, a: f32, b: f32, axis: [f32; 2]) -> DensityInstance {
        DensityInstance {
            geometry_a: [x, y, a, b],
            geometry_b: [axis[0], axis[1], 1.0, 1.0],
            ..Default::default()
        }
    }

    #[test]
    fn exact_particle_kernel_and_rotated_extent_match_gpu_contract() {
        let kernel = Kernel::from_instance(&instance(0.2, -0.1, 0.3, 0.12, [0.6, 0.8])).unwrap();
        assert!((kernel.at(0.2, -0.1) - 1.0).abs() < 1.0e-6);
        assert!((kernel.at(0.2 + 0.6 * 0.15, -0.1 + 0.8 * 0.15) - 0.75_f32.powi(3)).abs() < 1.0e-6);
        assert_eq!(kernel.at(0.9, 0.9), 0.0);
        assert!((kernel.extent[0] - (0.18_f32.powi(2) + 0.096_f32.powi(2)).sqrt()).abs() < 1.0e-6);
    }

    #[test]
    fn asymmetric_resting_and_disconnected_caps_stay_finite_and_independent() {
        let grid = OpticalVolumeGrid::reconstruct(
            &[
                instance(-0.28, 0.02, 0.24, 0.12, [1.0, 0.0]),
                instance(-0.18, -0.03, 0.25, 0.10, [0.96, 0.28]),
                instance(0.35, 0.11, 0.08, 0.10, [0.6, 0.8]),
            ],
            0.20,
        );
        let mut cap_count = 0;
        for normal in &grid.texels {
            assert!(normal.iter().all(|v| v.is_finite()));
            let length =
                (normal[0] * normal[0] + normal[1] * normal[1] + normal[2] * normal[2]).sqrt();
            assert!((length - 1.0).abs() < 2.0e-5);
            assert!(normal[3] >= 0.0);
            if normal[3] > 0.01 {
                cap_count += 1;
            }
        }
        assert!(cap_count > 150);
        let n = OPTICAL_VOLUME_SIZE;
        let sample = |px: f32, py: f32| {
            let x = (((px - grid.bounds[0]) * grid.bounds[2] * n as f32) - 0.5).round() as usize;
            let y = (((py - grid.bounds[1]) * grid.bounds[3] * n as f32) - 0.5).round() as usize;
            grid.texels[y * n + x]
        };
        assert!(sample(-0.23, 0.0)[3] > sample(0.35, 0.11)[3]);
        assert_eq!(sample(0.08, 0.0)[3], 0.0);
    }

    #[test]
    fn empty_invalid_and_subpixel_translation_are_bounded() {
        let empty = OpticalVolumeGrid::reconstruct(&[], 0.2);
        assert!(empty.texels.iter().all(|p| *p == [0.0, 0.0, 1.0, 0.0]));
        let invalid =
            OpticalVolumeGrid::reconstruct(&[instance(f32::NAN, 0.0, 0.2, 0.2, [1.0, 0.0])], 0.2);
        assert_eq!(invalid.texels, empty.texels);
        let a = OpticalVolumeGrid::reconstruct(&[instance(0.0, 0.0, 0.3, 0.2, [1.0, 0.0])], 0.2);
        let b = OpticalVolumeGrid::reconstruct(
            &[instance(0.00037, -0.00019, 0.3, 0.2, [1.0, 0.0])],
            0.2,
        );
        let difference = a
            .texels
            .iter()
            .zip(&b.texels)
            .flat_map(|(a, b)| a.iter().zip(b).map(|(a, b)| (a - b).abs()))
            .fold(0.0_f32, f32::max);
        assert!(
            difference < 0.0001,
            "translation changed material normals by {difference}"
        );
    }

    fn sample(grid: &OpticalVolumeGrid, px: f32, py: f32) -> [f32; 4] {
        let n = OPTICAL_VOLUME_SIZE;
        let x = (px - grid.bounds[0]) * grid.bounds[2] * n as f32 - 0.5;
        let y = (py - grid.bounds[1]) * grid.bounds[3] * n as f32 - 0.5;
        let ix = (x.max(0.0) as usize).min(n - 2);
        let iy = (y.max(0.0) as usize).min(n - 2);
        let tx = (x - ix as f32).clamp(0.0, 1.0);
        let ty = (y - iy as f32).clamp(0.0, 1.0);
        let mut value = [0.0; 4];
        for (channel, result) in value.iter_mut().enumerate() {
            *result = grid.texels[iy * n + ix][channel] * (1.0 - tx) * (1.0 - ty)
                + grid.texels[iy * n + ix + 1][channel] * tx * (1.0 - ty)
                + grid.texels[(iy + 1) * n + ix][channel] * (1.0 - tx) * ty
                + grid.texels[(iy + 1) * n + ix + 1][channel] * tx * ty;
        }
        value
    }

    #[test]
    fn packed_rotated_particle_reconstructs_the_analytic_elliptical_cap() {
        let iso = 0.2_f32;
        let scale = (1.0 - iso.cbrt()).sqrt();
        let a = 0.30 * scale;
        let b = 0.20 * scale;
        let c = 1.0 / (2.0 * (1.0 / (a * a) + 1.0 / (b * b)));
        let grid =
            OpticalVolumeGrid::reconstruct(&[instance(0.0, 0.0, 0.30, 0.20, [0.6, 0.8])], iso);
        let mut max_angle = 0.0_f32;
        for y in -5..=5 {
            for x in -5..=5 {
                let px = a * x as f32 / 7.0;
                let py = b * y as f32 / 7.0;
                let q = 1.0 - (px / a).powi(2) - (py / b).powi(2);
                if q < 0.2 {
                    continue;
                }
                let actual = sample(&grid, px * 0.6 - py * 0.8, px * 0.8 + py * 0.6);
                let gx = 2.0 * c * px / (a * a);
                let gy = 2.0 * c * py / (b * b);
                let expected =
                    glam::Vec3::new(gx * 0.6 - gy * 0.8, gx * 0.8 + gy * 0.6, (c * q).sqrt())
                        .normalize();
                let observed = glam::Vec3::new(actual[0], actual[1], actual[2]).normalize();
                max_angle =
                    max_angle.max(expected.dot(observed).clamp(-1.0, 1.0).acos().to_degrees());
            }
        }
        eprintln!("packed rotated compact-kernel cap max normal error={max_angle:.4}deg");
        assert!(max_angle < 1.0);
    }

    #[test]
    fn internal_subpixel_deformation_does_not_step_the_interior_normals() {
        let reconstruct = |step: usize| {
            OpticalVolumeGrid::reconstruct(
                &[
                    instance(-0.09, 0.015, 0.24, 0.17, [1.0, 0.0]),
                    instance(0.07 + step as f32 * 0.00015, -0.02, 0.22, 0.14, [0.98, 0.2]),
                ],
                0.34,
            )
        };
        let mut previous = reconstruct(0);
        let mut maximum_angle = 0.0_f32;
        for step in 1..=40 {
            let current = reconstruct(step);
            for y in -4..=4 {
                for x in -6..=6 {
                    let px = x as f32 * 0.021;
                    let py = y as f32 * 0.018;
                    let a = sample(&previous, px, py);
                    let b = sample(&current, px, py);
                    if a[3] < 0.04 || b[3] < 0.04 {
                        continue;
                    }
                    let a = glam::Vec3::new(a[0], a[1], a[2]).normalize();
                    let b = glam::Vec3::new(b[0], b[1], b[2]).normalize();
                    maximum_angle =
                        maximum_angle.max(a.dot(b).clamp(-1.0, 1.0).acos().to_degrees());
                }
            }
            previous = current;
        }
        eprintln!(
            "40 internal 0.00015-world-unit steps: max interior normal change={maximum_angle:.4}deg/frame"
        );
        assert!(maximum_angle < 0.5);
    }

    #[test]
    #[ignore = "release-only CPU latency benchmark"]
    fn benchmark_96_particle_body() {
        let mut instances = Vec::new();
        for y in 0..8 {
            for x in 0..12 {
                let px = (x as f32 - 5.5) * 0.035;
                let py = (y as f32 - 3.5) * 0.047;
                instances.push(instance(px, py, 0.09, 0.085, [1.0, 0.0]));
            }
        }
        for _ in 0..10 {
            std::hint::black_box(OpticalVolumeGrid::reconstruct(&instances, 0.7));
        }
        let mut elapsed = Vec::new();
        for _ in 0..100 {
            let start = std::time::Instant::now();
            std::hint::black_box(OpticalVolumeGrid::reconstruct(&instances, 0.7));
            elapsed.push(start.elapsed().as_secs_f64() * 1000.0);
        }
        elapsed.sort_by(f64::total_cmp);
        eprintln!(
            "96 particles, {OPTICAL_VOLUME_SIZE}x{OPTICAL_VOLUME_SIZE} CPU: p50={:.3}ms p95={:.3}ms max={:.3}ms",
            elapsed[50], elapsed[95], elapsed[99]
        );
    }
}
