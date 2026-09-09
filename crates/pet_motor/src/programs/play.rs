use lifecore::BehaviorGoalFrame;

use crate::{
    ActivePerformance, BehaviorContextFrame, MotorReadabilityTuning, SomaticActuationPacket,
};

#[allow(clippy::too_many_arguments)]
pub(crate) fn apply(
    _packet: &mut SomaticActuationPacket,
    _active: &ActivePerformance,
    _phase: &str,
    _progress: f32,
    _phase_started: bool,
    _goal: &BehaviorGoalFrame,
    _context: &BehaviorContextFrame,
    _tuning: MotorReadabilityTuning,
) -> bool {
    // P0 play is expressed by the playful regime plus punctuated locomotion.
    // Dedicated object/play recipes remain P1/P2 and do not become ActionIds.
    false
}
