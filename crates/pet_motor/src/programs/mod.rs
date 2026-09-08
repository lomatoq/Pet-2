mod defaults;
mod defense;
mod home;
mod locomotion;
mod physiology;
mod play;
mod rest;
mod social;
mod touch;

use lifecore::BehaviorGoalFrame;

use crate::{
    ActivePerformance, BehaviorContextFrame, BehaviorTarget, MotorReadabilityTuning, RegimeBlend,
    SomaticActuationPacket, SurfaceAttachmentCommand,
};

pub(crate) fn apply_regime(packet: &mut SomaticActuationPacket, regime: RegimeBlend) {
    physiology::apply_regime(packet, regime);
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn apply_program(
    packet: &mut SomaticActuationPacket,
    active: &ActivePerformance,
    phase_name: &str,
    progress: f32,
    phase_started: bool,
    goal: &BehaviorGoalFrame,
    context: &BehaviorContextFrame,
    tuning: MotorReadabilityTuning,
) {
    if let Some(target) = active
        .locked_target
        .as_ref()
        .and_then(BehaviorTarget::world_position)
    {
        packet.locomotion.target_position = Some(target);
        packet.locomotion.target_locked = true;
        packet.expression.gaze_target = Some(target);
    }
    let handled = locomotion::apply(
        packet,
        active,
        phase_name,
        progress,
        phase_started,
        goal,
        context,
        tuning,
    ) || rest::apply(
        packet,
        active,
        phase_name,
        progress,
        phase_started,
        goal,
        context,
        tuning,
    ) || social::apply(
        packet,
        active,
        phase_name,
        progress,
        phase_started,
        goal,
        context,
        tuning,
    ) || touch::apply(
        packet,
        active,
        phase_name,
        progress,
        phase_started,
        goal,
        context,
        tuning,
    ) || defense::apply(
        packet,
        active,
        phase_name,
        progress,
        phase_started,
        goal,
        context,
        tuning,
    ) || home::apply(
        packet,
        active,
        phase_name,
        progress,
        phase_started,
        goal,
        context,
        tuning,
    ) || physiology::apply(
        packet,
        active,
        phase_name,
        progress,
        phase_started,
        goal,
        context,
        tuning,
    ) || play::apply(
        packet,
        active,
        phase_name,
        progress,
        phase_started,
        goal,
        context,
        tuning,
    ) || defaults::apply(
        packet,
        active,
        phase_name,
        progress,
        phase_started,
        goal,
        context,
        tuning,
    );
    debug_assert!(
        handled,
        "unimplemented P0 motor program: {:?}",
        active.program
    );
    packet.sanitize();
}

pub(crate) fn surface_command(
    active: &ActivePerformance,
    load_fraction: f32,
    contact_fraction: f32,
    adhesion: f32,
) -> Option<SurfaceAttachmentCommand> {
    let BehaviorTarget::Surface(surface) = active.locked_target.as_ref()? else {
        return None;
    };
    let mut command = SurfaceAttachmentCommand {
        surface_id: surface.surface_id.clone(),
        anchor_point: surface.anchor_point,
        normal: surface.normal,
        tangent: surface.tangent,
        target_contact_fraction: contact_fraction,
        normal_compliance: 0.22,
        tangent_friction: 0.68,
        adhesion,
        load_fraction,
        break_force: 0.78,
        release_half_life: 0.34,
    };
    command.sanitize();
    Some(command)
}

pub(crate) fn smooth(progress: f32) -> f32 {
    let t = progress.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

pub(crate) fn target_axis(
    active: &ActivePerformance,
    context: &BehaviorContextFrame,
) -> glam::Vec2 {
    active
        .locked_target
        .as_ref()
        .and_then(BehaviorTarget::world_position)
        .map_or(glam::Vec2::X, |target| {
            (target - context.body.motion.world_position).normalize_or_zero()
        })
}
