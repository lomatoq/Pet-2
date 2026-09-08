//! Procedural body generation, animation, locomotion, hit testing, and rendering.
//! This crate intentionally contains no platform-specific APIs.

mod animation;
mod droplets;
mod ecology_render;
mod embodiment;
mod expression;
mod graph;
mod liquid;
mod liquid_render;
mod locomotion;
mod mesh;
mod morph;
mod physiology;
mod renderer;
mod tuning;
mod visual_traits;

pub use animation::{AnimationRuntime, JointState};
pub use droplets::{
    DropletLifecycle, DropletMotion, DropletRenderState, DropletRuntime, DropletState, MAX_DROPLETS,
};
pub use ecology_render::{EcologyCaptureExclusion, EcologyRenderer};
pub use embodiment::{EmbodiedPose, EmbodiedRuntime, GazeMode, VoiceVisualState};
pub use expression::ExpressionRuntime;
pub use graph::{BodyGraph, BodyNode, BodyPart};
pub use liquid::{
    BODY_MATERIAL_SNAPSHOT_SCHEMA_VERSION, BodyMaterialSnapshot, BodySnapshotError,
    BubbleRenderState, LiquidDiagnostics, LiquidMorphRuntime, LiquidRenderState,
    MAX_IDLE_FRAGMENTS, MAX_PARTICLES, ParticleRenderState, SavedComponentLifecycle,
    SavedLiquidParticle, SavedTrackedComponent, SavedViscoelasticBond,
    liquid_structural_tuning_hash,
};
pub use locomotion::BodySimulation;
pub use mesh::{MeshError, MeshVertex, ProceduralMesh, ProjectedHitShape};
pub use morph::{ModalDeformation, ModalDynamics};
pub use physiology::{
    EyeAutonomicModifiers, VisualMindInput, VisualPhysiologyPose, VisualPhysiologyRuntime,
};
pub use renderer::{
    CapturedFrame, DebugView, OcclusionMode, RenderOutcome, RenderParameters, Renderer,
    RendererCaptureError, RendererError, ReviewBackground,
};
pub use tuning::{
    AnalyticTuning, BodyRenderMode, ColorSourceMode, CompositorTuning, DenVisualTuning,
    DropletTuning, FaceTuning, InteractionTuning, LIQUID_TUNING_SCHEMA_VERSION,
    LiquidTuningAcknowledgement, LiquidTuningProfile, MaterialTuning, MaterialVariant,
    NervousReadabilityTuning, PbfTuning, TopologyConstraintMode, TuningProfileError,
    VoicePresentationTuning,
};
pub use visual_traits::DerivedVisualTraits;

use glam::{Vec2, Vec3};
use lifecore::{
    AffectState, BodyContactFeedbackV2, BodyEnvironmentFeedbackV2, BodyFeedback, BodyFeedbackV2,
    BodyFluidFeedbackV2, BodyGenome, BodyIntent, BodyMotionFeedbackV2, BodyShapeFeedbackV2,
    BodyTopologyFeedbackV2, ComponentLifecycle, EfferenceCopyV2, EmbodiedInteractionFrame,
    FastPhenotypeActuation, Genome, MaterialRuntimeActuation, PoseIntent, SensorFrame,
};
use pet_ecology::EmbodiedEnvironmentFrame;

const BODY_LAB_EYE_SIZE: f32 = 0.134_631_28;
const BODY_LAB_EYE_SPACING: f32 = 0.341_251_4;
const BODY_LAB_PUPIL_RATIO: f32 = 0.430_986_02;
const BODY_LAB_IRIS_SCALE: f32 = 0.625_005_66;
const BODY_LAB_IRIS_FIBER_COUNT: f32 = 31.097_24;
const BODY_LAB_IRIS_CONTRAST: f32 = 0.261_587_14;
const BODY_LAB_LIMBAL_STRENGTH: f32 = 0.507_179_1;
const BODY_LAB_CORNEA_STRENGTH: f32 = 0.555_337_37;

fn unit(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

fn blend_hsv_hue(base: Vec3, target_hue: f32, blend: f32) -> Vec3 {
    let blend = unit(blend);
    let delta = (target_hue.rem_euclid(1.0) - base.x + 0.5).rem_euclid(1.0) - 0.5;
    Vec3::new((base.x + delta * blend).rem_euclid(1.0), base.y, base.z)
}

fn blend_hsv_identity(genome: Vec3, authored: Vec3, genome_weight: f32) -> Vec3 {
    let genome_weight = unit(genome_weight);
    let authored_weight = 1.0 - genome_weight;
    let hue_delta = (authored.x - genome.x + 0.5).rem_euclid(1.0) - 0.5;
    Vec3::new(
        (genome.x + hue_delta * authored_weight).rem_euclid(1.0),
        genome.y * genome_weight + authored.y * authored_weight,
        genome.z * genome_weight + authored.z * authored_weight,
    )
}

fn apply_glow_runtime(
    base: Vec3,
    runtime: MaterialRuntimeActuation,
    mood_color_blend: f32,
) -> Vec3 {
    let mood_color_blend = unit(mood_color_blend);
    // Mood belongs to the emitted soul/rim light, not the body pigment. This
    // keeps the inherited genome palette recognizable while preserving a
    // continuous, readable affect channel in the glow.
    let target = Vec3::new(
        (base.x + runtime.hue_shift_turns.clamp(-0.0222, 0.0222) * 3.2).rem_euclid(1.0),
        (base.y + runtime.saturation_delta.clamp(-0.14, 0.08) * 1.25).clamp(0.0, 1.0),
        (base.z + runtime.value_delta.clamp(-0.12, 0.12) * 0.38).clamp(0.12, 1.0),
    );
    let hue_delta = (target.x - base.x + 0.5).rem_euclid(1.0) - 0.5;
    Vec3::new(
        (base.x + hue_delta * mood_color_blend).rem_euclid(1.0),
        base.y + (target.y - base.y) * mood_color_blend,
        base.z + (target.z - base.z) * mood_color_blend,
    )
}

/// Screen-space bounds produced from the same filtered liquid instances that the
/// renderer consumes. Coordinates are pixel offsets from the presented body center.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct LiquidVisualBounds {
    pub minimum: Vec2,
    pub maximum: Vec2,
    pub main_minimum: Vec2,
    pub main_maximum: Vec2,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct EcologyVisualEffect {
    pub hue: f32,
    pub color_blend: f32,
    pub flow_boost: f32,
    pub glow_boost: f32,
    pub cohesion_bias: f32,
    pub translucency_boost: f32,
    pub contrast_reduction: f32,
}

impl EcologyVisualEffect {
    #[must_use]
    pub fn bounded(mut self) -> Self {
        self.hue = if self.hue.is_finite() {
            self.hue.rem_euclid(1.0)
        } else {
            0.0
        };
        self.color_blend = unit(self.color_blend).min(0.65);
        self.flow_boost = unit(self.flow_boost);
        self.glow_boost = unit(self.glow_boost);
        self.cohesion_bias = unit(self.cohesion_bias);
        self.translucency_boost = unit(self.translucency_boost).min(0.25);
        self.contrast_reduction = unit(self.contrast_reduction).min(0.45);
        self
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProceduralBody {
    pub graph: BodyGraph,
    pub mesh: ProceduralMesh,
    pub hit_shape: ProjectedHitShape,
    pub animation: AnimationRuntime,
    pub expression: ExpressionRuntime,
    pub embodiment: EmbodiedRuntime,
    pub simulation: BodySimulation,
    body_genome: BodyGenome,
    base_visual_traits: DerivedVisualTraits,
    visual_traits: DerivedVisualTraits,
    tuning: LiquidTuningProfile,
    render_aspect: f32,
    presentation_scale: f32,
    presentation_offset: Vec2,
    occlusion_mode: OcclusionMode,
    occlusion_edge: f32,
    ecology_visual_effect: EcologyVisualEffect,
    fast_phenotype: FastPhenotypeActuation,
}

impl ProceduralBody {
    pub fn generate(genome: &Genome) -> Result<Self, MeshError> {
        let graph = BodyGraph::from_genome(&genome.body);
        let mesh = ProceduralMesh::generate(&genome.body)?;
        let hit_shape = ProjectedHitShape::from_genome(&genome.body);
        let animation = AnimationRuntime::new(&graph, genome.identity_seed);
        let visual_traits = DerivedVisualTraits::from_genome(genome);
        let tuning = LiquidTuningProfile::for_seed(genome.identity_seed);
        let mut body = Self {
            graph,
            mesh,
            hit_shape,
            animation,
            expression: ExpressionRuntime::default(),
            embodiment: EmbodiedRuntime::new(genome.identity_seed, &visual_traits),
            simulation: BodySimulation::new(genome.identity_seed),
            body_genome: genome.body.clone(),
            base_visual_traits: visual_traits,
            visual_traits,
            tuning: tuning.clone(),
            render_aspect: 1.0,
            presentation_scale: 1.0,
            presentation_offset: Vec2::ZERO,
            occlusion_mode: OcclusionMode::Front,
            occlusion_edge: 0.0,
            ecology_visual_effect: EcologyVisualEffect::default(),
            fast_phenotype: FastPhenotypeActuation::default(),
        };
        body.apply_tuning_profile(tuning)
            .expect("the built-in liquid tuning profile is valid");
        Ok(body)
    }

    pub fn fixed_update(
        &mut self,
        genome: &Genome,
        intent: &BodyIntent,
        sensors: &SensorFrame,
        dt: f32,
    ) -> &BodyFeedback {
        self.simulation
            .fixed_update(&genome.body, intent, sensors, dt)
    }

    pub fn set_embodied_environment(&mut self, environment: &EmbodiedEnvironmentFrame) {
        self.embodiment.liquid.set_embodied_environment(environment);
    }

    #[must_use]
    pub fn embodied_interaction_frame(&self) -> EmbodiedInteractionFrame {
        self.embodiment.liquid.embodied_interaction_frame()
    }

    #[must_use]
    pub fn body_material_snapshot(&self) -> BodyMaterialSnapshot {
        self.embodiment.liquid.body_material_snapshot(
            self.tuning.seed,
            self.tuning.schema_version,
            self.tuning.profile_revision,
        )
    }

    pub fn restore_body_material_snapshot(
        &mut self,
        snapshot: &BodyMaterialSnapshot,
    ) -> Result<(), BodySnapshotError> {
        self.embodiment.liquid.restore_body_material_snapshot(
            snapshot,
            self.tuning.seed,
            self.tuning.schema_version,
        )
    }

    pub fn set_ecology_visual_effect(&mut self, effect: EcologyVisualEffect) {
        self.ecology_visual_effect = effect.bounded();
    }

    /// Installs bounded state multipliers for the next body tick. Authored
    /// profile and structural solver settings remain untouched.
    pub fn set_fast_phenotype_actuation(&mut self, actuation: FastPhenotypeActuation) {
        self.fast_phenotype = if actuation.is_finite() {
            actuation
        } else {
            FastPhenotypeActuation::default()
        };
        self.embodiment
            .liquid
            .set_runtime_actuation(self.fast_phenotype.pbf);
        self.embodiment.set_nervous_system_actuation(
            self.fast_phenotype.face,
            self.fast_phenotype.visual_physiology,
        );
        let mut analytic = self.tuning.analytic;
        analytic.modal_response = (analytic.modal_response
            * self.fast_phenotype.analytic.modal_response_multiplier)
            .clamp(0.0, 3.0);
        analytic.modal_frequency = (analytic.modal_frequency
            * self.fast_phenotype.analytic.modal_frequency_multiplier)
            .clamp(0.25, 2.5);
        analytic.modal_damping = (analytic.modal_damping
            * self.fast_phenotype.analytic.modal_damping_multiplier)
            .clamp(0.15, 3.0);
        analytic.modal_amplitude = (analytic.modal_amplitude
            * self.fast_phenotype.analytic.modal_amplitude_multiplier)
            .clamp(0.0, 2.0);
        self.embodiment.modal_dynamics.set_tuning(analytic);
    }

    #[must_use]
    pub fn fast_phenotype_actuation(&self) -> &FastPhenotypeActuation {
        &self.fast_phenotype
    }

    /// Builds the immutable felt-body frame from authoritative body/PBF state.
    /// `previous` is used only for jerk; cognition consumes this frame next tick.
    #[must_use]
    pub fn body_feedback_v2(
        &self,
        intent: &BodyIntent,
        sensors: &SensorFrame,
        previous: Option<&BodyFeedbackV2>,
        frame_id: u64,
    ) -> BodyFeedbackV2 {
        let legacy = &self.simulation.feedback;
        let interaction = self.embodied_interaction_frame();
        let material = interaction.material;
        let diagnostics = self.embodiment.liquid.diagnostics();
        let contact = interaction.contact;
        let observed =
            &interaction.components[..usize::from(interaction.component_observation_count)];
        let contact_component = contact
            .active
            .then(|| {
                observed
                    .iter()
                    .min_by(|left, right| {
                        left.center_local
                            .distance(contact.point_local)
                            .total_cmp(&right.center_local.distance(contact.point_local))
                    })
                    .map(|component| component.component_id)
            })
            .flatten();
        let largest_fragment = observed
            .iter()
            .map(|component| component.mass_fraction)
            .fold(0.0_f32, f32::max);
        let main_component = observed
            .iter()
            .max_by(|left, right| left.mass_fraction.total_cmp(&right.mass_fraction));
        let merge_progress = observed
            .iter()
            .filter(|component| {
                matches!(
                    component.lifecycle,
                    ComponentLifecycle::Returning | ComponentLifecycle::Merging
                )
            })
            .map(|component| 1.0 - unit(component.distance_to_main))
            .fold(0.0_f32, f32::max);
        let position = legacy.world_position;
        let edge_distance = position
            .x
            .min(1.0 - position.x)
            .min(position.y)
            .min(1.0 - position.y);
        let previous_acceleration = previous.map_or(Vec2::ZERO, |frame| frame.motion.acceleration);
        let intended_velocity = self.simulation.motor_velocity;
        let actual_velocity = legacy.velocity.clamp_length_max(1.0);
        let length_ratio = diagnostics.stretch_ratio.clamp(0.5, 1.8);
        let width_ratio = (1.0 / length_ratio.max(0.01)).clamp(0.5, 1.8);
        let actual_shape = Vec3::new(
            length_ratio - 1.0,
            width_ratio - 1.0,
            unit(1.0 - material.deformation_energy) - 0.5,
        );
        let mut frame = BodyFeedbackV2 {
            frame_id,
            contact: BodyContactFeedbackV2 {
                component_id: contact_component,
                point_world: contact.point_world,
                point_local: contact.point_local,
                normal: contact.normal_local,
                area: contact.area_fraction,
                pressure: contact.effective_pressure,
                tangential_speed: unit(contact.relative_velocity_local.length() / 8.0),
                duration: contact.contact_seconds,
                contact_count: u16::from(contact.active),
                user_force_estimate: (contact.relative_velocity_local * contact.effective_pressure
                    / 8.0)
                    .clamp_length_max(1.0),
            },
            shape: BodyShapeFeedbackV2 {
                body_area_ratio: (1.0 + material.mass_conservation_error).clamp(0.5, 1.5),
                body_length_ratio: length_ratio,
                body_width_ratio: width_ratio,
                roundness: unit(1.0 - material.deformation_energy),
                deformation_energy: material.deformation_energy,
                maximum_strain: unit(material.maximum_strain),
                neck_tension: material.neck_tension,
                center_of_mass_offset: main_component
                    .map_or(Vec2::ZERO, |component| component.center_local),
                orientation_radians: diagnostics.orientation,
                angular_velocity: (diagnostics.rigid_angular_velocity / 8.0).clamp(-1.0, 1.0),
            },
            fluid: BodyFluidFeedbackV2 {
                slosh_energy: material.slosh_energy,
                internal_relative_speed: unit(material.internal_relative_speed / 8.0),
                pressure_variance: unit(material.deformation_rate.abs() / 8.0),
                settle_error: unit(diagnostics.density_error),
            },
            topology: BodyTopologyFeedbackV2 {
                connected_components: u16::from(material.component_count.max(1)),
                detached_mass_fraction: material.detached_mass_fraction,
                largest_fragment_fraction: largest_fragment,
                budget_remaining: material.topology_budget_remaining,
                merge_progress,
                recovery_active: interaction.recovery_event.is_some()
                    || observed.iter().any(|component| {
                        matches!(
                            component.lifecycle,
                            ComponentLifecycle::Returning
                                | ComponentLifecycle::Merging
                                | ComponentLifecycle::DeterministicRecovery
                        )
                    }),
            },
            motion: BodyMotionFeedbackV2 {
                world_position: position,
                velocity: actual_velocity,
                acceleration: legacy.acceleration.clamp_length_max(1.0),
                jerk: (legacy.acceleration.clamp_length_max(1.0) - previous_acceleration)
                    .clamp_length_max(1.0),
                grounded: legacy.grounded,
                clinging: legacy.clinging,
                collision_impulse: legacy
                    .collision
                    .as_ref()
                    .map_or(0.0, |event| unit(event.intensity)),
            },
            efference_copy: EfferenceCopyV2 {
                intended_velocity,
                intended_turn: intent.facing_direction.clamp(-1.0, 1.0),
                intended_shape_delta: Vec3::new(
                    self.fast_phenotype.analytic.body_length_scale - 1.0,
                    self.fast_phenotype.analytic.body_width_scale - 1.0,
                    self.fast_phenotype.analytic.roundness_bias,
                ),
                actual_velocity,
                actual_turn: (diagnostics.rigid_angular_velocity / 8.0).clamp(-1.0, 1.0),
                actual_shape_delta: actual_shape,
            },
            environment: BodyEnvironmentFeedbackV2 {
                distance_to_screen_edge: unit(edge_distance * 2.0),
                clipped_fraction: unit((0.03 - edge_distance).max(0.0) / 0.03),
                available_motion_radius: unit(edge_distance * 2.0),
                cursor_distance: sensors.cursor_distance_to_pet.clamp(0.0, 1.0),
                cursor_loom_rate: unit(sensors.cursor_approach_speed),
                user_present: sensors.user_presence.unwrap_or(0.0) > 0.05,
                seconds_since_interaction: sensors.user_idle_seconds.max(0.0),
            },
            ..BodyFeedbackV2::default()
        };
        frame.sanitize();
        frame
    }

    /// Compatibility update for headless callers that do not yet provide the full
    /// embodied context. Desktop runtimes should call `embodied_update`.
    pub fn animation_update(&mut self, intent: &BodyIntent, arousal: f32, dt: f32) {
        let affect = AffectState {
            arousal: arousal.clamp(0.0, 1.0),
            ..AffectState::default()
        };
        let audio = pet_audio::global_visual_feedback();
        let visual_mind = VisualMindInput {
            arousal: affect.arousal,
            local_luminance: 0.5,
            ..VisualMindInput::default()
        };
        self.embodied_update(
            intent,
            &SensorFrame::default(),
            affect,
            visual_mind,
            VoiceVisualState {
                active: audio.active,
                motif_id: audio.motif_id,
                syllable_index: audio.syllable_index,
                envelope: audio.envelope,
                mouth_open: audio.mouth_open,
                pitch_normalized: audio.pitch_normalized,
                noisiness: audio.noisiness,
                purr: audio.purr,
            },
            dt,
        );
    }

    pub fn embodied_update(
        &mut self,
        intent: &BodyIntent,
        sensors: &SensorFrame,
        affect: AffectState,
        visual_mind: VisualMindInput,
        voice: VoiceVisualState,
        dt: f32,
    ) {
        self.animation
            .update(&self.graph, intent, affect.arousal, dt);
        self.expression
            .update(intent.expression, self.animation.blink, dt);
        let mut effective_traits = self.visual_traits;
        let effect = self.ecology_visual_effect;
        effective_traits.flow_speed =
            (effective_traits.flow_speed * (1.0 + effect.flow_boost * 0.75)).clamp(0.02, 2.0);
        let cohesion_mix = effect.color_blend.max(effect.flow_boost) * 0.32;
        effective_traits.droplet_cohesion = (effective_traits.droplet_cohesion
            + (effect.cohesion_bias - effective_traits.droplet_cohesion) * cohesion_mix)
            .clamp(0.0, 1.0);
        effective_traits.translucency =
            (effective_traits.translucency + effect.translucency_boost).clamp(0.0, 1.0);
        self.embodiment.update(
            &self.body_genome,
            &effective_traits,
            visual_mind,
            intent,
            sensors,
            &self.simulation.feedback,
            affect,
            self.expression.current,
            self.tuning.face,
            voice,
            dt,
        );
        self.embodiment.liquid.set_face_attention_pose(
            (self.embodiment.pose.face_attention_offset
                + self.fast_phenotype.face.translation_offset)
                .clamp_length_max(0.082),
            (self.embodiment.pose.face_attention_roll + self.fast_phenotype.face.semantic_roll)
                .clamp(
                    -self.tuning.face.maximum_roll_radians,
                    self.tuning.face.maximum_roll_radians,
                ),
        );
        match intent.pose {
            PoseIntent::Clinging => {
                self.occlusion_mode = if intent.facing_direction >= 0.0 {
                    OcclusionMode::PeekFromLeft
                } else {
                    OcclusionMode::PeekFromRight
                };
                self.occlusion_edge = if intent.facing_direction >= 0.0 {
                    0.08
                } else {
                    -0.08
                };
            }
            PoseIntent::Landing if self.simulation.feedback.grounded => {
                self.occlusion_mode = OcclusionMode::PeekFromTop;
                self.occlusion_edge = -0.72;
            }
            _ => {
                self.occlusion_mode = OcclusionMode::Front;
                self.occlusion_edge = 0.0;
            }
        }
    }

    /// Advances frame-rate presentation trackers exactly once for a frame that
    /// is about to be drawn. Authoritative physics remains in `embodied_update`
    /// at 120 Hz; face and gaze caps therefore apply to what the user sees rather
    /// than to however many fixed ticks happened before this present.
    pub fn presentation_update(&mut self, dt: f32) {
        self.embodiment.presentation_update(dt);
    }

    #[must_use]
    pub fn visual_traits(&self) -> DerivedVisualTraits {
        self.visual_traits
    }

    pub fn set_visual_traits(&mut self, traits: DerivedVisualTraits) {
        if traits.is_valid() {
            self.base_visual_traits = traits;
            self.visual_traits = traits;
        }
    }

    pub fn apply_tuning_profile(
        &mut self,
        profile: LiquidTuningProfile,
    ) -> Result<(), TuningProfileError> {
        let profile = profile.sanitized()?;
        let mut traits = self.base_visual_traits;
        traits.shell_opacity = profile.material.opacity;
        traits.translucency = profile.material.translucency;
        traits.flow_speed = profile.material.internal_flow.clamp(0.0, 2.0);
        traits.droplet_count = profile.droplets.count as u8;
        traits.droplet_elasticity = profile.droplets.elasticity;
        traits.droplet_drag = profile.droplets.drag;
        traits.droplet_lag = profile.droplets.lag;
        traits.droplet_cohesion = profile.droplets.cohesion;
        traits.halo_strength = profile.material.halo;
        self.visual_traits = traits;
        self.embodiment.droplets.set_tuning(profile.droplets);
        self.embodiment.modal_dynamics.set_tuning(profile.analytic);
        self.embodiment.liquid.set_tuning(
            profile.pbf,
            profile.interaction,
            profile.face,
            profile.material.variant,
        );
        self.tuning = profile;
        Ok(())
    }

    #[must_use]
    pub fn tuning_profile(&self) -> &LiquidTuningProfile {
        &self.tuning
    }

    /// Configures the conversion from normalized virtual-desktop motion into the
    /// shader-local space used by the liquid simulation. Desktop Y points down,
    /// while shader-local Y points up, hence the explicit sign flip.
    pub fn set_desktop_motion_space(
        &mut self,
        desktop_physical_size: Vec2,
        overlay_physical_height: f32,
    ) {
        if !desktop_physical_size.is_finite()
            || !overlay_physical_height.is_finite()
            || overlay_physical_height <= 0.0
        {
            return;
        }
        let pixels_to_body = 2.0 * self.projection_scale() / overlay_physical_height.max(1.0);
        self.embodiment.set_world_to_body_scale(
            Vec2::new(desktop_physical_size.x, -desktop_physical_size.y) * pixels_to_body,
        );
    }

    pub fn set_render_aspect(&mut self, aspect: f32) {
        if aspect.is_finite() && aspect > 0.0 {
            self.render_aspect = aspect.clamp(0.25, 4.0);
        }
    }

    /// Separates the host window's transparent guard surface from the apparent
    /// organism size. Body Lab keeps the default 1.0; desktop production can use
    /// a larger projection scale together with a larger window to gain both a
    /// wider deformation guard band and a larger on-screen Pet.
    pub fn set_presentation_scale(&mut self, scale: f32) {
        if scale.is_finite() && scale > 0.0 {
            self.presentation_scale = scale.clamp(0.50, 3.0);
        }
    }

    /// Moves the rendered organism inside its transparent host surface without
    /// moving the simulated liquid frame. This is the sub-pixel presentation
    /// layer used by desktop camera-follow; Lab keeps the zero default.
    pub fn set_presentation_offset_pixels(&mut self, pixels: Vec2, viewport_height: f32) {
        if !pixels.is_finite() || !viewport_height.is_finite() || viewport_height <= 0.0 {
            return;
        }
        let pixels_to_local = 2.0 * self.projection_scale() / viewport_height.max(1.0);
        self.presentation_offset = Vec2::new(pixels.x, -pixels.y) * pixels_to_local;
        let projection = self.projection_scale();
        let limit = Vec2::new(self.render_aspect * projection, projection).max(Vec2::splat(0.82));
        self.presentation_offset = self.presentation_offset.clamp(-limit, limit);
    }

    /// Constrains the experimental particle liquid to a normalized preview rect.
    /// Body Lab uses this to keep detached material inside its actual canvas;
    /// production callers should leave it disabled.
    pub fn set_liquid_preview_bounds(&mut self, bounds: Option<(Vec2, Vec2)>) {
        let Some((normalized_minimum, normalized_maximum)) = bounds else {
            self.embodiment.liquid.set_local_containment_bounds(None);
            return;
        };
        if !normalized_minimum.is_finite() || !normalized_maximum.is_finite() {
            self.embodiment.liquid.set_local_containment_bounds(None);
            return;
        }
        let scale = self.projection_scale();
        let to_local = |normalized: Vec2| {
            let point = normalized.clamp(Vec2::ZERO, Vec2::ONE) * 2.0 - Vec2::ONE;
            Vec2::new(point.x * self.render_aspect, -point.y) * scale
        };
        // Rendering applies `presentation_offset` after simulation. Convert the
        // visible preview rectangle back into simulation-local space so a body
        // centred inside an egui canvas is constrained by that canvas, not by the
        // hidden full-window coordinate frame behind the controls.
        let a = to_local(normalized_minimum) - self.presentation_offset;
        let b = to_local(normalized_maximum) - self.presentation_offset;
        self.embodiment
            .liquid
            .set_local_containment_bounds(Some((a.min(b), a.max(b))));
    }

    #[must_use]
    pub fn projected_hit_test(&self, normalized_window_point: Vec2) -> bool {
        self.projected_hit_test_with_margin(normalized_window_point, 1.0, 0.0)
    }

    /// Uses the same transform as the renderer while allowing the desktop host
    /// to arm click capture slightly before the cursor reaches visible density.
    /// The margin covers one 60 Hz polling interval and small autonomous motion;
    /// it is not used by the material or by exact geometry tests.
    #[must_use]
    pub fn projected_hit_test_with_margin(
        &self,
        normalized_window_point: Vec2,
        viewport_height: f32,
        margin_pixels: f32,
    ) -> bool {
        if !normalized_window_point.is_finite() {
            return false;
        }
        let mut local_point = normalized_window_point * 2.0 - Vec2::ONE;
        local_point.x *= self.render_aspect;
        local_point.y = -local_point.y;
        local_point *= self.projection_scale();
        local_point -= self.presentation_offset;
        if self.tuning.render_mode == BodyRenderMode::ParticlePbf
            && self.embodiment.liquid.diagnostics().finite
        {
            let margin_local =
                margin_pixels.max(0.0) * 2.0 * self.projection_scale() / viewport_height.max(1.0);
            self.embodiment
                .liquid
                .hit_test_main_component_with_margin(local_point, margin_local)
        } else {
            self.hit_shape.contains(normalized_window_point)
        }
    }

    /// Returns conservative pixel bounds for camera guarding and physical desktop
    /// containment. The legacy-named `main_*` bounds include every real liquid
    /// particle and exclude only presentation bubbles; the face-carrier label
    /// must never own collision or navigation physics.
    #[must_use]
    pub fn liquid_visual_bounds_pixels(&self, viewport_height: f32) -> LiquidVisualBounds {
        if !viewport_height.is_finite() || viewport_height <= 0.0 {
            return LiquidVisualBounds::default();
        }
        let liquid = self.embodiment.liquid.render_state();
        let pixels_per_local = viewport_height / (2.0 * self.projection_scale().max(1.0e-5));
        let mut minimum = Vec2::splat(f32::INFINITY);
        let mut maximum = Vec2::splat(f32::NEG_INFINITY);
        let mut main_minimum = Vec2::splat(f32::INFINITY);
        let mut main_maximum = Vec2::splat(f32::NEG_INFINITY);
        for particle in &liquid.particles[..liquid.particle_count] {
            let center = Vec2::new(particle.position.x, -particle.position.y) * pixels_per_local;
            let radius = particle.major_radius.max(particle.minor_radius) * pixels_per_local;
            let extent = Vec2::splat(radius);
            minimum = minimum.min(center - extent);
            maximum = maximum.max(center + extent);
            main_minimum = main_minimum.min(center - extent);
            main_maximum = main_maximum.max(center + extent);
        }
        for bubble in &liquid.bubbles[..liquid.bubble_count] {
            let center = Vec2::new(bubble.position.x, -bubble.position.y) * pixels_per_local;
            let extent = Vec2::splat(bubble.radius * pixels_per_local);
            minimum = minimum.min(center - extent);
            maximum = maximum.max(center + extent);
        }
        if !minimum.is_finite() || !maximum.is_finite() {
            minimum = Vec2::splat(-1.0);
            maximum = Vec2::splat(1.0);
        }
        if !main_minimum.is_finite() || !main_maximum.is_finite() {
            main_minimum = minimum;
            main_maximum = maximum;
        }
        let organism_minimum = minimum;
        let organism_maximum = maximum;
        let rim_bloom_padding = 12.0 + self.tuning.material.edge_light_width.clamp(2.0, 32.0);
        minimum -= Vec2::splat(rim_bloom_padding);
        maximum += Vec2::splat(rim_bloom_padding);

        let shadow_offset = Vec2::new(
            self.tuning
                .compositor
                .shadow_horizontal_offset
                .clamp(-96.0, 96.0),
            self.tuning
                .compositor
                .shadow_vertical_offset
                .clamp(-96.0, 96.0),
        );
        let feather = self.tuning.compositor.shadow_feather.clamp(2.0, 128.0);
        minimum = minimum.min(organism_minimum + shadow_offset - Vec2::splat(feather));
        maximum = maximum.max(organism_maximum + shadow_offset + Vec2::splat(feather));
        main_minimum -= Vec2::splat(rim_bloom_padding);
        main_maximum += Vec2::splat(rim_bloom_padding);
        LiquidVisualBounds {
            minimum,
            maximum,
            main_minimum,
            main_maximum,
        }
    }

    fn projection_scale(&self) -> f32 {
        renderer::projection_scale(
            renderer::organism_scale(&self.mesh),
            self.tuning.render_mode,
        ) * self.presentation_scale
    }

    #[must_use]
    pub fn render_parameters(&self, genome: &Genome, arousal: f32) -> RenderParameters {
        let pose = self.embodiment.pose;
        let physiology = self.embodiment.physiology.pose;
        let traits = self.visual_traits;
        let profile = &self.tuning;
        let material = profile.material;
        let cinematic = material.variant == MaterialVariant::CinematicJelly;
        let fast = &self.fast_phenotype;
        let palette = |genome_hsv: Vec3, authored_hsv: [f32; 3]| match material.color_source_mode {
            ColorSourceMode::Authored => Vec3::from_array(authored_hsv),
            ColorSourceMode::Genome => genome_hsv,
            ColorSourceMode::GenomeAuthoredBlend => blend_hsv_identity(
                genome_hsv,
                Vec3::from_array(authored_hsv),
                material.genome_color_blend,
            ),
        };
        // Identity pigments stay fixed. Ecology and nervous-system affect are
        // expressed through the soul/rim emission below.
        let primary_hsv = palette(genome.body.primary_color_hsv, material.primary_hsv);
        let secondary_hsv = palette(genome.body.secondary_color_hsv, material.secondary_hsv);
        let glow_hsv = palette(genome.body.glow_color_hsv, material.glow_hsv);
        let effect = self.ecology_visual_effect;
        let mut glow_hsv = apply_glow_runtime(
            blend_hsv_hue(glow_hsv, effect.hue, effect.color_blend),
            fast.material,
            material.mood_color_blend,
        );
        glow_hsv.z = glow_hsv.z.max(0.12);
        RenderParameters {
            render_mode: profile.render_mode,
            render_scale: profile.compositor.render_scale,
            presentation_scale: self.presentation_scale * fast.apparent_scale,
            presentation_offset: self.presentation_offset,
            debug_view: DebugView::Material,
            time: self.animation.time,
            arousal,
            glow: (self.expression.current.body_glow * genome.body.bioluminescence
                + effect.glow_boost * 0.34)
                .clamp(0.0, 1.0),
            body_length: genome.body.body_length
                * profile.analytic.body_length_scale
                * fast.analytic.body_length_scale,
            body_width: genome.body.body_width
                * profile.analytic.body_width_scale
                * fast.analytic.body_width_scale,
            body_roundness: (genome.body.body_roundness
                + profile.analytic.roundness_bias
                + fast.analytic.roundness_bias)
                .clamp(0.0, 1.0),
            head_ratio: genome.body.head_ratio,
            eye_size: if cinematic {
                BODY_LAB_EYE_SIZE
            } else {
                genome.body.eye_size
            } * profile.face.eye_size_scale
                * fast.face.scale_multiplier
                * pose.eye_scale.clamp(0.88, 1.18),
            eye_spacing: if cinematic {
                BODY_LAB_EYE_SPACING
            } else {
                genome.body.eye_spacing
            } * profile.face.eye_spacing_scale,
            pupil_ratio: if cinematic {
                BODY_LAB_PUPIL_RATIO
            } else {
                genome.body.pupil_ratio
            } * profile.face.pupil_scale,
            softness: (genome.body.softness
                + profile.analytic.softness_bias
                + fast.analytic.softness_bias)
                .clamp(0.0, 1.0),
            tail_length: genome.body.tail_length,
            tail_thickness: genome.body.tail_thickness,
            ear_fin_size: genome.body.ear_fin_size,
            primary_hsv,
            secondary_hsv,
            glow_hsv,
            iris_hsv: Vec3::from_array(profile.face.iris_hsv),
            override_iris_color: profile.face.override_iris_color,
            gaze: pose.gaze,
            vergence: pose.vergence,
            // `pupil_scale` authors the inherited/resting iris ratio above. The
            // autonomic response is already normalized and must not be scaled twice.
            pupil_size: pose.pupil_size,
            pupil_asymmetry: pose.pupil_asymmetry,
            blink_left: pose.blink_left,
            blink_right: pose.blink_right,
            squint: pose.squint,
            brow_raise: pose.brow_raise,
            brow_tension: pose.brow_tension,
            brow_asymmetry: pose.brow_asymmetry,
            mouth_open: pose.mouth_open,
            mouth_curve: pose.mouth_curve,
            mouth_tension: pose.mouth_tension,
            cheek_glow: pose.cheek_glow,
            audio_envelope: pose.audio_envelope,
            purr: pose.purr,
            squash: pose.squash,
            tilt: pose.tilt,
            head_lag: pose.head_lag,
            tail_lag: pose.tail_lag,
            breath: pose.breath,
            compression: pose.compression,
            morph_mode2: pose.morph.mode2,
            morph_mode3: pose.morph.mode3,
            morph_mode4: pose.morph.mode4,
            morph_area_scale: pose.morph.area_scale,
            pattern_scale: genome.body.pattern_scale,
            pattern_contrast: (genome.body.pattern_contrast * (1.0 - effect.contrast_reduction))
                .max(0.18),
            pattern_seed: genome.body.pattern_seed,
            pulse: unit(physiology.pulse * 0.55 + fast.visual_physiology.pulse_amplitude * 0.45),
            shell_opacity: ((material.opacity - effect.translucency_boost * 0.34)
                * fast.visual_physiology.shell_opacity_multiplier)
                .clamp(0.62, 1.0),
            inner_density: (physiology.inner_density
                * fast.visual_physiology.inner_density_multiplier)
                .clamp(0.0, 2.0),
            translucency: (material.translucency
                + effect.translucency_boost
                + fast.visual_physiology.translucency_delta)
                .clamp(0.0, 1.0),
            core_glow: (physiology.core_glow
                * material.emission
                * fast.visual_physiology.core_glow_multiplier
                + effect.glow_boost * 0.42)
                .clamp(0.0, 2.0),
            halo: (material.halo * fast.visual_physiology.halo_multiplier).clamp(0.0, 2.0),
            iris_activity: unit(physiology.iris_activity * fast.visual_physiology.iris_activity),
            eye_wetness: unit(physiology.eye_wetness * (0.65 + fast.visual_physiology.eye_wetness)),
            flow_strength: (material.internal_flow.max(0.16)
                * fast.visual_physiology.flow_strength_multiplier)
                .clamp(0.0, 0.32),
            flow_scale: traits.flow_scale,
            flow_speed: (physiology.flow_speed * fast.visual_physiology.flow_speed_multiplier)
                .clamp(0.0, 3.0),
            flow_phase: physiology.flow_phase,
            flow_warp: traits.flow_warp,
            core_size: traits.core_size,
            cornea_strength: if cinematic {
                BODY_LAB_CORNEA_STRENGTH
            } else {
                traits.cornea_strength
            },
            iris_scale: if cinematic {
                BODY_LAB_IRIS_SCALE
            } else {
                traits.iris_scale
            },
            limbal_strength: if cinematic {
                BODY_LAB_LIMBAL_STRENGTH
            } else {
                traits.limbal_strength
            },
            iris_fiber_count: if cinematic {
                BODY_LAB_IRIS_FIBER_COUNT
            } else {
                traits.iris_fiber_count
            },
            iris_contrast: if cinematic {
                BODY_LAB_IRIS_CONTRAST
            } else {
                traits.iris_contrast
            },
            droplet_energy: fast.visual_physiology.droplet_energy,
            droplet_cohesion: fast.visual_physiology.droplet_cohesion,
            droplet_spread: fast.visual_physiology.droplet_spread,
            droplets: self.embodiment.droplets.render_states(),
            liquid: self.embodiment.liquid.render_state(),
            material_absorption: material.absorption,
            material_scattering: material.scattering,
            material_thickness: material.thickness,
            material_refraction: material.refraction,
            material_blur: material.blur,
            material_rim_strength: material.rim_strength,
            material_rim_power: material.rim_power,
            material_broad_specular: material.broad_specular,
            material_broad_specular_power: material.broad_specular_power,
            material_tight_specular: material.tight_specular,
            material_tight_specular_power: material.tight_specular_power,
            material_emission: (material.emission * fast.material.emission_multiplier)
                .clamp(0.0, 4.0),
            material_fresnel_f0: material.fresnel_f0,
            material_core_level: material.core_level,
            material_thickness_gamma: material.thickness_gamma,
            material_pseudo_depth: material.pseudo_depth,
            material_normal_scale: material.normal_scale,
            material_light_wrap: material.light_wrap,
            material_ambient_scatter: material.ambient_scatter,
            material_direct_scatter: material.direct_scatter,
            material_transmission_hue_preservation: material.transmission_hue_preservation,
            material_opacity: (material.opacity * fast.material.opacity_multiplier).clamp(0.0, 1.0),
            material_variant: material.variant,
            material_cinematic_smoothing: material.cinematic_smoothing,
            material_internal_orb_count: material.internal_orb_count,
            material_internal_orb_intensity: (material.internal_orb_intensity
                * fast.material.internal_orb_intensity_multiplier)
                .clamp(0.0, 3.0),
            material_internal_orb_size: material.internal_orb_size,
            material_internal_orb_halo: material.internal_orb_halo,
            material_internal_orb_speed: (material.internal_orb_speed
                * fast.material.internal_orb_speed_multiplier)
                .clamp(0.0, 4.0),
            material_internal_orb_depth: material.internal_orb_depth,
            material_internal_orb_spread: material.internal_orb_spread,
            material_studio_intensity: material.studio_intensity,
            material_studio_base_roughness: material.studio_base_roughness,
            material_studio_coat_roughness: material.studio_coat_roughness,
            material_narrow_rim_strength: material.narrow_rim_strength,
            material_broad_rim_strength: material.broad_rim_strength,
            material_rim_saturation: material.rim_saturation,
            material_edge_light_width: material.edge_light_width,
            material_caustic_strength: material.caustic_strength,
            material_caustic_scale: material.caustic_scale,
            // The dispersive caustic is an authored material pattern, not an
            // affect display. Nervous-system arousal/novelty may modulate the
            // organic flow channels, but must not retime this pattern: changing
            // its phase velocity reads as temporal flicker.
            material_caustic_speed: material.caustic_speed.clamp(0.0, 4.0),
            material_caustic_dispersion: material.caustic_dispersion,
            material_rounded_highlight_strength: material.rounded_highlight_strength,
            material_highlight_tint: material.highlight_tint,
            material_soul_glow_count: material.soul_glow_count,
            material_soul_glow_strength: (material.soul_glow_strength
                * fast.material.soul_glow_strength_multiplier)
                .clamp(0.0, 4.0),
            material_soul_glow_size: material.soul_glow_size,
            material_soul_glow_speed: material.soul_glow_speed,
            material_soul_glow_pulse: (material.soul_glow_pulse
                * fast.material.soul_glow_pulse_multiplier)
                .clamp(0.0, 4.0),
            material_soul_glow_feather: material.soul_glow_feather,
            material_bloom_strength: (material.bloom_strength.max(0.05)
                * fast.material.bloom_multiplier)
                .clamp(0.0, 0.18),
            liquid_iso_threshold: profile.pbf.iso_threshold,
            face_visible: profile.face.visible,
            face_eye_highlight_scale: profile.face.eye_highlight_scale,
            face_eye_socket_strength: profile.face.eye_socket_strength,
            face_relief_strength: profile.face.relief_strength,
            face_relief_darkness: profile.face.relief_darkness,
            face_relief_coat_strength: profile.face.relief_coat_strength,
            shadow_horizontal_offset: profile.compositor.shadow_horizontal_offset,
            shadow_vertical_offset: profile.compositor.shadow_vertical_offset,
            shadow_feather: profile.compositor.shadow_feather,
            shadow_opacity: profile.compositor.shadow_opacity,
            shadow_color: Vec3::from_array(profile.compositor.shadow_color),
            exposure: profile.compositor.exposure,
            // Window rectangles belong to perception and locomotion. They must never
            // multiply the organism alpha or become a rectangular visual mask.
            occlusion_mode: OcclusionMode::Front,
            occlusion_edge: 0.0,
            occlusion_softness: 0.018 + genome.body.softness * 0.025,
        }
    }
}

#[cfg(test)]
mod tests {
    use glam::Vec2;
    use lifecore::{BodyIntent, ExpressionState, Genome, LocomotionMode, PoseIntent, SensorFrame};

    use super::*;

    fn intent(mode: LocomotionMode, target: Vec2) -> BodyIntent {
        BodyIntent {
            locomotion: mode,
            target_position: target,
            target_surface: None,
            desired_speed: 0.35,
            facing_direction: 1.0,
            gaze_target: Some(target),
            pose: PoseIntent::Neutral,
            expression: ExpressionState::default(),
            interaction_target: None,
        }
    }

    #[test]
    fn graph_and_mesh_are_valid_and_deterministic() {
        let genome = Genome::from_seed(91);
        let a = ProceduralBody::generate(&genome).unwrap();
        let b = ProceduralBody::generate(&genome).unwrap();
        assert!(a.graph.is_valid());
        a.mesh.validate().unwrap();
        assert_eq!(a.mesh.stable_hash(), b.mesh.stable_hash());
        assert!(!a.mesh.vertices.is_empty());
        assert!(!a.mesh.indices.is_empty());
    }

    #[test]
    fn procedural_shader_parses_and_validates() {
        let source = include_str!("legacy_analytic.wgsl");
        let module = naga::front::wgsl::parse_str(source).expect("procedural pet WGSL parses");
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .expect("procedural pet WGSL validates");
        assert_eq!(
            source
                .matches("let main_field = body_field(point);")
                .count(),
            1,
            "the liquid morph should evaluate its full SDF only once per fragment"
        );
        assert!(source.contains("dpdx(field)"));
        assert!(source.contains("transmission"));
        assert!(source.contains("iridescent"));
        assert!(source.contains("living_flow"));
        assert!(source.contains("Beer-Lambert-style"));
        assert!(source.contains("optical_depth"));
        assert!(source.contains("membrane_alpha"));
        assert!(source.contains("flow_light"));
        assert!(source.contains("let shape_alpha = merged_coverage"));
        assert!(source.contains("droplets = min(droplets, distance)"));
        assert!(source.contains("packed.w > 0.001"));
        assert!(source.contains("let blur_offset"));
        assert!(!source.contains("soft_ground_shadow"));
        assert!(source.contains("sdf_antialias"));
        assert!(source.contains("uncovered_by_shape"));
        assert!(source.contains("limbal_ring"));
        assert!(source.contains("collarette"));
        assert!(source.contains("sd_liquid_bloblet"));
        assert!(source.contains("sd_tapered_segment"));
        assert!(source.contains("expressive_mouth_distance"));
        assert!(source.contains("let outer_mid"));
        assert!(source.contains("filament_strength"));
        assert!(source.contains("globals.droplet_bridge[index]"));
        assert!(source.contains("modal_radius"));
        assert!(source.contains("Rayleigh/Lamb-like"));
        assert!(source.contains("area_compensation"));
        assert!(!source.contains("flow_shadow"));
        assert!(source.contains("premultiplied"));

        let compose_source = include_str!("compose.wgsl");
        let compose_module =
            naga::front::wgsl::parse_str(compose_source).expect("resolve WGSL parses");
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .validate(&compose_module)
        .expect("resolve WGSL validates");
        assert!(compose_source.contains("resolved_organism"));
        assert!(compose_source.contains("if (globals.post.y < 1.5)"));
        assert!(
            compose_source
                .contains("return textureSample(organism_texture, organism_sampler, uv);")
        );
        assert!(compose_source.contains("footprint_shadow"));
        assert!(compose_source.contains("let source_uv = uv - globals.shadow.xy * pixel"));
        assert_eq!(
            compose_source
                .matches("textureSample(shadow_texture")
                .count(),
            1
        );
        assert!(!compose_source.contains("array<vec2<f32>, 40>"));
        assert!(!compose_source.contains("shadow_shape"));
        assert!(compose_source.contains("tone_map_premultiplied"));
        assert!(compose_source.contains("tone_map_reinhard_white"));
        assert!(compose_source.contains("vec2<f32>(0.0, 1.0)"));

        for (label, source) in [
            (
                "intermediate clear",
                include_str!("clear_intermediate.wgsl"),
            ),
            ("liquid density", include_str!("liquid_density.wgsl")),
            ("liquid filter", include_str!("liquid_filter.wgsl")),
            ("liquid surface", include_str!("liquid_surface.wgsl")),
            (
                "separable shadow filter",
                include_str!("shadow_filter.wgsl"),
            ),
        ] {
            let module = naga::front::wgsl::parse_str(source)
                .unwrap_or_else(|error| panic!("{label} WGSL parses: {error}"));
            naga::valid::Validator::new(
                naga::valid::ValidationFlags::all(),
                naga::valid::Capabilities::all(),
            )
            .validate(&module)
            .unwrap_or_else(|error| panic!("{label} WGSL validates: {error}"));
        }
        let density_source = include_str!("liquid_density.wgsl");
        assert!(density_source.contains("particle_fragment"));
        assert!(!density_source.contains("bond_fragment"));
        assert!(!density_source.contains("capsule_coordinate"));
        assert!(density_source.contains("point.y / organism_scale"));
        assert!(!density_source.contains("-point.y / organism_scale"));
        let liquid_surface = include_str!("liquid_surface.wgsl");
        assert!(liquid_surface.contains("Beer-Lambert absorption"));
        assert!(liquid_surface.contains("face_space"));
        assert!(liquid_surface.contains("point.y = -point.y"));
        assert!(liquid_surface.contains("vec2<f32>(0.0, 1.0)"));
        assert!(liquid_surface.contains("let visibility = 1.0"));
        assert!(liquid_surface.contains("dual") || liquid_surface.contains("tight_specular"));
        assert!(!liquid_surface.contains("clamp(color"));
        assert!(liquid_surface.contains("fiber_prefilter"));
        assert!(liquid_surface.contains("expressive_mouth_distance"));
        assert!(liquid_surface.contains("let p6"));
        assert!(liquid_surface.contains("var inner_mid"));
        assert!(liquid_surface.contains("let cheek_left"));
        assert!(liquid_surface.contains("safe_brow_center_y"));
        assert_eq!(
            liquid_surface.matches("= safe_brow_center_y(").count(),
            5,
            "every brow control point must preserve eye clearance"
        );
        assert!(!liquid_surface.contains("globals.viewport_time.y * 0.03"));
        let renderer_source = include_str!("renderer.rs");
        assert!(!renderer_source.contains("liquid_bubble.wgsl"));
        assert!(!renderer_source.contains("liquid_bubble_pipeline"));
        assert!(!renderer_source.contains("pack_bonds"));
    }

    #[test]
    fn extreme_legal_genomes_stay_valid() {
        for seed in 0..96 {
            let genome = Genome::from_seed(seed);
            let body = ProceduralBody::generate(&genome).unwrap();
            body.mesh.validate().unwrap();
            assert!(body.graph.is_valid());
        }
    }

    #[test]
    fn projected_hit_shape_tracks_visible_mass() {
        let genome = Genome::from_seed(7);
        let body = ProceduralBody::generate(&genome).unwrap();
        assert!(body.projected_hit_test(Vec2::new(0.5, 0.5)));
        assert!(!body.projected_hit_test(Vec2::new(0.0, 0.0)));
    }

    #[test]
    fn fixed_desktop_host_hit_test_tracks_large_shader_presentation_offsets() {
        let genome = Genome::from_seed(7);
        let mut body = ProceduralBody::generate(&genome).unwrap();
        let mut profile = LiquidTuningProfile::for_seed(genome.identity_seed);
        profile.render_mode = BodyRenderMode::ParticlePbf;
        body.apply_tuning_profile(profile).unwrap();
        let width = 2_560.0;
        let height = 1_080.0;
        body.set_render_aspect(width / height);
        body.set_presentation_scale(1.845 * height / 1_152.0);
        let target = Vec2::new(0.78, 0.65);
        let target_pixels = target * Vec2::new(width, height);
        body.set_presentation_offset_pixels(target_pixels - Vec2::new(width, height) * 0.5, height);
        body.animation_update(
            &intent(LocomotionMode::Hover, Vec2::splat(0.5)),
            0.0,
            1.0 / 120.0,
        );

        let parameters = body.render_parameters(&genome, 0.0);
        assert!(parameters.presentation_offset.x > 0.82);
        let mut target_local = target * 2.0 - Vec2::ONE;
        target_local.x *= width / height;
        target_local.y = -target_local.y;
        target_local *= body.projection_scale();
        target_local -= parameters.presentation_offset;
        let state = body.embodiment.liquid.render_state();
        let nearest = state.particles[..state.particle_count]
            .iter()
            .filter(|particle| particle.main_component)
            .map(|particle| particle.position.distance(target_local))
            .fold(f32::INFINITY, f32::min);
        assert!(
            body.projected_hit_test(target),
            "target local={target_local:?}, nearest particle={nearest}, offset={:?}",
            parameters.presentation_offset,
        );
        assert!(!body.projected_hit_test(Vec2::splat(0.5)));
    }

    #[test]
    fn tuning_profile_controls_the_explicit_renderer_path() {
        let genome = Genome::from_seed(71);
        let mut body = ProceduralBody::generate(&genome).unwrap();
        assert_eq!(
            body.render_parameters(&genome, 0.3).render_mode,
            BodyRenderMode::AnalyticJelly
        );

        let mut profile = LiquidTuningProfile::for_seed(genome.identity_seed);
        profile.render_mode = BodyRenderMode::ParticlePbf;
        profile.compositor.render_scale = 1;
        profile.compositor.shadow_horizontal_offset = -11.0;
        profile.compositor.shadow_vertical_offset = 14.0;
        profile.compositor.shadow_feather = 67.0;
        profile.compositor.exposure = 1.37;
        profile.face.visible = false;
        profile.face.origin = [0.08, 0.21];
        profile.face.scale = [1.2, 0.8];
        profile.pbf.particle_count = 48;
        profile.pbf.scorr_k = 0.011;
        profile.pbf.viscosity = 0.123;
        profile.pbf.render_center_smoothing = 0.24;
        profile.material.absorption = 0.31;
        profile.material.refraction = 1.27;
        profile.material.rim_strength = 0.83;
        profile.material.internal_flow = 1.17;
        profile.material.halo = 0.19;
        profile.material.pseudo_depth = 0.29;
        profile.material.variant = MaterialVariant::CurrentSafe;
        profile.material.internal_orb_count = 3;
        profile.material.internal_orb_intensity = 1.81;
        body.apply_tuning_profile(profile).unwrap();
        for _ in 0..120 {
            body.presentation_update(1.0 / 60.0);
        }
        let parameters = body.render_parameters(&genome, 0.3);

        assert_eq!(parameters.render_mode, BodyRenderMode::ParticlePbf);
        assert_eq!(parameters.render_scale, 1);
        assert!(!parameters.face_visible);
        assert_eq!(body.embodiment.liquid.tuning().particle_count, 48);
        assert_eq!(body.embodiment.liquid.tuning().scorr_k, 0.011);
        assert_eq!(body.embodiment.liquid.tuning().viscosity, 0.123);
        assert_eq!(
            body.embodiment.liquid.tuning().render_center_smoothing,
            0.24
        );
        assert_eq!(parameters.material_absorption, 0.31);
        assert_eq!(parameters.material_refraction, 1.27);
        assert_eq!(parameters.material_rim_strength, 0.83);
        assert_eq!(parameters.flow_strength, 0.32);
        assert_eq!(parameters.halo, 0.19);
        assert_eq!(parameters.material_pseudo_depth, 0.29);
        assert_eq!(parameters.material_variant, MaterialVariant::CurrentSafe);
        assert_eq!(parameters.material_internal_orb_count, 3);
        assert_eq!(parameters.material_internal_orb_intensity, 1.81);
        assert_eq!(parameters.shadow_horizontal_offset, -11.0);
        assert_eq!(parameters.shadow_vertical_offset, 14.0);
        assert_eq!(parameters.shadow_feather, 67.0);
        assert_eq!(parameters.exposure, 1.37);
        assert!(
            parameters
                .liquid
                .face_frame
                .origin
                .distance(Vec2::new(0.08, 0.21))
                < 1.0e-4
        );
        assert!(
            parameters
                .liquid
                .face_frame
                .scale
                .distance(Vec2::new(1.2, 0.8))
                < 1.0e-4
        );
        assert_eq!(parameters.occlusion_mode, OcclusionMode::Front);
    }

    #[test]
    fn fast_phenotype_changes_runtime_channels_without_touching_structural_locks() {
        let genome = Genome::from_seed(73);
        let mut body = ProceduralBody::generate(&genome).unwrap();
        let structural = (
            body.tuning.pbf.fixed_hz,
            body.tuning.pbf.particle_count,
            body.tuning.pbf.substeps,
            body.tuning.pbf.density_iterations,
            body.tuning.pbf.maximum_speed,
        );
        let baseline = body.render_parameters(&genome, 0.4);
        let mut fast = FastPhenotypeActuation::default();
        fast.analytic.body_length_scale = 1.05;
        fast.analytic.modal_amplitude_multiplier = 1.32;
        fast.material.emission_multiplier = 1.35;
        fast.material.hue_shift_turns = 0.02;
        fast.material.caustic_speed_multiplier = 1.8;
        fast.visual_physiology.pulse_amplitude = 0.9;
        fast.visual_physiology.droplet_energy = 0.85;
        fast.apparent_scale = 1.04;
        body.set_fast_phenotype_actuation(fast);
        body.embodied_update(
            &intent(LocomotionMode::Hover, Vec2::splat(0.5)),
            &SensorFrame::default(),
            AffectState::default(),
            VisualMindInput::default(),
            VoiceVisualState::default(),
            1.0 / 120.0,
        );
        let effective = body.render_parameters(&genome, 0.4);

        assert!(effective.body_length > baseline.body_length);
        assert!(effective.material_emission > baseline.material_emission);
        assert_eq!(effective.primary_hsv, baseline.primary_hsv);
        assert_eq!(effective.secondary_hsv, baseline.secondary_hsv);
        assert_ne!(effective.glow_hsv, baseline.glow_hsv);
        assert!(effective.pulse > baseline.pulse);
        assert_eq!(
            effective.material_caustic_speed,
            baseline.material_caustic_speed
        );
        assert_eq!(body.embodiment.physiology.pose.droplet_energy, 0.85);
        assert_eq!(
            (
                body.tuning.pbf.fixed_hz,
                body.tuning.pbf.particle_count,
                body.tuning.pbf.substeps,
                body.tuning.pbf.density_iterations,
                body.tuning.pbf.maximum_speed,
            ),
            structural
        );
    }

    #[test]
    fn mood_color_blend_changes_glow_without_touching_identity_pigment() {
        let base = Vec3::new(0.74, 0.62, 0.46);
        let runtime = MaterialRuntimeActuation {
            hue_shift_turns: 0.02,
            saturation_delta: 0.06,
            value_delta: 0.12,
            ..MaterialRuntimeActuation::default()
        };
        let identity = apply_glow_runtime(base, runtime, 0.0);
        let mood = apply_glow_runtime(base, runtime, 1.0);
        let hue_distance = (mood.x - base.x + 0.5).rem_euclid(1.0) - 0.5;

        assert_eq!(identity, base);
        assert!(hue_distance.abs() >= 0.06);
        assert!(
            mood.z - base.z < 0.07,
            "mood color washed the soul glow toward white"
        );
    }

    #[test]
    fn body_feedback_v2_is_authoritative_bounded_and_carries_efference_copy() {
        let genome = Genome::from_seed(79);
        let mut body = ProceduralBody::generate(&genome).unwrap();
        let intent = intent(LocomotionMode::Seek, Vec2::new(0.8, 0.2));
        let sensors = SensorFrame {
            cursor_position: Vec2::new(0.7, 0.3),
            cursor_distance_to_pet: 0.25,
            user_presence: Some(1.0),
            ..SensorFrame::default()
        };
        body.fixed_update(&genome, &intent, &sensors, 1.0 / 120.0);
        let frame = body.body_feedback_v2(&intent, &sensors, None, 17);

        assert_eq!(frame.frame_id, 17);
        assert!(frame.is_valid());
        assert_eq!(
            frame.efference_copy.intended_velocity,
            body.simulation.motor_velocity
        );
        assert_eq!(
            frame.motion.world_position,
            body.simulation.feedback.world_position
        );
        assert!(frame.environment.user_present);
    }

    #[test]
    fn desktop_displacement_reaches_the_liquid_in_body_space() {
        let genome = Genome::from_seed(29);
        let mut body = ProceduralBody::generate(&genome).unwrap();
        body.set_desktop_motion_space(Vec2::new(1_920.0, 1_080.0), 320.0);
        let intent = intent(LocomotionMode::Hover, Vec2::splat(0.5));
        body.simulation.feedback.world_position = Vec2::splat(0.5);
        body.embodied_update(
            &intent,
            &SensorFrame::default(),
            AffectState::default(),
            VisualMindInput::default(),
            VoiceVisualState::default(),
            1.0 / 120.0,
        );
        let before = body.embodiment.droplets.centroid();
        body.simulation.feedback.world_position += Vec2::new(0.01, 0.0);
        body.simulation.feedback.velocity = Vec2::new(0.10, 0.0);
        body.embodied_update(
            &intent,
            &SensorFrame::default(),
            AffectState::default(),
            VisualMindInput::default(),
            VoiceVisualState::default(),
            1.0 / 120.0,
        );
        let after = body.embodiment.droplets.centroid();
        assert!(
            after.x < before.x - 0.006,
            "desktop displacement should create visible but bounded shell lag: {before:?} -> {after:?}"
        );
    }

    #[test]
    fn acceleration_limited_locomotion_arrives_without_nan() {
        let genome = Genome::from_seed(17);
        let mut body = ProceduralBody::generate(&genome).unwrap();
        let sensors = SensorFrame::default();
        let intent = intent(LocomotionMode::Arrive, Vec2::new(0.8, 0.25));
        for _ in 0..3_600 {
            body.fixed_update(&genome, &intent, &sensors, 1.0 / 120.0);
            body.embodied_update(
                &intent,
                &sensors,
                AffectState::default(),
                VisualMindInput::default(),
                VoiceVisualState::default(),
                1.0 / 120.0,
            );
        }
        assert!(body.simulation.feedback.world_position.is_finite());
        assert!(body.simulation.feedback.velocity.is_finite());
        assert!(
            body.simulation
                .feedback
                .world_position
                .distance(intent.target_position)
                < 0.04
        );
    }

    #[test]
    fn particle_projection_and_face_stay_stable_while_blend_preserves_genome_color_identity() {
        let lab_genome = Genome::from_seed(42);
        let production_genome = Genome::from_seed(5_784_121_873_664_838_231);
        let mut lab = ProceduralBody::generate(&lab_genome).unwrap();
        let mut production = ProceduralBody::generate(&production_genome).unwrap();
        let mut lab_profile = LiquidTuningProfile::for_seed(42);
        lab_profile.render_mode = BodyRenderMode::ParticlePbf;
        lab.apply_tuning_profile(lab_profile.clone()).unwrap();
        production.apply_tuning_profile(lab_profile).unwrap();

        assert_eq!(
            lab.projection_scale(),
            renderer::PARTICLE_PBF_ORGANISM_SCALE
        );
        assert_eq!(production.projection_scale(), lab.projection_scale());
        let lab_parameters = lab.render_parameters(&lab_genome, 0.3);
        let production_parameters = production.render_parameters(&production_genome, 0.3);
        assert_eq!(production_parameters.eye_size, lab_parameters.eye_size);
        assert_eq!(
            production_parameters.eye_spacing,
            lab_parameters.eye_spacing
        );
        assert_eq!(
            production_parameters.pupil_ratio,
            lab_parameters.pupil_ratio
        );
        assert_ne!(
            production_parameters.primary_hsv,
            lab_parameters.primary_hsv
        );
        assert_eq!(
            lab.tuning_profile().material.color_source_mode,
            ColorSourceMode::GenomeAuthoredBlend
        );
    }

    #[test]
    fn presentation_scale_expands_projection_guard_and_stays_in_render_parameters() {
        let genome = Genome::from_seed(42);
        let mut body = ProceduralBody::generate(&genome).unwrap();
        let baseline = body.projection_scale();
        body.set_presentation_scale(1.23);
        assert!((body.projection_scale() - baseline * 1.23).abs() < 1.0e-6);
        assert_eq!(
            body.render_parameters(&genome, 0.3).presentation_scale,
            1.23
        );
        body.set_presentation_scale(f32::NAN);
        assert!((body.projection_scale() - baseline * 1.23).abs() < 1.0e-6);
    }

    #[test]
    fn ecology_color_effect_is_temporary_bounded_and_keeps_genome_identity() {
        let genome = Genome::from_seed(45);
        let original_hsv = genome.body.primary_color_hsv;
        let mut body = ProceduralBody::generate(&genome).unwrap();
        let baseline = body.render_parameters(&genome, 0.4);
        body.set_ecology_visual_effect(EcologyVisualEffect {
            hue: 0.82,
            color_blend: 9.0,
            flow_boost: 0.7,
            glow_boost: 0.8,
            cohesion_bias: 0.4,
            translucency_boost: 0.2,
            contrast_reduction: 0.9,
        });
        let affected = body.render_parameters(&genome, 0.4);
        assert_eq!(affected.primary_hsv, baseline.primary_hsv);
        assert_eq!(affected.secondary_hsv, baseline.secondary_hsv);
        assert_ne!(affected.glow_hsv.x, baseline.glow_hsv.x);
        assert!(affected.pattern_contrast >= 0.18);
        assert!(affected.translucency <= 1.0);
        assert_eq!(genome.body.primary_color_hsv, original_hsv);

        body.set_ecology_visual_effect(EcologyVisualEffect::default());
        let restored = body.render_parameters(&genome, 0.4);
        assert_eq!(restored.primary_hsv, baseline.primary_hsv);
        assert_eq!(restored.secondary_hsv, baseline.secondary_hsv);
        assert_eq!(restored.glow_hsv, baseline.glow_hsv);
    }
}
