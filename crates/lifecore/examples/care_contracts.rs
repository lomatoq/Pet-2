//! Reproducible production-contract probe, not native-body/user validation.
use lifecore::*;
use serde_json::json;

fn main() {
    for seed in [7, 63, 164] {
        for (name, quality) in [("sleep_nomination_only", 0.0), ("partial_rest", 0.5), ("quiet_supported_sleep", 1.0)] {
            let mut core = LifeCore::new(Genome::from_seed(seed), seed);
            core.state.drives.sleep = 0.99;
            core.state.drives.comfort = 0.99;
            let sensors = SensorFrame {
                time_of_day_01: (core.state.genome.temperament.circadian_phase + 0.5).fract(),
                ..Default::default()
            };
            for step in 0..=1200 {
                core.integrate_felt_state(InteroceptionSnapshot {
                    felt: FeltStateV1 { comfort: 0.8, body_integrity: 1.0, agency_match: 1.0, motor_efficacy: 0.9, ..Default::default() },
                    ..Default::default()
                }, EpisodeContextV1 { sleeping_or_deep_rest: quality, rest_quality: quality, ..Default::default() }, 0.1);
                core.tick(&sensors, &BodyFeedback::default(), 0.1);
                if step % 300 == 0 {
                    println!("{}", json!({"case": name, "seed": seed, "seconds": step as f32 * 0.1,
                        "assumed_physical_quality": quality, "sleep": core.state.drives.sleep,
                        "comfort": core.state.drives.comfort, "valence": core.state.affect.valence}));
                }
            }
        }
        for (name, present, pressure, neck, intended, actual) in [
            ("no_touch", false, 0.0, 0.0, 0.0, 0.0),
            ("gentle_stroke", true, 0.18, 0.0, 0.0, 0.0),
            ("gentle_transfer", true, 0.18, 0.0, 0.0, 0.6),
            ("loaded_opposition", true, 0.85, 0.8, -0.5, 0.6),
        ] {
            let genome = Genome::from_seed(seed);
            let body = BodyFeedbackV2 {
                contact: BodyContactFeedbackV2 { contact_count: u16::from(present), pressure, duration: 2.0, ..Default::default() },
                shape: BodyShapeFeedbackV2 { neck_tension: neck, ..Default::default() },
                efference_copy: EfferenceCopyV2 {
                    intended_velocity: glam::Vec2::new(intended, 0.0),
                    actual_velocity: glam::Vec2::new(actual, 0.0),
                    ..Default::default()
                },
                ..Default::default()
            };
            let source = EmbodimentSourceFrame {
                frame_id: 1, affect: AffectState::default(), drives: Drives::initial(&genome.temperament),
                temperament: genome.temperament, voice_seed: genome.voice.voice_seed,
                vita: VitaSomaticFrame::default(), morph: MorphNervousSystemFrame::default(), body,
                voice_feedback: VoiceFeedbackV1::default(), gesture: GestureFrameV1::default(),
                episode: EpisodeContextV1::default(), perception: PerceptionSelectionV1::default(),
                soft_touch_pressure_max: 0.42,
            };
            let mut director = BodyInteroceptionDirector::default();
            let mut snapshot = InteroceptionSnapshot::default();
            for _ in 0..100 { snapshot = director.tick(&source, 0.05); }
            println!("{}", json!({"case": name, "seed": seed, "felt": snapshot.felt,
                "emotions": snapshot.emotions, "scope": "production interoception; measured inputs are fixtures"}));
        }
    }
}
