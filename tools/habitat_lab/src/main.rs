use std::{env, error::Error, fmt::Write as _, fs, path::PathBuf, time::Instant};

use glam::Vec2;
use lifecore::{
    ActionId, BodyIntent, ExpressionState, LocomotionMode, PoseIntent, stable_hash_bytes,
};
use pet_ecology::{
    ActionSignature, EcologyBehaviorFrame, EcologyOutcome, EcologyState, EmbodiedEnvironmentFrame,
    EpisodeDirector, EpisodeGoal, EpisodePhase, MAX_OBJECT_SPEED, MorselProfile, NormalizedRect,
    ObjectCommand, ObjectId, ObjectKind, ObjectLifecycle, ObjectPhysicsConfig, RhythmSignature,
    WindowAffordance, WindowAffordanceFrame, WindowId, orb_is_inside_den_latch,
    step_den_attraction, step_object_with_windows,
};
use serde_json::{Value, json};

const LAB_HZ: f32 = 20.0;
const PHYSICS_STEPS: usize = 6;
const LAB_DESKTOP_ASPECT: f32 = 16.0 / 9.0;
const ALL_SCENARIOS: [&str; 18] = [
    "ecology_smoke",
    "orb_drag_throw",
    "orb_offer_user_ignores",
    "orb_offer_user_plays",
    "orb_intercept_miss_retry",
    "orb_window_bounce",
    "orb_trapped_help",
    "pet_window_squeeze_escape",
    "den_store_restart_retrieve",
    "focus_mode_return_home",
    "morsel_accept",
    "morsel_refuse",
    "saliency_shared_attention",
    "camouflage_readability",
    "teach_figure_eight",
    "skill_transfer_new_region",
    "rhythm_echo",
    "habitat_story_v1",
];

#[derive(Debug)]
struct Arguments {
    seed: u64,
    ticks: u64,
    speed: f32,
    scenario: String,
    brain_mode: String,
    focus_mode: bool,
    load: Option<PathBuf>,
    save: Option<PathBuf>,
    trace: Option<PathBuf>,
    screenshot: Option<PathBuf>,
}

impl Default for Arguments {
    fn default() -> Self {
        Self {
            seed: 42,
            ticks: 720,
            speed: 1.0,
            scenario: "ecology_smoke".into(),
            brain_mode: "morphic".into(),
            focus_mode: false,
            load: None,
            save: None,
            trace: None,
            screenshot: None,
        }
    }
}

impl Arguments {
    fn parse() -> Result<Self, Box<dyn Error>> {
        let mut parsed = Self::default();
        let mut arguments = env::args().skip(1).peekable();
        while let Some(argument) = arguments.next() {
            let value = |name: &str, arguments: &mut std::iter::Peekable<_>| {
                arguments
                    .next()
                    .ok_or_else(|| format!("{name} requires a value"))
            };
            match argument.as_str() {
                "--seed" => parsed.seed = value("--seed", &mut arguments)?.parse()?,
                "--ticks" | "--step" => {
                    parsed.ticks = value(argument.as_str(), &mut arguments)?.parse()?;
                }
                "--pause" => parsed.ticks = 0,
                "--speed" => parsed.speed = value("--speed", &mut arguments)?.parse()?,
                "--scenario" => parsed.scenario = value("--scenario", &mut arguments)?,
                "--brain-mode" => parsed.brain_mode = value("--brain-mode", &mut arguments)?,
                "--focus-mode" => parsed.focus_mode = true,
                "--load" => parsed.load = Some(value("--load", &mut arguments)?.into()),
                "--save" => parsed.save = Some(value("--save", &mut arguments)?.into()),
                "--trace" => parsed.trace = Some(value("--trace", &mut arguments)?.into()),
                "--screenshot" => {
                    parsed.screenshot = Some(value("--screenshot", &mut arguments)?.into());
                }
                "--help" | "-h" => {
                    print_help();
                    std::process::exit(0);
                }
                other => return Err(format!("unknown argument: {other}").into()),
            }
        }
        if ![
            "classic",
            "morphic",
            "fusion",
            "morph-shadow",
            "morph-fusion",
        ]
        .contains(&parsed.brain_mode.as_str())
        {
            return Err(format!("unsupported brain mode: {}", parsed.brain_mode).into());
        }
        if !ALL_SCENARIOS.contains(&parsed.scenario.as_str()) {
            return Err(format!("unknown deterministic scenario: {}", parsed.scenario).into());
        }
        if !parsed.speed.is_finite() || !(0.05..=64.0).contains(&parsed.speed) {
            return Err("--speed must be finite and between 0.05 and 64".into());
        }
        Ok(parsed)
    }
}

#[derive(Default)]
struct ScriptFlags {
    grab_tick: Option<u64>,
    throw_released: bool,
    restart_done: bool,
    trap_started: bool,
    help_seen: bool,
    trap_resolved: bool,
    morsel_spawned: bool,
    story_reload_done: bool,
}

struct Lab {
    state: EcologyState,
    director: EpisodeDirector,
    pet_position: Vec2,
    pet_velocity: Vec2,
    cursor: Vec2,
    selected_action: ActionId,
    focus_mode: bool,
    sleeping: bool,
    user_activity: f32,
    window_pressure: f32,
    window_escape_direction: Vec2,
    nearest_window_edge: Option<Vec2>,
    window_motion: f32,
    orb_trapped: bool,
    visual_target: Option<Vec2>,
    visual_hue: f32,
    visual_strength: f32,
    shared_attention: bool,
    click_rhythm: Option<RhythmSignature>,
    windows: WindowAffordanceFrame,
    timestamp: f64,
    transitions: Vec<Value>,
    reason_codes: Vec<String>,
    contacts: Vec<Value>,
    outcomes: Vec<String>,
    vocal_triggers: Vec<String>,
    motor_error: Option<f32>,
    object_trail: Vec<Vec2>,
    pet_trail: Vec<Vec2>,
    execution_path: Vec<Vec2>,
    last_goal_phase: Option<(EpisodeGoal, EpisodePhase)>,
    last_gaze_target: Option<Vec2>,
    last_scores: Vec<Value>,
    max_chromatic_blend: f32,
    max_camouflage_blend: f32,
    finite_and_bounded: bool,
    validation_error: Option<String>,
    flags: ScriptFlags,
    initial_taste_confidence: f32,
    initial_orb_familiarity: f32,
    initial_skill_competence: f32,
    episode_timings_us: Vec<f64>,
    object_physics_timings_us: Vec<f64>,
}

impl Lab {
    fn new(state: EcologyState, focus_mode: bool) -> Self {
        let orb_familiarity = state
            .objects
            .iter()
            .find(|object| object.kind == ObjectKind::Orb)
            .map_or(0.0, |object| object.familiarity);
        Self {
            pet_position: Vec2::splat(0.5),
            cursor: Vec2::new(0.72, 0.48),
            focus_mode,
            state,
            director: EpisodeDirector::default(),
            pet_velocity: Vec2::ZERO,
            selected_action: ActionId::IdleHover,
            sleeping: false,
            user_activity: 0.0,
            window_pressure: 0.0,
            window_escape_direction: Vec2::ZERO,
            nearest_window_edge: None,
            window_motion: 0.0,
            orb_trapped: false,
            visual_target: None,
            visual_hue: 0.0,
            visual_strength: 0.0,
            shared_attention: false,
            click_rhythm: None,
            windows: WindowAffordanceFrame::default(),
            timestamp: 25.0,
            transitions: Vec::new(),
            reason_codes: Vec::new(),
            contacts: Vec::new(),
            outcomes: Vec::new(),
            vocal_triggers: Vec::new(),
            motor_error: None,
            object_trail: Vec::new(),
            pet_trail: Vec::new(),
            execution_path: Vec::new(),
            last_goal_phase: None,
            last_gaze_target: None,
            last_scores: Vec::new(),
            max_chromatic_blend: 0.0,
            max_camouflage_blend: 0.0,
            finite_and_bounded: true,
            validation_error: None,
            flags: ScriptFlags::default(),
            initial_taste_confidence: 0.0,
            initial_orb_familiarity: orb_familiarity,
            initial_skill_competence: 0.0,
            episode_timings_us: Vec::new(),
            object_physics_timings_us: Vec::new(),
        }
    }

    fn orb(&self) -> &pet_ecology::WorldObject {
        self.state
            .objects
            .iter()
            .find(|object| object.kind == ObjectKind::Orb)
            .expect("canonical orb exists")
    }

    fn orb_mut(&mut self) -> &mut pet_ecology::WorldObject {
        self.state
            .objects
            .iter_mut()
            .find(|object| object.kind == ObjectKind::Orb)
            .expect("canonical orb exists")
    }

    fn active(&self) -> Option<(EpisodeGoal, EpisodePhase)> {
        self.director
            .active_episode()
            .map(|episode| (episode.goal, episode.phase))
    }

    fn tick(&mut self, tick: u64, dt: f32) {
        let brain_intent = BodyIntent {
            locomotion: LocomotionMode::Hover,
            target_position: self.pet_position,
            target_surface: None,
            desired_speed: 0.18,
            facing_direction: 1.0,
            gaze_target: Some(self.cursor),
            pose: PoseIntent::Neutral,
            expression: ExpressionState::default(),
            interaction_target: None,
        };
        let frame = EcologyBehaviorFrame {
            selected_action: self.selected_action,
            pet_position: self.pet_position,
            pet_velocity: self.pet_velocity,
            desktop_aspect: LAB_DESKTOP_ASPECT,
            cursor_position: self.cursor,
            pointer_down: false,
            user_activity: self.user_activity,
            user_available: if self.focus_mode { 0.0 } else { 1.0 },
            play_drive: 0.0,
            curiosity_drive: 0.0,
            autonomy_drive: 0.0,
            focus_mode: self.focus_mode,
            sleeping: self.sleeping,
            window_pressure: self.window_pressure,
            window_escape_direction: self.window_escape_direction,
            nearest_window_edge: self.nearest_window_edge,
            window_motion: self.window_motion,
            orb_trapped: self.orb_trapped,
            visual_target: self.visual_target,
            visual_hue: self.visual_hue,
            visual_strength: self.visual_strength,
            visual_colorfulness: self.visual_strength,
            visual_structure: self.visual_strength * 0.72,
            visual_surprise: self.visual_strength * 0.58,
            shared_attention: self.shared_attention,
            autonomous_play_ready: false,
            click_rhythm: self.click_rhythm,
            timestamp: self.timestamp,
        };
        let episode_started = Instant::now();
        let output = self.director.tick(&mut self.state, frame, brain_intent, dt);
        self.episode_timings_us
            .push(episode_started.elapsed().as_secs_f64() * 1_000_000.0);
        self.record_output(tick, &output);
        self.apply_commands(&output, dt);
        self.advance_pet(&output.body_intent, dt);
        let physics_started = Instant::now();
        self.advance_objects(dt);
        self.object_physics_timings_us
            .push(physics_started.elapsed().as_secs_f64() * 1_000_000.0);
        self.state.metabolism.advance(dt);
        self.timestamp += f64::from(dt);
        self.pet_trail.push(self.pet_position);
        self.object_trail.push(self.orb().position);
        if let Err(error) = self.state.validate() {
            self.finite_and_bounded = false;
            self.validation_error
                .get_or_insert_with(|| format!("tick {tick}: {error}"));
        }
        self.finite_and_bounded &= self.pet_position.is_finite()
            && self.pet_position.cmpge(Vec2::ZERO).all()
            && self.pet_position.cmple(Vec2::ONE).all();
    }

    fn record_output(&mut self, tick: u64, output: &pet_ecology::EcologyOutput) {
        let current = output.debug.active_goal.zip(output.debug.active_phase);
        if current != self.last_goal_phase {
            self.transitions.push(json!({
                "tick": tick,
                "goal": output.debug.active_goal.map(|goal| format!("{goal:?}")),
                "phase": output.debug.active_phase.map(|phase| format!("{phase:?}")),
                "reason": format!("{:?}", output.debug.selected_reason),
            }));
            self.last_goal_phase = current;
        }
        let reason = format!("{:?}", output.debug.selected_reason);
        if !self.reason_codes.contains(&reason) {
            self.reason_codes.push(reason);
        }
        self.last_scores = output.debug.scores[..output.debug.score_count]
            .iter()
            .map(|score| {
                json!({
                    "goal": score.goal.map(|goal| format!("{goal:?}")),
                    "score": score.score,
                    "eligible": score.eligible,
                    "reason": format!("{:?}", score.reason),
                })
            })
            .collect();
        self.last_gaze_target = output.body_intent.gaze_target;
        self.max_chromatic_blend = self
            .max_chromatic_blend
            .max(output.visual_context.chromatic_blend);
        self.max_camouflage_blend = self
            .max_camouflage_blend
            .max(output.visual_context.camouflage_blend);
        for outcome in output.outcomes.iter().take(output.outcome_count) {
            if let EcologyOutcome::SkillMotorError { error, .. } = outcome {
                self.motor_error = Some(*error);
            }
            self.outcomes.push(format!("{outcome:?}"));
        }
        if let Some(trigger) = output.vocal_trigger {
            let trigger = format!("{trigger:?}");
            if !self.vocal_triggers.contains(&trigger) {
                self.vocal_triggers.push(trigger);
            }
        }
        if matches!(
            output.debug.active_goal,
            Some(EpisodeGoal::PerformSkill | EpisodeGoal::PracticeSkill)
        ) {
            self.execution_path.push(output.body_intent.target_position);
        }
    }

    fn advance_pet(&mut self, intent: &BodyIntent, dt: f32) {
        let previous = self.pet_position;
        if !matches!(
            intent.locomotion,
            LocomotionMode::Hover | LocomotionMode::Sleep
        ) {
            let delta = intent.target_position - self.pet_position;
            let maximum_step = intent.desired_speed.clamp(0.0, 1.5) * dt * 0.32;
            self.pet_position += delta.normalize_or_zero() * delta.length().min(maximum_step);
            self.pet_position = self
                .pet_position
                .clamp(Vec2::splat(0.02), Vec2::splat(0.98));
        }
        self.pet_velocity = if dt > f32::EPSILON {
            (self.pet_position - previous) / dt
        } else {
            Vec2::ZERO
        };
    }

    fn apply_commands(&mut self, output: &pet_ecology::EcologyOutput, dt: f32) {
        for command in output
            .object_commands
            .iter()
            .copied()
            .take(output.object_command_count)
        {
            match command {
                ObjectCommand::None => {}
                ObjectCommand::ApplyImpulse { object_id, impulse } => {
                    self.clear_den_slot_references(object_id);
                    if let Some(object) = self
                        .state
                        .objects
                        .iter_mut()
                        .find(|object| object.id == object_id)
                    {
                        object.lifecycle = ObjectLifecycle::Free;
                        object.velocity =
                            clamp_velocity(object.velocity + impulse / object.mass.max(0.05));
                    }
                }
                ObjectCommand::MoveToward {
                    object_id,
                    target,
                    speed,
                } => {
                    self.clear_den_slot_references(object_id);
                    if let Some(object) = self
                        .state
                        .objects
                        .iter_mut()
                        .find(|object| object.id == object_id)
                    {
                        let alpha = 1.0 - (-speed.clamp(0.0, 12.0) * dt).exp();
                        let previous = object.position;
                        object.position = object
                            .position
                            .lerp(target.clamp(Vec2::ZERO, Vec2::ONE), alpha);
                        object.velocity =
                            clamp_velocity((object.position - previous) / dt.max(1e-4));
                        object.lifecycle = ObjectLifecycle::CarriedByPet;
                    }
                }
                ObjectCommand::Release {
                    object_id,
                    velocity,
                } => {
                    self.clear_den_slot_references(object_id);
                    if let Some(object) = self
                        .state
                        .objects
                        .iter_mut()
                        .find(|object| object.id == object_id)
                    {
                        object.lifecycle = ObjectLifecycle::Free;
                        object.velocity = clamp_velocity(velocity);
                    }
                }
                ObjectCommand::Store { object_id, slot } if usize::from(slot) < 3 => {
                    let Some(object_index) = self
                        .state
                        .objects
                        .iter()
                        .position(|object| object.id == object_id)
                    else {
                        continue;
                    };
                    let object = &self.state.objects[object_index];
                    let config = ObjectPhysicsConfig {
                        desktop_aspect: LAB_DESKTOP_ASPECT,
                        ..ObjectPhysicsConfig::default()
                    };
                    if object.lifecycle != ObjectLifecycle::CarriedByPet
                        || !orb_is_inside_den_latch(object, self.state.den.anchor, config)
                    {
                        continue;
                    }
                    self.clear_den_slot_references(object_id);
                    let object = &mut self.state.objects[object_index];
                    object.lifecycle = ObjectLifecycle::Free;
                    object.home_slot = Some(slot);
                    object.velocity = Vec2::ZERO;
                }
                ObjectCommand::Retrieve {
                    object_id,
                    target: _,
                } => {
                    self.clear_den_slot_references(object_id);
                    if let Some(object) = self
                        .state
                        .objects
                        .iter_mut()
                        .find(|object| object.id == object_id)
                    {
                        object.lifecycle = ObjectLifecycle::CarriedByPet;
                        object.home_slot = None;
                        object.velocity = Vec2::ZERO;
                    }
                }
                ObjectCommand::Consume { object_id } => {
                    if let Some(index) = self.state.objects.iter().position(|object| {
                        object.id == object_id && object.kind == ObjectKind::Morsel
                    }) {
                        self.state.objects.remove(index);
                        for slot in &mut self.state.den.slots {
                            if *slot == Some(object_id) {
                                *slot = None;
                            }
                        }
                    }
                }
                ObjectCommand::Store { .. } => {}
            }
        }
    }

    fn clear_den_slot_references(&mut self, object_id: ObjectId) {
        for slot in &mut self.state.den.slots {
            if *slot == Some(object_id) {
                *slot = None;
            }
        }
    }

    fn advance_objects(&mut self, dt: f32) {
        let config = ObjectPhysicsConfig {
            desktop_aspect: LAB_DESKTOP_ASPECT,
            ..ObjectPhysicsConfig::default()
        };
        let sub_dt = dt / PHYSICS_STEPS as f32;
        for _ in 0..PHYSICS_STEPS {
            let den_anchor = self.state.den.anchor;
            let orb_slot = self
                .state
                .objects
                .iter()
                .find(|object| object.kind == ObjectKind::Orb)
                .and_then(|orb| {
                    orb.home_slot
                        .filter(|slot| self.state.den.slots[usize::from(*slot)].is_none())
                        .or_else(|| {
                            self.state
                                .den
                                .slots
                                .iter()
                                .position(Option::is_none)
                                .map(|slot| slot as u8)
                        })
                });
            let mut captured = None;
            for object in &mut self.state.objects {
                let mut environment = EmbodiedEnvironmentFrame::default();
                step_object_with_windows(object, config, &self.windows, sub_dt, &mut environment);
                if step_den_attraction(object, den_anchor, config, sub_dt)
                    && let Some(slot) = orb_slot
                {
                    object.lifecycle = ObjectLifecycle::StoredInDen;
                    object.home_slot = Some(slot);
                    captured = Some((object.id, slot));
                }
                for contact in environment.contacts.iter().take(environment.contact_count) {
                    if self.contacts.len() < 96 {
                        self.contacts.push(json!({
                            "source": format!("{:?}", contact.source),
                            "point": contact.point_world.to_array(),
                            "normal": contact.normal_world.to_array(),
                            "intensity": contact.intensity,
                        }));
                    }
                }
            }
            if let Some((object_id, slot)) = captured {
                self.clear_den_slot_references(object_id);
                self.state.den.slots[usize::from(slot)] = Some(object_id);
            }
        }
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let arguments = Arguments::parse()?;
    let state = if let Some(path) = &arguments.load {
        EcologyState::restore(serde_json::from_slice(&fs::read(path)?)?)?
    } else {
        EcologyState::new(arguments.seed)
    };
    let mut lab = Lab::new(state, arguments.focus_mode);
    setup_scenario(&mut lab, &arguments.scenario)?;
    let dt = (1.0 / LAB_HZ * arguments.speed).min(0.25);
    for tick in 0..arguments.ticks {
        script_scenario(&mut lab, &arguments.scenario, tick)?;
        lab.tick(tick, dt);
        post_scenario(&mut lab, &arguments.scenario)?;
    }
    lab.state.validate()?;
    let encoded = serde_json::to_vec_pretty(&lab.state)?;
    let summary = summary(&lab, &arguments, &encoded);
    let summary_json = serde_json::to_string_pretty(&summary)?;
    println!("{summary_json}");
    if let Some(path) = &arguments.save {
        fs::write(path, &encoded)?;
    }
    if let Some(path) = &arguments.trace {
        fs::write(path, format!("{summary_json}\n"))?;
    }
    if let Some(path) = &arguments.screenshot {
        fs::write(path, habitat_svg(&lab, &arguments))?;
    }
    Ok(())
}

fn setup_scenario(lab: &mut Lab, scenario: &str) -> Result<(), Box<dyn Error>> {
    let den_anchor = lab.state.den.anchor;
    match scenario {
        "ecology_smoke" => {
            let orb = lab.orb_mut();
            orb.position = Vec2::new(0.42, 0.47);
            orb.velocity = Vec2::new(0.58, -0.31);
            orb.lifecycle = ObjectLifecycle::Free;
        }
        "orb_drag_throw" => {
            let orb = lab.orb_mut();
            orb.position = Vec2::new(0.22, 0.62);
            orb.velocity = Vec2::ZERO;
            orb.lifecycle = ObjectLifecycle::GrabbedByUser;
        }
        "orb_offer_user_ignores" | "orb_offer_user_plays" => {
            lab.pet_position = (lab.orb().position + Vec2::new(0.08, 0.02))
                .clamp(Vec2::splat(0.02), Vec2::splat(0.98));
            lab.selected_action = ActionId::BringProceduralOrb;
            lab.user_activity = 0.45;
        }
        "orb_intercept_miss_retry" => {
            lab.pet_position = Vec2::new(0.16, 0.78);
            let orb = lab.orb_mut();
            orb.position = Vec2::new(0.38, 0.24);
            orb.velocity = Vec2::new(0.82, 0.16);
            orb.lifecycle = ObjectLifecycle::Free;
            lab.selected_action = ActionId::PlayCursorChase;
            lab.user_activity = 0.72;
        }
        "orb_window_bounce" => {
            let orb = lab.orb_mut();
            orb.position = Vec2::new(0.18, 0.42);
            orb.velocity = Vec2::new(1.35, 0.03);
            orb.lifecycle = ObjectLifecycle::Free;
            lab.windows = moving_window(
                Vec2::new(0.48, 0.18),
                Vec2::new(0.54, 0.76),
                Vec2::new(-0.12, 0.0),
                0.0,
            );
        }
        "orb_trapped_help" => {
            lab.pet_position = Vec2::new(0.44, 0.52);
            let orb = lab.orb_mut();
            orb.position = Vec2::new(0.54, 0.52);
            orb.velocity = Vec2::ZERO;
            orb.lifecycle = ObjectLifecycle::Free;
            lab.orb_trapped = true;
            lab.user_activity = 0.55;
            lab.window_escape_direction = Vec2::X;
        }
        "pet_window_squeeze_escape" => {
            lab.pet_position = Vec2::new(0.46, 0.5);
            lab.window_pressure = 0.78;
            lab.window_escape_direction = Vec2::X;
            lab.nearest_window_edge = Some(Vec2::new(0.43, 0.5));
            lab.window_motion = 0.84;
        }
        "den_store_restart_retrieve" => {
            lab.pet_position = (lab.orb().position + Vec2::new(0.03, 0.0))
                .clamp(Vec2::splat(0.02), Vec2::splat(0.98));
            lab.selected_action = ActionId::BringProceduralOrb;
            lab.user_activity = 0.20;
        }
        "focus_mode_return_home" => {
            lab.pet_position = Vec2::new(1.0 - den_anchor.x, 0.22);
            lab.focus_mode = true;
            lab.selected_action = ActionId::BringProceduralOrb;
        }
        "morsel_accept" => {
            lab.state.metabolism.reserve = pet_ecology::METABOLIC_RESERVE_FLOOR;
            lab.state.metabolism.satiation = 0.0;
            spawn_fixture_morsel(lab, Vec2::new(0.58, 0.50), 0.12)?;
            lab.pet_position = Vec2::new(0.48, 0.50);
        }
        "morsel_refuse" => {
            lab.state.metabolism.satiation = 1.0;
            lab.state.taste.hue_bins[1] = -1.0;
            lab.state.taste.confidence = 1.0;
            spawn_fixture_morsel(lab, Vec2::new(0.58, 0.50), 0.12)?;
            lab.pet_position = Vec2::new(0.48, 0.50);
        }
        "saliency_shared_attention" => {
            lab.visual_target = Some(Vec2::new(0.78, 0.22));
            lab.visual_hue = 0.58;
            lab.visual_strength = 0.92;
            lab.shared_attention = true;
            lab.cursor = Vec2::new(0.78, 0.22);
        }
        "camouflage_readability" => {
            lab.visual_target = Some(Vec2::new(0.31, 0.66));
            lab.visual_hue = 0.84;
            lab.visual_strength = 0.96;
            lab.selected_action = ActionId::HideAndSeek;
        }
        "teach_figure_eight" | "skill_transfer_new_region" => {
            let signature = figure_eight_signature()?;
            let _ = lab.state.skills.observe(signature, 0.0)?;
            lab.initial_skill_competence = lab.state.skills.skills[0].competence;
            lab.selected_action = ActionId::HappyDisplay;
            lab.pet_position = if scenario == "skill_transfer_new_region" {
                Vec2::new(0.76, 0.28)
            } else {
                Vec2::splat(0.5)
            };
        }
        "rhythm_echo" => {
            lab.click_rhythm = RhythmSignature::from_onsets(&[0.0, 0.20, 0.50, 0.70, 1.10]);
            lab.selected_action = ActionId::MimicClickRhythm;
            lab.user_activity = 0.65;
        }
        "habitat_story_v1" => {
            let orb_id = lab.orb().id;
            let anchor = lab.state.den.anchor;
            lab.state.den.slots[0] = Some(orb_id);
            let orb = lab.orb_mut();
            orb.lifecycle = ObjectLifecycle::StoredInDen;
            orb.home_slot = Some(0);
            orb.position = anchor;
            orb.velocity = Vec2::ZERO;
            lab.pet_position = anchor;
            lab.cursor = Vec2::new(0.68, 0.44);
            lab.user_activity = 0.55;
            lab.selected_action = ActionId::BringProceduralOrb;
        }
        _ => return Err(format!("unknown deterministic scenario: {scenario}").into()),
    }
    lab.initial_taste_confidence = lab.state.taste.confidence;
    Ok(())
}

fn script_scenario(lab: &mut Lab, scenario: &str, tick: u64) -> Result<(), Box<dyn Error>> {
    match scenario {
        "orb_drag_throw" => {
            if tick < 14 {
                let alpha = tick as f32 / 13.0;
                let position = Vec2::new(0.22, 0.62).lerp(Vec2::new(0.67, 0.30), alpha);
                let orb = lab.orb_mut();
                orb.position = position;
                orb.velocity = Vec2::new(0.90, -0.64);
                orb.lifecycle = ObjectLifecycle::GrabbedByUser;
            } else if tick == 14 {
                let orb = lab.orb_mut();
                orb.lifecycle = ObjectLifecycle::Free;
                orb.velocity = Vec2::new(1.62, -0.82);
                lab.flags.throw_released = true;
            }
        }
        "orb_offer_user_plays" => scripted_offer_play(lab, tick),
        "pet_window_squeeze_escape" if tick >= 28 => {
            lab.window_pressure = 0.0;
            lab.window_motion = 0.0;
        }
        "den_store_restart_retrieve" if lab.flags.restart_done => {
            lab.selected_action = ActionId::BringProceduralOrb;
        }
        "saliency_shared_attention" if tick >= 55 => {
            lab.shared_attention = false;
            lab.visual_strength = 0.0;
        }
        "habitat_story_v1" => scripted_habitat_story(lab, tick)?,
        _ => {}
    }
    Ok(())
}

fn post_scenario(lab: &mut Lab, scenario: &str) -> Result<(), Box<dyn Error>> {
    if scenario == "den_store_restart_retrieve"
        && !lab.flags.restart_done
        && lab.orb().lifecycle == ObjectLifecycle::StoredInDen
    {
        let encoded = serde_json::to_vec(&lab.state)?;
        lab.state = EcologyState::restore(serde_json::from_slice(&encoded)?)?;
        lab.director = EpisodeDirector::default();
        lab.flags.restart_done = true;
        lab.outcomes.push("SaveReloadCompleted".into());
    }
    if scenario == "habitat_story_v1" {
        if lab
            .vocal_triggers
            .iter()
            .any(|trigger| trigger == "NeedHelp")
        {
            lab.flags.help_seen = true;
        }
        if lab.flags.help_seen
            && lab
                .outcomes
                .iter()
                .any(|outcome| outcome == "EpisodeCompleted(RetrieveOrb)")
        {
            lab.flags.trap_resolved = true;
        }
    }
    Ok(())
}

fn scripted_offer_play(lab: &mut Lab, tick: u64) {
    if lab.flags.throw_released {
        lab.selected_action = ActionId::PlayCursorChase;
        return;
    }
    if let Some(grab_tick) = lab.flags.grab_tick {
        if tick > grab_tick {
            let orb = lab.orb_mut();
            orb.lifecycle = ObjectLifecycle::Free;
            orb.position = Vec2::new(0.76, 0.26);
            orb.velocity = Vec2::new(0.86, 0.18);
            lab.flags.throw_released = true;
            lab.selected_action = ActionId::PlayCursorChase;
        }
    } else if lab.active() == Some((EpisodeGoal::OfferOrb, EpisodePhase::WaitForUser)) {
        let cursor = lab.cursor;
        let orb = lab.orb_mut();
        orb.lifecycle = ObjectLifecycle::GrabbedByUser;
        orb.position = cursor;
        orb.velocity = Vec2::ZERO;
        lab.flags.grab_tick = Some(tick);
    }
}

fn scripted_habitat_story(lab: &mut Lab, tick: u64) -> Result<(), Box<dyn Error>> {
    lab.selected_action = ActionId::BringProceduralOrb;
    scripted_offer_play(lab, tick);
    let catch_finished = lab
        .outcomes
        .iter()
        .any(|outcome| outcome == "EpisodeCompleted(InterceptOrb)");
    if lab.flags.throw_released && !catch_finished {
        lab.selected_action = ActionId::PlayCursorChase;
    } else if catch_finished {
        lab.selected_action = ActionId::BringProceduralOrb;
    }
    if catch_finished && !lab.flags.trap_started {
        let orb = lab.orb_mut();
        orb.position = Vec2::new(0.55, 0.36);
        orb.velocity = Vec2::ZERO;
        orb.lifecycle = ObjectLifecycle::Free;
        lab.orb_trapped = true;
        lab.window_escape_direction = Vec2::new(-1.0, 0.0);
        lab.flags.trap_started = true;
    }
    if lab.flags.help_seen {
        lab.orb_trapped = false;
    }
    if lab.flags.trap_resolved && !lab.flags.morsel_spawned {
        lab.state.metabolism.reserve = pet_ecology::METABOLIC_RESERVE_FLOOR;
        lab.state.metabolism.satiation = 0.0;
        spawn_fixture_morsel(lab, Vec2::new(0.64, 0.48), 0.58)?;
        lab.flags.morsel_spawned = true;
    }
    let food_finished = lab
        .outcomes
        .iter()
        .any(|outcome| outcome.starts_with("MorselConsumed("))
        || lab
            .vocal_triggers
            .iter()
            .any(|trigger| trigger == "FoodRefused");
    if food_finished
        && lab.orb().lifecycle == ObjectLifecycle::StoredInDen
        && !lab.flags.story_reload_done
    {
        let encoded = serde_json::to_vec(&lab.state)?;
        lab.state = EcologyState::restore(serde_json::from_slice(&encoded)?)?;
        lab.director = EpisodeDirector::default();
        lab.flags.story_reload_done = true;
        lab.focus_mode = true;
        lab.outcomes.push("SaveReloadCompleted".into());
    }
    Ok(())
}

fn spawn_fixture_morsel(lab: &mut Lab, position: Vec2, hue: f32) -> Result<(), Box<dyn Error>> {
    let profile = MorselProfile {
        hue,
        saturation: 0.82,
        value: 0.88,
        warmth: 0.62,
        pulse_rate: 0.44,
        stimulation: 0.56,
        cohesion_bias: 0.68,
        novelty: 0.86,
    };
    lab.state
        .spawn_morsel(position, profile, lab.timestamp)
        .ok_or_else(|| "could not spawn deterministic morsel".into())
        .map(|_| ())
}

fn figure_eight_signature() -> Result<ActionSignature, Box<dyn Error>> {
    let points = (0..96)
        .map(|index| {
            let phase = index as f32 / 95.0 * std::f32::consts::TAU;
            Vec2::splat(0.5) + Vec2::new(phase.sin() * 0.20, phase.sin() * phase.cos() * 0.16)
        })
        .collect::<Vec<_>>();
    ActionSignature::from_trace(&points, 2.4)
        .ok_or_else(|| "figure-eight fixture was invalid".into())
}

fn moving_window(
    minimum: Vec2,
    maximum: Vec2,
    velocity: Vec2,
    pressure: f32,
) -> WindowAffordanceFrame {
    let mut frame = WindowAffordanceFrame::default();
    frame.push(WindowAffordance {
        id: WindowId(1),
        z_order: 0,
        bounds: NormalizedRect { minimum, maximum },
        velocity,
        nearest_edge_point: Vec2::new(minimum.x, (minimum.y + maximum.y) * 0.5),
        nearest_edge_normal: Vec2::NEG_X,
        overlap_pressure: pressure,
        popup_pressure: 0.0,
        motion_energy: (velocity.length() / 2.0).clamp(0.0, 1.0),
        is_visible: true,
    });
    frame.finish();
    frame
}

fn clamp_velocity(velocity: Vec2) -> Vec2 {
    if !velocity.is_finite() {
        Vec2::ZERO
    } else if velocity.length_squared() > MAX_OBJECT_SPEED * MAX_OBJECT_SPEED {
        velocity.normalize_or_zero() * MAX_OBJECT_SPEED
    } else {
        velocity
    }
}

fn summary(lab: &Lab, arguments: &Arguments, encoded: &[u8]) -> Value {
    let final_skill_competence = lab
        .state
        .skills
        .skills
        .first()
        .map_or(0.0, |skill| skill.competence);
    let passed = scenario_passed(lab, &arguments.scenario);
    let episode_timing = timing_summary(&lab.episode_timings_us);
    let physics_timing = timing_summary(&lab.object_physics_timings_us);
    json!({
        "scenario": arguments.scenario,
        "seed": arguments.seed,
        "ticks": arguments.ticks,
        "speed": arguments.speed,
        "brain_mode": arguments.brain_mode,
        "focus_mode": lab.focus_mode,
        "ecology_schema": lab.state.schema_version,
        "active_goal": lab.active().map(|active| format!("{:?}", active.0)),
        "phase": lab.active().map(|active| format!("{:?}", active.1)),
        "episode_transitions": lab.transitions,
        "reason_codes": lab.reason_codes,
        "candidate_scores": lab.last_scores,
        "contacts": lab.contacts,
        "outcomes": lab.outcomes,
        "vocal_triggers": lab.vocal_triggers,
        "motor_error": lab.motor_error,
        "measurements_us": {
            "episode": episode_timing,
            "object_physics": physics_timing,
        },
        "learning_deltas": {
            "taste_confidence": lab.state.taste.confidence - lab.initial_taste_confidence,
            "orb_familiarity": lab.orb().familiarity - lab.initial_orb_familiarity,
            "skill_competence": final_skill_competence - lab.initial_skill_competence,
        },
        "visual": {
            "gaze_target": lab.last_gaze_target.map(|target| target.to_array()),
            "chromatic_blend_max": lab.max_chromatic_blend,
            "camouflage_blend_max": lab.max_camouflage_blend,
            "pattern_readability_floor": 0.18,
        },
        "object_positions": lab.state.objects.iter().map(|object| object.position.to_array()).collect::<Vec<_>>(),
        "object_velocities": lab.state.objects.iter().map(|object| object.velocity.to_array()).collect::<Vec<_>>(),
        "object_lifecycles": lab.state.objects.iter().map(|object| format!("{:?}", object.lifecycle)).collect::<Vec<_>>(),
        "hashes": {
            "ecology_state": stable_hash_bytes(encoded),
            "transition_trace": stable_hash_bytes(serde_json::to_string(&lab.transitions).unwrap_or_default().as_bytes()),
        },
        "privacy_capabilities": {
            "visual_reduction": "16x9_features_only",
            "window_reduction": "geometry_velocity_pressure_only",
            "pixels": "not_collected",
            "titles": "not_collected",
            "typing": "not_collected",
            "os_object_ids": "not_collected",
            "microphone_audio": "not_collected",
        },
        "finite_and_bounded": lab.finite_and_bounded,
        "validation_error": lab.validation_error,
        "verdict": if passed { "pass" } else { "incomplete" },
    })
}

fn timing_summary(samples: &[f64]) -> Value {
    if samples.is_empty() {
        return json!({ "p50": 0.0, "p95": 0.0, "max": 0.0 });
    }
    let mut sorted = samples.to_vec();
    sorted.sort_by(f64::total_cmp);
    let at = |fraction: f64| {
        let index = ((sorted.len() - 1) as f64 * fraction).round() as usize;
        sorted[index]
    };
    json!({
        "p50": at(0.50),
        "p95": at(0.95),
        "max": sorted[sorted.len() - 1],
    })
}

fn scenario_passed(lab: &Lab, scenario: &str) -> bool {
    let has_outcome = |needle: &str| lab.outcomes.iter().any(|value| value.contains(needle));
    let has_vocal = |needle: &str| lab.vocal_triggers.iter().any(|value| value == needle);
    let has_goal = |needle: &str| {
        lab.transitions.iter().any(|transition| {
            transition
                .get("goal")
                .and_then(Value::as_str)
                .is_some_and(|goal| goal == needle)
        })
    };
    let scenario_result = match scenario {
        "ecology_smoke" => true,
        "orb_drag_throw" => lab.flags.throw_released && lab.object_trail.len() > 20,
        "orb_offer_user_ignores" => has_goal("CarryOrbHome") || has_outcome("ObjectStored"),
        "orb_offer_user_plays" => {
            has_outcome("EpisodeCompleted(OfferOrb)") && has_goal("InterceptOrb")
        }
        "orb_intercept_miss_retry" => {
            has_vocal("MissAndRetry") && (has_vocal("CatchSuccess") || has_goal("RetrieveOrb"))
        }
        "orb_window_bounce" => !lab.contacts.is_empty(),
        "orb_trapped_help" => has_vocal("NeedHelp"),
        "pet_window_squeeze_escape" => {
            has_goal("EscapePressure") && has_goal("RecoverAfterPressure")
        }
        "den_store_restart_retrieve" => {
            lab.flags.restart_done && has_outcome("EpisodeCompleted(RetrieveOrb)")
        }
        "focus_mode_return_home" => {
            has_outcome("EpisodeCompleted(ReturnHome)") && !has_goal("OfferOrb")
        }
        "morsel_accept" => has_outcome("MorselConsumed"),
        "morsel_refuse" => has_vocal("FoodRefused") && lab.state.objects.len() >= 2,
        "saliency_shared_attention" => {
            has_goal("SharedAttention") && lab.last_gaze_target.is_some()
        }
        "camouflage_readability" => {
            has_goal("Camouflage")
                && (0.0..=0.55).contains(&lab.max_camouflage_blend)
                && lab.max_camouflage_blend > 0.0
        }
        "teach_figure_eight" => !lab.state.skills.skills.is_empty(),
        "skill_transfer_new_region" => {
            has_goal("PerformSkill") && lab.motor_error.is_some() && lab.execution_path.len() > 8
        }
        "rhythm_echo" => has_goal("RhythmEcho") && has_vocal("RhythmEcho"),
        "habitat_story_v1" => {
            has_goal("OfferOrb")
                && has_goal("InterceptOrb")
                && (has_vocal("MissAndRetry") || has_vocal("CatchSuccess"))
                && has_vocal("NeedHelp")
                && (has_outcome("MorselConsumed") || has_vocal("FoodRefused"))
                && lab.flags.story_reload_done
        }
        _ => false,
    };
    lab.finite_and_bounded && scenario_result
}

fn habitat_svg(lab: &Lab, arguments: &Arguments) -> String {
    const WIDTH: f32 = 1_280.0;
    const HEIGHT: f32 = 720.0;
    let mut svg = String::from(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="1280" height="720" viewBox="0 0 1280 720">
<rect width="1280" height="720" fill="#10131b"/>
"##,
    );
    for row in 0..9 {
        for column in 0..16 {
            let energy = ((column * 13 + row * 7 + arguments.seed as usize) % 17) as f32 / 16.0;
            let opacity = 0.025 + energy * 0.075;
            let _ = writeln!(
                svg,
                r##"<rect x="{}" y="{}" width="80" height="80" fill="#7bdfff" opacity="{opacity:.3}"/>"##,
                column * 80,
                row * 80
            );
        }
    }
    for window in lab.windows.as_slice() {
        let minimum = window.bounds.minimum * Vec2::new(WIDTH, HEIGHT);
        let size = (window.bounds.maximum - window.bounds.minimum) * Vec2::new(WIDTH, HEIGHT);
        let _ = writeln!(
            svg,
            r##"<rect x="{:.1}" y="{:.1}" width="{:.1}" height="{:.1}" rx="10" fill="#252a38" stroke="#75809c" stroke-width="3"/><line x1="{:.1}" y1="{:.1}" x2="{:.1}" y2="{:.1}" stroke="#ffcf70" stroke-width="4"/>"##,
            minimum.x,
            minimum.y,
            size.x,
            size.y,
            minimum.x + size.x * 0.5,
            minimum.y + size.y * 0.5,
            minimum.x + size.x * 0.5 + window.velocity.x * 90.0,
            minimum.y + size.y * 0.5 + window.velocity.y * 90.0,
        );
    }
    let safe_gap = lab.orb().radius_px_at_reference * HEIGHT / 1_152.0;
    let _ = writeln!(
        svg,
        r##"<rect x="{safe_gap:.1}" y="{safe_gap:.1}" width="{:.1}" height="{:.1}" fill="none" stroke="#9ca9c2" stroke-width="1" stroke-dasharray="5 9" opacity="0.45"/>"##,
        WIDTH - safe_gap * 2.0,
        HEIGHT - safe_gap * 2.0,
    );
    svg.push_str(&trail_svg(&lab.object_trail, "#65ddff", WIDTH, HEIGHT));
    svg.push_str(&trail_svg(&lab.pet_trail, "#d6a8ff", WIDTH, HEIGHT));
    svg.push_str(&trail_svg(&lab.execution_path, "#ffcf70", WIDTH, HEIGHT));
    for contact in lab.contacts.iter().rev().take(12) {
        let Some(point) = contact.get("point").and_then(Value::as_array) else {
            continue;
        };
        let Some(normal) = contact.get("normal").and_then(Value::as_array) else {
            continue;
        };
        let (Some(px), Some(py), Some(nx), Some(ny)) = (
            point.first().and_then(Value::as_f64),
            point.get(1).and_then(Value::as_f64),
            normal.first().and_then(Value::as_f64),
            normal.get(1).and_then(Value::as_f64),
        ) else {
            continue;
        };
        let x = px as f32 * WIDTH;
        let y = py as f32 * HEIGHT;
        let _ = writeln!(
            svg,
            r##"<line x1="{x:.1}" y1="{y:.1}" x2="{:.1}" y2="{:.1}" stroke="#ff8d70" stroke-width="3"/>"##,
            x + nx as f32 * 34.0,
            y + ny as f32 * 34.0,
        );
    }
    let den = lab.state.den.anchor * Vec2::new(WIDTH, HEIGHT);
    let _ = writeln!(
        svg,
        r##"<path d="M {x:.1} {y:.1} q -72 -55 -92 8 q 28 72 96 38" fill="#342b55" stroke="#b39cff" stroke-width="5" opacity="0.92"/>"##,
        x = den.x,
        y = den.y
    );
    for object in &lab.state.objects {
        let point = object.position * Vec2::new(WIDTH, HEIGHT);
        let radius = object.radius_px_at_reference * HEIGHT / 1_152.0;
        let fill = if object.kind == ObjectKind::Orb {
            "#65ddff"
        } else {
            "#ffcf70"
        };
        let _ = writeln!(
            svg,
            r##"<circle cx="{:.1}" cy="{:.1}" r="{radius:.1}" fill="{fill}" stroke="#eaffff" stroke-width="4"/>"##,
            point.x, point.y,
        );
    }
    let pet = lab.pet_position * Vec2::new(WIDTH, HEIGHT);
    let _ = writeln!(
        svg,
        r##"<ellipse cx="{:.1}" cy="{:.1}" rx="58" ry="72" fill="#c996ff" stroke="#f2e6ff" stroke-width="5"/><circle cx="{:.1}" cy="{:.1}" r="9" fill="#111522"/><circle cx="{:.1}" cy="{:.1}" r="9" fill="#111522"/>"##,
        pet.x,
        pet.y,
        pet.x - 20.0,
        pet.y - 10.0,
        pet.x + 20.0,
        pet.y - 10.0,
    );
    if let Some(gaze) = lab.last_gaze_target {
        let gaze = gaze * Vec2::new(WIDTH, HEIGHT);
        let _ = writeln!(
            svg,
            r##"<line x1="{:.1}" y1="{:.1}" x2="{:.1}" y2="{:.1}" stroke="#f4f7ff" stroke-width="2" stroke-dasharray="8 8"/>"##,
            pet.x, pet.y, gaze.x, gaze.y,
        );
    }
    let active = lab.active().map_or_else(
        || "idle".into(),
        |value| format!("{:?} / {:?}", value.0, value.1),
    );
    let _ = writeln!(
        svg,
        r##"<rect x="18" y="14" width="760" height="112" rx="12" fill="#090b11" opacity="0.88"/><text x="32" y="45" fill="#f4f7ff" font-family="Segoe UI, sans-serif" font-size="23">Habitat Lab · {} · {} ticks</text><text x="32" y="78" fill="#b9c5dc" font-family="Segoe UI, sans-serif" font-size="18">{} · contacts {} · outcomes {}</text><text x="32" y="105" fill="#80d9b7" font-family="Segoe UI, sans-serif" font-size="15">privacy: 16x9 features · no pixels/text/audio/native ids</text>"##,
        arguments.scenario,
        arguments.ticks,
        active,
        lab.contacts.len(),
        lab.outcomes.len(),
    );
    let mut scores = lab.last_scores.iter().collect::<Vec<_>>();
    scores.sort_by(|left, right| {
        let left = left.get("score").and_then(Value::as_f64).unwrap_or(0.0);
        let right = right.get("score").and_then(Value::as_f64).unwrap_or(0.0);
        right.total_cmp(&left)
    });
    svg.push_str(
        r##"<rect x="798" y="14" width="464" height="174" rx="12" fill="#090b11" opacity="0.88"/><text x="816" y="40" fill="#f4f7ff" font-family="Segoe UI, sans-serif" font-size="17">Decision evidence</text>"##,
    );
    for (index, score) in scores.into_iter().take(4).enumerate() {
        let goal = score.get("goal").and_then(Value::as_str).unwrap_or("none");
        let value = score.get("score").and_then(Value::as_f64).unwrap_or(0.0);
        let eligible = score
            .get("eligible")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let marker = if eligible { "●" } else { "○" };
        let _ = writeln!(
            svg,
            r##"<text x="816" y="{}" fill="#b9c5dc" font-family="Segoe UI, sans-serif" font-size="14">{} {} · {:.3}</text>"##,
            66 + index * 24,
            marker,
            svg_text(goal),
            value,
        );
    }
    let timeline = lab.transitions.iter().rev().take(3).collect::<Vec<_>>();
    for (index, transition) in timeline.into_iter().rev().enumerate() {
        let tick = transition.get("tick").and_then(Value::as_u64).unwrap_or(0);
        let goal = transition
            .get("goal")
            .and_then(Value::as_str)
            .unwrap_or("idle");
        let phase = transition
            .get("phase")
            .and_then(Value::as_str)
            .unwrap_or("none");
        let _ = writeln!(
            svg,
            r##"<text x="1010" y="{}" fill="#8fa0be" font-family="Segoe UI, sans-serif" font-size="12">t{} {} / {}</text>"##,
            66 + index * 24,
            tick,
            svg_text(goal),
            svg_text(phase),
        );
    }
    svg.push_str("</svg>\n");
    svg
}

fn svg_text(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn trail_svg(points: &[Vec2], color: &str, width: f32, height: f32) -> String {
    let points = points
        .iter()
        .step_by((points.len() / 96).max(1))
        .map(|point| format!("{:.1},{:.1}", point.x * width, point.y * height))
        .collect::<Vec<_>>()
        .join(" ");
    format!(
        r##"<polyline points="{points}" fill="none" stroke="{color}" stroke-width="3" opacity="0.55"/>"##
    )
}

fn print_help() {
    println!(
        "Habitat Lab\n\
         --scenario {}\n\
         --seed N --ticks N|--step N|--pause --speed X\n\
         --brain-mode classic|morphic|fusion|morph-shadow|morph-fusion --focus-mode\n\
         --load ecology.json --save ecology.json --trace trace.json --screenshot habitat.svg",
        ALL_SCENARIOS.join("|")
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(name: &str, ticks: u64) -> Lab {
        let mut lab = Lab::new(EcologyState::new(42), false);
        setup_scenario(&mut lab, name).unwrap();
        for tick in 0..ticks {
            script_scenario(&mut lab, name, tick).unwrap();
            lab.tick(tick, 1.0 / LAB_HZ);
            post_scenario(&mut lab, name).unwrap();
        }
        lab
    }

    #[test]
    fn every_named_scenario_is_deterministic_finite_and_bounded() {
        for scenario in ALL_SCENARIOS {
            let ticks = if scenario == "habitat_story_v1" {
                1_600
            } else {
                720
            };
            let first = run(scenario, ticks);
            let second = run(scenario, ticks);
            assert!(
                first.finite_and_bounded,
                "{scenario}: {:?}; den={:?}; objects={:?}",
                first.validation_error,
                first.state.den.slots,
                first
                    .state
                    .objects
                    .iter()
                    .map(|object| (object.id, object.kind, object.lifecycle))
                    .collect::<Vec<_>>()
            );
            assert_eq!(first.state, second.state, "{scenario}");
            assert_eq!(first.transitions, second.transitions, "{scenario}");
            assert!(scenario_passed(&first, scenario), "{scenario}");
        }
    }

    #[test]
    fn trace_contains_only_reduced_privacy_safe_semantics() {
        let lab = run("habitat_story_v1", 1_600);
        let encoded = serde_json::to_vec(&lab.state).unwrap();
        let arguments = Arguments {
            scenario: "habitat_story_v1".into(),
            ticks: 1_600,
            ..Arguments::default()
        };
        let trace = serde_json::to_string(&summary(&lab, &arguments, &encoded)).unwrap();
        for forbidden in [
            "raw_pixel_bytes",
            "window_title",
            "typed_character",
            "key_code",
            "clipboard",
            "absolute_path",
            "hwnd",
            "native_handle_value",
            "screen_text",
            "microphone_sample_buffer",
        ] {
            assert!(!trace.contains(forbidden), "forbidden field {forbidden}");
        }
    }
}
