//! A controlled cursor replay through the actual liquid solver, contact bridge,
//! gesture classifier and felt-state graph. No physical summary is fabricated.
//! The carrier stays still to isolate material touch from autonomous navigation.
use glam::Vec2;
use lifecore::{
    BodyIntent, BodyInteroceptionDirector, EmbodimentSourceFrame, Genome, LifeCore, LocomotionMode,
    PoseIntent, SensorFrame,
};
use pet_body::{LiquidTuningProfile, ProceduralBody, VisualMindInput, VoiceVisualState};
use pet_perception::{EmbodiedGestureClassifier, EmbodiedGestureClassifierTuning};
use serde_json::{Value, json};

const HZ: u32 = 120;
const DT: f32 = 1.0 / HZ as f32;

fn pointer_path(case: &str, seconds: f32) -> Vec2 {
    match case {
        "gentle_pull" | "gentle_release" => {
            let duration = if case == "gentle_release" { 2.0 } else { 4.0 };
            let t = (seconds / duration).clamp(0.0, 1.0);
            Vec2::new(0.20 + 0.12 * t * t * (3.0 - 2.0 * t), 0.03)
        }
        "gentle_sweep" => Vec2::new(
            0.21 + 0.045 * (seconds * 0.7).sin(),
            0.03 + 0.05 * (seconds * 0.9).sin(),
        ),
        "sharp_pull" => {
            let t = (seconds * 2.0).fract();
            Vec2::new(if t < 0.12 { 0.20 } else { 0.76 }, 0.02)
        }
        "stationary_hold" => Vec2::new(0.20, 0.03),
        "overstretched_hold" | "large_release" => {
            let t = seconds.clamp(0.0, 1.0);
            Vec2::new(0.20 + 0.56 * t * t * (3.0 - 2.0 * t), 0.03)
        }
        _ => Vec2::new(0.20, 0.03),
    }
}

fn physical_motion_sample(body: &ProceduralBody, profile: &LiquidTuningProfile) -> Value {
    let snapshot = body.embodiment.liquid.body_material_snapshot(
        profile.seed,
        profile.schema_version,
        profile.profile_revision,
    );
    let mass = snapshot
        .particles
        .iter()
        .map(|p| p.inverse_mass.max(1.0e-5).recip())
        .sum::<f32>();
    let center = snapshot
        .particles
        .iter()
        .map(|p| p.position / p.inverse_mass.max(1.0e-5))
        .sum::<Vec2>()
        / mass.max(1.0e-5);
    let velocity = snapshot
        .particles
        .iter()
        .map(|p| p.velocity / p.inverse_mass.max(1.0e-5))
        .sum::<Vec2>()
        / mass.max(1.0e-5);
    let mut inertia = 0.0;
    let mut angular_momentum = 0.0;
    let mut covariance = [0.0_f32; 3];
    for p in &snapshot.particles {
        let m = p.inverse_mass.max(1.0e-5).recip();
        let q = p.position - center;
        inertia += m * q.length_squared();
        angular_momentum += m * q.perp_dot(p.velocity - velocity);
        covariance[0] += m * q.x * q.x;
        covariance[1] += m * q.y * q.y;
        covariance[2] += m * q.x * q.y;
    }
    for value in &mut covariance {
        *value /= mass.max(1.0e-5);
    }
    let angular_velocity = angular_momentum / inertia.max(1.0e-5);
    let internal_kinetic_energy = snapshot
        .particles
        .iter()
        .map(|p| {
            let relative = p.velocity - velocity - (p.position - center).perp() * angular_velocity;
            0.5 * relative.length_squared() / p.inverse_mass.max(1.0e-5)
        })
        .sum::<f32>();
    let diagnostics = body.embodiment.liquid.diagnostics();
    json!({"mass":mass,"mass_bits_checksum":snapshot.total_mass_bits_checksum,
        "particle_count":snapshot.particle_count,"center":center,"mean_velocity":velocity,
        "angular_velocity":angular_velocity,"covariance_xx_yy_xy":covariance,
        "internal_kinetic_energy":internal_kinetic_energy,
        "diagnostics":{"finite":diagnostics.finite,"components":diagnostics.component_count,
            "failsafe_hits":diagnostics.failsafe_hits,"recovery_count":diagnostics.recovery_count,
            "main_mass":diagnostics.main_mass,"detached_mass":diagnostics.detached_mass,
            "maximum_speed":diagnostics.maximum_speed,"maximum_bond_strain":diagnostics.maximum_bond_strain,
            "kinetic_energy":diagnostics.kinetic_energy,"maximum_compression":diagnostics.maximum_compression}})
}

fn run_case(case: &str, profile: &LiquidTuningProfile) -> Result<Value, String> {
    let genome = Genome::from_seed(profile.seed);
    let mut body = Box::new(ProceduralBody::generate(&genome).map_err(|e| e.to_string())?);
    body.apply_tuning_profile(profile.clone())
        .map_err(|e| e.to_string())?;
    body.set_desktop_motion_space(Vec2::new(1920.0, 1080.0), 512.0);
    body.embodiment.managed_blink = true;
    let mut core = LifeCore::new(genome.clone(), profile.seed);
    let mut interoceptor = BodyInteroceptionDirector::default();
    let mut classifier = EmbodiedGestureClassifier::default();
    let tuning = profile.interaction;
    classifier.set_tuning(EmbodiedGestureClassifierTuning {
        window_seconds: tuning.gesture_window_seconds,
        commit_confidence: tuning.gesture_commit_confidence,
        ambiguity_margin: tuning.gesture_ambiguity_margin,
        soft_touch_pressure_max: tuning.soft_touch_pressure_max,
        stretch_strain_min: tuning.stretch_strain_min,
        stretch_strain_max: tuning.stretch_strain_max,
        flick_speed_min: tuning.flick_speed_min,
        rhythm_interval_cv_max: tuning.rhythm_interval_cv_max,
        rhythm_min_impulses: tuning.rhythm_min_impulses,
        boundary_strain: tuning.boundary_strain,
    });
    let intent = BodyIntent {
        locomotion: LocomotionMode::Hover,
        target_position: Vec2::splat(0.5),
        target_surface: None,
        desired_speed: 0.0,
        facing_direction: 1.0,
        gaze_target: None,
        pose: PoseIntent::Neutral,
        expression: Default::default(),
        interaction_target: None,
    };
    let mut source = EmbodimentSourceFrame {
        frame_id: 0,
        affect: core.state.affect,
        drives: core.state.drives,
        temperament: genome.temperament,
        voice_seed: genome.voice.voice_seed,
        vita: Default::default(),
        morph: Default::default(),
        body: Default::default(),
        voice_feedback: Default::default(),
        gesture: Default::default(),
        episode: Default::default(),
        perception: Default::default(),
        soft_touch_pressure_max: tuning.soft_touch_pressure_max,
    };
    let mut samples = Vec::new();
    let mut release_motion_120hz = Vec::new();
    let mut responses = Vec::new();
    let mut peak_pleasantness = 0.0_f32;
    let mut peak_pain = 0.0_f32;
    let mut peak_slip = 0.0_f32;
    let mut peak_strain = 0.0_f32;
    let mut maximum_components = 1;
    let mut active_samples = 0;
    let mut pleasant_samples = 0;
    let release_case = matches!(case, "gentle_release" | "large_release");
    let release_seconds = if release_case { 6.0 } else { 10.0 };
    let total_seconds = if release_case { 16 } else { 12 };
    for tick in 0..total_seconds * HZ {
        let seconds = tick as f32 * DT;
        let held = case != "absent" && (2.0..release_seconds).contains(&seconds);
        let local = pointer_path(case, (seconds - 2.0).max(0.0));
        let sensors = SensorFrame {
            cursor_position: body.simulation.feedback.world_position
                + local / body.embodiment.world_to_body_scale(),
            pointer_down: held,
            pet_hovered: held,
            pet_touched: held,
            pet_dragged: held,
            ..Default::default()
        };
        body.embodied_update(
            &intent,
            &sensors,
            core.state.affect,
            VisualMindInput::default(),
            VoiceVisualState::default(),
            DT,
        );
        let physical = body.embodied_interaction_frame();
        if release_case && (6.0..=10.0).contains(&seconds) {
            release_motion_120hz.push(json!({"seconds":seconds,
                "motion":physical_motion_sample(&body, profile),
                "material":physical.material}));
        }
        maximum_components = maximum_components.max(physical.material.component_count);
        if let Some(event) = classifier.ingest(physical) {
            let response = core.observe_embodied_gesture(event);
            responses.push(
                json!({"seconds":seconds,"classification":event.classification,
                "boundary":event.boundary,"physical":physical,"response":response}),
            );
        }
        if tick % 6 == 0 {
            let feedback =
                body.body_feedback_v2(&intent, &sensors, Some(&source.body), u64::from(tick));
            source.frame_id = u64::from(tick);
            source.body = feedback;
            source.affect = core.state.affect;
            source.drives = core.state.drives;
            let snapshot = interoceptor.tick(&source, 0.05);
            core.integrate_felt_state(snapshot, source.episode, 0.05);
            let felt = snapshot.felt;
            peak_pain = peak_pain.max(felt.pain_like);
            peak_slip = peak_slip.max(physical.contact.relative_velocity_local.length());
            peak_strain = peak_strain.max(physical.material.maximum_strain);
            if physical.contact.active {
                active_samples += 1;
                peak_pleasantness = peak_pleasantness.max(felt.contact_pleasantness);
                if felt.contact_pleasantness > 0.42 && felt.pain_like < 0.15 {
                    pleasant_samples += 1;
                }
            }
            samples.push(json!({"seconds":seconds,"contact":physical.contact,
                "material":physical.material,"felt":felt,"affect":core.state.affect,
                "motion":physical_motion_sample(&body, profile)}));
        }
    }
    let diagnostics = body.embodiment.liquid.diagnostics();
    if !diagnostics.finite || diagnostics.failsafe_hits > 0 {
        return Err(format!("{case}: invalid physics {diagnostics:?}"));
    }
    Ok(
        json!({"case":case,"active_samples":active_samples,"pleasant_samples":pleasant_samples,
        "peak_pleasantness":peak_pleasantness,"peak_pain":peak_pain,"peak_slip":peak_slip,
        "peak_strain":peak_strain,"failsafe_hits":diagnostics.failsafe_hits,
        "final_components":diagnostics.component_count,"maximum_components":maximum_components,
        "release_seconds":release_seconds,"responses":responses,"samples":samples,
        "release_motion_120hz":release_motion_120hz}),
    )
}

fn review(profile_path: &str, output: &str) -> Result<(), String> {
    let profile: LiquidTuningProfile =
        serde_json::from_slice(&std::fs::read(profile_path).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    let mut cases = Vec::new();
    for case in [
        "absent",
        "gentle_pull",
        "gentle_sweep",
        "sharp_pull",
        "stationary_hold",
        "overstretched_hold",
        "gentle_release",
        "large_release",
    ] {
        let result = run_case(case, &profile)?;
        println!(
            "{case}: active={}, pleasant={}, peak pleasure={}, pain={}, slip={}, strain={}",
            result["active_samples"],
            result["pleasant_samples"],
            result["peak_pleasantness"],
            result["peak_pain"],
            result["peak_slip"],
            result["peak_strain"]
        );
        cases.push(result);
    }
    std::fs::write(output, serde_json::to_vec_pretty(&json!({
        "protocol":"actual native liquid; unmodified profile; genuine classifier; stationary carrier; no fabricated physical summaries; no app sensitivity calibration",
        "profile":profile_path,"seed":profile.seed,"cases":cases
    })).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}

fn main() -> Result<(), String> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 2 {
        return Err("usage: material_care_review PROFILE_JSON OUTPUT_JSON".to_owned());
    }
    std::thread::Builder::new()
        .stack_size(16 * 1024 * 1024)
        .spawn(move || review(&args[0], &args[1]))
        .map_err(|e| e.to_string())?
        .join()
        .map_err(|_| "material review panicked".to_owned())?
}
