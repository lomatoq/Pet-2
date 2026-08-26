//! Privacy-preserving desktop perception for VITA.
//!
//! This crate consumes normalized, abstract desktop signals. It never receives or
//! stores typed characters, key codes, screenshots, microphone recordings, window
//! text, accessibility names, clipboard contents, or document content.

mod visual_grid;

pub use visual_grid::*;

use std::collections::{BTreeMap, VecDeque};

use glam::Vec2;
use lifecore::{
    BodyFeedback, PointerGesturePercept, Rect, SensorFrame, StimulusEvent, StimulusKind,
    VitaPerceptFrame, stable_hash_bytes,
};
use pet_ecology::{
    NormalizedRect, RhythmSignature, WindowAffordance, WindowAffordanceFrame, WindowId,
};
use serde::{Deserialize, Serialize};

const CURSOR_HISTORY_SECONDS: f64 = 2.4;
const MAX_CURSOR_SAMPLES: usize = 180;
const MAX_RHYTHM_EVENTS: usize = 64;
const SURFACE_FORGET_SECONDS: f64 = 3.0;

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct VisualFeatureFrame {
    pub mean_luminance: f32,
    pub local_luminance: f32,
    pub contrast: f32,
    pub colorfulness: f32,
    pub warmth: f32,
    pub dominant_hue: f32,
    pub motion_energy: f32,
    pub edge_density: f32,
    pub sudden_change: f32,
}

impl VisualFeatureFrame {
    #[must_use]
    pub fn bounded(mut self) -> Self {
        for value in [
            &mut self.mean_luminance,
            &mut self.local_luminance,
            &mut self.contrast,
            &mut self.colorfulness,
            &mut self.warmth,
            &mut self.dominant_hue,
            &mut self.motion_energy,
            &mut self.edge_density,
            &mut self.sudden_change,
        ] {
            *value = if value.is_finite() {
                value.clamp(0.0, 1.0)
            } else {
                0.0
            };
        }
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct CursorSample {
    timestamp: f64,
    position: Vec2,
}

#[derive(Debug, Clone, PartialEq)]
struct SurfaceHistory {
    rect: Rect,
    velocity: Vec2,
    last_seen: f64,
    age_seconds: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PerceptionRuntime {
    cursor_history: VecDeque<CursorSample>,
    keyboard_activity: VecDeque<f64>,
    click_activity: VecDeque<f64>,
    last_idle_seconds: Option<f32>,
    last_keyboard_activity: Option<f64>,
    scroll_velocity: f32,
    scroll_burst: f32,
    surfaces: BTreeMap<String, SurfaceHistory>,
    visual: Option<VisualFeatureFrame>,
    visual_age: f32,
    previous_visual: Option<VisualFeatureFrame>,
    window_affordances: WindowAffordanceFrame,
    spatial_visual: Option<SpatialVisualFrame>,
    spatial_attention: SpatialAttentionRuntime,
}

impl Default for PerceptionRuntime {
    fn default() -> Self {
        Self {
            cursor_history: VecDeque::with_capacity(MAX_CURSOR_SAMPLES),
            keyboard_activity: VecDeque::with_capacity(MAX_RHYTHM_EVENTS),
            click_activity: VecDeque::with_capacity(MAX_RHYTHM_EVENTS),
            last_idle_seconds: None,
            last_keyboard_activity: None,
            scroll_velocity: 0.0,
            scroll_burst: 0.0,
            surfaces: BTreeMap::new(),
            visual: None,
            visual_age: f32::INFINITY,
            previous_visual: None,
            window_affordances: WindowAffordanceFrame::default(),
            spatial_visual: None,
            spatial_attention: SpatialAttentionRuntime::default(),
        }
    }
}

impl PerceptionRuntime {
    /// Adds an abstract keyboard-activity timestamp. The caller must discard the key
    /// code and character before invoking this method.
    pub fn note_key_activity(&mut self, timestamp: f64) {
        push_time(&mut self.keyboard_activity, timestamp);
        self.last_keyboard_activity = Some(timestamp);
    }

    pub fn note_click(&mut self, timestamp: f64) {
        push_time(&mut self.click_activity, timestamp);
    }

    /// Adds normalized wheel movement in `-1..=1`; no target content is retained.
    pub fn note_scroll(&mut self, normalized_delta: f32) {
        let delta = finite(normalized_delta).clamp(-1.0, 1.0);
        self.scroll_velocity = (self.scroll_velocity + delta * 0.72).clamp(-1.0, 1.0);
        self.scroll_burst = (self.scroll_burst + delta.abs() * 0.48).clamp(0.0, 1.0);
    }

    pub fn set_visual_features(&mut self, frame: VisualFeatureFrame) {
        let frame = frame.bounded();
        self.previous_visual = self.visual;
        self.visual = Some(frame);
        self.visual_age = 0.0;
    }

    pub fn set_spatial_visual(&mut self, frame: SpatialVisualFrame) {
        self.spatial_visual = Some(frame.bounded());
        self.visual_age = 0.0;
    }

    pub fn cue_shared_attention(&mut self, position: Vec2, duration_seconds: f32) {
        self.spatial_attention.cue(position, duration_seconds);
    }

    #[must_use]
    pub const fn visual_attention_target(&self) -> Option<VisualAttentionTarget> {
        self.spatial_attention.target()
    }

    #[must_use]
    pub fn recent_click_rhythm(&self) -> Option<RhythmSignature> {
        let count = self.click_activity.len().min(9);
        if count < 2 {
            return None;
        }
        let mut onsets = [0.0_f64; 9];
        for (target, onset) in onsets.iter_mut().zip(
            self.click_activity
                .iter()
                .skip(self.click_activity.len() - count),
        ) {
            *target = *onset;
        }
        RhythmSignature::from_onsets(&onsets[..count])
    }

    #[must_use]
    pub const fn window_affordances(&self) -> &WindowAffordanceFrame {
        &self.window_affordances
    }

    #[must_use]
    pub fn update(
        &mut self,
        sensors: &SensorFrame,
        body: &BodyFeedback,
        dt: f32,
    ) -> VitaPerceptFrame {
        let dt = finite(dt).clamp(0.0, 0.25);
        let timestamp = if sensors.timestamp.is_finite() {
            sensors.timestamp
        } else {
            self.cursor_history
                .back()
                .map_or(0.0, |sample| sample.timestamp + f64::from(dt))
        };
        self.observe_cursor(sensors, timestamp);
        self.infer_private_input_activity(sensors, timestamp);
        if sensors.pointer_pressed {
            self.note_click(timestamp);
        }
        prune_times(&mut self.keyboard_activity, timestamp, 8.0);
        prune_times(&mut self.click_activity, timestamp, 8.0);
        self.scroll_velocity *= (-dt * 5.5).exp();
        self.scroll_burst *= (-dt * 2.2).exp();
        self.visual_age += dt;
        self.spatial_attention.update(
            self.spatial_visual
                .as_ref()
                .filter(|_| self.visual_age < 2.5),
            dt,
        );

        let pointer = self.pointer_gestures(body.world_position, sensors);
        let typing_rate_hz = event_rate(&self.keyboard_activity, timestamp, 2.0).clamp(0.0, 24.0);
        let typing_burstiness = interval_burstiness(&self.keyboard_activity, timestamp, 5.0);
        let click_rate_hz = event_rate(&self.click_activity, timestamp, 3.0).clamp(0.0, 20.0);
        let typing_pause_seconds = self
            .last_keyboard_activity
            .map_or(60.0, |last| (timestamp - last).max(0.0) as f32)
            .clamp(0.0, 3_600.0);
        let ecology = self.update_windows(sensors, body.world_position, timestamp, dt);

        let visual = self.visual.filter(|_| self.visual_age < 2.5);
        let mut events = Vec::with_capacity(16);
        push_pointer_events(&mut events, pointer, sensors.cursor_position);
        if typing_rate_hz > 0.35 {
            events.push(
                StimulusEvent {
                    kind: StimulusKind::TypingRhythm,
                    position: sensors
                        .active_window_rect
                        .map(|rect| (rect.minimum + rect.maximum) * 0.5),
                    intensity: (typing_rate_hz / 8.0).clamp(0.0, 1.0),
                    novelty: typing_burstiness,
                    threat: 0.0,
                    social_relevance: 0.18,
                }
                .bounded(),
            );
        }
        if self.scroll_velocity.abs() > 0.08 {
            events.push(
                StimulusEvent {
                    kind: StimulusKind::ScrollFlow,
                    position: sensors
                        .active_window_rect
                        .map(|rect| (rect.minimum + rect.maximum) * 0.5),
                    intensity: self.scroll_velocity.abs(),
                    novelty: self.scroll_burst,
                    threat: 0.04,
                    social_relevance: 0.06,
                }
                .bounded(),
            );
        }
        events.extend(ecology.events.iter().copied());
        if let Some(target) = self.spatial_attention.target() {
            let kind = match target.kind {
                VisualRegionKind::Bright => StimulusKind::BrightArea,
                VisualRegionKind::Dark => StimulusKind::DarkArea,
                _ => StimulusKind::VisualChange,
            };
            events.push(
                StimulusEvent {
                    kind,
                    position: Some(target.position),
                    intensity: target.score,
                    novelty: if target.explicit { 1.0 } else { target.score },
                    threat: 0.0,
                    social_relevance: if target.explicit { 0.72 } else { 0.0 },
                }
                .bounded(),
            );
        }
        if let Some(visual) = visual {
            if visual.sudden_change > 0.10 || visual.motion_energy > 0.18 {
                events.push(
                    StimulusEvent {
                        kind: StimulusKind::VisualChange,
                        position: sensors
                            .active_window_rect
                            .map(|rect| (rect.minimum + rect.maximum) * 0.5),
                        intensity: visual.motion_energy.max(visual.sudden_change),
                        novelty: visual.sudden_change,
                        threat: visual.sudden_change * 0.18,
                        social_relevance: 0.0,
                    }
                    .bounded(),
                );
            }
            let local = visual.local_luminance;
            if !(0.18..=0.82).contains(&local) {
                events.push(
                    StimulusEvent {
                        kind: if local < 0.18 {
                            StimulusKind::DarkArea
                        } else {
                            StimulusKind::BrightArea
                        },
                        position: Some(body.world_position),
                        intensity: (local - 0.5).abs() * 2.0,
                        novelty: self.visual_delta(),
                        threat: if local > 0.90 { 0.12 } else { 0.0 },
                        social_relevance: 0.0,
                    }
                    .bounded(),
                );
            }
        }
        events.sort_by(|left, right| {
            event_priority(right)
                .total_cmp(&event_priority(left))
                .then_with(|| stimulus_order(left.kind).cmp(&stimulus_order(right.kind)))
        });
        events.truncate(16);

        let user_available = sensors.user_availability.unwrap_or_else(|| {
            let focus_penalty = (typing_rate_hz / 10.0).clamp(0.0, 0.55);
            ((1.0 - sensors.user_idle_seconds / 120.0).clamp(0.0, 1.0) - focus_penalty)
                .clamp(0.0, 1.0)
        });
        let mut frame = VitaPerceptFrame {
            pointer,
            typing_rate_hz,
            typing_burstiness,
            typing_pause_seconds,
            click_rate_hz,
            scroll_velocity: self.scroll_velocity,
            scroll_burstiness: self.scroll_burst,
            window_motion: ecology.motion,
            window_pressure: ecology.pressure,
            popup_salience: ecology.popup_salience,
            nearest_window_edge: ecology.nearest_edge,
            mean_luminance: visual.map(|frame| frame.mean_luminance),
            local_luminance: visual.map(|frame| frame.local_luminance),
            colorfulness: visual.map(|frame| frame.colorfulness),
            warmth: visual.map(|frame| frame.warmth),
            dominant_hue: visual.map(|frame| frame.dominant_hue),
            visual_motion: visual.map(|frame| frame.motion_energy),
            visual_change: visual.map(|frame| frame.sudden_change),
            user_available,
            events,
        };
        frame.sanitize();
        frame
    }

    fn observe_cursor(&mut self, sensors: &SensorFrame, timestamp: f64) {
        self.cursor_history.push_back(CursorSample {
            timestamp,
            position: sensors.cursor_position.clamp(Vec2::ZERO, Vec2::ONE),
        });
        while self.cursor_history.len() > MAX_CURSOR_SAMPLES
            || self
                .cursor_history
                .front()
                .is_some_and(|sample| timestamp - sample.timestamp > CURSOR_HISTORY_SECONDS)
        {
            self.cursor_history.pop_front();
        }
    }

    fn infer_private_input_activity(&mut self, sensors: &SensorFrame, timestamp: f64) {
        let idle = sensors.user_idle_seconds.max(0.0);
        if let Some(previous) = self.last_idle_seconds {
            let reset = idle + 0.018 < previous;
            let cursor_quiet = sensors.cursor_velocity.length() < 0.025
                && sensors.cursor_acceleration.length() < 0.20
                && !sensors.pointer_pressed
                && !sensors.pointer_down;
            if reset && cursor_quiet {
                self.note_key_activity(timestamp);
            }
        }
        self.last_idle_seconds = Some(idle);
    }

    fn pointer_gestures(&self, pet_position: Vec2, sensors: &SensorFrame) -> PointerGesturePercept {
        let speed = sensors.cursor_velocity.length();
        let distance = sensors.cursor_distance_to_pet;
        let approach = (sensors.cursor_approach_speed * 2.8).clamp(0.0, 1.0)
            * (1.0 - distance).clamp(0.0, 1.0);
        let avoid = (-sensors.cursor_approach_speed * 2.8).clamp(0.0, 1.0)
            * (1.0 - distance).clamp(0.0, 1.0);
        let poke = if sensors.pointer_pressed && distance < 0.10 {
            1.0
        } else {
            (approach * speed * 1.4 * (1.0 - distance * 5.0)).clamp(0.0, 1.0)
        };
        let petting = if sensors.pointer_down && distance < 0.10 && speed < 0.18 {
            (1.0 - speed / 0.18).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let circle = self.circularity(pet_position);
        let crossings = self.body_crossings(pet_position, 0.10);
        let chase_invitation = (circle * 0.55 + (crossings as f32 / 5.0).clamp(0.0, 1.0) * 0.45)
            * (0.35 + speed.min(1.0) * 0.65);
        PointerGesturePercept {
            approach,
            avoid,
            poke,
            circle,
            chase_invitation: chase_invitation.clamp(0.0, 1.0),
            petting,
            fast_swipe: ((speed - 0.45) / 0.65).clamp(0.0, 1.0) * (1.0 - distance).clamp(0.0, 1.0),
        }
    }

    fn circularity(&self, center: Vec2) -> f32 {
        if self.cursor_history.len() < 8 {
            return 0.0;
        }
        let mut total_angle = 0.0;
        let mut radii = Vec::with_capacity(self.cursor_history.len());
        let mut previous = None;
        for sample in &self.cursor_history {
            let delta = sample.position - center;
            let radius = delta.length();
            if !(0.025..=0.35).contains(&radius) {
                continue;
            }
            let angle = delta.y.atan2(delta.x);
            if let Some(previous_angle) = previous {
                total_angle += wrap_angle(angle - previous_angle).abs();
            }
            previous = Some(angle);
            radii.push(radius);
        }
        if radii.len() < 6 {
            return 0.0;
        }
        let mean = radii.iter().sum::<f32>() / radii.len() as f32;
        let variance = radii
            .iter()
            .map(|radius| (radius - mean).powi(2))
            .sum::<f32>()
            / radii.len() as f32;
        let radial_stability = (1.0 - variance.sqrt() / mean.max(0.01)).clamp(0.0, 1.0);
        (total_angle / std::f32::consts::TAU).clamp(0.0, 1.0) * radial_stability
    }

    fn body_crossings(&self, center: Vec2, radius: f32) -> usize {
        let mut previous = None;
        let mut crossings = 0;
        for sample in &self.cursor_history {
            let inside = sample.position.distance(center) < radius;
            if previous.is_some_and(|was_inside| was_inside != inside) {
                crossings += 1;
            }
            previous = Some(inside);
        }
        crossings
    }

    fn update_windows(
        &mut self,
        sensors: &SensorFrame,
        pet_position: Vec2,
        timestamp: f64,
        dt: f32,
    ) -> WindowEcology {
        let mut ecology = WindowEcology::default();
        let mut affordances = WindowAffordanceFrame::default();
        for surface in &sensors.visible_surfaces {
            let id = surface.id.0.clone();
            let previous = self.surfaces.get(&id).cloned();
            let center = rect_center(surface.rect);
            let previous_center = previous
                .as_ref()
                .map_or(center, |history| rect_center(history.rect));
            let raw_velocity = if dt > 0.0001 {
                (center - previous_center) / dt
            } else {
                Vec2::ZERO
            };
            let velocity = previous.as_ref().map_or(raw_velocity, |history| {
                history.velocity.lerp(raw_velocity, 0.55)
            });
            let age = previous
                .as_ref()
                .map_or(0.0, |history| history.age_seconds + dt);
            let (nearest, nearest_normal, inside_depth) =
                closest_point_and_normal(pet_position, surface.rect);
            let distance = nearest.distance(pet_position);
            let direction_to_pet = (pet_position - center).normalize_or_zero();
            let approach_speed = velocity.dot(direction_to_pet).max(0.0);
            let motion = (velocity.length() * 0.30).clamp(0.0, 1.0);
            let overlap_pressure = (inside_depth * 18.0).clamp(0.0, 1.0);
            let pressure =
                (overlap_pressure + approach_speed * 0.42 * (1.0 - distance * 4.0)).clamp(0.0, 1.0);
            ecology.motion = ecology.motion.max(motion);
            ecology.pressure = ecology.pressure.max(pressure);
            if ecology.nearest_edge.is_none_or(|current| {
                current.distance(pet_position) > nearest.distance(pet_position)
            }) {
                ecology.nearest_edge = Some(nearest);
            }
            let is_new = previous.is_none();
            let mut popup = 0.0;
            if is_new && age < 0.2 {
                let area = rect_area(surface.rect);
                popup =
                    ((0.32 - area).max(0.0) * 2.2 + (1.0 - distance * 2.0) * 0.35).clamp(0.0, 1.0);
                ecology.popup_salience = ecology.popup_salience.max(popup);
                if popup > 0.12 {
                    ecology.events.push(
                        StimulusEvent {
                            kind: StimulusKind::Popup,
                            position: Some(center),
                            intensity: popup,
                            novelty: 1.0,
                            threat: popup * 0.22,
                            social_relevance: 0.0,
                        }
                        .bounded(),
                    );
                }
            }
            affordances.push(WindowAffordance {
                id: WindowId(stable_hash_bytes(surface.id.0.as_bytes()).max(1)),
                bounds: NormalizedRect {
                    minimum: surface.rect.minimum,
                    maximum: surface.rect.maximum,
                },
                velocity,
                nearest_edge_point: nearest,
                nearest_edge_normal: nearest_normal,
                overlap_pressure: pressure,
                popup_pressure: popup,
                motion_energy: motion,
                is_visible: true,
            });
            if motion > 0.08 || pressure > 0.08 {
                ecology.events.push(
                    StimulusEvent {
                        kind: StimulusKind::MovingWindow,
                        position: Some(nearest),
                        intensity: motion.max(pressure),
                        novelty: (motion * 0.65 + bool_value(is_new) * 0.35).clamp(0.0, 1.0),
                        threat: pressure,
                        social_relevance: 0.0,
                    }
                    .bounded(),
                );
            }
            self.surfaces.insert(
                id,
                SurfaceHistory {
                    rect: surface.rect,
                    velocity,
                    last_seen: timestamp,
                    age_seconds: age,
                },
            );
        }
        self.surfaces.retain(|_, history| {
            timestamp - history.last_seen <= SURFACE_FORGET_SECONDS
                && history.rect.minimum.is_finite()
                && history.rect.maximum.is_finite()
        });
        affordances.finish();
        self.window_affordances = affordances;
        ecology
    }

    fn visual_delta(&self) -> f32 {
        match (self.previous_visual, self.visual) {
            (Some(previous), Some(current)) => ((current.mean_luminance - previous.mean_luminance)
                .abs()
                + (current.colorfulness - previous.colorfulness).abs() * 0.5
                + (current.dominant_hue - previous.dominant_hue)
                    .abs()
                    .min(1.0)
                    * 0.3)
                .clamp(0.0, 1.0),
            _ => 0.0,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
struct WindowEcology {
    motion: f32,
    pressure: f32,
    popup_salience: f32,
    nearest_edge: Option<Vec2>,
    events: Vec<StimulusEvent>,
}

fn push_pointer_events(
    events: &mut Vec<StimulusEvent>,
    pointer: PointerGesturePercept,
    cursor: Vec2,
) {
    let intensity = pointer
        .poke
        .max(pointer.circle)
        .max(pointer.chase_invitation)
        .max(pointer.petting)
        .max(pointer.fast_swipe);
    if intensity > 0.08 {
        events.push(
            StimulusEvent {
                kind: StimulusKind::PointerGesture,
                position: Some(cursor),
                intensity,
                novelty: pointer.circle.max(pointer.fast_swipe * 0.55),
                threat: pointer.poke * 0.35 + pointer.fast_swipe * 0.30,
                social_relevance: pointer.petting.max(pointer.chase_invitation),
            }
            .bounded(),
        );
    } else {
        events.push(
            StimulusEvent {
                kind: StimulusKind::Cursor,
                position: Some(cursor),
                intensity: pointer.approach.max(0.08),
                novelty: 0.0,
                threat: 0.0,
                social_relevance: pointer.approach * 0.20,
            }
            .bounded(),
        );
    }
}

fn push_time(events: &mut VecDeque<f64>, timestamp: f64) {
    if timestamp.is_finite() {
        events.push_back(timestamp);
        while events.len() > MAX_RHYTHM_EVENTS {
            events.pop_front();
        }
    }
}

fn prune_times(events: &mut VecDeque<f64>, now: f64, window: f64) {
    while events
        .front()
        .is_some_and(|timestamp| now - timestamp > window)
    {
        events.pop_front();
    }
}

fn event_rate(events: &VecDeque<f64>, now: f64, window: f64) -> f32 {
    if window <= 0.0 {
        return 0.0;
    }
    let count = events
        .iter()
        .filter(|timestamp| now - **timestamp <= window)
        .count();
    count as f32 / window as f32
}

fn interval_burstiness(events: &VecDeque<f64>, now: f64, window: f64) -> f32 {
    let recent: Vec<_> = events
        .iter()
        .copied()
        .filter(|timestamp| now - *timestamp <= window)
        .collect();
    if recent.len() < 3 {
        return 0.0;
    }
    let intervals: Vec<_> = recent
        .windows(2)
        .map(|pair| (pair[1] - pair[0]).max(0.0001) as f32)
        .collect();
    let mean = intervals.iter().sum::<f32>() / intervals.len() as f32;
    let standard_deviation = (intervals
        .iter()
        .map(|interval| (*interval - mean).powi(2))
        .sum::<f32>()
        / intervals.len() as f32)
        .sqrt();
    (standard_deviation / (standard_deviation + mean)).clamp(0.0, 1.0)
}

fn wrap_angle(angle: f32) -> f32 {
    (angle + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI
}

fn closest_point_on_rect(point: Vec2, rect: Rect) -> Vec2 {
    let clamped = point.clamp(rect.minimum, rect.maximum);
    let distances = [
        (clamped.x - rect.minimum.x).abs(),
        (rect.maximum.x - clamped.x).abs(),
        (clamped.y - rect.minimum.y).abs(),
        (rect.maximum.y - clamped.y).abs(),
    ];
    let edge = distances
        .iter()
        .enumerate()
        .min_by(|left, right| left.1.total_cmp(right.1))
        .map_or(0, |(index, _)| index);
    match edge {
        0 => Vec2::new(rect.minimum.x, clamped.y),
        1 => Vec2::new(rect.maximum.x, clamped.y),
        2 => Vec2::new(clamped.x, rect.minimum.y),
        _ => Vec2::new(clamped.x, rect.maximum.y),
    }
}

fn closest_point_and_normal(point: Vec2, rect: Rect) -> (Vec2, Vec2, f32) {
    let nearest = closest_point_on_rect(point, rect);
    let inside = point.cmpge(rect.minimum).all() && point.cmple(rect.maximum).all();
    if inside {
        let distances = [
            (point.x - rect.minimum.x, Vec2::NEG_X),
            (rect.maximum.x - point.x, Vec2::X),
            (point.y - rect.minimum.y, Vec2::NEG_Y),
            (rect.maximum.y - point.y, Vec2::Y),
        ];
        let (depth, normal) = distances
            .into_iter()
            .min_by(|left, right| left.0.total_cmp(&right.0))
            .unwrap_or((0.0, Vec2::Y));
        (nearest, normal, depth.max(0.0))
    } else {
        (nearest, (point - nearest).normalize_or_zero(), 0.0)
    }
}

fn rect_center(rect: Rect) -> Vec2 {
    (rect.minimum + rect.maximum) * 0.5
}

fn rect_area(rect: Rect) -> f32 {
    let extent = (rect.maximum - rect.minimum).max(Vec2::ZERO);
    extent.x * extent.y
}

fn event_priority(event: &StimulusEvent) -> f32 {
    event.intensity * 0.38
        + event.novelty * 0.25
        + event.threat * 0.27
        + event.social_relevance * 0.20
}

fn stimulus_order(kind: StimulusKind) -> u8 {
    match kind {
        StimulusKind::Popup => 0,
        StimulusKind::MovingWindow => 1,
        StimulusKind::PointerGesture => 2,
        StimulusKind::VisualChange => 3,
        StimulusKind::ScrollFlow => 4,
        StimulusKind::TypingRhythm => 5,
        StimulusKind::Cursor => 6,
        StimulusKind::WindowEdge => 7,
        StimulusKind::BrightArea => 8,
        StimulusKind::DarkArea => 9,
        StimulusKind::User => 10,
        StimulusKind::SelfBody => 11,
    }
}

fn finite(value: f32) -> f32 {
    if value.is_finite() { value } else { 0.0 }
}

fn bool_value(value: bool) -> f32 {
    if value { 1.0 } else { 0.0 }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lifecore::{SurfaceId, SurfaceRect};

    #[test]
    fn pointer_circle_is_recognized() {
        let mut runtime = PerceptionRuntime::default();
        let body = BodyFeedback::default();
        let mut sensors = SensorFrame::default();
        for tick in 0..120 {
            let phase = tick as f32 / 120.0 * std::f32::consts::TAU * 1.4;
            sensors.timestamp = tick as f64 / 60.0;
            sensors.cursor_position = Vec2::splat(0.5) + Vec2::new(phase.cos(), phase.sin()) * 0.16;
            sensors.cursor_velocity = Vec2::new(-phase.sin(), phase.cos()) * 0.32;
            sensors.cursor_distance_to_pet = sensors.cursor_position.distance(body.world_position);
            let _ = runtime.update(&sensors, &body, 1.0 / 60.0);
        }
        let frame = runtime.update(&sensors, &body, 1.0 / 60.0);
        assert!(frame.pointer.circle > 0.35);
        assert!(frame.pointer.chase_invitation > 0.20);
    }

    #[test]
    fn window_motion_creates_pressure_event() {
        let mut runtime = PerceptionRuntime::default();
        let body = BodyFeedback::default();
        let mut sensors = SensorFrame::default();
        for tick in 0..30 {
            sensors.timestamp = tick as f64 / 60.0;
            let left = 0.92 - tick as f32 * 0.012;
            sensors.visible_surfaces = vec![SurfaceRect {
                id: SurfaceId("moving".into()),
                rect: Rect {
                    minimum: Vec2::new(left, 0.35),
                    maximum: Vec2::new(left + 0.24, 0.70),
                },
            }];
            let _ = runtime.update(&sensors, &body, 1.0 / 60.0);
        }
        let frame = runtime.update(&sensors, &body, 1.0 / 60.0);
        assert!(frame.window_motion > 0.05);
        assert!(
            frame
                .events
                .iter()
                .any(|event| event.kind == StimulusKind::MovingWindow)
        );
        let affordances = runtime.window_affordances();
        assert_eq!(affordances.count, 1);
        assert!(affordances.windows[0].velocity.length() > 0.01);
        assert!(affordances.windows[0].id.0 != 0);
        assert!(affordances.windows[0].is_valid());
    }

    #[test]
    fn window_affordance_exposes_geometry_but_not_native_identity() {
        let mut runtime = PerceptionRuntime::default();
        let sensors = SensorFrame {
            timestamp: 1.0,
            visible_surfaces: vec![SurfaceRect {
                id: SurfaceId("window:0xDEADBEEF".into()),
                rect: Rect {
                    minimum: Vec2::new(0.4, 0.4),
                    maximum: Vec2::new(0.7, 0.7),
                },
            }],
            ..SensorFrame::default()
        };
        let _ = runtime.update(&sensors, &BodyFeedback::default(), 1.0 / 60.0);
        let affordance = runtime.window_affordances().windows[0];
        assert!(affordance.bounds.is_valid());
        assert_ne!(affordance.id.0, 0xDEAD_BEEF);
    }

    #[test]
    fn no_input_content_is_represented() {
        let mut runtime = PerceptionRuntime::default();
        runtime.note_key_activity(1.0);
        runtime.note_key_activity(1.1);
        runtime.note_scroll(0.7);
        let sensors = SensorFrame {
            timestamp: 1.2,
            ..SensorFrame::default()
        };
        let frame = runtime.update(&sensors, &BodyFeedback::default(), 1.0 / 60.0);
        let json = serde_json::to_string(&frame).unwrap();
        assert!(!json.contains("key_code"));
        assert!(!json.contains("character"));
        assert!(frame.typing_rate_hz > 0.0);
        assert!(frame.scroll_velocity > 0.0);
    }
}
