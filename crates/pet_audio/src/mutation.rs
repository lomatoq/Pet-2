pub use lifecore::mutate_motif;

#[cfg(test)]
mod tests {
    use lifecore::{Genome, VocalRequest, generate_initial_motifs};

    use crate::{
        OfflinePcm, OfflineSampleFormat, VoiceSynthesisStyle, mutate_motif, render_motif,
        render_motif_style,
    };

    fn request(motif_id: u64) -> VocalRequest {
        VocalRequest {
            motif_id,
            performance_seed: 0x00A1_1D10,
            gain: 0.25,
            pan: 0.0,
            pitch_scale: 1.0,
            tempo_scale: 1.0,
            stress: 0.0,
            purr: false,
            gesture: lifecore::VoiceGesture::WarmChuff,
            priority: 128,
            rhythm_intervals: [0.0; 8],
        }
    }

    #[test]
    fn offline_pcm_is_finite_bounded_and_deterministic() {
        let genome = Genome::from_seed(42);
        let motif = &generate_initial_motifs(&genome.voice)[0];
        let first = render_motif(
            &genome.voice,
            motif,
            &request(motif.id),
            44_100,
            2,
            OfflineSampleFormat::F32,
        );
        let second = render_motif(
            &genome.voice,
            motif,
            &request(motif.id),
            44_100,
            2,
            OfflineSampleFormat::F32,
        );
        assert_eq!(first, second);
        let OfflinePcm::F32(samples) = first else {
            panic!("expected f32 samples");
        };
        assert!(samples.iter().all(|sample| sample.is_finite()));
        assert!(samples.iter().all(|sample| sample.abs() <= 0.86));
        let peak = samples
            .iter()
            .map(|sample| sample.abs())
            .fold(0.0_f32, f32::max);
        let rms = (samples.iter().map(|sample| sample * sample).sum::<f32>()
            / samples.len().max(1) as f32)
            .sqrt();
        assert!((0.15..=0.75).contains(&peak), "peak={peak}");
        assert!(rms >= 0.025, "rms={rms}");
    }

    #[test]
    fn output_matrix_preserves_duration_and_safe_channels() {
        let genome = Genome::from_seed(7);
        let motif = &generate_initial_motifs(&genome.voice)[0];
        for sample_rate in [44_100, 48_000] {
            for channels in [1, 2] {
                let expected =
                    crate::VoiceCommand::prepare(&genome.voice, motif, &request(motif.id))
                        .total_frames(sample_rate)
                        * channels as usize;
                for format in [
                    OfflineSampleFormat::F32,
                    OfflineSampleFormat::I16,
                    OfflineSampleFormat::U16,
                ] {
                    assert_eq!(
                        render_motif(
                            &genome.voice,
                            motif,
                            &request(motif.id),
                            sample_rate,
                            channels,
                            format,
                        )
                        .sample_count(),
                        expected
                    );
                }
            }
        }
    }

    #[test]
    fn bounded_mutation_is_related_but_not_identical() {
        let genome = Genome::from_seed(9);
        let parent = &generate_initial_motifs(&genome.voice)[0];
        let child = mutate_motif(parent, 1234);
        assert_eq!(child.parent_id, Some(parent.id));
        assert_ne!(child, *parent);
        assert!((1..=6).contains(&child.syllables.len()));
        for (before, after) in parent.syllables.iter().zip(&child.syllables) {
            assert!((after.pitch_peak / before.pitch_peak - 1.0).abs() <= 0.051);
            assert!((after.duration_ms / before.duration_ms - 1.0).abs() <= 0.101);
        }
    }

    #[test]
    fn different_voice_genomes_produce_distinct_pcm() {
        let first = Genome::from_seed(11);
        let second = Genome::from_seed(12);
        let motif = &generate_initial_motifs(&first.voice)[0];
        let first_pcm = render_motif(
            &first.voice,
            motif,
            &request(motif.id),
            48_000,
            1,
            OfflineSampleFormat::I16,
        );
        let second_pcm = render_motif(
            &second.voice,
            motif,
            &request(motif.id),
            48_000,
            1,
            OfflineSampleFormat::I16,
        );
        assert_ne!(first_pcm, second_pcm);
    }

    #[test]
    fn performance_seed_changes_rendition_without_changing_phrase_length() {
        let genome = Genome::from_seed(13);
        let motif = &generate_initial_motifs(&genome.voice)[0];
        let first_request = request(motif.id);
        let mut second_request = first_request.clone();
        second_request.performance_seed ^= 0xDEAD_BEEF;
        let first = render_motif(
            &genome.voice,
            motif,
            &first_request,
            48_000,
            1,
            OfflineSampleFormat::F32,
        );
        let second = render_motif(
            &genome.voice,
            motif,
            &second_request,
            48_000,
            1,
            OfflineSampleFormat::F32,
        );
        assert_eq!(first.sample_count(), second.sample_count());
        assert_ne!(first, second);
    }

    #[test]
    fn gain_and_tempo_create_clear_bounded_performance_differences() {
        let genome = Genome::from_seed(14);
        let motif = &generate_initial_motifs(&genome.voice)[0];
        let mut quiet = request(motif.id);
        quiet.gain = 0.08;
        let mut loud = quiet.clone();
        loud.gain = 0.30;
        let OfflinePcm::F32(quiet_pcm) = render_motif(
            &genome.voice,
            motif,
            &quiet,
            48_000,
            1,
            OfflineSampleFormat::F32,
        ) else {
            unreachable!()
        };
        let OfflinePcm::F32(loud_pcm) = render_motif(
            &genome.voice,
            motif,
            &loud,
            48_000,
            1,
            OfflineSampleFormat::F32,
        ) else {
            unreachable!()
        };
        let rms = |samples: &[f32]| {
            (samples.iter().map(|sample| sample * sample).sum::<f32>()
                / samples.len().max(1) as f32)
                .sqrt()
        };
        assert!(rms(&loud_pcm) / rms(&quiet_pcm).max(0.000_001) >= 2.0);

        let mut lingering = request(motif.id);
        lingering.tempo_scale = 0.65;
        let mut brisk = lingering.clone();
        brisk.tempo_scale = 1.40;
        let lingering_frames =
            crate::VoiceCommand::prepare(&genome.voice, motif, &lingering).total_frames(48_000);
        let brisk_frames =
            crate::VoiceCommand::prepare(&genome.voice, motif, &brisk).total_frames(48_000);
        let tempo_ratio = lingering_frames as f32 / brisk_frames as f32;
        assert!(
            (1.10..=1.45).contains(&tempo_ratio),
            "bounded mammalian tempo ratio={tempo_ratio}"
        );
    }

    #[test]
    fn organic_chain_is_declicked_dc_safe_and_not_high_frequency_noise() {
        let genome = Genome::from_seed(15);
        let motif = &generate_initial_motifs(&genome.voice)[0];
        let OfflinePcm::F32(samples) = render_motif(
            &genome.voice,
            motif,
            &request(motif.id),
            48_000,
            1,
            OfflineSampleFormat::F32,
        ) else {
            unreachable!()
        };
        let mean = samples.iter().sum::<f32>() / samples.len().max(1) as f32;
        let signal_energy = samples.iter().map(|sample| sample * sample).sum::<f32>();
        let mut difference_energy = 0.0;
        let mut maximum_step = 0.0_f32;
        for pair in samples.windows(2) {
            let step = pair[1] - pair[0];
            difference_energy += step * step;
            maximum_step = maximum_step.max(step.abs());
        }
        assert!(mean.abs() < 0.003, "dc mean={mean}");
        assert!(maximum_step < 0.10, "maximum sample step={maximum_step}");
        assert!(
            difference_energy / signal_energy.max(0.000_001) < 0.32,
            "excess high-frequency energy"
        );
        assert!(samples.first().is_some_and(|sample| sample.abs() < 0.000_1));
        assert!(samples.last().is_some_and(|sample| sample.abs() < 0.005));
    }

    #[test]
    fn roughness_and_mouth_resonance_are_audible_genome_dimensions() {
        let genome = Genome::from_seed(16);
        let motif = &generate_initial_motifs(&genome.voice)[0];
        let mut smooth = genome.voice.clone();
        smooth.roughness = 0.0;
        smooth.mouth_resonance = 0.0;
        let mut textured = smooth.clone();
        textured.roughness = 0.8;
        textured.mouth_resonance = 0.9;
        let smooth_pcm = render_motif(
            &smooth,
            motif,
            &request(motif.id),
            48_000,
            1,
            OfflineSampleFormat::I16,
        );
        let textured_pcm = render_motif(
            &textured,
            motif,
            &request(motif.id),
            48_000,
            1,
            OfflineSampleFormat::I16,
        );
        assert_ne!(smooth_pcm, textured_pcm);
    }

    #[test]
    fn mammalian_voice_gestures_are_pcm_distinct_without_ultrahigh_f0() {
        let genome = Genome::from_seed(17);
        let motif = &generate_initial_motifs(&genome.voice)[0];
        let gestures = [
            lifecore::VoiceGesture::PurrHum,
            lifecore::VoiceGesture::WarmChuff,
            lifecore::VoiceGesture::MewWhine,
            lifecore::VoiceGesture::LowRumble,
            lifecore::VoiceGesture::ClippedPulse,
            lifecore::VoiceGesture::ReliefExhale,
        ];
        let rendered = gestures
            .into_iter()
            .map(|gesture| {
                let mut request = request(motif.id);
                request.gesture = gesture;
                let command = crate::VoiceCommand::prepare_style(
                    &genome.voice,
                    motif,
                    &request,
                    VoiceSynthesisStyle::LivingMammalian,
                );
                assert!((65.0..=365.0).contains(&command.base_pitch_hz));
                render_motif_style(
                    &genome.voice,
                    motif,
                    &request,
                    48_000,
                    1,
                    OfflineSampleFormat::I16,
                    VoiceSynthesisStyle::LivingMammalian,
                )
            })
            .collect::<Vec<_>>();
        for left in 0..rendered.len() {
            for right in left + 1..rendered.len() {
                assert_ne!(rendered[left], rendered[right]);
            }
        }
    }
}
