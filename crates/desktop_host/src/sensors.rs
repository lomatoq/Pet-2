use std::time::Instant;

use glam::Vec2;
use lifecore::{AppCategory, DayPhase, Rect, SensorFrame, SurfaceId, SurfaceRect};
use serde::{Deserialize, Serialize};

use crate::{DisplayTopology, PhysicalDesktopPoint, RectI};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApplicationInfo {
    pub category: AppCategory,
    /// May be used within the current session for learning, but must not be persisted.
    #[serde(skip)]
    pub transient_identifier: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DesktopSurface {
    pub transient_id: String,
    pub bounds: RectI,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DesktopSnapshot {
    pub timestamp: f64,
    pub topology_revision: u64,
    pub cursor: Option<PhysicalDesktopPoint>,
    pub idle_seconds: Option<f32>,
    pub active_application: Option<ApplicationInfo>,
    pub active_window: Option<RectI>,
    pub visible_surfaces: Vec<DesktopSurface>,
}

impl DesktopSnapshot {
    #[must_use]
    pub fn unavailable(timestamp: f64, topology_revision: u64) -> Self {
        Self {
            timestamp,
            topology_revision,
            cursor: None,
            idle_seconds: None,
            active_application: None,
            active_window: None,
            visible_surfaces: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct PointerState {
    pub down: bool,
    pub pressed: bool,
    pub released: bool,
    pub pet_hovered: bool,
    pub pet_touched: bool,
    pub pet_dragged: bool,
}

pub struct SensorNormalizer {
    started: Instant,
    previous_cursor: Option<Vec2>,
    previous_velocity: Vec2,
    previous_timestamp: Option<f64>,
}

impl Default for SensorNormalizer {
    fn default() -> Self {
        Self {
            started: Instant::now(),
            previous_cursor: None,
            previous_velocity: Vec2::ZERO,
            previous_timestamp: None,
        }
    }
}

impl SensorNormalizer {
    #[must_use]
    pub fn monotonic_seconds(&self) -> f64 {
        self.started.elapsed().as_secs_f64()
    }

    #[must_use]
    pub fn normalize(
        &mut self,
        snapshot: &DesktopSnapshot,
        topology: &DisplayTopology,
        pet_position: Vec2,
        pointer: PointerState,
        local_time_01: f32,
    ) -> SensorFrame {
        let dt = self.previous_timestamp.map_or(1.0 / 60.0, |previous| {
            (snapshot.timestamp - previous).max(1.0e-4)
        });
        let cursor = snapshot
            .cursor
            .map(|point| normalize_virtual(topology, point))
            .unwrap_or_else(|| self.previous_cursor.unwrap_or(Vec2::splat(0.5)));
        let velocity = self
            .previous_cursor
            .map_or(Vec2::ZERO, |previous| (cursor - previous) / dt as f32);
        let acceleration = (velocity - self.previous_velocity) / dt as f32;
        self.previous_cursor = Some(cursor);
        self.previous_velocity = velocity;
        self.previous_timestamp = Some(snapshot.timestamp);
        let visible_surfaces = snapshot
            .visible_surfaces
            .iter()
            .filter(|surface| surface.bounds.is_valid())
            .map(|surface| SurfaceRect {
                id: SurfaceId(surface.transient_id.clone()),
                rect: normalize_rect(topology, surface.bounds),
            })
            .collect();
        let time_of_day_01 = local_time_01.rem_euclid(1.0);
        SensorFrame {
            timestamp: snapshot.timestamp,
            screen_size: Vec2::ONE,
            cursor_position: cursor,
            cursor_velocity: velocity,
            cursor_acceleration: acceleration,
            cursor_distance_to_pet: cursor.distance(pet_position),
            cursor_approach_speed: -velocity.dot((cursor - pet_position).normalize_or_zero()),
            pointer_down: pointer.down,
            pointer_pressed: pointer.pressed,
            pointer_released: pointer.released,
            pet_hovered: pointer.pet_hovered,
            pet_touched: pointer.pet_touched,
            pet_dragged: pointer.pet_dragged,
            user_idle_seconds: snapshot.idle_seconds.unwrap_or(0.0),
            user_activity_rate: snapshot
                .idle_seconds
                .map_or(0.0, |idle| (1.0 - idle / 30.0).clamp(0.0, 1.0)),
            recent_click_rhythm: [0.0; 8],
            active_app_category: snapshot
                .active_application
                .as_ref()
                .map_or(AppCategory::Unknown, |app| app.category),
            active_window_rect: snapshot
                .active_window
                .filter(|rect| rect.is_valid())
                .map(|rect| normalize_rect(topology, rect)),
            visible_surfaces,
            time_of_day_01,
            day_phase: day_phase(time_of_day_01),
            audio_rms: None,
            voice_activity: None,
            user_presence: snapshot
                .idle_seconds
                .map(|idle| if idle < 180.0 { 1.0 } else { 0.0 }),
            user_availability: snapshot
                .idle_seconds
                .map(|idle| (1.0 - idle / 120.0).clamp(0.0, 1.0)),
        }
    }
}

fn virtual_bounds(topology: &DisplayTopology) -> RectI {
    if topology.virtual_physical_bounds.is_valid() {
        return topology.virtual_physical_bounds;
    }
    topology
        .monitors
        .iter()
        .fold(RectI::default(), |bounds, monitor| {
            if !bounds.is_valid() {
                monitor.physical_bounds
            } else {
                RectI {
                    minimum: PhysicalDesktopPoint {
                        x: bounds.minimum.x.min(monitor.physical_bounds.minimum.x),
                        y: bounds.minimum.y.min(monitor.physical_bounds.minimum.y),
                    },
                    maximum: PhysicalDesktopPoint {
                        x: bounds.maximum.x.max(monitor.physical_bounds.maximum.x),
                        y: bounds.maximum.y.max(monitor.physical_bounds.maximum.y),
                    },
                }
            }
        })
}

fn normalize_virtual(topology: &DisplayTopology, point: PhysicalDesktopPoint) -> Vec2 {
    let bounds = virtual_bounds(topology);
    if !bounds.is_valid() {
        return Vec2::splat(0.5);
    }
    Vec2::new(
        (point.x - bounds.minimum.x) as f32 / bounds.width() as f32,
        (point.y - bounds.minimum.y) as f32 / bounds.height() as f32,
    )
    .clamp(Vec2::ZERO, Vec2::ONE)
}

fn normalize_rect(topology: &DisplayTopology, bounds: RectI) -> Rect {
    Rect {
        minimum: normalize_virtual(topology, bounds.minimum),
        maximum: normalize_virtual(topology, bounds.maximum),
    }
}

fn day_phase(time: f32) -> DayPhase {
    match time {
        value if value < 0.23 => DayPhase::Night,
        value if value < 0.38 => DayPhase::Morning,
        value if value < 0.75 => DayPhase::Day,
        value if value < 0.88 => DayPhase::Evening,
        _ => DayPhase::Night,
    }
}
