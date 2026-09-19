//! Bounded real-device smoke probe for Pet 2 local hearing.
//!
//! Prints only status, peak RMS, and event counts. It stores no samples or model.

use std::{
    thread,
    time::{Duration, Instant},
};

use desktop_host::{
    AudioInputConfig, AudioInputState, AudioPercept, CueModelV1, LocalAudioInput,
    OutputReferenceFrame,
};

fn main() {
    let seconds = std::env::args()
        .nth(1)
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(10)
        .clamp(1, 60);
    let mut input = LocalAudioInput::new(AudioInputConfig::default(), CueModelV1::new())
        .expect("empty hearing model must be valid");
    if let Err(error) = input.start() {
        eprintln!("microphone probe could not start: {error}");
        std::process::exit(2);
    }

    let deadline = Instant::now() + Duration::from_secs(seconds);
    let mut reached_listening = false;
    let mut max_rms = 0.0f32;
    let mut onsets = 0u64;
    let mut segments = 0u64;
    let mut accepted = 0u64;
    let mut rejected = 0u64;
    while Instant::now() < deadline {
        let _ = input.submit_output_reference(OutputReferenceFrame::default());
        let status = input.status();
        reached_listening |= status.state == AudioInputState::Listening;
        max_rms = max_rms.max(status.rms);
        for percept in input.drain_percepts() {
            match percept {
                AudioPercept::Onset { .. } => onsets += 1,
                AudioPercept::Segment { .. } => segments += 1,
                AudioPercept::CueAccepted { .. } => accepted += 1,
                AudioPercept::CueRejected { .. } => rejected += 1,
                _ => {}
            }
        }
        thread::sleep(Duration::from_millis(25));
    }
    let status = input.status();
    input.stop();
    println!(
        "{}",
        serde_json::json!({
            "seconds": seconds,
            "reached_listening": reached_listening,
            "final_status": status,
            "max_rms": max_rms,
            "onsets": onsets,
            "segments": segments,
            "cue_accepted": accepted,
            "cue_rejected": rejected,
        })
    );
    if !reached_listening {
        std::process::exit(3);
    }
}
