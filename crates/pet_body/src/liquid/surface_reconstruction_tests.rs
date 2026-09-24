use super::*;

fn radial_spectrum(runtime: &LiquidMorphRuntime) -> [f32; 33] {
    let render = runtime.render_state();
    let mut radii = [0.0; 128];
    for (i, radius) in radii.iter_mut().enumerate() {
        let axis = Vec2::from_angle(std::f32::consts::TAU * i as f32 / 128.0);
        let mut low = 0.0;
        let mut high = 1.0;
        for _ in 0..20 {
            let r = (low + high) * 0.5;
            let point = axis * r;
            let density: f32 = render.particles[..render.particle_count]
                .iter()
                .map(|p| {
                    let d = point - p.position;
                    let q = Vec2::new(
                        d.dot(p.axis_major) / p.major_radius,
                        d.dot(p.axis_major.perp()) / p.minor_radius,
                    );
                    (1.0 - q.length_squared()).max(0.0).powi(3) * p.density
                })
                .sum();
            if density >= runtime.tuning.iso_threshold {
                low = r;
            } else {
                high = r;
            }
        }
        *radius = (low + high) * 0.5;
    }
    std::array::from_fn(|k| {
        let sum: Vec2 = radii
            .iter()
            .enumerate()
            .map(|(i, r)| Vec2::from_angle(std::f32::consts::TAU * (i * k) as f32 / 128.0) * *r)
            .sum();
        sum.length() / 128.0
    })
}

#[test]
fn reconstruction_smooths_particle_ripples_but_keeps_bulk_deformation() {
    let mut runtime = LiquidMorphRuntime::new_with_tuning(
        5784121873664838231,
        PbfTuning {
            spacing_scale: 0.88,
            kernel_radius_scale: 1.14,
            render_center_smoothing: 0.22,
            anisotropy_max: 2.01,
            iso_threshold: 0.34,
            ..PbfTuning::default()
        },
    );
    for p in &mut runtime.particles[..runtime.particle_count] {
        let radial = p.position.normalize_or_zero();
        let angle = radial.y.atan2(radial.x);
        let ripple = ((angle * 11.0).sin() * 0.024 + (angle * 17.0).cos() * 0.016)
            * (p.position.length() / 0.36).clamp(0.0, 1.0).powi(3);
        p.position += radial * ripple;
        p.position *= Vec2::new(1.20, 1.0 / 1.20);
        // Surface exposure is a physical diagnostic, not a random permission
        // for neighbouring reconstruction kernels to have different smoothness.
        p.surface_score = (angle * 7.0).sin().mul_add(0.5, 0.5);
    }
    let before = runtime.particles;
    runtime.snap_render_proxies();
    let spectrum = radial_spectrum(&runtime);
    let roughness = spectrum[5..].iter().map(|a| a * a).sum::<f32>().sqrt();
    eprintln!(
        "surface spectrum: radius={}, elongation={}, roughness={roughness}",
        spectrum[0], spectrum[2]
    );
    assert!(roughness < 0.0038);
    assert!(
        spectrum[2] > 0.012,
        "the bulk ellipse must remain deformable"
    );
    assert!(
        spectrum[0] > 0.31,
        "smoothing must not erase the bulk volume"
    );
    for (p, old) in runtime.particles.iter().zip(before) {
        assert_eq!(p.position, old.position);
        assert_eq!(p.velocity, old.velocity);
        assert_eq!(p.inverse_mass, old.inverse_mass);
    }
}

#[test]
fn surface_diagnostic_noise_does_not_dent_the_contour_or_bridge_a_drop() {
    let mut runtime = LiquidMorphRuntime::new(42);
    let before: Vec<_> = (0..runtime.particle_count)
        .map(|i| runtime.anisotropy_target(i))
        .collect();
    for (i, p) in runtime.particles[..runtime.particle_count]
        .iter_mut()
        .enumerate()
    {
        p.surface_score = if i % 2 == 0 { 0.0 } else { 1.0 };
    }
    for (i, target) in before.into_iter().enumerate() {
        assert_eq!(runtime.anisotropy_target(i), target);
    }
    let i = runtime.particle_count - 1;
    runtime.particles[i].component_id = 1;
    // A nearby detached droplet is still its own round surface, not an
    // elongated bridge created by the larger body's reconstruction samples.
    runtime.particles[i].position = runtime.particles[0].position + Vec2::X * 0.12;
    let (center, _, aspect) = runtime.anisotropy_target(i);
    assert!(center.distance(runtime.particles[i].position) < 1e-6);
    assert_eq!(aspect, 1.0);
}
