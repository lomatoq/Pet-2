use super::*;

#[test]
fn actual_v63_first_unstable_tick_density_stage() {
    let incident: serde_json::Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/v63-first-unstable-material.json"
    ))
    .unwrap();
    let profile: crate::LiquidTuningProfile =
        serde_json::from_value(incident["profile"].clone()).unwrap();
    let snapshot: BodyMaterialSnapshot =
        serde_json::from_value(incident["body_snapshot"].clone()).unwrap();
    let packet: SomaticActuationPacket =
        serde_json::from_value(incident["somatic"].clone()).unwrap();
    let genome = lifecore::Genome::from_seed(profile.seed);
    let mut body = crate::ProceduralBody::generate(&genome).unwrap();
    body.apply_tuning_profile(profile.clone()).unwrap();
    body.restore_body_material_snapshot(&snapshot).unwrap();
    let runtime = &mut body.embodiment.liquid;
    let dt = 1.0 / 120.0;
    for p in &mut runtime.particles[..runtime.particle_count] {
        p.predicted_position = p.position + p.velocity * dt;
    }
    let input_peak = runtime.particles[..runtime.particle_count]
        .iter()
        .map(|p| p.velocity.length())
        .fold(0.0_f32, f32::max);
    let before = runtime.particles;
    for iterations in 1..=6 {
        let mut candidate = before;
        solve_density_constraints(
            &mut candidate,
            runtime.particle_count,
            DensityConstraintParameters {
                rest_density: runtime.rest_density,
                kernel_radius: KERNEL_RADIUS * profile.pbf.kernel_radius_scale,
                compliance: profile.pbf.density_compliance
                    * packet.material.density_compliance_multiplier,
                scorr_k: 0.0,
                scorr_q_ratio: profile.pbf.scorr_q_ratio,
                scorr_power: profile.pbf.scorr_power,
                iterations,
                dt,
                containment_bounds: None,
                support_plane: None,
                cradle: None,
            },
        );
        let speed = candidate[..runtime.particle_count]
            .iter()
            .map(|p| (p.predicted_position - p.position).length() / dt)
            .fold(0.0_f32, f32::max);
        let compression = candidate[..runtime.particle_count]
            .iter()
            .map(|p| (p.density / runtime.rest_density - 1.0).max(0.0))
            .fold(0.0_f32, f32::max);
        eprintln!("iterations={iterations}, speed={speed}, maxcompression={compression}");
    }
    solve_density_constraints(
        &mut runtime.particles,
        runtime.particle_count,
        DensityConstraintParameters {
            rest_density: runtime.rest_density,
            kernel_radius: KERNEL_RADIUS * profile.pbf.kernel_radius_scale,
            compliance: profile.pbf.density_compliance
                * packet.material.density_compliance_multiplier,
            scorr_k: 0.0,
            scorr_q_ratio: profile.pbf.scorr_q_ratio,
            scorr_power: profile.pbf.scorr_power,
            iterations: profile.pbf.density_iterations,
            dt,
            containment_bounds: None,
            support_plane: None,
            cradle: None,
        },
    );
    let peak = runtime.particles[..runtime.particle_count]
        .iter()
        .map(|p| (p.predicted_position - p.position).length() / dt)
        .fold(0.0_f32, f32::max);
    let correction = runtime.particles[..runtime.particle_count]
        .iter()
        .zip(before)
        .map(|(p, b)| p.predicted_position.distance(b.predicted_position))
        .fold(0.0_f32, f32::max);
    eprintln!(
        "ACTUAL density replay: input_peak={input_peak}, output_peak={peak}, max_correction={correction}, rest_density={}",
        runtime.rest_density
    );
    assert!(
        peak < input_peak + 0.1,
        "density solve amplified the captured launch: {input_peak} -> {peak}"
    );
    assert!(
        correction < 0.1,
        "one pressure step crossed multiple kernel neighborhoods: {correction}"
    );
    let before_center = before[..runtime.particle_count]
        .iter()
        .map(|p| p.predicted_position)
        .sum::<Vec2>();
    let after_center = runtime.particles[..runtime.particle_count]
        .iter()
        .map(|p| p.predicted_position)
        .sum::<Vec2>();
    assert!(
        before_center.distance(after_center) < 1.0e-4,
        "pressure relaxation changed momentum"
    );
    assert!(
        runtime.particles[..runtime.particle_count]
            .iter()
            .all(|p| p.density <= runtime.rest_density * 1.01)
    );
}

#[test]
fn actual_v63_pre_failsafe_material_recovers_without_pressure_scatter() {
    let incident: serde_json::Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/v63-first-unstable-material.json"
    ))
    .unwrap();
    let profile: crate::LiquidTuningProfile =
        serde_json::from_value(incident["profile"].clone()).unwrap();
    let snapshot: BodyMaterialSnapshot =
        serde_json::from_value(incident["body_snapshot"].clone()).unwrap();
    let packet: SomaticActuationPacket =
        serde_json::from_value(incident["somatic"].clone()).unwrap();
    let genome = lifecore::Genome::from_seed(profile.seed);
    let mut body = crate::ProceduralBody::generate(&genome).unwrap();
    body.apply_tuning_profile(profile).unwrap();
    body.restore_body_material_snapshot(&snapshot).unwrap();
    body.set_somatic_actuation(packet.clone());
    let mut phenotype = lifecore::FastPhenotypeActuation::default();
    pet_motor::SomaticActuationBus::compose(&mut phenotype, &packet);
    body.set_fast_phenotype_actuation(phenotype);
    let runtime = &mut body.embodiment.liquid;
    let mut intent = lifecore::BodyIntent {
        locomotion: lifecore::LocomotionMode::Hover,
        target_position: Vec2::splat(0.5),
        target_surface: None,
        desired_speed: 0.0,
        facing_direction: 1.0,
        gaze_target: None,
        pose: lifecore::PoseIntent::Compact,
        expression: Default::default(),
        interaction_target: None,
    };
    let mut peak_speed = 0.0_f32;
    let mut min_main = 96.0_f32;
    // The captured first frame is already inside a transient launch. Replaying
    // that packet for five seconds invents sustained actuation; use the actual
    // subsequent Brake/Orient/Recover evidence instead.
    let trace = incident["recovery_trace"].as_array().unwrap();
    let mut next_phase = 0;
    for tick in 0..600 {
        let elapsed = tick as f64 / 120.0;
        while next_phase < trace.len()
            && elapsed >= trace[next_phase]["offset_seconds"].as_f64().unwrap()
        {
            let recorded = &trace[next_phase];
            let packet: SomaticActuationPacket =
                serde_json::from_value(recorded["somatic"].clone()).unwrap();
            intent.pose = serde_json::from_value(recorded["pose"].clone()).unwrap();
            runtime.set_somatic_actuation(packet.clone());
            let mut phenotype = lifecore::FastPhenotypeActuation::default();
            pet_motor::SomaticActuationBus::compose(&mut phenotype, &packet);
            runtime.set_runtime_actuation(phenotype.pbf);
            next_phase += 1;
        }
        runtime.update_with_tilt(
            &genome.body,
            &DerivedVisualTraits::from_genome(&genome),
            VisualPhysiologyPose::default(),
            VisualMindInput::default(),
            &intent,
            &SensorFrame::default(),
            &BodyFeedback::default(),
            DropletMotion {
                world_to_body_scale: Vec2::ONE,
                ..Default::default()
            },
            ModalDeformation::default(),
            0.5,
            0.0,
            1.0 / 120.0,
        );
        let d = runtime.diagnostics();
        peak_speed = peak_speed.max(d.maximum_speed);
        if d.main_mass < min_main {
            eprintln!(
                "V63 reduced mass tick={tick} d={d:?} guard={:?}",
                runtime.topology_decision
            );
        }
        min_main = min_main.min(d.main_mass);
        assert!(d.finite, "nonfinite at{tick}");
        assert_eq!(d.failsafe_hits, 0, "emergency speed projection at{tick}");
        assert_eq!(d.recovery_count, 0);
    }
    eprintln!(
        "ACTUAL full material replay: peak_speed={peak_speed}, min_main={min_main}, final_speed={}",
        runtime.diagnostics.maximum_speed
    );
    assert!(peak_speed < 8.0, "numerical scatter: {peak_speed}");
    assert!(
        min_main >= 96.0 * 0.82,
        "exceeded real detachment budget: {min_main}"
    );
}

#[test]
fn actual_v64_remerge_next_tick_constraint_stage() {
    let incident: serde_json::Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/v64-pre-remerge-scatter.json"
    ))
    .unwrap();
    let profile: crate::LiquidTuningProfile =
        serde_json::from_value(incident["profile"].clone()).unwrap();
    let snapshot: BodyMaterialSnapshot =
        serde_json::from_value(incident["body_snapshot"].clone()).unwrap();
    let packet: SomaticActuationPacket =
        serde_json::from_value(incident["somatic"].clone()).unwrap();
    let genome = lifecore::Genome::from_seed(profile.seed);
    let mut body = crate::ProceduralBody::generate(&genome).unwrap();
    body.apply_tuning_profile(profile.clone()).unwrap();
    body.restore_body_material_snapshot(&snapshot).unwrap();
    let runtime = &mut body.embodiment.liquid;
    let dt = 1.0 / 120.0;
    for p in &mut runtime.particles[..runtime.particle_count] {
        p.predicted_position = p.position + p.velocity * dt;
    }
    let stage_peak = |particles: &[LiquidParticle]| {
        particles
            .iter()
            .map(|p| (p.predicted_position - p.position).length() / dt)
            .fold(0.0_f32, f32::max)
    };
    let input_peak = stage_peak(&runtime.particles[..runtime.particle_count]);
    solve_density_constraints(
        &mut runtime.particles,
        runtime.particle_count,
        DensityConstraintParameters {
            rest_density: runtime.rest_density,
            kernel_radius: KERNEL_RADIUS * profile.pbf.kernel_radius_scale,
            compliance: profile.pbf.density_compliance
                * packet.material.density_compliance_multiplier,
            scorr_k: 0.0,
            scorr_q_ratio: profile.pbf.scorr_q_ratio,
            scorr_power: profile.pbf.scorr_power,
            iterations: profile.pbf.density_iterations,
            dt,
            containment_bounds: None,
            support_plane: None,
            cradle: None,
        },
    );
    let density_peak = stage_peak(&runtime.particles[..runtime.particle_count]);
    let spacing = PARTICLE_SPACING * profile.pbf.spacing_scale;
    let link = component_graph_spacing(
        spacing,
        KERNEL_RADIUS * profile.pbf.kernel_radius_scale,
        profile.pbf.iso_threshold,
        runtime.cinematic_features,
    ) * profile.pbf.component_link_radius_scale;
    let before_repair = runtime.particles;
    let input_components = runtime
        .topology_guard
        .observe(
            &runtime.particles,
            runtime.particle_count,
            link,
            profile.interaction,
        )
        .predicted_component_count;
    let decision = runtime.topology_guard.enforce(
        &mut runtime.particles,
        runtime.particle_count,
        link,
        profile.interaction,
    );
    let topology_peak = stage_peak(&runtime.particles[..runtime.particle_count]);
    eprintln!(
        "ACTUAL V64 constraints: input={input_peak}, density={density_peak}, topology={topology_peak}, decision={decision:?}, link={link}"
    );
    assert!(
        topology_peak < density_peak + 1.0e-4,
        "constraint repair injected kinetic scatter"
    );
    assert!(
        decision.predicted_component_count <= input_components,
        "repair created more fragments"
    );
    let displacement_momentum = runtime.particles[..runtime.particle_count]
        .iter()
        .zip(&before_repair[..runtime.particle_count])
        .map(|(p, before)| (p.position - before.position) / p.inverse_mass)
        .sum::<Vec2>();
    assert!(displacement_momentum.length() < 1.0e-4);
}

#[test]
fn actual_v64_supported_sleep_to_guard_stays_coherent() {
    let incident: serde_json::Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/v64-supported-sleep-before-guard.json"
    ))
    .unwrap();
    let profile: crate::LiquidTuningProfile =
        serde_json::from_value(incident["profile"].clone()).unwrap();
    let snapshot: BodyMaterialSnapshot =
        serde_json::from_value(incident["body_snapshot"].clone()).unwrap();
    let rest: SomaticActuationPacket = serde_json::from_value(incident["somatic"].clone()).unwrap();
    let guard: SomaticActuationPacket =
        serde_json::from_value(incident["guard"]["somatic"].clone()).unwrap();
    let genome = lifecore::Genome::from_seed(profile.seed);
    let mut body = crate::ProceduralBody::generate(&genome).unwrap();
    body.apply_tuning_profile(profile).unwrap();
    body.restore_body_material_snapshot(&snapshot).unwrap();
    let mut intent = lifecore::BodyIntent {
        locomotion: lifecore::LocomotionMode::Sleep,
        target_position: rest.locomotion.target_position.unwrap(),
        target_surface: None,
        desired_speed: 0.0,
        facing_direction: 1.0,
        gaze_target: None,
        pose: lifecore::PoseIntent::Landing,
        expression: Default::default(),
        interaction_target: None,
    };
    let feedback = BodyFeedback {
        world_position: intent.target_position,
        ..Default::default()
    };
    let runtime = &mut body.embodiment.liquid;
    let mut peak = 0.0_f32;
    let mut min_main = 96.0_f32;
    for tick in 0..1200 {
        let packet = if tick < 120 { &rest } else { &guard };
        if tick == 120 {
            intent.pose = lifecore::PoseIntent::Compact;
        }
        runtime.set_somatic_actuation(packet.clone());
        let mut phenotype = lifecore::FastPhenotypeActuation::default();
        pet_motor::SomaticActuationBus::compose(&mut phenotype, packet);
        runtime.set_runtime_actuation(phenotype.pbf);
        runtime.update_with_tilt(
            &genome.body,
            &DerivedVisualTraits::from_genome(&genome),
            VisualPhysiologyPose::default(),
            VisualMindInput::default(),
            &intent,
            &SensorFrame::default(),
            &feedback,
            DropletMotion {
                world_to_body_scale: Vec2::new(15.16463, -6.347985),
                ..Default::default()
            },
            ModalDeformation::default(),
            0.5,
            0.0,
            1.0 / 120.0,
        );
        let d = runtime.diagnostics();
        peak = peak.max(d.maximum_speed);
        min_main = min_main.min(d.main_mass);
        assert!(d.finite);
        assert_eq!(
            d.failsafe_hits, 0,
            "guard generated emergency energy at {tick}"
        );
        assert_eq!(d.recovery_count, 0);
    }
    eprintln!(
        "ACTUAL supported state / guard counterfactual: peak={peak}, min_main={min_main}, final={:?}",
        runtime.diagnostics()
    );
    assert_eq!(min_main, 96.0, "quiet supported guard fragmented");
}

#[test]
fn actual_v64_fragment_remerge_does_not_reignite_scatter() {
    let incident: serde_json::Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/v64-pre-remerge-scatter.json"
    ))
    .unwrap();
    let profile: crate::LiquidTuningProfile =
        serde_json::from_value(incident["profile"].clone()).unwrap();
    let snapshot: BodyMaterialSnapshot =
        serde_json::from_value(incident["body_snapshot"].clone()).unwrap();
    let packet: SomaticActuationPacket =
        serde_json::from_value(incident["somatic"].clone()).unwrap();
    let genome = lifecore::Genome::from_seed(profile.seed);
    let mut body = crate::ProceduralBody::generate(&genome).unwrap();
    body.apply_tuning_profile(profile).unwrap();
    body.restore_body_material_snapshot(&snapshot).unwrap();
    let runtime = &mut body.embodiment.liquid;
    runtime.set_somatic_actuation(packet.clone());
    let mut phenotype = lifecore::FastPhenotypeActuation::default();
    pet_motor::SomaticActuationBus::compose(&mut phenotype, &packet);
    runtime.set_runtime_actuation(phenotype.pbf);
    let intent = lifecore::BodyIntent {
        locomotion: lifecore::LocomotionMode::Hover,
        target_position: Vec2::splat(0.5),
        target_surface: None,
        desired_speed: 0.0,
        facing_direction: 1.0,
        gaze_target: None,
        pose: lifecore::PoseIntent::Compact,
        expression: Default::default(),
        interaction_target: None,
    };
    let mut peak = 0.0_f32;
    let mut min_main = 96.0_f32;
    for tick in 0..1200 {
        runtime.update_with_tilt(
            &genome.body,
            &DerivedVisualTraits::from_genome(&genome),
            VisualPhysiologyPose::default(),
            VisualMindInput::default(),
            &intent,
            &SensorFrame::default(),
            &BodyFeedback::default(),
            DropletMotion {
                world_to_body_scale: Vec2::new(15.16463, -6.347985),
                ..Default::default()
            },
            ModalDeformation::default(),
            0.5,
            0.0,
            1.0 / 120.0,
        );
        let d = runtime.diagnostics();
        peak = peak.max(d.maximum_speed);
        min_main = min_main.min(d.main_mass);
        assert!(d.finite);
        assert_eq!(
            d.failsafe_hits, 0,
            "captured remerge injected emergency energy at {tick}"
        );
        assert_eq!(d.recovery_count, 0);
    }
    let final_state = runtime.diagnostics();
    eprintln!(
        "ACTUAL remerge material counterfactual: peak={peak}, min_main={min_main}, final={final_state:?}"
    );
    assert!(peak < 8.0);
    assert_eq!(
        final_state.main_mass, 96.0,
        "all captured real mass must return"
    );
    assert_eq!(final_state.component_count, 1);
}

#[test]
fn captured_supported_material_survives_bounded_frustration_discharge() {
    let incident: serde_json::Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/v64-supported-sleep-before-guard.json"
    ))
    .unwrap();
    let profile: crate::LiquidTuningProfile =
        serde_json::from_value(incident["profile"].clone()).unwrap();
    let snapshot: BodyMaterialSnapshot =
        serde_json::from_value(incident["body_snapshot"].clone()).unwrap();
    let rest: SomaticActuationPacket = serde_json::from_value(incident["somatic"].clone()).unwrap();
    let genome = lifecore::Genome::from_seed(profile.seed);
    let origin = rest.locomotion.target_position.unwrap();
    let floor = rest.support.as_ref().unwrap().anchor_point.y;
    let screen = Vec2::new(3440.0, 1440.0);
    let scale = Vec2::new(15.16463, -6.347985);
    let dt = 1.0 / 120.0;
    // Exercise the actual EmotionBurst excursion/speed envelope in both an
    // unobstructed direction and the captured nearest desktop wall direction.
    // This is a physical counterfactual, not a reconstructed lost live trace.
    for direction in [-Vec2::Y, Vec2::Y] {
        let mut body = crate::ProceduralBody::generate(&genome).unwrap();
        body.apply_tuning_profile(profile.clone()).unwrap();
        body.restore_body_material_snapshot(&snapshot).unwrap();
        body.simulation.set_motion_space_pixels(screen);
        body.set_render_aspect(screen.x / screen.y);
        body.embodiment.set_world_to_body_scale(scale);
        body.simulation.feedback.world_position = origin;
        let mut peak = 0.0_f32;
        let mut min_main = 96.0_f32;
        for tick in 0..1200 {
            let t = tick as f32 * dt;
            let supported = t < 1.0;
            let phase = ((t - 1.0) / 1.25).clamp(0.0, 1.0);
            let excursion = (std::f32::consts::PI * phase).sin().powi(2);
            let packet = if supported {
                rest.clone()
            } else {
                SomaticActuationPacket::default()
            };
            body.set_somatic_actuation(packet.clone());
            let mut phenotype = lifecore::FastPhenotypeActuation::default();
            pet_motor::SomaticActuationBus::compose(&mut phenotype, &packet);
            body.set_fast_phenotype_actuation(phenotype);
            let intent = lifecore::BodyIntent {
                locomotion: if supported {
                    lifecore::LocomotionMode::Sleep
                } else {
                    lifecore::LocomotionMode::Seek
                },
                target_position: (origin + direction * (0.045 * excursion))
                    .clamp(Vec2::splat(0.02), Vec2::splat(0.98)),
                target_surface: None,
                desired_speed: if supported || t >= 2.25 {
                    0.0
                } else {
                    0.20 + 0.30 * excursion
                },
                facing_direction: 1.0,
                gaze_target: None,
                pose: if supported {
                    lifecore::PoseIntent::Landing
                } else {
                    lifecore::PoseIntent::Compact
                },
                expression: lifecore::ExpressionState {
                    brow_tension: 0.7,
                    eye_aperture: 0.75,
                    ..Default::default()
                },
                interaction_target: None,
            };
            let previous_velocity = body.simulation.feedback.velocity;
            body.fixed_update(&genome, &intent, &SensorFrame::default(), dt);
            if supported {
                body.simulation.feedback.world_position = origin;
                body.simulation.feedback.velocity = Vec2::ZERO;
            } else {
                let maximum_root =
                    floor - body.liquid_contact_bounds_pixels(1.0).maximum.y / screen.y;
                if body.simulation.feedback.world_position.y > maximum_root {
                    body.simulation.feedback.world_position.y = maximum_root;
                    if body.simulation.feedback.velocity.y > 0.0 {
                        body.simulation.feedback.velocity.y *= -0.12;
                    }
                    body.simulation.feedback.acceleration =
                        (body.simulation.feedback.velocity - previous_velocity) / dt;
                }
            }
            body.embodiment.set_world_to_body_scale(scale);
            body.embodied_update(
                &intent,
                &SensorFrame::default(),
                lifecore::AffectState::default(),
                VisualMindInput::default(),
                crate::VoiceVisualState::default(),
                dt,
            );
            body.presentation_update(dt);
            let d = body.embodiment.liquid.diagnostics();
            peak = peak.max(d.maximum_speed);
            min_main = min_main.min(d.main_mass);
            assert!(d.finite);
            assert_eq!(d.failsafe_hits, 0, "discharge {direction:?} at {tick}");
            assert_eq!(d.recovery_count, 0);
        }
        eprintln!(
            "Bounded frustration discharge {direction:?}: peak={peak}, min_main={min_main}, final={:?}",
            body.embodiment.liquid.diagnostics()
        );
        assert_eq!(
            min_main, 96.0,
            "bounded discharge scattered captured coherent mass"
        );
    }
}
