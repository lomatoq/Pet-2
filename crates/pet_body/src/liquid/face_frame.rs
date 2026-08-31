use glam::Vec2;

use crate::FaceTuning;

use super::particles::{LiquidParticle, MAX_LIQUID_PARTICLES};

const DEFAULT_FACE_ORIGIN: Vec2 = Vec2::new(0.0, 0.13);
const MAX_ORIGIN_STEP: f32 = 0.008;
const MAX_ROLL_STEP: f32 = 0.02;
const MAX_SCALE_STEP: f32 = 0.01;
const MIN_SUPPORTED_PARTICLES: usize = 3;
const RESET_RECOVERY_SECONDS: f32 = 0.25;
const MAX_ATTENTION_OFFSET: f32 = 0.085;
const MAX_ATTENTION_ROLL: f32 = 0.15;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FaceFrame {
    pub origin: Vec2,
    pub axis_x: Vec2,
    pub axis_y: Vec2,
    pub scale: Vec2,
    pub confidence: f32,
}

impl Default for FaceFrame {
    fn default() -> Self {
        Self {
            origin: DEFAULT_FACE_ORIGIN,
            axis_x: Vec2::X,
            axis_y: Vec2::Y,
            scale: Vec2::ONE,
            confidence: 1.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct FaceFrameRuntime {
    pub frame: FaceFrame,
    tuning: FaceTuning,
    /// The material particle that owns the semantic face. Particle slots and
    /// face weights are immutable for a runtime, so this is selected once and
    /// deliberately never follows whichever component happens to be largest.
    carrier_index: Option<usize>,
    target_origin: Vec2,
    target_roll: f32,
    target_scale: Vec2,
    target_confidence: f32,
    origin_velocity: Vec2,
    roll_velocity: f32,
    scale_velocity: Vec2,
    recovery_remaining: f32,
    attention_offset: Vec2,
    attention_roll: f32,
}

impl Default for FaceFrameRuntime {
    fn default() -> Self {
        Self {
            frame: FaceFrame::default(),
            tuning: FaceTuning::default(),
            carrier_index: None,
            target_origin: DEFAULT_FACE_ORIGIN,
            target_roll: 0.0,
            target_scale: Vec2::ONE,
            target_confidence: 1.0,
            origin_velocity: Vec2::ZERO,
            roll_velocity: 0.0,
            scale_velocity: Vec2::ZERO,
            recovery_remaining: 0.0,
            attention_offset: Vec2::ZERO,
            attention_roll: 0.0,
        }
    }
}

impl FaceFrameRuntime {
    pub fn set_tuning(&mut self, tuning: FaceTuning) {
        if tuning == self.tuning {
            return;
        }
        // Static face controls remain live while the simulation is paused, but
        // they still enter through the presentation target. The redraw loop is
        // the sole owner of visible frame motion and enforces the same caps as a
        // material-driven target change.
        let origin_delta = Vec2::from_array(tuning.origin) - Vec2::from_array(self.tuning.origin);
        self.target_origin += origin_delta;
        let previous_scale = Vec2::from_array(self.tuning.scale).max(Vec2::splat(0.001));
        let next_scale = Vec2::from_array(tuning.scale);
        let scale_ratio = next_scale / previous_scale;
        self.target_scale =
            (self.target_scale * scale_ratio).clamp(Vec2::splat(0.55), Vec2::splat(1.50));
        self.tuning = tuning;
    }

    /// Keeps the last valid rendered frame while freshly reset material becomes
    /// authoritative over a short presentation-only recovery window.
    pub fn begin_recovery(&mut self) {
        self.target_origin = self.frame.origin;
        self.target_roll = frame_roll(self.frame);
        self.target_scale = self.frame.scale;
        self.target_confidence = self.frame.confidence;
        self.origin_velocity = Vec2::ZERO;
        self.roll_velocity = 0.0;
        self.scale_velocity = Vec2::ZERO;
        self.recovery_remaining = RESET_RECOVERY_SECONDS;
    }

    /// Sets a brain-authored additive pose for the complete facial mask. This
    /// does not reparent the permanent carrier or advance presentation.
    pub fn set_attention_pose(&mut self, offset: Vec2, roll: f32) {
        self.attention_offset = if offset.is_finite() {
            offset.clamp_length_max(MAX_ATTENTION_OFFSET)
        } else {
            Vec2::ZERO
        };
        self.attention_roll = if roll.is_finite() {
            roll.clamp(-MAX_ATTENTION_ROLL, MAX_ATTENTION_ROLL)
        } else {
            0.0
        };
    }

    /// Samples the semantic face target from simulation state. This deliberately
    /// does not advance the rendered frame; callers present it once per redraw.
    #[allow(clippy::too_many_arguments)]
    pub fn sample_target(
        &mut self,
        particles: &[LiquidParticle; MAX_LIQUID_PARTICLES],
        count: usize,
        _main_component: u8,
        _main_com: Vec2,
        body_origin: Vec2,
        dt: f32,
    ) {
        let count = count.min(MAX_LIQUID_PARTICLES);
        let dt = if dt.is_finite() {
            dt.clamp(0.0, 0.05)
        } else {
            0.0
        };
        let recovery_blend = if self.recovery_remaining > 0.0 && dt > 0.0 {
            (dt / self.recovery_remaining.max(dt)).clamp(0.0, 1.0)
        } else {
            1.0
        };

        if self.carrier_index.is_none() {
            self.carrier_index = particles[..count]
                .iter()
                .enumerate()
                .max_by(|(index_a, a), (index_b, b)| {
                    a.face_weight
                        .total_cmp(&b.face_weight)
                        // `max_by` chooses the greater item; reverse the index
                        // comparison so an exact tie deterministically picks the
                        // lower immutable particle slot.
                        .then_with(|| index_b.cmp(index_a))
                })
                .map(|(index, _)| index);
        }

        let mut total_face_weight = 0.0;
        for particle in &particles[..count] {
            total_face_weight += particle.face_weight.max(0.0).powi(2);
        }

        let carrier_component = self
            .carrier_index
            .filter(|&index| index < count)
            .map(|index| particles[index].component_id);
        let mut supported_weight = 0.0;
        let mut supported_count = 0_usize;
        let mut component_center = Vec2::ZERO;
        let mut component_count = 0_usize;
        if let Some(component) = carrier_component {
            for particle in &particles[..count] {
                if particle.component_id != component || !particle.render_position.is_finite() {
                    continue;
                }
                component_center += particle.render_position - body_origin;
                component_count += 1;
                let weight = particle.face_weight.max(0.0).powi(2);
                if weight <= 1.0e-6 {
                    continue;
                }
                supported_count += 1;
                supported_weight += weight;
            }
        }

        let raw_confidence = (supported_weight / total_face_weight.max(1.0e-5)).clamp(0.0, 1.0);
        let enough_support = supported_count >= MIN_SUPPORTED_PARTICLES
            && component_count >= MIN_SUPPORTED_PARTICLES
            && supported_weight > 1.0e-5
            && raw_confidence >= 0.08;
        if enough_support {
            component_center /= component_count as f32;

            // The neutral semantic face follows the permanent carrier's center
            // plus its authored upright offset. Face-weighted material is used
            // only for carrier support/confidence: if it rigidly spins, the
            // material may flow behind the face but cannot make the face orbit.
            // There is intentionally no largest-component, AABB, cursor, or
            // interaction fallback.
            let origin_bias = Vec2::from_array(self.tuning.origin) - DEFAULT_FACE_ORIGIN;
            let candidate_origin =
                component_center + DEFAULT_FACE_ORIGIN + origin_bias + self.attention_offset;
            if candidate_origin.is_finite() {
                self.target_origin = self.target_origin.lerp(candidate_origin, recovery_blend);
            }

            // Material rotation is deliberately excluded: the neutral semantic
            // face is exactly upright. Only brain-authorized attention adds a
            // bounded head-turn roll.
            let candidate_roll = self
                .attention_roll
                .clamp(-face_roll_limit(self.tuning), face_roll_limit(self.tuning));
            self.target_roll +=
                shortest_angle_delta(self.target_roll, candidate_roll) * recovery_blend;
            let candidate_scale =
                Vec2::from_array(self.tuning.scale).clamp(Vec2::splat(0.55), Vec2::splat(1.50));
            self.target_scale = self.target_scale.lerp(candidate_scale, recovery_blend);
        }

        self.target_confidence = if enough_support {
            self.target_confidence + (raw_confidence - self.target_confidence) * recovery_blend
        } else {
            0.0
        };
        self.recovery_remaining = (self.recovery_remaining - dt).max(0.0);
        if self.recovery_remaining <= 1.0e-6 {
            self.recovery_remaining = 0.0;
        }
    }

    #[cfg(test)]
    #[allow(clippy::too_many_arguments)]
    pub fn update(
        &mut self,
        particles: &[LiquidParticle; MAX_LIQUID_PARTICLES],
        count: usize,
        main_component: u8,
        main_com: Vec2,
        body_origin: Vec2,
        dt: f32,
    ) {
        self.sample_target(particles, count, main_component, main_com, body_origin, dt);
    }

    /// Advances the visible frame exactly once for a presented redraw.
    pub fn present(&mut self, dt: f32) {
        let dt = if dt.is_finite() {
            dt.clamp(0.0, 0.05)
        } else {
            0.0
        };

        // Exact critically damped integration remains stable across a 50 ms
        // presentation hitch. The final hard caps are a second safety boundary:
        // topology changes can move the target, never teleport the rendered face.
        let origin_before = self.frame.origin;
        let origin_frequency = self.tuning.translation_smoothing.clamp(0.5, 40.0);
        critical_damped_vec2(
            &mut self.frame.origin,
            &mut self.origin_velocity,
            self.target_origin,
            origin_frequency,
            dt,
        );
        cap_vec2_step(
            &mut self.frame.origin,
            &mut self.origin_velocity,
            origin_before,
            MAX_ORIGIN_STEP,
            dt,
        );

        let roll_before = frame_roll(self.frame);
        let roll_target = roll_before + shortest_angle_delta(roll_before, self.target_roll);
        let mut roll = roll_before;
        critical_damped_scalar(
            &mut roll,
            &mut self.roll_velocity,
            roll_target,
            self.tuning.rotation_smoothing.clamp(0.5, 40.0),
            dt,
        );
        let raw_roll_step = shortest_angle_delta(roll_before, roll);
        let roll_step = raw_roll_step.clamp(-MAX_ROLL_STEP, MAX_ROLL_STEP);
        let unclamped_roll = roll_before + roll_step;
        roll = unclamped_roll.clamp(-face_roll_limit(self.tuning), face_roll_limit(self.tuning));
        let roll_was_capped =
            (roll_step - raw_roll_step).abs() > 1.0e-7 || (roll - unclamped_roll).abs() > 1.0e-7;
        if roll_was_capped && dt > 0.0 {
            self.roll_velocity = shortest_angle_delta(roll_before, roll) / dt;
        }
        self.frame.axis_x = Vec2::from_angle(roll);
        self.frame.axis_y = Vec2::new(-self.frame.axis_x.y, self.frame.axis_x.x);

        let scale_before = self.frame.scale;
        critical_damped_vec2(
            &mut self.frame.scale,
            &mut self.scale_velocity,
            self.target_scale,
            self.tuning.scale_smoothing.clamp(0.5, 40.0),
            dt,
        );
        let raw_scale_step = self.frame.scale - scale_before;
        let scale_step =
            raw_scale_step.clamp(Vec2::splat(-MAX_SCALE_STEP), Vec2::splat(MAX_SCALE_STEP));
        self.frame.scale = (scale_before + scale_step).clamp(Vec2::splat(0.55), Vec2::splat(1.50));
        if scale_step != raw_scale_step && dt > 0.0 {
            self.scale_velocity = scale_step / dt;
        }

        // Low support holds the last geometric target while confidence decays.
        // Recovery is also filtered so split/remerge cannot flash the face.
        let confidence_rate = if self.target_confidence > self.frame.confidence {
            12.0
        } else {
            7.0
        };
        self.frame.confidence += (self.target_confidence - self.frame.confidence)
            * (1.0 - (-confidence_rate * dt).exp());
        self.frame.confidence = self.frame.confidence.clamp(0.0, 1.0);

        if !self.frame.origin.is_finite()
            || !self.frame.scale.is_finite()
            || !self.frame.axis_x.is_finite()
            || !self.frame.confidence.is_finite()
        {
            // Preserve the last valid presentation instead of snapping to an
            // invalid material target. Reset only derivative state.
            self.frame.origin = origin_before;
            self.frame.scale = scale_before;
            self.frame.axis_x = Vec2::from_angle(roll_before);
            self.frame.axis_y = Vec2::new(-self.frame.axis_x.y, self.frame.axis_x.x);
            self.frame.confidence = 0.0;
            self.origin_velocity = Vec2::ZERO;
            self.roll_velocity = 0.0;
            self.scale_velocity = Vec2::ZERO;
        }
    }
}

fn frame_roll(frame: FaceFrame) -> f32 {
    frame.axis_x.y.atan2(frame.axis_x.x)
}

fn face_roll_limit(tuning: FaceTuning) -> f32 {
    if tuning.maximum_roll_radians <= 1.0e-4 {
        0.18
    } else {
        tuning.maximum_roll_radians.clamp(0.0, 0.32)
    }
}

fn shortest_angle_delta(from: f32, to: f32) -> f32 {
    let mut delta = (to - from) % std::f32::consts::TAU;
    if delta > std::f32::consts::PI {
        delta -= std::f32::consts::TAU;
    } else if delta < -std::f32::consts::PI {
        delta += std::f32::consts::TAU;
    }
    delta
}

fn critical_damped_vec2(
    current: &mut Vec2,
    velocity: &mut Vec2,
    target: Vec2,
    frequency: f32,
    dt: f32,
) {
    if !current.is_finite() || !velocity.is_finite() || !target.is_finite() {
        *velocity = Vec2::ZERO;
        return;
    }
    let omega = frequency.max(0.0);
    let offset = *current - target;
    let helper = *velocity + offset * omega;
    let decay = (-omega * dt).exp();
    *current = target + (offset + helper * dt) * decay;
    *velocity = (*velocity - helper * (omega * dt)) * decay;
}

fn critical_damped_scalar(
    current: &mut f32,
    velocity: &mut f32,
    target: f32,
    frequency: f32,
    dt: f32,
) {
    if !current.is_finite() || !velocity.is_finite() || !target.is_finite() {
        *velocity = 0.0;
        return;
    }
    let omega = frequency.max(0.0);
    let offset = *current - target;
    let helper = *velocity + offset * omega;
    let decay = (-omega * dt).exp();
    *current = target + (offset + helper * dt) * decay;
    *velocity = (*velocity - helper * (omega * dt)) * decay;
}

fn cap_vec2_step(
    current: &mut Vec2,
    velocity: &mut Vec2,
    previous: Vec2,
    maximum_step: f32,
    dt: f32,
) {
    let raw_step = *current - previous;
    let step = raw_step.clamp_length_max(maximum_step);
    *current = previous + step;
    if step != raw_step && dt > 0.0 {
        *velocity = step / dt;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::liquid::particles::initialize_particles;

    fn update(
        runtime: &mut FaceFrameRuntime,
        particles: &[LiquidParticle; MAX_LIQUID_PARTICLES],
        count: usize,
        dt: f32,
    ) {
        runtime.update(particles, count, 0, Vec2::ZERO, Vec2::ZERO, dt);
        runtime.present(dt);
    }

    #[test]
    fn material_rotation_cannot_tilt_the_neutral_semantic_face() {
        let (mut particles, count) = initialize_particles(9);
        for particle in &mut particles[..count] {
            particle.render_position =
                Vec2::new(-particle.render_position.y, particle.render_position.x);
        }
        let mut runtime = FaceFrameRuntime::default();
        for _ in 0..120 {
            update(&mut runtime, &particles, count, 1.0 / 120.0);
        }
        let angle = frame_roll(runtime.frame);
        assert_eq!(runtime.target_roll, 0.0);
        assert!(angle.abs() < 1.0e-6, "neutral face roll {angle}");
        assert!(runtime.frame.axis_y.y > 0.999_999);
    }

    #[test]
    fn rigid_material_spin_cannot_orbit_the_semantic_face() {
        let (mut particles, count) = initialize_particles(10);
        let mut runtime = FaceFrameRuntime::default();
        for _ in 0..120 {
            update(&mut runtime, &particles, count, 1.0 / 120.0);
        }
        let target_before = runtime.target_origin;
        let component_center = particles[..count]
            .iter()
            .map(|particle| particle.render_position)
            .sum::<Vec2>()
            / count as f32;

        for angle in [0.7_f32, 1.4, 2.1, 2.8] {
            let rotation = Vec2::from_angle(angle);
            for particle in &mut particles[..count] {
                let arm = particle.render_position - component_center;
                particle.render_position = component_center
                    + Vec2::new(
                        arm.x * rotation.x - arm.y * rotation.y,
                        arm.x * rotation.y + arm.y * rotation.x,
                    );
            }
            runtime.update(&particles, count, 0, Vec2::ZERO, Vec2::ZERO, 1.0 / 120.0);
            assert!(
                runtime.target_origin.distance(target_before) < 1.0e-5,
                "semantic face orbited with material spin: before={target_before:?}, after={:?}",
                runtime.target_origin
            );
        }
    }

    #[test]
    fn editor_origin_and_scale_are_live_while_simulation_is_paused() {
        let mut runtime = FaceFrameRuntime::default();
        let tuning = FaceTuning {
            origin: [0.08, 0.21],
            scale: [1.2, 0.8],
            ..FaceTuning::default()
        };

        runtime.set_tuning(tuning);
        assert_eq!(runtime.frame, FaceFrame::default());
        for _ in 0..120 {
            runtime.present(1.0 / 60.0);
        }

        assert!(runtime.frame.origin.distance(Vec2::new(0.08, 0.21)) < 1.0e-5);
        assert!(runtime.frame.scale.distance(Vec2::new(1.2, 0.8)) < 1.0e-5);
        let settled = runtime.frame;
        runtime.set_tuning(tuning);
        assert_eq!(runtime.frame, settled);
    }

    #[test]
    fn neutral_x_uses_carrier_symmetry_center_and_preserves_authored_bias() {
        let (mut particles, count) = initialize_particles(0x000C_373A);
        for particle in &mut particles[..count] {
            particle.component_id = 0;
            if particle.face_weight > 0.20 {
                particle.render_position.x += 0.26;
            }
        }
        let component_center_x = particles[..count]
            .iter()
            .map(|particle| particle.render_position.x)
            .sum::<f32>()
            / count as f32;
        let face_weight_sum = particles[..count]
            .iter()
            .map(|particle| particle.face_weight.max(0.0).powi(2))
            .sum::<f32>();
        let weighted_face_x = particles[..count]
            .iter()
            .map(|particle| particle.render_position.x * particle.face_weight.max(0.0).powi(2))
            .sum::<f32>()
            / face_weight_sum;
        assert!((weighted_face_x - component_center_x).abs() > 0.04);

        let mut runtime = FaceFrameRuntime::default();
        runtime.update(&particles, count, 0, Vec2::ZERO, Vec2::ZERO, 1.0 / 120.0);
        assert!((runtime.target_origin.x - component_center_x).abs() < 1.0e-6);

        runtime.set_tuning(FaceTuning {
            origin: [0.065, DEFAULT_FACE_ORIGIN.y],
            ..FaceTuning::default()
        });
        runtime.update(&particles, count, 0, Vec2::ZERO, Vec2::ZERO, 1.0 / 120.0);
        assert!((runtime.target_origin.x - component_center_x - 0.065).abs() < 1.0e-6);
    }

    #[test]
    fn attention_moves_and_rolls_the_whole_frame_with_presentation_caps() {
        let (particles, count) = initialize_particles(0xA77E_1710);
        let mut runtime = FaceFrameRuntime::default();
        runtime.set_tuning(FaceTuning {
            maximum_roll_radians: 0.18,
            ..FaceTuning::default()
        });
        runtime.update(&particles, count, 0, Vec2::ZERO, Vec2::ZERO, 1.0 / 120.0);
        let neutral_target = runtime.target_origin;

        runtime.set_attention_pose(Vec2::new(0.048, 0.018), -0.09);
        runtime.update(&particles, count, 0, Vec2::ZERO, Vec2::ZERO, 1.0 / 120.0);
        assert!(
            runtime
                .target_origin
                .distance(neutral_target + Vec2::new(0.048, 0.018))
                < 1.0e-6
        );
        assert!((runtime.target_roll + 0.09).abs() < 1.0e-6);

        let before = runtime.frame;
        runtime.present(0.05);
        assert!(runtime.frame.origin.distance(before.origin) <= MAX_ORIGIN_STEP + 1.0e-6);
        assert!(
            shortest_angle_delta(frame_roll(before), frame_roll(runtime.frame)).abs()
                <= MAX_ROLL_STEP + 1.0e-6
        );

        for _ in 0..180 {
            runtime.present(1.0 / 60.0);
        }
        assert!(runtime.frame.origin.distance(runtime.target_origin) < 1.0e-5);
        assert!((frame_roll(runtime.frame) + 0.09).abs() < 1.0e-5);

        runtime.set_attention_pose(Vec2::ZERO, 0.0);
        runtime.update(&particles, count, 0, Vec2::ZERO, Vec2::ZERO, 1.0 / 120.0);
        assert_eq!(runtime.target_roll, 0.0);
    }

    #[test]
    fn identical_live_tuning_does_not_reset_tracker_velocity() {
        let (mut particles, count) = initialize_particles(18);
        let mut runtime = FaceFrameRuntime::default();
        for particle in &mut particles[..count] {
            particle.render_position += Vec2::new(0.45, 0.0);
        }
        runtime.update(&particles, count, 0, Vec2::ZERO, Vec2::ZERO, 1.0 / 120.0);
        runtime.present(1.0 / 60.0);
        let velocity_before = runtime.origin_velocity;
        assert!(velocity_before.length() > 0.0);

        runtime.set_tuning(FaceTuning::default());

        assert_eq!(runtime.origin_velocity, velocity_before);
    }

    #[test]
    fn unchanged_material_keeps_the_face_origin_fixed() {
        let (particles, count) = initialize_particles(31);
        let mut runtime = FaceFrameRuntime::default();
        for _ in 0..120 {
            update(&mut runtime, &particles, count, 1.0 / 120.0);
        }
        let settled = runtime.frame.origin;
        for _ in 0..120 {
            update(&mut runtime, &particles, count, 1.0 / 120.0);
        }

        assert!(runtime.frame.origin.distance(settled) < 1.0e-5);
    }

    #[test]
    fn simulation_sampling_never_advances_the_presented_frame() {
        let (mut particles, count) = initialize_particles(32);
        let mut runtime = FaceFrameRuntime::default();
        for particle in &mut particles[..count] {
            particle.render_position += Vec2::new(0.40, -0.20);
        }
        let before = runtime.frame;

        for _ in 0..6 {
            runtime.update(&particles, count, 0, Vec2::ZERO, Vec2::ZERO, 1.0 / 120.0);
        }
        assert_eq!(runtime.frame, before);

        runtime.present(1.0 / 60.0);
        assert!(runtime.frame.origin.distance(before.origin) > 0.0);
        assert!(runtime.frame.origin.distance(before.origin) <= MAX_ORIGIN_STEP + 1.0e-6);
    }

    #[test]
    fn horizontal_body_stretch_cannot_widen_the_eyes() {
        let (mut particles, count) = initialize_particles(51);
        let tuning = FaceTuning {
            scale: [1.12, 0.94],
            ..FaceTuning::default()
        };
        let mut runtime = FaceFrameRuntime::default();
        runtime.set_tuning(tuning);
        for particle in &mut particles[..count] {
            particle.render_position.x *= 1.65;
        }

        for _ in 0..180 {
            update(&mut runtime, &particles, count, 1.0 / 120.0);
        }

        assert!((runtime.frame.scale.x - tuning.scale[0]).abs() < 1.0e-4);
        assert!((runtime.frame.scale.y - tuning.scale[1]).abs() < 1.0e-4);
    }

    #[test]
    fn permanent_carrier_survives_split_remerge_and_largest_component_changes() {
        let (mut particles, count) = initialize_particles(77);
        let mut runtime = FaceFrameRuntime::default();
        update(&mut runtime, &particles, count, 1.0 / 120.0);
        let carrier = runtime.carrier_index.expect("carrier");
        let carrier_rest = particles[carrier].rest_position;

        for particle in &mut particles[..count] {
            let belongs_to_face_patch = particle.rest_position.distance(carrier_rest) < 0.19;
            particle.component_id = u8::from(belongs_to_face_patch);
            if belongs_to_face_patch {
                particle.render_position += Vec2::new(0.42, -0.08);
            }
        }
        let before_split = runtime.frame.origin;
        update(&mut runtime, &particles, count, 0.05);
        assert_eq!(runtime.carrier_index, Some(carrier));
        assert!(runtime.frame.origin.distance(before_split) <= MAX_ORIGIN_STEP + 1.0e-6);

        // A competing particle becoming more face-like cannot reparent the
        // already-owned semantic face.
        let competitor = (carrier + 1) % count;
        particles[competitor].face_weight = 10.0;
        particles[competitor].component_id = 2;
        update(&mut runtime, &particles, count, 1.0 / 120.0);
        assert_eq!(runtime.carrier_index, Some(carrier));

        for particle in &mut particles[..count] {
            particle.component_id = 0;
        }
        let before_remerge = runtime.frame.origin;
        update(&mut runtime, &particles, count, 0.05);
        assert_eq!(runtime.carrier_index, Some(carrier));
        assert!(runtime.frame.origin.distance(before_remerge) <= MAX_ORIGIN_STEP + 1.0e-6);
    }

    #[test]
    fn low_support_holds_geometry_and_decays_confidence() {
        let (mut particles, count) = initialize_particles(91);
        let mut runtime = FaceFrameRuntime::default();
        for _ in 0..180 {
            update(&mut runtime, &particles, count, 1.0 / 120.0);
        }
        let carrier = runtime.carrier_index.expect("carrier");
        let confidence_before = runtime.frame.confidence;
        for (index, particle) in particles[..count].iter_mut().enumerate() {
            particle.component_id = if index == carrier { 7 } else { 3 };
            if index == carrier {
                particle.render_position += Vec2::splat(10.0);
            }
        }
        let origin_before = runtime.frame.origin;
        update(&mut runtime, &particles, count, 0.05);

        assert!(runtime.frame.origin.distance(origin_before) <= MAX_ORIGIN_STEP + 1.0e-6);
        assert!(runtime.frame.confidence < confidence_before);
    }

    #[test]
    fn render_positions_drive_target_and_hitch_cannot_jump_any_frame_channel() {
        let (mut particles, count) = initialize_particles(123);
        let mut runtime = FaceFrameRuntime::default();
        for _ in 0..120 {
            update(&mut runtime, &particles, count, 1.0 / 120.0);
        }
        for particle in &mut particles[..count] {
            let p = particle.render_position;
            particle.render_position = Vec2::new(-p.y, p.x) + Vec2::new(0.8, -0.5);
        }
        let before = runtime.frame;
        update(&mut runtime, &particles, count, 0.05);
        let roll_step = shortest_angle_delta(frame_roll(before), frame_roll(runtime.frame)).abs();
        let scale_step = (runtime.frame.scale - before.scale).abs().max_element();

        assert!(runtime.frame.origin.distance(before.origin) <= MAX_ORIGIN_STEP + 1.0e-6);
        assert!(roll_step <= MAX_ROLL_STEP + 1.0e-6);
        assert!(scale_step <= MAX_SCALE_STEP + 1.0e-6);
        assert!(runtime.frame.origin.x > before.origin.x);
    }

    #[test]
    fn reset_recovery_blends_new_material_target_over_quarter_second() {
        let (mut particles, count) = initialize_particles(404);
        let mut runtime = FaceFrameRuntime::default();
        for _ in 0..120 {
            update(&mut runtime, &particles, count, 1.0 / 120.0);
        }
        let target_before = runtime.target_origin;
        let frame_before = runtime.frame;
        for particle in &mut particles[..count] {
            particle.render_position += Vec2::new(0.60, -0.30);
        }

        runtime.begin_recovery();
        update(&mut runtime, &particles, count, 0.05);

        assert!((runtime.recovery_remaining - 0.20).abs() < 1.0e-6);
        assert!(runtime.target_origin.distance(target_before) < 0.16);
        assert!(runtime.frame.origin.distance(frame_before.origin) <= MAX_ORIGIN_STEP + 1.0e-6);
        for _ in 0..4 {
            update(&mut runtime, &particles, count, 0.05);
        }
        assert_eq!(runtime.recovery_remaining, 0.0);
        assert!(
            runtime
                .target_origin
                .distance(target_before + Vec2::new(0.60, -0.30))
                < 1.0e-4
        );
    }
}
