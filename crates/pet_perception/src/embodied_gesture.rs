use std::f32::consts::TAU;

use glam::Vec2;
use lifecore::{
    ComponentLifecycle, EmbodiedGestureEvent, EmbodiedGestureKind, EmbodiedInteractionFrame,
    GestureBoundaryEvent, GestureCause, GestureClassification, MAX_GESTURE_CAUSES,
};
use pet_ecology::{ActionSignature, GestureSignature};

const MAX_SAMPLES: usize = 256;
const MAX_CONTACT_ONSETS: usize = 16;
const CLASSIFIER_PERIOD_SECONDS: f64 = 1.0 / 20.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EmbodiedGestureClassifierTuning {
    pub window_seconds: f32,
    pub commit_confidence: f32,
    pub ambiguity_margin: f32,
    pub soft_touch_pressure_max: f32,
    pub stretch_strain_min: f32,
    pub stretch_strain_max: f32,
    pub flick_speed_min: f32,
    pub rhythm_interval_cv_max: f32,
    pub rhythm_min_impulses: u8,
    pub boundary_strain: f32,
}

impl Default for EmbodiedGestureClassifierTuning {
    fn default() -> Self {
        Self {
            window_seconds: 2.0,
            commit_confidence: 0.62,
            ambiguity_margin: 0.12,
            soft_touch_pressure_max: 0.24,
            stretch_strain_min: 0.14,
            stretch_strain_max: 0.58,
            flick_speed_min: 2.40,
            rhythm_interval_cv_max: 0.20,
            rhythm_min_impulses: 3,
            boundary_strain: 0.72,
        }
    }
}

impl EmbodiedGestureClassifierTuning {
    fn sanitized(mut self) -> Self {
        self.window_seconds = finite(self.window_seconds).clamp(0.5, 4.0);
        self.commit_confidence = finite(self.commit_confidence).clamp(0.50, 0.85);
        self.ambiguity_margin = finite(self.ambiguity_margin).clamp(0.05, 0.35);
        self.soft_touch_pressure_max = finite(self.soft_touch_pressure_max).clamp(0.05, 0.50);
        self.stretch_strain_min = finite(self.stretch_strain_min).clamp(0.05, 0.45);
        self.stretch_strain_max =
            finite(self.stretch_strain_max).clamp(self.stretch_strain_min + 0.05, 1.20);
        self.flick_speed_min = finite(self.flick_speed_min).clamp(0.5, 8.0);
        self.rhythm_interval_cv_max = finite(self.rhythm_interval_cv_max).clamp(0.05, 0.50);
        self.rhythm_min_impulses = self.rhythm_min_impulses.clamp(3, 8);
        self.boundary_strain = finite(self.boundary_strain).clamp(0.45, 1.20);
        self
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct GestureSample {
    timestamp: f64,
    active: bool,
    point_local: Vec2,
    pressure: f32,
    area: f32,
    speed: f32,
    acceleration: f32,
    relative_speed: f32,
    strain: f32,
    neck_tension: f32,
    neck_thickness: f32,
    deformation: f32,
    slosh: f32,
    component_count: u8,
    detached_mass: f32,
    fragment_toward_main: f32,
    topology_attempt: f32,
}

impl GestureSample {
    fn from_frame(frame: EmbodiedInteractionFrame) -> Self {
        let fragment_toward_main = frame.components
            [..usize::from(frame.component_observation_count)]
            .iter()
            .filter(|component| {
                component.lifecycle != ComponentLifecycle::Attached
                    && component.lifecycle != ComponentLifecycle::Recovered
            })
            .map(|component| {
                let direction = (-component.center_local).normalize_or_zero();
                direction.dot(component.velocity_local).max(0.0)
            })
            .fold(0.0_f32, f32::max);
        Self {
            timestamp: frame.timestamp,
            active: frame.contact.active,
            point_local: frame.contact.point_local,
            pressure: frame.contact.effective_pressure,
            area: frame.contact.area_fraction,
            speed: frame.contact.pointer_speed,
            acceleration: frame.contact.pointer_acceleration,
            relative_speed: frame.contact.relative_velocity_local.length(),
            strain: frame.material.maximum_strain,
            neck_tension: frame.material.neck_tension,
            neck_thickness: frame.material.neck_thickness,
            deformation: frame.material.deformation_energy,
            slosh: frame.material.slosh_energy,
            component_count: frame.material.component_count,
            detached_mass: frame.material.detached_mass_fraction,
            fragment_toward_main,
            topology_attempt: if frame.material.topology_budget_exhausted {
                1.0
            } else {
                0.0
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct EmbodiedGestureClassifier {
    samples: [GestureSample; MAX_SAMPLES],
    sample_head: usize,
    sample_len: usize,
    contact_onsets: [f64; MAX_CONTACT_ONSETS],
    onset_head: usize,
    onset_len: usize,
    tuning: EmbodiedGestureClassifierTuning,
    next_episode_id: u64,
    episode_id: u64,
    episode_active: bool,
    episode_committed: bool,
    previous_contact: bool,
    previous_topology_exhausted: bool,
    previous_boundary: GestureBoundaryEvent,
    last_classification_at: f64,
    predicted_kind: EmbodiedGestureKind,
    prediction_confidence: f32,
    latest: GestureClassification,
    latest_signature: Option<GestureSignature>,
}

impl Default for EmbodiedGestureClassifier {
    fn default() -> Self {
        Self {
            samples: [GestureSample::default(); MAX_SAMPLES],
            sample_head: 0,
            sample_len: 0,
            contact_onsets: [0.0; MAX_CONTACT_ONSETS],
            onset_head: 0,
            onset_len: 0,
            tuning: EmbodiedGestureClassifierTuning::default(),
            next_episode_id: 1,
            episode_id: 0,
            episode_active: false,
            episode_committed: false,
            previous_contact: false,
            previous_topology_exhausted: false,
            previous_boundary: GestureBoundaryEvent::None,
            last_classification_at: f64::NEG_INFINITY,
            predicted_kind: EmbodiedGestureKind::Unknown,
            prediction_confidence: 0.0,
            latest: GestureClassification::default(),
            latest_signature: None,
        }
    }
}

impl EmbodiedGestureClassifier {
    pub fn reserve_episode_ids_before(&mut self, next: u64) {
        self.next_episode_id = self.next_episode_id.max(next).max(1);
    }

    pub const fn next_episode_id(&self) -> u64 {
        self.next_episode_id
    }

    pub fn set_tuning(&mut self, tuning: EmbodiedGestureClassifierTuning) {
        self.tuning = tuning.sanitized();
    }

    #[must_use]
    pub const fn latest(&self) -> GestureClassification {
        self.latest
    }

    #[must_use]
    pub fn take_latest_signature(&mut self) -> Option<GestureSignature> {
        self.latest_signature.take()
    }

    /// Ingests only body-local, bounded physical summaries. No native pointer
    /// identifier, desktop origin, raw path, or screen content enters this state.
    pub fn ingest(&mut self, mut frame: EmbodiedInteractionFrame) -> Option<EmbodiedGestureEvent> {
        frame.sanitize();
        let sample = GestureSample::from_frame(frame);
        let contact_onset = sample.active && !self.previous_contact;
        let contact_release = !sample.active && self.previous_contact;
        self.previous_contact = sample.active;
        let topology_edge =
            frame.material.topology_budget_exhausted && !self.previous_topology_exhausted;
        self.previous_topology_exhausted = frame.material.topology_budget_exhausted;
        if contact_onset {
            self.push_onset(sample.timestamp);
        }
        let physical_edge = contact_onset
            || contact_release
            || frame.detached_event.is_some()
            || frame.remerge_event.is_some()
            || frame.recovery_event.is_some()
            || topology_edge;
        if !self.episode_active && (sample.active || physical_edge) {
            self.begin_episode();
        }
        if !self.episode_active {
            return None;
        }
        self.push_sample(sample);
        self.prune_samples(sample.timestamp);

        let boundary = boundary_for(frame, self.tuning);
        let boundary_edge =
            boundary != GestureBoundaryEvent::None && boundary != self.previous_boundary;
        self.previous_boundary = boundary;
        let due = sample.timestamp - self.last_classification_at >= CLASSIFIER_PERIOD_SECONDS;
        if !due && !physical_edge && !boundary_edge {
            return None;
        }
        self.last_classification_at = sample.timestamp;
        let ended = contact_release || (physical_edge && !sample.active);
        let mut classification = self.classify(ended, boundary);
        let new_commit = classification.committed && !self.episode_committed;
        self.episode_committed |= classification.committed;
        classification.committed = new_commit || (ended && self.episode_committed);
        self.latest = classification;

        let should_emit = new_commit || ended || boundary_edge;
        if should_emit {
            self.latest_signature = self.gesture_signature();
        }
        let event = should_emit.then_some(EmbodiedGestureEvent {
            classification,
            frame,
            boundary,
            observation_quality: observation_quality(self.sample_len, frame),
        });
        if ended {
            if classification.kind != EmbodiedGestureKind::Unknown
                && classification.confidence >= self.tuning.commit_confidence
            {
                self.predicted_kind = classification.kind;
                self.prediction_confidence = classification.confidence;
            } else {
                self.prediction_confidence *= 0.75;
            }
            self.episode_active = false;
            self.episode_committed = false;
            self.previous_boundary = GestureBoundaryEvent::None;
            self.sample_head = 0;
            self.sample_len = 0;
        }
        event
    }

    fn begin_episode(&mut self) {
        self.episode_id = self.next_episode_id;
        self.next_episode_id = self.next_episode_id.saturating_add(1);
        self.episode_active = true;
        self.episode_committed = false;
        self.sample_head = 0;
        self.sample_len = 0;
    }

    fn push_sample(&mut self, sample: GestureSample) {
        let index = (self.sample_head + self.sample_len) % MAX_SAMPLES;
        self.samples[index] = sample;
        if self.sample_len < MAX_SAMPLES {
            self.sample_len += 1;
        } else {
            self.sample_head = (self.sample_head + 1) % MAX_SAMPLES;
        }
    }

    fn push_onset(&mut self, timestamp: f64) {
        let index = (self.onset_head + self.onset_len) % MAX_CONTACT_ONSETS;
        self.contact_onsets[index] = timestamp;
        if self.onset_len < MAX_CONTACT_ONSETS {
            self.onset_len += 1;
        } else {
            self.onset_head = (self.onset_head + 1) % MAX_CONTACT_ONSETS;
        }
    }

    fn prune_samples(&mut self, now: f64) {
        while self.sample_len > 0 {
            let age = now - self.samples[self.sample_head].timestamp;
            if age <= f64::from(self.tuning.window_seconds) {
                break;
            }
            self.sample_head = (self.sample_head + 1) % MAX_SAMPLES;
            self.sample_len -= 1;
        }
    }

    fn sample(&self, logical: usize) -> GestureSample {
        self.samples[(self.sample_head + logical) % MAX_SAMPLES]
    }

    fn classify(&self, ended: bool, boundary: GestureBoundaryEvent) -> GestureClassification {
        let features = Features::extract(self);
        let t = self.tuning;
        let scores = [
            (
                EmbodiedGestureKind::SoftTouch,
                mean(&[
                    band(features.duration, 0.08, 0.80, 0.16),
                    low(features.pressure_max, t.soft_touch_pressure_max, 0.20),
                    low(features.relative_speed_mean, 0.20, 0.35),
                    low(features.strain_peak, t.stretch_strain_min, 0.18),
                    low(features.acceleration_max, 1.2, 3.0),
                    1.0 - features.component_transition,
                ]) * low(features.detached_mass_peak, 0.01, 0.04),
            ),
            (
                EmbodiedGestureKind::SlowStretch,
                mean(&[
                    high(features.duration, 0.35, 0.45),
                    high(features.radial_displacement, 0.12, 0.25),
                    band(features.speed_mean, 0.03, 0.30, 0.18),
                    band(
                        features.strain_peak,
                        t.stretch_strain_min,
                        t.stretch_strain_max,
                        0.22,
                    ),
                    high(features.strain_slope, 0.02, 0.45),
                    low(features.jerk_mean, 8.0, 18.0),
                ]),
            ),
            (
                EmbodiedGestureKind::Tickle,
                mean(&[
                    low(features.pressure_mean, 0.28, 0.25),
                    band(features.direction_change_hz, 4.0, 12.0, 3.0),
                    low(features.spatial_extent, 0.28, 0.35),
                    high(features.direction_change_hz, 3.0, 6.0),
                    low(features.neck_peak, 0.62, 0.30),
                ]),
            ),
            (
                EmbodiedGestureKind::RhythmicTouch,
                mean(&[
                    high(
                        f32::from(features.onset_count),
                        f32::from(t.rhythm_min_impulses.saturating_sub(1)),
                        1.0,
                    ),
                    band(features.onset_frequency, 1.0, 5.0, 1.5),
                    low(features.onset_cv, t.rhythm_interval_cv_max, 0.25),
                    band(features.motif_duration, 0.5, 4.0, 0.75),
                    low(features.pressure_max, 0.75, 0.25),
                ]),
            ),
            (
                EmbodiedGestureKind::CircularTwist,
                mean(&[
                    high(features.accumulated_turn.abs(), 0.65, 0.75),
                    low(features.closure_error, 0.30, 0.40),
                    high(features.tangential_radial_ratio, 1.5, 2.0),
                    low(features.pressure_max, 0.70, 0.30),
                    high(features.area_mean, 0.04, 0.12),
                ]),
            ),
            (
                EmbodiedGestureKind::SharpFlick,
                mean(&[
                    low(features.duration, 0.30, 0.25),
                    high(features.release_speed, t.flick_speed_min, 2.4),
                    high(features.acceleration_max, 2.4, 5.0),
                    high(features.jerk_mean, 8.0, 24.0),
                    high(features.deformation_drop, 0.08, 0.35),
                    high(features.slosh_peak, 0.08, 0.35),
                ]),
            ),
            (
                EmbodiedGestureKind::Hold,
                mean(&[
                    high(features.duration, 0.65, 0.65),
                    low(features.speed_mean, 0.05, 0.12),
                    low(features.pressure_variance, 0.025, 0.10),
                    low(features.strain_variation, 0.05, 0.20),
                ]),
            ),
            (
                EmbodiedGestureKind::PullAndRelease,
                mean(&[
                    high(features.strain_peak, t.stretch_strain_min, 0.35),
                    if ended { 1.0 } else { 0.0 },
                    high(features.deformation_drop, 0.10, 0.35),
                    high(features.release_speed, 0.45, 1.5),
                    high(features.slosh_peak, 0.10, 0.35),
                ]),
            ),
            (
                EmbodiedGestureKind::FragmentSeparationAttempt,
                mean(&[
                    high(features.neck_peak, 0.62, 0.30),
                    low(features.neck_thickness_min, 0.42, 0.30),
                    high(features.strain_integral, 0.08, 0.30),
                    high(
                        features.component_transition.max(features.topology_attempt),
                        0.5,
                        0.5,
                    ),
                ]),
            ),
            (
                EmbodiedGestureKind::SharedPlayInvitation,
                mean(&[
                    high(f32::from(features.onset_count), 2.0, 3.0),
                    high(features.direction_change_hz, 1.0, 4.0),
                    low(features.pressure_max, 0.35, 0.30),
                    low(features.neck_peak, 0.50, 0.30),
                    band(features.motif_duration, 0.5, 4.0, 1.0),
                ]),
            ),
            (
                EmbodiedGestureKind::FragmentHelp,
                mean(&[
                    high(features.detached_mass_peak, 0.02, 0.16),
                    high(features.fragment_toward_main, 0.05, 0.50),
                    low(features.pressure_mean, 0.35, 0.25),
                    high(features.distance_drop, 0.02, 0.20),
                ]),
            ),
        ];
        let mut best = (EmbodiedGestureKind::Unknown, 0.0_f32);
        let mut second = 0.0_f32;
        for candidate in scores {
            if candidate.1 > best.1 {
                second = best.1;
                best = candidate;
            } else if candidate.1 > second {
                second = candidate.1;
            }
        }
        let quality = features.quality;
        let duration_completeness = (features.duration / 0.35).clamp(0.35, 1.0);
        let separation = ((best.1 - second) / best.1.max(1.0e-5)).clamp(0.0, 1.0);
        let confidence =
            (best.1 * quality * (0.55 + separation * 0.45) * duration_completeness).clamp(0.0, 1.0);
        let margin = best.1 - second;
        let safety_commit = boundary != GestureBoundaryEvent::None
            && (features.neck_peak >= 0.62
                || features.pressure_max >= 0.85
                || features.topology_attempt > 0.5);
        let committed = (best.1 >= t.commit_confidence
            && margin >= t.ambiguity_margin
            && confidence >= t.commit_confidence * 0.72)
            || safety_commit;
        let kind = if committed || ended {
            best.0
        } else {
            EmbodiedGestureKind::Unknown
        };
        let categorical_surprise =
            if self.predicted_kind == EmbodiedGestureKind::Unknown || self.predicted_kind == kind {
                0.0
            } else {
                self.prediction_confidence
            };
        let physical_magnitude_error = (features.intensity - self.latest.intensity)
            .abs()
            .clamp(0.0, 1.0);
        let timing_error = if self.latest.episode_id == 0 {
            0.0
        } else {
            ((features.duration - 0.65).abs() / 2.0).clamp(0.0, 1.0)
        };
        let topology_outcome_error = if kind == EmbodiedGestureKind::FragmentSeparationAttempt {
            (1.0 - features.component_transition).max(features.topology_attempt * 0.5)
        } else {
            features.component_transition * 0.25
        };
        let mut result = GestureClassification {
            episode_id: self.episode_id,
            kind,
            confidence,
            second_best_confidence: second.clamp(0.0, confidence),
            intensity: features.intensity,
            predicted_kind: self.predicted_kind,
            prediction_confidence: self.prediction_confidence,
            prediction_error: (categorical_surprise * 0.45
                + physical_magnitude_error * 0.25
                + timing_error * 0.20
                + topology_outcome_error * 0.10)
                .clamp(0.0, 1.0),
            causes: [GestureCause::None; MAX_GESTURE_CAUSES],
            cause_count: 0,
            committed,
            ended,
        };
        add_causes(&mut result, features);
        result.sanitize();
        result
    }

    fn gesture_signature(&self) -> Option<GestureSignature> {
        if self.sample_len < 2 {
            return None;
        }
        let features = Features::extract(self);
        let mut points = [Vec2::ZERO; MAX_SAMPLES];
        let mut point_count = 0_usize;
        let mut active_count = 0_usize;
        for logical in 0..self.sample_len {
            let sample = self.sample(logical);
            if sample.active {
                active_count += 1;
                points[point_count] = sample.point_local;
                point_count += 1;
            }
        }
        if point_count < 2 {
            return None;
        }
        let duration = features.duration.clamp(0.05, 6.0);
        let path = ActionSignature::from_trace(&points[..point_count], duration)?;
        let (rhythm_intervals, rhythm_count) =
            normalized_onset_intervals(self, self.sample(self.sample_len - 1).timestamp);
        let signature = GestureSignature {
            path,
            pressure_mean: features.pressure_mean.clamp(0.0, 1.0),
            pressure_variance: features.pressure_variance.clamp(0.0, 1.0),
            strain_peak: features.strain_peak.clamp(0.0, 8.0),
            release_speed: features.release_speed.clamp(0.0, 4.0),
            duty_cycle: (active_count as f32 / self.sample_len as f32).clamp(0.0, 1.0),
            rhythm_intervals,
            rhythm_count,
        };
        signature.is_valid().then_some(signature)
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct Features {
    duration: f32,
    motif_duration: f32,
    area_mean: f32,
    pressure_mean: f32,
    pressure_max: f32,
    pressure_variance: f32,
    speed_mean: f32,
    acceleration_max: f32,
    jerk_mean: f32,
    relative_speed_mean: f32,
    radial_displacement: f32,
    tangential_radial_ratio: f32,
    accumulated_turn: f32,
    closure_error: f32,
    direction_change_hz: f32,
    spatial_extent: f32,
    strain_peak: f32,
    strain_slope: f32,
    strain_integral: f32,
    strain_variation: f32,
    neck_peak: f32,
    neck_thickness_min: f32,
    deformation_drop: f32,
    slosh_peak: f32,
    release_speed: f32,
    onset_count: u8,
    onset_frequency: f32,
    onset_cv: f32,
    component_transition: f32,
    detached_mass_peak: f32,
    fragment_toward_main: f32,
    distance_drop: f32,
    topology_attempt: f32,
    intensity: f32,
    quality: f32,
}

impl Features {
    fn extract(classifier: &EmbodiedGestureClassifier) -> Self {
        if classifier.sample_len == 0 {
            return Self::default();
        }
        let first = classifier.sample(0);
        let last = classifier.sample(classifier.sample_len - 1);
        let duration = (last.timestamp - first.timestamp).max(0.0) as f32;
        let mut feature = Self {
            duration,
            motif_duration: duration,
            pressure_max: first.pressure,
            acceleration_max: first.acceleration,
            strain_peak: first.strain,
            neck_peak: first.neck_tension,
            neck_thickness_min: first.neck_thickness,
            slosh_peak: first.slosh,
            detached_mass_peak: first.detached_mass,
            fragment_toward_main: first.fragment_toward_main,
            topology_attempt: first.topology_attempt,
            quality: (classifier.sample_len as f32 / 10.0).clamp(0.35, 1.0),
            ..Self::default()
        };
        let mut pressure_squared = 0.0_f32;
        let mut strain_min = first.strain;
        let mut deformation_peak = first.deformation;
        let mut radial_min = first.point_local.length();
        let mut radial_max = radial_min;
        let mut min_point = first.point_local;
        let mut max_point = first.point_local;
        let mut tangential_motion = 0.0_f32;
        let mut radial_motion = 0.0_f32;
        let mut direction_changes = 0_u32;
        let mut previous_direction = Vec2::ZERO;
        let mut previous_acceleration = first.acceleration;
        let mut previous = first;
        let mut distance_first = f32::INFINITY;
        let mut distance_last = f32::INFINITY;
        for logical in 0..classifier.sample_len {
            let sample = classifier.sample(logical);
            feature.area_mean += sample.area;
            feature.pressure_mean += sample.pressure;
            feature.speed_mean += sample.speed;
            feature.relative_speed_mean += sample.relative_speed;
            pressure_squared += sample.pressure * sample.pressure;
            feature.pressure_max = feature.pressure_max.max(sample.pressure);
            feature.acceleration_max = feature.acceleration_max.max(sample.acceleration);
            feature.strain_peak = feature.strain_peak.max(sample.strain);
            strain_min = strain_min.min(sample.strain);
            feature.neck_peak = feature.neck_peak.max(sample.neck_tension);
            feature.neck_thickness_min = feature.neck_thickness_min.min(sample.neck_thickness);
            deformation_peak = deformation_peak.max(sample.deformation);
            feature.slosh_peak = feature.slosh_peak.max(sample.slosh);
            feature.detached_mass_peak = feature.detached_mass_peak.max(sample.detached_mass);
            feature.fragment_toward_main = feature
                .fragment_toward_main
                .max(sample.fragment_toward_main);
            feature.component_transition = feature.component_transition.max(
                if sample.component_count != first.component_count {
                    1.0
                } else {
                    0.0
                },
            );
            feature.topology_attempt = feature.topology_attempt.max(sample.topology_attempt);
            feature.strain_integral += sample.strain * (1.0 / 120.0);
            let radius = sample.point_local.length();
            radial_min = radial_min.min(radius);
            radial_max = radial_max.max(radius);
            min_point = min_point.min(sample.point_local);
            max_point = max_point.max(sample.point_local);
            if logical > 0 {
                let delta = sample.point_local - previous.point_local;
                let direction = delta.normalize_or_zero();
                let radial = previous.point_local.normalize_or_zero();
                radial_motion += delta.dot(radial).abs();
                tangential_motion += delta.perp_dot(radial).abs();
                if previous.point_local.length_squared() > 1.0e-6
                    && sample.point_local.length_squared() > 1.0e-6
                {
                    feature.accumulated_turn += previous
                        .point_local
                        .perp_dot(sample.point_local)
                        .atan2(previous.point_local.dot(sample.point_local))
                        / TAU;
                }
                if previous_direction.length_squared() > 0.5
                    && direction.length_squared() > 0.5
                    && previous_direction.dot(direction) < 0.50
                {
                    direction_changes += 1;
                }
                if direction.length_squared() > 0.5 {
                    previous_direction = direction;
                }
                feature.jerk_mean += (sample.acceleration - previous_acceleration).abs()
                    / ((sample.timestamp - previous.timestamp).max(1.0 / 240.0) as f32);
            }
            if sample.detached_mass > 0.0 {
                let distance = sample.point_local.length();
                if !distance_first.is_finite() {
                    distance_first = distance;
                }
                distance_last = distance;
            }
            previous_acceleration = sample.acceleration;
            previous = sample;
        }
        let count = classifier.sample_len as f32;
        feature.area_mean /= count;
        feature.pressure_mean /= count;
        feature.speed_mean /= count;
        feature.relative_speed_mean /= count;
        feature.pressure_variance =
            (pressure_squared / count - feature.pressure_mean.powi(2)).max(0.0);
        feature.jerk_mean /= (count - 1.0).max(1.0);
        feature.radial_displacement = radial_max - radial_min;
        feature.tangential_radial_ratio = tangential_motion / radial_motion.max(1.0e-4);
        feature.closure_error = last.point_local.distance(first.point_local)
            / (max_point - min_point).length().max(0.02);
        feature.direction_change_hz = direction_changes as f32 / duration.max(1.0 / 120.0);
        feature.spatial_extent = (max_point - min_point).length();
        feature.strain_slope = (last.strain - first.strain) / duration.max(1.0 / 120.0);
        feature.strain_variation = feature.strain_peak - strain_min;
        feature.deformation_drop = (deformation_peak - last.deformation).max(0.0);
        feature.release_speed = if !last.active {
            let start = classifier.sample_len.saturating_sub(6);
            (start..classifier.sample_len)
                .map(|logical| classifier.sample(logical).speed)
                .fold(0.0_f32, f32::max)
        } else {
            0.0
        };
        feature.distance_drop = if distance_first.is_finite() && distance_last.is_finite() {
            (distance_first - distance_last).max(0.0)
        } else {
            0.0
        };
        let (onset_count, onset_frequency, onset_cv, motif_duration) =
            onset_features(classifier, last.timestamp);
        feature.onset_count = onset_count;
        feature.onset_frequency = onset_frequency;
        feature.onset_cv = onset_cv;
        feature.motif_duration = motif_duration.max(duration);
        feature.intensity = feature
            .pressure_max
            .max(feature.strain_peak)
            .max(feature.slosh_peak)
            .max((feature.release_speed / 4.0).clamp(0.0, 1.0));
        feature
    }
}

fn normalized_onset_intervals(classifier: &EmbodiedGestureClassifier, now: f64) -> ([f32; 8], u8) {
    let mut onsets = [0.0_f64; MAX_CONTACT_ONSETS];
    let mut onset_count = 0_usize;
    for logical in 0..classifier.onset_len {
        let timestamp =
            classifier.contact_onsets[(classifier.onset_head + logical) % MAX_CONTACT_ONSETS];
        if now - timestamp <= 4.0 {
            onsets[onset_count] = timestamp;
            onset_count += 1;
        }
    }
    let interval_count = onset_count.saturating_sub(1).min(8);
    if interval_count == 0 {
        return ([0.0; 8], 0);
    }
    let first_interval = onset_count - interval_count - 1;
    let mean = (first_interval..first_interval + interval_count)
        .map(|index| (onsets[index + 1] - onsets[index]).max(1.0e-4) as f32)
        .sum::<f32>()
        / interval_count as f32;
    let mut intervals = [0.0_f32; 8];
    for (output, index) in intervals
        .iter_mut()
        .zip(first_interval..first_interval + interval_count)
    {
        *output = (((onsets[index + 1] - onsets[index]) as f32) / mean).clamp(0.1, 4.0);
    }
    (intervals, interval_count as u8)
}

fn onset_features(classifier: &EmbodiedGestureClassifier, now: f64) -> (u8, f32, f32, f32) {
    let mut values = [0.0_f64; MAX_CONTACT_ONSETS];
    let mut count = 0_usize;
    for logical in 0..classifier.onset_len {
        let timestamp =
            classifier.contact_onsets[(classifier.onset_head + logical) % MAX_CONTACT_ONSETS];
        if now - timestamp <= 4.0 {
            values[count] = timestamp;
            count += 1;
        }
    }
    if count < 2 {
        return (count as u8, 0.0, 1.0, 0.0);
    }
    let mut mean = 0.0_f64;
    for index in 1..count {
        mean += values[index] - values[index - 1];
    }
    mean /= (count - 1) as f64;
    let mut variance = 0.0_f64;
    for index in 1..count {
        variance += ((values[index] - values[index - 1]) - mean).powi(2);
    }
    variance /= (count - 1) as f64;
    (
        count as u8,
        (1.0 / mean.max(1.0e-4)) as f32,
        (variance.sqrt() / mean.max(1.0e-4)) as f32,
        (values[count - 1] - values[0]) as f32,
    )
}

fn boundary_for(
    frame: EmbodiedInteractionFrame,
    tuning: EmbodiedGestureClassifierTuning,
) -> GestureBoundaryEvent {
    if frame.material.topology_budget_exhausted {
        GestureBoundaryEvent::TopologyBudgetExhausted
    } else if frame.material.maximum_strain >= tuning.boundary_strain
        || frame.material.neck_tension >= 0.92
    {
        GestureBoundaryEvent::Overstrain
    } else if frame.contact.effective_pressure >= 0.85 {
        GestureBoundaryEvent::ExcessivePressure
    } else {
        GestureBoundaryEvent::None
    }
}

fn observation_quality(sample_count: usize, frame: EmbodiedInteractionFrame) -> f32 {
    let continuity = (sample_count as f32 / 12.0).clamp(0.35, 1.0);
    let mass_quality = (1.0 - frame.material.mass_conservation_error * 20.0).clamp(0.0, 1.0);
    (continuity * mass_quality).clamp(0.0, 1.0)
}

fn add_causes(classification: &mut GestureClassification, features: Features) {
    let candidates = [
        (features.duration > 0.0, GestureCause::ContactOnset),
        (features.pressure_mean < 0.30, GestureCause::LowPressure),
        (features.duration > 0.65, GestureCause::SustainedPressure),
        (
            features.radial_displacement > 0.10,
            GestureCause::RadialStretch,
        ),
        (
            features.tangential_radial_ratio > 1.5,
            GestureCause::TangentialMotion,
        ),
        (features.release_speed > 1.0, GestureCause::HighReleaseSpeed),
        (
            features.acceleration_max > 2.4,
            GestureCause::HighAcceleration,
        ),
        (
            features.onset_count >= 3 && features.onset_cv < 0.25,
            GestureCause::RegularImpulseTrain,
        ),
        (
            features.accumulated_turn.abs() > 0.60 && features.closure_error < 0.35,
            GestureCause::ClosedCircularPath,
        ),
        (features.neck_peak > 0.60, GestureCause::RisingNeckTension),
        (
            features.component_transition > 0.5,
            GestureCause::ComponentSplit,
        ),
        (
            features.fragment_toward_main > 0.05,
            GestureCause::FragmentTowardMain,
        ),
    ];
    for (present, cause) in candidates {
        if !present || usize::from(classification.cause_count) >= MAX_GESTURE_CAUSES {
            continue;
        }
        classification.causes[usize::from(classification.cause_count)] = cause;
        classification.cause_count += 1;
    }
    if classification.prediction_error > 0.45
        && usize::from(classification.cause_count) < MAX_GESTURE_CAUSES
    {
        classification.causes[usize::from(classification.cause_count)] =
            GestureCause::PredictionMismatch;
        classification.cause_count += 1;
    }
}

fn band(value: f32, minimum: f32, maximum: f32, shoulder: f32) -> f32 {
    high(value, minimum, shoulder) * low(value, maximum, shoulder)
}

fn high(value: f32, threshold: f32, width: f32) -> f32 {
    ((value - threshold) / width.max(1.0e-5)).clamp(0.0, 1.0)
}

fn low(value: f32, threshold: f32, width: f32) -> f32 {
    1.0 - high(value, threshold, width)
}

fn mean(values: &[f32]) -> f32 {
    values.iter().sum::<f32>() / values.len().max(1) as f32
}

fn finite(value: f32) -> f32 {
    if value.is_finite() { value } else { 0.0 }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lifecore::{BodyMaterialState, PointerMaterialContact};

    fn frame(time: f64, active: bool, point: Vec2, pressure: f32) -> EmbodiedInteractionFrame {
        EmbodiedInteractionFrame {
            timestamp: time,
            contact: PointerMaterialContact {
                active,
                point_local: point,
                area_fraction: if active { 0.12 } else { 0.0 },
                effective_pressure: pressure,
                pointer_speed: 0.08,
                ..PointerMaterialContact::default()
            },
            material: BodyMaterialState {
                component_count: 1,
                topology_budget_remaining: 1.0,
                ..BodyMaterialState::default()
            },
            ..EmbodiedInteractionFrame::default()
        }
    }

    #[test]
    fn soft_touch_requires_low_pressure_and_low_strain() {
        let mut first = EmbodiedGestureClassifier::default();
        let mut second = EmbodiedGestureClassifier::default();
        let mut first_events = Vec::new();
        let mut second_events = Vec::new();
        for index in 0..62 {
            let active = index < 61;
            let sample = frame(index as f64 / 120.0, active, Vec2::new(0.05, 0.02), 0.10);
            if let Some(event) = first.ingest(sample) {
                first_events.push(event);
            }
            if let Some(event) = second.ingest(sample) {
                second_events.push(event);
            }
        }
        assert_eq!(first_events, second_events);
        assert!(first_events.iter().any(|event| {
            event.classification.kind == EmbodiedGestureKind::SoftTouch
                && event.classification.committed
        }));
    }

    #[test]
    fn mouse_button_alone_does_not_classify_a_gesture() {
        let mut classifier = EmbodiedGestureClassifier::default();
        let event = classifier.ingest(frame(0.0, true, Vec2::ZERO, 0.0));
        assert!(event.is_none_or(|event| !event.classification.committed));
        assert_eq!(classifier.latest().kind, EmbodiedGestureKind::Unknown);
    }

    #[test]
    fn slow_stretch_and_sharp_flick_are_causally_distinct() {
        let mut slow = EmbodiedGestureClassifier::default();
        slow.begin_episode();
        for index in 0..80 {
            let phase = index as f32 / 79.0;
            slow.push_sample(GestureSample {
                timestamp: index as f64 / 120.0,
                active: true,
                point_local: Vec2::new(0.05 + phase * 0.28, 0.0),
                pressure: 0.38,
                area: 0.12,
                speed: 0.18,
                relative_speed: 0.16,
                strain: 0.05 + phase * 0.28,
                neck_thickness: 0.75 - phase * 0.25,
                deformation: phase * 0.30,
                component_count: 1,
                ..GestureSample::default()
            });
        }
        let slow_result = slow.classify(true, GestureBoundaryEvent::None);

        let mut flick = EmbodiedGestureClassifier::default();
        flick.begin_episode();
        for index in 0..16 {
            let phase = index as f32 / 15.0;
            flick.push_sample(GestureSample {
                timestamp: index as f64 / 120.0,
                active: index < 15,
                point_local: Vec2::new(0.04 + phase * 0.42, 0.0),
                pressure: 0.35,
                area: 0.10,
                speed: if index >= 10 { 4.2 } else { 0.2 },
                acceleration: if index % 2 == 0 { 7.0 } else { 0.5 },
                relative_speed: 2.8,
                strain: phase * 0.18,
                deformation: if index < 12 { 0.32 } else { 0.02 },
                slosh: if index >= 12 { 0.40 } else { 0.02 },
                component_count: 1,
                ..GestureSample::default()
            });
        }
        let flick_result = flick.classify(true, GestureBoundaryEvent::None);

        assert_eq!(slow_result.kind, EmbodiedGestureKind::SlowStretch);
        assert_eq!(flick_result.kind, EmbodiedGestureKind::SharpFlick);
        assert_ne!(slow_result.causes, flick_result.causes);
        assert!(flick_result.intensity > slow_result.intensity);
    }

    #[test]
    fn rhythm_requires_regular_physical_contact_impulses() {
        let mut classifier = EmbodiedGestureClassifier::default();
        classifier.begin_episode();
        for onset in [0.0, 0.4, 0.8, 1.2] {
            classifier.push_onset(onset);
        }
        for index in 0..=144 {
            classifier.push_sample(GestureSample {
                timestamp: index as f64 / 120.0,
                active: (index / 24) % 2 == 0,
                point_local: Vec2::new(0.04, 0.01),
                pressure: 0.22,
                area: 0.10,
                speed: 0.02,
                component_count: 1,
                ..GestureSample::default()
            });
        }
        let regular = classifier.classify(true, GestureBoundaryEvent::None);
        classifier.contact_onsets[..4].copy_from_slice(&[0.0, 0.10, 0.85, 1.20]);
        let irregular = classifier.classify(true, GestureBoundaryEvent::None);

        assert_eq!(regular.kind, EmbodiedGestureKind::RhythmicTouch);
        assert_ne!(irregular.kind, EmbodiedGestureKind::RhythmicTouch);
    }

    #[test]
    fn circle_requires_turn_and_closure_not_only_cursor_motion() {
        fn classification(closed: bool) -> GestureClassification {
            let mut classifier = EmbodiedGestureClassifier::default();
            classifier.begin_episode();
            for index in 0..=72 {
                let phase = index as f32 / 72.0;
                let point = if closed {
                    Vec2::from_angle(phase * TAU) * 0.18
                } else {
                    Vec2::new(-0.18 + phase * 0.36, 0.06)
                };
                classifier.push_sample(GestureSample {
                    timestamp: index as f64 / 60.0,
                    active: index < 72,
                    point_local: point,
                    pressure: 0.32,
                    area: 0.12,
                    speed: 0.65,
                    relative_speed: 0.4,
                    component_count: 1,
                    ..GestureSample::default()
                });
            }
            classifier.classify(true, GestureBoundaryEvent::None)
        }

        assert_eq!(
            classification(true).kind,
            EmbodiedGestureKind::CircularTwist
        );
        assert_ne!(
            classification(false).kind,
            EmbodiedGestureKind::CircularTwist
        );
    }

    #[test]
    fn fragment_help_requires_component_motion_toward_main() {
        fn classify(toward_main: f32) -> GestureClassification {
            let mut classifier = EmbodiedGestureClassifier::default();
            classifier.begin_episode();
            for index in 0..48 {
                let phase = index as f32 / 47.0;
                classifier.push_sample(GestureSample {
                    timestamp: index as f64 / 60.0,
                    active: index < 47,
                    point_local: Vec2::new(0.60 - phase * 0.36, 0.0),
                    pressure: 0.20,
                    area: 0.10,
                    speed: 0.20,
                    component_count: 2,
                    detached_mass: 0.10,
                    fragment_toward_main: toward_main,
                    ..GestureSample::default()
                });
            }
            classifier.classify(true, GestureBoundaryEvent::None)
        }

        assert_eq!(classify(0.45).kind, EmbodiedGestureKind::FragmentHelp);
        assert_ne!(classify(0.0).kind, EmbodiedGestureKind::FragmentHelp);
    }

    #[test]
    fn ambiguous_gesture_exposes_second_best_and_does_not_double_commit() {
        let mut classifier = EmbodiedGestureClassifier::default();
        classifier.set_tuning(EmbodiedGestureClassifierTuning {
            commit_confidence: 0.85,
            ambiguity_margin: 0.35,
            ..EmbodiedGestureClassifierTuning::default()
        });
        let _ = classifier.ingest(frame(0.0, true, Vec2::new(0.05, 0.0), 0.30));
        let event = classifier
            .ingest(frame(0.12, false, Vec2::new(0.07, 0.0), 0.0))
            .expect("release emits the ambiguous observation");
        assert!(!event.classification.committed);
        assert!(event.classification.second_best_confidence > 0.0);
        assert!(
            classifier
                .ingest(frame(0.13, false, Vec2::new(0.07, 0.0), 0.0))
                .is_none()
        );
    }

    #[test]
    fn topology_rejection_is_a_boundary_even_when_semantics_are_ambiguous() {
        let mut classifier = EmbodiedGestureClassifier::default();
        let mut sample = frame(0.0, true, Vec2::new(0.2, 0.0), 0.50);
        sample.material.maximum_strain = 0.75;
        sample.material.neck_tension = 0.95;
        sample.material.neck_thickness = 0.20;
        sample.material.topology_budget_exhausted = true;
        let event = classifier.ingest(sample).expect("physical edge emits");
        assert_eq!(
            event.boundary,
            GestureBoundaryEvent::TopologyBudgetExhausted
        );
        assert!(event.classification.committed);
    }

    #[test]
    fn sustained_topology_rejection_is_one_edge_not_an_episode_storm() {
        let mut classifier = EmbodiedGestureClassifier::default();
        let mut emitted = Vec::new();
        for index in 0..120 {
            let mut sample = frame(index as f64 / 120.0, false, Vec2::new(0.2, 0.0), 0.0);
            sample.material.maximum_strain = 0.75;
            sample.material.neck_tension = 0.95;
            sample.material.neck_thickness = 0.20;
            sample.material.topology_budget_exhausted = true;
            if let Some(event) = classifier.ingest(sample) {
                emitted.push(event);
            }
        }
        assert_eq!(emitted.len(), 1);
        assert_eq!(emitted[0].classification.episode_id, 1);
    }

    #[test]
    fn sample_storage_is_bounded_under_a_long_hold() {
        let mut classifier = EmbodiedGestureClassifier::default();
        for index in 0..2_000 {
            let _ = classifier.ingest(frame(
                index as f64 / 120.0,
                true,
                Vec2::new(0.04, 0.01),
                0.12,
            ));
        }
        assert!(classifier.sample_len <= MAX_SAMPLES);
        assert!(classifier.onset_len <= MAX_CONTACT_ONSETS);
    }
}
