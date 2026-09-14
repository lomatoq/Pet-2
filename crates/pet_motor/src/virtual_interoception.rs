//! Explicit fictional chemistry of virtual scene objects, never a sensor of
//! real air, user health or illness. Finite novelty/contact doses habituate.
use crate::{BehaviorContextFrame, RepertoireEvent, SomaticActuationPacket};
use glam::Vec2;
use lifecore::{ActionId, BehaviorGoalFrame, PrimaryIntent};
use serde::Serialize;

pub const VIRTUAL_INTEROCEPTION_IDS: &[u16] = &[
    62, 63, 64, 65, 66, 67, 68, 69, 70, 71, 72, 73, 74, 87, 88, 90,
];

#[derive(Debug, Clone, Copy, Default, Serialize)]
pub struct VirtualInteroceptionSignals {
    pub intensity: f32,
    pub gradient: Vec2,
    pub adaptation: f32,
    pub irritation: f32,
    pub contact_residue: f32,
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (
        BehaviorGoalFrame,
        BehaviorContextFrame,
        SomaticActuationPacket,
    ) {
        let mut life = lifecore::LifeCore::new(lifecore::Genome::from_seed(42), 42);
        let body_intent = life
            .tick(&Default::default(), &Default::default(), 0.05)
            .body_intent;
        let goal = BehaviorGoalFrame {
            action: ActionId::IdleHover,
            body_intent,
            affect: Default::default(),
            drives: life.state.drives,
            felt: Default::default(),
            derived: Default::default(),
            attachment: 0.4,
            recent_outcome: None,
        };
        let mut c = BehaviorContextFrame::default();
        c.body.motion.world_position = Vec2::splat(0.5);
        c.screen_edge_supported = true;
        c.screen_edge_support_stable_seconds = 1.0;
        c.somatic.supported = true;
        (goal, c, SomaticActuationPacket::default())
    }
    fn step(
        s: &mut VirtualInteroception,
        g: &BehaviorGoalFrame,
        c: &BehaviorContextFrame,
        p: &SomaticActuationPacket,
        count: usize,
    ) -> Vec<u16> {
        (0..count)
            .flat_map(|_| s.observe(g, c, p, 0.05))
            .map(|e| e.id)
            .collect()
    }
    #[test]
    fn finite_virtual_dose_sneezes_once_then_habituates_deterministically() {
        let (g, mut c, p) = fixture();
        let mut a = VirtualInteroception::default();
        let _ = step(&mut a, &g, &c, &p, 1);
        c.orb_id = Some(7);
        c.orb_position = Some(Vec2::new(0.51, 0.5));
        let mut b = a.clone();
        let ids = step(&mut a, &g, &c, &p, 2400);
        assert_eq!(ids, step(&mut b, &g, &c, &p, 2400));
        for id in [68, 71, 72, 74] {
            assert_eq!(ids.iter().filter(|&&i| i == id).count(), 1, "{ids:?}");
        }
        assert!(step(&mut a, &g, &c, &p, 2400).is_empty());
        assert!(a.signals.irritation < 0.001);
        assert!((0.0..=1.0).contains(&a.signals.adaptation));
    }
    #[test]
    fn disappearance_aborts_prepared_sneeze_and_sleep_cannot_start_it() {
        let (mut g, mut c, p) = fixture();
        let mut s = VirtualInteroception::default();
        let _ = step(&mut s, &g, &c, &p, 1);
        c.orb_id = Some(7);
        c.orb_position = Some(Vec2::splat(0.5));
        let mut prepared = false;
        for _ in 0..60 {
            if step(&mut s, &g, &c, &p, 1).contains(&71) {
                prepared = true;
                break;
            }
        }
        assert!(prepared);
        c.orb_position = None;
        let ids = step(&mut s, &g, &c, &p, 40);
        assert!(ids.contains(&73));
        assert!(ids.contains(&70));
        assert!(!ids.contains(&72));
        g.action = ActionId::Sleep;
        c.orb_id = Some(8);
        c.orb_position = Some(Vec2::splat(0.5));
        assert!(step(&mut s, &g, &c, &p, 400).is_empty());
    }
    #[test]
    fn source_height_familiarity_and_native_recipe_path_are_reachable() {
        let (g, mut c, p) = fixture();
        let mut s = VirtualInteroception::default();
        let _ = step(&mut s, &g, &c, &p, 1);
        c.orb_id = Some(9);
        c.orb_position = Some(Vec2::new(0.5, 0.4));
        assert!(step(&mut s, &g, &c, &p, 1).contains(&66));
        let _ = step(&mut s, &g, &c, &p, 600);
        c.body.motion.world_position = Vec2::new(0.0, 0.0);
        let _ = step(&mut s, &g, &c, &p, 1);
        c.body.motion.world_position = Vec2::splat(0.5);
        assert!(step(&mut s, &g, &c, &p, 1).contains(&67));
        // End-to-end: no explicit event injection. The actual context adapter
        // must deliver the sneeze to its existing bounded output renderer.
        let mut runtime = crate::RepertoireRuntime::new(42);
        c.orb_position = None;
        c.orb_id = None;
        let _ = runtime.tick(&g, &c, &p, &[], 0.05);
        c.orb_position = Some(Vec2::new(0.51, 0.5));
        c.orb_id = Some(10);
        let mut sneeze = false;
        for _ in 0..400 {
            let out = runtime.tick(&g, &c, &p, &[], 0.05);
            if out.active_id == Some(72) && out.budget > 0.1 {
                sneeze = true;
                assert!(out.effect.breath.abs() > 0.0 || out.field.is_some());
            }
        }
        assert!(sneeze);
    }

    #[test]
    fn spatial_samples_and_contact_care_require_actual_scene_changes() {
        let (g, mut c, mut p) = fixture();
        let mut s = VirtualInteroception::default();
        let _ = step(&mut s, &g, &c, &p, 1);
        c.den_anchor = Some(Vec2::new(0.6, 0.5));
        assert!(step(&mut s, &g, &c, &p, 1).contains(&62));
        c.orb_id = Some(1);
        c.orb_position = Some(Vec2::new(0.72, 0.5));
        let ids = step(&mut s, &g, &c, &p, 1);
        assert!(ids.contains(&64), "{ids:?}");
        c.orb_position = Some(Vec2::new(0.56, 0.5));
        assert!(step(&mut s, &g, &c, &p, 1).contains(&69));
        c.body.motion.world_position = Vec2::new(0.43, 0.5);
        let ids = step(&mut s, &g, &c, &p, 1);
        assert!(ids.contains(&63));
        assert!(ids.contains(&87));
        p.fields.fill(Some(crate::body_field(
            crate::SomaticFieldKind::Brace,
            Vec2::ZERO,
            Vec2::X,
            0.4,
            0.1,
            0.5,
        )));
        let ids = step(&mut s, &g, &c, &p, 80);
        assert!(ids.contains(&88));
        assert!(ids.contains(&90));
        assert!(step(&mut s, &g, &c, &p, 1000).is_empty());
    }
}

#[derive(Debug, Clone, Default)]
pub struct VirtualInteroception {
    pub signals: VirtualInteroceptionSignals,
    initialized: bool,
    orb: Option<Vec2>,
    orb_id: Option<u64>,
    den: Option<Vec2>,
    last_sample: Vec2,
    novelty: f32,
    sampled: bool,
    weak_sampled: bool,
    contact: bool,
    sneeze_stage: u8,
    stage_seconds: f32,
    refractory: f32,
    care_stage: u8,
    care_seconds: f32,
}

impl VirtualInteroception {
    pub fn observe(
        &mut self,
        goal: &BehaviorGoalFrame,
        c: &BehaviorContextFrame,
        packet: &SomaticActuationPacket,
        dt: f32,
    ) -> Vec<RepertoireEvent> {
        let dt = if dt.is_finite() {
            dt.clamp(0.0, 0.25)
        } else {
            0.0
        };
        let position = c.body.motion.world_position;
        if !position.is_finite() {
            return Vec::new();
        }
        let orb = c.orb_position.filter(|p| p.is_finite());
        let den = c.den_anchor.filter(|p| p.is_finite());
        let changed =
            self.initialized && (c.orb_id != self.orb_id || orb.is_some() != self.orb.is_some());
        let moved = self.initialized
            && orb
                .zip(self.orb)
                .is_some_and(|(a, b)| a.distance(b) > 0.035);
        let den_changed = self.initialized && den != self.den;
        let allowed = position.is_finite()
            && !c.pet_dragged
            && !c.pet_touched
            && !c.orb_user_held
            && goal.action != ActionId::Sleep
            && c.companion_intent != PrimaryIntent::Sleep
            && goal.felt.startle < 0.5
            && goal.felt.pain_like < 0.2
            && goal.affect.stress < 0.58
            && matches!(
                c.world_goal,
                crate::MotorWorldGoal::None | crate::MotorWorldGoal::ReturnHome
            );
        let mut events = Vec::new();
        if !self.initialized {
            self.weak_sampled = true;
        }
        let mut emit = |id, target: Option<Vec2>| {
            if allowed {
                events.push(RepertoireEvent {
                    id,
                    confidence: 0.85,
                    target,
                    side: target.map_or(0.0, |p| (p.x - position.x).signum()),
                });
            }
        };
        // A new scene object releases one finite virtual scent dose. Moving
        // the same source changes geometry, not its identity or novelty dose.
        if changed && orb.is_some() {
            self.novelty = 1.0;
            self.sampled = false;
            self.weak_sampled = false;
        }
        if changed && orb.is_none() && self.sampled {
            emit(70, self.orb);
        }
        if den_changed && den.is_some_and(|p| p.distance(position) < 0.25) {
            emit(62, den);
        }
        if moved {
            emit(69, self.orb);
        }
        let source = orb.or(den);
        let offset = source.map_or(Vec2::ZERO, |p| p - position);
        let distance = offset.length();
        let intensity = if source.is_some() && distance.is_finite() {
            (-distance * distance / (2.0 * 0.14_f32.powi(2))).exp()
        } else {
            0.0
        };
        let old_intensity = self.signals.intensity;
        self.signals.intensity = intensity;
        self.signals.gradient = offset.normalize_or_zero() * intensity;
        self.signals.adaptation +=
            (intensity - self.signals.adaptation) * (1.0 - (-dt / 2.0).exp());
        self.novelty *= (-dt / 6.0).exp();
        if self.initialized
            && allowed
            && intensity > 0.12
            && (old_intensity <= 0.12 || (changed && orb.is_some()))
        {
            emit(if self.novelty > 0.3 { 68 } else { 67 }, source);
            self.sampled = true;
            self.weak_sampled = false;
            self.last_sample = position;
        }
        if self.initialized && allowed && intensity > 0.12 && !self.weak_sampled {
            if intensity < 0.35 {
                emit(64, source);
            } else if offset.y < -0.05 && c.support_confirmed() {
                emit(66, source);
            } else if offset.x.abs() > 0.05 {
                emit(65, source);
            }
            self.weak_sampled = true;
        }
        if self.sampled && intensity > 0.12 && position.distance(self.last_sample) > 0.06 {
            // Evidence is an actual new spatial sample, not invented travel.
            emit(63, source);
            self.last_sample = position;
        }
        self.refractory = (self.refractory - dt).max(0.0);
        let excitation = intensity * self.novelty;
        self.signals.irritation = (self.signals.irritation
            + dt * (excitation * 0.65 - 0.14 - self.signals.adaptation * 0.12))
            .clamp(0.0, 1.0);
        self.stage_seconds += dt;
        if !allowed {
            self.sneeze_stage = 0;
            self.signals.irritation *= (-dt / 0.5).exp();
        } else if self.sneeze_stage == 0
            && self.refractory == 0.0
            && self.signals.irritation >= 0.22
        {
            emit(71, source);
            self.sneeze_stage = 1;
            self.stage_seconds = 0.0;
        } else if self.sneeze_stage == 1
            && (intensity < 0.1 || (self.stage_seconds > 0.6 && self.signals.irritation < 0.12))
        {
            emit(73, source);
            self.sneeze_stage = 0;
            self.refractory = 20.0;
        } else if self.sneeze_stage == 1
            && self.signals.irritation >= 0.42
            && self.stage_seconds > 0.6
        {
            emit(72, source);
            self.sneeze_stage = 2;
            self.stage_seconds = 0.0;
            self.signals.irritation = 0.0;
            self.novelty *= 0.1;
        } else if self.sneeze_stage == 2 && self.stage_seconds > 1.2 {
            emit(74, None);
            self.sneeze_stage = 0;
            self.refractory = 20.0;
        }
        let contact = orb.is_some() && distance < if self.contact { 0.09 } else { 0.07 };
        if self.initialized && contact && !self.contact {
            // Contact deposits a bounded fictional residue, not dirt/illness.
            self.signals.contact_residue = (self.signals.contact_residue + 0.6).min(1.0);
            self.care_stage = 0;
        }
        self.signals.contact_residue *= (-dt / 8.0).exp();
        self.care_seconds += dt;
        if allowed && c.support_confirmed() && !contact {
            if self.care_stage == 0 && self.signals.contact_residue > 0.35 {
                emit(87, None);
                self.care_stage = 1;
                self.care_seconds = 0.0;
            } else if self.care_stage == 1 && self.care_seconds > 1.5 {
                emit(88, None);
                self.care_stage = 2;
                self.care_seconds = 0.0;
            } else if self.care_stage == 2 && self.care_seconds > 2.0 {
                // Retry only when the real controller could not deliver the
                // local care field: all slots busy, with residue remaining.
                if packet.fields.iter().all(Option::is_some) && self.signals.contact_residue > 0.2 {
                    emit(90, None);
                }
                self.care_stage = 3;
            }
        }
        self.orb = orb;
        self.orb_id = c.orb_id;
        self.den = den;
        self.contact = contact;
        self.initialized = true;
        events
    }
}
