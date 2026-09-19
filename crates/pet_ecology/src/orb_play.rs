use glam::Vec2;

/// Six approach geometries and four contact tactics. Identity stays fixed for
/// the whole bout; fatigue, actual contact and available space stay authoritative.
pub const ORB_PLAY_NAMES: [&str; 24] = [
    "stalk_pounce",
    "stalk_side_bat",
    "stalk_lob",
    "stalk_roll",
    "left_hook_pounce",
    "left_hook_bat",
    "left_hook_lob",
    "left_hook_roll",
    "right_hook_pounce",
    "right_hook_bat",
    "right_hook_lob",
    "right_hook_roll",
    "feint_pounce",
    "feint_bat",
    "feint_lob",
    "feint_roll",
    "intercept_pounce",
    "intercept_bat",
    "intercept_lob",
    "intercept_roll",
    "retreat_pounce",
    "retreat_bat",
    "retreat_lob",
    "retreat_roll",
];

#[derive(Clone, Copy, Debug)]
pub struct OrbPlayPlan {
    pub target: Vec2,
    pub speed: f32,
    pub impulse: Vec2,
    pub pause_seconds: f32,
}

pub fn orb_play_name(episode_id: u64) -> &'static str {
    ORB_PLAY_NAMES[(episode_id % 24) as usize]
}

#[allow(clippy::too_many_arguments)]
pub fn orb_play_plan(
    episode_id: u64,
    attempt: u8,
    elapsed: f32,
    pet: Vec2,
    orb: Vec2,
    velocity: Vec2,
    aspect: f32,
    drive: f32,
    fatigue: f32,
) -> OrbPlayPlan {
    let variant = (episode_id % 24) as usize;
    let approach = variant / 4;
    let tactic = variant % 4;
    let scale = Vec2::new(aspect.clamp(0.25, 8.0), 1.0);
    let offset = (orb - pet) * scale;
    let axis = offset.normalize_or(Vec2::X);
    let side = Vec2::new(-axis.y, axis.x);
    let distance = offset.length();
    let fatigue = fatigue.clamp(0.0, 1.0);
    let drive = drive.clamp(0.0, 1.0);
    let pause_seconds = 0.70 + fatigue * 1.6 + tactic as f32 * 0.08;
    let preparation = 0.12 + approach as f32 * 0.035;
    let observing = if attempt == 0 {
        elapsed < preparation
    } else {
        elapsed < pause_seconds * 0.55
    };
    // Curves collapse on arrival so an authored arc never prevents contact.
    let radius = (distance - 0.035).clamp(0.0, 0.065);
    let approach_offset = match approach {
        1 => side * radius,
        2 => -side * radius,
        3 => side * radius * if elapsed < 0.42 { 1.0 } else { -0.45 },
        4 => velocity.clamp_length_max(0.6) * 0.20,
        5 if elapsed < 0.42 => -axis * radius,
        _ => Vec2::ZERO,
    };
    let sign = if attempt.is_multiple_of(2) { 1.0 } else { -1.0 };
    let direction = match tactic {
        0 => (axis - Vec2::Y * 0.55).normalize_or(-Vec2::Y),
        1 => (side * sign - Vec2::Y * 0.18).normalize_or(Vec2::X),
        2 => (axis * 0.28 - Vec2::Y).normalize_or(-Vec2::Y),
        _ => Vec2::new(if orb.x < 0.5 { 1.0 } else { -1.0 }, 0.0),
    };
    let strength =
        [0.24, 0.18, 0.28, 0.11][tactic] * (0.75 + drive * 0.25) * (1.0 - fatigue * 0.45);
    let burst = (elapsed - preparation).clamp(0.0, 0.10) / 0.10;
    let speed = if observing {
        0.0
    } else {
        (0.30 + 0.52 * burst) * (1.0 - fatigue * 0.48) * if distance < 0.045 { 0.45 } else { 1.0 }
    };
    OrbPlayPlan {
        target: if observing {
            pet
        } else {
            (orb + approach_offset / scale).clamp(Vec2::splat(0.025), Vec2::splat(0.975))
        },
        speed,
        impulse: direction * strength,
        pause_seconds,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repertoire_has_24_distinct_executed_trajectory_contact_pairs() {
        let mut signatures = std::collections::HashSet::new();
        for id in 0..24 {
            let mut signature = Vec::new();
            for elapsed in [0.15, 0.35, 0.65] {
                let p = orb_play_plan(
                    id,
                    0,
                    elapsed,
                    Vec2::new(0.3, 0.4),
                    Vec2::new(0.6, 0.6),
                    Vec2::new(0.2, -0.1),
                    1.8,
                    0.8,
                    0.0,
                );
                signature.extend(
                    [p.target.x, p.target.y, p.speed, p.impulse.x, p.impulse.y]
                        .map(|x| (x * 10000.0).round() as i32),
                );
            }
            assert!(
                signatures.insert(signature),
                "duplicate {}",
                orb_play_name(id)
            );
        }
    }

    #[test]
    fn bouts_pause_and_fatigue_reduces_burst_and_contact_energy() {
        for id in 0..24 {
            let plan = |attempt, elapsed, fatigue| {
                orb_play_plan(
                    id,
                    attempt,
                    elapsed,
                    Vec2::splat(0.2),
                    Vec2::splat(0.7),
                    Vec2::ZERO,
                    1.0,
                    0.9,
                    fatigue,
                )
            };
            assert_eq!(plan(0, 0.0, 0.0).speed, 0.0);
            assert_eq!(plan(1, 0.1, 0.0).speed, 0.0);
            let fresh = plan(0, 0.7, 0.0);
            let tired = plan(0, 0.7, 0.9);
            assert!(fresh.speed > 0.75 && tired.speed < fresh.speed);
            assert!(tired.impulse.length() < fresh.impulse.length());
            assert!(tired.pause_seconds > fresh.pause_seconds);
        }
    }
}
