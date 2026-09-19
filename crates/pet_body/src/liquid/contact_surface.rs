//! Contact contour of the shader's additive compact density, not its zero-density
//! splat hull. Body-local units; no glow, face, pigment or component selection.
//! Bounded ray reconstruction: this is a numerical contour approximation. The
//! conservative kernel hull remains the caller's fallback if no interior is found.
use crate::ParticleRenderState;
use glam::Vec2;

#[derive(Clone, Copy)]
struct Kernel {
    center: Vec2,
    axis: Vec2,
    inv_radii: Vec2,
    radii: Vec2,
    density: f32,
    support_radius: f32,
}

fn kernels(particles: &[ParticleRenderState]) -> Vec<Kernel> {
    particles
        .iter()
        .filter(|p| p.position.is_finite() && p.density.is_finite() && p.density > 0.0)
        .map(|p| Kernel {
            center: p.position,
            axis: if !p.axis_major.is_finite() || p.axis_major.length_squared() < 0.25 {
                Vec2::X
            } else {
                p.axis_major.normalize()
            },
            radii: Vec2::new(p.major_radius, p.minor_radius)
                .clamp(Vec2::splat(0.002), Vec2::splat(0.40)),
            inv_radii: Vec2::new(p.major_radius, p.minor_radius)
                .clamp(Vec2::splat(0.002), Vec2::splat(0.40))
                .recip(),
            density: p.density.clamp(0.0, 2.0),
            support_radius: p.major_radius.max(p.minor_radius).clamp(0.002, 0.40),
        })
        .collect()
}

fn raw_density(kernels: &[Kernel], point: Vec2) -> f32 {
    let mut density = 0.0;
    for k in kernels {
        let delta = point - k.center;
        // Most samples lie on an outer contour. Reject distant zero-support
        // kernels before doing their rotated quadratic; this is exact, not LOD.
        if delta.x.abs() >= k.support_radius || delta.y.abs() >= k.support_radius {
            continue;
        }
        let q = Vec2::new(delta.dot(k.axis), delta.dot(k.axis.perp())) * k.inv_radii;
        let w = (1.0 - q.length_squared()).max(0.0);
        density += k.density * w * w * w;
    }
    density
}

/// Presentation-only critically damped translation, advanced once per redraw.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct SmoothFaceOrigin {
    pub origin: Option<Vec2>,
    pub velocity: Vec2,
}

impl SmoothFaceOrigin {
    pub fn advance(&mut self, target: Vec2, dt: f32) -> Vec2 {
        let Some(previous) = self.origin else {
            self.origin = Some(target);
            return target;
        };
        let dt = if dt.is_finite() {
            dt.clamp(0.0, 0.05)
        } else {
            0.0
        };
        // Exact critically damped spring: no overshoot, no frame-rate lerp.
        let omega = 9.0;
        let delta = previous - target;
        let transient = self.velocity + delta * omega;
        let decay = (-omega * dt).exp();
        let next = target + (delta + transient * dt) * decay;
        self.velocity = (self.velocity - transient * (omega * dt)) * decay;
        let next = previous + (next - previous).clamp_length_max(0.65 * dt);
        self.origin = Some(next);
        next
    }
}

/// Keep the complete face footprint in the dense main liquid component.
/// Desired feeding translation is projected into that interior, not onto its rim.
pub fn contain_face_origin(
    particles: &[ParticleRenderState],
    iso: f32,
    origin: Vec2,
    desired: Vec2,
    axis_x: Vec2,
    axis_y: Vec2,
    scale: Vec2,
) -> Vec2 {
    let mut counts = [0usize; 256];
    for p in particles {
        counts[usize::from(p.component_id)] += 1;
    }
    let largest = counts
        .iter()
        .enumerate()
        .max_by_key(|(_, count)| **count)
        .map_or(0, |(id, _)| id);
    let main: Vec<_> = particles
        .iter()
        .copied()
        .filter(|p| usize::from(p.component_id) == largest)
        .collect();
    let ks = kernels(&main);
    if ks.is_empty() {
        return origin;
    }
    let offsets = [
        Vec2::ZERO,
        Vec2::new(-0.24, 0.09),
        Vec2::new(0.24, 0.09),
        Vec2::new(-0.24, -0.04),
        Vec2::new(0.24, -0.04),
        Vec2::new(-0.10, -0.19),
        Vec2::new(0.10, -0.19),
        Vec2::new(0.0, 0.13),
    ];
    let clearance = |p: Vec2| {
        offsets
            .iter()
            .map(|o| raw_density(&ks, p + axis_x * o.x * scale.x + axis_y * o.y * scale.y))
            .fold(f32::INFINITY, f32::min)
    };
    let threshold = iso.max(0.01) * 1.20;
    if clearance(desired) >= threshold {
        return desired;
    }
    let mut anchor = origin;
    if clearance(anchor) < threshold {
        // The old region disappeared: find the broadest remaining volume.
        // Prefer the nearest valid interior, not a distant density maximum.
        if let Some(nearest) = main
            .iter()
            .step_by(3)
            .map(|p| p.position)
            .filter(|&p| clearance(p) >= threshold)
            .min_by(|a, b| {
                a.distance_squared(origin)
                    .total_cmp(&b.distance_squared(origin))
            })
        {
            anchor = nearest;
        }
        let mut best = clearance(anchor);
        for particle in main.iter().step_by(3) {
            if best >= threshold {
                break;
            }
            let candidate = particle.position;
            let c = clearance(candidate);
            if c > best {
                best = c;
                anchor = candidate;
            }
        }
    }
    let mut safe = anchor;
    // Walk outward from the valid volume, stopping before an unsupported gap.
    for step in 1..=16 {
        let candidate = anchor.lerp(desired, step as f32 / 16.0);
        if clearance(candidate) < threshold {
            // Refine the first boundary instead of snapping between 1/16 steps.
            let mut outside = candidate;
            for _ in 0..10 {
                let middle = safe.lerp(outside, 0.5);
                if clearance(middle) >= threshold {
                    safe = middle;
                } else {
                    outside = middle;
                }
            }
            break;
        }
        safe = candidate;
    }
    safe
}

fn density(kernels: &[Kernel], point: Vec2, filter: Vec2) -> f32 {
    if filter == Vec2::ZERO {
        return raw_density(kernels, point);
    }
    raw_density(kernels, point) * 0.46
        + 0.135
            * (raw_density(kernels, point + Vec2::new(filter.x, 0.0))
                + raw_density(kernels, point - Vec2::new(filter.x, 0.0))
                + raw_density(kernels, point + Vec2::new(0.0, filter.y))
                + raw_density(kernels, point - Vec2::new(0.0, filter.y)))
}

fn exit(
    kernels: &[Kernel],
    seed: Vec2,
    direction: Vec2,
    outer: f32,
    iso: f32,
    filter: Vec2,
) -> Vec2 {
    let mut inside = seed.dot(direction);
    let mut outside = outer;
    let tangent = direction.perp();
    let t = seed.dot(tangent);
    for _ in 0..14 {
        let middle = (inside + outside) * 0.5;
        if density(kernels, direction * middle + tangent * t, filter) >= iso {
            inside = middle;
        } else {
            outside = middle;
        }
    }
    direction * inside + tangent * t
}

/// Tests a finite support strip against the same raw density iso-contour.
/// Two distinct tangent samples must bracket the surface within `tolerance`.
/// This is a cheap local query, not a full contour reconstruction per physics tick.
pub(super) fn surface_patch_contacts_plane(
    particles: &[ParticleRenderState],
    iso: f32,
    anchor: Vec2,
    normal: Vec2,
    tangent: Vec2,
    tolerance: f32,
) -> bool {
    let ks = kernels(particles);
    if ks.is_empty() || !iso.is_finite() || iso <= 0.0 || !anchor.is_finite() {
        return false;
    }
    let mut contacts = 0;
    for sample in -30..=30 {
        let point = anchor + tangent * (sample as f32 * 0.02);
        let inside = raw_density(&ks, point + normal * tolerance);
        let outside = raw_density(&ks, point - normal * tolerance);
        if inside >= iso && outside < iso {
            contacts += 1;
            if contacts >= 2 {
                return true;
            }
        }
    }
    false
}

/// Approximates the 50%-coverage iso contour in body-local units. `filter_offset`
/// is zero for raw density, or the shader cross-filter radius in body-local units.
/// Includes every supplied real particle irrespective of component/face ownership.
/// Returns None for invalid/no measurable liquid; caller retains conservative hull.
pub fn contact_surface_bounds(
    particles: &[ParticleRenderState],
    iso: f32,
    filter_offset: Vec2,
) -> Option<(Vec2, Vec2)> {
    if !iso.is_finite() || iso <= 0.0 || !filter_offset.is_finite() {
        return None;
    }
    let kernels = kernels(particles);
    if kernels.is_empty() || kernels.iter().any(|k| !k.radii.is_finite()) {
        return None;
    }
    let filter = filter_offset.abs();
    let mut extents = [0.0; 4];
    for (index, direction) in [Vec2::X, -Vec2::X, Vec2::Y, -Vec2::Y]
        .into_iter()
        .enumerate()
    {
        extents[index] = directional_extent(&kernels, iso, filter, direction)?;
    }
    Some((
        Vec2::new(-extents[1], -extents[3]),
        Vec2::new(extents[0], extents[2]),
    ))
}

/// Fast bottom-only query sharing the full oracle's reconstruction algorithm.
pub fn contact_surface_min_y(
    particles: &[ParticleRenderState],
    iso: f32,
    filter_offset: Vec2,
) -> Option<f32> {
    if !iso.is_finite() || iso <= 0.0 || !filter_offset.is_finite() {
        return None;
    }
    let kernels = kernels(particles);
    if kernels.is_empty() || kernels.iter().any(|k| !k.radii.is_finite()) {
        return None;
    }
    directional_extent(&kernels, iso, filter_offset.abs(), Vec2::NEG_Y).map(|v| -v)
}

fn directional_extent(kernels: &[Kernel], iso: f32, filter: Vec2, direction: Vec2) -> Option<f32> {
    directional_point(kernels, iso, filter, direction).map(|p| p.dot(direction))
}

/// Actual additive-density support, excluding the invisible kernel skirt.
pub fn contact_surface_support(
    particles: &[ParticleRenderState],
    iso: f32,
    direction: Vec2,
) -> Option<Vec2> {
    if !iso.is_finite() || iso <= 0.0 || !direction.is_finite() {
        return None;
    }
    let ks = kernels(particles);
    if ks.is_empty() {
        return None;
    }
    directional_point(&ks, iso, Vec2::ZERO, direction.normalize_or(Vec2::X))
}

/// Circle versus the visible density contour. Candidate rays start inside the
/// gel, never at a splat's outer radius. Sweeps are bounded to 32 intervals.
pub fn contact_surface_circle(
    particles: &[ParticleRenderState],
    iso: f32,
    previous: Vec2,
    center: Vec2,
    radius: f32,
) -> Option<(Vec2, Vec2, f32, bool)> {
    if !iso.is_finite()
        || iso <= 0.0
        || !previous.is_finite()
        || !center.is_finite()
        || !radius.is_finite()
        || radius <= 0.0
    {
        return None;
    }
    let ks = kernels(particles);
    let steps = ((center - previous).length() / (radius * 0.5))
        .ceil()
        .clamp(1.0, 32.0) as usize;
    // Current overlap wins; previous points are only a tunnelling fallback.
    for index in (0..=steps).rev() {
        let c = previous.lerp(center, index as f32 / steps as f32);
        let inside = raw_density(&ks, c) >= iso;
        let mut best: Option<(Vec2, f32)> = None;
        for k in &ks {
            if !inside && (c - k.center).length() > k.support_radius + radius {
                continue;
            }
            if raw_density(&ks, k.center) < iso {
                continue;
            }
            let direction = (c - k.center).normalize_or(Vec2::X);
            let outer = ks
                .iter()
                .map(|p| p.center.dot(direction) + p.support_radius)
                .fold(f32::NEG_INFINITY, f32::max);
            let p = exit(&ks, k.center, direction, outer, iso, Vec2::ZERO);
            let distance = p.distance(c);
            if best.is_none_or(|(_, d)| distance < d) {
                best = Some((p, distance));
            }
        }
        if let Some((point, distance)) = best
            && (inside || distance <= radius)
        {
            let normal = if inside {
                (point - c).normalize_or(Vec2::X)
            } else {
                (c - point).normalize_or(Vec2::X)
            };
            return Some((
                point,
                normal,
                if inside {
                    radius + distance
                } else {
                    radius - distance
                },
                index < steps,
            ));
        }
    }
    None
}

fn directional_point(kernels: &[Kernel], iso: f32, filter: Vec2, direction: Vec2) -> Option<Vec2> {
    let mut candidates: Vec<_> = kernels
        .iter()
        .map(|k| {
            let a = k.axis.dot(direction) * k.radii.x;
            let b = k.axis.perp().dot(direction) * k.radii.y;
            let radius = (a * a + b * b).sqrt();
            (k.center.dot(direction) + radius, k, radius)
        })
        .collect();
    candidates.sort_by(|a, b| b.0.total_cmp(&a.0));
    let outer = candidates[0].0 + filter.dot(direction.abs()) + 0.00001;
    let mut best: Option<Vec2> = None;
    for (bound, k, radius) in candidates {
        if best.is_some_and(|p| bound + filter.dot(direction.abs()) <= p.dot(direction)) {
            continue;
        }
        // Exact isolated-ellipse support point is a valid interior seed of
        // the summed field; its tangent is also exact for rotated ellipses.
        let isolated = (1.0 - (iso / k.density).cbrt()).max(0.0).sqrt();
        let q_dir = k.axis * (k.axis.dot(direction) * k.radii.x.powi(2))
            + k.axis.perp() * (k.axis.perp().dot(direction) * k.radii.y.powi(2));
        let edge_seed = k.center + q_dir * (isolated * 0.999 / radius.max(0.00001));
        for seed in [edge_seed, k.center] {
            if density(kernels, seed, filter) < iso {
                continue;
            }
            let p = exit(kernels, seed, direction, outer, iso, filter);
            if best.is_none_or(|old| p.dot(direction) > old.dot(direction)) {
                best = Some(p);
            }
        }
    }
    let mut best = best?;
    // Local tangential contour maximization corrects the overlapping-cluster
    // extremum away from any single particle's analytic support direction.
    let mut step = kernels
        .iter()
        .map(|k| k.radii.min_element())
        .fold(0.0, f32::max)
        * 0.5;
    for _ in 0..9 {
        for sign in [-1.0, 1.0] {
            let seed = best + direction.perp() * (step * sign) - direction * step;
            if density(kernels, seed, filter) < iso {
                continue;
            }
            let candidate = exit(kernels, seed, direction, outer, iso, filter);
            if candidate.dot(direction) > best.dot(direction) {
                best = candidate;
            }
        }
        step *= 0.5;
    }
    Some(best)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn particle(position: Vec2, axis: Vec2, radii: Vec2) -> ParticleRenderState {
        ParticleRenderState {
            position,
            axis_major: axis,
            major_radius: radii.x,
            minor_radius: radii.y,
            density: 1.0,
            ..ParticleRenderState::default()
        }
    }
    #[test]
    fn face_target_jump_is_continuous_and_settles_without_overshoot() {
        for hz in [30, 60, 120] {
            let mut tracker = SmoothFaceOrigin {
                origin: Some(Vec2::ZERO),
                ..Default::default()
            };
            let target = Vec2::new(0.2, -0.1);
            let mut previous = Vec2::ZERO;
            for _ in 0..hz * 2 {
                let next = tracker.advance(target, 1.0 / hz as f32);
                assert!(next.distance(previous) <= 0.65 / hz as f32 + 1e-6);
                assert!(next.distance(target) <= previous.distance(target) + 1e-6);
                previous = next;
            }
            assert!(previous.distance(target) < 0.0001);
            assert_eq!(tracker.advance(target, 0.0), previous);
        }
    }

    #[test]
    fn feeding_face_stays_in_largest_volume_instead_of_chasing_food_outside() {
        let mut ps = Vec::new();
        for x in -3..=3 {
            for y in -3..=3 {
                let mut p = particle(
                    Vec2::new(x as f32 * 0.09, y as f32 * 0.09),
                    Vec2::X,
                    Vec2::splat(0.18),
                );
                p.component_id = 2;
                ps.push(p);
            }
        }
        let mut detached = particle(Vec2::new(1.5, 0.0), Vec2::X, Vec2::splat(0.1));
        detached.component_id = 1;
        detached.main_component = true;
        ps.push(detached);
        let origin = contain_face_origin(
            &ps,
            0.18,
            Vec2::ZERO,
            Vec2::new(0.0, -1.0),
            Vec2::X,
            Vec2::Y,
            Vec2::ONE,
        );
        assert!(origin.y > -0.25 && origin.x.abs() < 0.3);
        let moved = contain_face_origin(
            &ps,
            0.18,
            detached.position,
            detached.position,
            Vec2::X,
            Vec2::Y,
            Vec2::ONE,
        );
        assert!(moved.length() < 0.4);
        let ks = kernels(&ps[..49]);
        for offset in [
            Vec2::new(-0.24, 0.09),
            Vec2::new(0.24, 0.09),
            Vec2::new(0.0, -0.19),
        ] {
            assert!(raw_density(&ks, origin + offset) >= 0.18);
        }
    }

    #[test]
    fn isolated_and_rotated_ellipse_match_analytic_iso_not_kernel_hull() {
        for angle in [0.0_f32, 0.4, 1.2] {
            let p = particle(
                Vec2::new(0.2, -0.1),
                Vec2::from_angle(angle),
                Vec2::new(0.2, 0.1),
            );
            let (lo, hi) = contact_surface_bounds(&[p], 0.34, Vec2::ZERO).unwrap();
            let scale = (1.0 - 0.34_f32.cbrt()).sqrt();
            let extent = Vec2::new(
                ((p.axis_major.x * 0.2).powi(2) + (p.axis_major.y * 0.1).powi(2)).sqrt(),
                ((p.axis_major.y * 0.2).powi(2) + (p.axis_major.x * 0.1).powi(2)).sqrt(),
            ) * scale;
            assert!((hi - p.position - extent).abs().max_element() < 0.0001);
            assert!((lo - p.position + extent).abs().max_element() < 0.0001);
        }
    }
    #[test]
    fn orb_contacts_density_core_not_invisible_kernel_skirt() {
        let ps = [particle(Vec2::ZERO, Vec2::X, Vec2::new(0.20, 0.10))];
        let iso = 0.34;
        let edge = contact_surface_support(&ps, iso, Vec2::X).unwrap();
        let radius = 0.03;
        let outside = edge + Vec2::X * (radius + 0.015);
        assert!(outside.x < 0.20 + radius); // old kernel collision would fire
        assert!(contact_surface_circle(&ps, iso, outside, outside, radius).is_none());
        let inside = edge + Vec2::X * (radius - 0.006);
        let (point, normal, depth, swept) =
            contact_surface_circle(&ps, iso, inside, inside, radius).unwrap();
        assert!((point - edge).length() < 0.0001);
        assert!((depth - 0.006).abs() < 0.0001);
        assert!(normal.dot(Vec2::X) > 0.999 && !swept);
        assert!(contact_surface_circle(&ps, iso, outside, -outside, radius).is_some());
    }
    #[test]
    fn dense_cluster_matches_independent_grid_oracle_and_ignores_cosmetics() {
        let mut ps = Vec::new();
        for y in -2..=2 {
            for x in -3..=3 {
                ps.push(particle(
                    Vec2::new(x as f32 * 0.055, y as f32 * 0.055),
                    Vec2::from_angle(0.37),
                    Vec2::new(0.13, 0.09),
                ));
            }
        }
        let actual = contact_surface_bounds(&ps, 0.34, Vec2::ZERO).unwrap();
        let ks = kernels(&ps);
        let mut lo = Vec2::splat(f32::INFINITY);
        let mut hi = -lo;
        for y in -180..=180 {
            for x in -250..=250 {
                let p = Vec2::new(x as f32, y as f32) * 0.002;
                if raw_density(&ks, p) >= 0.34 {
                    lo = lo.min(p);
                    hi = hi.max(p);
                }
            }
        }
        assert!(
            (actual.0 - lo).abs().max_element() < 0.003,
            "{actual:?} grid={lo:?}/{hi:?}"
        );
        assert!((actual.1 - hi).abs().max_element() < 0.003);
        for p in &mut ps {
            p.face_weight = 0.9;
            p.main_component = false;
            p.pigment = 0.8;
            p.emission = 1.7;
            p.optical_thickness = 0.02;
        }
        assert_eq!(
            actual,
            contact_surface_bounds(&ps, 0.34, Vec2::ZERO).unwrap()
        );
    }
    #[test]
    fn reconstruction_cost_96_particles() {
        let mut ps = Vec::new();
        for y in 0..8 {
            for x in 0..12 {
                ps.push(particle(
                    Vec2::new(x as f32, y as f32) * 0.04,
                    Vec2::X,
                    Vec2::new(0.10, 0.07),
                ));
            }
        }
        let start = std::time::Instant::now();
        for _ in 0..100 {
            std::hint::black_box(
                contact_surface_bounds(std::hint::black_box(&ps), 0.34, Vec2::ZERO).unwrap(),
            );
        }
        eprintln!(
            "contact contour 96 particles mean {:?}",
            start.elapsed() / 100
        );
        let expected = contact_surface_bounds(&ps, 0.34, Vec2::ZERO).unwrap().0.y;
        let start = std::time::Instant::now();
        for _ in 0..100 {
            let bottom =
                contact_surface_min_y(std::hint::black_box(&ps), 0.34, Vec2::ZERO).unwrap();
            assert_eq!(bottom, expected);
        }
        eprintln!(
            "bottom contour 96 particles mean {:?}",
            start.elapsed() / 100
        );
    }

    #[test]
    fn detached_non_face_particle_still_supplies_physical_extent() {
        let mut ps = [
            particle(Vec2::ZERO, Vec2::X, Vec2::splat(0.1)),
            particle(Vec2::new(0.6, -0.4), Vec2::X, Vec2::splat(0.08)),
        ];
        ps[0].face_weight = 1.0;
        ps[0].main_component = true;
        ps[1].component_id = 2;
        let (lo, hi) = contact_surface_bounds(&ps, 0.34, Vec2::ZERO).unwrap();
        let scale = (1.0 - 0.34_f32.cbrt()).sqrt();
        assert!((hi.x - (0.6 + 0.08 * scale)).abs() < 0.0001);
        assert!((lo.y - (-0.4 - 0.08 * scale)).abs() < 0.0001);
    }

    #[test]
    fn filtered_surface_uses_density_not_optical_or_glow_channels() {
        let ps = [particle(Vec2::ZERO, Vec2::X, Vec2::splat(0.1))];
        let filter = Vec2::splat(0.008);
        let (lo, hi) = contact_surface_bounds(&ps, 0.34, filter).unwrap();
        assert!((lo + hi).length() < 0.0001);
        let ks = kernels(&ps);
        assert!((density(&ks, Vec2::new(hi.x, 0.0), filter) - 0.34).abs() < 0.0002);
    }

    #[test]
    fn plane_support_uses_iso_contour_and_rejects_invisible_kernel_skirt() {
        let ps = [particle(Vec2::ZERO, Vec2::X, Vec2::splat(0.1653))];
        let radius = 0.1653 * (1.0 - 0.34_f32.cbrt()).sqrt();
        assert!(surface_patch_contacts_plane(
            &ps,
            0.34,
            Vec2::new(0.0, -radius),
            Vec2::Y,
            Vec2::X,
            0.025
        ));
        assert!(!surface_patch_contacts_plane(
            &ps,
            0.34,
            Vec2::new(0.0, -0.1653),
            Vec2::Y,
            Vec2::X,
            0.025
        ));
        assert!(!surface_patch_contacts_plane(
            &ps,
            0.34,
            Vec2::new(0.0, -radius - 0.08),
            Vec2::Y,
            Vec2::X,
            0.025
        ));
    }
}
