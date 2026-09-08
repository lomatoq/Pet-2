use glam::Vec2;
use lifecore::BehaviorGoalFrame;

use crate::{
    ActivePerformance, BehaviorContextFrame, BehaviorProgramId as P, MotorPoseIntent,
    MotorReadabilityTuning, SomaticActuationPacket, SomaticFieldKind, VoiceSemanticIntent,
    body_field, push_field,
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
        P::MoveOrientReflex | P::MovePunctuatedTravel | P::MoveBrakeSquashRecover
    ) {
        return false;
    }
    let axis = target_axis(active, context);
    let forward = if axis.length_squared() > 0.0 {
        axis
    } else {
        Vec2::X
    };
    let contrast = tuning.phase_contrast * active.sampled_style.amplitude.max(0.65);
    match active.program {
        P::MoveOrientReflex => {
            packet.locomotion.pose = MotorPoseIntent::Orient;
            packet.locomotion.speed_multiplier = 0.0;
            packet.locomotion.gaze_lead = 1.0;
            if phase == "front_axis_turn" {
                push_field(
                    packet,
                    body_field(
                        SomaticFieldKind::MassShift,
                        forward * 0.18,
                        forward,
                        0.48,
                        0.22 * contrast,
                        progress,
                    ),
                );
            } else if phase == "freeze_30_120ms" {
                push_field(
                    packet,
                    body_field(
                        SomaticFieldKind::Brace,
                        Vec2::ZERO,
                        forward,
                        0.60,
                        0.16 * contrast,
                        progress,
                    ),
                );
            }
            if phase_started && phase == "eyes_first" {
                packet.voice.semantic = VoiceSemanticIntent::Notice;
                packet.voice.emit_once = context.selected_salience > 0.72;
                packet.voice.intensity = context.selected_salience * 0.45;
            }
        }
        P::MovePunctuatedTravel => {
            packet.locomotion.pose = MotorPoseIntent::Travel;
            packet.locomotion.approach_arc =
                active.sampled_style.arc_sign * 0.08 * active.sampled_style.asymmetry;
            packet.locomotion.gaze_lead = 0.90;
            match phase {
                "orient" => packet.locomotion.speed_multiplier = 0.0,
                "prepare" => {
                    packet.locomotion.speed_multiplier = 0.05;
                    packet.locomotion.acceleration_limit = 0.25;
                    push_field(
                        packet,
                        body_field(
                            SomaticFieldKind::MassShift,
                            -forward * 0.16,
                            -forward,
                            0.55,
                            0.30 * tuning.preparation_gain,
                            progress,
                        ),
                    );
                }
                "accelerate" => {
                    packet.locomotion.speed_multiplier = 0.25 + smooth(progress) * 0.85;
                    packet.locomotion.acceleration_limit = 0.45 + smooth(progress) * 0.55;
                    packet.material.flight_stretch_multiplier = 1.0 + 0.18 * contrast;
                    push_field(
                        packet,
                        body_field(
                            SomaticFieldKind::MassShift,
                            forward * 0.22,
                            forward,
                            0.50,
                            0.34 * contrast,
                            progress,
                        ),
                    );
                }
                "coast" => {
                    packet.locomotion.speed_multiplier = 1.0;
                    packet.material.flight_stretch_multiplier = 1.10;
                    packet.material.flight_damping_multiplier = 0.92;
                }
                "brake" => {
                    packet.locomotion.pose = MotorPoseIntent::Brake;
                    packet.locomotion.speed_multiplier = 1.0 - smooth(progress);
                    packet.locomotion.braking = smooth(progress);
                    packet.material.flight_damping_multiplier = 1.0 + 0.28 * contrast;
                    push_field(
                        packet,
                        body_field(
                            SomaticFieldKind::Flatten,
                            forward * 0.27,
                            forward,
                            0.36,
                            0.42 * contrast,
                            progress,
                        ),
                    );
                }
                "arrival_pause" => {
                    packet.locomotion.speed_multiplier = 0.0;
                    packet.locomotion.arrival_pause = 1.0;
                    packet.material.flight_damping_multiplier = 1.18;
                    packet.internal.flow_damping = 0.25;
                }
                "appraise" => {
                    packet.locomotion.speed_multiplier = 0.0;
                    packet.locomotion.arrival_pause = 0.8;
                    packet.expression.blink = if progress > 0.55 { 0.30 } else { 0.0 };
                }
                _ => {}
            }
        }
        P::MoveBrakeSquashRecover => {
            packet.locomotion.pose = MotorPoseIntent::Brake;
            packet.locomotion.speed_multiplier = 0.0;
            packet.locomotion.braking = 1.0;
            match phase {
                "anticipate_brake" => packet.material.flight_damping_multiplier = 1.18,
                "leading_squash" => {
                    push_field(
                        packet,
                        body_field(
                            SomaticFieldKind::Flatten,
                            forward * 0.26,
                            forward,
                            0.38,
                            0.48 * contrast,
                            progress,
                        ),
                    );
                    packet.expression.effort = 0.38;
                }
                "core_catch_up" => {
                    push_field(
                        packet,
                        body_field(
                            SomaticFieldKind::Wave,
                            -forward * 0.24,
                            forward,
                            0.72,
                            0.34 * contrast,
                            progress,
                        ),
                    );
                }
                "rebound" => {
                    push_field(
                        packet,
                        body_field(
                            SomaticFieldKind::Pulse,
                            Vec2::ZERO,
                            forward,
                            0.64,
                            0.22 * tuning.follow_through_gain,
                            progress,
                        ),
                    );
                }
                "stillness" => {
                    packet.material.flight_damping_multiplier = 1.30;
                    packet.internal.flow_damping = 0.38;
                }
                _ => {}
            }
        }
        _ => unreachable!(),
    }
    true
}
