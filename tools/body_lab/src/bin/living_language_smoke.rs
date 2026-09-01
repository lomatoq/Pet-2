use std::{
    error::Error,
    fs::{self, File},
    io::{BufWriter, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::Arc,
};

use desktop_host::StateStore;
use glam::Vec2;
use lifecore::{
    AffectState, BodyIntent, CommunicativeIntent, CreaturePhrase, ExpressionDirector,
    ExpressionState, Genome, InteractionBodyActuation, InteractionExpressionTarget,
    InteractionGazeTarget, InteractionReasonCode, InteractionResponsePlan, LifeCore,
    LivingStateFrame, LocomotionMode, PhysicalExpressionContext, PoseIntent, SensorFrame,
    SocialIntent, VocalTrigger, VoiceGesture, WorldAffordance, WorldEntity, WorldEntityKind,
    WorldModelFrame,
};
use pet_audio::{
    OfflinePcm, OfflineSampleFormat, VoiceCommand, VoiceSynthesisStyle, export_debug_wav,
    render_motif_style,
};
use pet_body::{
    EcologyRenderer, LiquidTuningProfile, ProceduralBody, Renderer, ReviewBackground,
    VisualMindInput, VoiceVisualState,
};
use pet_ecology::EcologyState;
use serde::Serialize;
use winit::{
    dpi::PhysicalSize,
    event_loop::EventLoop,
    window::{Window, WindowLevel},
};

const WIDTH: u32 = 512;
const HEIGHT: u32 = 288;
const FPS: u32 = 24;
const BODY_HZ: u32 = 120;
const DURATION_SECONDS: f32 = 1.5;
const SEED: u64 = 0x5045_5432_4C41_4E47;

#[derive(Clone, Copy, PartialEq, Eq)]
enum ObjectFixture {
    None,
    Moving,
    Query,
}

#[derive(Clone, Copy)]
struct Fixture {
    name: &'static str,
    reason: InteractionReasonCode,
    trigger: VocalTrigger,
    physical: PhysicalExpressionContext,
    audience: f32,
    temperament: &'static str,
    object: ObjectFixture,
}

#[derive(Serialize)]
struct AudioMetrics {
    style: &'static str,
    motif_id: u64,
    performance_seed: u64,
    gesture: VoiceGesture,
    sample_count: usize,
    seconds: f64,
    unit_rate_hz: f32,
    rms: f32,
    peak: f32,
    zero_crossing_rate: f32,
    spectral_centroid_hz: f32,
    roughness_index: f32,
    nonlinear_event_count: u32,
    base_f0_hz: f32,
    minimum_f0_hz: f32,
    maximum_f0_hz: f32,
    f0_contour_hz: Vec<f32>,
}

#[derive(Clone, Copy, Serialize)]
struct BodyTracePoint {
    time_seconds: f32,
    gaze_target: InteractionGazeTarget,
    local_pulse: f32,
    recoil: f32,
    resistance: f32,
    cooperation: f32,
    maximum_strain: f32,
    material_speed: f32,
    stretch_ratio: f32,
    observed_physical: PhysicalExpressionContext,
}

#[derive(Serialize)]
struct VideoMetrics {
    renderer: &'static str,
    width: u32,
    height: u32,
    frames: u32,
    duration_seconds: f32,
    reaction_onset_seconds: f32,
    expression_hold_seconds: f32,
    maximum_strain: f32,
    maximum_speed: f32,
    maximum_stretch_ratio: f32,
    recovery_count: u64,
    final_expression: ExpressionState,
    body_actuation_trace: Vec<BodyTracePoint>,
}

#[derive(Serialize)]
struct FixtureTrace {
    fixture: String,
    fixed_seed: u64,
    temperament: &'static str,
    intent: SocialIntent,
    actual_physical_context: PhysicalExpressionContext,
    baseline_plan: InteractionResponsePlan,
    living_phrase: CreaturePhrase,
    baseline_audio: AudioMetrics,
    living_audio: AudioMetrics,
    baseline_video: VideoMetrics,
    living_video: VideoMetrics,
    baseline_wav: String,
    living_wav: String,
    baseline_mp4: String,
    living_mp4: String,
}

#[derive(Serialize)]
struct CaptureManifest {
    schema_version: u32,
    fixed_seed: u64,
    renderer: &'static str,
    tuning_profile: String,
    tuning_revision: u64,
    fixtures: Vec<FixtureTrace>,
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = std::env::args().skip(1);
    let output = match args.next().as_deref() {
        Some("--output") => PathBuf::from(args.next().ok_or("--output requires a path")?),
        Some(other) => return Err(format!("unknown argument: {other}").into()),
        None => PathBuf::from("artifacts/living-language-smoke"),
    };
    fs::create_dir_all(&output)?;
    let ffmpeg = find_ffmpeg()?;
    let genome = Genome::from_seed(SEED);
    println!("capture setup: genome ready");
    let tuning = StateStore::discover()?
        .load_liquid_tuning::<LiquidTuningProfile>()?
        .ok_or("the active liquid-tuning.json is missing; refusing to fake the production body")?
        .sanitized()?;
    println!(
        "capture setup: active tuning {} r{}",
        tuning.name, tuning.profile_revision
    );

    let event_loop = EventLoop::new()?;
    println!("capture setup: event loop ready");
    #[allow(deprecated)]
    let window = Arc::new(
        event_loop.create_window(
            Window::default_attributes()
                .with_title("PET 2 Living Language Capture")
                .with_inner_size(PhysicalSize::new(WIDTH, HEIGHT))
                .with_window_level(WindowLevel::AlwaysOnBottom)
                .with_visible(false),
        )?,
    );
    println!("capture setup: window ready");
    let renderer_mesh = production_mesh(&genome)?;
    println!("capture setup: production body mesh ready");
    let mut renderer = pollster::block_on(Renderer::new_with_render_scale(
        Arc::clone(&window),
        &renderer_mesh,
        tuning.compositor.render_scale,
    ))?;
    println!("capture setup: production renderer ready");
    renderer.set_review_background(ReviewBackground::Warm);
    renderer.set_studio_material_backdrop(true);
    let mut ecology_renderer = EcologyRenderer::new(renderer.device(), renderer.surface_format());
    window.set_visible(true);

    let mut traces = Vec::new();
    for (index, fixture) in fixtures().into_iter().enumerate() {
        let directory = output.join(format!("{:02}-{}", index + 1, fixture.name));
        fs::create_dir_all(&directory)?;
        let baseline_plan = baseline_plan(fixture, index as u64 + 1);
        let mut life = LifeCore::new(genome.clone(), 0xC0DE_0000 + index as u64);
        life.state.affect = fixture_affect(fixture);
        let living_frame = LivingStateFrame::from_life(&life.state);
        let world = fixture_world(fixture);
        let living_phrase = ExpressionDirector::default().direct_world(
            baseline_plan,
            living_frame,
            fixture.physical,
            world,
        );

        let baseline_wav = directory.join("baseline.wav");
        let living_wav = directory.join("living.wav");
        let baseline_audio = render_audio(
            &genome,
            fixture,
            index as u64,
            VoiceSynthesisStyle::Legacy,
            &baseline_wav,
        )?;
        let living_audio = render_audio(
            &genome,
            fixture,
            index as u64,
            VoiceSynthesisStyle::LivingMammalian,
            &living_wav,
        )?;

        let baseline_mp4 = directory.join("baseline.mp4");
        let living_mp4 = directory.join("living.mp4");
        let baseline_video = render_video(
            &ffmpeg,
            &genome,
            &tuning,
            &mut renderer,
            &mut ecology_renderer,
            baseline_plan,
            fixture,
            &baseline_mp4,
        )?;
        let living_video = render_video(
            &ffmpeg,
            &genome,
            &tuning,
            &mut renderer,
            &mut ecology_renderer,
            living_phrase.plan,
            fixture,
            &living_mp4,
        )?;
        let trace = FixtureTrace {
            fixture: fixture.name.to_owned(),
            fixed_seed: SEED ^ index as u64,
            temperament: fixture.temperament,
            intent: living_phrase.intent,
            actual_physical_context: fixture.physical,
            baseline_plan,
            living_phrase,
            baseline_audio,
            living_audio,
            baseline_video,
            living_video,
            baseline_wav: baseline_wav.display().to_string(),
            living_wav: living_wav.display().to_string(),
            baseline_mp4: baseline_mp4.display().to_string(),
            living_mp4: living_mp4.display().to_string(),
        };
        write_json(&directory.join("trace.json"), &trace)?;
        traces.push(trace);
        println!(
            "captured {}/13 {} with production renderer",
            index + 1,
            fixture.name
        );
    }
    let manifest = CaptureManifest {
        schema_version: 2,
        fixed_seed: SEED,
        renderer: "pet_body::Renderer production GPU pipeline",
        tuning_profile: tuning.name.clone(),
        tuning_revision: tuning.profile_revision,
        fixtures: traces,
    };
    write_json(&output.join("manifest.json"), &manifest)?;
    window.set_visible(false);
    println!("Living Language Smoke complete: {}", output.display());
    Ok(())
}

fn production_mesh(genome: &Genome) -> Result<pet_body::ProceduralMesh, Box<dyn Error>> {
    Ok(ProceduralBody::generate(genome)?.mesh)
}

fn fixtures() -> [Fixture; 13] {
    let gentle = PhysicalExpressionContext {
        contact: true,
        pressure: 0.16,
        deformation: 0.18,
        strain: 0.10,
        slosh_energy: 0.12,
        topology_budget_used: 0.0,
        detached_fraction: 0.0,
        speed: 0.10,
        acceleration: 0.08,
        airborne: false,
    };
    [
        fixture(
            "soft-touch",
            InteractionReasonCode::GentleContact,
            VocalTrigger::SoftTouch,
            gentle,
            0.95,
            "bold",
            ObjectFixture::None,
        ),
        fixture(
            "calm-stroke",
            InteractionReasonCode::GentleContact,
            VocalTrigger::SoftTouch,
            PhysicalExpressionContext {
                pressure: 0.22,
                deformation: 0.22,
                speed: 0.07,
                slosh_energy: 0.16,
                ..gentle
            },
            0.55,
            "shy",
            ObjectFixture::None,
        ),
        fixture(
            "slow-pull",
            InteractionReasonCode::EffortfulResistance,
            VocalTrigger::MissAndRetry,
            PhysicalExpressionContext {
                pressure: 0.32,
                deformation: 0.58,
                strain: 0.64,
                speed: 0.26,
                slosh_energy: 0.42,
                airborne: true,
                ..gentle
            },
            0.82,
            "neutral",
            ObjectFixture::None,
        ),
        fixture(
            "sharp-flick",
            InteractionReasonCode::PhysicalStartle,
            VocalTrigger::PhysicalStartle,
            PhysicalExpressionContext {
                pressure: 0.68,
                deformation: 0.52,
                strain: 0.48,
                speed: 0.90,
                acceleration: 0.96,
                slosh_energy: 0.76,
                airborne: true,
                ..gentle
            },
            0.74,
            "neutral",
            ObjectFixture::None,
        ),
        fixture(
            "prolonged-hold",
            InteractionReasonCode::CalmBoundary,
            VocalTrigger::CalmBoundary,
            PhysicalExpressionContext {
                pressure: 0.88,
                deformation: 0.62,
                strain: 0.78,
                topology_budget_used: 0.72,
                speed: 0.02,
                ..gentle
            },
            0.96,
            "neutral",
            ObjectFixture::None,
        ),
        fixture(
            "fragment-separation",
            InteractionReasonCode::ComponentDetached,
            VocalTrigger::ComponentDetached,
            PhysicalExpressionContext {
                pressure: 0.42,
                deformation: 0.78,
                strain: 0.86,
                topology_budget_used: 0.54,
                detached_fraction: 0.22,
                speed: 0.48,
                airborne: true,
                ..gentle
            },
            0.84,
            "neutral",
            ObjectFixture::None,
        ),
        fixture(
            "remerge",
            InteractionReasonCode::SuccessfulReunion,
            VocalTrigger::ComponentRemerged,
            PhysicalExpressionContext {
                pressure: 0.08,
                deformation: 0.34,
                strain: 0.20,
                detached_fraction: 0.04,
                slosh_energy: 0.38,
                airborne: true,
                ..gentle
            },
            0.92,
            "neutral",
            ObjectFixture::None,
        ),
        fixture(
            "playful-flight",
            InteractionReasonCode::PlayfulCooperation,
            VocalTrigger::PlayfulRelease,
            PhysicalExpressionContext {
                contact: false,
                pressure: 0.0,
                deformation: 0.28,
                strain: 0.18,
                speed: 0.70,
                acceleration: 0.38,
                slosh_energy: 0.66,
                airborne: true,
                ..gentle
            },
            0.88,
            "bold",
            ObjectFixture::None,
        ),
        fixture(
            "hard-braking",
            InteractionReasonCode::EffortfulResistance,
            VocalTrigger::MissAndRetry,
            PhysicalExpressionContext {
                contact: false,
                pressure: 0.0,
                deformation: 0.46,
                strain: 0.38,
                speed: 0.82,
                acceleration: 0.94,
                slosh_energy: 0.72,
                airborne: true,
                ..gentle
            },
            0.76,
            "neutral",
            ObjectFixture::None,
        ),
        fixture(
            "new-moving-object",
            InteractionReasonCode::CuriousInspection,
            VocalTrigger::VisualNotice,
            PhysicalExpressionContext {
                contact: false,
                pressure: 0.0,
                deformation: 0.04,
                strain: 0.02,
                speed: 0.18,
                acceleration: 0.20,
                airborne: true,
                ..gentle
            },
            0.68,
            "shy",
            ObjectFixture::Moving,
        ),
        fixture(
            "object-oriented-query",
            InteractionReasonCode::CuriousInspection,
            VocalTrigger::NeedHelp,
            PhysicalExpressionContext {
                contact: false,
                pressure: 0.0,
                deformation: 0.08,
                strain: 0.05,
                speed: 0.10,
                acceleration: 0.06,
                airborne: false,
                ..gentle
            },
            0.90,
            "neutral",
            ObjectFixture::Query,
        ),
        fixture(
            "ignored-social-bid",
            InteractionReasonCode::SharedRitual,
            VocalTrigger::ToyOffer,
            PhysicalExpressionContext {
                contact: false,
                pressure: 0.0,
                deformation: 0.12,
                strain: 0.06,
                speed: 0.20,
                acceleration: 0.12,
                airborne: true,
                ..gentle
            },
            0.0,
            "shy",
            ObjectFixture::None,
        ),
        fixture(
            "successful-user-response",
            InteractionReasonCode::SuccessfulReunion,
            VocalTrigger::CatchSuccess,
            PhysicalExpressionContext {
                pressure: 0.12,
                deformation: 0.18,
                strain: 0.08,
                slosh_energy: 0.30,
                speed: 0.24,
                acceleration: 0.16,
                airborne: true,
                ..gentle
            },
            1.0,
            "bold",
            ObjectFixture::None,
        ),
    ]
}

const fn fixture(
    name: &'static str,
    reason: InteractionReasonCode,
    trigger: VocalTrigger,
    physical: PhysicalExpressionContext,
    audience: f32,
    temperament: &'static str,
    object: ObjectFixture,
) -> Fixture {
    Fixture {
        name,
        reason,
        trigger,
        physical,
        audience,
        temperament,
        object,
    }
}

fn fixture_affect(fixture: Fixture) -> AffectState {
    let attachment = match fixture.temperament {
        "bold" => 0.88,
        "shy" => 0.34,
        _ => fixture.audience * 0.68,
    };
    let arousal = (fixture.physical.load() * 0.70
        + fixture.physical.speed * 0.22
        + if fixture.temperament == "bold" {
            0.12
        } else if fixture.temperament == "shy" {
            -0.04
        } else {
            0.0
        })
    .clamp(0.08, 0.92);
    let stress = if matches!(
        fixture.reason,
        InteractionReasonCode::CalmBoundary
            | InteractionReasonCode::PhysicalStartle
            | InteractionReasonCode::ComponentDetached
    ) {
        0.70
    } else if fixture.temperament == "shy" {
        0.28
    } else {
        0.10
    };
    AffectState {
        attachment,
        arousal,
        stress,
        ..AffectState::default()
    }
}

fn fixture_world(fixture: Fixture) -> WorldModelFrame {
    let mut world = WorldModelFrame {
        audience_present: fixture.audience > 0.0,
        audience_attention: fixture.audience,
        ..WorldModelFrame::default()
    };
    if fixture.object != ObjectFixture::None {
        world.observe(WorldEntity::point(
            0x0B1E_C700,
            WorldEntityKind::ProceduralObject,
            Vec2::new(0.72, 0.47),
            if fixture.object == ObjectFixture::Moving {
                Vec2::new(-0.18, 0.08)
            } else {
                Vec2::ZERO
            },
            0.96,
            0.94,
            if fixture.object == ObjectFixture::Query {
                WorldAffordance::Help
            } else {
                WorldAffordance::Observe
            },
            0.0,
        ));
    }
    world
}

fn baseline_plan(fixture: Fixture, id: u64) -> InteractionResponsePlan {
    let boundary = fixture.reason == InteractionReasonCode::CalmBoundary;
    let alarm = matches!(
        fixture.reason,
        InteractionReasonCode::PhysicalStartle | InteractionReasonCode::ComponentDetached
    );
    InteractionResponsePlan {
        response_id: id,
        episode_id: id,
        reason: fixture.reason,
        communicative_intent: if boundary {
            CommunicativeIntent::SetCalmBoundary
        } else {
            CommunicativeIntent::AcknowledgeContact
        },
        gaze: if fixture.physical.contact {
            InteractionGazeTarget::ContactPoint
        } else {
            InteractionGazeTarget::Cursor
        },
        expression: InteractionExpressionTarget {
            eye_aperture: if alarm { 1.0 } else { 0.88 },
            eye_scale: 1.0,
            mouth_curve: if boundary { -0.16 } else { 0.10 },
            mouth_compression: if boundary { 0.58 } else { 0.10 },
            effort: fixture.physical.load() * 0.45,
            amplitude: 0.46,
            ..InteractionExpressionTarget::default()
        },
        body: InteractionBodyActuation {
            local_pulse: if alarm { 0.020 } else { 0.008 },
            recoil: if alarm {
                0.055
            } else {
                fixture.physical.load() * 0.018
            },
            resistance: if boundary { 0.72 } else { 0.12 },
            cooperation: if boundary { 0.0 } else { 0.42 },
            ..InteractionBodyActuation::default()
        },
        voice_trigger: Some(fixture.trigger),
        onset_seconds: 0.08,
        hold_seconds: 0.25,
        release_seconds: 0.18,
        ..InteractionResponsePlan::default()
    }
}

fn render_audio(
    genome: &Genome,
    fixture: Fixture,
    index: u64,
    style: VoiceSynthesisStyle,
    path: &Path,
) -> Result<AudioMetrics, Box<dyn Error>> {
    let rendition_seed = if fixture.trigger == VocalTrigger::SoftTouch {
        0xA0D1_0001
    } else {
        0xA0D1_0000 + index
    };
    let mut life = LifeCore::new(genome.clone(), rendition_seed);
    life.state.affect = fixture_affect(fixture);
    let request = life
        .request_vocalization(fixture.trigger, &SensorFrame::default())
        .ok_or("fixture produced no vocal request")?;
    let motif = life
        .state
        .vocal_motifs
        .iter()
        .find(|motif| motif.id == request.motif_id)
        .ok_or("selected motif missing")?;
    let command = VoiceCommand::prepare_style(&genome.voice, motif, &request, style);
    let OfflinePcm::F32(samples) = render_motif_style(
        &genome.voice,
        motif,
        &request,
        48_000,
        1,
        OfflineSampleFormat::F32,
        style,
    ) else {
        unreachable!()
    };
    export_debug_wav(path, &samples, 48_000, 1)?;
    let count = samples.len().max(1) as f32;
    let seconds = samples.len() as f64 / 48_000.0;
    let rms = (samples.iter().map(|sample| sample * sample).sum::<f32>() / count).sqrt();
    let peak = samples.iter().copied().map(f32::abs).fold(0.0, f32::max);
    let zero_crossing_rate = samples
        .windows(2)
        .filter(|pair| pair[0].is_sign_positive() != pair[1].is_sign_positive())
        .count() as f32
        / count;
    Ok(AudioMetrics {
        style: if style == VoiceSynthesisStyle::Legacy {
            "legacy"
        } else {
            "living_mammalian"
        },
        motif_id: request.motif_id,
        performance_seed: request.performance_seed,
        gesture: request.gesture,
        sample_count: samples.len(),
        seconds,
        unit_rate_hz: f32::from(command.syllable_count) / seconds.max(0.001) as f32,
        rms,
        peak,
        zero_crossing_rate,
        spectral_centroid_hz: spectral_centroid(&samples, 48_000.0),
        roughness_index: roughness_index(&samples),
        nonlinear_event_count: nonlinear_event_count(&samples),
        base_f0_hz: command.base_pitch_hz,
        minimum_f0_hz: command.minimum_f0_hz,
        maximum_f0_hz: command.maximum_f0_hz,
        f0_contour_hz: commanded_f0_contour(&command),
    })
}

#[allow(clippy::too_many_arguments)]
fn render_video(
    ffmpeg: &Path,
    genome: &Genome,
    tuning: &LiquidTuningProfile,
    renderer: &mut Renderer,
    ecology_renderer: &mut EcologyRenderer,
    plan: InteractionResponsePlan,
    fixture: Fixture,
    path: &Path,
) -> Result<VideoMetrics, Box<dyn Error>> {
    renderer.reset_perceptual_capture_state();
    let mut body = ProceduralBody::generate(genome)?;
    body.apply_tuning_profile(tuning.clone())?;
    body.set_render_aspect(WIDTH as f32 / HEIGHT as f32);
    body.set_presentation_scale(0.72);
    body.embodiment
        .set_world_to_body_scale(Vec2::new(5.0, -3.0));
    let gaze_target = plan
        .gaze
        .world_position()
        .or_else(|| (plan.gaze == InteractionGazeTarget::Viewer).then_some(Vec2::splat(0.5)))
        .unwrap_or_else(|| {
            if plan.gaze == InteractionGazeTarget::Away {
                Vec2::new(0.28, 0.44)
            } else {
                Vec2::new(0.66, 0.46)
            }
        });
    let mut intent = BodyIntent {
        locomotion: LocomotionMode::Hover,
        target_position: Vec2::splat(0.5),
        target_surface: None,
        desired_speed: if fixture.physical.airborne {
            0.18 + fixture.physical.speed * 0.28
        } else {
            0.0
        },
        facing_direction: 1.0,
        gaze_target: Some(gaze_target),
        pose: if plan.body.resistance > 0.5 {
            PoseIntent::Compact
        } else if plan.expression.relief > 0.5 {
            PoseIntent::Display
        } else {
            PoseIntent::Curious
        },
        expression: expression_state(plan.expression),
        interaction_target: None,
    };
    let frame_count = (DURATION_SECONDS * FPS as f32).round() as u32;
    let mut child = Command::new(ffmpeg)
        .args([
            "-y",
            "-loglevel",
            "error",
            "-f",
            "rawvideo",
            "-pix_fmt",
            "rgba",
            "-s:v",
            "512x288",
            "-r",
            "24",
            "-i",
            "-",
            "-an",
            "-c:v",
            "libopenh264",
            "-b:v",
            "1200k",
            "-pix_fmt",
            "yuv420p",
        ])
        .arg(path)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    let mut stdin = child.stdin.take().ok_or("ffmpeg stdin unavailable")?;
    let mut sensors = SensorFrame::default();
    let mut ecology = EcologyState::new(genome.identity_seed);
    let mut maximum_strain = 0.0_f32;
    let mut maximum_speed = 0.0_f32;
    let mut maximum_stretch_ratio = 0.0_f32;
    let mut trace = Vec::with_capacity(frame_count as usize);
    let ticks = frame_count * (BODY_HZ / FPS);
    for tick in 0..ticks {
        let time = tick as f32 / BODY_HZ as f32;
        let phase = (time / DURATION_SECONDS).clamp(0.0, 1.0);
        let active = fixture.physical.contact && (0.12..0.82).contains(&phase);
        sensors.timestamp = f64::from(time);
        sensors.cursor_position = Vec2::new(
            0.52 + (phase * 8.0).sin() * fixture.physical.speed * 0.06,
            0.49,
        );
        sensors.cursor_velocity = Vec2::new((phase * 8.0).cos() * fixture.physical.speed, 0.0);
        sensors.pointer_down = active;
        sensors.pointer_pressed = active && tick == (ticks as f32 * 0.12) as u32;
        sensors.pointer_released = !active && tick == (ticks as f32 * 0.82) as u32;
        sensors.pet_hovered = active;
        sensors.pet_dragged = active;
        let envelope = interaction_envelope(phase);
        sensors.interaction_actuation = scale_actuation(plan.body, envelope);
        if fixture.physical.airborne {
            let brake = if fixture.name == "hard-braking" && phase > 0.48 {
                0.06
            } else {
                1.0
            };
            intent.desired_speed = (0.18 + fixture.physical.speed * 0.28) * brake;
            intent.target_position = Vec2::new(
                0.50 + (phase * std::f32::consts::TAU).sin() * 0.08,
                0.50 - (phase * std::f32::consts::PI).sin() * 0.05,
            );
        }
        if fixture.object != ObjectFixture::None {
            let position = if fixture.object == ObjectFixture::Moving {
                Vec2::new(
                    0.74 - phase * 0.18,
                    0.42 + (phase * std::f32::consts::TAU).sin() * 0.08,
                )
            } else {
                Vec2::new(0.72, 0.47)
            };
            if let Some(object) = ecology.objects.first_mut() {
                object.velocity = (position - object.position) * FPS as f32;
                object.position = position;
                object.novelty = 0.94;
            }
            intent.gaze_target = Some(position);
        }
        body.fixed_update(genome, &intent, &sensors, 1.0 / BODY_HZ as f32);
        body.embodied_update(
            &intent,
            &sensors,
            fixture_affect(fixture),
            VisualMindInput {
                arousal: fixture_affect(fixture).arousal,
                local_luminance: 0.48,
                ..VisualMindInput::default()
            },
            VoiceVisualState::default(),
            1.0 / BODY_HZ as f32,
        );
        sensors.embodied_interaction = body.embodied_interaction_frame();
        maximum_strain =
            maximum_strain.max(body.embodiment.liquid.diagnostics().maximum_bond_strain);
        maximum_speed = maximum_speed.max(body.embodiment.liquid.diagnostics().maximum_speed);
        maximum_stretch_ratio =
            maximum_stretch_ratio.max(body.embodiment.liquid.diagnostics().stretch_ratio);
        if tick % (BODY_HZ / FPS) == 0 {
            body.presentation_update(1.0 / FPS as f32);
            let mut parameters = body.render_parameters(genome, fixture_affect(fixture).arousal);
            parameters.time = time;
            let captured = if fixture.object == ObjectFixture::None {
                renderer.render_capture(parameters)?
            } else {
                renderer.render_capture_with_overlay(parameters, |_, queue, encoder, target| {
                    ecology_renderer.render(
                        queue,
                        encoder,
                        target,
                        &ecology,
                        WIDTH as f32 / HEIGHT as f32,
                        time,
                    );
                })?
            };
            stdin.write_all(&captured.rgba8)?;
            trace.push(BodyTracePoint {
                time_seconds: time,
                gaze_target: plan.gaze,
                local_pulse: sensors.interaction_actuation.local_pulse,
                recoil: sensors.interaction_actuation.recoil,
                resistance: sensors.interaction_actuation.resistance,
                cooperation: sensors.interaction_actuation.cooperation,
                maximum_strain: body.embodiment.liquid.diagnostics().maximum_bond_strain,
                material_speed: body.embodiment.liquid.diagnostics().maximum_speed,
                stretch_ratio: body.embodiment.liquid.diagnostics().stretch_ratio,
                observed_physical: PhysicalExpressionContext::from_frames(
                    &sensors,
                    &body.simulation.feedback,
                ),
            });
        }
    }
    drop(stdin);
    if !child.wait()?.success() {
        return Err("ffmpeg failed while encoding a production capture".into());
    }
    Ok(VideoMetrics {
        renderer: "pet_body::Renderer production GPU pipeline",
        width: WIDTH,
        height: HEIGHT,
        frames: frame_count,
        duration_seconds: DURATION_SECONDS,
        reaction_onset_seconds: plan.onset_seconds,
        expression_hold_seconds: plan.hold_seconds,
        maximum_strain,
        maximum_speed,
        maximum_stretch_ratio,
        recovery_count: body.embodiment.liquid.diagnostics().recovery_count,
        final_expression: intent.expression,
        body_actuation_trace: trace,
    })
}

fn interaction_envelope(phase: f32) -> f32 {
    if phase < 0.18 {
        phase / 0.18
    } else if phase > 0.82 {
        (1.0 - phase) / 0.18
    } else {
        1.0
    }
    .clamp(0.0, 1.0)
}

fn expression_state(target: InteractionExpressionTarget) -> ExpressionState {
    ExpressionState {
        eye_aperture: target.eye_aperture,
        eye_scale: target.eye_scale,
        mouth_curve: target.mouth_curve,
        mouth_compression: target.mouth_compression,
        mouth_asymmetry: target.mouth_asymmetry,
        brow_asymmetry: target.brow_asymmetry,
        effort: target.effort,
        relief: target.relief,
        ..ExpressionState::default()
    }
}

fn scale_actuation(mut body: InteractionBodyActuation, weight: f32) -> InteractionBodyActuation {
    body.compliance_delta *= weight;
    body.cohesion_delta *= weight;
    body.local_pulse *= weight;
    body.lean *= weight;
    body.recoil *= weight;
    body.resistance *= weight;
    body.cooperation *= weight;
    body
}

fn commanded_f0_contour(command: &VoiceCommand) -> Vec<f32> {
    command.syllables[..usize::from(command.syllable_count)]
        .iter()
        .flat_map(|syllable| {
            [
                syllable.pitch_start,
                syllable.pitch_peak,
                syllable.pitch_end,
            ]
        })
        .map(|pitch| {
            (command.base_pitch_hz * command.pitch_scale * pitch)
                .clamp(command.minimum_f0_hz, command.maximum_f0_hz)
        })
        .collect()
}

fn spectral_centroid(samples: &[f32], sample_rate: f32) -> f32 {
    let window_size = samples.len().min(2_048);
    if window_size < 2 {
        return 0.0;
    }
    let start = samples.len().saturating_sub(window_size) / 2;
    let window = &samples[start..start + window_size];
    let maximum_bin = ((8_000.0 * window_size as f32 / sample_rate) as usize).min(window_size / 2);
    let mut weighted = 0.0_f64;
    let mut magnitude_sum = 0.0_f64;
    for bin in 1..=maximum_bin {
        let mut real = 0.0_f64;
        let mut imaginary = 0.0_f64;
        for (index, sample) in window.iter().enumerate() {
            let hann =
                0.5 - 0.5 * (std::f64::consts::TAU * index as f64 / (window_size - 1) as f64).cos();
            let angle = std::f64::consts::TAU * bin as f64 * index as f64 / window_size as f64;
            real += f64::from(*sample) * hann * angle.cos();
            imaginary -= f64::from(*sample) * hann * angle.sin();
        }
        let magnitude = real.hypot(imaginary);
        let frequency = bin as f64 * f64::from(sample_rate) / window_size as f64;
        weighted += magnitude * frequency;
        magnitude_sum += magnitude;
    }
    if magnitude_sum > f64::EPSILON {
        (weighted / magnitude_sum) as f32
    } else {
        0.0
    }
}

fn roughness_index(samples: &[f32]) -> f32 {
    let frame = 240;
    let energies = samples
        .chunks(frame)
        .map(|chunk| {
            (chunk.iter().map(|sample| sample * sample).sum::<f32>() / chunk.len().max(1) as f32)
                .sqrt()
        })
        .collect::<Vec<_>>();
    if energies.len() < 2 {
        return 0.0;
    }
    let delta = energies
        .windows(2)
        .map(|pair| (pair[1] - pair[0]).abs())
        .sum::<f32>()
        / (energies.len() - 1) as f32;
    let mean = energies.iter().sum::<f32>() / energies.len() as f32;
    (delta / mean.max(1.0e-5)).clamp(0.0, 8.0)
}

fn nonlinear_event_count(samples: &[f32]) -> u32 {
    let frame = 240;
    let energies = samples
        .chunks(frame)
        .map(|chunk| chunk.iter().copied().map(f32::abs).fold(0.0, f32::max))
        .collect::<Vec<_>>();
    energies
        .windows(3)
        .filter(|window| {
            window[1] > 0.08 && window[1] > window[0] * 1.8 && window[1] > window[2] * 1.25
        })
        .count() as u32
}

fn find_ffmpeg() -> Result<PathBuf, Box<dyn Error>> {
    if let Some(path) = std::env::var_os("FFMPEG")
        .map(PathBuf::from)
        .filter(|path| path.is_file())
    {
        return Ok(path);
    }
    let krita = PathBuf::from(r"C:\Program Files\Krita (x64)\bin\ffmpeg.exe");
    if krita.is_file() {
        return Ok(krita);
    }
    Err("ffmpeg not found; set FFMPEG or install Krita's bundled encoder".into())
}

fn write_json(path: &Path, value: &impl Serialize) -> Result<(), Box<dyn Error>> {
    let mut writer = BufWriter::new(File::create(path)?);
    serde_json::to_writer_pretty(&mut writer, value)?;
    writer.write_all(b"\n")?;
    writer.flush()?;
    Ok(())
}
