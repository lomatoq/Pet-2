use glam::Vec2;
use lifecore::{BodyFeedback, BodyGenome, BodyIntent, LocomotionMode, SensorFrame, SurfaceRect};

const BODY_SCREEN_GRAVITY: f32 = 0.16;

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
        let was_grounded = self.feedback.grounded;
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
            LocomotionMode::Flee => 8.0,
            LocomotionMode::Seek
            | LocomotionMode::SurfaceApproach
            | LocomotionMode::Landing
            | LocomotionMode::EdgeCling => 6.2,
            LocomotionMode::Orbit => 4.4,
            LocomotionMode::Hover | LocomotionMode::Arrive => 2.8,
            LocomotionMode::Wander => 1.65,
            LocomotionMode::Sleep | LocomotionMode::Cocoon => 6.0,
        };
        let target_alpha = 1.0 - (-target_response * dt).exp();
        let embodied_target = self.embodied_target.get_or_insert(requested_target);
        *embodied_target = embodied_target.lerp(requested_target, target_alpha);
        let mut target = *embodied_target * scale;
        let mut desired_velocity;
        let expression_energy = if intent.expression.body_glow.is_finite()
            && intent.expression.pupil_size.is_finite()
        {
            (intent.expression.body_glow * 0.60 + intent.expression.pupil_size * 0.40)
                .clamp(0.0, 1.0)
        } else {
            0.32
        };
        let purposeful_effort = ((intent.desired_speed - 0.12) / 0.40).clamp(0.0, 1.0);
        let base_speed_cap = match intent.locomotion {
            LocomotionMode::Flee => 0.11 + purposeful_effort * 0.21,
            LocomotionMode::Seek => 0.09 + purposeful_effort * 0.21,
            LocomotionMode::SurfaceApproach
            | LocomotionMode::Landing
            | LocomotionMode::EdgeCling => 0.080 + purposeful_effort * 0.070,
            LocomotionMode::Orbit => 0.095 + purposeful_effort * 0.165,
            LocomotionMode::Arrive => 0.090 + purposeful_effort * 0.100,
            _ => 0.085,
        };
        // The expression comes from the authoritative affect/brain pipeline.
        // Let it modulate actuator effort as well as the face so high arousal is
        // physically quicker while low energy remains visibly heavier.
        let speed_cap = base_speed_cap * (0.82 + expression_energy * 0.28);
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
        let purposeful_mode = matches!(
            intent.locomotion,
            LocomotionMode::Seek | LocomotionMode::Flee | LocomotionMode::Orbit
        );
        let requested_response = if purposeful_mode {
            purposeful_effort
        } else {
            0.0
        };
        let braking_response = if purposeful_mode
            && velocity.length_squared() > 1.0
            && desired_velocity.dot(velocity) < velocity.length_squared() * 0.70
        {
            (velocity.length() / (0.24 * reference_span)).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let purposeful_response = requested_response.max(braking_response);
        let maximum_acceleration = ((0.48 / genome.inertia.max(0.2)).clamp(0.18, 1.30)
            * reference_span
            * (0.84 + expression_energy * 0.32)
            * (1.0 + purposeful_response * 0.58))
            .min(1.85 * reference_span);
        let velocity_response = 5.8 + purposeful_response * 3.2;
        let motor_acceleration = ((desired_velocity - velocity) * velocity_response)
            .clamp_length_max(maximum_acceleration);
        let supported = self.feedback.grounded
            || self.feedback.clinging
            || (was_grounded
                && matches!(
                    intent.locomotion,
                    LocomotionMode::Sleep | LocomotionMode::Cocoon
                ));
        let lift_fraction = match intent.locomotion {
            LocomotionMode::Sleep | LocomotionMode::Cocoon => 0.0,
            LocomotionMode::EdgeCling => 1.0,
            LocomotionMode::Landing if self.feedback.grounded => 1.0,
            _ => (0.74 + expression_energy * 0.18).clamp(0.0, 0.94),
        };
        let gravity = if supported {
            0.0
        } else {
            BODY_SCREEN_GRAVITY * reference_span * (1.0 - lift_fraction)
        };
        let acceleration =
            (motor_acceleration + Vec2::Y * gravity).clamp_length_max(maximum_acceleration * 1.10);
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
    surfaces
        .iter()
        .enumerate()
        .fold(Vec2::ZERO, |avoidance, (index, surface)| {
            let closest =
                position.clamp(surface.rect.minimum * scale, surface.rect.maximum * scale);
            let closest_world = closest / scale;
            let occluded = surfaces[..index].iter().any(|front| {
                closest_world.x > front.rect.minimum.x + 1.0e-5
                    && closest_world.x < front.rect.maximum.x - 1.0e-5
                    && closest_world.y > front.rect.minimum.y + 1.0e-5
                    && closest_world.y < front.rect.maximum.y - 1.0e-5
            });
            if occluded {
                return avoidance;
            }
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

    #[test]
    fn sleep_releases_active_lift_and_falls_under_screen_gravity() {
        let genome = Genome::from_seed(93);
        let mut simulation = BodySimulation::new(93);
        simulation.feedback.world_position = Vec2::new(0.5, 0.30);
        simulation.set_motion_space_pixels(Vec2::new(1_920.0, 1_080.0));
        let intent = BodyIntent {
            locomotion: LocomotionMode::Sleep,
            target_position: simulation.feedback.world_position,
            target_surface: None,
            desired_speed: 0.0,
            facing_direction: 1.0,
            gaze_target: None,
            pose: PoseIntent::Sleeping,
            expression: ExpressionState::default(),
            interaction_target: None,
        };

        for _ in 0..120 {
            simulation.fixed_update(&genome.body, &intent, &SensorFrame::default(), 1.0 / 120.0);
        }

        assert!(simulation.feedback.world_position.y > 0.31);
        assert!(simulation.feedback.velocity.y > 0.0);
    }

    #[test]
    fn affective_expression_changes_motor_effort_without_breaking_speed_cap() {
        let genome = Genome::from_seed(94);
        let mut low = BodySimulation::new(94);
        let mut high = BodySimulation::new(94);
        low.feedback.world_position = Vec2::new(0.20, 0.50);
        high.feedback.world_position = low.feedback.world_position;
        low.set_motion_space_pixels(Vec2::new(1_920.0, 1_080.0));
        high.set_motion_space_pixels(Vec2::new(1_920.0, 1_080.0));
        let base = BodyIntent {
            locomotion: LocomotionMode::Seek,
            target_position: Vec2::new(0.80, 0.50),
            target_surface: None,
            desired_speed: 0.09,
            facing_direction: 1.0,
            gaze_target: None,
            pose: PoseIntent::Curious,
            expression: ExpressionState {
                body_glow: 0.05,
                pupil_size: 0.20,
                ..ExpressionState::default()
            },
            interaction_target: None,
        };
        let mut energized = base.clone();
        energized.expression.body_glow = 1.0;
        energized.expression.pupil_size = 1.0;

        for _ in 0..120 {
            low.fixed_update(&genome.body, &base, &SensorFrame::default(), 1.0 / 120.0);
            high.fixed_update(
                &genome.body,
                &energized,
                &SensorFrame::default(),
                1.0 / 120.0,
            );
        }

        let physical_scale = Vec2::new(1_920.0, 1_080.0);
        let low_speed = (low.feedback.velocity * physical_scale).length();
        let high_speed = (high.feedback.velocity * physical_scale).length();
        assert!(high_speed > low_speed + 8.0);
        assert!(high_speed < 120.0);
    }

    #[test]
    fn explicit_high_effort_seek_is_visibly_faster_than_ambient_seek() {
        let genome = Genome::from_seed(95);
        let mut ambient = BodySimulation::new(95);
        let mut purposeful = BodySimulation::new(95);
        ambient.feedback.world_position = Vec2::new(0.20, 0.50);
        purposeful.feedback.world_position = ambient.feedback.world_position;
        let physical_scale = Vec2::new(1_920.0, 1_080.0);
        ambient.set_motion_space_pixels(physical_scale);
        purposeful.set_motion_space_pixels(physical_scale);
        let ambient_intent = BodyIntent {
            locomotion: LocomotionMode::Seek,
            target_position: Vec2::new(0.80, 0.50),
            target_surface: None,
            desired_speed: 0.09,
            facing_direction: 1.0,
            gaze_target: None,
            pose: PoseIntent::Curious,
            expression: ExpressionState::default(),
            interaction_target: None,
        };
        let mut purposeful_intent = ambient_intent.clone();
        purposeful_intent.desired_speed = 0.64;

        for _ in 0..120 {
            ambient.fixed_update(
                &genome.body,
                &ambient_intent,
                &SensorFrame::default(),
                1.0 / 120.0,
            );
            purposeful.fixed_update(
                &genome.body,
                &purposeful_intent,
                &SensorFrame::default(),
                1.0 / 120.0,
            );
        }

        let ambient_speed = (ambient.feedback.velocity * physical_scale).length();
        let purposeful_speed = (purposeful.feedback.velocity * physical_scale).length();
        assert!(purposeful_speed > ambient_speed * 2.5);
        assert!(
            (270.0..=310.0).contains(&purposeful_speed),
            "purposeful seek speed={purposeful_speed} px/s"
        );
    }

    #[test]
    fn purposeful_seek_flee_and_orbit_have_fast_physical_pixel_speed_bands() {
        let genome = Genome::from_seed(0xFA57);
        let regular_scale = Vec2::new(1_920.0, 1_080.0);
        let ultrawide_scale = Vec2::new(3_440.0, 1_080.0);
        let base_intent = BodyIntent {
            locomotion: LocomotionMode::Seek,
            target_position: Vec2::new(0.92, 0.50),
            target_surface: None,
            desired_speed: 0.72,
            facing_direction: 1.0,
            gaze_target: None,
            pose: PoseIntent::Curious,
            expression: ExpressionState::default(),
            interaction_target: None,
        };
        let mut regular_seek = BodySimulation::new(0xFA57);
        let mut ultrawide_seek = BodySimulation::new(0xFA57);
        regular_seek.feedback.world_position = Vec2::new(0.08, 0.50);
        ultrawide_seek.feedback.world_position = Vec2::new(0.08, 0.50);
        regular_seek.set_motion_space_pixels(regular_scale);
        ultrawide_seek.set_motion_space_pixels(ultrawide_scale);
        for _ in 0..150 {
            regular_seek.fixed_update(
                &genome.body,
                &base_intent,
                &SensorFrame::default(),
                1.0 / 120.0,
            );
            ultrawide_seek.fixed_update(
                &genome.body,
                &base_intent,
                &SensorFrame::default(),
                1.0 / 120.0,
            );
        }
        let seek_speed = (regular_seek.feedback.velocity * regular_scale).length();
        let ultrawide_seek_speed = (ultrawide_seek.feedback.velocity * ultrawide_scale).length();
        assert!(
            (270.0..=310.0).contains(&seek_speed),
            "seek={seek_speed} px/s"
        );
        assert!((seek_speed - ultrawide_seek_speed).abs() < 0.75);

        let mut flee = BodySimulation::new(0xFA57);
        flee.feedback.world_position = Vec2::new(0.62, 0.50);
        flee.set_motion_space_pixels(regular_scale);
        let mut flee_intent = base_intent.clone();
        flee_intent.locomotion = LocomotionMode::Flee;
        flee_intent.target_position = Vec2::new(0.18, 0.50);
        for _ in 0..150 {
            flee.fixed_update(
                &genome.body,
                &flee_intent,
                &SensorFrame::default(),
                1.0 / 120.0,
            );
        }
        let flee_speed = (flee.feedback.velocity * regular_scale).length();
        assert!(
            (290.0..=335.0).contains(&flee_speed),
            "flee={flee_speed} px/s"
        );

        let mut orbit = BodySimulation::new(0xFA57);
        orbit.feedback.world_position = Vec2::new(0.59, 0.50);
        orbit.set_motion_space_pixels(regular_scale);
        let mut orbit_intent = base_intent;
        orbit_intent.locomotion = LocomotionMode::Orbit;
        orbit_intent.target_position = Vec2::new(0.50, 0.50);
        for _ in 0..150 {
            orbit.fixed_update(
                &genome.body,
                &orbit_intent,
                &SensorFrame::default(),
                1.0 / 120.0,
            );
        }
        let orbit_speed = (orbit.feedback.velocity * regular_scale).length();
        assert!(
            (235.0..=285.0).contains(&orbit_speed),
            "orbit={orbit_speed} px/s"
        );
    }

    #[test]
    fn purposeful_launch_and_brake_are_quick_but_never_reverse_or_teleport_in_one_step() {
        let genome = Genome::from_seed(0x000B_2A4E);
        let scale = Vec2::new(1_920.0, 1_080.0);
        let mut simulation = BodySimulation::new(0x000B_2A4E);
        simulation.feedback.world_position = Vec2::new(0.12, 0.50);
        simulation.set_motion_space_pixels(scale);
        let mut intent = BodyIntent {
            locomotion: LocomotionMode::Seek,
            target_position: Vec2::new(4.0, 0.50),
            target_surface: None,
            desired_speed: 0.72,
            facing_direction: 1.0,
            gaze_target: None,
            pose: PoseIntent::Curious,
            expression: ExpressionState::default(),
            interaction_target: None,
        };

        for _ in 0..30 {
            simulation.fixed_update(&genome.body, &intent, &SensorFrame::default(), 1.0 / 120.0);
        }
        let launch_speed = (simulation.feedback.velocity * scale).length();
        assert!(launch_speed > 175.0, "launch={launch_speed} px/s");
        assert!(
            simulation
                .embodied_target
                .is_some_and(|target| target.is_finite() && target.max_element() <= 0.98)
        );

        intent.desired_speed = 0.0;
        let before_position = simulation.feedback.world_position;
        let before_velocity = simulation.feedback.velocity.x * scale.x;
        simulation.fixed_update(&genome.body, &intent, &SensorFrame::default(), 1.0 / 30.0);
        let one_step_velocity = simulation.feedback.velocity.x * scale.x;
        assert!(before_velocity > 0.0 && one_step_velocity > 0.0);
        assert!(one_step_velocity < before_velocity);
        assert!(simulation.feedback.world_position.distance(before_position) < 0.01);

        for _ in 0..30 {
            simulation.fixed_update(&genome.body, &intent, &SensorFrame::default(), 1.0 / 120.0);
        }
        let braked_speed = (simulation.feedback.velocity * scale).length();
        assert!(
            braked_speed < launch_speed * 0.45,
            "launch={launch_speed} brake={braked_speed} px/s"
        );
        assert!(simulation.feedback.world_position.is_finite());
        assert!(simulation.feedback.velocity.is_finite());
        assert!(simulation.feedback.acceleration.is_finite());
    }

    #[test]
    fn covered_back_window_edge_does_not_double_motor_avoidance() {
        let scale = Vec2::new(1_920.0, 1_080.0);
        let position = Vec2::new(0.58, 0.50) * scale;
        let front = SurfaceRect {
            id: lifecore::SurfaceId("front".into()),
            rect: lifecore::Rect {
                minimum: Vec2::new(0.59, 0.30),
                maximum: Vec2::new(0.72, 0.70),
            },
        };
        let behind = SurfaceRect {
            id: lifecore::SurfaceId("behind".into()),
            rect: lifecore::Rect {
                minimum: Vec2::new(0.60, 0.35),
                maximum: Vec2::new(0.68, 0.65),
            },
        };
        let intent = BodyIntent {
            locomotion: LocomotionMode::Hover,
            target_position: position / scale,
            target_surface: None,
            desired_speed: 0.0,
            facing_direction: 1.0,
            gaze_target: None,
            pose: PoseIntent::Neutral,
            expression: ExpressionState::default(),
            interaction_target: None,
        };

        let front_only = avoid_surfaces(
            position,
            scale,
            1_080.0,
            std::slice::from_ref(&front),
            &intent,
        );
        let stacked = avoid_surfaces(position, scale, 1_080.0, &[front, behind], &intent);

        assert_eq!(front_only, stacked);
        assert!(stacked.x < 0.0);
    }
}
