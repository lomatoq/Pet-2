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
    goal: &BehaviorGoalFrame,
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
    let support_tangent = active
        .locked_target
        .as_ref()
        .and_then(|target| match target {
            crate::BehaviorTarget::Surface(surface) => Some(surface.tangent),
            _ => None,
        })
        .unwrap_or(Vec2::X)
        .normalize_or(Vec2::X);
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
            // Loaded deformation is a held constraint, not a transient pose.
            // Removing it after spread lets the shape spring round again and
            // shifts the measured support boundary under a stationary root.
            if support_confirmed {
                let flatten = match phase {
                    "lower_lift" => 0.18 * smooth(progress),
                    "spread_contact_patch" => 0.18 + 0.16 * smooth(progress),
                    _ => 0.34,
                };
                push_field(
                    packet,
                    crate::LocalSomaticField {
                        kind: SomaticFieldKind::Flatten,
                        space: FieldSpace::SurfaceTangentNormal,
                        center: contact_axis * 0.24,
                        axis: contact_axis,
                        radius: 0.68,
                        strength: flatten * tuning.support_gain,
                        falloff: 2.0,
                        frequency_hz: 0.0,
                        phase_01: 1.0,
                        target_component: None,
                    },
                );
            }
            if phase == "micro_adjust" {
                push_field(
                    packet,
                    wave_field(
                        FieldSpace::SurfaceTangentNormal,
                        Vec2::ZERO,
                        support_tangent,
                        0.54,
                        0.10,
                        0.7,
                        progress,
                    ),
                );
            }
            if phase == "rest_hold" {
                // An awake supported gel relaxes into slow circulation and
                // compliance while the measured plane still carries its load.
                packet.material.density_compliance_multiplier = 1.06;
                packet.material.viscosity_multiplier = 0.90;
                packet.material.surface_tension_multiplier = 0.95;
                packet.internal.breath_amplitude_multiplier = 0.76;
                packet.internal.breath_speed_multiplier = 0.72;
                packet.internal.flow_strength_multiplier = 0.72;
                packet.internal.flow_speed_multiplier = 0.64;
                packet.internal.flow_damping = 0.12;
                packet.material.flight_damping_multiplier = 1.18;
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
            let settle = smooth(progress);
            let depth = smooth((context.somatic.supported_seconds / 30.0).clamp(0.0, 1.0));
            let arousal_pulse = if phase == "micro_arousal_or_continue" {
                single_pulse(progress)
            } else {
                0.0
            };
            packet.locomotion.pose = MotorPoseIntent::SupportedSleep;
            packet.locomotion.speed_multiplier = 0.0;
            packet.locomotion.lift_fraction = 0.22;
            if support_confirmed {
                let load = if phase == "sleep_onset" {
                    0.40 + 0.02 * settle
                } else {
                    0.42 + depth * 0.015 - arousal_pulse * 0.01
                };
                packet.support = surface_command(active, load, 0.38, 0.28);
            }
            packet.material.motor_gain_multiplier = 0.58;
            packet.material.flight_damping_multiplier = 1.34;
            packet.material.viscosity_multiplier = 1.18;
            packet.internal.flow_speed_multiplier = 0.36;
            packet.internal.flow_strength_multiplier = 0.46;
            packet.internal.breath_amplitude_multiplier = if phase == "sleep_onset" {
                0.76 - 0.18 * settle
            } else {
                0.58 - depth * 0.04 + arousal_pulse * 0.06
            };
            packet.internal.breath_speed_multiplier = if phase == "sleep_onset" {
                0.49 - 0.05 * settle
            } else {
                0.44 - depth * 0.03 + arousal_pulse * 0.04
            };
            packet.expression.eye_aperture_delta = -0.72;
            // Sleeping load is visible in the body, not only in the eyelids:
            // gravity spreads the lower metaballs into the screen-edge support.
            if support_confirmed {
                push_field(
                    packet,
                    body_field(
                        SomaticFieldKind::Flatten,
                        Vec2::new(active.sampled_style.arc_sign.signum() * 0.035, -0.30),
                        Vec2::Y,
                        0.72,
                        (if phase == "sleep_onset" {
                            0.42 + 0.04 * settle
                        } else {
                            0.46
                        }) * tuning.support_gain,
                        1.0,
                    ),
                );
            }
            if phase == "micro_arousal_or_continue" {
                // A selected sleep check is a small bodily/breath response,
                // not an instruction to open the eyes or wake the pet.
                packet.internal.pulse_amplitude = 0.04 + arousal_pulse * 0.06;
            }
            if phase_started && phase == "sleep_onset" {
                packet.voice.semantic = VoiceSemanticIntent::PhysiologicalBreath;
                packet.voice.emit_once = true;
                packet.voice.intensity = 0.12;
            }
        }
        P::RestRemDreamWake => {
            let pulse = single_pulse(progress);
            let waking = goal.action == lifecore::ActionId::WakeUp
                || context.companion_intent == lifecore::PrimaryIntent::Wake;
            packet.locomotion.pose = MotorPoseIntent::SupportedSleep;
            packet.locomotion.speed_multiplier = 0.0;
            if support_confirmed {
                packet.support = surface_command(active, 0.42 - pulse * 0.012, 0.36, 0.26);
                // Hold the same loaded footprint throughout REM-like phases.
                // Only a genuine wake command progressively unloads it.
                let load_envelope = if phase == "wake_stretch_or_nrem" && waking {
                    1.0 - smooth(progress)
                } else {
                    1.0
                };
                push_field(
                    packet,
                    body_field(
                        SomaticFieldKind::Flatten,
                        Vec2::new(active.sampled_style.arc_sign.signum() * 0.035, -0.30),
                        Vec2::Y,
                        0.72,
                        0.44 * tuning.support_gain * load_envelope,
                        1.0,
                    ),
                );
            }
            packet.material.flight_damping_multiplier = 1.28;
            packet.internal.flow_speed_multiplier = 0.48;
            packet.expression.eye_aperture_delta = -0.72;
            let breathing_accent = match phase {
                "rem_onset" => smooth(progress),
                "structured_local_twitches" => 1.0 + pulse * 0.25,
                "dream_murmur" => 1.0 + pulse * 0.5,
                "micro_arousal" => 1.0 - pulse * 0.25,
                _ => 1.0 - smooth(progress),
            };
            packet.internal.breath_amplitude_multiplier = 0.56 + 0.08 * breathing_accent;
            packet.internal.breath_speed_multiplier = 0.43 + 0.06 * breathing_accent;
            if phase == "structured_local_twitches" && support_confirmed {
                let side = if active.sampled_style.arc_sign >= 0.0 {
                    1.0
                } else {
                    -1.0
                };
                push_field(
                    packet,
                    body_field(
                        SomaticFieldKind::Pulse,
                        Vec2::new(side * 0.24, -0.05),
                        Vec2::new(side, 0.2),
                        0.24,
                        0.07 * pulse,
                        progress,
                    ),
                );
            }
            if phase == "dream_murmur" && phase_started {
                packet.voice.semantic = VoiceSemanticIntent::DreamMurmur;
                packet.voice.emit_once = true;
                packet.voice.intensity = 0.10;
            }
            if phase == "wake_stretch_or_nrem" && waking {
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

/// Exactly one smooth excursion within an explicitly selected phase. It has
/// zero value and zero derivative at either end; no free-running sleep jitter.
fn single_pulse(progress: f32) -> f32 {
    let p = progress.clamp(0.0, 1.0);
    let shape = 4.0 * p * (1.0 - p);
    shape * shape
}

#[cfg(test)]
mod sleep_tests {
    use super::*;
    fn fixture(program: P) -> (BehaviorGoalFrame, BehaviorContextFrame, ActivePerformance) {
        let mut life = lifecore::LifeCore::new(lifecore::Genome::from_seed(42), 42);
        let body_intent = life
            .tick(&Default::default(), &Default::default(), 0.05)
            .body_intent;
        let goal = BehaviorGoalFrame {
            action: lifecore::ActionId::Sleep,
            body_intent,
            affect: Default::default(),
            drives: life.state.drives,
            felt: Default::default(),
            derived: Default::default(),
            attachment: 0.4,
            recent_outcome: None,
        };
        let mut context = BehaviorContextFrame::default();
        context.somatic.supported = true;
        context.screen_edge_supported = true;
        context.companion_intent = lifecore::PrimaryIntent::Sleep;
        context.surfaces.push(crate::SurfaceCandidate {
            surface_id: lifecore::SurfaceId("screen:bottom_edge".into()),
            minimum: Vec2::new(0.0, 0.95),
            maximum: Vec2::ONE,
            velocity: Vec2::ZERO,
            familiarity: 1.0,
            recent_failed_landings: 0,
        });
        let mut runtime = crate::BehaviorPerformanceRuntime::new(42);
        runtime.begin_lab_fixture(program, &goal, &context);
        (goal, context, runtime.active().unwrap().clone())
    }
    fn packet(
        active: &ActivePerformance,
        goal: &BehaviorGoalFrame,
        context: &BehaviorContextFrame,
        phase: &str,
        p: f32,
    ) -> SomaticActuationPacket {
        let mut packet = SomaticActuationPacket::default();
        assert!(apply(
            &mut packet,
            active,
            phase,
            p,
            false,
            goal,
            context,
            Default::default()
        ));
        packet
    }
    #[test]
    fn sleeping_phases_keep_loaded_shape_closed_eyes_and_one_bounded_twitch() {
        let (goal, mut context, active) = fixture(P::RestRemDreamWake);
        for phase in [
            "rem_onset",
            "structured_local_twitches",
            "dream_murmur",
            "micro_arousal",
            "wake_stretch_or_nrem",
        ] {
            for p in [0.0, 0.25, 0.5, 0.75, 1.0] {
                let out = packet(&active, &goal, &context, phase, p);
                assert_eq!(out.locomotion.pose, MotorPoseIntent::SupportedSleep);
                assert!(out.support.is_some());
                assert_eq!(out.expression.eye_aperture_delta, -0.72);
                assert!(
                    out.fields
                        .iter()
                        .flatten()
                        .any(|f| f.kind == SomaticFieldKind::Flatten && f.strength > 0.0)
                );
                for field in out
                    .fields
                    .iter()
                    .flatten()
                    .filter(|f| f.kind == SomaticFieldKind::Pulse)
                {
                    assert!(field.strength <= 0.07001);
                    assert_eq!(field.frequency_hz, 0.0);
                    if p == 0.0 || p == 1.0 {
                        assert_eq!(field.strength, 0.0);
                    }
                }
            }
        }
        context.somatic.supported = false;
        context.screen_edge_supported = false;
        let airborne = packet(&active, &goal, &context, "structured_local_twitches", 0.5);
        assert!(airborne.support.is_none());
        assert!(
            !airborne
                .fields
                .iter()
                .flatten()
                .any(|f| matches!(f.kind, SomaticFieldKind::Flatten | SomaticFieldKind::Pulse))
        );
    }
    #[test]
    fn nrem_breath_deepens_from_contact_dwell_and_real_wake_is_not_blocked() {
        let (mut goal, mut context, active) = fixture(P::RestNremSleep);
        let shallow = packet(&active, &goal, &context, "nrem_hold", 0.0);
        context.somatic.supported_seconds = 30.0;
        let deep = packet(&active, &goal, &context, "nrem_hold", 0.0);
        assert!(deep.internal.breath_speed_multiplier < shallow.internal.breath_speed_multiplier);
        assert!(
            deep.internal.breath_amplitude_multiplier
                < shallow.internal.breath_amplitude_multiplier
        );
        assert_eq!(deep.expression.eye_aperture_delta, -0.72);
        let (_, _, rem) = fixture(P::RestRemDreamWake);
        goal.action = lifecore::ActionId::WakeUp;
        context.companion_intent = lifecore::PrimaryIntent::Wake;
        let wake = packet(&rem, &goal, &context, "wake_stretch_or_nrem", 1.0);
        assert_eq!(wake.locomotion.pose, MotorPoseIntent::Recover);
        assert_eq!(wake.expression.eye_aperture_delta, 0.0);
    }

    #[test]
    fn awake_settle_shape_follows_the_measured_support_normal_and_tangent() {
        let (goal, mut context, mut active) = fixture(P::RestSitSettle);
        context.surfaces.clear();
        context.somatic.supported = true;
        active.locked_target = Some(crate::BehaviorTarget::Surface(crate::SurfaceTarget {
            surface_id: lifecore::SurfaceId("screen:left_edge".into()),
            anchor_point: Vec2::new(0.0, 0.5),
            normal: Vec2::X,
            tangent: Vec2::Y,
            center_clearance: 0.038,
            score: 1.0,
        }));
        let spread = packet(&active, &goal, &context, "spread_contact_patch", 1.0);
        let flatten = spread
            .fields
            .iter()
            .flatten()
            .find(|field| field.kind == SomaticFieldKind::Flatten)
            .expect("support-oriented flatten");
        assert_eq!(flatten.space, FieldSpace::SurfaceTangentNormal);
        assert!(flatten.axis.dot(Vec2::NEG_X) > 0.99);
        let adjust = packet(&active, &goal, &context, "micro_adjust", 0.5);
        let wave = adjust
            .fields
            .iter()
            .flatten()
            .find(|field| field.kind == SomaticFieldKind::Wave)
            .expect("tangent micro-adjustment");
        assert!(wave.axis.dot(Vec2::Y) > 0.99);
    }
}
