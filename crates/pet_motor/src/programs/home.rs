use glam::Vec2;
use lifecore::BehaviorGoalFrame;

use crate::{
    ActivePerformance, BehaviorContextFrame, BehaviorProgramId as P, MotorPoseIntent,
    MotorReadabilityTuning, MotorWorldEvent, SomaticActuationPacket, SomaticFieldKind,
    VoiceSemanticIntent, body_field, push_field,
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
        P::HomeLowEnergyRecharge | P::HomeDenReturnEscort
    ) {
        return false;
    }
    let axis = target_axis(active, context);
    match active.program {
        P::HomeLowEnergyRecharge => {
            packet.locomotion.pose = MotorPoseIntent::Recover;
            packet.material.motor_gain_multiplier = 0.64;
            packet.material.flight_damping_multiplier = 1.24;
            packet.material.viscosity_multiplier = 1.16;
            packet.internal.flow_speed_multiplier = 0.50;
            packet.locomotion.lift_fraction = 0.56;
            match phase {
                "reduce_lift" => packet.locomotion.speed_multiplier = 0.0,
                "seek_support" => {
                    packet.locomotion.pose = MotorPoseIntent::Travel;
                    packet.locomotion.speed_multiplier = 0.28;
                }
                "compact_or_spread_by_temperature" => {
                    push_field(
                        packet,
                        body_field(
                            SomaticFieldKind::GravityBias,
                            Vec2::ZERO,
                            Vec2::Y,
                            0.80,
                            0.24,
                            progress,
                        ),
                    );
                }
                "slow_breathe" => {
                    packet.locomotion.speed_multiplier = 0.0;
                    packet.internal.breath_amplitude_multiplier = 0.72;
                    packet.internal.breath_speed_multiplier = 0.58;
                }
                "recover" => {
                    packet.material.motor_gain_multiplier = 0.64 + 0.36 * smooth(progress);
                    packet.locomotion.lift_fraction = 0.56 + 0.44 * smooth(progress);
                }
                _ => {}
            }
        }
        P::HomeDenReturnEscort => {
            packet.locomotion.gaze_lead = 1.0;
            match phase {
                "look_den" => {
                    packet.locomotion.pose = MotorPoseIntent::Orient;
                    packet.locomotion.speed_multiplier = 0.0;
                }
                "approach_bouts" => {
                    packet.locomotion.pose = MotorPoseIntent::Travel;
                    packet.locomotion.speed_multiplier = 0.54;
                    packet.locomotion.approach_arc = active.sampled_style.arc_sign * 0.04;
                }
                "escort_or_carry" => {
                    packet.locomotion.pose = MotorPoseIntent::Travel;
                    packet.locomotion.speed_multiplier = 0.38;
                    if let Some(orb) = context.orb_position {
                        packet.expression.gaze_target = Some(orb);
                    }
                    push_field(
                        packet,
                        body_field(
                            SomaticFieldKind::MassShift,
                            axis * 0.18,
                            axis,
                            0.52,
                            0.24 * tuning.local_field_gain,
                            progress,
                        ),
                    );
                }
                "watch_capture" => {
                    packet.locomotion.pose = MotorPoseIntent::Brake;
                    packet.locomotion.speed_multiplier = 0.0;
                    packet.locomotion.braking = 1.0;
                    if let Some(orb) = context.orb_position {
                        packet.expression.gaze_target = Some(orb);
                    }
                    if context.world_event == MotorWorldEvent::OrbCaptureAcceleration {
                        push_field(
                            packet,
                            body_field(
                                SomaticFieldKind::Brace,
                                axis * 0.16,
                                axis,
                                0.44,
                                0.20,
                                progress,
                            ),
                        );
                    }
                }
                "appraise" => {
                    packet.locomotion.speed_multiplier = 0.0;
                    let success =
                        context.orb_stored || context.world_event == MotorWorldEvent::OrbStored;
                    packet.expression.relief = f32::from(success) * 0.82;
                    packet.internal.pulse_amplitude = if success { 0.46 } else { 0.18 };
                    push_field(
                        packet,
                        body_field(
                            SomaticFieldKind::Pulse,
                            Vec2::ZERO,
                            Vec2::Y,
                            0.72,
                            if success { 0.30 } else { 0.16 },
                            progress,
                        ),
                    );
                    if phase_started {
                        packet.voice.semantic = if success {
                            VoiceSemanticIntent::Relief
                        } else {
                            VoiceSemanticIntent::Query
                        };
                        packet.voice.emit_once = true;
                        packet.voice.intensity = if success { 0.32 } else { 0.24 };
                    }
                }
                _ => {}
            }
        }
        _ => unreachable!(),
    }
    true
}
