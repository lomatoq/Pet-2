use lifecore::*;
use std::time::Instant;
fn random(state: &mut u64) -> f32 {
    *state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
    (*state >> 40) as f32 / (1_u64 << 24) as f32
}
fn main() {
    let mut accuracies = Vec::new();
    let mut timings = Vec::new();
    let mut size = 0;
    for seed in 1..=10 {
        let mut rng = seed;
        let mut learner = AdaptiveLearning::default();
        for step in 1..=2400 {
            let gentle = random(&mut rng) > 0.5;
            let x = [1.0, 0.0, 0.2, 0.1, 0.5, 0.6, f32::from(gentle), 0.9];
            let started = Instant::now();
            let selected = learner.responses.choose(x, step as f64, random(&mut rng));
            learner.responses.propose(step, x, selected, step as f64);
            learner.responses.acknowledge(step);
            // Independent two-context scenario: user's desired response is determined before choice.
            let wanted = if gentle { 1 } else { 2 };
            learner.responses.outcome(
                step,
                Some(if selected == wanted { 1.0 } else { -0.5 }),
                1.0,
                step as f64,
            );
            let mut frame = BodyFeedbackV2 {
                frame_id: step * 2,
                ..Default::default()
            };
            let command =
                glam::Vec2::new(random(&mut rng) * 0.2 - 0.1, random(&mut rng) * 0.2 - 0.1);
            learner.body.begin(frame, command, step as f64 * 0.05);
            frame.frame_id += 1;
            frame.motion.velocity = command * 0.6;
            learner.body.complete(frame, step as f64 * 0.05 + 0.05);
            learner.body.tick(0.05);
            timings.push(started.elapsed().as_secs_f64() * 1e6);
        }
        let mut hits = 0;
        for i in 0..1000 {
            let gentle = i % 2 == 0;
            let selected = learner.responses.choose(
                [1.0, 0.0, 0.2, 0.1, 0.55, 0.6, f32::from(gentle), 0.9],
                100000.0,
                random(&mut rng),
            );
            hits += usize::from(selected == if gentle { 1 } else { 2 });
        }
        accuracies.push(hits as f64 / 1000.0);
        let before = learner.responses.observations;
        let start = Instant::now();
        learner.body.rehearse();
        let replay_us = start.elapsed().as_secs_f64() * 1e6;
        assert_eq!(before, learner.responses.observations);
        size = size.max(serde_json::to_vec(&learner).unwrap().len());
        if seed == 10 {
            eprintln!("replay_free_regime_8_updates_us={replay_us:.3}");
        }
    }
    timings.sort_by(f64::total_cmp);
    println!(
        "{}",
        serde_json::json!({"seeds":10,"training_responses_per_seed":2400,"heldout_trials_per_seed":1000,"context_accuracy":accuracies,"fixed_ack_baseline":0.5,"added_step_p95_us":timings[timings.len()*95/100],"added_step_p99_us":timings[timings.len()*99/100],"serialized_learning_max_bytes":size})
    );
    assert!(accuracies.iter().all(|v| *v > 0.75));
}
