//! Finite surface-care motor plan. Mechanical effort, not a random timer, builds
//! virtual local strain. Navigation is an intent proposal; only measured contact
//! admits pressure/rubbing. The application retains root and safety authority.
use glam::Vec2;
use pet_motor::RepertoireEvent;
use serde::Serialize;

/// Currently supplied by native screen/contact/support/topology observations.
/// Smooth/moving materials and path replanning remain explicit hooks, not count.
pub const INTEGRATED_SURFACE_CARE_IDS: &[u16] = &[
    92, 93, 94, 95, 96, 98, 100, 180, 182, 183, 187, 188, 193, 199,
];

#[derive(Clone, Copy, Debug)]
pub struct SurfaceCareInput {
    pub position: Vec2,
    pub velocity: Vec2,
    /// Visible contour offsets in normalized desktop coordinates, not glow.
    pub half_extent: Vec2,
    pub desktop_min: Vec2,
    pub desktop_max: Vec2,
    pub physical_effort: f32,
    pub sleeping: bool,
    pub quiet: bool,
    pub dragged: bool,
    pub gripping: bool,
    pub danger: bool,
    pub purposeful: bool,
    /// Actual signed outward-from-wall contact normal; None is not touching.
    pub contact_normal: Option<Vec2>,
    pub contact_load: f32,
    /// Observed tangential contact/slip and material friction, when available.
    pub surface_friction: Option<f32>,
    pub surface_velocity: Vec2,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub enum SurfaceCarePhase {
    #[default]
    Idle,
    Approach,
    Load,
    Rub,
    Unload,
    Depart,
}

#[derive(Clone, Debug, Default)]
pub struct SurfaceCareOutput {
    pub preferred_rest_target: Option<Vec2>,
    pub target: Option<Vec2>,
    pub speed: f32,
    /// Bounded normal pressure, only while physically touching. Must be applied
    /// through the existing local body/contact solver, never a root teleport.
    pub pressure: f32,
    pub contact_measured: bool,
    pub contact_normal: Vec2,
    pub events: Vec<RepertoireEvent>,
}

#[derive(Debug, Default, Serialize)]
pub struct SurfaceCareRuntime {
    pub phase: SurfaceCarePhase,
    pub strain: f32,
    pub phase_age: f32,
    cooldown: f32,
    side: f32,
    vertical_bias: f32,
    contact_origin: Vec2,
    prior_contact: bool,
    bout_count: u32,
    pub stroke_duration: f32,
    pub stroke_amplitude: f32,
    pub stroke_shape: f32,
    pressure_reported: bool,
    moving_reported: bool,
    completed_side: f32,
    stroke_travel: f32,
    commanded_pressure: f32,
    unload_pressure: f32,
}

fn event(id: u16, target: Vec2, side: f32) -> RepertoireEvent {
    RepertoireEvent {
        id,
        confidence: 0.9,
        target: Some(target),
        side,
    }
}
fn smooth(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

impl SurfaceCareRuntime {
    pub fn tick(&mut self, input: SurfaceCareInput, dt: f32) -> SurfaceCareOutput {
        let mut out = SurfaceCareOutput::default();
        if !dt.is_finite()
            || dt <= 0.0
            || !input.position.is_finite()
            || !input.velocity.is_finite()
            || !input.half_extent.is_finite()
            || !input.desktop_min.is_finite()
            || !input.desktop_max.is_finite()
        {
            return out;
        }
        let dt = dt.min(0.1);
        // Observe real impacts independently of the optional care motor plan.
        // This is only an expression event: the existing solver owns recoil.
        let boundary_hit = input.contact_load.is_finite()
            && input.contact_load > 0.03
            && input
                .contact_normal
                .is_some_and(|n| n.is_finite() && n.length_squared() > 0.5);
        if boundary_hit && !self.prior_contact {
            let normal = input.contact_normal.unwrap().normalize_or_zero();
            out.events.push(event(
                180,
                (input.position + normal * 0.025).clamp(input.desktop_min, input.desktop_max),
                normal.x,
            ));
        }
        self.prior_contact = boundary_hit;
        self.cooldown = (self.cooldown - dt).max(0.0);
        let safe = !(input.sleeping
            || input.quiet
            || input.dragged
            || input.gripping
            || input.danger
            || input.purposeful);
        if !safe {
            if self.phase != SurfaceCarePhase::Idle {
                self.cooldown = self.cooldown.max(20.0);
            }
            self.phase = SurfaceCarePhase::Idle;
            self.phase_age = 0.0;
            self.commanded_pressure = 0.0;
            return out;
        }
        let effort = if input.physical_effort.is_finite() {
            input.physical_effort.clamp(0.0, 1.0)
        } else {
            0.0
        };
        let moving = (input.velocity.length() / 0.25).clamp(0.0, 1.0);
        // Passive time never creates an itch. Flight effort builds a finite
        // fictional tissue-strain dose, which actual rubbing consumes.
        if self.phase == SurfaceCarePhase::Idle {
            self.strain = (self.strain + dt * (effort * moving * 0.12 - 0.004)).clamp(0.0, 1.0);
            self.vertical_bias +=
                (input.velocity.y - self.vertical_bias) * (1.0 - (-dt / 2.0).exp());
            if self.strain < 0.6 || self.cooldown > 0.0 {
                return out;
            }
            let middle = (input.desktop_min.x + input.desktop_max.x) * 0.5;
            self.side = if input.position.x < middle { -1.0 } else { 1.0 };
            if self.completed_side == self.side && self.strain > 0.85 {
                self.side = -self.side;
            }
            if self.completed_side != 0.0 && self.side != self.completed_side {
                out.events.push(event(100, input.position, self.side));
            }
            self.phase = SurfaceCarePhase::Approach;
            self.bout_count = self.bout_count.wrapping_add(1);
            let variant = (self.bout_count.wrapping_mul(2_654_435_761) >> 16) as f32 / 65535.0;
            // Sample once at the causal bout boundary; never frame noise.
            self.stroke_duration = 2.6 + 0.6 * self.strain + 0.25 * variant;
            self.stroke_amplitude = 0.025 + 0.012 * self.strain + 0.003 * variant;
            self.stroke_shape = (variant - 0.5) * 0.12;
            self.phase_age = 0.0;
            self.pressure_reported = false;
            self.moving_reported = false;
        }
        self.phase_age += dt;
        let extent = input
            .half_extent
            .abs()
            .clamp(Vec2::splat(0.005), Vec2::splat(0.24));
        let min = input.desktop_min + extent;
        let max = (input.desktop_max - extent).max(min);
        let wall_x = if self.side < 0.0 { min.x } else { max.x };
        let touching = input
            .contact_normal
            .is_some_and(|n| n.is_finite() && n.dot(Vec2::new(-self.side, 0.0)) > 0.65);
        out.contact_measured = touching;
        out.contact_normal = Vec2::new(-self.side, 0.0);
        let mut target = Vec2::new(wall_x, input.position.y.clamp(min.y, max.y));
        out.speed = 0.22;
        match self.phase {
            SurfaceCarePhase::Approach => {
                if touching {
                    self.phase = SurfaceCarePhase::Load;
                    self.phase_age = 0.0;
                    self.contact_origin = input.position;
                    out.events.push(event(92, target, self.side));
                } else if self.phase_age > 12.0 {
                    self.phase = SurfaceCarePhase::Idle;
                    self.cooldown = 45.0;
                    return SurfaceCareOutput::default();
                }
            }
            SurfaceCarePhase::Load | SurfaceCarePhase::Rub => {
                out.speed = 0.07;
                if !touching {
                    self.phase = SurfaceCarePhase::Approach;
                    self.phase_age = 0.0;
                } else {
                    out.pressure = if self.phase == SurfaceCarePhase::Rub {
                        0.16
                    } else {
                        0.16 * smooth(self.phase_age / 0.65)
                    };
                    if input.contact_load > 0.65 {
                        out.pressure *= 0.2;
                        target.x -= self.side * 0.008;
                        if !self.pressure_reported {
                            out.events.push(event(96, target, self.side));
                            self.pressure_reported = true;
                        }
                    }
                    if input.surface_velocity.length() > 0.01 && !self.moving_reported {
                        out.events.push(event(97, target, self.side));
                        self.moving_reported = true;
                        self.contact_origin += input.surface_velocity * dt;
                    }
                    if self.phase == SurfaceCarePhase::Load && self.phase_age >= 0.65 {
                        self.phase = SurfaceCarePhase::Rub;
                        self.phase_age = 0.0;
                        self.stroke_travel = 0.0;
                        out.events.push(event(
                            if self.vertical_bias < 0.0 { 93 } else { 94 },
                            target,
                            self.side,
                        ));
                    }
                    if self.phase == SurfaceCarePhase::Rub {
                        // One closed, smooth three-second stroke, not perpetual
                        // oscillation. Actual slip is required to relieve strain.
                        let u = (self.phase_age / self.stroke_duration.max(0.1)).clamp(0.0, 1.0);
                        let first_direction = if self.vertical_bias < 0.0 { -1.0 } else { 1.0 };
                        let stroke = first_direction
                            * (std::f32::consts::TAU * u).sin()
                            * smooth(u * 5.0)
                            * smooth((1.0 - u) * 5.0)
                            * (1.0 + self.stroke_shape * (std::f32::consts::TAU * u).cos());
                        target.y = (self.contact_origin.y + stroke * self.stroke_amplitude)
                            .clamp(min.y, max.y);
                        self.strain = (self.strain - input.velocity.y.abs() * dt * 5.0).max(0.0);
                        self.stroke_travel += input.velocity.y.abs() * dt;
                        let smooth_wall = input
                            .surface_friction
                            .is_some_and(|f| f.is_finite() && f < 0.08);
                        // Failed useful travel is measurable even without a
                        // material sensor. Do not claim the wall is smooth.
                        let ineffective = self.phase_age >= 1.4 && self.stroke_travel < 0.003;
                        if smooth_wall || ineffective || self.phase_age >= self.stroke_duration {
                            if smooth_wall || ineffective {
                                out.events.push(event(95, target, self.side));
                            }
                            out.events.push(event(98, target, self.side));
                            self.phase = SurfaceCarePhase::Unload;
                            self.unload_pressure = self.commanded_pressure.min(out.pressure);
                            self.phase_age = 0.0;
                        }
                    }
                }
            }
            SurfaceCarePhase::Unload => {
                out.pressure = if touching {
                    self.unload_pressure * (1.0 - smooth(self.phase_age / 0.8))
                } else {
                    0.0
                };
                out.speed = 0.04;
                if self.phase_age >= 0.8 {
                    self.phase = SurfaceCarePhase::Depart;
                    self.phase_age = 0.0;
                }
            }
            SurfaceCarePhase::Depart => {
                target.x = (wall_x - self.side * 0.05).clamp(min.x, max.x);
                out.speed = 0.10;
                if input.position.distance(target) < 0.015 || self.phase_age > 2.0 {
                    self.phase = SurfaceCarePhase::Idle;
                    self.completed_side = self.side;
                    self.cooldown = 90.0;
                    out.target = None;
                    return out;
                }
            }
            SurfaceCarePhase::Idle => {}
        }
        // Phase timers never reset the physical load. Safety/contact loss may
        // remove pressure immediately; ordinary transitions slew continuously.
        if touching {
            self.commanded_pressure +=
                (out.pressure - self.commanded_pressure).clamp(-0.25 * dt, 0.25 * dt);
        } else {
            self.commanded_pressure = 0.0;
        }
        out.pressure = self.commanded_pressure;
        out.target = Some(target);
        out
    }
}

/// Other spatial events need their real planner/topology evidence. This adapter
/// does not invent window contents, obstacles, learned favorites or monitors.
#[derive(Default)]
pub struct SpatialEvidenceEvents {
    topology_count: Option<usize>,
    support_id: Option<u64>,
    support_age: f32,
    pub favorite: Option<u64>,
    pub favorite_position: Option<Vec2>,
    route_length: Option<f32>,
    route_origin: Option<Vec2>,
    was_blocked: bool,
    corner_latched: bool,
    assessed_support: Option<u64>,
}

#[derive(Default)]
pub struct SpatialEvidence {
    pub position: Vec2,
    pub monitor_count: usize,
    pub valid_recovery_position: Option<Vec2>,
    pub candidate_support: Option<(u64, f32)>, // actual id and width / body width
    pub supported_id: Option<u64>,
    pub available_support_ids: Vec<u64>,
    pub stable_support: bool,
    pub corner_contact_count: usize,
    pub route_length: Option<f32>, // measured planner length, NOT straight-line guess
    pub route_blocked: bool,
    pub alternate_route_target: Option<Vec2>,
}

impl SpatialEvidenceEvents {
    pub fn observe(&mut self, input: &SpatialEvidence, dt: f32) -> Vec<RepertoireEvent> {
        let mut events = Vec::new();
        let dt = if dt.is_finite() {
            dt.clamp(0.0, 0.1)
        } else {
            0.0
        };
        if self.topology_count.is_some_and(|n| input.monitor_count < n)
            && let Some(target) = input.valid_recovery_position
        {
            events.push(event(188, target, 0.0));
        }
        self.topology_count = Some(input.monitor_count);
        if let Some((id, width)) = input.candidate_support
            && self.assessed_support != Some(id)
            && width.is_finite()
        {
            events.push(event(
                if width >= 1.15 { 182 } else { 183 },
                input.position,
                0.0,
            ));
            self.assessed_support = Some(id);
        }
        if input.supported_id == self.support_id && input.stable_support {
            self.support_age += dt;
            if self.support_age >= 8.0 && self.favorite != input.supported_id {
                self.favorite = input.supported_id;
                self.favorite_position = Some(input.position);
                events.push(event(193, input.position, 0.0));
            }
        } else {
            self.support_id = input.supported_id;
            self.support_age = 0.0;
        }
        if let Some(favorite) = self.favorite
            && !input.available_support_ids.contains(&favorite)
            && let Some(target) = input.alternate_route_target
        {
            events.push(event(199, target, 0.0));
            self.favorite = None;
            self.favorite_position = None;
        }
        let corner = input.corner_contact_count >= 2 && input.stable_support;
        if corner && !self.corner_latched {
            events.push(event(187, input.position, 0.0));
        }
        self.corner_latched = corner;
        if input.route_blocked
            && !self.was_blocked
            && let Some(target) = input.alternate_route_target
        {
            events.push(event(186, target, 0.0));
        }
        self.was_blocked = input.route_blocked;
        if let Some(length) = input.route_length.filter(|v| v.is_finite() && *v >= 0.0) {
            if self.route_length.is_some_and(|old| length < old * 0.75)
                && self
                    .route_origin
                    .is_some_and(|old| old.distance(input.position) < 0.005)
                && let Some(target) = input.alternate_route_target
            {
                events.push(event(185, target, 0.0));
            }
            self.route_length = Some(length);
            self.route_origin = Some(input.position);
        }
        events
    }
}
