use glam::{Quat, Vec3};
use lifecore::BodyGenome;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BodyPart {
    Root,
    Torso,
    Head,
    EyeLeft,
    EyeRight,
    EyelidLeft,
    EyelidRight,
    Mouth,
    WingLeft,
    WingRight,
    ForeLimbLeft,
    ForeLimbRight,
    Tail,
    Crest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BodyNode {
    pub part: BodyPart,
    pub parent: Option<usize>,
    pub local_position: Vec3,
    pub local_rotation: Quat,
    pub local_scale: Vec3,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BodyGraph {
    pub nodes: Vec<BodyNode>,
}

impl BodyGraph {
    #[must_use]
    pub fn from_genome(genome: &BodyGenome) -> Self {
        let mut nodes = Vec::with_capacity(14 + usize::from(genome.tail_segments));
        let root = push(&mut nodes, BodyPart::Root, None, Vec3::ZERO, Vec3::ONE);
        let torso = push(
            &mut nodes,
            BodyPart::Torso,
            Some(root),
            Vec3::ZERO,
            Vec3::new(
                genome.body_width,
                genome.body_length,
                genome.body_width * 0.72,
            ),
        );
        let head = push(
            &mut nodes,
            BodyPart::Head,
            Some(torso),
            Vec3::new(0.0, genome.body_length * 0.42, 0.04),
            Vec3::splat(genome.head_ratio),
        );
        for (part, x) in [
            (BodyPart::EyeLeft, -genome.eye_spacing * 0.5),
            (BodyPart::EyeRight, genome.eye_spacing * 0.5),
        ] {
            push(
                &mut nodes,
                part,
                Some(head),
                Vec3::new(x, 0.05, genome.head_ratio * 0.45),
                Vec3::splat(genome.eye_size),
            );
        }
        for part in [BodyPart::EyelidLeft, BodyPart::EyelidRight, BodyPart::Mouth] {
            push(&mut nodes, part, Some(head), Vec3::ZERO, Vec3::ONE);
        }
        for (part, side) in [(BodyPart::WingLeft, -1.0), (BodyPart::WingRight, 1.0)] {
            push(
                &mut nodes,
                part,
                Some(torso),
                Vec3::new(side * genome.body_width * 0.42, 0.08, -0.02),
                Vec3::new(genome.wing_span, genome.wing_aspect, 1.0),
            );
        }
        for (part, side) in [
            (BodyPart::ForeLimbLeft, -1.0),
            (BodyPart::ForeLimbRight, 1.0),
        ] {
            push(
                &mut nodes,
                part,
                Some(torso),
                Vec3::new(
                    side * genome.body_width * 0.30,
                    -genome.body_length * 0.22,
                    0.05,
                ),
                Vec3::new(
                    genome.limb_thickness,
                    genome.limb_length,
                    genome.limb_thickness,
                ),
            );
        }
        let mut parent = torso;
        for segment in 0..genome.tail_segments {
            parent = push(
                &mut nodes,
                BodyPart::Tail,
                Some(parent),
                Vec3::new(
                    0.0,
                    -genome.tail_length / f32::from(genome.tail_segments),
                    -0.04,
                ),
                Vec3::splat(genome.tail_thickness * (1.0 - f32::from(segment) * 0.055).max(0.45)),
            );
        }
        if genome.crest_size > 0.01 {
            push(
                &mut nodes,
                BodyPart::Crest,
                Some(head),
                Vec3::new(0.0, genome.head_ratio * 0.45, 0.0),
                Vec3::splat(genome.crest_size),
            );
        }
        Self { nodes }
    }

    #[must_use]
    pub fn is_valid(&self) -> bool {
        self.nodes.iter().enumerate().all(|(index, node)| {
            node.parent.is_none_or(|parent| parent < index)
                && node.local_position.is_finite()
                && node.local_rotation.is_finite()
                && node.local_scale.is_finite()
                && node.local_scale.min_element() > 0.0
        })
    }
}

fn push(
    nodes: &mut Vec<BodyNode>,
    part: BodyPart,
    parent: Option<usize>,
    local_position: Vec3,
    local_scale: Vec3,
) -> usize {
    let index = nodes.len();
    nodes.push(BodyNode {
        part,
        parent,
        local_position,
        local_rotation: Quat::IDENTITY,
        local_scale,
    });
    index
}
