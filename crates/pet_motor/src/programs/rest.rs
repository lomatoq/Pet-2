use glam::Vec2;
use lifecore::BehaviorGoalFrame;

use crate::{
    ActivePerformance, BehaviorContextFrame, BehaviorProgramId as P, FieldSpace, MotorPoseIntent,
    MotorReadabilityTuning, SomaticActuationPacket, SomaticFieldKind, VoiceSemanticIntent,
    body_field, push_field, wave_field,
};

use super::{smooth, surface_command, target_axis};

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
        P::RestSurfaceRoostSearch
            | P::RestLandingSoftTouchdown
            | P::RestSitSettle
            | P::RestDrowsyYawn
            | P::RestNremSleep
            | P::RestRemDreamWake
    ) {
        return false;
    }
    let axis = target_axis(active, context);
    let contact_axis = active
        .locked_target
        .as_ref()
        .and_then(|target| match target {
            crate::BehaviorTarget::Surface(surface) => Some(-surface.normal),
            _ => None,
        })
        .unwrap_or(axis)
        .normalize_or_zero();
    let contact_axis = if contact_axis.length_squared() > 1.0e-6 {
        contact_axis
    } else {
        Vec2::Y
    };
    let support_confirmed = context.support_confirmed();
    match active.program {
        P::RestSurfaceRoostSearch => {
            packet.locomotion.pose = MotorPoseIntent::Orient;
            packet.locomotion.gaze_lead = 1.0;
            if phase == "approach_commit" {
                // Screen-edge sleep owns an exact silhouette contact target;
                // Landing bypasses the generic 176 px free-flight edge inset.
                packet.locomotion.pose = MotorPoseIntent::Landing;
                packet.locomotion.speed_multiplier = 0.42 + smooth(progress) * 0.28;
                packet.locomotion.acceleration_limit = 0.55;
                push_field(
                    packet,
                    body_field(
                        SomaticFieldKind::MassShift,
                        axis * 0.18,
                        axis,
                        0.52,
                        0.24 * tuning.preparation_gain,
                        progress,
                    ),
                );
            } else {
                packet.locomotion.speed_multiplier = 0.0;
            }
        }
        P::RestLandingSoftTouchdown => {
            packet.locomotion.pose = MotorPoseIntent::Landing;
            packet.locomotion.gaze_lead = 1.0;
            match phase {
                "orient" => packet.locomotion.speed_multiplier = 0.0,
                "preload" => {
                    packet.locomotion.speed_multiplier = 0.08;
                    push_field(
                        packet,
                        body_field(
                            SomaticFieldKind::MassShift,
                            -axis * 0.16,
                            -axis,
                            0.55,
                            0.30 * tuning.preparation_gain,
                            progress,
                        ),
                    );
                }
                "decelerate" => {
                    packet.locomotion.speed_multiplier = 0.52 * (1.0 - smooth(progress)) + 0.10;
                    packet.locomotion.braking = smooth(progress);
                    packet.material.flight_damping_multiplier = 1.22;
                }
                "first_contact" => {
                    packet.locomotion.speed_multiplier = 0.0;
                    packet.support = surface_command(active, 0.14, 0.25, 0.12);
                    push_field(
                        packet,
                        body_field(
                            SomaticFieldKind::Flatten,
                            contact_axis * 0.27,
                            contact_axis,
                            0.46,
                            0.28 * tuning.support_gain,
                            progress,
                        ),
                    );
                    packet.expression.blink = 0.32;
                }
                "load_transfer" => {
                    packet.locomotion.speed_multiplier = 0.0;
                    packet.locomotion.lift_fraction = 1.0 - 0.55 * smooth(progress);
                    packet.support = surface_command(active, 0.34, 0.34, 0.20);
                    push_field(
                        packet,
                        wave_field(
                            FieldSpace::SurfaceTangentNormal,
                            Vec2::ZERO,
                            contact_axis,
                            0.75,
                            0.28 * tuning.follow_through_gain,
                            1.2,
                            progress,
                        ),
                    );
                }
                "appraise" => {
                    packet.locomotion.speed_multiplier = 0.0;
                    packet.locomotion.lift_fraction = 0.45;
                    packet.support = surface_command(active, 0.36, 0.34, 0.22);
                    packet.expression.relief = if context.has_bottom_screen_edge() {
                        (context.screen_edge_support_stable_seconds / 0.30).clamp(0.0, 1.0)
                    } else {
                        context.somatic.support_stability
                    };
                    if phase_started && support_confirmed {
                        packet.voice.semantic = VoiceSemanticIntent::Relief;
                        packet.voice.emit_once = true;
                        packet.voice.intensity = 0.24;
                    }
                }
                _ => {}
            }
        }
        P::RestSitSettle => {
            packet.locomotion.pose = MotorPoseIntent::SupportedRest;
            packet.locomotion.speed_multiplier = 0.0;
            packet.locomotion.lift_fraction = 0.36;
            let load = match phase {
                "lower_lift" => 0.18 + 0.14 * smooth(progress),
                "spread_contact_patch" => 0.32 + 0.08 * smooth(progress),
                _ => 0.40,
            };
            packet.support = surface_command(active, load, 0.36, 0.25);
            if phase == "spread_contact_patch" {
                push_field(
                    packet,
                    body_field(
                        SomaticFieldKind::Flatten,
                        Vec2::new(0.0, 0.24),
                        Vec2::Y,
                        0.58,
                        0.30 * tuning.support_gain,
                        progress,
                    ),
                );
            }
            if phase == "micro_adjust" {
                push_field(
                    packet,
                    wave_field(
                        FieldSpace::SurfaceTangentNormal,
                        Vec2::ZERO,
                        Vec2::X,
                        0.54,
                        0.10,
                        0.7,
                        progress,
                    ),
                );
            }
            if phase == "rest_hold" {
                packet.internal.breath_amplitude_multiplier = 0.76;
                packet.internal.breath_speed_multiplier = 0.72;
                packet.material.flight_damping_multiplier = 1.26;
            }
        }
        P::RestDrowsyYawn => {
            packet.locomotion.pose = if support_confirmed {
                MotorPoseIntent::SupportedRest
            } else {
                MotorPoseIntent::Recover
            };
            packet.locomotion.speed_multiplier = 0.0;
            if support_confirmed {
                packet.support = surface_command(active, 0.34, 0.34, 0.20);
            }
            match phase {
                "anticipatory_inhale" => {
                    packet.internal.breath_amplitude_multiplier = 1.45;
                    packet.internal.breath_speed_multiplier = 0.72;
                    push_field(
                        packet,
                        wave_field(
                            FieldSpace::BodyLocal,
                            Vec2::ZERO,
                            Vec2::Y,
                            0.82,
                            0.30,
                            0.45,
                            progress,
                        ),
                    );
                }
                "whole_body_stretch" => {
                    packet.material.flight_stretch_multiplier = 1.24;
                    push_field(
                        packet,
                        body_field(
                            SomaticFieldKind::MassShift,
                            Vec2::ZERO,
                            Vec2::Y,
                            0.82,
                            0.36,
                            progress,
                        ),
                    );
                }
                "audible_yawn_peak" => {
                    packet.expression.mouth_open = 0.90;
                    packet.expression.eye_aperture_delta = -0.35;
                    if phase_started {
                        packet.voice.semantic = VoiceSemanticIntent::Yawn;
                        packet.voice.emit_once = true;
                        packet.voice.intensity = 0.52;
                    }
                }
                "slow_exhale" => {
                    packet.internal.breath_amplitude_multiplier = 1.22;
                    packet.internal.breath_speed_multiplier = 0.48;
                    packet.internal.flow_damping = 0.34;
                }
                "blink_settle" => {
                    packet.expression.blink = 0.90;
                    packet.expression.eye_aperture_delta = -0.28;
                }
                _ => {}
            }
        }
        P::RestNremSleep => {
            packet.locomotion.pose = MotorPoseIntent::SupportedSleep;
            packet.locomotion.speed_multiplier = 0.0;
            packet.locomotion.lift_fraction = 0.22;
            packet.support = surface_command(active, 0.42, 0.38, 0.28);
            packet.material.motor_gain_multiplier = 0.58;
            packet.material.flight_damping_multiplier = 1.34;
            packet.material.viscosity_multiplier = 1.18;
            packet.internal.flow_speed_multiplier = 0.36;
            packet.internal.flow_strength_multiplier = 0.46;
            packet.internal.breath_amplitude_multiplier = 0.58;
            packet.internal.breath_speed_multiplier = 0.44;
            packet.expression.eye_aperture_delta = -0.72;
            // Sleeping load is visible in the body, not only in the eyelids:
            // gravity spreads the lower metaballs into the screen-edge support.
            push_field(
                packet,
                body_field(
                    SomaticFieldKind::Flatten,
                    Vec2::new(0.0, 0.30),
                    Vec2::Y,
                    0.72,
                    0.46 * tuning.support_gain,
                    progress,
                ),
            );
            if phase == "micro_arousal_or_continue" {
                packet.expression.eye_aperture_delta = -0.48;
                packet.internal.pulse_amplitude = 0.12;
            }
            if phase_started && phase == "sleep_onset" {
                packet.voice.semantic = VoiceSemanticIntent::PhysiologicalBreath;
                packet.voice.emit_once = true;
                packet.voice.intensity = 0.12;
            }
        }
        P::RestRemDreamWake => {
            packet.locomotion.pose = MotorPoseIntent::SupportedSleep;
            packet.locomotion.speed_multiplier = 0.0;
            packet.support = surface_command(active, 0.40, 0.36, 0.26);
            packet.material.flight_damping_multiplier = 1.28;
            packet.internal.flow_speed_multiplier = 0.48;
            packet.expression.eye_aperture_delta = -0.62;
            if phase == "structured_local_twitches" {
                let side = if active.sampled_style.arc_sign >= 0.0 {
                    1.0
                } else {
                    -1.0
                };
                push_field(
                    packet,
                    body_field(
                        SomaticFieldKind::Pulse,
                        Vec2::new(side * 0.24, 0.05),
                        Vec2::new(side, 0.2),
                        0.24,
                        0.18,
                        progress,
                    ),
                );
            }
            if phase == "dream_murmur" && phase_started {
                packet.voice.semantic = VoiceSemanticIntent::DreamMurmur;
                packet.voice.emit_once = true;
                packet.voice.intensity = 0.10;
            }
            if phase == "wake_stretch_or_nrem" {
                packet.locomotion.pose = MotorPoseIntent::Recover;
                packet.expression.eye_aperture_delta = -0.20 + 0.20 * smooth(progress);
                push_field(
                    packet,
                    body_field(
                        SomaticFieldKind::Gather,
                        Vec2::ZERO,
                        Vec2::Y,
                        0.75,
                        0.24,
                        progress,
                    ),
                );
            }
        }
        _ => unreachable!(),
    }
    true
}
