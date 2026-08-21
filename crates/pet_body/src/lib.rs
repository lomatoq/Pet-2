//! Procedural body generation, animation, locomotion, hit testing, and rendering.
//! This crate intentionally contains no platform-specific APIs.

mod animation;
mod embodiment;
mod expression;
mod graph;
mod locomotion;
mod mesh;
mod renderer;

pub use animation::{AnimationRuntime, JointState};
pub use embodiment::{EmbodiedPose, EmbodiedRuntime, GazeMode, VoiceVisualState};
pub use expression::ExpressionRuntime;
pub use graph::{BodyGraph, BodyNode, BodyPart};
pub use locomotion::BodySimulation;
pub use mesh::{MeshError, MeshVertex, ProceduralMesh, ProjectedHitShape};
pub use renderer::{OcclusionMode, RenderOutcome, RenderParameters, Renderer, RendererError};

use glam::Vec2;
use lifecore::{AffectState, BodyFeedback, BodyGenome, BodyIntent, Genome, PoseIntent, SensorFrame};

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
    occlusion_mode: OcclusionMode,
    occlusion_edge: f32,
}

impl ProceduralBody {
    pub fn generate(genome: &Genome) -> Result<Self, MeshError> {
        let graph = BodyGraph::from_genome(&genome.body);
        let mesh = ProceduralMesh::generate(&genome.body)?;
        let hit_shape = ProjectedHitShape::from_genome(&genome.body);
        let animation = AnimationRuntime::new(&graph, genome.identity_seed);
        Ok(Self {
            graph,
            mesh,
            hit_shape,
            animation,
            expression: ExpressionRuntime::default(),
            embodiment: EmbodiedRuntime::new(genome.identity_seed),
            simulation: BodySimulation::new(genome.identity_seed),
            body_genome: genome.body.clone(),
            occlusion_mode: OcclusionMode::Front,
            occlusion_edge: 0.0,
        })
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

    /// Compatibility update for headless callers that do not yet provide the full
    /// embodied context. Desktop runtimes should call `embodied_update`.
    pub fn animation_update(&mut self, intent: &BodyIntent, arousal: f32, dt: f32) {
        let affect = AffectState {
            arousal: arousal.clamp(0.0, 1.0),
            ..AffectState::default()
        };
        let audio = pet_audio::global_visual_feedback();
        self.embodied_update(
            intent,
            &SensorFrame::default(),
            affect,
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
        voice: VoiceVisualState,
        dt: f32,
    ) {
        self.animation.update(&self.graph, intent, affect.arousal, dt);
        self.expression
            .update(intent.expression, self.animation.blink, dt);
        self.embodiment.update(
            &self.body_genome,
            intent,
            sensors,
            &self.simulation.feedback,
            affect,
            self.expression.current,
            voice,
            dt,
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

    #[must_use]
    pub fn projected_hit_test(&self, normalized_window_point: Vec2) -> bool {
        self.hit_shape.contains(normalized_window_point)
    }

    #[must_use]
    pub fn render_parameters(&self, genome: &Genome, arousal: f32) -> RenderParameters {
        let pose = self.embodiment.pose;
        RenderParameters {
            time: self.animation.time,
            arousal,
            glow: self.expression.current.body_glow * genome.body.bioluminescence,
            body_length: genome.body.body_length,
            body_width: genome.body.body_width,
            body_roundness: genome.body.body_roundness,
            head_ratio: genome.body.head_ratio,
            eye_size: genome.body.eye_size,
            eye_spacing: genome.body.eye_spacing,
            pupil_ratio: genome.body.pupil_ratio,
            softness: genome.body.softness,
            tail_length: genome.body.tail_length,
            tail_thickness: genome.body.tail_thickness,
            ear_fin_size: genome.body.ear_fin_size,
            primary_hsv: genome.body.primary_color_hsv,
            secondary_hsv: genome.body.secondary_color_hsv,
            glow_hsv: genome.body.glow_color_hsv,
            gaze: pose.gaze,
            vergence: pose.vergence,
            pupil_size: pose.pupil_size,
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
            pattern_scale: genome.body.pattern_scale,
            pattern_contrast: genome.body.pattern_contrast,
            pattern_seed: genome.body.pattern_seed,
            occlusion_mode: self.occlusion_mode,
            occlusion_edge: self.occlusion_edge,
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
        let module = naga::front::wgsl::parse_str(include_str!("pet.wgsl"))
            .expect("procedural pet WGSL parses");
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .expect("procedural pet WGSL validates");
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
}
