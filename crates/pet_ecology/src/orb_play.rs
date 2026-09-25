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

/// Current physiology, not another action policy or a random animation timer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OrbPlayState {
    pub affect: lifecore::AffectState,
    pub felt: lifecore::FeltStateV1,
    pub patience: f32,
    pub persistence: f32,
    pub playfulness: f32,
}
impl Default for OrbPlayState {
    fn default() -> Self { Self { affect: Default::default(), felt: Default::default(), patience:0.5,persistence:0.5,playfulness:0.5 } }
}
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize)]
pub struct OrbMotivation {
    pub grip: f32, pub bat: f32, pub home: f32, pub explore: f32,
    pub travel: f32, pub effort_rate: f32, pub preparation: f32,
}
impl OrbPlayState {
    pub fn motivation(self, metabolism:&crate::MetabolicState, play:f32, fatigue:f32,
        affinity:f32, relative_speed:f32) -> OrbMotivation {
        let vigor=(metabolism.reserve*(1.0-fatigue)* (1.0-self.affect.stress*0.7)).clamp(0.0,1.0);
        let fullness=metabolism.satiation*metabolism.satiation;
        let engage=play*0.7+self.playfulness*0.18+self.felt.play_readiness*0.35;
        let security=self.affect.confidence*0.3+self.affect.valence.max(0.0)*0.15+affinity*0.5;
        let grip=(engage+security+self.patience*0.13)*vigor*(1.0-fullness*0.3);
        let bat=relative_speed*2.2+self.affect.arousal*0.24+(1.0-self.patience)*0.12+fatigue*0.22;
        let home=affinity*(0.2+self.patience*0.25)+fatigue*0.4+fullness*0.2+self.affect.stress*0.3;
        let explore=engage*vigor+self.felt.exploration_readiness*0.25;
        OrbMotivation {grip,bat,home,explore,
            travel:0.06+(self.persistence*0.12+explore*0.22)*(1.0-fullness*0.35),
            effort_rate:0.035+(1.0-vigor)*0.20+fullness*0.06+self.felt.physical_load*0.12+(metabolism.relative_mass()-1.0).max(0.0)*0.08,
            preparation:0.16+self.patience*0.35+(1.0-vigor)*0.25 }
    }
}

#[cfg(test)]
mod motivation_tests {
    use super::*;
    #[test]
    fn appetite_mood_fatigue_and_character_change_grip_and_effort() {
        let state=OrbPlayState::default();
        let metabolism=crate::MetabolicState::default();
        let calm=state.motivation(&metabolism,0.25,0.1,1.0,0.01);
        let mut tired=state;tired.affect.stress=0.8;
        let tired=tired.motivation(&metabolism,0.25,0.8,1.0,0.01);
        assert!(calm.grip>calm.bat);
        assert!(tired.grip<tired.bat && tired.effort_rate>calm.effort_rate);
        let mut full=metabolism.clone();full.satiation=1.0;full.body_condition=0.8;
        let full=state.motivation(&full,0.25,0.1,1.0,0.01);
        assert!(full.grip<calm.grip && full.home>calm.home && full.effort_rate>calm.effort_rate);
        let mut patient=state;patient.patience=0.95;patient.persistence=0.95;
        let patient=patient.motivation(&metabolism,0.25,0.1,1.0,0.01);
        assert!(patient.travel>calm.travel && patient.preparation>calm.preparation);
    }
}
