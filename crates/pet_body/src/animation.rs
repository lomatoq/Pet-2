use glam::{Quat, Vec3};
use lifecore::{BodyIntent, PoseIntent};

use crate::{BodyGraph, BodyPart};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct JointState {
    pub rotation: Quat,
    pub target_rotation: Quat,
    pub angular_velocity: Vec3,
    pub position: Vec3,
    pub target_position: Vec3,
    pub velocity: Vec3,
}

impl JointState {
    fn from_graph(position: Vec3, rotation: Quat) -> Self {
        Self {
            rotation,
            target_rotation: rotation,
            angular_velocity: Vec3::ZERO,
            position,
            target_position: position,
            velocity: Vec3::ZERO,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct AnimationRuntime {
    pub joints: Vec<JointState>,
    pub time: f32,
    pub blink: f32,
    pub gaze_offset: Vec3,
    seed_phase: f32,
    blink_clock: f32,
    next_blink: f32,
}

impl AnimationRuntime {
    #[must_use]
    pub fn new(graph: &BodyGraph, seed: u64) -> Self {
        let seed_phase = (seed as u32 as f32 / u32::MAX as f32) * std::f32::consts::TAU;
        Self {
            joints: graph
                .nodes
                .iter()
                .map(|node| JointState::from_graph(node.local_position, node.local_rotation))
                .collect(),
            time: 0.0,
            blink: 0.0,
            gaze_offset: Vec3::ZERO,
            seed_phase,
            blink_clock: 0.0,
            next_blink: 2.4 + seed_phase.sin().abs() * 2.8,
        }
    }

    pub fn update(&mut self, graph: &BodyGraph, intent: &BodyIntent, arousal: f32, dt: f32) {
        let dt = dt.clamp(0.0, 0.05);
        self.time += dt;
        self.blink_clock += dt;
        if self.blink_clock >= self.next_blink {
            let phase = ((self.blink_clock - self.next_blink) / 0.18).clamp(0.0, 1.0);
            self.blink = (phase * std::f32::consts::PI).sin().powi(2);
            if phase >= 1.0 {
                self.blink_clock = 0.0;
                self.next_blink =
                    2.2 + 3.6 * (0.5 + 0.5 * (self.time * 0.37 + self.seed_phase).sin());
            }
        } else {
            self.blink = 0.0;
        }

        let gaze = intent
            .gaze_target
            .map(|target| {
                (target - glam::Vec2::splat(0.5))
                    .clamp(glam::Vec2::splat(-0.5), glam::Vec2::splat(0.5))
            })
            .unwrap_or(glam::Vec2::ZERO);
        let gaze_target = Vec3::new(gaze.x, -gaze.y, 0.0) * 0.08;
        self.gaze_offset += (gaze_target - self.gaze_offset) * (1.0 - (-12.0 * dt).exp());

        for (index, (joint, node)) in self.joints.iter_mut().zip(&graph.nodes).enumerate() {
            joint.target_position = node.local_position;
            joint.target_rotation = node.local_rotation;
            let idle = (self.time * 1.37 + self.seed_phase + index as f32 * 0.23).sin();
            match node.part {
                BodyPart::Root => {
                    joint.target_position.y += idle * (0.008 + arousal * 0.008);
                }
                BodyPart::Torso => {
                    joint.target_rotation = Quat::from_rotation_z(idle * 0.025);
                    joint.target_position.z += (self.time * 2.0).sin() * 0.005;
                }
                BodyPart::Head => {
                    let curious = if intent.pose == PoseIntent::Curious {
                        0.16
                    } else {
                        0.0
                    };
                    joint.target_rotation = Quat::from_rotation_z(
                        curious + (self.time * 0.7 + self.seed_phase).sin() * 0.025,
                    );
                }
                BodyPart::WingLeft | BodyPart::WingRight => {
                    let side = if node.part == BodyPart::WingLeft {
                        -1.0
                    } else {
                        1.0
                    };
                    let flap = (self.time * (5.0 + arousal * 7.0) + side * 0.08).sin();
                    joint.target_rotation =
                        Quat::from_rotation_y(side * flap * (0.10 + arousal * 0.24));
                }
                BodyPart::Tail => {
                    joint.target_rotation = Quat::from_rotation_z(
                        (self.time * 1.8 - index as f32 * 0.31 + self.seed_phase).sin()
                            * (0.05 + arousal * 0.08),
                    );
                }
                BodyPart::ForeLimbLeft | BodyPart::ForeLimbRight => {
                    let fold = match intent.pose {
                        PoseIntent::Compact | PoseIntent::Sleeping | PoseIntent::Cocoon => 0.42,
                        PoseIntent::Landing | PoseIntent::Clinging => -0.22,
                        _ => 0.04,
                    };
                    joint.target_rotation = Quat::from_rotation_x(fold);
                }
                BodyPart::EyeLeft | BodyPart::EyeRight => {
                    joint.target_position += self.gaze_offset;
                }
                _ => {}
            }
            spring_position(joint, dt, 18.0);
            spring_rotation(joint, dt, 15.0);
        }
    }
}

fn spring_position(joint: &mut JointState, dt: f32, frequency: f32) {
    let acceleration = (joint.target_position - joint.position) * frequency * frequency
        - joint.velocity * (2.0 * frequency);
    joint.velocity += acceleration * dt;
    joint.position += joint.velocity * dt;
}

fn spring_rotation(joint: &mut JointState, dt: f32, frequency: f32) {
    let mut target = joint.target_rotation;
    if joint.rotation.dot(target) < 0.0 {
        target = -target;
    }
    let delta = target * joint.rotation.conjugate();
    let (axis, angle) = delta.to_axis_angle();
    let error = if axis.is_finite() && angle.is_finite() {
        axis * angle
    } else {
        Vec3::ZERO
    };
    let acceleration = error * frequency * frequency - joint.angular_velocity * (2.0 * frequency);
    joint.angular_velocity += acceleration * dt;
    let step = joint.angular_velocity * dt;
    let length = step.length();
    if length > f32::EPSILON {
        joint.rotation =
            (Quat::from_axis_angle(step / length, length) * joint.rotation).normalize();
    }
}
