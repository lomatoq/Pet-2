//! Offline production-renderer fixtures, no live device and no persisted state.
use lifecore::{VocalFamily, VocalRequest, VocalStyle, VoiceGesture};
use pet_audio::{OfflinePcm, OfflineSampleFormat, PhraseVariationState};
use std::{error::Error, path::PathBuf};

fn main() -> Result<(), Box<dyn Error>> {
    let output = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("artifacts/voice-v14"));
    std::fs::create_dir_all(&output)?;
    let voice = lifecore::Genome::from_seed(42).voice;
    let motifs = lifecore::generate_initial_motifs(&voice);
    let cases = [
        (
            "angry-shout",
            VocalFamily::FrustratedGrunt,
            VocalStyle::Frustrated,
            VoiceGesture::LowRumble,
            -1.0,
            1.0,
            0.8,
        ),
        (
            "alarm",
            VocalFamily::StartleSqueak,
            VocalStyle::Startle,
            VoiceGesture::ClippedPulse,
            -0.8,
            1.0,
            1.0,
        ),
        (
            "cheerful-laugh",
            VocalFamily::PlayYip,
            VocalStyle::PlayInvite,
            VoiceGesture::WarmChuff,
            0.9,
            0.9,
            0.1,
        ),
        (
            "brief-play-yip",
            VocalFamily::PlayYip,
            VocalStyle::PlayInvite,
            VoiceGesture::WarmChuff,
            0.3,
            0.5,
            0.1,
        ),
        (
            "quiet-breathy-not-full-whisper",
            VocalFamily::SoftContact,
            VocalStyle::SocialContact,
            VoiceGesture::ReliefExhale,
            0.3,
            0.2,
            0.1,
        ),
    ];
    let mut metadata = Vec::new();
    for (index, (name, family, style, gesture, valence, arousal, stress)) in
        cases.into_iter().enumerate()
    {
        let motif = motifs
            .iter()
            .find(|m| m.family == family)
            .ok_or("missing fixture family")?;
        let quiet = index == 4;
        let mut request = VocalRequest {
            motif_id: motif.id,
            performance_seed: 100 + index as u64,
            gain: if quiet { 0.24 * 0.22 } else { 0.24 },
            pan: 0.0,
            pitch_scale: 1.0,
            tempo_scale: 1.0,
            stress,
            purr: false,
            gesture,
            priority: 128,
            style,
            valence,
            arousal,
            fatigue: 0.15,
            confidence: 0.8,
            attachment: 0.5,
            rhythm_intervals: [0.0; 8],
            phenotype: Default::default(),
        };
        if quiet {
            request.phenotype.breathiness_delta = 0.25;
        }
        let command = PhraseVariationState::default().prepare(&voice, motif, &request);
        metadata.push(render(&output, name, command)?);
    }
    metadata.push(render(
        &output,
        "sleep-breath",
        pet_audio::prepare_nonphonated(
            &voice,
            pet_audio::NonPhonatedRequest {
                kind: pet_audio::NonPhonatedKind::SleepBreath,
                intensity: 0.25,
                pan: 0.0,
                seed: 414,
            },
        ),
    )?);
    let report = serde_json::json!({
        "fixture": "public genome seed42; synthetic requests, not live recordings",
        "renderer": "production SynthVoice; 24000Hz mono 16bit WAV",
        "thresholds": { "angry_shout": "Frustrated arousal>.72 and valence<-.45",
            "alarm_effort": "Startle arousal>.65 and stress>.50; no pain inference",
            "laugh": "PlayInvite valence>.55 and arousal>.65",
            "quiet": "gain*.22 and breathiness_delta=.25; not full unvoiced whisper" },
        "listening_verified": false, "clips": metadata,
    });
    std::fs::write(
        output.join("metadata.json"),
        serde_json::to_string_pretty(&report)?,
    )?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

fn render(
    output: &std::path::Path,
    name: &str,
    command: pet_audio::VoiceCommand,
) -> Result<serde_json::Value, Box<dyn Error>> {
    let result = pet_audio::render_prepared_command(command, 24_000, 1, OfflineSampleFormat::F32);
    let OfflinePcm::F32(samples) = result.pcm else {
        return Err("float output expected".into());
    };
    assert!(samples.iter().all(|v| v.is_finite() && v.abs() <= 1.0));
    let peak = samples.iter().map(|v| v.abs()).fold(0.0f32, f32::max);
    let rms =
        (samples.iter().map(|v| f64::from(*v).powi(2)).sum::<f64>() / samples.len() as f64).sqrt();
    if name == "quiet-breathy-not-full-whisper" || name == "sleep-breath" {
        assert!((0.0001..0.003).contains(&rms), "{name}: RMS {rms}");
        assert!(peak < 0.02, "quiet fixture peak {peak}");
    }
    pet_audio::export_debug_wav(&output.join(format!("{name}.wav")), &samples, 24_000, 1)?;
    Ok(
        serde_json::json!({ "file": format!("{name}.wav"), "seconds": samples.len() as f64 / 24000.0,
        "peak": peak, "rms": rms, "segments": command.syllable_count, "effort": command.shout,
        "nonphonated": command.nonphonated.is_some(), "gain": command.gain }),
    )
}
