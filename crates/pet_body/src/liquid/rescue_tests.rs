use glam::Vec2;
use lifecore::{BodyIntent, Genome, InteractionBodyActuation, LocomotionMode, PoseIntent};

use super::*;

const DT: f32 = 1.0 / 120.0;

fn neutral_intent() -> BodyIntent {
    BodyIntent {
        locomotion: LocomotionMode::Hover,
        target_position: Vec2::splat(0.5),
        target_surface: None,
        desired_speed: 0.0,
        facing_direction: 1.0,
        gaze_target: None,
        pose: PoseIntent::Neutral,
        expression: lifecore::ExpressionState::default(),
        interaction_target: None,
    }
}

fn migrated_runtime(seed: u64) -> (Genome, LiquidMorphRuntime) {
    let genome = Genome::from_seed(seed);
    // Reproduce the physical portion of the user's actual saved v15 profile,
    // then exercise the same schema migration used by Pet and Body Lab.
    let mut profile = crate::LiquidTuningProfile::for_seed(seed);
    profile.schema_version = 15;
    profile.pbf.substeps = 1;
    profile.pbf.impact_substeps = 2;
    profile.pbf.density_iterations = 4;
    profile.pbf.impact_density_iterations = 6;
    profile.pbf.density_compliance = 8.0e-6;
    profile.pbf.scorr_k = 0.005;
    profile.pbf.viscosity = 0.0;
    profile.pbf.surface_tension = 0.62;
    profile.pbf.shape_recovery = 10.0;
    profile.pbf.spacing_scale = 0.88;
    profile.pbf.kernel_radius_scale = 1.14;
    profile.pbf.flight_inertia = 0.90;
    profile.pbf.flight_damping = 4.2;
    profile.pbf.upright_stabilization = 14.0;
    profile.pbf.grab_stiffness = 240.0;
    profile.pbf.anisotropy_max = 2.15;
    profile.pbf.return_strength = 0.34;
    profile.pbf.idle_fragment_size = 0.05;
    profile.pbf.idle_bud_pull_strength = 3.2;
    profile.pbf.pinch_bounce = 1.0;
    let profile = profile
        .sanitized()
        .expect("v15 fixture must migrate to the current schema");
    assert_eq!(profile.schema_version, crate::LIQUID_TUNING_SCHEMA_VERSION);
    assert_eq!(profile.pbf.fixed_hz, 120.0);
    assert_eq!(profile.pbf.substeps, 1);
    assert_eq!(profile.pbf.density_iterations, 6);
    assert_eq!(profile.pbf.viscosity, 0.0);
    assert_eq!(profile.pbf.shape_recovery, 0.0);
    assert_eq!(profile.pbf.idle_fragment_size, 0.0);
    let mut runtime = LiquidMorphRuntime::new(seed);
    runtime.set_tuning(
        profile.pbf,
        profile.interaction,
        profile.face,
        profile.material.variant,
    );
    (genome, runtime)
}

fn step(
    runtime: &mut LiquidMorphRuntime,
    genome: &Genome,
    sensors: &SensorFrame,
    feedback: &BodyFeedback,
    motion: DropletMotion,
) {
    runtime.update_with_tilt(
        &genome.body,
        &DerivedVisualTraits::from_genome(genome),
        VisualPhysiologyPose::default(),
        VisualMindInput::default(),
        &neutral_intent(),
        sensors,
        feedback,
        motion,
        ModalDeformation::default(),
        0.5,
        0.0,
        DT,
    );
}

fn idle_step(runtime: &mut LiquidMorphRuntime, genome: &Genome) {
    step(
        runtime,
        genome,
        &SensorFrame::default(),
        &BodyFeedback::default(),
        DropletMotion {
            world_to_body_scale: Vec2::ONE,
            ..DropletMotion::default()
        },
    );
}

fn settle(runtime: &mut LiquidMorphRuntime, genome: &Genome, seconds: f32) {
    for _ in 0..(seconds * 120.0) as usize {
        idle_step(runtime, genome);
    }
}

fn particle_center(runtime: &LiquidMorphRuntime) -> Vec2 {
    runtime.particles[..runtime.particle_count]
        .iter()
        .map(|particle| particle.position)
        .sum::<Vec2>()
        / runtime.particle_count as f32
}

fn rms_speed(runtime: &LiquidMorphRuntime) -> f32 {
    (runtime.particles[..runtime.particle_count]
        .iter()
        .map(|particle| particle.velocity.length_squared())
        .sum::<f32>()
        / runtime.particle_count as f32)
        .sqrt()
}

fn pointer_sensor(local: Vec2, held: bool) -> SensorFrame {
    SensorFrame {
        cursor_position: Vec2::splat(0.5) + local,
        pointer_down: held,
        pet_hovered: held,
        pet_touched: held,
        pet_dragged: held,
        ..SensorFrame::default()
    }
}

fn rigid_motion_explained(before: &[Vec2], after: &[Vec2]) -> f32 {
    let count = before.len().min(after.len()).max(1) as f32;
    let before_center = before.iter().copied().sum::<Vec2>() / count;
    let after_center = after.iter().copied().sum::<Vec2>() / count;
    let mut dot = 0.0;
    let mut cross = 0.0;
    for (&from, &to) in before.iter().zip(after) {
        let from = from - before_center;
        let to = to - after_center;
        dot += from.dot(to);
        cross += from.perp_dot(to);
    }
    let angle = cross.atan2(dot);
    let rotation = Vec2::from_angle(angle);
    let mut residual = 0.0;
    let mut total_motion = 0.0;
    for (&from, &to) in before.iter().zip(after) {
        let predicted = after_center + rotation.rotate(from - before_center);
        residual += to.distance_squared(predicted);
        total_motion += to.distance_squared(from);
    }
    (1.0 - residual / total_motion.max(1.0e-8)).clamp(0.0, 1.0)
}

fn rigid_velocity_explained(runtime: &LiquidMorphRuntime) -> Option<f32> {
    let center = particle_center(runtime);
    let mean_velocity = runtime.particles[..runtime.particle_count]
        .iter()
        .map(|particle| particle.velocity)
        .sum::<Vec2>()
        / runtime.particle_count as f32;
    let mut angular_numerator = 0.0_f32;
    let mut angular_denominator = 0.0_f32;
    let mut total_energy = 0.0_f32;
    for particle in &runtime.particles[..runtime.particle_count] {
        let local = particle.position - center;
        angular_numerator += local.perp_dot(particle.velocity - mean_velocity);
        angular_denominator += local.length_squared();
        total_energy += particle.velocity.length_squared();
    }
    if total_energy / runtime.particle_count as f32 <= 0.05_f32.powi(2) {
        return None;
    }
    let angular_velocity = angular_numerator / angular_denominator.max(1.0e-8);
    let residual = runtime.particles[..runtime.particle_count]
        .iter()
        .map(|particle| {
            let local = particle.position - center;
            let rigid_velocity = mean_velocity + Vec2::new(-local.y, local.x) * angular_velocity;
            particle.velocity.distance_squared(rigid_velocity)
        })
        .sum::<f32>();
    Some((1.0 - residual / total_energy.max(1.0e-8)).clamp(0.0, 1.0))
}

fn triangle_wave(cycles: f32) -> f32 {
    1.0 - 4.0 * (cycles.fract() - 0.5).abs()
}

fn topology_signature(runtime: &LiquidMorphRuntime) -> Vec<usize> {
    let mut component_sizes = [0_usize; u8::MAX as usize + 1];
    for particle in &runtime.particles[..runtime.particle_count] {
        component_sizes[usize::from(particle.component_id)] += 1;
    }
    let mut signature: Vec<usize> = component_sizes
        .into_iter()
        .filter(|size| *size > 0)
        .collect();
    signature.sort_unstable();
    signature
}

fn bidirectional_particle_set_distance(
    first: &LiquidMorphRuntime,
    second: &LiquidMorphRuntime,
) -> f32 {
    fn directed(first: &LiquidMorphRuntime, second: &LiquidMorphRuntime) -> f32 {
        first.particles[..first.particle_count]
            .iter()
            .map(|particle| {
                second.particles[..second.particle_count]
                    .iter()
                    .map(|other| particle.position.distance(other.position))
                    .fold(f32::INFINITY, f32::min)
            })
            .fold(0.0_f32, f32::max)
    }
    directed(first, second).max(directed(second, first))
}

#[test]
fn idle_and_sparse_reconstruction_obey_the_new_contract() {
    for seed in 0..3_u64 {
        let (genome, mut runtime) = migrated_runtime(0x5100 + seed);
        settle(&mut runtime, &genome, 8.0);
        let diagnostics = runtime.diagnostics();
        assert!(diagnostics.finite, "seed={seed} {diagnostics:?}");
        assert_eq!(
            diagnostics.component_count, 1,
            "seed={seed} {diagnostics:?}"
        );
        assert_eq!(diagnostics.failsafe_hits, 0, "seed={seed} {diagnostics:?}");
        assert_eq!(diagnostics.recovery_count, 0, "seed={seed} {diagnostics:?}");
        assert_eq!(diagnostics.bubble_count, 0);
        assert_eq!(diagnostics.budding_count, 0);
    }

    let (_, mut runtime) = migrated_runtime(0x5A4E);
    runtime.particle_count = 5;
    for index in 0..runtime.particle_count {
        runtime.particles[index].position = Vec2::new(index as f32 * 0.035, 0.0);
        runtime.particles[index].render_axis_major = Vec2::Y;
        runtime.particles[index].velocity = Vec2::new(index as f32 * 50.0, -70.0);
    }
    let with_velocity = runtime.anisotropy_target(2);
    for particle in &mut runtime.particles[..runtime.particle_count] {
        particle.velocity = Vec2::ZERO;
    }
    let without_velocity = runtime.anisotropy_target(2);
    assert_eq!(with_velocity, without_velocity);
    assert_eq!(with_velocity.2, 1.0, "sparse splat became a capsule");
}

#[test]
fn stationary_pointer_field_settles_without_jitter_or_step_spikes() {
    let (genome, mut runtime) = migrated_runtime(0x501D);
    settle(&mut runtime, &genome, 2.0);
    let feedback = BodyFeedback::default();
    let sensors = pointer_sensor(Vec2::new(0.28, 0.02), true);
    let motion = DropletMotion {
        world_to_body_scale: Vec2::ONE,
        ..DropletMotion::default()
    };
    let kernel_radius = KERNEL_RADIUS * runtime.tuning.kernel_radius_scale;
    let mut maximum_step = 0.0_f32;
    let mut initial_rms = 0.0_f32;
    let mut initial_samples = 0.0_f32;
    let mut tail_rms = 0.0_f32;
    let mut tail_samples = 0.0_f32;
    for tick in 0..1_200 {
        let before: Vec<Vec2> = runtime.particles[..runtime.particle_count]
            .iter()
            .map(|particle| particle.position)
            .collect();
        step(&mut runtime, &genome, &sensors, &feedback, motion);
        for (particle, previous) in runtime.particles[..runtime.particle_count]
            .iter()
            .zip(before)
        {
            maximum_step = maximum_step.max(particle.position.distance(previous));
        }
        if (60..180).contains(&tick) {
            initial_rms += rms_speed(&runtime);
            initial_samples += 1.0;
        } else if tick >= 1_080 {
            tail_rms += rms_speed(&runtime);
            tail_samples += 1.0;
        }
    }
    initial_rms /= initial_samples;
    tail_rms /= tail_samples;
    assert!(
        tail_rms < initial_rms * 0.75 && tail_rms < 0.06,
        "pointer hold did not attenuate: initial rms={initial_rms}, tail rms={tail_rms}"
    );
    assert!(maximum_step < kernel_radius * 0.10, "step={maximum_step}");
    assert_eq!(runtime.diagnostics().failsafe_hits, 0);
    assert_eq!(runtime.diagnostics().recovery_count, 0);
}

#[test]
fn slow_stretch_raises_strain_without_matching_flick_kinematics() {
    let (genome, mut runtime) = migrated_runtime(0x10CA1);
    settle(&mut runtime, &genome, 2.0);
    let before: Vec<Vec2> = runtime.particles[..runtime.particle_count]
        .iter()
        .map(|particle| particle.position)
        .collect();
    let feedback = BodyFeedback::default();
    let mut peak_strain = 0.0_f32;
    let mut peak_pointer_speed = 0.0_f32;
    for tick in 0..300 {
        let t = tick as f32 / 299.0;
        let local = Vec2::new(
            0.24 + t * 0.32,
            0.02 + (t * std::f32::consts::PI).sin() * 0.015,
        );
        step(
            &mut runtime,
            &genome,
            &pointer_sensor(local, true),
            &feedback,
            DropletMotion {
                world_to_body_scale: Vec2::ONE,
                ..DropletMotion::default()
            },
        );
        let physical = runtime.embodied_interaction_frame();
        peak_strain = peak_strain.max(physical.material.maximum_strain);
        peak_pointer_speed = peak_pointer_speed.max(physical.contact.pointer_speed);
    }
    let after: Vec<Vec2> = runtime.particles[..runtime.particle_count]
        .iter()
        .map(|particle| particle.position)
        .collect();
    let mut ranked: Vec<usize> = (0..before.len()).collect();
    ranked.sort_by(|&a, &b| before[b].x.total_cmp(&before[a].x));
    let quartile = before.len() / 4;
    let near = ranked[..quartile]
        .iter()
        .map(|&index| after[index].distance(before[index]))
        .sum::<f32>()
        / quartile as f32;
    let far = ranked[ranked.len() - quartile..]
        .iter()
        .map(|&index| after[index].distance(before[index]))
        .sum::<f32>()
        / quartile as f32;
    let rigid = rigid_motion_explained(&before, &after);
    assert!(near >= far * 2.0, "near={near} far={far} rigid={rigid}");
    assert!(rigid < 0.85, "rigid fit explained {rigid:.3}");
    assert!(peak_strain >= 0.10, "peak strain={peak_strain}");
    assert!(
        peak_pointer_speed < 1.0,
        "slow stretch reported flick-like speed={peak_pointer_speed}"
    );
    assert_eq!(runtime.diagnostics().failsafe_hits, 0);
    assert_eq!(runtime.diagnostics().recovery_count, 0);
}

#[test]
fn sharp_flick_raises_release_speed_and_slosh_without_fake_hold() {
    let (genome, mut runtime) = migrated_runtime(0xF11C);
    settle(&mut runtime, &genome, 2.0);
    let feedback = BodyFeedback::default();
    let motion = DropletMotion {
        world_to_body_scale: Vec2::ONE,
        ..DropletMotion::default()
    };
    let mut peak_slosh = 0.0_f32;
    let mut peak_speed = 0.0_f32;
    for tick in 0..18 {
        let phase = tick as f32 / 17.0;
        let local = Vec2::new(0.20 + phase * 0.42, -0.01);
        step(
            &mut runtime,
            &genome,
            &pointer_sensor(local, true),
            &feedback,
            motion,
        );
        let physical = runtime.embodied_interaction_frame();
        peak_slosh = peak_slosh.max(physical.material.slosh_energy);
        peak_speed = peak_speed.max(physical.contact.pointer_speed);
    }
    step(
        &mut runtime,
        &genome,
        &pointer_sensor(Vec2::new(0.62, -0.01), false),
        &feedback,
        motion,
    );
    let released = runtime.embodied_interaction_frame();
    peak_slosh = peak_slosh.max(released.material.slosh_energy);
    peak_speed = peak_speed.max(released.contact.pointer_speed);

    assert!(!released.contact.active);
    assert_eq!(released.contact.contact_seconds, 0.0);
    assert!(peak_speed >= 1.0, "release speed={peak_speed}");
    assert!(peak_slosh >= 0.015, "slosh={peak_slosh}");
    assert_eq!(runtime.diagnostics().recovery_count, 0);
}

#[test]
fn mass_is_conserved_before_during_and_after_remerge() {
    let (genome, mut runtime) = migrated_runtime(0x7EA4);
    settle(&mut runtime, &genome, 2.0);
    let initial_mass_bits = runtime.particles[..runtime.particle_count]
        .iter()
        .map(|particle| particle.inverse_mass.max(1.0e-5).recip())
        .sum::<f32>()
        .to_bits();
    let feedback = BodyFeedback::default();
    let motion = DropletMotion {
        world_to_body_scale: Vec2::ONE,
        ..DropletMotion::default()
    };
    let mut observed_natural_tear = None;
    let mut release_position = Vec2::new(0.82, 0.03);
    for tick in 0..420 {
        let t = tick as f32 / 419.0;
        let local = Vec2::new(0.24 + t * 0.58, 0.03);
        step(
            &mut runtime,
            &genome,
            &pointer_sensor(local, true),
            &feedback,
            motion,
        );
        let detached = runtime.diagnostics().detached_mass.round() as usize;
        if (8..=16).contains(&detached) {
            let detached_mass_bits = runtime.particles[..runtime.particle_count]
                .iter()
                .map(|particle| particle.inverse_mass.max(1.0e-5).recip())
                .sum::<f32>()
                .to_bits();
            assert_eq!(detached_mass_bits, initial_mass_bits);
            observed_natural_tear = Some(detached);
            release_position = local;
            break;
        }
    }
    assert!(
        observed_natural_tear.is_some(),
        "no 8–16 particle natural tear was observed"
    );

    let released = pointer_sensor(release_position, false);
    let mut previous_velocities: Vec<Vec2> = runtime.particles[..runtime.particle_count]
        .iter()
        .map(|particle| particle.velocity)
        .collect();
    let mut previous_components = runtime.diagnostics().component_count;
    let mut merge_delta_v = 0.0_f32;
    let mut mass_inside_field_at_three_seconds = 0_usize;
    for tick in 0..600 {
        step(&mut runtime, &genome, &released, &feedback, motion);
        let components = runtime.diagnostics().component_count;
        if previous_components > 1 && components == 1 {
            merge_delta_v = runtime.particles[..runtime.particle_count]
                .iter()
                .zip(&previous_velocities)
                .map(|(particle, previous)| particle.velocity.distance(*previous))
                .fold(0.0_f32, f32::max);
        }
        previous_components = components;
        previous_velocities = runtime.particles[..runtime.particle_count]
            .iter()
            .map(|particle| particle.velocity)
            .collect();
        if tick == 359 {
            let radii = Vec2::new(0.35, 0.43) * runtime.tuning.character_field_radius_scale;
            mass_inside_field_at_three_seconds = runtime.particles[..runtime.particle_count]
                .iter()
                .filter(|particle| {
                    let q = particle.position / radii;
                    q.length_squared() <= 1.0
                })
                .count();
        }
    }
    let diagnostics = runtime.diagnostics();
    let final_mass_bits = runtime.particles[..runtime.particle_count]
        .iter()
        .map(|particle| particle.inverse_mass.max(1.0e-5).recip())
        .sum::<f32>()
        .to_bits();
    assert_eq!(final_mass_bits, initial_mass_bits);
    assert_eq!(diagnostics.component_count, 1, "{diagnostics:?}");
    assert!(
        diagnostics.main_mass >= runtime.particle_count as f32 * 0.95,
        "{diagnostics:?}"
    );
    assert!(
        mass_inside_field_at_three_seconds as f32 >= runtime.particle_count as f32 * 0.95,
        "only {mass_inside_field_at_three_seconds}/{} particles were inside the character field at 3 s",
        runtime.particle_count
    );
    assert!(rms_speed(&runtime) < 0.055, "rms={}", rms_speed(&runtime));
    assert!(merge_delta_v < 0.08, "merge delta-v={merge_delta_v}");
    assert_eq!(diagnostics.failsafe_hits, 0, "{diagnostics:?}");
    assert_eq!(diagnostics.recovery_count, 0, "{diagnostics:?}");
}

#[test]
fn fixed_seed_replays_identical_split_and_remerge_ticks() {
    fn replay() -> (u32, u32) {
        let (genome, mut runtime) = migrated_runtime(0x7EA4);
        settle(&mut runtime, &genome, 2.0);
        let feedback = BodyFeedback::default();
        let motion = DropletMotion {
            world_to_body_scale: Vec2::ONE,
            ..DropletMotion::default()
        };
        let mut split_tick = None;
        let mut release_position = Vec2::new(0.82, 0.03);
        for tick in 0..420_u32 {
            let phase = tick as f32 / 419.0;
            let local = Vec2::new(0.24 + phase * 0.58, 0.03);
            step(
                &mut runtime,
                &genome,
                &pointer_sensor(local, true),
                &feedback,
                motion,
            );
            if runtime
                .embodied_interaction_frame()
                .detached_event
                .is_some()
            {
                split_tick = Some(tick);
                release_position = local;
                break;
            }
        }
        let split_tick = split_tick.expect("fixed seed did not split");
        let released = pointer_sensor(release_position, false);
        let mut merge_tick = None;
        for tick in 0..600_u32 {
            step(&mut runtime, &genome, &released, &feedback, motion);
            if runtime.embodied_interaction_frame().remerge_event.is_some() {
                merge_tick = Some(split_tick + 1 + tick);
                break;
            }
        }
        (split_tick, merge_tick.expect("fixed seed did not remerge"))
    }

    let first = replay();
    let second = replay();
    assert_eq!(first, second);
    assert!(first.1 > first.0);
}

#[test]
fn voluntary_separation_uses_real_production_particles_under_the_hard_guard() {
    let (genome, mut runtime) = migrated_runtime(0x7EA4);
    let mut production = runtime.tuning;
    production.surface_tension = 2.5;
    production.grab_stiffness = 390.0;
    production.pointer_support_scale = 2.4;
    production.pointer_response_hz = 35.0;
    production.return_strength = 0.05;
    production.character_field_radius_scale = 1.17;
    runtime.set_tuning(
        production,
        runtime.interaction_tuning,
        FaceTuning::default(),
        MaterialVariant::CinematicJelly,
    );
    settle(&mut runtime, &genome, 2.0);
    let feedback = BodyFeedback::default();
    let motion = DropletMotion {
        world_to_body_scale: Vec2::ONE,
        ..DropletMotion::default()
    };
    let mut maximum_detached = 0.0_f32;
    let mut maximum_strain = 0.0_f32;
    let mut maximum_radius = 0.0_f32;
    let mut maximum_predicted_detached = 0.0_f32;
    let mut budget_rejections = 0_u32;
    let mut maximum_predicted_components = 0_usize;
    let mut minimum_predicted_fragment = usize::MAX;
    let mut release_position = Vec2::new(0.82, 0.03);
    for tick in 0..420 {
        let phase = tick as f32 / 419.0;
        let mut sensors = pointer_sensor(Vec2::new(0.24 + phase * 0.58, 0.03), true);
        sensors.interaction_actuation = InteractionBodyActuation {
            compliance_delta: 0.25,
            cohesion_delta: -0.25,
            cooperation: 1.0,
            allow_intentional_bud: true,
            ..InteractionBodyActuation::default()
        };
        step(&mut runtime, &genome, &sensors, &feedback, motion);
        maximum_detached = maximum_detached.max(
            runtime
                .embodied_interaction_frame()
                .material
                .detached_mass_fraction,
        );
        maximum_strain =
            maximum_strain.max(runtime.embodied_interaction_frame().material.maximum_strain);
        maximum_radius = maximum_radius.max(
            runtime.particles[..runtime.particle_count]
                .iter()
                .map(|particle| particle.position.distance(runtime.components.main_com))
                .fold(0.0, f32::max),
        );
        maximum_predicted_detached =
            maximum_predicted_detached.max(runtime.topology_decision.detached_mass_fraction);
        budget_rejections += u32::from(runtime.topology_decision.budget_exhausted);
        maximum_predicted_components =
            maximum_predicted_components.max(runtime.topology_decision.predicted_component_count);
        minimum_predicted_fragment =
            minimum_predicted_fragment.min(runtime.topology_decision.minimum_fragment_particles);
        if maximum_detached > 0.0 {
            release_position = Vec2::new(0.24 + phase * 0.58, 0.03);
            break;
        }
    }
    assert!(
        maximum_detached > 0.0,
        "detached={maximum_detached} predicted={maximum_predicted_detached} components={maximum_predicted_components} min_fragment={minimum_predicted_fragment} rejects={budget_rejections} strain={maximum_strain} radius={maximum_radius} diagnostics={:?}",
        runtime.diagnostics()
    );
    assert!(maximum_detached <= runtime.interaction_tuning.maximum_detached_mass_fraction + 1.0e-5);
    let released = pointer_sensor(release_position, false);
    let mut saw_remerge = false;
    for _ in 0..1_320 {
        step(&mut runtime, &genome, &released, &feedback, motion);
        saw_remerge |= runtime.embodied_interaction_frame().remerge_event.is_some();
        if saw_remerge && runtime.diagnostics().component_count == 1 {
            break;
        }
    }
    assert!(saw_remerge);
    assert_eq!(runtime.diagnostics().component_count, 1);
    assert_eq!(runtime.diagnostics().recovery_count, 0);
}

#[test]
fn flight_uses_continuous_inertia_then_the_same_field_gathers_mass() {
    let (genome, mut runtime) = migrated_runtime(0xF11E);
    settle(&mut runtime, &genome, 2.0);
    let feedback = BodyFeedback::default();
    for _ in 0..120 {
        step(
            &mut runtime,
            &genome,
            &SensorFrame::default(),
            &feedback,
            DropletMotion {
                acceleration: Vec2::new(2.4, 0.0),
                velocity: Vec2::new(0.8, 0.0),
                world_to_body_scale: Vec2::ONE,
                ..DropletMotion::default()
            },
        );
    }
    let lag = particle_center(&runtime).x;
    assert!(lag < -0.025, "flight did not leave mass behind: lag={lag}");
    for _ in 0..600 {
        step(
            &mut runtime,
            &genome,
            &SensorFrame::default(),
            &feedback,
            DropletMotion {
                world_to_body_scale: Vec2::ONE,
                ..DropletMotion::default()
            },
        );
    }
    let diagnostics = runtime.diagnostics();
    assert!(
        particle_center(&runtime).length() < 0.025,
        "center={:?}",
        particle_center(&runtime)
    );
    assert_eq!(diagnostics.component_count, 1, "{diagnostics:?}");
    assert_eq!(diagnostics.failsafe_hits, 0, "{diagnostics:?}");
    assert_eq!(diagnostics.recovery_count, 0, "{diagnostics:?}");
}

#[test]
fn emergency_recovery_preserves_the_visible_mass_and_face_then_crossfades() {
    let (genome, mut runtime) = migrated_runtime(0xFA11_5AFE);
    settle(&mut runtime, &genome, 2.0);
    for _ in 0..120 {
        runtime.presentation_update(1.0 / 60.0);
    }
    let before = runtime.render_state();

    // A poisoned solver value must reset authoritative mass without exposing a
    // one-frame teleport in either the reconstruction or semantic face.
    runtime.particles[0].velocity = Vec2::splat(f32::NAN);
    idle_step(&mut runtime, &genome);
    let recovered = runtime.render_state();
    assert!(recovered.diagnostics.finite, "{:?}", recovered.diagnostics);
    assert_eq!(recovered.diagnostics.recovery_count, 1);
    assert_eq!(recovered.face_frame, before.face_frame);
    let mut visible_steps: Vec<f32> = recovered.particles[..recovered.particle_count]
        .iter()
        .zip(&before.particles[..before.particle_count])
        .map(|(after, before)| after.position.distance(before.position))
        .collect();
    visible_steps.sort_by(f32::total_cmp);
    let p95_step = visible_steps[(visible_steps.len() as f32 * 0.95).floor() as usize];
    assert!(
        p95_step <= KERNEL_RADIUS * runtime.tuning.kernel_radius_scale * 0.10,
        "recovery frame p95 visible step={p95_step}; steps={visible_steps:?}"
    );

    idle_step(&mut runtime, &genome);
    runtime.presentation_update(1.0 / 60.0);
    let next = runtime.render_state();
    let face_step = next.face_frame.origin.distance(recovered.face_frame.origin);
    let roll_before = recovered
        .face_frame
        .axis_x
        .y
        .atan2(recovered.face_frame.axis_x.x);
    let roll_after = next.face_frame.axis_x.y.atan2(next.face_frame.axis_x.x);
    let roll_step = (roll_after - roll_before).abs();
    let scale_step = (next.face_frame.scale - recovered.face_frame.scale)
        .abs()
        .max_element();
    assert!(face_step <= 0.008_001, "face origin step={face_step}");
    assert!(roll_step <= 0.020_001, "face roll step={roll_step}");
    assert!(scale_step <= 0.010_001, "face scale step={scale_step}");
    assert_eq!(next.diagnostics.recovery_count, 1);
}

#[test]
#[ignore = "production acceptance: 10 deterministic seeds × 120 simulated seconds"]
fn production_idle_acceptance_10_seeds_120_seconds() {
    for seed in 0..10_u64 {
        let (genome, mut runtime) = migrated_runtime(0x1D1E_0000 + seed);
        let mut compression_samples = Vec::with_capacity(96 * 120 * 120);
        let mut middle_energy = 0.0_f32;
        let mut middle_samples = 0.0_f32;
        let mut tail_energy = 0.0_f32;
        let mut tail_samples = 0.0_f32;
        for tick in 0..(120 * 120) {
            idle_step(&mut runtime, &genome);
            for particle in &runtime.particles[..runtime.particle_count] {
                compression_samples
                    .push((particle.density / runtime.rest_density.max(0.01) - 1.0).max(0.0));
            }
            if (7_200..10_800).contains(&tick) {
                middle_energy += runtime.diagnostics().kinetic_energy;
                middle_samples += 1.0;
            } else if tick >= 10_800 {
                tail_energy += runtime.diagnostics().kinetic_energy;
                tail_samples += 1.0;
            }
            let diagnostics = runtime.diagnostics();
            assert!(
                diagnostics.finite,
                "seed={seed} tick={tick} {diagnostics:?}"
            );
            assert_eq!(diagnostics.component_count, 1, "seed={seed} tick={tick}");
            assert_eq!(diagnostics.failsafe_hits, 0, "seed={seed} tick={tick}");
            assert_eq!(diagnostics.recovery_count, 0, "seed={seed} tick={tick}");
        }
        compression_samples.sort_by(f32::total_cmp);
        let p95 = compression_samples[(compression_samples.len() as f32 * 0.95).floor() as usize];
        let maximum = *compression_samples.last().unwrap_or(&0.0);
        let middle_energy = middle_energy / middle_samples.max(1.0);
        let tail_energy = tail_energy / tail_samples.max(1.0);
        assert!(p95 <= 0.10, "seed={seed} p95 compression={p95}");
        assert!(maximum <= 0.25, "seed={seed} max compression={maximum}");
        assert!(
            tail_energy <= middle_energy * 1.05 + 1.0e-5,
            "seed={seed} kinetic energy grew: middle={middle_energy}, tail={tail_energy}"
        );
    }
}

#[test]
#[ignore = "production acceptance: 60 simulated seconds of adversarial pointer motion"]
fn production_pointer_stress_60_seconds_circles_and_8hz_zigzag() {
    let (genome, mut runtime) = migrated_runtime(0xC1AC_1E08);
    settle(&mut runtime, &genome, 2.0);
    let feedback = BodyFeedback::default();
    let motion = DropletMotion {
        world_to_body_scale: Vec2::ONE,
        ..DropletMotion::default()
    };
    let mut peak_density_ratio = 0.0_f32;
    let mut peak_components = 1_usize;
    let mut minimum_main_mass = runtime.particle_count as f32;
    let mut peak_extent = 0.0_f32;
    let mut rigid_samples = Vec::with_capacity(60 * 120);
    let mut current_rigid_run = 0_usize;
    let mut longest_rigid_run = 0_usize;

    for tick in 0..(60 * 120) {
        let time = tick as f32 * DT;
        let local = if time < 30.0 {
            let angle = std::f32::consts::TAU * 0.65 * time;
            Vec2::from_angle(angle) * 0.34
        } else {
            let zigzag_time = time - 30.0;
            Vec2::new(
                triangle_wave(8.0 * zigzag_time) * 0.38,
                (std::f32::consts::TAU * 1.3 * zigzag_time).sin() * 0.12,
            )
        };
        step(
            &mut runtime,
            &genome,
            &pointer_sensor(local, true),
            &feedback,
            motion,
        );

        let diagnostics = runtime.diagnostics();
        assert!(diagnostics.finite, "tick={tick} {diagnostics:?}");
        assert_eq!(diagnostics.failsafe_hits, 0, "tick={tick} {diagnostics:?}");
        assert_eq!(diagnostics.recovery_count, 0, "tick={tick} {diagnostics:?}");
        peak_components = peak_components.max(diagnostics.component_count);
        minimum_main_mass = minimum_main_mass.min(diagnostics.main_mass);
        for particle in &runtime.particles[..runtime.particle_count] {
            peak_density_ratio =
                peak_density_ratio.max(particle.density / runtime.rest_density.max(0.01));
            peak_extent = peak_extent.max(particle.position.distance(runtime.body_origin));
        }

        if let Some(explained) = rigid_velocity_explained(&runtime) {
            rigid_samples.push(explained);
            if explained >= 0.97 {
                current_rigid_run += 1;
                longest_rigid_run = longest_rigid_run.max(current_rigid_run);
            } else {
                current_rigid_run = 0;
            }
        } else {
            current_rigid_run = 0;
        }
    }

    rigid_samples.sort_by(f32::total_cmp);
    let rigid_p95 = rigid_samples
        .get((rigid_samples.len() as f32 * 0.95).floor() as usize)
        .copied()
        .unwrap_or(0.0);
    eprintln!(
        "pointer stress: peak_density_ratio={peak_density_ratio:.4}, peak_components={peak_components}, minimum_main_mass={minimum_main_mass:.1}, peak_extent={peak_extent:.4}, rigid_p95={rigid_p95:.4}, longest_rigid_run_ticks={longest_rigid_run}"
    );
    assert!(
        peak_density_ratio <= 1.30,
        "peak density was {peak_density_ratio:.4} rho0"
    );
    assert!(
        peak_components <= 6,
        "exploded into {peak_components} components"
    );
    assert!(
        minimum_main_mass >= runtime.particle_count as f32 * 0.50,
        "main blob collapsed to {minimum_main_mass}/{} particles",
        runtime.particle_count
    );
    assert!(
        peak_extent <= 1.10,
        "particle escaped to radius {peak_extent}"
    );
    assert!(rigid_p95 < 0.97, "rigid velocity p95={rigid_p95}");
    assert!(
        longest_rigid_run < 120,
        "rigid convergence persisted for {longest_rigid_run} ticks"
    );
}

#[test]
#[ignore = "production acceptance: render-rate-independent fixed-120 replay"]
fn production_fixed_120_replay_is_independent_of_30_60_144hz_presentation() {
    fn sampled_sensor(time: f32) -> SensorFrame {
        if !(1.0..6.0).contains(&time) {
            return pointer_sensor(Vec2::ZERO, false);
        }
        let local = if time < 4.0 {
            let angle = std::f32::consts::TAU * 0.55 * (time - 1.0);
            Vec2::from_angle(angle) * 0.32
        } else {
            Vec2::new(
                triangle_wave(5.0 * (time - 4.0)) * 0.34,
                (std::f32::consts::TAU * 0.8 * (time - 4.0)).sin() * 0.10,
            )
        };
        pointer_sensor(local, true)
    }

    fn replay_motion(time: f32) -> DropletMotion {
        let (velocity, acceleration) = if (6.0..6.5).contains(&time) {
            (Vec2::new(0.8, 0.0), Vec2::new(2.4, 0.0))
        } else if (6.5..7.0).contains(&time) {
            (Vec2::new(0.8, 0.0), Vec2::ZERO)
        } else if (7.0..7.5).contains(&time) {
            (Vec2::new(0.4, 0.0), Vec2::new(-2.4, 0.0))
        } else {
            (Vec2::ZERO, Vec2::ZERO)
        };
        DropletMotion {
            velocity,
            acceleration,
            world_to_body_scale: Vec2::ONE,
            ..DropletMotion::default()
        }
    }

    fn run_replay(presentation_hz: u32) -> LiquidMorphRuntime {
        let (genome, mut runtime) = migrated_runtime(0x120F_24A6);
        let feedback = BodyFeedback::default();
        let mut presentation_frame = 0_u32;
        for tick in 0..(12 * 120_u32) {
            while u64::from(presentation_frame) * 120
                <= u64::from(tick) * u64::from(presentation_hz)
            {
                runtime.presentation_update(1.0 / presentation_hz as f32);
                presentation_frame += 1;
            }
            let simulation_time = tick as f32 * DT;
            step(
                &mut runtime,
                &genome,
                &sampled_sensor(simulation_time),
                &feedback,
                replay_motion(simulation_time),
            );
        }
        runtime
    }

    let replay_30 = run_replay(30);
    let replay_60 = run_replay(60);
    let replay_144 = run_replay(144);
    let pairs = [
        ("30/60", &replay_30, &replay_60),
        ("30/144", &replay_30, &replay_144),
        ("60/144", &replay_60, &replay_144),
    ];
    let tolerance = 0.35 * 0.02;
    let mut maximum_set_distance = 0.0_f32;
    let mut topology_matches = true;
    for (label, first, second) in pairs {
        let set_distance = bidirectional_particle_set_distance(first, second);
        maximum_set_distance = maximum_set_distance.max(set_distance);
        let material_distance = first.particles[..first.particle_count]
            .iter()
            .zip(&second.particles[..second.particle_count])
            .map(|(a, b)| a.position.distance(b.position))
            .fold(0.0_f32, f32::max);
        let first_topology = topology_signature(first);
        let second_topology = topology_signature(second);
        eprintln!(
            "presentation replay {label}: set_distance={set_distance:.6}, material_distance={material_distance:.6}, topology={first_topology:?}/{second_topology:?}"
        );
        assert!(first.diagnostics().finite && second.diagnostics().finite);
        assert_eq!(first.diagnostics().failsafe_hits, 0, "{label}");
        assert_eq!(second.diagnostics().failsafe_hits, 0, "{label}");
        assert_eq!(first.diagnostics().recovery_count, 0, "{label}");
        assert_eq!(second.diagnostics().recovery_count, 0, "{label}");
        topology_matches &= first_topology == second_topology;
    }
    assert!(topology_matches, "presentation rate changed final topology");
    assert!(
        maximum_set_distance <= tolerance,
        "maximum particle-set delta {maximum_set_distance} exceeded {tolerance}"
    );
}
