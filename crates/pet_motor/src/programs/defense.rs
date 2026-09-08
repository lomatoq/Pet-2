use glam::Vec2;
use lifecore::BehaviorGoalFrame;

use crate::{
    ActivePerformance, BehaviorContextFrame, BehaviorProgramId as P, MotorPoseIntent,
    MotorReadabilityTuning, SomaticActuationPacket, SomaticFieldKind, VoiceSemanticIntent,
    body_field, component_field, push_field,
};

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
        P::DefenseThreatHardenCompact | P::DefenseFragmentTrackAndRemerge
    ) {
        return false;
    }
    packet.locomotion.pose = MotorPoseIntent::Threat;
    packet.locomotion.speed_multiplier = 0.0;
    match active.program {
        P::DefenseThreatHardenCompact => {
            packet.material.density_compliance_multiplier = 0.74;
            packet.material.viscosity_multiplier = 1.34;
            packet.material.surface_tension_multiplier = 1.20;
            packet.material.flight_stretch_multiplier = 0.84;
            packet.internal.flow_speed_multiplier = 0.34;
            packet.internal.flow_strength_multiplier = 0.48;
            packet.expression.squint_delta = 0.28;
            if matches!(phase, "compact" | "increase_tone" | "hold_escape_axis") {
                let escape = if context.body.contact.normal.length_squared() > 0.0 {
                    context.body.contact.normal
                } else {
                    (context.body.motion.world_position - context.cursor_position)
                        .normalize_or_zero()
                };
                push_field(
                    packet,
                    body_field(
                        SomaticFieldKind::Gather,
                        Vec2::ZERO,
                        escape,
                        0.78,
                        0.46 * tuning.local_field_gain,
                        progress,
                    ),
                );
                push_field(
                    packet,
                    body_field(
                        SomaticFieldKind::Brace,
                        Vec2::ZERO,
                        escape,
                        0.68,
                        0.42,
                        progress,
                    ),
                );
            }
            if phase == "release_gradually" {
                packet.material.density_compliance_multiplier = 0.74 + progress * 0.26;
                packet.material.viscosity_multiplier = 1.34 - progress * 0.34;
            }
            if phase_started && phase == "increase_tone" {
                packet.voice.semantic = VoiceSemanticIntent::Boundary;
                packet.voice.emit_once = true;
                packet.voice.intensity = 0.46;
            }
        }
        P::DefenseFragmentTrackAndRemerge => {
            packet.locomotion.pose = MotorPoseIntent::Recover;
            packet.material.surface_tension_multiplier = 1.24;
            packet.material.shape_recovery_delta = 0.30;
            let component = active
                .locked_target
                .as_ref()
                .and_then(|target| match target {
                    crate::BehaviorTarget::Component(id) => Some(*id),
                    _ => None,
                })
                .unwrap_or(1);
            if matches!(
                phase,
                "approach_both_sides" | "first_contact" | "zipper_coalescence"
            ) {
                push_field(
                    packet,
                    component_field(
                        SomaticFieldKind::Gather,
                        component,
                        Vec2::ZERO,
                        1.0,
                        0.58 * tuning.local_field_gain,
                        progress,
                    ),
                );
                push_field(
                    packet,
                    body_field(
                        SomaticFieldKind::Gather,
                        Vec2::ZERO,
                        Vec2::ZERO,
                        0.92,
                        0.32,
                        progress,
                    ),
                );
            }
            if phase == "rebound_wave" {
                push_field(
                    packet,
                    body_field(
                        SomaticFieldKind::Wave,
                        Vec2::ZERO,
                        Vec2::Y,
                        0.85,
                        0.22,
                        progress,
                    ),
                );
            }
            if phase == "relief" && context.body.topology.connected_components <= 1 {
                packet.expression.relief = 0.78;
                if phase_started {
                    packet.voice.semantic = VoiceSemanticIntent::Relief;
                    packet.voice.emit_once = true;
                    packet.voice.intensity = 0.36;
                }
            }
        }
        _ => unreachable!(),
    }
    true
}
