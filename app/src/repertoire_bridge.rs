//! One-way presentation bridge. Motor/navigation/support and the existing blink
//! scheduler remain authoritative; recipes can fill only bounded residual slots.
use lifecore::{BodyIntent, PoseIntent};
use pet_motor::{RepertoireOutput, SomaticActuationPacket};

pub fn merge_packet(packet: &mut SomaticActuationPacket, repertoire: &RepertoireOutput) {
    let breath = finite(repertoire.effect.breath).clamp(-0.35, 0.35);
    packet.internal.breath_amplitude_multiplier =
        (packet.internal.breath_amplitude_multiplier + breath).clamp(0.35, 1.8);
    // Never evict a base-program field. The runtime already gates contact,
    // danger, dragging and object manipulation before supplying this field.
    if let Some(field) = repertoire.field
        && let Some(slot) = packet.fields.iter_mut().find(|slot| slot.is_none())
    {
        *slot = Some(field.bounded());
    }
}

pub fn merge_face(intent: &mut BodyIntent, repertoire: &RepertoireOutput) {
    // Explicit extra guard even if the producer has already suppressed sleep.
    if intent.pose == PoseIntent::Sleeping {
        return;
    }
    let e = repertoire.effect;
    let face = &mut intent.expression;
    face.brow_asymmetry = add(face.brow_asymmetry, e.brow_asymmetry, -0.65, 0.65);
    face.mouth_asymmetry = add(face.mouth_asymmetry, e.mouth_asymmetry, -0.65, 0.65);
    face.mouth_curve = add(face.mouth_curve, e.mouth_curve, -1.0, 1.0);
    face.mouth_open = add(face.mouth_open, e.mouth_open, 0.0, 1.0);
    // A blink recipe nominates the scheduler, never creates a second closure
    // via aperture or writes blink_left/right. Drowsy lids are not blinks.
    if !matches!(repertoire.active_id, Some(21 | 22 | 23 | 28 | 29 | 30)) {
        face.eye_aperture = add(face.eye_aperture, e.eye_aperture, 0.15, 1.5);
    }
    face.eye_scale = add(face.eye_scale, e.eye_scale, 0.7, 1.55);
    face.squint = add(face.squint, e.squint, 0.0, 0.75);
    if repertoire.active_id == Some(16) {
        // A bounded unilateral lid lift during recovery from drowse, not a
        // second blink or an eye-position offset. Expression smoothing owns
        // the transition; the residual disappears with this finite recipe.
        face.geometry.lids[0][0] = add(face.geometry.lids[0][0], e.eye_aperture * 0.8, -1.0, 1.0);
        face.geometry.lids[0][1] = add(face.geometry.lids[0][1], e.eye_aperture * 0.7, -1.0, 1.0);
    }
}

fn finite(value: f32) -> f32 {
    if value.is_finite() { value } else { 0.0 }
}

fn add(base: f32, residual: f32, low: f32, high: f32) -> f32 {
    // Do not re-clamp an untouched base expression owned by another subsystem.
    let residual = finite(residual);
    if residual == 0.0 {
        base
    } else {
        (finite(base) + residual).clamp(low, high)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pet_motor::{LocalSomaticField, RepertoireEffect};

    fn intent() -> BodyIntent {
        let mut life = lifecore::LifeCore::new(lifecore::Genome::from_seed(42), 42);
        life.tick(&Default::default(), &Default::default(), 0.05)
            .body_intent
    }

    #[test]
    fn repertoire_bridge_preserves_motor_authority_and_filled_slots() {
        let mut packet = SomaticActuationPacket::default();
        let field = LocalSomaticField {
            kind: pet_motor::SomaticFieldKind::Brace,
            space: pet_motor::FieldSpace::BodyLocal,
            center: glam::Vec2::ZERO,
            axis: glam::Vec2::Y,
            radius: 0.3,
            strength: 0.03,
            falloff: 2.0,
            frequency_hz: 0.0,
            phase_01: 0.0,
            target_component: None,
        };
        packet.fields.fill(Some(field));
        let before = packet.clone();
        merge_packet(
            &mut packet,
            &RepertoireOutput {
                field: Some(LocalSomaticField {
                    strength: 0.07,
                    ..field
                }),
                effect: RepertoireEffect {
                    breath: 0.2,
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        assert_eq!(packet.fields, before.fields);
        assert_eq!(packet.locomotion, before.locomotion);
        assert_eq!(packet.support, before.support);
        assert_eq!(packet.shape, before.shape);
        assert_eq!(packet.material, before.material);
        assert_eq!(packet.expression, before.expression);
        assert!((packet.internal.breath_amplitude_multiplier - 1.2).abs() < 0.001);
    }

    #[test]
    fn repertoire_bridge_sleep_and_blink_owner_are_preserved() {
        let mut body = intent();
        body.pose = PoseIntent::Sleeping;
        let before = body.expression;
        let output = RepertoireOutput {
            active_id: Some(22),
            effect: RepertoireEffect {
                eye_aperture: -0.8,
                eye_scale: 0.4,
                mouth_open: 0.7,
                ..Default::default()
            },
            ..Default::default()
        };
        merge_face(&mut body, &output);
        assert_eq!(body.expression, before);
        body.pose = PoseIntent::Neutral;
        merge_face(&mut body, &output);
        assert_eq!(body.expression.eye_aperture, before.eye_aperture);
        assert_eq!(body.expression.blink_left, before.blink_left);
        assert_eq!(body.expression.blink_right, before.blink_right);
    }

    #[test]
    fn repertoire_bridge_zero_residual_is_identity_and_nan_is_ignored() {
        let mut body = intent();
        let before = body.clone();
        merge_face(&mut body, &RepertoireOutput::default());
        assert_eq!(body.expression, before.expression);
        merge_face(
            &mut body,
            &RepertoireOutput {
                effect: RepertoireEffect {
                    eye_scale: f32::NAN,
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        assert_eq!(body.expression, before.expression);
        assert_eq!(body.target_position, before.target_position);
    }
}
