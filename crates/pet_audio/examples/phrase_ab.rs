//! Read-only saved-voice A/B. Usage: phrase_ab <state.json> <output-dir> [motif-id]
//! Source state is never changed. No live audio device is opened.
use lifecore::{VocalFamily, VocalMotif, VocalRequest, VocalStyle, VoiceGenome};
use pet_audio::{
    OfflinePcm, OfflineSampleFormat, PhraseVariationState, VoiceCommand, export_debug_wav,
    render_prepared_command,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let state_path = args.get(1).ok_or("expected state.json path")?;
    let output = std::path::PathBuf::from(args.get(2).ok_or("expected output directory")?);
    let saved: serde_json::Value = serde_json::from_slice(&std::fs::read(state_path)?)?;
    let state = &saved["life"]["state"];
    let voice: VoiceGenome = serde_json::from_value(state["genome"]["voice"].clone())?;
    let motifs: Vec<VocalMotif> = serde_json::from_value(state["vocal_motifs"].clone())?;
    let requested = args.get(3).map(|id| id.parse::<u64>()).transpose()?;
    let motif = motifs
        .iter()
        .find(|motif| {
            requested.map_or(
                motif.family == VocalFamily::SoftContact && motif.syllables.len() == 5,
                |id| motif.id == id,
            )
        })
        .ok_or("matching stored motif not found")?;
    let mut phrasing = PhraseVariationState::default();
    let mut before = Vec::new();
    let mut after = Vec::new();
    let mut counts = Vec::new();
    let mut orders = Vec::new();
    for index in 0..8 {
        // Controlled request, not a claim that these were the live user's moods.
        let request = VocalRequest {
            motif_id: motif.id,
            performance_seed: 100 + index,
            gain: 0.24,
            pan: 0.0,
            pitch_scale: 1.0,
            tempo_scale: 1.0,
            stress: 0.1,
            purr: false,
            gesture: lifecore::VoiceGesture::WarmChuff,
            priority: 128,
            style: VocalStyle::SocialContact,
            valence: 0.4,
            arousal: 0.65,
            fatigue: 0.15,
            confidence: 0.8,
            attachment: 0.5,
            rhythm_intervals: [0.0; 8],
            phenotype: Default::default(),
        };
        let baseline = VoiceCommand::prepare(&voice, motif, &request);
        let varied = phrasing.prepare(&voice, motif, &request);
        counts.push(varied.syllable_count);
        orders.push(
            varied.syllables[..varied.syllable_count as usize]
                .iter()
                .map(|s| (s.gesture.frontness * 100.0).round() as i32)
                .collect::<Vec<_>>(),
        );
        for (command, destination) in [(baseline, &mut before), (varied, &mut after)] {
            let rendered = render_prepared_command(command, 24_000, 2, OfflineSampleFormat::F32);
            let OfflinePcm::F32(samples) = rendered.pcm else {
                unreachable!()
            };
            assert!(
                samples
                    .iter()
                    .all(|sample| sample.is_finite() && sample.abs() <= 1.0)
            );
            destination.extend(samples);
            destination.extend(std::iter::repeat_n(0.0, 24_000 * 2 / 2));
        }
    }
    std::fs::create_dir_all(&output)?;
    export_debug_wav(
        &output.join("A-stored-five-segments.wav"),
        &before,
        24_000,
        2,
    )?;
    export_debug_wav(
        &output.join("B-production-structural-phrases.wav"),
        &after,
        24_000,
        2,
    )?;
    println!(
        "motif_id={} stored_count={} performed_counts={counts:?}",
        motif.id,
        motif.syllables.len()
    );
    println!("articulation_frontness_trajectory={orders:?}");
    println!(
        "A_seconds={:.2} B_seconds={:.2}; output={}",
        before.len() as f32 / 48_000.0,
        after.len() as f32 / 48_000.0,
        output.display()
    );
    Ok(())
}
