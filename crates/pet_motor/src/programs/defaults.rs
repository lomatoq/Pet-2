use glam::Vec2;
use lifecore::BehaviorGoalFrame;

use crate::{
    ActivePerformance, BehaviorContextFrame, BehaviorProgramId as P, FieldSpace, MotorPoseIntent,
    MotorReadabilityTuning, ProgramFamily, SomaticActuationPacket, SomaticFieldKind,
    VoiceSemanticIntent, body_field, push_field, wave_field,
};

/// Family-level procedural defaults for the catalog programs that do not need
/// bespoke tuning. One controller per family keeps all 64 programs executable
/// and bounded without 64 independent visual calibration passes.
#[allow(clippy::too_many_arguments)]
pub(crate) fn apply(
    packet: &mut SomaticActuationPacket,
    active: &ActivePerformance,
    phase: &str,
    progress: f32,
    phase_started: bool,
    goal: &BehaviorGoalFrame,
    context: &BehaviorContextFrame,
    tuning: MotorReadabilityTuning,
) -> bool {
    let sign = active.sampled_style.arc_sign;
    let amplitude = active.sampled_style.amplitude;
    match active.program.family() {
        ProgramFamily::RestSleep => {
            packet.locomotion.pose = MotorPoseIntent::SupportedRest;
            packet.locomotion.speed_multiplier = 0.0;
            packet.locomotion.lift_fraction = 0.44;
            packet.material.viscosity_multiplier = 1.12;
            packet.material.flight_damping_multiplier = 1.22;
            packet.internal.flow_speed_multiplier = 0.62;
            packet.internal.breath_speed_multiplier = 0.72;
            if phase == "lower_load" || phase == "settle_contact" {
                push_field(
                    packet,
                    body_field(
                        SomaticFieldKind::Flatten,
                        Vec2::new(sign * 0.08, 0.26),
                        Vec2::Y,
                        0.58,
                        0.22 * amplitude * tuning.support_gain,
                        progress,
                    ),
                );
            }
        }
        ProgramFamily::LocomotionAttention => {
            packet.locomotion.pose = if phase == "orient" {
                MotorPoseIntent::Orient
            } else {
                MotorPoseIntent::Travel
            };
            packet.locomotion.gaze_lead = 1.0;
            packet.locomotion.speed_multiplier = match active.program {
                P::MoveCautiousApproach => 0.42,
                P::MoveExcitedDashOvershoot => 1.28,
                P::MoveInspectPauseScan | P::MoveCheckBackSocialReference => 0.34,
                _ => 0.72,
            };
            packet.locomotion.approach_arc = sign
                * if active.program == P::MoveCuriosityArcApproach {
                    0.22
                } else {
                    0.06
                };
            packet.material.flight_stretch_multiplier = 1.06;
            push_field(
                packet,
                body_field(
                    SomaticFieldKind::MassShift,
                    Vec2::new(sign * 0.18, -0.04),
                    Vec2::new(sign, 0.08),
                    0.48,
                    0.16 * amplitude * tuning.preparation_gain,
                    progress,
                ),
            );
        }
        ProgramFamily::AffiliationSocial => {
            packet.locomotion.pose = MotorPoseIntent::OfferContact;
            packet.locomotion.speed_multiplier = if phase == "approach_or_present" {
                0.48
            } else {
                0.0
            };
            packet.locomotion.gaze_lead = 1.0;
            packet.expression.gaze_target = Some(context.cursor_position);
            packet.material.density_compliance_multiplier = 1.06;
            packet.material.viscosity_multiplier = 0.94;
            packet.internal.flow_speed_multiplier = 0.78;
            packet.internal.breath_speed_multiplier = 0.86;
            if phase == "signal" {
                packet.expression.blink = if matches!(
                    active.program,
                    P::SocialSlowBlinkAffiliation | P::SocialMutualGazePulse
                ) {
                    0.82
                } else {
                    0.24
                };
                push_field(
                    packet,
                    wave_field(
                        FieldSpace::BodyLocal,
                        Vec2::new(sign * 0.20, 0.04),
                        Vec2::X * sign,
                        0.44,
                        0.13 * amplitude * tuning.local_field_gain,
                        0.85,
                        progress,
                    ),
                );
                if phase_started && active.program == P::SocialAttentionBidWait {
                    packet.voice.semantic = VoiceSemanticIntent::Invite;
                    packet.voice.emit_once = true;
                    packet.voice.intensity = 0.24;
                }
            }
        }
        ProgramFamily::TouchManipulation => {
            packet.locomotion.pose = MotorPoseIntent::Touch;
            packet.locomotion.speed_multiplier = 0.0;
            packet.expression.gaze_target = Some(context.cursor_position);
            packet.material.density_compliance_multiplier = 1.10;
            packet.material.viscosity_multiplier = 0.90;
            let kind = match active.program {
                P::TouchCircularStirCooperate => SomaticFieldKind::Curl,
                P::TouchTickleWriggle | P::TouchRhythmicTouchSync => SomaticFieldKind::Wave,
                P::TouchLeanIntoStroke => SomaticFieldKind::Attract,
                _ => SomaticFieldKind::Shear,
            };
            push_field(
                packet,
                crate::LocalSomaticField {
                    kind,
                    space: FieldSpace::PointerContact,
                    center: Vec2::ZERO,
                    axis: context.cursor_velocity.normalize_or(Vec2::new(sign, 0.0)),
                    radius: 0.34,
                    strength: 0.20 * amplitude * tuning.local_field_gain,
                    falloff: 2.4,
                    frequency_hz: if matches!(kind, SomaticFieldKind::Wave) {
                        2.2
                    } else {
                        0.0
                    },
                    phase_01: progress,
                    target_component: context.body.contact.component_id,
                },
            );
        }
        ProgramFamily::PlayObject => {
            packet.locomotion.pose = MotorPoseIntent::Content;
            packet.locomotion.speed_multiplier = if phase == "play_bout" { 1.06 } else { 0.18 };
            packet.locomotion.approach_arc = sign * 0.16;
            packet.locomotion.gaze_lead = 1.0;
            packet.material.density_compliance_multiplier = 1.12;
            packet.material.viscosity_multiplier = 0.78;
            packet.material.motor_gain_multiplier = 1.14;
            packet.internal.flow_speed_multiplier = 1.22;
            push_field(
                packet,
                body_field(
                    match active.program {
                        P::PlayOrbCatchEnvelop => SomaticFieldKind::Grip,
                        P::PlaySelfPlayDroplet => SomaticFieldKind::Bud,
                        _ => SomaticFieldKind::Orbit,
                    },
                    Vec2::new(sign * 0.18, 0.04),
                    Vec2::new(sign, -0.10),
                    0.48,
                    0.20 * amplitude * tuning.local_field_gain,
                    progress,
                ),
            );
        }
        ProgramFamily::MetabolismHome => {
            packet.locomotion.pose = if matches!(
                active.program,
                P::HomeDigestionSatiation | P::HomeDenNestRest
            ) {
                MotorPoseIntent::SupportedRest
            } else {
                MotorPoseIntent::Travel
            };
            packet.locomotion.speed_multiplier = if phase == "approach_or_sample" {
                0.58
            } else {
                0.0
            };
            packet.material.viscosity_multiplier = 1.06;
            packet.internal.breath_speed_multiplier = 0.82;
            packet.internal.flow_speed_multiplier = 0.72;
            push_field(
                packet,
                body_field(
                    if active.program == P::HomeFoodRefusePushAway {
                        SomaticFieldKind::Repel
                    } else {
                        SomaticFieldKind::Gather
                    },
                    Vec2::new(sign * 0.12, 0.12),
                    Vec2::new(sign, 0.0),
                    0.52,
                    0.14 * amplitude * tuning.local_field_gain,
                    progress,
                ),
            );
        }
        ProgramFamily::DefenseIntegrity => {
            packet.locomotion.pose = MotorPoseIntent::Threat;
            packet.locomotion.speed_multiplier = 0.0;
            packet.material.density_compliance_multiplier = 0.78;
            packet.material.viscosity_multiplier = 1.30;
            packet.material.surface_tension_multiplier = 1.20;
            packet.material.flight_damping_multiplier = 1.30;
            packet.internal.flow_damping = 0.54;
            push_field(
                packet,
                body_field(
                    if active.program == P::DefensePostStressShakeOff {
                        SomaticFieldKind::Wave
                    } else {
                        SomaticFieldKind::Brace
                    },
                    Vec2::new(sign * 0.16, 0.0),
                    Vec2::new(-sign, 0.0),
                    0.56,
                    0.26 * amplitude * tuning.local_field_gain,
                    progress,
                ),
            );
            if phase_started && active.program == P::DefenseOverpressureBoundary {
                packet.voice.semantic = VoiceSemanticIntent::Boundary;
                packet.voice.emit_once = true;
                packet.voice.intensity = 0.30;
            }
        }
        ProgramFamily::PhysiologyMaterial => {
            packet.locomotion.pose = if active.program == P::StateSadHeavySag {
                MotorPoseIntent::Recover
            } else {
                MotorPoseIntent::Content
            };
            packet.locomotion.speed_multiplier = 0.0;
            packet.material.viscosity_multiplier = if active.program == P::StateSadHeavySag {
                1.18
            } else {
                0.94
            };
            packet.internal.flow_speed_multiplier = if active.program == P::StateSadHeavySag {
                0.48
            } else {
                0.86
            };
            packet.internal.breath_speed_multiplier = 0.82;
            push_field(
                packet,
                body_field(
                    match active.program {
                        P::StateSadHeavySag => SomaticFieldKind::GravityBias,
                        P::StateCuriousProbeBud => SomaticFieldKind::Bud,
                        P::StateBoredFidgetSelfStim => SomaticFieldKind::Pulse,
                        P::StateSelfGroomRealign | P::StateEmotionTransitionSettle => {
                            SomaticFieldKind::Gather
                        }
                        _ => SomaticFieldKind::Wave,
                    },
                    Vec2::new(sign * 0.18, 0.12),
                    Vec2::new(sign * 0.25, 1.0),
                    0.54,
                    0.16 * amplitude * tuning.local_field_gain,
                    progress,
                ),
            );
            if phase_started && active.program == P::StateSocialPurrCoregulation {
                packet.voice.semantic = VoiceSemanticIntent::Purr;
                packet.voice.emit_once = true;
                packet.voice.intensity = goal.attachment.clamp(0.25, 0.55);
            }
        }
    }
    true
}
