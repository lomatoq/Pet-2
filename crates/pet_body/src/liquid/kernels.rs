use glam::Vec2;

#[must_use]
pub fn density_kernel(delta: Vec2, radius: f32) -> f32 {
    let radius_squared = radius * radius;
    let distance_squared = delta.length_squared();
    if distance_squared >= radius_squared || radius <= f32::EPSILON {
        return 0.0;
    }
    let q = 1.0 - distance_squared / radius_squared;
    q * q * q
}

#[must_use]
pub fn density_gradient(delta: Vec2, radius: f32) -> Vec2 {
    let radius_squared = radius * radius;
    let distance_squared = delta.length_squared();
    if distance_squared >= radius_squared || radius <= f32::EPSILON {
        return Vec2::ZERO;
    }
    let q = 1.0 - distance_squared / radius_squared;
    delta * (-6.0 * q * q / radius_squared)
}

/// Compact, pairing-resistant Wendland C2 kernel. The omitted normalization
/// constant is intentional: callers either calibrate against the same kernel or
/// normalize its gradient to an authored peak response.
#[must_use]
pub fn wendland_c2_kernel(delta: Vec2, radius: f32) -> f32 {
    if radius <= f32::EPSILON {
        return 0.0;
    }
    let q = delta.length() / radius;
    if q >= 1.0 {
        return 0.0;
    }
    let one_minus_q = 1.0 - q;
    one_minus_q.powi(4) * (1.0 + 4.0 * q)
}

/// Gradient of [`wendland_c2_kernel`] with respect to `delta`.
///
/// The vector is exactly zero at the origin and at the support boundary, which
/// makes it suitable for a moving pointer potential without a force cusp.
#[must_use]
pub fn wendland_c2_gradient(delta: Vec2, radius: f32) -> Vec2 {
    if radius <= f32::EPSILON {
        return Vec2::ZERO;
    }
    let distance_squared = delta.length_squared();
    let radius_squared = radius * radius;
    if distance_squared <= 1.0e-12 || distance_squared >= radius_squared {
        return Vec2::ZERO;
    }
    let q = distance_squared.sqrt() / radius;
    delta * (-20.0 * (1.0 - q).powi(3) / radius_squared)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compact_kernel_and_gradient_are_finite_and_zero_at_support() {
        assert_eq!(density_kernel(Vec2::ZERO, 0.145), 1.0);
        assert_eq!(density_kernel(Vec2::splat(0.2), 0.145), 0.0);
        assert_eq!(density_gradient(Vec2::splat(0.2), 0.145), Vec2::ZERO);
        assert!(density_gradient(Vec2::new(0.04, -0.02), 0.145).is_finite());
    }

    #[test]
    fn wendland_c2_is_smooth_at_the_center_and_support() {
        let radius = 0.25;
        assert_eq!(wendland_c2_kernel(Vec2::ZERO, radius), 1.0);
        assert_eq!(wendland_c2_gradient(Vec2::ZERO, radius), Vec2::ZERO);
        assert_eq!(wendland_c2_kernel(Vec2::new(radius, 0.0), radius), 0.0);
        assert_eq!(
            wendland_c2_gradient(Vec2::new(radius, 0.0), radius),
            Vec2::ZERO
        );

        let left = wendland_c2_gradient(Vec2::new(radius * 0.25, 0.0), radius);
        let right = wendland_c2_gradient(Vec2::new(-radius * 0.25, 0.0), radius);
        assert!(left.x < 0.0);
        assert_eq!(left, -right);
        assert!(left.is_finite());
    }
}
