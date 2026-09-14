//! Attention/rest transitions, not a second blink clock or an inferred sensor.
use glam::Vec2;
use lifecore::{ActionId, BehaviorGoalFrame};
use pet_motor::{BehaviorContextFrame, BehaviorProgramId, RepertoireEvent, SomaticActuationPacket};

pub const PERCEPTION_REPERTOIRE_IDS: &[u16] = &[7, 14, 16, 27, 78, 109, 116];

#[derive(Clone, Copy)]
struct Observation {
    side: i8,
    drowsy: bool,
    yawn: bool,
    salience: f32,
    startle: f32,
    moving: bool,
    touched: bool,
    gap: Option<Vec2>,
}

#[derive(Default)]
pub struct RepertoirePerceptionEvents {
    previous: Option<Observation>,
    #[allow(dead_code)]
    // Awaiting authoritative renderer visibility hook; not counted as connected.
    orb_visibility: Option<(u64, bool, Vec2)>,
}

impl RepertoirePerceptionEvents {
    /// Call only with measured renderer visibility, including draw order.
    /// Body overlap alone is NOT evidence that an orb was hidden. The target
    /// is the last visible edge supplied by that projection, not a fake scan.
    #[allow(dead_code)] // No substitute overlap heuristic is wired as occlusion.
    pub fn observe_orb_visibility(
        &mut self,
        id: u64,
        visible: bool,
        edge: Vec2,
    ) -> Option<RepertoireEvent> {
        if !edge.is_finite() {
            self.orb_visibility = None;
            return None;
        }
        let event = self
            .orb_visibility
            .filter(|(old, seen, _)| *old == id && *seen && !visible)
            .map(|(_, _, last_edge)| RepertoireEvent {
                id: 4,
                confidence: 0.9,
                target: Some(last_edge),
                side: 0.0,
            });
        self.orb_visibility = Some((id, visible, edge));
        event
    }
    /// Observe the selected packet and physical feedback before repertoire
    /// composition. Outputs accompany an existing action, never command it.
    pub fn observe(
        &mut self,
        goal: &BehaviorGoalFrame,
        context: &BehaviorContextFrame,
        packet: &SomaticActuationPacket,
    ) -> Vec<RepertoireEvent> {
        let target = goal.body_intent.gaze_target.filter(|p| p.is_finite());
        let delta = target.map(|p| p - context.body.motion.world_position);
        let previous_side = self.previous.map_or(0, |p| p.side);
        // Hysteresis avoids a new tilt whenever attention crosses a pixel.
        let side = delta.map_or(0, |d| {
            let threshold = if previous_side != 0 { 0.10 } else { 0.16 };
            if d.x.abs() > threshold && d.x.abs() > d.y.abs() * 1.4 {
                if d.x > 0.0 { 1 } else { -1 }
            } else {
                0
            }
        });
        let yawn = packet.program == Some(BehaviorProgramId::RestDrowsyYawn);
        let drowsy = yawn || (context.support_confirmed() && goal.felt.sleep_pressure > 0.65);
        let moving = matches!(
            goal.action,
            ActionId::ApproachCursor
                | ActionId::ExploreScreen
                | ActionId::PlayCursorChase
                | ActionId::BringProceduralOrb
                | ActionId::SelfPlay
        );
        let next = Observation {
            side,
            drowsy,
            yawn,
            salience: context.selected_salience,
            startle: goal.felt.startle,
            moving,
            touched: context.pet_touched,
            gap: nearby_measured_gap(context),
        };
        let mut events = Vec::new();
        if let Some(old) = self.previous {
            if goal.action != ActionId::Sleep
                && goal.felt.startle <= 0.55
                && let Some(gap) = next.gap
                && old.gap.is_none_or(|p| p.distance(gap) > 0.05)
            {
                events.push(RepertoireEvent {
                    id: 7,
                    confidence: 0.85,
                    target: Some(gap),
                    side: (gap.x - context.body.motion.world_position.x).signum(),
                });
            }
            let mut emit = |id| {
                events.push(RepertoireEvent {
                    id,
                    confidence: 0.9,
                    target,
                    side: f32::from(side),
                })
            };
            let asleep = goal.action == ActionId::Sleep;
            let surprise = next.startle > 0.55 && old.startle <= 0.55;
            let attention = next.salience > 0.6 && next.salience > old.salience + 0.2;
            if asleep
                && context.support_confirmed()
                && next.touched
                && !old.touched
                && (attention || context.cursor_velocity.length() > 0.12)
                && !context.pet_dragged
                && goal.felt.startle <= 0.55
                && goal.felt.pain_like <= 0.22
                && goal.drives.safety <= 0.65
            {
                emit(116);
            }
            if !asleep {
                if side != 0 && side != old.side && !surprise {
                    emit(14);
                }
                if old.drowsy && surprise {
                    emit(27);
                }
                if old.drowsy && !next.drowsy && attention && !surprise {
                    emit(16);
                }
                if old.yawn && !next.yawn && (attention || surprise) {
                    emit(78);
                }
                if context.support_confirmed()
                    && moving
                    && !old.moving
                    && !context.pet_dragged
                    && !surprise
                {
                    emit(109);
                }
            }
        }
        self.previous = Some(next);
        events
    }
}

// A geometric observation, not permission to squeeze between windows.
fn nearby_measured_gap(context: &BehaviorContextFrame) -> Option<Vec2> {
    let body = context.body.motion.world_position;
    let diameter = context.body_diameter.x;
    if !body.is_finite() || !diameter.is_finite() || diameter <= 0.0 {
        return None;
    }
    let mut best: Option<Vec2> = None;
    for a in context.surfaces.iter().take(32) {
        for b in context.surfaces.iter().take(32) {
            if a.surface_id == b.surface_id
                || !a.minimum.is_finite()
                || !a.maximum.is_finite()
                || !b.minimum.is_finite()
                || !b.maximum.is_finite()
                || a.minimum.x >= a.maximum.x
                || b.minimum.x >= b.maximum.x
            {
                continue;
            }
            let width = b.minimum.x - a.maximum.x;
            let low = a.minimum.y.max(b.minimum.y);
            let high = a.maximum.y.min(b.maximum.y);
            if width < diameter * 0.7
                || width > diameter * 1.25
                || high - low < context.body_diameter.y
            {
                continue;
            }
            let point = Vec2::new((b.minimum.x + a.maximum.x) * 0.5, body.y.clamp(low, high));
            if point.distance(body) < 0.20
                && best.is_none_or(|p| point.distance_squared(body) < p.distance_squared(body))
            {
                best = Some(point);
            }
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> (
        BehaviorGoalFrame,
        BehaviorContextFrame,
        SomaticActuationPacket,
    ) {
        let mut life = lifecore::LifeCore::new(lifecore::Genome::from_seed(42), 42);
        let body_intent = life
            .tick(&Default::default(), &Default::default(), 0.05)
            .body_intent;
        (
            BehaviorGoalFrame {
                action: ActionId::IdleHover,
                body_intent,
                affect: Default::default(),
                drives: life.state.drives,
                felt: Default::default(),
                derived: Default::default(),
                attachment: 0.4,
                recent_outcome: None,
            },
            BehaviorContextFrame::default(),
            SomaticActuationPacket::default(),
        )
    }

    #[test]
    fn narrow_gap_uses_two_actual_overlapping_surfaces() {
        let (_, mut context, _) = fixture();
        context.body.motion.world_position = Vec2::splat(0.5);
        context.body_diameter = Vec2::splat(0.1);
        let surface = |id: &str, min: Vec2, max: Vec2| pet_motor::SurfaceCandidate {
            surface_id: lifecore::SurfaceId(id.into()),
            minimum: min,
            maximum: max,
            velocity: Vec2::ZERO,
            familiarity: 0.0,
            recent_failed_landings: 0,
        };
        context.surfaces = vec![
            surface("a", Vec2::new(0.1, 0.3), Vec2::new(0.45, 0.8)),
            surface("b", Vec2::new(0.55, 0.3), Vec2::new(0.9, 0.8)),
        ];
        assert_eq!(nearby_measured_gap(&context), Some(Vec2::splat(0.5)));
        context.surfaces[1].minimum.x = 0.7;
        assert!(nearby_measured_gap(&context).is_none());
        context.surfaces.truncate(1);
        assert!(nearby_measured_gap(&context).is_none());
    }

    #[test]
    fn visibility_requires_same_previously_seen_object_and_never_repeats() {
        let mut observer = RepertoirePerceptionEvents::default();
        assert!(
            observer
                .observe_orb_visibility(1, false, Vec2::ONE)
                .is_none()
        );
        assert!(observer.observe_orb_visibility(1, true, Vec2::X).is_none());
        let event = observer
            .observe_orb_visibility(1, false, Vec2::ZERO)
            .unwrap();
        assert_eq!(event.id, 4);
        assert_eq!(event.target, Some(Vec2::X));
        assert!(
            observer
                .observe_orb_visibility(1, false, Vec2::ZERO)
                .is_none()
        );
        assert!(
            observer
                .observe_orb_visibility(2, false, Vec2::ZERO)
                .is_none()
        );
    }

    #[test]
    fn sleep_check_needs_new_meaningful_touch_not_a_timer() {
        let (mut goal, mut context, packet) = fixture();
        let mut observer = RepertoirePerceptionEvents::default();
        goal.action = ActionId::Sleep;
        goal.drives.safety = 0.0;
        context.somatic.supported = true;
        observer.observe(&goal, &context, &packet);
        context.pet_touched = true;
        context.cursor_velocity = Vec2::new(0.15, 0.0);
        assert!(
            observer
                .observe(&goal, &context, &packet)
                .iter()
                .any(|e| e.id == 116)
        );
        for _ in 0..10_000 {
            assert!(observer.observe(&goal, &context, &packet).is_empty());
        }
    }

    #[test]
    fn static_side_attention_never_becomes_periodic_nodding() {
        let (mut goal, context, packet) = fixture();
        let mut observer = RepertoirePerceptionEvents::default();
        goal.body_intent.gaze_target = Some(context.body.motion.world_position);
        assert!(observer.observe(&goal, &context, &packet).is_empty());
        goal.body_intent.gaze_target = Some(context.body.motion.world_position + Vec2::X * 0.3);
        assert_eq!(observer.observe(&goal, &context, &packet)[0].id, 14);
        for _ in 0..24_000 {
            assert!(observer.observe(&goal, &context, &packet).is_empty());
        }
    }

    #[test]
    fn supported_rise_requires_real_support_and_sleep_never_opens_eyes() {
        let (mut goal, mut context, packet) = fixture();
        let mut observer = RepertoirePerceptionEvents::default();
        observer.observe(&goal, &context, &packet);
        goal.action = ActionId::ExploreScreen;
        assert!(observer.observe(&goal, &context, &packet).is_empty());
        goal.action = ActionId::IdleHover;
        context.somatic.supported = true;
        observer.observe(&goal, &context, &packet);
        goal.action = ActionId::ExploreScreen;
        assert!(
            observer
                .observe(&goal, &context, &packet)
                .iter()
                .any(|e| e.id == 109)
        );
        goal.action = ActionId::Sleep;
        goal.felt.startle = 1.0;
        assert!(observer.observe(&goal, &context, &packet).is_empty());
    }

    #[test]
    fn yawn_completion_is_not_interruption_but_a_new_stimulus_is() {
        let (goal, mut context, mut packet) = fixture();
        let mut observer = RepertoirePerceptionEvents::default();
        packet.program = Some(BehaviorProgramId::RestDrowsyYawn);
        observer.observe(&goal, &context, &packet);
        packet.program = None;
        assert!(
            !observer
                .observe(&goal, &context, &packet)
                .iter()
                .any(|e| e.id == 78)
        );
        packet.program = Some(BehaviorProgramId::RestDrowsyYawn);
        observer.observe(&goal, &context, &packet);
        context.selected_salience = 0.9;
        packet.program = None;
        let events = observer.observe(&goal, &context, &packet);
        assert!(events.iter().any(|e| e.id == 78));
        assert!(events.iter().any(|e| e.id == 16));
        assert!(observer.observe(&goal, &context, &packet).is_empty());
    }
}
