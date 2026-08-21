use glam::Vec2;
use lifecore::{
    BodyFeedback, BodyGenome, BodyIntent, CollisionEvent, LocomotionMode, SensorFrame, SurfaceRect,
};

#[derive(Debug, Clone, PartialEq)]
pub struct BodySimulation {
    pub feedback: BodyFeedback,
    wander_phase: f32,
}

impl BodySimulation {
    #[must_use]
    pub fn new(seed: u64) -> Self {
        Self {
            feedback: BodyFeedback::default(),
            wander_phase: seed as u32 as f32 / u32::MAX as f32 * std::f32::consts::TAU,
        }
    }

    pub fn fixed_update(
        &mut self,
        genome: &BodyGenome,
        intent: &BodyIntent,
        sensors: &SensorFrame,
        dt: f32,
    ) -> &BodyFeedback {
        let dt = dt.clamp(0.0, 1.0 / 30.0);
        self.wander_phase += dt * 0.73;
        self.feedback.collision = None;
        self.feedback.grounded = false;
        self.feedback.clinging = false;
        self.feedback.current_surface = None;

        let position = self.feedback.world_position;
        let mut target = intent
            .target_position
            .clamp(Vec2::splat(0.02), Vec2::splat(0.98));
        let mut desired_velocity;
        let desired_speed = intent.desired_speed.clamp(0.0, 1.2);
        match intent.locomotion {
            LocomotionMode::Hover => {
                target +=
                    Vec2::new(self.wander_phase.cos(), (self.wander_phase * 1.31).sin()) * 0.012;
                desired_velocity = arrive(position, target, desired_speed.max(0.06));
            }
            LocomotionMode::Seek => {
                desired_velocity = direction(position, target) * desired_speed;
            }
            LocomotionMode::Arrive => {
                desired_velocity = arrive(position, target, desired_speed);
            }
            LocomotionMode::Flee => {
                desired_velocity = -direction(position, target) * desired_speed;
            }
            LocomotionMode::Orbit => {
                let radial = position - target;
                let tangent = Vec2::new(-radial.y, radial.x).normalize_or_zero();
                let radius_error = 0.16 - radial.length();
                desired_velocity =
                    tangent * desired_speed + direction(position, target) * radius_error;
            }
            LocomotionMode::Wander => {
                desired_velocity = Vec2::new(
                    (self.wander_phase * 1.17).cos(),
                    (self.wander_phase * 0.83).sin(),
                )
                .normalize_or_zero()
                    * desired_speed.max(0.05);
            }
            LocomotionMode::SurfaceApproach
            | LocomotionMode::Landing
            | LocomotionMode::EdgeCling => {
                if let Some(surface) = select_surface(intent, &sensors.visible_surfaces) {
                    target = surface_target(surface, intent.locomotion);
                    desired_velocity = arrive(position, target, desired_speed.max(0.05));
                    let close = position.distance(target) < 0.018;
                    self.feedback.current_surface = Some(surface.id.clone());
                    self.feedback.grounded = close && intent.locomotion == LocomotionMode::Landing;
                    self.feedback.clinging =
                        close && intent.locomotion == LocomotionMode::EdgeCling;
                } else {
                    desired_velocity = arrive(position, target, desired_speed);
                }
            }
            LocomotionMode::Sleep | LocomotionMode::Cocoon => {
                desired_velocity = Vec2::ZERO;
            }
        }

        desired_velocity += avoid_surfaces(position, &sensors.visible_surfaces, intent);
        let maximum_acceleration = (0.55 / genome.inertia.max(0.2)).clamp(0.20, 2.0);
        let acceleration = ((desired_velocity - self.feedback.velocity) * 8.0)
            .clamp_length_max(maximum_acceleration);
        self.feedback.acceleration = acceleration;
        self.feedback.velocity = (self.feedback.velocity + acceleration * dt).clamp_length_max(1.2);
        self.feedback.world_position += self.feedback.velocity * dt;

        for axis in 0..2 {
            let value = self.feedback.world_position[axis];
            if !(0.015..=0.985).contains(&value) {
                let normal = if value < 0.015 { 1.0 } else { -1.0 };
                self.feedback.world_position[axis] = value.clamp(0.015, 0.985);
                let intensity = self.feedback.velocity[axis].abs().clamp(0.0, 1.0);
                self.feedback.velocity[axis] *= -0.22;
                let mut collision_normal = Vec2::ZERO;
                collision_normal[axis] = normal;
                self.feedback.collision = Some(CollisionEvent {
                    normal: collision_normal,
                    intensity,
                });
            }
        }
        self.feedback.cursor_contact = position.distance(sensors.cursor_position) < 0.04;
        self.feedback.locomotion_completed = self.feedback.world_position.distance(target) < 0.025
            && self.feedback.velocity.length() < 0.035;
        self.feedback.pose_error =
            if self.feedback.world_position.is_finite() && self.feedback.velocity.is_finite() {
                0.0
            } else {
                self.feedback = BodyFeedback::default();
                1.0
            };
        &self.feedback
    }
}

fn direction(from: Vec2, to: Vec2) -> Vec2 {
    (to - from).normalize_or_zero()
}

fn arrive(position: Vec2, target: Vec2, speed: f32) -> Vec2 {
    let delta = target - position;
    let distance = delta.length();
    delta.normalize_or_zero() * speed * (distance / 0.18).clamp(0.0, 1.0)
}

fn select_surface<'a>(intent: &BodyIntent, surfaces: &'a [SurfaceRect]) -> Option<&'a SurfaceRect> {
    intent
        .target_surface
        .as_ref()
        .and_then(|target| surfaces.iter().find(|surface| surface.id == *target))
        .or_else(|| surfaces.first())
}

fn surface_target(surface: &SurfaceRect, mode: LocomotionMode) -> Vec2 {
    match mode {
        LocomotionMode::EdgeCling => Vec2::new(
            surface.rect.minimum.x,
            (surface.rect.minimum.y + surface.rect.maximum.y) * 0.5,
        ),
        _ => Vec2::new(
            (surface.rect.minimum.x + surface.rect.maximum.x) * 0.5,
            surface.rect.minimum.y,
        ),
    }
}

fn avoid_surfaces(position: Vec2, surfaces: &[SurfaceRect], intent: &BodyIntent) -> Vec2 {
    if matches!(
        intent.locomotion,
        LocomotionMode::SurfaceApproach | LocomotionMode::Landing | LocomotionMode::EdgeCling
    ) {
        return Vec2::ZERO;
    }
    surfaces.iter().fold(Vec2::ZERO, |avoidance, surface| {
        let closest = position.clamp(surface.rect.minimum, surface.rect.maximum);
        let delta = position - closest;
        let distance = delta.length();
        if distance < 0.035 {
            avoidance + delta.normalize_or_zero() * (0.035 - distance) * 8.0
        } else {
            avoidance
        }
    })
}
