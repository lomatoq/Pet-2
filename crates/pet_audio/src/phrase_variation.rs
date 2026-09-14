//! Continuous, affect-conditioned phrase composition on the producer thread.
//! No frame/sample noise, finite preset bank, voice-genome mutation or new calls.
use crate::VoiceCommand;
use lifecore::{VocalMotif, VocalRequest, VocalStyle, VoiceGenome};

const HISTORY: usize = 16;
const MAX_PROPOSALS: u64 = 64;

/// Quantized *audible* rhythm/contour, not RNG state or low floating-point bits.
/// Durations: 4% bins; pauses: 12%; pitch anchors: quarter-semitone bins.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PhraseFingerprint([i16; 11]);

impl PhraseFingerprint {
    fn from_command(command: &VoiceCommand, baseline: &VoiceCommand) -> Self {
        let count = usize::from(command.syllable_count);
        if count == 0 {
            return Self([0; 11]);
        }
        let middle = (count - 1) / 2;
        let first = command.syllables[0];
        let mid = command.syllables[middle];
        let last = command.syllables[count - 1];
        let bfirst = baseline.syllables[0];
        let base_last = usize::from(baseline.syllable_count).saturating_sub(1);
        let bmid = baseline.syllables[middle.min(base_last)];
        let blast = baseline.syllables[base_last];
        let gaps = command.syllables[..count]
            .iter()
            .map(|s| s.gap_after_ms)
            .sum::<f32>();
        let base_gaps = baseline.syllables[..usize::from(baseline.syllable_count)]
            .iter()
            .map(|s| s.gap_after_ms)
            .sum::<f32>();
        let quant = |value: f32, bin: f32| (value / bin).round().clamp(-32767.0, 32767.0) as i16;
        let pitch =
            |value: f32, base: f32| quant(12.0 * (value / base.max(0.001)).max(0.001).log2(), 0.25);
        Self([
            quant(first.duration_ms / bfirst.duration_ms.max(1.0), 0.04),
            quant(mid.duration_ms / bmid.duration_ms.max(1.0), 0.04),
            quant(last.duration_ms / blast.duration_ms.max(1.0), 0.04),
            quant(
                if base_gaps > 0.1 {
                    gaps / base_gaps
                } else {
                    1.0
                },
                0.12,
            ),
            pitch(first.pitch_start, bfirst.pitch_start),
            pitch(mid.pitch_peak, bmid.pitch_peak),
            pitch(last.pitch_end, blast.pitch_end),
            count as i16,
            quant(first.gesture.frontness, 0.10),
            quant(mid.gesture.constriction, 0.08),
            quant(last.gesture.nasality, 0.10),
        ])
    }

    /// At least ~16% note duration, 36% pause ratio OR .75 semitone differs.
    /// This prevents novelty being claimed from an inaudible low-bit change.
    fn distance(self, other: Self) -> f32 {
        self.0
            .iter()
            .zip(other.0)
            .enumerate()
            .map(|(i, (a, b))| {
                f32::from(a.abs_diff(b))
                    / if i < 3 {
                        4.0
                    } else if i == 7 {
                        0.5
                    } else {
                        3.0
                    }
            })
            .fold(0.0, f32::max)
    }
}

#[derive(Debug, Clone, Default)]
pub struct PhraseVariationState {
    sequence: u64,
    recent: [Option<PhraseFingerprint>; HISTORY],
}

impl PhraseVariationState {
    /// One invocation per admitted utterance. AudioEngine stages a clone and
    /// commits it only when queue admission succeeds; callback never uses this.
    pub fn prepare(
        &mut self,
        voice: &VoiceGenome,
        motif: &VocalMotif,
        request: &VocalRequest,
    ) -> VoiceCommand {
        let baseline = VoiceCommand::prepare(voice, motif, request);
        if motif.syllables.is_empty()
            || !matches!(
                request.style,
                VocalStyle::SocialContact
                    | VocalStyle::TouchResponse
                    | VocalStyle::PlayInvite
                    | VocalStyle::AttentionCall
                    | VocalStyle::ContentMurmur
                    | VocalStyle::Purr
                    | VocalStyle::Frustrated
            )
        {
            return baseline;
        }
        let root = mix(request.performance_seed ^ motif.seed)
            .wrapping_add(mix(self.sequence ^ 0x5048_5241_5345));
        let mut selected = baseline;
        let mut selected_key = PhraseFingerprint::from_command(&baseline, &baseline);
        let mut best_distance = -1.0;
        // Reuse the allocated syllable buffer across proposals; every proposal
        // starts from the learned motif, never accumulates mutation/drift.
        let mut performed = motif.clone();
        for attempt in 0..MAX_PROPOSALS {
            compose(
                &mut performed,
                motif,
                request,
                mix(root.wrapping_add(attempt)),
            );
            let mut candidate = VoiceCommand::prepare(voice, &performed, request);
            candidate.shout = shout_intensity(request);
            let key = PhraseFingerprint::from_command(&candidate, &baseline);
            let nearest = self
                .recent
                .iter()
                .flatten()
                .map(|old| key.distance(*old))
                .fold(f32::INFINITY, f32::min);
            if nearest > best_distance {
                selected = candidate;
                selected_key = key;
                best_distance = nearest;
            }
            if nearest >= 1.0 {
                break;
            }
        }
        // If a highly constrained style exhausts the search, retain the farthest
        // legal candidate; never exceed pitch/energy limits to force novelty.
        self.sequence = self.sequence.wrapping_add(1);
        self.recent.rotate_right(1);
        self.recent[0] = Some(selected_key);
        selected
    }
}

/// Correlated latent controls define one smooth phrase arc. Individual syllables
/// sample that arc; there are no independently randomized notes or mood labels.
fn compose(performed: &mut VocalMotif, motif: &VocalMotif, request: &VocalRequest, seed: u64) {
    let shout = shout_intensity(request);
    let laugh = request.style == VocalStyle::PlayInvite
        && finite(request.valence) > 0.55
        && finite(request.arousal) > 0.65;
    let unit = |salt| ((mix(seed ^ salt) >> 40) as u32) as f32 / 0x00ff_ffff as f32;
    let signed = |salt| unit(salt) * 2.0 - 1.0;
    let arousal = finite(request.arousal).clamp(0.0, 1.0);
    let fatigue = finite(request.fatigue).clamp(0.0, 1.0);
    let stress = finite(request.stress).clamp(0.0, 1.0);
    let valence = finite(request.valence).clamp(-1.0, 1.0);
    let confidence = finite(request.confidence).clamp(0.0, 1.0);
    let caution = (stress * 0.7 + (1.0 - confidence) * 0.3).clamp(0.0, 1.0);
    let contour = request.phenotype.phrase_contour;
    let curious = finite(contour.curiosity_question).clamp(0.0, 1.0);
    let joy = finite(contour.joy_rise).clamp(0.0, 1.0);
    let sadness = finite(contour.sadness_fall).clamp(0.0, 1.0);
    let calm = finite(contour.calm_level).clamp(0.0, 1.0);
    let strength = match request.style {
        VocalStyle::Purr => 0.35,
        VocalStyle::Frustrated => 0.55,
        VocalStyle::ContentMurmur | VocalStyle::TouchResponse => 0.72,
        _ => 1.0,
    };
    let duration_center = 1.0 + fatigue * 0.20 + caution * 0.05 - arousal * 0.10;
    let tempo = duration_center * (1.0 + signed(1) * 0.20 * strength);
    let rhythm_slope = signed(2) * 0.26 * strength;
    let accent_center = unit(3);
    let accent_width = 0.22 + unit(4) * 0.35;
    let gap_scale = (1.0 + fatigue * 0.22 + caution * 0.30 - arousal * 0.16)
        * (1.0 + signed(5) * 0.28 * strength);
    let gap_slope = signed(6) * 0.32;
    let register = signed(7) * 0.55 * strength;
    let slope = (signed(8) * 1.35 * strength + curious * 0.85 + valence * 0.40
        - caution * 0.50
        - fatigue * 0.35
        - sadness * 0.40)
        * (1.0 - calm * 0.20);
    let arch = (signed(9) * 1.20 * strength + arousal * 0.65 + joy * 0.40 - fatigue * 0.40)
        * (1.0 - calm * 0.25);
    let arc = |phase: f32| {
        (register + slope * (phase - 0.5) + arch * (4.0 * phase * (1.0 - phase) - 0.5))
            .clamp(-2.4, 2.4)
    };
    // Assemble a new 1–5-segment call, rather than replaying the motif's fixed
    // count/order. Fatigue biases toward fewer segments; arousal permits longer
    // articulatory sequences. The selected structure is fixed for this call.
    let span = 1.0 + 4.0 * (1.0 - fatigue * 0.5) * (0.65 + arousal * 0.35);
    let segment_count = match request.style {
        VocalStyle::PlayInvite if laugh => 3 + (unit(10) * 3.0) as usize % 3,
        VocalStyle::PlayInvite => 1 + usize::from(unit(10) > 0.65),
        VocalStyle::Purr => 1,
        VocalStyle::Frustrated if shout > 0.0 => 1,
        VocalStyle::Frustrated => 1 + usize::from(unit(10) > 0.55),
        _ => (1 + (unit(10) * span) as usize).clamp(1, 5),
    };
    let count = segment_count as f32;
    let source_count = motif.syllables.len();
    let source_offset = (unit(11) * source_count as f32) as usize % source_count;
    let stride = if unit(12) > 0.5 {
        source_count.saturating_sub(1).max(1)
    } else {
        1
    };
    let front_start = signed(13) * 0.38;
    let front_end = signed(14) * 0.38;
    let nasal_shape = signed(15) * 0.16;
    let constriction_shape = signed(16) * 0.16;
    let base_duration = motif
        .syllables
        .iter()
        .map(|s| s.duration_ms)
        .sum::<f32>()
        .clamp(300.0, 600.0);
    let duration_budget = match request.style {
        VocalStyle::PlayInvite if laugh => 360.0 + unit(17) * 240.0,
        VocalStyle::PlayInvite => 110.0 + unit(17) * 160.0,
        VocalStyle::Purr => (650.0 + unit(17) * 500.0) * (1.0 + fatigue * 0.1),
        VocalStyle::Frustrated if shout > 0.0 => 500.0 + unit(17) * 250.0,
        VocalStyle::Frustrated => (180.0 + unit(17) * 240.0) * (1.0 + fatigue * 0.1),
        _ => (base_duration * tempo * (0.9 + unit(17) * 0.35)).clamp(280.0, 900.0),
    };
    performed.syllables.clear();
    let mut duration_weight = 0.0;
    for index in 0..segment_count {
        let source = &motif.syllables[(source_offset + index * stride) % source_count];
        let mut syllable = source.clone();
        let phase = (index as f32 + 0.5) / count;
        let offset = ((phase - accent_center) / accent_width).clamp(-1.0, 1.0);
        let accent = (1.0 - offset * offset).max(0.0);
        syllable.duration_ms = (1.0 + rhythm_slope * (phase - 0.5)) * (0.78 + 0.44 * accent);
        duration_weight += syllable.duration_ms;
        syllable.gap_after_ms = if index + 1 == segment_count {
            0.0
        } else {
            ((18.0 + 95.0 * unit(18)) * gap_scale * (1.0 + gap_slope * (phase - 0.5)))
                .clamp(8.0, 180.0)
        };
        syllable.pitch_start *= (arc(index as f32 / count) / 12.0).exp2();
        syllable.pitch_peak *= (arc((index as f32 + 0.45) / count) / 12.0).exp2();
        syllable.pitch_end *= (arc((index as f32 + 1.0) / count) / 12.0).exp2();
        // State already controls tract/pressure in PreparedSyllable::prepare.
        // Accenting only de-emphasizes neighbors, never raises peak gain.
        syllable.amplitude *= 0.90 + 0.10 * accent;
        // Coherent articulation traverses the same speaker's tract, not a new
        // genome or unrelated random vowel per audio frame. Inter-note ramps
        // remain the existing DynamicTract/control-rate smoothing path.
        let articulatory_phase = phase * phase * (3.0 - 2.0 * phase);
        syllable.gesture.frontness = (syllable.gesture.frontness
            + front_start
            + (front_end - front_start) * articulatory_phase)
            .clamp(-1.0, 1.0);
        syllable.gesture.nasality =
            (syllable.gesture.nasality + nasal_shape * (0.5 + 0.5 * phase)).clamp(0.0, 1.0);
        syllable.gesture.constriction =
            (syllable.gesture.constriction + constriction_shape * (1.0 - accent)).clamp(0.0, 1.0);
        syllable.mouth_open =
            (syllable.mouth_open + (front_end - front_start) * 0.18 * phase).clamp(0.0, 1.0);
        // Open, non-nasal effort changes articulation rather than master gain.
        syllable.mouth_open += (0.98 - syllable.mouth_open) * shout;
        syllable.gesture.nasality *= 1.0 - shout * 0.85;
        syllable.gesture.constriction *= 1.0 - shout * 0.75;
        syllable.gesture.adduction += (0.85 - syllable.gesture.adduction) * shout;
        syllable.pitch_peak *= (shout * 2.0 / 12.0).exp2();
        if laugh {
            // Alternating voiced and breathy releases form one related bout,
            // not repeated identical notes or uncorrelated noise per sample.
            let exhaled = index % 2 == 1;
            syllable.gap_after_ms = if index + 1 == segment_count {
                0.0
            } else {
                28.0 + 30.0 * unit(19)
            };
            syllable.gesture.adduction *= if exhaled { 0.36 } else { 0.88 };
            syllable.gesture.open_quotient = if exhaled { 0.76 } else { 0.58 };
            syllable.noisiness = (syllable.noisiness + if exhaled { 0.22 } else { 0.04 }).min(0.42);
            syllable.amplitude *= if exhaled { 0.55 } else { 0.90 };
            syllable.mouth_open = 0.55 + 0.20 * accent;
            syllable.gesture.nasality *= 0.45;
        } else if request.style == VocalStyle::PlayInvite {
            syllable.gesture.closure_sharpness =
                (syllable.gesture.closure_sharpness + 0.18).min(1.0);
            syllable.mouth_open = syllable.mouth_open.max(0.48);
        }
        syllable.gesture.sanitize();
        performed.syllables.push(syllable);
    }
    for syllable in &mut performed.syllables {
        syllable.duration_ms =
            (duration_budget * syllable.duration_ms / duration_weight.max(0.1)).clamp(40.0, 1300.0);
    }
}

fn shout_intensity(request: &VocalRequest) -> f32 {
    if request.style != VocalStyle::Frustrated {
        return 0.0;
    }
    let drive = ((finite(request.arousal) - 0.72) / 0.28).clamp(0.0, 1.0)
        * ((-finite(request.valence) - 0.45) / 0.55).clamp(0.0, 1.0);
    drive * drive * (3.0 - 2.0 * drive)
}

fn finite(value: f32) -> f32 {
    if value.is_finite() { value } else { 0.0 }
}

fn mix(mut value: u64) -> u64 {
    value = value.wrapping_add(0x9e37_79b9_7f4a_7c15);
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn request(id: u64) -> VocalRequest {
        VocalRequest {
            motif_id: id,
            performance_seed: 17,
            gain: 0.24,
            pan: 0.0,
            pitch_scale: 1.0,
            tempo_scale: 1.0,
            stress: 0.0,
            purr: false,
            gesture: lifecore::VoiceGesture::WarmChuff,
            priority: 128,
            style: VocalStyle::SocialContact,
            valence: 0.3,
            arousal: 0.25,
            fatigue: 0.0,
            confidence: 0.8,
            attachment: 0.5,
            rhythm_intervals: [0.0; 8],
            phenotype: Default::default(),
        }
    }
    fn fingerprint(command: &VoiceCommand) -> Vec<u32> {
        command.syllables[..usize::from(command.syllable_count)]
            .iter()
            .flat_map(|s| {
                [
                    s.duration_ms.to_bits(),
                    s.gap_after_ms.to_bits(),
                    s.pitch_start.to_bits(),
                    s.pitch_peak.to_bits(),
                    s.pitch_end.to_bits(),
                ]
            })
            .collect()
    }

    #[test]
    fn phrase_variation_baseline_anchors_repeat_but_renditions_do_not() {
        let voice = lifecore::Genome::from_seed(42).voice;
        let motif = lifecore::generate_initial_motifs(&voice)[0].clone();
        let mut state = PhraseVariationState::default();
        let mut baseline_keys = std::collections::HashSet::new();
        let mut performed_keys = std::collections::HashSet::new();
        let mut coarse_keys = std::collections::HashSet::new();
        let started = std::time::Instant::now();
        for seed in 0..1000 {
            let mut request = request(motif.id);
            request.performance_seed = seed;
            let baseline = VoiceCommand::prepare(&voice, &motif, &request);
            baseline_keys.insert(fingerprint(&baseline));
            let previous = state.recent;
            let command = state.prepare(&voice, &motif, &request);
            let key = PhraseFingerprint::from_command(&command, &baseline);
            for old in previous.into_iter().flatten() {
                assert!(
                    key.distance(old) >= 1.0,
                    "near repeat at seed {seed}: {key:?} / {old:?}"
                );
            }
            performed_keys.insert(fingerprint(&command));
            coarse_keys.insert(key.0);
        }
        eprintln!(
            "phrase grammar 1000 utterances: {:?}; coarse fingerprints {}",
            started.elapsed(),
            coarse_keys.len()
        );
        assert_eq!(baseline_keys.len(), 1);
        assert_eq!(performed_keys.len(), 1000);
        assert!(coarse_keys.len() > 900);
        // Generous portable guard, not a promise of a specific user's latency.
        assert!(started.elapsed().as_secs_f32() < 5.0);
    }

    #[test]
    fn phrase_variation_preserves_identity_credit_and_bounded_energy() {
        let voice = lifecore::Genome::from_seed(42).voice;
        let motif = lifecore::generate_initial_motifs(&voice)[0].clone();
        let request = request(motif.id);
        let baseline = VoiceCommand::prepare(&voice, &motif, &request);
        let mut state = PhraseVariationState::default();
        let mut replay = state.clone();
        for _ in 0..1000 {
            let command = state.prepare(&voice, &motif, &request);
            assert_eq!(command, replay.prepare(&voice, &motif, &request));
            assert_eq!(command.anatomy, baseline.anatomy);
            assert_eq!(command.base_pitch_hz, baseline.base_pitch_hz);
            assert_eq!(command.motif_id, baseline.motif_id);
            assert_eq!(command.request_id, baseline.request_id);
            assert_eq!(command.gain, baseline.gain);
            let phonation_ms = command.syllables[..command.syllable_count as usize]
                .iter()
                .map(|s| s.duration_ms)
                .sum::<f32>();
            assert!((279.9..=900.1).contains(&phonation_ms));
            assert!((1..=5).contains(&command.syllable_count));
            assert!(command.total_frames(24_000) < 24_000 * 4);
            for s in &command.syllables[..command.syllable_count as usize] {
                let b = baseline.syllables[0];
                assert!((40.0..=900.0).contains(&s.duration_ms));
                assert!((0.0..=180.0).contains(&s.gap_after_ms));
                for ratio in [
                    s.pitch_start / b.pitch_start,
                    s.pitch_peak / b.pitch_peak,
                    s.pitch_end / b.pitch_end,
                ] {
                    assert!(ratio.is_finite() && (0.870..=1.149).contains(&ratio));
                }
                assert!(s.amplitude <= b.amplitude);
            }
        }
    }

    #[test]
    fn phrase_variation_preserves_exact_rhythm_and_protective_requests() {
        let voice = lifecore::Genome::from_seed(42).voice;
        let motif = lifecore::generate_initial_motifs(&voice)[0].clone();
        for style in [VocalStyle::RhythmMimic, VocalStyle::Startle] {
            let mut request = request(motif.id);
            request.style = style;
            let mut state = PhraseVariationState::default();
            assert_eq!(
                state.prepare(&voice, &motif, &request),
                VoiceCommand::prepare(&voice, &motif, &request)
            );
            assert_eq!(state.sequence, 0);
        }
        let mut request = request(motif.id);
        request.style = VocalStyle::RhythmMimic;
        request.rhythm_intervals[0] = 0.8;
        let mut state = PhraseVariationState::default();
        assert_eq!(
            state.prepare(&voice, &motif, &request),
            VoiceCommand::prepare(&voice, &motif, &request)
        );
    }

    #[test]
    fn actual_alarm_effort_opens_mouth_without_rewriting_protective_phrase() {
        let voice = lifecore::Genome::from_seed(42).voice;
        let motif = lifecore::generate_initial_motifs(&voice)
            .into_iter()
            .find(|m| m.family == lifecore::VocalFamily::StartleSqueak)
            .unwrap();
        let mut request = request(motif.id);
        request.style = VocalStyle::Startle;
        request.gesture = lifecore::VoiceGesture::ClippedPulse;
        request.arousal = 1.0;
        request.stress = 1.0;
        let command = PhraseVariationState::default().prepare(&voice, &motif, &request);
        assert_eq!(command, VoiceCommand::prepare(&voice, &motif, &request));
        assert_eq!(command.shout, 1.0);
        assert!(command.syllables[0].mouth_open >= 0.94);
        assert_eq!(
            command.syllables[0].duration_ms,
            motif.syllables[0].duration_ms
        );
        let render =
            crate::render_prepared_command(command, 24_000, 1, crate::OfflineSampleFormat::F32);
        let crate::OfflinePcm::F32(samples) = render.pcm else {
            panic!("float PCM expected")
        };
        assert!(samples.iter().all(|s| s.is_finite() && s.abs() <= 1.0));
        assert!(samples.iter().any(|s| s.abs() > 0.00001));
        request.stress = 0.4;
        let mild = VoiceCommand::prepare(&voice, &motif, &request);
        assert_eq!(mild.shout, 0.0);
        assert_eq!(mild.gain, command.gain);
    }

    #[test]
    fn playful_laughter_has_alternating_release_and_yip_is_brief() {
        let voice = lifecore::Genome::from_seed(42).voice;
        let motif = lifecore::generate_initial_motifs(&voice)[0].clone();
        let mut request = request(motif.id);
        request.style = VocalStyle::PlayInvite;
        request.arousal = 0.9;
        request.valence = 0.9;
        let laughing = PhraseVariationState::default().prepare(&voice, &motif, &request);
        assert!((3..=5).contains(&laughing.syllable_count));
        assert!(laughing.syllables[1].gesture.adduction < laughing.syllables[0].gesture.adduction);
        assert!(laughing.syllables[1].amplitude < laughing.syllables[0].amplitude);
        request.valence = 0.2;
        let yip = PhraseVariationState::default().prepare(&voice, &motif, &request);
        assert!((1..=2).contains(&yip.syllable_count));
        assert_eq!(laughing.gain, yip.gain);
        let mut waveforms = Vec::new();
        for command in [laughing, yip] {
            let render =
                crate::render_prepared_command(command, 24_000, 1, crate::OfflineSampleFormat::F32);
            let crate::OfflinePcm::F32(samples) = render.pcm else {
                panic!("float PCM expected")
            };
            assert!(samples.iter().all(|s| s.is_finite() && s.abs() <= 1.0));
            assert!(samples.iter().any(|s| s.abs() > 0.00001));
            waveforms.push(samples);
        }
        assert_ne!(waveforms[0], waveforms[1]);
    }

    #[test]
    fn angry_shout_is_causal_bounded_and_renders_without_gain_boost() {
        let voice = lifecore::Genome::from_seed(42).voice;
        let motif = lifecore::generate_initial_motifs(&voice)[0].clone();
        let mut request = request(motif.id);
        request.style = VocalStyle::Frustrated;
        request.arousal = 1.0;
        request.valence = -1.0;
        let baseline = VoiceCommand::prepare(&voice, &motif, &request);
        let command = PhraseVariationState::default().prepare(&voice, &motif, &request);
        assert_eq!(command.shout, 1.0);
        assert_eq!(command.syllable_count, 1);
        assert!((500.0..=750.0).contains(&command.syllables[0].duration_ms));
        assert!(command.syllables[0].mouth_open >= 0.97);
        assert_eq!(command.gain, baseline.gain);
        let rendered =
            crate::render_prepared_command(command, 24_000, 1, crate::OfflineSampleFormat::F32);
        let crate::OfflinePcm::F32(samples) = rendered.pcm else {
            panic!("float output expected")
        };
        assert!(samples.iter().all(|s| s.is_finite() && s.abs() <= 1.0));
        assert!(samples.iter().any(|s| s.abs() > 0.00001));
        request.valence = 0.5;
        assert_eq!(shout_intensity(&request), 0.0);
        request.valence = -1.0;
        request.arousal = 0.5;
        assert_eq!(shout_intensity(&request), 0.0);
        request.arousal = 1.0;
        request.style = VocalStyle::Startle;
        assert_eq!(shout_intensity(&request), 0.0);
    }

    #[test]
    fn phrase_variation_purr_is_sustained_and_frustration_is_short() {
        let voice = lifecore::Genome::from_seed(42).voice;
        let mut motif = lifecore::generate_initial_motifs(&voice)[0].clone();
        motif.syllables = vec![motif.syllables[0].clone(); 5];
        for style in [VocalStyle::Purr, VocalStyle::Frustrated] {
            let mut request = request(motif.id);
            request.style = style;
            let mut state = PhraseVariationState::default();
            let mut counts = std::collections::BTreeSet::new();
            for seed in 0..1000 {
                request.performance_seed = seed;
                let command = state.prepare(&voice, &motif, &request);
                counts.insert(command.syllable_count);
                assert_eq!(command.motif_id, motif.id);
                let mut performed = motif.clone();
                compose(&mut performed, &motif, &request, seed);
                let duration: f32 = performed.syllables.iter().map(|s| s.duration_ms).sum();
                let bounds = if style == VocalStyle::Purr {
                    649.9..=1265.1
                } else {
                    179.9..=462.1
                };
                assert!(bounds.contains(&duration), "{style:?}: {duration}");
            }
            let expected = if style == VocalStyle::Purr {
                vec![1]
            } else {
                vec![1, 2]
            };
            assert_eq!(counts.into_iter().collect::<Vec<_>>(), expected);
        }
    }

    #[test]
    fn phrase_variation_changes_stored_five_note_structure_and_source_order() {
        let voice = lifecore::Genome::from_seed(42).voice;
        let mut motif = lifecore::generate_initial_motifs(&voice)[0].clone();
        motif.syllables = (0..5)
            .map(|index| {
                let mut source = motif.syllables[0].clone();
                source.gesture.tongue_height = index as f32 * 0.2;
                source
            })
            .collect();
        let mut request = request(motif.id);
        request.arousal = 0.9;
        let mut state = PhraseVariationState::default();
        let mut counts = std::collections::BTreeSet::new();
        let mut orders = std::collections::HashSet::new();
        for seed in 0..1000 {
            request.performance_seed = seed;
            let command = state.prepare(&voice, &motif, &request);
            counts.insert(command.syllable_count);
            let order: Vec<_> = command.syllables[..command.syllable_count as usize]
                .iter()
                .map(|s| (s.gesture.tongue_height * 10.0).round() as i32)
                .collect();
            orders.insert(order);
        }
        assert_eq!(counts.into_iter().collect::<Vec<_>>(), vec![1, 2, 3, 4, 5]);
        assert!(orders.len() > 20);
        let mut ordinary = request.clone();
        ordinary.rhythm_intervals = [1.0, 0.5, 1.0, 0.5, 0.0, 0.0, 0.0, 0.0];
        let with_history = VoiceCommand::prepare(&voice, &motif, &ordinary);
        ordinary.rhythm_intervals = [0.0; 8];
        assert_eq!(
            with_history,
            VoiceCommand::prepare(&voice, &motif, &ordinary)
        );
    }

    #[test]
    fn phrase_variation_mixed_moods_change_delivery_without_changing_identity() {
        let voice = lifecore::Genome::from_seed(42).voice;
        let motif = lifecore::generate_initial_motifs(&voice)[0].clone();
        let mut excited = request(motif.id);
        excited.valence = 0.8;
        excited.arousal = 0.85;
        excited.phenotype.phrase_contour.joy_rise = 0.8;
        let mut tired = excited.clone();
        tired.fatigue = 0.9;
        let a = PhraseVariationState::default().prepare(&voice, &motif, &excited);
        let b = PhraseVariationState::default().prepare(&voice, &motif, &tired);
        let duration = |c: &VoiceCommand| {
            c.syllables[..c.syllable_count as usize]
                .iter()
                .map(|s| s.duration_ms)
                .sum::<f32>()
        };
        assert!(duration(&b) > duration(&a) * 1.10);
        assert_eq!(a.anatomy.tract_length, b.anatomy.tract_length);
        assert_eq!(a.base_pitch_hz, b.base_pitch_hz);
        let mut calm = request(motif.id);
        calm.phenotype.phrase_contour.curiosity_question = 0.8;
        let mut wary = calm.clone();
        wary.stress = 0.8;
        wary.confidence = 0.2;
        let c = PhraseVariationState::default().prepare(&voice, &motif, &calm);
        let w = PhraseVariationState::default().prepare(&voice, &motif, &wary);
        assert!(w.syllables[0].pitch_end < c.syllables[0].pitch_end);
        assert!(w.syllables[0].gap_after_ms >= c.syllables[0].gap_after_ms);
        assert_ne!(fingerprint(&c), fingerprint(&w));
    }

    #[test]
    fn phrase_variation_mood_interpolation_is_continuous_for_fixed_utterance() {
        let voice = lifecore::Genome::from_seed(42).voice;
        let motif = lifecore::generate_initial_motifs(&voice)[0].clone();
        let mut previous: Option<VoiceCommand> = None;
        for step in 0..101 {
            let t = step as f32 / 100.0;
            let mut request = request(motif.id);
            request.fatigue = t;
            request.stress = t * 0.7;
            request.valence = 0.8 - t * 0.6;
            let command = PhraseVariationState::default().prepare(&voice, &motif, &request);
            if let Some(old) = previous {
                assert!(
                    (command.syllables[0].duration_ms / old.syllables[0].duration_ms - 1.0).abs()
                        < 0.01
                );
                assert!(
                    (command.syllables[0].pitch_end / old.syllables[0].pitch_end - 1.0).abs()
                        < 0.01
                );
            }
            previous = Some(command);
        }
    }
}
