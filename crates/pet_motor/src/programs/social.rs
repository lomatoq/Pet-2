use glam::Vec2;
use lifecore::BehaviorGoalFrame;

use crate::{
    ActivePerformance, BehaviorContextFrame, BehaviorProgramId as P, MotorPoseIntent,
    MotorReadabilityTuning, SomaticActuationPacket, SomaticFieldKind, VoiceSemanticIntent,
    body_field, contact_field, push_field,
};

use super::{smooth, target_axis};

#[allow(clippy::too_many_arguments)]
pub(crate) fn apply(
    packet: &mut SomaticActuationPacket,
    active: &ActivePerformance,
    phase: &str,
    progress: f32,
    phase_started: bool,
    _goal: &BehaviorGoalFrame,
    context: &BehaviorContextFrame,
    tuning: MotorReadabilityTuning,
) -> bool {
    if !matches!(
        active.program,
        P::SocialPettingSolicitation | P::SocialRubNuzzleCursor
    ) {
        return false;
    }
    let axis = target_axis(active, context);
    match active.program {
        P::SocialPettingSolicitation => {
            packet.locomotion.pose = MotorPoseIntent::OfferContact;
            match phase {
                "orient" => {
                    packet.locomotion.speed_multiplier = 0.0;
                    packet.locomotion.gaze_lead = 1.0;
                }
                "approach" => {
                    packet.locomotion.pose = MotorPoseIntent::Travel;
                    packet.locomotion.speed_multiplier = 0.42 + 0.20 * smooth(progress);
                    packet.locomotion.approach_arc = active.sampled_style.arc_sign * 0.05;
                }
                "stop_short" => {
                    packet.locomotion.pose = MotorPoseIntent::Brake;
                    packet.locomotion.speed_multiplier = 0.0;
                    packet.locomotion.braking = 1.0;
                    push_field(
                        packet,
                        body_field(
                            SomaticFieldKind::Flatten,
                            axis * 0.22,
                            axis,
                            0.38,
                            0.20,
                            progress,
                        ),
                    );
                }
                "present_side" => {
                    packet.locomotion.speed_multiplier = 0.0;
                    let side = Vec2::new(-axis.y, axis.x) * active.sampled_style.arc_sign;
                    push_field(
                        packet,
                        body_field(
                            SomaticFieldKind::MassShift,
                            side * 0.22,
                            side,
                            0.45,
                            0.28 * tuning.preparation_gain,
                            progress,
                        ),
                    );
                    packet.material.density_compliance_multiplier = 1.10;
                }
                "look_wait" => {
                    packet.locomotion.speed_multiplier = 0.0;
                    packet.locomotion.arrival_pause = 1.0;
                    packet.expression.gaze_target = Some(context.cursor_position);
                    if phase_started {
                        packet.voice.semantic = VoiceSemanticIntent::Invite;
                        packet.voice.emit_once = true;
                        packet.voice.intensity = 0.32;
                    }
                }
                "accept_or_withdraw" => {
                    if !active
                        .social_bid
                        .as_ref()
                        .is_some_and(|bid| bid.response_received)
                    {
                        let away = (context.body.motion.world_position - context.cursor_position)
                            .normalize_or_zero();
                        packet.locomotion.target_position = Some(
                            (context.body.motion.world_position + away * 0.04)
                                .clamp(Vec2::ZERO, Vec2::ONE),
                        );
                        packet.expression.gaze_target = packet.locomotion.target_position;
                    }
                    packet.locomotion.speed_multiplier = if active
                        .social_bid
                        .as_ref()
                        .is_some_and(|bid| bid.response_received)
                    {
                        0.0
                    } else {
                        0.24
                    };
                    packet.expression.relief = f32::from(
                        active
                            .social_bid
                            .as_ref()
                            .is_some_and(|bid| bid.response_received),
                    ) * 0.45;
                }
                _ => {}
            }
        }
        P::SocialRubNuzzleCursor => {
            packet.locomotion.pose = MotorPoseIntent::Touch;
            packet.locomotion.speed_multiplier = if phase == "approach" { 0.28 } else { 0.0 };
            packet.material.density_compliance_multiplier = 1.16;
            packet.material.viscosity_multiplier = 0.92;
            if phase == "first_nudge" {
                push_field(
                    packet,
                    contact_field(SomaticFieldKind::Attract, axis, 0.30, 0.22, progress),
                );
            }
            if matches!(phase, "tangential_rub" | "second_rub_or_release") {
                let tangent = Vec2::new(-axis.y, axis.x)
                    * active.sampled_style.arc_sign
                    * if phase == "second_rub_or_release" {
                        -1.0
                    } else {
                        1.0
                    };
                push_field(
                    packet,
                    contact_field(
                        SomaticFieldKind::Shear,
                        tangent,
                        0.34,
                        0.32 * tuning.local_field_gain,
                        progress,
                    ),
                );
                push_field(
                    packet,
                    contact_field(SomaticFieldKind::Anchor, axis, 0.28, 0.14, progress),
                );
            }
            if phase == "pause" {
                packet.internal.pulse_amplitude = 0.18;
            }
            if phase_started && phase == "first_nudge" {
                packet.voice.semantic = VoiceSemanticIntent::Purr;
                packet.voice.emit_once = true;
                packet.voice.intensity = 0.24;
            }
        }
        _ => unreachable!(),
    }
    true
}
