use glam::Vec3;
use rand::{RngCore, SeedableRng};
use rand_chacha::ChaCha8Rng;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Genome {
    pub identity_seed: u64,
    pub lineage_id: u128,
    pub generation: u32,
    pub body: BodyGenome,
    pub voice: VoiceGenome,
    pub temperament: TemperamentGenome,
    pub brain: BrainGenome,
    pub mutation_rate: f32,
    pub developmental_plasticity: f32,
}

impl Genome {
    #[must_use]
    pub fn from_seed(seed: u64) -> Self {
        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        let lineage_id = (u128::from(rng.next_u64()) << 64) | u128::from(rng.next_u64());
        let body = BodyGenome::generate(&mut rng);
        let voice = VoiceGenome::generate(&mut rng);
        let temperament = TemperamentGenome::generate(&mut rng);
        let brain = BrainGenome::generate(&mut rng);
        Self {
            identity_seed: seed,
            lineage_id,
            generation: 0,
            body,
            voice,
            temperament,
            brain,
            mutation_rate: range(&mut rng, 0.04, 0.14),
            developmental_plasticity: range(&mut rng, 0.35, 0.85),
        }
    }

    #[must_use]
    pub fn stable_hash(&self) -> u64 {
        let bytes = serde_json::to_vec(self).expect("Genome serialization is infallible");
        crate::stable_hash_bytes(&bytes)
    }

    /// Validates the portable genotype without silently clamping imported data.
    ///
    /// Checkpoints are lineage evidence, so accepting a NaN or an out-of-domain
    /// trait and repairing it in place would change the recorded organism. New
    /// mutations are clamped before they are recorded; persisted data must
    /// already satisfy the same bounds.
    #[must_use]
    pub fn is_valid(&self) -> bool {
        self.body.is_valid()
            && self.voice.is_valid()
            && self.temperament.is_valid()
            && self.brain.is_valid()
            && unit_value(self.mutation_rate)
            && unit_value(self.developmental_plasticity)
    }

    pub(crate) fn clamp_all(&mut self) {
        self.body.clamp_all();
        self.voice.clamp_all();
        self.temperament.clamp_all();
        self.brain.clamp_all();
        self.mutation_rate = unit(self.mutation_rate);
        self.developmental_plasticity = unit(self.developmental_plasticity);
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TemperamentGenome {
    pub sociability: f32,
    pub curiosity: f32,
    pub boldness: f32,
    pub playfulness: f32,
    pub patience: f32,
    pub persistence: f32,
    pub autonomy: f32,
    pub vocality: f32,
    pub adaptability: f32,
    pub attachment_speed: f32,
    pub exploration_rate: f32,
    pub circadian_phase: f32,
}

impl TemperamentGenome {
    fn generate(rng: &mut impl RngCore) -> Self {
        Self {
            sociability: centered_unit(rng),
            curiosity: centered_unit(rng),
            boldness: centered_unit(rng),
            playfulness: centered_unit(rng),
            patience: centered_unit(rng),
            persistence: centered_unit(rng),
            autonomy: centered_unit(rng),
            vocality: centered_unit(rng),
            adaptability: centered_unit(rng),
            attachment_speed: centered_unit(rng),
            exploration_rate: centered_unit(rng),
            circadian_phase: unit_f32(rng),
        }
    }

    fn clamp_all(&mut self) {
        for value in [
            &mut self.sociability,
            &mut self.curiosity,
            &mut self.boldness,
            &mut self.playfulness,
            &mut self.patience,
            &mut self.persistence,
            &mut self.autonomy,
            &mut self.vocality,
            &mut self.adaptability,
            &mut self.attachment_speed,
            &mut self.exploration_rate,
            &mut self.circadian_phase,
        ] {
            *value = unit(*value);
        }
    }

    fn is_valid(&self) -> bool {
        [
            self.sociability,
            self.curiosity,
            self.boldness,
            self.playfulness,
            self.patience,
            self.persistence,
            self.autonomy,
            self.vocality,
            self.adaptability,
            self.attachment_speed,
            self.exploration_rate,
            self.circadian_phase,
        ]
        .into_iter()
        .all(unit_value)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BrainGenome {
    pub network_seed: u64,
    pub recurrent_connectivity: f32,
    pub plastic_fraction: f32,
    pub learning_rate: f32,
    pub eligibility_tau: f32,
    pub plastic_decay: f32,
    pub maximum_plastic_delta: f32,
    pub time_constant_scale: f32,
}

impl BrainGenome {
    fn generate(rng: &mut impl RngCore) -> Self {
        Self {
            network_seed: rng.next_u64(),
            recurrent_connectivity: range(rng, 0.08, 0.12),
            plastic_fraction: range(rng, 0.14, 0.28),
            learning_rate: range(rng, 0.003, 0.018),
            eligibility_tau: range(rng, 0.8, 3.0),
            plastic_decay: range(rng, 0.998, 0.999_95),
            maximum_plastic_delta: range(rng, 0.12, 0.35),
            time_constant_scale: range(rng, 0.8, 1.25),
        }
    }

    fn clamp_all(&mut self) {
        self.recurrent_connectivity = self.recurrent_connectivity.clamp(0.08, 0.12);
        self.plastic_fraction = self.plastic_fraction.clamp(0.08, 0.35);
        self.learning_rate = self.learning_rate.clamp(0.001, 0.03);
        self.eligibility_tau = self.eligibility_tau.clamp(0.2, 6.0);
        self.plastic_decay = self.plastic_decay.clamp(0.99, 1.0);
        self.maximum_plastic_delta = self.maximum_plastic_delta.clamp(0.05, 0.4);
        self.time_constant_scale = self.time_constant_scale.clamp(0.65, 1.5);
    }

    fn is_valid(&self) -> bool {
        finite_in_range(self.recurrent_connectivity, 0.08, 0.12)
            && finite_in_range(self.plastic_fraction, 0.08, 0.35)
            && finite_in_range(self.learning_rate, 0.001, 0.03)
            && finite_in_range(self.eligibility_tau, 0.2, 6.0)
            && finite_in_range(self.plastic_decay, 0.99, 1.0)
            && finite_in_range(self.maximum_plastic_delta, 0.05, 0.4)
            && finite_in_range(self.time_constant_scale, 0.65, 1.5)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct VoiceAnatomy {
    pub schema: u8,
    pub fold_mass: f32,
    pub neutral_tension: f32,
    pub resting_adduction: f32,
    pub glottal_leak: f32,
    pub tract_length: f32,
    pub tract_compliance: f32,
    pub oral_loss: f32,
    pub nasal_capacity: f32,
    pub body_coupling: f32,
    pub instability_susceptibility: f32,
    pub source_feedback: f32,
}

impl Default for VoiceAnatomy {
    fn default() -> Self {
        Self {
            schema: 0,
            fold_mass: 0.0,
            neutral_tension: 0.0,
            resting_adduction: 0.0,
            glottal_leak: 0.0,
            tract_length: 0.0,
            tract_compliance: 0.0,
            oral_loss: 0.0,
            nasal_capacity: 0.0,
            body_coupling: 0.0,
            instability_susceptibility: 0.0,
            source_feedback: 0.0,
        }
    }
}

impl VoiceAnatomy {
    fn from_identity(
        voice_seed: u64,
        base_pitch_hz: f32,
        formant_scale: f32,
        formant_spacing: f32,
        mouth_resonance: f32,
        roughness: f32,
        breathiness: f32,
    ) -> Self {
        let seeded = |channel: u64| seeded_unit(voice_seed ^ channel);
        let low_pitch_mass = ((800.0 - base_pitch_hz.clamp(220.0, 800.0)) / 580.0).clamp(0.0, 1.0);
        let tract_length = ((1.60 - formant_scale.clamp(0.55, 1.60)) / 1.05).clamp(0.0, 1.0);
        Self {
            schema: 1,
            fold_mass: unit(0.18 + low_pitch_mass * 0.66 + (seeded(0x11) - 0.5) * 0.12),
            neutral_tension: unit(
                0.34 + (1.0 - low_pitch_mass) * 0.42 + (seeded(0x22) - 0.5) * 0.10,
            ),
            resting_adduction: unit(0.38 + mouth_resonance * 0.30 + (seeded(0x33) - 0.5) * 0.12),
            glottal_leak: unit(0.06 + breathiness * 0.52 + (seeded(0x44) - 0.5) * 0.08),
            tract_length: unit(tract_length * 0.82 + seeded(0x55) * 0.18),
            tract_compliance: unit(0.28 + (1.0 - mouth_resonance) * 0.34 + seeded(0x66) * 0.22),
            oral_loss: unit(
                0.18 + (1.0 - formant_spacing.clamp(0.60, 1.50) / 1.50) * 0.26
                    + seeded(0x77) * 0.18,
            ),
            nasal_capacity: unit(0.12 + breathiness * 0.30 + seeded(0x88) * 0.28),
            body_coupling: unit(0.34 + mouth_resonance * 0.38 + seeded(0x99) * 0.18),
            instability_susceptibility: unit(0.08 + roughness * 0.62 + seeded(0xaa) * 0.16),
            source_feedback: (0.035 + mouth_resonance * 0.085 + seeded(0xbb) * 0.035)
                .clamp(0.02, 0.18),
        }
    }

    fn clamp_all(&mut self) {
        self.schema = 1;
        self.fold_mass = unit(self.fold_mass);
        self.neutral_tension = unit(self.neutral_tension);
        self.resting_adduction = unit(self.resting_adduction);
        self.glottal_leak = unit(self.glottal_leak);
        self.tract_length = unit(self.tract_length);
        self.tract_compliance = unit(self.tract_compliance);
        self.oral_loss = unit(self.oral_loss);
        self.nasal_capacity = unit(self.nasal_capacity);
        self.body_coupling = unit(self.body_coupling);
        self.instability_susceptibility = unit(self.instability_susceptibility);
        self.source_feedback = self.source_feedback.clamp(0.02, 0.18);
    }

    fn is_valid(self) -> bool {
        self.schema == 1
            && [
                self.fold_mass,
                self.neutral_tension,
                self.resting_adduction,
                self.glottal_leak,
                self.tract_length,
                self.tract_compliance,
                self.oral_loss,
                self.nasal_capacity,
                self.body_coupling,
                self.instability_susceptibility,
            ]
            .into_iter()
            .all(unit_value)
            && finite_in_range(self.source_feedback, 0.02, 0.18)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VoiceGenome {
    pub base_pitch_hz: f32,
    pub pitch_range_octaves: f32,
    pub harmonic_mix: [f32; 4],
    pub breathiness: f32,
    pub roughness: f32,
    pub brightness: f32,
    pub formant_scale: f32,
    pub formant_spacing: f32,
    pub mouth_resonance: f32,
    pub vibrato_rate: f32,
    pub vibrato_depth: f32,
    pub trill_rate: f32,
    pub attack_ms: f32,
    pub release_ms: f32,
    pub click_amount: f32,
    pub purr_rate: f32,
    pub phrase_speed: f32,
    pub maximum_loudness: f32,
    pub voice_seed: u64,
    #[serde(default)]
    pub anatomy: VoiceAnatomy,
}

impl VoiceGenome {
    fn generate(rng: &mut impl RngCore) -> Self {
        let mut harmonic_mix = [
            range(rng, 0.38, 0.72),
            range(rng, 0.10, 0.32),
            range(rng, 0.04, 0.20),
            range(rng, 0.01, 0.10),
        ];
        normalize_mix(&mut harmonic_mix);
        let voice_seed = rng.next_u64();
        let mut voice = Self {
            // New identities start in the common audible cat-call / small-dog
            // whine band. Higher syllable contours still reach excited squeaks.
            base_pitch_hz: range(rng, 340.0, 620.0),
            pitch_range_octaves: range(rng, 0.55, 1.65),
            harmonic_mix,
            breathiness: range(rng, 0.02, 0.42),
            roughness: range(rng, 0.0, 0.28),
            brightness: centered_unit(rng),
            formant_scale: range(rng, 0.72, 1.32),
            formant_spacing: range(rng, 0.78, 1.25),
            mouth_resonance: centered_unit(rng),
            vibrato_rate: range(rng, 3.0, 9.0),
            vibrato_depth: range(rng, 0.002, 0.032),
            trill_rate: range(rng, 8.0, 22.0),
            attack_ms: range(rng, 6.0, 48.0),
            release_ms: range(rng, 30.0, 150.0),
            click_amount: range(rng, 0.0, 0.28),
            purr_rate: range(rng, 25.0, 30.0),
            phrase_speed: range(rng, 0.75, 1.35),
            maximum_loudness: range(rng, 0.14, 0.34),
            voice_seed,
            anatomy: VoiceAnatomy::default(),
        };
        voice.repair_anatomy();
        voice
    }

    /// Additive migration for snapshots created before embodied voice anatomy.
    pub fn repair_anatomy(&mut self) {
        if self.anatomy.schema == 1 && self.anatomy.is_valid() {
            return;
        }
        self.anatomy = VoiceAnatomy::from_identity(
            self.voice_seed,
            self.base_pitch_hz,
            self.formant_scale,
            self.formant_spacing,
            self.mouth_resonance,
            self.roughness,
            self.breathiness,
        );
    }

    pub(crate) fn clamp_all(&mut self) {
        self.repair_anatomy();
        self.base_pitch_hz = self.base_pitch_hz.clamp(220.0, 800.0);
        self.pitch_range_octaves = self.pitch_range_octaves.clamp(0.3, 2.0);
        for harmonic in &mut self.harmonic_mix {
            *harmonic = harmonic.max(0.0);
        }
        normalize_mix(&mut self.harmonic_mix);
        self.breathiness = unit(self.breathiness);
        self.roughness = unit(self.roughness);
        self.brightness = unit(self.brightness);
        self.formant_scale = self.formant_scale.clamp(0.55, 1.6);
        self.formant_spacing = self.formant_spacing.clamp(0.6, 1.5);
        self.mouth_resonance = unit(self.mouth_resonance);
        self.vibrato_rate = self.vibrato_rate.clamp(1.0, 14.0);
        self.vibrato_depth = self.vibrato_depth.clamp(0.0, 0.08);
        self.trill_rate = self.trill_rate.clamp(4.0, 30.0);
        self.attack_ms = self.attack_ms.clamp(2.0, 120.0);
        self.release_ms = self.release_ms.clamp(15.0, 300.0);
        self.click_amount = unit(self.click_amount);
        self.purr_rate = self.purr_rate.clamp(20.0, 38.0);
        self.phrase_speed = self.phrase_speed.clamp(0.5, 1.8);
        self.maximum_loudness = self.maximum_loudness.clamp(0.05, 0.5);
        self.anatomy.clamp_all();
    }

    fn is_valid(&self) -> bool {
        let harmonic_sum = self.harmonic_mix.iter().sum::<f32>();
        finite_in_range(self.base_pitch_hz, 220.0, 800.0)
            && finite_in_range(self.pitch_range_octaves, 0.3, 2.0)
            && self
                .harmonic_mix
                .iter()
                .all(|harmonic| harmonic.is_finite() && *harmonic >= 0.0)
            && harmonic_sum.is_finite()
            && (harmonic_sum - 1.0).abs() <= 0.002
            && unit_value(self.breathiness)
            && unit_value(self.roughness)
            && unit_value(self.brightness)
            && finite_in_range(self.formant_scale, 0.55, 1.6)
            && finite_in_range(self.formant_spacing, 0.6, 1.5)
            && unit_value(self.mouth_resonance)
            && finite_in_range(self.vibrato_rate, 1.0, 14.0)
            && finite_in_range(self.vibrato_depth, 0.0, 0.08)
            && finite_in_range(self.trill_rate, 4.0, 30.0)
            && finite_in_range(self.attack_ms, 2.0, 120.0)
            && finite_in_range(self.release_ms, 15.0, 300.0)
            && unit_value(self.click_amount)
            && finite_in_range(self.purr_rate, 20.0, 38.0)
            && finite_in_range(self.phrase_speed, 0.5, 1.8)
            && finite_in_range(self.maximum_loudness, 0.05, 0.5)
            && self.anatomy.is_valid()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BodyGenome {
    pub body_length: f32,
    pub body_width: f32,
    pub body_roundness: f32,
    pub head_ratio: f32,
    pub eye_size: f32,
    pub eye_spacing: f32,
    pub pupil_ratio: f32,
    pub wing_span: f32,
    pub wing_aspect: f32,
    pub wing_roundness: f32,
    pub wing_translucency: f32,
    pub tail_length: f32,
    pub tail_thickness: f32,
    pub tail_segments: u8,
    pub limb_length: f32,
    pub limb_thickness: f32,
    pub ear_fin_size: f32,
    pub crest_size: f32,
    pub softness: f32,
    pub visual_mass: f32,
    pub inertia: f32,
    pub primary_color_hsv: Vec3,
    pub secondary_color_hsv: Vec3,
    pub glow_color_hsv: Vec3,
    pub pattern_seed: u64,
    pub pattern_scale: f32,
    pub pattern_contrast: f32,
    pub bioluminescence: f32,
}

impl BodyGenome {
    fn generate(rng: &mut impl RngCore) -> Self {
        let hue = unit_f32(rng);
        Self {
            body_length: range(rng, 0.72, 1.20),
            body_width: range(rng, 0.48, 0.92),
            body_roundness: range(rng, 0.55, 1.0),
            head_ratio: range(rng, 0.38, 0.62),
            eye_size: range(rng, 0.10, 0.22),
            eye_spacing: range(rng, 0.20, 0.42),
            pupil_ratio: range(rng, 0.38, 0.72),
            wing_span: range(rng, 0.55, 1.25),
            wing_aspect: range(rng, 0.55, 1.55),
            wing_roundness: range(rng, 0.25, 0.9),
            wing_translucency: range(rng, 0.20, 0.66),
            tail_length: range(rng, 0.45, 1.35),
            tail_thickness: range(rng, 0.05, 0.16),
            tail_segments: 4 + (rng.next_u32() % 5) as u8,
            limb_length: range(rng, 0.16, 0.38),
            limb_thickness: range(rng, 0.05, 0.14),
            ear_fin_size: range(rng, 0.08, 0.30),
            crest_size: range(rng, 0.0, 0.24),
            softness: centered_unit(rng),
            visual_mass: centered_unit(rng),
            inertia: range(rng, 0.25, 0.82),
            primary_color_hsv: Vec3::new(hue, range(rng, 0.35, 0.78), range(rng, 0.62, 0.94)),
            secondary_color_hsv: Vec3::new(
                (hue + range(rng, 0.08, 0.28)).fract(),
                range(rng, 0.24, 0.72),
                range(rng, 0.60, 0.96),
            ),
            glow_color_hsv: Vec3::new(
                (hue + range(rng, 0.42, 0.58)).fract(),
                range(rng, 0.38, 0.85),
                range(rng, 0.78, 1.0),
            ),
            pattern_seed: rng.next_u64(),
            pattern_scale: range(rng, 1.2, 4.5),
            pattern_contrast: range(rng, 0.08, 0.52),
            bioluminescence: range(rng, 0.08, 0.55),
        }
    }

    pub(crate) fn clamp_all(&mut self) {
        self.body_length = self.body_length.clamp(0.60, 1.35);
        self.body_width = self.body_width.clamp(0.40, 1.05);
        self.body_roundness = self.body_roundness.clamp(0.35, 1.0);
        self.head_ratio = self.head_ratio.clamp(0.30, 0.70);
        self.eye_size = self.eye_size.clamp(0.07, 0.26);
        self.eye_spacing = self.eye_spacing.clamp(0.16, 0.48);
        self.pupil_ratio = self.pupil_ratio.clamp(0.25, 0.82);
        self.wing_span = self.wing_span.clamp(0.35, 1.5);
        self.wing_aspect = self.wing_aspect.clamp(0.4, 1.8);
        self.wing_roundness = unit(self.wing_roundness);
        self.wing_translucency = self.wing_translucency.clamp(0.10, 0.78);
        self.tail_length = self.tail_length.clamp(0.25, 1.6);
        self.tail_thickness = self.tail_thickness.clamp(0.035, 0.20);
        self.tail_segments = self.tail_segments.clamp(3, 10);
        self.limb_length = self.limb_length.clamp(0.10, 0.46);
        self.limb_thickness = self.limb_thickness.clamp(0.035, 0.18);
        self.ear_fin_size = self.ear_fin_size.clamp(0.03, 0.38);
        self.crest_size = self.crest_size.clamp(0.0, 0.32);
        self.softness = unit(self.softness);
        self.visual_mass = unit(self.visual_mass);
        self.inertia = unit(self.inertia);
        self.primary_color_hsv = clamp_hsv(self.primary_color_hsv);
        self.secondary_color_hsv = clamp_hsv(self.secondary_color_hsv);
        self.glow_color_hsv = clamp_hsv(self.glow_color_hsv);
        self.pattern_scale = self.pattern_scale.clamp(0.8, 6.0);
        self.pattern_contrast = self.pattern_contrast.clamp(0.02, 0.72);
        self.bioluminescence = self.bioluminescence.clamp(0.04, 0.75);
    }

    fn is_valid(&self) -> bool {
        finite_in_range(self.body_length, 0.60, 1.35)
            && finite_in_range(self.body_width, 0.40, 1.05)
            && finite_in_range(self.body_roundness, 0.35, 1.0)
            && finite_in_range(self.head_ratio, 0.30, 0.70)
            && finite_in_range(self.eye_size, 0.07, 0.26)
            && finite_in_range(self.eye_spacing, 0.16, 0.48)
            && finite_in_range(self.pupil_ratio, 0.25, 0.82)
            && finite_in_range(self.wing_span, 0.35, 1.5)
            && finite_in_range(self.wing_aspect, 0.4, 1.8)
            && unit_value(self.wing_roundness)
            && finite_in_range(self.wing_translucency, 0.10, 0.78)
            && finite_in_range(self.tail_length, 0.25, 1.6)
            && finite_in_range(self.tail_thickness, 0.035, 0.20)
            && (3..=10).contains(&self.tail_segments)
            && finite_in_range(self.limb_length, 0.10, 0.46)
            && finite_in_range(self.limb_thickness, 0.035, 0.18)
            && finite_in_range(self.ear_fin_size, 0.03, 0.38)
            && finite_in_range(self.crest_size, 0.0, 0.32)
            && unit_value(self.softness)
            && unit_value(self.visual_mass)
            && unit_value(self.inertia)
            && hsv_is_valid(self.primary_color_hsv)
            && hsv_is_valid(self.secondary_color_hsv)
            && hsv_is_valid(self.glow_color_hsv)
            && finite_in_range(self.pattern_scale, 0.8, 6.0)
            && finite_in_range(self.pattern_contrast, 0.02, 0.72)
            && finite_in_range(self.bioluminescence, 0.04, 0.75)
    }
}

pub(crate) fn unit_f32(rng: &mut impl RngCore) -> f32 {
    (f64::from(rng.next_u32()) / f64::from(u32::MAX)) as f32
}

pub(crate) fn range(rng: &mut impl RngCore, minimum: f32, maximum: f32) -> f32 {
    minimum + (maximum - minimum) * unit_f32(rng)
}

pub(crate) fn signed_unit(rng: &mut impl RngCore) -> f32 {
    unit_f32(rng) * 2.0 - 1.0
}

fn centered_unit(rng: &mut impl RngCore) -> f32 {
    ((unit_f32(rng) + unit_f32(rng)) * 0.5).clamp(0.0, 1.0)
}

fn unit(value: f32) -> f32 {
    value.clamp(0.0, 1.0)
}

fn seeded_unit(seed: u64) -> f32 {
    let mixed = splitmix64(seed);
    ((mixed >> 40) as u32) as f32 / 0x00ff_ffff as f32
}

fn splitmix64(mut value: u64) -> u64 {
    value = value.wrapping_add(0x9e37_79b9_7f4a_7c15);
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

fn normalize_mix(mix: &mut [f32; 4]) {
    let sum = mix.iter().sum::<f32>().max(f32::EPSILON);
    for value in mix {
        *value /= sum;
    }
}

fn clamp_hsv(value: Vec3) -> Vec3 {
    Vec3::new(
        value.x.rem_euclid(1.0),
        unit(value.y),
        unit(value.z).max(0.12),
    )
}

fn finite_in_range(value: f32, minimum: f32, maximum: f32) -> bool {
    value.is_finite() && (minimum..=maximum).contains(&value)
}

fn unit_value(value: f32) -> bool {
    finite_in_range(value, 0.0, 1.0)
}

fn hsv_is_valid(value: Vec3) -> bool {
    value.is_finite()
        && (0.0..1.0).contains(&value.x)
        && unit_value(value.y)
        && finite_in_range(value.z, 0.12, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_genome_is_valid() {
        assert!(Genome::from_seed(42).is_valid());
    }

    #[test]
    fn validation_rejects_non_finite_and_out_of_domain_traits() {
        let mut genome = Genome::from_seed(42);
        genome.body.tail_length = f32::NAN;
        assert!(!genome.is_valid());

        let mut genome = Genome::from_seed(42);
        genome.voice.maximum_loudness = 0.75;
        assert!(!genome.is_valid());

        let mut genome = Genome::from_seed(42);
        genome.temperament.autonomy = -0.01;
        assert!(!genome.is_valid());
    }
}
