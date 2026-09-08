use lifecore::{ExpressionState, FastPhenotypeActuation, VocalRequest, VoiceGenome};
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const LIQUID_TUNING_SCHEMA_VERSION: u32 = 21;
const OLDEST_MIGRATABLE_LIQUID_TUNING_SCHEMA_VERSION: u32 = 6;

/// Approved Body Lab seed-42 palette. Cinematic intentionally uses this authored
/// art direction instead of letting unrelated inherited genome hues change the
/// material while the character look is being locked.
pub const BODY_LAB_PRIMARY_HSV: [f32; 3] = [0.772_313_36, 0.734_904_9, 0.853_215_5];
pub const BODY_LAB_SECONDARY_HSV: [f32; 3] = [0.913_958, 0.325_985_5, 0.957_325_7];
pub const BODY_LAB_GLOW_HSV: [f32; 3] = [0.284_376_26, 0.428_986, 0.817_787_7];

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BodyRenderMode {
    #[default]
    AnalyticJelly,
    ParticlePbf,
}

/// The incumbent material remains a byte-for-byte shader branch so visual
/// experiments always have an immediate rollback in Body Lab.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MaterialVariant {
    #[default]
    CurrentSafe,
    CinematicJelly,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ColorSourceMode {
    Authored,
    Genome,
    #[default]
    GenomeAuthoredBlend,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LiquidTuningProfile {
    pub schema_version: u32,
    pub profile_revision: u64,
    pub name: String,
    pub seed: u64,
    pub render_mode: BodyRenderMode,
    pub analytic: AnalyticTuning,
    pub pbf: PbfTuning,
    pub interaction: InteractionTuning,
    pub droplets: DropletTuning,
    pub material: MaterialTuning,
    pub face: FaceTuning,
    pub compositor: CompositorTuning,
    /// Runtime-only visual calibration for the den/home lens. This never owns
    /// den physics or storage semantics.
    pub den: DenVisualTuning,
    /// User-authored presentation trim applied after the nervous-system voice
    /// phenotype and before the immutable genome loudness ceiling.
    pub voice: VoicePresentationTuning,
    /// Readability gain around neutral fast-coupling values. Structural solver
    /// locks and identity fields are deliberately absent.
    pub nervous: NervousReadabilityTuning,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DenVisualTuning {
    /// Perceived feature size: larger values produce larger energy packets.
    pub noise_size: f32,
    pub noise_strength: f32,
    pub inward_speed: f32,
    /// Independent speed of the broad concentric wave fronts moving inward.
    pub ripple_inward_speed: f32,
    pub noise_detail_scale: f32,
    pub noise_detail_mix: f32,
    pub noise_warp: f32,
    /// Angular coherence of the inward noise field. Zero permits many narrow
    /// radial sectors; 100 produces a few broad connected packets without
    /// attenuating their amplitude.
    pub noise_band_width: f32,
    pub ripple_strength: f32,
    pub ripple_opacity: f32,
    pub displacement_strength: f32,
    pub displacement_blur: f32,
    pub displacement_noise_mix: f32,
    pub displacement_radius: f32,
    pub refraction_strength: f32,
    pub refraction_opacity: f32,
    pub dispersion_strength: f32,
    pub tint_strength: f32,
    pub caustic_strength: f32,
    pub glow_strength: f32,
    pub particle_brightness: f32,
    pub particle_count: u8,
    pub center_mask_radius: f32,
    pub center_mask_feather: f32,
    pub center_mask_opacity: f32,
    /// Maximum fractional animation-speed lift while an unstored orb enters
    /// the den. `0.15` means exactly fifteen percent, integrated without a
    /// phase discontinuity.
    pub orb_speedup_fraction: f32,
    pub orb_attack_seconds: f32,
    pub orb_release_seconds: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct VoicePresentationTuning {
    pub pitch_multiplier: f32,
    pub formant_multiplier: f32,
    pub tempo_multiplier: f32,
    pub loudness_multiplier: f32,
    pub attack_multiplier: f32,
    pub release_multiplier: f32,
    pub breathiness_delta: f32,
    pub roughness_delta: f32,
    pub brightness_delta: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct NervousReadabilityTuning {
    pub shape_gain: f32,
    pub breathing_gain: f32,
    /// Amplifies bounded PBF motion multipliers. This never changes fixed_hz,
    /// solver iterations, particle_count, or any other structural lock.
    pub particle_motion_gain: f32,
    /// Makes emotional compression/expansion legible through dynamic density
    /// compliance. It deliberately does not alter the authored rest spacing.
    pub particle_spacing_response_gain: f32,
    pub viscosity_response_gain: f32,
    pub cohesion_response_gain: f32,
    pub recovery_response_gain: f32,
    pub material_gain: f32,
    pub soul_glow_gain: f32,
    pub internal_flow_gain: f32,
    pub pulse_gain: f32,
    pub expression_gain: f32,
    pub motion_gain: f32,
    pub voice_gain: f32,
    pub threat_sensitivity: f32,
    pub pain_sensitivity: f32,
    pub contact_sensitivity: f32,
    pub safety_sensitivity: f32,
    pub restraint_sensitivity: f32,
    pub fatigue_sensitivity: f32,
    pub novelty_sensitivity: f32,
    pub startle_sensitivity: f32,
    pub agency_sensitivity: f32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TopologyConstraintMode {
    ObserveOnly,
    #[default]
    GuardedNecks,
    Viscoelastic,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct InteractionTuning {
    pub enabled: bool,
    pub topology_mode: TopologyConstraintMode,
    pub contact_weight_floor: f32,
    pub pressure_reference: f32,
    pub signal_smoothing_hz: f32,
    pub gesture_window_seconds: f32,
    pub gesture_commit_confidence: f32,
    pub gesture_ambiguity_margin: f32,
    pub soft_touch_pressure_max: f32,
    pub stretch_strain_min: f32,
    pub stretch_strain_max: f32,
    pub flick_speed_min: f32,
    pub rhythm_interval_cv_max: f32,
    pub rhythm_min_impulses: u8,
    pub maximum_detached_components: u8,
    pub maximum_detached_mass_fraction: f32,
    pub minimum_fragment_particles: u8,
    pub split_hold_seconds: f32,
    pub boundary_strain: f32,
    pub boundary_hold_seconds: f32,
    pub fragment_lifetime_seconds: f32,
    pub offscreen_recovery_delay_seconds: f32,
    pub recovery_field_boost: f32,
    pub turn_wait_seconds: f32,
    pub turn_cooldown_seconds: f32,
    pub response_amplitude: f32,
    pub learning_openness: f32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LiquidTuningAcknowledgement {
    pub profile_revision: u64,
    pub schema_version: u32,
    pub material_variant: MaterialVariant,
    pub build_version: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AnalyticTuning {
    pub body_length_scale: f32,
    pub body_width_scale: f32,
    pub roundness_bias: f32,
    pub softness_bias: f32,
    pub modal_response: f32,
    pub modal_frequency: f32,
    pub modal_damping: f32,
    pub modal_amplitude: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PbfTuning {
    pub fixed_hz: f32,
    pub particle_count: usize,
    pub substeps: usize,
    pub impact_substeps: usize,
    pub density_iterations: usize,
    pub impact_density_iterations: usize,
    pub density_compliance: f32,
    pub rest_density_scale: f32,
    pub scorr_k: f32,
    pub scorr_q_ratio: f32,
    pub scorr_power: u32,
    /// Minimum pair-symmetric XSPH dissipation used for numerical stability.
    /// This is deliberately separate from the authored material viscosity.
    pub numerical_xsph: f32,
    pub viscosity: f32,
    pub surface_tension: f32,
    pub shape_recovery: f32,
    pub spacing_scale: f32,
    pub kernel_radius_scale: f32,
    pub motor_gain: f32,
    /// Strength of the bounded centre-of-mass lag opposite screen acceleration.
    pub flight_inertia: f32,
    /// Zero-sum material stretch caused by screen acceleration and turns.
    pub flight_stretch: f32,
    /// Continuous damping of particle velocity relative to the main component.
    pub flight_damping: f32,
    /// Maximum flight COM lag expressed as a fraction of the reference body radius.
    pub flight_max_lag: f32,
    pub angular_damping: f32,
    pub upright_stabilization: f32,
    pub maximum_speed: f32,
    pub grab_radius: f32,
    pub grab_stiffness: f32,
    /// Compact pointer-field support measured in smoothing radii.
    pub pointer_support_scale: f32,
    /// Critically damped pointer-field centre response.
    pub pointer_response_hz: f32,
    pub grab_damping: f32,
    pub grab_follow: f32,
    pub grab_stretch: f32,
    pub tear_speed: f32,
    pub tear_rate: f32,
    pub grab_com_stabilization: f32,
    pub anisotropy_max: f32,
    pub render_center_smoothing: f32,
    pub render_response_hz: f32,
    pub iso_threshold: f32,
    pub component_link_radius_scale: f32,
    pub return_delay: f32,
    pub return_strength: f32,
    /// Scale applied to the permanent character-field ellipse.
    pub character_field_radius_scale: f32,
    pub idle_fragment_interval: f32,
    pub idle_fragment_lifetime: f32,
    pub idle_fragment_size: f32,
    pub idle_bud_interval: f32,
    pub idle_bud_duration: f32,
    pub idle_bud_pull_strength: f32,
    pub idle_bud_neck_scale: f32,
    pub idle_bud_maximum: usize,
    pub idle_breath_amplitude: f32,
    pub idle_breath_speed: f32,
    pub idle_lean_angle: f32,
    pub idle_lean_rate: f32,
    pub lean_return_half_life: f32,
    pub upright_hold: f32,
    pub neck_continuity: f32,
    pub pinch_bounce: f32,
    pub pinch_spray_count: usize,
    pub pinch_spray_cone: f32,
    pub pinch_spray_size: f32,
    pub pinch_spray_speed_variance: f32,
    pub bond_compliance: f32,
    pub bond_create_radius_scale: f32,
    pub bond_create_speed_limit: f32,
    pub bond_yield_strain: f32,
    pub bond_break_strain: f32,
    pub bond_relaxation_time: f32,
    pub bond_iterations: usize,
    pub bond_neck_scale: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DropletTuning {
    pub count: usize,
    pub size_scale: f32,
    pub spread_scale: f32,
    pub elasticity: f32,
    pub drag: f32,
    pub lag: f32,
    pub cohesion: f32,
    pub inertia_scale: f32,
    pub maximum_mobile: usize,
    pub detach_distance: f32,
    pub return_strength: f32,
    pub tether_strength: f32,
    pub bridge_radius_scale: f32,
    pub merge_distance: f32,
    pub stretch_scale: f32,
    pub maximum_distance: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MaterialTuning {
    #[serde(default = "safe_material_variant")]
    pub variant: MaterialVariant,
    pub override_genome_colors: bool,
    pub color_source_mode: ColorSourceMode,
    /// Weight of inherited Genome color in the circular-HSV identity blend.
    pub genome_color_blend: f32,
    /// Presentation-only weight of the bounded R12 mood/material readout.
    /// Zero preserves the identity palette; one shows the full mood target.
    pub mood_color_blend: f32,
    pub primary_hsv: [f32; 3],
    pub secondary_hsv: [f32; 3],
    pub glow_hsv: [f32; 3],
    pub absorption: f32,
    pub scattering: f32,
    pub thickness: f32,
    pub translucency: f32,
    pub refraction: f32,
    pub blur: f32,
    pub rim_strength: f32,
    pub rim_power: f32,
    pub broad_specular: f32,
    pub broad_specular_power: f32,
    pub tight_specular: f32,
    pub tight_specular_power: f32,
    pub emission: f32,
    pub fresnel_f0: f32,
    pub core_level: f32,
    pub thickness_gamma: f32,
    pub pseudo_depth: f32,
    pub normal_scale: f32,
    pub light_wrap: f32,
    pub ambient_scatter: f32,
    pub direct_scatter: f32,
    pub transmission_hue_preservation: f32,
    pub internal_flow: f32,
    pub halo: f32,
    pub opacity: f32,
    pub cinematic_smoothing: f32,
    pub internal_orb_count: usize,
    pub internal_orb_intensity: f32,
    pub internal_orb_size: f32,
    pub internal_orb_halo: f32,
    pub internal_orb_speed: f32,
    pub internal_orb_depth: f32,
    pub internal_orb_spread: f32,
    pub studio_intensity: f32,
    pub studio_base_roughness: f32,
    pub studio_coat_roughness: f32,
    pub narrow_rim_strength: f32,
    pub broad_rim_strength: f32,
    pub rim_saturation: f32,
    pub edge_light_width: f32,
    pub caustic_strength: f32,
    pub caustic_scale: f32,
    pub caustic_speed: f32,
    pub caustic_dispersion: f32,
    pub rounded_highlight_strength: f32,
    pub highlight_tint: f32,
    pub soul_glow_count: usize,
    pub soul_glow_strength: f32,
    pub soul_glow_size: f32,
    pub soul_glow_speed: f32,
    pub soul_glow_pulse: f32,
    pub soul_glow_feather: f32,
    pub bloom_strength: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FaceTuning {
    pub visible: bool,
    /// When disabled the iris follows the inherited body/glow palette.
    pub override_iris_color: bool,
    /// Authored iris color in HSV (hue, saturation, linear value).
    pub iris_hsv: [f32; 3],
    pub origin: [f32; 2],
    pub scale: [f32; 2],
    pub eye_size_scale: f32,
    pub eye_spacing_scale: f32,
    pub pupil_scale: f32,
    pub eye_highlight_scale: f32,
    pub eye_socket_strength: f32,
    pub relief_strength: f32,
    pub relief_darkness: f32,
    pub relief_coat_strength: f32,
    pub pupil_light_response: f32,
    pub pupil_emotion_response: f32,
    pub pupil_focus_response: f32,
    pub microsaccade_amount: f32,
    pub microsaccade_rate: f32,
    pub maximum_roll_radians: f32,
    pub translation_smoothing: f32,
    pub rotation_smoothing: f32,
    pub scale_smoothing: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CompositorTuning {
    pub render_scale: u32,
    pub shadow_horizontal_offset: f32,
    pub shadow_vertical_offset: f32,
    #[serde(alias = "shadow_blur_radius")]
    pub shadow_feather: f32,
    pub shadow_opacity: f32,
    pub shadow_color: [f32; 3],
    pub exposure: f32,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum TuningProfileError {
    #[error("unsupported liquid tuning schema {found}; expected {expected}")]
    UnsupportedSchema { found: u32, expected: u32 },
}

impl Default for LiquidTuningProfile {
    fn default() -> Self {
        Self::for_seed(0x5045_5432_D15C_0A57)
    }
}

impl LiquidTuningProfile {
    #[must_use]
    pub fn for_seed(seed: u64) -> Self {
        Self {
            schema_version: LIQUID_TUNING_SCHEMA_VERSION,
            profile_revision: 0,
            name: "PET-2 Jelly".to_owned(),
            seed,
            render_mode: BodyRenderMode::AnalyticJelly,
            analytic: AnalyticTuning::default(),
            pbf: PbfTuning::default(),
            interaction: InteractionTuning::default(),
            droplets: DropletTuning::default(),
            material: MaterialTuning::default(),
            face: FaceTuning::default(),
            compositor: CompositorTuning::default(),
            den: DenVisualTuning::default(),
            voice: VoicePresentationTuning::default(),
            nervous: NervousReadabilityTuning::default(),
        }
    }

    pub fn sanitized(mut self) -> Result<Self, TuningProfileError> {
        let source_schema = self.schema_version;
        let migrate_to_lab_palette =
            self.schema_version <= 12 && self.material.variant == MaterialVariant::CinematicJelly;
        if (OLDEST_MIGRATABLE_LIQUID_TUNING_SCHEMA_VERSION..=8).contains(&self.schema_version) {
            // Schema 7 added soft-field drag. Schema 8 added the first material
            // experiment; schema 9 replaces that rejected representation with the
            // v2 volume pipeline. Old profiles always enter through Current / Safe
            // so loading a file cannot silently change its look.
            self.material.variant = MaterialVariant::CurrentSafe;
            self.schema_version = LIQUID_TUNING_SCHEMA_VERSION;
        } else if (9..=20).contains(&self.schema_version) {
            // Cinematic profiles already opted into their material. Schemas 10
            // through 15 preserve the selected material lane. Schema 16 replaces
            // the unstable mode-switched solver settings below without touching
            // authored look, face, shadow, or pointer strength.
            self.schema_version = LIQUID_TUNING_SCHEMA_VERSION;
        } else if self.schema_version != LIQUID_TUNING_SCHEMA_VERSION {
            return Err(TuningProfileError::UnsupportedSchema {
                found: self.schema_version,
                expected: LIQUID_TUNING_SCHEMA_VERSION,
            });
        }
        if migrate_to_lab_palette && self.material.variant == MaterialVariant::CinematicJelly {
            self.material.override_genome_colors = true;
            self.material.primary_hsv = BODY_LAB_PRIMARY_HSV;
            self.material.secondary_hsv = BODY_LAB_SECONDARY_HSV;
            self.material.glow_hsv = BODY_LAB_GLOW_HSV;
        }
        if source_schema <= 17 {
            self.material.color_source_mode = if self.material.override_genome_colors {
                ColorSourceMode::GenomeAuthoredBlend
            } else {
                ColorSourceMode::Genome
            };
            self.material.genome_color_blend = 0.65;
        }
        if source_schema <= 18 {
            self.material.mood_color_blend = 0.72;
        }
        if source_schema < 17 {
            self.pbf.apply_v16_solver_defaults();
        }
        self.name = self.name.trim().chars().take(64).collect();
        if self.name.is_empty() {
            self.name = "PET-2 Jelly".to_owned();
        }
        self.analytic.sanitize();
        self.pbf.sanitize();
        self.interaction.sanitize();
        self.droplets.sanitize();
        self.material.sanitize();
        self.face.sanitize();
        self.compositor.sanitize();
        self.den.sanitize();
        self.voice.sanitize();
        self.nervous.sanitize();
        Ok(self)
    }
}

impl Default for DenVisualTuning {
    fn default() -> Self {
        Self {
            noise_size: 2.65,
            noise_strength: 1.18,
            inward_speed: 0.48,
            ripple_inward_speed: 0.42,
            noise_detail_scale: 1.72,
            noise_detail_mix: 0.14,
            noise_warp: 0.30,
            noise_band_width: 0.82,
            ripple_strength: 0.30,
            ripple_opacity: 0.24,
            displacement_strength: 1.35,
            displacement_blur: 0.052,
            displacement_noise_mix: 0.86,
            displacement_radius: 1.0,
            refraction_strength: 1.0,
            refraction_opacity: 0.82,
            dispersion_strength: 0.72,
            tint_strength: 0.55,
            caustic_strength: 0.65,
            glow_strength: 0.24,
            particle_brightness: 1.15,
            particle_count: 18,
            center_mask_radius: 0.16,
            center_mask_feather: 0.34,
            center_mask_opacity: 0.94,
            orb_speedup_fraction: 0.15,
            orb_attack_seconds: 0.55,
            orb_release_seconds: 1.20,
        }
    }
}

impl DenVisualTuning {
    fn sanitize(&mut self) {
        self.noise_size = bounded(self.noise_size, 0.60, 5.0, 2.65);
        self.noise_strength = bounded(self.noise_strength, 0.0, 2.0, 1.18);
        self.inward_speed = bounded(self.inward_speed, 0.0, 1.0, 0.48);
        self.ripple_inward_speed = bounded(self.ripple_inward_speed, 0.0, 1.0, 0.42);
        self.noise_detail_scale = bounded(self.noise_detail_scale, 1.0, 4.0, 1.72);
        self.noise_detail_mix = bounded(self.noise_detail_mix, 0.0, 1.0, 0.14);
        self.noise_warp = bounded(self.noise_warp, 0.0, 1.0, 0.30);
        self.noise_band_width = bounded(self.noise_band_width, 0.0, 100.0, 0.82);
        self.ripple_strength = bounded(self.ripple_strength, 0.0, 1.5, 0.30);
        self.ripple_opacity = bounded(self.ripple_opacity, 0.0, 1.0, 0.24);
        self.displacement_strength = bounded(self.displacement_strength, 0.0, 2.5, 1.35);
        self.displacement_blur = bounded(self.displacement_blur, 0.012, 0.14, 0.052);
        self.displacement_noise_mix = bounded(self.displacement_noise_mix, 0.0, 1.0, 0.86);
        self.displacement_radius = bounded(self.displacement_radius, 0.45, 1.30, 1.0);
        self.refraction_strength = bounded(self.refraction_strength, 0.0, 1.5, 1.0);
        self.refraction_opacity = bounded(self.refraction_opacity, 0.0, 1.5, 0.82);
        self.dispersion_strength = bounded(self.dispersion_strength, 0.0, 1.5, 0.72);
        self.tint_strength = bounded(self.tint_strength, 0.0, 2.0, 0.55);
        self.caustic_strength = bounded(self.caustic_strength, 0.0, 2.0, 0.65);
        self.glow_strength = bounded(self.glow_strength, 0.0, 1.0, 0.24);
        self.particle_brightness = bounded(self.particle_brightness, 0.0, 2.0, 1.15);
        self.particle_count = self.particle_count.clamp(0, 24);
        self.center_mask_radius = bounded(self.center_mask_radius, 0.0, 0.72, 0.16);
        self.center_mask_feather = bounded(self.center_mask_feather, 0.01, 0.80, 0.34);
        self.center_mask_opacity = bounded(self.center_mask_opacity, 0.0, 1.0, 0.94);
        self.orb_speedup_fraction = bounded(self.orb_speedup_fraction, 0.0, 0.50, 0.15);
        self.orb_attack_seconds = bounded(self.orb_attack_seconds, 0.08, 3.0, 0.55);
        self.orb_release_seconds = bounded(self.orb_release_seconds, 0.08, 5.0, 1.20);
    }
}

impl Default for VoicePresentationTuning {
    fn default() -> Self {
        Self {
            pitch_multiplier: 1.0,
            formant_multiplier: 1.0,
            tempo_multiplier: 0.96,
            loudness_multiplier: 1.0,
            attack_multiplier: 1.0,
            release_multiplier: 1.18,
            breathiness_delta: 0.0,
            roughness_delta: 0.0,
            brightness_delta: 0.0,
        }
    }
}

impl VoicePresentationTuning {
    fn sanitize(&mut self) {
        self.pitch_multiplier = bounded(self.pitch_multiplier, 0.75, 1.30, 1.0);
        self.formant_multiplier = bounded(self.formant_multiplier, 0.94, 1.06, 1.0);
        self.tempo_multiplier = bounded(self.tempo_multiplier, 0.65, 1.35, 0.96);
        self.loudness_multiplier = bounded(self.loudness_multiplier, 0.30, 1.25, 1.0);
        self.attack_multiplier = bounded(self.attack_multiplier, 0.72, 1.28, 1.0);
        self.release_multiplier = bounded(self.release_multiplier, 0.75, 1.40, 1.18);
        self.breathiness_delta = bounded(self.breathiness_delta, -0.35, 0.35, 0.0);
        self.roughness_delta = bounded(self.roughness_delta, -0.35, 0.35, 0.0);
        self.brightness_delta = bounded(self.brightness_delta, -0.35, 0.35, 0.0);
    }

    pub fn apply_to_request(self, request: &mut VocalRequest, genome: &VoiceGenome) {
        request.pitch_scale = (request.pitch_scale * self.pitch_multiplier).clamp(0.62, 1.48);
        request.tempo_scale = (request.tempo_scale * self.tempo_multiplier).clamp(0.50, 1.80);
        request.gain =
            (request.gain * self.loudness_multiplier).clamp(0.0, genome.maximum_loudness);
        request.phenotype.formant_scale_multiplier = (request.phenotype.formant_scale_multiplier
            * self.formant_multiplier)
            .clamp(0.94, 1.06);
        request.phenotype.attack_multiplier =
            (request.phenotype.attack_multiplier * self.attack_multiplier).clamp(0.62, 1.35);
        request.phenotype.release_multiplier =
            (request.phenotype.release_multiplier * self.release_multiplier).clamp(0.65, 1.45);
        request.phenotype.breathiness_delta =
            (request.phenotype.breathiness_delta + self.breathiness_delta).clamp(-0.45, 0.45);
        request.phenotype.roughness_delta =
            (request.phenotype.roughness_delta + self.roughness_delta).clamp(-0.45, 0.45);
        request.phenotype.brightness_delta =
            (request.phenotype.brightness_delta + self.brightness_delta).clamp(-0.45, 0.45);
    }
}

impl Default for NervousReadabilityTuning {
    fn default() -> Self {
        Self {
            shape_gain: 1.75,
            breathing_gain: 1.55,
            particle_motion_gain: 1.45,
            particle_spacing_response_gain: 1.40,
            viscosity_response_gain: 1.35,
            cohesion_response_gain: 1.35,
            recovery_response_gain: 1.40,
            material_gain: 1.65,
            soul_glow_gain: 1.55,
            internal_flow_gain: 1.50,
            pulse_gain: 1.45,
            expression_gain: 1.40,
            motion_gain: 1.30,
            voice_gain: 1.12,
            threat_sensitivity: 1.0,
            pain_sensitivity: 1.0,
            contact_sensitivity: 1.0,
            safety_sensitivity: 1.0,
            restraint_sensitivity: 1.0,
            fatigue_sensitivity: 1.0,
            novelty_sensitivity: 1.0,
            startle_sensitivity: 1.0,
            agency_sensitivity: 1.0,
        }
    }
}

impl NervousReadabilityTuning {
    /// Runtime guardrail for a free-moving desktop organism. Pet Lab keeps the
    /// full diagnostic range so every coupling remains inspectable, while the
    /// live Pet prevents a high-gain preset from turning ordinary background
    /// fatigue or approach evidence into persistent sleep-face flight.
    #[must_use]
    pub fn for_live_runtime(self) -> Self {
        Self {
            particle_motion_gain: self.particle_motion_gain.min(2.0),
            expression_gain: self.expression_gain.min(1.55),
            motion_gain: self.motion_gain.min(1.35),
            fatigue_sensitivity: self.fatigue_sensitivity.min(1.25),
            startle_sensitivity: self.startle_sensitivity.min(1.80),
            ..self
        }
    }

    fn sanitize(&mut self) {
        self.shape_gain = bounded(self.shape_gain, 0.0, 3.0, 1.75);
        self.breathing_gain = bounded(self.breathing_gain, 0.0, 3.0, 1.55);
        self.particle_motion_gain = bounded(self.particle_motion_gain, 0.0, 3.0, 1.45);
        self.particle_spacing_response_gain =
            bounded(self.particle_spacing_response_gain, 0.0, 3.0, 1.40);
        self.viscosity_response_gain = bounded(self.viscosity_response_gain, 0.0, 3.0, 1.35);
        self.cohesion_response_gain = bounded(self.cohesion_response_gain, 0.0, 3.0, 1.35);
        self.recovery_response_gain = bounded(self.recovery_response_gain, 0.0, 3.0, 1.40);
        self.material_gain = bounded(self.material_gain, 0.0, 3.0, 1.65);
        self.soul_glow_gain = bounded(self.soul_glow_gain, 0.0, 3.0, 1.55);
        self.internal_flow_gain = bounded(self.internal_flow_gain, 0.0, 3.0, 1.50);
        self.pulse_gain = bounded(self.pulse_gain, 0.0, 3.0, 1.45);
        self.expression_gain = bounded(self.expression_gain, 0.0, 2.5, 1.28);
        self.motion_gain = bounded(self.motion_gain, 0.0, 2.5, 1.20);
        self.voice_gain = bounded(self.voice_gain, 0.0, 2.5, 1.12);
        self.threat_sensitivity = bounded(self.threat_sensitivity, 0.0, 3.0, 1.0);
        self.pain_sensitivity = bounded(self.pain_sensitivity, 0.0, 3.0, 1.0);
        self.contact_sensitivity = bounded(self.contact_sensitivity, 0.0, 3.0, 1.0);
        self.safety_sensitivity = bounded(self.safety_sensitivity, 0.0, 3.0, 1.0);
        self.restraint_sensitivity = bounded(self.restraint_sensitivity, 0.0, 3.0, 1.0);
        self.fatigue_sensitivity = bounded(self.fatigue_sensitivity, 0.0, 3.0, 1.0);
        self.novelty_sensitivity = bounded(self.novelty_sensitivity, 0.0, 3.0, 1.0);
        self.startle_sensitivity = bounded(self.startle_sensitivity, 0.0, 3.0, 1.0);
        self.agency_sensitivity = bounded(self.agency_sensitivity, 0.0, 3.0, 1.0);
    }

    pub fn apply(self, actuation: &mut FastPhenotypeActuation) {
        let around_one = |value: f32, gain: f32, minimum: f32, maximum: f32| {
            (1.0 + (value - 1.0) * gain).clamp(minimum, maximum)
        };
        let unit_gain = |value: f32, neutral: f32, gain: f32| {
            (neutral + (value - neutral) * gain).clamp(0.0, 1.0)
        };

        let shape = self.shape_gain;
        actuation.analytic.body_length_scale =
            around_one(actuation.analytic.body_length_scale, shape, 0.90, 1.10);
        actuation.analytic.body_width_scale =
            around_one(actuation.analytic.body_width_scale, shape, 0.90, 1.10);
        actuation.analytic.roundness_bias =
            (actuation.analytic.roundness_bias * shape).clamp(-0.10, 0.14);
        actuation.analytic.softness_bias =
            (actuation.analytic.softness_bias * shape).clamp(-0.16, 0.18);
        actuation.apparent_scale = around_one(actuation.apparent_scale, shape, 0.92, 1.10);
        let breathing = self.breathing_gain;
        actuation.pbf.idle_breath_amplitude_multiplier = around_one(
            actuation.pbf.idle_breath_amplitude_multiplier,
            breathing,
            0.55,
            1.75,
        );
        actuation.pbf.idle_breath_speed_multiplier = around_one(
            actuation.pbf.idle_breath_speed_multiplier,
            breathing,
            0.45,
            1.90,
        );

        let particle_motion = self.particle_motion_gain;
        actuation.pbf.flight_inertia_multiplier = around_one(
            actuation.pbf.flight_inertia_multiplier,
            particle_motion,
            0.85,
            1.20,
        );
        actuation.pbf.flight_stretch_multiplier = around_one(
            actuation.pbf.flight_stretch_multiplier,
            particle_motion,
            0.72,
            1.40,
        );
        actuation.pbf.flight_damping_multiplier = around_one(
            actuation.pbf.flight_damping_multiplier,
            particle_motion,
            0.82,
            1.28,
        );
        actuation.pbf.flight_max_lag_multiplier = around_one(
            actuation.pbf.flight_max_lag_multiplier,
            particle_motion,
            0.80,
            1.35,
        );
        actuation.pbf.angular_damping_multiplier = around_one(
            actuation.pbf.angular_damping_multiplier,
            particle_motion,
            0.78,
            1.35,
        );
        actuation.pbf.motor_gain_multiplier = around_one(
            actuation.pbf.motor_gain_multiplier,
            particle_motion,
            0.72,
            1.16,
        );
        actuation.pbf.density_compliance_multiplier = around_one(
            actuation.pbf.density_compliance_multiplier,
            self.particle_spacing_response_gain,
            0.75,
            1.25,
        );
        actuation.pbf.viscosity_multiplier = around_one(
            actuation.pbf.viscosity_multiplier,
            self.viscosity_response_gain,
            0.78,
            1.30,
        );
        actuation.pbf.surface_tension_multiplier = around_one(
            actuation.pbf.surface_tension_multiplier,
            self.cohesion_response_gain,
            0.80,
            1.25,
        );
        actuation.pbf.shape_recovery_delta =
            (actuation.pbf.shape_recovery_delta * self.recovery_response_gain).clamp(0.0, 0.42);
        actuation.pbf.upright_stabilization_delta = (actuation.pbf.upright_stabilization_delta
            * self.recovery_response_gain)
            .clamp(-0.35, 0.35);

        let material = self.material_gain;
        actuation.material.hue_shift_turns =
            (actuation.material.hue_shift_turns * material).clamp(-0.04, 0.04);
        actuation.material.saturation_delta =
            (actuation.material.saturation_delta * material).clamp(-0.20, 0.16);
        actuation.material.value_delta =
            (actuation.material.value_delta * material).clamp(-0.18, 0.18);
        actuation.material.emission_multiplier =
            around_one(actuation.material.emission_multiplier, material, 0.55, 1.65);
        actuation.material.soul_glow_strength_multiplier = around_one(
            actuation.material.soul_glow_strength_multiplier,
            self.soul_glow_gain,
            0.55,
            1.60,
        );
        actuation.material.soul_glow_pulse_multiplier = around_one(
            actuation.material.soul_glow_pulse_multiplier,
            self.soul_glow_gain,
            0.50,
            1.60,
        );
        actuation.visual_physiology.pulse_amplitude = unit_gain(
            actuation.visual_physiology.pulse_amplitude,
            0.16,
            self.pulse_gain,
        );
        actuation.visual_physiology.flow_strength_multiplier = around_one(
            actuation.visual_physiology.flow_strength_multiplier,
            self.internal_flow_gain,
            0.45,
            1.75,
        );
        actuation.visual_physiology.flow_speed_multiplier = around_one(
            actuation.visual_physiology.flow_speed_multiplier,
            self.internal_flow_gain,
            0.45,
            1.75,
        );
        actuation.visual_physiology.droplet_energy =
            unit_gain(actuation.visual_physiology.droplet_energy, 0.20, material);

        let expression = self.expression_gain;
        let neutral = ExpressionState::default();
        macro_rules! expression_unit {
            ($field:ident) => {
                actuation.expression.$field =
                    unit_gain(actuation.expression.$field, neutral.$field, expression);
            };
        }
        macro_rules! expression_signed {
            ($field:ident) => {
                actuation.expression.$field =
                    (actuation.expression.$field * expression).clamp(-1.0, 1.0);
            };
        }
        expression_unit!(squint);
        expression_unit!(pupil_size);
        expression_unit!(pupil_focus);
        expression_unit!(brow_tension);
        expression_unit!(mouth_tension);
        expression_unit!(cheek_glow);
        expression_unit!(body_glow);
        expression_unit!(eye_aperture);
        expression_unit!(eye_scale);
        expression_unit!(mouth_compression);
        expression_unit!(effort);
        expression_unit!(relief);
        expression_signed!(brow_raise);
        expression_signed!(mouth_curve);
        expression_signed!(brow_asymmetry);
        expression_signed!(mouth_asymmetry);

        let motion = self.motion_gain;
        actuation.action.speed = (actuation.action.speed * motion).clamp(0.0, 1.0);
        actuation.action.turn = (actuation.action.turn * motion).clamp(-1.0, 1.0);
        actuation.action.approach = (actuation.action.approach * motion).clamp(0.0, 1.0);
        actuation.action.avoid = (actuation.action.avoid * motion).clamp(0.0, 1.0);
        actuation.interaction.lean = (actuation.interaction.lean * motion).clamp(-1.0, 1.0);
        actuation.interaction.recoil = (actuation.interaction.recoil * motion).clamp(0.0, 1.0);
        actuation.interaction.local_pulse =
            (actuation.interaction.local_pulse * motion).clamp(0.0, 1.0);

        let voice = self.voice_gain;
        actuation.voice.pitch_multiplier =
            around_one(actuation.voice.pitch_multiplier, voice, 0.68, 1.42);
        actuation.voice.pitch_variation_multiplier = around_one(
            actuation.voice.pitch_variation_multiplier,
            voice,
            0.55,
            1.55,
        );
        actuation.voice.phrase_speed_multiplier =
            around_one(actuation.voice.phrase_speed_multiplier, voice, 0.55, 1.60);
        actuation.voice.loudness_multiplier =
            around_one(actuation.voice.loudness_multiplier, voice, 0.45, 1.45);
        actuation.voice.attack_multiplier =
            around_one(actuation.voice.attack_multiplier, voice, 0.62, 1.35);
        actuation.voice.release_multiplier =
            around_one(actuation.voice.release_multiplier, voice, 0.65, 1.45);
        actuation.voice.breathiness_delta =
            (actuation.voice.breathiness_delta * voice).clamp(-0.45, 0.45);
        actuation.voice.roughness_delta =
            (actuation.voice.roughness_delta * voice).clamp(-0.45, 0.45);
        actuation.voice.brightness_delta =
            (actuation.voice.brightness_delta * voice).clamp(-0.45, 0.45);
    }
}

impl Default for InteractionTuning {
    fn default() -> Self {
        Self {
            enabled: true,
            topology_mode: TopologyConstraintMode::GuardedNecks,
            contact_weight_floor: 0.04,
            pressure_reference: 1.0,
            signal_smoothing_hz: 14.0,
            gesture_window_seconds: 2.0,
            gesture_commit_confidence: 0.62,
            gesture_ambiguity_margin: 0.12,
            soft_touch_pressure_max: 0.24,
            stretch_strain_min: 0.14,
            stretch_strain_max: 0.58,
            flick_speed_min: 2.40,
            rhythm_interval_cv_max: 0.20,
            rhythm_min_impulses: 3,
            maximum_detached_components: 3,
            maximum_detached_mass_fraction: 0.18,
            minimum_fragment_particles: 4,
            split_hold_seconds: 0.075,
            boundary_strain: 0.72,
            boundary_hold_seconds: 0.45,
            fragment_lifetime_seconds: 10.0,
            offscreen_recovery_delay_seconds: 1.25,
            recovery_field_boost: 1.60,
            turn_wait_seconds: 1.15,
            turn_cooldown_seconds: 1.25,
            response_amplitude: 1.0,
            learning_openness: 1.0,
        }
    }
}

impl InteractionTuning {
    fn sanitize(&mut self) {
        self.contact_weight_floor = bounded(self.contact_weight_floor, 0.0, 0.25, 0.04);
        self.pressure_reference = bounded(self.pressure_reference, 0.05, 8.0, 1.0);
        self.signal_smoothing_hz = bounded(self.signal_smoothing_hz, 1.0, 60.0, 14.0);
        self.gesture_window_seconds = bounded(self.gesture_window_seconds, 0.5, 4.0, 2.0);
        self.gesture_commit_confidence = bounded(self.gesture_commit_confidence, 0.50, 0.85, 0.62);
        self.gesture_ambiguity_margin = bounded(self.gesture_ambiguity_margin, 0.05, 0.35, 0.12);
        self.soft_touch_pressure_max = bounded(self.soft_touch_pressure_max, 0.05, 0.50, 0.24);
        self.stretch_strain_min = bounded(self.stretch_strain_min, 0.05, 0.45, 0.14);
        self.stretch_strain_max = bounded(
            self.stretch_strain_max,
            self.stretch_strain_min + 0.05,
            1.20,
            0.58,
        );
        self.flick_speed_min = bounded(self.flick_speed_min, 0.5, 8.0, 2.40);
        self.rhythm_interval_cv_max = bounded(self.rhythm_interval_cv_max, 0.05, 0.50, 0.20);
        self.rhythm_min_impulses = self.rhythm_min_impulses.clamp(3, 8);
        self.maximum_detached_components = self.maximum_detached_components.clamp(1, 3);
        self.maximum_detached_mass_fraction =
            bounded(self.maximum_detached_mass_fraction, 0.05, 0.25, 0.18);
        self.minimum_fragment_particles = self.minimum_fragment_particles.clamp(3, 12);
        self.split_hold_seconds = bounded(self.split_hold_seconds, 0.025, 0.50, 0.075);
        self.boundary_strain = bounded(self.boundary_strain, 0.45, 1.20, 0.72);
        self.boundary_hold_seconds = bounded(self.boundary_hold_seconds, 0.10, 2.0, 0.45);
        self.fragment_lifetime_seconds = bounded(self.fragment_lifetime_seconds, 3.0, 15.0, 10.0);
        self.offscreen_recovery_delay_seconds =
            bounded(self.offscreen_recovery_delay_seconds, 0.25, 5.0, 1.25);
        self.recovery_field_boost = bounded(self.recovery_field_boost, 1.0, 2.0, 1.60);
        self.turn_wait_seconds = bounded(self.turn_wait_seconds, 0.45, 2.50, 1.15);
        self.turn_cooldown_seconds = bounded(self.turn_cooldown_seconds, 0.25, 5.0, 1.25);
        self.response_amplitude = bounded(self.response_amplitude, 0.25, 1.25, 1.0);
        self.learning_openness = bounded(self.learning_openness, 0.50, 1.25, 1.0);
    }
}

impl Default for AnalyticTuning {
    fn default() -> Self {
        Self {
            body_length_scale: 1.0,
            body_width_scale: 1.0,
            roundness_bias: 0.0,
            softness_bias: 0.0,
            modal_response: 1.0,
            modal_frequency: 1.0,
            modal_damping: 1.0,
            modal_amplitude: 1.0,
        }
    }
}

impl AnalyticTuning {
    fn sanitize(&mut self) {
        self.body_length_scale = bounded(self.body_length_scale, 0.65, 1.45, 1.0);
        self.body_width_scale = bounded(self.body_width_scale, 0.65, 1.45, 1.0);
        self.roundness_bias = bounded(self.roundness_bias, -0.45, 0.45, 0.0);
        self.softness_bias = bounded(self.softness_bias, -0.55, 0.30, 0.0);
        self.modal_response = bounded(self.modal_response, 0.0, 3.0, 1.0);
        self.modal_frequency = bounded(self.modal_frequency, 0.25, 2.5, 1.0);
        self.modal_damping = bounded(self.modal_damping, 0.15, 3.0, 1.0);
        self.modal_amplitude = bounded(self.modal_amplitude, 0.0, 2.0, 1.0);
    }
}

impl Default for PbfTuning {
    fn default() -> Self {
        Self {
            fixed_hz: 120.0,
            particle_count: 96,
            substeps: 1,
            impact_substeps: 1,
            density_iterations: 6,
            impact_density_iterations: 6,
            density_compliance: 8.0e-6,
            rest_density_scale: 1.0,
            scorr_k: 0.005,
            scorr_q_ratio: 0.20,
            scorr_power: 4,
            numerical_xsph: 0.010,
            viscosity: 0.080,
            surface_tension: 0.62,
            shape_recovery: 0.0,
            spacing_scale: 0.88,
            kernel_radius_scale: 1.14,
            motor_gain: 1.0,
            flight_inertia: 0.90,
            flight_stretch: 0.85,
            flight_damping: 4.2,
            flight_max_lag: 0.16,
            angular_damping: 2.0,
            upright_stabilization: 0.0,
            maximum_speed: 5.0,
            grab_radius: 0.100,
            grab_stiffness: 240.0,
            pointer_support_scale: 1.75,
            pointer_response_hz: 18.0,
            grab_damping: 8.0,
            grab_follow: 0.55,
            grab_stretch: 1.40,
            tear_speed: 0.75,
            tear_rate: 12.0,
            grab_com_stabilization: 12.0,
            anisotropy_max: 2.15,
            render_center_smoothing: 0.22,
            render_response_hz: 10.0,
            iso_threshold: 0.34,
            component_link_radius_scale: 1.30,
            return_delay: 1.20,
            return_strength: 0.34,
            character_field_radius_scale: 1.0,
            idle_fragment_interval: 5.4,
            idle_fragment_lifetime: 3.2,
            idle_fragment_size: 0.0,
            idle_bud_interval: 0.90,
            idle_bud_duration: 0.65,
            idle_bud_pull_strength: 0.0,
            idle_bud_neck_scale: 1.0,
            idle_bud_maximum: 2,
            idle_breath_amplitude: 0.015,
            idle_breath_speed: 1.0,
            idle_lean_angle: 0.10,
            idle_lean_rate: 0.12,
            lean_return_half_life: 0.38,
            upright_hold: 4.5,
            neck_continuity: 1.0,
            pinch_bounce: 0.0,
            pinch_spray_count: 4,
            pinch_spray_cone: 0.872_664_63,
            pinch_spray_size: 1.0,
            pinch_spray_speed_variance: 1.0,
            bond_compliance: 4.0e-4,
            bond_create_radius_scale: 1.30,
            bond_create_speed_limit: 0.48,
            bond_yield_strain: 0.14,
            bond_break_strain: 0.82,
            bond_relaxation_time: 0.48,
            bond_iterations: 3,
            bond_neck_scale: 1.0,
        }
    }
}

impl PbfTuning {
    fn apply_v16_solver_defaults(&mut self) {
        self.fixed_hz = 120.0;
        self.substeps = 1;
        self.impact_substeps = 1;
        self.density_iterations = 6;
        self.impact_density_iterations = 6;
        self.numerical_xsph = 0.010;
        self.shape_recovery = 0.0;
        self.upright_stabilization = 0.0;
        self.character_field_radius_scale = 1.0;
        self.pointer_support_scale = 1.75;
        self.pointer_response_hz = 18.0;
        self.idle_fragment_size = 0.0;
        self.idle_bud_pull_strength = 0.0;
        self.pinch_bounce = 0.0;
    }

    fn sanitize(&mut self) {
        // Schema 16 has one authoritative, deterministic time lane. The legacy
        // fields remain serialized so older profiles still load, but cannot create
        // drag/impact-only solver modes.
        self.fixed_hz = 120.0;
        self.particle_count = self.particle_count.clamp(24, 96);
        self.substeps = 1;
        self.impact_substeps = 1;
        self.density_iterations = self.density_iterations.clamp(1, 12);
        self.impact_density_iterations = self.density_iterations;
        self.density_compliance = bounded(self.density_compliance, 1.0e-8, 5.0e-4, 8.0e-6);
        self.rest_density_scale = bounded(self.rest_density_scale, 0.55, 1.65, 1.0);
        self.scorr_k = bounded(self.scorr_k, 0.0, 0.04, 0.005);
        self.scorr_q_ratio = bounded(self.scorr_q_ratio, 0.10, 0.40, 0.20);
        self.scorr_power = self.scorr_power.clamp(2, 8);
        self.numerical_xsph = bounded(self.numerical_xsph, 0.005, 0.030, 0.010);
        self.viscosity = bounded(self.viscosity, 0.0, 0.22, 0.080);
        self.surface_tension = bounded(self.surface_tension, 0.0, 2.5, 0.62);
        self.shape_recovery = bounded(self.shape_recovery, 0.0, 24.0, 0.0);
        self.spacing_scale = bounded(self.spacing_scale, 0.65, 1.45, 1.0);
        self.kernel_radius_scale = bounded(self.kernel_radius_scale, 0.65, 1.70, 1.0);
        self.motor_gain = bounded(self.motor_gain, 0.0, 3.0, 1.0);
        self.flight_inertia = bounded(self.flight_inertia, 0.0, 2.0, 0.90);
        self.flight_stretch = bounded(self.flight_stretch, 0.0, 2.0, 0.85);
        self.flight_damping = bounded(self.flight_damping, 0.5, 12.0, 4.2);
        self.flight_max_lag = bounded(self.flight_max_lag, 0.05, 0.35, 0.16);
        self.angular_damping = bounded(self.angular_damping, 0.0, 3.0, 2.0);
        self.upright_stabilization = bounded(self.upright_stabilization, 0.0, 40.0, 0.0);
        self.maximum_speed = bounded(self.maximum_speed, 0.5, 16.0, 5.0);
        self.grab_radius = bounded(self.grab_radius, 0.03, 0.18, 0.100);
        self.grab_stiffness = bounded(self.grab_stiffness, 20.0, 400.0, 240.0);
        self.pointer_support_scale = bounded(self.pointer_support_scale, 1.5, 2.5, 1.75);
        self.pointer_response_hz = bounded(self.pointer_response_hz, 4.0, 40.0, 18.0);
        self.grab_damping = bounded(self.grab_damping, 0.0, 30.0, 8.0);
        self.grab_follow = bounded(self.grab_follow, 0.0, 1.0, 0.55);
        self.grab_stretch = bounded(self.grab_stretch, 0.0, 3.0, 1.40);
        self.tear_speed = bounded(self.tear_speed, 0.20, 6.0, 0.75);
        self.tear_rate = bounded(self.tear_rate, 0.0, 30.0, 12.0);
        self.grab_com_stabilization = bounded(self.grab_com_stabilization, 0.0, 24.0, 12.0);
        self.anisotropy_max = bounded(self.anisotropy_max, 1.0, 2.15, 2.15);
        self.render_center_smoothing = bounded(self.render_center_smoothing, 0.0, 0.35, 0.22);
        self.render_response_hz = bounded(self.render_response_hz, 1.0, 60.0, 10.0);
        self.iso_threshold = bounded(self.iso_threshold, 0.10, 1.20, 0.34);
        self.component_link_radius_scale =
            bounded(self.component_link_radius_scale, 1.0, 1.75, 1.30);
        self.return_delay = bounded(self.return_delay, 0.0, 5.0, 1.20);
        self.return_strength = bounded(self.return_strength, 0.0, 1.0, 0.34);
        self.character_field_radius_scale =
            bounded(self.character_field_radius_scale, 0.65, 1.5, 1.0);
        self.idle_fragment_interval = bounded(self.idle_fragment_interval, 1.0, 30.0, 5.4);
        self.idle_fragment_lifetime = bounded(self.idle_fragment_lifetime, 0.8, 8.0, 3.2);
        self.idle_fragment_size = bounded(self.idle_fragment_size, 0.0, 0.10, 0.0);
        self.idle_bud_interval = bounded(self.idle_bud_interval, 0.30, 6.0, 0.90);
        self.idle_bud_duration = bounded(self.idle_bud_duration, 0.25, 1.80, 0.65);
        self.idle_bud_pull_strength = bounded(self.idle_bud_pull_strength, 0.0, 12.0, 0.0);
        self.idle_bud_neck_scale = bounded(self.idle_bud_neck_scale, 0.0, 2.5, 1.0);
        self.idle_bud_maximum = self.idle_bud_maximum.clamp(1, 3);
        self.idle_breath_amplitude = bounded(self.idle_breath_amplitude, 0.0, 0.05, 0.015);
        self.idle_breath_speed = bounded(self.idle_breath_speed, 0.20, 2.0, 1.0);
        self.idle_lean_angle = bounded(self.idle_lean_angle, 0.0, 0.18, 0.10);
        self.idle_lean_rate = bounded(self.idle_lean_rate, 0.03, 0.30, 0.12);
        self.lean_return_half_life = bounded(self.lean_return_half_life, 0.12, 1.2, 0.38);
        self.upright_hold = bounded(self.upright_hold, 1.0, 10.0, 4.5);
        self.neck_continuity = bounded(self.neck_continuity, 0.55, 1.65, 1.0);
        self.pinch_bounce = bounded(self.pinch_bounce, 0.0, 2.0, 0.0);
        self.pinch_spray_count = self.pinch_spray_count.clamp(3, 4);
        self.pinch_spray_cone = bounded(self.pinch_spray_cone, 0.35, 1.22, 0.872_664_63);
        self.pinch_spray_size = bounded(self.pinch_spray_size, 0.35, 1.8, 1.0);
        self.pinch_spray_speed_variance = bounded(self.pinch_spray_speed_variance, 0.0, 1.5, 1.0);
        self.bond_compliance = bounded(self.bond_compliance, 1.0e-7, 5.0e-3, 4.0e-4);
        self.bond_create_radius_scale = bounded(self.bond_create_radius_scale, 1.0, 1.75, 1.30);
        self.bond_create_speed_limit = bounded(self.bond_create_speed_limit, 0.05, 3.0, 0.48);
        self.bond_yield_strain = bounded(self.bond_yield_strain, 0.01, 0.75, 0.14);
        self.bond_break_strain = bounded(self.bond_break_strain, 0.08, 2.5, 0.82);
        self.bond_relaxation_time = bounded(self.bond_relaxation_time, 0.03, 3.0, 0.48);
        self.bond_iterations = self.bond_iterations.clamp(1, 10);
        self.bond_neck_scale = bounded(self.bond_neck_scale, 0.0, 2.5, 1.0);
    }
}

impl Default for DropletTuning {
    fn default() -> Self {
        Self {
            count: 8,
            size_scale: 1.0,
            spread_scale: 1.0,
            elasticity: 9.0,
            drag: 4.6,
            lag: 0.18,
            cohesion: 0.68,
            inertia_scale: 1.0,
            maximum_mobile: 2,
            detach_distance: 1.0,
            return_strength: 1.0,
            tether_strength: 1.0,
            bridge_radius_scale: 1.0,
            merge_distance: 1.0,
            stretch_scale: 1.0,
            maximum_distance: 0.90,
        }
    }
}

impl DropletTuning {
    fn sanitize(&mut self) {
        self.count = self.count.clamp(1, 8);
        self.size_scale = bounded(self.size_scale, 0.25, 2.0, 1.0);
        self.spread_scale = bounded(self.spread_scale, 0.20, 2.5, 1.0);
        self.elasticity = bounded(self.elasticity, 0.5, 30.0, 9.0);
        self.drag = bounded(self.drag, 0.1, 20.0, 4.6);
        self.lag = bounded(self.lag, 0.0, 0.60, 0.18);
        self.cohesion = bounded(self.cohesion, 0.0, 1.5, 0.68);
        self.inertia_scale = bounded(self.inertia_scale, 0.0, 3.0, 1.0);
        self.maximum_mobile = self.maximum_mobile.clamp(0, self.count);
        self.detach_distance = bounded(self.detach_distance, 0.25, 2.5, 1.0);
        self.return_strength = bounded(self.return_strength, 0.0, 3.0, 1.0);
        self.tether_strength = bounded(self.tether_strength, 0.0, 3.0, 1.0);
        self.bridge_radius_scale = bounded(self.bridge_radius_scale, 0.0, 2.5, 1.0);
        self.merge_distance = bounded(self.merge_distance, 0.25, 3.0, 1.0);
        self.stretch_scale = bounded(self.stretch_scale, 0.0, 3.0, 1.0);
        self.maximum_distance = bounded(self.maximum_distance, 0.25, 1.5, 0.90);
    }
}

impl Default for MaterialTuning {
    fn default() -> Self {
        Self {
            variant: MaterialVariant::CinematicJelly,
            override_genome_colors: true,
            color_source_mode: ColorSourceMode::GenomeAuthoredBlend,
            genome_color_blend: 0.65,
            mood_color_blend: 0.72,
            primary_hsv: BODY_LAB_PRIMARY_HSV,
            secondary_hsv: BODY_LAB_SECONDARY_HSV,
            glow_hsv: BODY_LAB_GLOW_HSV,
            absorption: 1.0,
            scattering: 0.72,
            thickness: 1.0,
            translucency: 0.72,
            refraction: 1.0,
            blur: 1.15,
            rim_strength: 0.90,
            rim_power: 2.8,
            broad_specular: 0.38,
            broad_specular_power: 12.0,
            tight_specular: 0.95,
            tight_specular_power: 64.0,
            emission: 0.16,
            fresnel_f0: 0.055,
            core_level: 1.35,
            thickness_gamma: 0.78,
            pseudo_depth: 0.30,
            normal_scale: 2.20,
            light_wrap: 0.55,
            ambient_scatter: 0.22,
            direct_scatter: 0.55,
            transmission_hue_preservation: 0.72,
            internal_flow: 0.18,
            halo: 0.035,
            opacity: 0.92,
            cinematic_smoothing: 0.68,
            internal_orb_count: 8,
            internal_orb_intensity: 0.92,
            internal_orb_size: 0.045,
            internal_orb_halo: 1.20,
            internal_orb_speed: 0.32,
            internal_orb_depth: 0.62,
            internal_orb_spread: 0.72,
            studio_intensity: 1.45,
            studio_base_roughness: 0.32,
            studio_coat_roughness: 0.09,
            narrow_rim_strength: 1.0,
            broad_rim_strength: 0.62,
            rim_saturation: 1.15,
            edge_light_width: 18.0,
            caustic_strength: 0.28,
            caustic_scale: 3.4,
            caustic_speed: 0.32,
            caustic_dispersion: 0.45,
            rounded_highlight_strength: 0.85,
            highlight_tint: 0.16,
            soul_glow_count: 4,
            soul_glow_strength: 0.20,
            soul_glow_size: 0.13,
            soul_glow_speed: 0.10,
            soul_glow_pulse: 0.18,
            soul_glow_feather: 0.68,
            bloom_strength: 0.55,
        }
    }
}

impl MaterialTuning {
    fn sanitize(&mut self) {
        self.genome_color_blend = bounded(self.genome_color_blend, 0.0, 1.0, 0.65);
        self.mood_color_blend = bounded(self.mood_color_blend, 0.0, 1.0, 0.72);
        sanitize_hsv(&mut self.primary_hsv);
        sanitize_hsv(&mut self.secondary_hsv);
        sanitize_hsv(&mut self.glow_hsv);
        self.absorption = bounded(self.absorption, 0.0, 4.0, 1.0);
        self.scattering = bounded(self.scattering, 0.0, 4.0, 0.72);
        self.thickness = bounded(self.thickness, 0.10, 4.0, 1.0);
        self.translucency = bounded(self.translucency, 0.0, 1.0, 0.72);
        self.refraction = bounded(self.refraction, 0.0, 3.0, 1.0);
        self.blur = bounded(self.blur, 0.0, 5.0, 1.15);
        self.rim_strength = bounded(self.rim_strength, 0.0, 4.0, 0.90);
        self.rim_power = bounded(self.rim_power, 0.5, 12.0, 2.8);
        self.broad_specular = bounded(self.broad_specular, 0.0, 1.0, 0.38);
        self.broad_specular_power = bounded(self.broad_specular_power, 4.0, 64.0, 12.0);
        self.tight_specular = bounded(self.tight_specular, 0.0, 1.5, 0.95);
        self.tight_specular_power = bounded(self.tight_specular_power, 24.0, 192.0, 64.0);
        self.emission = bounded(self.emission, 0.0, 2.0, 0.16);
        self.fresnel_f0 = bounded(self.fresnel_f0, 0.01, 0.12, 0.055);
        self.core_level = bounded(self.core_level, 0.55, 3.0, 1.35);
        self.thickness_gamma = bounded(self.thickness_gamma, 0.25, 2.5, 0.78);
        self.pseudo_depth = bounded(self.pseudo_depth, 0.05, 0.60, 0.30);
        self.normal_scale = bounded(self.normal_scale, 0.20, 4.0, 2.20);
        self.light_wrap = bounded(self.light_wrap, 0.0, 1.5, 0.55);
        self.ambient_scatter = bounded(self.ambient_scatter, 0.0, 1.5, 0.22);
        self.direct_scatter = bounded(self.direct_scatter, 0.0, 2.0, 0.55);
        self.transmission_hue_preservation =
            bounded(self.transmission_hue_preservation, 0.0, 1.0, 0.72);
        self.internal_flow = bounded(self.internal_flow, 0.0, 2.0, 0.18);
        self.halo = bounded(self.halo, 0.0, 0.30, 0.035);
        self.opacity = bounded(self.opacity, 0.05, 1.0, 0.92);
        self.cinematic_smoothing = bounded(self.cinematic_smoothing, 0.0, 1.0, 0.68);
        self.internal_orb_count = self.internal_orb_count.clamp(0, 8);
        self.internal_orb_intensity = bounded(self.internal_orb_intensity, 0.0, 4.0, 0.92);
        self.internal_orb_size = bounded(self.internal_orb_size, 0.012, 0.10, 0.045);
        self.internal_orb_halo = bounded(self.internal_orb_halo, 0.0, 3.0, 1.20);
        self.internal_orb_speed = bounded(self.internal_orb_speed, 0.0, 2.0, 0.32);
        self.internal_orb_depth = bounded(self.internal_orb_depth, 0.0, 1.0, 0.62);
        self.internal_orb_spread = bounded(self.internal_orb_spread, 0.0, 1.0, 0.72);
        self.studio_intensity = bounded(self.studio_intensity, 0.0, 4.0, 1.45);
        self.studio_base_roughness = bounded(self.studio_base_roughness, 0.05, 1.0, 0.32);
        self.studio_coat_roughness = bounded(self.studio_coat_roughness, 0.02, 0.50, 0.09);
        self.narrow_rim_strength = bounded(self.narrow_rim_strength, 0.0, 3.0, 1.0);
        self.broad_rim_strength = bounded(self.broad_rim_strength, 0.0, 3.0, 0.62);
        self.rim_saturation = bounded(self.rim_saturation, 0.0, 1.6, 1.15);
        self.edge_light_width = bounded(self.edge_light_width, 2.0, 32.0, 18.0);
        self.caustic_strength = bounded(self.caustic_strength, 0.0, 4.0, 0.28);
        self.caustic_scale = bounded(self.caustic_scale, 0.5, 12.0, 3.4);
        self.caustic_speed = bounded(self.caustic_speed, 0.0, 2.0, 0.32);
        self.caustic_dispersion = bounded(self.caustic_dispersion, 0.0, 1.0, 0.45);
        self.rounded_highlight_strength = bounded(self.rounded_highlight_strength, 0.0, 3.0, 0.85);
        self.highlight_tint = bounded(self.highlight_tint, 0.0, 0.5, 0.16);
        self.soul_glow_count = self.soul_glow_count.clamp(0, 6);
        self.soul_glow_strength = bounded(self.soul_glow_strength, 0.0, 2.0, 0.20);
        self.soul_glow_size = bounded(self.soul_glow_size, 0.04, 0.30, 0.13);
        self.soul_glow_speed = bounded(self.soul_glow_speed, 0.0, 0.50, 0.10);
        self.soul_glow_pulse = bounded(self.soul_glow_pulse, 0.0, 0.60, 0.18);
        self.soul_glow_feather = bounded(self.soul_glow_feather, 0.20, 1.50, 0.68);
        self.bloom_strength = bounded(self.bloom_strength, 0.0, 3.0, 0.55);
    }
}

const fn safe_material_variant() -> MaterialVariant {
    MaterialVariant::CurrentSafe
}

impl Default for FaceTuning {
    fn default() -> Self {
        Self {
            visible: true,
            override_iris_color: false,
            iris_hsv: [0.055, 0.82, 0.48],
            origin: [0.0, 0.13],
            scale: [1.0, 1.0],
            eye_size_scale: 1.0,
            eye_spacing_scale: 1.0,
            pupil_scale: 1.0,
            eye_highlight_scale: 1.45,
            eye_socket_strength: 0.18,
            relief_strength: 0.55,
            relief_darkness: 0.72,
            relief_coat_strength: 1.20,
            pupil_light_response: 1.0,
            pupil_emotion_response: 1.0,
            pupil_focus_response: 1.0,
            microsaccade_amount: 1.0,
            microsaccade_rate: 1.0,
            maximum_roll_radians: 0.0,
            translation_smoothing: 16.0,
            rotation_smoothing: 10.0,
            scale_smoothing: 7.0,
        }
    }
}

impl FaceTuning {
    fn sanitize(&mut self) {
        sanitize_hsv(&mut self.iris_hsv);
        self.origin[0] = bounded(self.origin[0], -0.25, 0.25, 0.0);
        self.origin[1] = bounded(self.origin[1], -0.15, 0.35, 0.13);
        self.scale[0] = bounded(self.scale[0], 0.55, 1.50, 1.0);
        self.scale[1] = bounded(self.scale[1], 0.55, 1.50, 1.0);
        // v16 promises lossless migration of authored face proportions. A few
        // v15 profiles intentionally used eyes above the old Lab slider cap.
        self.eye_size_scale = bounded(self.eye_size_scale, 0.55, 2.0, 1.0);
        self.eye_spacing_scale = bounded(self.eye_spacing_scale, 0.60, 1.50, 1.0);
        self.pupil_scale = bounded(self.pupil_scale, 0.50, 1.50, 1.0);
        self.eye_highlight_scale = bounded(self.eye_highlight_scale, 0.50, 2.50, 1.45);
        self.eye_socket_strength = bounded(self.eye_socket_strength, 0.0, 1.0, 0.18);
        self.relief_strength = bounded(self.relief_strength, 0.0, 1.5, 0.55);
        self.relief_darkness = bounded(self.relief_darkness, 0.35, 0.95, 0.72);
        self.relief_coat_strength = bounded(self.relief_coat_strength, 0.0, 2.5, 1.20);
        self.pupil_light_response = bounded(self.pupil_light_response, 0.0, 2.0, 1.0);
        self.pupil_emotion_response = bounded(self.pupil_emotion_response, 0.0, 2.0, 1.0);
        self.pupil_focus_response = bounded(self.pupil_focus_response, 0.0, 2.0, 1.0);
        self.microsaccade_amount = bounded(self.microsaccade_amount, 0.0, 2.0, 1.0);
        self.microsaccade_rate = bounded(self.microsaccade_rate, 0.0, 2.0, 1.0);
        self.maximum_roll_radians = bounded(self.maximum_roll_radians, 0.0, 0.45, 0.0);
        self.translation_smoothing = bounded(self.translation_smoothing, 0.5, 40.0, 16.0);
        self.rotation_smoothing = bounded(self.rotation_smoothing, 0.5, 40.0, 10.0);
        self.scale_smoothing = bounded(self.scale_smoothing, 0.5, 40.0, 7.0);
    }
}

impl Default for CompositorTuning {
    fn default() -> Self {
        Self {
            render_scale: 2,
            shadow_horizontal_offset: 0.0,
            shadow_vertical_offset: 9.0,
            shadow_feather: 22.0,
            shadow_opacity: 0.050,
            shadow_color: [0.008, 0.012, 0.020],
            exposure: 1.0,
        }
    }
}

impl CompositorTuning {
    fn sanitize(&mut self) {
        self.render_scale = self.render_scale.clamp(1, 2);
        self.shadow_horizontal_offset = bounded(self.shadow_horizontal_offset, -96.0, 96.0, 0.0);
        self.shadow_vertical_offset = bounded(self.shadow_vertical_offset, -96.0, 96.0, 9.0);
        self.shadow_feather = bounded(self.shadow_feather, 2.0, 128.0, 22.0);
        self.shadow_opacity = bounded(self.shadow_opacity, 0.0, 0.50, 0.050);
        for channel in &mut self.shadow_color {
            *channel = bounded(*channel, 0.0, 1.0, 0.01);
        }
        self.exposure = bounded(self.exposure, 0.25, 3.0, 1.0);
    }
}

fn sanitize_hsv(value: &mut [f32; 3]) {
    value[0] = bounded(value[0], 0.0, 1.0, 0.5);
    value[1] = bounded(value[1], 0.0, 1.0, 0.5);
    value[2] = bounded(value[2], 0.0, 2.0, 0.8);
}

fn bounded(value: f32, minimum: f32, maximum: f32, fallback: f32) -> f32 {
    if value.is_finite() {
        value.clamp(minimum, maximum)
    } else {
        fallback
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_round_trips_with_safe_analytic_runtime_and_cinematic_lab_material() {
        let mut profile = LiquidTuningProfile::for_seed(42);
        profile.face.override_iris_color = true;
        profile.face.iris_hsv = [0.31, 0.72, 0.91];
        assert_eq!(profile.render_mode, BodyRenderMode::AnalyticJelly);
        assert_eq!(profile.material.variant, MaterialVariant::CinematicJelly);
        let json = serde_json::to_string(&profile).unwrap();
        let decoded: LiquidTuningProfile = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.sanitized().unwrap(), profile);
    }

    #[test]
    fn schema_twenty_one_defaults_keep_the_single_stable_solver_lane() {
        let pbf = PbfTuning::default();
        assert_eq!(LIQUID_TUNING_SCHEMA_VERSION, 21);
        assert_eq!(pbf.substeps, 1);
        assert_eq!(pbf.impact_substeps, 1);
        assert_eq!(pbf.density_iterations, 6);
        assert_eq!(pbf.impact_density_iterations, 6);
        assert_eq!(pbf.numerical_xsph, 0.010);
        assert_eq!(pbf.character_field_radius_scale, 1.0);
        assert_eq!(pbf.pointer_support_scale, 1.75);
        assert_eq!(pbf.pointer_response_hz, 18.0);
        assert_eq!(pbf.shape_recovery, 0.0);
        assert_eq!(pbf.upright_stabilization, 0.0);
        assert_eq!(pbf.idle_fragment_size, 0.0);
        assert_eq!(pbf.idle_bud_pull_strength, 0.0);
        assert_eq!(pbf.pinch_bounce, 0.0);
    }

    #[test]
    fn nervous_readability_amplifies_dynamic_parcel_response_not_structural_settings() {
        let mut actuation = FastPhenotypeActuation::default();
        actuation.pbf.density_compliance_multiplier = 1.10;
        actuation.pbf.viscosity_multiplier = 0.90;
        actuation.pbf.surface_tension_multiplier = 1.08;
        actuation.pbf.flight_stretch_multiplier = 1.12;
        actuation.pbf.shape_recovery_delta = 0.10;
        actuation.visual_physiology.pulse_amplitude = 0.40;
        let original = actuation.clone();
        NervousReadabilityTuning {
            particle_motion_gain: 2.0,
            particle_spacing_response_gain: 2.0,
            viscosity_response_gain: 2.0,
            cohesion_response_gain: 2.0,
            recovery_response_gain: 2.0,
            pulse_gain: 2.0,
            ..NervousReadabilityTuning::default()
        }
        .apply(&mut actuation);

        assert!(
            (actuation.pbf.density_compliance_multiplier - 1.0).abs()
                > (original.pbf.density_compliance_multiplier - 1.0).abs()
        );
        assert!(
            (actuation.pbf.viscosity_multiplier - 1.0).abs()
                > (original.pbf.viscosity_multiplier - 1.0).abs()
        );
        assert!(
            (actuation.pbf.surface_tension_multiplier - 1.0).abs()
                > (original.pbf.surface_tension_multiplier - 1.0).abs()
        );
        assert!(
            (actuation.pbf.flight_stretch_multiplier - 1.0).abs()
                > (original.pbf.flight_stretch_multiplier - 1.0).abs()
        );
        assert!(actuation.pbf.shape_recovery_delta > original.pbf.shape_recovery_delta);
        assert!(
            (actuation.visual_physiology.pulse_amplitude - 0.16).abs()
                > (original.visual_physiology.pulse_amplitude - 0.16).abs()
        );

        let profile = LiquidTuningProfile::default();
        assert_eq!(
            profile.pbf.particle_count,
            PbfTuning::default().particle_count
        );
        assert_eq!(profile.pbf.fixed_hz, PbfTuning::default().fixed_hz);
        assert_eq!(
            profile.pbf.spacing_scale,
            PbfTuning::default().spacing_scale
        );
    }

    #[test]
    fn live_nervous_guardrail_preserves_visual_diagnostics_but_caps_motion_and_fatigue() {
        let diagnostic = NervousReadabilityTuning {
            shape_gain: 2.8,
            material_gain: 2.8,
            particle_motion_gain: 2.8,
            expression_gain: 2.4,
            motion_gain: 2.4,
            fatigue_sensitivity: 2.6,
            startle_sensitivity: 2.7,
            ..NervousReadabilityTuning::default()
        };

        let live = diagnostic.for_live_runtime();

        assert_eq!(live.shape_gain, diagnostic.shape_gain);
        assert_eq!(live.material_gain, diagnostic.material_gain);
        assert_eq!(live.particle_motion_gain, 2.0);
        assert_eq!(live.expression_gain, 1.55);
        assert_eq!(live.motion_gain, 1.35);
        assert_eq!(live.fatigue_sensitivity, 1.25);
        assert_eq!(live.startle_sensitivity, 1.80);
    }

    #[test]
    fn schema_seventeen_authored_override_migrates_to_identity_blend() {
        let mut profile = LiquidTuningProfile::for_seed(42);
        profile.schema_version = 17;
        profile.material.override_genome_colors = true;
        profile.material.color_source_mode = ColorSourceMode::Authored;
        profile.material.genome_color_blend = 0.0;
        let migrated = profile.sanitized().expect("schema 17 migration");
        assert_eq!(migrated.schema_version, 21);
        assert_eq!(
            migrated.material.color_source_mode,
            ColorSourceMode::GenomeAuthoredBlend
        );
        assert_eq!(migrated.material.genome_color_blend, 0.65);
        assert_eq!(migrated.material.mood_color_blend, 0.72);
    }

    #[test]
    fn schema_eighteen_gains_the_bounded_mood_color_blend() {
        let mut profile = LiquidTuningProfile::for_seed(42);
        profile.schema_version = 18;
        profile.material.mood_color_blend = 0.0;
        let migrated = profile.sanitized().expect("schema 18 migration");
        assert_eq!(migrated.schema_version, 21);
        assert_eq!(migrated.material.mood_color_blend, 0.72);
    }

    #[test]
    fn schema_sixteen_gains_safe_interaction_defaults() {
        let profile = LiquidTuningProfile::for_seed(42);
        let mut value = serde_json::to_value(&profile).unwrap();
        value["schema_version"] = serde_json::json!(16);
        value.as_object_mut().unwrap().remove("interaction");
        let migrated: LiquidTuningProfile = serde_json::from_value(value).unwrap();
        let migrated = migrated.sanitized().unwrap();
        assert_eq!(migrated.schema_version, LIQUID_TUNING_SCHEMA_VERSION);
        assert_eq!(
            migrated.interaction.topology_mode,
            TopologyConstraintMode::GuardedNecks
        );
        assert_eq!(migrated.interaction.maximum_detached_components, 3);
        assert_eq!(migrated.interaction.maximum_detached_mass_fraction, 0.18);
    }

    #[test]
    fn non_finite_values_are_replaced_and_structural_values_are_bounded() {
        let mut profile = LiquidTuningProfile::default();
        profile.material.blur = f32::NAN;
        profile.material.internal_orb_count = usize::MAX;
        profile.material.internal_orb_intensity = f32::INFINITY;
        profile.pbf.upright_stabilization = f32::NAN;
        profile.pbf.numerical_xsph = f32::NAN;
        profile.pbf.character_field_radius_scale = 9.0;
        profile.pbf.pointer_support_scale = 0.0;
        profile.pbf.pointer_response_hz = f32::NAN;
        profile.pbf.render_response_hz = f32::NAN;
        profile.pbf.fixed_hz = 240.0;
        profile.pbf.particle_count = usize::MAX;
        profile.droplets.count = 0;
        let clean = profile.sanitized().unwrap();
        assert_eq!(clean.material.blur, 1.15);
        assert_eq!(clean.material.internal_orb_count, 8);
        assert_eq!(clean.material.internal_orb_intensity, 0.92);
        assert_eq!(clean.pbf.upright_stabilization, 0.0);
        assert_eq!(clean.pbf.numerical_xsph, 0.010);
        assert_eq!(clean.pbf.character_field_radius_scale, 1.5);
        assert_eq!(clean.pbf.pointer_support_scale, 1.5);
        assert_eq!(clean.pbf.pointer_response_hz, 18.0);
        assert_eq!(clean.pbf.render_response_hz, 10.0);
        assert_eq!(clean.pbf.fixed_hz, 120.0);
        assert_eq!(clean.pbf.particle_count, 96);
        assert_eq!(clean.droplets.count, 1);
    }

    #[test]
    fn unknown_schema_is_rejected_instead_of_silently_changing_the_pet() {
        let mut profile = LiquidTuningProfile::default();
        profile.schema_version += 1;
        assert!(matches!(
            profile.sanitized(),
            Err(TuningProfileError::UnsupportedSchema { .. })
        ));
    }

    #[test]
    fn schema_six_profiles_migrate_with_zero_g_drag_defaults() {
        let profile = LiquidTuningProfile::for_seed(73);
        let mut value = serde_json::to_value(&profile).unwrap();
        value["schema_version"] = serde_json::json!(6);
        let pbf = value["pbf"].as_object_mut().unwrap();
        pbf.remove("grab_follow");
        pbf.remove("grab_stretch");
        let legacy: LiquidTuningProfile = serde_json::from_value(value).unwrap();
        let migrated = legacy.sanitized().unwrap();

        assert_eq!(migrated.schema_version, LIQUID_TUNING_SCHEMA_VERSION);
        assert_eq!(migrated.pbf.grab_follow, 0.55);
        assert_eq!(migrated.pbf.grab_stretch, 1.40);
        assert_eq!(migrated.material.variant, MaterialVariant::CurrentSafe);
    }

    #[test]
    fn schema_seven_profiles_migrate_to_the_preserved_material() {
        let profile = LiquidTuningProfile::for_seed(91);
        let mut value = serde_json::to_value(&profile).unwrap();
        value["schema_version"] = serde_json::json!(7);
        value["material"].as_object_mut().unwrap().remove("variant");
        let legacy: LiquidTuningProfile = serde_json::from_value(value).unwrap();
        let migrated = legacy.sanitized().unwrap();

        assert_eq!(migrated.schema_version, LIQUID_TUNING_SCHEMA_VERSION);
        assert_eq!(migrated.material.variant, MaterialVariant::CurrentSafe);
    }

    #[test]
    fn schema_eight_profiles_migrate_away_from_the_rejected_v1_material() {
        let profile = LiquidTuningProfile::for_seed(109);
        let mut value = serde_json::to_value(&profile).unwrap();
        value["schema_version"] = serde_json::json!(8);
        let material = value["material"].as_object_mut().unwrap();
        for key in [
            "studio_intensity",
            "studio_base_roughness",
            "studio_coat_roughness",
            "edge_light_width",
            "caustic_strength",
            "caustic_scale",
            "caustic_speed",
            "bloom_strength",
        ] {
            material.remove(key);
        }
        let legacy: LiquidTuningProfile = serde_json::from_value(value).unwrap();
        let migrated = legacy.sanitized().unwrap();

        assert_eq!(migrated.schema_version, LIQUID_TUNING_SCHEMA_VERSION);
        assert_eq!(migrated.material.variant, MaterialVariant::CurrentSafe);
        assert_eq!(migrated.material.edge_light_width, 18.0);
        assert_eq!(migrated.material.bloom_strength, 0.55);
    }

    #[test]
    fn schema_nine_profiles_preserve_cinematic_authored_values() {
        let mut profile = LiquidTuningProfile::for_seed(127);
        profile.schema_version = 9;
        profile.material.variant = MaterialVariant::CinematicJelly;
        profile.material.caustic_strength = 1.37;
        profile.material.edge_light_width = 23.0;
        profile.pbf.viscosity = 0.047;
        let mut value = serde_json::to_value(&profile).unwrap();
        let material = value["material"].as_object_mut().unwrap();
        for key in [
            "narrow_rim_strength",
            "broad_rim_strength",
            "caustic_dispersion",
            "rounded_highlight_strength",
            "highlight_tint",
            "soul_glow_count",
            "soul_glow_strength",
            "soul_glow_size",
            "soul_glow_speed",
            "soul_glow_pulse",
            "soul_glow_feather",
        ] {
            material.remove(key);
        }
        let pbf = value["pbf"].as_object_mut().unwrap();
        for key in [
            "idle_bud_interval",
            "idle_bud_duration",
            "idle_bud_pull_strength",
            "idle_bud_neck_scale",
            "idle_bud_maximum",
            "idle_breath_amplitude",
            "idle_breath_speed",
        ] {
            pbf.remove(key);
        }
        let legacy: LiquidTuningProfile = serde_json::from_value(value).unwrap();
        let migrated = legacy.sanitized().unwrap();

        assert_eq!(migrated.schema_version, LIQUID_TUNING_SCHEMA_VERSION);
        assert_eq!(migrated.material.variant, MaterialVariant::CinematicJelly);
        assert_eq!(migrated.material.caustic_strength, 1.37);
        assert_eq!(migrated.material.edge_light_width, 23.0);
        assert_eq!(migrated.pbf.viscosity, 0.047);
        assert_eq!(migrated.material.caustic_dispersion, 0.45);
        assert_eq!(migrated.material.soul_glow_count, 4);
        assert_eq!(migrated.pbf.idle_bud_interval, 0.90);
        assert_eq!(migrated.pbf.idle_breath_amplitude, 0.015);
    }

    #[test]
    fn schema_ten_profiles_preserve_variant_and_gain_v31_defaults() {
        let mut profile = LiquidTuningProfile::for_seed(211);
        profile.schema_version = 10;
        profile.material.variant = MaterialVariant::CinematicJelly;
        profile.material.studio_intensity = 2.17;
        profile.pbf.viscosity = 0.043;
        let mut value = serde_json::to_value(&profile).unwrap();
        let material = value["material"].as_object_mut().unwrap();
        material.remove("internal_orb_spread");
        material.remove("rim_saturation");
        let pbf = value["pbf"].as_object_mut().unwrap();
        pbf.remove("idle_lean_angle");
        pbf.remove("idle_lean_rate");

        let migrated: LiquidTuningProfile = serde_json::from_value(value).unwrap();
        let migrated = migrated.sanitized().unwrap();

        assert_eq!(migrated.schema_version, LIQUID_TUNING_SCHEMA_VERSION);
        assert_eq!(migrated.material.variant, MaterialVariant::CinematicJelly);
        assert_eq!(migrated.material.studio_intensity, 2.17);
        assert_eq!(migrated.pbf.viscosity, 0.043);
        assert_eq!(migrated.material.internal_orb_spread, 0.72);
        assert_eq!(migrated.material.rim_saturation, 1.15);
        assert_eq!(migrated.pbf.idle_lean_angle, 0.10);
        assert_eq!(migrated.pbf.idle_lean_rate, 0.12);
    }

    #[test]
    fn schema_eleven_profiles_gain_v32_controls_without_losing_authored_values() {
        let mut profile = LiquidTuningProfile::for_seed(0x12);
        profile.schema_version = 11;
        profile.profile_revision = 41;
        profile.material.variant = MaterialVariant::CinematicJelly;
        profile.material.studio_intensity = 1.73;
        profile.pbf.viscosity = 0.039;
        let mut value = serde_json::to_value(&profile).unwrap();
        value.as_object_mut().unwrap().remove("profile_revision");
        let pbf = value["pbf"].as_object_mut().unwrap();
        for key in [
            "lean_return_half_life",
            "upright_hold",
            "neck_continuity",
            "pinch_bounce",
            "pinch_spray_count",
            "pinch_spray_cone",
            "pinch_spray_size",
            "pinch_spray_speed_variance",
        ] {
            pbf.remove(key);
        }
        let face = value["face"].as_object_mut().unwrap();
        for key in [
            "relief_darkness",
            "relief_coat_strength",
            "pupil_light_response",
            "pupil_emotion_response",
            "pupil_focus_response",
            "microsaccade_amount",
            "microsaccade_rate",
        ] {
            face.remove(key);
        }

        let migrated: LiquidTuningProfile = serde_json::from_value(value).unwrap();
        let migrated = migrated.sanitized().unwrap();
        assert_eq!(migrated.schema_version, LIQUID_TUNING_SCHEMA_VERSION);
        assert_eq!(migrated.profile_revision, 0);
        assert_eq!(migrated.material.variant, MaterialVariant::CinematicJelly);
        assert_eq!(migrated.material.studio_intensity, 1.73);
        assert_eq!(migrated.pbf.viscosity, 0.039);
        assert_eq!(migrated.pbf.pinch_spray_count, 4);
        assert_eq!(migrated.pbf.neck_continuity, 1.0);
        assert_eq!(migrated.face.relief_darkness, 0.72);
        assert_eq!(migrated.face.microsaccade_rate, 1.0);
    }

    #[test]
    fn schema_twelve_cinematic_profile_adopts_the_approved_lab_palette() {
        let mut profile = LiquidTuningProfile::for_seed(42);
        profile.schema_version = 12;
        profile.profile_revision = 7;
        profile.material.variant = MaterialVariant::CinematicJelly;
        profile.material.override_genome_colors = false;
        profile.material.studio_intensity = 2.17;

        let migrated = profile.sanitized().unwrap();
        assert_eq!(migrated.schema_version, LIQUID_TUNING_SCHEMA_VERSION);
        assert_eq!(migrated.profile_revision, 7);
        assert!(migrated.material.override_genome_colors);
        assert_eq!(migrated.material.primary_hsv, BODY_LAB_PRIMARY_HSV);
        assert_eq!(migrated.material.secondary_hsv, BODY_LAB_SECONDARY_HSV);
        assert_eq!(migrated.material.glow_hsv, BODY_LAB_GLOW_HSV);
        assert_eq!(migrated.material.studio_intensity, 2.17);
    }

    #[test]
    fn schema_thirteen_preserves_authored_liquid_and_gains_comoving_flight_defaults() {
        let mut profile = LiquidTuningProfile::for_seed(0xF1137);
        profile.schema_version = 13;
        profile.profile_revision = 52;
        profile.material.variant = MaterialVariant::CinematicJelly;
        profile.pbf.viscosity = 0.047;
        let mut value = serde_json::to_value(&profile).unwrap();
        let pbf = value["pbf"].as_object_mut().unwrap();
        pbf.remove("flight_inertia");
        pbf.remove("flight_stretch");
        pbf.remove("flight_damping");
        pbf.remove("flight_max_lag");

        let migrated: LiquidTuningProfile = serde_json::from_value(value).unwrap();
        let migrated = migrated.sanitized().unwrap();
        assert_eq!(migrated.schema_version, LIQUID_TUNING_SCHEMA_VERSION);
        assert_eq!(migrated.profile_revision, 52);
        assert_eq!(migrated.material.variant, MaterialVariant::CinematicJelly);
        assert_eq!(migrated.pbf.viscosity, 0.047);
        assert_eq!(migrated.pbf.flight_inertia, 0.90);
        assert_eq!(migrated.pbf.flight_stretch, 0.85);
        assert_eq!(migrated.pbf.flight_damping, 4.2);
        assert_eq!(migrated.pbf.flight_max_lag, 0.16);
    }

    #[test]
    fn schema_fourteen_profiles_gain_drop_shadow_controls_and_accept_blur_alias() {
        let mut profile = LiquidTuningProfile::for_seed(0x5A_D0);
        profile.schema_version = 14;
        profile.profile_revision = 73;
        profile.material.primary_hsv = [0.13, 0.42, 0.77];
        profile.compositor.shadow_feather = 37.0;
        let mut value = serde_json::to_value(&profile).unwrap();
        let compositor = value["compositor"].as_object_mut().unwrap();
        let legacy_feather = compositor.remove("shadow_feather").unwrap();
        compositor.insert("shadow_blur_radius".to_owned(), legacy_feather);
        for key in ["shadow_horizontal_offset", "shadow_color"] {
            compositor.remove(key);
        }

        let migrated: LiquidTuningProfile = serde_json::from_value(value).unwrap();
        let migrated = migrated.sanitized().unwrap();
        assert_eq!(migrated.schema_version, LIQUID_TUNING_SCHEMA_VERSION);
        assert_eq!(migrated.profile_revision, 73);
        assert_eq!(migrated.material.primary_hsv, [0.13, 0.42, 0.77]);
        assert_eq!(migrated.compositor.shadow_feather, 37.0);
        assert_eq!(migrated.compositor.shadow_horizontal_offset, 0.0);
        assert_eq!(migrated.compositor.shadow_color, [0.008, 0.012, 0.020]);
    }

    #[test]
    fn schema_fifteen_preserves_authored_look_and_pointer_strength_but_resets_solver() {
        let mut profile = LiquidTuningProfile::for_seed(0x16);
        profile.schema_version = 15;
        profile.profile_revision = 91;
        profile.render_mode = BodyRenderMode::ParticlePbf;
        profile.material.variant = MaterialVariant::CinematicJelly;
        profile.material.primary_hsv = [0.11, 0.52, 0.83];
        profile.material.studio_intensity = 2.03;
        profile.face.eye_size_scale = 1.97;
        profile.compositor.shadow_feather = 84.0;
        profile.compositor.shadow_opacity = 0.41;
        profile.pbf.grab_stiffness = 317.0;
        profile.pbf.viscosity = 0.0;
        profile.pbf.surface_tension = 1.17;
        profile.pbf.substeps = 4;
        profile.pbf.impact_substeps = 6;
        profile.pbf.density_iterations = 2;
        profile.pbf.impact_density_iterations = 12;
        profile.pbf.shape_recovery = 22.0;
        profile.pbf.upright_stabilization = 31.0;
        profile.pbf.idle_fragment_size = 0.09;
        profile.pbf.idle_bud_pull_strength = 11.0;
        profile.pbf.pinch_bounce = 1.9;

        let mut value = serde_json::to_value(&profile).unwrap();
        let pbf = value["pbf"].as_object_mut().unwrap();
        for key in [
            "numerical_xsph",
            "character_field_radius_scale",
            "pointer_support_scale",
            "pointer_response_hz",
        ] {
            pbf.remove(key);
        }

        let legacy: LiquidTuningProfile = serde_json::from_value(value).unwrap();
        let migrated = legacy.sanitized().unwrap();
        assert_eq!(migrated.schema_version, LIQUID_TUNING_SCHEMA_VERSION);
        assert_eq!(migrated.profile_revision, 91);
        assert_eq!(migrated.render_mode, BodyRenderMode::ParticlePbf);
        assert_eq!(migrated.material.variant, MaterialVariant::CinematicJelly);
        assert_eq!(migrated.material.primary_hsv, [0.11, 0.52, 0.83]);
        assert_eq!(migrated.material.studio_intensity, 2.03);
        assert_eq!(migrated.face.eye_size_scale, 1.97);
        assert_eq!(migrated.compositor.shadow_feather, 84.0);
        assert_eq!(migrated.compositor.shadow_opacity, 0.41);
        assert_eq!(migrated.pbf.grab_stiffness, 317.0);
        assert_eq!(migrated.pbf.viscosity, 0.0);
        assert_eq!(migrated.pbf.surface_tension, 1.17);
        assert_eq!(migrated.pbf.substeps, 1);
        assert_eq!(migrated.pbf.impact_substeps, 1);
        assert_eq!(migrated.pbf.density_iterations, 6);
        assert_eq!(migrated.pbf.impact_density_iterations, 6);
        assert_eq!(migrated.pbf.numerical_xsph, 0.010);
        assert_eq!(migrated.pbf.character_field_radius_scale, 1.0);
        assert_eq!(migrated.pbf.pointer_support_scale, 1.75);
        assert_eq!(migrated.pbf.pointer_response_hz, 18.0);
        assert_eq!(migrated.pbf.shape_recovery, 0.0);
        assert_eq!(migrated.pbf.upright_stabilization, 0.0);
        assert_eq!(migrated.pbf.idle_fragment_size, 0.0);
        assert_eq!(migrated.pbf.idle_bud_pull_strength, 0.0);
        assert_eq!(migrated.pbf.pinch_bounce, 0.0);
    }
}
