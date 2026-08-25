use glam::Vec2;
use lifecore::{BodyFeedback, BodyGenome, BodyIntent, LocomotionMode, SensorFrame, SurfaceRect};

#[derive(Debug, Clone, PartialEq)]
pub struct BodySimulation {
    pub feedback: BodyFeedback,
    wander_phase: f32,
    /// Physical desktop extent used for locomotion math. `BodyFeedback` remains
    /// the normalized persistence/LifeCore mirror, but direction, arrival and
    /// acceleration are evaluated in isotropic screen pixels.
    motion_space_pixels: Option<Vec2>,
    /// LifeCore may legitimately reconsider a waypoint at a brain tick. The
    /// body must not turn that discrete decision into a screen-space velocity
    /// discontinuity, so the embodied target has its own continuous state.
    embodied_target: Option<Vec2>,
}

impl BodySimulation {
    #[must_use]
    pub fn new(seed: u64) -> Self {
        Self {
            feedback: BodyFeedback::default(),
            wander_phase: seed as u32 as f32 / u32::MAX as f32 * std::f32::consts::TAU,
            motion_space_pixels: None,
            embodied_target: None,
        }
    }

    pub fn set_motion_space_pixels(&mut self, size: Vec2) {
        if size.is_finite() && size.min_element() > 1.0 {
            self.motion_space_pixels = Some(size);
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

        let scale = self.motion_space_pixels.unwrap_or(Vec2::ONE);
        let physical_motion = self.motion_space_pixels.is_some();
        let reference_span = if physical_motion {
            // Author speed in stable physical pixels instead of letting very tall
            // or ultrawide desktops multiply a normalized LifeCore request.
            scale.min_element().clamp(1.0, 1_152.0)
        } else {
            1.0
        };
        let position = self.feedback.world_position * scale;
        let mut requested_target = intent
            .target_position
            .clamp(Vec2::splat(0.02), Vec2::splat(0.98));
        if matches!(
            intent.locomotion,
            LocomotionMode::SurfaceApproach | LocomotionMode::Landing | LocomotionMode::EdgeCling
        ) && let Some(surface) = select_surface(intent, &sensors.visible_surfaces)
        {
            requested_target = surface_target(surface, intent.locomotion);
        }
        let target_response = match intent.locomotion {
            LocomotionMode::Flee => 4.8,
            LocomotionMode::Seek
            | LocomotionMode::SurfaceApproach
            | LocomotionMode::Landing
            | LocomotionMode::EdgeCling => 3.4,
            LocomotionMode::Hover | LocomotionMode::Arrive | LocomotionMode::Orbit => 2.4,
            LocomotionMode::Wander => 1.65,
            LocomotionMode::Sleep | LocomotionMode::Cocoon => 6.0,
        };
        let target_alpha = 1.0 - (-target_response * dt).exp();
        let embodied_target = self.embodied_target.get_or_insert(requested_target);
        *embodied_target = embodied_target.lerp(requested_target, target_alpha);
        let mut target = *embodied_target * scale;
        let mut desired_velocity;
        let speed_cap = match intent.locomotion {
            LocomotionMode::Flee => 0.10,
            LocomotionMode::Seek => 0.09,
            LocomotionMode::SurfaceApproach
            | LocomotionMode::Landing
            | LocomotionMode::EdgeCling => 0.075,
            _ => 0.085,
        };
        let desired_speed = intent.desired_speed.clamp(0.0, speed_cap) * reference_span;
        match intent.locomotion {
            LocomotionMode::Hover => {
                target += Vec2::new(self.wander_phase.cos(), (self.wander_phase * 1.31).sin())
                    * (0.012 * reference_span);
                desired_velocity = arrive(
                    position,
                    target,
                    desired_speed.max(0.06 * reference_span),
                    0.18 * reference_span,
                );
            }
            LocomotionMode::Seek => {
                desired_velocity = direction(position, target) * desired_speed;
            }
            LocomotionMode::Arrive => {
                desired_velocity = arrive(position, target, desired_speed, 0.18 * reference_span);
            }
            LocomotionMode::Flee => {
                desired_velocity = -direction(position, target) * desired_speed;
            }
            LocomotionMode::Orbit => {
                let radial = position - target;
                let tangent = Vec2::new(-radial.y, radial.x).normalize_or_zero();
                let radius_error = 0.16 * reference_span - radial.length();
                desired_velocity =
                    tangent * desired_speed + direction(position, target) * radius_error * 0.9;
            }
            LocomotionMode::Wander => {
                // Wander has an authored waypoint from LifeCore. The old implementation
                // ignored it and drove a perpetual screen-wide oscillator that bounced
                // off the desktop bounds. Arrive gives each bout a readable destination
                // and naturally settles there before LifeCore chooses another one.
                desired_velocity = arrive(
                    position,
                    target,
                    desired_speed.max(0.05 * reference_span),
                    0.18 * reference_span,
                );
            }
            LocomotionMode::SurfaceApproach
            | LocomotionMode::Landing
            | LocomotionMode::EdgeCling => {
                if let Some(surface) = select_surface(intent, &sensors.visible_surfaces) {
                    desired_velocity = arrive(
                        position,
                        target,
                        desired_speed.max(0.05 * reference_span),
                        0.18 * reference_span,
                    );
                    let close = position.distance(target) < 0.018 * reference_span;
                    self.feedback.current_surface = Some(surface.id.clone());
                    self.feedback.grounded = close && intent.locomotion == LocomotionMode::Landing;
                    self.feedback.clinging =
                        close && intent.locomotion == LocomotionMode::EdgeCling;
                } else {
                    desired_velocity =
                        arrive(position, target, desired_speed, 0.18 * reference_span);
                }
            }
            LocomotionMode::Sleep | LocomotionMode::Cocoon => {
                desired_velocity = Vec2::ZERO;
            }
        }

        desired_velocity += avoid_surfaces(
            position,
            scale,
            reference_span,
            &sensors.visible_surfaces,
            intent,
        );
        let velocity = self.feedback.velocity * scale;
        let maximum_acceleration =
            (0.42 / genome.inertia.max(0.2)).clamp(0.16, 1.45) * reference_span;
        let acceleration =
            ((desired_velocity - velocity) * 5.2).clamp_length_max(maximum_acceleration);
        let velocity = (velocity + acceleration * dt).clamp_length_max(1.2 * reference_span);
        let position = position + velocity * dt;
        self.feedback.acceleration = acceleration / scale;
        self.feedback.velocity = velocity / scale;
        self.feedback.world_position = position / scale;

        // Physical monitor-union boundaries are applied by the desktop host. A
        // normalized virtual-rectangle clamp cannot represent gaps or L layouts.
        self.feedback.cursor_contact =
            position.distance(sensors.cursor_position * scale) < 0.04 * reference_span;
        self.feedback.locomotion_completed = position.distance(target) < 0.025 * reference_span
            && velocity.length() < 0.035 * reference_span;
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

fn arrive(position: Vec2, target: Vec2, speed: f32, arrival_radius: f32) -> Vec2 {
    let delta = target - position;
    let distance = delta.length();
    delta.normalize_or_zero() * speed * (distance / arrival_radius.max(1.0e-5)).clamp(0.0, 1.0)
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

fn avoid_surfaces(
    position: Vec2,
    scale: Vec2,
    reference_span: f32,
    surfaces: &[SurfaceRect],
    intent: &BodyIntent,
) -> Vec2 {
    if matches!(
        intent.locomotion,
        LocomotionMode::SurfaceApproach | LocomotionMode::Landing | LocomotionMode::EdgeCling
    ) {
        return Vec2::ZERO;
    }
    surfaces.iter().fold(Vec2::ZERO, |avoidance, surface| {
        let closest = position.clamp(surface.rect.minimum * scale, surface.rect.maximum * scale);
        let delta = position - closest;
        let distance = delta.length();
        let clearance = 0.035 * reference_span;
        if distance < clearance {
            avoidance + delta.normalize_or_zero() * (clearance - distance) * 8.0
        } else {
            avoidance
        }
    })
}

#[cfg(test)]
mod tests {
    use lifecore::{ExpressionState, Genome, PoseIntent};

    use super::*;

    #[test]
    fn wander_arrives_at_its_waypoint_instead_of_hitting_desktop_edges() {
        let genome = Genome::from_seed(73);
        let mut simulation = BodySimulation::new(73);
        simulation.feedback.world_position = Vec2::new(0.28, 0.36);
        let target = Vec2::new(0.68, 0.62);
        let intent = BodyIntent {
            locomotion: LocomotionMode::Wander,
            target_position: target,
            target_surface: None,
            desired_speed: 0.11,
            facing_direction: 1.0,
            gaze_target: None,
            pose: PoseIntent::Curious,
            expression: ExpressionState::default(),
            interaction_target: None,
        };
        let sensors = SensorFrame::default();

        for _ in 0..960 {
            simulation.fixed_update(&genome.body, &intent, &sensors, 1.0 / 120.0);
        }

        assert!(simulation.feedback.world_position.distance(target) < 0.025);
        assert!(simulation.feedback.velocity.length() < 0.035);
        assert!(simulation.feedback.collision.is_none());
        assert!((0.02..=0.98).contains(&simulation.feedback.world_position.x));
        assert!((0.02..=0.98).contains(&simulation.feedback.world_position.y));
    }

    #[test]
    fn physical_speed_is_not_multiplied_by_ultrawide_width() {
        let genome = Genome::from_seed(91);
        let intent = BodyIntent {
            locomotion: LocomotionMode::Seek,
            target_position: Vec2::new(0.92, 0.5),
            target_surface: None,
            desired_speed: 0.08,
            facing_direction: 1.0,
            gaze_target: None,
            pose: PoseIntent::Curious,
            expression: ExpressionState::default(),
            interaction_target: None,
        };
        let mut regular = BodySimulation::new(91);
        let mut ultrawide = BodySimulation::new(91);
        regular.feedback.world_position = Vec2::new(0.08, 0.5);
        ultrawide.feedback.world_position = Vec2::new(0.08, 0.5);
        regular.set_motion_space_pixels(Vec2::new(1_920.0, 1_080.0));
        ultrawide.set_motion_space_pixels(Vec2::new(3_440.0, 1_080.0));

        for _ in 0..120 {
            regular.fixed_update(&genome.body, &intent, &SensorFrame::default(), 1.0 / 120.0);
            ultrawide.fixed_update(&genome.body, &intent, &SensorFrame::default(), 1.0 / 120.0);
        }

        let regular_speed = regular.feedback.velocity.x * 1_920.0;
        let ultrawide_speed = ultrawide.feedback.velocity.x * 3_440.0;
        assert!((regular_speed - ultrawide_speed).abs() < 0.5);
        assert!((80.0..=90.0).contains(&regular_speed));
    }

    #[test]
    fn discrete_waypoint_change_cannot_reverse_screen_velocity_in_one_tick() {
        let genome = Genome::from_seed(92);
        let mut simulation = BodySimulation::new(92);
        simulation.feedback.world_position = Vec2::new(0.45, 0.5);
        simulation.set_motion_space_pixels(Vec2::new(3_440.0, 1_440.0));
        let mut intent = BodyIntent {
            locomotion: LocomotionMode::Wander,
            target_position: Vec2::new(0.85, 0.5),
            target_surface: None,
            desired_speed: 0.08,
            facing_direction: 1.0,
            gaze_target: None,
            pose: PoseIntent::Curious,
            expression: ExpressionState::default(),
            interaction_target: None,
        };
        for _ in 0..90 {
            simulation.fixed_update(&genome.body, &intent, &SensorFrame::default(), 1.0 / 120.0);
        }
        let before = simulation.feedback.velocity.x * 3_440.0;
        assert!(before > 40.0);

        intent.target_position = Vec2::new(0.15, 0.5);
        simulation.fixed_update(&genome.body, &intent, &SensorFrame::default(), 1.0 / 120.0);
        let after = simulation.feedback.velocity.x * 3_440.0;
        assert!(
            after > 0.0,
            "waypoint switch reversed {before} px/s to {after} px/s"
        );
        assert!((after - before).abs() < 8.0);
    }
}
