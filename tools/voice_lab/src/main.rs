use std::{env, error::Error, fs, path::PathBuf};

use lifecore::{
    BodyVoiceFrame, Genome, VocalFamily, VocalMotif, VocalRequest, VocalStyle, VoiceGesture,
    generate_initial_motifs,
};
use pet_audio::{
    BodyVoiceAnalyzer, OfflinePcm, OfflineSampleFormat, VoiceCommand, VoiceDiagnostics,
    export_debug_wav, render_motif_with_body_timeline,
};
use serde::Serialize;

const SAMPLE_RATE: u32 = 48_000;
const CHANNELS: u16 = 1;
const BODY_RATE_HZ: u32 = 100;
const VOICE_SEED: u64 = 0x0A11_FEED_2026_0901;

#[derive(Debug, Clone, Copy)]
enum BodyScript {
    Neutral,
    GentleTouch,
    StartleTouch,
    Stretch,
    Compression,
    Detach,
    Remerge,
    Slosh,
}

#[derive(Debug, Clone, Copy)]
struct ScenarioSpec {
    name: &'static str,
    family: VocalFamily,
    style: VocalStyle,
    gesture: VoiceGesture,
    body: BodyScript,
    valence: f32,
    arousal: f32,
    fatigue: f32,
    confidence: f32,
    attachment: f32,
    stress: f32,
    gain: f32,
    purr: bool,
}

#[derive(Debug, Serialize)]
struct GateResults {
    passed: bool,
    finite: bool,
    bounded: bool,
    dc_safe: bool,
    continuous: bool,
    f0_safe: bool,
    pressure_onset: bool,
    tract_audible: bool,
    purr_irregular: bool,
    rms: f32,
}

#[derive(Debug, Serialize)]
struct ScenarioManifest {
    name: &'static str,
    voice_seed: u64,
    wav: String,
    sample_rate: u32,
    channels: u16,
    request: VocalRequest,
    body_frame_rate_hz: u32,
    body_timeline: Vec<BodyVoiceFrame>,
    diagnostics: VoiceDiagnostics,
    gates: GateResults,
}

#[derive(Debug, Serialize)]
struct SuiteManifest {
    schema: u8,
    suite: &'static str,
    backend: &'static str,
    scenarios: Vec<ScenarioManifest>,
    passed: bool,
}

fn main() -> Result<(), Box<dyn Error>> {
    let output = parse_arguments()?;
    fs::create_dir_all(&output)?;
    let genome = Genome::from_seed(VOICE_SEED);
    let canonical = generate_initial_motifs(&genome.voice);
    let mut manifests = Vec::with_capacity(scenarios().len());

    for (index, spec) in scenarios().into_iter().enumerate() {
        let mut motif = canonical
            .iter()
            .find(|motif| motif.family == spec.family)
            .cloned()
            .expect("all Voice Lab families are canonical");
        tailor_motif(&mut motif, index);
        let request = scenario_request(spec, motif.id, index);
        let command = VoiceCommand::prepare(&genome.voice, &motif, &request);
        let frame_count = command.total_frames(SAMPLE_RATE);
        let body_frame_count = ((frame_count as u64 * u64::from(BODY_RATE_HZ))
            .div_ceil(u64::from(SAMPLE_RATE))) as usize;
        let body_timeline = build_body_timeline(spec.body, body_frame_count.max(1));
        let render = render_motif_with_body_timeline(
            &genome.voice,
            &motif,
            &request,
            &body_timeline,
            BODY_RATE_HZ,
            SAMPLE_RATE,
            CHANNELS,
            OfflineSampleFormat::F32,
        );
        let OfflinePcm::F32(samples) = render.pcm else {
            unreachable!("Voice Lab requests f32 PCM")
        };
        let wav_path = output.join(format!("{}.wav", spec.name));
        export_debug_wav(&wav_path, &samples, SAMPLE_RATE, CHANNELS)?;
        let gates = evaluate_gates(&samples, render.diagnostics, spec.purr);
        manifests.push(ScenarioManifest {
            name: spec.name,
            voice_seed: VOICE_SEED,
            wav: wav_path.display().to_string(),
            sample_rate: SAMPLE_RATE,
            channels: CHANNELS,
            request,
            body_frame_rate_hz: BODY_RATE_HZ,
            body_timeline,
            diagnostics: render.diagnostics,
            gates,
        });
    }

    let passed = manifests.iter().all(|scenario| scenario.gates.passed);
    let manifest = SuiteManifest {
        schema: 1,
        suite: "organic-embodied",
        backend: "breath-lf-glottis-oral-nasal-waveguide-liquid-body",
        scenarios: manifests,
        passed,
    };
    let manifest_path = output.join("manifest.json");
    fs::write(&manifest_path, serde_json::to_vec_pretty(&manifest)?)?;
    println!(
        "Voice Lab: {}/{} scenarios passed; output={}",
        manifest
            .scenarios
            .iter()
            .filter(|scenario| scenario.gates.passed)
            .count(),
        manifest.scenarios.len(),
        output.display()
    );
    if passed {
        Ok(())
    } else {
        Err("one or more Voice Lab scenarios failed diagnostics".into())
    }
}

fn parse_arguments() -> Result<PathBuf, Box<dyn Error>> {
    let mut suite = None;
    let mut output = PathBuf::from("artifacts/voice_lab/organic-embodied");
    let mut arguments = env::args().skip(1);
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--suite" => suite = arguments.next(),
            "--output" => {
                output = PathBuf::from(arguments.next().ok_or("--output requires a directory")?);
            }
            _ => return Err(format!("unknown Voice Lab argument: {argument}").into()),
        }
    }
    if suite.as_deref().unwrap_or("organic-embodied") != "organic-embodied" {
        return Err("only the final organic-embodied backend is available".into());
    }
    Ok(output)
}

fn scenarios() -> [ScenarioSpec; 15] {
    [
        scenario(
            "01_soft_contact",
            VocalFamily::SoftContact,
            VocalStyle::SocialContact,
            VoiceGesture::WarmChuff,
            BodyScript::Neutral,
            0.35,
            0.22,
            0.0,
            0.82,
            0.72,
            0.05,
            0.25,
            false,
        ),
        scenario(
            "02_question_whine",
            VocalFamily::QuestionWhine,
            VocalStyle::AttentionCall,
            VoiceGesture::MewWhine,
            BodyScript::Neutral,
            0.05,
            0.48,
            0.0,
            0.64,
            0.48,
            0.18,
            0.24,
            false,
        ),
        scenario(
            "03_play_yip",
            VocalFamily::PlayYip,
            VocalStyle::PlayInvite,
            VoiceGesture::ClippedPulse,
            BodyScript::Neutral,
            0.72,
            0.78,
            0.0,
            0.90,
            0.64,
            0.16,
            0.27,
            false,
        ),
        scenario(
            "04_touch_gentle",
            VocalFamily::SoftContact,
            VocalStyle::TouchResponse,
            VoiceGesture::WarmChuff,
            BodyScript::GentleTouch,
            0.58,
            0.30,
            0.0,
            0.86,
            0.82,
            0.04,
            0.24,
            false,
        ),
        scenario(
            "05_touch_startle",
            VocalFamily::StartleSqueak,
            VocalStyle::Startle,
            VoiceGesture::ClippedPulse,
            BodyScript::StartleTouch,
            -0.30,
            0.96,
            0.0,
            0.25,
            0.22,
            0.92,
            0.28,
            false,
        ),
        scenario(
            "06_content_murmur",
            VocalFamily::ContentMurmur,
            VocalStyle::ContentMurmur,
            VoiceGesture::LowRumble,
            BodyScript::Neutral,
            0.76,
            0.16,
            0.0,
            0.92,
            0.94,
            0.02,
            0.24,
            false,
        ),
        scenario(
            "07_purr",
            VocalFamily::Purr,
            VocalStyle::Purr,
            VoiceGesture::PurrHum,
            BodyScript::Neutral,
            0.88,
            0.12,
            0.0,
            0.94,
            1.0,
            0.0,
            0.23,
            true,
        ),
        scenario(
            "08_fatigued",
            VocalFamily::QuestionWhine,
            VocalStyle::AttentionCall,
            VoiceGesture::WarmChuff,
            BodyScript::Neutral,
            -0.08,
            0.24,
            0.70,
            0.42,
            0.55,
            0.12,
            0.24,
            false,
        ),
        scenario(
            "09_body_stretch",
            VocalFamily::SoftContact,
            VocalStyle::SocialContact,
            VoiceGesture::WarmChuff,
            BodyScript::Stretch,
            0.18,
            0.38,
            0.0,
            0.74,
            0.60,
            0.25,
            0.25,
            false,
        ),
        scenario(
            "10_body_compression",
            VocalFamily::ContentMurmur,
            VocalStyle::TouchResponse,
            VoiceGesture::LowRumble,
            BodyScript::Compression,
            0.02,
            0.46,
            0.0,
            0.68,
            0.55,
            0.42,
            0.25,
            false,
        ),
        scenario(
            "11_detach",
            VocalFamily::AttentionCall,
            VocalStyle::AttentionCall,
            VoiceGesture::MewWhine,
            BodyScript::Detach,
            -0.34,
            0.75,
            0.0,
            0.45,
            0.50,
            0.62,
            0.27,
            false,
        ),
        scenario(
            "12_remerge",
            VocalFamily::SoftContact,
            VocalStyle::TouchResponse,
            VoiceGesture::WarmChuff,
            BodyScript::Remerge,
            0.68,
            0.48,
            0.0,
            0.82,
            0.90,
            0.12,
            0.26,
            false,
        ),
        scenario(
            "13_slosh",
            VocalFamily::PlayfulTrill,
            VocalStyle::PlayInvite,
            VoiceGesture::WarmChuff,
            BodyScript::Slosh,
            0.55,
            0.66,
            0.0,
            0.78,
            0.62,
            0.20,
            0.25,
            false,
        ),
        scenario(
            "14_rhythm_mimic",
            VocalFamily::RhythmMimic,
            VocalStyle::RhythmMimic,
            VoiceGesture::ClippedPulse,
            BodyScript::Neutral,
            0.42,
            0.56,
            0.0,
            0.84,
            0.58,
            0.10,
            0.25,
            false,
        ),
        scenario(
            "15_frustrated_grunt",
            VocalFamily::FrustratedGrunt,
            VocalStyle::Frustrated,
            VoiceGesture::LowRumble,
            BodyScript::Compression,
            -0.72,
            0.88,
            0.18,
            0.38,
            0.28,
            0.94,
            0.28,
            false,
        ),
    ]
}

#[allow(clippy::too_many_arguments)]
const fn scenario(
    name: &'static str,
    family: VocalFamily,
    style: VocalStyle,
    gesture: VoiceGesture,
    body: BodyScript,
    valence: f32,
    arousal: f32,
    fatigue: f32,
    confidence: f32,
    attachment: f32,
    stress: f32,
    gain: f32,
    purr: bool,
) -> ScenarioSpec {
    ScenarioSpec {
        name,
        family,
        style,
        gesture,
        body,
        valence,
        arousal,
        fatigue,
        confidence,
        attachment,
        stress,
        gain,
        purr,
    }
}

fn scenario_request(spec: ScenarioSpec, motif_id: u64, index: usize) -> VocalRequest {
    let mut rhythm_intervals = [0.0; 8];
    if spec.style == VocalStyle::RhythmMimic {
        rhythm_intervals[..5].copy_from_slice(&[0.72, 1.18, 0.78, 1.46, 0.92]);
    }
    VocalRequest {
        motif_id,
        performance_seed: VOICE_SEED ^ (index as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15),
        gain: spec.gain,
        pan: 0.0,
        pitch_scale: 1.0,
        tempo_scale: 1.0,
        stress: spec.stress,
        purr: spec.purr,
        gesture: spec.gesture,
        priority: 180,
        style: spec.style,
        valence: spec.valence,
        arousal: spec.arousal,
        fatigue: spec.fatigue,
        confidence: spec.confidence,
        attachment: spec.attachment,
        rhythm_intervals,
        phenotype: Default::default(),
    }
}

fn tailor_motif(motif: &mut VocalMotif, index: usize) {
    let syllable = motif
        .syllables
        .first()
        .cloned()
        .expect("canonical motifs contain a syllable");
    match index {
        1 => {
            motif.syllables.truncate(1);
            motif.syllables[0].duration_ms = 1_050.0;
            motif.syllables[0].gap_after_ms = 0.0;
            motif.syllables[0].pitch_start = 0.84;
            motif.syllables[0].pitch_peak = 1.27;
            motif.syllables[0].pitch_end = 1.06;
        }
        2 => {
            motif.syllables = (0..3)
                .map(|pulse| {
                    let mut value = syllable.clone();
                    value.duration_ms = 86.0 + pulse as f32 * 9.0;
                    value.gap_after_ms = if pulse == 2 { 0.0 } else { 62.0 };
                    value.pitch_peak = 1.05 + pulse as f32 * 0.07;
                    value
                })
                .collect();
        }
        6 => set_sustained(motif, 3_928.0, 0.96, 1.01, 0.94),
        7 => {
            set_sustained(motif, 1_350.0, 0.91, 1.04, 0.86);
            // Fatigue reduces this motor target during preparation. Start from
            // a viable intended pressure so the result is weak and leaky, not
            // an inaudible failed attempt.
            motif.syllables[0].gesture.pressure_peak = 0.78;
            motif.syllables[0].gesture.adduction = 0.48;
            motif.syllables[0].gesture.open_quotient = 0.68;
        }
        8..=12 => set_sustained(motif, 1_600.0, 0.94, 1.08, 0.98),
        14 => {
            motif.syllables = (0..3)
                .map(|pulse| {
                    let mut value = syllable.clone();
                    value.duration_ms = 92.0;
                    value.gap_after_ms = if pulse == 2 { 0.0 } else { 48.0 };
                    value.pitch_start = 0.86;
                    value.pitch_peak = 0.96 + pulse as f32 * 0.03;
                    value.pitch_end = 0.82;
                    value
                })
                .collect();
        }
        _ => {}
    }
}

fn set_sustained(motif: &mut VocalMotif, duration_ms: f32, start: f32, peak: f32, end: f32) {
    motif.syllables.truncate(1);
    motif.syllables[0].duration_ms = duration_ms;
    motif.syllables[0].gap_after_ms = 0.0;
    motif.syllables[0].pitch_start = start;
    motif.syllables[0].pitch_peak = peak;
    motif.syllables[0].pitch_end = end;
}

fn build_body_timeline(script: BodyScript, frame_count: usize) -> Vec<BodyVoiceFrame> {
    let mut analyzer = BodyVoiceAnalyzer::default();
    (0..frame_count)
        .map(|frame| {
            let progress = frame as f32 / frame_count.saturating_sub(1).max(1) as f32;
            let raw = scripted_body_frame(script, progress, frame, frame_count);
            analyzer.update(raw, 1.0 / BODY_RATE_HZ as f32)
        })
        .collect()
}

fn scripted_body_frame(
    script: BodyScript,
    progress: f32,
    frame: usize,
    frame_count: usize,
) -> BodyVoiceFrame {
    let mut body = BodyVoiceFrame::default();
    match script {
        BodyScript::Neutral => {}
        BodyScript::GentleTouch => {
            body.contact_area = bell(progress, 0.34, 0.22) * 0.24;
            body.material_stress = bell(progress, 0.34, 0.20) * 0.16;
            body.contact_impulse = event(frame, frame_count, 0.24) * 0.18;
        }
        BodyScript::StartleTouch => {
            body.contact_area = bell(progress, 0.30, 0.11) * 0.72;
            body.material_stress = bell(progress, 0.30, 0.14) * 0.88;
            body.collision_impulse = event(frame, frame_count, 0.22) * 0.95;
            body.contact_impulse = event(frame, frame_count, 0.23) * 0.82;
        }
        BodyScript::Stretch => {
            let amount = smoothstep(((progress - 0.12) / 0.68).clamp(0.0, 1.0));
            body.stretch = amount * 0.88;
            body.shape_aspect_ratio = 1.0 + amount * 1.85;
            body.bond_strain = amount * 0.74;
            body.material_stress = amount * 0.46;
        }
        BodyScript::Compression => {
            let amount = if progress < 0.58 {
                smoothstep((progress / 0.58).clamp(0.0, 1.0))
            } else {
                1.0 - smoothstep(((progress - 0.58) / 0.34).clamp(0.0, 1.0))
            };
            body.compression = amount * 0.86;
            body.shape_aspect_ratio = 1.0 + amount * 0.22;
            body.material_stress = amount * 0.84;
            body.contact_area = amount * 0.58;
            body.release_impulse = event(frame, frame_count, 0.60) * 0.62;
        }
        BodyScript::Detach => {
            let amount = smoothstep(((progress - 0.28) / 0.34).clamp(0.0, 1.0)) * 0.31;
            body.detached_mass_ratio = amount;
            body.main_mass_ratio = 1.0 - amount;
            body.component_count = if amount > 0.01 { 2 } else { 1 };
            body.bond_strain = bell(progress, 0.32, 0.16) * 0.92;
            body.detach_impulse = event(frame, frame_count, 0.29) * 0.78;
        }
        BodyScript::Remerge => {
            let amount = if progress < 0.57 {
                0.27
            } else {
                0.27 * (1.0 - smoothstep(((progress - 0.57) / 0.16).clamp(0.0, 1.0)))
            };
            body.detached_mass_ratio = amount;
            body.main_mass_ratio = 1.0 - amount;
            body.component_count = if amount > 0.01 { 2 } else { 1 };
            body.remerge_impulse = event(frame, frame_count, 0.69) * 0.82;
            body.slosh_energy = bell(progress, 0.70, 0.18) * 0.52;
        }
        BodyScript::Slosh => {
            let stepped_noise = hash_unit(frame as u64 / 5) * 0.28;
            body.slosh_energy = (0.62 + stepped_noise).clamp(0.0, 1.0);
            body.internal_speed = (0.54 + hash_unit(frame as u64 / 3 + 91) * 0.36).clamp(0.0, 1.0);
            body.shape_aspect_ratio = 1.04;
        }
    }
    body
}

fn event(frame: usize, frame_count: usize, at: f32) -> f32 {
    let event_frame = (frame_count as f32 * at).round() as usize;
    usize::from(frame == event_frame) as f32
}

fn bell(value: f32, center: f32, width: f32) -> f32 {
    (1.0 - ((value - center) / width.max(0.001)).abs()).clamp(0.0, 1.0)
}

fn smoothstep(value: f32) -> f32 {
    value * value * (3.0 - 2.0 * value)
}

fn hash_unit(mut value: u64) -> f32 {
    value = value.wrapping_add(0x9E37_79B9_7F4A_7C15);
    value = (value ^ (value >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    value ^= value >> 31;
    ((value >> 40) as f32 / (1_u32 << 24) as f32) * 2.0 - 1.0
}

fn evaluate_gates(
    samples: &[f32],
    diagnostics: VoiceDiagnostics,
    require_purr: bool,
) -> GateResults {
    let finite = samples.iter().all(|sample| sample.is_finite());
    let rms = (samples.iter().map(|sample| sample * sample).sum::<f32>()
        / samples.len().max(1) as f32)
        .sqrt();
    let bounded = diagnostics.maximum_sample <= 0.78 && rms > 0.002;
    let dc_safe = diagnostics.dc_mean.abs() < 0.0025;
    let continuous = diagnostics.maximum_discontinuity < 0.16;
    let f0_safe = diagnostics.maximum_f0_hz <= 1_600.0;
    let pressure_onset = diagnostics.pressure_onset_frame.is_some();
    let tract_audible = diagnostics.oral_energy > 0.0 && diagnostics.nasal_energy > 0.0;
    let purr_irregular = !require_purr
        || (diagnostics.purr_event_count > 20
            && (0.04..=0.12).contains(&diagnostics.purr_interval_cv));
    let passed = finite
        && bounded
        && dc_safe
        && continuous
        && f0_safe
        && pressure_onset
        && tract_audible
        && purr_irregular
        && diagnostics.non_finite_resets == 0;
    GateResults {
        passed,
        finite,
        bounded,
        dc_safe,
        continuous,
        f0_safe,
        pressure_onset,
        tract_audible,
        purr_irregular,
        rms,
    }
}
