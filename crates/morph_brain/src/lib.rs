//! Same-process Rust integration of the neural core from the colleague's
//! `Thandorcat/morph` project.
//!
//! The checked-in asset is generated from the pinned upstream JavaScript
//! implementation. Pet 2 owns sensors, homeostasis, persistence, physics,
//! rendering and audio; Morph contributes its spiking decision network and
//! plastic command pathways through a narrow semantic output.

use std::{collections::HashMap, sync::OnceLock};

use glam::Vec2;
use lifecore::{BodyFeedback, FeedbackEvent, LifeState, MorphSomaticInput, SensorFrame};
use serde::{Deserialize, Serialize};

pub const UPSTREAM_COMMIT: &str = "6aa4e7c871c11ff2fa1619942611d4fb50457e49";
pub const STATE_SCHEMA: u32 = 1;

const ASSET_JSON: &str = include_str!("../assets/network-seed-1234.json");
const SYN_TAU: [f32; 5] = [5.0, 8.0, 15.0, 50.0, 300.0];
const DELAY_SLOTS: usize = 16;
const V_RESET: f32 = -0.2;
pub const MORPH_COMMAND_COUNT: usize = 16;
pub const MORPH_OBJECT_SLOT_COUNT: usize = 6;
pub const MORPH_ACTION_CONTROL_COUNT: usize = 8;
const COMMAND_NAMES: [&str; MORPH_COMMAND_COUNT] = [
    "C_FLEE",
    "C_APPR",
    "C_TURN_L",
    "C_TURN_R",
    "C_PERK",
    "C_MELT",
    "C_GROOM",
    "C_PLAY",
    "C_SAMPLE",
    "C_PUSH",
    "C_TOUCH",
    "C_PULL",
    "C_LISTEN",
    "C_SNIFF",
    "C_GRASP",
    "C_RELEASE",
];
const PRIMARY_COMMAND_NAMES: [&str; 6] =
    ["C_FLEE", "C_APPR", "C_PERK", "C_MELT", "C_GROOM", "C_PLAY"];
const MANIPULATION_COMMAND_NAMES: [&str; 4] = ["C_SAMPLE", "C_PUSH", "C_TOUCH", "C_PULL"];
const PERCEPTION_COMMAND_NAMES: [&str; 2] = ["C_LISTEN", "C_SNIFF"];
const CARRY_COMMAND_NAMES: [&str; 2] = ["C_GRASP", "C_RELEASE"];
const ACTION_CONTROL_NAMES: [&str; MORPH_ACTION_CONTROL_COUNT] = [
    "C_SAMPLE",
    "C_PUSH",
    "C_TOUCH",
    "C_PULL",
    "C_LISTEN",
    "C_SNIFF",
    "C_GRASP",
    "C_RELEASE",
];

mod sensor_adapter;
pub use sensor_adapter::*;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MorphCommand {
    Flee,
    Approach,
    Perk,
    Melt,
    Groom,
    Play,
    Sample,
    Push,
    Touch,
    Pull,
    Listen,
    Sniff,
    Grasp,
    Release,
    #[default]
    Idle,
}

impl MorphCommand {
    #[must_use]
    pub const fn as_wire(self) -> &'static str {
        match self {
            Self::Flee => "C_FLEE",
            Self::Approach => "C_APPR",
            Self::Perk => "C_PERK",
            Self::Melt => "C_MELT",
            Self::Groom => "C_GROOM",
            Self::Play => "C_PLAY",
            Self::Sample => "C_SAMPLE",
            Self::Push => "C_PUSH",
            Self::Touch => "C_TOUCH",
            Self::Pull => "C_PULL",
            Self::Listen => "C_LISTEN",
            Self::Sniff => "C_SNIFF",
            Self::Grasp => "C_GRASP",
            Self::Release => "C_RELEASE",
            Self::Idle => "idle",
        }
    }

    #[must_use]
    pub fn from_wire(value: &str) -> Self {
        match value {
            "C_FLEE" => Self::Flee,
            "C_APPR" => Self::Approach,
            "C_PERK" => Self::Perk,
            "C_MELT" => Self::Melt,
            "C_GROOM" => Self::Groom,
            "C_PLAY" => Self::Play,
            "C_SAMPLE" => Self::Sample,
            "C_PUSH" => Self::Push,
            "C_TOUCH" => Self::Touch,
            "C_PULL" => Self::Pull,
            "C_LISTEN" => Self::Listen,
            "C_SNIFF" => Self::Sniff,
            "C_GRASP" => Self::Grasp,
            "C_RELEASE" => Self::Release,
            _ => Self::Idle,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct MorphObjectInput {
    pub position: Vec2,
    pub salience: f32,
    pub motion: f32,
    pub size: f32,
    pub roundness: f32,
    pub color_rgb: [f32; 3],
    pub state: f32,
    pub familiarity: f32,
    pub edge: f32,
    pub luminance: f32,
    pub texture: f32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct MorphWorldInput {
    pub objects: [Option<MorphObjectInput>; MORPH_OBJECT_SLOT_COUNT],
    pub selected_slot: Option<u8>,
    /// Biases for SAMPLE, PUSH, TOUCH, PULL, LISTEN, SNIFF, GRASP, RELEASE.
    /// These are affordance suggestions; the corresponding neural WTA remains
    /// authoritative over which mutually-exclusive control actually fires.
    pub action_biases: [f32; MORPH_ACTION_CONTROL_COUNT],
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct MorphOutput {
    pub command: MorphCommand,
    pub manipulation: MorphCommand,
    pub perception: MorphCommand,
    pub carry: MorphCommand,
    pub command_rates: [f32; MORPH_COMMAND_COUNT],
    pub winner_rate: f32,
    pub manipulation_rate: f32,
    pub perception_rate: f32,
    pub carry_rate: f32,
    pub confidence: f32,
    pub attention: MorphAttention,
    pub attention_object_slot: Option<u8>,
    pub object_target: Option<Vec2>,
    pub valence: f32,
    pub arousal: f32,
    pub conflict: f32,
    pub turn: f32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MorphAttention {
    Cursor,
    Edge,
    SelfBody,
    #[default]
    Wander,
    Object,
}

/// Compact, stable telemetry view of the named Morph populations that are
/// meaningful at the organism boundary. Rates are exponentially-smoothed Hz.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub struct MorphPopulationRates {
    pub exp: f32,
    pub prox: f32,
    pub mot: f32,
    pub tch: f32,
    pub vib: f32,
    pub loom: f32,
    pub hab: f32,
    pub nov: f32,
    pub kc: f32,
    pub valp: f32,
    pub valn: f32,
    pub mbon_a: f32,
    pub mbon_v: f32,
    pub att: f32,
    pub rest: f32,
}

/// Current learned-weight envelope plus its safe range relative to the pinned
/// upstream weights. The relative values make differently-scaled pathways
/// directly comparable in the developer monitor.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct MorphWeightRange {
    pub count: usize,
    pub minimum: f32,
    pub maximum: f32,
    pub relative_minimum: f32,
    pub relative_maximum: f32,
    pub relative_lower_bound: f32,
    pub relative_upper_bound: f32,
}

/// Read-only neural state intended for live telemetry. Calling
/// [`MorphBrain::diagnostics`] never advances the network or consumes random
/// numbers, so deterministic replay and the pinned golden trace are unchanged.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct MorphDiagnostics {
    pub upstream_age_ms: f64,
    pub reward_trace: f32,
    pub current_spike_count: usize,
    pub mean_population_rate: f32,
    pub max_population_rate: f32,
    pub population_rates: MorphPopulationRates,
    pub classical_weights: MorphWeightRange,
    pub operant_weights: MorphWeightRange,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MorphBrainState {
    pub schema: u32,
    pub upstream_commit: String,
    pub plastic_weights: Vec<f32>,
    pub operant_weights: Vec<f32>,
    pub reward_trace: f32,
    pub age_ms: f64,
}

impl MorphBrainState {
    #[must_use]
    pub fn is_valid(&self) -> bool {
        self.schema == STATE_SCHEMA
            && self.upstream_commit == UPSTREAM_COMMIT
            && self.reward_trace.is_finite()
            && self.age_ms.is_finite()
            && self.age_ms >= 0.0
            && self.plastic_weights.iter().all(|weight| weight.is_finite())
            && self.operant_weights.iter().all(|weight| weight.is_finite())
    }
}

pub struct MorphBrain {
    net: Network,
    readout: Readout,
    plasticity: Plasticity,
    identity_signature: Vec<f32>,
    environment_rng: XorShift32,
    reward_trace: f32,
    vibration: f32,
    rest_ou: f32,
    exploring: MorphCommand,
    explore_remaining_ms: f32,
    command_habituation: [f32; 6],
    slow_accumulator_ms: f32,
    age_ms: f64,
    last_output: MorphOutput,
    somatic: MorphSomaticInput,
}

impl MorphBrain {
    pub fn new(identity_seed: u64, restored: Option<MorphBrainState>) -> Result<Self, String> {
        let asset = network_asset()?;
        let neural_seed = fold_seed(identity_seed ^ u64::from(asset.source_seed));
        let mut signature_rng = XorShift32::new(fold_seed(identity_seed ^ 0x51_67_49_4e));
        let sig_size = asset
            .pops
            .iter()
            .find(|population| population.name == "SIG")
            .map_or(0, |population| population.size);
        let mut identity_signature = (0..sig_size)
            .map(|_| signature_rng.next_f32())
            .collect::<Vec<_>>();
        normalize_signature(&mut identity_signature);
        let net = Network::new(asset, neural_seed)?;
        let mut brain = Self {
            readout: Readout::new(&net)?,
            plasticity: Plasticity::new(&net, asset),
            net,
            identity_signature,
            environment_rng: XorShift32::new(fold_seed(identity_seed ^ 0x4d_4f_52_50_48)),
            reward_trace: 0.0,
            vibration: 0.0,
            rest_ou: 0.0,
            exploring: MorphCommand::Idle,
            explore_remaining_ms: 0.0,
            command_habituation: [0.0; 6],
            slow_accumulator_ms: 0.0,
            age_ms: 0.0,
            last_output: MorphOutput::default(),
            somatic: MorphSomaticInput::default(),
        };
        if let Some(state) = restored.filter(MorphBrainState::is_valid) {
            brain.restore_weights(&state);
            // A user save preserves learned weights, not in-flight eligibility.
            // Never apply a pre-restart reward to a new sensory context.
            brain.reward_trace = 0.0;
            brain.age_ms = state.age_ms.max(0.0);
        }
        Ok(brain)
    }

    #[must_use]
    pub fn tick(
        &mut self,
        sensors: &SensorFrame,
        body: &BodyFeedback,
        life: &LifeState,
        dt: f32,
    ) -> MorphOutput {
        self.tick_with_world(sensors, body, life, &MorphWorldInput::default(), dt)
    }

    #[must_use]
    pub fn tick_with_world(
        &mut self,
        sensors: &SensorFrame,
        body: &BodyFeedback,
        life: &LifeState,
        world: &MorphWorldInput,
        dt: f32,
    ) -> MorphOutput {
        let dt_ms = if dt.is_finite() {
            (dt.clamp(0.001, 0.1) * 1_000.0).round().max(1.0) as usize
        } else {
            1
        };
        self.age_ms += dt_ms as f64;
        self.transduce(sensors, body, life, world, dt_ms as f32);
        self.apply_modulators(life);

        let mut completed = 0;
        while completed < dt_ms {
            let batch = (dt_ms - completed).min(8);
            for _ in 0..batch {
                self.net.step();
                self.plasticity.observe_spikes(&self.net);
                self.readout.observe_attention(&self.net);
            }
            self.net.update_rates(batch);
            completed += batch;
        }
        self.slow_accumulator_ms += dt_ms as f32;
        while self.slow_accumulator_ms >= 100.0 {
            self.slow_accumulator_ms -= 100.0;
            self.plasticity
                .update(&mut self.net, self.reward_trace, 100.0);
            self.reward_trace *= (-0.1_f32 / 1.8).exp();
        }
        self.readout.update(&self.net, dt_ms as f32);
        self.last_output = self.readout.output(&self.net, world);
        self.update_command_habituation(self.last_output.command, dt_ms as f32);
        self.plasticity.execution_age_ms += dt_ms as f32;
        if self.plasticity.execution_age_ms > 2_000.0 {
            self.plasticity.last_command = MorphCommand::Idle;
        }
        self.last_output
    }

    /// Called only for a completed physical frame, after all intent overrides.
    pub fn acknowledge_execution(&mut self, command: MorphCommand) {
        self.plasticity.last_command = command;
        self.plasticity.execution_age_ms = 0.0;
    }

    pub fn apply_feedback(&mut self, event: &FeedbackEvent) {
        let reward = event.reward().clamp(-1.0, 1.0);
        self.reward_trace = (self.reward_trace * 0.55 + reward * 0.75).clamp(-1.0, 1.0);
        self.vibration = self.vibration.max(reward.abs() * 0.35);
        self.plasticity.note_feedback(&mut self.net, reward);
    }

    /// Installs the previous body/cognition frame's normalized somatic input.
    /// It is read on the next Morph tick, preventing same-tick body loops.
    pub fn set_somatic_input(&mut self, mut input: MorphSomaticInput) {
        input.sanitize();
        self.somatic = input;
    }

    #[must_use]
    pub fn snapshot(&self) -> MorphBrainState {
        MorphBrainState {
            schema: STATE_SCHEMA,
            upstream_commit: UPSTREAM_COMMIT.to_owned(),
            plastic_weights: self.plasticity.classical_weights.clone(),
            operant_weights: self.plasticity.operant_weights.clone(),
            reward_trace: self.reward_trace,
            age_ms: self.age_ms,
        }
    }

    #[must_use]
    pub const fn last_output(&self) -> MorphOutput {
        self.last_output
    }

    #[must_use]
    pub fn network_shape(&self) -> (usize, usize, usize) {
        (self.net.n, self.net.edge_post.len(), self.net.pops.len())
    }

    /// Returns the live state required by the developer telemetry panel without
    /// exposing mutable network internals.
    #[must_use]
    pub fn diagnostics(&self) -> MorphDiagnostics {
        let population_count = self.net.rates.len();
        let (population_sum, max_population_rate) = self
            .net
            .rates
            .iter()
            .copied()
            .map(sanitize_rate)
            .fold((0.0_f32, 0.0_f32), |(sum, maximum), rate| {
                (sum + rate, maximum.max(rate))
            });
        let mean_population_rate = if population_count == 0 {
            0.0
        } else {
            population_sum / population_count as f32
        };
        let rate = |name| sanitize_rate(self.net.rate(name));

        MorphDiagnostics {
            upstream_age_ms: self.age_ms,
            reward_trace: sanitize_signed(self.reward_trace),
            current_spike_count: self.net.spike_count.min(self.net.n),
            mean_population_rate,
            max_population_rate,
            population_rates: MorphPopulationRates {
                exp: rate("EXP"),
                prox: rate("PROX"),
                mot: rate("MOT"),
                tch: rate("TCH"),
                vib: rate("VIB"),
                loom: rate("LOOM"),
                hab: rate("HAB"),
                nov: rate("NOV"),
                kc: rate("KC"),
                valp: rate("VALP"),
                valn: rate("VALN"),
                mbon_a: rate("MBON_A"),
                mbon_v: rate("MBON_V"),
                att: rate("ATT"),
                rest: rate("REST"),
            },
            classical_weights: weight_range(
                &self.plasticity.classical_weights,
                &self.plasticity.classical_initial,
                0.20,
                1.0,
            ),
            operant_weights: weight_range(
                &self.plasticity.operant_weights,
                &self.plasticity.operant_initial,
                0.25,
                2.6,
            ),
        }
    }

    fn restore_weights(&mut self, state: &MorphBrainState) {
        self.plasticity.restore(&mut self.net, state);
    }

    fn transduce(
        &mut self,
        sensors: &SensorFrame,
        body: &BodyFeedback,
        life: &LifeState,
        world: &MorphWorldInput,
        dt_ms: f32,
    ) {
        self.net.external.fill(0.0);
        let present = sensors.user_presence.unwrap_or({
            if sensors.user_idle_seconds < 180.0 {
                1.0
            } else {
                0.0
            }
        }) > 0.05;
        let distance = sensors.cursor_distance_to_pet.max(0.0);
        let speed = sensors.cursor_velocity.length().max(0.0);
        let expansion = (sensors.cursor_approach_speed.max(0.0) * 2.4).clamp(0.0, 1.0);
        let proximity = (if present {
            (-distance * 4.8).exp().clamp(0.0, 1.0)
        } else {
            0.0
        })
        .max(self.somatic.proximity);
        let motion = (if present {
            (1.0 + speed * 4.5).ln() / 6.0_f32.ln()
        } else {
            0.0
        }
        .clamp(0.0, 1.0))
        .max(self.somatic.motion);
        let calm_touch = (if sensors.pet_touched || body.cursor_contact {
            (1.0 - speed * 1.8).clamp(0.0, 1.0)
        } else {
            0.0
        })
        .max(self.somatic.touch);
        if sensors.pointer_pressed {
            self.vibration = self.vibration.max(0.72);
        }
        if let Some(collision) = &body.collision {
            self.vibration = self.vibration.max(collision.intensity.clamp(0.0, 1.0));
        }
        let vibration = self.vibration.max(self.somatic.vibration);
        self.vibration *= (-dt_ms / 30.0).exp();
        let daytime = sensors.time_of_day_01 >= 8.0 / 24.0 && sensors.time_of_day_01 <= 21.0 / 24.0;
        let luminance = sensors
            .local_luminance
            .or(sensors.mean_luminance)
            .unwrap_or(0.6)
            .clamp(0.0, 1.0)
            * if daytime { 1.0 } else { 0.25 };
        let self_motion = (body.velocity.length() * 2.8
            + body.acceleration.length() * 0.20
            + body.pose_error * 0.45)
            .clamp(0.0, 1.0);
        drive_feature(
            &mut self.net,
            "EXP",
            expansion.max(self.somatic.exploration),
        );
        drive_feature(&mut self.net, "PROX", proximity);
        drive_feature(&mut self.net, "MOT", motion);
        drive_feature(&mut self.net, "TCH", calm_touch);
        drive_feature(&mut self.net, "VIB", vibration);
        drive_feature(&mut self.net, "LOOM", self.somatic.looming);
        drive_feature(&mut self.net, "HAB", self.somatic.habituation);
        drive_feature(&mut self.net, "NOV", self.somatic.novelty);
        drive_feature(&mut self.net, "LGT", luminance);
        drive_feature(&mut self.net, "SLF", self_motion);
        drive_feature(
            &mut self.net,
            "SND",
            sensors.audio_rms.unwrap_or(0.0).clamp(0.0, 1.0),
        );
        for name in ["ODR", "TMP_HOT", "TMP_COLD", "GLR", "SURF", "REW"] {
            drive_feature(&mut self.net, name, 0.0);
        }
        let object_salience = world
            .objects
            .iter()
            .flatten()
            .map(|object| object.salience)
            .fold(0.0_f32, f32::max);
        let object_motion = world
            .objects
            .iter()
            .flatten()
            .map(|object| object.motion)
            .fold(0.0_f32, f32::max);
        drive_feature(&mut self.net, "OBJ", object_salience);
        drive_feature(&mut self.net, "OBJM", object_motion);
        for (name, feature) in [
            ("OBJ_SIZE", MorphObjectFeature::Size),
            ("OBJ_ROUND", MorphObjectFeature::Roundness),
            ("OBJ_R", MorphObjectFeature::Red),
            ("OBJ_G", MorphObjectFeature::Green),
            ("OBJ_B", MorphObjectFeature::Blue),
            ("OBJ_STATE", MorphObjectFeature::State),
            ("OBJ_FAM", MorphObjectFeature::Familiarity),
            ("OBJ_EDGE", MorphObjectFeature::Edge),
            ("OBJ_MOTION", MorphObjectFeature::Motion),
            ("OBJ_LIGHT", MorphObjectFeature::Luminance),
            ("OBJ_TEXTURE", MorphObjectFeature::Texture),
        ] {
            drive_object_slots(&mut self.net, name, world, feature);
        }
        if let Some(signature) = self.net.pop("SIG") {
            for index in 0..signature.size {
                let value = if present {
                    self.identity_signature.get(index).copied().unwrap_or(0.0)
                } else {
                    0.0
                };
                self.net.external[signature.start + index] = 0.85 + 1.05 * value;
            }
        }

        self.rest_ou = self.rest_ou * 0.995 + (self.environment_rng.next_f32() * 2.0 - 1.0) * 0.06;
        let attention = (proximity * 0.85 + motion * 0.5 + calm_touch + vibration)
            .clamp(0.0, 1.0)
            .max(self.somatic.attention);
        let boredom =
            ((0.34 + life.drives.social * 0.30 + life.drives.curiosity * 0.26 + self.rest_ou)
                * (1.0 - attention * 0.88))
                .clamp(0.0, 1.0);
        let unmet = life
            .drives
            .social
            .max(life.drives.play)
            .max(life.drives.curiosity)
            .max(life.drives.sleep);
        self.net.drive(
            "REST",
            1.30 + 3.20 * boredom * (0.42 + 0.58 * (1.0 - unmet)),
        );
        self.net.drive("REST", 2.4 * self.somatic.rest);
        self.net.drive("MBON_A", 0.55);
        self.net.drive("MBON_V", 0.55);
        self.net.drive(
            "VALP",
            1.62 + life.affect.valence.max(0.0) * 0.8
                + calm_touch * 0.45
                + self.somatic.positive_outcome * 0.90,
        );
        self.net.drive(
            "VALN",
            1.62 + (-life.affect.valence).max(0.0) * 0.8
                + life.drives.safety * 1.1
                + self.somatic.negative_outcome * 1.00,
        );
        self.net.drive("CPG_a", 1.50);
        self.net.drive("CPG_b", 1.50 * 0.98);

        if let Some(attention_pop) = self.net.pop("ATT") {
            let target_count = (attention_pop.size / 2).max(1);
            let cursor_salience = if present {
                (proximity * 0.9 + motion * 0.5 + life.drives.social * 0.48).clamp(0.0, 1.0)
            } else {
                0.0
            };
            for target in 0..target_count {
                let salience = if target == 0 {
                    cursor_salience
                } else if (1..=MORPH_OBJECT_SLOT_COUNT).contains(&target) {
                    world.objects[target - 1].map_or(0.0, |object| object.salience)
                } else if target + 3 == target_count {
                    (0.18 + life.drives.curiosity * 0.46).clamp(0.0, 1.0)
                } else if target + 2 == target_count {
                    (0.12 + life.drives.safety * 0.30).clamp(0.0, 1.0)
                } else if target + 1 == target_count {
                    (0.10 + life.drives.comfort * 0.30).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                self.net
                    .drive_slice("ATT", target * 2, target * 2 + 2, 0.92 + 1.15 * salience);
            }
        }
        let delta = sensors.cursor_position - body.world_position;
        let turn = (delta.x.abs() * 3.2).clamp(0.0, 1.0) * 1.75;
        let command_bias = -0.30 * life.drives.sleep - 0.10 * life.affect.stress;
        let exploration = self.exploration_boosts(life, dt_ms);
        self.net.drive(
            "C_TURN_L",
            command_bias + if delta.x < 0.0 { turn } else { 0.0 },
        );
        self.net.drive(
            "C_TURN_R",
            command_bias + if delta.x > 0.0 { turn } else { 0.0 },
        );
        self.net.drive(
            "C_APPR",
            command_bias
                + life.drives.social * 0.36
                + life.drives.curiosity * 0.10
                + life.affect.attachment * proximity * 0.52
                - life.drives.sleep * 0.85
                + exploration[0]
                - self.command_habituation[1] * 1.10,
        );
        self.net.drive(
            "C_PLAY",
            command_bias
                + life.drives.play * 0.38
                + sensors.user_activity_rate * 0.12
                + proximity * 0.18
                - life.drives.sleep * 1.10
                + exploration[2]
                - self.command_habituation[5] * 1.10,
        );
        self.net.drive(
            "C_PERK",
            command_bias
                + life.drives.curiosity * 0.34
                + life.drives.novelty * 0.24
                + motion * 0.22
                + life.drives.safety * 0.18
                - life.drives.sleep * 0.65
                + exploration[1]
                - self.command_habituation[2] * 1.10,
        );
        self.net.drive(
            "C_MELT",
            command_bias
                + life.drives.sleep * 1.25
                + life.drives.comfort * 0.36
                + calm_touch * 0.34
                + exploration[4]
                - self.command_habituation[3] * 1.10,
        );
        self.net.drive(
            "C_GROOM",
            command_bias
                + life.drives.comfort * 0.42
                + life.drives.autonomy * 0.22
                + life.affect.stress * 0.24
                - life.drives.sleep * 0.35
                + exploration[3]
                - self.command_habituation[4] * 1.10,
        );
        let calm_veto = ((1.0 - motion) * (1.0 - expansion) * proximity * 1.35).clamp(0.0, 1.35);
        self.net.drive(
            "C_FLEE",
            command_bias + life.drives.safety * 1.20 + expansion * 0.42 + vibration * 0.20
                - calm_veto
                - self.command_habituation[0] * 0.55,
        );
        let action_sleep_veto = [1.00, 1.05, 0.85, 1.05, 0.70, 0.75, 1.00, 0.80];
        for index in 0..MORPH_ACTION_CONTROL_COUNT {
            self.net.drive(
                ACTION_CONTROL_NAMES[index],
                command_bias - life.drives.sleep * action_sleep_veto[index]
                    + world.action_biases[index].clamp(0.0, 2.4),
            );
        }
        if let Some(object) = world
            .selected_slot
            .and_then(|slot| world.objects.get(usize::from(slot)))
            .copied()
            .flatten()
        {
            let distance = object.position.distance(body.world_position);
            let approach = ((distance - 0.055) / 0.30).clamp(0.0, 1.0) * object.salience;
            self.net.drive("C_APPR", approach * 0.42);
            self.net
                .drive("C_PERK", object.salience * 0.22 + object.motion * 0.18);
        }
    }

    fn apply_modulators(&mut self, life: &LifeState) {
        let arousal = life.affect.arousal.clamp(0.0, 1.0);
        let calm =
            ((1.0 - life.affect.stress) * (0.35 + life.drives.comfort * 0.35)).clamp(0.0, 1.0);
        self.net.weight_gain = 1.0 + 1.2 * arousal;
        self.net.noise_gain = 1.0 + 3.0 * arousal;
        self.net.set_tau_gain(1.0 - 0.3 * arousal);
        self.net.threshold = 1.0 + 0.15 * calm;
        self.net.sfa_gain = (1.0 + 0.9 * arousal - 0.55 * calm).max(0.2);
        self.net.scale_edges(
            &self.plasticity.bond_edges,
            1.0 + 0.8 * life.affect.attachment,
        );
        self.net.scale_edges(
            &self.plasticity.dopamine_edges,
            1.0 + arousal * self.reward_trace.max(0.0),
        );
    }

    fn exploration_boosts(&mut self, life: &LifeState, dt_ms: f32) -> [f32; 5] {
        let candidates = [
            (
                MorphCommand::Approach,
                0.10 + life.drives.social * 0.62 + life.drives.curiosity * 0.16,
            ),
            (
                MorphCommand::Perk,
                0.12 + life.drives.curiosity * 0.78
                    + life.drives.novelty * 0.44
                    + life.drives.safety * 0.20,
            ),
            (
                MorphCommand::Play,
                0.08 + life.drives.play * 1.16 * (0.35 + (1.0 - life.drives.sleep) * 0.65),
            ),
            (
                MorphCommand::Groom,
                0.10 + life.drives.comfort * 0.27
                    + life.affect.stress * 0.25
                    + life.drives.autonomy * 0.18,
            ),
            (
                MorphCommand::Melt,
                0.07 + life.drives.sleep * 0.83 + life.drives.comfort * 0.31
                    - life.drives.curiosity.max(life.drives.play) * 0.32,
            ),
        ];
        self.explore_remaining_ms -= dt_ms;
        if self.exploring == MorphCommand::Idle || self.explore_remaining_ms <= 0.0 {
            let total = candidates
                .iter()
                .map(|(command, weight)| {
                    weight.max(0.02)
                        * if *command == self.exploring {
                            0.62
                        } else {
                            1.0
                        }
                })
                .sum::<f32>();
            let mut cursor = self.environment_rng.next_f32() * total;
            for (command, weight) in candidates {
                cursor -= weight.max(0.02) * if command == self.exploring { 0.62 } else { 1.0 };
                if cursor <= 0.0 {
                    self.exploring = command;
                    break;
                }
            }
            let calm_persistence = 1.0 - life.affect.arousal.clamp(0.0, 1.0) * 0.38;
            self.explore_remaining_ms =
                (1_900.0 + self.environment_rng.next_f32() * 3_600.0) * calm_persistence;
        }
        let selected_weight = candidates
            .iter()
            .find(|(command, _)| *command == self.exploring)
            .map_or(0.08, |(_, weight)| *weight);
        let urgency = ((selected_weight - 0.08) / 0.92).clamp(0.0, 1.0);
        let boost = 1.15 + urgency * 0.30;
        let mut result = [0.0; 5];
        let index = match self.exploring {
            MorphCommand::Approach => Some(0),
            MorphCommand::Perk => Some(1),
            MorphCommand::Play => Some(2),
            MorphCommand::Groom => Some(3),
            MorphCommand::Melt => Some(4),
            MorphCommand::Flee
            | MorphCommand::Sample
            | MorphCommand::Push
            | MorphCommand::Touch
            | MorphCommand::Pull
            | MorphCommand::Listen
            | MorphCommand::Sniff
            | MorphCommand::Grasp
            | MorphCommand::Release
            | MorphCommand::Idle => None,
        };
        if let Some(index) = index {
            let calibration = [1.00, 1.08, 0.90, 0.65, 1.05];
            result[index] = boost * calibration[index];
        }
        result
    }

    fn update_command_habituation(&mut self, command: MorphCommand, dt_ms: f32) {
        let active = match command {
            MorphCommand::Flee => Some(0),
            MorphCommand::Approach => Some(1),
            MorphCommand::Perk => Some(2),
            MorphCommand::Melt => Some(3),
            MorphCommand::Groom => Some(4),
            MorphCommand::Play => Some(5),
            MorphCommand::Sample
            | MorphCommand::Push
            | MorphCommand::Touch
            | MorphCommand::Pull
            | MorphCommand::Listen
            | MorphCommand::Sniff
            | MorphCommand::Grasp
            | MorphCommand::Release
            | MorphCommand::Idle => None,
        };
        for index in 0..self.command_habituation.len() {
            let target = if active == Some(index) { 1.0 } else { 0.0 };
            let tau_ms = if target > self.command_habituation[index] {
                2_600.0
            } else {
                6_500.0
            };
            let alpha = 1.0 - (-dt_ms / tau_ms).exp();
            self.command_habituation[index] += (target - self.command_habituation[index]) * alpha;
        }
    }
}

#[derive(Debug, Deserialize)]
struct NetworkAsset {
    schema: u32,
    upstream_commit: String,
    source_seed: u32,
    n: usize,
    pops: Vec<PopulationAsset>,
    tau_m: Vec<f32>,
    sigma: Vec<f32>,
    t_ref: Vec<u8>,
    pop_of: Vec<u16>,
    thr_off: Vec<f32>,
    sfa_b: Vec<f32>,
    sfa_decay: Vec<f32>,
    row_start: Vec<usize>,
    edge_post: Vec<usize>,
    edge_weight: Vec<f32>,
    edge_delay: Vec<u8>,
    edge_channel: Vec<u8>,
    edge_std: Vec<i32>,
    std_u: Vec<f32>,
    std_recovery: Vec<f32>,
    meta: MetaAsset,
}

#[derive(Debug, Deserialize)]
struct PopulationAsset {
    name: String,
    start: usize,
    size: usize,
}

#[derive(Debug, Deserialize)]
struct MetaAsset {
    plastic: PlasticAsset,
    kc_start: usize,
    kc_size: usize,
    bond_edges: Vec<usize>,
    dopamine_edges: Vec<usize>,
    operant: OperantAsset,
}

#[derive(Debug, Deserialize)]
struct PlasticAsset {
    edge: Vec<usize>,
    pre: Vec<usize>,
    sign: Vec<i8>,
}

#[derive(Debug, Deserialize)]
struct OperantAsset {
    edge: Vec<usize>,
    pre: Vec<usize>,
    command: Vec<u8>,
    names: Vec<String>,
}

fn network_asset() -> Result<&'static NetworkAsset, String> {
    static ASSET: OnceLock<Result<NetworkAsset, String>> = OnceLock::new();
    ASSET
        .get_or_init(|| {
            let asset: NetworkAsset = serde_json::from_str(ASSET_JSON)
                .map_err(|error| format!("invalid embedded Morph network: {error}"))?;
            validate_asset(&asset)?;
            Ok(asset)
        })
        .as_ref()
        .map_err(Clone::clone)
}

fn validate_asset(asset: &NetworkAsset) -> Result<(), String> {
    if asset.schema != 1 || asset.upstream_commit != UPSTREAM_COMMIT {
        return Err("embedded Morph network provenance mismatch".to_owned());
    }
    let n = asset.n;
    for (name, length) in [
        ("tau_m", asset.tau_m.len()),
        ("sigma", asset.sigma.len()),
        ("t_ref", asset.t_ref.len()),
        ("pop_of", asset.pop_of.len()),
        ("thr_off", asset.thr_off.len()),
        ("sfa_b", asset.sfa_b.len()),
        ("sfa_decay", asset.sfa_decay.len()),
    ] {
        if length != n {
            return Err(format!("Morph asset {name} length {length} != {n}"));
        }
    }
    let edges = asset.edge_post.len();
    if asset.row_start.len() != n + 1
        || asset.row_start.last().copied() != Some(edges)
        || asset.edge_weight.len() != edges
        || asset.edge_delay.len() != edges
        || asset.edge_channel.len() != edges
        || asset.edge_std.len() != edges
    {
        return Err("Morph edge arrays are inconsistent".to_owned());
    }
    if asset.pops.len() != 57 || n != 526 || edges != 17_475 {
        return Err(format!(
            "unexpected Morph topology: {n} neurons, {} populations, {edges} edges",
            asset.pops.len()
        ));
    }
    Ok(())
}

#[derive(Debug, Clone, Copy)]
struct Population {
    start: usize,
    size: usize,
    index: usize,
}

struct Network {
    n: usize,
    pops: Vec<Population>,
    pop_by_name: HashMap<String, Population>,
    tau_m: Vec<f32>,
    sigma: Vec<f32>,
    t_ref: Vec<u8>,
    pop_of: Vec<u16>,
    threshold_offset: Vec<f32>,
    sfa_b: Vec<f32>,
    sfa_decay: Vec<f32>,
    adaptation: Vec<f32>,
    voltage: Vec<f32>,
    refractory: Vec<u8>,
    external: Vec<f32>,
    conductance: Vec<f32>,
    pending: Vec<f32>,
    spiked: Vec<u8>,
    spike_list: Vec<usize>,
    spike_count: usize,
    membrane_decay: Vec<f32>,
    noise_scale: Vec<f32>,
    synapse_decay: [f32; 5],
    row_start: Vec<usize>,
    edge_post: Vec<usize>,
    edge_weight: Vec<f32>,
    edge_base: Vec<f32>,
    edge_delay: Vec<u8>,
    edge_channel: Vec<u8>,
    edge_std: Vec<i32>,
    std_x: Vec<f32>,
    std_u: Vec<f32>,
    std_recovery: Vec<f32>,
    rates: Vec<f32>,
    population_spikes: Vec<u32>,
    gaussian: Vec<f32>,
    gaussian_cursor: usize,
    time: usize,
    threshold: f32,
    weight_gain: f32,
    noise_gain: f32,
    sfa_gain: f32,
    tau_gain: f32,
}

impl Network {
    fn new(asset: &NetworkAsset, seed: u32) -> Result<Self, String> {
        let pops = asset
            .pops
            .iter()
            .enumerate()
            .map(|(index, population)| Population {
                start: population.start,
                size: population.size,
                index,
            })
            .collect::<Vec<_>>();
        let pop_by_name = asset
            .pops
            .iter()
            .zip(pops.iter().copied())
            .map(|(asset, population)| (asset.name.clone(), population))
            .collect();
        let mut rng = XorShift32::new(seed);
        let gaussian = gaussian_pool(&mut rng, 8_192);
        let gaussian_cursor = (rng.next_f64() * gaussian.len() as f64) as usize;
        let n = asset.n;
        let mut network = Self {
            n,
            pops,
            pop_by_name,
            tau_m: asset.tau_m.clone(),
            sigma: asset.sigma.clone(),
            t_ref: asset.t_ref.clone(),
            pop_of: asset.pop_of.clone(),
            threshold_offset: asset.thr_off.clone(),
            sfa_b: asset.sfa_b.clone(),
            sfa_decay: asset.sfa_decay.clone(),
            adaptation: vec![0.0; n],
            voltage: vec![0.0; n],
            refractory: vec![0; n],
            external: vec![0.0; n],
            conductance: vec![0.0; SYN_TAU.len() * n],
            pending: vec![0.0; SYN_TAU.len() * DELAY_SLOTS * n],
            spiked: vec![0; n],
            spike_list: vec![0; n],
            spike_count: 0,
            membrane_decay: vec![0.0; n],
            noise_scale: vec![0.0; n],
            synapse_decay: SYN_TAU.map(|tau| (-1.0 / tau).exp()),
            row_start: asset.row_start.clone(),
            edge_post: asset.edge_post.clone(),
            edge_weight: asset.edge_weight.clone(),
            edge_base: asset.edge_weight.clone(),
            edge_delay: asset.edge_delay.clone(),
            edge_channel: asset.edge_channel.clone(),
            edge_std: asset.edge_std.clone(),
            std_x: vec![1.0; asset.std_u.len()],
            std_u: asset.std_u.clone(),
            std_recovery: asset.std_recovery.clone(),
            rates: vec![0.0; asset.pops.len()],
            population_spikes: vec![0; asset.pops.len()],
            gaussian,
            gaussian_cursor,
            time: 0,
            threshold: 1.0,
            weight_gain: 1.0,
            noise_gain: 1.0,
            sfa_gain: 1.0,
            tau_gain: f32::NAN,
        };
        network.set_tau_gain(1.0);
        Ok(network)
    }

    fn pop(&self, name: &str) -> Option<Population> {
        self.pop_by_name.get(name).copied()
    }

    fn drive(&mut self, name: &str, value: f32) {
        if let Some(population) = self.pop(name) {
            self.external[population.start..population.start + population.size].fill(value);
        }
    }

    fn drive_slice(&mut self, name: &str, from: usize, to: usize, value: f32) {
        if let Some(population) = self.pop(name) {
            let start = population.start + from.min(population.size);
            let end = population.start + to.min(population.size);
            if start < end {
                self.external[start..end].fill(value);
            }
        }
    }

    fn rate(&self, name: &str) -> f32 {
        self.pop(name)
            .map_or(0.0, |population| self.rates[population.index])
    }

    fn set_tau_gain(&mut self, gain: f32) {
        if self.tau_gain == gain {
            return;
        }
        self.tau_gain = gain;
        for neuron in 0..self.n {
            let tau = (self.tau_m[neuron] * gain).max(1.0);
            let decay = (-1.0 / tau).exp();
            self.membrane_decay[neuron] = decay;
            self.noise_scale[neuron] = self.sigma[neuron] * (1.0 - decay * decay).sqrt();
        }
    }

    fn scale_edges(&mut self, edges: &[usize], scale: f32) {
        for &edge in edges {
            if edge < self.edge_weight.len() {
                self.edge_weight[edge] = self.edge_base[edge] * scale;
            }
        }
    }

    fn step(&mut self) {
        let slot = self.time & (DELAY_SLOTS - 1);
        for channel in 0..SYN_TAU.len() {
            let decay = self.synapse_decay[channel];
            let conductance_base = channel * self.n;
            let pending_base = (channel * DELAY_SLOTS + slot) * self.n;
            for neuron in 0..self.n {
                let conductance = conductance_base + neuron;
                let pending = pending_base + neuron;
                self.conductance[conductance] =
                    self.conductance[conductance] * decay + self.pending[pending];
                self.pending[pending] = 0.0;
            }
        }

        let gaussian_mask = self.gaussian.len() - 1;
        let mut spike_count = 0;
        for neuron in 0..self.n {
            self.adaptation[neuron] *= self.sfa_decay[neuron];
            if self.refractory[neuron] > 0 {
                self.refractory[neuron] -= 1;
                self.voltage[neuron] = V_RESET;
                self.spiked[neuron] = 0;
                continue;
            }
            let mut current = self.external[neuron] - self.adaptation[neuron];
            for channel in 0..SYN_TAU.len() {
                current += self.conductance[channel * self.n + neuron];
            }
            let decay = self.membrane_decay[neuron];
            let mut voltage = current + (self.voltage[neuron] - current) * decay;
            self.gaussian_cursor = (self.gaussian_cursor + 1) & gaussian_mask;
            voltage +=
                self.noise_scale[neuron] * self.noise_gain * self.gaussian[self.gaussian_cursor];
            if voltage >= self.threshold + self.threshold_offset[neuron] {
                self.adaptation[neuron] += self.sfa_b[neuron] * self.sfa_gain;
                voltage = V_RESET;
                self.refractory[neuron] = self.t_ref[neuron];
                self.spike_list[spike_count] = neuron;
                self.spiked[neuron] = 1;
                spike_count += 1;
            } else {
                self.spiked[neuron] = 0;
            }
            self.voltage[neuron] = voltage;
        }
        self.spike_count = spike_count;

        for spike in 0..spike_count {
            let neuron = self.spike_list[spike];
            self.population_spikes[self.pop_of[neuron] as usize] += 1;
            for edge in self.row_start[neuron]..self.row_start[neuron + 1] {
                let mut weight = self.edge_weight[edge];
                let std_index = self.edge_std[edge];
                if std_index >= 0 {
                    let std_index = std_index as usize;
                    weight *= self.std_x[std_index];
                    self.std_x[std_index] -= self.std_u[std_index] * self.std_x[std_index];
                }
                let target_slot = (self.time + self.edge_delay[edge] as usize) & (DELAY_SLOTS - 1);
                let index = (self.edge_channel[edge] as usize * DELAY_SLOTS + target_slot) * self.n
                    + self.edge_post[edge];
                self.pending[index] += weight * self.weight_gain;
            }
        }
        for index in 0..self.std_x.len() {
            self.std_x[index] += (1.0 - self.std_x[index]) * self.std_recovery[index];
        }
        self.time = self.time.wrapping_add(1);
    }

    fn update_rates(&mut self, steps: usize) {
        let alpha = (steps as f32 / 80.0).min(1.0);
        for (index, population) in self.pops.iter().enumerate() {
            let instant = self.population_spikes[index] as f32
                / population.size as f32
                / (steps as f32 / 1_000.0);
            self.rates[index] += (instant - self.rates[index]) * alpha;
            self.population_spikes[index] = 0;
        }
    }
}

struct Readout {
    attention_pop: Population,
    attention_spikes: Vec<f32>,
    attention_winner: usize,
    winner: MorphCommand,
    manipulation: MorphCommand,
    perception: MorphCommand,
    carry: MorphCommand,
    winner_rate: f32,
    manipulation_rate: f32,
    perception_rate: f32,
    carry_rate: f32,
    valence: f32,
    arousal: f32,
    conflict: f32,
}

impl Readout {
    fn new(net: &Network) -> Result<Self, String> {
        for name in COMMAND_NAMES {
            if net.pop(name).is_none() {
                return Err(format!("Morph asset is missing {name}"));
            }
        }
        let attention_pop = net
            .pop("ATT")
            .ok_or_else(|| "Morph asset is missing ATT".to_owned())?;
        Ok(Self {
            attention_pop,
            attention_spikes: vec![0.0; attention_pop.size / 2],
            attention_winner: (attention_pop.size / 2).saturating_sub(3),
            winner: MorphCommand::Idle,
            manipulation: MorphCommand::Idle,
            perception: MorphCommand::Idle,
            carry: MorphCommand::Idle,
            winner_rate: 0.0,
            manipulation_rate: 0.0,
            perception_rate: 0.0,
            carry_rate: 0.0,
            valence: 0.0,
            arousal: 0.0,
            conflict: 0.0,
        })
    }

    fn observe_attention(&mut self, net: &Network) {
        for index in 0..self.attention_pop.size {
            if net.spiked[self.attention_pop.start + index] != 0 {
                self.attention_spikes[index / 2] += 1.0;
            }
        }
    }

    fn update(&mut self, net: &Network, dt_ms: f32) {
        let positive = net.rate("VALP");
        let negative = net.rate("VALN");
        let valence_raw = (positive - negative) / (positive + negative + 2.0);
        let rates = PRIMARY_COMMAND_NAMES.map(|name| net.rate(name).max(0.0));
        let sum = rates.iter().sum::<f32>();
        let arousal_raw = (0.60 * ((net.weight_gain - 1.0) / 1.2).clamp(0.0, 1.0)
            + 0.40 * (sum / PRIMARY_COMMAND_NAMES.len() as f32 / 45.0).clamp(0.0, 1.0))
        .clamp(0.0, 1.0);
        let conflict_raw = if sum > 0.001 {
            let entropy = rates.iter().fold(0.0, |entropy, rate| {
                let probability = rate / sum;
                if probability > 0.000_001 {
                    entropy - probability * probability.ln()
                } else {
                    entropy
                }
            });
            (entropy / (PRIMARY_COMMAND_NAMES.len() as f32).ln()).clamp(0.0, 1.0)
        } else {
            0.0
        };
        self.valence += (valence_raw - self.valence) * (1.0 - (-dt_ms / 700.0).exp());
        self.arousal += (arousal_raw - self.arousal) * (1.0 - (-dt_ms / 250.0).exp());
        self.conflict += (conflict_raw - self.conflict) * (1.0 - (-dt_ms / 300.0).exp());

        let (best_index, best_rate) = rates
            .iter()
            .copied()
            .enumerate()
            .max_by(|(_, left), (_, right)| left.total_cmp(right))
            .unwrap_or((0, 0.0));
        let candidate = MorphCommand::from_wire(PRIMARY_COMMAND_NAMES[best_index]);
        let threshold = if candidate == self.winner { 2.5 } else { 7.0 };
        self.winner = if best_rate > threshold {
            candidate
        } else {
            MorphCommand::Idle
        };
        self.winner_rate = best_rate;
        (self.manipulation, self.manipulation_rate) =
            group_winner(net, &MANIPULATION_COMMAND_NAMES, self.manipulation);
        (self.perception, self.perception_rate) =
            group_winner(net, &PERCEPTION_COMMAND_NAMES, self.perception);
        (self.carry, self.carry_rate) = group_winner(net, &CARRY_COMMAND_NAMES, self.carry);

        if let Some((index, score)) = self
            .attention_spikes
            .iter()
            .copied()
            .enumerate()
            .max_by(|(_, left), (_, right)| left.total_cmp(right))
            && score > 0.2
        {
            self.attention_winner = index;
        }
        for score in &mut self.attention_spikes {
            *score *= 0.88;
        }
    }

    fn output(&self, net: &Network, world: &MorphWorldInput) -> MorphOutput {
        let command_rates = COMMAND_NAMES.map(|name| net.rate(name));
        let attention_count = self.attention_spikes.len();
        let attention_object_slot =
            if (1..=MORPH_OBJECT_SLOT_COUNT).contains(&self.attention_winner) {
                Some((self.attention_winner - 1) as u8)
            } else {
                None
            };
        let attention = if self.attention_winner == 0 {
            MorphAttention::Cursor
        } else if self.attention_winner + 3 == attention_count {
            MorphAttention::Wander
        } else if self.attention_winner + 2 == attention_count {
            MorphAttention::Edge
        } else if self.attention_winner + 1 == attention_count {
            MorphAttention::SelfBody
        } else {
            MorphAttention::Object
        };
        let (manipulation, manipulation_rate) =
            gated_object_control(self.manipulation, self.manipulation_rate, world);
        let (perception, perception_rate) =
            gated_object_control(self.perception, self.perception_rate, world);
        let (carry, carry_rate) = gated_object_control(self.carry, self.carry_rate, world);
        MorphOutput {
            command: self.winner,
            manipulation,
            perception,
            carry,
            command_rates,
            winner_rate: self.winner_rate,
            manipulation_rate,
            perception_rate,
            carry_rate,
            confidence: (self.winner_rate / 45.0).clamp(0.0, 1.0) * (1.0 - self.conflict),
            attention,
            attention_object_slot,
            object_target: attention_object_slot
                .or(world.selected_slot)
                .and_then(|slot| world.objects.get(usize::from(slot)))
                .copied()
                .flatten()
                .map(|object| object.position),
            valence: self.valence.clamp(-1.0, 1.0),
            arousal: self.arousal.clamp(0.0, 1.0),
            conflict: self.conflict.clamp(0.0, 1.0),
            turn: ((net.rate("C_TURN_R") - net.rate("C_TURN_L")) / 45.0).clamp(-1.0, 1.0),
        }
    }
}

fn gated_object_control(
    command: MorphCommand,
    rate: f32,
    world: &MorphWorldInput,
) -> (MorphCommand, f32) {
    let enabled = ACTION_CONTROL_NAMES
        .iter()
        .position(|name| *name == command.as_wire())
        .is_some_and(|index| world.action_biases[index] > 0.05);
    if enabled {
        (command, rate)
    } else {
        (MorphCommand::Idle, 0.0)
    }
}

fn group_winner<const N: usize>(
    net: &Network,
    names: &[&str; N],
    previous: MorphCommand,
) -> (MorphCommand, f32) {
    let (index, rate) = names
        .iter()
        .map(|name| net.rate(name).max(0.0))
        .enumerate()
        .max_by(|(_, left), (_, right)| left.total_cmp(right))
        .unwrap_or((0, 0.0));
    let candidate = MorphCommand::from_wire(names[index]);
    let threshold = if candidate == previous { 2.5 } else { 7.0 };
    if rate > threshold {
        (candidate, rate)
    } else {
        (MorphCommand::Idle, rate)
    }
}

struct Plasticity {
    kc_start: usize,
    kc_trace: Vec<f32>,
    classical_edges: Vec<usize>,
    classical_pre: Vec<usize>,
    classical_sign: Vec<i8>,
    classical_weights: Vec<f32>,
    classical_initial: Vec<f32>,
    operant_edges: Vec<usize>,
    operant_pre: Vec<usize>,
    operant_command: Vec<u8>,
    operant_weights: Vec<f32>,
    operant_initial: Vec<f32>,
    operant_names: Vec<String>,
    bond_edges: Vec<usize>,
    dopamine_edges: Vec<usize>,
    last_command: MorphCommand,
    execution_age_ms: f32,
}

impl Plasticity {
    fn new(net: &Network, asset: &NetworkAsset) -> Self {
        let classical_weights = asset
            .meta
            .plastic
            .edge
            .iter()
            .map(|&edge| net.edge_weight[edge])
            .collect::<Vec<_>>();
        let operant_weights = asset
            .meta
            .operant
            .edge
            .iter()
            .map(|&edge| net.edge_weight[edge])
            .collect::<Vec<_>>();
        Self {
            kc_start: asset.meta.kc_start,
            kc_trace: vec![0.0; asset.meta.kc_size],
            classical_edges: asset.meta.plastic.edge.clone(),
            classical_pre: asset.meta.plastic.pre.clone(),
            classical_sign: asset.meta.plastic.sign.clone(),
            classical_initial: classical_weights.clone(),
            classical_weights,
            operant_edges: asset.meta.operant.edge.clone(),
            operant_pre: asset.meta.operant.pre.clone(),
            operant_command: asset.meta.operant.command.clone(),
            operant_initial: operant_weights.clone(),
            operant_weights,
            operant_names: asset.meta.operant.names.clone(),
            bond_edges: asset.meta.bond_edges.clone(),
            dopamine_edges: asset.meta.dopamine_edges.clone(),
            last_command: MorphCommand::Idle,
            execution_age_ms: 0.0,
        }
    }

    fn observe_spikes(&mut self, net: &Network) {
        for (index, trace) in self.kc_trace.iter_mut().enumerate() {
            if net.spiked[self.kc_start + index] != 0 {
                *trace += 1.0;
            }
        }
    }

    fn note_feedback(&mut self, net: &mut Network, reward: f32) {
        // Commit this executed action's outcome immediately on the simulation
        // thread. Another outcome before the slow tick cannot replace it.
        let command = std::mem::take(&mut self.last_command);
        if let Some(command_index) = self
            .operant_names
            .iter()
            .position(|name| name == command.as_wire())
        {
            for i in 0..self.operant_edges.len() {
                if self.operant_command[i] as usize == command_index {
                    let initial = self.operant_initial[i];
                    self.operant_weights[i] = (self.operant_weights[i]
                        + 0.0035 * self.kc_trace[self.operant_pre[i]] * reward)
                        .clamp(initial * 0.25, initial * 2.6);
                }
            }
            self.flush(net);
        }
    }

    fn update(&mut self, net: &mut Network, reward: f32, dt_ms: f32) {
        let decay = (-dt_ms / 900.0).exp();
        if reward.abs() > 0.004 {
            let carve = if reward > 0.0 { -1 } else { 1 };
            for index in 0..self.classical_edges.len() {
                if self.classical_sign[index] != carve {
                    continue;
                }
                let trace = self.kc_trace[self.classical_pre[index]];
                if trace <= 0.000_1 {
                    continue;
                }
                let minimum = self.classical_initial[index] * 0.20;
                self.classical_weights[index] = (self.classical_weights[index]
                    - 0.006 * trace * reward.abs())
                .clamp(minimum, self.classical_initial[index]);
            }
        }
        for trace in &mut self.kc_trace {
            *trace *= decay;
        }
        self.flush(net);
    }

    fn flush(&self, net: &mut Network) {
        for (index, &edge) in self.classical_edges.iter().enumerate() {
            net.edge_weight[edge] = self.classical_weights[index];
            net.edge_base[edge] = self.classical_weights[index];
        }
        for (index, &edge) in self.operant_edges.iter().enumerate() {
            net.edge_weight[edge] = self.operant_weights[index];
            net.edge_base[edge] = self.operant_weights[index];
        }
    }

    fn restore(&mut self, net: &mut Network, state: &MorphBrainState) {
        if state.plastic_weights.len() == self.classical_weights.len() {
            for (index, &weight) in state.plastic_weights.iter().enumerate() {
                self.classical_weights[index] = weight.clamp(
                    self.classical_initial[index] * 0.20,
                    self.classical_initial[index],
                );
            }
        }
        if state.operant_weights.len() == self.operant_weights.len() {
            for (index, &weight) in state.operant_weights.iter().enumerate() {
                self.operant_weights[index] = weight.clamp(
                    self.operant_initial[index] * 0.25,
                    self.operant_initial[index] * 2.6,
                );
            }
        }
        self.flush(net);
    }
}

fn drive_feature(net: &mut Network, name: &str, value: f32) {
    net.drive(name, 0.85 + 1.05 * value.clamp(0.0, 1.0));
}

fn sanitize_rate(value: f32) -> f32 {
    if value.is_finite() {
        value.max(0.0)
    } else {
        0.0
    }
}

fn sanitize_signed(value: f32) -> f32 {
    if value.is_finite() { value } else { 0.0 }
}

fn weight_range(
    weights: &[f32],
    initial: &[f32],
    relative_lower_bound: f32,
    relative_upper_bound: f32,
) -> MorphWeightRange {
    let count = weights.len().min(initial.len());
    if count == 0 {
        return MorphWeightRange {
            relative_lower_bound,
            relative_upper_bound,
            ..MorphWeightRange::default()
        };
    }

    let mut minimum = f32::INFINITY;
    let mut maximum = f32::NEG_INFINITY;
    let mut relative_minimum = f32::INFINITY;
    let mut relative_maximum = f32::NEG_INFINITY;
    for (&weight, &baseline) in weights.iter().zip(initial).take(count) {
        let weight = sanitize_signed(weight);
        let relative = if baseline.is_finite() && baseline.abs() > f32::EPSILON {
            weight / baseline
        } else {
            1.0
        };
        minimum = minimum.min(weight);
        maximum = maximum.max(weight);
        relative_minimum = relative_minimum.min(relative);
        relative_maximum = relative_maximum.max(relative);
    }

    MorphWeightRange {
        count,
        minimum,
        maximum,
        relative_minimum,
        relative_maximum,
        relative_lower_bound,
        relative_upper_bound,
    }
}

#[derive(Clone, Copy)]
enum MorphObjectFeature {
    Size,
    Roundness,
    Red,
    Green,
    Blue,
    State,
    Familiarity,
    Edge,
    Motion,
    Luminance,
    Texture,
}

fn drive_object_slots(
    net: &mut Network,
    name: &str,
    world: &MorphWorldInput,
    feature: MorphObjectFeature,
) {
    let Some(population) = net.pop(name) else {
        return;
    };
    for index in 0..population.size {
        let value = world
            .objects
            .get(index)
            .copied()
            .flatten()
            .map(|object| match feature {
                MorphObjectFeature::Size => object.size,
                MorphObjectFeature::Roundness => object.roundness,
                MorphObjectFeature::Red => object.color_rgb[0],
                MorphObjectFeature::Green => object.color_rgb[1],
                MorphObjectFeature::Blue => object.color_rgb[2],
                MorphObjectFeature::State => object.state,
                MorphObjectFeature::Familiarity => object.familiarity,
                MorphObjectFeature::Edge => object.edge,
                MorphObjectFeature::Motion => object.motion,
                MorphObjectFeature::Luminance => object.luminance,
                MorphObjectFeature::Texture => object.texture,
            });
        net.external[population.start + index] =
            value.map_or(0.0, |value| 0.85 + 1.05 * value.clamp(0.0, 1.0));
    }
}

fn normalize_signature(signature: &mut [f32]) {
    let maximum = signature.iter().copied().fold(0.0_f32, f32::max);
    if maximum <= f32::EPSILON {
        return;
    }
    for value in signature {
        *value = (*value / maximum).powi(6);
    }
}

fn fold_seed(seed: u64) -> u32 {
    let mixed = seed ^ (seed >> 32);
    let folded = mixed as u32;
    if folded == 0 { 0x9e37_79b9 } else { folded }
}

#[derive(Clone, Copy)]
struct XorShift32 {
    state: u32,
}

impl XorShift32 {
    fn new(seed: u32) -> Self {
        Self {
            state: if seed == 0 { 0x9e37_79b9 } else { seed },
        }
    }

    fn next_u32(&mut self) -> u32 {
        let mut state = self.state;
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        self.state = state;
        state
    }

    fn next_f32(&mut self) -> f32 {
        self.next_f64() as f32
    }

    fn next_f64(&mut self) -> f64 {
        f64::from(self.next_u32()) / 4_294_967_296.0
    }
}

fn gaussian_pool(rng: &mut XorShift32, size: usize) -> Vec<f32> {
    let mut pool = vec![0.0; size];
    for index in (0..size).step_by(2) {
        let uniform = rng.next_f64().max(0.000_000_001);
        let radius = (-2.0 * uniform.ln()).sqrt();
        let theta = std::f64::consts::TAU * rng.next_f64();
        pool[index] = (radius * theta.cos()) as f32;
        if index + 1 < size {
            pool[index + 1] = (radius * theta.sin()) as f32;
        }
    }
    pool
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec2;
    use lifecore::{Genome, LifeCore};
    use serde::Deserialize;

    #[derive(Deserialize)]
    struct NeuralParityFixture {
        schema: u32,
        upstream_commit: String,
        seed: u32,
        frames: usize,
        frame_ms: usize,
        rates: Vec<f32>,
        voltage: Vec<f32>,
        adaptation: Vec<f32>,
        std_x: Vec<f32>,
    }

    #[test]
    fn close_outcomes_do_not_overwrite_each_other() {
        let mut brain = MorphBrain::new(17, None).unwrap();
        brain.plasticity.kc_trace.fill(1.0);
        brain.acknowledge_execution(MorphCommand::Play);
        brain.apply_feedback(&FeedbackEvent::Reward(1.0));
        let first = brain.plasticity.operant_weights.clone();
        brain.apply_feedback(&FeedbackEvent::Reward(-1.0));
        assert_eq!(first, brain.plasticity.operant_weights);
        brain.acknowledge_execution(MorphCommand::Groom);
        brain.apply_feedback(&FeedbackEvent::Reward(1.0));
        assert!(first != brain.plasticity.operant_weights);
        for (i, w) in first.iter().enumerate() {
            if brain.plasticity.operant_names[brain.plasticity.operant_command[i] as usize]
                == MorphCommand::Play.as_wire()
            {
                assert_eq!(*w, brain.plasticity.operant_weights[i]);
            }
        }
    }

    #[test]
    fn reward_credit_is_for_executed_command_and_cannot_move_to_later_proposal() {
        let mut brain = MorphBrain::new(17, None).unwrap();
        let before = brain.plasticity.operant_weights.clone();
        brain.plasticity.kc_trace.fill(1.0);
        brain.apply_feedback(&FeedbackEvent::Reward(1.0));
        brain.plasticity.update(&mut brain.net, 0.0, 100.0);
        assert_eq!(
            brain.plasticity.operant_weights, before,
            "a proposal alone has no execution credit"
        );
        brain.acknowledge_execution(MorphCommand::Play);
        brain.apply_feedback(&FeedbackEvent::Reward(1.0));
        brain.acknowledge_execution(MorphCommand::Groom);
        brain.plasticity.update(&mut brain.net, 0.0, 100.0);
        for (i, weight) in brain.plasticity.operant_weights.iter().enumerate() {
            let name =
                &brain.plasticity.operant_names[brain.plasticity.operant_command[i] as usize];
            if name != MorphCommand::Play.as_wire() {
                assert_eq!(*weight, before[i]);
            }
        }
        assert!(
            brain.plasticity.operant_weights != before,
            "executed Play must learn"
        );
        let weights = brain.plasticity.operant_weights.clone();
        brain.plasticity.update(&mut brain.net, 0.0, 100.0);
        assert_eq!(
            brain.plasticity.operant_weights, weights,
            "one outcome is consumed once"
        );
        let restored = MorphBrain::new(17, Some(brain.snapshot())).unwrap();
        assert_eq!(restored.reward_trace, 0.0);
        assert_eq!(restored.plasticity.operant_weights, weights);
    }

    #[test]
    fn embedded_asset_is_the_real_pinned_topology() {
        let brain = MorphBrain::new(7, None).unwrap();
        assert_eq!(brain.network_shape(), (526, 17_475, 57));
        assert_eq!(network_asset().unwrap().upstream_commit, UPSTREAM_COMMIT);
    }

    #[test]
    fn equal_seed_and_trace_are_deterministic() {
        let life = LifeCore::new(Genome::from_seed(11), 13);
        let mut first = MorphBrain::new(17, None).unwrap();
        let mut second = MorphBrain::new(17, None).unwrap();
        let mut sensors = SensorFrame::default();
        let body = BodyFeedback::default();
        let mut a = MorphOutput::default();
        let mut b = MorphOutput::default();
        for tick in 0..40 {
            sensors.cursor_position = Vec2::new(0.5 + tick as f32 * 0.002, 0.4);
            sensors.cursor_distance_to_pet = 0.35 - tick as f32 * 0.004;
            sensors.cursor_approach_speed = 0.08;
            a = first.tick(&sensors, &body, &life.state, 0.05);
            b = second.tick(&sensors, &body, &life.state, 0.05);
        }
        assert_eq!(a, b);
        assert!(a.command_rates.iter().all(|rate| rate.is_finite()));
    }

    #[test]
    fn learned_weights_roundtrip_and_remain_bounded() {
        let life = LifeCore::new(Genome::from_seed(21), 23);
        let mut brain = MorphBrain::new(29, None).unwrap();
        let sensors = SensorFrame {
            cursor_distance_to_pet: 0.08,
            pet_touched: true,
            ..SensorFrame::default()
        };
        let body = BodyFeedback {
            cursor_contact: true,
            ..BodyFeedback::default()
        };
        let newborn = brain.snapshot();
        for _ in 0..30 {
            let _ = brain.tick(&sensors, &body, &life.state, 0.05);
        }
        brain.apply_feedback(&FeedbackEvent::PettingStarted);
        for _ in 0..8 {
            let _ = brain.tick(&sensors, &body, &life.state, 0.05);
        }
        let snapshot = brain.snapshot();
        assert!(snapshot.is_valid());
        assert!(
            snapshot.plastic_weights != newborn.plastic_weights
                || snapshot.operant_weights != newborn.operant_weights
        );
        let restored = MorphBrain::new(29, Some(snapshot.clone())).unwrap();
        assert_eq!(
            restored.snapshot().plastic_weights,
            snapshot.plastic_weights
        );
        assert_eq!(
            restored.snapshot().operant_weights,
            snapshot.operant_weights
        );
    }

    #[test]
    fn object_slots_drive_the_parallel_manipulation_control_without_stealing_locomotion() {
        let life = LifeCore::new(Genome::from_seed(31), 37);
        let mut brain = MorphBrain::new(41, None).unwrap();
        let sensors = SensorFrame::default();
        let body = BodyFeedback {
            world_position: Vec2::splat(0.5),
            ..BodyFeedback::default()
        };
        let mut world = MorphWorldInput {
            selected_slot: Some(0),
            ..MorphWorldInput::default()
        };
        world.objects[0] = Some(MorphObjectInput {
            position: Vec2::new(0.56, 0.5),
            salience: 1.0,
            motion: 0.6,
            size: 0.5,
            roundness: 1.0,
            color_rgb: [0.9, 0.3, 0.1],
            state: 1.0,
            familiarity: 0.4,
            edge: 0.0,
            luminance: 0.8,
            texture: 0.5,
        });
        world.action_biases[1] = 2.4;

        let mut saw_push = false;
        let mut saw_primary = false;
        for _ in 0..120 {
            let output = brain.tick_with_world(&sensors, &body, &life.state, &world, 0.05);
            saw_push |= output.manipulation == MorphCommand::Push;
            saw_primary |= output.command != MorphCommand::Idle;
            assert_eq!(output.command_rates.len(), MORPH_COMMAND_COUNT);
            assert_eq!(output.object_target, Some(Vec2::new(0.56, 0.5)));
        }
        assert!(
            saw_push,
            "the dedicated manipulation WTA never emitted PUSH"
        );
        assert!(
            saw_primary,
            "parallel object control incorrectly silenced the locomotor WTA"
        );
    }

    #[test]
    fn diagnostics_are_typed_finite_and_observational_only() {
        let life = LifeCore::new(Genome::from_seed(0xD1A6), 0x1057);
        let mut observed = MorphBrain::new(0xA11D, None).unwrap();
        let mut control = MorphBrain::new(0xA11D, None).unwrap();
        let mut sensors = SensorFrame::default();
        let body = BodyFeedback::default();

        for tick in 0..48 {
            sensors.cursor_position = Vec2::new(0.18 + tick as f32 * 0.009, 0.62);
            sensors.cursor_distance_to_pet = (0.42 - tick as f32 * 0.006).max(0.04);
            sensors.cursor_approach_speed = 0.11;
            let observed_output = observed.tick(&sensors, &body, &life.state, 0.025);
            let diagnostics = observed.diagnostics();
            let control_output = control.tick(&sensors, &body, &life.state, 0.025);

            assert_eq!(observed_output, control_output);
            assert_eq!(diagnostics.upstream_age_ms, (tick + 1) as f64 * 25.0);
            assert!(diagnostics.reward_trace.is_finite());
            assert!(diagnostics.current_spike_count <= observed.network_shape().0);
            assert!(diagnostics.mean_population_rate.is_finite());
            assert!(diagnostics.max_population_rate.is_finite());
            assert!(diagnostics.mean_population_rate >= 0.0);
            assert!(diagnostics.max_population_rate >= diagnostics.mean_population_rate);
            assert_eq!(diagnostics.population_rates.exp, observed.net.rate("EXP"));
            assert_eq!(diagnostics.population_rates.prox, observed.net.rate("PROX"));
            assert_eq!(diagnostics.population_rates.mot, observed.net.rate("MOT"));
            assert_eq!(diagnostics.population_rates.tch, observed.net.rate("TCH"));
            assert_eq!(diagnostics.population_rates.vib, observed.net.rate("VIB"));
            assert_eq!(diagnostics.population_rates.loom, observed.net.rate("LOOM"));
            assert_eq!(diagnostics.population_rates.hab, observed.net.rate("HAB"));
            assert_eq!(diagnostics.population_rates.nov, observed.net.rate("NOV"));
            assert_eq!(diagnostics.population_rates.kc, observed.net.rate("KC"));
            assert_eq!(diagnostics.population_rates.valp, observed.net.rate("VALP"));
            assert_eq!(diagnostics.population_rates.valn, observed.net.rate("VALN"));
            assert_eq!(
                diagnostics.population_rates.mbon_a,
                observed.net.rate("MBON_A")
            );
            assert_eq!(
                diagnostics.population_rates.mbon_v,
                observed.net.rate("MBON_V")
            );
            assert_eq!(diagnostics.population_rates.att, observed.net.rate("ATT"));
            assert_eq!(diagnostics.population_rates.rest, observed.net.rate("REST"));
        }

        assert_eq!(observed.snapshot(), control.snapshot());
    }

    #[test]
    fn diagnostics_report_the_enforced_classical_and_operant_weight_bounds() {
        let brain = MorphBrain::new(0x00B0_A1D5, None).unwrap();
        let mut state = brain.snapshot();
        for (index, weight) in state.plastic_weights.iter_mut().enumerate() {
            *weight = if index % 2 == 0 { -100.0 } else { 100.0 };
        }
        for (index, weight) in state.operant_weights.iter_mut().enumerate() {
            *weight = if index % 2 == 0 { -100.0 } else { 100.0 };
        }
        let restored = MorphBrain::new(0x00B0_A1D5, Some(state)).unwrap();
        let diagnostics = restored.diagnostics();

        assert_eq!(
            diagnostics.classical_weights.count,
            restored.plasticity.classical_weights.len()
        );
        assert_eq!(
            diagnostics.operant_weights.count,
            restored.plasticity.operant_weights.len()
        );
        assert!(diagnostics.classical_weights.minimum > 0.0);
        assert!(diagnostics.operant_weights.minimum > 0.0);
        assert!(
            diagnostics.classical_weights.relative_minimum
                >= diagnostics.classical_weights.relative_lower_bound - 1.0e-6
        );
        assert!(
            diagnostics.classical_weights.relative_maximum
                <= diagnostics.classical_weights.relative_upper_bound + 1.0e-6
        );
        assert!(
            diagnostics.operant_weights.relative_minimum
                >= diagnostics.operant_weights.relative_lower_bound - 1.0e-6
        );
        assert!(
            diagnostics.operant_weights.relative_maximum
                <= diagnostics.operant_weights.relative_upper_bound + 1.0e-6
        );
    }

    #[test]
    fn rust_lif_engine_matches_the_original_javascript_golden_trace() {
        let fixture: NeuralParityFixture =
            serde_json::from_str(include_str!("../assets/neural-parity.json")).unwrap();
        assert_eq!(fixture.schema, 1);
        assert_eq!(fixture.upstream_commit, UPSTREAM_COMMIT);
        let asset = network_asset().unwrap();
        let mut net = Network::new(asset, fixture.seed).unwrap();
        for frame in 0..fixture.frames {
            net.external.fill(0.0);
            let phase = frame as f32 / (fixture.frames - 1) as f32;
            net.drive(
                "EXP",
                0.85 + 1.05 * (1.0 - (phase - 0.25).abs() * 4.0).max(0.0),
            );
            net.drive("PROX", 0.85 + 1.05 * phase);
            net.drive(
                "MOT",
                0.85 + 1.05 * (0.5 + 0.5 * (frame as f32 * 0.31).sin()),
            );
            net.drive("TCH", 0.85 + 1.05 * if frame >= 24 { 0.72 } else { 0.0 });
            net.drive("VIB", 0.85 + 1.05 * if frame == 12 { 0.9 } else { 0.0 });
            net.drive("LGT", 1.35);
            net.drive("SLF", 1.02);
            net.drive("REST", 1.75 + 0.2 * (frame as f32 * 0.17).cos());
            net.drive("MBON_A", 0.55);
            net.drive("MBON_V", 0.55);
            net.drive("VALP", 1.62 + if frame >= 24 { 0.32 } else { 0.0 });
            net.drive("VALN", 1.62 + if frame == 12 { 0.45 } else { 0.0 });
            net.drive("CPG_a", 1.50);
            net.drive("CPG_b", 1.47);
            net.drive("C_APPR", if frame < 20 { 0.42 } else { 0.12 });
            net.drive(
                "C_PERK",
                if (10..24).contains(&frame) {
                    0.58
                } else {
                    0.10
                },
            );
            net.drive("C_PLAY", if frame >= 24 { 0.64 } else { 0.08 });
            net.drive("C_GROOM", 0.14);
            net.drive("C_MELT", 0.06);
            net.drive("C_FLEE", 0.0);
            let mut done = 0;
            while done < fixture.frame_ms {
                let batch = (fixture.frame_ms - done).min(8);
                for _ in 0..batch {
                    net.step();
                }
                net.update_rates(batch);
                done += batch;
            }
        }
        assert_vectors_close("rates", &net.rates, &fixture.rates, 0.000_8);
        assert_vectors_close("voltage", &net.voltage, &fixture.voltage, 0.000_8);
        assert_vectors_close("adaptation", &net.adaptation, &fixture.adaptation, 0.000_8);
        assert_vectors_close("std_x", &net.std_x, &fixture.std_x, 0.000_8);
    }

    fn assert_vectors_close(name: &str, actual: &[f32], expected: &[f32], tolerance: f32) {
        assert_eq!(actual.len(), expected.len(), "{name} length");
        let (index, error) = actual
            .iter()
            .zip(expected)
            .enumerate()
            .map(|(index, (actual, expected))| (index, (actual - expected).abs()))
            .max_by(|(_, left), (_, right)| left.total_cmp(right))
            .unwrap_or((0, 0.0));
        assert!(
            error <= tolerance,
            "{name} max error {error} at {index}, actual {}, expected {}",
            actual[index],
            expected[index]
        );
    }
}
