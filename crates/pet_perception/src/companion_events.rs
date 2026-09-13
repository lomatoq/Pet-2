use glam::Vec2;
use lifecore::{
    AppraisedEvent, CompanionEventKind, CompanionEventSource, EmbodiedGestureKind, IntentTarget,
    IntentTargetKind,
};

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct CompanionEventInput {
    pub timestamp: f64,
    pub pet_position: Vec2,
    pub cursor_position: Vec2,
    pub cursor_velocity: Vec2,
    pub cursor_acceleration: Vec2,
    pub cursor_distance: f32,
    pub pointer_down: bool,
    pub pointer_pressed: bool,
    pub pointer_released: bool,
    pub direct_contact: bool,
    pub contact_pressure: f32,
    pub contact_strain: f32,
    pub contact_seconds: f32,
    pub gesture_confidence: f32,
    pub gesture: Option<EmbodiedGestureKind>,
    pub user_idle_seconds: f32,
    pub user_activity_rate: f32,
    pub previous_user_idle_seconds: f32,
    pub visual_novelty: f32,
    pub window_motion: f32,
    pub window_pressure: f32,
    pub window_target: Option<Vec2>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct SemanticEventBatch {
    pub events: [Option<AppraisedEvent>; 8],
    pub count: u8,
}

impl SemanticEventBatch {
    fn push(&mut self, event: AppraisedEvent) {
        let index = usize::from(self.count);
        if index < self.events.len() {
            self.events[index] = Some(event.sanitized());
            self.count += 1;
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = AppraisedEvent> + '_ {
        self.events.iter().flatten().copied()
    }
}

pub struct CompanionEventBuilder;

impl CompanionEventBuilder {
    #[must_use]
    pub fn build(input: CompanionEventInput) -> SemanticEventBatch {
        let mut out = SemanticEventBatch::default();
        if !input.pet_position.is_finite() || !input.cursor_position.is_finite() {
            return out;
        }
        let to_cursor = input.cursor_position - input.pet_position;
        let radial = to_cursor.normalize_or_zero();
        let radial_speed = input.cursor_velocity.dot(radial);
        let cursor_target = IntentTarget {
            kind: IntentTargetKind::Cursor,
            position: Some(input.cursor_position),
            confidence: 1.0,
            component_id: None,
        };

        // Proximity is attention evidence only. It is never upgraded to touch.
        if !input.direct_contact && input.cursor_distance < 0.20 {
            let kind = if input.cursor_velocity.length() < 0.015 {
                CompanionEventKind::PointerHover
            } else if radial_speed < -0.025 {
                CompanionEventKind::PointerApproach
            } else if radial_speed > 0.025 {
                CompanionEventKind::PointerWithdraw
            } else {
                CompanionEventKind::None
            };
            if kind != CompanionEventKind::None {
                out.push(AppraisedEvent {
                    source: CompanionEventSource::UserPointer,
                    kind,
                    target: cursor_target,
                    confidence: proximity_confidence(input.cursor_distance),
                    novelty: 0.12,
                    controllability: 0.90,
                    directness: 0.35,
                    social_likelihood: if kind == CompanionEventKind::PointerHover {
                        0.48
                    } else {
                        0.28
                    },
                    threat_likelihood: if input.cursor_velocity.length() > 0.9 {
                        0.18
                    } else {
                        0.03
                    },
                    play_likelihood: if input.cursor_velocity.length() > 0.12 {
                        0.22
                    } else {
                        0.08
                    },
                    contact_quality: 0.0,
                    prediction_error: 0.10,
                    expectedness: 0.60,
                    timestamp: input.timestamp,
                });
            }
        }

        if input.direct_contact {
            let speed = input.cursor_velocity.length();
            let quality = (1.0
                - input.contact_pressure.clamp(0.0, 1.0) * 0.35
                - input.contact_strain.clamp(0.0, 1.0) * 0.55
                - (speed - 0.10).max(0.0) * 0.25)
                .clamp(-1.0, 1.0);
            let kind = if input.pointer_released {
                CompanionEventKind::Release
            } else {
                match input.gesture {
                    Some(EmbodiedGestureKind::Tickle) => CompanionEventKind::Tickle,
                    Some(EmbodiedGestureKind::RhythmicTouch) => CompanionEventKind::RhythmicTap,
                    Some(EmbodiedGestureKind::SharedPlayInvitation) => {
                        CompanionEventKind::PlayInvitation
                    }
                    Some(EmbodiedGestureKind::PullAndRelease) => CompanionEventKind::Pull,
                    Some(EmbodiedGestureKind::SharpFlick) => CompanionEventKind::Flick,
                    _ if input.contact_strain > 0.72 || speed > 1.0 => CompanionEventKind::Flick,
                    _ if input.contact_seconds > 0.45 && speed < 0.025 => CompanionEventKind::Hold,
                    _ if input.contact_seconds > 0.15 && speed < 0.35 => CompanionEventKind::Stroke,
                    _ => CompanionEventKind::SoftTouch,
                }
            };
            out.push(AppraisedEvent {
                source: CompanionEventSource::DirectContact,
                kind,
                target: IntentTarget {
                    kind: IntentTargetKind::ContactPoint,
                    position: Some(input.cursor_position),
                    confidence: input.gesture_confidence.max(0.55),
                    component_id: None,
                },
                confidence: input.gesture_confidence.max(0.55),
                novelty: 0.18,
                controllability: 0.72,
                directness: 1.0,
                social_likelihood: if quality > 0.3 { 0.82 } else { 0.20 },
                threat_likelihood: if quality < 0.0 {
                    (-quality).clamp(0.0, 1.0)
                } else {
                    0.02
                },
                play_likelihood: if kind == CompanionEventKind::Flick {
                    0.18
                } else {
                    0.12
                },
                contact_quality: quality,
                prediction_error: (input.cursor_acceleration.length() * 0.12).clamp(0.0, 1.0),
                expectedness: 0.55,
                timestamp: input.timestamp,
            });
        }

        if input.pointer_pressed && !input.direct_contact {
            // A click elsewhere is context, never pet contact.
            out.push(AppraisedEvent {
                source: CompanionEventSource::DesktopActivity,
                kind: CompanionEventKind::UserActive,
                target: IntentTarget::default(),
                confidence: 0.65,
                novelty: 0.02,
                controllability: 0.0,
                directness: 0.0,
                social_likelihood: 0.0,
                threat_likelihood: 0.0,
                play_likelihood: 0.0,
                contact_quality: 0.0,
                prediction_error: 0.0,
                expectedness: 0.95,
                timestamp: input.timestamp,
            });
        }

        if input.previous_user_idle_seconds >= 45.0 && input.user_idle_seconds < 3.0 {
            out.push(AppraisedEvent {
                source: CompanionEventSource::DesktopActivity,
                kind: CompanionEventKind::UserReturned,
                target: IntentTarget {
                    kind: IntentTargetKind::UserProxy,
                    confidence: 0.8,
                    ..IntentTarget::default()
                },
                confidence: 0.85,
                novelty: (input.previous_user_idle_seconds / 900.0).clamp(0.1, 0.8),
                controllability: 0.0,
                directness: 0.15,
                social_likelihood: 0.72,
                threat_likelihood: 0.0,
                play_likelihood: 0.10,
                contact_quality: 0.0,
                prediction_error: 0.15,
                expectedness: 0.50,
                timestamp: input.timestamp,
            });
        }

        if input.visual_novelty > 0.45 {
            out.push(AppraisedEvent {
                source: CompanionEventSource::VisualField,
                kind: CompanionEventKind::VisualNovelty,
                target: IntentTarget {
                    kind: IntentTargetKind::ScreenRegion,
                    confidence: input.visual_novelty,
                    ..IntentTarget::default()
                },
                confidence: input.visual_novelty,
                novelty: input.visual_novelty,
                controllability: 0.2,
                directness: 0.0,
                social_likelihood: 0.0,
                threat_likelihood: 0.05,
                play_likelihood: 0.02,
                contact_quality: 0.0,
                prediction_error: input.visual_novelty,
                expectedness: 1.0 - input.visual_novelty,
                timestamp: input.timestamp,
            });
        }

        if input.window_motion > 0.08 || input.window_pressure > 0.08 {
            let pressure = input.window_pressure.clamp(0.0, 1.0);
            out.push(AppraisedEvent {
                source: CompanionEventSource::WindowEnvironment,
                kind: if pressure > 0.55 {
                    CompanionEventKind::SurfaceCollision
                } else {
                    CompanionEventKind::WindowMoved
                },
                target: IntentTarget {
                    kind: IntentTargetKind::Surface,
                    position: input.window_target.filter(|point| point.is_finite()),
                    confidence: input.window_motion.max(pressure).clamp(0.0, 1.0),
                    component_id: None,
                },
                confidence: input.window_motion.max(pressure).clamp(0.0, 1.0),
                novelty: (input.window_motion * 0.35 + pressure * 0.65).clamp(0.0, 1.0),
                controllability: 0.20,
                directness: 0.0,
                social_likelihood: 0.0,
                threat_likelihood: pressure,
                play_likelihood: 0.0,
                contact_quality: 0.0,
                prediction_error: pressure,
                expectedness: (1.0 - input.window_motion).clamp(0.0, 1.0),
                timestamp: input.timestamp,
            });
        }

        out
    }
}

fn proximity_confidence(distance: f32) -> f32 {
    (1.0 - distance.clamp(0.0, 0.20) / 0.20).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_near_pet_is_not_contact() {
        let batch = CompanionEventBuilder::build(CompanionEventInput {
            pet_position: Vec2::splat(0.5),
            cursor_position: Vec2::new(0.51, 0.5),
            cursor_distance: 0.01,
            ..CompanionEventInput::default()
        });
        assert!(
            batch
                .iter()
                .all(|event| event.source != CompanionEventSource::DirectContact)
        );
    }

    #[test]
    fn returning_user_is_social_context_not_forced_affection() {
        let batch = CompanionEventBuilder::build(CompanionEventInput {
            pet_position: Vec2::splat(0.5),
            cursor_position: Vec2::splat(0.8),
            cursor_distance: 0.4,
            previous_user_idle_seconds: 300.0,
            user_idle_seconds: 0.0,
            ..CompanionEventInput::default()
        });
        let returned = batch
            .iter()
            .find(|event| event.kind == CompanionEventKind::UserReturned)
            .unwrap();
        assert!(returned.social_likelihood > 0.5);
        assert!(returned.directness < 0.5);
    }
}
