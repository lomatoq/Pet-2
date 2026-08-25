use std::f32::consts::{FRAC_PI_3, TAU};

use crate::DerivedVisualTraits;

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct EyeAutonomicModifiers {
    pub pain: f32,
    pub pharmacologic_mydriasis: f32,
    pub pharmacologic_miosis: f32,
    pub neural_reactivity_loss: f32,
    /// Signed left/right bias. Zero is the healthy symmetric default.
    pub anisocoria: f32,
}

impl EyeAutonomicModifiers {
    fn sanitize(&mut self) {
        self.pain = unit(self.pain);
        self.pharmacologic_mydriasis = unit(self.pharmacologic_mydriasis);
        self.pharmacologic_miosis = unit(self.pharmacologic_miosis);
        self.neural_reactivity_loss = unit(self.neural_reactivity_loss);
        self.anisocoria = finite_or(self.anisocoria, 0.0).clamp(-1.0, 1.0);
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct VisualMindInput {
    pub valence: f32,
    pub arousal: f32,
    pub stress: f32,
    pub attachment: f32,
    pub confidence: f32,
    pub frustration: f32,
    pub fatigue: f32,
    pub curiosity: f32,
    pub novelty: f32,
    pub social_focus: f32,
    pub attention_confidence: f32,
    pub attention_commitment: f32,
    pub self_uncertainty: f32,
    pub local_luminance: f32,
    pub scroll_velocity: f32,
    pub window_pressure: f32,
    pub eye_modifiers: EyeAutonomicModifiers,
}

impl VisualMindInput {
    pub fn sanitize(&mut self) {
        self.valence = finite_or(self.valence, 0.0).clamp(-1.0, 1.0);
        self.arousal = unit(self.arousal);
        self.stress = unit(self.stress);
        self.attachment = unit(self.attachment);
        self.confidence = unit(self.confidence);
        self.frustration = unit(self.frustration);
        self.fatigue = unit(self.fatigue);
        self.curiosity = unit(self.curiosity);
        self.novelty = unit(self.novelty);
        self.social_focus = unit(self.social_focus);
        self.attention_confidence = unit(self.attention_confidence);
        self.attention_commitment = unit(self.attention_commitment);
        self.self_uncertainty = unit(self.self_uncertainty);
        self.local_luminance = finite_or(self.local_luminance, 0.5).clamp(0.0, 1.0);
        self.scroll_velocity = finite_or(self.scroll_velocity, 0.0).clamp(-1.0, 1.0);
        self.window_pressure = unit(self.window_pressure);
        self.eye_modifiers.sanitize();
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct VisualPhysiologyPose {
    pub pulse: f32,
    pub pulse_amplitude: f32,
    pub shell_opacity: f32,
    pub inner_density: f32,
    pub translucency: f32,
    pub flow_strength: f32,
    pub flow_speed: f32,
    pub flow_phase: f32,
    pub core_glow: f32,
    pub halo: f32,
    pub iris_activity: f32,
    pub eye_wetness: f32,
    pub droplet_energy: f32,
    pub droplet_spread: f32,
    pub droplet_cohesion: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VisualPhysiologyRuntime {
    pub pose: VisualPhysiologyPose,
    metabolic_phase: f32,
    secondary_phase: f32,
    irregular_phase: f32,
    seed_phase_a: f32,
    seed_phase_b: f32,
}

impl VisualPhysiologyRuntime {
    #[must_use]
    pub fn new(identity_seed: u64, traits: &DerivedVisualTraits) -> Self {
        let seed_unit = identity_seed as u32 as f32 / u32::MAX as f32;
        Self {
            pose: VisualPhysiologyPose {
                pulse_amplitude: traits.metabolic_amplitude,
                shell_opacity: traits.shell_opacity,
                inner_density: 0.76,
                translucency: traits.translucency,
                flow_strength: 0.20,
                flow_speed: traits.flow_speed,
                core_glow: 0.08,
                halo: traits.halo_strength,
                iris_activity: 0.24,
                eye_wetness: 0.62,
                droplet_energy: 0.22,
                droplet_spread: 0.76,
                droplet_cohesion: traits.droplet_cohesion,
                ..VisualPhysiologyPose::default()
            },
            metabolic_phase: seed_unit * TAU,
            secondary_phase: seed_unit * TAU * 0.47,
            irregular_phase: seed_unit * TAU * 0.21,
            seed_phase_a: (seed_unit * TAU + FRAC_PI_3).rem_euclid(TAU),
            seed_phase_b: (seed_unit * TAU * 1.7 + 0.37).rem_euclid(TAU),
        }
    }

    pub fn update(&mut self, traits: &DerivedVisualTraits, mut mind: VisualMindInput, dt: f32) {
        mind.sanitize();
        let dt = finite_or(dt, 0.0).clamp(0.0, 0.05);
        let base_frequency = traits.metabolic_frequency
            * lerp(0.78, 1.72, mind.arousal)
            * lerp(1.0, 0.64, mind.fatigue);
        let irregularity = mind.stress.powf(1.4);
        self.metabolic_phase = (self.metabolic_phase + dt * base_frequency * TAU).rem_euclid(TAU);
        self.secondary_phase =
            (self.secondary_phase + dt * base_frequency * TAU * 0.47).rem_euclid(TAU);
        self.irregular_phase = (self.irregular_phase
            + dt * base_frequency * TAU * (0.21 + irregularity * 0.18))
            .rem_euclid(TAU);

        let primary = self.metabolic_phase.sin();
        let secondary = (self.secondary_phase + self.seed_phase_a).sin();
        let irregular =
            (self.irregular_phase + secondary * irregularity * 0.8 + self.seed_phase_b).sin();
        let pulse_raw = primary * 0.62 + secondary * 0.25 + irregular * 0.13;
        let amplitude_target = (traits.metabolic_amplitude
            + mind.arousal * 0.010
            + mind.attachment * 0.003
            + mind.novelty * 0.004
            - mind.fatigue * 0.004)
            .clamp(0.004, 0.028);
        self.pose.pulse_amplitude = smooth(self.pose.pulse_amplitude, amplitude_target, 1.4, dt);
        self.pose.pulse = pulse_raw * self.pose.pulse_amplitude;

        let shell_opacity_target =
            (traits.shell_opacity - mind.fatigue * 0.028 - mind.stress * 0.012
                + mind.confidence * 0.010)
                .clamp(0.90, 0.995);
        let inner_density_target = (0.72 + mind.arousal * 0.10 + mind.attachment * 0.06
            - mind.fatigue * 0.12
            - mind.self_uncertainty * 0.04)
            .clamp(0.55, 0.92);
        let translucency_target =
            (traits.translucency + mind.fatigue * 0.12 + mind.curiosity * 0.04
                - mind.stress * 0.05)
                .clamp(0.12, 0.55);
        self.pose.shell_opacity = smooth(self.pose.shell_opacity, shell_opacity_target, 1.8, dt);
        self.pose.inner_density = smooth(self.pose.inner_density, inner_density_target, 1.5, dt);
        self.pose.translucency = smooth(self.pose.translucency, translucency_target, 1.35, dt);

        let flow_strength_target = (0.16
            + mind.arousal * 0.26
            + mind.curiosity * 0.18
            + mind.novelty * 0.12
            + mind.stress * 0.08
            - mind.fatigue * 0.08)
            .clamp(0.04, 0.62);
        let flow_speed_target =
            (traits.flow_speed * lerp(0.55, 1.85, mind.arousal) * lerp(1.0, 0.58, mind.fatigue))
                .clamp(0.025, 0.55);
        self.pose.flow_strength = smooth(self.pose.flow_strength, flow_strength_target, 1.6, dt);
        self.pose.flow_speed = smooth(self.pose.flow_speed, flow_speed_target, 1.4, dt);
        self.pose.flow_phase = (self.pose.flow_phase + dt * self.pose.flow_speed).rem_euclid(TAU);

        let positive_valence = mind.valence.max(0.0);
        let core_target = (0.06
            + mind.attachment * 0.26
            + positive_valence * 0.18
            + mind.arousal * 0.12
            + mind.social_focus * 0.16)
            .clamp(0.02, 0.72);
        let halo_target = (traits.halo_strength
            + mind.social_focus * 0.055
            + mind.attachment * 0.035
            + mind.arousal * 0.025
            - mind.fatigue * 0.018)
            .clamp(0.01, 0.14);
        self.pose.core_glow = smooth(self.pose.core_glow, core_target, 1.8, dt);
        self.pose.halo = smooth(self.pose.halo, halo_target, 1.2, dt);
        self.pose.iris_activity = smooth(
            self.pose.iris_activity,
            (0.18 + mind.curiosity * 0.42 + mind.social_focus * 0.24 + mind.arousal * 0.12)
                .clamp(0.12, 0.92),
            2.4,
            dt,
        );
        self.pose.eye_wetness = smooth(
            self.pose.eye_wetness,
            (0.52 + mind.attachment * 0.16 + mind.social_focus * 0.10 - mind.fatigue * 0.08)
                .clamp(0.38, 0.88),
            1.8,
            dt,
        );

        self.pose.droplet_energy = smooth(
            self.pose.droplet_energy,
            (0.18 + mind.arousal * 0.42 + mind.curiosity * 0.22 + mind.novelty * 0.14
                - mind.fatigue * 0.18)
                .clamp(0.06, 0.92),
            2.1,
            dt,
        );
        self.pose.droplet_spread = smooth(
            self.pose.droplet_spread,
            (0.76 + mind.arousal * 0.28 + mind.curiosity * 0.18
                - mind.stress * 0.10
                - mind.fatigue * 0.22)
                .clamp(0.42, 1.22),
            1.6,
            dt,
        );
        self.pose.droplet_cohesion = smooth(
            self.pose.droplet_cohesion,
            (traits.droplet_cohesion + mind.attachment * 0.10 + mind.fatigue * 0.08
                - mind.arousal * 0.08)
                .clamp(0.32, 0.96),
            1.5,
            dt,
        );
        debug_assert!(self.pose_is_finite());
    }

    #[must_use]
    pub fn metabolic_phase(&self) -> f32 {
        self.metabolic_phase
    }

    #[must_use]
    pub fn pose_is_finite(&self) -> bool {
        let pose = self.pose;
        [
            pose.pulse,
            pose.pulse_amplitude,
            pose.shell_opacity,
            pose.inner_density,
            pose.translucency,
            pose.flow_strength,
            pose.flow_speed,
            pose.flow_phase,
            pose.core_glow,
            pose.halo,
            pose.iris_activity,
            pose.eye_wetness,
            pose.droplet_energy,
            pose.droplet_spread,
            pose.droplet_cohesion,
        ]
        .into_iter()
        .all(f32::is_finite)
    }
}

fn finite_or(value: f32, fallback: f32) -> f32 {
    if value.is_finite() { value } else { fallback }
}

fn unit(value: f32) -> f32 {
    finite_or(value, 0.0).clamp(0.0, 1.0)
}

fn lerp(start: f32, end: f32, amount: f32) -> f32 {
    start + (end - start) * amount
}

fn smooth(current: f32, target: f32, speed: f32, dt: f32) -> f32 {
    current + (target - current) * (1.0 - (-speed * dt).exp())
}

#[cfg(test)]
mod tests {
    use lifecore::Genome;

    use super::*;

    fn run(
        traits: &DerivedVisualTraits,
        mind: VisualMindInput,
        seconds: f32,
    ) -> VisualPhysiologyRuntime {
        let mut runtime = VisualPhysiologyRuntime::new(42, traits);
        for _ in 0..(seconds * 120.0) as usize {
            runtime.update(traits, mind, 1.0 / 120.0);
        }
        runtime
    }

    #[test]
    fn physiology_is_deterministic_finite_and_bounded() {
        let traits = DerivedVisualTraits::from_genome(&Genome::from_seed(42));
        let mut mind = VisualMindInput {
            arousal: f32::NAN,
            local_luminance: f32::INFINITY,
            ..VisualMindInput::default()
        };
        mind.sanitize();
        let first = run(&traits, mind, 120.0);
        let second = run(&traits, mind, 120.0);
        assert_eq!(first, second);
        assert!(first.pose_is_finite());
        assert!((0.90..=0.995).contains(&first.pose.shell_opacity));
        assert!((0.55..=0.92).contains(&first.pose.inner_density));
        assert!((0.12..=0.55).contains(&first.pose.translucency));
    }

    #[test]
    fn state_changes_have_causal_visual_signatures() {
        let traits = DerivedVisualTraits::from_genome(&Genome::from_seed(7));
        let calm = run(&traits, VisualMindInput::default(), 8.0);
        let excited = run(
            &traits,
            VisualMindInput {
                arousal: 1.0,
                curiosity: 1.0,
                attachment: 1.0,
                ..VisualMindInput::default()
            },
            8.0,
        );
        let sleepy = run(
            &traits,
            VisualMindInput {
                fatigue: 1.0,
                ..VisualMindInput::default()
            },
            8.0,
        );
        assert!(
            excited.metabolic_phase() > calm.metabolic_phase()
                || excited.pose.flow_speed > calm.pose.flow_speed
        );
        assert!(excited.pose.flow_speed > sleepy.pose.flow_speed);
        assert!(excited.pose.core_glow > calm.pose.core_glow);
        assert!(sleepy.pose.translucency > calm.pose.translucency);
        assert!(sleepy.pose.droplet_energy < excited.pose.droplet_energy);
    }
}
