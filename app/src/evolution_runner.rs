use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fs::{self, File, OpenOptions},
    io::{BufReader, BufWriter, Write},
    path::{Path, PathBuf},
    time::Instant,
};

use desktop_host::{
    EVOLUTION_CONFIG_SCHEMA_VERSION, EVOLUTION_PROGRESS_SCHEMA_VERSION,
    EVOLUTION_REPORT_SCHEMA_VERSION, EvolutionAudioSummary, EvolutionCheckpoint, EvolutionConfig,
    EvolutionEligibility, EvolutionInvariantSummary, EvolutionOutcomeModel,
    EvolutionPerformanceSummary, EvolutionPersistence, EvolutionPolicy, EvolutionPreset,
    EvolutionProgress, EvolutionProgressStage, EvolutionReplicateSummary, EvolutionRunReport,
    PORTABLE_STATE_SCHEMA_VERSION, PortablePetState, QuietAdvanceMode, StateStore,
};
use glam::Vec2;
use lifecore::{
    BodyIntent, EmbodiedGestureKind, ExpressionDirector, ExpressionState, FeedbackEvent,
    InteractionOutcome, InteractionOutcomeKind, InteractionTurnState, LifeCore, LifeSnapshot,
    LivingStateFrame, LocomotionMode, PhysicalExpressionContext, PoseIntent, SensorFrame,
    VitaState, persisted_life_snapshot_hash,
};
use morph_brain::{MorphBrain, MorphBrainState, MorphWorldInput};
use pet_audio::{OfflinePcm, OfflineSampleFormat, export_debug_wav, render_motif};
use pet_body::{
    BodyMaterialSnapshot, LiquidTuningProfile, ProceduralBody, VisualMindInput, VoiceVisualState,
};
use pet_ecology::EcologyState;

use crate::{Arguments, BrainMode, PreparedState, VitaRuntime, classifier_tuning};

const BASE_HZ: u64 = 120;
const PERCEPTION_DIVISOR: u64 = 2;
const LIFE_DIVISOR: u64 = 6;
const TELEMETRY_DIVISOR: u64 = 24;
const BODY_DT: f32 = 1.0 / BASE_HZ as f32;
const PERCEPTION_DT: f32 = PERCEPTION_DIVISOR as f32 / BASE_HZ as f32;
const LIFE_DT: f32 = LIFE_DIVISOR as f32 / BASE_HZ as f32;
const SEPARATION_SETTLE_SECONDS: f32 = 30.0;

#[derive(Clone)]
struct InitialState {
    life: LifeSnapshot,
    vita: VitaState,
    morph: MorphBrainState,
    ecology: EcologyState,
    position: desktop_host::PersistedPetPosition,
    tuning: LiquidTuningProfile,
    body: BodyMaterialSnapshot,
}

struct ReplicateResult {
    life: LifeCore,
    vita: VitaRuntime,
    morph: MorphBrain,
    body: ProceduralBody,
    summary: EvolutionReplicateSummary,
    checkpoints: Vec<EvolutionCheckpoint>,
    quiet_or_no_response_episodes: u32,
    safe_boundary_episodes: u32,
    sleep_consolidations: u32,
    body_timings_us: Vec<f64>,
    base_ticks: u64,
}

const MAX_EXPORTED_WAVS_PER_REPLICATE: u32 = 24;

struct OfflineAudioRecorder {
    directory: PathBuf,
    replicate_index: u32,
    summary: EvolutionAudioSummary,
    rms_sum: f64,
}

impl OfflineAudioRecorder {
    fn new(directory: PathBuf, replicate_index: u32) -> Self {
        Self {
            directory,
            replicate_index,
            summary: EvolutionAudioSummary {
                rms_min: f32::MAX,
                ..EvolutionAudioSummary::default()
            },
            rms_sum: 0.0,
        }
    }

    fn render(&mut self, life: &mut LifeCore, request: lifecore::VocalRequest) {
        let Some(motif) = life
            .state
            .vocal_motifs
            .iter()
            .find(|motif| motif.id == request.motif_id)
            .cloned()
        else {
            life.cancel_vocal_request(request.performance_seed);
            return;
        };
        let OfflinePcm::F32(samples) = render_motif(
            &life.state.genome.voice,
            &motif,
            &request,
            48_000,
            1,
            OfflineSampleFormat::F32,
        ) else {
            unreachable!("offline runner requests f32 PCM")
        };
        let count = samples.len().max(1) as f32;
        let rms = (samples
            .iter()
            .map(|sample| f64::from(*sample) * f64::from(*sample))
            .sum::<f64>()
            / f64::from(count))
        .sqrt() as f32;
        let peak = samples
            .iter()
            .copied()
            .map(f32::abs)
            .fold(0.0_f32, f32::max);
        let zero_crossings = samples
            .windows(2)
            .filter(|pair| pair[0].is_sign_positive() != pair[1].is_sign_positive())
            .count() as f32;
        let zcr = zero_crossings / count;
        self.summary.rendered_count = self.summary.rendered_count.saturating_add(1);
        self.summary.total_rendered_seconds += samples.len() as f64 / 48_000.0;
        self.summary.rms_min = self.summary.rms_min.min(rms);
        self.summary.rms_max = self.summary.rms_max.max(rms);
        self.summary.peak_max = self.summary.peak_max.max(peak);
        self.rms_sum += f64::from(zcr);
        if self.summary.exported_wav_count < MAX_EXPORTED_WAVS_PER_REPLICATE
            && fs::create_dir_all(&self.directory).is_ok()
        {
            let path = self.directory.join(format!(
                "replicate-{:02}-voice-{:04}-{:016x}.wav",
                self.replicate_index, self.summary.rendered_count, request.performance_seed
            ));
            if export_debug_wav(&path, &samples, 48_000, 1).is_ok() {
                self.summary.exported_wav_count = self.summary.exported_wav_count.saturating_add(1);
                self.summary.wav_artifacts.push(path.display().to_string());
            }
        }
        let _ = life.confirm_vocal_request_rendered_offline(request.performance_seed);
    }

    fn finish(mut self) -> EvolutionAudioSummary {
        if self.summary.rendered_count == 0 {
            self.summary.rms_min = 0.0;
        } else {
            self.summary.zero_crossing_rate_mean =
                (self.rms_sum / f64::from(self.summary.rendered_count)) as f32;
        }
        self.summary
    }
}

struct ProgressWriter {
    path: Option<PathBuf>,
    started: Instant,
    progress: EvolutionProgress,
    finished: bool,
}

impl ProgressWriter {
    fn new(path: Option<PathBuf>, replicate_count: u32, episode_count: u32) -> Self {
        Self {
            path,
            started: Instant::now(),
            progress: EvolutionProgress {
                schema_version: EVOLUTION_PROGRESS_SCHEMA_VERSION,
                stage: EvolutionProgressStage::Starting,
                replicate_index: 0,
                replicate_count,
                episode_index: 0,
                episode_count,
                simulated_seconds: 0.0,
                audio_render_count: 0,
                learning_update_count: 0,
                elapsed_wall_seconds: 0.0,
                eta_seconds: None,
                last_error: None,
            },
            finished: false,
        }
    }

    fn write(&mut self) {
        self.progress.elapsed_wall_seconds = self.started.elapsed().as_secs_f64();
        let fraction = f64::from(self.progress.fraction());
        self.progress.eta_seconds = (fraction > 0.0 && fraction < 1.0)
            .then(|| self.progress.elapsed_wall_seconds * (1.0 - fraction) / fraction);
        if let Some(path) = &self.path
            && let Err(error) = atomic_json(path, &self.progress, false)
        {
            eprintln!("failed to update evolution progress: {error}");
        }
        println!(
            "progress stage={:?} replicate={}/{} episode={}/{} sim_time={:.2}s audio_render_count={} learning_update_count={} eta={:?} last_error={:?}",
            self.progress.stage,
            self.progress
                .replicate_index
                .saturating_add(1)
                .min(self.progress.replicate_count),
            self.progress.replicate_count,
            self.progress.episode_index,
            self.progress.episode_count,
            self.progress.simulated_seconds,
            self.progress.audio_render_count,
            self.progress.learning_update_count,
            self.progress.eta_seconds,
            self.progress.last_error,
        );
    }

    fn complete(&mut self) {
        self.progress.stage = EvolutionProgressStage::Completed;
        self.progress.replicate_index = self.progress.replicate_count;
        self.progress.episode_index = self.progress.episode_count;
        self.finished = true;
        self.write();
    }
}

impl Drop for ProgressWriter {
    fn drop(&mut self) {
        if !self.finished {
            self.progress.stage = EvolutionProgressStage::Failed;
            self.progress.last_error =
                Some("runner exited before completion; inspect runner.log".into());
            self.write();
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CurriculumFixture {
    SoftTouch,
    SlowStretch,
    Tickle,
    RhythmicTouch,
    CircularTwist,
    SharpFlick,
    Hold,
    PullRelease,
    AllowedSeparation,
    BlockedSeparation,
    FragmentHelp,
    NoResponse,
    SleepQuietContact,
}

const CURRICULUM_V1: [CurriculumFixture; 13] = [
    CurriculumFixture::SoftTouch,
    CurriculumFixture::SlowStretch,
    CurriculumFixture::Tickle,
    CurriculumFixture::RhythmicTouch,
    CurriculumFixture::CircularTwist,
    CurriculumFixture::SharpFlick,
    CurriculumFixture::Hold,
    CurriculumFixture::PullRelease,
    CurriculumFixture::AllowedSeparation,
    CurriculumFixture::BlockedSeparation,
    CurriculumFixture::FragmentHelp,
    CurriculumFixture::NoResponse,
    CurriculumFixture::SleepQuietContact,
];

#[derive(Debug, Clone, Copy, Default)]
struct PointerSample {
    local: Vec2,
    velocity_local: Vec2,
    down: bool,
    pressed: bool,
    released: bool,
}

struct FixtureRuntime {
    kind: CurriculumFixture,
    active_seconds: f32,
    intensity: f32,
    previous_local: Vec2,
    previous_down: bool,
}

impl FixtureRuntime {
    fn new(kind: CurriculumFixture, seed: u64, episode: u32) -> Self {
        let mixed = seed.wrapping_add(u64::from(episode).wrapping_mul(0x9E37_79B9_7F4A_7C15));
        let variation = f32::from((mixed >> 17) as u8) / 255.0;
        Self {
            kind,
            active_seconds: match kind {
                CurriculumFixture::SharpFlick => 0.24,
                CurriculumFixture::SoftTouch => 0.70,
                CurriculumFixture::Tickle | CurriculumFixture::RhythmicTouch => 1.25,
                CurriculumFixture::Hold | CurriculumFixture::SleepQuietContact => 1.35,
                CurriculumFixture::AllowedSeparation
                | CurriculumFixture::BlockedSeparation
                | CurriculumFixture::FragmentHelp => 3.60,
                _ => 1.10,
            },
            intensity: (0.82 + variation * 0.16).clamp(0.0, 1.0),
            previous_local: Vec2::ZERO,
            previous_down: false,
        }
    }

    fn sample(&mut self, elapsed: f32) -> PointerSample {
        let phase = (elapsed / self.active_seconds).clamp(0.0, 1.0);
        let strength = self.intensity;
        let (local, down) = match self.kind {
            CurriculumFixture::SoftTouch | CurriculumFixture::NoResponse => {
                (Vec2::new(-0.045, 0.015), phase < 0.88)
            }
            CurriculumFixture::SlowStretch => (
                Vec2::new(-0.055 + phase.min(0.88) * 0.28 * strength, 0.01),
                phase < 0.90,
            ),
            CurriculumFixture::Tickle => (
                Vec2::new(
                    -0.025 + (phase * std::f32::consts::TAU * 8.0).sin() * 0.050,
                    (phase * std::f32::consts::TAU * 11.0).sin() * 0.032,
                ),
                phase < 0.92,
            ),
            CurriculumFixture::RhythmicTouch => {
                let beat = (phase * 3.0).fract();
                (Vec2::new(-0.045, 0.012), phase < 0.94 && beat < 0.48)
            }
            CurriculumFixture::CircularTwist => {
                let angle = phase * std::f32::consts::TAU * 1.10;
                (Vec2::new(angle.cos(), angle.sin()) * 0.14, phase < 0.92)
            }
            CurriculumFixture::SharpFlick => (
                Vec2::new(-0.11 + phase.min(0.70) / 0.70 * 0.42 * strength, -0.01),
                phase < 0.70,
            ),
            CurriculumFixture::Hold => (Vec2::new(-0.040, 0.018), phase < 0.94),
            CurriculumFixture::PullRelease => (
                Vec2::new(-0.05 + phase.min(0.76) / 0.76 * 0.36 * strength, 0.0),
                phase < 0.76,
            ),
            CurriculumFixture::AllowedSeparation => (
                Vec2::new(0.24 + phase.min(0.78) / 0.78 * 0.58 * strength, 0.03),
                phase < 0.80,
            ),
            CurriculumFixture::BlockedSeparation => (
                Vec2::new(0.24 + phase.min(0.84) / 0.84 * 0.76 * strength, 0.03),
                phase < 0.86,
            ),
            CurriculumFixture::FragmentHelp => {
                if phase < 0.55 {
                    (
                        Vec2::new(0.24 + phase / 0.55 * 0.58 * strength, 0.025),
                        true,
                    )
                } else if phase < 0.65 {
                    (Vec2::new(0.82, 0.025), false)
                } else {
                    let help = ((phase - 0.65) / 0.30).clamp(0.0, 1.0);
                    (Vec2::new(0.82 - help * 0.70, 0.025), phase < 0.95)
                }
            }
            CurriculumFixture::SleepQuietContact => (Vec2::new(-0.035, 0.015), phase < 0.42),
        };
        let velocity_local = (local - self.previous_local) / BODY_DT;
        let sample = PointerSample {
            local,
            velocity_local,
            down,
            pressed: down && !self.previous_down,
            released: !down && self.previous_down,
        };
        self.previous_local = local;
        self.previous_down = down;
        sample
    }
}

pub(crate) fn run(
    arguments: &Arguments,
    store: &StateStore,
    prepared: PreparedState,
) -> Result<(), Box<dyn Error>> {
    let mut config = load_config(arguments)?;
    if let Some(persistence) = arguments.evolution_persist {
        config.persistence = persistence;
    }
    if let Some(maximum) = arguments.evolution_max_generations {
        config.maximum_generations = maximum;
    }
    config.validate()?;
    let report_path = arguments
        .evolution_report
        .clone()
        .unwrap_or_else(|| store.paths.evolution_runs.join("latest-report.json"));
    let audio_directory = report_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("audio");
    let mut progress = ProgressWriter::new(
        arguments.evolution_progress.clone(),
        config.replicate_count,
        config.scheduled_episode_count(),
    );
    progress.write();

    if arguments.simulate_hours.is_some() {
        eprintln!(
            "warning: --simulate-hours now uses explicit approximate calendar-only advancement; no synthetic interaction learning is generated"
        );
    }

    let PreparedState {
        life,
        loaded_life_state_hash: _,
        position,
        vita,
        morph,
        ecology,
    } = prepared;
    let mut body = ProceduralBody::generate(&life.state.genome)?;
    let stored_tuning = store.load_liquid_tuning::<LiquidTuningProfile>()?;
    let tuning = stored_tuning
        .unwrap_or_else(|| {
            super::approved_production_liquid_tuning(life.state.genome.identity_seed)
        })
        .sanitized()?;
    let tuning = super::production_liquid_tuning(tuning);
    body.apply_tuning_profile(tuning.clone())?;
    if !arguments.reset_pet && arguments.import_state.is_none() {
        let _ = super::load_restore_body_state(store, &mut body)?;
    }
    let initial = InitialState {
        life: life.snapshot(),
        vita: vita.snapshot(),
        morph: morph.snapshot(),
        ecology: ecology.snapshot(),
        position,
        tuning,
        body: body.body_material_snapshot(),
    };

    let started = Instant::now();
    let mut results = Vec::with_capacity(config.replicate_count as usize);
    for replicate in 0..config.replicate_count {
        progress.progress.stage = EvolutionProgressStage::Replicate;
        progress.progress.replicate_index = replicate;
        progress.progress.episode_index = 0;
        progress.write();
        results.push(run_replicate(
            &config,
            &initial,
            replicate,
            &audio_directory,
            &mut progress,
        )?);
    }
    progress.progress.stage = EvolutionProgressStage::Finalizing;
    progress.write();
    let wall_seconds = started.elapsed().as_secs_f64();
    let all_invariants_passed = results
        .iter()
        .all(|result| result.summary.invariants.passed);
    let all_eligible = results
        .iter()
        .all(|result| result.summary.eligibility.eligible);
    let primary = results
        .first()
        .ok_or("evolution runner produced no deterministic replicate")?;
    let persisted_state = if all_invariants_passed {
        persist_accepted_state(&config, store, &initial, primary)?
    } else {
        None
    };
    let mut all_body_timings = results
        .iter()
        .flat_map(|result| result.body_timings_us.iter().copied())
        .collect::<Vec<_>>();
    all_body_timings.sort_by(f64::total_cmp);
    let completed_episodes = results
        .iter()
        .map(|result| result.summary.completed_episodes)
        .sum::<u32>();
    let performance = EvolutionPerformanceSummary {
        wall_seconds,
        active_body_steps: all_body_timings.len() as u64,
        body_step_p50_microseconds: percentile(&all_body_timings, 0.50),
        body_step_p95_microseconds: percentile(&all_body_timings, 0.95),
        body_step_max_microseconds: all_body_timings.last().copied().unwrap_or(0.0),
        episodes_per_wall_second: f64::from(completed_episodes) / wall_seconds.max(1.0e-9),
    };
    let report = EvolutionRunReport {
        schema_version: EVOLUTION_REPORT_SCHEMA_VERSION,
        config: config.clone(),
        status: if all_invariants_passed {
            if config.evolution_policy == EvolutionPolicy::Off || all_eligible {
                "completed".to_owned()
            } else {
                "completed_not_eligible".to_owned()
            }
        } else {
            "rejected_invariant_failure".to_owned()
        },
        clock_mode: config.quiet_advance,
        approximate_calendar_advance: config.quiet_advance == QuietAdvanceMode::CalendarOnly,
        base_hz: BASE_HZ as u32,
        perception_hz: (BASE_HZ / PERCEPTION_DIVISOR) as u32,
        life_hz: (BASE_HZ / LIFE_DIVISOR) as u32,
        telemetry_hz: (BASE_HZ / TELEMETRY_DIVISOR) as u32,
        simulated_seconds: primary.base_ticks as f64 / BASE_HZ as f64,
        base_ticks: primary.base_ticks,
        completed_episodes: primary.summary.completed_episodes,
        quiet_or_no_response_episodes: primary.quiet_or_no_response_episodes,
        safe_boundary_episodes: primary.safe_boundary_episodes,
        sleep_consolidations: primary.sleep_consolidations,
        gesture_distribution: primary.summary.gesture_distribution.clone(),
        lexicon_updates: primary.summary.lexicon_updates,
        convention_updates: 0,
        audio: primary.summary.audio.clone(),
        telemetry_samples: primary.summary.telemetry_samples,
        eligibility: primary.summary.eligibility.clone(),
        invariants: primary.summary.invariants.clone(),
        initial_generation: initial.life.state.genome.generation,
        final_generation: primary.life.state.genome.generation,
        initial_genome_hash: initial.life.state.genome.stable_hash(),
        final_genome_hash: primary.life.state.genome.stable_hash(),
        final_life_state_hash: life_hash(&primary.life)?,
        checkpoints: primary.checkpoints.clone(),
        replicates: results
            .iter()
            .map(|result| result.summary.clone())
            .collect(),
        performance,
        persisted_state,
    };
    atomic_report(&report_path, &report)?;
    progress.complete();
    println!("{}", serde_json::to_string_pretty(&report)?);
    if !all_invariants_passed {
        return Err(format!(
            "evolution rejected by invariants; report written to {}",
            report_path.display()
        )
        .into());
    }
    Ok(())
}

fn load_config(arguments: &Arguments) -> Result<EvolutionConfig, Box<dyn Error>> {
    if let Some(path) = &arguments.evolution_config {
        let config = serde_json::from_reader(BufReader::new(File::open(path)?))?;
        return Ok(config);
    }
    let hours = f64::from(arguments.simulate_hours.unwrap_or(1.0));
    Ok(EvolutionConfig {
        schema_version: EVOLUTION_CONFIG_SCHEMA_VERSION,
        name: "Legacy calendar-only compatibility run".to_owned(),
        preset: EvolutionPreset::Custom,
        simulated_hours: hours,
        episodes_per_day: 0,
        total_episodes: Some(0),
        replicate_count: 1,
        seed: arguments.seed.unwrap_or(0x5045_5432_D15C_0A57),
        outcome_model: EvolutionOutcomeModel::QuietUser,
        quiet_advance: QuietAdvanceMode::CalendarOnly,
        include_saved_gesture_replays: false,
        sleep_consolidation: false,
        persistent_learning: false,
        evolution_policy: EvolutionPolicy::Off,
        maximum_generations: 0,
        checkpoint_interval_hours: hours.clamp(1.0, 168.0),
        persistence: EvolutionPersistence::DryRun,
    })
}

fn run_replicate(
    config: &EvolutionConfig,
    initial: &InitialState,
    replicate_index: u32,
    audio_directory: &Path,
    progress: &mut ProgressWriter,
) -> Result<ReplicateResult, Box<dyn Error>> {
    let seed = config
        .seed
        .wrapping_add(u64::from(replicate_index).wrapping_mul(0xA076_1D64_78BD_642F));
    let mut life = LifeCore::restore(initial.life.clone())?;
    let mut vita = VitaRuntime::new(life.state.genome.identity_seed, Some(initial.vita.clone()));
    let mut morph = MorphBrain::new(life.state.genome.identity_seed, Some(initial.morph.clone()))?;
    let mut body = ProceduralBody::generate(&life.state.genome)?;
    body.apply_tuning_profile(initial.tuning.clone())?;
    body.restore_body_material_snapshot(&initial.body)?;
    body.embodiment
        .set_world_to_body_scale(Vec2::new(12.0, -12.0));
    vita.set_embodied_gesture_tuning(classifier_tuning(initial.tuning.interaction));

    let total_ticks = (config.simulated_hours * 3_600.0 * BASE_HZ as f64).round() as u64;
    let episode_count = config.scheduled_episode_count();
    let mut base_tick = 0_u64;
    let checkpoint_ticks =
        (config.checkpoint_interval_hours * 3_600.0 * BASE_HZ as f64).round() as u64;
    let mut next_checkpoint = checkpoint_ticks.max(1);
    let mut checkpoints = Vec::new();
    let mut sensors = SensorFrame::default();
    let mut intent = stable_body_intent();
    let mut gesture_distribution = BTreeMap::new();
    let mut observed_gesture_episodes = BTreeSet::new();
    let mut responded_episodes = BTreeSet::new();
    let mut invariants = EvolutionInvariantSummary::default();
    let mut body_timings_us = Vec::new();
    let mut quiet_or_no_response_episodes = 0_u32;
    let mut safe_boundary_episodes = 0_u32;
    let mut telemetry_samples = 0_u64;
    let mut completed_episodes = 0_u32;
    let initial_recovery_count = body.embodiment.liquid.diagnostics().recovery_count;
    let initial_lexicon_updates = lexicon_updates(&life);
    let audio_before = progress.progress.audio_render_count;
    let learning_before = progress.progress.learning_update_count;
    let mut audio = OfflineAudioRecorder::new(audio_directory.to_owned(), replicate_index);
    let mut expression_director = ExpressionDirector::default();

    for episode in 0..episode_count {
        let scheduled =
            (u64::from(episode) + 1).saturating_mul(total_ticks) / (u64::from(episode_count) + 1);
        let quiet_target = scheduled.min(total_ticks);
        advance_quiet(
            config.quiet_advance,
            &mut base_tick,
            quiet_target,
            &mut sensors,
            &body,
            &mut life,
            &mut vita,
            &mut morph,
            &mut intent,
            &mut telemetry_samples,
            &mut audio,
            &mut expression_director,
        );
        capture_checkpoints(
            &life,
            base_tick,
            &mut next_checkpoint,
            checkpoint_ticks,
            &mut checkpoints,
        )?;
        if base_tick >= total_ticks {
            break;
        }
        let kind = if config.preset == EvolutionPreset::BoundarySafety {
            CurriculumFixture::BlockedSeparation
        } else {
            CURRICULUM_V1[episode as usize % CURRICULUM_V1.len()]
        };
        run_episode(
            config,
            kind,
            seed,
            episode,
            total_ticks,
            &mut base_tick,
            &mut sensors,
            &mut life,
            &mut vita,
            &mut morph,
            &mut body,
            &mut intent,
            &mut gesture_distribution,
            &mut observed_gesture_episodes,
            &mut responded_episodes,
            &mut invariants,
            &mut body_timings_us,
            &mut telemetry_samples,
            &mut quiet_or_no_response_episodes,
            &mut safe_boundary_episodes,
            &mut audio,
            &mut expression_director,
        );
        completed_episodes = completed_episodes.saturating_add(1);
        progress.progress.stage = EvolutionProgressStage::Episode;
        progress.progress.replicate_index = replicate_index;
        progress.progress.episode_index = completed_episodes;
        progress.progress.simulated_seconds = base_tick as f64 / BASE_HZ as f64;
        progress.progress.audio_render_count =
            audio_before.saturating_add(audio.summary.rendered_count);
        progress.progress.learning_update_count = learning_before
            .saturating_add(lexicon_updates(&life).saturating_sub(initial_lexicon_updates));
        progress.write();
        capture_checkpoints(
            &life,
            base_tick,
            &mut next_checkpoint,
            checkpoint_ticks,
            &mut checkpoints,
        )?;
    }
    advance_quiet(
        config.quiet_advance,
        &mut base_tick,
        total_ticks,
        &mut sensors,
        &body,
        &mut life,
        &mut vita,
        &mut morph,
        &mut intent,
        &mut telemetry_samples,
        &mut audio,
        &mut expression_director,
    );
    capture_checkpoints(
        &life,
        base_tick,
        &mut next_checkpoint,
        checkpoint_ticks,
        &mut checkpoints,
    )?;

    let sleep_consolidations = if config.sleep_consolidation {
        ((config.simulated_hours / 24.0) * 5.0).floor().max(1.0) as u32
    } else {
        0
    };
    for _ in 0..sleep_consolidations {
        life.consolidate_sleep();
    }
    let diagnostics = body.embodiment.liquid.diagnostics();
    invariants.emergency_recoveries = diagnostics
        .recovery_count
        .saturating_sub(initial_recovery_count);
    if invariants.emergency_recoveries > 0 {
        invariants.failures.push(format!(
            "{} whole-solver emergency recoveries",
            invariants.emergency_recoveries
        ));
    }
    if diagnostics.component_count > 1 {
        invariants.unremerged_components = (diagnostics.component_count - 1) as u32;
        invariants.failures.push(format!(
            "{} detached components remained after final settle",
            invariants.unremerged_components
        ));
    }
    let life_valid = life.state.is_valid();
    let vita_valid = vita.snapshot().is_valid();
    let morph_valid = morph.snapshot().is_valid();
    if !life_valid || !vita_valid || !morph_valid {
        invariants.non_finite_failures = invariants.non_finite_failures.saturating_add(1);
        invariants.failures.push(format!(
            "final mind validation failed: life={life_valid}, vita={vita_valid}, morph={morph_valid}"
        ));
    }
    invariants.passed = invariants.failures.is_empty();

    let lexicon_updates = lexicon_updates(&life).saturating_sub(initial_lexicon_updates);
    let interaction_closed = life.state.interactions.pending_credit.is_none()
        && vita.interaction_turn().state == InteractionTurnState::Idle;
    if !interaction_closed {
        eprintln!(
            "evolution interaction remained open: pending_credit={}, turn={:?}, episode={}, response_emitted={}",
            life.state.interactions.pending_credit.is_some(),
            vita.interaction_turn().state,
            vita.interaction_turn().episode_id,
            vita.interaction_turn().response_emitted,
        );
    }
    let eligibility = eligibility(
        config,
        completed_episodes,
        &gesture_distribution,
        sleep_consolidations,
        quiet_or_no_response_episodes,
        safe_boundary_episodes,
        &invariants,
        interaction_closed,
    );
    if eligibility.eligible
        && config.evolution_policy == EvolutionPolicy::EligibleMaxOne
        && config.maximum_generations > 0
    {
        let _ = life.trigger_metamorphosis();
        vita.note_metamorphosis();
    }
    let final_life_state_hash = life_hash(&life)?;
    let audio = audio.finish();
    progress.progress.audio_render_count = audio_before.saturating_add(audio.rendered_count);
    progress.progress.learning_update_count = learning_before.saturating_add(lexicon_updates);
    let summary = EvolutionReplicateSummary {
        replicate_index,
        seed,
        completed_episodes,
        final_generation: life.state.genome.generation,
        final_genome_hash: life.state.genome.stable_hash(),
        final_life_state_hash,
        lexicon_updates,
        audio,
        telemetry_samples,
        gesture_distribution,
        eligibility,
        invariants,
    };
    Ok(ReplicateResult {
        life,
        vita,
        morph,
        body,
        summary,
        checkpoints,
        quiet_or_no_response_episodes,
        safe_boundary_episodes,
        sleep_consolidations,
        body_timings_us,
        base_ticks: base_tick,
    })
}

#[allow(clippy::too_many_arguments)]
fn advance_quiet(
    mode: QuietAdvanceMode,
    base_tick: &mut u64,
    target_tick: u64,
    sensors: &mut SensorFrame,
    body: &ProceduralBody,
    life: &mut LifeCore,
    vita: &mut VitaRuntime,
    morph: &mut MorphBrain,
    intent: &mut BodyIntent,
    telemetry_samples: &mut u64,
    audio: &mut OfflineAudioRecorder,
    expression_director: &mut ExpressionDirector,
) {
    if target_tick <= *base_tick {
        return;
    }
    if mode == QuietAdvanceMode::CalendarOnly {
        let ticks = target_tick - *base_tick;
        life.advance_calendar_only(ticks as f64 / BASE_HZ as f64);
        *telemetry_samples = telemetry_samples.saturating_add(ticks / TELEMETRY_DIVISOR);
        *base_tick = target_tick;
        return;
    }
    let feedback = body.simulation.feedback.clone();
    loop {
        let next_perception_tick = (*base_tick / PERCEPTION_DIVISOR)
            .saturating_add(1)
            .saturating_mul(PERCEPTION_DIVISOR);
        if next_perception_tick > target_tick {
            break;
        }
        *base_tick = next_perception_tick;
        set_quiet_sensors(sensors, *base_tick, feedback.world_position);
        vita.observe(sensors, &feedback, PERCEPTION_DT);
        if (*base_tick).is_multiple_of(LIFE_DIVISOR) {
            expression_director.tick(LIFE_DT);
            let morph_output = morph.tick(sensors, &feedback, &life.state, LIFE_DT);
            let mut output = life.tick(sensors, &feedback, LIFE_DT);
            if let Some(request) = output.vocal_request.take() {
                audio.render(life, request);
            }
            (*intent, _) = vita.resolve_intent_with_morph(
                BrainMode::MorphFusion,
                &life.state,
                sensors,
                &feedback,
                output.body_intent,
                Some(morph_output),
                LIFE_DT,
            );
        }
        if (*base_tick).is_multiple_of(TELEMETRY_DIVISOR) {
            *telemetry_samples = telemetry_samples.saturating_add(1);
        }
    }
    *base_tick = target_tick;
}

#[allow(clippy::too_many_arguments)]
fn run_episode(
    config: &EvolutionConfig,
    kind: CurriculumFixture,
    seed: u64,
    episode_index: u32,
    total_ticks: u64,
    base_tick: &mut u64,
    sensors: &mut SensorFrame,
    life: &mut LifeCore,
    vita: &mut VitaRuntime,
    morph: &mut MorphBrain,
    body: &mut ProceduralBody,
    intent: &mut BodyIntent,
    gesture_distribution: &mut BTreeMap<String, u32>,
    observed_gesture_episodes: &mut BTreeSet<u64>,
    responded_episodes: &mut BTreeSet<u64>,
    invariants: &mut EvolutionInvariantSummary,
    body_timings_us: &mut Vec<f64>,
    telemetry_samples: &mut u64,
    quiet_or_no_response_episodes: &mut u32,
    safe_boundary_episodes: &mut u32,
    audio: &mut OfflineAudioRecorder,
    expression_director: &mut ExpressionDirector,
) {
    let mut fixture = FixtureRuntime::new(kind, seed, episode_index);
    let anticipation = 0.25_f32;
    let response = 0.85_f32;
    let settle = if matches!(
        kind,
        CurriculumFixture::AllowedSeparation
            | CurriculumFixture::BlockedSeparation
            | CurriculumFixture::FragmentHelp
    ) {
        // Detached mass must return through ordinary PBF/recovery fields.  A
        // deliberately generous but bounded window avoids treating a slow,
        // physically valid return as a failed run; no mass is deleted,
        // teleported, or reconstructed here.
        SEPARATION_SETTLE_SECONDS
    } else {
        1.25
    };
    let duration = anticipation + fixture.active_seconds + response + settle;
    let episode_ticks = (duration * BASE_HZ as f32).ceil() as u64;
    let mut saw_safe_boundary = false;
    let interaction_tuning = body.tuning_profile().interaction;
    let recovery_before = body.embodiment.liquid.diagnostics().recovery_count;
    for local_tick in 0..episode_ticks {
        if *base_tick >= total_ticks {
            break;
        }
        *base_tick = base_tick.saturating_add(1);
        let elapsed = local_tick as f32 * BODY_DT;
        let active_elapsed = elapsed - anticipation;
        let pointer = if (0.0..fixture.active_seconds).contains(&active_elapsed) {
            fixture.sample(active_elapsed)
        } else {
            let released = fixture.previous_down;
            fixture.previous_down = false;
            PointerSample {
                local: fixture.previous_local,
                released,
                ..PointerSample::default()
            }
        };
        set_pointer_sensors(sensors, *base_tick, body, pointer);
        sensors.embodied_interaction = body.embodied_interaction_frame();
        body.simulation.feedback.cursor_contact = sensors.embodied_interaction.contact.active;

        if (*base_tick).is_multiple_of(PERCEPTION_DIVISOR) {
            vita.observe(sensors, &body.simulation.feedback, PERCEPTION_DT);
        }
        if (*base_tick).is_multiple_of(LIFE_DIVISOR) {
            expression_director.tick(LIFE_DT);
            let morph_output = morph.tick_with_world(
                sensors,
                &body.simulation.feedback,
                &life.state,
                &MorphWorldInput::default(),
                LIFE_DT,
            );
            let mut output = life.tick(sensors, &body.simulation.feedback, LIFE_DT);
            if let Some((event, _signature)) = vita.take_embodied_gesture_observation() {
                let physical_episode = event.classification.episode_id;
                if event.boundary != lifecore::GestureBoundaryEvent::None {
                    saw_safe_boundary = true;
                }
                if event.classification.kind != EmbodiedGestureKind::Unknown
                    && observed_gesture_episodes.insert(physical_episode)
                {
                    *gesture_distribution
                        .entry(format!("{:?}", event.classification.kind))
                        .or_default() += 1;
                }
                if let Some(plan) = life.observe_embodied_gesture_with_tuning(
                    event,
                    interaction_tuning.response_amplitude,
                    interaction_tuning.turn_wait_seconds,
                    interaction_tuning.turn_cooldown_seconds,
                    interaction_tuning.learning_openness,
                ) {
                    let phrase = expression_director.direct_world(
                        plan,
                        LivingStateFrame::from_life(&life.state),
                        PhysicalExpressionContext::from_frames(sensors, &body.simulation.feedback),
                        lifecore::WorldModelFrame::from_frames(sensors, &body.simulation.feedback),
                    );
                    let plan = phrase.plan;
                    if !responded_episodes.insert(plan.episode_id) {
                        invariants.duplicate_response_failures =
                            invariants.duplicate_response_failures.saturating_add(1);
                        invariants.failures.push(format!(
                            "gesture episode {} emitted multiple responses",
                            plan.episode_id
                        ));
                    }
                    let _ = vita.accept_interaction_response(plan);
                    if output.vocal_request.is_none() {
                        output.vocal_request = plan
                            .voice_trigger
                            .and_then(|trigger| life.request_vocalization(trigger, sensors));
                    }
                } else {
                    vita.finish_interaction_appraisal_without_response(physical_episode);
                }
            }
            if let Some(request) = output.vocal_request.take() {
                audio.render(life, request);
            }
            (*intent, _) = vita.resolve_intent_with_morph(
                BrainMode::MorphFusion,
                &life.state,
                sensors,
                &body.simulation.feedback,
                output.body_intent,
                Some(morph_output),
                LIFE_DT,
            );
            sensors.interaction_actuation = vita.interaction_actuation();
        }

        let fixture_phase = (active_elapsed / fixture.active_seconds).clamp(0.0, 1.0);
        let cooperative_separation = pointer.down
            && (kind == CurriculumFixture::AllowedSeparation
                || (kind == CurriculumFixture::FragmentHelp && fixture_phase < 0.55));
        if cooperative_separation {
            // This is an explicit closed curriculum condition representing the
            // creature's voluntary yield. It drives only bounded physical
            // actuation; classification and topology still come from PBF.
            sensors.interaction_actuation.allow_intentional_bud = true;
            sensors.interaction_actuation.compliance_delta = 0.25;
            sensors.interaction_actuation.cohesion_delta = -0.25;
            sensors.interaction_actuation.cooperation = 1.0;
            sensors.interaction_actuation.resistance = 0.0;
        }

        let started = Instant::now();
        body.fixed_update(&life.state.genome, intent, sensors, BODY_DT);
        body.embodied_update(
            intent,
            sensors,
            life.state.affect,
            VisualMindInput::default(),
            VoiceVisualState::default(),
            BODY_DT,
        );
        body_timings_us.push(started.elapsed().as_secs_f64() * 1_000_000.0);
        check_body_invariants(body, interaction_tuning, invariants);
        if (*base_tick).is_multiple_of(TELEMETRY_DIVISOR) {
            *telemetry_samples = telemetry_samples.saturating_add(1);
        }
    }

    if saw_safe_boundary {
        *safe_boundary_episodes = safe_boundary_episodes.saturating_add(1);
    }
    let no_response = matches!(
        kind,
        CurriculumFixture::NoResponse | CurriculumFixture::SleepQuietContact
    ) || config.outcome_model == EvolutionOutcomeModel::QuietUser
        || (config.outcome_model == EvolutionOutcomeModel::MixedRealistic
            && episode_index.is_multiple_of(4));
    if no_response {
        *quiet_or_no_response_episodes = quiet_or_no_response_episodes.saturating_add(1);
    }
    resolve_episode_outcome(config, life, vita, morph, episode_index, no_response);
    let recovery_after = body.embodiment.liquid.diagnostics().recovery_count;
    if recovery_after > recovery_before {
        invariants.emergency_recoveries = invariants
            .emergency_recoveries
            .saturating_add(recovery_after - recovery_before);
    }
}

fn resolve_episode_outcome(
    config: &EvolutionConfig,
    life: &mut LifeCore,
    vita: &mut VitaRuntime,
    morph: &mut MorphBrain,
    episode_index: u32,
    no_response: bool,
) {
    let Some(credit) = life.state.interactions.pending_credit else {
        return;
    };
    let kind = if !config.persistent_learning || no_response {
        InteractionOutcomeKind::NoResponse
    } else {
        match config.outcome_model {
            EvolutionOutcomeModel::RespectfulSupportive => {
                if episode_index.is_multiple_of(7) {
                    InteractionOutcomeKind::NoResponse
                } else {
                    InteractionOutcomeKind::VoluntaryContinuation
                }
            }
            EvolutionOutcomeModel::MixedRealistic => {
                if episode_index.is_multiple_of(9) {
                    InteractionOutcomeKind::ExplicitNegative
                } else {
                    InteractionOutcomeKind::VoluntaryContinuation
                }
            }
            EvolutionOutcomeModel::QuietUser | EvolutionOutcomeModel::BoundaryValidation => {
                InteractionOutcomeKind::NoResponse
            }
        }
    };
    life.resolve_interaction_outcome(InteractionOutcome {
        episode_id: credit.episode_id,
        response_id: Some(credit.response_id),
        kind,
        confidence: 1.0,
        elapsed_seconds: credit.elapsed_seconds,
    });
    let feedback = match kind {
        InteractionOutcomeKind::VoluntaryContinuation
        | InteractionOutcomeKind::ExplicitPositive => Some(FeedbackEvent::PlayStarted),
        InteractionOutcomeKind::ExplicitNegative | InteractionOutcomeKind::Refusal => {
            Some(FeedbackEvent::PushedAway)
        }
        _ => None,
    };
    if let Some(feedback) = feedback {
        vita.apply_feedback(&feedback);
        morph.apply_feedback(&feedback);
    }
}

fn set_quiet_sensors(sensors: &mut SensorFrame, tick: u64, body_position: Vec2) {
    *sensors = SensorFrame::default();
    sensors.timestamp = tick as f64 / BASE_HZ as f64;
    sensors.time_of_day_01 = (sensors.timestamp / 86_400.0).fract() as f32;
    sensors.cursor_position = Vec2::new(0.05, 0.05);
    sensors.cursor_distance_to_pet = sensors.cursor_position.distance(body_position);
    sensors.user_idle_seconds = 120.0;
    sensors.user_availability = Some(0.0);
    sensors.user_presence = Some(0.0);
}

fn set_pointer_sensors(
    sensors: &mut SensorFrame,
    tick: u64,
    body: &ProceduralBody,
    sample: PointerSample,
) {
    let body_position = body.simulation.feedback.world_position;
    let scale = body.embodiment.world_to_body_scale();
    let safe_scale = Vec2::new(
        if scale.x.abs() > 1.0e-5 { scale.x } else { 1.0 },
        if scale.y.abs() > 1.0e-5 {
            scale.y
        } else {
            -1.0
        },
    );
    sensors.timestamp = tick as f64 / BASE_HZ as f64;
    sensors.time_of_day_01 = (sensors.timestamp / 86_400.0).fract() as f32;
    sensors.cursor_position = body_position + sample.local / safe_scale;
    sensors.cursor_velocity = sample.velocity_local / safe_scale;
    sensors.cursor_acceleration = Vec2::ZERO;
    sensors.cursor_distance_to_pet = sensors.cursor_position.distance(body_position);
    sensors.pointer_down = sample.down;
    sensors.pointer_pressed = sample.pressed;
    sensors.pointer_released = sample.released;
    sensors.pet_hovered = true;
    sensors.pet_dragged = sample.down;
    sensors.pet_touched = sample.pressed;
    sensors.user_idle_seconds = 0.0;
    sensors.user_activity_rate = 0.7;
    sensors.user_availability = Some(1.0);
    sensors.user_presence = Some(1.0);
}

fn stable_body_intent() -> BodyIntent {
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

fn check_body_invariants(
    body: &ProceduralBody,
    tuning: pet_body::InteractionTuning,
    invariants: &mut EvolutionInvariantSummary,
) {
    let frame = body.embodied_interaction_frame();
    let diagnostics = body.embodiment.liquid.diagnostics();
    if !diagnostics.finite || !frame.is_valid() {
        invariants.non_finite_failures = invariants.non_finite_failures.saturating_add(1);
        if invariants.non_finite_failures == 1 {
            invariants.failures.push(format!(
                "body/frame validation failed: diagnostics_finite={}, contact_valid={}, material_valid={}, frame_valid={}, sequence={}, components={}, mass_error={:.8}, contact={:?}",
                diagnostics.finite,
                frame.contact.is_valid(),
                frame.material.is_valid(),
                frame.is_valid(),
                frame.sequence,
                frame.material.component_count,
                frame.material.mass_conservation_error,
                frame.contact,
            ));
        }
    }
    if frame.material.mass_conservation_error > 1.0e-5 {
        invariants.mass_conservation_failures =
            invariants.mass_conservation_failures.saturating_add(1);
        if invariants.mass_conservation_failures == 1 {
            invariants
                .failures
                .push("body component summaries violated mass conservation".to_owned());
        }
    }
    invariants.maximum_detached_mass_fraction = invariants
        .maximum_detached_mass_fraction
        .max(frame.material.detached_mass_fraction);
    if frame.material.detached_mass_fraction > tuning.maximum_detached_mass_fraction + 1.0e-5
        || frame.material.component_count > tuning.maximum_detached_components.saturating_add(1)
    {
        invariants.topology_budget_failures = invariants.topology_budget_failures.saturating_add(1);
        if invariants.topology_budget_failures == 1 {
            invariants
                .failures
                .push("hard topology component or detached-mass budget exceeded".to_owned());
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn eligibility(
    config: &EvolutionConfig,
    completed_episodes: u32,
    gesture_distribution: &BTreeMap<String, u32>,
    sleep_consolidations: u32,
    quiet_episodes: u32,
    boundary_episodes: u32,
    invariants: &EvolutionInvariantSummary,
    interaction_closed: bool,
) -> EvolutionEligibility {
    let mut result = EvolutionEligibility {
        simulated_hours_met: config.simulated_hours >= 24.0,
        interaction_episodes_met: completed_episodes >= 120,
        gesture_diversity_met: gesture_distribution.len() >= 8,
        sleep_consolidations_met: sleep_consolidations >= 5,
        quiet_episodes_met: quiet_episodes >= 12,
        safe_boundary_episodes_met: boundary_episodes >= 8,
        mass_and_remerge_passed: invariants.mass_conservation_failures == 0
            && invariants.unremerged_components == 0
            && invariants.topology_budget_failures == 0,
        zero_recovery_failures: invariants.non_finite_failures == 0
            && invariants.emergency_recoveries == 0,
        interaction_closed,
        ..EvolutionEligibility::default()
    };
    let checks = [
        (
            result.simulated_hours_met,
            "minimum 24 simulated hours not met",
        ),
        (
            result.interaction_episodes_met,
            "minimum 120 interaction episodes not met",
        ),
        (
            result.gesture_diversity_met,
            "minimum eight observed gesture classes not met",
        ),
        (
            result.sleep_consolidations_met,
            "minimum five sleep consolidations not met",
        ),
        (
            result.quiet_episodes_met,
            "minimum twelve quiet/no-response episodes not met",
        ),
        (
            result.safe_boundary_episodes_met,
            "minimum eight safe boundary episodes not met",
        ),
        (
            result.mass_and_remerge_passed,
            "mass/topology/remerge acceptance failed",
        ),
        (
            result.zero_recovery_failures,
            "non-finite or emergency recovery failure observed",
        ),
        (
            result.interaction_closed,
            "interaction turn remained active",
        ),
    ];
    result.reasons = checks
        .into_iter()
        .filter_map(|(passed, reason)| (!passed).then_some(reason.to_owned()))
        .collect();
    result.eligible = result.reasons.is_empty();
    result
}

fn capture_checkpoints(
    life: &LifeCore,
    base_tick: u64,
    next_checkpoint: &mut u64,
    checkpoint_ticks: u64,
    checkpoints: &mut Vec<EvolutionCheckpoint>,
) -> Result<(), Box<dyn Error>> {
    while base_tick >= *next_checkpoint {
        checkpoints.push(EvolutionCheckpoint {
            simulated_hour: (*next_checkpoint / (3_600 * BASE_HZ)) as u32,
            genome_hash: life.state.genome.stable_hash(),
            life_state_hash: life_hash(life)?,
            generation: life.state.genome.generation,
        });
        *next_checkpoint = next_checkpoint.saturating_add(checkpoint_ticks.max(1));
    }
    Ok(())
}

fn lexicon_updates(life: &LifeCore) -> u32 {
    life.state.vocal_lexicon.update_count
}

fn life_hash(life: &LifeCore) -> Result<u64, serde_json::Error> {
    persisted_life_snapshot_hash(&life.snapshot())
}

fn persist_accepted_state(
    config: &EvolutionConfig,
    live_store: &StateStore,
    initial: &InitialState,
    primary: &ReplicateResult,
) -> Result<Option<String>, Box<dyn Error>> {
    let destination = match config.persistence {
        EvolutionPersistence::DryRun => return Ok(None),
        EvolutionPersistence::Fork => {
            let safe_name = config
                .name
                .chars()
                .map(|character| {
                    if character.is_ascii_alphanumeric() {
                        character.to_ascii_lowercase()
                    } else {
                        '-'
                    }
                })
                .collect::<String>();
            let run_id = format!(
                "{}-{:016x}",
                safe_name.trim_matches('-'),
                primary.summary.final_life_state_hash
            );
            StateStore::at(live_store.paths.evolution_runs.join(run_id))
        }
        EvolutionPersistence::SaveFinal => live_store.clone(),
    };
    let portable = PortablePetState {
        schema_version: PORTABLE_STATE_SCHEMA_VERSION,
        life: primary.life.snapshot(),
        vita: Some(primary.vita.snapshot()),
        position: initial.position.clone(),
    };
    destination.save_state(&portable)?;
    destination.save_liquid_tuning(&initial.tuning)?;
    destination.save_morph_brain(&primary.morph.snapshot())?;
    // Synthetic curricula are never allowed to write user convention state.
    destination.save_ecology_state(&initial.ecology)?;
    destination.save_body_state(&primary.body.body_material_snapshot())?;
    Ok(Some(destination.paths.root.display().to_string()))
}

fn atomic_report(path: &Path, report: &EvolutionRunReport) -> Result<(), Box<dyn Error>> {
    atomic_json(path, report, true)
}

fn atomic_json<T: serde::Serialize>(
    path: &Path,
    value: &T,
    keep_previous: bool,
) -> Result<(), Box<dyn Error>> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty());
    if let Some(parent) = parent {
        fs::create_dir_all(parent)?;
    }
    let temporary = temporary_report_path(path);
    if temporary.exists() {
        fs::remove_file(&temporary)?;
    }
    let file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&temporary)?;
    let mut writer = BufWriter::new(file);
    serde_json::to_writer_pretty(&mut writer, value)?;
    writer.write_all(b"\n")?;
    writer.flush()?;
    writer.get_ref().sync_all()?;
    drop(writer);
    if path.exists() {
        let backup = if keep_previous {
            path.with_extension("previous.json")
        } else {
            path.with_extension("replaced.json")
        };
        if backup.exists() {
            fs::remove_file(&backup)?;
        }
        fs::rename(path, backup)?;
    }
    fs::rename(&temporary, path)?;
    if !keep_previous {
        let backup = path.with_extension("replaced.json");
        if backup.exists() {
            fs::remove_file(backup)?;
        }
    }
    Ok(())
}

fn temporary_report_path(path: &Path) -> PathBuf {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("evolution-report.json");
    path.with_file_name(format!(".{name}.tmp"))
}

fn percentile(sorted: &[f64], percentile: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let index = ((sorted.len() - 1) as f64 * percentile.clamp(0.0, 1.0)).round() as usize;
    sorted[index]
}

#[cfg(test)]
mod tests {
    use super::*;
    use lifecore::{Genome, LIFECORE_HZ};

    #[test]
    fn calendar_only_advances_time_without_learning() {
        let mut life = LifeCore::new(Genome::from_seed(12), 13);
        let before_variants = life.state.interactions.variants;
        let before_successes = life.state.successful_interactions;
        life.advance_calendar_only(3_600.0);
        assert_eq!(life.state.elapsed_seconds, 3_600.0);
        assert_eq!(
            life.state.tick_count,
            (LIFECORE_HZ as u64).saturating_mul(3_600)
        );
        assert_eq!(life.state.interactions.variants, before_variants);
        assert_eq!(life.state.successful_interactions, before_successes);
    }

    #[test]
    fn curriculum_is_closed_and_contains_all_thirteen_physical_fixtures() {
        assert_eq!(CURRICULUM_V1.len(), 13);
        assert!(CURRICULUM_V1.contains(&CurriculumFixture::AllowedSeparation));
        assert!(CURRICULUM_V1.contains(&CurriculumFixture::BlockedSeparation));
        assert!(CURRICULUM_V1.contains(&CurriculumFixture::NoResponse));
    }

    #[test]
    fn exact_runner_preserves_subsystem_fixed_dt() {
        assert_eq!(BASE_HZ / PERCEPTION_DIVISOR, 60);
        assert_eq!(BASE_HZ / LIFE_DIVISOR, 20);
        assert_eq!(BASE_HZ / TELEMETRY_DIVISOR, 5);
        assert_eq!(BODY_DT, 1.0 / 120.0);
        assert_eq!(PERCEPTION_DT, 1.0 / 60.0);
        assert_eq!(LIFE_DT, 1.0 / 20.0);
        assert_eq!(SEPARATION_SETTLE_SECONDS, 30.0);
    }

    #[test]
    fn exact_quiet_scheduler_preserves_absolute_clock_phases() {
        let mut life = LifeCore::new(Genome::from_seed(0xC10C), 0x71C5);
        let mut vita = VitaRuntime::new(life.state.genome.identity_seed, None);
        let mut morph = MorphBrain::new(life.state.genome.identity_seed, None).unwrap();
        let body = ProceduralBody::generate(&life.state.genome).unwrap();
        let mut sensors = SensorFrame::default();
        let mut intent = stable_body_intent();
        let mut base_tick = 1;
        let mut telemetry_samples = 0;
        let directory = tempfile::tempdir().unwrap();
        let mut audio = OfflineAudioRecorder::new(directory.path().to_owned(), 0);
        let mut expression_director = ExpressionDirector::default();

        advance_quiet(
            QuietAdvanceMode::Exact,
            &mut base_tick,
            25,
            &mut sensors,
            &body,
            &mut life,
            &mut vita,
            &mut morph,
            &mut intent,
            &mut telemetry_samples,
            &mut audio,
            &mut expression_director,
        );

        assert_eq!(base_tick, 25);
        assert_eq!(life.state.tick_count, 4);
        assert_eq!(telemetry_samples, 1);
        assert_eq!(sensors.timestamp, 24.0 / BASE_HZ as f64);
    }

    #[test]
    fn seven_day_preset_is_reproducible() {
        let first: EvolutionConfig = serde_json::from_str(include_str!(
            "../../config/evolution/seven-day-socialization.json"
        ))
        .unwrap();
        let second: EvolutionConfig = serde_json::from_str(include_str!(
            "../../config/evolution/seven-day-socialization.json"
        ))
        .unwrap();
        first.validate().unwrap();
        second.validate().unwrap();
        assert_eq!(first, second);
        assert_eq!(
            first.scheduled_episode_count(),
            second.scheduled_episode_count()
        );
        for episode in 0..first.scheduled_episode_count() {
            let kind = CURRICULUM_V1[episode as usize % CURRICULUM_V1.len()];
            let mut left = FixtureRuntime::new(kind, first.seed, episode);
            let mut right = FixtureRuntime::new(kind, second.seed, episode);
            for tick in 0..240 {
                let elapsed = tick as f32 * BODY_DT;
                let left_sample = left.sample(elapsed);
                let right_sample = right.sample(elapsed);
                assert_eq!(
                    left_sample.local.to_array().map(f32::to_bits),
                    right_sample.local.to_array().map(f32::to_bits)
                );
                assert_eq!(
                    left_sample.velocity_local.to_array().map(f32::to_bits),
                    right_sample.velocity_local.to_array().map(f32::to_bits)
                );
                assert_eq!(left_sample.down, right_sample.down);
            }
        }
    }

    #[test]
    fn ten_day_one_hour_preset_is_valid_bounded_and_non_destructive() {
        let config: EvolutionConfig =
            serde_json::from_str(include_str!("../../config/evolution/ten-day-one-hour.json"))
                .unwrap();
        config.validate().unwrap();
        assert_eq!(config.simulated_hours, 240.0);
        assert_eq!(config.scheduled_episode_count(), 360);
        assert_eq!(config.replicate_count, 1);
        assert_eq!(config.quiet_advance, QuietAdvanceMode::CalendarOnly);
        assert!(config.persistent_learning);
        assert_eq!(config.evolution_policy, EvolutionPolicy::EligibleMaxOne);
        assert_eq!(config.maximum_generations, 1);
        assert_eq!(config.persistence, EvolutionPersistence::Fork);
    }

    #[test]
    fn synthetic_curriculum_cannot_write_user_specific_conventions() {
        use pet_ecology::{ActionSignature, GestureConventionMeaning, GestureSignature};

        let life = LifeCore::new(Genome::from_seed(0x0C01_1EC7), 0xE701);
        let mut ecology = EcologyState::new(life.state.genome.identity_seed);
        let path = [
            Vec2::new(-0.2, 0.0),
            Vec2::new(0.0, 0.2),
            Vec2::new(0.2, 0.0),
        ];
        let signature = GestureSignature {
            path: ActionSignature::from_trace(&path, 0.8).unwrap(),
            pressure_mean: 0.3,
            pressure_variance: 0.02,
            strain_peak: 0.2,
            release_speed: 0.4,
            duty_cycle: 0.7,
            rhythm_intervals: [1.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
            rhythm_count: 2,
        };
        ecology
            .gesture_conventions
            .observe_success(GestureConventionMeaning::RhythmMotif, signature, 1.0, true)
            .unwrap();
        let tuning = crate::approved_production_liquid_tuning(life.state.genome.identity_seed);
        let mut body = ProceduralBody::generate(&life.state.genome).unwrap();
        body.apply_tuning_profile(tuning.clone()).unwrap();
        let initial = InitialState {
            life: life.snapshot(),
            vita: VitaRuntime::new(life.state.genome.identity_seed, None).snapshot(),
            morph: MorphBrain::new(life.state.genome.identity_seed, None)
                .unwrap()
                .snapshot(),
            ecology: ecology.clone(),
            position: desktop_host::PersistedPetPosition::default(),
            tuning,
            body: body.body_material_snapshot(),
        };
        let config = EvolutionConfig {
            schema_version: EVOLUTION_CONFIG_SCHEMA_VERSION,
            name: "synthetic-convention-safety".to_owned(),
            preset: EvolutionPreset::Custom,
            simulated_hours: 0.01,
            episodes_per_day: 0,
            total_episodes: Some(0),
            replicate_count: 1,
            seed: 77,
            outcome_model: EvolutionOutcomeModel::MixedRealistic,
            quiet_advance: QuietAdvanceMode::CalendarOnly,
            include_saved_gesture_replays: false,
            sleep_consolidation: false,
            persistent_learning: false,
            evolution_policy: EvolutionPolicy::Off,
            maximum_generations: 0,
            checkpoint_interval_hours: 1.0,
            persistence: EvolutionPersistence::Fork,
        };
        config.validate().unwrap();
        let audio_directory = tempfile::tempdir().unwrap();
        let mut progress = ProgressWriter::new(None, 1, 0);
        let result =
            run_replicate(&config, &initial, 0, audio_directory.path(), &mut progress).unwrap();
        progress.finished = true;
        let directory = tempfile::tempdir().unwrap();
        let store = StateStore::at(directory.path());
        let persisted = persist_accepted_state(&config, &store, &initial, &result)
            .unwrap()
            .unwrap();
        let restored = StateStore::at(persisted)
            .load_ecology_state()
            .unwrap()
            .unwrap();
        assert_eq!(restored.gesture_conventions, ecology.gesture_conventions);
        assert_eq!(result.summary.lexicon_updates, 0);
    }
}
