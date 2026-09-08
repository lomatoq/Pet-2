use glam::Vec2;

use crate::{FieldSpace, LocalSomaticField, SomaticActuationPacket, SomaticFieldKind};

#[must_use]
pub fn body_field(
    kind: SomaticFieldKind,
    center: Vec2,
    axis: Vec2,
    radius: f32,
    strength: f32,
    progress: f32,
) -> LocalSomaticField {
    LocalSomaticField {
        kind,
        space: FieldSpace::BodyLocal,
        center,
        axis,
        radius,
        strength,
        falloff: 2.2,
        frequency_hz: 0.0,
        phase_01: progress,
        target_component: None,
    }
    .bounded()
}

#[must_use]
pub fn contact_field(
    kind: SomaticFieldKind,
    axis: Vec2,
    radius: f32,
    strength: f32,
    progress: f32,
) -> LocalSomaticField {
    LocalSomaticField {
        kind,
        space: FieldSpace::PointerContact,
        center: Vec2::ZERO,
        axis,
        radius,
        strength,
        falloff: 2.8,
        frequency_hz: 0.0,
        phase_01: progress,
        target_component: None,
    }
    .bounded()
}

#[must_use]
pub fn component_field(
    kind: SomaticFieldKind,
    component: u8,
    axis: Vec2,
    radius: f32,
    strength: f32,
    progress: f32,
) -> LocalSomaticField {
    LocalSomaticField {
        kind,
        space: FieldSpace::ComponentLocal,
        center: Vec2::ZERO,
        axis,
        radius,
        strength,
        falloff: 2.0,
        frequency_hz: 0.0,
        phase_01: progress,
        target_component: Some(component),
    }
    .bounded()
}

#[must_use]
pub fn wave_field(
    space: FieldSpace,
    center: Vec2,
    axis: Vec2,
    radius: f32,
    strength: f32,
    frequency_hz: f32,
    progress: f32,
) -> LocalSomaticField {
    LocalSomaticField {
        kind: SomaticFieldKind::Wave,
        space,
        center,
        axis,
        radius,
        strength,
        falloff: 1.8,
        frequency_hz,
        phase_01: progress,
        target_component: None,
    }
    .bounded()
}

pub fn push_field(packet: &mut SomaticActuationPacket, field: LocalSomaticField) -> bool {
    if let Some(slot) = packet.fields.iter_mut().find(|slot| slot.is_none()) {
        *slot = Some(field.bounded());
        true
    } else {
        false
    }
}
