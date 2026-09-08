use glam::Vec2;
use lifecore::BehaviorGoalFrame;

use crate::{
    ActivePerformance, BehaviorContextFrame, BehaviorProgramId as P, FieldSpace, MotorPoseIntent,
    MotorReadabilityTuning, SomaticActuationPacket, SomaticFieldKind, VoiceSemanticIntent,
    body_field, push_field, wave_field,
};

use super::smooth;

#[allow(clippy::too_many_arguments)]
pub(crate) fn apply(
    packet: &mut SomaticActuationPacket,
    active: &ActivePerformance,
    phase: &str,
    progress: f32,
    phase_started: bool,
    _goal: &BehaviorGoalFrame,
    _context: &BehaviorContextFrame,
    tuning: MotorReadabilityTuning,
) -> bool {
    if !matches!(
        active.program,
        P::StateContentedOpenDrift | P::StateRespiratorySighReset
    ) {
        return false;
    }
    match active.program {
        P::StateContentedOpenDrift => {
            packet.locomotion.pose = MotorPoseIntent::Content;
            packet.material.density_compliance_multiplier = 1.08;
            packet.material.viscosity_multiplier = 0.88;
            packet.material.surface_tension_multiplier = 0.94;
            packet.material.rest_spacing_multiplier = 1.05;
            packet.internal.flow_speed_multiplier = 0.66;
            if phase == "open_lobes" {
                let side = active.sampled_style.arc_sign;
                push_field(
                    packet,
                    body_field(
                        SomaticFieldKind::MassShift,
                        Vec2::new(side * 0.20, 0.0),
                        Vec2::X * side,
                        0.44,
                        0.20 * tuning.local_field_gain,
                        progress,
                    ),
                );
            }
            if phase == "slow_internal_rotation" {
                push_field(
                    packet,
                    body_field(
                        SomaticFieldKind::Orbit,
                        Vec2::ZERO,
                        Vec2::Y,
                        0.70,
                        0.12,
                        progress,
                    ),
                );
            }
            if phase == "short_drift_bout" {
                packet.locomotion.speed_multiplier = 0.24;
                packet.locomotion.approach_arc = active.sampled_style.arc_sign * 0.04;
            } else {
                packet.locomotion.speed_multiplier = 0.0;
            }
            if phase == "pause" {
                packet.locomotion.arrival_pause = 1.0;
            }
            if phase == "recenter" {
                push_field(
                    packet,
                    body_field(
                        SomaticFieldKind::Gather,
                        Vec2::ZERO,
                        Vec2::ZERO,
                        0.78,
                        0.14,
                        progress,
                    ),
                );
            }
        }
        P::StateRespiratorySighReset => {
            packet.locomotion.pose = MotorPoseIntent::Content;
            packet.locomotion.speed_multiplier = 0.0;
            match phase {
                "deep_inhale" => {
                    packet.internal.breath_amplitude_multiplier = 1.55;
                    packet.internal.breath_speed_multiplier = 0.62;
                    push_field(
                        packet,
                        wave_field(
                            FieldSpace::BodyLocal,
                            Vec2::ZERO,
                            Vec2::Y,
                            0.82,
                            0.34 * tuning.local_field_gain,
                            0.42,
                            progress,
                        ),
                    );
                }
                "brief_hold" => {
                    packet.internal.breath_amplitude_multiplier = 1.28;
                    packet.internal.breath_speed_multiplier = 0.30;
                }
                "long_exhale" => {
                    packet.internal.breath_amplitude_multiplier = 1.18 - 0.40 * smooth(progress);
                    packet.internal.breath_speed_multiplier = 0.42;
                    packet.internal.flow_damping = 0.28 + 0.24 * smooth(progress);
                    if phase_started {
                        packet.voice.semantic = VoiceSemanticIntent::PhysiologicalBreath;
                        packet.voice.emit_once = true;
                        packet.voice.intensity = 0.22;
                    }
                }
                "rhythm_reset" => {
                    packet.internal.breath_amplitude_multiplier = 0.82 + 0.18 * smooth(progress);
                    packet.internal.breath_speed_multiplier = 0.72 + 0.28 * smooth(progress);
                }
                _ => {}
            }
        }
        _ => unreachable!(),
    }
    true
}

pub(crate) fn apply_regime(packet: &mut SomaticActuationPacket, regime: crate::RegimeBlend) {
    use crate::SomaticRegime as R;
    let primary = regime.primary;
    let secondary = regime.secondary;
    let w = regime.primary_weight.clamp(0.5, 1.0);
    let a = regime_material(primary);
    let b = regime_material(secondary);
    packet.material.density_compliance_multiplier = a.0 * w + b.0 * (1.0 - w);
    packet.material.viscosity_multiplier = a.1 * w + b.1 * (1.0 - w);
    packet.material.surface_tension_multiplier = a.2 * w + b.2 * (1.0 - w);
    packet.material.motor_gain_multiplier = a.3 * w + b.3 * (1.0 - w);
    packet.internal.flow_speed_multiplier = a.4 * w + b.4 * (1.0 - w);
    if primary == R::Fatigued {
        packet.locomotion.lift_fraction = 0.58;
        packet.material.flight_damping_multiplier = 1.22;
    }
    if primary == R::Threatened {
        packet.locomotion.pose = MotorPoseIntent::Threat;
    }
}

fn regime_material(regime: crate::SomaticRegime) -> (f32, f32, f32, f32, f32) {
    use crate::SomaticRegime as R;
    match regime {
        R::CalmContent => (1.07, 0.90, 0.94, 0.90, 0.66),
        R::Playful => (1.12, 0.78, 1.01, 1.12, 1.24),
        R::Affiliative => (1.04, 0.96, 0.98, 0.94, 0.82),
        R::Curious => (1.04, 0.92, 0.98, 0.98, 1.08),
        R::Fatigued => (1.10, 1.18, 1.02, 0.66, 0.48),
        R::SadLowValence => (1.08, 1.14, 0.94, 0.70, 0.48),
        R::Threatened => (0.78, 1.32, 1.20, 0.90, 0.34),
        R::Frustrated => (0.94, 1.06, 1.04, 1.02, 0.88),
    }
}
