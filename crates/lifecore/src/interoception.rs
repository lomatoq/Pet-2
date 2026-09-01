//! Deterministic interoception graph compiled from the R12 nervous-system map.

use serde::{Deserialize, Serialize};

use crate::{
    DerivedNervousState, EmbodimentSourceFrame, EmotionReadouts, FeltStateV1,
    MORPH_POPULATION_COUNT, MorphPopulationFrame, asymmetric, unit,
};

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MorphSomaticInput {
    pub touch: f32,
    pub proximity: f32,
    pub motion: f32,
    pub vibration: f32,
    pub looming: f32,
    pub habituation: f32,
    pub novelty: f32,
    pub exploration: f32,
    pub positive_outcome: f32,
    pub negative_outcome: f32,
    pub attention: f32,
    pub rest: f32,
}

impl MorphSomaticInput {
    pub fn sanitize(&mut self) {
        for value in [
            &mut self.touch,
            &mut self.proximity,
            &mut self.motion,
            &mut self.vibration,
            &mut self.looming,
            &mut self.habituation,
            &mut self.novelty,
            &mut self.exploration,
            &mut self.positive_outcome,
            &mut self.negative_outcome,
            &mut self.attention,
            &mut self.rest,
        ] {
            *value = unit(*value);
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct InteroceptionSnapshot {
    pub source_frame_id: u64,
    pub source_episode_id: u64,
    pub derived: DerivedNervousState,
    pub felt: FeltStateV1,
    pub emotions: EmotionReadouts,
    pub morph_sensors: MorphSomaticInput,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
struct RollingSignal {
    mean: f32,
    variance: f32,
    initialized: bool,
}

impl RollingSignal {
    fn observe(&mut self, value: f32, dt: f32) -> f32 {
        let value = if value.is_finite() { value } else { 0.0 };
        if !self.initialized {
            self.mean = value;
            self.variance = 0.04;
            self.initialized = true;
            return 0.0;
        }
        // A 45 s baseline is slow enough to preserve events and fast enough to
        // adapt to individual Morph population scales.
        let alpha = 1.0 - (-dt.clamp(0.0, 0.25) / 45.0).exp();
        let delta = value - self.mean;
        self.mean += delta * alpha;
        self.variance += (delta * delta - self.variance) * alpha;
        (delta / self.variance.max(1.0e-4).sqrt()).clamp(-3.0, 3.0)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BodyInteroceptionDirector {
    baselines: [RollingSignal; MORPH_POPULATION_COUNT],
    previous_threat: f32,
    previous_stress: f32,
    previous_comfort: f32,
    previous_integrity: f32,
    previous_motor_efficacy: f32,
    previous_cursor_loom: f32,
    felt: FeltStateV1,
    emotions: EmotionReadouts,
    morph_sensors: MorphSomaticInput,
}

impl Default for BodyInteroceptionDirector {
    fn default() -> Self {
        Self {
            baselines: [RollingSignal::default(); MORPH_POPULATION_COUNT],
            previous_threat: 0.0,
            previous_stress: 0.0,
            previous_comfort: 0.0,
            previous_integrity: 1.0,
            previous_motor_efficacy: 0.5,
            previous_cursor_loom: 0.0,
            felt: FeltStateV1 {
                body_integrity: 1.0,
                body_ownership: 0.5,
                agency_match: 0.5,
                motor_efficacy: 0.5,
                balance: 0.5,
                social_safety: 0.4,
                ..FeltStateV1::default()
            },
            emotions: EmotionReadouts::default(),
            morph_sensors: MorphSomaticInput::default(),
        }
    }
}

impl BodyInteroceptionDirector {
    /// Consumes a complete immutable source frame. Callers double-buffer body
    /// frames so physics result N cannot influence cognition until N+1.
    #[must_use]
    pub fn tick(&mut self, source: &EmbodimentSourceFrame, dt: f32) -> InteroceptionSnapshot {
        let dt = dt.clamp(1.0 / 240.0, 0.25);
        let z = self.normalized_populations(source.morph.populations, dt);
        let d = derive_axes(source, z);
        let target = derive_felt(source, d, self, dt);
        self.filter_felt(target, dt);
        self.filter_emotions(emotion_targets(source, d, self.felt), dt);
        self.filter_morph_sensors(morph_targets(source, d, self.felt, self), dt);
        self.previous_threat = d.neural_threat;
        self.previous_stress = d.stress;
        self.previous_comfort = self.felt.comfort;
        self.previous_integrity = self.felt.body_integrity;
        self.previous_motor_efficacy = self.felt.motor_efficacy;
        self.previous_cursor_loom = source.body.environment.cursor_loom_rate;
        InteroceptionSnapshot {
            source_frame_id: source.frame_id,
            source_episode_id: source.episode.episode_id,
            derived: d,
            felt: self.felt,
            emotions: self.emotions,
            morph_sensors: self.morph_sensors,
        }
    }

    fn normalized_populations(&mut self, p: MorphPopulationFrame, dt: f32) -> MorphPopulationFrame {
        let values = p.values();
        MorphPopulationFrame::from_values(std::array::from_fn(|i| {
            self.baselines[i].observe(values[i], dt)
        }))
    }

    fn filter_felt(&mut self, target: FeltStateV1, dt: f32) {
        macro_rules! f {
            ($name:ident, $rise:expr, $fall:expr) => {
                self.felt.$name = asymmetric(self.felt.$name, target.$name, $rise, $fall, dt);
            };
        }
        f!(physical_load, 0.05, 0.60);
        f!(pain_like, 0.035, 1.20);
        f!(comfort, 0.35, 1.50);
        f!(body_integrity, 0.08, 0.90);
        f!(body_ownership, 0.20, 1.10);
        f!(agency_match, 0.06, 0.80);
        f!(motor_efficacy, 0.18, 1.10);
        f!(restraint, 0.08, 0.70);
        f!(balance, 0.16, 0.85);
        f!(activation, 0.12, 0.75);
        f!(social_safety, 0.25, 1.50);
        f!(contact_pleasantness, 0.08, 0.80);
        f!(vulnerability, 0.20, 1.60);
        f!(effort, 0.06, 0.75);
        f!(surprise, 0.04, 0.60);
        f!(startle, 0.035, 0.28);
        f!(relief, 0.08, 1.20);
        f!(boredom, 3.0, 15.0);
        f!(loneliness, 8.0, 45.0);
        f!(play_readiness, 0.25, 1.50);
        f!(exploration_readiness, 0.18, 1.0);
        f!(sleep_pressure, 0.6, 2.5);
        self.felt.sanitize();
    }

    fn filter_emotions(&mut self, t: EmotionReadouts, dt: f32) {
        macro_rules! f {
            ($name:ident, $rise:expr, $fall:expr) => {
                self.emotions.$name = asymmetric(self.emotions.$name, t.$name, $rise, $fall, dt);
            };
        }
        f!(calm, 0.7, 2.4);
        f!(contentment, 0.8, 3.0);
        f!(joy, 0.15, 1.2);
        f!(interest, 0.18, 1.0);
        f!(playfulness, 0.25, 1.5);
        f!(affection, 0.4, 2.5);
        f!(anticipation, 0.12, 0.8);
        f!(surprise, 0.04, 0.6);
        f!(startle, 0.035, 0.28);
        f!(fear, 0.06, 1.4);
        f!(anxiety, 1.5, 8.0);
        f!(protest, 0.2, 2.0);
        f!(sadness, 1.0, 6.0);
        f!(frustration, 0.35, 4.0);
        f!(confusion, 0.18, 1.8);
        f!(discomfort, 0.05, 1.2);
        f!(relief, 0.08, 1.2);
        f!(boredom, 3.0, 15.0);
        f!(loneliness, 8.0, 45.0);
        f!(determination, 0.45, 2.5);
        f!(exhaustion, 2.0, 12.0);
        f!(vulnerability, 0.3, 2.2);
    }

    fn filter_morph_sensors(&mut self, t: MorphSomaticInput, dt: f32) {
        macro_rules! f {
            ($name:ident, $rise:expr, $fall:expr) => {
                self.morph_sensors.$name =
                    asymmetric(self.morph_sensors.$name, t.$name, $rise, $fall, dt);
            };
        }
        f!(touch, 0.02, 0.18);
        f!(proximity, 0.04, 0.22);
        f!(motion, 0.03, 0.20);
        f!(vibration, 0.01, 0.25);
        f!(looming, 0.015, 0.35);
        f!(habituation, 0.8, 5.0);
        f!(novelty, 0.04, 0.75);
        f!(exploration, 0.12, 0.9);
        f!(positive_outcome, 0.03, 0.55);
        f!(negative_outcome, 0.025, 0.7);
        f!(attention, 0.03, 0.3);
        f!(rest, 0.6, 2.5);
        self.morph_sensors.sanitize();
    }
}

fn derive_axes(source: &EmbodimentSourceFrame, z: MorphPopulationFrame) -> DerivedNervousState {
    let affect = source.affect;
    let mood = source.vita.mood;
    let appraisal = source.vita.appraisal;
    let morph = source.morph;
    let drives = source.drives;
    DerivedNervousState {
        positive_valence: unit(
            affect
                .valence
                .max(mood.baseline_valence)
                .max(0.70 * morph.valence),
        ),
        negative_valence: unit(
            (-affect.valence)
                .max(-mood.baseline_valence)
                .max(-0.70 * morph.valence),
        ),
        arousal: unit(0.55 * affect.arousal + 0.25 * mood.baseline_arousal + 0.20 * morph.arousal),
        stress: unit(affect.stress.max(appraisal.threat).max(drives.safety)),
        fatigue: unit(mood.fatigue.max(drives.sleep)),
        confidence: unit(
            0.55 * affect.confidence
                + 0.25 * mood.confidence
                + 0.20 * source.vita.body_schema_confidence,
        ),
        curiosity: unit(
            0.45 * source.temperament.curiosity
                + 0.30 * drives.curiosity
                + 0.25 * appraisal.novelty,
        ),
        social_warmth: unit(
            0.45 * affect.attachment
                + 0.30 * mood.social_openness
                + 0.25 * source.temperament.sociability,
        ),
        neural_approach: unit(z.mbon_a - z.mbon_v + 0.5),
        neural_threat: unit(0.55 * unit(z.loom) + 0.25 * unit(z.vib) + 0.20 * unit(z.valn)),
        neural_novelty: unit(0.55 * unit(z.nov) + 0.25 * unit(z.exp) - 0.20 * unit(z.hab)),
        neural_rest: unit(z.rest),
        attention_strength: unit(
            0.55 * unit(z.att) + 0.25 * source.vita.attention_confidence + 0.20 * morph.confidence,
        ),
        attention_commitment: unit(source.vita.attention_commitment_remaining / 0.8),
        self_uncertainty: unit(source.vita.uncertainty),
        habituation: unit(z.hab),
    }
}

fn derive_felt(
    source: &EmbodimentSourceFrame,
    d: DerivedNervousState,
    previous: &BodyInteroceptionDirector,
    dt: f32,
) -> FeltStateV1 {
    let b = source.body;
    let a = source.vita.appraisal;
    let intent = b.efference_copy;
    let physical_load = unit(
        0.24 * b.contact.pressure
            + 0.20 * b.shape.deformation_energy
            + 0.24 * b.shape.maximum_strain
            + 0.10 * b.shape.neck_tension
            + 0.12 * (1.0 - b.topology.budget_remaining)
            + 0.32 * b.topology.detached_mass_fraction,
    );
    let integrity = unit(
        1.0 - (0.52 * b.topology.detached_mass_fraction
            + 0.20 * b.shape.maximum_strain
            + 0.12 * b.shape.neck_tension
            + 0.16 * (1.0 - b.topology.budget_remaining)),
    );
    let agency_match = unit(
        1.0 - 0.45 * (intent.intended_velocity - intent.actual_velocity).length()
            - 0.25 * (intent.intended_turn - intent.actual_turn).abs()
            - 0.30 * (intent.intended_shape_delta - intent.actual_shape_delta).length(),
    );
    let motor_efficacy = unit(
        0.45 * agency_match
            + 0.25 * a.controllability
            + 0.20 * d.confidence
            + 0.10 * (1.0 - source.vita.prediction_error),
    );
    let pain_stimulus = b
        .shape
        .maximum_strain
        .max(b.shape.neck_tension)
        .max(b.contact.pressure)
        .max(2.2 * b.topology.detached_mass_fraction);
    let pain_like =
        unit(smoothstep(0.52, 0.90, pain_stimulus) * (0.65 + 0.35 * (1.0 - a.controllability)));
    let pleasant = unit(
        soft_band(
            b.contact.pressure,
            0.03,
            source.soft_touch_pressure_max.max(0.031),
            0.10,
        ) * (1.0 - pain_like)
            * (0.35 + 0.65 * d.social_warmth)
            * (0.40 + 0.60 * a.expectedness),
    );
    let restraint = unit(
        b.contact.duration / 1.2
            * (intent.intended_velocity - intent.actual_velocity).length()
            * (0.45 + 0.55 * b.contact.pressure),
    );
    let comfort = unit(
        0.35 * (1.0 - source.drives.comfort)
            + 0.25 * (1.0 - physical_load)
            + 0.20 * pleasant
            + 0.10 * bool_value(b.motion.grounded)
            + 0.10 * (1.0 - d.stress),
    );
    let balance = unit(
        0.40 * (1.0 - b.shape.center_of_mass_offset.length())
            + 0.25 * (1.0 - b.shape.angular_velocity.abs())
            + 0.20 * bool_value(b.motion.grounded)
            + 0.15 * bool_value(b.motion.clinging),
    );
    let activation = unit(
        0.55 * d.arousal
            + 0.20 * b.fluid.slosh_energy
            + 0.15 * b.motion.velocity.length()
            + 0.10 * d.neural_novelty,
    );
    let social_safety = unit(
        0.35 * d.social_warmth
            + 0.20 * (1.0 - d.stress)
            + 0.15 * a.expectedness
            + 0.15 * a.controllability
            + 0.15 * pleasant,
    );
    let vulnerability = unit(
        0.35 * d.self_uncertainty
            + 0.25 * (1.0 - integrity)
            + 0.20 * (1.0 - balance)
            + 0.20 * source.vita.external_force_likelihood,
    );
    let effort = unit(
        0.62 * physical_load
            + 0.20 * intent.intended_velocity.length()
            + 0.18 * intent.intended_turn.abs(),
    );
    let surprise = unit(a.novelty * (1.0 - a.expectedness) * (0.40 + 0.60 * a.certainty));
    let threat_rise = rise(d.neural_threat, previous.previous_threat, dt);
    let loom_rise = rise(
        b.environment.cursor_loom_rate,
        previous.previous_cursor_loom,
        dt,
    );
    let startle = unit(0.52 * threat_rise + 0.28 * b.motion.collision_impulse + 0.20 * loom_rise);
    let relief = unit(
        0.45 * fall(d.stress, previous.previous_stress, dt)
            + 0.25 * fall(d.neural_threat, previous.previous_threat, dt)
            + 0.20 * rise(comfort, previous.previous_comfort, dt)
            + 0.10 * rise(integrity, previous.previous_integrity, dt),
    );
    let play_readiness = unit(
        source.temperament.playfulness
            * source.drives.play
            * (1.0 - d.stress)
            * (0.35 + 0.65 * d.positive_valence)
            * (0.40 + 0.60 * activation),
    );
    let exploration_readiness = unit(
        d.curiosity
            * (0.45 + 0.55 * d.neural_novelty)
            * (1.0 - 0.65 * d.stress)
            * (0.45 + 0.55 * motor_efficacy),
    );
    let sleep_pressure =
        unit(0.55 * source.drives.sleep + 0.35 * source.vita.mood.fatigue + 0.10 * d.neural_rest);
    let boredom = unit(
        (0.45 * source.drives.novelty + 0.35 * source.drives.play + 0.20 * d.habituation)
            * (1.0 - d.neural_novelty)
            * (1.0 - 0.65 * activation),
    );
    let lonely_time = unit(source.body.environment.seconds_since_interaction / 120.0);
    let loneliness = unit(
        source.drives.social
            * (0.40 + 0.60 * source.affect.attachment)
            * (0.35 + 0.65 * lonely_time),
    );
    let body_ownership = unit(
        0.40 * integrity
            + 0.30 * agency_match
            + 0.20 * source.vita.body_schema_confidence
            + 0.10 * (1.0 - source.vita.external_force_likelihood),
    );
    FeltStateV1 {
        physical_load,
        pain_like,
        comfort,
        body_integrity: integrity,
        body_ownership,
        agency_match,
        motor_efficacy,
        restraint,
        balance,
        activation,
        social_safety,
        contact_pleasantness: pleasant,
        vulnerability,
        effort,
        surprise,
        startle,
        relief,
        boredom,
        loneliness,
        play_readiness,
        exploration_readiness,
        sleep_pressure,
    }
}

fn emotion_targets(
    source: &EmbodimentSourceFrame,
    d: DerivedNervousState,
    f: FeltStateV1,
) -> EmotionReadouts {
    let a = source.vita.appraisal;
    let calm = unit((1.0 - d.arousal) * (1.0 - d.stress) * f.comfort * f.balance);
    let contentment =
        unit(d.positive_valence * (1.0 - 0.65 * d.arousal) * f.comfort * f.social_safety);
    let joy = unit(d.positive_valence * d.arousal * (0.35 + 0.65 * a.goal_congruence));
    let interest =
        unit(f.exploration_readiness * d.attention_strength * (1.0 - 0.55 * f.physical_load));
    let affection = unit(
        source.affect.attachment * a.social_relevance * f.social_safety * (1.0 - d.neural_threat),
    );
    let anticipation = unit(
        d.attention_strength
            * d.attention_commitment
            * a.expectedness
            * (0.35 + 0.65 * a.goal_congruence),
    );
    let fear = unit(
        d.neural_threat * d.arousal * (1.0 - a.controllability) * (0.45 + 0.55 * f.vulnerability),
    );
    let anxiety = unit(
        d.stress
            * d.self_uncertainty
            * (1.0 - 0.65 * a.controllability)
            * (0.45 + 0.55 * f.vulnerability),
    );
    let protest = unit(
        source
            .affect
            .frustration
            .max(f.restraint)
            .max(source.drives.autonomy)
            * d.arousal
            * (0.35 + 0.65 * a.agency)
            * (1.0 - 0.55 * d.fatigue),
    );
    let sadness = unit(
        d.negative_valence
            * (0.40 + 0.60 * d.fatigue)
            * (1.0 - 0.45 * d.arousal)
            * (0.45 + 0.55 * (1.0 - f.motor_efficacy)),
    );
    let frustration = unit(
        source.affect.frustration
            * (0.45 + 0.55 * (1.0 - f.motor_efficacy))
            * (0.55 + 0.45 * d.arousal),
    );
    let confusion = unit(source.morph.conflict * d.self_uncertainty * (0.35 + 0.65 * f.surprise));
    let determination = unit(
        d.confidence
            * source.temperament.persistence
            * a.goal_congruence
            * (1.0 - 0.65 * d.fatigue)
            * (1.0 - 0.40 * confusion),
    );
    let exhaustion = unit(d.fatigue * f.sleep_pressure * (0.55 + 0.45 * f.physical_load));
    EmotionReadouts {
        calm,
        contentment,
        joy,
        interest,
        playfulness: f.play_readiness,
        affection,
        anticipation,
        surprise: f.surprise,
        startle: f.startle,
        fear,
        anxiety,
        protest,
        sadness,
        frustration,
        confusion,
        discomfort: unit(f.pain_like.max(f.physical_load).max(1.0 - f.comfort)),
        relief: f.relief,
        boredom: f.boredom,
        loneliness: f.loneliness,
        determination,
        exhaustion,
        vulnerability: f.vulnerability,
    }
}

fn morph_targets(
    source: &EmbodimentSourceFrame,
    d: DerivedNervousState,
    f: FeltStateV1,
    previous: &BodyInteroceptionDirector,
) -> MorphSomaticInput {
    let b = source.body;
    let a = source.vita.appraisal;
    MorphSomaticInput {
        touch: unit(
            0.45 * b.contact.area
                + 0.35 * b.contact.pressure
                + 0.20 * (b.contact.duration / 0.6).min(1.0),
        ),
        proximity: unit(1.0 - b.environment.cursor_distance),
        motion: unit(
            0.45 * b.motion.velocity.length()
                + 0.30 * b.motion.acceleration.length()
                + 0.25 * b.environment.cursor_loom_rate,
        ),
        vibration: unit(
            0.55 * b.motion.collision_impulse
                + 0.30 * b.motion.jerk.length()
                + 0.15 * b.fluid.slosh_energy,
        ),
        looming: unit(
            b.environment.cursor_loom_rate * (0.35 + 0.65 * (1.0 - b.environment.cursor_distance)),
        ),
        habituation: unit(0.60 * source.gesture.repetition_similarity + 0.40 * a.expectedness),
        novelty: unit(
            0.50 * source.gesture.novelty + 0.35 * a.novelty + 0.15 * (1.0 - a.expectedness),
        ),
        exploration: unit(
            0.50 * f.exploration_readiness
                + 0.30 * f.exploration_readiness
                + 0.20 * b.environment.available_motion_radius,
        ),
        positive_outcome: unit(
            0.55 * source.episode.reward_positive
                + 0.25 * f.contact_pleasantness
                + 0.20 * rise(f.motor_efficacy, previous.previous_motor_efficacy, 0.05),
        ),
        negative_outcome: unit(
            0.50 * source.episode.reward_negative + 0.30 * f.pain_like + 0.20 * f.restraint,
        ),
        attention: unit(
            0.55 * source.perception.selected_salience
                + 0.25 * d.attention_commitment
                + 0.20 * source.morph.confidence,
        ),
        rest: unit(
            0.55 * f.sleep_pressure
                + 0.20 * d.neural_rest
                + 0.15 * bool_value(b.motion.grounded)
                + 0.10 * (1.0 - d.stress),
        ),
    }
}

fn bool_value(value: bool) -> f32 {
    if value { 1.0 } else { 0.0 }
}

fn rise(current: f32, previous: f32, dt: f32) -> f32 {
    unit((current - previous).max(0.0) / dt.max(1.0 / 240.0))
}

fn fall(current: f32, previous: f32, dt: f32) -> f32 {
    unit((previous - current).max(0.0) / dt.max(1.0 / 240.0))
}

fn smoothstep(lo: f32, hi: f32, value: f32) -> f32 {
    let x = unit((value - lo) / (hi - lo).max(1.0e-5));
    x * x * (3.0 - 2.0 * x)
}

fn soft_band(value: f32, lo: f32, hi: f32, feather: f32) -> f32 {
    if value < lo {
        smoothstep(lo - feather, lo, value)
    } else if value > hi {
        1.0 - smoothstep(hi, hi + feather, value)
    } else {
        1.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Genome, VitaSomaticFrame};

    fn source() -> EmbodimentSourceFrame {
        let genome = Genome::from_seed(7);
        EmbodimentSourceFrame {
            frame_id: 1,
            affect: crate::AffectState::default(),
            drives: crate::Drives::initial(&genome.temperament),
            temperament: genome.temperament,
            voice_seed: genome.voice.voice_seed,
            vita: VitaSomaticFrame::default(),
            morph: crate::MorphNervousSystemFrame::default(),
            body: crate::BodyFeedbackV2::default(),
            voice_feedback: crate::VoiceFeedbackV1::default(),
            gesture: crate::GestureFrameV1::default(),
            episode: crate::EpisodeContextV1::default(),
            perception: crate::PerceptionSelectionV1::default(),
            soft_touch_pressure_max: 0.42,
        }
    }

    #[test]
    fn excessive_pressure_becomes_pain_not_pleasant_touch() {
        let mut director = BodyInteroceptionDirector::default();
        let mut gentle = source();
        gentle.body.contact.area = 0.55;
        gentle.body.contact.pressure = 0.18;
        gentle.vita.appraisal.expectedness = 0.9;
        gentle.vita.appraisal.controllability = 0.9;
        gentle.affect.attachment = 0.8;
        let mut harsh = gentle.clone();
        harsh.frame_id = 2;
        harsh.body.contact.pressure = 0.96;
        harsh.body.shape.maximum_strain = 0.92;
        let gentle_out = director.tick(&gentle, 0.05);
        for _ in 0..20 {
            let _ = director.tick(&harsh, 0.05);
        }
        let harsh_out = director.tick(&harsh, 0.05);
        assert!(gentle_out.felt.contact_pleasantness > gentle_out.felt.pain_like);
        assert!(harsh_out.felt.pain_like > harsh_out.felt.contact_pleasantness);
    }

    #[test]
    fn efference_match_distinguishes_voluntary_from_restrained_motion() {
        let mut director = BodyInteroceptionDirector::default();
        let mut free = source();
        free.body.contact.duration = 2.0;
        free.body.contact.pressure = 0.5;
        free.body.efference_copy.intended_velocity = glam::Vec2::new(0.7, 0.0);
        free.body.efference_copy.actual_velocity = glam::Vec2::new(0.7, 0.0);
        let mut held = free.clone();
        held.body.efference_copy.actual_velocity = glam::Vec2::ZERO;
        for _ in 0..20 {
            let _ = director.tick(&free, 0.05);
        }
        let voluntary = director.tick(&free, 0.05);
        let mut held_director = BodyInteroceptionDirector::default();
        for _ in 0..20 {
            let _ = held_director.tick(&held, 0.05);
        }
        let restrained = held_director.tick(&held, 0.05);
        assert!(voluntary.felt.agency_match > restrained.felt.agency_match);
        assert!(restrained.felt.restraint > voluntary.felt.restraint);
    }

    #[test]
    fn all_channels_stay_bounded_and_finite() {
        let mut director = BodyInteroceptionDirector::default();
        let mut s = source();
        s.body.shape.maximum_strain = f32::INFINITY;
        s.body.contact.pressure = f32::NAN;
        s.body.sanitize();
        let out = director.tick(&s, 0.05);
        assert!(out.felt.is_valid());
        assert!(
            out.emotions
                .values()
                .into_iter()
                .all(|v| v.is_finite() && (0.0..=1.0).contains(&v))
        );
    }
}
