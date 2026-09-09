use lifecore::{
    BodyIntent, CausalSourceTerm, CausalTargetRecord, FastPhenotypeActuation, LocomotionMode,
    PoseIntent,
};

use crate::{BehaviorContextFrame, MotorPoseIntent, SomaticActuationPacket, SomaticRegime};

/// Bounded owner-aware merge between an active phase performance and the
/// continuous nervous phenotype. Motor timing is stronger than ambient state;
/// integrity and threat performances also suppress incompatible positive play.
pub struct SomaticActuationBus;

impl SomaticActuationBus {
    pub fn compose(phenotype: &mut FastPhenotypeActuation, packet: &SomaticActuationPacket) {
        let m = packet.material;
        let rest_spacing_compliance = 1.0 + (m.rest_spacing_multiplier - 1.0) * 1.4;
        phenotype.pbf.density_compliance_multiplier = (phenotype.pbf.density_compliance_multiplier
            * m.density_compliance_multiplier
            * rest_spacing_compliance)
            .clamp(0.60, 1.40);
        phenotype.pbf.viscosity_multiplier =
            (phenotype.pbf.viscosity_multiplier * m.viscosity_multiplier).clamp(0.65, 1.50);
        phenotype.pbf.surface_tension_multiplier = (phenotype.pbf.surface_tension_multiplier
            * m.surface_tension_multiplier)
            .clamp(0.75, 1.35);
        phenotype.pbf.flight_damping_multiplier = (phenotype.pbf.flight_damping_multiplier
            * m.flight_damping_multiplier)
            .clamp(0.70, 1.45);
        phenotype.pbf.flight_stretch_multiplier = (phenotype.pbf.flight_stretch_multiplier
            * m.flight_stretch_multiplier)
            .clamp(0.75, 1.40);
        phenotype.pbf.motor_gain_multiplier =
            (phenotype.pbf.motor_gain_multiplier * m.motor_gain_multiplier).clamp(0.50, 1.35);
        phenotype.pbf.angular_damping_multiplier = (phenotype.pbf.angular_damping_multiplier
            * m.angular_damping_multiplier)
            .clamp(0.70, 1.45);
        phenotype.pbf.shape_recovery_delta =
            (phenotype.pbf.shape_recovery_delta + m.shape_recovery_delta).clamp(-0.20, 0.55);
        phenotype.pbf.idle_breath_amplitude_multiplier =
            (phenotype.pbf.idle_breath_amplitude_multiplier
                * packet.internal.breath_amplitude_multiplier)
                .clamp(0.35, 1.80);
        phenotype.pbf.idle_breath_speed_multiplier = (phenotype.pbf.idle_breath_speed_multiplier
            * packet.internal.breath_speed_multiplier)
            .clamp(0.30, 1.80);
        phenotype.material.internal_flow_multiplier = (phenotype.material.internal_flow_multiplier
            * packet.internal.flow_strength_multiplier
            * (1.0 - packet.internal.flow_damping * 0.65))
            .clamp(0.20, 1.60);
        phenotype.material.caustic_speed_multiplier = (phenotype.material.caustic_speed_multiplier
            * packet.internal.flow_speed_multiplier)
            .clamp(0.20, 1.60);
        phenotype.visual_physiology.flow_strength_multiplier =
            (phenotype.visual_physiology.flow_strength_multiplier
                * packet.internal.flow_strength_multiplier
                * (1.0 - packet.internal.flow_damping * 0.65))
                .clamp(0.20, 1.60);
        phenotype.visual_physiology.flow_speed_multiplier =
            (phenotype.visual_physiology.flow_speed_multiplier
                * packet.internal.flow_speed_multiplier)
                .clamp(0.20, 1.60);
        phenotype.visual_physiology.pulse_amplitude = phenotype
            .visual_physiology
            .pulse_amplitude
            .max(packet.internal.pulse_amplitude);

        if let Some(gaze) = packet.expression.gaze_target {
            phenotype.face.gaze_target = Some(gaze);
        }
        phenotype.expression.eye_aperture = (phenotype.expression.eye_aperture
            + packet.expression.eye_aperture_delta)
            .clamp(0.0, 1.0);
        phenotype.expression.squint =
            (phenotype.expression.squint + packet.expression.squint_delta).clamp(0.0, 1.0);
        phenotype.expression.mouth_open = phenotype
            .expression
            .mouth_open
            .max(packet.expression.mouth_open);
        phenotype.expression.relief = phenotype.expression.relief.max(packet.expression.relief);
        phenotype.expression.effort = phenotype.expression.effort.max(packet.expression.effort);
        if packet.expression.blink > 0.0 {
            phenotype.expression.blink_left =
                phenotype.expression.blink_left.max(packet.expression.blink);
            phenotype.expression.blink_right = phenotype
                .expression
                .blink_right
                .max(packet.expression.blink * 0.94);
        }

        phenotype.action.speed =
            (phenotype.action.speed * packet.locomotion.speed_multiplier.max(0.05)).clamp(0.0, 1.0);
        phenotype.action.settle = phenotype.action.settle.max(
            packet
                .locomotion
                .arrival_pause
                .max(1.0 - packet.locomotion.lift_fraction),
        );

        if packet.expression.acknowledgement > 0.0 {
            phenotype.expression.pupil_focus = phenotype.expression.pupil_focus.max(0.90);
            phenotype.face.semantic_roll += 0.045 * packet.expression.acknowledgement;
        }
        let meaningful = packet.expression.acknowledgement > 0.0
            || crate::response_phase(&packet.phase_name)
            || matches!(
                packet.phase_name.as_str(),
                "signal" | "present_side" | "inspect" | "hold_boundary"
            )
            || matches!(
                packet.program,
                Some(crate::BehaviorProgramId::MoveInspectPauseScan)
            );
        if meaningful {
            phenotype.face.microsaccade_amount_multiplier *= 0.4;
            phenotype.pbf.idle_lean_angle_multiplier *= 0.5;
            phenotype.visual_physiology.droplet_energy *= 0.6;
        }
        let protective = packet
            .program
            .is_some_and(|program| program.family() == crate::ProgramFamily::DefenseIntegrity)
            || packet.regime.primary == SomaticRegime::Threatened;
        if protective {
            phenotype.interaction.allow_intentional_bud = false;
            phenotype.interaction.cooperation = 0.0;
            phenotype.action.play = 0.0;
            phenotype.action.social_approach = 0.0;
            phenotype.expression.mouth_curve = phenotype.expression.mouth_curve.min(0.0);
            phenotype.expression.cheek_glow = 0.0;
            phenotype.expression.geometry.mouth[1] =
                phenotype.expression.geometry.mouth[1].min(0.0);
            phenotype.expression.geometry.mouth[2] =
                phenotype.expression.geometry.mouth[2].min(0.0);
            phenotype.expression.geometry.lids[0][2] = 0.0;
            phenotype.expression.geometry.lids[1][2] = 0.0;
            phenotype.material.soul_glow_strength_multiplier =
                phenotype.material.soul_glow_strength_multiplier.min(1.0);
            if packet.regime.primary == SomaticRegime::Threatened {
                phenotype.expression.squint = phenotype.expression.squint.max(0.28);
            }
            phenotype.voice.purr_amount = 0.0;
            phenotype.voice.trill_amount = 0.0;
        }
        append_trace(phenotype, packet);
    }

    pub fn apply_to_intent(
        packet: &SomaticActuationPacket,
        context: &BehaviorContextFrame,
        intent: &mut BodyIntent,
    ) {
        if let Some(target) = packet.locomotion.target_position {
            let offset = target - context.body.motion.world_position;
            let tangent = glam::Vec2::new(-offset.y, offset.x).normalize_or_zero();
            let arc = tangent * packet.locomotion.approach_arc * offset.length().min(0.30);
            intent.target_position = (target + arc).clamp(glam::Vec2::ZERO, glam::Vec2::ONE);
        }
        // A neutral packet must preserve a stop or an intentionally small effort.
        let base_speed = if packet.program.is_some()
            && matches!(
                packet.locomotion.pose,
                MotorPoseIntent::Travel | MotorPoseIntent::Landing
            ) {
            intent.desired_speed.max(0.08)
        } else {
            intent.desired_speed.max(0.0)
        };
        intent.desired_speed = if packet.locomotion.speed_multiplier <= 0.001 {
            0.0
        } else {
            (base_speed
                * packet.locomotion.speed_multiplier
                * packet.locomotion.acceleration_limit.sqrt()
                * (1.0 - packet.locomotion.braking * 0.88))
                .clamp(0.0, 1.0)
        };
        if let Some(gaze) = packet.expression.gaze_target {
            intent.gaze_target = Some(gaze.clamp(glam::Vec2::ZERO, glam::Vec2::ONE));
        } else if packet.locomotion.gaze_lead > 0.01
            && let Some(target) = packet.locomotion.target_position
        {
            intent.gaze_target = Some(target.clamp(glam::Vec2::ZERO, glam::Vec2::ONE));
        }
        if let Some(support) = &packet.support {
            intent.target_surface = Some(support.surface_id.clone());
        }
        match packet.locomotion.pose {
            MotorPoseIntent::Neutral => {}
            MotorPoseIntent::Orient => {
                intent.pose = PoseIntent::Curious;
                intent.locomotion = LocomotionMode::Hover;
            }
            MotorPoseIntent::Travel => {
                intent.locomotion = LocomotionMode::Arrive;
            }
            MotorPoseIntent::Brake => {
                intent.locomotion = LocomotionMode::Arrive;
            }
            MotorPoseIntent::Landing => {
                intent.pose = PoseIntent::Landing;
                intent.locomotion = LocomotionMode::Landing;
            }
            MotorPoseIntent::SupportedRest => {
                intent.pose = PoseIntent::Landing;
                intent.locomotion = LocomotionMode::Landing;
            }
            MotorPoseIntent::SupportedSleep => {
                if context.support_confirmed() {
                    intent.pose = PoseIntent::Sleeping;
                    intent.locomotion = LocomotionMode::Sleep;
                } else {
                    intent.pose = PoseIntent::Landing;
                    intent.locomotion = LocomotionMode::Landing;
                }
            }
            MotorPoseIntent::Touch | MotorPoseIntent::OfferContact => {
                intent.pose = PoseIntent::Curious;
                intent.locomotion = LocomotionMode::Hover;
            }
            MotorPoseIntent::Threat => {
                intent.pose = PoseIntent::Compact;
            }
            MotorPoseIntent::Content => {
                intent.pose = PoseIntent::Neutral;
            }
            MotorPoseIntent::Recover => {
                intent.pose = PoseIntent::Compact;
                intent.locomotion = LocomotionMode::Hover;
            }
        }
    }
}

fn append_trace(phenotype: &mut FastPhenotypeActuation, packet: &SomaticActuationPacket) {
    let Some(program) = packet.program else {
        return;
    };
    let episode_id = phenotype.episode_id;
    let frame_id = packet.frame_id;
    let phase = packet.phase_name.clone();
    let source_terms = vec![
        CausalSourceTerm {
            path: format!("motor.program.{}", program.wire_name()),
            value: 1.0,
        },
        CausalSourceTerm {
            path: format!("motor.phase.{phase}"),
            value: 1.0,
        },
    ];
    for (path, raw, effective, lo, hi) in [
        (
            "motor.material.density_compliance_multiplier",
            packet.material.density_compliance_multiplier,
            phenotype.pbf.density_compliance_multiplier,
            0.60,
            1.40,
        ),
        (
            "motor.material.viscosity_multiplier",
            packet.material.viscosity_multiplier,
            phenotype.pbf.viscosity_multiplier,
            0.65,
            1.50,
        ),
        (
            "motor.material.rest_spacing_multiplier",
            packet.material.rest_spacing_multiplier,
            packet.material.rest_spacing_multiplier,
            0.90,
            1.12,
        ),
        (
            "motor.material.surface_tension_multiplier",
            packet.material.surface_tension_multiplier,
            phenotype.pbf.surface_tension_multiplier,
            0.75,
            1.35,
        ),
        (
            "motor.locomotion.speed_multiplier",
            packet.locomotion.speed_multiplier,
            packet.locomotion.speed_multiplier,
            0.0,
            1.50,
        ),
        (
            "motor.internal.flow_speed_multiplier",
            packet.internal.flow_speed_multiplier,
            phenotype.material.caustic_speed_multiplier,
            0.20,
            1.60,
        ),
    ] {
        phenotype.trace.push(CausalTargetRecord {
            frame_id,
            episode_id,
            target_path: path.to_owned(),
            coupling_id: "r13_behavior_performance_runtime".to_owned(),
            formula: format!(
                "program={} phase={} bounded motor-over-phenotype composition",
                program.wire_name(),
                phase
            ),
            source_terms: source_terms.clone(),
            identity_or_profile_base: 1.0,
            raw_target: raw,
            influence_budget_scale: 1.0,
            clamp_min: lo,
            clamp_max: hi,
            clamp_reason: "r13_motor_safety_envelope".to_owned(),
            filtered_value: effective,
            effective_value: effective,
            persistence_class: "phase_ephemeral".to_owned(),
            rise_tau_seconds: 0.06,
            fall_tau_seconds: 0.24,
            component_id_if_local: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use glam::Vec2;
    use lifecore::{BodyIntent, FastPhenotypeActuation, LocomotionMode, PoseIntent};

    use super::*;
    use crate::BehaviorProgramId;

    fn intent() -> BodyIntent {
        BodyIntent {
            locomotion: LocomotionMode::Arrive,
            target_position: Vec2::new(0.7, 0.5),
            target_surface: None,
            desired_speed: 0.8,
            facing_direction: 1.0,
            gaze_target: None,
            pose: PoseIntent::Neutral,
            expression: Default::default(),
            interaction_target: None,
        }
    }

    #[test]
    fn neutral_packet_preserves_stop_and_small_effort() {
        for speed in [0.0, 0.01, 0.04] {
            let mut intent = intent();
            intent.desired_speed = speed;
            SomaticActuationBus::apply_to_intent(
                &SomaticActuationPacket::default(),
                &BehaviorContextFrame::default(),
                &mut intent,
            );
            assert_eq!(intent.desired_speed, speed);
        }
    }

    #[test]
    fn locomotion_envelope_has_measured_semantic_consequences() {
        let mut packet = SomaticActuationPacket::default();
        packet.locomotion.target_position = Some(Vec2::new(0.80, 0.66));
        packet.locomotion.approach_arc = 0.20;
        packet.locomotion.speed_multiplier = 0.8;
        packet.locomotion.acceleration_limit = 0.25;
        packet.locomotion.braking = 0.50;
        packet.locomotion.gaze_lead = 1.0;
        let mut context = BehaviorContextFrame::default();
        context.body.motion.world_position = Vec2::new(0.20, 0.30);
        let mut intent = intent();
        SomaticActuationBus::apply_to_intent(&packet, &context, &mut intent);
        assert_ne!(
            intent.target_position,
            packet.locomotion.target_position.unwrap()
        );
        assert!((0.0..0.8).contains(&intent.desired_speed));
        assert_eq!(intent.gaze_target, packet.locomotion.target_position);
    }

    #[test]
    fn integrity_program_suppresses_incompatible_play() {
        let material = crate::BoundedMaterialActuation {
            rest_spacing_multiplier: 1.10,
            ..crate::BoundedMaterialActuation::default()
        };
        let packet = SomaticActuationPacket {
            program: Some(BehaviorProgramId::DefenseThreatHardenCompact),
            material,
            ..SomaticActuationPacket::default()
        };
        let mut phenotype = FastPhenotypeActuation::default();
        phenotype.action.play = 1.0;
        phenotype.interaction.cooperation = 1.0;
        SomaticActuationBus::compose(&mut phenotype, &packet);
        assert_eq!(phenotype.action.play, 0.0);
        assert_eq!(phenotype.interaction.cooperation, 0.0);
        assert!(phenotype.pbf.density_compliance_multiplier > 1.0);
    }
}
