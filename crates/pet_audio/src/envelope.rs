#[must_use]
pub fn amplitude_envelope(
    frame: usize,
    total_frames: usize,
    attack_frames: usize,
    release_frames: usize,
) -> f32 {
    if total_frames == 0 {
        return 0.0;
    }
    let attack = if attack_frames == 0 {
        1.0
    } else {
        frame as f32 / attack_frames as f32
    }
    .clamp(0.0, 1.0);
    let remaining = total_frames.saturating_sub(frame);
    let release = if release_frames == 0 {
        1.0
    } else {
        remaining as f32 / release_frames as f32
    }
    .clamp(0.0, 1.0);
    smoothstep(attack) * smoothstep(release)
}

fn smoothstep(value: f32) -> f32 {
    value * value * (3.0 - 2.0 * value)
}
