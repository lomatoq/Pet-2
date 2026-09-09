//! Shared measured inputs for live and deterministic motor execution.
use glam::Vec2;
use lifecore::{BodyIntent, LifeCore, SensorFrame};
use pet_body::ProceduralBody;
use pet_ecology::{EpisodePhase, ObjectKind, ObjectLifecycle};
use pet_motor::{BehaviorContextFrame, MotorWorldEvent, SurfaceCandidate};

use crate::{EcologyRuntime, NervousSystemRuntime, VitaRuntime, motor_world_goal};

#[allow(clippy::too_many_arguments)]
pub fn from_frames(
    nervous: &NervousSystemRuntime,
    vita: &VitaRuntime,
    body: &ProceduralBody,
    life: &LifeCore,
    sensors: &SensorFrame,
    intent: &BodyIntent,
    ecology: Option<&mut EcologyRuntime>,
) -> BehaviorContextFrame {
    let feedback = nervous.body_feedback();
    let gesture = vita.latest_embodied_gesture();
    let felt = nervous.snapshot().felt;
    let mut context = BehaviorContextFrame {
        frame_id: feedback.frame_id,
        timestamp_seconds: sensors.timestamp,
        body: feedback,
        somatic: body.somatic_feedback(),
        cursor_position: sensors.cursor_position,
        cursor_velocity: sensors.cursor_velocity,
        cursor_acceleration: sensors.cursor_acceleration,
        pointer_down: sensors.pointer_down,
        pointer_pressed: sensors.pointer_pressed,
        pointer_released: sensors.pointer_released,
        pet_touched: sensors.pet_touched,
        pet_dragged: sensors.pet_dragged,
        selected_salience: vita.visual_attention_target().map_or(0.0, |t| t.score),
        gesture: gesture.kind,
        gesture_confidence: gesture.confidence,
        gesture_ended: gesture.ended,
        boundary_violation: felt
            .pain_like
            .max(felt.restraint)
            .max((feedback.contact.pressure - 0.55).max(0.0) / 0.45)
            .clamp(0.0, 1.0),
        locomotion_completed: body.simulation.feedback.locomotion_completed,
        focus_mode: life.state.focus_mode || vita.state().desktop_rhythm.protects_focused_work(),
        surfaces: sensors
            .visible_surfaces
            .iter()
            .map(|s| SurfaceCandidate {
                surface_id: s.id.clone(),
                minimum: s.rect.minimum,
                maximum: s.rect.maximum,
                velocity: Vec2::ZERO,
                familiarity: if intent.target_surface.as_ref() == Some(&s.id) {
                    0.55
                } else {
                    0.20
                },
                recent_failed_landings: 0,
            })
            .collect(),
        ..BehaviorContextFrame::default()
    };
    if let Some(ecology) = ecology {
        context.world_event = ecology.take_den_event().unwrap_or(MotorWorldEvent::None);
        let episode = ecology.active_episode();
        context.world_goal = if episode
            .is_some_and(|e| e.reason_code == pet_ecology::EpisodeReason::PettingContinuation)
        {
            pet_motor::MotorWorldGoal::PetMore
        } else {
            motor_world_goal(episode.map(|e| e.goal))
        };
        context.world_social_hold = episode.is_some_and(|e| {
            matches!(
                e.phase,
                EpisodePhase::WaitForUser | EpisodePhase::AskForHelp
            )
        });
        context.world_help_wait = episode.is_some_and(|e| e.phase == EpisodePhase::AskForHelp);
        let state = ecology.state();
        let orb = state.objects.iter().find(|o| o.kind == ObjectKind::Orb);
        context.den_anchor = Some(state.den.anchor);
        context.den_familiarity = state.den.familiarity;
        context.edible_position = state
            .objects
            .iter()
            .find(|o| {
                o.kind == ObjectKind::Morsel
                    && !matches!(
                        o.lifecycle,
                        ObjectLifecycle::Consumed | ObjectLifecycle::StoredInDen
                    )
            })
            .map(|o| o.position);
        context.object_affordance = if orb.is_some() {
            pet_motor::ObjectAffordance::Toy
        } else {
            pet_motor::ObjectAffordance::Unknown
        };
        context.orb_user_held = orb.is_some_and(|o| o.lifecycle == ObjectLifecycle::GrabbedByUser);
        context.orb_position = orb.map(|o| o.position);
        context.orb_id = orb.map(|o| o.id);
        context.orb_stored = orb.is_some_and(|o| o.lifecycle == ObjectLifecycle::StoredInDen);
        context.preferred_touch_side =
            match state.successful_touch_sides[1].cmp(&state.successful_touch_sides[0]) {
                std::cmp::Ordering::Greater => 1.0,
                std::cmp::Ordering::Less => -1.0,
                std::cmp::Ordering::Equal => 0.0,
            };
    }
    context
}
