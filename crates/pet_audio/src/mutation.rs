pub use lifecore::mutate_motif;

#[cfg(test)]
mod tests {
    use lifecore::{Genome, VocalRequest, generate_initial_motifs};

    use crate::{OfflinePcm, OfflineSampleFormat, mutate_motif, render_motif};

    fn request(motif_id: u64) -> VocalRequest {
        VocalRequest {
            motif_id,
            gain: 0.25,
            pan: 0.0,
            pitch_scale: 1.0,
            tempo_scale: 1.0,
            stress: 0.0,
            purr: false,
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
}
