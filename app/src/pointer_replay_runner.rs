use std::{error::Error, fs::File, io::BufReader};

use glam::Vec2;
use lifecore::{
    BodyIntent, EmbodiedGestureKind, ExpressionState, InteractionReasonCode, LifeCore,
    LocomotionMode, PoseIntent, SensorFrame, stable_hash_bytes,
};
use morph_brain::{MorphBrain, MorphWorldInput};
use pet_body::{LiquidTuningProfile, ProceduralBody, VisualMindInput, VoiceVisualState};
use serde::Serialize;

use crate::{
    Arguments, BrainMode, PreparedState, VitaRuntime, classifier_tuning,
    replay::pointer_interaction::PointerInteractionReplay,
};

const REPLAY_HZ: u64 = 120;
const PERCEPTION_DIVISOR: u64 = 2;
const LIFE_DIVISOR: u64 = 6;
const BODY_DT: f32 = 1.0 / 120.0;

#[derive(Debug, Clone, PartialEq, Serialize)]
struct ReplayOutcome {
    semantic_input_hash: u64,
    final_state_hash: u64,
    final_mass_checksum: u64,
    selected_gesture: EmbodiedGestureKind,
    response_reason: InteractionReasonCode,
    response_variant: u8,
    component_timeline: Vec<(u64, u8)>,
    detached_ticks: Vec<u64>,
    remerge_ticks: Vec<u64>,
    response_count: u32,
}

pub(crate) fn run(
    arguments: &Arguments,
    store: &desktop_host::StateStore,
    prepared: PreparedState,
) -> Result<(), Box<dyn Error>> {
    let path = arguments
        .pointer_replay
        .as_ref()
        .ok_or("pointer replay path is missing")?;
    let replay: PointerInteractionReplay =
        serde_json::from_reader(BufReader::new(File::open(path)?))?;
    let PreparedState {
        life, vita, morph, ..
    } = prepared;
    if replay.identity_seed != life.state.genome.identity_seed {
        return Err("pointer replay identity does not match the prepared Pet identity".into());
    }
    let tuning = store
        .load_liquid_tuning::<LiquidTuningProfile>()?
        .unwrap_or_else(|| super::approved_production_liquid_tuning(replay.identity_seed))
        .sanitized()?;
    let tuning = super::production_liquid_tuning(tuning);
    replay.validate(tuning.pbf)?;
    if replay.fixed_hz != REPLAY_HZ as u32 {
        return Err("production pointer replay currently requires fixed_hz = 120".into());
    }
    let outcome = execute_replay(&replay, life, vita, morph, tuning)?;
    let failures = expectation_failures(&replay, &outcome);
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "schema_version": replay.schema_version,
            "replay": path,
            "outcome": outcome,
            "expectation_failures": failures,
            "passed": failures.is_empty(),
        }))?
    );
    if failures.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "pointer replay expectation mismatch: {}",
            failures.join("; ")
        )
        .into())
    }
}

fn execute_replay(
    replay: &PointerInteractionReplay,
    mut life: LifeCore,
    mut vita: VitaRuntime,
    mut morph: MorphBrain,
    tuning: LiquidTuningProfile,
) -> Result<ReplayOutcome, Box<dyn Error>> {
    replay.validate(tuning.pbf)?;
    let mut body = ProceduralBody::generate(&life.state.genome)?;
    body.apply_tuning_profile(tuning.clone())?;
    body.restore_body_material_snapshot(&replay.initial_body_snapshot)?;
    body.embodiment
        .set_world_to_body_scale(Vec2::new(12.0, -12.0));
    vita.set_embodied_gesture_tuning(classifier_tuning(tuning.interaction));
    let mut sensors = SensorFrame::default();
    let mut intent = stable_intent();
    let mut sample_index = 0_usize;
    let mut current_local = Vec2::ZERO;
    let mut previous_local = Vec2::ZERO;
    let mut current_down = false;
    let mut previous_down = false;
    let mut current_hovered = false;
    let expected_last_tick = replay
        .expected
        .component_timeline
        .last()
        .map_or(0, |checkpoint| checkpoint.tick)
        .max(replay.expected.detached_ticks.last().copied().unwrap_or(0))
        .max(replay.expected.remerge_ticks.last().copied().unwrap_or(0));
    let end_tick = replay
        .samples
        .last()
        .map_or(1, |sample| sample.tick)
        .max(expected_last_tick)
        .saturating_add(LIFE_DIVISOR)
        .min(u64::from(replay.fixed_hz) * 30);
    let mut selected_gesture = EmbodiedGestureKind::Unknown;
    let mut response_reason = InteractionReasonCode::CuriousInspection;
    let mut response_variant = 0_u8;
    let mut response_count = 0_u32;
    let mut component_timeline = vec![(
        0,
        body.embodied_interaction_frame().material.component_count,
    )];
    let mut detached_ticks = Vec::new();
    let mut remerge_ticks = Vec::new();

    for tick in 1..=end_tick {
        while sample_index < replay.samples.len() && replay.samples[sample_index].tick <= tick {
            let sample = replay.samples[sample_index];
            current_local = sample.position_body_local;
            current_down = sample.down;
            current_hovered = sample.hovered;
            sample_index += 1;
        }
        let velocity_local = (current_local - previous_local) / BODY_DT;
        let scale = body.embodiment.world_to_body_scale();
        let safe_scale = Vec2::new(
            if scale.x.abs() > 1.0e-5 { scale.x } else { 1.0 },
            if scale.y.abs() > 1.0e-5 {
                scale.y
            } else {
                -1.0
            },
        );
        let body_position = body.simulation.feedback.world_position;
        sensors.timestamp = tick as f64 / REPLAY_HZ as f64;
        sensors.cursor_position = body_position + current_local / safe_scale;
        sensors.cursor_velocity = velocity_local / safe_scale;
        sensors.cursor_acceleration = Vec2::ZERO;
        sensors.cursor_distance_to_pet = sensors.cursor_position.distance(body_position);
        sensors.pointer_down = current_down;
        sensors.pointer_pressed = current_down && !previous_down;
        sensors.pointer_released = !current_down && previous_down;
        sensors.pet_hovered = current_hovered;
        sensors.pet_dragged = current_down && current_hovered;
        sensors.pet_touched = sensors.pointer_pressed && current_hovered;
        sensors.embodied_interaction = body.embodied_interaction_frame();
        body.simulation.feedback.cursor_contact = sensors.embodied_interaction.contact.active;
        if tick.is_multiple_of(PERCEPTION_DIVISOR) {
            vita.observe(&sensors, &body.simulation.feedback, 1.0 / 60.0);
        }
        if tick.is_multiple_of(LIFE_DIVISOR) {
            let morph_output = morph.tick_with_world(
                &sensors,
                &body.simulation.feedback,
                &life.state,
                &MorphWorldInput::default(),
                1.0 / 20.0,
            );
            let mut output = life.tick(&sensors, &body.simulation.feedback, 1.0 / 20.0);
            if let Some(event) = vita.take_embodied_gesture() {
                let episode_id = event.classification.episode_id;
                if selected_gesture == EmbodiedGestureKind::Unknown
                    && event.classification.kind != EmbodiedGestureKind::Unknown
                {
                    selected_gesture = event.classification.kind;
                }
                if let Some(plan) = life.observe_embodied_gesture_with_tuning(
                    event,
                    tuning.interaction.response_amplitude,
                    tuning.interaction.turn_wait_seconds,
                    tuning.interaction.turn_cooldown_seconds,
                    tuning.interaction.learning_openness,
                ) {
                    if vita.accept_interaction_response(plan) {
                        response_count = response_count.saturating_add(1);
                        response_reason = plan.reason;
                        response_variant = life
                            .state
                            .interactions
                            .pending_credit
                            .map_or(0, |credit| credit.variant);
                    }
                } else {
                    vita.finish_interaction_appraisal_without_response(episode_id);
                }
            }
            if let Some(request) = output.vocal_request.take() {
                life.cancel_vocal_request(request.performance_seed);
            }
            (intent, _) = vita.resolve_intent_with_morph(
                BrainMode::MorphFusion,
                &life.state,
                &sensors,
                &body.simulation.feedback,
                output.body_intent,
                Some(morph_output),
                1.0 / 20.0,
            );
            sensors.interaction_actuation = vita.interaction_actuation();
        }
        body.fixed_update(&life.state.genome, &intent, &sensors, BODY_DT);
        body.embodied_update(
            &intent,
            &sensors,
            life.state.affect,
            VisualMindInput::default(),
            VoiceVisualState::default(),
            BODY_DT,
        );
        let frame = body.embodied_interaction_frame();
        if !frame.is_valid() || !body.embodiment.liquid.diagnostics().finite {
            return Err(format!("pointer replay body invariant failed at tick {tick}").into());
        }
        if component_timeline
            .last()
            .is_none_or(|(_, count)| *count != frame.material.component_count)
        {
            component_timeline.push((tick, frame.material.component_count));
        }
        if frame.detached_event.is_some() {
            detached_ticks.push(tick);
        }
        if frame.remerge_event.is_some() {
            remerge_ticks.push(tick);
        }
        previous_local = current_local;
        previous_down = current_down;
    }
    let final_body = body.body_material_snapshot();
    // Serialize concrete contracts directly. Going through serde_json::Value
    // would reject the genome's full-width u128 lineage identifier.
    let mut state_bytes = serde_json::to_vec(&life.snapshot())?;
    state_bytes.extend_from_slice(&serde_json::to_vec(&final_body)?);
    state_bytes.extend_from_slice(&serde_json::to_vec(&selected_gesture)?);
    state_bytes.extend_from_slice(&serde_json::to_vec(&response_reason)?);
    state_bytes.push(response_variant);
    state_bytes.extend_from_slice(&response_count.to_le_bytes());
    Ok(ReplayOutcome {
        semantic_input_hash: replay.semantic_hash(),
        final_state_hash: stable_hash_bytes(&state_bytes),
        final_mass_checksum: final_body.total_mass_bits_checksum,
        selected_gesture,
        response_reason,
        response_variant,
        component_timeline,
        detached_ticks,
        remerge_ticks,
        response_count,
    })
}

fn expectation_failures(replay: &PointerInteractionReplay, outcome: &ReplayOutcome) -> Vec<String> {
    let mut failures = Vec::new();
    if outcome.final_mass_checksum != replay.expected.mass_checksum {
        failures.push("mass checksum changed".to_owned());
    }
    if !replay.expected.component_timeline.is_empty() {
        let actual = outcome
            .component_timeline
            .iter()
            .map(|(tick, component_count)| (*tick, *component_count))
            .collect::<Vec<_>>();
        let expected = replay
            .expected
            .component_timeline
            .iter()
            .map(|checkpoint| (checkpoint.tick, checkpoint.component_count))
            .collect::<Vec<_>>();
        if actual != expected {
            failures.push("component timeline differs".to_owned());
        }
    }
    if !replay.expected.detached_ticks.is_empty()
        && outcome.detached_ticks != replay.expected.detached_ticks
    {
        failures.push("detachment ticks differ".to_owned());
    }
    if !replay.expected.remerge_ticks.is_empty()
        && outcome.remerge_ticks != replay.expected.remerge_ticks
    {
        failures.push("remerge ticks differ".to_owned());
    }
    if replay.expected.selected_gesture != EmbodiedGestureKind::Unknown {
        if outcome.selected_gesture != replay.expected.selected_gesture {
            failures.push("selected gesture differs".to_owned());
        }
        if outcome.response_reason != replay.expected.response_reason
            || outcome.response_variant != replay.expected.response_variant
        {
            failures.push("response reason or variant differs".to_owned());
        }
    }
    if replay.expected.final_state_hash != 0
        && outcome.final_state_hash != replay.expected.final_state_hash
    {
        failures.push("final state hash differs".to_owned());
    }
    failures
}

fn stable_intent() -> BodyIntent {
    BodyIntent {
        locomotion: LocomotionMode::Hover,
        target_position: Vec2::splat(0.5),
        target_surface: None,
        desired_speed: 0.0,
        facing_direction: 1.0,
        gaze_target: None,
        pose: PoseIntent::Neutral,
        expression: ExpressionState::default(),
        interaction_target: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::replay::pointer_interaction::{
        ComponentTimelineCheckpoint, POINTER_INTERACTION_REPLAY_SCHEMA_VERSION,
        PointerReplayExpectations, PointerReplaySample,
    };
    use lifecore::Genome;
    use pet_body::LIQUID_TUNING_SCHEMA_VERSION;

    fn deterministic_case() -> (PointerInteractionReplay, Genome, LiquidTuningProfile) {
        let genome = Genome::from_seed(42);
        let tuning = LiquidTuningProfile::for_seed(genome.identity_seed)
            .sanitized()
            .unwrap();
        let mut body = ProceduralBody::generate(&genome).unwrap();
        body.apply_tuning_profile(tuning.clone()).unwrap();
        let snapshot = body.body_material_snapshot();
        let replay = PointerInteractionReplay {
            schema_version: POINTER_INTERACTION_REPLAY_SCHEMA_VERSION,
            base_commit: "test".to_owned(),
            identity_seed: genome.identity_seed,
            tuning_schema_version: LIQUID_TUNING_SCHEMA_VERSION,
            tuning_revision: tuning.profile_revision,
            fixed_hz: 120,
            initial_body_snapshot: snapshot.clone(),
            samples: vec![
                PointerReplaySample {
                    tick: 1,
                    position_body_local: Vec2::new(-0.045, 0.015),
                    down: true,
                    hovered: true,
                },
                PointerReplaySample {
                    tick: 84,
                    position_body_local: Vec2::new(-0.045, 0.015),
                    down: false,
                    hovered: true,
                },
                PointerReplaySample {
                    tick: 240,
                    position_body_local: Vec2::new(-0.045, 0.015),
                    down: false,
                    hovered: false,
                },
            ],
            expected: PointerReplayExpectations {
                mass_checksum: snapshot.total_mass_bits_checksum,
                ..PointerReplayExpectations::default()
            },
        };
        (replay, genome, tuning)
    }

    fn run_case(
        replay: &PointerInteractionReplay,
        genome: &Genome,
        tuning: LiquidTuningProfile,
    ) -> ReplayOutcome {
        let life = LifeCore::new(genome.clone(), 42 ^ 0xA11F_EC0A);
        let vita = VitaRuntime::new(genome.identity_seed, None);
        let morph = MorphBrain::new(genome.identity_seed, None).unwrap();
        execute_replay(replay, life, vita, morph, tuning).unwrap()
    }

    #[test]
    fn fixed_seed_pointer_replay_reproduces_classification_response_and_topology() {
        let (replay, genome, tuning) = deterministic_case();
        let first = run_case(&replay, &genome, tuning.clone());
        let second = run_case(&replay, &genome, tuning);
        assert_eq!(first, second);
        assert_eq!(first.final_mass_checksum, replay.expected.mass_checksum);
        assert!(first.response_count <= 1);
    }

    #[test]
    fn replay_input_contains_no_derived_future_state() {
        let (replay, _, _) = deterministic_case();
        let samples = serde_json::to_value(&replay.samples).unwrap();
        let encoded = serde_json::to_string(&samples).unwrap();
        for forbidden in [
            "gesture",
            "response",
            "component",
            "strain",
            "pressure",
            "future",
            "desktop",
        ] {
            assert!(!encoded.contains(forbidden));
        }
    }

    #[test]
    #[ignore = "explicitly regenerates the checked-in deterministic replay fixture"]
    fn write_pull_split_remerge_fixture() {
        let output = std::env::var_os("PET2_WRITE_POINTER_FIXTURE")
            .map(std::path::PathBuf::from)
            .expect("set PET2_WRITE_POINTER_FIXTURE to the destination JSON path");
        let genome = Genome::from_seed(42);
        let tuning = crate::production_liquid_tuning(
            crate::approved_production_liquid_tuning(genome.identity_seed)
                .sanitized()
                .unwrap(),
        );
        let mut body = ProceduralBody::generate(&genome).unwrap();
        body.apply_tuning_profile(tuning.clone()).unwrap();
        let snapshot = body.body_material_snapshot();
        let mut samples = Vec::with_capacity(1_800);
        for tick in 1..=1_800_u64 {
            let (position, down, hovered) = if tick <= 420 {
                let phase = (tick - 1) as f32 / 419.0;
                (Vec2::new(0.24 + phase * 0.58, 0.03), true, true)
            } else if tick == 421 {
                (Vec2::new(0.82, 0.03), false, true)
            } else {
                (Vec2::new(0.82, 0.03), false, false)
            };
            samples.push(PointerReplaySample {
                tick,
                position_body_local: position,
                down,
                hovered,
            });
        }
        let mut replay = PointerInteractionReplay {
            schema_version: POINTER_INTERACTION_REPLAY_SCHEMA_VERSION,
            base_commit: "6010c77447c9cb68fbe732e8ee8223626741575c".to_owned(),
            identity_seed: genome.identity_seed,
            tuning_schema_version: tuning.schema_version,
            tuning_revision: tuning.profile_revision,
            fixed_hz: 120,
            initial_body_snapshot: snapshot.clone(),
            samples,
            expected: PointerReplayExpectations {
                mass_checksum: snapshot.total_mass_bits_checksum,
                ..PointerReplayExpectations::default()
            },
        };
        let outcome = run_case(&replay, &genome, tuning.clone());
        replay.expected.component_timeline = outcome
            .component_timeline
            .iter()
            .map(|(tick, component_count)| ComponentTimelineCheckpoint {
                tick: *tick,
                component_count: *component_count,
            })
            .collect();
        replay.expected.detached_ticks = outcome.detached_ticks;
        replay.expected.remerge_ticks = outcome.remerge_ticks;
        replay.expected.selected_gesture = outcome.selected_gesture;
        replay.expected.response_reason = outcome.response_reason;
        replay.expected.response_variant = outcome.response_variant;
        replay.expected.final_state_hash = outcome.final_state_hash;
        replay.validate(tuning.pbf).unwrap();
        if let Some(parent) = output.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        let mut bytes = serde_json::to_vec_pretty(&replay).unwrap();
        bytes.push(b'\n');
        std::fs::write(output, bytes).unwrap();
    }
}
