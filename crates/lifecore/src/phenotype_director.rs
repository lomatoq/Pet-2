//! Bounded brain-to-body phenotype coupling compiled from the R12 map.

use serde::{Deserialize, Serialize};

use crate::phenotype::bounded_signed;
use crate::{
    AnalyticRuntimeActuation, BodyActionIntent, CausalSourceTerm, CausalTargetRecord,
    EmbodimentSourceFrame, FaceRuntimeActuation, FastPhenotypeActuation, InteroceptionSnapshot,
    MaterialRuntimeActuation, PbfRuntimeActuation, PhraseContourWeights, VisualPhysiologyActuation,
    VoicePhenotypeActuation, asymmetric, unit,
};

/// Canonical R12 fast coupling identifiers. Keeping this list alongside the
/// executable formulas makes graph coverage machine-checkable without turning
/// the external parameter catalogue into a second runtime authority.
pub const R12_FAST_COUPLING_IDS: [&str; 28] = [
    "area_preserving_breathing_shape",
    "roundness_and_softness_expression",
    "softness_compliance",
    "viscosity_damping",
    "surface_tension_cohesion",
    "flight_lag_and_stretch",
    "motor_authority_and_balance",
    "shape_recovery_and_protection",
    "breath_and_pulse",
    "expressive_lean_and_idle_motion",
    "identity_preserving_color",
    "emission_and_soul_glow",
    "optical_density",
    "internal_flow",
    "internal_orb_activity",
    "droplet_behavior",
    "attention_face",
    "eye_aperture_blink_and_wetness",
    "conflict_asymmetry",
    "mouth_brow_affect",
    "physical_load_response",
    "localized_pain_protection",
    "apparent_size",
    "voice_prosody",
    "analytic_modal_response",
    "interaction_contact_policy",
    "relief_exhale_and_mouth_sync",
    "face_carrier_embodiment",
];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BodyPhenotypeDirector {
    output: FastPhenotypeActuation,
    #[serde(default = "default_true")]
    fast_couplings_enabled: bool,
}

impl Default for BodyPhenotypeDirector {
    fn default() -> Self {
        Self {
            output: FastPhenotypeActuation::default(),
            fast_couplings_enabled: true,
        }
    }
}

impl BodyPhenotypeDirector {
    pub fn set_fast_couplings_enabled(&mut self, enabled: bool) {
        self.fast_couplings_enabled = enabled;
    }

    #[must_use]
    pub fn tick(
        &mut self,
        source: &EmbodimentSourceFrame,
        interoception: InteroceptionSnapshot,
        dt: f32,
    ) -> FastPhenotypeActuation {
        let raw = raw_targets(source, interoception);
        if !self.fast_couplings_enabled {
            self.output = FastPhenotypeActuation {
                frame_id: source.frame_id,
                episode_id: source.episode.episode_id,
                ..FastPhenotypeActuation::default()
            };
            self.output.trace = build_trace(source, interoception, &raw, &self.output);
            for record in &mut self.output.trace {
                record.influence_budget_scale = 0.0;
                record.clamp_reason = "fast_couplings_disabled".to_owned();
            }
            return self.output.clone();
        }
        self.filter(&raw, dt.clamp(1.0 / 240.0, 0.25));
        self.output.schema_version = crate::NERVOUS_SYSTEM_SCHEMA_VERSION;
        self.output.frame_id = source.frame_id;
        self.output.episode_id = source.episode.episode_id;
        self.output.trace = build_trace(source, interoception, &raw, &self.output);
        self.output.clone()
    }

    fn filter(&mut self, t: &FastPhenotypeActuation, dt: f32) {
        macro_rules! f {
            ($section:ident.$name:ident, $rise:expr, $fall:expr) => {
                self.output.$section.$name = asymmetric(
                    self.output.$section.$name,
                    t.$section.$name,
                    $rise,
                    $fall,
                    dt,
                );
            };
            ($name:ident, $rise:expr, $fall:expr) => {
                self.output.$name = asymmetric(self.output.$name, t.$name, $rise, $fall, dt);
            };
        }

        f!(analytic.body_length_scale, 0.30, 1.10);
        f!(analytic.body_width_scale, 0.30, 1.10);
        f!(analytic.roundness_bias, 0.25, 1.20);
        f!(analytic.softness_bias, 0.25, 1.20);
        f!(analytic.modal_response_multiplier, 0.10, 0.90);
        f!(analytic.modal_frequency_multiplier, 0.10, 0.90);
        f!(analytic.modal_damping_multiplier, 0.10, 0.90);
        f!(analytic.modal_amplitude_multiplier, 0.10, 0.90);

        f!(pbf.density_compliance_multiplier, 0.20, 0.75);
        f!(pbf.viscosity_multiplier, 0.45, 1.40);
        f!(pbf.surface_tension_multiplier, 0.18, 0.90);
        f!(pbf.flight_inertia_multiplier, 0.16, 0.65);
        f!(pbf.flight_stretch_multiplier, 0.16, 0.65);
        f!(pbf.flight_damping_multiplier, 0.45, 1.40);
        f!(pbf.flight_max_lag_multiplier, 0.16, 0.65);
        f!(pbf.angular_damping_multiplier, 0.18, 0.85);
        f!(pbf.motor_gain_multiplier, 0.18, 0.85);
        f!(pbf.shape_recovery_delta, 0.08, 0.65);
        f!(pbf.upright_stabilization_delta, 0.18, 0.85);
        f!(pbf.idle_breath_amplitude_multiplier, 0.08, 0.90);
        f!(pbf.idle_breath_speed_multiplier, 0.08, 0.90);
        f!(pbf.idle_lean_angle_multiplier, 0.14, 0.75);
        f!(pbf.idle_lean_rate_multiplier, 0.14, 0.75);

        f!(material.hue_shift_turns, 0.55, 1.80);
        f!(material.saturation_delta, 0.55, 1.80);
        f!(material.value_delta, 0.55, 1.80);
        f!(material.emission_multiplier, 0.20, 0.95);
        f!(material.soul_glow_strength_multiplier, 0.20, 0.95);
        f!(material.soul_glow_pulse_multiplier, 0.20, 0.95);
        f!(material.opacity_multiplier, 0.50, 1.60);
        f!(material.translucency_delta, 0.50, 1.60);
        f!(material.internal_flow_multiplier, 0.28, 1.00);
        f!(material.caustic_speed_multiplier, 0.28, 1.00);
        f!(material.halo_multiplier, 0.20, 0.95);
        f!(material.bloom_multiplier, 0.20, 0.95);
        f!(material.internal_orb_speed_multiplier, 0.35, 1.30);
        f!(material.internal_orb_intensity_multiplier, 0.35, 1.30);

        f!(face.microsaccade_amount_multiplier, 0.12, 0.50);
        f!(face.microsaccade_rate_multiplier, 0.12, 0.50);
        f!(face.semantic_roll, 0.14, 0.60);
        f!(face.scale_multiplier, 0.10, 0.55);
        f!(face.blink_rate_multiplier, 0.08, 0.85);
        self.output.face.translation_offset = self.output.face.translation_offset.lerp(
            t.face.translation_offset,
            alpha(
                0.10,
                0.55,
                self.output.face.translation_offset.length(),
                t.face.translation_offset.length(),
                dt,
            ),
        );
        self.output.face.gaze_target = t.face.gaze_target;

        f!(visual_physiology.pulse_amplitude, 0.08, 0.90);
        f!(visual_physiology.inner_density_multiplier, 0.50, 1.60);
        f!(visual_physiology.shell_opacity_multiplier, 0.50, 1.60);
        f!(visual_physiology.translucency_delta, 0.50, 1.60);
        f!(visual_physiology.core_glow_multiplier, 0.20, 0.95);
        f!(visual_physiology.halo_multiplier, 0.20, 0.95);
        f!(visual_physiology.iris_activity, 0.08, 0.85);
        f!(visual_physiology.eye_wetness, 0.08, 0.85);
        f!(visual_physiology.flow_strength_multiplier, 0.28, 1.00);
        f!(visual_physiology.flow_speed_multiplier, 0.28, 1.00);
        f!(visual_physiology.droplet_energy, 0.18, 0.95);
        f!(visual_physiology.droplet_spread, 0.18, 0.95);
        f!(visual_physiology.droplet_cohesion, 0.18, 0.95);

        macro_rules! voice {
            ($name:ident) => {
                f!(voice.$name, 0.10, 1.10)
            };
        }
        voice!(pitch_multiplier);
        voice!(pitch_variation_multiplier);
        voice!(formant_scale_multiplier);
        voice!(breathiness_delta);
        voice!(roughness_delta);
        voice!(brightness_delta);
        voice!(phrase_speed_multiplier);
        voice!(attack_multiplier);
        voice!(release_multiplier);
        voice!(loudness_multiplier);
        voice!(purr_amount);
        voice!(trill_amount);
        voice!(call_probability);
        voice!(breath_phase_lock);
        voice!(effort_noise);
        self.output.voice.enabled = t.voice.enabled;
        self.output.voice.phrase_contour = t.voice.phrase_contour;

        macro_rules! action {
            ($name:ident) => {
                f!(action.$name, 0.12, 0.75)
            };
        }
        action!(approach);
        action!(avoid);
        action!(speed);
        action!(turn);
        action!(gaze_commitment);
        action!(explore);
        action!(play);
        action!(settle);
        action!(protect);
        action!(social_approach);
        action!(resist);
        action!(cooperate);
        action!(calibrate_body);
        action!(intentional_bud_request);
        action!(vocalize);

        f!(interaction.compliance_delta, 0.08, 0.55);
        f!(interaction.cohesion_delta, 0.08, 0.55);
        f!(interaction.local_pulse, 0.025, 0.55);
        f!(interaction.lean, 0.14, 0.75);
        f!(interaction.recoil, 0.025, 0.55);
        f!(interaction.resistance, 0.08, 0.55);
        f!(interaction.cooperation, 0.08, 0.55);
        self.output.interaction.target_component = t.interaction.target_component;
        self.output.interaction.allow_intentional_bud = t.interaction.allow_intentional_bud;
        self.output.interaction.sanitize();

        macro_rules! expression {
            ($name:ident, $r:expr, $f:expr) => {
                f!(expression.$name, $r, $f)
            };
        }
        expression!(squint, 0.08, 0.85);
        expression!(pupil_size, 0.12, 0.50);
        expression!(pupil_focus, 0.12, 0.50);
        expression!(brow_raise, 0.10, 0.75);
        expression!(brow_tension, 0.10, 0.75);
        expression!(mouth_open, 0.06, 0.75);
        expression!(mouth_curve, 0.10, 0.75);
        expression!(mouth_tension, 0.10, 0.75);
        expression!(cheek_glow, 0.10, 0.75);
        expression!(body_glow, 0.10, 0.75);
        expression!(eye_aperture, 0.08, 0.85);
        expression!(eye_scale, 0.12, 0.50);
        expression!(brow_asymmetry, 0.14, 0.60);
        expression!(mouth_compression, 0.08, 0.45);
        expression!(mouth_asymmetry, 0.14, 0.60);
        expression!(effort, 0.08, 0.45);
        expression!(relief, 0.06, 0.75);
        self.output.expression.blink_left = 0.0;
        self.output.expression.blink_right = 0.0;
        f!(apparent_scale, 0.45, 1.60);
    }
}

const fn default_true() -> bool {
    true
}

#[allow(clippy::field_reassign_with_default)]
fn raw_targets(source: &EmbodimentSourceFrame, i: InteroceptionSnapshot) -> FastPhenotypeActuation {
    let d = i.derived;
    let f = i.felt;
    let e = i.emotions;
    let morph = source.morph;
    let mut out = FastPhenotypeActuation {
        frame_id: source.frame_id,
        episode_id: source.episode.episode_id,
        ..FastPhenotypeActuation::default()
    };

    let length =
        (1.0 + 0.030 * d.arousal + 0.018 * d.curiosity - 0.040 * d.fatigue - 0.025 * d.stress)
            .clamp(0.94, 1.05);
    out.analytic = AnalyticRuntimeActuation {
        body_length_scale: length,
        body_width_scale: (1.0 / length).clamp(0.95, 1.06),
        roundness_bias: (0.040 * e.contentment + 0.030 * e.affection
            - 0.030 * e.fear
            - 0.018 * e.determination)
            .clamp(-0.05, 0.07),
        softness_bias: (0.055 * e.affection + 0.035 * e.contentment + 0.025 * d.fatigue
            - 0.065 * e.fear
            - 0.040 * e.protest)
            .clamp(-0.08, 0.09),
        modal_response_multiplier: (0.86 + 0.24 * d.arousal + 0.18 * e.playfulness
            - 0.18 * d.fatigue)
            .clamp(0.65, 1.28),
        modal_frequency_multiplier: (0.82 + 0.34 * d.arousal + 0.22 * f.startle - 0.20 * d.fatigue)
            .clamp(0.60, 1.40),
        modal_damping_multiplier: (1.0 + 0.24 * d.stress + 0.22 * d.fatigue + 0.12 * e.calm
            - 0.20 * e.playfulness)
            .clamp(0.72, 1.38),
        modal_amplitude_multiplier: (0.78
            + 0.32 * e.playfulness
            + 0.24 * d.arousal
            + 0.20 * f.startle
            - 0.22 * d.stress)
            .clamp(0.55, 1.42),
    };

    out.pbf = PbfRuntimeActuation {
        density_compliance_multiplier: (1.0 + 0.16 * d.social_warmth + 0.12 * d.fatigue
            - 0.22 * d.stress
            - 0.12 * d.neural_threat
            + 0.08 * d.neural_approach)
            .clamp(0.75, 1.25),
        viscosity_multiplier: (1.0 + 0.22 * d.fatigue + 0.10 * d.stress - 0.18 * d.arousal)
            .clamp(0.78, 1.30),
        surface_tension_multiplier: (1.0
            + 0.18 * d.stress
            + 0.14 * d.neural_threat
            + 0.08 * d.fatigue
            - 0.12 * source.drives.play
            - 0.08 * d.curiosity)
            .clamp(0.80, 1.25),
        flight_inertia_multiplier: (0.92 + 0.20 * d.fatigue + 0.10 * (1.0 - d.confidence))
            .clamp(0.85, 1.20),
        flight_stretch_multiplier: (0.90 + 0.22 * d.arousal + 0.12 * d.neural_novelty
            - 0.10 * d.stress)
            .clamp(0.85, 1.22),
        flight_damping_multiplier: (1.0 + 0.20 * d.fatigue + 0.14 * d.neural_rest
            - 0.12 * d.arousal)
            .clamp(0.82, 1.28),
        flight_max_lag_multiplier: (0.90 + 0.18 * d.arousal + 0.10 * d.fatigue).clamp(0.90, 1.18),
        angular_damping_multiplier: (1.0 + 0.18 * d.fatigue + 0.12 * e.fear
            - 0.12 * e.determination)
            .clamp(0.82, 1.30),
        motor_gain_multiplier: (0.88 + 0.14 * d.confidence + 0.10 * f.motor_efficacy
            - 0.15 * d.fatigue
            - 0.10 * e.fear)
            .clamp(0.72, 1.16),
        shape_recovery_delta: (0.18 * d.stress + 0.20 * e.fear + 0.22 * f.pain_like
            - 0.10 * e.playfulness)
            .clamp(0.0, 0.42),
        upright_stabilization_delta: (0.22 * e.determination
            + 0.12 * d.confidence
            + 0.10 * (1.0 - f.balance))
            .clamp(0.0, 0.34),
        idle_breath_amplitude_multiplier: (0.85
            + 0.32 * d.arousal
            + 0.18 * f.physical_load
            + 0.22 * f.startle
            - 0.16 * e.exhaustion)
            .clamp(0.65, 1.45),
        idle_breath_speed_multiplier: (0.78 + 0.48 * d.arousal + 0.30 * f.startle
            - 0.28 * d.fatigue)
            .clamp(0.55, 1.65),
        idle_lean_angle_multiplier: (0.85 + 0.25 * e.interest + 0.15 * e.affection
            - 0.18 * e.sadness)
            .clamp(0.65, 1.25),
        idle_lean_rate_multiplier: (0.80 + 0.35 * d.arousal + 0.18 * e.interest - 0.25 * d.fatigue)
            .clamp(0.55, 1.35),
    };

    out.material = MaterialRuntimeActuation {
        hue_shift_turns: (0.012 * d.positive_valence - 0.010 * d.negative_valence
            + 0.006 * d.neural_novelty)
            .clamp(-0.0222, 0.0222),
        saturation_delta: (0.06 * d.arousal + 0.04 * d.curiosity
            - 0.12 * d.stress
            - 0.05 * d.fatigue)
            .clamp(-0.14, 0.08),
        value_delta: (0.08 * d.positive_valence + 0.10 * d.arousal
            - 0.10 * d.fatigue
            - 0.06 * d.stress)
            .clamp(-0.12, 0.12),
        emission_multiplier: (0.82
            + 0.30 * d.arousal
            + 0.22 * d.social_warmth
            + 0.14 * d.positive_valence
            - 0.18 * d.fatigue)
            .clamp(0.70, 1.35),
        soul_glow_strength_multiplier: (0.85
            + 0.20 * d.social_warmth
            + 0.18 * d.positive_valence
            + 0.14 * d.arousal)
            .clamp(0.72, 1.32),
        soul_glow_pulse_multiplier: (0.70 + 0.40 * d.arousal + 0.18 * d.neural_novelty)
            .clamp(0.70, 1.30),
        opacity_multiplier: (1.0 + 0.08 * d.confidence + 0.10 * d.stress
            - 0.10 * d.fatigue
            - 0.08 * d.self_uncertainty)
            .clamp(0.86, 1.14),
        translucency_delta: (0.10 * d.fatigue + 0.06 * d.curiosity - 0.08 * d.stress)
            .clamp(-0.10, 0.12),
        internal_flow_multiplier: (0.80
            + 0.35 * d.arousal
            + 0.25 * d.curiosity
            + 0.18 * d.neural_novelty
            + 0.10 * d.stress
            - 0.20 * d.fatigue)
            .clamp(0.65, 1.35),
        caustic_speed_multiplier: (0.80 + 0.35 * d.arousal + 0.20 * d.neural_novelty
            - 0.25 * d.fatigue)
            .clamp(0.60, 1.35),
        halo_multiplier: (0.82 + 0.22 * e.affection + 0.20 * d.arousal + 0.14 * f.relief)
            .clamp(0.70, 1.30),
        bloom_multiplier: (0.86 + 0.24 * d.arousal + 0.20 * e.joy + 0.12 * e.affection
            - 0.12 * d.fatigue)
            .clamp(0.72, 1.32),
        internal_orb_speed_multiplier: (0.72 + 0.30 * d.neural_novelty + 0.24 * d.arousal
            - 0.20 * d.fatigue)
            .clamp(0.55, 1.30),
        internal_orb_intensity_multiplier: (0.82
            + 0.22 * d.attention_strength
            + 0.18 * d.neural_novelty
            + 0.12 * e.calm)
            .clamp(0.70, 1.30),
    };

    let confusion_asymmetry =
        (morph.turn * (0.18 + 0.42 * morph.conflict) + 0.12 * e.confusion).clamp(-0.55, 0.55);
    let focus = unit(
        0.35 + 0.40 * source.vita.attention_confidence
            + 0.20 * d.attention_strength
            + 0.15 * d.confidence,
    );
    let mut expression = crate::ExpressionState::default();
    expression.pupil_focus = focus;
    expression.pupil_size = (0.42 + 0.34 * d.arousal + 0.12 * d.neural_threat).clamp(0.32, 0.92);
    expression.eye_scale =
        (0.96 + 0.10 * e.interest + 0.08 * f.surprise - 0.08 * d.fatigue).clamp(0.88, 1.18);
    // One is the neutral aperture in the renderer and readability calibration
    // amplifies deviations around that neutral. A 0.72 base therefore made a
    // fully awake animal look chronically drowsy even at near-zero fatigue.
    expression.eye_aperture = (0.94 + 0.18 * f.surprise + 0.12 * e.interest + 0.16 * f.startle
        - 0.34 * d.fatigue
        - 0.14 * e.sadness)
        .clamp(0.28, 1.0);
    expression.squint = unit(0.15 + 0.42 * f.pain_like + 0.20 * e.protest + 0.12 * d.stress);
    expression.brow_asymmetry = confusion_asymmetry;
    expression.mouth_asymmetry = confusion_asymmetry;
    expression.mouth_curve = bounded_signed(
        0.65 * d.positive_valence + 0.22 * e.affection
            - 0.58 * d.negative_valence
            - 0.30 * e.protest,
    );
    expression.mouth_tension =
        unit(0.20 + 0.45 * e.protest + 0.30 * e.fear + 0.20 * e.determination);
    expression.brow_raise =
        bounded_signed(0.38 * f.surprise + 0.20 * e.interest - 0.35 * e.protest);
    expression.brow_tension =
        unit(0.18 + 0.42 * e.fear + 0.38 * e.protest + 0.22 * e.determination);
    expression.cheek_glow = unit(0.15 + 0.50 * e.affection + 0.25 * e.joy);
    expression.body_glow = unit(0.20 + 0.40 * e.joy + 0.25 * e.affection + 0.15 * e.determination);
    expression.effort = f.physical_load;
    expression.mouth_compression = unit(0.15 + 0.75 * f.physical_load);
    expression.relief = f.relief;
    expression.mouth_open = unit(
        0.46 * source.voice_feedback.envelope
            + 0.22 * f.surprise
            + 0.16 * f.startle
            + 0.16 * f.physical_load,
    );
    out.expression = expression;

    out.visual_physiology = VisualPhysiologyActuation {
        pulse_amplitude: unit(0.16 + 0.42 * d.arousal + 0.18 * f.startle + 0.12 * f.physical_load),
        inner_density_multiplier: out.material.opacity_multiplier,
        shell_opacity_multiplier: out.material.opacity_multiplier,
        translucency_delta: out.material.translucency_delta,
        core_glow_multiplier: out.material.soul_glow_strength_multiplier,
        halo_multiplier: out.material.halo_multiplier,
        iris_activity: unit(
            0.30 + 0.40 * d.attention_strength + 0.20 * e.interest + 0.10 * f.surprise,
        ),
        eye_wetness: unit(0.42 + 0.22 * e.sadness + 0.14 * d.stress + 0.10 * d.fatigue),
        flow_strength_multiplier: out.material.internal_flow_multiplier,
        flow_speed_multiplier: out.material.caustic_speed_multiplier,
        droplet_energy: unit(
            0.20 + 0.45 * e.playfulness + 0.28 * d.curiosity + 0.18 * d.arousal - 0.25 * d.fatigue,
        ),
        droplet_spread: unit(
            0.16 + 0.38 * e.playfulness + 0.22 * d.curiosity - 0.30 * e.fear - 0.22 * d.stress,
        ),
        droplet_cohesion: unit(
            0.55 + 0.25 * e.fear + 0.20 * d.stress + 0.15 * (1.0 - f.body_integrity)
                - 0.25 * e.playfulness,
        ),
    };

    out.face = FaceRuntimeActuation {
        gaze_target: if source.perception.selected_salience > 0.05 {
            Some(source.body.contact.point_world)
        } else {
            None
        },
        microsaccade_amount_multiplier: (1.0 + 0.20 * d.curiosity - 0.35 * d.attention_commitment)
            .clamp(0.55, 1.20),
        microsaccade_rate_multiplier: (0.90 + 0.25 * d.curiosity + 0.20 * e.anxiety
            - 0.35 * d.attention_commitment)
            .clamp(0.55, 1.30),
        semantic_roll: (morph.turn * (0.04 + 0.07 * e.confusion)).clamp(-0.12, 0.12),
        translation_offset: (0.18 * source.body.shape.center_of_mass_offset
            + 0.12 * glam::Vec2::new(morph.turn, 0.0))
        .clamp_length_max(0.035),
        scale_multiplier: (1.0 + 0.025 * f.surprise + 0.018 * e.interest
            - 0.022 * e.fear
            - 0.018 * f.effort)
            .clamp(0.95, 1.05),
        blink_rate_multiplier: (0.75 + 0.65 * d.fatigue + 0.20 * d.stress
            - 0.22 * d.attention_commitment)
            .clamp(0.55, 1.55),
    };

    let avoid = d.neural_threat * (1.0 - 0.45 * d.confidence);
    let explore = f.exploration_readiness;
    let play = f.play_readiness;
    let social_approach =
        unit(0.55 * e.affection + 0.25 * f.social_safety + 0.20 * source.drives.social);
    let settle = f.sleep_pressure.max(e.exhaustion).max(d.neural_rest);
    let protect = e.fear.max(f.pain_like) * f.vulnerability;
    let resist = unit(
        source.drives.autonomy.max(e.protest)
            * f.restraint
            * (0.45 + 0.55 * source.temperament.autonomy),
    );
    let mut speed = unit(
        (0.40 + 0.60 * unit(morph.winner_rate / 45.0))
            * (0.45 + 0.55 * morph.confidence)
            * (0.70 + 0.30 * d.arousal)
            * (1.0 - 0.55 * d.fatigue)
            * (1.0 - 0.45 * e.fear),
    );
    speed = unit(speed + 0.18 * explore * (0.5 + 0.5 * source.temperament.exploration_rate));
    speed *= 1.0 - 0.65 * settle;
    speed *= 1.0 - 0.38 * morph.conflict;
    let vocal_p = unit(
        source.temperament.vocality
            * (0.20
                + 0.28 * source.drives.social
                + 0.18 * e.joy
                + 0.18 * e.interest
                + 0.16 * e.affection)
            * (1.0 - 0.80 * e.fear),
    );
    let command_gate = semantic_command_rate(source, &["C_LISTEN", "C_PERK", "C_APPR"]);
    let vocalize = bool_value(
        !source.voice_feedback.phonating
            && deterministic_bernoulli(
                vocal_p * command_gate.max(0.35),
                source.episode.episode_id,
                source.voice_seed,
            ),
    );
    out.action = BodyActionIntent {
        approach: d.neural_approach * (1.0 - d.neural_threat),
        avoid,
        speed,
        turn: bounded_signed(
            morph.turn * (0.35 + 0.65 * morph.confidence)
                + morph.turn * (0.10 + 0.25 * e.confusion),
        ),
        gaze_commitment: d.attention_strength,
        explore,
        play,
        settle,
        protect,
        social_approach,
        resist,
        cooperate: social_approach * (1.0 - f.restraint),
        calibrate_body: unit(
            source.vita.calibration_urge * explore * f.body_integrity * (1.0 - d.stress),
        ),
        intentional_bud_request: play * source.body.topology.budget_remaining,
        vocalize,
    };

    let load_recoil = (0.08
        * d.neural_threat
        * source
            .body
            .motion
            .acceleration
            .length()
            .max(source.body.contact.pressure))
    .clamp(0.0, 0.08);
    let protect_recoil = (0.055 * f.startle + 0.035 * f.pain_like).min(0.08);
    let cooperation = unit(
        (0.55 * f.contact_pleasantness + 0.25 * f.social_safety + 0.20 * e.playfulness)
            * (1.0 - f.restraint)
            * (1.0 - e.protest),
    );
    out.interaction = crate::InteractionBodyActuation {
        target_component: source.body.contact.component_id,
        compliance_delta: (0.18 * f.contact_pleasantness + 0.10 * e.playfulness
            - 0.20 * f.pain_like
            - 0.14 * e.protest)
            .clamp(-0.25, 0.25),
        cohesion_delta: (0.18 * e.fear + 0.16 * f.pain_like + 0.10 * (1.0 - f.body_integrity)
            - 0.14 * e.playfulness)
            .clamp(-0.25, 0.25),
        local_pulse: (0.024 * f.pain_like).clamp(0.0, 0.03),
        lean: (morph.turn * (0.020 + 0.035 * morph.confidence)
            + 0.030 * e.interest
            + 0.018 * e.affection
            - 0.035 * e.fear)
            .clamp(-0.075, 0.075),
        recoil: load_recoil.max(protect_recoil).max(0.055 * f.pain_like),
        resistance: unit(
            (0.55 * e.protest + 0.45 * f.pain_like)
                .max(d.stress * f.physical_load)
                .max(resist),
        ),
        cooperation,
        allow_intentional_bud: out.action.intentional_bud_request > 0.62
            && cooperation > 0.60
            && source.body.topology.budget_remaining > 0.55
            && f.pain_like < 0.12,
    };
    out.interaction.sanitize();

    out.voice = VoicePhenotypeActuation {
        enabled: vocalize > 0.5 && e.fear < 0.82,
        pitch_multiplier: (1.0 + 0.07 * d.positive_valence + 0.10 * f.surprise + 0.08 * f.startle
            - 0.08 * e.sadness
            - 0.06 * d.fatigue
            - 0.05 * e.protest)
            .clamp(0.82, 1.18),
        pitch_variation_multiplier: (0.85 + 0.35 * d.arousal + 0.20 * e.interest
            - 0.20 * d.fatigue)
            .clamp(0.65, 1.35),
        formant_scale_multiplier: (1.0 + 0.025 * d.confidence - 0.030 * e.fear
            + 0.020 * e.affection)
            .clamp(0.94, 1.06),
        breathiness_delta: (0.18 * d.fatigue + 0.12 * e.fear + 0.10 * e.affection
            - 0.10 * e.protest)
            .clamp(-0.12, 0.24),
        roughness_delta: (0.20 * e.protest + 0.15 * f.physical_load + 0.10 * e.fear)
            .clamp(-0.05, 0.25),
        brightness_delta: (0.16 * e.joy + 0.12 * e.interest - 0.18 * e.sadness - 0.12 * d.fatigue)
            .clamp(-0.22, 0.20),
        phrase_speed_multiplier: (0.82 + 0.28 * d.arousal + 0.14 * e.joy
            - 0.24 * d.fatigue
            - 0.12 * e.sadness)
            .clamp(0.62, 1.28),
        attack_multiplier: (1.10 - 0.25 * f.startle - 0.12 * e.protest + 0.18 * d.fatigue)
            .clamp(0.62, 1.35),
        release_multiplier: (0.90 + 0.28 * e.affection + 0.25 * e.sadness + 0.18 * d.fatigue
            - 0.20 * f.startle)
            .clamp(0.65, 1.45),
        loudness_multiplier: (0.72 + 0.24 * d.arousal + 0.18 * e.determination + 0.12 * e.joy
            - 0.18 * e.fear
            - 0.22 * d.fatigue)
            .clamp(0.45, 1.10),
        purr_amount: unit(0.62 * e.affection + 0.38 * e.contentment),
        trill_amount: unit(0.46 * e.joy + 0.34 * e.playfulness + 0.20 * f.surprise),
        call_probability: vocal_p,
        phrase_contour: PhraseContourWeights {
            joy_rise: e.joy,
            sadness_fall: e.sadness,
            curiosity_question: e.interest,
            protest_firm: e.protest,
            calm_level: e.calm,
        },
        breath_phase_lock: unit(
            0.45 + 0.40 * f.relief + 0.25 * bool_value(source.voice_feedback.phonating)
                - 0.30 * f.startle,
        ),
        effort_noise: unit(0.55 * f.physical_load + 0.25 * e.protest + 0.20 * e.fear),
    };
    out.apparent_scale = (1.0 + 0.025 * d.confidence + 0.018 * d.positive_valence
        - 0.030 * d.stress
        - 0.025 * d.fatigue)
        .clamp(0.95, 1.04);
    out
}

fn build_trace(
    source: &EmbodimentSourceFrame,
    i: InteroceptionSnapshot,
    raw: &FastPhenotypeActuation,
    effective: &FastPhenotypeActuation,
) -> Vec<CausalTargetRecord> {
    let d = i.derived;
    let e = i.emotions;
    let f = i.felt;
    let mut records = Vec::with_capacity(64);
    macro_rules! add {
        ($path:literal, $base:expr, $raw:expr, $eff:expr, $lo:expr, $hi:expr, $source_path:literal, $source:expr) => {
            records.push(record(
                source,
                $path,
                $base,
                $raw,
                $eff,
                $lo,
                $hi,
                &[($source_path, $source)],
                None,
            ));
        };
    }
    add!(
        "analytic_runtime.body_length_scale",
        1.0,
        raw.analytic.body_length_scale,
        effective.analytic.body_length_scale,
        0.94,
        1.05,
        "derived.arousal",
        d.arousal
    );
    add!(
        "analytic_runtime.body_width_scale",
        1.0,
        raw.analytic.body_width_scale,
        effective.analytic.body_width_scale,
        0.95,
        1.06,
        "derived.fatigue",
        d.fatigue
    );
    add!(
        "analytic_runtime.roundness_bias",
        0.0,
        raw.analytic.roundness_bias,
        effective.analytic.roundness_bias,
        -0.05,
        0.07,
        "emotion.contentment",
        e.contentment
    );
    add!(
        "analytic_runtime.softness_bias",
        0.0,
        raw.analytic.softness_bias,
        effective.analytic.softness_bias,
        -0.08,
        0.09,
        "emotion.affection",
        e.affection
    );
    for (path, raw_value, value, lo, hi) in [
        (
            "pbf_runtime.density_compliance_multiplier",
            raw.pbf.density_compliance_multiplier,
            effective.pbf.density_compliance_multiplier,
            0.75,
            1.25,
        ),
        (
            "pbf_runtime.viscosity_multiplier",
            raw.pbf.viscosity_multiplier,
            effective.pbf.viscosity_multiplier,
            0.78,
            1.30,
        ),
        (
            "pbf_runtime.surface_tension_multiplier",
            raw.pbf.surface_tension_multiplier,
            effective.pbf.surface_tension_multiplier,
            0.80,
            1.25,
        ),
        (
            "pbf_runtime.flight_inertia_multiplier",
            raw.pbf.flight_inertia_multiplier,
            effective.pbf.flight_inertia_multiplier,
            0.85,
            1.20,
        ),
        (
            "pbf_runtime.flight_stretch_multiplier",
            raw.pbf.flight_stretch_multiplier,
            effective.pbf.flight_stretch_multiplier,
            0.85,
            1.22,
        ),
        (
            "pbf_runtime.flight_damping_multiplier",
            raw.pbf.flight_damping_multiplier,
            effective.pbf.flight_damping_multiplier,
            0.82,
            1.28,
        ),
        (
            "pbf_runtime.flight_max_lag_multiplier",
            raw.pbf.flight_max_lag_multiplier,
            effective.pbf.flight_max_lag_multiplier,
            0.90,
            1.18,
        ),
        (
            "pbf_runtime.angular_damping_multiplier",
            raw.pbf.angular_damping_multiplier,
            effective.pbf.angular_damping_multiplier,
            0.82,
            1.30,
        ),
        (
            "pbf_runtime.motor_gain_multiplier",
            raw.pbf.motor_gain_multiplier,
            effective.pbf.motor_gain_multiplier,
            0.72,
            1.16,
        ),
        (
            "pbf_runtime.shape_recovery_delta",
            raw.pbf.shape_recovery_delta,
            effective.pbf.shape_recovery_delta,
            0.0,
            0.42,
        ),
        (
            "pbf_runtime.upright_stabilization_delta",
            raw.pbf.upright_stabilization_delta,
            effective.pbf.upright_stabilization_delta,
            0.0,
            0.34,
        ),
        (
            "pbf_runtime.idle_breath_amplitude_multiplier",
            raw.pbf.idle_breath_amplitude_multiplier,
            effective.pbf.idle_breath_amplitude_multiplier,
            0.65,
            1.45,
        ),
        (
            "pbf_runtime.idle_breath_speed_multiplier",
            raw.pbf.idle_breath_speed_multiplier,
            effective.pbf.idle_breath_speed_multiplier,
            0.55,
            1.65,
        ),
    ] {
        records.push(record(
            source,
            path,
            if path.ends_with("delta") { 0.0 } else { 1.0 },
            raw_value,
            value,
            lo,
            hi,
            &[("derived.physical_load", f.physical_load)],
            None,
        ));
    }
    for (path, raw_value, value, lo, hi) in [
        (
            "material_runtime.hue_shift_turns",
            raw.material.hue_shift_turns,
            effective.material.hue_shift_turns,
            -0.0222,
            0.0222,
        ),
        (
            "material_runtime.saturation_delta",
            raw.material.saturation_delta,
            effective.material.saturation_delta,
            -0.14,
            0.08,
        ),
        (
            "material_runtime.value_delta",
            raw.material.value_delta,
            effective.material.value_delta,
            -0.12,
            0.12,
        ),
        (
            "material_runtime.emission_multiplier",
            raw.material.emission_multiplier,
            effective.material.emission_multiplier,
            0.70,
            1.35,
        ),
        (
            "material_runtime.opacity_multiplier",
            raw.material.opacity_multiplier,
            effective.material.opacity_multiplier,
            0.86,
            1.14,
        ),
        (
            "material_runtime.internal_flow_multiplier",
            raw.material.internal_flow_multiplier,
            effective.material.internal_flow_multiplier,
            0.65,
            1.35,
        ),
        (
            "material_runtime.halo_multiplier",
            raw.material.halo_multiplier,
            effective.material.halo_multiplier,
            0.70,
            1.30,
        ),
        (
            "material_runtime.bloom_multiplier",
            raw.material.bloom_multiplier,
            effective.material.bloom_multiplier,
            0.72,
            1.32,
        ),
    ] {
        records.push(record(
            source,
            path,
            if path.ends_with("delta") || path.ends_with("turns") {
                0.0
            } else {
                1.0
            },
            raw_value,
            value,
            lo,
            hi,
            &[("derived.arousal", d.arousal)],
            None,
        ));
    }
    for (path, raw_value, value, lo, hi) in [
        (
            "expression.pupil_focus",
            raw.expression.pupil_focus,
            effective.expression.pupil_focus,
            0.0,
            1.0,
        ),
        (
            "expression.eye_aperture",
            raw.expression.eye_aperture,
            effective.expression.eye_aperture,
            0.28,
            1.0,
        ),
        (
            "expression.mouth_curve",
            raw.expression.mouth_curve,
            effective.expression.mouth_curve,
            -1.0,
            1.0,
        ),
        (
            "expression.mouth_tension",
            raw.expression.mouth_tension,
            effective.expression.mouth_tension,
            0.0,
            1.0,
        ),
        (
            "expression.effort",
            raw.expression.effort,
            effective.expression.effort,
            0.0,
            1.0,
        ),
        (
            "expression.mouth_open",
            raw.expression.mouth_open,
            effective.expression.mouth_open,
            0.0,
            1.0,
        ),
    ] {
        records.push(record(
            source,
            path,
            0.0,
            raw_value,
            value,
            lo,
            hi,
            &[("voice_feedback.envelope", source.voice_feedback.envelope)],
            None,
        ));
    }
    for (path, raw_value, value, lo, hi) in [
        (
            "action_intent.approach",
            raw.action.approach,
            effective.action.approach,
            0.0,
            1.0,
        ),
        (
            "action_intent.avoid",
            raw.action.avoid,
            effective.action.avoid,
            0.0,
            1.0,
        ),
        (
            "action_intent.speed",
            raw.action.speed,
            effective.action.speed,
            0.0,
            1.0,
        ),
        (
            "action_intent.turn",
            raw.action.turn,
            effective.action.turn,
            -1.0,
            1.0,
        ),
        (
            "action_intent.explore",
            raw.action.explore,
            effective.action.explore,
            0.0,
            1.0,
        ),
        (
            "action_intent.play",
            raw.action.play,
            effective.action.play,
            0.0,
            1.0,
        ),
        (
            "action_intent.protect",
            raw.action.protect,
            effective.action.protect,
            0.0,
            1.0,
        ),
        (
            "action_intent.resist",
            raw.action.resist,
            effective.action.resist,
            0.0,
            1.0,
        ),
        (
            "action_intent.vocalize",
            raw.action.vocalize,
            effective.action.vocalize,
            0.0,
            1.0,
        ),
    ] {
        records.push(record(
            source,
            path,
            0.0,
            raw_value,
            value,
            lo,
            hi,
            &[("morph_output.confidence", source.morph.confidence)],
            None,
        ));
    }
    for (path, raw_value, value, lo, hi) in [
        (
            "voice_runtime.pitch_multiplier",
            raw.voice.pitch_multiplier,
            effective.voice.pitch_multiplier,
            0.82,
            1.18,
        ),
        (
            "voice_runtime.breathiness_delta",
            raw.voice.breathiness_delta,
            effective.voice.breathiness_delta,
            -0.12,
            0.24,
        ),
        (
            "voice_runtime.roughness_delta",
            raw.voice.roughness_delta,
            effective.voice.roughness_delta,
            -0.05,
            0.25,
        ),
        (
            "voice_runtime.brightness_delta",
            raw.voice.brightness_delta,
            effective.voice.brightness_delta,
            -0.22,
            0.20,
        ),
        (
            "voice_runtime.phrase_speed_multiplier",
            raw.voice.phrase_speed_multiplier,
            effective.voice.phrase_speed_multiplier,
            0.62,
            1.28,
        ),
        (
            "voice_runtime.loudness_multiplier",
            raw.voice.loudness_multiplier,
            effective.voice.loudness_multiplier,
            0.45,
            1.10,
        ),
    ] {
        records.push(record(
            source,
            path,
            if path.ends_with("delta") { 0.0 } else { 1.0 },
            raw_value,
            value,
            lo,
            hi,
            &[("felt.physical_load", f.physical_load)],
            None,
        ));
    }
    records.push(record(
        source,
        "interaction_body_actuation.local_pulse",
        0.0,
        raw.interaction.local_pulse,
        effective.interaction.local_pulse,
        0.0,
        0.03,
        &[("felt.pain_like", f.pain_like)],
        source.body.contact.component_id,
    ));
    records.push(record(
        source,
        "render_runtime.apparent_scale",
        1.0,
        raw.apparent_scale,
        effective.apparent_scale,
        0.95,
        1.04,
        &[("derived.confidence", d.confidence)],
        None,
    ));

    // Complete the trace for every scalar written by the immutable actuation
    // packet. Earlier records retain their most specific source term; these
    // entries cover the remaining effective targets with the compiled formula,
    // clamp, filter time constants and a representative causal source bundle.
    let common_sources = [
        ("derived.arousal", d.arousal),
        ("derived.stress", d.stress),
        ("derived.fatigue", d.fatigue),
        ("derived.confidence", d.confidence),
        ("derived.curiosity", d.curiosity),
        ("derived.neural_threat", d.neural_threat),
        ("derived.neural_novelty", d.neural_novelty),
        ("felt.physical_load", f.physical_load),
        ("felt.pain_like", f.pain_like),
        ("felt.startle", f.startle),
        ("emotion.joy", e.joy),
        ("emotion.fear", e.fear),
        ("emotion.affection", e.affection),
        ("emotion.protest", e.protest),
        ("morph_output.confidence", source.morph.confidence),
        ("voice_feedback.envelope", source.voice_feedback.envelope),
    ];
    macro_rules! ensure {
        ($path:expr, $base:expr, $raw:expr, $eff:expr, $lo:expr, $hi:expr) => {
            if !records.iter().any(|item| item.target_path == $path) {
                records.push(record(
                    source,
                    $path,
                    $base,
                    $raw,
                    $eff,
                    $lo,
                    $hi,
                    &common_sources,
                    None,
                ));
            }
        };
    }
    for (path, base, raw_value, value, lo, hi) in [
        (
            "analytic_runtime.modal_response_multiplier",
            1.0,
            raw.analytic.modal_response_multiplier,
            effective.analytic.modal_response_multiplier,
            0.65,
            1.28,
        ),
        (
            "analytic_runtime.modal_frequency_multiplier",
            1.0,
            raw.analytic.modal_frequency_multiplier,
            effective.analytic.modal_frequency_multiplier,
            0.60,
            1.40,
        ),
        (
            "analytic_runtime.modal_damping_multiplier",
            1.0,
            raw.analytic.modal_damping_multiplier,
            effective.analytic.modal_damping_multiplier,
            0.72,
            1.38,
        ),
        (
            "analytic_runtime.modal_amplitude_multiplier",
            1.0,
            raw.analytic.modal_amplitude_multiplier,
            effective.analytic.modal_amplitude_multiplier,
            0.55,
            1.42,
        ),
        (
            "pbf_runtime.idle_lean_angle_multiplier",
            1.0,
            raw.pbf.idle_lean_angle_multiplier,
            effective.pbf.idle_lean_angle_multiplier,
            0.65,
            1.25,
        ),
        (
            "pbf_runtime.idle_lean_rate_multiplier",
            1.0,
            raw.pbf.idle_lean_rate_multiplier,
            effective.pbf.idle_lean_rate_multiplier,
            0.55,
            1.35,
        ),
        (
            "material_runtime.soul_glow_strength_multiplier",
            1.0,
            raw.material.soul_glow_strength_multiplier,
            effective.material.soul_glow_strength_multiplier,
            0.72,
            1.32,
        ),
        (
            "material_runtime.soul_glow_pulse_multiplier",
            1.0,
            raw.material.soul_glow_pulse_multiplier,
            effective.material.soul_glow_pulse_multiplier,
            0.70,
            1.30,
        ),
        (
            "material_runtime.translucency_delta",
            0.0,
            raw.material.translucency_delta,
            effective.material.translucency_delta,
            -0.10,
            0.12,
        ),
        (
            "material_runtime.caustic_speed_multiplier",
            1.0,
            raw.material.caustic_speed_multiplier,
            effective.material.caustic_speed_multiplier,
            0.60,
            1.35,
        ),
        (
            "material_runtime.internal_orb_speed_multiplier",
            1.0,
            raw.material.internal_orb_speed_multiplier,
            effective.material.internal_orb_speed_multiplier,
            0.55,
            1.30,
        ),
        (
            "material_runtime.internal_orb_intensity_multiplier",
            1.0,
            raw.material.internal_orb_intensity_multiplier,
            effective.material.internal_orb_intensity_multiplier,
            0.70,
            1.30,
        ),
        (
            "face_runtime.microsaccade_amount_multiplier",
            1.0,
            raw.face.microsaccade_amount_multiplier,
            effective.face.microsaccade_amount_multiplier,
            0.55,
            1.20,
        ),
        (
            "face_runtime.microsaccade_rate_multiplier",
            1.0,
            raw.face.microsaccade_rate_multiplier,
            effective.face.microsaccade_rate_multiplier,
            0.55,
            1.30,
        ),
        (
            "face_runtime.semantic_roll",
            0.0,
            raw.face.semantic_roll,
            effective.face.semantic_roll,
            -0.12,
            0.12,
        ),
        (
            "face_runtime.translation_offset.x",
            0.0,
            raw.face.translation_offset.x,
            effective.face.translation_offset.x,
            -0.035,
            0.035,
        ),
        (
            "face_runtime.translation_offset.y",
            0.0,
            raw.face.translation_offset.y,
            effective.face.translation_offset.y,
            -0.035,
            0.035,
        ),
        (
            "face_runtime.scale_multiplier",
            1.0,
            raw.face.scale_multiplier,
            effective.face.scale_multiplier,
            0.95,
            1.05,
        ),
        (
            "face_runtime.blink_rate_multiplier",
            1.0,
            raw.face.blink_rate_multiplier,
            effective.face.blink_rate_multiplier,
            0.55,
            1.55,
        ),
        (
            "visual_physiology.pulse_amplitude",
            0.16,
            raw.visual_physiology.pulse_amplitude,
            effective.visual_physiology.pulse_amplitude,
            0.0,
            1.0,
        ),
        (
            "visual_physiology.inner_density_multiplier",
            1.0,
            raw.visual_physiology.inner_density_multiplier,
            effective.visual_physiology.inner_density_multiplier,
            0.86,
            1.14,
        ),
        (
            "visual_physiology.shell_opacity_multiplier",
            1.0,
            raw.visual_physiology.shell_opacity_multiplier,
            effective.visual_physiology.shell_opacity_multiplier,
            0.86,
            1.14,
        ),
        (
            "visual_physiology.translucency_delta",
            0.0,
            raw.visual_physiology.translucency_delta,
            effective.visual_physiology.translucency_delta,
            -0.10,
            0.12,
        ),
        (
            "visual_physiology.core_glow_multiplier",
            1.0,
            raw.visual_physiology.core_glow_multiplier,
            effective.visual_physiology.core_glow_multiplier,
            0.72,
            1.32,
        ),
        (
            "visual_physiology.halo_multiplier",
            1.0,
            raw.visual_physiology.halo_multiplier,
            effective.visual_physiology.halo_multiplier,
            0.70,
            1.30,
        ),
        (
            "visual_physiology.iris_activity",
            0.30,
            raw.visual_physiology.iris_activity,
            effective.visual_physiology.iris_activity,
            0.0,
            1.0,
        ),
        (
            "visual_physiology.eye_wetness",
            0.42,
            raw.visual_physiology.eye_wetness,
            effective.visual_physiology.eye_wetness,
            0.0,
            1.0,
        ),
        (
            "visual_physiology.flow_strength_multiplier",
            1.0,
            raw.visual_physiology.flow_strength_multiplier,
            effective.visual_physiology.flow_strength_multiplier,
            0.65,
            1.35,
        ),
        (
            "visual_physiology.flow_speed_multiplier",
            1.0,
            raw.visual_physiology.flow_speed_multiplier,
            effective.visual_physiology.flow_speed_multiplier,
            0.60,
            1.35,
        ),
        (
            "visual_physiology.droplet_energy",
            0.20,
            raw.visual_physiology.droplet_energy,
            effective.visual_physiology.droplet_energy,
            0.0,
            1.0,
        ),
        (
            "visual_physiology.droplet_spread",
            0.16,
            raw.visual_physiology.droplet_spread,
            effective.visual_physiology.droplet_spread,
            0.0,
            1.0,
        ),
        (
            "visual_physiology.droplet_cohesion",
            0.55,
            raw.visual_physiology.droplet_cohesion,
            effective.visual_physiology.droplet_cohesion,
            0.0,
            1.0,
        ),
    ] {
        ensure!(path, base, raw_value, value, lo, hi);
    }
    for (path, base, raw_value, value, lo, hi) in [
        (
            "voice_runtime.pitch_variation_multiplier",
            1.0,
            raw.voice.pitch_variation_multiplier,
            effective.voice.pitch_variation_multiplier,
            0.65,
            1.35,
        ),
        (
            "voice_runtime.formant_scale_multiplier",
            1.0,
            raw.voice.formant_scale_multiplier,
            effective.voice.formant_scale_multiplier,
            0.94,
            1.06,
        ),
        (
            "voice_runtime.attack_multiplier",
            1.0,
            raw.voice.attack_multiplier,
            effective.voice.attack_multiplier,
            0.62,
            1.35,
        ),
        (
            "voice_runtime.release_multiplier",
            1.0,
            raw.voice.release_multiplier,
            effective.voice.release_multiplier,
            0.65,
            1.45,
        ),
        (
            "voice_runtime.purr_amount",
            0.0,
            raw.voice.purr_amount,
            effective.voice.purr_amount,
            0.0,
            1.0,
        ),
        (
            "voice_runtime.trill_amount",
            0.0,
            raw.voice.trill_amount,
            effective.voice.trill_amount,
            0.0,
            1.0,
        ),
        (
            "voice_runtime.call_probability",
            0.0,
            raw.voice.call_probability,
            effective.voice.call_probability,
            0.0,
            1.0,
        ),
        (
            "voice_runtime.breath_phase_lock",
            0.0,
            raw.voice.breath_phase_lock,
            effective.voice.breath_phase_lock,
            0.0,
            1.0,
        ),
        (
            "voice_runtime.effort_noise",
            0.0,
            raw.voice.effort_noise,
            effective.voice.effort_noise,
            0.0,
            1.0,
        ),
        (
            "voice_runtime.enabled",
            0.0,
            bool_value(raw.voice.enabled),
            bool_value(effective.voice.enabled),
            0.0,
            1.0,
        ),
        (
            "voice_runtime.phrase_contour.joy_rise",
            0.0,
            raw.voice.phrase_contour.joy_rise,
            effective.voice.phrase_contour.joy_rise,
            0.0,
            1.0,
        ),
        (
            "voice_runtime.phrase_contour.sadness_fall",
            0.0,
            raw.voice.phrase_contour.sadness_fall,
            effective.voice.phrase_contour.sadness_fall,
            0.0,
            1.0,
        ),
        (
            "voice_runtime.phrase_contour.curiosity_question",
            0.0,
            raw.voice.phrase_contour.curiosity_question,
            effective.voice.phrase_contour.curiosity_question,
            0.0,
            1.0,
        ),
        (
            "voice_runtime.phrase_contour.protest_firm",
            0.0,
            raw.voice.phrase_contour.protest_firm,
            effective.voice.phrase_contour.protest_firm,
            0.0,
            1.0,
        ),
        (
            "voice_runtime.phrase_contour.calm_level",
            0.0,
            raw.voice.phrase_contour.calm_level,
            effective.voice.phrase_contour.calm_level,
            0.0,
            1.0,
        ),
    ] {
        ensure!(path, base, raw_value, value, lo, hi);
    }
    for (path, raw_value, value, lo, hi) in [
        (
            "action_intent.gaze_commitment",
            raw.action.gaze_commitment,
            effective.action.gaze_commitment,
            0.0,
            1.0,
        ),
        (
            "action_intent.settle",
            raw.action.settle,
            effective.action.settle,
            0.0,
            1.0,
        ),
        (
            "action_intent.social_approach",
            raw.action.social_approach,
            effective.action.social_approach,
            0.0,
            1.0,
        ),
        (
            "action_intent.cooperate",
            raw.action.cooperate,
            effective.action.cooperate,
            0.0,
            1.0,
        ),
        (
            "action_intent.calibrate_body",
            raw.action.calibrate_body,
            effective.action.calibrate_body,
            0.0,
            1.0,
        ),
        (
            "action_intent.intentional_bud_request",
            raw.action.intentional_bud_request,
            effective.action.intentional_bud_request,
            0.0,
            1.0,
        ),
        (
            "interaction_body_actuation.compliance_delta",
            raw.interaction.compliance_delta,
            effective.interaction.compliance_delta,
            -0.25,
            0.25,
        ),
        (
            "interaction_body_actuation.cohesion_delta",
            raw.interaction.cohesion_delta,
            effective.interaction.cohesion_delta,
            -0.25,
            0.25,
        ),
        (
            "interaction_body_actuation.lean",
            raw.interaction.lean,
            effective.interaction.lean,
            -0.08,
            0.08,
        ),
        (
            "interaction_body_actuation.recoil",
            raw.interaction.recoil,
            effective.interaction.recoil,
            0.0,
            0.08,
        ),
        (
            "interaction_body_actuation.resistance",
            raw.interaction.resistance,
            effective.interaction.resistance,
            0.0,
            1.0,
        ),
        (
            "interaction_body_actuation.cooperation",
            raw.interaction.cooperation,
            effective.interaction.cooperation,
            0.0,
            1.0,
        ),
        (
            "interaction_body_actuation.allow_intentional_bud",
            bool_value(raw.interaction.allow_intentional_bud),
            bool_value(effective.interaction.allow_intentional_bud),
            0.0,
            1.0,
        ),
    ] {
        ensure!(path, 0.0, raw_value, value, lo, hi);
    }
    if !records
        .iter()
        .any(|item| item.target_path == "interaction_body_actuation.target_component")
    {
        records.push(record(
            source,
            "interaction_body_actuation.target_component",
            -1.0,
            raw.interaction.target_component.map_or(-1.0, f32::from),
            effective
                .interaction
                .target_component
                .map_or(-1.0, f32::from),
            -1.0,
            3.0,
            &common_sources,
            source.body.contact.component_id,
        ));
    }
    for (path, base, raw_value, value, lo, hi) in [
        (
            "expression.squint",
            0.0,
            raw.expression.squint,
            effective.expression.squint,
            0.0,
            1.0,
        ),
        (
            "expression.pupil_size",
            0.5,
            raw.expression.pupil_size,
            effective.expression.pupil_size,
            0.32,
            0.92,
        ),
        (
            "expression.brow_raise",
            0.0,
            raw.expression.brow_raise,
            effective.expression.brow_raise,
            -1.0,
            1.0,
        ),
        (
            "expression.brow_tension",
            0.0,
            raw.expression.brow_tension,
            effective.expression.brow_tension,
            0.0,
            1.0,
        ),
        (
            "expression.cheek_glow",
            0.0,
            raw.expression.cheek_glow,
            effective.expression.cheek_glow,
            0.0,
            1.0,
        ),
        (
            "expression.body_glow",
            0.2,
            raw.expression.body_glow,
            effective.expression.body_glow,
            0.0,
            1.0,
        ),
        (
            "expression.eye_scale",
            1.0,
            raw.expression.eye_scale,
            effective.expression.eye_scale,
            0.88,
            1.18,
        ),
        (
            "expression.brow_asymmetry",
            0.0,
            raw.expression.brow_asymmetry,
            effective.expression.brow_asymmetry,
            -0.55,
            0.55,
        ),
        (
            "expression.mouth_compression",
            0.0,
            raw.expression.mouth_compression,
            effective.expression.mouth_compression,
            0.0,
            1.0,
        ),
        (
            "expression.mouth_asymmetry",
            0.0,
            raw.expression.mouth_asymmetry,
            effective.expression.mouth_asymmetry,
            -0.55,
            0.55,
        ),
        (
            "expression.relief",
            0.0,
            raw.expression.relief,
            effective.expression.relief,
            0.0,
            1.0,
        ),
    ] {
        ensure!(path, base, raw_value, value, lo, hi);
    }
    records
}

#[allow(clippy::too_many_arguments)]
fn record(
    source: &EmbodimentSourceFrame,
    target_path: &str,
    base: f32,
    raw: f32,
    effective: f32,
    lo: f32,
    hi: f32,
    source_terms: &[(&str, f32)],
    component: Option<u8>,
) -> CausalTargetRecord {
    let (coupling_id, formula, rise_tau_seconds, fall_tau_seconds) = trace_metadata(target_path);
    CausalTargetRecord {
        frame_id: source.frame_id,
        episode_id: source.episode.episode_id,
        target_path: target_path.to_owned(),
        coupling_id: coupling_id.to_owned(),
        formula: formula.to_owned(),
        source_terms: source_terms
            .iter()
            .map(|(path, value)| CausalSourceTerm {
                path: (*path).to_owned(),
                value: *value,
            })
            .collect(),
        identity_or_profile_base: base,
        raw_target: raw,
        influence_budget_scale: 1.0,
        clamp_min: lo,
        clamp_max: hi,
        clamp_reason: "r12_safe_runtime_bound".to_owned(),
        filtered_value: effective,
        effective_value: effective,
        persistence_class: "fast_state_ephemeral".to_owned(),
        rise_tau_seconds,
        fall_tau_seconds,
        component_id_if_local: component,
    }
}

fn trace_metadata(target: &str) -> (&'static str, &'static str, f32, f32) {
    match target {
        "analytic_runtime.body_length_scale" | "analytic_runtime.body_width_scale" => (
            "area_preserving_breathing_shape",
            "length=clamp(1+0.030*arousal+0.018*curiosity-0.040*fatigue-0.025*stress,0.94,1.05); width=clamp(1/length,0.95,1.06)",
            0.30,
            1.10,
        ),
        "analytic_runtime.roundness_bias" | "analytic_runtime.softness_bias" => (
            "roundness_and_softness_expression",
            "roundness=clamp(0.040*contentment+0.030*affection-0.030*fear-0.018*determination,-0.05,0.07); softness=clamp(0.055*affection+0.035*contentment+0.025*fatigue-0.065*fear-0.040*protest,-0.08,0.09)",
            0.25,
            1.20,
        ),
        path if path.starts_with("analytic_runtime.modal_") => (
            "analytic_modal_response",
            "response=clamp(0.86+0.24*arousal+0.18*playfulness-0.18*fatigue,0.65,1.28); frequency=clamp(0.82+0.34*arousal+0.22*startle-0.20*fatigue,0.60,1.40); damping=clamp(1+0.24*stress+0.22*fatigue+0.12*calm-0.20*playfulness,0.72,1.38); amplitude=clamp(0.78+0.32*playfulness+0.24*arousal+0.20*startle-0.22*stress,0.55,1.42)",
            0.10,
            0.90,
        ),
        "pbf_runtime.density_compliance_multiplier" => (
            "softness_compliance",
            "clamp(1+0.16*social_warmth+0.12*fatigue-0.22*stress-0.12*neural_threat+0.08*neural_approach,0.75,1.25)",
            0.20,
            0.75,
        ),
        "pbf_runtime.viscosity_multiplier" | "pbf_runtime.flight_damping_multiplier" => (
            "viscosity_damping",
            "viscosity=clamp(1+0.22*fatigue+0.10*stress-0.18*arousal,0.78,1.30); damping=clamp(1+0.20*fatigue+0.14*neural_rest-0.12*arousal,0.82,1.28)",
            0.45,
            1.40,
        ),
        "pbf_runtime.surface_tension_multiplier" | "visual_physiology.droplet_cohesion" => (
            "surface_tension_cohesion",
            "clamp(1+0.18*stress+0.14*neural_threat+0.08*fatigue-0.12*drives.play-0.08*curiosity,0.80,1.25)",
            0.18,
            0.90,
        ),
        "pbf_runtime.flight_inertia_multiplier"
        | "pbf_runtime.flight_stretch_multiplier"
        | "pbf_runtime.flight_max_lag_multiplier" => (
            "flight_lag_and_stretch",
            "inertia=clamp(0.92+0.20*fatigue+0.10*(1-confidence),0.85,1.20); stretch=clamp(0.90+0.22*arousal+0.12*neural_novelty-0.10*stress,0.85,1.22); max_lag=clamp(0.90+0.18*arousal+0.10*fatigue,0.90,1.18)",
            0.16,
            0.65,
        ),
        "pbf_runtime.motor_gain_multiplier"
        | "pbf_runtime.angular_damping_multiplier"
        | "pbf_runtime.upright_stabilization_delta" => (
            "motor_authority_and_balance",
            "motor=clamp(0.88+0.14*confidence+0.10*motor_efficacy-0.15*fatigue-0.10*fear,0.72,1.16); angular=clamp(1+0.18*fatigue+0.12*fear-0.12*determination,0.82,1.30); upright=clamp(0.22*determination+0.12*confidence+0.10*(1-balance),0,0.34)",
            0.18,
            0.85,
        ),
        "pbf_runtime.shape_recovery_delta" | "interaction_body_actuation.resistance" => (
            "shape_recovery_and_protection",
            "shape_recovery=clamp(0.18*stress+0.20*fear+0.22*pain_like-0.10*playfulness,0,0.42); resistance=max(clamp01(0.55*protest+0.45*pain_like),stress*physical_load,autonomy_restraint)",
            0.08,
            0.65,
        ),
        "pbf_runtime.idle_breath_amplitude_multiplier"
        | "pbf_runtime.idle_breath_speed_multiplier"
        | "visual_physiology.pulse_amplitude" => (
            "breath_and_pulse",
            "amplitude=clamp(0.85+0.32*arousal+0.18*physical_load+0.22*startle-0.16*exhaustion,0.65,1.45); speed=clamp(0.78+0.48*arousal+0.30*startle-0.28*fatigue,0.55,1.65); pulse=clamp01(0.16+0.42*arousal+0.18*startle+0.12*physical_load)",
            0.08,
            0.90,
        ),
        "interaction_body_actuation.lean"
        | "pbf_runtime.idle_lean_angle_multiplier"
        | "pbf_runtime.idle_lean_rate_multiplier" => (
            "expressive_lean_and_idle_motion",
            "lean=clamp(morph.turn*(0.020+0.035*morph.confidence)+0.030*interest+0.018*affection-0.035*fear,-0.075,0.075); angle=clamp(0.85+0.25*interest+0.15*affection-0.18*sadness,0.65,1.25); rate=clamp(0.80+0.35*arousal+0.18*interest-0.25*fatigue,0.55,1.35)",
            0.14,
            0.75,
        ),
        "material_runtime.hue_shift_turns"
        | "material_runtime.saturation_delta"
        | "material_runtime.value_delta" => (
            "identity_preserving_color",
            "hue=clamp(0.012*positive_valence-0.010*negative_valence+0.006*neural_novelty,-0.0222,0.0222); saturation=clamp(0.06*arousal+0.04*curiosity-0.12*stress-0.05*fatigue,-0.14,0.08); value=clamp(0.08*positive_valence+0.10*arousal-0.10*fatigue-0.06*stress,-0.12,0.12)",
            0.55,
            1.80,
        ),
        "material_runtime.emission_multiplier"
        | "material_runtime.soul_glow_strength_multiplier"
        | "material_runtime.soul_glow_pulse_multiplier"
        | "material_runtime.halo_multiplier"
        | "material_runtime.bloom_multiplier"
        | "visual_physiology.core_glow_multiplier"
        | "visual_physiology.halo_multiplier" => (
            "emission_and_soul_glow",
            "bounded R12 emission, social glow, pulse, halo and bloom weighted sums of arousal, valence, affection, relief and fatigue",
            0.20,
            0.95,
        ),
        "material_runtime.opacity_multiplier"
        | "material_runtime.translucency_delta"
        | "visual_physiology.inner_density_multiplier"
        | "visual_physiology.shell_opacity_multiplier"
        | "visual_physiology.translucency_delta" => (
            "optical_density",
            "opacity=clamp(1+0.08*confidence+0.10*stress-0.10*fatigue-0.08*self_uncertainty,0.86,1.14); translucency=clamp(0.10*fatigue+0.06*curiosity-0.08*stress,-0.10,0.12)",
            0.50,
            1.60,
        ),
        "material_runtime.internal_flow_multiplier"
        | "material_runtime.caustic_speed_multiplier"
        | "visual_physiology.flow_strength_multiplier"
        | "visual_physiology.flow_speed_multiplier" => (
            "internal_flow",
            "strength=clamp(0.80+0.35*arousal+0.25*curiosity+0.18*neural_novelty+0.10*stress-0.20*fatigue,0.65,1.35); speed=clamp(0.80+0.35*arousal+0.20*neural_novelty-0.25*fatigue,0.60,1.35)",
            0.28,
            1.00,
        ),
        "material_runtime.internal_orb_speed_multiplier"
        | "material_runtime.internal_orb_intensity_multiplier" => (
            "internal_orb_activity",
            "speed=clamp(0.72+0.30*neural_novelty+0.24*arousal-0.20*fatigue,0.55,1.30); intensity=clamp(0.82+0.22*attention_strength+0.18*neural_novelty+0.12*calm,0.70,1.30)",
            0.35,
            1.30,
        ),
        "visual_physiology.droplet_energy" | "visual_physiology.droplet_spread" => (
            "droplet_behavior",
            "energy=clamp01(0.20+0.45*playfulness+0.28*curiosity+0.18*arousal-0.25*fatigue); spread=clamp01(0.16+0.38*playfulness+0.22*curiosity-0.30*fear-0.22*stress)",
            0.18,
            0.95,
        ),
        "expression.pupil_focus"
        | "expression.pupil_size"
        | "expression.eye_scale"
        | "face_runtime.microsaccade_amount_multiplier"
        | "face_runtime.microsaccade_rate_multiplier" => (
            "attention_face",
            "bounded attention confidence/commitment, arousal, threat, curiosity and fatigue face carrier formula",
            0.12,
            0.50,
        ),
        "expression.eye_aperture"
        | "expression.squint"
        | "face_runtime.blink_rate_multiplier"
        | "visual_physiology.eye_wetness"
        | "visual_physiology.iris_activity" => (
            "eye_aperture_blink_and_wetness",
            "bounded surprise, interest, startle, pain, protest, stress, sadness, fatigue and attention eye formula",
            0.08,
            0.85,
        ),
        "expression.brow_asymmetry"
        | "expression.mouth_asymmetry"
        | "face_runtime.semantic_roll" => (
            "conflict_asymmetry",
            "asymmetry=clamp(morph.turn*(0.18+0.42*morph.conflict)+0.12*confusion,-0.55,0.55); roll=clamp(morph.turn*(0.04+0.07*confusion),-0.12,0.12)",
            0.14,
            0.60,
        ),
        "expression.mouth_curve"
        | "expression.mouth_tension"
        | "expression.brow_raise"
        | "expression.brow_tension"
        | "expression.cheek_glow"
        | "expression.body_glow" => (
            "mouth_brow_affect",
            "bounded R12 mouth, brow and glow weighted sums of valence, affection, protest, fear, determination and joy",
            0.10,
            0.75,
        ),
        "expression.effort" | "expression.mouth_compression" => (
            "physical_load_response",
            "effort=physical_load; compression=clamp01(0.15+0.75*physical_load)",
            0.08,
            0.45,
        ),
        "interaction_body_actuation.target_component"
        | "interaction_body_actuation.local_pulse"
        | "interaction_body_actuation.recoil"
        | "action_intent.protect" => (
            "localized_pain_protection",
            "target=contact_or_strained_component; local_pulse=clamp(0.024*pain_like,0,0.03); recoil=max(load_recoil,startle,pain); protect=max(fear,pain_like)*vulnerability",
            0.025,
            0.55,
        ),
        "render_runtime.apparent_scale" => (
            "apparent_size",
            "clamp(1+0.025*confidence+0.018*positive_valence-0.030*stress-0.025*fatigue,0.95,1.04)",
            0.45,
            1.60,
        ),
        path if path.starts_with("voice_runtime.") => (
            "voice_prosody",
            "bounded R12 prosody/phonation formula from valence, arousal, felt load, startle, fatigue, social emotion and heard VoiceFeedbackV1",
            0.10,
            1.10,
        ),
        "interaction_body_actuation.compliance_delta"
        | "interaction_body_actuation.cohesion_delta"
        | "interaction_body_actuation.cooperation"
        | "interaction_body_actuation.allow_intentional_bud" => (
            "interaction_contact_policy",
            "bounded contact pleasantness/playfulness/pain/protest/integrity policy; budding also requires topology budget and low pain",
            0.08,
            0.55,
        ),
        "expression.relief" | "expression.mouth_open" => (
            "relief_exhale_and_mouth_sync",
            "relief_expression=relief; mouth_open=clamp01(0.46*voice_feedback.envelope+0.22*surprise+0.16*startle+0.16*physical_load)",
            0.06,
            0.75,
        ),
        path if path.starts_with("face_runtime.translation_offset")
            || path == "face_runtime.scale_multiplier" =>
        {
            (
                "face_carrier_embodiment",
                "translation=clamp2(0.18*body.center_of_mass_offset+0.12*[morph.turn,0],0.035); scale=clamp(1+0.025*surprise+0.018*interest-0.022*fear-0.018*effort,0.95,1.05)",
                0.10,
                0.55,
            )
        }
        path if path.starts_with("action_intent.") => (
            "brain_to_action_closed_loop",
            "bounded semantic Morph command, derived appraisal, felt-state and homeostatic action formula",
            0.12,
            0.75,
        ),
        _ => (
            "r12_bounded_fast_target",
            "compiled bounded R12 target formula",
            0.12,
            0.75,
        ),
    }
}

fn semantic_command_rate(source: &EmbodimentSourceFrame, names: &[&str]) -> f32 {
    names
        .iter()
        .filter_map(|name| source.morph.command_rate(name))
        .fold(0.0, f32::max)
}

fn deterministic_bernoulli(p: f32, episode_id: u64, voice_seed: u64) -> bool {
    let mut x = episode_id ^ voice_seed ^ 0x766f_6361_6c5f_6269;
    x = x.wrapping_add(0x9e37_79b9_7f4a_7c15);
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^= x >> 31;
    let sample = ((x >> 40) as u32) as f32 / 0x00ff_ffff as f32;
    sample < unit(p)
}

fn bool_value(value: bool) -> f32 {
    if value { 1.0 } else { 0.0 }
}

fn alpha(rise: f32, fall: f32, current: f32, target: f32, dt: f32) -> f32 {
    let tau = if target > current { rise } else { fall };
    1.0 - (-dt / tau).exp()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BodyInteroceptionDirector, Genome, VitaSomaticFrame};

    fn source() -> EmbodimentSourceFrame {
        let genome = Genome::from_seed(42);
        EmbodimentSourceFrame {
            frame_id: 7,
            affect: crate::AffectState::default(),
            drives: crate::Drives::initial(&genome.temperament),
            temperament: genome.temperament,
            voice_seed: genome.voice.voice_seed,
            vita: VitaSomaticFrame::default(),
            morph: crate::MorphNervousSystemFrame::default(),
            body: crate::BodyFeedbackV2::default(),
            voice_feedback: crate::VoiceFeedbackV1::default(),
            gesture: crate::GestureFrameV1::default(),
            episode: crate::EpisodeContextV1 {
                episode_id: 11,
                ..Default::default()
            },
            perception: crate::PerceptionSelectionV1::default(),
            soft_touch_pressure_max: 0.42,
        }
    }

    #[test]
    fn runtime_targets_are_bounded_and_traceable() {
        let s = source();
        let mut interoception = BodyInteroceptionDirector::default();
        let i = interoception.tick(&s, 0.05);
        let mut director = BodyPhenotypeDirector::default();
        let output = director.tick(&s, i, 0.05);
        assert!(output.is_finite());
        assert!(output.material.hue_shift_turns.abs() <= 0.0222);
        assert!(output.trace.len() >= 100);
        let unique = output
            .trace
            .iter()
            .map(|record| record.target_path.as_str())
            .collect::<std::collections::HashSet<_>>();
        assert_eq!(unique.len(), output.trace.len());
        assert!(
            output
                .trace
                .iter()
                .all(|record| !record.source_terms.is_empty()
                    && !record.coupling_id.is_empty()
                    && !record.formula.is_empty()
                    && record.rise_tau_seconds > 0.0
                    && record.fall_tau_seconds > 0.0)
        );
        let coupling_ids = output
            .trace
            .iter()
            .map(|record| record.coupling_id.as_str())
            .collect::<std::collections::HashSet<_>>();
        assert!(
            R12_FAST_COUPLING_IDS
                .iter()
                .all(|coupling_id| coupling_ids.contains(coupling_id)),
            "every canonical R12 fast coupling must own at least one traced target"
        );
    }

    #[test]
    fn stress_never_changes_structural_solver_or_identity_fields() {
        let s = source();
        let mut interoception = BodyInteroceptionDirector::default();
        let i = interoception.tick(&s, 0.05);
        let mut director = BodyPhenotypeDirector::default();
        let output = director.tick(&s, i, 0.05);
        let serialized = serde_json::to_string(&output).expect("serialize phenotype");
        for forbidden in [
            "fixed_hz",
            "particle_count",
            "density_iterations",
            "maximum_speed",
            "maximum_loudness",
            "learning_rate",
            "identity_seed",
        ] {
            assert!(
                !serialized.contains(forbidden),
                "structural lock leaked: {forbidden}"
            );
        }
    }

    #[test]
    fn zero_delta_state_converges_to_identity_multipliers() {
        let mut s = source();
        s.affect = crate::AffectState {
            valence: 0.0,
            arousal: 0.0,
            stress: 0.0,
            confidence: 0.0,
            attachment: 0.0,
            frustration: 0.0,
        };
        s.drives.sleep = 0.0;
        s.vita.mood = crate::MoodState {
            baseline_valence: 0.0,
            baseline_arousal: 0.0,
            social_openness: 0.0,
            confidence: 0.0,
            fatigue: 0.0,
        };
        let i = InteroceptionSnapshot::default();
        let mut director = BodyPhenotypeDirector::default();
        let output = director.tick(&s, i, 0.05);
        assert!((0.94..=1.05).contains(&output.analytic.body_length_scale));
        assert!((0.70..=1.35).contains(&output.material.emission_multiplier));
        assert!(output.material.hue_shift_turns.abs() <= 0.0222);
    }

    #[test]
    fn awake_eyes_are_open_and_fatigue_remains_visibly_distinct() {
        let s = source();
        let awake = raw_targets(&s, InteroceptionSnapshot::default());
        let mut drowsy_state = InteroceptionSnapshot::default();
        drowsy_state.derived.fatigue = 1.0;
        let drowsy = raw_targets(&s, drowsy_state);

        assert!(awake.expression.eye_aperture >= 0.90);
        assert!(drowsy.expression.eye_aperture <= 0.65);
        assert!(
            awake.expression.eye_aperture - drowsy.expression.eye_aperture >= 0.30,
            "awake and drowsy eyelids must remain perceptually separable"
        );
    }

    #[test]
    fn disabling_fast_couplings_clears_residuals_to_exact_identity_base() {
        let s = source();
        let mut interoception = BodyInteroceptionDirector::default();
        let i = interoception.tick(&s, 0.05);
        let mut director = BodyPhenotypeDirector::default();
        let _ = director.tick(&s, i, 0.05);
        director.set_fast_couplings_enabled(false);
        let output = director.tick(&s, i, 0.05);
        let neutral = FastPhenotypeActuation::default();

        assert_eq!(output.analytic, neutral.analytic);
        assert_eq!(output.pbf, neutral.pbf);
        assert_eq!(output.material, neutral.material);
        assert_eq!(output.face, neutral.face);
        assert_eq!(output.visual_physiology, neutral.visual_physiology);
        assert_eq!(output.voice, neutral.voice);
        assert_eq!(output.action, neutral.action);
        assert_eq!(output.interaction, neutral.interaction);
        assert_eq!(output.expression, neutral.expression);
        assert_eq!(output.apparent_scale, 1.0);
        assert!(
            output
                .trace
                .iter()
                .all(|record| record.influence_budget_scale == 0.0)
        );
    }
}
