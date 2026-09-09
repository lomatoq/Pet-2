use glam::Vec2;
use lifecore::BehaviorGoalFrame;

use crate::{
    ActivePerformance, BehaviorContextFrame, BehaviorProgramId as P, MotorPoseIntent,
    MotorReadabilityTuning, SomaticActuationPacket, SomaticFieldKind, VoiceSemanticIntent,
    contact_field, push_field,
};

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
    if !matches!(
        active.program,
        P::TouchSoftTouchYield | P::TouchSustainedHoldRelaxOrResist | P::TouchPullReleaseRebound
    ) {
        return false;
    }
    packet.locomotion.pose = MotorPoseIntent::Touch;
    packet.locomotion.speed_multiplier = 0.0;
    let contact_axis = if context.body.contact.normal.length_squared() > 0.0 {
        -context.body.contact.normal
    } else {
        Vec2::Y
    };
    match active.program {
        P::TouchSoftTouchYield => {
            packet.material.density_compliance_multiplier = 1.12;
            packet.material.viscosity_multiplier = 0.94;
            if matches!(phase, "local_yield" | "micro_lean") {
                push_field(
                    packet,
                    contact_field(
                        SomaticFieldKind::Attract,
                        contact_axis,
                        0.30,
                        0.24 * tuning.local_field_gain,
                        progress,
                    ),
                );
            }
            if phase == "recovery_or_hold" {
                push_field(
                    packet,
                    contact_field(
                        SomaticFieldKind::Wave,
                        -contact_axis,
                        0.50,
                        0.18 * tuning.follow_through_gain,
                        progress,
                    ),
                );
            }
            if phase_started && phase == "local_yield" {
                packet.voice.semantic = VoiceSemanticIntent::Contact;
                packet.voice.emit_once = true;
                packet.voice.intensity = 0.18;
            }
        }
        P::TouchSustainedHoldRelaxOrResist => {
            let unsafe_hold = goal.felt.restraint > 0.44
                || context.boundary_violation > 0.12
                || context.body.contact.pressure > 0.52;
            if unsafe_hold {
                packet.material.density_compliance_multiplier = 0.78;
                packet.material.viscosity_multiplier = 1.28;
                packet.material.surface_tension_multiplier = 1.18;
                push_field(
                    packet,
                    contact_field(SomaticFieldKind::Brace, -contact_axis, 0.48, 0.52, progress),
                );
                if phase_started && phase == "relax_or_brace" {
                    packet.voice.semantic = VoiceSemanticIntent::Boundary;
                    packet.voice.emit_once = true;
                    packet.voice.intensity = 0.48;
                }
            } else {
                packet.material.density_compliance_multiplier = 1.18;
                packet.material.viscosity_multiplier = 1.06;
                packet.internal.flow_damping = 0.26;
                push_field(
                    packet,
                    contact_field(SomaticFieldKind::Anchor, contact_axis, 0.36, 0.18, progress),
                );
            }
            if phase == "release_recover" {
                push_field(
                    packet,
                    contact_field(SomaticFieldKind::Wave, -contact_axis, 0.58, 0.20, progress),
                );
            }
        }
        P::TouchPullReleaseRebound => {
            packet.material.flight_damping_multiplier = 1.14;
            match phase {
                "tether_form" | "controlled_extension" => {
                    push_field(
                        packet,
                        contact_field(
                            SomaticFieldKind::Grip,
                            contact_axis,
                            0.28,
                            0.34 * tuning.local_field_gain,
                            progress,
                        ),
                    );
                    packet.expression.effort = context
                        .body
                        .shape
                        .neck_tension
                        .max(context.body.shape.maximum_strain);
                }
                "release_detect" => packet.material.flight_damping_multiplier = 1.35,
                "snap_limited_return" => {
                    push_field(
                        packet,
                        contact_field(
                            SomaticFieldKind::Gather,
                            -contact_axis,
                            0.70,
                            0.42 * tuning.follow_through_gain,
                            progress,
                        ),
                    );
                }
                "secondary_wave" => {
                    push_field(
                        packet,
                        contact_field(SomaticFieldKind::Wave, -contact_axis, 0.74, 0.26, progress),
                    );
                }
                "appraise" => {
                    let safe = context.body.shape.maximum_strain < 0.58
                        && context.body.topology.connected_components <= 1;
                    packet.expression.relief = f32::from(safe) * 0.60;
                    if phase_started && safe {
                        packet.voice.semantic = VoiceSemanticIntent::Relief;
                        packet.voice.emit_once = true;
                        packet.voice.intensity = 0.28;
                    }
                }
                _ => {}
            }
        }
        _ => unreachable!(),
    }
    true
}
