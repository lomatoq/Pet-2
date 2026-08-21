//! Procedural body generation, animation, locomotion, hit testing, and rendering.
//! This crate intentionally contains no platform-specific APIs.

mod animation;
mod expression;
mod graph;
mod locomotion;
mod mesh;
mod renderer;

pub use animation::{AnimationRuntime, JointState};
pub use expression::ExpressionRuntime;
pub use graph::{BodyGraph, BodyNode, BodyPart};
pub use locomotion::BodySimulation;
pub use mesh::{MeshError, MeshVertex, ProceduralMesh, ProjectedHitShape};
pub use renderer::{RenderOutcome, RenderParameters, Renderer, RendererError};

use glam::Vec2;
use lifecore::{BodyFeedback, BodyIntent, Genome, SensorFrame};

#[derive(Debug, Clone, PartialEq)]
pub struct ProceduralBody {
    pub graph: BodyGraph,
    pub mesh: ProceduralMesh,
    pub hit_shape: ProjectedHitShape,
    pub animation: AnimationRuntime,
    pub expression: ExpressionRuntime,
    pub simulation: BodySimulation,
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
            simulation: BodySimulation::new(genome.identity_seed),
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

    pub fn animation_update(&mut self, intent: &BodyIntent, arousal: f32, dt: f32) {
        self.animation.update(&self.graph, intent, arousal, dt);
        self.expression
            .update(intent.expression, self.animation.blink, dt);
    }

    #[must_use]
    pub fn projected_hit_test(&self, normalized_window_point: Vec2) -> bool {
        self.hit_shape.contains(normalized_window_point)
    }

    #[must_use]
    pub fn render_parameters(&self, genome: &Genome, arousal: f32) -> RenderParameters {
        RenderParameters {
            time: self.animation.time,
            arousal,
            blink: self
                .expression
                .current
                .blink_left
                .max(self.expression.current.blink_right),
            glow: self.expression.current.body_glow * genome.body.bioluminescence,
            pattern_scale: genome.body.pattern_scale,
            pattern_contrast: genome.body.pattern_contrast,
            pattern_seed: genome.body.pattern_seed,
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
            body.animation_update(&intent, 0.4, 1.0 / 120.0);
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
