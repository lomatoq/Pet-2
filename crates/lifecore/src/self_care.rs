//! Causal microacts for a liquid animal, not a clock-driven animation playlist.
//! These are engineering analogies to animal self-care, not biological diagnoses.
use glam::Vec2;
use serde::{Deserialize, Serialize};

use crate::{AffectState, BodyFeedback, Drives, SensorFrame, TemperamentGenome};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SelfCareKind {
    #[default]
    None,
    /// Investigation is owned by existing sensory repertoire motifs.
    Investigate,
    Groom,
    Scratch,
    Nuzzle,
    Decline,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SelfCareMotorFrame {
    pub kind: SelfCareKind,
    pub strength: f32,
    pub phase: f32,
    pub side: f32,
    pub target: Option<Vec2>,
    pub body_lean: f32,
    pub body_pulse: f32,
    pub face_turn: f32,
    pub face_lowering: f32,
    pub squint: f32,
    pub mouth_open: f32,
    pub mouth_asymmetry: f32,
    pub tongue_extension: f32,
    pub reluctance: f32,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SelfCareState {
    /// Actual strain/contact aftermath; grooming depletes this reservoir.
    pub surface_disarray: f32,
    /// Touch saturation, distinct from attachment or a judgement of the user.
    pub contact_load: f32,
    /// Recent gentle-contact evidence; never created by mere pointer proximity.
    pub affiliative_evidence: f32,
    /// Effort/refractory reservoir; recovers quietly, not by soliciting attention.
    pub care_fatigue: f32,
    pub active: SelfCareKind,
    pub completed: u64,
    pub phase: f32,
    pub strength: f32,
    pub side: f32,
    last_strain: f32,
    capture_origin: Option<Vec2>,
    manipulation_latched: bool,
}

impl SelfCareState {
    pub fn is_valid(&self) -> bool {
        [
            self.surface_disarray,
            self.contact_load,
            self.affiliative_evidence,
            self.care_fatigue,
            self.phase,
            self.strength,
            self.last_strain,
        ]
        .iter()
        .all(|x| x.is_finite() && (0.0..=1.0).contains(x))
            && self.side.is_finite()
            && (-1.0..=1.0).contains(&self.side)
            && self.capture_origin.is_none_or(|p| p.is_finite())
    }

    pub fn resume_after_absence(&mut self) {
        // A route/gesture cannot keep running while the application is closed.
        self.active = SelfCareKind::None;
        self.strength = 0.0;
        self.phase = 0.0;
        self.last_strain = 0.0;
        self.capture_origin = None;
        self.manipulation_latched = false;
        // Contact-specific permission expires; identity and bodily aftermath do not.
        self.affiliative_evidence = 0.0;
    }

    #[allow(clippy::too_many_arguments)]
    pub fn tick(
        &mut self,
        sensors: &SensorFrame,
        body: &BodyFeedback,
        traits: &TemperamentGenome,
        drives: &mut Drives,
        affect: AffectState,
        busy: bool,
        dt: f32,
    ) -> SelfCareMotorFrame {
        let dt = if dt.is_finite() {
            dt.clamp(0.0, 0.25)
        } else {
            0.0
        };
        if dt == 0.0 {
            return SelfCareMotorFrame::default();
        }
        let contact = sensors.embodied_interaction.contact;
        let material = sensors.embodied_interaction.material;
        let strain = (material.neck_tension * 0.5
            + material.deformation_energy * 0.3
            + (material.maximum_strain - 0.5).max(0.0) * 0.2)
            .clamp(0.0, 1.0);
        // Own grooming motion must not manufacture an endless need to groom.
        if self.active == SelfCareKind::None {
            let impact = body
                .collision
                .as_ref()
                .map_or(0.0, |x| x.intensity.clamp(0.0, 1.0));
            self.surface_disarray +=
                (strain - self.last_strain).max(0.0) * 0.3 + (strain * 0.025 + impact * 0.08) * dt;
        }
        self.last_strain = strain;
        self.surface_disarray = (self.surface_disarray - dt * 0.0008).clamp(0.0, 1.0);
        self.care_fatigue = (self.care_fatigue - dt * 0.012).max(0.0);
        if contact.active {
            let pressure = contact.effective_pressure.clamp(0.0, 1.0);
            let motion = contact.relative_velocity_local.length().min(2.0);
            let gentle = (1.0 - pressure * 2.0).max(0.0) * (1.0 - motion * 0.5);
            self.contact_load =
                (self.contact_load + dt * (0.018 + pressure * 0.13 + motion * 0.04)).min(1.0);
            self.affiliative_evidence = (self.affiliative_evidence + dt * gentle * 0.16
                - dt * (1.0 - gentle) * 0.3)
                .clamp(0.0, 1.0);
            if contact.point_local.x.abs() > 0.05 {
                self.side = contact.point_local.x.signum();
            }
        } else {
            self.contact_load = (self.contact_load - dt * 0.035).max(0.0);
            self.affiliative_evidence *= (-dt / 1.5).exp();
        }
        // Native `pet_dragged` means pointer capture, including a stationary
        // gentle touch. It is not itself evidence of lifting the animal.
        // Once a real pull/pickup is observed, keep ownership with manipulation
        // until release, even if the pointer subsequently stops in mid-air.
        if sensors.pet_dragged {
            let origin = *self.capture_origin.get_or_insert(body.world_position);
            self.manipulation_latched |= body.world_position.distance(origin) > 0.045
                || contact.pointer_speed > 2.0
                || contact.effective_pressure > 0.7
                || material.neck_tension > 0.45
                || material.maximum_strain > 1.5;
        } else {
            self.capture_origin = None;
            self.manipulation_latched = false;
        }
        let physical_manipulation =
            sensors.pet_dragged && (!contact.active || self.manipulation_latched);
        let busy = busy || physical_manipulation || drives.safety > 0.62;
        if busy {
            self.active = SelfCareKind::None;
            self.strength = 0.0;
            self.phase = 0.0;
            return SelfCareMotorFrame::default();
        }
        let vigor = (1.0 - drives.sleep * 0.75) * (1.0 - self.care_fatigue);
        let quiet = (1.0 - body.velocity.length() * 6.0).clamp(0.0, 1.0);
        let rest_value = 0.045 + self.care_fatigue * 0.45 + drives.sleep * 0.09;
        let groom = self.surface_disarray * quiet * vigor * (0.65 + traits.patience * 0.35);
        let scratch = self.surface_disarray
            * quiet
            * vigor
            * (0.4 + self.last_strain * 0.8 + (1.0 - traits.patience) * 0.4);
        let nuzzle = self.affiliative_evidence
            * f32::from(contact.active)
            * (0.3 + affect.attachment * 0.7)
            * (0.3 + traits.sociability * 0.7)
            * (1.0 - affect.stress)
            * (1.0 - self.contact_load)
            * vigor;
        let decline = f32::from(contact.active)
            * self.contact_load
            * (0.4 + drives.autonomy * 0.6 + drives.sleep * 0.6 + affect.stress * 0.65);
        let candidates = [
            (SelfCareKind::Groom, groom),
            (SelfCareKind::Scratch, scratch),
            (SelfCareKind::Nuzzle, nuzzle),
            (SelfCareKind::Decline, decline),
        ];
        let active_value = candidates
            .iter()
            .find(|(kind, _)| *kind == self.active)
            .map_or(0.0, |x| x.1);
        if self.active == SelfCareKind::None {
            let &(kind, value) = candidates
                .iter()
                .max_by(|a, b| a.1.total_cmp(&b.1))
                .unwrap();
            if value > rest_value {
                self.active = kind;
                self.phase = 0.0;
                if self.side == 0.0 {
                    self.side = if traits.boldness >= 0.5 { 1.0 } else { -1.0 };
                }
            }
        } else if active_value < rest_value * 0.6
            || (self.active == SelfCareKind::Nuzzle && !contact.active)
        {
            self.strength *= (-dt / 0.18).exp();
            if self.strength < 0.02 {
                self.active = SelfCareKind::None;
                self.completed = self.completed.saturating_add(1);
            }
            return self.motor(sensors);
        }
        if self.active == SelfCareKind::None {
            return SelfCareMotorFrame::default();
        }
        let urgency = candidates
            .iter()
            .find(|(kind, _)| *kind == self.active)
            .map_or(0.0, |x| x.1);
        let target_strength = (0.32 + urgency * 1.1).clamp(0.0, 0.9);
        self.strength += (target_strength - self.strength) * (1.0 - (-dt / 0.18).exp());
        let frequency = match self.active {
            SelfCareKind::Scratch => 2.1 + vigor * 1.2 + self.surface_disarray,
            SelfCareKind::Groom => 0.7 + vigor * 0.8,
            SelfCareKind::Nuzzle => 0.35 + traits.sociability * 0.35,
            _ => 0.3,
        };
        self.phase = (self.phase + dt * frequency).rem_euclid(1.0);
        let work = self.strength * dt;
        self.care_fatigue = (self.care_fatigue + work * 0.065).min(1.0);
        drives.sleep = (drives.sleep + work * 0.0008).min(1.0);
        match self.active {
            SelfCareKind::Groom | SelfCareKind::Scratch => {
                self.surface_disarray =
                    (self.surface_disarray - work * (0.055 + vigor * 0.045)).max(0.0);
                drives.comfort = (drives.comfort - work * 0.005).max(0.0);
            }
            SelfCareKind::Nuzzle => {
                self.affiliative_evidence = (self.affiliative_evidence - work * 0.32).max(0.0);
                drives.social = (drives.social - work * 0.012).max(0.0);
            }
            // Declining is communication, not an automatic cure for continued touch.
            _ => {}
        }
        self.motor(sensors)
    }

    fn motor(&self, sensors: &SensorFrame) -> SelfCareMotorFrame {
        if self.active == SelfCareKind::None {
            return SelfCareMotorFrame::default();
        }
        let wave = (self.phase * std::f32::consts::TAU).sin();
        let pulse = (0.5 - 0.5 * (self.phase * std::f32::consts::TAU).cos()).max(0.0);
        let mut frame = SelfCareMotorFrame {
            kind: self.active,
            strength: self.strength,
            phase: self.phase,
            side: self.side,
            ..SelfCareMotorFrame::default()
        };
        match self.active {
            SelfCareKind::Groom => {
                frame.face_lowering = 0.35 + pulse * 0.4;
                frame.face_turn = self.side * 0.45;
                frame.mouth_asymmetry = self.side * pulse * 0.55;
                frame.tongue_extension = pulse.powi(3);
                frame.mouth_open = 0.08 + pulse * 0.16;
                frame.squint = 0.4;
                frame.body_lean = self.side * 0.25;
            }
            SelfCareKind::Scratch => {
                frame.face_turn = self.side * 0.55;
                frame.body_pulse = pulse;
                frame.body_lean = self.side * (0.25 + wave * 0.22);
                frame.squint = 0.5 + pulse * 0.3;
                frame.mouth_asymmetry = self.side * 0.3;
            }
            SelfCareKind::Nuzzle => {
                frame.target = Some(sensors.embodied_interaction.contact.point_world);
                frame.face_turn = self.side * 0.4;
                frame.body_lean = self.side * (0.5 + wave * 0.25);
                frame.squint = 0.55;
            }
            SelfCareKind::Decline => {
                frame.target = Some(sensors.cursor_position);
                frame.face_turn = -self.side * 0.6;
                frame.body_lean = -self.side * 0.55;
                frame.squint = 0.2;
                frame.reluctance = 1.0;
                frame.mouth_asymmetry = -self.side * 0.35;
            }
            _ => {}
        }
        frame
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Genome;

    #[test]
    fn native_capture_through_lifecore_tick_still_allows_gentle_contact_response() {
        let mut genome = Genome::from_seed(8);
        genome.temperament.sociability = 1.0;
        let mut life = crate::LifeCore::new(genome, 8);
        life.state.affect.attachment = 0.9;
        let mut sensors = SensorFrame {
            pet_dragged: true,
            ..Default::default()
        };
        sensors.pointer_down = true;
        sensors.pet_hovered = true;
        sensors.embodied_interaction.contact.active = true;
        sensors.embodied_interaction.contact.point_local.x = 0.5;
        sensors.embodied_interaction.contact.effective_pressure = 0.1;
        let mut nuzzled = false;
        for tick in 0..800 {
            sensors.timestamp = tick as f64 * 0.05;
            sensors.pet_touched = tick == 0;
            let output = life.tick(&sensors, &BodyFeedback::default(), 0.05);
            assert!(!matches!(
                output.body_intent.expression.face_pose,
                crate::FacePose::Boundary | crate::FacePose::Startled
            ));
            nuzzled |= life
                .update_self_care(&sensors, &BodyFeedback::default(), false, 0.05)
                .kind
                == SelfCareKind::Nuzzle;
        }
        assert!(nuzzled);
    }

    #[test]
    fn own_actuation_strain_cannot_replenish_grooming_for_twenty_minutes() {
        let genome = Genome::from_seed(14);
        let mut drives = Drives::initial(&genome.temperament);
        let mut state = SelfCareState {
            surface_disarray: 0.35,
            ..Default::default()
        };
        let mut sensors = SensorFrame::default();
        let mut last_frame = SelfCareMotorFrame::default();
        let mut late_active = 0;
        for tick in 0..24_000 {
            // Feedback generated by the previous gesture, not new external touch.
            sensors.embodied_interaction.material.deformation_energy = last_frame.strength;
            sensors.embodied_interaction.material.neck_tension =
                last_frame.body_pulse * last_frame.strength;
            last_frame = state.tick(
                &sensors,
                &BodyFeedback::default(),
                &genome.temperament,
                &mut drives,
                AffectState::default(),
                false,
                0.05,
            );
            if tick > 2400 && last_frame.kind != SelfCareKind::None {
                late_active += 1;
            }
            assert!(state.is_valid());
        }
        assert_eq!(late_active, 0);
        assert!(state.surface_disarray < 0.01);
    }

    #[test]
    fn idle_without_evidence_does_not_play_random_gestures() {
        let genome = Genome::from_seed(4);
        let mut drives = Drives::initial(&genome.temperament);
        let mut state = SelfCareState::default();
        for _ in 0..12000 {
            assert_eq!(
                state
                    .tick(
                        &SensorFrame::default(),
                        &BodyFeedback::default(),
                        &genome.temperament,
                        &mut drives,
                        AffectState::default(),
                        false,
                        0.05
                    )
                    .kind,
                SelfCareKind::None
            );
        }
    }

    #[test]
    fn strain_causes_bounded_self_care_then_recovery_not_an_endless_loop() {
        let genome = Genome::from_seed(4);
        let mut drives = Drives::initial(&genome.temperament);
        let mut state = SelfCareState::default();
        let mut sensors = SensorFrame::default();
        sensors.embodied_interaction.material.neck_tension = 0.9;
        sensors.embodied_interaction.material.maximum_strain = 1.8;
        let mut active = 0;
        for tick in 0..2400 {
            if tick == 20 {
                sensors.embodied_interaction.material = Default::default();
            }
            let frame = state.tick(
                &sensors,
                &BodyFeedback::default(),
                &genome.temperament,
                &mut drives,
                AffectState::default(),
                false,
                0.05,
            );
            active += usize::from(matches!(
                frame.kind,
                SelfCareKind::Groom | SelfCareKind::Scratch
            ));
            assert!(state.is_valid());
        }
        assert!(active > 10 && active < 800, "{active}");
        assert_eq!(state.active, SelfCareKind::None);
    }

    #[test]
    fn contact_affection_and_saturation_change_response_without_forgetting_bond() {
        let mut genome = Genome::from_seed(8);
        genome.temperament.sociability = 1.0;
        let mut drives = Drives::initial(&genome.temperament);
        let affect = AffectState {
            attachment: 0.9,
            stress: 0.0,
            ..Default::default()
        };
        // Native capture sets this even for gentle stationary pressure.
        let mut sensors = SensorFrame {
            pet_dragged: true,
            ..Default::default()
        };
        sensors.embodied_interaction.contact.active = true;
        sensors.embodied_interaction.contact.point_local.x = 0.5;
        let mut state = SelfCareState::default();
        let mut nuzzled = false;
        let mut declined = false;
        for _ in 0..2400 {
            let frame = state.tick(
                &sensors,
                &BodyFeedback::default(),
                &genome.temperament,
                &mut drives,
                affect,
                false,
                0.05,
            );
            nuzzled |= frame.kind == SelfCareKind::Nuzzle;
            declined |= frame.kind == SelfCareKind::Decline;
        }
        assert!(nuzzled && declined);
        let load = state.contact_load;
        sensors.embodied_interaction.contact.active = false;
        sensors.pet_dragged = false;
        for _ in 0..1600 {
            state.tick(
                &sensors,
                &BodyFeedback::default(),
                &genome.temperament,
                &mut drives,
                affect,
                false,
                0.05,
            );
        }
        assert!(state.contact_load < load * 0.1);
        assert_eq!(affect.attachment, 0.9);
    }

    #[test]
    fn real_pickup_retains_motor_ownership_until_release_even_when_it_stops() {
        let mut genome = Genome::from_seed(8);
        genome.temperament.sociability = 1.0;
        let mut drives = Drives::initial(&genome.temperament);
        let affect = AffectState {
            attachment: 0.9,
            stress: 0.0,
            ..Default::default()
        };
        let mut sensors = SensorFrame {
            pet_dragged: true,
            ..Default::default()
        };
        sensors.embodied_interaction.contact.active = true;
        sensors.embodied_interaction.contact.point_local.x = 0.5;
        sensors.embodied_interaction.contact.pointer_speed = 2.5;
        let mut state = SelfCareState::default();
        for tick in 0..1000 {
            if tick == 10 {
                sensors.embodied_interaction.contact.pointer_speed = 0.0;
            }
            let frame = state.tick(
                &sensors,
                &BodyFeedback::default(),
                &genome.temperament,
                &mut drives,
                affect,
                false,
                0.05,
            );
            assert_eq!(frame.kind, SelfCareKind::None);
        }
        sensors.pet_dragged = false;
        sensors.embodied_interaction.contact.active = false;
        state.tick(
            &sensors,
            &BodyFeedback::default(),
            &genome.temperament,
            &mut drives,
            affect,
            false,
            0.05,
        );
        assert!(!state.manipulation_latched);

        // Slow translation of the whole animal is also a pickup, not a stroke.
        sensors.pet_dragged = true;
        sensors.embodied_interaction.contact.active = true;
        let mut body = BodyFeedback::default();
        state.tick(
            &sensors,
            &body,
            &genome.temperament,
            &mut drives,
            affect,
            false,
            0.05,
        );
        body.world_position.x += 0.06;
        assert_eq!(
            state
                .tick(
                    &sensors,
                    &body,
                    &genome.temperament,
                    &mut drives,
                    affect,
                    false,
                    0.05
                )
                .kind,
            SelfCareKind::None
        );
        assert!(state.manipulation_latched);
    }

    #[test]
    fn busy_owner_prevents_invisible_relief_and_resume_preserves_need() {
        let genome = Genome::from_seed(4);
        let mut drives = Drives::initial(&genome.temperament);
        let comfort = drives.comfort;
        let mut state = SelfCareState {
            surface_disarray: 0.6,
            ..Default::default()
        };
        for _ in 0..200 {
            let frame = state.tick(
                &SensorFrame::default(),
                &BodyFeedback::default(),
                &genome.temperament,
                &mut drives,
                AffectState::default(),
                true,
                0.05,
            );
            assert_eq!(frame, SelfCareMotorFrame::default());
        }
        assert_eq!(drives.comfort, comfort);
        assert!(state.surface_disarray > 0.58);
        let before = state.surface_disarray;
        let mut restored: SelfCareState =
            serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap();
        restored.resume_after_absence();
        assert_eq!(restored.surface_disarray, before);
        assert!(restored.is_valid());
        assert!(
            serde_json::from_str::<SelfCareState>("{}")
                .unwrap()
                .is_valid()
        );
    }
}
