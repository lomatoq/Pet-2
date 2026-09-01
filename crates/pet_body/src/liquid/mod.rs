#[cfg(test)]
mod active_flow;
mod body_snapshot;
mod collisions;
mod component_lifecycle;
mod components;
mod density;
mod face_frame;
mod interaction;
mod kernels;
mod motor_field;
mod particles;
#[cfg(test)]
mod rescue_tests;
#[cfg(test)]
mod shape_homeostasis;
mod surface_tension;
mod topology_guard;
mod viscoelastic_bonds;
mod viscosity;
mod xpbd;

use std::{array, f32::consts::TAU};

use glam::Vec2;
use lifecore::{BodyFeedback, BodyGenome, BodyIntent, EmbodiedInteractionFrame, SensorFrame};
use pet_ecology::EmbodiedEnvironmentFrame;

use crate::{
    DerivedVisualTraits, DropletMotion, FaceTuning, InteractionTuning, MaterialVariant,
    ModalDeformation, PbfTuning, TopologyConstraintMode, VisualMindInput, VisualPhysiologyPose,
};

use self::{
    collisions::{
        GrabParameters, MaterialGrab, MaterialStressFrame, apply_external_contact_forces,
        apply_interaction_forces,
    },
    component_lifecycle::ComponentLifecycleTracker,
    components::{ComponentSummary, assign_components},
    density::{calibrate_rest_density, mean_density_error, update_density_and_surface},
    face_frame::{FaceFrame, FaceFrameRuntime},
    interaction::LiquidInteractionProbe,
    motor_field::{CharacterFieldParameters, apply_character_field},
    particles::{
        KERNEL_RADIUS, LiquidParticle, MAX_LIQUID_PARTICLES, PARTICLE_SPACING,
        initialize_particles_with,
    },
    surface_tension::apply_surface_tension,
    topology_guard::{TopologyDecision, TopologyGuard},
    viscoelastic_bonds::{
        BondMaterial, BondUpdateParameters, MAX_BONDS, ViscoelasticBond, initialize_bonds,
        solve_bonds, update_bonds,
    },
    viscosity::apply_xsph_viscosity,
    xpbd::{DensityConstraintParameters, solve_density_constraints},
};

pub const MAX_IDLE_FRAGMENTS: usize = 24;
pub const MAX_LIQUID_RENDER_PARTICLES: usize = MAX_LIQUID_PARTICLES + MAX_IDLE_FRAGMENTS;
pub const MAX_PARTICLES: usize = MAX_LIQUID_RENDER_PARTICLES;

pub use body_snapshot::{
    BODY_MATERIAL_SNAPSHOT_SCHEMA_VERSION, BodyMaterialSnapshot, BodySnapshotError,
    SavedComponentLifecycle, SavedLiquidParticle, SavedTrackedComponent, SavedViscoelasticBond,
    liquid_structural_tuning_hash,
};

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum LeanPhase {
    Motion,
    RecoverUpright,
    #[default]
    UprightHold,
    IdleLean,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct PinchEvent {
    position: Vec2,
    normal: Vec2,
    velocity: Vec2,
    radius: f32,
    emission: f32,
    pigment: f32,
    material_coordinate: Vec2,
    optical_thickness: f32,
    lifetime: f32,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum IdleFragmentLifecycle {
    #[default]
    Legacy,
    Budding,
    /// Pinch spray moves while hidden for a few frames so the individual
    /// kernels become a fan before they contribute to the visible iso-field.
    Spray,
    Detached,
    Fading,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct IdleFragment {
    position: Vec2,
    velocity: Vec2,
    age: f32,
    lifetime: f32,
    radius: f32,
    emission: f32,
    pigment: f32,
    material_coordinate: Vec2,
    optical_thickness: f32,
    active: bool,
    lifecycle: IdleFragmentLifecycle,
    source_index: usize,
    direction: Vec2,
    anchor_position: Vec2,
    detached_lifetime: f32,
    /// Hysteretic state of the same anisotropic iso-field shown by the GPU.
    render_connected: bool,
    seen_render_connection: bool,
    previous_saddle_density: f32,
}

impl IdleFragment {
    fn render_scale(self) -> f32 {
        if !self.active || self.lifetime <= 1.0e-5 {
            return 0.0;
        }
        if self.age >= self.lifetime && self.lifecycle != IdleFragmentLifecycle::Budding {
            return 0.0;
        }
        let progress = (self.age / self.lifetime).clamp(0.0, 1.0);
        match self.lifecycle {
            IdleFragmentLifecycle::Legacy => {
                let appear = smoothstep01((progress / 0.08).clamp(0.0, 1.0));
                appear * (1.0 - progress).powf(0.72)
            }
            IdleFragmentLifecycle::Budding => smoothstep01((progress / 0.16).clamp(0.0, 1.0)),
            IdleFragmentLifecycle::Spray => {
                let reveal_delay = (0.085 / self.lifetime).clamp(0.04, 0.28);
                let reveal_duration = (0.075 / self.lifetime).clamp(0.04, 0.24);
                smoothstep01(
                    ((progress - reveal_delay) / reveal_duration.max(1.0e-5)).clamp(0.0, 1.0),
                )
            }
            IdleFragmentLifecycle::Detached => 1.0,
            IdleFragmentLifecycle::Fading => (1.0 - progress).powf(0.72),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ParticleRenderState {
    pub position: Vec2,
    /// Immutable material-space coordinate carried through the reconstruction.
    pub material_coordinate: Vec2,
    pub axis_major: Vec2,
    pub major_radius: f32,
    pub minor_radius: f32,
    pub density: f32,
    pub optical_thickness: f32,
    pub emission: f32,
    pub pigment: f32,
    pub face_weight: f32,
    pub velocity: Vec2,
    pub component_id: u8,
    pub main_component: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct BubbleRenderState {
    pub position: Vec2,
    pub velocity: Vec2,
    pub radius: f32,
    pub emission: f32,
    pub pigment: f32,
    pub opacity: f32,
    pub material_coordinate: Vec2,
    pub optical_thickness: f32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct LiquidDiagnostics {
    pub particle_count: usize,
    pub component_count: usize,
    pub main_mass: f32,
    pub detached_mass: f32,
    pub density_error: f32,
    pub maximum_speed: f32,
    pub maximum_bond_strain: f32,
    pub rigid_angular_velocity: f32,
    pub visual_weber: f32,
    pub visual_ohnesorge: f32,
    pub visual_deborah: f32,
    pub face_confidence: f32,
    pub com_follow_ratio: f32,
    pub stretch_ratio: f32,
    pub stress_magnitude: f32,
    pub cursor_distance: f32,
    pub orientation: f32,
    pub lean_target: f32,
    pub budding_count: usize,
    pub bubble_count: usize,
    /// Total translational kinetic energy in the authoritative particle state.
    pub kinetic_energy: f32,
    /// Largest instantaneous compression ratio above calibrated rest density.
    pub maximum_compression: f32,
    /// Emergency circuit-breaker activations. Production acceptance requires zero.
    pub failsafe_hits: u64,
    /// Whole-solver recoveries. Production acceptance requires zero.
    pub recovery_count: u64,
    pub finite: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LiquidRenderState {
    pub particles: [ParticleRenderState; MAX_LIQUID_RENDER_PARTICLES],
    pub particle_count: usize,
    pub bubbles: [BubbleRenderState; MAX_IDLE_FRAGMENTS],
    pub bubble_count: usize,
    pub face_frame: FaceFrame,
    pub diagnostics: LiquidDiagnostics,
}

impl Default for LiquidRenderState {
    fn default() -> Self {
        Self {
            particles: [ParticleRenderState::default(); MAX_LIQUID_RENDER_PARTICLES],
            particle_count: 0,
            bubbles: [BubbleRenderState::default(); MAX_IDLE_FRAGMENTS],
            bubble_count: 0,
            face_frame: FaceFrame::default(),
            diagnostics: LiquidDiagnostics::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct LiquidMorphRuntime {
    particles: [LiquidParticle; MAX_LIQUID_PARTICLES],
    particle_count: usize,
    material_grab: MaterialGrab,
    interaction_probe: LiquidInteractionProbe,
    topology_guard: TopologyGuard,
    topology_decision: TopologyDecision,
    component_lifecycle: ComponentLifecycleTracker,
    bonds: [ViscoelasticBond; MAX_BONDS],
    bond_contact_age: [f32; MAX_LIQUID_PARTICLES * MAX_LIQUID_PARTICLES],
    rest_density: f32,
    body_origin: Vec2,
    elapsed: f32,
    seed_phase: f32,
    components: ComponentSummary,
    face_frame: FaceFrameRuntime,
    /// Unoriented major axis of the area-preserving character-field ellipse.
    flight_field_axis: Vec2,
    /// Ratio of major/minor field metric radii. One is the neutral field.
    flight_field_aspect: f32,
    diagnostics: LiquidDiagnostics,
    navigation_anchor_strength: f32,
    local_containment_bounds: Option<(Vec2, Vec2)>,
    relaxation_elapsed: f32,
    idle_fragments: [IdleFragment; MAX_IDLE_FRAGMENTS],
    idle_fragment_timer: f32,
    idle_fragment_sequence: u64,
    pinch_sequence: u64,
    lean_phase: LeanPhase,
    lean_phase_elapsed: f32,
    lean_phase_duration: f32,
    lean_target: f32,
    idle_lean_goal: f32,
    lean_sequence: u64,
    cinematic_features: bool,
    /// Emergency recovery is a presentation cross-fade, never a visible solver
    /// teleport. Physics owns only the newly initialized particles; these values
    /// retain the last valid splats for exactly the 250 ms visual hand-off.
    presentation_recovery_remaining: f32,
    presentation_recovery_from: [(Vec2, Vec2, f32, f32); MAX_LIQUID_PARTICLES],
    failsafe_hits: u64,
    recovery_count: u64,
    seed: u64,
    tuning: PbfTuning,
    interaction_tuning: InteractionTuning,
    pending_structural_tuning: Option<PendingLiquidTuning>,
    environment: EmbodiedEnvironmentFrame,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct PendingLiquidTuning {
    tuning: PbfTuning,
    interaction: InteractionTuning,
    face: FaceTuning,
    material_variant: MaterialVariant,
}

#[allow(dead_code)]
impl LiquidMorphRuntime {
    #[must_use]
    pub fn new(seed: u64) -> Self {
        Self::new_with_tuning(seed, PbfTuning::default())
    }

    fn new_with_tuning(seed: u64, tuning: PbfTuning) -> Self {
        let spacing = PARTICLE_SPACING * tuning.spacing_scale;
        let kernel_radius = KERNEL_RADIUS * tuning.kernel_radius_scale;
        let (mut particles, particle_count) =
            initialize_particles_with(seed, tuning.particle_count, tuning.spacing_scale);
        update_density_and_surface(&mut particles, particle_count, kernel_radius);
        let rest_density = calibrate_rest_density(&particles, particle_count, kernel_radius)
            * tuning.rest_density_scale;
        let component_spacing =
            component_graph_spacing(spacing, kernel_radius, tuning.iso_threshold, false);
        let components = assign_components(
            &mut particles,
            particle_count,
            component_spacing,
            tuning.component_link_radius_scale,
        );
        let bonds = initialize_bonds(
            &particles,
            particle_count,
            spacing,
            tuning.bond_create_radius_scale,
        );
        for particle in &mut particles[..particle_count] {
            particle.render_surface_score = particle.surface_score;
        }
        let mut runtime = Self {
            particles,
            particle_count,
            material_grab: MaterialGrab::default(),
            interaction_probe: LiquidInteractionProbe::default(),
            topology_guard: TopologyGuard::default(),
            topology_decision: TopologyDecision::default(),
            component_lifecycle: ComponentLifecycleTracker::default(),
            bonds,
            bond_contact_age: [0.0; MAX_LIQUID_PARTICLES * MAX_LIQUID_PARTICLES],
            rest_density,
            body_origin: Vec2::ZERO,
            elapsed: 0.0,
            seed_phase: seed as u32 as f32 / u32::MAX as f32 * TAU,
            components,
            face_frame: FaceFrameRuntime::default(),
            flight_field_axis: Vec2::Y,
            flight_field_aspect: 1.0,
            diagnostics: LiquidDiagnostics::default(),
            navigation_anchor_strength: 1.0,
            local_containment_bounds: None,
            relaxation_elapsed: 0.0,
            idle_fragments: [IdleFragment::default(); MAX_IDLE_FRAGMENTS],
            idle_fragment_timer: 1.6 + deterministic_unit(seed, 0, 0x71) * 2.2,
            idle_fragment_sequence: 0,
            pinch_sequence: 0,
            lean_phase: LeanPhase::UprightHold,
            lean_phase_elapsed: 0.0,
            lean_phase_duration: tuning.upright_hold,
            lean_target: 0.0,
            idle_lean_goal: 0.0,
            lean_sequence: 0,
            // The standalone runtime retains the preserved Current / Safe lane.
            // Applying a Cinematic material profile explicitly enables the v3
            // breathing and budding lifecycle below.
            cinematic_features: false,
            presentation_recovery_remaining: 0.0,
            presentation_recovery_from: [(Vec2::ZERO, Vec2::X, 1.0, 0.0); MAX_LIQUID_PARTICLES],
            failsafe_hits: 0,
            recovery_count: 0,
            seed,
            tuning,
            interaction_tuning: InteractionTuning::default(),
            pending_structural_tuning: None,
            environment: EmbodiedEnvironmentFrame::default(),
        };
        runtime.snap_render_proxies();
        runtime
    }

    pub fn set_tuning(
        &mut self,
        tuning: PbfTuning,
        interaction_tuning: InteractionTuning,
        mut face: FaceTuning,
        material_variant: MaterialVariant,
    ) {
        if material_variant == MaterialVariant::CinematicJelly {
            // Cinematic Jelly reserves enough range for the semantic attention
            // turn. Neutral remains exactly upright because material rotation
            // never feeds the face target.
            face.maximum_roll_radians = face.maximum_roll_radians.max(0.18);
        }
        let structural_change = self.tuning.particle_count != tuning.particle_count
            || (self.tuning.spacing_scale - tuning.spacing_scale).abs() > f32::EPSILON
            || (self.tuning.kernel_radius_scale - tuning.kernel_radius_scale).abs() > f32::EPSILON
            || (self.tuning.rest_density_scale - tuning.rest_density_scale).abs() > f32::EPSILON;
        if structural_change && !self.component_lifecycle.is_stable() {
            self.pending_structural_tuning = Some(PendingLiquidTuning {
                tuning,
                interaction: interaction_tuning,
                face,
                material_variant,
            });
            let mut immediately_safe = tuning;
            immediately_safe.particle_count = self.tuning.particle_count;
            immediately_safe.spacing_scale = self.tuning.spacing_scale;
            immediately_safe.kernel_radius_scale = self.tuning.kernel_radius_scale;
            immediately_safe.rest_density_scale = self.tuning.rest_density_scale;
            self.tuning = immediately_safe;
            self.interaction_tuning = interaction_tuning;
            self.face_frame.set_tuning(face);
            self.cinematic_features = material_variant == MaterialVariant::CinematicJelly;
            return;
        }
        if structural_change {
            let body_origin = self.body_origin;
            let elapsed = self.elapsed;
            let navigation_anchor_strength = self.navigation_anchor_strength;
            let local_containment_bounds = self.local_containment_bounds;
            let face_frame = self.face_frame.clone();
            let flight_field_axis = self.flight_field_axis;
            let flight_field_aspect = self.flight_field_aspect;
            let failsafe_hits = self.failsafe_hits;
            let recovery_count = self.recovery_count;
            let mut replacement = Self::new_with_tuning(self.seed, tuning);
            replacement.body_origin = body_origin;
            replacement.elapsed = elapsed;
            replacement.navigation_anchor_strength = navigation_anchor_strength;
            replacement.local_containment_bounds = local_containment_bounds;
            replacement.face_frame = face_frame;
            replacement.face_frame.begin_recovery();
            replacement.face_frame.set_tuning(face);
            replacement.flight_field_axis = flight_field_axis;
            replacement.flight_field_aspect = flight_field_aspect;
            replacement.cinematic_features = material_variant == MaterialVariant::CinematicJelly;
            replacement.failsafe_hits = failsafe_hits;
            replacement.recovery_count = recovery_count;
            replacement.interaction_tuning = interaction_tuning;
            replacement.pending_structural_tuning = None;
            *self = replacement;
        } else {
            self.tuning = tuning;
            self.interaction_tuning = interaction_tuning;
            self.face_frame.set_tuning(face);
            self.cinematic_features = material_variant == MaterialVariant::CinematicJelly;
        }
    }

    #[must_use]
    pub fn tuning(&self) -> PbfTuning {
        self.tuning
    }

    /// Controls only the host/navigation COM leash. Zero leaves the liquid in a
    /// translationally free local frame; cohesion and component return stay active.
    pub fn set_navigation_anchor_strength(&mut self, strength: f32) {
        if strength.is_finite() {
            self.navigation_anchor_strength = strength.clamp(0.0, 1.0);
        }
    }

    /// Optional renderer-local containment used by Body Lab. Production leaves
    /// this disabled so desktop/window interaction remains governed by perception.
    pub fn set_local_containment_bounds(&mut self, bounds: Option<(Vec2, Vec2)>) {
        self.local_containment_bounds = bounds.and_then(|(minimum, maximum)| {
            if minimum.is_finite()
                && maximum.is_finite()
                && (maximum - minimum).cmpgt(Vec2::splat(0.35)).all()
            {
                Some((minimum, maximum))
            } else {
                None
            }
        });
    }

    pub fn set_embodied_environment(&mut self, environment: &EmbodiedEnvironmentFrame) {
        self.environment = environment.clone();
    }

    /// Supplies the brain-authored target for the complete permanent face
    /// carrier. This changes presentation only; particles and components are
    /// never selected or forced by attention.
    pub fn set_face_attention_pose(&mut self, offset: Vec2, roll: f32) {
        self.face_frame.set_attention_pose(offset, roll);
    }

    #[allow(clippy::too_many_arguments)]
    pub fn update(
        &mut self,
        _genome: &BodyGenome,
        traits: &DerivedVisualTraits,
        _physiology: VisualPhysiologyPose,
        mind: VisualMindInput,
        _intent: &BodyIntent,
        sensors: &SensorFrame,
        feedback: &BodyFeedback,
        motion: DropletMotion,
        morph: ModalDeformation,
        breath: f32,
        dt: f32,
    ) {
        self.update_with_tilt(
            _genome,
            traits,
            _physiology,
            mind,
            _intent,
            sensors,
            feedback,
            motion,
            morph,
            breath,
            0.0,
            dt,
        );
    }

    #[allow(clippy::too_many_arguments)]
    pub fn update_with_tilt(
        &mut self,
        _genome: &BodyGenome,
        _traits: &DerivedVisualTraits,
        _physiology: VisualPhysiologyPose,
        _mind: VisualMindInput,
        _intent: &BodyIntent,
        sensors: &SensorFrame,
        feedback: &BodyFeedback,
        motion: DropletMotion,
        _morph: ModalDeformation,
        _breath: f32,
        _motion_tilt: f32,
        dt: f32,
    ) {
        let dt = if dt.is_finite() {
            dt.clamp(0.0, 0.05)
        } else {
            0.0
        };
        if dt <= f32::EPSILON {
            return;
        }
        if self.component_lifecycle.is_stable()
            && let Some(pending) = self.pending_structural_tuning.take()
        {
            self.set_tuning(
                pending.tuning,
                pending.interaction,
                pending.face,
                pending.material_variant,
            );
        }
        self.elapsed = (self.elapsed + dt).rem_euclid(3_600.0);
        if self.body_origin.length() > 16.0 {
            let rebase = self.body_origin;
            for particle in &mut self.particles[..self.particle_count] {
                particle.position -= rebase;
                particle.previous_position -= rebase;
                particle.predicted_position -= rebase;
                particle.render_position -= rebase;
            }
            self.components.main_com -= rebase;
            self.body_origin = Vec2::ZERO;
        }
        // Keep the last valid reconstruction before touching authoritative
        // state. If a poisoned value propagates through this tick, recovery
        // must cross-fade from what was actually visible, not from the already
        // invalid render proxies produced by the failed solve.
        let recovery_from = array::from_fn(|index| {
            if index >= self.particle_count {
                return (Vec2::ZERO, Vec2::X, 1.0, 0.0);
            }
            let particle = self.particles[index];
            (
                particle.render_position,
                normalized_or(particle.render_axis_major, Vec2::X),
                if particle.render_aspect.is_finite() {
                    particle.render_aspect.clamp(1.0, 2.15)
                } else {
                    1.0
                },
                if particle.render_surface_score.is_finite() {
                    particle.render_surface_score.clamp(0.0, 1.0)
                } else {
                    0.0
                },
            )
        });

        // Schema 16 has exactly one authoritative physics lane. The cinematic
        // material may alter shaders and lighting, never real mass or topology.
        self.idle_fragments.fill(IdleFragment::default());
        let parameters = MaterialParameters::from_tuning(self.tuning);
        let spacing = PARTICLE_SPACING * self.tuning.spacing_scale;
        let kernel_radius = KERNEL_RADIUS * self.tuning.kernel_radius_scale;
        self.update_flight_field_shape(motion, dt);
        let field_scale = self.tuning.character_field_radius_scale;
        apply_character_field(
            &mut self.particles,
            self.particle_count,
            self.body_origin,
            motion.acceleration,
            CharacterFieldParameters {
                radii: Vec2::new(0.35, 0.43) * field_scale,
                // Preserve the authored v15 return-strength feel while changing
                // its meaning from a component servo to a continuous well.
                well_acceleration: parameters.character_field_strength * (3.0 / 0.34),
                inertia_scale: self.tuning.flight_inertia,
                maximum_inertial_acceleration: 6.0,
                velocity_damping: (self.tuning.flight_damping * 0.75).clamp(0.0, 3.5),
                flight_axis: self.flight_field_axis,
                flight_aspect: self.flight_field_aspect,
            },
        );
        let cooperative_separation_scale = if sensors.interaction_actuation.allow_intentional_bud {
            0.35
        } else {
            1.0
        };
        apply_surface_tension(
            &mut self.particles,
            self.particle_count,
            kernel_radius,
            self.rest_density,
            parameters.surface_tension
                * (1.0 + sensors.interaction_actuation.cohesion_delta).clamp(0.75, 1.25)
                * cooperative_separation_scale,
        );
        let grab_parameters = GrabParameters {
            support_radius: kernel_radius
                * self.tuning.pointer_support_scale
                * if sensors.interaction_actuation.allow_intentional_bud {
                    0.72
                } else {
                    1.0
                },
            peak_acceleration: (self.tuning.grab_stiffness / 40.0).clamp(0.0, 10.0).min(
                if sensors.interaction_actuation.allow_intentional_bud {
                    6.0
                } else {
                    10.0
                },
            ),
            response_hz: if sensors.interaction_actuation.allow_intentional_bud {
                self.tuning.pointer_response_hz.min(18.0)
            } else {
                self.tuning.pointer_response_hz
            },
            max_speed_radii_per_second: 8.0,
            max_acceleration_radii_per_second_squared: 80.0,
            fade_in_seconds: 0.020,
            fade_out_seconds: 0.040,
        };
        apply_interaction_forces(
            &mut self.particles,
            self.particle_count,
            &mut self.material_grab,
            grab_parameters,
            self.body_origin,
            feedback.world_position,
            motion.world_to_body_scale,
            self.local_containment_bounds,
            sensors,
            feedback,
            // Mouse input owns only the one compact pointer potential.
            0.0,
            self.components.main_component,
            self.components.main_com,
            dt,
        );
        apply_external_contact_forces(
            &mut self.particles,
            self.particle_count,
            self.body_origin,
            feedback.world_position,
            motion.world_to_body_scale,
            &self.environment,
            grab_parameters.support_radius * 2.2,
        );
        self.component_lifecycle.apply_recovery_field(
            &mut self.particles,
            self.particle_count,
            self.components.main_com,
            self.local_containment_bounds,
            self.interaction_tuning,
        );
        if sensors.interaction_actuation.local_pulse > 0.0
            || sensors.interaction_actuation.recoil > 0.0
        {
            let contact = self.material_grab.readback().contact_center;
            let pulse_acceleration =
                (sensors.interaction_actuation.local_pulse / 0.03).clamp(0.0, 1.0) * 0.60;
            let recoil_acceleration =
                (sensors.interaction_actuation.recoil / 0.08).clamp(0.0, 1.0) * 0.85;
            let recoil_direction = (self.components.main_com - contact).normalize_or_zero();
            for particle in &mut self.particles[..self.particle_count] {
                let radial = (particle.position - contact).normalize_or_zero();
                particle.force +=
                    radial * pulse_acceleration + recoil_direction * recoil_acceleration;
            }
        }

        let bond_material = BondMaterial {
            compliance: self.tuning.bond_compliance,
            yield_strain: self.tuning.bond_yield_strain,
            break_strain: self.tuning.bond_break_strain,
            relaxation_time: self.tuning.bond_relaxation_time,
        };
        update_bonds(
            &self.particles,
            self.particle_count,
            &mut self.bonds,
            &mut self.bond_contact_age,
            BondUpdateParameters {
                material: bond_material,
                spacing,
                create_radius_scale: self.tuning.bond_create_radius_scale,
                create_speed_limit: self.tuning.bond_create_speed_limit,
                dt,
            },
        );

        for particle in &mut self.particles[..self.particle_count] {
            particle.previous_position = particle.position;
            particle.predicted_position =
                particle.position + particle.velocity * dt + particle.force * (dt * dt);
            particle.force = Vec2::ZERO;
        }
        let containment_bounds =
            self.local_containment_bounds
                .and_then(|(local_minimum, local_maximum)| {
                    let guard = kernel_radius * 0.48;
                    let minimum = self.body_origin + local_minimum + Vec2::splat(guard);
                    let maximum = self.body_origin + local_maximum - Vec2::splat(guard);
                    (maximum - minimum)
                        .cmpgt(Vec2::splat(1.0e-4))
                        .all()
                        .then_some((minimum, maximum))
                });
        solve_density_constraints(
            &mut self.particles,
            self.particle_count,
            DensityConstraintParameters {
                rest_density: self.rest_density,
                kernel_radius,
                compliance: self.tuning.density_compliance
                    * (1.0 + sensors.interaction_actuation.compliance_delta * 2.0)
                        .clamp(0.50, 1.50),
                // Akinci cohesion owns free-surface regularization. Reapplying
                // PBF artificial pressure in every XPBD iteration injected an
                // idle expansion pulse into the calibrated pack.
                scorr_k: 0.0,
                scorr_q_ratio: self.tuning.scorr_q_ratio,
                scorr_power: self.tuning.scorr_power,
                iterations: self.tuning.density_iterations,
                dt,
                containment_bounds,
            },
        );
        if self.interaction_tuning.topology_mode == TopologyConstraintMode::Viscoelastic {
            solve_bonds(
                &mut self.particles,
                &mut self.bonds,
                bond_material,
                self.tuning.bond_iterations,
                dt,
            );
        }
        let topology_link_distance = component_graph_spacing(
            spacing,
            kernel_radius,
            self.tuning.iso_threshold,
            self.cinematic_features,
        ) * self.tuning.component_link_radius_scale
            * 1.08;
        self.topology_decision = match self.interaction_tuning.topology_mode {
            TopologyConstraintMode::ObserveOnly => self.topology_guard.observe(
                &self.particles,
                self.particle_count,
                topology_link_distance,
                self.interaction_tuning,
            ),
            TopologyConstraintMode::GuardedNecks | TopologyConstraintMode::Viscoelastic => {
                self.topology_guard.enforce(
                    &mut self.particles,
                    self.particle_count,
                    topology_link_distance,
                    self.interaction_tuning,
                )
            }
        };

        // This is a circuit breaker, not normal material behavior. Acceptance
        // requires the counter to stay at zero in every replay.
        let emergency_speed = self.tuning.maximum_speed.max(5.0) * 8.0;
        let mut hit_circuit_breaker = false;
        for particle in &mut self.particles[..self.particle_count] {
            let mut velocity = (particle.predicted_position - particle.position) / dt;
            if velocity.is_finite() && velocity.length() > emergency_speed {
                velocity = velocity.clamp_length_max(emergency_speed);
                particle.predicted_position = particle.position + velocity * dt;
                hit_circuit_breaker = true;
            }
            particle.velocity = velocity;
            particle.position = particle.predicted_position;
        }
        if hit_circuit_breaker {
            self.failsafe_hits = self.failsafe_hits.saturating_add(1);
        }
        apply_xsph_viscosity(
            &mut self.particles,
            self.particle_count,
            kernel_radius,
            parameters.numerical_xsph,
            parameters.viscosity,
            dt,
        );
        update_density_and_surface(&mut self.particles, self.particle_count, kernel_radius);
        let component_spacing = component_graph_spacing(
            spacing,
            kernel_radius,
            self.tuning.iso_threshold,
            self.cinematic_features,
        );
        self.components = assign_components(
            &mut self.particles,
            self.particle_count,
            component_spacing,
            self.tuning.component_link_radius_scale,
        );
        let observed_strain = self
            .bonds
            .iter()
            .filter(|bond| bond.active)
            .map(|bond| bond.strain.max(0.0))
            .fold(0.0_f32, f32::max)
            .max((self.material_stretch_ratio() - 1.0).max(0.0));
        self.component_lifecycle.update(
            &self.particles,
            self.particle_count,
            self.components,
            self.material_grab.is_active(),
            observed_strain,
            self.local_containment_bounds,
            self.interaction_tuning,
            dt,
        );
        self.update_render_proxies(dt);

        let stress = self.material_grab.material_stress(
            &self.particles,
            self.particle_count,
            self.components.main_component,
            self.components.main_com,
        );
        self.face_frame.sample_target(
            &self.particles,
            self.particle_count,
            self.components.main_component,
            self.components.main_com,
            self.body_origin,
            dt,
        );
        self.update_diagnostics(parameters, stress, motion.velocity.length());
        let grab_readback = self.material_grab.readback();
        self.interaction_probe.update(
            &self.particles,
            self.particle_count,
            self.components,
            grab_readback,
            self.body_origin,
            feedback.world_position,
            motion.world_to_body_scale,
            sensors.cursor_position,
            observed_strain,
            self.diagnostics.density_error,
            self.interaction_tuning,
            self.topology_decision.budget_exhausted,
            dt,
        );
        self.component_lifecycle
            .decorate(self.interaction_probe.latest_mut());
        if !self.diagnostics.finite {
            self.recover_from_nonfinite(recovery_from);
        }
    }

    /// Retained only as source history while the rescue is being reviewed. It is
    /// never compiled and therefore cannot become a second solver lane.
    #[cfg(any())]
    #[allow(clippy::too_many_arguments)]
    fn update_with_tilt_legacy(
        &mut self,
        _genome: &BodyGenome,
        traits: &DerivedVisualTraits,
        _physiology: VisualPhysiologyPose,
        mut mind: VisualMindInput,
        _intent: &BodyIntent,
        sensors: &SensorFrame,
        feedback: &BodyFeedback,
        motion: DropletMotion,
        morph: ModalDeformation,
        breath: f32,
        motion_tilt: f32,
        dt: f32,
    ) {
        mind.sanitize();
        let dt = if dt.is_finite() {
            dt.clamp(0.0, 0.05)
        } else {
            0.0
        };
        if dt <= f32::EPSILON {
            return;
        }
        self.elapsed = (self.elapsed + dt).rem_euclid(3_600.0);
        if self.body_origin.length() > 16.0 {
            let rebase = self.body_origin;
            for particle in &mut self.particles[..self.particle_count] {
                particle.position -= rebase;
                particle.previous_position -= rebase;
                particle.predicted_position -= rebase;
                particle.render_position -= rebase;
            }
            for fragment in &mut self.idle_fragments {
                if fragment.active {
                    fragment.position -= rebase;
                }
            }
            self.components.main_com -= rebase;
            self.body_origin = Vec2::ZERO;
        }

        let impact = feedback
            .collision
            .as_ref()
            .map_or(0.0, |collision| collision.intensity)
            .clamp(0.0, 1.0);
        let flight_drive = if self.cinematic_features {
            smoothstep01(((motion.acceleration.length() - 0.08) / 0.72).clamp(0.0, 1.0))
        } else {
            0.0
        };
        let impact_acceleration_threshold = if self.cinematic_features { 3.4 } else { 5.5 };
        let high_impact = motion.acceleration.length() > impact_acceleration_threshold
            || impact > 0.35
            || mind.window_pressure > 0.45
            || mind.scroll_velocity.abs() > 0.65
            || (self.cinematic_features && sensors.pet_dragged);
        let substeps = if high_impact {
            self.tuning.impact_substeps
        } else {
            self.tuning.substeps
        };
        let sub_dt = dt / substeps as f32;
        let parameters = MaterialParameters::from_tuning(self.tuning);
        let spacing = PARTICLE_SPACING * self.tuning.spacing_scale;
        let kernel_radius = KERNEL_RADIUS * self.tuning.kernel_radius_scale;
        let grab_parameters = GrabParameters {
            radius: self.tuning.grab_radius,
            stiffness: self.tuning.grab_stiffness,
            damping: self.tuning.grab_damping,
            follow: self.tuning.grab_follow,
            stretch: self.tuning.grab_stretch,
        };
        let transient_drive = motion.acceleration.length() * 0.06
            + mind.window_pressure * 0.8
            + mind.scroll_velocity.abs() * 0.4
            + if sensors.pet_dragged { 1.0 } else { 0.0 };
        // Screen velocity is an activity cue for lean/breathing, not a sustained
        // deformation source. Keeping it in the recovery gate disabled shape
        // homeostasis for the entire cruise and let every earlier acceleration
        // accumulate into a permanent tail.
        let pose_drive = motion.velocity.length() * 0.45 + transient_drive;
        let lean_target = if self.cinematic_features {
            self.update_cinematic_lean(motion_tilt, pose_drive, sensors.pointer_released, dt)
        } else {
            0.0
        };

        if self.cinematic_features {
            // Bud positions must advance before they pull the shell, but visible
            // topology is resolved only after physics, containment, and render
            // proxy filtering below. Keeping those phases separate prevents a
            // CPU pinch event from preceding the field the GPU actually shows.
            self.advance_cinematic_fragments(dt, motion.presentation_displacement);
        }

        for _ in 0..substeps {
            apply_motor_field(
                &mut self.particles,
                self.particle_count,
                self.body_origin,
                motion.velocity,
                motion.acceleration,
                self.components,
                morph,
                mind,
                self.tuning.motor_gain,
                self.tuning.flight_inertia,
                self.tuning.flight_stretch,
                self.tuning.flight_damping,
                self.tuning.flight_max_lag,
                self.tuning.angular_damping,
                self.navigation_anchor_strength,
                self.cinematic_features,
            );
            apply_active_flow(
                &mut self.particles,
                self.particle_count,
                self.body_origin,
                self.seed_phase,
                self.elapsed,
                mind,
                traits.flow_speed,
            );
            if self.cinematic_features {
                self.apply_idle_breathing(breath, pose_drive, sub_dt);
                self.apply_budding_forces();
            }
            // Dynamic neighbours and the permanent character field own cohesion.
            // Pointer input adds only one temporary position gradient below.
            apply_surface_tension(
                &mut self.particles,
                self.particle_count,
                spacing,
                kernel_radius,
                parameters.surface_tension,
                sub_dt,
            );
            apply_interaction_forces(
                &mut self.particles,
                self.particle_count,
                &mut self.material_grab,
                grab_parameters,
                self.body_origin,
                feedback.world_position,
                motion.world_to_body_scale,
                self.local_containment_bounds,
                sensors,
                feedback,
                mind.scroll_velocity,
                self.components.main_component,
                self.components.main_com,
                sub_dt,
            );
            apply_external_contact_forces(
                &mut self.particles,
                self.particle_count,
                self.body_origin,
                feedback.world_position,
                motion.world_to_body_scale,
                &self.environment,
                grab_parameters.support_radius * 2.2,
            );
            apply_homeostatic_return(
                &mut self.particles,
                self.particle_count,
                self.components,
                self.body_origin,
                parameters.return_delay,
                // Pointer input is a second field, not a replacement owner. The
                // character well remains present while material is being pulled.
                parameters.return_strength,
                sub_dt,
            );
            if self.material_grab.is_active()
                || flight_drive > 0.05
                || mind.window_pressure > 0.28
                || mind.scroll_velocity.abs() > 0.42
            {
                self.relaxation_elapsed = 0.0;
            } else {
                self.relaxation_elapsed = (self.relaxation_elapsed + sub_dt).min(8.0);
            }
            let relaxation =
                smoothstep01(((self.relaxation_elapsed - 0.18) / 0.82).clamp(0.0, 1.0))
                    * (1.0 - flight_drive);
            apply_shape_homeostasis(
                &mut self.particles,
                self.particle_count,
                self.components.main_component,
                self.tuning.shape_recovery,
                relaxation,
            );
            self.apply_local_containment_forces(kernel_radius);

            for particle in &mut self.particles[..self.particle_count] {
                particle.previous_position = particle.position;
                particle.predicted_position = particle.position
                    + particle.velocity * sub_dt
                    + particle.force * (sub_dt * sub_dt);
                particle.force = Vec2::ZERO;
            }
            self.solve_local_containment(kernel_radius);
            for particle in &mut self.particles[..self.particle_count] {
                particle.velocity = ((particle.predicted_position - particle.position) / sub_dt)
                    .clamp_length_max(self.tuning.maximum_speed);
                particle.position = particle.predicted_position;
            }
            apply_xsph_viscosity(
                &mut self.particles,
                self.particle_count,
                kernel_radius,
                parameters.viscosity,
                sub_dt,
            );
            self.damp_free_flight(sub_dt, parameters.viscosity);
            self.damp_main_component_chatter(sub_dt, transient_drive, flight_drive);
            update_density_and_surface(&mut self.particles, self.particle_count, kernel_radius);
            let component_spacing = component_graph_spacing(
                spacing,
                kernel_radius,
                self.tuning.iso_threshold,
                self.cinematic_features,
            );
            self.components = assign_components(
                &mut self.particles,
                self.particle_count,
                component_spacing,
                self.tuning.component_link_radius_scale,
            );
            self.stabilize_main_orientation(sub_dt, lean_target);
        }

        self.update_render_proxies(dt);
        if self.cinematic_features {
            // Pointer drag never emits topology events. Cinematic idle buds retain
            // their own visual pinch lifecycle, independent of input.
            self.resolve_cinematic_pinches();
        } else {
            self.update_idle_fragments_legacy(dt);
        }

        let stress = self.material_grab.material_stress(
            &self.particles,
            self.particle_count,
            self.components.main_component,
            self.components.main_com,
        );
        self.face_frame.update(
            &self.particles,
            self.particle_count,
            self.components.main_component,
            self.components.main_com,
            self.body_origin,
            dt,
        );
        self.update_diagnostics(parameters, stress, motion.velocity.length());
        if !self.diagnostics.finite {
            let tuning = self.tuning;
            let face_frame = self.face_frame.clone();
            let cinematic_features = self.cinematic_features;
            *self = Self::new_with_tuning(self.seed, tuning);
            self.face_frame = face_frame;
            self.cinematic_features = cinematic_features;
        }
    }

    fn recover_from_nonfinite(
        &mut self,
        mut recovery_from: [(Vec2, Vec2, f32, f32); MAX_LIQUID_PARTICLES],
    ) {
        let tuning = self.tuning;
        let body_origin = self.body_origin;
        let elapsed = self.elapsed;
        let navigation_anchor_strength = self.navigation_anchor_strength;
        let local_containment_bounds = self.local_containment_bounds;
        let cinematic_features = self.cinematic_features;
        let flight_field_axis = self.flight_field_axis;
        let flight_field_aspect = self.flight_field_aspect;
        let failsafe_hits = self.failsafe_hits;
        let recovery_count = self.recovery_count.saturating_add(1);
        let mut face_frame = self.face_frame.clone();
        face_frame.begin_recovery();

        let mut replacement = Self::new_with_tuning(self.seed, tuning);
        replacement.body_origin = body_origin;
        replacement.elapsed = elapsed;
        replacement.navigation_anchor_strength = navigation_anchor_strength;
        replacement.local_containment_bounds = local_containment_bounds;
        replacement.cinematic_features = cinematic_features;
        replacement.flight_field_axis = flight_field_axis;
        replacement.flight_field_aspect = flight_field_aspect;
        replacement.failsafe_hits = failsafe_hits;
        replacement.recovery_count = recovery_count;
        replacement.face_frame = face_frame;
        replacement.presentation_recovery_remaining = 0.25;
        replacement.presentation_recovery_from = recovery_from;
        for (particle, recovery) in replacement.particles[..replacement.particle_count]
            .iter_mut()
            .zip(recovery_from[..replacement.particle_count].iter_mut())
        {
            particle.position += body_origin;
            particle.previous_position += body_origin;
            particle.predicted_position += body_origin;
            particle.render_position += body_origin;
            let (from_position, from_axis, from_aspect, from_surface) = *recovery;
            if from_position.is_finite() {
                particle.render_position = from_position;
                particle.render_axis_major = from_axis;
                particle.render_aspect = from_aspect;
                particle.render_surface_score = from_surface;
            } else {
                *recovery = (
                    particle.render_position,
                    particle.render_axis_major,
                    particle.render_aspect,
                    particle.render_surface_score,
                );
            }
        }
        replacement.presentation_recovery_from = recovery_from;
        replacement.update_diagnostics(
            MaterialParameters::from_tuning(tuning),
            MaterialStressFrame::default(),
            0.0,
        );
        *self = replacement;
    }

    /// Continuously reshapes the one permanent field into a slightly flattened,
    /// area-preserving flight silhouette. No particle position is affinely
    /// transformed and no component or topology state participates.
    fn update_flight_field_shape(&mut self, motion: DropletMotion, dt: f32) {
        let speed_drive = smoothstep01(((motion.velocity.length() - 0.12) / 0.88).clamp(0.0, 1.0));
        let acceleration_drive =
            smoothstep01(((motion.acceleration.length() - 0.16) / 1.34).clamp(0.0, 1.0));
        let drive = (1.0 - (1.0 - speed_drive) * (1.0 - acceleration_drive * 0.82)).clamp(0.0, 1.0);

        let velocity_direction = motion.velocity.normalize_or_zero();
        let acceleration_direction = motion.acceleration.normalize_or_zero();
        let travel_direction = (velocity_direction * speed_drive
            + acceleration_direction * acceleration_drive * 0.46)
            .normalize_or_zero();
        if travel_direction.length_squared() > 1.0e-8 {
            let mut target_axis = Vec2::new(-travel_direction.y, travel_direction.x);
            if target_axis.dot(self.flight_field_axis) < 0.0 {
                target_axis = -target_axis;
            }
            let current_angle = self.flight_field_axis.y.atan2(self.flight_field_axis.x);
            let target_angle = target_axis.y.atan2(target_axis.x);
            let axis_blend = 1.0 - (-6.5 * dt).exp();
            let next_angle = current_angle + wrap_angle(target_angle - current_angle) * axis_blend;
            self.flight_field_axis = Vec2::from_angle(next_angle);
        }

        let authored_stretch = self.tuning.flight_stretch.clamp(0.0, 2.0);
        let target_aspect = (1.0 + drive * authored_stretch * 0.36).clamp(1.0, 1.34);
        let aspect_rate = 3.5 + drive * 3.5;
        self.flight_field_aspect +=
            (target_aspect - self.flight_field_aspect) * (1.0 - (-aspect_rate * dt).exp());
        if !self.flight_field_axis.is_finite() || !self.flight_field_aspect.is_finite() {
            self.flight_field_axis = Vec2::Y;
            self.flight_field_aspect = 1.0;
        } else {
            self.flight_field_axis = self.flight_field_axis.normalize_or_zero();
            if self.flight_field_axis.length_squared() <= 1.0e-8 {
                self.flight_field_axis = Vec2::Y;
            }
            self.flight_field_aspect = self.flight_field_aspect.clamp(1.0, 1.34);
        }
    }

    #[must_use]
    pub fn render_state(&self) -> LiquidRenderState {
        let particles: [ParticleRenderState; MAX_LIQUID_RENDER_PARTICLES] =
            array::from_fn(|index| {
                if index >= self.particle_count {
                    return ParticleRenderState::default();
                }
                let particle = self.particles[index];
                let render_center = particle.render_position;
                let axis_major = particle.render_axis_major;
                let aspect = particle.render_aspect;
                let area_scale = aspect.sqrt();
                let kernel_radius = KERNEL_RADIUS * self.tuning.kernel_radius_scale;
                ParticleRenderState {
                    position: render_center - self.body_origin,
                    material_coordinate: particle.rest_position,
                    axis_major,
                    major_radius: kernel_radius * area_scale,
                    minor_radius: kernel_radius / area_scale,
                    density: 1.0,
                    optical_thickness: 0.82 + (1.0 - particle.render_surface_score) * 0.38,
                    emission: particle.emission,
                    pigment: particle.pigment,
                    face_weight: particle.face_weight,
                    velocity: particle.velocity,
                    component_id: particle.component_id,
                    main_component: particle.component_id == self.components.main_component,
                }
            });
        let mut bubbles = [BubbleRenderState::default(); MAX_IDLE_FRAGMENTS];
        let mut bubble_count = 0;
        for fragment in self
            .idle_fragments
            .iter()
            .filter(|fragment| fragment.active)
        {
            if bubble_count >= MAX_IDLE_FRAGMENTS {
                break;
            }
            let scale = fragment.render_scale();
            if scale <= 0.001 {
                continue;
            }
            let radius = fragment.radius * scale;
            bubbles[bubble_count] = BubbleRenderState {
                position: fragment.position - self.body_origin,
                velocity: fragment.velocity,
                radius,
                emission: fragment.emission,
                pigment: fragment.pigment,
                opacity: scale,
                material_coordinate: fragment.material_coordinate,
                optical_thickness: fragment.optical_thickness,
            };
            bubble_count += 1;
        }
        LiquidRenderState {
            particles,
            particle_count: self.particle_count,
            bubbles,
            bubble_count,
            face_frame: self.face_frame.frame,
            diagnostics: self.diagnostics,
        }
    }

    #[must_use]
    pub fn diagnostics(&self) -> LiquidDiagnostics {
        self.diagnostics
    }

    #[must_use]
    pub fn embodied_interaction_frame(&self) -> EmbodiedInteractionFrame {
        self.interaction_probe.latest()
    }

    /// Advances presentation-only trackers once per displayed frame. The fixed
    /// liquid simulation samples face targets but deliberately does not filter
    /// them, so two 120 Hz ticks before one 60 Hz present cannot double a visual
    /// step limit.
    pub fn presentation_update(&mut self, dt: f32) {
        self.face_frame.present(dt);
    }

    #[must_use]
    pub fn hit_test_main_component(&self, local_point: Vec2) -> bool {
        self.hit_test_main_component_with_margin(local_point, 0.0)
    }

    #[must_use]
    pub fn hit_test_main_component_with_margin(
        &self,
        local_point: Vec2,
        margin_local: f32,
    ) -> bool {
        let radius = KERNEL_RADIUS * self.tuning.kernel_radius_scale * 0.82 + margin_local.max(0.0);
        self.particles[..self.particle_count]
            .iter()
            .any(|particle| {
                (particle.render_position - self.body_origin).distance(local_point) <= radius
            })
    }

    fn damp_free_flight(&mut self, dt: f32, viscosity: f32) {
        if self.navigation_anchor_strength >= 0.999 || self.material_grab.is_active() {
            return;
        }
        // XSPH preserves common momentum. A small continuous-time ambient drag is
        // therefore required for the lab's "slosh, then settle here" zero-G mode.
        let free_fraction = 1.0 - self.navigation_anchor_strength;
        let decay = (-(0.42 + viscosity.clamp(0.0, 0.22) * 2.5) * free_fraction * dt).exp();
        for particle in &mut self.particles[..self.particle_count] {
            particle.velocity *= decay;
        }
    }

    fn material_stretch_ratio(&self) -> f32 {
        let mut axis = self.material_grab.drag_axis();
        if axis.length_squared() < 0.5 {
            axis = Vec2::X;
        }
        let mut current_min = f32::INFINITY;
        let mut current_max = f32::NEG_INFINITY;
        let mut rest_min = f32::INFINITY;
        let mut rest_max = f32::NEG_INFINITY;
        for particle in &self.particles[..self.particle_count] {
            if particle.component_id != self.components.main_component {
                continue;
            }
            let current = (particle.position - self.components.main_com).dot(axis);
            let rest = particle.rest_position.dot(axis);
            current_min = current_min.min(current);
            current_max = current_max.max(current);
            rest_min = rest_min.min(rest);
            rest_max = rest_max.max(rest);
        }
        let current_extent = (current_max - current_min).max(0.0);
        let rest_extent = (rest_max - rest_min).max(1.0e-4);
        (current_extent / rest_extent).clamp(0.0, 8.0)
    }

    fn damp_main_component_chatter(&mut self, dt: f32, external_drive: f32, flight_drive: f32) {
        if !self.cinematic_features {
            let calmness = 1.0 - ((external_drive - 0.04) / 0.76).clamp(0.0, 1.0);
            let legacy_decay = (-(3.0 - calmness * 0.50) * dt).exp();
            self.scale_main_relative_velocity(legacy_decay);
            return;
        }
        // XSPH already removes neighbour-scale noise and the authored flight
        // damping owns large-scale slosh. This final projection is therefore only
        // a weak safety damper; during coherent flight/grab it must not erase the
        // very mode that makes the body read as liquid.
        let coherent_drive = flight_drive.max(if self.material_grab.is_active() {
            1.0
        } else {
            0.0
        });
        let disruptive_drive = (external_drive - coherent_drive).clamp(0.0, 1.0);
        let rate =
            (1.10 * (1.0 - coherent_drive * 0.82) + disruptive_drive * 1.20).clamp(0.18, 2.30);
        let decay = (-rate * dt).exp();
        self.scale_main_relative_velocity(decay);
    }

    fn scale_main_relative_velocity(&mut self, decay: f32) {
        let mut mean_velocity = Vec2::ZERO;
        let mut count = 0.0_f32;
        for particle in &self.particles[..self.particle_count] {
            if particle.component_id == self.components.main_component {
                mean_velocity += particle.velocity;
                count += 1.0;
            }
        }
        mean_velocity /= count.max(1.0);
        for particle in &mut self.particles[..self.particle_count] {
            if particle.component_id == self.components.main_component {
                particle.velocity = mean_velocity + (particle.velocity - mean_velocity) * decay;
            }
        }
    }

    fn apply_idle_breathing(&mut self, breath: f32, external_drive: f32, _dt: f32) {
        let amplitude = self.tuning.idle_breath_amplitude;
        if amplitude <= 1.0e-5 || self.material_grab.is_active() {
            return;
        }
        let calm = 1.0 - smoothstep01(((external_drive - 0.03) / 0.42).clamp(0.0, 1.0));
        if calm <= 1.0e-4 {
            return;
        }
        let pose_wave = breath.clamp(0.0, 1.0) * 2.0 - 1.0;
        // The material mode owns the deliberately slow 5–8 s silhouette cycle;
        // physiology only phase-modulates it so emotion remains causal without
        // turning calm breathing into a fast global scale pulse.
        let material_wave = (self.elapsed * 0.92 * self.tuning.idle_breath_speed
            + self.seed_phase * 1.9
            + pose_wave * 0.16)
            .sin();
        let breath_wave = material_wave * 0.78 + pose_wave * 0.22;
        let mut proposed = [Vec2::ZERO; MAX_LIQUID_PARTICLES];
        let mut total = Vec2::ZERO;
        let mut member_count = 0.0_f32;
        for (index, particle) in self.particles[..self.particle_count].iter().enumerate() {
            if particle.component_id != self.components.main_component {
                continue;
            }
            let arm = particle.position - self.components.main_com;
            let angle = arm.y.atan2(arm.x);
            let harmonic = (angle * 2.0 + self.seed_phase).cos() * 0.72
                + (angle * 3.0 - self.seed_phase * 0.7).sin() * 0.28;
            let surface_weight = 0.36 + particle.surface_score.clamp(0.0, 1.0) * 0.64;
            let force = arm.normalize_or_zero()
                * harmonic
                * breath_wave
                * amplitude
                * 7.0
                * calm
                * surface_weight;
            proposed[index] = force;
            total += force;
            member_count += 1.0;
        }
        if member_count < 4.0 {
            return;
        }
        let mean = total / member_count;
        let mut torque = 0.0;
        let mut inertia = 0.0;
        for (index, particle) in self.particles[..self.particle_count].iter().enumerate() {
            if particle.component_id != self.components.main_component {
                continue;
            }
            proposed[index] -= mean;
            let arm = particle.position - self.components.main_com;
            torque += arm.perp_dot(proposed[index]);
            inertia += arm.length_squared();
        }
        let angular_projection = torque / inertia.max(1.0e-5);
        for (index, particle) in self.particles[..self.particle_count].iter_mut().enumerate() {
            if particle.component_id == self.components.main_component {
                let arm = particle.position - self.components.main_com;
                particle.force += proposed[index] - Vec2::new(-arm.y, arm.x) * angular_projection;
            }
        }
    }

    fn apply_budding_forces(&mut self) {
        let kernel_radius = KERNEL_RADIUS * self.tuning.kernel_radius_scale;
        let sigma_squared = (kernel_radius * 1.20).powi(2).max(1.0e-5);
        let main_component = self.components.main_component;
        let main_count = self.particles[..self.particle_count]
            .iter()
            .filter(|particle| particle.component_id == main_component)
            .count() as f32;
        if main_count < 4.0 {
            return;
        }
        for fragment in self.idle_fragments.iter().copied().filter(|fragment| {
            fragment.active && fragment.lifecycle == IdleFragmentLifecycle::Budding
        }) {
            let source_index = fragment
                .source_index
                .min(self.particle_count.saturating_sub(1));
            let source = self.particles[source_index];
            if source.component_id != main_component {
                continue;
            }
            let progress = (fragment.age / fragment.lifetime.max(1.0e-5)).clamp(0.0, 1.0);
            let envelope = (std::f32::consts::PI * progress).sin().max(0.0).powf(0.72);
            let pull = self.tuning.idle_bud_pull_strength * envelope;
            if pull <= 1.0e-5 {
                continue;
            }
            let mut weights = [0.0_f32; MAX_LIQUID_PARTICLES];
            let mut weight_sum = 0.0;
            for (index, particle) in self.particles[..self.particle_count].iter().enumerate() {
                if particle.component_id != main_component {
                    continue;
                }
                let distance_squared = particle.position.distance_squared(source.position);
                let weight = (-distance_squared / (2.0 * sigma_squared)).exp()
                    * (0.35 + particle.surface_score.clamp(0.0, 1.0) * 0.65);
                weights[index] = weight;
                weight_sum += weight;
            }
            let distributed_counterforce = pull * weight_sum / main_count;
            for (index, particle) in self.particles[..self.particle_count].iter_mut().enumerate() {
                if particle.component_id == main_component {
                    particle.force +=
                        fragment.direction * (pull * weights[index] - distributed_counterforce);
                }
            }
        }
    }

    fn advance_cinematic_fragments(&mut self, dt: f32, presentation_displacement: Vec2) {
        let containment = self.local_containment_bounds;
        let body_origin = self.body_origin;
        let main_component = self.components.main_component;
        let main_com = self.components.main_com;
        let kernel_radius = KERNEL_RADIUS * self.tuning.kernel_radius_scale;
        // Detached kernels live in desktop/presentation space. The renderer adds
        // the body's current presentation offset to every local point, so remove
        // that body's per-tick translation here to keep free fragments inertial.
        // Budding kernels remain material children until the visible pinch frame.
        for fragment in &mut self.idle_fragments {
            if fragment.active
                && matches!(
                    fragment.lifecycle,
                    IdleFragmentLifecycle::Spray
                        | IdleFragmentLifecycle::Detached
                        | IdleFragmentLifecycle::Fading
                )
            {
                fragment.position -= presentation_displacement;
                fragment.anchor_position -= presentation_displacement;
            }
        }
        for fragment in &mut self.idle_fragments {
            if !fragment.active {
                continue;
            }
            match fragment.lifecycle {
                IdleFragmentLifecycle::Budding => {
                    fragment.age += dt;
                    let mut source_index = fragment
                        .source_index
                        .min(self.particle_count.saturating_sub(1));
                    if self.particles[source_index].component_id != main_component {
                        source_index = self.particles[..self.particle_count]
                            .iter()
                            .enumerate()
                            .filter(|(_, particle)| particle.component_id == main_component)
                            .min_by(|(_, a), (_, b)| {
                                a.position
                                    .distance_squared(fragment.anchor_position)
                                    .total_cmp(
                                        &b.position.distance_squared(fragment.anchor_position),
                                    )
                            })
                            .map_or(source_index, |(index, _)| index);
                        fragment.source_index = source_index;
                    }
                    let anchor = self.particles[source_index].position;
                    fragment.anchor_position = anchor;
                    fragment.direction = normalized_or(anchor - main_com, fragment.direction);
                    let raw_progress = fragment.age / fragment.lifetime.max(1.0e-5);
                    let progress = raw_progress.clamp(0.0, 1.0);
                    let overdue = (fragment.age - fragment.lifetime).max(0.0);
                    let pending_extension =
                        kernel_radius * 2.35 * smoothstep01((overdue / 0.28).clamp(0.0, 1.0));
                    let excursion = bud_excursion(
                        kernel_radius,
                        fragment.radius,
                        progress,
                        self.tuning.idle_bud_neck_scale * self.tuning.neck_continuity,
                    ) + pending_extension;
                    let target = anchor + fragment.direction * excursion;
                    let previous = fragment.position;
                    fragment.position = fragment.position.lerp(target, 1.0 - (-12.0 * dt).exp());
                    fragment.velocity = (fragment.position - previous) / dt.max(1.0e-5);
                }
                IdleFragmentLifecycle::Spray | IdleFragmentLifecycle::Detached => {
                    fragment.age += dt;
                    fragment.position += fragment.velocity * dt;
                    let drag = if fragment.lifecycle == IdleFragmentLifecycle::Spray {
                        0.34
                    } else {
                        0.20
                    };
                    fragment.velocity *= (-drag * dt).exp();
                    if fragment.age >= fragment.lifetime {
                        fragment.lifecycle = IdleFragmentLifecycle::Fading;
                        fragment.age = 0.0;
                        fragment.lifetime = fragment.detached_lifetime * 0.45;
                    }
                }
                IdleFragmentLifecycle::Fading => {
                    fragment.age += dt;
                    fragment.position += fragment.velocity * dt;
                    fragment.velocity *= (-0.28 * dt).exp();
                    if fragment.age >= fragment.lifetime {
                        *fragment = IdleFragment::default();
                        continue;
                    }
                }
                IdleFragmentLifecycle::Legacy => {
                    *fragment = IdleFragment::default();
                    continue;
                }
            }

            if let Some((minimum, maximum)) = containment {
                let minimum = body_origin + minimum + Vec2::splat(fragment.radius);
                let maximum = body_origin + maximum - Vec2::splat(fragment.radius);
                for axis in 0..2 {
                    if fragment.position[axis] < minimum[axis] {
                        fragment.position[axis] = minimum[axis];
                        fragment.velocity[axis] = fragment.velocity[axis].abs() * 0.18;
                    } else if fragment.position[axis] > maximum[axis] {
                        fragment.position[axis] = maximum[axis];
                        fragment.velocity[axis] = -fragment.velocity[axis].abs() * 0.18;
                    }
                }
            }
        }

        if self.tuning.idle_fragment_size <= 0.001 || self.material_grab.is_active() {
            self.idle_fragment_timer = self.idle_fragment_timer.max(0.18);
            return;
        }
        self.idle_fragment_timer -= dt;
        if self.idle_fragment_timer > 0.0 {
            return;
        }

        let sequence = self.idle_fragment_sequence;
        let interval_jitter = lerp(
            5.0 / 9.0,
            5.0 / 3.0,
            deterministic_unit(self.seed, sequence, 0xA3),
        );
        self.idle_fragment_timer = self.tuning.idle_bud_interval * interval_jitter;
        let budding_count = self
            .idle_fragments
            .iter()
            .filter(|fragment| {
                fragment.active && fragment.lifecycle == IdleFragmentLifecycle::Budding
            })
            .count();
        if budding_count >= self.tuning.idle_bud_maximum {
            return;
        }
        let Some(slot) = self
            .idle_fragments
            .iter()
            .position(|fragment| !fragment.active)
        else {
            return;
        };

        let angle = deterministic_unit(self.seed, sequence, 0xC7) * TAU;
        let preferred_direction = Vec2::from_angle(angle);
        let Some((source_index, source)) = self.particles[..self.particle_count]
            .iter()
            .enumerate()
            .filter(|(_, particle)| particle.component_id == main_component)
            .max_by(|(_, a), (_, b)| {
                let score_a =
                    (a.position - main_com).dot(preferred_direction) + a.surface_score * 0.025;
                let score_b =
                    (b.position - main_com).dot(preferred_direction) + b.surface_score * 0.025;
                score_a.total_cmp(&score_b)
            })
        else {
            return;
        };
        let outward = normalized_or(source.position - main_com, preferred_direction);
        let size_roll = deterministic_unit(self.seed, sequence, 0x117);
        let size_jitter = deterministic_unit(self.seed, sequence, 0x12B);
        let radius_scale = if size_roll < 0.64 {
            0.42 + size_jitter * 0.36
        } else if size_roll < 0.93 {
            0.82 + size_jitter * 0.48
        } else {
            1.36 + size_jitter * 0.54
        };
        let radius = self.tuning.idle_fragment_size * radius_scale;
        let lifetime_jitter = 0.78 + deterministic_unit(self.seed, sequence, 0x139) * 0.48;
        let bud_jitter = 0.86 + deterministic_unit(self.seed, sequence, 0x151) * 0.28;
        self.idle_fragments[slot] = IdleFragment {
            position: source.position + outward * kernel_radius * 0.54,
            anchor_position: source.position,
            velocity: Vec2::ZERO,
            age: 0.0,
            lifetime: self.tuning.idle_bud_duration * bud_jitter,
            detached_lifetime: self.tuning.idle_fragment_lifetime * lifetime_jitter,
            radius,
            emission: (source.emission * 0.72 + 0.12).clamp(0.0, 1.0),
            pigment: source.pigment,
            material_coordinate: source.rest_position,
            optical_thickness: 0.82 + (1.0 - source.surface_score) * 0.38,
            active: true,
            lifecycle: IdleFragmentLifecycle::Budding,
            source_index,
            direction: outward,
            render_connected: false,
            seen_render_connection: false,
            previous_saddle_density: 0.0,
        };
        self.idle_fragment_sequence = self.idle_fragment_sequence.wrapping_add(1);
    }

    fn resolve_cinematic_pinches(&mut self) {
        let kernel_radius = KERNEL_RADIUS * self.tuning.kernel_radius_scale;
        let iso_threshold = self.tuning.iso_threshold.clamp(0.08, 0.82);
        let hysteresis = 0.018;
        let fragment_snapshot = self.idle_fragments;
        let mut pinch_events = [PinchEvent::default(); MAX_IDLE_FRAGMENTS];
        let mut pinch_event_count = 0;

        for (index, fragment) in self.idle_fragments.iter_mut().enumerate() {
            if !fragment.active || fragment.lifecycle != IdleFragmentLifecycle::Budding {
                continue;
            }
            let (_, pinch_position, saddle_density) = rendered_bud_connectivity(
                &self.particles[..self.particle_count],
                &fragment_snapshot,
                index,
                *fragment,
                kernel_radius,
                iso_threshold,
            );

            if !fragment.seen_render_connection && saddle_density >= iso_threshold {
                fragment.seen_render_connection = true;
                fragment.render_connected = true;
            } else if saddle_density >= iso_threshold + hysteresis {
                fragment.render_connected = true;
            }
            let crossed_visible_saddle = fragment.seen_render_connection
                && fragment.render_connected
                && saddle_density < iso_threshold - hysteresis;
            fragment.previous_saddle_density = saddle_density;
            if !crossed_visible_saddle {
                continue;
            }

            fragment.render_connected = false;
            let release_boost = fragment.direction
                * (0.040 + deterministic_unit(self.seed, self.pinch_sequence, 0x173) * 0.035);
            fragment.lifecycle = IdleFragmentLifecycle::Detached;
            fragment.age = 0.0;
            fragment.lifetime = fragment.detached_lifetime * 0.55;
            fragment.velocity += release_boost;
            pinch_events[pinch_event_count] = PinchEvent {
                position: pinch_position,
                normal: fragment.direction,
                velocity: fragment.velocity,
                radius: fragment.radius,
                emission: fragment.emission,
                pigment: fragment.pigment,
                material_coordinate: fragment.material_coordinate,
                optical_thickness: fragment.optical_thickness,
                lifetime: fragment.detached_lifetime,
            };
            pinch_event_count += 1;
        }

        for event in pinch_events[..pinch_event_count].iter().copied() {
            self.apply_pinch_recoil(event, kernel_radius);
            self.spawn_pinch_spray(event);
            self.pinch_sequence = self.pinch_sequence.wrapping_add(1);
        }
    }

    fn apply_pinch_recoil(&mut self, event: PinchEvent, kernel_radius: f32) {
        if self.tuning.pinch_bounce <= 1.0e-5 {
            return;
        }
        let normal = normalized_or(event.normal, Vec2::Y);
        let sigma_squared = (kernel_radius * 1.05).powi(2).max(1.0e-5);
        let main_component = self.components.main_component;
        let main_count = self.particles[..self.particle_count]
            .iter()
            .filter(|particle| particle.component_id == main_component)
            .count() as f32;
        if main_count < 4.0 {
            return;
        }
        let mut weight_sum = 0.0;
        for particle in &self.particles[..self.particle_count] {
            if particle.component_id == main_component {
                weight_sum += (-particle.position.distance_squared(event.position)
                    / (2.0 * sigma_squared))
                    .exp();
            }
        }
        let peak = (0.050 + event.velocity.length().min(0.40) * 0.18) * self.tuning.pinch_bounce;
        let mean_delta = -normal * peak * weight_sum / main_count;
        for particle in &mut self.particles[..self.particle_count] {
            if particle.component_id != main_component {
                continue;
            }
            let weight =
                (-particle.position.distance_squared(event.position) / (2.0 * sigma_squared)).exp();
            let local_delta = -normal * peak * weight;
            particle.velocity += local_delta - mean_delta;
        }
    }

    fn spawn_pinch_spray(&mut self, event: PinchEvent) {
        let count = self.tuning.pinch_spray_count.clamp(3, 4);
        let Some(slots) = self.reserve_spray_slots(count) else {
            return;
        };
        let cone = self.tuning.pinch_spray_cone;
        let normal = normalized_or(event.normal, Vec2::Y);
        let base_angle = normal.y.atan2(normal.x);
        let sequence = self.pinch_sequence;
        // A minimum separation speed plus delayed visibility prevents the newborn
        // kernels from reading as a row of beads at the former neck location.
        let base_speed = event.velocity.length().max(0.150);
        for (spray_index, slot) in slots.iter().copied().enumerate().take(count) {
            let fraction = if count <= 1 {
                0.5
            } else {
                spray_index as f32 / (count - 1) as f32
            };
            let jitter =
                (deterministic_unit(self.seed, sequence.wrapping_add(spray_index as u64), 0x5A7)
                    - 0.5)
                    * cone
                    * 0.12;
            let angle = base_angle + lerp(-cone, cone, fraction) + jitter;
            let direction = Vec2::from_angle(angle);
            let size_roll =
                deterministic_unit(self.seed, sequence.wrapping_add(spray_index as u64), 0x5C9);
            let radius = event.radius * lerp(0.08, 0.24, size_roll) * self.tuning.pinch_spray_size;
            let random_speed = lerp(
                0.30,
                0.90,
                deterministic_unit(self.seed, sequence.wrapping_add(spray_index as u64), 0x5E1),
            );
            let speed_factor = lerp(
                0.60,
                random_speed,
                self.tuning.pinch_spray_speed_variance.min(1.0),
            );
            let lifetime = lerp(
                0.60,
                1.40,
                deterministic_unit(self.seed, sequence.wrapping_add(spray_index as u64), 0x60D),
            );
            self.idle_fragments[slot] = IdleFragment {
                position: event.position + direction * (event.radius * 0.10 + radius * 0.35),
                velocity: direction * base_speed * speed_factor + event.velocity * 0.06,
                age: 0.0,
                lifetime: lifetime * 0.55,
                radius,
                emission: event.emission,
                pigment: event.pigment,
                material_coordinate: event.material_coordinate,
                optical_thickness: event.optical_thickness,
                active: true,
                lifecycle: IdleFragmentLifecycle::Spray,
                source_index: 0,
                direction,
                anchor_position: event.position,
                detached_lifetime: lifetime,
                render_connected: false,
                seen_render_connection: false,
                previous_saddle_density: 0.0,
            };
        }
    }

    fn reserve_spray_slots(&self, count: usize) -> Option<[usize; 4]> {
        let count = count.clamp(3, 4);
        let mut slots = [usize::MAX; 4];
        let mut selected = [false; MAX_IDLE_FRAGMENTS];
        let mut reserved = 0;

        for (index, fragment) in self.idle_fragments.iter().enumerate() {
            if !fragment.active && reserved < count {
                slots[reserved] = index;
                selected[index] = true;
                reserved += 1;
            }
        }
        for lifecycle in [IdleFragmentLifecycle::Fading, IdleFragmentLifecycle::Spray] {
            while reserved < count {
                let candidate = self
                    .idle_fragments
                    .iter()
                    .enumerate()
                    .filter(|(index, fragment)| {
                        !selected[*index] && fragment.active && fragment.lifecycle == lifecycle
                    })
                    .max_by(|(_, a), (_, b)| {
                        (a.age / a.lifetime.max(1.0e-5))
                            .total_cmp(&(b.age / b.lifetime.max(1.0e-5)))
                    })
                    .map(|(index, _)| index);
                let Some(index) = candidate else {
                    break;
                };
                slots[reserved] = index;
                selected[index] = true;
                reserved += 1;
            }
        }
        (reserved == count).then_some(slots)
    }

    fn update_idle_fragments_legacy(&mut self, dt: f32) {
        let containment = self.local_containment_bounds;
        let body_origin = self.body_origin;
        for fragment in &mut self.idle_fragments {
            if !fragment.active {
                continue;
            }
            fragment.age += dt;
            if fragment.age >= fragment.lifetime {
                *fragment = IdleFragment::default();
                continue;
            }
            fragment.position += fragment.velocity * dt;
            fragment.velocity *= (-0.24 * dt).exp();
            if let Some((minimum, maximum)) = containment {
                let minimum = body_origin + minimum + Vec2::splat(fragment.radius);
                let maximum = body_origin + maximum - Vec2::splat(fragment.radius);
                for axis in 0..2 {
                    if fragment.position[axis] < minimum[axis] {
                        fragment.position[axis] = minimum[axis];
                        fragment.velocity[axis] = fragment.velocity[axis].abs() * 0.18;
                    } else if fragment.position[axis] > maximum[axis] {
                        fragment.position[axis] = maximum[axis];
                        fragment.velocity[axis] = -fragment.velocity[axis].abs() * 0.18;
                    }
                }
            }
        }

        if self.tuning.idle_fragment_size <= 0.001 {
            return;
        }
        if self.material_grab.is_active() {
            self.idle_fragment_timer = self.idle_fragment_timer.max(0.75);
            return;
        }
        self.idle_fragment_timer -= dt;
        if self.idle_fragment_timer > 0.0 {
            return;
        }

        let event_sequence = self.idle_fragment_sequence;
        let interval_draw = deterministic_unit(self.seed, event_sequence, 0xA3).clamp(0.015, 0.985);
        // A bounded exponential interval avoids a mechanical metronome while
        // keeping the longest silent period predictable for a companion UI.
        let poisson_interval = (-interval_draw.ln()).clamp(0.42, 2.15);
        self.idle_fragment_timer = self.tuning.idle_fragment_interval * poisson_interval;
        let burst_roll = deterministic_unit(self.seed, event_sequence, 0xB5);
        let burst_count = if burst_roll < 0.10 {
            3
        } else if burst_roll < 0.38 {
            2
        } else {
            1
        };

        for burst_index in 0..burst_count {
            let Some(slot) = self
                .idle_fragments
                .iter()
                .position(|fragment| !fragment.active)
            else {
                break;
            };
            let sequence = self.idle_fragment_sequence;
            self.idle_fragment_sequence = self.idle_fragment_sequence.wrapping_add(1);
            let angle = (deterministic_unit(self.seed, sequence, 0xC7)
                + burst_index as f32 * 0.071)
                .fract()
                * TAU;
            let preferred_direction = Vec2::from_angle(angle);
            let Some(source) = self.particles[..self.particle_count]
                .iter()
                .filter(|particle| particle.component_id == self.components.main_component)
                .max_by(|a, b| {
                    let score_a = (a.position - self.components.main_com).dot(preferred_direction)
                        + a.surface_score * 0.025;
                    let score_b = (b.position - self.components.main_com).dot(preferred_direction)
                        + b.surface_score * 0.025;
                    score_a.total_cmp(&score_b)
                })
                .copied()
            else {
                break;
            };
            let outward = normalized_or(
                source.position - self.components.main_com,
                preferred_direction,
            );
            let tangent = Vec2::new(-outward.y, outward.x);
            let tangent_sign = deterministic_unit(self.seed, sequence, 0xE9) * 2.0 - 1.0;
            let size_roll = deterministic_unit(self.seed, sequence, 0x117);
            let size_jitter = deterministic_unit(self.seed, sequence, 0x12B);
            let radius_scale = if size_roll < 0.62 {
                0.44 + size_jitter * 0.31
            } else if size_roll < 0.92 {
                0.84 + size_jitter * 0.34
            } else {
                1.36 + size_jitter * 0.58
            };
            let radius = self.tuning.idle_fragment_size * radius_scale;
            let speed = (0.032 + deterministic_unit(self.seed, sequence, 0xF1) * 0.040)
                * (1.15 - radius_scale.min(1.8) * 0.18);
            let lifetime_jitter = 0.78 + deterministic_unit(self.seed, sequence, 0x139) * 0.48;
            let kernel_radius = KERNEL_RADIUS * self.tuning.kernel_radius_scale;
            self.idle_fragments[slot] = IdleFragment {
                position: source.position + outward * (kernel_radius * 0.84 + radius * 0.55),
                velocity: outward * speed + tangent * tangent_sign * (0.008 + speed * 0.11),
                age: 0.0,
                lifetime: self.tuning.idle_fragment_lifetime
                    * lifetime_jitter
                    * (0.92 + radius_scale.min(1.8) * 0.12),
                radius,
                emission: (source.emission * 0.72 + 0.12).clamp(0.0, 1.0),
                pigment: source.pigment,
                material_coordinate: source.rest_position,
                optical_thickness: 0.82 + (1.0 - source.surface_score) * 0.38,
                active: true,
                ..IdleFragment::default()
            };
        }
    }

    fn apply_local_containment_forces(&mut self, kernel_radius: f32) {
        let Some((local_minimum, local_maximum)) = self.local_containment_bounds else {
            return;
        };
        let guard = kernel_radius * 0.48;
        let soft_zone = (kernel_radius * 0.92).max(0.04);
        let minimum = self.body_origin + local_minimum + Vec2::splat(guard);
        let maximum = self.body_origin + local_maximum - Vec2::splat(guard);
        for particle in &mut self.particles[..self.particle_count] {
            for axis in 0..2 {
                let lower_distance = particle.position[axis] - minimum[axis];
                if lower_distance < soft_zone {
                    let weight = ((soft_zone - lower_distance) / soft_zone).clamp(0.0, 2.0);
                    particle.force[axis] +=
                        weight * weight * 15.0 - particle.velocity[axis].min(0.0) * weight * 4.2;
                }
                let upper_distance = maximum[axis] - particle.position[axis];
                if upper_distance < soft_zone {
                    let weight = ((soft_zone - upper_distance) / soft_zone).clamp(0.0, 2.0);
                    particle.force[axis] -=
                        weight * weight * 15.0 + particle.velocity[axis].max(0.0) * weight * 4.2;
                }
            }
        }
    }

    fn solve_local_containment(&mut self, kernel_radius: f32) {
        let Some((local_minimum, local_maximum)) = self.local_containment_bounds else {
            return;
        };
        let guard = kernel_radius * 0.48;
        let minimum = self.body_origin + local_minimum + Vec2::splat(guard);
        let maximum = self.body_origin + local_maximum - Vec2::splat(guard);
        for particle in &mut self.particles[..self.particle_count] {
            particle.predicted_position = particle.predicted_position.clamp(minimum, maximum);
        }
    }

    fn stabilize_main_orientation(&mut self, dt: f32, target_angle: f32) {
        let strength = self.tuning.upright_stabilization;
        if strength <= 0.0 {
            return;
        }
        let orientation = self.main_component_orientation();
        let error = wrap_angle(target_angle - orientation);
        let correction = (error * (1.0 - (-strength * dt).exp())).clamp(-0.08, 0.08);
        let rotation = Vec2::from_angle(correction);
        let center = self.components.main_com;
        let mut mean_velocity = Vec2::ZERO;
        let mut count = 0.0_f32;
        for particle in &self.particles[..self.particle_count] {
            if particle.component_id == self.components.main_component {
                mean_velocity += particle.velocity;
                count += 1.0;
            }
        }
        mean_velocity /= count.max(1.0);
        for particle in &mut self.particles[..self.particle_count] {
            if particle.component_id != self.components.main_component {
                continue;
            }
            particle.position = center + rotate_vector(particle.position - center, rotation);
            particle.previous_position =
                center + rotate_vector(particle.previous_position - center, rotation);
            particle.predicted_position =
                center + rotate_vector(particle.predicted_position - center, rotation);
            particle.velocity =
                mean_velocity + rotate_vector(particle.velocity - mean_velocity, rotation);
        }

        // Project only the least-squares rigid spin out of velocity. Residual local
        // vorticity, slosh and shear survive, so this is an orientation gauge rather
        // than a freeze or a screen-space pin.
        let angular_velocity = self.main_component_angular_velocity();
        let spin_decay = 1.0 - (-(strength + self.tuning.angular_damping * 24.0) * dt).exp();
        for particle in &mut self.particles[..self.particle_count] {
            if particle.component_id != self.components.main_component {
                continue;
            }
            let arm = particle.position - center;
            particle.velocity -= Vec2::new(-arm.y, arm.x) * angular_velocity * spin_decay;
        }
    }

    fn update_cinematic_lean(
        &mut self,
        motion_tilt: f32,
        external_drive: f32,
        pointer_released: bool,
        dt: f32,
    ) -> f32 {
        let directly_driven = self.material_grab.is_active() || external_drive > 0.22;
        if pointer_released {
            self.lean_phase = LeanPhase::RecoverUpright;
            self.lean_phase_elapsed = 0.0;
            self.lean_target = self.main_component_orientation().clamp(-0.18, 0.18);
        } else if self.lean_phase == LeanPhase::RecoverUpright {
            // Release slosh must not immediately re-enter MotionLean. Only a new
            // strong drive interrupts the explicit return-to-upright contract.
            if self.material_grab.is_active() || external_drive > 0.72 {
                self.lean_phase = LeanPhase::Motion;
            }
        } else if directly_driven {
            self.lean_phase = LeanPhase::Motion;
        } else if self.lean_phase == LeanPhase::Motion {
            self.lean_phase = LeanPhase::RecoverUpright;
            self.lean_phase_elapsed = 0.0;
            self.lean_target = self.main_component_orientation().clamp(-0.18, 0.18);
        }

        match self.lean_phase {
            LeanPhase::Motion => {
                self.lean_target = motion_tilt.clamp(-0.18, 0.18);
            }
            LeanPhase::RecoverUpright => {
                let decay = (-std::f32::consts::LN_2 * dt
                    / self.tuning.lean_return_half_life.max(0.12))
                .exp();
                self.lean_target *= decay;
                self.lean_phase_elapsed += dt;
                if self.lean_target.abs() <= 0.008_726_646
                    && self.main_component_orientation().abs() <= 0.012
                {
                    self.lean_target = 0.0;
                    self.lean_phase = LeanPhase::UprightHold;
                    self.lean_phase_elapsed = 0.0;
                    self.lean_phase_duration = self.tuning.upright_hold
                        * lerp(
                            0.78,
                            1.28,
                            deterministic_unit(self.seed, self.lean_sequence, 0x1EAD),
                        );
                }
            }
            LeanPhase::UprightHold => {
                self.lean_target = 0.0;
                self.lean_phase_elapsed += dt;
                if self.lean_phase_elapsed >= self.lean_phase_duration
                    && self.tuning.idle_lean_angle > 1.0e-6
                {
                    let signed =
                        deterministic_unit(self.seed, self.lean_sequence, 0x1EA1) * 2.0 - 1.0;
                    let magnitude = lerp(
                        0.32,
                        1.0,
                        deterministic_unit(self.seed, self.lean_sequence, 0x1EB3),
                    );
                    self.idle_lean_goal = signed * magnitude * self.tuning.idle_lean_angle;
                    self.lean_phase = LeanPhase::IdleLean;
                    self.lean_phase_elapsed = 0.0;
                    self.lean_phase_duration = lerp(
                        0.18,
                        0.32,
                        deterministic_unit(self.seed, self.lean_sequence, 0x1EC7),
                    ) / self.tuning.idle_lean_rate.max(0.03);
                    self.lean_sequence = self.lean_sequence.wrapping_add(1);
                }
            }
            LeanPhase::IdleLean => {
                self.lean_phase_elapsed += dt;
                let phase =
                    (self.lean_phase_elapsed / self.lean_phase_duration.max(0.1)).clamp(0.0, 1.0);
                let envelope = if phase < 0.20 {
                    smoothstep01(phase / 0.20)
                } else if phase > 0.74 {
                    smoothstep01((1.0 - phase) / 0.26)
                } else {
                    1.0
                };
                self.lean_target = self.idle_lean_goal * envelope;
                if phase >= 1.0 {
                    self.lean_phase = LeanPhase::RecoverUpright;
                    self.lean_phase_elapsed = 0.0;
                }
            }
        }
        self.lean_target.clamp(-0.18, 0.18)
    }

    fn main_component_orientation(&self) -> f32 {
        let mut rest_center = Vec2::ZERO;
        let mut count = 0.0_f32;
        for particle in &self.particles[..self.particle_count] {
            if particle.component_id == self.components.main_component {
                rest_center += particle.rest_position;
                count += 1.0;
            }
        }
        rest_center /= count.max(1.0);
        let mut dot = 0.0;
        let mut cross = 0.0;
        for particle in &self.particles[..self.particle_count] {
            if particle.component_id != self.components.main_component {
                continue;
            }
            let rest = particle.rest_position - rest_center;
            let current = particle.position - self.components.main_com;
            dot += rest.dot(current);
            cross += rest.perp_dot(current);
        }
        cross.atan2(dot)
    }

    fn main_component_angular_velocity(&self) -> f32 {
        let mut mean_velocity = Vec2::ZERO;
        let mut count = 0.0_f32;
        for particle in &self.particles[..self.particle_count] {
            if particle.component_id == self.components.main_component {
                mean_velocity += particle.velocity;
                count += 1.0;
            }
        }
        mean_velocity /= count.max(1.0);
        let mut momentum = 0.0;
        let mut inertia = 0.0;
        for particle in &self.particles[..self.particle_count] {
            if particle.component_id != self.components.main_component {
                continue;
            }
            let arm = particle.position - self.components.main_com;
            momentum += arm.perp_dot(particle.velocity - mean_velocity);
            inertia += arm.length_squared();
        }
        momentum / inertia.max(1.0e-5)
    }

    fn anisotropy_target(&self, index: usize) -> (Vec2, Vec2, f32) {
        let particle = self.particles[index];
        let kernel_radius = KERNEL_RADIUS * self.tuning.kernel_radius_scale;
        // Yu-Turk reconstruction needs a wider, weighted neighbourhood than the
        // physical density solve. Sparse samples are explicitly isotropic below;
        // stretching an under-resolved point is what produced the hard capsules.
        let support = kernel_radius * 2.0;
        let support_squared = support * support;
        let mut mean = Vec2::ZERO;
        let mut mean_weight = 0.0;
        let mut reliable_weight = 0.0;
        let mut reliable_weight_squared = 0.0;
        for other in &self.particles[..self.particle_count] {
            let delta_from_particle = other.position - particle.position;
            let distance_squared = delta_from_particle.length_squared();
            if distance_squared >= support_squared {
                continue;
            }
            let distance = distance_squared.sqrt();
            let weight = (1.0 - distance / support).powi(3);
            mean += other.position * weight;
            mean_weight += weight;
            if distance_squared > 1.0e-10 {
                reliable_weight += weight;
                reliable_weight_squared += weight * weight;
            }
        }
        mean = if mean_weight > 1.0e-5 {
            mean / mean_weight
        } else {
            particle.position
        };
        let mut xx = 0.0;
        let mut xy = 0.0;
        let mut yy = 0.0;
        if mean_weight > 1.0e-5 {
            for other in &self.particles[..self.particle_count] {
                let delta_from_particle = other.position - particle.position;
                let distance_squared = delta_from_particle.length_squared();
                if distance_squared >= support_squared {
                    continue;
                }
                let weight = (1.0 - distance_squared.sqrt() / support).powi(3);
                let centered = other.position - mean;
                xx += centered.x * centered.x * weight;
                xy += centered.x * centered.y * weight;
                yy += centered.y * centered.y * weight;
            }
            xx /= mean_weight;
            xy /= mean_weight;
            yy /= mean_weight;
        }
        let effective_neighbors = if reliable_weight_squared > 1.0e-8 {
            reliable_weight * reliable_weight / reliable_weight_squared
        } else {
            0.0
        };
        let trace = xx + yy;
        let discriminant = ((xx - yy).powi(2) + 4.0 * xy.powi(2)).sqrt();
        let major = ((trace + discriminant) * 0.5).max(1.0e-6);
        // Bound the covariance condition number before converting it to a splat
        // aspect ratio. A 4:1 eigenvalue ratio corresponds to at most a 2:1 axis
        // ratio and cannot bridge disconnected mass with one long ellipse.
        let minor = ((trace - discriminant) * 0.5).max(major * 0.25).max(1.0e-6);
        let axis = if discriminant > 1.0e-6 {
            let candidate = Vec2::new(major - yy, xy);
            if candidate.length_squared() > 1.0e-10 {
                candidate.normalize()
            } else if xx >= yy {
                Vec2::X
            } else {
                Vec2::Y
            }
        } else {
            particle.render_axis_major
        };
        let covariance_aspect = (major / minor).sqrt();
        let anisotropy_confidence =
            smoothstep01(((effective_neighbors - 6.0) / 4.0).clamp(0.0, 1.0));
        let authored_cap = self.tuning.anisotropy_max.min(2.15);
        let aspect = if effective_neighbors < 6.0 {
            1.0
        } else {
            (1.0 + anisotropy_confidence * particle.surface_score * (covariance_aspect - 1.0))
                .clamp(1.0, authored_cap)
        };
        let render_center = particle
            .position
            .lerp(mean, self.tuning.render_center_smoothing);
        (render_center, axis, aspect)
    }

    fn snap_render_proxies(&mut self) {
        let targets: [(Vec2, Vec2, f32); MAX_LIQUID_PARTICLES] =
            array::from_fn(|index| self.anisotropy_target(index.min(self.particle_count - 1)));
        for (particle, (position, axis, aspect)) in self.particles[..self.particle_count]
            .iter_mut()
            .zip(targets)
        {
            particle.render_position = position;
            particle.render_axis_major = axis;
            particle.render_aspect = aspect;
            particle.render_surface_score = particle.surface_score;
        }
    }

    fn update_render_proxies(&mut self, dt: f32) {
        // Reconstruction covariance is much noisier than the conserved particle
        // motion. Filter only the visual splats with a continuous-time response so
        // changing the fixed simulation frequency cannot change the apparent
        // damping. Axis sign continuity also prevents equivalent eigenvectors from
        // flipping by 180 degrees between ticks.
        let targets: [(Vec2, Vec2, f32); MAX_LIQUID_PARTICLES] =
            array::from_fn(|index| self.anisotropy_target(index.min(self.particle_count - 1)));
        if self.presentation_recovery_remaining > 0.0 {
            self.presentation_recovery_remaining =
                (self.presentation_recovery_remaining - dt).max(0.0);
            let phase = 1.0 - self.presentation_recovery_remaining / 0.25;
            let blend = smoothstep01(phase.clamp(0.0, 1.0));
            for ((particle, recovery), target) in self.particles[..self.particle_count]
                .iter_mut()
                .zip(self.presentation_recovery_from[..self.particle_count].iter())
                .zip(targets[..self.particle_count].iter().copied())
            {
                let (from_position, from_axis, from_aspect, from_surface) = *recovery;
                let (target_position, mut target_axis, target_aspect) = target;
                let from_axis = normalized_or(from_axis, Vec2::X);
                if from_axis.dot(target_axis) < 0.0 {
                    target_axis = -target_axis;
                }
                particle.render_position = from_position.lerp(target_position, blend);
                particle.render_axis_major =
                    normalized_or(from_axis.lerp(target_axis, blend), from_axis);
                particle.render_aspect = lerp(from_aspect, target_aspect, blend)
                    .clamp(1.0, self.tuning.anisotropy_max.min(2.15));
                particle.render_surface_score =
                    lerp(from_surface, particle.surface_score, blend).clamp(0.0, 1.0);
            }
            return;
        }
        let rate = self.tuning.render_response_hz;
        let position_blend = 1.0 - (-rate * dt).exp();
        let shape_blend = 1.0 - (-rate * 0.72 * dt).exp();
        for (particle, (target_position, mut target_axis, target_aspect)) in self.particles
            [..self.particle_count]
            .iter_mut()
            .zip(targets)
        {
            if particle.render_axis_major.dot(target_axis) < 0.0 {
                target_axis = -target_axis;
            }
            particle.render_position = particle
                .render_position
                .lerp(target_position, position_blend);
            let current_axis = if particle.render_axis_major.length_squared() > 0.5 {
                particle.render_axis_major.normalize()
            } else {
                Vec2::X
            };
            let angular_error = current_axis
                .perp_dot(target_axis)
                .atan2(current_axis.dot(target_axis));
            let angular_step =
                angular_error.clamp(-240.0_f32.to_radians() * dt, 240.0_f32.to_radians() * dt);
            particle.render_axis_major = Vec2::from_angle(angular_step).rotate(current_axis);
            particle.render_aspect = if target_aspect <= 1.000_1 {
                1.0
            } else {
                let desired_step = (target_aspect - particle.render_aspect) * shape_blend;
                (particle.render_aspect + desired_step.clamp(-2.0 * dt, 2.0 * dt))
                    .clamp(1.0, self.tuning.anisotropy_max.min(2.15))
            };
            particle.render_surface_score = (particle.render_surface_score
                + (particle.surface_score - particle.render_surface_score) * shape_blend)
                .clamp(0.0, 1.0);
        }
    }

    fn update_diagnostics(
        &mut self,
        parameters: MaterialParameters,
        stress: MaterialStressFrame,
        speed: f32,
    ) {
        let maximum_speed = self.particles[..self.particle_count]
            .iter()
            .map(|particle| particle.velocity.length())
            .fold(0.0_f32, f32::max);
        let kinetic_energy = self.particles[..self.particle_count]
            .iter()
            .map(|particle| {
                let mass = particle.inverse_mass.max(1.0e-5).recip();
                0.5 * mass * particle.velocity.length_squared()
            })
            .sum();
        let maximum_compression = self.particles[..self.particle_count]
            .iter()
            .map(|particle| (particle.density / self.rest_density.max(0.01) - 1.0).max(0.0))
            .fold(0.0_f32, f32::max);
        let maximum_bond_strain = self
            .bonds
            .iter()
            .filter(|bond| bond.active)
            .map(|bond| bond.strain.max(0.0))
            .fold(0.0_f32, f32::max);
        let density = 1.0;
        let body_size = 0.39;
        let visual_weber =
            density * speed * speed * body_size / parameters.surface_tension.max(0.01);
        let visual_ohnesorge = parameters.viscosity
            / (density * parameters.surface_tension * body_size)
                .sqrt()
                .max(0.01);
        let visual_deborah = self.tuning.bond_relaxation_time / (0.39 / speed.max(0.05));
        let budding_count = self
            .idle_fragments
            .iter()
            .filter(|fragment| {
                fragment.active && fragment.lifecycle == IdleFragmentLifecycle::Budding
            })
            .count();
        let bubble_count = self
            .idle_fragments
            .iter()
            .filter(|fragment| fragment.active)
            .count();
        let finite = self.particles[..self.particle_count]
            .iter()
            .all(|particle| particle.is_finite())
            && self.rest_density.is_finite()
            && self.body_origin.is_finite()
            && stress.center.is_finite()
            && self.idle_fragments.iter().all(|fragment| {
                !fragment.active
                    || (fragment.position.is_finite()
                        && fragment.velocity.is_finite()
                        && fragment.anchor_position.is_finite()
                        && fragment.direction.is_finite()
                        && [
                            fragment.age,
                            fragment.lifetime,
                            fragment.detached_lifetime,
                            fragment.radius,
                            fragment.emission,
                            fragment.pigment,
                        ]
                        .into_iter()
                        .all(f32::is_finite))
            })
            && [
                stress.magnitude,
                stress.cursor_distance,
                stress.com_follow_ratio,
            ]
            .into_iter()
            .all(f32::is_finite);
        self.diagnostics = LiquidDiagnostics {
            particle_count: self.particle_count,
            component_count: self.components.component_count,
            main_mass: self.components.main_mass,
            detached_mass: self.components.detached_mass,
            density_error: mean_density_error(
                &self.particles,
                self.particle_count,
                self.rest_density,
            ),
            maximum_speed,
            maximum_bond_strain,
            rigid_angular_velocity: self.main_component_angular_velocity(),
            visual_weber,
            visual_ohnesorge,
            visual_deborah,
            face_confidence: self.face_frame.frame.confidence,
            com_follow_ratio: stress.com_follow_ratio,
            stretch_ratio: self.material_stretch_ratio(),
            stress_magnitude: stress.magnitude,
            cursor_distance: stress.cursor_distance,
            orientation: self.main_component_orientation(),
            lean_target: self.lean_target,
            budding_count,
            bubble_count,
            kinetic_energy,
            maximum_compression,
            failsafe_hits: self.failsafe_hits,
            recovery_count: self.recovery_count,
            finite,
        };
    }
}

#[allow(dead_code)]
fn rotate_vector(vector: Vec2, rotation: Vec2) -> Vec2 {
    Vec2::new(
        vector.x * rotation.x - vector.y * rotation.y,
        vector.x * rotation.y + vector.y * rotation.x,
    )
}

fn normalized_or(vector: Vec2, fallback: Vec2) -> Vec2 {
    if vector.length_squared() > 1.0e-8 {
        vector.normalize()
    } else {
        fallback
    }
}

fn smoothstep01(value: f32) -> f32 {
    value * value * (3.0 - 2.0 * value)
}

fn lerp(start: f32, end: f32, amount: f32) -> f32 {
    start + (end - start) * amount
}

#[allow(dead_code)]
fn bud_excursion(kernel_radius: f32, bubble_radius: f32, progress: f32, neck_width: f32) -> f32 {
    let unscaled = kernel_radius * 0.52
        + bubble_radius * lerp(0.22, 2.35, smoothstep01(progress.clamp(0.0, 1.0)));
    // Wider requested necks keep the bud deeper in the parent iso-field; thinner
    // necks let it travel farther before the connectivity oracle pinches it off.
    let distance_scale = (1.0 - (neck_width.clamp(0.0, 2.5) - 1.0) * 0.16).clamp(0.76, 1.16);
    unscaled * distance_scale
}

#[allow(dead_code)]
fn compact_density(distance_squared: f32, support_radius: f32, density: f32) -> f32 {
    let normalized = distance_squared / support_radius.max(1.0e-5).powi(2);
    if normalized >= 1.0 {
        0.0
    } else {
        (1.0 - normalized).powi(3) * density
    }
}

fn component_graph_spacing(
    _legacy_spacing: f32,
    kernel_radius: f32,
    iso_threshold: f32,
    _cinematic: bool,
) -> f32 {
    // Two equal compact kernels remain one visible liquid component while their
    // midpoint density is above the same iso-level used by the surface shader.
    // Express that center distance through the existing scale parameter so its
    // default 1.30 is exactly render-topology aligned instead of classifying a
    // split while the screen still shows one connected blob.
    let half_density = (iso_threshold.clamp(0.08, 0.82) * 0.5).cbrt();
    let visible_link = 2.0 * kernel_radius.max(1.0e-5) * (1.0 - half_density).max(0.0).sqrt();
    visible_link / 1.30
}

/// Samples the same anisotropic compact field that `liquid_density.wgsl` presents.
/// A bud detaches only after this visible saddle crosses the material iso-level;
/// physical particles and smoothed render proxies intentionally cannot disagree.
/// There are deliberately no bridge beads: the neck is the actual body/bud overlap.
#[allow(dead_code)]
fn rendered_bud_connectivity(
    particles: &[LiquidParticle],
    fragments: &[IdleFragment],
    fragment_index: usize,
    fragment: IdleFragment,
    base_kernel_radius: f32,
    iso_threshold: f32,
) -> (bool, Vec2, f32) {
    let bubble_radius = fragment.radius * fragment.render_scale();
    let source_index = fragment.source_index.min(particles.len().saturating_sub(1));
    let render_anchor = particles
        .get(source_index)
        .map_or(fragment.anchor_position, |particle| {
            particle.render_position
        });
    let span = fragment.position - render_anchor;
    if bubble_radius <= 1.0e-5 || span.length_squared() <= 1.0e-10 {
        return (true, render_anchor, f32::INFINITY);
    }

    let speed = fragment.velocity.length();
    let bubble_aspect = 1.0 + (speed / 0.10).clamp(0.0, 1.0) * 0.12;
    let bubble_area_scale = bubble_aspect.sqrt();
    let bubble_axis = if speed > 1.0e-5 {
        fragment.velocity / speed
    } else {
        Vec2::X
    };
    let bubble_major = bubble_radius * bubble_area_scale;
    let bubble_minor = bubble_radius / bubble_area_scale;

    let mut minimum_density = f32::INFINITY;
    let mut pinch_position = render_anchor + span * 0.5;
    let transverse = Vec2::new(-span.y, span.x).normalize_or_zero();
    let transverse_extent = base_kernel_radius.max(bubble_minor) * 0.72;
    for sample in 1..24 {
        let t = sample as f32 / 24.0;
        let centerline = render_anchor + span * t;
        let mut cross_section_density = 0.0_f32;
        let mut cross_section_position = centerline;
        for transverse_sample in 0..5 {
            let offset = (transverse_sample as f32 * 0.5 - 1.0) * transverse_extent;
            let point = centerline + transverse * offset;
            let mut density = compact_ellipsoid_density(
                point,
                fragment.position,
                bubble_axis,
                bubble_major,
                bubble_minor,
                0.92,
            );
            for particle in particles {
                let aspect = particle.render_aspect.max(1.0);
                let area_scale = aspect.sqrt();
                density += compact_ellipsoid_density(
                    point,
                    particle.render_position,
                    particle.render_axis_major,
                    base_kernel_radius * area_scale,
                    base_kernel_radius / area_scale,
                    1.0,
                );
            }
            for (other_index, other) in fragments.iter().enumerate() {
                if other_index == fragment_index || !other.active {
                    continue;
                }
                let scale = other.render_scale();
                if scale <= 0.001 {
                    continue;
                }
                let radius = other.radius * scale;
                let other_speed = other.velocity.length();
                let other_aspect = 1.0 + (other_speed / 0.10).clamp(0.0, 1.0) * 0.12;
                let other_area_scale = other_aspect.sqrt();
                let other_axis = if other_speed > 1.0e-5 {
                    other.velocity / other_speed
                } else {
                    Vec2::X
                };
                density += compact_ellipsoid_density(
                    point,
                    other.position,
                    other_axis,
                    radius * other_area_scale,
                    radius / other_area_scale,
                    0.92,
                );
            }
            if density > cross_section_density {
                cross_section_density = density;
                cross_section_position = point;
            }
        }
        if cross_section_density < minimum_density {
            minimum_density = cross_section_density;
            pinch_position = cross_section_position;
        }
    }
    (
        minimum_density >= iso_threshold,
        pinch_position,
        minimum_density,
    )
}

#[allow(dead_code)]
fn compact_ellipsoid_density(
    point: Vec2,
    center: Vec2,
    axis: Vec2,
    major_radius: f32,
    minor_radius: f32,
    density: f32,
) -> f32 {
    let axis = normalized_or(axis, Vec2::X);
    let perpendicular = Vec2::new(-axis.y, axis.x);
    let delta = point - center;
    let coordinate = Vec2::new(
        delta.dot(axis) / major_radius.max(1.0e-5),
        delta.dot(perpendicular) / minor_radius.max(1.0e-5),
    );
    compact_density(coordinate.length_squared(), 1.0, density)
}

#[allow(dead_code)]
fn wrap_angle(angle: f32) -> f32 {
    (angle + std::f32::consts::PI).rem_euclid(TAU) - std::f32::consts::PI
}

fn deterministic_unit(seed: u64, sequence: u64, salt: u64) -> f32 {
    let mut value = seed
        ^ sequence.wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ salt.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value ^= value >> 30;
    value = value.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value ^= value >> 27;
    value = value.wrapping_mul(0x94D0_49BB_1331_11EB);
    value ^= value >> 31;
    ((value >> 40) as u32) as f32 / 0x00FF_FFFF as f32
}

#[cfg(test)]
mod flight_field_tests {
    use super::*;

    fn motion(velocity: Vec2, acceleration: Vec2) -> DropletMotion {
        DropletMotion {
            velocity,
            acceleration,
            ..DropletMotion::default()
        }
    }

    #[test]
    fn flight_field_flattens_continuously_and_relaxes_without_axis_flip() {
        let mut runtime = LiquidMorphRuntime::new(0x00F1_1E1D);
        let dt = 1.0 / 120.0;
        for _ in 0..240 {
            runtime.update_flight_field_shape(motion(Vec2::new(1.4, 0.0), Vec2::ZERO), dt);
        }

        assert!(runtime.flight_field_aspect > 1.24);
        assert!(runtime.flight_field_aspect <= 1.34);
        assert!(runtime.flight_field_axis.dot(Vec2::X).abs() < 1.0e-5);
        let area_scale = runtime.flight_field_aspect.sqrt();
        assert!((area_scale * area_scale.recip() - 1.0).abs() < 1.0e-6);

        let axis_before_reversal = runtime.flight_field_axis;
        runtime.update_flight_field_shape(motion(Vec2::new(-1.4, 0.0), Vec2::ZERO), dt);
        assert!(
            runtime.flight_field_axis.dot(axis_before_reversal) > 0.999_99,
            "an unoriented field must not flip on velocity reversal"
        );

        for _ in 0..720 {
            runtime.update_flight_field_shape(DropletMotion::default(), dt);
        }
        assert!((runtime.flight_field_aspect - 1.0).abs() < 1.0e-5);
        assert!(runtime.flight_field_axis.is_finite());
    }

    #[test]
    fn flight_field_replay_is_render_rate_independent() {
        fn replay(hz: usize) -> (Vec2, f32) {
            let mut runtime = LiquidMorphRuntime::new(0xA2EA_5AFE);
            let dt = 1.0 / hz as f32;
            for _ in 0..hz * 2 {
                runtime.update_flight_field_shape(
                    motion(Vec2::new(0.82, -0.64), Vec2::new(0.24, -0.18)),
                    dt,
                );
            }
            (runtime.flight_field_axis, runtime.flight_field_aspect)
        }

        let at_30 = replay(30);
        let at_60 = replay(60);
        let at_144 = replay(144);
        assert!(at_30.0.distance(at_60.0) < 1.0e-5);
        assert!(at_60.0.distance(at_144.0) < 1.0e-5);
        assert!((at_30.1 - at_60.1).abs() < 1.0e-5);
        assert!((at_60.1 - at_144.1).abs() < 1.0e-5);
    }

    #[test]
    fn zero_authored_flight_stretch_keeps_the_neutral_field() {
        let mut runtime = LiquidMorphRuntime::new(0x0000_00FF);
        runtime.tuning.flight_stretch = 0.0;
        for _ in 0..240 {
            runtime.update_flight_field_shape(
                motion(Vec2::new(3.0, 1.0), Vec2::new(2.0, -1.0)),
                1.0 / 120.0,
            );
        }
        assert_eq!(runtime.flight_field_aspect, 1.0);
    }

    #[test]
    fn structural_tuning_is_deferred_while_mass_is_detached() {
        let mut runtime = LiquidMorphRuntime::new(0xD3FE_22ED);
        let detached_start = runtime.particle_count - 4;
        for particle in &mut runtime.particles[detached_start..runtime.particle_count] {
            particle.component_id = 1;
            particle.position += Vec2::new(0.8, 0.0);
        }
        runtime.component_lifecycle.update(
            &runtime.particles,
            runtime.particle_count,
            ComponentSummary {
                component_count: 2,
                main_component: 0,
                main_mass: (runtime.particle_count - 4) as f32,
                detached_mass: 4.0,
                main_com: Vec2::ZERO,
            },
            true,
            0.8,
            None,
            runtime.interaction_tuning,
            1.0 / 120.0,
        );
        assert!(!runtime.component_lifecycle.is_stable());
        let original_particle_count = runtime.tuning.particle_count;
        let mut requested = runtime.tuning;
        requested.particle_count = if original_particle_count == 48 {
            64
        } else {
            48
        };

        runtime.set_tuning(
            requested,
            runtime.interaction_tuning,
            FaceTuning::default(),
            MaterialVariant::CinematicJelly,
        );

        assert_eq!(runtime.tuning.particle_count, original_particle_count);
        assert_eq!(
            runtime
                .pending_structural_tuning
                .expect("structural change remains queued")
                .tuning
                .particle_count,
            requested.particle_count
        );
    }
}

#[derive(Debug, Clone, Copy)]
struct MaterialParameters {
    numerical_xsph: f32,
    viscosity: f32,
    surface_tension: f32,
    character_field_strength: f32,
}

impl MaterialParameters {
    fn from_tuning(tuning: PbfTuning) -> Self {
        Self {
            numerical_xsph: tuning.numerical_xsph,
            viscosity: tuning.viscosity,
            surface_tension: tuning.surface_tension,
            character_field_strength: tuning.return_strength,
        }
    }
}

#[cfg(any())]
mod tests {
    use lifecore::{BodyIntent, Genome, LocomotionMode, PoseIntent, SensorFrame};

    use super::components::classify_render_components;
    use super::*;
    use crate::{DerivedVisualTraits, VisualPhysiologyRuntime};

    fn neutral_intent() -> BodyIntent {
        BodyIntent {
            locomotion: LocomotionMode::Hover,
            target_position: Vec2::splat(0.5),
            target_surface: None,
            desired_speed: 0.0,
            facing_direction: 1.0,
            gaze_target: None,
            pose: PoseIntent::Neutral,
            expression: lifecore::ExpressionState::default(),
            interaction_target: None,
        }
    }

    fn run(runtime: &mut LiquidMorphRuntime, genome: &lifecore::Genome, seconds: f32) {
        let traits = DerivedVisualTraits::from_genome(genome);
        let physiology = VisualPhysiologyRuntime::new(genome.identity_seed, &traits);
        for _ in 0..(seconds * 120.0) as usize {
            runtime.update(
                &genome.body,
                &traits,
                physiology.pose,
                VisualMindInput::default(),
                &neutral_intent(),
                &SensorFrame::default(),
                &BodyFeedback::default(),
                DropletMotion::default(),
                ModalDeformation::default(),
                0.5,
                1.0 / 120.0,
            );
        }
    }

    fn horizontal_extent(runtime: &LiquidMorphRuntime) -> f32 {
        extent_along(runtime, Vec2::X)
    }

    fn extent_along(runtime: &LiquidMorphRuntime, axis: Vec2) -> f32 {
        let axis = axis.normalize_or_zero();
        let minimum = runtime.particles[..runtime.particle_count]
            .iter()
            .map(|particle| particle.position.dot(axis))
            .fold(f32::INFINITY, f32::min);
        let maximum = runtime.particles[..runtime.particle_count]
            .iter()
            .map(|particle| particle.position.dot(axis))
            .fold(f32::NEG_INFINITY, f32::max);
        maximum - minimum
    }

    #[test]
    fn calm_blob_is_one_deterministic_finite_component() {
        let genome = Genome::from_seed(42);
        let mut first = LiquidMorphRuntime::new(genome.identity_seed);
        let mut second = first.clone();
        run(&mut first, &genome, 8.0);
        run(&mut second, &genome, 8.0);
        assert_eq!(first, second);
        assert!(first.diagnostics.finite);
        assert_eq!(first.diagnostics.component_count, 1);
        assert_eq!(first.diagnostics.detached_mass, 0.0);
        assert!(horizontal_extent(&first) > 0.40);
        assert!(
            first.render_state().face_frame.origin.y > first.components.main_com.y + 0.05,
            "face frame left the upper material patch: {:?}; COM={:?}; max_r={}",
            first.render_state().face_frame,
            first.components.main_com,
            first.particles[..first.particle_count]
                .iter()
                .map(|particle| particle.position.length())
                .fold(0.0_f32, f32::max)
        );
    }

    #[test]
    fn idle_fragments_are_deterministic_sparse_and_shrink_away() {
        let mut first = LiquidMorphRuntime::new(73);
        let mut replay = first.clone();
        first.idle_fragment_timer = 0.0;
        replay.idle_fragment_timer = 0.0;
        first.update_idle_fragments_legacy(1.0 / 120.0);
        replay.update_idle_fragments_legacy(1.0 / 120.0);
        assert_eq!(first.idle_fragments, replay.idle_fragments);

        let slot = first
            .idle_fragments
            .iter()
            .position(|fragment| fragment.active)
            .expect("idle emitter did not create a fragment");
        let active_count = first
            .idle_fragments
            .iter()
            .filter(|fragment| fragment.active)
            .count();
        assert!((1..=3).contains(&active_count));
        let maximum_lifetime = first
            .idle_fragments
            .iter()
            .filter(|fragment| fragment.active)
            .map(|fragment| fragment.lifetime)
            .fold(0.0_f32, f32::max);
        let lifetime = first.idle_fragments[slot].lifetime;
        first.idle_fragment_timer = 100.0;
        first.update_idle_fragments_legacy(lifetime * 0.14);
        let young_scale = first.idle_fragments[slot].render_scale();
        let young_render = first.render_state();
        assert_eq!(young_render.particle_count, first.particle_count);
        assert!((1..=active_count).contains(&young_render.bubble_count));
        first.update_idle_fragments_legacy(lifetime * 0.62);
        let old_scale = first.idle_fragments[slot].render_scale();
        assert!(old_scale < young_scale * 0.55);
        first.update_idle_fragments_legacy(lifetime * 0.30);
        assert!(!first.idle_fragments[slot].active);
        first.update_idle_fragments_legacy(maximum_lifetime + 0.1);
        let expired_render = first.render_state();
        assert_eq!(expired_render.particle_count, first.particle_count);
        assert_eq!(expired_render.bubble_count, 0);
    }

    #[test]
    fn cinematic_buds_are_bounded_zero_sum_and_use_only_the_shared_field() {
        let mut runtime = LiquidMorphRuntime::new(151);
        runtime.cinematic_features = true;
        runtime.idle_fragment_timer = 0.0;
        runtime.advance_cinematic_fragments(1.0 / 120.0, Vec2::ZERO);
        runtime.update_render_proxies(1.0 / 120.0);
        runtime.resolve_cinematic_pinches();
        assert!(
            (0.5..=1.5).contains(&runtime.idle_fragment_timer),
            "default bud interval escaped its deterministic bounds: {}",
            runtime.idle_fragment_timer
        );
        let slot = runtime
            .idle_fragments
            .iter()
            .position(|fragment| {
                fragment.active && fragment.lifecycle == IdleFragmentLifecycle::Budding
            })
            .expect("cinematic emitter did not create a bud");
        let lifetime = runtime.idle_fragments[slot].lifetime;
        runtime.idle_fragments[slot].age = lifetime * 0.50;
        let fragment = runtime.idle_fragments[slot];
        let kernel_radius = KERNEL_RADIUS * runtime.tuning.kernel_radius_scale;
        let (_, pinch_position, minimum_density) = rendered_bud_connectivity(
            &runtime.particles[..runtime.particle_count],
            &[],
            usize::MAX,
            fragment,
            kernel_radius,
            runtime.tuning.iso_threshold,
        );
        assert!(pinch_position.is_finite());
        assert!(minimum_density.is_finite());
        let render = runtime.render_state();
        assert_eq!(render.particle_count, runtime.particle_count);
        assert_eq!(render.bubble_count, 1);

        for particle in &mut runtime.particles[..runtime.particle_count] {
            particle.force = Vec2::ZERO;
        }
        runtime.apply_budding_forces();
        let net_force = runtime.particles[..runtime.particle_count]
            .iter()
            .filter(|particle| particle.component_id == runtime.components.main_component)
            .map(|particle| particle.force)
            .sum::<Vec2>();
        let maximum_force = runtime.particles[..runtime.particle_count]
            .iter()
            .map(|particle| particle.force.length())
            .fold(0.0_f32, f32::max);
        assert!(net_force.length() < 1.0e-4, "bud moved COM: {net_force:?}");
        assert!(maximum_force > 0.01, "bud produced no local edge pull");
    }

    #[test]
    fn natural_body_and_bud_field_connects_then_pinches_without_bridge_beads() {
        let runtime = LiquidMorphRuntime::new(0xF13D);
        let main_component = runtime.components.main_component;
        let (source_index, source) = runtime.particles[..runtime.particle_count]
            .iter()
            .enumerate()
            .filter(|(_, particle)| particle.component_id == main_component)
            .max_by(|(_, a), (_, b)| a.position.x.total_cmp(&b.position.x))
            .unwrap();
        let kernel_radius = KERNEL_RADIUS * runtime.tuning.kernel_radius_scale;
        let mut fragment = IdleFragment {
            active: true,
            lifecycle: IdleFragmentLifecycle::Budding,
            age: 0.5,
            lifetime: 1.0,
            radius: 0.052,
            anchor_position: source.position,
            position: source.position + Vec2::X * 0.085,
            direction: Vec2::X,
            source_index,
            ..IdleFragment::default()
        };
        let (connected, _, connected_minimum) = rendered_bud_connectivity(
            &runtime.particles[..runtime.particle_count],
            &[],
            usize::MAX,
            fragment,
            kernel_radius,
            runtime.tuning.iso_threshold,
        );
        assert!(
            connected,
            "overlapping implicit fields did not merge: {connected_minimum}"
        );

        fragment.position = source.position + Vec2::X * 0.30;
        let (connected, pinch, detached_minimum) = rendered_bud_connectivity(
            &runtime.particles[..runtime.particle_count],
            &[],
            usize::MAX,
            fragment,
            kernel_radius,
            runtime.tuning.iso_threshold,
        );
        assert!(
            !connected,
            "separated fields remained connected: {detached_minimum}"
        );
        assert!(pinch.x > source.position.x && pinch.x < fragment.position.x);
    }

    #[test]
    fn cinematic_bud_detaches_once_at_the_visible_saddle_across_tick_rates() {
        for hz in [30.0_f32, 60.0, 120.0] {
            let mut runtime = LiquidMorphRuntime::new(0xB0D5);
            runtime.cinematic_features = true;
            runtime.idle_fragment_timer = 0.0;
            runtime.advance_cinematic_fragments(1.0 / hz, Vec2::ZERO);
            runtime.update_render_proxies(1.0 / hz);
            runtime.resolve_cinematic_pinches();
            runtime.idle_fragment_timer = 100.0;
            let slot = runtime
                .idle_fragments
                .iter()
                .position(|fragment| {
                    fragment.active && fragment.lifecycle == IdleFragmentLifecycle::Budding
                })
                .expect("bud was not spawned");
            let maximum_ticks = (hz * 1.5) as usize;
            let mut detach_tick = None;
            for tick in 0..maximum_ticks {
                runtime.advance_cinematic_fragments(1.0 / hz, Vec2::ZERO);
                runtime.update_render_proxies(1.0 / hz);
                runtime.resolve_cinematic_pinches();
                if runtime.idle_fragments[slot].lifecycle != IdleFragmentLifecycle::Budding {
                    detach_tick = Some(tick);
                    break;
                }
            }
            assert!(
                runtime.idle_fragments[slot].seen_render_connection,
                "{hz} Hz bud never entered the visible body field"
            );
            assert!(detach_tick.is_some(), "{hz} Hz bud never visibly pinched");
            assert_eq!(
                runtime.pinch_sequence, 1,
                "{hz} Hz emitted duplicate pinch events"
            );
            assert!(!runtime.idle_fragments[slot].render_connected);
        }
    }

    #[test]
    fn an_early_visible_saddle_cross_cannot_leave_an_immortal_bud() {
        let mut runtime = LiquidMorphRuntime::new(0xEA71);
        runtime.cinematic_features = true;
        runtime.idle_fragment_timer = 0.0;
        runtime.advance_cinematic_fragments(1.0 / 120.0, Vec2::ZERO);
        runtime.update_render_proxies(1.0 / 120.0);
        runtime.resolve_cinematic_pinches();
        runtime.idle_fragment_timer = 100.0;
        let slot = runtime
            .idle_fragments
            .iter()
            .position(|fragment| fragment.lifecycle == IdleFragmentLifecycle::Budding)
            .expect("bud was not spawned");
        assert!(runtime.idle_fragments[slot].seen_render_connection);
        let fragment = &mut runtime.idle_fragments[slot];
        fragment.age = fragment.lifetime * 0.10;
        fragment.position = fragment.anchor_position + fragment.direction * 0.42;
        runtime.resolve_cinematic_pinches();
        assert_eq!(
            runtime.idle_fragments[slot].lifecycle,
            IdleFragmentLifecycle::Detached
        );
        assert_eq!(runtime.pinch_sequence, 1);
    }

    #[test]
    fn bud_field_neck_width_monotonically_controls_excursion() {
        let thin = bud_excursion(0.16, 0.052, 0.68, 0.0);
        let default = bud_excursion(0.16, 0.052, 0.68, 1.0);
        let wide = bud_excursion(0.16, 0.052, 0.68, 2.5);
        assert!(thin > default && default > wide, "{thin} {default} {wide}");
    }

    #[test]
    fn pinch_spray_is_a_varied_normal_sector_instead_of_a_linear_tail() {
        let mut runtime = LiquidMorphRuntime::new(0x5A7A);
        runtime.tuning.pinch_spray_count = 4;
        runtime.tuning.pinch_spray_cone = 50.0_f32.to_radians();
        runtime.tuning.pinch_spray_speed_variance = 1.0;
        let normal = Vec2::new(0.8, -0.6).normalize();
        let event = PinchEvent {
            position: Vec2::new(0.22, -0.13),
            normal,
            velocity: normal * 0.32,
            radius: 0.052,
            emission: 0.37,
            pigment: 0.68,
            material_coordinate: Vec2::new(0.1, -0.2),
            optical_thickness: 1.12,
            lifetime: 2.0,
        };
        runtime.spawn_pinch_spray(event);
        let spray = runtime
            .idle_fragments
            .iter()
            .filter(|fragment| fragment.active)
            .collect::<Vec<_>>();
        assert_eq!(spray.len(), 4);
        assert!(spray.iter().all(|fragment| {
            fragment.lifecycle == IdleFragmentLifecycle::Spray
                && fragment.render_scale() <= f32::EPSILON
        }));
        let mut angles = spray
            .iter()
            .map(|fragment| {
                normal
                    .perp_dot(fragment.direction)
                    .atan2(normal.dot(fragment.direction))
            })
            .collect::<Vec<_>>();
        angles.sort_by(f32::total_cmp);
        assert!(
            angles
                .iter()
                .all(|angle| { angle.abs() <= runtime.tuning.pinch_spray_cone * 1.07 })
        );
        assert!(
            angles
                .windows(2)
                .all(|pair| pair[1] - pair[0] >= 12.0_f32.to_radians())
        );
        assert!(spray.iter().all(|fragment| {
            (event.radius * 0.08..=event.radius * 0.24).contains(&fragment.radius)
                && (0.6..=1.4).contains(&fragment.detached_lifetime)
                && fragment.emission == event.emission
                && fragment.pigment == event.pigment
        }));
        let speeds = spray
            .iter()
            .map(|fragment| fragment.velocity.length())
            .collect::<Vec<_>>();
        assert!(
            speeds
                .windows(2)
                .any(|pair| (pair[0] - pair[1]).abs() > 1.0e-4)
        );

        let initial_positions = spray
            .iter()
            .map(|fragment| fragment.position)
            .collect::<Vec<_>>();
        drop(spray);
        for _ in 0..8 {
            runtime.advance_cinematic_fragments(1.0 / 60.0, Vec2::ZERO);
            runtime.update_render_proxies(1.0 / 60.0);
            runtime.resolve_cinematic_pinches();
        }
        let separated = runtime
            .idle_fragments
            .iter()
            .filter(|fragment| {
                fragment.active && fragment.lifecycle == IdleFragmentLifecycle::Spray
            })
            .collect::<Vec<_>>();
        assert_eq!(separated.len(), 4);
        assert!(
            separated
                .iter()
                .all(|fragment| fragment.render_scale() > 0.0)
        );
        assert!(separated.iter().enumerate().all(|(index, fragment)| {
            fragment.position.distance(initial_positions[index]) > 0.004
        }));
        let tangent = Vec2::new(-normal.y, normal.x);
        let tangent_spread = separated
            .iter()
            .map(|fragment| (fragment.position - event.position).dot(tangent))
            .fold(
                (f32::INFINITY, f32::NEG_INFINITY),
                |(minimum, maximum), value| (minimum.min(value), maximum.max(value)),
            );
        assert!(tangent_spread.0 < -0.004 && tangent_spread.1 > 0.004);
    }

    #[test]
    fn detached_fragments_keep_world_position_when_the_body_translates() {
        let mut runtime = LiquidMorphRuntime::new(0xD37A);
        runtime.cinematic_features = true;
        runtime.idle_fragment_timer = 100.0;
        for (index, lifecycle) in [
            IdleFragmentLifecycle::Detached,
            IdleFragmentLifecycle::Spray,
            IdleFragmentLifecycle::Fading,
        ]
        .into_iter()
        .enumerate()
        {
            runtime.idle_fragments[index] = IdleFragment {
                active: true,
                lifecycle,
                position: Vec2::new(0.24 + index as f32 * 0.03, -0.11),
                anchor_position: Vec2::new(0.20 + index as f32 * 0.03, -0.10),
                velocity: Vec2::ZERO,
                lifetime: 10.0,
                detached_lifetime: 10.0,
                radius: 0.03,
                ..IdleFragment::default()
            };
        }
        let body_displacement = Vec2::new(0.075, -0.032);
        let world_before = runtime.idle_fragments.map(|fragment| fragment.position);
        let anchors_before = runtime
            .idle_fragments
            .map(|fragment| fragment.anchor_position);
        runtime.advance_cinematic_fragments(1.0 / 120.0, body_displacement);
        for index in 0..3 {
            let world_after = runtime.idle_fragments[index].position + body_displacement;
            assert!(world_after.distance(world_before[index]) < 1.0e-5);
            assert!(
                (runtime.idle_fragments[index].anchor_position + body_displacement)
                    .distance(anchors_before[index])
                    < 1.0e-5
            );
        }
    }

    #[test]
    fn pinch_spray_reservation_is_atomic_under_pool_pressure() {
        let mut runtime = LiquidMorphRuntime::new(0xA70C);
        runtime.tuning.pinch_spray_count = 4;
        for (index, fragment) in runtime.idle_fragments.iter_mut().enumerate() {
            *fragment = IdleFragment {
                active: true,
                lifecycle: if index < 20 {
                    IdleFragmentLifecycle::Detached
                } else {
                    IdleFragmentLifecycle::Spray
                },
                age: index as f32 * 0.01,
                lifetime: 2.0,
                detached_lifetime: 2.0,
                radius: 0.02,
                ..IdleFragment::default()
            };
        }
        runtime.spawn_pinch_spray(PinchEvent {
            normal: Vec2::X,
            radius: 0.05,
            lifetime: 2.0,
            ..PinchEvent::default()
        });
        assert_eq!(
            runtime
                .idle_fragments
                .iter()
                .filter(|fragment| fragment.lifecycle == IdleFragmentLifecycle::Spray)
                .count(),
            4
        );

        for (index, fragment) in runtime.idle_fragments.iter_mut().enumerate() {
            fragment.lifecycle = if index < 22 {
                IdleFragmentLifecycle::Detached
            } else {
                IdleFragmentLifecycle::Budding
            };
        }
        let before = runtime.idle_fragments;
        runtime.spawn_pinch_spray(PinchEvent {
            normal: Vec2::X,
            radius: 0.05,
            lifetime: 2.0,
            ..PinchEvent::default()
        });
        assert_eq!(runtime.idle_fragments, before, "partial spray was emitted");
    }

    #[test]
    fn pinch_recoil_is_local_and_zero_sum() {
        let mut runtime = LiquidMorphRuntime::new(0xB0A7CE);
        let before_mean = runtime.particles[..runtime.particle_count]
            .iter()
            .map(|particle| particle.velocity)
            .sum::<Vec2>()
            / runtime.particle_count as f32;
        let edge = runtime.particles[..runtime.particle_count]
            .iter()
            .max_by(|a, b| a.position.x.total_cmp(&b.position.x))
            .unwrap()
            .position;
        runtime.apply_pinch_recoil(
            PinchEvent {
                position: edge,
                normal: Vec2::X,
                velocity: Vec2::X * 0.28,
                radius: 0.04,
                ..PinchEvent::default()
            },
            KERNEL_RADIUS * runtime.tuning.kernel_radius_scale,
        );
        let after_mean = runtime.particles[..runtime.particle_count]
            .iter()
            .map(|particle| particle.velocity)
            .sum::<Vec2>()
            / runtime.particle_count as f32;
        let maximum_impulse = runtime.particles[..runtime.particle_count]
            .iter()
            .map(|particle| particle.velocity.length())
            .fold(0.0_f32, f32::max);
        assert!(maximum_impulse > 0.01);
        assert!(after_mean.distance(before_mean) < 1.0e-5);
    }

    #[test]
    fn release_lean_recovers_upright_before_idle_lean_can_resume() {
        let mut runtime = LiquidMorphRuntime::new(0xA11E);
        runtime.cinematic_features = true;
        let center = runtime.components.main_com;
        let rotation = Vec2::from_angle(0.10);
        for particle in &mut runtime.particles[..runtime.particle_count] {
            let arm = rotate_vector(particle.position - center, rotation);
            particle.position = center + arm;
            particle.previous_position = particle.position;
            particle.predicted_position = particle.position;
        }
        let dt = 1.0 / 120.0;
        let target = runtime.update_cinematic_lean(0.0, 0.0, true, dt);
        runtime.stabilize_main_orientation(dt, target);
        for _ in 1..240 {
            let target = runtime.update_cinematic_lean(0.0, 0.0, false, dt);
            runtime.stabilize_main_orientation(dt, target);
        }
        assert!(runtime.main_component_orientation().abs() < 0.5_f32.to_radians());
        assert_eq!(runtime.lean_phase, LeanPhase::UprightHold);
    }

    #[test]
    fn cinematic_breathing_force_has_no_translation_or_rigid_spin() {
        let mut runtime = LiquidMorphRuntime::new(163);
        runtime.cinematic_features = true;
        for particle in &mut runtime.particles[..runtime.particle_count] {
            particle.force = Vec2::ZERO;
        }
        runtime.apply_idle_breathing(0.92, 0.0, 1.0 / 120.0);
        let center = runtime.components.main_com;
        let mut net_force = Vec2::ZERO;
        let mut torque = 0.0;
        let mut maximum_force = 0.0_f32;
        for particle in &runtime.particles[..runtime.particle_count] {
            if particle.component_id != runtime.components.main_component {
                continue;
            }
            net_force += particle.force;
            torque += (particle.position - center).perp_dot(particle.force);
            maximum_force = maximum_force.max(particle.force.length());
        }
        assert!(maximum_force > 0.001);
        assert!(
            net_force.length() < 1.0e-4,
            "breath moved COM: {net_force:?}"
        );
        assert!(
            torque.abs() < 1.0e-4,
            "breath injected rigid spin: {torque}"
        );
    }

    #[test]
    fn cinematic_breathing_is_slow_and_frequency_stable() {
        let genome = Genome::from_seed(179);
        let traits = DerivedVisualTraits::from_genome(&genome);
        let physiology = VisualPhysiologyRuntime::new(genome.identity_seed, &traits);
        let mut modulation = [0.0_f32; 3];
        for (result, hz) in modulation.iter_mut().zip([30.0_f32, 60.0, 120.0]) {
            let mut tuning = PbfTuning {
                idle_fragment_size: 0.0,
                ..PbfTuning::default()
            };
            tuning.fixed_hz = hz;
            let mut runtime = LiquidMorphRuntime::new_with_tuning(genome.identity_seed, tuning);
            runtime.cinematic_features = true;
            let mut run_traits = traits;
            run_traits.flow_speed = 0.0;
            let dt = 1.0 / hz;
            let mut minimum_extent = f32::INFINITY;
            let mut maximum_extent = 0.0_f32;
            for frame in 0..(hz as usize * 9) {
                let time = frame as f32 * dt;
                let breath = 0.5 + 0.5 * (time * 1.2 + runtime.seed_phase).sin();
                runtime.update(
                    &genome.body,
                    &run_traits,
                    physiology.pose,
                    VisualMindInput::default(),
                    &neutral_intent(),
                    &SensorFrame::default(),
                    &BodyFeedback::default(),
                    DropletMotion::default(),
                    ModalDeformation::default(),
                    breath,
                    dt,
                );
                if time >= 2.0 {
                    let extent = horizontal_extent(&runtime);
                    minimum_extent = minimum_extent.min(extent);
                    maximum_extent = maximum_extent.max(extent);
                }
            }
            let mean_extent = (minimum_extent + maximum_extent) * 0.5;
            *result = (maximum_extent - minimum_extent) / mean_extent.max(1.0e-5);
            assert!(runtime.diagnostics.finite);
        }
        assert!(
            modulation
                .iter()
                .all(|value| (0.01..=0.025).contains(value)),
            "breathing escaped the 1–2.5% silhouette band: {modulation:?}"
        );
        let minimum = modulation.into_iter().fold(f32::INFINITY, f32::min);
        let maximum = modulation.into_iter().fold(f32::NEG_INFINITY, f32::max);
        assert!(
            (maximum - minimum) / maximum.max(1.0e-5) <= 0.10,
            "breathing changed with fixed Hz: {modulation:?}"
        );
    }

    #[test]
    fn lab_containment_keeps_every_component_inside_without_affecting_free_mode() {
        let mut runtime = LiquidMorphRuntime::new(79);
        let bounds = (Vec2::new(-0.52, -0.46), Vec2::new(0.52, 0.46));
        runtime.set_local_containment_bounds(Some(bounds));
        let kernel_radius = KERNEL_RADIUS * runtime.tuning.kernel_radius_scale;
        runtime.particles[0].position = Vec2::new(0.50, 0.0);
        runtime.particles[0].velocity = Vec2::new(1.0, 0.0);
        runtime.particles[0].force = Vec2::ZERO;
        runtime.apply_local_containment_forces(kernel_radius);
        assert!(runtime.particles[0].force.x < 0.0);

        for (index, particle) in runtime.particles[..runtime.particle_count]
            .iter_mut()
            .enumerate()
        {
            particle.predicted_position = Vec2::new(
                if index & 1 == 0 { -2.0 } else { 2.0 },
                if index & 2 == 0 { -2.0 } else { 2.0 },
            );
        }
        runtime.solve_local_containment(kernel_radius);
        let guard = kernel_radius * 0.48;
        let minimum = bounds.0 + Vec2::splat(guard);
        let maximum = bounds.1 - Vec2::splat(guard);
        assert!(
            runtime.particles[..runtime.particle_count]
                .iter()
                .all(|particle| particle.predicted_position.cmpge(minimum).all()
                    && particle.predicted_position.cmple(maximum).all())
        );

        runtime.set_local_containment_bounds(None);
        runtime.particles[0].predicted_position = Vec2::new(2.0, -2.0);
        runtime.solve_local_containment(kernel_radius);
        assert_eq!(
            runtime.particles[0].predicted_position,
            Vec2::new(2.0, -2.0)
        );
    }

    #[test]
    fn upright_gauge_removes_rigid_spin_without_collapsing_the_blob() {
        let mut runtime = LiquidMorphRuntime::new(91);
        let center = runtime.components.main_com;
        let rotation = Vec2::from_angle(0.72);
        for particle in &mut runtime.particles[..runtime.particle_count] {
            let arm = rotate_vector(particle.position - center, rotation);
            particle.position = center + arm;
            particle.previous_position = particle.position;
            particle.predicted_position = particle.position;
            particle.velocity = Vec2::new(-arm.y, arm.x) * 2.4;
        }
        assert!(runtime.main_component_orientation() > 0.65);
        assert!(runtime.main_component_angular_velocity() > 2.2);

        for _ in 0..90 {
            runtime.stabilize_main_orientation(1.0 / 120.0, 0.0);
        }

        assert!(runtime.main_component_orientation().abs() < 0.001);
        assert!(runtime.main_component_angular_velocity().abs() < 0.001);
        let maximum_radius = runtime.particles[..runtime.particle_count]
            .iter()
            .map(|particle| particle.position.distance(center))
            .fold(0.0_f32, f32::max);
        assert!(maximum_radius > 0.25);
    }

    #[test]
    fn cinematic_idle_lean_is_seeded_bounded_and_preserves_com() {
        let mut first = LiquidMorphRuntime::new(0x1EA1);
        first.cinematic_features = true;
        let mut replay = first.clone();
        let original_com = first.components.main_com;
        let mut maximum = 0.0_f32;
        for _ in 0..(120 * 18) {
            first.elapsed += 1.0 / 120.0;
            replay.elapsed += 1.0 / 120.0;
            let target = first.update_cinematic_lean(0.0, 0.0, false, 1.0 / 120.0);
            let replay_target = replay.update_cinematic_lean(0.0, 0.0, false, 1.0 / 120.0);
            assert_eq!(target, replay_target);
            assert!(target.abs() <= first.tuning.idle_lean_angle + 1.0e-6);
            first.stabilize_main_orientation(1.0 / 120.0, target);
            replay.stabilize_main_orientation(1.0 / 120.0, replay_target);
            maximum = maximum.max(first.main_component_orientation().abs());
        }
        assert!(maximum > first.tuning.idle_lean_angle * 0.28);
        assert!(first.components.main_com.distance(original_com) < 1.0e-6);
        assert!(first.main_component_angular_velocity().abs() < 0.02);
        assert_eq!(first.particles, replay.particles);
    }

    #[test]
    fn idle_lean_response_is_stable_at_30_60_and_120_hz() {
        let mut maxima = [0.0_f32; 3];
        for (result, hz) in maxima.iter_mut().zip([30.0_f32, 60.0, 120.0]) {
            let mut runtime = LiquidMorphRuntime::new(0xB0AD);
            runtime.cinematic_features = true;
            let dt = 1.0 / hz;
            for _ in 0..(hz as usize * 18) {
                runtime.elapsed += dt;
                let target = runtime.update_cinematic_lean(0.0, 0.0, false, dt);
                runtime.stabilize_main_orientation(dt, target);
                *result = result.max(runtime.main_component_orientation().abs());
            }
            assert!(*result <= runtime.tuning.idle_lean_angle + 0.004);
        }
        let minimum = maxima.into_iter().fold(f32::INFINITY, f32::min);
        let maximum = maxima.into_iter().fold(f32::NEG_INFINITY, f32::max);
        assert!(
            (maximum - minimum) / maximum.max(1.0e-5) <= 0.10,
            "{maxima:?}"
        );
    }

    #[test]
    fn calm_reconstruction_is_stable_across_fixed_frequencies() {
        let genome = Genome::from_seed(42);
        let traits = DerivedVisualTraits::from_genome(&genome);
        let physiology = VisualPhysiologyRuntime::new(genome.identity_seed, &traits);
        let baseline = PbfTuning::default();
        let mut render_speeds = [0.0; 2];
        for (result, hz) in render_speeds.iter_mut().zip([60.0_f32, 120.0]) {
            let dt = 1.0 / hz;
            let mut runtime = LiquidMorphRuntime::new_with_tuning(genome.identity_seed, baseline);
            let mut run_traits = traits;
            run_traits.flow_speed = 0.18;
            let mut previous = runtime.render_state();
            let mut render_speed = 0.0;
            let mut samples = 0.0;
            for frame in 0..(hz as usize * 4) {
                runtime.update(
                    &genome.body,
                    &run_traits,
                    physiology.pose,
                    VisualMindInput {
                        curiosity: 0.45,
                        attachment: 0.55,
                        confidence: 0.60,
                        ..VisualMindInput::default()
                    },
                    &neutral_intent(),
                    &SensorFrame::default(),
                    &BodyFeedback::default(),
                    DropletMotion::default(),
                    ModalDeformation::default(),
                    0.5,
                    dt,
                );
                let current = runtime.render_state();
                if frame >= hz as usize * 3 {
                    render_speed += current.particles[..current.particle_count]
                        .iter()
                        .zip(&previous.particles[..previous.particle_count])
                        .map(|(current, previous)| {
                            current.position.distance(previous.position) / dt
                        })
                        .sum::<f32>()
                        / current.particle_count as f32;
                    samples += 1.0;
                }
                previous = current;
            }
            assert!(runtime.diagnostics.finite);
            *result = render_speed / samples;
        }
        assert!(
            render_speeds.into_iter().all(|speed| speed < 0.04),
            "calm reconstruction moved too quickly: {render_speeds:?}"
        );
    }

    #[test]
    fn comoving_flight_lags_against_acceleration_then_settles_at_constant_velocity() {
        let genome = Genome::from_seed(7);
        let traits = DerivedVisualTraits::from_genome(&genome);
        let physiology = VisualPhysiologyRuntime::new(genome.identity_seed, &traits);
        let mut runtime = LiquidMorphRuntime::new(genome.identity_seed);
        let initial_com = runtime.components.main_com;
        for _ in 0..60 {
            runtime.update(
                &genome.body,
                &traits,
                physiology.pose,
                VisualMindInput {
                    arousal: 0.75,
                    ..VisualMindInput::default()
                },
                &neutral_intent(),
                &SensorFrame::default(),
                &BodyFeedback {
                    velocity: Vec2::new(0.8, 0.0),
                    acceleration: Vec2::new(0.8, 0.0),
                    ..BodyFeedback::default()
                },
                DropletMotion {
                    displacement: Vec2::new(0.8 / 120.0, 0.0),
                    presentation_displacement: Vec2::new(0.8 / 120.0, 0.0),
                    velocity: Vec2::new(0.8, 0.0),
                    acceleration: Vec2::new(0.8, 0.0),
                    world_to_body_scale: Vec2::ONE,
                },
                ModalDeformation::default(),
                0.5,
                1.0 / 120.0,
            );
        }
        let accelerating_com = runtime.components.main_com;
        assert!(
            accelerating_com.x < initial_com.x - 0.001,
            "acceleration +X must create inertial lag -X: initial={initial_com:?}, accelerated={accelerating_com:?}"
        );
        assert!(runtime.components.main_mass >= 92.0);

        for _ in 0..240 {
            runtime.update(
                &genome.body,
                &traits,
                physiology.pose,
                VisualMindInput::default(),
                &neutral_intent(),
                &SensorFrame::default(),
                &BodyFeedback {
                    velocity: Vec2::new(0.8, 0.0),
                    ..BodyFeedback::default()
                },
                DropletMotion {
                    velocity: Vec2::new(0.8, 0.0),
                    world_to_body_scale: Vec2::ONE,
                    ..DropletMotion::default()
                },
                ModalDeformation::default(),
                0.5,
                1.0 / 120.0,
            );
        }
        let cruising_com = runtime.components.main_com;
        assert!(
            cruising_com.distance(initial_com) < 0.025,
            "constant velocity must not leave a permanent tail: {cruising_com:?}"
        );
        assert!(runtime.components.main_mass >= 92.0);

        for _ in 0..30 {
            runtime.update(
                &genome.body,
                &traits,
                physiology.pose,
                VisualMindInput::default(),
                &neutral_intent(),
                &SensorFrame::default(),
                &BodyFeedback {
                    velocity: Vec2::new(0.4, 0.0),
                    acceleration: Vec2::new(-1.6, 0.0),
                    ..BodyFeedback::default()
                },
                DropletMotion {
                    velocity: Vec2::new(0.4, 0.0),
                    acceleration: Vec2::new(-1.6, 0.0),
                    world_to_body_scale: Vec2::ONE,
                    ..DropletMotion::default()
                },
                ModalDeformation::default(),
                0.5,
                1.0 / 120.0,
            );
        }
        assert!(
            runtime.components.main_com.x > cruising_com.x,
            "braking must produce one forward overshoot"
        );
        assert!(runtime.components.main_mass >= 92.0);
    }

    #[test]
    fn production_cruise_recovers_shape_instead_of_accumulating_a_permanent_tail() {
        let genome = Genome::from_seed(0xC0A57);
        let traits = DerivedVisualTraits::from_genome(&genome);
        let physiology = VisualPhysiologyRuntime::new(genome.identity_seed, &traits);
        let mut runtime = LiquidMorphRuntime::new(genome.identity_seed);
        let dt = 1.0 / 120.0;

        for _ in 0..60 {
            runtime.update(
                &genome.body,
                &traits,
                physiology.pose,
                VisualMindInput::default(),
                &neutral_intent(),
                &SensorFrame::default(),
                &BodyFeedback {
                    velocity: Vec2::new(2.0, 0.0),
                    acceleration: Vec2::new(3.2, 0.0),
                    ..BodyFeedback::default()
                },
                DropletMotion {
                    velocity: Vec2::new(2.0, 0.0),
                    acceleration: Vec2::new(3.2, 0.0),
                    world_to_body_scale: Vec2::ONE,
                    ..DropletMotion::default()
                },
                ModalDeformation::default(),
                0.5,
                dt,
            );
            assert!(runtime.components.main_mass >= 92.0);
        }
        let driven_stretch = runtime.diagnostics.stretch_ratio;

        for _ in 0..480 {
            runtime.update(
                &genome.body,
                &traits,
                physiology.pose,
                VisualMindInput::default(),
                &neutral_intent(),
                &SensorFrame::default(),
                &BodyFeedback {
                    velocity: Vec2::new(2.0, 0.0),
                    ..BodyFeedback::default()
                },
                DropletMotion {
                    velocity: Vec2::new(2.0, 0.0),
                    world_to_body_scale: Vec2::ONE,
                    ..DropletMotion::default()
                },
                ModalDeformation::default(),
                0.5,
                dt,
            );
            assert!(runtime.components.main_mass >= 92.0);
        }

        assert!(
            runtime.diagnostics.stretch_ratio <= 1.18,
            "constant-velocity cruise kept a permanent tail: driven={driven_stretch}, settled={}",
            runtime.diagnostics.stretch_ratio
        );
        assert!(runtime.diagnostics.finite);
    }

    #[test]
    fn cinematic_production_scale_acceleration_has_readable_deformation_and_settles() {
        let genome = Genome::from_seed(0xC1A0);
        let traits = DerivedVisualTraits::from_genome(&genome);
        let physiology = VisualPhysiologyRuntime::new(genome.identity_seed, &traits);
        let mut runtime = LiquidMorphRuntime::new(genome.identity_seed);
        runtime.cinematic_features = true;
        runtime.tuning.idle_fragment_size = 0.0;
        let dt = 1.0 / 120.0;
        let resting_extent = horizontal_extent(&runtime).max(1.0e-5);
        let mut maximum_stretch = 1.0_f32;
        for _ in 0..72 {
            runtime.update(
                &genome.body,
                &traits,
                physiology.pose,
                VisualMindInput::default(),
                &neutral_intent(),
                &SensorFrame::default(),
                &BodyFeedback {
                    velocity: Vec2::new(0.55, 0.0),
                    acceleration: Vec2::new(1.0, 0.0),
                    ..BodyFeedback::default()
                },
                DropletMotion {
                    velocity: Vec2::new(0.55, 0.0),
                    acceleration: Vec2::new(1.0, 0.0),
                    world_to_body_scale: Vec2::ONE,
                    ..DropletMotion::default()
                },
                ModalDeformation::default(),
                0.5,
                dt,
            );
            maximum_stretch = maximum_stretch.max(horizontal_extent(&runtime) / resting_extent);
            assert!(runtime.components.main_mass >= 92.0);
        }
        assert!(
            maximum_stretch >= 1.08,
            "ordinary flight remained visually rigid: {maximum_stretch}"
        );

        for _ in 0..300 {
            runtime.update(
                &genome.body,
                &traits,
                physiology.pose,
                VisualMindInput::default(),
                &neutral_intent(),
                &SensorFrame::default(),
                &BodyFeedback {
                    velocity: Vec2::new(0.55, 0.0),
                    ..BodyFeedback::default()
                },
                DropletMotion {
                    velocity: Vec2::new(0.55, 0.0),
                    world_to_body_scale: Vec2::ONE,
                    ..DropletMotion::default()
                },
                ModalDeformation::default(),
                0.5,
                dt,
            );
            assert!(runtime.components.main_mass >= 92.0);
        }
        let settled_stretch = horizontal_extent(&runtime) / resting_extent;
        assert!(
            settled_stretch <= 1.04,
            "flight left a permanent tail: {settled_stretch}"
        );
    }

    #[test]
    fn comoving_flight_response_is_stable_at_30_60_and_120_hz() {
        let genome = Genome::from_seed(0xF11E);
        let traits = DerivedVisualTraits::from_genome(&genome);
        let physiology = VisualPhysiologyRuntime::new(genome.identity_seed, &traits);
        let mut maximum_extents = [0.0_f32; 3];
        for (result, hz) in maximum_extents.iter_mut().zip([30.0_f32, 60.0, 120.0]) {
            let mut runtime = LiquidMorphRuntime::new(genome.identity_seed);
            runtime.cinematic_features = true;
            runtime.tuning.idle_fragment_size = 0.0;
            runtime.tuning.fixed_hz = hz;
            let dt = 1.0 / hz;
            let resting_extent = horizontal_extent(&runtime).max(1.0e-5);
            for frame in 0..(hz * 2.0) as usize {
                let acceleration = if frame < (hz * 0.45) as usize {
                    Vec2::new(2.4, 0.0)
                } else {
                    Vec2::ZERO
                };
                runtime.update(
                    &genome.body,
                    &traits,
                    physiology.pose,
                    VisualMindInput::default(),
                    &neutral_intent(),
                    &SensorFrame::default(),
                    &BodyFeedback {
                        velocity: Vec2::new(0.6, 0.0),
                        acceleration,
                        ..BodyFeedback::default()
                    },
                    DropletMotion {
                        velocity: Vec2::new(0.6, 0.0),
                        acceleration,
                        world_to_body_scale: Vec2::ONE,
                        ..DropletMotion::default()
                    },
                    ModalDeformation::default(),
                    0.5,
                    dt,
                );
                *result = (*result).max(horizontal_extent(&runtime) / resting_extent);
                assert!(runtime.components.main_mass >= 92.0);
            }
        }
        let minimum = maximum_extents.into_iter().fold(f32::INFINITY, f32::min);
        let maximum = maximum_extents.into_iter().fold(0.0_f32, f32::max);
        assert!(
            minimum >= 1.06,
            "frequency test passed with a rigid response: {maximum_extents:?}"
        );
        assert!(
            maximum / minimum.max(1.0e-5) <= 1.10,
            "frequency-dependent flight stretch: {maximum_extents:?}"
        );
    }

    #[test]
    fn moving_character_field_leaves_material_behind_then_gathers_it_after_stop() {
        let genome = Genome::from_seed(0x4F11);
        let traits = DerivedVisualTraits::from_genome(&genome);
        let physiology = VisualPhysiologyRuntime::new(genome.identity_seed, &traits);
        let mut runtime = LiquidMorphRuntime::new(genome.identity_seed);
        runtime.cinematic_features = true;
        runtime.tuning.idle_fragment_size = 0.0;
        let dt = 1.0 / 120.0;
        let rest_x = extent_along(&runtime, Vec2::X);
        let rest_y = extent_along(&runtime, Vec2::Y);
        let mut peak_x = 1.0_f32;
        let mut peak_y = 1.0_f32;
        let mut peak_detached_mass = 0.0_f32;

        for frame in 0..300 {
            let (velocity, acceleration) = match frame {
                0..=59 => (Vec2::new(0.7, 0.0), Vec2::new(4.2, 0.0)),
                60..=119 => (Vec2::new(0.7, 0.0), Vec2::ZERO),
                120..=179 => (Vec2::new(0.2, 0.0), Vec2::new(-4.2, 0.0)),
                180..=239 => (Vec2::new(0.0, 0.7), Vec2::new(0.0, 4.2)),
                _ => (Vec2::new(0.0, 0.7), Vec2::ZERO),
            };
            runtime.update(
                &genome.body,
                &traits,
                physiology.pose,
                VisualMindInput::default(),
                &neutral_intent(),
                &SensorFrame::default(),
                &BodyFeedback {
                    velocity,
                    acceleration,
                    ..BodyFeedback::default()
                },
                DropletMotion {
                    velocity,
                    acceleration,
                    world_to_body_scale: Vec2::ONE,
                    ..DropletMotion::default()
                },
                ModalDeformation::default(),
                0.5,
                dt,
            );
            peak_x = peak_x.max(extent_along(&runtime, Vec2::X) / rest_x);
            peak_y = peak_y.max(extent_along(&runtime, Vec2::Y) / rest_y);
            peak_detached_mass = peak_detached_mass.max(runtime.components.detached_mass);
            assert!(
                runtime.components.main_mass >= 72.0,
                "frame {frame}: {:?}",
                runtime.components
            );
            assert!(runtime.components.component_count <= 6, "frame {frame}");
        }
        assert!(peak_x >= 1.08, "start/brake stayed rigid: {peak_x}");
        assert!(peak_y >= 1.05, "turn stayed rigid: {peak_y}");
        assert!(
            (1.0..=24.0).contains(&peak_detached_mass),
            "flight did not create bounded real lagging mass: {peak_detached_mass}"
        );

        let mut post_stop_peak_speed = 0.0_f32;
        for _ in 0..960 {
            runtime.update(
                &genome.body,
                &traits,
                physiology.pose,
                VisualMindInput::default(),
                &neutral_intent(),
                &SensorFrame::default(),
                &BodyFeedback::default(),
                DropletMotion::default(),
                ModalDeformation::default(),
                0.5,
                dt,
            );
            post_stop_peak_speed = post_stop_peak_speed.max(runtime.diagnostics.maximum_speed);
        }
        assert!(extent_along(&runtime, Vec2::X) / rest_x <= 1.05);
        assert!(extent_along(&runtime, Vec2::Y) / rest_y <= 1.05);
        eprintln!(
            "moving field: peak_detached_mass={peak_detached_mass:.1}, post_stop_peak_speed={post_stop_peak_speed:.3}, final_components={}",
            runtime.components.component_count
        );
        assert_eq!(runtime.components.component_count, 1);
        assert_eq!(runtime.components.main_mass, runtime.particle_count as f32);
        assert!(
            post_stop_peak_speed < 4.0,
            "catch-up created a collision-energy spike: {post_stop_peak_speed}"
        );
        assert!(
            runtime
                .idle_fragments
                .iter()
                .all(|fragment| !fragment.active),
            "flight created an authored fragment instead of moving real material"
        );
    }

    #[test]
    fn high_impact_substep_threshold_has_no_deformation_pop() {
        let peak = |acceleration_magnitude: f32| {
            let genome = Genome::from_seed(0x5AB5);
            let traits = DerivedVisualTraits::from_genome(&genome);
            let physiology = VisualPhysiologyRuntime::new(genome.identity_seed, &traits);
            let mut runtime = LiquidMorphRuntime::new(genome.identity_seed);
            runtime.cinematic_features = true;
            runtime.tuning.idle_fragment_size = 0.0;
            let rest = horizontal_extent(&runtime);
            let mut maximum = 1.0_f32;
            for _ in 0..90 {
                let acceleration = Vec2::new(acceleration_magnitude, 0.0);
                runtime.update(
                    &genome.body,
                    &traits,
                    physiology.pose,
                    VisualMindInput::default(),
                    &neutral_intent(),
                    &SensorFrame::default(),
                    &BodyFeedback {
                        acceleration,
                        ..BodyFeedback::default()
                    },
                    DropletMotion {
                        acceleration,
                        world_to_body_scale: Vec2::ONE,
                        ..DropletMotion::default()
                    },
                    ModalDeformation::default(),
                    0.5,
                    1.0 / 120.0,
                );
                maximum = maximum.max(horizontal_extent(&runtime) / rest);
                assert!(runtime.components.main_mass >= 92.0);
            }
            maximum
        };
        let below = peak(3.39);
        let above = peak(3.41);
        assert!(
            (above - below).abs() / below.max(1.0e-5) <= 0.035,
            "substep threshold popped: below={below}, above={above}"
        );
    }

    #[test]
    fn mouse_field_follows_cursor_then_releases_non_rigid_slosh() {
        let genome = Genome::from_seed(17);
        let traits = DerivedVisualTraits::from_genome(&genome);
        let physiology = VisualPhysiologyRuntime::new(genome.identity_seed, &traits);
        let mut runtime = LiquidMorphRuntime::new(genome.identity_seed);
        runtime.set_navigation_anchor_strength(0.0);
        run(&mut runtime, &genome, 2.0);
        let initial_render = runtime.render_state();
        let initial_right_edge = runtime.particles[..runtime.particle_count]
            .iter()
            .map(|particle| particle.position.x)
            .fold(f32::NEG_INFINITY, f32::max);
        let initial_com = runtime.components.main_com;
        let feedback = BodyFeedback {
            world_position: Vec2::splat(0.5),
            ..BodyFeedback::default()
        };
        let motion = DropletMotion {
            world_to_body_scale: Vec2::new(2.0, -2.0),
            ..DropletMotion::default()
        };
        let dt = 1.0 / 120.0;

        for frame in 0..60 {
            let progress = (frame + 1) as f32 / 60.0;
            runtime.update(
                &genome.body,
                &traits,
                physiology.pose,
                VisualMindInput::default(),
                &neutral_intent(),
                &SensorFrame {
                    cursor_position: Vec2::new(0.5 + progress * 0.12, 0.5),
                    cursor_velocity: Vec2::new(0.24, 0.0),
                    pointer_down: true,
                    pointer_pressed: frame == 0,
                    pet_dragged: true,
                    ..SensorFrame::default()
                },
                &feedback,
                motion,
                ModalDeformation::default(),
                0.5,
                dt,
            );
        }
        let dragged_right_edge = runtime.particles[..runtime.particle_count]
            .iter()
            .map(|particle| particle.position.x)
            .fold(f32::NEG_INFINITY, f32::max);
        let dragged_render = runtime.render_state();
        let moved_particle_count = dragged_render.particles[..dragged_render.particle_count]
            .iter()
            .zip(&initial_render.particles[..initial_render.particle_count])
            .filter(|(dragged, initial)| dragged.position.distance(initial.position) > 0.01)
            .count();
        let mean_particle_displacement = dragged_render.particles[..dragged_render.particle_count]
            .iter()
            .zip(&initial_render.particles[..initial_render.particle_count])
            .map(|(dragged, initial)| dragged.position.distance(initial.position))
            .sum::<f32>()
            / dragged_render.particle_count as f32;
        let mut released_runtime = runtime.clone();
        released_runtime.update(
            &genome.body,
            &traits,
            physiology.pose,
            VisualMindInput::default(),
            &neutral_intent(),
            &SensorFrame {
                cursor_position: Vec2::new(0.62, 0.5),
                pointer_released: true,
                ..SensorFrame::default()
            },
            &feedback,
            motion,
            ModalDeformation::default(),
            0.5,
            dt,
        );
        let mean_velocity = released_runtime.particles[..released_runtime.particle_count]
            .iter()
            .map(|particle| particle.velocity)
            .sum::<Vec2>()
            / released_runtime.particle_count as f32;
        let relative_speed = released_runtime.particles[..released_runtime.particle_count]
            .iter()
            .map(|particle| (particle.velocity - mean_velocity).length())
            .sum::<f32>()
            / released_runtime.particle_count as f32;

        let mut previous = runtime.render_state();
        let mut held_render_speed = 0.0;
        for frame in 0..120 {
            runtime.update(
                &genome.body,
                &traits,
                physiology.pose,
                VisualMindInput::default(),
                &neutral_intent(),
                &SensorFrame {
                    cursor_position: Vec2::new(0.62, 0.5),
                    pointer_down: true,
                    pet_dragged: true,
                    ..SensorFrame::default()
                },
                &feedback,
                motion,
                ModalDeformation::default(),
                0.5,
                dt,
            );
            let current = runtime.render_state();
            if frame >= 90 {
                held_render_speed += current.particles[..current.particle_count]
                    .iter()
                    .zip(&previous.particles[..previous.particle_count])
                    .map(|(current, previous)| current.position.distance(previous.position) / dt)
                    .sum::<f32>()
                    / current.particle_count as f32;
            }
            previous = current;
        }
        held_render_speed /= 30.0;
        eprintln!(
            "cursor field short pull: edge_delta={:.4}, com_delta={:.4}, moved_particles={moved_particle_count}, mean_particle_displacement={mean_particle_displacement:.4}, held_render_speed={held_render_speed:.4}, release_relative_speed={relative_speed:.4}",
            dragged_right_edge - initial_right_edge,
            runtime.components.main_com.x - initial_com.x,
        );
        // The cursor field may compress before it transports mass, so an interior
        // pull need not monotonically extend the outer silhouette.
        assert!(
            runtime.components.main_com.x > initial_com.x + 0.004,
            "short cursor pull did not move COM: initial={initial_com:?}, final={:?}",
            runtime.components.main_com
        );
        // The regression is about actual visible material. Face or COM motion is
        // not accepted as proof that the cursor interaction works.
        assert!(moved_particle_count >= runtime.particle_count / 3);
        assert!(mean_particle_displacement > 0.006);
        assert!(held_render_speed < 0.08);
        assert!(relative_speed > 0.015);
        assert!(runtime.diagnostics.finite);
        assert!(released_runtime.diagnostics.finite);
    }

    #[test]
    fn stationary_inner_pointer_field_settles_without_visible_jerk() {
        let genome = Genome::from_seed(0xF13D);
        let traits = DerivedVisualTraits::from_genome(&genome);
        let physiology = VisualPhysiologyRuntime::new(genome.identity_seed, &traits);
        let tuning = PbfTuning {
            viscosity: 0.0,
            idle_fragment_size: 0.0,
            ..PbfTuning::default()
        };
        let mut runtime = LiquidMorphRuntime::new_with_tuning(genome.identity_seed, tuning);
        runtime.cinematic_features = true;
        runtime.set_navigation_anchor_strength(0.0);
        run(&mut runtime, &genome, 2.0);
        let target = runtime.components.main_com + Vec2::new(0.07, 0.02);
        let cursor_local = runtime.particles[..runtime.particle_count]
            .iter()
            .min_by(|a, b| {
                a.position
                    .distance_squared(target)
                    .total_cmp(&b.position.distance_squared(target))
            })
            .map_or(target, |particle| particle.position);
        let world_to_body_scale = Vec2::new(2.0, -2.0);
        let feedback = BodyFeedback {
            world_position: Vec2::splat(0.5),
            ..BodyFeedback::default()
        };
        let motion = DropletMotion {
            world_to_body_scale,
            ..DropletMotion::default()
        };
        let dt = 1.0 / 120.0;
        let mut previous = runtime.render_state();
        let mut previous_velocities = [Vec2::ZERO; MAX_LIQUID_PARTICLES];
        let mut mean_render_speed = 0.0_f32;
        let mut mean_velocity_jump = 0.0_f32;
        let mut maximum_render_step = 0.0_f32;
        let mut samples = 0.0_f32;
        for frame in 0..720 {
            runtime.update(
                &genome.body,
                &traits,
                physiology.pose,
                VisualMindInput::default(),
                &neutral_intent(),
                &SensorFrame {
                    cursor_position: Vec2::splat(0.5) + cursor_local / world_to_body_scale,
                    pointer_down: true,
                    pointer_pressed: frame == 0,
                    pet_dragged: true,
                    ..SensorFrame::default()
                },
                &feedback,
                motion,
                ModalDeformation::default(),
                0.5,
                dt,
            );
            let current = runtime.render_state();
            for (index, previous_velocity) in previous_velocities
                .iter_mut()
                .enumerate()
                .take(current.particle_count)
            {
                let step = current.particles[index]
                    .position
                    .distance(previous.particles[index].position);
                let render_velocity =
                    (current.particles[index].position - previous.particles[index].position) / dt;
                if frame >= 480 {
                    mean_render_speed += render_velocity.length();
                    mean_velocity_jump += render_velocity.distance(*previous_velocity);
                    maximum_render_step = maximum_render_step.max(step);
                    samples += 1.0;
                }
                *previous_velocity = render_velocity;
            }
            previous = current;
        }
        mean_render_speed /= samples.max(1.0);
        mean_velocity_jump /= samples.max(1.0);
        eprintln!(
            "stationary field: mean_render_speed={mean_render_speed:.5}, mean_velocity_jump={mean_velocity_jump:.5}, max_step={maximum_render_step:.6}, components={}",
            runtime.components.component_count,
        );
        assert!(runtime.material_grab.is_active());
        assert!(
            runtime
                .idle_fragments
                .iter()
                .all(|fragment| !fragment.active)
        );
        assert!(mean_render_speed < 0.06);
        assert!(mean_velocity_jump < 0.01);
        assert!(maximum_render_step < 0.01);
        assert!(runtime.diagnostics.finite);
    }

    #[test]
    fn slow_surface_field_can_pull_mass_out_of_the_main_field() {
        let genome = Genome::from_seed(117);
        let traits = DerivedVisualTraits::from_genome(&genome);
        let physiology = VisualPhysiologyRuntime::new(genome.identity_seed, &traits);
        let mut runtime = LiquidMorphRuntime::new(genome.identity_seed);
        run(&mut runtime, &genome, 2.0);
        let initial_extent = horizontal_extent(&runtime);
        let start = runtime.particles[..runtime.particle_count]
            .iter()
            .max_by(|a, b| a.position.x.total_cmp(&b.position.x))
            .map_or(runtime.components.main_com, |particle| particle.position);
        let feedback = BodyFeedback {
            world_position: Vec2::splat(0.5),
            ..BodyFeedback::default()
        };
        let motion = DropletMotion {
            world_to_body_scale: Vec2::new(2.0, -2.0),
            ..DropletMotion::default()
        };
        let frames = 240;
        let mut peak_detached_mass = 0.0_f32;
        let mut peak_extent = initial_extent;
        for frame in 0..frames {
            let progress = (frame + 1) as f32 / frames as f32;
            let cursor_local = start + Vec2::new(progress * 0.24, 0.0);
            runtime.update(
                &genome.body,
                &traits,
                physiology.pose,
                VisualMindInput::default(),
                &neutral_intent(),
                &SensorFrame {
                    cursor_position: Vec2::splat(0.5) + cursor_local / Vec2::new(2.0, -2.0),
                    cursor_velocity: Vec2::new(0.06, 0.0),
                    pointer_down: true,
                    pointer_pressed: frame == 0,
                    pet_dragged: true,
                    ..SensorFrame::default()
                },
                &feedback,
                motion,
                ModalDeformation::default(),
                0.5,
                1.0 / 120.0,
            );
            peak_detached_mass = peak_detached_mass.max(runtime.components.detached_mass);
            peak_extent = peak_extent.max(horizontal_extent(&runtime));
        }
        let held_cursor = Vec2::splat(0.5) + (start + Vec2::new(0.24, 0.0)) / Vec2::new(2.0, -2.0);
        let mut previous = runtime.render_state();
        let mut previous_velocities = [Vec2::ZERO; MAX_LIQUID_PARTICLES];
        let mut held_velocity_jump = 0.0_f32;
        let mut held_samples = 0.0_f32;
        for frame in 0..480 {
            runtime.update(
                &genome.body,
                &traits,
                physiology.pose,
                VisualMindInput::default(),
                &neutral_intent(),
                &SensorFrame {
                    cursor_position: held_cursor,
                    pointer_down: true,
                    pet_dragged: true,
                    ..SensorFrame::default()
                },
                &feedback,
                motion,
                ModalDeformation::default(),
                0.5,
                1.0 / 120.0,
            );
            let current = runtime.render_state();
            for (index, previous_velocity) in previous_velocities
                .iter_mut()
                .enumerate()
                .take(current.particle_count)
            {
                let velocity = (current.particles[index].position
                    - previous.particles[index].position)
                    * 120.0;
                if frame >= 240 {
                    held_velocity_jump += velocity.distance(*previous_velocity);
                    held_samples += 1.0;
                }
                *previous_velocity = velocity;
            }
            previous = current;
        }
        held_velocity_jump /= held_samples.max(1.0);
        let final_extent = horizontal_extent(&runtime);
        let detached_during_pull = runtime.components.detached_mass;
        eprintln!(
            "surface field: initial_extent={initial_extent:.4}, peak_extent={peak_extent:.4}, final_extent={final_extent:.4}, peak_detached={peak_detached_mass:.1}, held_detached={detached_during_pull:.1}, held_jump={held_velocity_jump:.5}"
        );
        assert!(runtime.components.component_count <= 4);
        assert!(final_extent > initial_extent + 0.04);
        assert!(detached_during_pull > 0.0);
        assert!(
            held_velocity_jump < 0.01,
            "held tear jitter={held_velocity_jump}"
        );
        assert!(
            runtime
                .idle_fragments
                .iter()
                .all(|fragment| !fragment.active)
        );
        assert_eq!(runtime.particle_count, particles::DEFAULT_LIQUID_PARTICLES);
        assert!(runtime.diagnostics.finite);

        for frame in 0..960 {
            runtime.update(
                &genome.body,
                &traits,
                physiology.pose,
                VisualMindInput::default(),
                &neutral_intent(),
                &SensorFrame {
                    pointer_released: frame == 0,
                    ..SensorFrame::default()
                },
                &feedback,
                motion,
                ModalDeformation::default(),
                0.5,
                1.0 / 120.0,
            );
        }
        assert_eq!(runtime.components.component_count, 1);
        assert_eq!(runtime.components.main_mass, runtime.particle_count as f32);
    }

    #[test]
    fn stationary_high_speed_clamp_hold_cannot_self_accelerate_returning_material() {
        let genome = Genome::from_seed(181);
        let traits = DerivedVisualTraits::from_genome(&genome);
        let physiology = VisualPhysiologyRuntime::new(genome.identity_seed, &traits);
        let mut tuning = PbfTuning {
            maximum_speed: 16.0,
            ..PbfTuning::default()
        };
        tuning.return_delay = 0.55;
        let mut runtime = LiquidMorphRuntime::new_with_tuning(genome.identity_seed, tuning);
        run(&mut runtime, &genome, 2.0);
        runtime.set_navigation_anchor_strength(0.0);
        let start = runtime.components.main_com;
        let feedback = BodyFeedback {
            world_position: Vec2::splat(0.5),
            ..BodyFeedback::default()
        };
        let motion = DropletMotion {
            world_to_body_scale: Vec2::new(2.0, -2.0),
            ..DropletMotion::default()
        };
        let dt = 1.0 / 120.0;
        let drag_frames = 22;
        for frame in 0..drag_frames {
            let progress = (frame + 1) as f32 / drag_frames as f32;
            let cursor_local = start + Vec2::new(progress * 0.45, 0.0);
            runtime.update(
                &genome.body,
                &traits,
                physiology.pose,
                VisualMindInput::default(),
                &neutral_intent(),
                &SensorFrame {
                    cursor_position: Vec2::splat(0.5) + cursor_local / Vec2::new(2.0, -2.0),
                    cursor_velocity: Vec2::new(1.25, 0.0),
                    pointer_down: true,
                    pointer_pressed: frame == 0,
                    pet_dragged: true,
                    ..SensorFrame::default()
                },
                &feedback,
                motion,
                ModalDeformation::default(),
                0.5,
                dt,
            );
        }

        let held_cursor = Vec2::splat(0.5) + (start + Vec2::new(0.45, 0.0)) / Vec2::new(2.0, -2.0);
        let mut first_half_peak = 0.0_f32;
        let mut second_half_peak = 0.0_f32;
        for frame in 0..960 {
            runtime.update(
                &genome.body,
                &traits,
                physiology.pose,
                VisualMindInput::default(),
                &neutral_intent(),
                &SensorFrame {
                    cursor_position: held_cursor,
                    pointer_down: true,
                    pet_dragged: true,
                    ..SensorFrame::default()
                },
                &feedback,
                motion,
                ModalDeformation::default(),
                0.5,
                dt,
            );
            if frame < 480 {
                first_half_peak = first_half_peak.max(runtime.diagnostics.maximum_speed);
            } else {
                second_half_peak = second_half_peak.max(runtime.diagnostics.maximum_speed);
            }
        }

        assert!(
            first_half_peak < 4.0,
            "unbounded first-half speed {first_half_peak}"
        );
        assert!(
            second_half_peak <= first_half_peak * 1.20,
            "stationary hold kept pumping energy: first={first_half_peak:.3}, second={second_half_peak:.3}"
        );
        let final_speed = runtime.diagnostics.maximum_speed;
        assert!(
            final_speed <= first_half_peak * 1.10 && final_speed < 1.5,
            "stationary field did not stay bounded: peak={first_half_peak:.3}, final={final_speed:.3}"
        );
        assert!(runtime.diagnostics.finite);
    }

    #[test]
    fn zero_g_center_field_moves_and_settles_without_origin_return() {
        let genome = Genome::from_seed(211);
        let traits = DerivedVisualTraits::from_genome(&genome);
        let physiology = VisualPhysiologyRuntime::new(genome.identity_seed, &traits);
        let mut runtime = LiquidMorphRuntime::new(genome.identity_seed);
        run(&mut runtime, &genome, 2.0);
        runtime.set_navigation_anchor_strength(0.0);
        let initial_com = runtime.components.main_com;
        let feedback = BodyFeedback {
            world_position: Vec2::splat(0.5),
            ..BodyFeedback::default()
        };
        let motion = DropletMotion {
            world_to_body_scale: Vec2::new(2.0, -2.0),
            ..DropletMotion::default()
        };
        let dt = 1.0 / 120.0;
        for frame in 0..240 {
            let progress = (frame + 1) as f32 / 240.0;
            let cursor_local = initial_com + Vec2::new(progress * 0.24, 0.0);
            runtime.update(
                &genome.body,
                &traits,
                physiology.pose,
                VisualMindInput::default(),
                &neutral_intent(),
                &SensorFrame {
                    cursor_position: Vec2::splat(0.5) + cursor_local / Vec2::new(2.0, -2.0),
                    cursor_velocity: Vec2::new(0.06, 0.0),
                    pointer_down: true,
                    pointer_pressed: frame == 0,
                    pet_dragged: true,
                    ..SensorFrame::default()
                },
                &feedback,
                motion,
                ModalDeformation::default(),
                0.5,
                dt,
            );
        }
        let release_com = runtime.components.main_com;
        let follow_ratio = (release_com.x - initial_com.x) / 0.24;
        // The pointer is a local gravity field, not a whole-body kinematic
        // handle. A center pull therefore transports some mass while the
        // permanent character field retains the rest.
        assert!(
            (0.15..=0.50).contains(&follow_ratio),
            "zero-G COM follow ratio {follow_ratio}; diagnostics={:?}",
            runtime.diagnostics
        );
        let release_speed = runtime.particles[..runtime.particle_count]
            .iter()
            .map(|particle| particle.velocity)
            .sum::<Vec2>()
            .length()
            / runtime.particle_count as f32;
        for frame in 0..240 {
            runtime.update(
                &genome.body,
                &traits,
                physiology.pose,
                VisualMindInput::default(),
                &neutral_intent(),
                &SensorFrame {
                    cursor_position: Vec2::splat(0.5) + Vec2::new(0.12, 0.0),
                    pointer_released: frame == 0,
                    ..SensorFrame::default()
                },
                &feedback,
                motion,
                ModalDeformation::default(),
                0.5,
                dt,
            );
        }
        let settled_com = runtime.components.main_com;
        let settled_speed = runtime.particles[..runtime.particle_count]
            .iter()
            .map(|particle| particle.velocity)
            .sum::<Vec2>()
            .length()
            / runtime.particle_count as f32;
        assert!(settled_com.x >= release_com.x - 0.02);
        assert!(settled_speed < release_speed * 0.35);
        assert!(settled_com.x > initial_com.x + 0.04);
        assert!(runtime.diagnostics.finite);
    }

    #[test]
    fn aggressive_pointer_pull_does_not_split_the_creature_into_two_bodies() {
        let genome = Genome::from_seed(307);
        let traits = DerivedVisualTraits::from_genome(&genome);
        let physiology = VisualPhysiologyRuntime::new(genome.identity_seed, &traits);
        let mut runtime = LiquidMorphRuntime::new(genome.identity_seed);
        run(&mut runtime, &genome, 2.0);
        runtime.set_navigation_anchor_strength(0.0);
        let start = runtime.components.main_com;
        let feedback = BodyFeedback {
            world_position: Vec2::splat(0.5),
            ..BodyFeedback::default()
        };
        let motion = DropletMotion {
            world_to_body_scale: Vec2::new(2.0, -2.0),
            ..DropletMotion::default()
        };
        let dt = 1.0 / 120.0;
        let drag_frames = 22;
        let mut minimum_main_mass = runtime.components.main_mass;
        for frame in 0..drag_frames {
            let progress = (frame + 1) as f32 / drag_frames as f32;
            let cursor_local = start + Vec2::new(progress * 0.45, 0.0);
            runtime.update(
                &genome.body,
                &traits,
                physiology.pose,
                VisualMindInput::default(),
                &neutral_intent(),
                &SensorFrame {
                    cursor_position: Vec2::splat(0.5) + cursor_local / Vec2::new(2.0, -2.0),
                    cursor_velocity: Vec2::new(1.25, 0.0),
                    pointer_down: true,
                    pointer_pressed: frame == 0,
                    pet_dragged: true,
                    ..SensorFrame::default()
                },
                &feedback,
                motion,
                ModalDeformation::default(),
                0.5,
                dt,
            );
            minimum_main_mass = minimum_main_mass.min(runtime.components.main_mass);
        }

        for frame in 0..240 {
            runtime.update(
                &genome.body,
                &traits,
                physiology.pose,
                VisualMindInput::default(),
                &neutral_intent(),
                &SensorFrame {
                    pointer_released: frame == 0,
                    ..SensorFrame::default()
                },
                &feedback,
                motion,
                ModalDeformation::default(),
                0.5,
                dt,
            );
            minimum_main_mass = minimum_main_mass.min(runtime.components.main_mass);
        }
        assert!(
            minimum_main_mass >= runtime.particle_count as f32 - 4.0,
            "ordinary pointer interaction caused macroscopic fission: min main mass={minimum_main_mass}, diagnostics={:?}",
            runtime.diagnostics,
        );
        assert_eq!(runtime.components.component_count, 1);
        assert!(runtime.diagnostics.finite);
    }

    #[test]
    fn body_lab_long_drag_remains_finite_without_authored_drag_fragments() {
        let genome = Genome::from_seed(0xD3A6);
        let traits = DerivedVisualTraits::from_genome(&genome);
        let physiology = VisualPhysiologyRuntime::new(genome.identity_seed, &traits);
        let mut tuning = PbfTuning {
            // Match the user's current Body Lab profile. Zero viscosity is the
            // adversarial end of the supported range and used to expose the
            // hard-island failure most clearly.
            viscosity: 0.0,
            ..PbfTuning::default()
        };
        tuning.idle_fragment_size = 0.0;
        let mut runtime = LiquidMorphRuntime::new_with_tuning(genome.identity_seed, tuning);
        runtime.cinematic_features = true;
        run(&mut runtime, &genome, 2.0);
        runtime.set_navigation_anchor_strength(0.0);
        // Body Lab's 1180x820 default window and 0.0..0.67 preview column,
        // transformed through the same particle projection as ProceduralBody.
        runtime.set_local_containment_bounds(Some((
            Vec2::new(-1.806, -1.167),
            Vec2::new(0.614, 1.167),
        )));

        let start = runtime.particles[..runtime.particle_count]
            .iter()
            .max_by(|a, b| a.position.x.total_cmp(&b.position.x))
            .map_or(runtime.components.main_com, |particle| particle.position);
        let initial_extent = horizontal_extent(&runtime);
        let feedback = BodyFeedback {
            world_position: Vec2::splat(0.5),
            ..BodyFeedback::default()
        };
        // Body Lab maps the full normalized window through its 1180x820 aspect.
        // A captured pointer may continue into the controls beyond x=0.67.
        let world_to_body_scale = Vec2::new(3.612, -2.5115);
        let motion = DropletMotion {
            world_to_body_scale,
            ..DropletMotion::default()
        };
        let dt = 1.0 / 120.0;
        let drag_frames = 420;
        let mut previous_cursor = start;
        let mut maximum_render_components = 1;
        let mut maximum_physical_components = 1;
        let mut minimum_physical_main_mass = runtime.particle_count as f32;
        let mut maximum_stretch = 1.0_f32;

        let mut maximum_density_ratio = 1.0_f32;
        let mut previous_core_velocity = Vec2::ZERO;
        let mut maximum_core_velocity_jump = 0.0_f32;
        for frame in 0..drag_frames {
            let progress = ((frame + 1).min(180)) as f32 / 180.0;
            let offset = Vec2::new(
                progress * 1.55,
                (progress * std::f32::consts::PI).sin() * 0.38,
            );
            let cursor_local = start + offset;
            let cursor_velocity = (cursor_local - previous_cursor) / dt / world_to_body_scale;
            previous_cursor = cursor_local;
            runtime.update(
                &genome.body,
                &traits,
                physiology.pose,
                VisualMindInput::default(),
                &neutral_intent(),
                &SensorFrame {
                    cursor_position: Vec2::splat(0.5) + cursor_local / world_to_body_scale,
                    cursor_velocity,
                    pointer_down: true,
                    pointer_pressed: frame == 0,
                    pet_dragged: true,
                    ..SensorFrame::default()
                },
                &feedback,
                motion,
                ModalDeformation::default(),
                0.5,
                dt,
            );

            let render_components = classify_render_components(
                &runtime.particles,
                runtime.particle_count,
                KERNEL_RADIUS * runtime.tuning.kernel_radius_scale,
                (runtime.tuning.iso_threshold - 0.018).clamp(0.06, 0.80),
            );
            maximum_render_components =
                maximum_render_components.max(render_components.summary.component_count);
            maximum_stretch = maximum_stretch.max(horizontal_extent(&runtime) / initial_extent);
            maximum_density_ratio = maximum_density_ratio.max(
                runtime.particles[..runtime.particle_count]
                    .iter()
                    .map(|particle| particle.density / runtime.rest_density.max(0.01))
                    .fold(0.0_f32, f32::max),
            );
            let mut physical_particles = runtime.particles;
            let component_spacing = component_graph_spacing(
                PARTICLE_SPACING * runtime.tuning.spacing_scale,
                KERNEL_RADIUS * runtime.tuning.kernel_radius_scale,
                runtime.tuning.iso_threshold,
                runtime.cinematic_features,
            );
            let physical = assign_components(
                &mut physical_particles,
                runtime.particle_count,
                component_spacing,
                runtime.tuning.component_link_radius_scale,
            );
            maximum_physical_components = maximum_physical_components.max(physical.component_count);
            minimum_physical_main_mass = minimum_physical_main_mass.min(physical.main_mass);
            let mean_core_velocity = runtime.particles[..runtime.particle_count]
                .iter()
                .map(|particle| particle.velocity)
                .sum::<Vec2>()
                / runtime.particle_count as f32;
            if frame > 0 {
                maximum_core_velocity_jump = maximum_core_velocity_jump
                    .max(mean_core_velocity.distance(previous_core_velocity));
            }
            previous_core_velocity = mean_core_velocity;
        }

        eprintln!(
            "Body Lab field probe: physical={maximum_physical_components}, render={maximum_render_components}, min_mass={minimum_physical_main_mass}, stretch={maximum_stretch:.3}, density={maximum_density_ratio:.3}, error={:.3}, core_dv={maximum_core_velocity_jump:.3}",
            runtime.diagnostics.density_error
        );

        assert!(
            maximum_stretch <= 3.0
                && maximum_density_ratio <= 3.0
                && maximum_core_velocity_jump <= 0.40
                && runtime
                    .idle_fragments
                    .iter()
                    .all(|fragment| !fragment.active),
            "Body Lab field became unstable: physical_components={maximum_physical_components}, render_components={maximum_render_components}, min_main_mass={minimum_physical_main_mass}, max_stretch={maximum_stretch:.3}, max_density_ratio={maximum_density_ratio:.3}, diagnostics={:?}",
            runtime.diagnostics,
        );
        assert!(runtime.diagnostics.finite);
    }

    #[test]
    fn zero_g_drag_response_is_stable_across_supported_fixed_frequencies() {
        let genome = Genome::from_seed(401);
        let traits = DerivedVisualTraits::from_genome(&genome);
        let physiology = VisualPhysiologyRuntime::new(genome.identity_seed, &traits);
        let feedback = BodyFeedback {
            world_position: Vec2::splat(0.5),
            ..BodyFeedback::default()
        };
        let motion = DropletMotion {
            world_to_body_scale: Vec2::new(2.0, -2.0),
            ..DropletMotion::default()
        };
        let mut stretch_ratios = [0.0_f32; 3];
        for (result, hz) in stretch_ratios.iter_mut().zip([30.0_f32, 60.0, 120.0]) {
            let mut runtime = LiquidMorphRuntime::new(genome.identity_seed);
            run(&mut runtime, &genome, 2.0);
            runtime.set_navigation_anchor_strength(0.0);
            let start = runtime.components.main_com;
            let initial_extent = horizontal_extent(&runtime);
            let frames = (hz * 2.0) as usize;
            let dt = 1.0 / hz;
            for frame in 0..frames {
                let progress = (frame + 1) as f32 / frames as f32;
                let cursor_local = start + Vec2::new(progress * 0.24, 0.0);
                runtime.update(
                    &genome.body,
                    &traits,
                    physiology.pose,
                    VisualMindInput::default(),
                    &neutral_intent(),
                    &SensorFrame {
                        cursor_position: Vec2::splat(0.5) + cursor_local / Vec2::new(2.0, -2.0),
                        cursor_velocity: Vec2::new(0.06, 0.0),
                        pointer_down: true,
                        pointer_pressed: frame == 0,
                        pet_dragged: true,
                        ..SensorFrame::default()
                    },
                    &feedback,
                    motion,
                    ModalDeformation::default(),
                    0.5,
                    dt,
                );
            }
            *result = horizontal_extent(&runtime) / initial_extent;
            assert!(runtime.diagnostics.finite);
        }
        let minimum = stretch_ratios.into_iter().fold(f32::INFINITY, f32::min);
        let maximum = stretch_ratios.into_iter().fold(f32::NEG_INFINITY, f32::max);
        assert!(
            (maximum - minimum) / maximum.max(1.0e-5) <= 0.10,
            "drag stretch changed with Hz: {stretch_ratios:?}"
        );
    }

    #[test]
    fn long_replay_preserves_particle_samples_and_never_produces_nan() {
        let genome = Genome::from_seed(99);
        let mut runtime = LiquidMorphRuntime::new(genome.identity_seed);
        run(&mut runtime, &genome, 120.0);
        assert!(runtime.diagnostics.finite);
        assert_eq!(
            runtime.diagnostics.particle_count,
            particles::DEFAULT_LIQUID_PARTICLES
        );
        assert!(runtime.diagnostics.main_mass + runtime.diagnostics.detached_mass > 71.9);
    }
}
