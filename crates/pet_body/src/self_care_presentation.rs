//! Causal motor presentation; no action scheduler and no independent blink owner.
use lifecore::{BodyIntent, SelfCareMotorFrame};

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(crate) struct SelfCarePresentation {
    pub strength: f32,
    pub tongue: f32,
    pub side: f32,
    pub lean: f32,
    pub pulse: f32,
    pub turn: f32,
    pub lowering: f32,
    pub squint: f32,
    pub mouth_open: f32,
    pub asymmetry: f32,
    pub reluctance: f32,
    pub affection: f32,
}
impl SelfCarePresentation {
    pub fn update(&mut self, motor: SelfCareMotorFrame, suppressed: bool, dt: f32) {
        let unit = |v: f32| {
            if v.is_finite() {
                v.clamp(0.0, 1.0)
            } else {
                0.0
            }
        };
        let signed = |v: f32| {
            if v.is_finite() {
                v.clamp(-1.0, 1.0)
            } else {
                0.0
            }
        };
        let authority = if suppressed || motor.kind == lifecore::SelfCareKind::None {
            0.0
        } else {
            unit(motor.strength)
        };
        let dt = if dt.is_finite() {
            dt.clamp(0.0, 0.05)
        } else {
            0.0
        };
        let alpha = 1.0 - (-dt / 0.075).exp();
        let smooth = |value: &mut f32, target: f32| *value += (target - *value) * alpha;
        smooth(&mut self.strength, authority);
        smooth(&mut self.side, signed(motor.side));
        smooth(&mut self.lean, signed(motor.body_lean) * authority);
        smooth(&mut self.pulse, unit(motor.body_pulse) * authority);
        smooth(&mut self.turn, signed(motor.face_turn) * authority);
        smooth(&mut self.lowering, unit(motor.face_lowering) * authority);
        smooth(&mut self.squint, unit(motor.squint) * authority);
        smooth(&mut self.mouth_open, unit(motor.mouth_open) * authority);
        smooth(
            &mut self.asymmetry,
            signed(motor.mouth_asymmetry) * authority,
        );
        smooth(&mut self.reluctance, unit(motor.reluctance) * authority);
        smooth(
            &mut self.affection,
            if motor.kind == lifecore::SelfCareKind::Nuzzle {
                authority
            } else {
                0.0
            },
        );
        smooth(&mut self.tongue, unit(motor.tongue_extension) * authority);
        // Feeding and protective events take ownership immediately, even while
        // the body's softer motion releases over a short physical timescale.
        if suppressed || motor.kind == lifecore::SelfCareKind::None {
            self.tongue = 0.0;
            self.mouth_open = 0.0;
            self.squint = 0.0;
            self.asymmetry = 0.0;
        }
    }
    pub fn compose_body(&self, packet: &mut pet_motor::SomaticActuationPacket) {
        let amount = self.lean.abs().max(self.pulse);
        if amount < 0.001 {
            return;
        }
        // Additive local yielding uses the same bounded physical field as the
        // existing motor repertoire. Never steal a field or alter support.
        if let Some(slot) = packet.fields.iter_mut().find(|slot| slot.is_none()) {
            *slot = Some(
                pet_motor::LocalSomaticField {
                    kind: pet_motor::SomaticFieldKind::Shear,
                    space: pet_motor::FieldSpace::BodyLocal,
                    center: glam::Vec2::new(self.side * 0.20, -0.10),
                    axis: glam::Vec2::new(self.lean.signum(), -self.pulse * 0.45)
                        .normalize_or_zero(),
                    radius: 0.30,
                    strength: amount * 0.06,
                    falloff: 2.2,
                    frequency_hz: 0.0,
                    phase_01: 0.0,
                    target_component: None,
                }
                .bounded(),
            );
        }
    }
    pub fn apply_face(&self, intent: &mut BodyIntent) {
        let face = &mut intent.expression;
        face.squint = face.squint.max(self.squint * 0.65);
        face.mouth_open = face.mouth_open.max(self.mouth_open);
        face.mouth_asymmetry = (face.mouth_asymmetry + self.asymmetry * 0.55).clamp(-1.0, 1.0);
        face.mouth_curve =
            (face.mouth_curve - self.reluctance * 0.28 + self.affection * 0.36).clamp(-1.0, 1.0);
        face.brow_raise =
            (face.brow_raise + self.reluctance * 0.25 + self.affection * 0.12).clamp(-1.0, 1.0);
        face.brow_tension *= 1.0 - self.affection * 0.5;
        // Soft unilateral lid recruitment follows which side is being tended;
        // blinking remains wholly owned by the managed eyelid controller.
        let side_index = usize::from(self.side > 0.0);
        face.geometry.lids[side_index][2] += self.squint * 0.14;
        face.geometry = face.geometry.sanitized();
        face.brow_asymmetry =
            (face.brow_asymmetry + self.side * self.reluctance * 0.3).clamp(-1.0, 1.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn self_care_field_is_bounded_and_never_replaces_owned_motor_fields() {
        let p = SelfCarePresentation {
            strength: 1.0,
            lean: -0.8,
            pulse: 0.7,
            side: -1.0,
            ..Default::default()
        };
        let mut packet = pet_motor::SomaticActuationPacket::default();
        p.compose_body(&mut packet);
        let field = packet.fields[0].unwrap();
        assert!(field.strength <= 0.06 && field.radius <= 0.30);
        assert_eq!(field.frequency_hz, 0.0);
        packet.fields.fill(Some(field));
        let before = packet.clone();
        p.compose_body(&mut packet);
        assert_eq!(packet, before);
    }

    #[test]
    fn self_care_is_readable_but_does_not_own_blinks_or_gaze() {
        let mut p = SelfCarePresentation::default();
        let motor = SelfCareMotorFrame {
            kind: lifecore::SelfCareKind::Groom,
            strength: 1.0,
            tongue_extension: 0.9,
            mouth_open: 0.3,
            squint: 0.7,
            body_lean: 0.8,
            face_lowering: 0.6,
            mouth_asymmetry: 0.5,
            ..Default::default()
        };
        for _ in 0..60 {
            p.update(motor, false, 1.0 / 120.0);
        }
        let mut intent = BodyIntent {
            locomotion: lifecore::LocomotionMode::Hover,
            target_position: glam::Vec2::ZERO,
            target_surface: None,
            desired_speed: 0.0,
            facing_direction: 0.0,
            gaze_target: None,
            pose: lifecore::PoseIntent::Neutral,
            expression: Default::default(),
            interaction_target: None,
        };
        intent.expression.blink_left = 0.73;
        intent.expression.blink_right = 0.12;
        intent.gaze_target = Some(glam::Vec2::new(0.2, 0.8));
        p.apply_face(&mut intent);
        assert!(p.tongue > 0.85 && intent.expression.mouth_open > 0.28);
        assert!(intent.expression.squint > 0.4);
        assert_eq!(
            (intent.expression.blink_left, intent.expression.blink_right),
            (0.73, 0.12)
        );
        assert_eq!(intent.gaze_target, Some(glam::Vec2::new(0.2, 0.8)));
        p.update(motor, true, 1.0 / 120.0);
        assert_eq!(p.tongue, 0.0);
        assert_eq!(p.mouth_open, 0.0);
        for _ in 0..120 {
            p.update(SelfCareMotorFrame::default(), false, 1.0 / 120.0);
        }
        assert!(p.lean.abs() < 0.001 && p.lowering < 0.001);
    }
    #[test]
    fn nonfinite_motor_channels_cannot_escape_to_renderer() {
        let mut p = SelfCarePresentation::default();
        p.update(
            SelfCareMotorFrame {
                strength: f32::NAN,
                tongue_extension: f32::INFINITY,
                body_lean: f32::NAN,
                ..Default::default()
            },
            false,
            0.05,
        );
        assert_eq!(p, SelfCarePresentation::default());
    }
}
