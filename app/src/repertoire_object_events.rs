//! Measured orb events, separate from motor program/phase names. A MoveToward
//! command sets CarriedByPet before capture, so lifecycle alone is NOT a catch.
use glam::Vec2;
use pet_ecology::{
    EpisodeGoal, EpisodePhase, ObjectCommand, ObjectId, ObjectLifecycle, ObjectPhysicsConfig,
    PhysicalGrabFrame, WorldObject,
};
use pet_motor::RepertoireEvent;

pub const OBJECT_REPERTOIRE_IDS: &[u16] = &[
    55, 79, 122, 123, 124, 125, 126, 127, 128, 129, 130, 132, 135, 136, 137, 138, 139, 140, 141,
    142, 143, 144, 145, 146, 148, 149, 150, 169, 191, 192, 194, 195, 196, 198,
];

#[derive(Clone, Copy)]
struct Observation {
    penetration: f32,
    id: ObjectId,
    velocity: Vec2,
    position: Vec2,
    contact: bool,
    placing: bool,
    lifecycle: ObjectLifecycle,
    familiarity: f32,
}

#[derive(Default)]
pub struct RepertoireObjectEvents {
    pending_adaptation: Vec<u16>,
    throw_plan: Option<pet_ecology::OrbThrowPlan>,
    completed_shared_play: bool,
    last_play_impulse_age: Option<f32>,
    load_cooldown: f32,
    wall_transfer_reported: bool,
    previous: Option<Observation>,
    contact_age: f32,
    separation_age: f32,
    grip_confirmed: bool,
    incoming_speed: f32,
    far_pull_age: f32,
    far_pull_reported: bool,
    awaiting_user_return: bool,
    returning_user_ball: bool,
    bounce_cooldown: f32,
    config: Option<ObjectPhysicsConfig>,
    scene_goal: Option<EpisodeGoal>,
    scene_phase: Option<EpisodePhase>,
    previous_goal: Option<EpisodeGoal>,
    previous_phase: Option<EpisodePhase>,
    fatigue: f32,
}

impl RepertoireObjectEvents {
    pub fn note_adaptation_event(&mut self, id: u16) {
        if matches!(id, 191 | 192 | 195 | 196) && !self.pending_adaptation.contains(&id) {
            self.pending_adaptation.push(id);
        }
    }
    /// Supply the same authoritative plan as the episode, not a guessed style.
    pub fn set_throw_plan(&mut self, plan: Option<pet_ecology::OrbThrowPlan>) {
        self.throw_plan = plan;
    }
    /// A completed shared episode outcome, never an intention or elapsed timer.
    pub fn note_completed_shared_play(&mut self) {
        self.completed_shared_play = true;
    }
    pub fn set_scene_context(
        &mut self,
        config: ObjectPhysicsConfig,
        goal: Option<EpisodeGoal>,
        phase: Option<EpisodePhase>,
        fatigue: f32,
    ) {
        self.config = Some(config);
        self.scene_goal = goal;
        self.scene_phase = phase;
        self.fatigue = if fatigue.is_finite() {
            fatigue.clamp(0.0, 1.0)
        } else {
            0.0
        };
    }
    /// Input object is the post-command actual ecology state; physical contact
    /// was measured before the command. They refer to the same canonical orb.
    pub fn observe(
        &mut self,
        object: Option<&WorldObject>,
        physical: PhysicalGrabFrame,
        commands: &[ObjectCommand],
        pet_velocity: Vec2,
        dt: f32,
    ) -> Vec<RepertoireEvent> {
        let Some(object) = object.filter(|o| o.position.is_finite() && o.velocity.is_finite())
        else {
            *self = Self::default();
            return Vec::new();
        };
        let dt = if dt.is_finite() {
            dt.clamp(0.0, 0.1)
        } else {
            0.0
        };
        let contact = physical.contact || physical.swept_contact;
        let placing = commands.iter().any(|command| {
            matches!(command,
            ObjectCommand::Store { object_id, .. } if *object_id == object.id)
        });
        let next = Observation {
            penetration: physical.penetration_px,
            id: object.id,
            velocity: object.velocity,
            position: object.position,
            contact,
            placing,
            lifecycle: object.lifecycle,
            familiarity: object.familiarity,
        };
        let Some(old) = self.previous.filter(|old| old.id == object.id) else {
            *self = Self {
                throw_plan: self.throw_plan,
                config: self.config,
                scene_goal: self.scene_goal,
                scene_phase: self.scene_phase,
                fatigue: self.fatigue,
                ..Self::default()
            };
            self.previous = Some(next);
            self.incoming_speed = object.velocity.length();
            self.contact_age = 0.0;
            self.grip_confirmed = false;
            self.returning_user_ball = false;
            self.awaiting_user_return = false;
            self.previous_goal = self.scene_goal;
            self.previous_phase = self.scene_phase;
            return Vec::new();
        };
        let mut events = Vec::with_capacity(4);
        let previously_gripped = self.grip_confirmed;
        let mut emit = |id| {
            events.push(RepertoireEvent {
                id,
                confidence: 1.0,
                target: Some(object.position),
                side: (object.position.x - physical.socket_position.x).signum(),
            })
        };
        self.bounce_cooldown = (self.bounce_cooldown - dt).max(0.0);
        for id in self.pending_adaptation.drain(..) {
            emit(id);
        }
        self.load_cooldown = (self.load_cooldown - dt).max(0.0);
        if self.completed_shared_play {
            emit(169);
            self.completed_shared_play = false;
        }
        if let Some(age) = &mut self.last_play_impulse_age {
            let old_age = *age;
            *age += dt;
            if old_age < 0.65
                && *age >= 0.65
                && self.fatigue >= 0.65
                && contact
                && self.scene_goal == Some(EpisodeGoal::SoloOrbPlay)
            {
                emit(194);
            }
        }
        if previously_gripped
            && contact
            && physical.penetration_px.is_finite()
            && old.penetration.is_finite()
            && physical.penetration_px - old.penetration > 1.5
            && self.load_cooldown <= 0.0
        {
            emit(127);
            self.load_cooldown = 1.0;
        }
        if previously_gripped && self.scene_goal == Some(EpisodeGoal::SoloOrbPlay)
            && self.scene_phase == Some(EpisodePhase::Prepare)
            && self.previous_phase != Some(EpisodePhase::Prepare)
            && commands.iter().any(|c| matches!(c, ObjectCommand::MoveToward {object_id, ..} if *object_id == object.id)) {
            emit(79);
            emit(129);
            if let Some(plan) = self.throw_plan {
                emit(if plan.strong { 137 } else { 136 });
                if plan.clearance_limited { emit(139); }
            }
        }
        if self.scene_goal == Some(EpisodeGoal::SoloOrbPlay)
            && self.scene_phase == Some(EpisodePhase::Manipulate)
            && self.previous_phase == Some(EpisodePhase::Evaluate)
            && contact && object.velocity.length() < 0.03
            && commands.iter().any(|c| matches!(c, ObjectCommand::MoveToward {object_id, ..} if *object_id == object.id)) {
            emit(135);
        }
        if !previously_gripped {
            self.wall_transfer_reported = false;
        }
        if previously_gripped && !self.wall_transfer_reported {
            for command in commands {
                if let ObjectCommand::MoveToward {
                    object_id, target, ..
                } = command
                    && *object_id == object.id
                    && target.is_finite()
                {
                    let shift = *target - physical.socket_position;
                    if shift.length() > 0.005
                        && shift.x * (0.5 - physical.socket_position.x) > 0.0
                        && (physical.socket_position.x < 0.065
                            || physical.socket_position.x > 0.935)
                        && (object.position - old.position).dot(shift) > 0.000001
                    {
                        emit(128);
                        self.wall_transfer_reported = true;
                    }
                }
            }
        }
        // Re-run the existing physical predictor, including gravity, drag and
        // radius-aware desktop collisions. Uncommanded direction reversal is
        // evidence of a bounce; agreement determines routine versus surprising.
        if let Some(config) = self
            .config
            .filter(|c| c.desktop_aspect.is_finite() && c.reference_height_px.is_finite())
            && dt > 0.0
            && old.lifecycle == ObjectLifecycle::Free
            && object.lifecycle == ObjectLifecycle::Free
            && !contact
            && !old.contact
            && commands.is_empty()
            && self.bounce_cooldown <= 0.0
            && old.velocity.length() > 0.08
            && object.velocity.length() > 0.06
            && old
                .velocity
                .normalize_or_zero()
                .dot(object.velocity.normalize_or_zero())
                < 0.25
        {
            let mut predicted = object.clone();
            predicted.position = old.position;
            predicted.velocity = old.velocity;
            let steps = (dt * 120.0).ceil().max(1.0) as u32;
            for _ in 0..steps {
                pet_ecology::step_object(&mut predicted, config, dt / steps as f32);
            }
            let delta = predicted.position - object.position;
            let position_error = Vec2::new(delta.x * config.desktop_aspect, delta.y).length();
            let routine =
                position_error < 0.015 && (predicted.velocity - object.velocity).length() < 0.06;
            emit(if routine { 142 } else { 143 });
            self.bounce_cooldown = 0.8;
        }
        if self.scene_goal == Some(EpisodeGoal::SoloOrbPlay)
            && self.previous_goal == Some(EpisodeGoal::OfferOrb)
            && self.previous_phase == Some(EpisodePhase::WaitForUser)
        {
            emit(149);
        }
        if contact && !old.contact && object.lifecycle != ObjectLifecycle::GrabbedByUser {
            emit(122);
            self.incoming_speed = old.velocity.length();
        }
        let aspect = self.config.map_or(1.0, |c| {
            if c.desktop_aspect.is_finite() {
                c.desktop_aspect.clamp(0.1, 10.0)
            } else {
                1.0
            }
        });
        let to_socket = (physical.socket_position - object.position) * Vec2::new(aspect, 1.0);
        let socket_close = to_socket.is_finite() && to_socket.length() < 0.04;
        let matched_speed =
            pet_velocity.is_finite() && (object.velocity - pet_velocity).length() < 0.05;
        let carried = object.lifecycle == ObjectLifecycle::CarriedByPet;
        // Existing real command + observed motion toward the socket, not an
        // invented force sensor. Sustain and hysteresis reject distance noise.
        let pulling = carried
            && to_socket.is_finite()
            && to_socket.length() > 0.08
            && object.velocity.dot(to_socket) > 0.001
            && commands.iter().any(|command| {
                matches!(command,
                ObjectCommand::MoveToward { object_id, .. } if *object_id == object.id)
            });
        self.far_pull_age = if pulling { self.far_pull_age + dt } else { 0.0 };
        if !carried || to_socket.length() < 0.05 {
            self.far_pull_reported = false;
        }
        if self.far_pull_age >= 0.2 && !self.far_pull_reported {
            emit(125);
            self.far_pull_reported = true;
        }
        let applied_nudge = object.lifecycle == ObjectLifecycle::Free
            && matches!(
                old.lifecycle,
                ObjectLifecycle::Free | ObjectLifecycle::Sleeping
            )
            && old.velocity.length() < 0.015
            && commands.iter().any(|command| {
                matches!(command,
                ObjectCommand::ApplyImpulse { object_id, impulse }
                    if *object_id == object.id && impulse.is_finite()
                    && (0.001..=0.12).contains(&impulse.length())
                    && (object.velocity - old.velocity).dot(*impulse) > 0.0001)
            });
        if applied_nudge {
            emit(141);
        }
        if contact
            && object.lifecycle == ObjectLifecycle::Free
            && self.scene_goal == Some(EpisodeGoal::SoloOrbPlay)
        {
            for command in commands {
                if let ObjectCommand::ApplyImpulse { object_id, impulse } = command
                    && *object_id == object.id
                    && impulse.is_finite()
                    && (object.velocity - old.velocity).dot(*impulse) > 0.0001
                {
                    self.last_play_impulse_age = Some(0.0);
                    let aspect = self.config.map_or(1.0, |c| c.desktop_aspect.max(0.1));
                    let wall_distance = object.position.x.min(1.0 - object.position.x) * aspect;
                    let toward_wall = impulse.x * (object.position.x - 0.5) > 0.0;
                    if wall_distance < 0.07 && toward_wall && impulse.length() <= 0.071 {
                        emit(145);
                    } else if 1.0 - object.position.y < 0.06
                        && impulse.y.abs() < 0.001
                        && wall_distance >= 0.07
                        && impulse.length() <= 0.11
                    {
                        emit(144);
                    }
                }
            }
        }
        // A returned pass requires an earlier measured handoff, a user-held ->
        // free edge, and an incoming trajectory. Dropping away is not a reply.
        let pet_release = commands.iter().any(|command| {
            matches!(command,
            ObjectCommand::Release { object_id, .. } | ObjectCommand::ApplyImpulse { object_id, .. }
                if *object_id == object.id)
        });
        if self.awaiting_user_return
            && old.lifecycle == ObjectLifecycle::GrabbedByUser
            && object.lifecycle == ObjectLifecycle::Free
            && !pet_release
        {
            if object.velocity.length() > 0.06
                && to_socket.is_finite()
                && object
                    .velocity
                    .normalize_or_zero()
                    .dot(to_socket.normalize_or_zero())
                    > 0.65
            {
                emit(148);
                self.returning_user_ball = true;
            }
            self.awaiting_user_return = false;
        }
        // Familiarity already accumulates in ecology from real interactions.
        // This only expresses threshold crossing; it does not invent learning.
        if contact
            && old.familiarity.is_finite()
            && object.familiarity.is_finite()
            && old.familiarity < 0.65
            && object.familiarity >= 0.65
        {
            emit(198);
        }
        if carried && contact && socket_close && matched_speed {
            self.contact_age += dt;
            self.separation_age = 0.0;
            if !self.grip_confirmed && self.contact_age >= 0.15 {
                self.grip_confirmed = true;
                emit(124);
                if self.returning_user_ball {
                    emit(55);
                    self.returning_user_ball = false;
                }
                if self.incoming_speed > 0.08 {
                    emit(123);
                    emit(146);
                }
            }
        } else {
            self.contact_age = 0.0;
            self.separation_age += dt;
        }
        // A real release command must be reflected in the post-command state;
        // an attempted/rejected release cannot claim a throw or follow-through.
        let released = commands.iter().any(|command| {
            matches!(command,
            ObjectCommand::Release { object_id, velocity }
                if *object_id == object.id && velocity.is_finite()
                && object.lifecycle == ObjectLifecycle::Free
                && object.velocity.length() > 0.03)
        });
        if self.grip_confirmed && released {
            emit(140);
            if object.velocity.y < -0.08 {
                emit(138);
            }
            self.grip_confirmed = false;
        } else if self.grip_confirmed && object.lifecycle == ObjectLifecycle::GrabbedByUser {
            emit(130);
            self.awaiting_user_return = true;
            self.grip_confirmed = false;
        } else if self.grip_confirmed && carried && self.separation_age >= 0.2 {
            // Lost sustained contact while the actual grip still tries to hold.
            // This nominates a pause/expression, never teleports or regrips.
            emit(126);
            self.grip_confirmed = false;
        } else if !carried {
            self.grip_confirmed = false;
        }
        if placing && !old.placing && carried && self.grip_confirmed {
            emit(132);
        }
        if placing
            && previously_gripped
            && old.lifecycle == ObjectLifecycle::CarriedByPet
            && object.lifecycle == ObjectLifecycle::StoredInDen
            && self.fatigue >= 0.65
        {
            emit(150);
        }
        self.previous = Some(next);
        self.previous_goal = self.scene_goal;
        self.previous_phase = self.scene_phase;
        events
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn object_grip_distance_uses_same_physical_units_on_portrait_and_wide_desktops() {
        for aspect in [0.5, 1.0, 3.0] {
            for (gap, expected) in [(0.03, true), (0.05, false)] {
                let (mut orb, mut physical) = fixture();
                physical.socket_position.x = orb.position.x + gap / aspect;
                let mut adapter = RepertoireObjectEvents::default();
                adapter.set_scene_context(
                    ObjectPhysicsConfig {
                        desktop_aspect: aspect,
                        ..Default::default()
                    },
                    None,
                    None,
                    0.0,
                );
                adapter.observe(Some(&orb), physical, &[], Vec2::ZERO, 0.05);
                orb.lifecycle = ObjectLifecycle::CarriedByPet;
                for _ in 0..4 {
                    adapter.observe(Some(&orb), physical, &[], Vec2::ZERO, 0.05);
                }
                assert_eq!(
                    adapter.grip_confirmed, expected,
                    "aspect={aspect}, gap={gap}"
                );
            }
        }
    }
    #[test]
    fn real_throw_preparation_and_loaded_grip_are_not_bare_phase_labels() {
        let (mut orb, physical) = fixture();
        let mut adapter = RepertoireObjectEvents::default();
        confirm(&mut adapter, &mut orb, physical);
        adapter.set_scene_context(
            ObjectPhysicsConfig::default(),
            Some(EpisodeGoal::SoloOrbPlay),
            Some(EpisodePhase::Manipulate),
            0.0,
        );
        adapter.observe(Some(&orb), physical, &[], Vec2::ZERO, 0.05);
        adapter.set_throw_plan(Some(pet_ecology::OrbThrowPlan {
            velocity: Vec2::X * 0.1,
            strong: true,
            clearance_limited: true,
        }));
        adapter.set_scene_context(
            ObjectPhysicsConfig::default(),
            Some(EpisodeGoal::SoloOrbPlay),
            Some(EpisodePhase::Prepare),
            0.0,
        );
        let command = ObjectCommand::MoveToward {
            object_id: orb.id,
            target: physical.socket_position,
            speed: 3.0,
        };
        let events = adapter.observe(Some(&orb), physical, &[command], Vec2::ZERO, 0.05);
        for id in [79, 129, 137, 139] {
            assert!(events.iter().any(|e| e.id == id), "{id}");
        }
        assert!(
            !adapter
                .observe(Some(&orb), physical, &[command], Vec2::ZERO, 0.05)
                .iter()
                .any(|e| e.id == 79)
        );
        let loaded = PhysicalGrabFrame {
            penetration_px: physical.penetration_px + 2.0,
            ..physical
        };
        assert!(
            adapter
                .observe(Some(&orb), loaded, &[], Vec2::ZERO, 0.05)
                .iter()
                .any(|e| e.id == 127)
        );
        adapter.note_completed_shared_play();
        assert!(
            adapter
                .observe(Some(&orb), loaded, &[], Vec2::ZERO, 0.05)
                .iter()
                .any(|e| e.id == 169)
        );
        assert!(
            !adapter
                .observe(Some(&orb), loaded, &[], Vec2::ZERO, 0.05)
                .iter()
                .any(|e| e.id == 169)
        );
    }
    #[test]
    fn object_roll_and_wall_probe_require_confirmed_velocity_change() {
        for (position, impulse, expected) in [
            (Vec2::new(0.4, 0.97), Vec2::new(0.10, 0.0), 144),
            (Vec2::new(0.02, 0.5), Vec2::new(-0.065, 0.0), 145),
        ] {
            let (mut orb, mut physical) = fixture();
            orb.position = position;
            orb.velocity = Vec2::ZERO;
            orb.lifecycle = ObjectLifecycle::Free;
            physical.socket_position = position;
            let mut adapter = RepertoireObjectEvents::default();
            adapter.set_scene_context(
                ObjectPhysicsConfig::default(),
                Some(EpisodeGoal::SoloOrbPlay),
                Some(EpisodePhase::Execute),
                0.0,
            );
            adapter.observe(Some(&orb), physical, &[], Vec2::ZERO, 0.05);
            let command = ObjectCommand::ApplyImpulse {
                object_id: orb.id,
                impulse,
            };
            assert!(
                !adapter
                    .observe(Some(&orb), physical, &[command], Vec2::ZERO, 0.05)
                    .iter()
                    .any(|e| e.id == expected)
            );
            orb.velocity = impulse;
            assert!(
                adapter
                    .observe(Some(&orb), physical, &[command], Vec2::ZERO, 0.05)
                    .iter()
                    .any(|e| e.id == expected)
            );
        }
    }
    #[test]
    fn integrated_coverage_counts_only_declared_producers() {
        let mut ids = pet_motor::autonomous_repertoire_coverage();
        ids.extend_from_slice(super::OBJECT_REPERTOIRE_IDS);
        ids.extend_from_slice(crate::repertoire_perception_events::PERCEPTION_REPERTOIRE_IDS);
        ids.extend_from_slice(crate::repertoire_learning_events::LEARNING_REPERTOIRE_IDS);
        ids.extend_from_slice(crate::surface_care_runtime::INTEGRATED_SURFACE_CARE_IDS);
        // Ordinary blink is already executed by BlinkController and observed
        // in NervousSystemRuntime. Never enqueue it as a second blink recipe.
        ids.push(21);
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), 196);
        let missing: Vec<_> = (1..=200).filter(|id| !ids.contains(id)).collect();
        assert_eq!(missing, [4, 97, 185, 186]);
        eprintln!(
            "Integrated causes {}/200; still missing {:?}",
            ids.len(),
            missing
        );
    }

    use super::*;
    #[test]
    fn object_repertoire_tired_finish_requires_completed_storage() {
        let (mut orb, physical) = fixture();
        let mut adapter = RepertoireObjectEvents::default();
        confirm(&mut adapter, &mut orb, physical);
        adapter.set_scene_context(
            ObjectPhysicsConfig::default(),
            Some(EpisodeGoal::CarryOrbHome),
            Some(EpisodePhase::Complete),
            0.8,
        );
        let store = ObjectCommand::Store {
            object_id: orb.id,
            slot: 0,
        };
        assert!(
            !adapter
                .observe(Some(&orb), physical, &[store], Vec2::ZERO, 0.05)
                .iter()
                .any(|e| e.id == 150)
        );
        orb.lifecycle = ObjectLifecycle::StoredInDen;
        assert!(
            adapter
                .observe(Some(&orb), physical, &[store], Vec2::ZERO, 0.05)
                .iter()
                .any(|e| e.id == 150)
        );
        assert!(
            !adapter
                .observe(Some(&orb), physical, &[store], Vec2::ZERO, 0.05)
                .iter()
                .any(|e| e.id == 150)
        );
    }
    #[test]
    fn object_repertoire_return_joy_requires_actual_recapture() {
        let (mut orb, physical) = fixture();
        let mut adapter = RepertoireObjectEvents::default();
        confirm(&mut adapter, &mut orb, physical);
        orb.lifecycle = ObjectLifecycle::GrabbedByUser;
        adapter.observe(Some(&orb), physical, &[], Vec2::ZERO, 0.05);
        orb.lifecycle = ObjectLifecycle::Free;
        orb.position.x = 0.3;
        orb.velocity.x = 0.2;
        assert!(
            !adapter
                .observe(Some(&orb), physical, &[], Vec2::ZERO, 0.05)
                .iter()
                .any(|e| e.id == 55)
        );
        orb.lifecycle = ObjectLifecycle::CarriedByPet;
        orb.position = physical.socket_position;
        orb.velocity = Vec2::ZERO;
        let mut joys = 0;
        for _ in 0..20 {
            joys += adapter
                .observe(Some(&orb), physical, &[], Vec2::ZERO, 0.05)
                .iter()
                .filter(|e| e.id == 55)
                .count();
        }
        assert_eq!(joys, 1);
    }
    #[test]
    fn object_repertoire_bounce_uses_previous_trajectory_and_rejects_commands() {
        for unexpected in [false, true] {
            let (mut orb, mut physical) = fixture();
            physical.contact = false;
            let config = ObjectPhysicsConfig::default();
            orb.position = Vec2::new(
                0.5,
                1.0 - orb.radius_px_at_reference / config.reference_height_px - 0.001,
            );
            orb.velocity = Vec2::new(0.0, 0.4);
            let mut adapter = RepertoireObjectEvents::default();
            adapter.set_scene_context(config, None, None, 0.0);
            adapter.observe(Some(&orb), physical, &[], Vec2::ZERO, 0.05);
            for _ in 0..6 {
                pet_ecology::step_object(&mut orb, config, 1.0 / 120.0);
            }
            if unexpected {
                orb.velocity.x = 0.35;
            }
            let events = adapter.observe(Some(&orb), physical, &[], Vec2::ZERO, 0.05);
            assert_eq!(events.iter().any(|e| e.id == 142), !unexpected);
            assert_eq!(events.iter().any(|e| e.id == 143), unexpected);
            assert!(
                adapter
                    .observe(Some(&orb), physical, &[], Vec2::ZERO, 0.05)
                    .is_empty()
            );
        }
    }

    #[test]
    fn object_repertoire_solo_transition_requires_actual_waiting_episode() {
        let (orb, physical) = fixture();
        let mut adapter = RepertoireObjectEvents::default();
        adapter.set_scene_context(
            ObjectPhysicsConfig::default(),
            Some(EpisodeGoal::OfferOrb),
            Some(EpisodePhase::WaitForUser),
            0.0,
        );
        adapter.observe(Some(&orb), physical, &[], Vec2::ZERO, 0.05);
        adapter.set_scene_context(
            ObjectPhysicsConfig::default(),
            Some(EpisodeGoal::SoloOrbPlay),
            Some(EpisodePhase::Prepare),
            0.0,
        );
        assert!(
            adapter
                .observe(Some(&orb), physical, &[], Vec2::ZERO, 0.05)
                .iter()
                .any(|e| e.id == 149)
        );
        assert!(
            !adapter
                .observe(Some(&orb), physical, &[], Vec2::ZERO, 0.05)
                .iter()
                .any(|e| e.id == 149)
        );
    }
    fn fixture() -> (WorldObject, PhysicalGrabFrame) {
        let mut orb = WorldObject::canonical_orb(42, Vec2::splat(0.5));
        orb.position = Vec2::splat(0.5);
        let physical = PhysicalGrabFrame {
            socket_position: orb.position,
            contact: true,
            ..Default::default()
        };
        (orb, physical)
    }
    fn confirm(
        adapter: &mut RepertoireObjectEvents,
        orb: &mut WorldObject,
        physical: PhysicalGrabFrame,
    ) {
        adapter.observe(Some(orb), physical, &[], Vec2::ZERO, 0.05);
        orb.lifecycle = ObjectLifecycle::CarriedByPet;
        for _ in 0..4 {
            adapter.observe(Some(orb), physical, &[], Vec2::ZERO, 0.05);
        }
        assert!(adapter.grip_confirmed);
    }
    #[test]
    fn object_repertoire_far_pull_needs_sustained_real_motion() {
        let (mut orb, physical) = fixture();
        let mut adapter = RepertoireObjectEvents::default();
        adapter.observe(Some(&orb), physical, &[], Vec2::ZERO, 0.05);
        orb.lifecycle = ObjectLifecycle::CarriedByPet;
        orb.position.x = 0.35;
        let command = ObjectCommand::MoveToward {
            object_id: orb.id,
            target: physical.socket_position,
            speed: 0.1,
        };
        for _ in 0..10 {
            assert!(
                !adapter
                    .observe(Some(&orb), physical, &[command], Vec2::ZERO, 0.05)
                    .iter()
                    .any(|e| e.id == 125)
            );
        }
        orb.velocity.x = 0.1;
        let mut count = 0;
        for _ in 0..30 {
            count += adapter
                .observe(Some(&orb), physical, &[command], Vec2::ZERO, 0.05)
                .iter()
                .filter(|e| e.id == 125)
                .count();
        }
        assert_eq!(count, 1);
    }

    #[test]
    fn object_repertoire_nudge_requires_applied_impulse_not_command_alone() {
        let (mut orb, physical) = fixture();
        let mut adapter = RepertoireObjectEvents::default();
        adapter.observe(Some(&orb), physical, &[], Vec2::ZERO, 0.05);
        let command = ObjectCommand::ApplyImpulse {
            object_id: orb.id,
            impulse: Vec2::new(0.05, 0.0),
        };
        assert!(
            adapter
                .observe(Some(&orb), physical, &[command], Vec2::ZERO, 0.05)
                .is_empty()
        );
        orb.velocity.x = 0.1;
        assert!(
            adapter
                .observe(Some(&orb), physical, &[command], Vec2::ZERO, 0.05)
                .iter()
                .any(|e| e.id == 141)
        );
        assert!(
            !adapter
                .observe(Some(&orb), physical, &[command], Vec2::ZERO, 0.05)
                .iter()
                .any(|e| e.id == 141)
        );
    }

    #[test]
    fn object_repertoire_rally_requires_confirmed_handoff_and_incoming_return() {
        for toward_pet in [false, true] {
            let (mut orb, physical) = fixture();
            let mut adapter = RepertoireObjectEvents::default();
            confirm(&mut adapter, &mut orb, physical);
            orb.lifecycle = ObjectLifecycle::GrabbedByUser;
            adapter.observe(Some(&orb), physical, &[], Vec2::ZERO, 0.05);
            orb.position.x = 0.3;
            orb.lifecycle = ObjectLifecycle::Free;
            orb.velocity.x = if toward_pet { 0.15 } else { -0.15 };
            let events = adapter.observe(Some(&orb), physical, &[], Vec2::ZERO, 0.05);
            assert_eq!(events.iter().any(|e| e.id == 148), toward_pet);
        }
    }

    #[test]
    fn object_repertoire_familiarity_is_observed_once_not_synthesized() {
        let (mut orb, physical) = fixture();
        let mut adapter = RepertoireObjectEvents::default();
        orb.familiarity = 0.64;
        adapter.observe(Some(&orb), physical, &[], Vec2::ZERO, 0.05);
        orb.familiarity = 0.66;
        assert!(
            adapter
                .observe(Some(&orb), physical, &[], Vec2::ZERO, 0.05)
                .iter()
                .any(|e| e.id == 198)
        );
        assert!(
            adapter
                .observe(Some(&orb), physical, &[], Vec2::ZERO, 0.05)
                .is_empty()
        );
    }

    #[test]
    fn object_repertoire_carried_label_without_contact_is_not_capture() {
        let (mut orb, mut physical) = fixture();
        physical.contact = false;
        let mut adapter = RepertoireObjectEvents::default();
        adapter.observe(Some(&orb), physical, &[], Vec2::ZERO, 0.05);
        orb.lifecycle = ObjectLifecycle::CarriedByPet;
        for _ in 0..100 {
            assert!(
                adapter
                    .observe(Some(&orb), physical, &[], Vec2::ZERO, 0.05)
                    .is_empty()
            );
        }
    }
    #[test]
    fn object_repertoire_confirmed_grip_and_user_take_are_once_only() {
        let (mut orb, physical) = fixture();
        let mut adapter = RepertoireObjectEvents::default();
        confirm(&mut adapter, &mut orb, physical);
        assert!(
            adapter
                .observe(Some(&orb), physical, &[], Vec2::ZERO, 0.05)
                .is_empty()
        );
        orb.lifecycle = ObjectLifecycle::GrabbedByUser;
        let events = adapter.observe(Some(&orb), physical, &[], Vec2::ZERO, 0.05);
        assert_eq!(events.iter().map(|e| e.id).collect::<Vec<_>>(), vec![130]);
        assert!(
            adapter
                .observe(Some(&orb), physical, &[], Vec2::ZERO, 0.05)
                .is_empty()
        );
    }
    #[test]
    fn object_repertoire_throw_requires_measured_grip_and_applied_release() {
        let (mut orb, physical) = fixture();
        let mut adapter = RepertoireObjectEvents::default();
        confirm(&mut adapter, &mut orb, physical);
        let command = ObjectCommand::Release {
            object_id: orb.id,
            velocity: Vec2::new(0.1, -0.2),
        };
        assert!(
            adapter
                .observe(Some(&orb), physical, &[command], Vec2::ZERO, 0.05)
                .is_empty()
        );
        orb.lifecycle = ObjectLifecycle::Free;
        orb.velocity = Vec2::new(0.1, -0.2);
        let events = adapter.observe(Some(&orb), physical, &[command], Vec2::ZERO, 0.05);
        assert!(events.iter().any(|e| e.id == 140));
        assert!(events.iter().any(|e| e.id == 138));
    }
    #[test]
    fn object_repertoire_separation_and_object_identity_are_debounced() {
        let (mut orb, mut physical) = fixture();
        let mut adapter = RepertoireObjectEvents::default();
        confirm(&mut adapter, &mut orb, physical);
        physical.contact = false;
        for _ in 0..3 {
            assert!(
                adapter
                    .observe(Some(&orb), physical, &[], Vec2::ZERO, 0.05)
                    .is_empty()
            );
        }
        assert!(
            adapter
                .observe(Some(&orb), physical, &[], Vec2::ZERO, 0.05)
                .iter()
                .any(|e| e.id == 126)
        );
        assert!(
            adapter
                .observe(None, physical, &[], Vec2::ZERO, 0.05)
                .is_empty()
        );
        assert!(!adapter.grip_confirmed);
    }
}
