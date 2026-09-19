//! Event-driven, bounded expressive micro-recipes. This layer cannot write a
//! root transform, velocity, support command, object grip or persistent need.
use crate::repertoire_catalog::{
    CHANNEL_BODY, CHANNEL_BREATH, CHANNEL_GAZE, CHANNEL_LIDS, REPERTOIRE_COUNT, RepertoireEffect,
    repertoire_recipe,
};
use crate::{
    BehaviorContextFrame, FieldSpace, LocalSomaticField, MotorWorldEvent, MotorWorldGoal,
    SomaticActuationPacket, SomaticFieldKind,
};
use glam::Vec2;
use lifecore::{ActionId, BehaviorGoalFrame, PrimaryIntent};
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RepertoireEvent {
    /// One-based ID from ORGANIC_BEHAVIOR_REPERTOIRE.md. External producers must
    /// supply an actual event, not a continuously asserted animation request.
    pub id: u16,
    pub confidence: f32,
    pub target: Option<Vec2>,
    pub side: f32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub enum RepertoirePhase {
    #[default]
    Idle,
    Prepare,
    Express,
    Recover,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub enum RepertoireReason {
    #[default]
    NoEvent,
    ContextEvent,
    ExternalEvent,
    ActiveCommitment,
    Completed,
    Cooldown,
    InvalidEvidence,
    SafetySuppressed,
    SleepSuppressed,
    ResourceBusy,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct RepertoireBlinkRequest {
    pub strength: f32,
    pub duration_seconds: f32,
    pub sleep_check: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
pub struct RepertoireOutput {
    pub effect: RepertoireEffect,
    pub active_id: Option<u16>,
    pub phase: RepertoirePhase,
    pub reason: RepertoireReason,
    /// Fraction of the single-accent budget currently used, [0,1].
    pub budget: f32,
    pub gaze_target: Option<Vec2>,
    /// Routed to the existing blink owner on onset, never applied to eyelids.
    pub blink_request: Option<RepertoireBlinkRequest>,
    /// A bounded local residual only; the base program retains support/root.
    pub field: Option<LocalSomaticField>,
}

#[derive(Debug, Clone)]
struct Observation {
    program: Option<crate::BehaviorProgramId>,
    cursor_velocity: Vec2,
    den: Option<Vec2>,
    fatigue: f32,
    pleasantness: f32,
    supported_seconds: f32,
    surface_speed: f32,
    orb: Option<Vec2>,
    orb_velocity: Vec2,
    orb_id: Option<u64>,
    stored: bool,
    held: bool,
    near_orb: bool,
    near_den: bool,
    supported: bool,
    sleeping: bool,
    touched: bool,
    dragged: bool,
    cursor_near: bool,
    stress: f32,
    startle: f32,
    error: f32,
    primary: PrimaryIntent,
    world_goal: MotorWorldGoal,
    world_event: MotorWorldEvent,
    surface_count: usize,
    phase: String,
}

#[derive(Debug, Clone, Default)]
pub struct RepertoireContextAdapter {
    virtual_interoception: crate::VirtualInteroception,
    previous: Option<Observation>,
    seed: u64,
    event_sequence: u64,
    last_variant: u16,
    repeated_failures: u8,
    stuck_seconds: f32,
    loaded_carry_seconds: f32,
    post_carry_release_seconds: f32,
    irritation_blink_latched: bool,
    initiative_charge: f32,
    initiative_pause: f32,
    previous_initiative: u16,
}

impl RepertoireContextAdapter {
    /// Scene differences and the explicit bounded virtual chemistry simulation
    /// cause events. No real-world scent, illness or user emotion is inferred.
    pub fn observe(
        &mut self,
        goal: &BehaviorGoalFrame,
        context: &BehaviorContextFrame,
        packet: &SomaticActuationPacket,
        dt: f32,
    ) -> Vec<RepertoireEvent> {
        let dt = safe_dt(dt);
        let position = context.body.motion.world_position;
        let orb = context.orb_position.filter(|p| p.is_finite());
        let den = context.den_anchor.filter(|p| p.is_finite());
        let same_orb = self
            .previous
            .as_ref()
            .is_some_and(|p| p.orb_id == context.orb_id);
        let orb_velocity = if same_orb && dt >= 0.001 {
            self.previous
                .as_ref()
                .and_then(|p| p.orb)
                .zip(orb)
                .map_or(Vec2::ZERO, |(old, new)| {
                    ((new - old) / dt).clamp_length_max(3.0)
                })
        } else {
            Vec2::ZERO
        };
        let next = Observation {
            program: packet.program,
            cursor_velocity: context.cursor_velocity,
            den,
            fatigue: goal.derived.fatigue,
            pleasantness: goal.felt.contact_pleasantness,
            supported_seconds: context
                .somatic
                .supported_seconds
                .max(context.screen_edge_support_stable_seconds),
            surface_speed: packet
                .support
                .as_ref()
                .and_then(|support| {
                    context
                        .surfaces
                        .iter()
                        .find(|s| s.surface_id == support.surface_id)
                })
                .map_or(0.0, |s| {
                    if s.velocity.is_finite() {
                        s.velocity.length()
                    } else {
                        0.0
                    }
                }),
            orb,
            orb_velocity,
            orb_id: context.orb_id,
            stored: context.orb_stored,
            held: context.orb_user_held,
            near_orb: orb.is_some_and(|p| p.distance(position) < 0.16),
            near_den: den.is_some_and(|p| p.distance(position) < 0.16),
            supported: context.support_confirmed(),
            sleeping: goal.action == ActionId::Sleep
                || context.companion_intent == PrimaryIntent::Sleep,
            touched: context.pet_touched,
            dragged: context.pet_dragged,
            cursor_near: context.cursor_position.is_finite()
                && context.cursor_position.distance(position) < 0.14,
            stress: goal.affect.stress,
            startle: goal.felt.startle,
            error: context.somatic.motor_error,
            primary: context.companion_intent,
            world_goal: context.world_goal,
            world_event: context.world_event,
            surface_count: context.surfaces.len(),
            phase: packet.phase_name.clone(),
        };
        let mut events = Vec::with_capacity(12);
        let mut emit = |id, target: Option<Vec2>, confidence| {
            if events.len() < 24 {
                events.push(RepertoireEvent {
                    id,
                    confidence,
                    target,
                    side: target.map_or(0.0, |p| (p.x - position.x).signum()),
                });
            }
        };
        if let Some(old) = &self.previous {
            if next.sleeping && !old.sleeping {
                emit(112, None, 1.0);
            }
            if !next.sleeping && old.sleeping {
                emit(118, orb.or(den), 1.0);
            }
            if next.sleeping {
                // A real touch is a quiet sleeping-body response, NOT a wake
                // instruction, ordinary gaze shift or eyelid opening request.
                if next.touched && !old.touched {
                    emit(115, None, 0.8);
                }
                if next.phase != old.phase {
                    match next.phase.as_str() {
                        "nrem_hold" => emit(113, None, 0.95),
                        "structured_local_twitches" => emit(114, None, 0.8),
                        "micro_arousal_or_continue" => emit(115, None, 0.75),
                        _ => {}
                    }
                }
            } else {
                // A scheduler bout renewal is not a new visible action. Only
                // an actual program/phase edge may replay its accompaniment;
                // contact, target and intent changes have their own evidence
                // below. Otherwise an unchanged pose becomes a periodic nod
                // as soon as the recipe's cooldown expires.
                if old.program != next.program || old.phase != next.phase {
                    for &id in phase_accompaniments(packet, goal, context) {
                        emit(id, orb.or(goal.body_intent.gaze_target), 0.85);
                    }
                }
                if next.touched
                    && old.touched
                    && next.cursor_velocity.length() > 0.02
                    && old.cursor_velocity.length() > 0.02
                    && next.cursor_velocity.dot(old.cursor_velocity) < 0.0
                {
                    emit(152, Some(context.body.contact.point_world), 0.9);
                }
                if next.touched
                    && old.cursor_velocity.length() < 0.3
                    && next.cursor_velocity.length() > 0.8
                {
                    emit(154, Some(context.body.contact.point_world), 0.9);
                }
                if old.near_orb && !next.near_orb && next.orb.is_some() {
                    emit(12, orb, 0.85);
                }
                if old
                    .den
                    .zip(next.den)
                    .is_some_and(|(a, b)| a.distance(b) > 0.02)
                {
                    emit(184, den, 0.9);
                }
                if next.supported && old.surface_speed < 0.025 && next.surface_speed > 0.04 {
                    emit(if next.surface_speed > 0.25 { 190 } else { 189 }, None, 0.8);
                }
                if next.fatigue > 0.55 && old.fatigue <= 0.55 {
                    emit(if next.supported { 106 } else { 25 }, None, 0.85);
                }
                if next.supported && old.supported_seconds < 30.0 && next.supported_seconds >= 30.0
                {
                    emit(104, None, 0.8);
                }
                if next.supported
                    && old.supported_seconds < 120.0
                    && next.supported_seconds >= 120.0
                {
                    emit(84, None, 0.8);
                }
                if next.touched && next.pleasantness > 0.65 && old.pleasantness <= 0.65 {
                    emit(156, Some(context.body.contact.point_world), 0.85);
                }
                if old.primary == PrimaryIntent::GroomSelf && next.primary != old.primary {
                    emit(89, None, 0.75);
                }
                if next.near_orb && !old.near_orb {
                    emit(1, orb, 0.9);
                }
                if next.orb.is_some() && (old.orb.is_none() || !same_orb) {
                    emit(61, orb, 0.8);
                }
                if old.orb.is_some() && next.orb.is_none() {
                    emit(176, old.orb, 0.9);
                }
                if same_orb && next.orb.is_some() {
                    let old_speed = old.orb_velocity.length();
                    let speed = orb_velocity.length();
                    if old_speed < 0.025 && speed > 0.08 {
                        emit(2, orb, 0.9);
                    }
                    if old_speed > 0.08 && speed < 0.025 {
                        emit(13, orb, 0.8);
                    }
                    if old_speed > 0.08 && speed > 0.08 && old.orb_velocity.dot(orb_velocity) < 0.0
                    {
                        emit(3, orb, 0.9);
                    }
                }
                if next.near_den && !old.near_den {
                    emit(5, den, 0.9);
                }
                if next.supported && !old.supported {
                    emit(102, den, 1.0);
                }
                if !next.supported && old.supported {
                    emit(110, None, 1.0);
                }
                if next.stored && !old.stored {
                    emit(133, orb, 1.0);
                }
                if next.held && !old.held {
                    // User ownership alone does not prove a hand-off from the
                    // pet. The ecology lifecycle adapter owns that evidence.
                    emit(17, orb, 1.0);
                }
                if next.touched && !old.touched {
                    emit(155, Some(context.body.contact.point_world), 1.0);
                }
                if !next.touched && old.touched {
                    emit(153, None, 0.9);
                }
                if next.dragged && !old.dragged {
                    emit(158, None, 1.0);
                }
                if !next.dragged && old.dragged {
                    emit(if next.supported { 159 } else { 160 }, None, 1.0);
                }
                if next.cursor_near && !old.cursor_near {
                    emit(161, Some(context.cursor_position), 0.85);
                }
                if !next.cursor_near && old.cursor_near {
                    emit(162, Some(context.cursor_position), 0.75);
                }
                if old.stress > 0.4 && next.stress < 0.25 {
                    emit(76, None, 0.9);
                }
                if old.startle > 0.5 && next.startle < 0.3 {
                    emit(172, orb, 0.9);
                }
                if old.startle < 0.35 && next.startle > 0.55 {
                    emit(
                        171,
                        goal.body_intent.gaze_target.filter(|p| p.is_finite()),
                        0.95,
                    );
                }
                if old.error < 0.15 && next.error > 0.45 {
                    emit(174, orb, 0.8);
                    self.repeated_failures = self.repeated_failures.saturating_add(1);
                    if self.repeated_failures >= 3 {
                        emit(175, orb, 0.85);
                    }
                }
                if old.error > 0.45 && next.error < 0.15 {
                    emit(178, orb, 0.85);
                }
                if !same_orb {
                    self.repeated_failures = 0;
                }
                let was_stuck = self.stuck_seconds;
                if next.error > 0.45
                    && context.body.motion.velocity.length() < 0.015
                    && goal.body_intent.desired_speed > 0.20
                    && !next.touched
                    && !next.dragged
                {
                    self.stuck_seconds += dt;
                } else {
                    self.stuck_seconds = 0.0;
                }
                if was_stuck < 1.0 && self.stuck_seconds >= 1.0 {
                    emit(177, orb, 0.8);
                }
                if next.surface_count > old.surface_count {
                    emit(181, None, 0.8);
                }
                if next.world_event != old.world_event {
                    match next.world_event {
                        MotorWorldEvent::OrbStored => emit(134, orb, 1.0),
                        MotorWorldEvent::CaptureFailed => emit(147, orb, 1.0),
                        MotorWorldEvent::DenFieldEntered => emit(35, den, 0.9),
                        _ => {}
                    }
                }
                if next.world_goal != old.world_goal {
                    match next.world_goal {
                        MotorWorldGoal::CarryOrbHome => emit(131, orb.or(den), 0.95),
                        MotorWorldGoal::RetrieveOrb => emit(121, orb, 0.95),
                        MotorWorldGoal::OfferOrb => emit(165, orb, 0.95),
                        _ => {}
                    }
                }
                if next.primary != old.primary {
                    match next.primary {
                        PrimaryIntent::Inspect | PrimaryIntent::SearchObject => {
                            emit(31, orb.or(den), 0.85)
                        }
                        PrimaryIntent::InvitePlay => emit(59, orb, 0.9),
                        PrimaryIntent::Celebrate => emit(42, orb, 0.9),
                        PrimaryIntent::RecoverFromMiss => emit(54, orb, 0.8),
                        PrimaryIntent::Nuzzle | PrimaryIntent::AcceptContact if next.touched => {
                            emit(151, Some(context.body.contact.point_world), 0.9)
                        }
                        PrimaryIntent::Rest if next.supported => emit(105, orb, 0.9),
                        PrimaryIntent::GroomSelf => emit(81, None, 0.8),
                        PrimaryIntent::QuietCompanionship => emit(163, None, 0.8),
                        _ => {}
                    }
                }
            }
        }
        // Internal needs also earn a brief initiative; scene edges alone left
        // an otherwise healthy, unchanged desktop almost expressionless.
        self.initiative_pause = (self.initiative_pause - dt).max(0.0);
        let available = !next.sleeping
            && !next.dragged
            && !next.touched
            && !context.focus_mode
            && !physical_danger(goal, context)
            && context.world_goal == MotorWorldGoal::None;
        if available && self.initiative_pause <= 0.0 {
            let urge = (goal.felt.boredom * 0.6
                + goal.derived.curiosity * 0.5
                + goal.felt.play_readiness * 0.35)
                .clamp(0.0, 1.0);
            self.initiative_charge = (self.initiative_charge + dt * (urge - 0.15)).max(0.0);
            if self.initiative_charge >= 14.0 {
                self.initiative_charge = 0.0;
                self.initiative_pause = 18.0 + (self.event_sequence % 13) as f32;
                let variants: &[u16] = if next.supported && goal.derived.fatigue > 0.4 {
                    &[104, 105, 106]
                } else if next.supported && goal.felt.boredom > 0.35 {
                    &[84, 103, 104, 89]
                } else if orb.is_some() && goal.derived.curiosity > 0.3 {
                    &[14, 15, 20, 10]
                } else {
                    &[10, 14, 15, 22]
                };
                let hash = self
                    .seed
                    .wrapping_add(self.event_sequence.wrapping_mul(0x9E3779B97F4A7C15));
                let sample = ((hash ^ (hash >> 29)).wrapping_mul(0xBF58476D1CE4E5B9) >> 32) as f32
                    / u32::MAX as f32;
                let weight = |id: u16| {
                    if id == self.previous_initiative {
                        0.15
                    } else {
                        1.0
                    }
                };
                let mut choice = sample * variants.iter().map(|&id| weight(id)).sum::<f32>();
                let mut id = variants[0];
                for &candidate in variants {
                    id = candidate;
                    choice -= weight(candidate);
                    if choice <= 0.0 {
                        break;
                    }
                }
                self.previous_initiative = id;
                emit(id, orb.or(den), 0.9);
            }
        }
        self.previous = Some(next);
        for event in &mut events {
            self.event_sequence = self.event_sequence.wrapping_add(1);
            let alternatives = expressive_variants(event.id);
            let mut viable = Vec::with_capacity(alternatives.len() + 1);
            // The original accompaniment remains the most likely individual
            // response. Other entries are compatible expressions of this same
            // cause, never additional claims about unobserved physical success.
            viable.extend([event.id; 3]);
            for &id in alternatives {
                let candidate = RepertoireEvent { id, ..*event };
                if id != self.last_variant
                    && eligible(candidate, goal, context).is_ok()
                    && variant_evidence(id, goal, context)
                {
                    viable.push(id);
                }
            }
            let mut hash = self.seed
                ^ self.event_sequence.wrapping_mul(0x9E3779B97F4A7C15)
                ^ u64::from(event.id).wrapping_mul(0xBF58476D1CE4E5B9);
            hash = (hash ^ (hash >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
            hash = (hash ^ (hash >> 27)).wrapping_mul(0x94D049BB133111EB);
            hash ^= hash >> 31;
            event.id = viable[hash as usize % viable.len()];
            self.last_variant = event.id;
        }
        // A carried object's measured load, not a carry command alone, earns
        // one post-load release. Never perturb the active grip to perform it.
        let load = context.body.motion.object_load;
        let carrying = matches!(
            context.world_goal,
            MotorWorldGoal::CarryOrbHome | MotorWorldGoal::ReturnOrb
        );
        if physical_danger(goal, context) || context.pet_dragged || sleeping(goal, context) {
            self.loaded_carry_seconds = 0.0;
            self.post_carry_release_seconds = 0.0;
        } else if carrying
            && load.is_finite()
            && load > 0.12
            && context.body.motion.velocity.length() > 0.015
        {
            self.loaded_carry_seconds = (self.loaded_carry_seconds + dt).min(2.0);
            self.post_carry_release_seconds = 0.0;
        } else if self.loaded_carry_seconds >= 0.8 && load.is_finite() && load < 0.04 {
            self.post_carry_release_seconds += dt;
            if !carrying && context.support_confirmed() {
                events.push(RepertoireEvent {
                    id: 83,
                    confidence: 0.9,
                    target: None,
                    side: 0.0,
                });
                self.loaded_carry_seconds = 0.0;
                self.post_carry_release_seconds = 0.0;
            } else if self.post_carry_release_seconds > 5.0 {
                self.loaded_carry_seconds = 0.0;
                self.post_carry_release_seconds = 0.0;
            }
        } else if !carrying && self.loaded_carry_seconds < 0.8 {
            self.loaded_carry_seconds = 0.0;
        }
        events.extend(
            self.virtual_interoception
                .observe(goal, context, packet, dt),
        );
        let irritation = self.virtual_interoception.signals.irritation;
        if irritation < 0.10 {
            self.irritation_blink_latched = false;
        }
        if irritation >= 0.22 && !self.irritation_blink_latched {
            self.irritation_blink_latched = true;
            events.push(RepertoireEvent {
                id: 29,
                confidence: 0.9,
                target: None,
                side: 0.0,
            });
        }
        events
    }
}

fn safe_dt(dt: f32) -> f32 {
    if dt.is_finite() {
        dt.clamp(0.0, 0.25)
    } else {
        0.0
    }
}

/// IDs for which this adapter has a concrete producer. Others require an
/// explicit event from a future sensor/task/learning subsystem; they are dormant
/// autonomously, even though their recipes can be tested through the event hook.
pub const AUTONOMOUS_REPERTOIRE_IDS: &[u16] = &[
    1, 2, 3, 5, 13, 29, 31, 35, 42, 54, 59, 61, 76, 81, 102, 105, 110, 112, 113, 114, 115, 118,
    121, 131, 133, 134, 147, 151, 153, 155, 158, 159, 160, 161, 162, 163, 165, 171, 172, 174, 176,
    178, 181, 184, 189, 190, 106, 25, 104, 84, 156, 89, 12, 152, 154, 175, 177, 83,
];

// Real selected motor phases offer additional expressive accompaniments; they
// are not evidence of successful orb grip/throw, smell, or learning. Those must
// arrive from their respective authoritative outcome producers.
fn phase_accompaniments(
    packet: &SomaticActuationPacket,
    goal: &BehaviorGoalFrame,
    context: &BehaviorContextFrame,
) -> &'static [u16] {
    use crate::BehaviorProgramId as P;
    match (packet.program, packet.phase_name.as_str()) {
        (Some(P::MoveOrientReflex), "eyes_first") => &[17],
        (Some(P::MoveOrientReflex), "front_axis_turn") => &[18],
        (Some(P::RestSurfaceRoostSearch), "approach_commit") => &[6],
        (Some(P::RestSitSettle), "lower_lift") => &[101],
        (Some(P::RestSitSettle), "rest_hold") if context.orb_position.is_some() => &[108],
        (Some(P::RestDrowsyYawn), "anticipatory_inhale" | "audible_yawn_peak") => &[77],
        (Some(P::RestDrowsyYawn), "blink_settle") => &[26],
        (Some(P::RestRemDreamWake), "wake_stretch_or_nrem") if goal.action == ActionId::WakeUp => {
            &[119, 120]
        }
        (Some(P::SocialSlowBlinkAffiliation), "signal") => &[22],
        (Some(P::SocialPettingSolicitation), "accept_or_withdraw") if !context.pet_touched => &[60],
        (Some(P::TouchSustainedHoldRelaxOrResist), "hold") if context.pet_touched => &[157],
        (Some(P::TouchTickleWriggle), "cooperate_or_guard")
            if context.pet_touched && goal.felt.contact_pleasantness > 0.5 =>
        {
            &[53]
        }
        (Some(P::HomeDenNestRest), "consume_carry_or_rest") if context.support_confirmed() => {
            &[107]
        }
        (Some(P::RestLeanRest), "hold")
            if context.support_confirmed()
                && packet
                    .support
                    .as_ref()
                    .is_some_and(|s| s.normal.x.abs() > 0.7) =>
        {
            &[91, 99]
        }
        (Some(P::RestNestAdjust), "settle_contact") if context.support_confirmed() => &[104],
        (Some(P::StateSelfGroomRealign), "body_reconfigure") => &[81],
        (Some(P::StateSelfGroomRealign), "settle") => &[89],
        (Some(P::DefensePostStressShakeOff), "release") => &[82],
        (Some(P::StateRespiratorySighReset), "deep_inhale") => &[75],
        (Some(P::StateRespiratorySighReset), "long_exhale") => &[76],
        (Some(P::StateRespiratorySighReset), "rhythm_reset") => &[80],
        (Some(P::MoveBrakeSquashRecover), "stillness") => &[179],
        (Some(P::MoveInspectPauseScan), "punctuate") if context.orb_position.is_none() => &[10],
        (Some(P::DefenseStartleOrientFreeze), "protect") => &[23],
        (Some(P::MoveOrientReflex), "freeze_30_120ms")
            if goal.felt.startle > 0.1 && goal.felt.startle <= 0.55 =>
        {
            &[173]
        }
        _ => &[],
    }
}

pub const PHASE_REPERTOIRE_IDS: &[u16] = &[
    6, 10, 17, 18, 22, 23, 26, 53, 60, 75, 76, 77, 80, 81, 82, 89, 91, 99, 101, 104, 107, 108, 119,
    120, 157, 173, 179,
];

/// Context-compatible expressive alternatives. These do not command the action
/// described by a recipe's cause: e.g. a landing's relief face cannot land a pet.
fn expressive_variants(source: u16) -> &'static [u16] {
    match source {
        1 => &[11, 20, 43],
        2 => &[17],
        3 => &[19],
        5 => &[35, 39],
        13 => &[15, 19, 31, 45],
        31 => &[15, 31, 32, 37, 38, 43, 45],
        35 => &[5, 39],
        42 => &[41, 42, 47, 51, 52, 56, 57],
        54 => &[36, 45, 54, 174],
        59 => &[28, 38, 50, 58, 59],
        61 => &[31, 43, 61, 68],
        76 => &[49, 76],
        81 => &[81, 84, 85],
        84 => &[84, 85],
        89 => &[89],
        102 => &[8, 30, 86, 102, 103],
        104 => &[104, 105],
        105 => &[25, 39, 105, 106],
        106 => &[25, 39, 106],
        112 => &[111, 112],
        113 => &[113],
        114 => &[114],
        115 => &[115, 117],
        118 => &[24, 118, 119, 120],
        121 => &[40, 46, 121],
        130 => &[130, 161],
        131 => &[46, 131],
        133 => &[41, 133, 134],
        134 => &[134],
        147 => &[36, 45, 54, 147, 174],
        151 => &[22, 48, 53, 151, 155, 156],
        153 => &[153, 157],
        155 => &[8, 151, 155, 156],
        156 => &[22, 53, 156],
        158 => &[158],
        159 => &[86, 102, 159],
        160 => &[160],
        161 => &[17, 38, 161, 164],
        162 => &[162],
        163 => &[39, 163, 170],
        165 => &[40, 59, 165, 166],
        171 => &[33, 44, 171],
        172 => &[34, 49, 76, 172],
        174 => &[36, 45, 174],
        176 => &[9, 176],
        178 => &[41, 178, 179],
        181 => &[31, 181],
        184 => &[5, 31, 184],
        189 => &[189],
        190 => &[190],
        _ => &[],
    }
}

fn variant_evidence(id: u16, goal: &BehaviorGoalFrame, context: &BehaviorContextFrame) -> bool {
    match id {
        11 | 20 => context
            .orb_position
            .is_some_and(|p| p.distance(context.body.motion.world_position) < 0.12),
        25 | 39 | 106 => goal.derived.fatigue > 0.5,
        28 | 47 | 50 | 51 | 52 | 56 | 57 | 58 => {
            goal.affect.valence > 0.15 && goal.felt.pain_like < 0.1
        }
        48 => goal.felt.physical_load > 0.5,
        53 | 151 | 156 => context.pet_touched && goal.felt.contact_pleasantness > 0.5,
        83 => matches!(
            context.world_goal,
            MotorWorldGoal::CarryOrbHome | MotorWorldGoal::ReturnOrb
        ),
        84 | 85 => context.support_confirmed() && context.somatic.supported_seconds > 20.0,
        99 => context.support_confirmed(),
        157 => context.pet_touched && context.pointer_down,
        170 => context.companion_intent == PrimaryIntent::QuietCompanionship && context.focus_mode,
        // These scalar accompaniments require no extra sensor or inferred habit.
        _ => true,
    }
}

pub fn autonomous_repertoire_coverage() -> Vec<u16> {
    let mut ids = AUTONOMOUS_REPERTOIRE_IDS.to_vec();
    for &source in AUTONOMOUS_REPERTOIRE_IDS {
        ids.extend_from_slice(expressive_variants(source));
    }
    ids.extend_from_slice(PHASE_REPERTOIRE_IDS);
    ids.extend_from_slice(crate::VIRTUAL_INTEROCEPTION_IDS);
    ids.sort_unstable();
    ids.dedup();
    ids
}

#[derive(Debug, Clone, Copy)]
struct PendingEvent {
    event: RepertoireEvent,
    age: f32,
    external: bool,
}
#[derive(Debug, Clone, Copy)]
struct ActiveRecipe {
    event: RepertoireEvent,
    elapsed: f32,
    external: bool,
    onset: bool,
    variation: RecipeVariation,
}

/// A performance is sampled once. No white noise enters per-frame actuation.
#[derive(Debug, Clone, Copy, PartialEq)]
struct RecipeVariation {
    duration: f32,
    amplitude: f32,
    asymmetry: f32,
    prepare_end: f32,
    recover_start: f32,
}

impl Default for RecipeVariation {
    fn default() -> Self {
        Self {
            duration: 1.0,
            amplitude: 1.0,
            asymmetry: 1.0,
            prepare_end: 0.25,
            recover_start: 0.65,
        }
    }
}

fn recipe_variation(seed: u64, sequence: u64, id: u16, fatigue: f32) -> RecipeVariation {
    // Protective/ordinary blinks and sleep checks retain their owner's timing.
    if matches!(id, 21..=30 | 116) {
        return RecipeVariation::default();
    }
    let mut state = seed ^ sequence.wrapping_mul(0x9e37_79b9_7f4a_7c15) ^ u64::from(id);
    let mut sample = || {
        state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut x = state;
        x = (x ^ (x >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        x = (x ^ (x >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        (((x ^ (x >> 31)) >> 40) as u32) as f32 / 16_777_215.0
    };
    let fatigue = if fatigue.is_finite() {
        fatigue.clamp(0.0, 1.0)
    } else {
        1.0
    };
    RecipeVariation {
        duration: 0.94 + sample() * 0.12 + fatigue * 0.05,
        amplitude: (0.90 + sample() * 0.10) * (1.0 - fatigue * 0.12),
        asymmetry: 0.88 + sample() * 0.12,
        prepare_end: 0.22 + sample() * 0.06,
        recover_start: 0.62 + sample() * 0.06,
    }
}

#[derive(Debug, Clone)]
pub struct RepertoireRuntime {
    throat: crate::VirtualThroatState,
    voice_sample: (bool, f32),
    adapter: RepertoireContextAdapter,
    active: Option<ActiveRecipe>,
    pending: Vec<PendingEvent>,
    cooldowns: [f32; REPERTOIRE_COUNT],
    external_asserted: [bool; REPERTOIRE_COUNT],
    preferred_side: f32,
    variation_seed: u64,
    performance_sequence: u64,
}

impl RepertoireRuntime {
    /// Actual synthesizer output only, consumed once by the following tick.
    pub fn observe_voice_output(&mut self, active: bool, measured_effort: f32) {
        self.voice_sample = (active, measured_effort);
    }

    pub fn throat_status(&self) -> crate::VirtualThroatStatus {
        self.throat.status()
    }

    pub fn new(seed: u64) -> Self {
        Self {
            throat: crate::VirtualThroatState::default(),
            voice_sample: (false, 0.0),
            adapter: RepertoireContextAdapter {
                seed,
                ..Default::default()
            },
            active: None,
            pending: Vec::with_capacity(8),
            cooldowns: [0.0; REPERTOIRE_COUNT],
            external_asserted: [false; REPERTOIRE_COUNT],
            preferred_side: if seed.count_ones().is_multiple_of(2) {
                1.0
            } else {
                -1.0
            },
            variation_seed: seed,
            performance_sequence: 0,
        }
    }

    /// Events nominate recipes; they never override existing navigation/grip.
    /// External callbacks are rising-edge debounced so a held request cannot
    /// restart itself when its cooldown expires.
    pub fn tick(
        &mut self,
        goal: &BehaviorGoalFrame,
        context: &BehaviorContextFrame,
        packet: &SomaticActuationPacket,
        external_events: &[RepertoireEvent],
        dt: f32,
    ) -> RepertoireOutput {
        let dt = safe_dt(dt);
        for cooldown in &mut self.cooldowns {
            *cooldown = (*cooldown - dt).max(0.0);
        }
        for pending in &mut self.pending {
            pending.age += dt;
        }
        self.pending.retain(|p| p.age <= 2.0);
        let mut reason = RepertoireReason::NoEvent;
        let events = self.adapter.observe(goal, context, packet, dt);
        for event in events {
            self.queue(event, false, &mut reason);
        }
        let mut asserted = [false; REPERTOIRE_COUNT];
        for &event in external_events.iter().take(REPERTOIRE_COUNT) {
            if event.id == 0 || usize::from(event.id) > REPERTOIRE_COUNT {
                reason = RepertoireReason::InvalidEvidence;
                continue;
            }
            let index = usize::from(event.id - 1);
            asserted[index] = true;
            if !self.external_asserted[index] {
                self.queue(event, true, &mut reason);
            }
        }
        self.external_asserted = asserted;
        if let Some(active) = &mut self.active {
            active.elapsed += dt;
            active.onset = false;
        }
        if let Some(active) = self.active {
            let recipe = repertoire_recipe(active.event.id).expect("validated recipe ID");
            if let Err(blocked) = eligible(active.event, goal, context) {
                self.finish();
                reason = blocked;
            } else if active.elapsed >= recipe.duration.max(0.1) * active.variation.duration {
                self.finish();
                reason = RepertoireReason::Completed;
            }
        }
        self.pending.retain(|pending| {
            if let Err(blocked) = eligible(pending.event, goal, context) {
                reason = blocked;
                false
            } else {
                true
            }
        });
        let best = self
            .pending
            .iter()
            .enumerate()
            .filter(|(_, pending)| self.cooldowns[usize::from(pending.event.id - 1)] <= 0.0)
            .max_by(|(_, a), (_, b)| {
                let pa = repertoire_recipe(a.event.id)
                    .expect("validated recipe")
                    .priority;
                let pb = repertoire_recipe(b.event.id)
                    .expect("validated recipe")
                    .priority;
                pa.cmp(&pb)
                    .then(a.event.confidence.total_cmp(&b.event.confidence))
                    .then_with(|| b.event.id.cmp(&a.event.id))
            })
            .map(|(index, _)| index);
        if let Some(index) = best {
            let pending = self.pending[index];
            let may_start = self.active.is_none_or(|active| {
                let old = repertoire_recipe(active.event.id).expect("validated recipe");
                let new = repertoire_recipe(pending.event.id).expect("validated recipe");
                active.elapsed >= (old.duration * 0.4).clamp(0.25, 1.2)
                    && new.priority > old.priority
            });
            if may_start {
                self.finish();
                self.pending.remove(index);
                self.performance_sequence = self.performance_sequence.wrapping_add(1);
                self.active = Some(ActiveRecipe {
                    event: pending.event,
                    elapsed: 0.0,
                    external: pending.external,
                    onset: true,
                    variation: recipe_variation(
                        self.variation_seed,
                        self.performance_sequence,
                        pending.event.id,
                        goal.derived.fatigue,
                    ),
                });
            }
        }
        let voice_sample = std::mem::replace(&mut self.voice_sample, (false, 0.0));
        let throat_allowed = self.active.is_none()
            && !sleeping(goal, context)
            && !physical_danger(goal, context)
            && !context.pet_dragged
            && !context.pet_touched
            && !context.orb_user_held
            && context.world_goal == MotorWorldGoal::None;
        let throat_effect = self.throat.tick(
            voice_sample.0,
            voice_sample.1,
            self.adapter.virtual_interoception.signals.irritation,
            throat_allowed,
            dt,
        );
        let Some(active) = self.active else {
            let diaphragm = throat_effect.breath.abs() * 0.12;
            return RepertoireOutput {
                reason,
                effect: throat_effect,
                budget: throat_effect
                    .breath
                    .abs()
                    .max(throat_effect.mouth_open.abs())
                    .max(throat_effect.mouth_asymmetry.abs()),
                field: if diaphragm > 0.001 && packet.fields.iter().any(Option::is_none) {
                    Some(crate::body_field(
                        SomaticFieldKind::Pulse,
                        Vec2::new(0.0, -0.12),
                        Vec2::Y,
                        0.28,
                        diaphragm,
                        0.5,
                    ))
                } else {
                    None
                },
                ..Default::default()
            };
        };
        render_recipe(active, goal, context, packet, self.preferred_side)
    }

    fn queue(&mut self, event: RepertoireEvent, external: bool, reason: &mut RepertoireReason) {
        if repertoire_recipe(event.id).is_none()
            || !event.confidence.is_finite()
            || event.confidence < 0.45
            || !event.side.is_finite()
            || event.target.is_some_and(|p| !p.is_finite())
        {
            *reason = RepertoireReason::InvalidEvidence;
            return;
        }
        if self.cooldowns[usize::from(event.id - 1)] > 0.0 {
            *reason = RepertoireReason::Cooldown;
            return;
        }
        if self
            .active
            .is_some_and(|active| active.event.id == event.id)
            || self.pending.iter().any(|p| p.event.id == event.id)
        {
            return;
        }
        if self.pending.len() < 8 {
            self.pending.push(PendingEvent {
                event,
                age: 0.0,
                external,
            });
        } else {
            *reason = RepertoireReason::ResourceBusy;
        }
    }

    fn finish(&mut self) {
        if let Some(active) = self.active.take() {
            self.cooldowns[usize::from(active.event.id - 1)] =
                repertoire_recipe(active.event.id).map_or(1.0, |recipe| recipe.cooldown.max(0.25));
        }
    }
}

fn sleeping(goal: &BehaviorGoalFrame, context: &BehaviorContextFrame) -> bool {
    goal.action == ActionId::Sleep || context.companion_intent == PrimaryIntent::Sleep
}

fn physical_danger(goal: &BehaviorGoalFrame, context: &BehaviorContextFrame) -> bool {
    goal.felt.pain_like > 0.22
        || goal.felt.startle > 0.55
        || goal.drives.safety > 0.65
        || context.boundary_violation > 0.18
        || context.body.topology.connected_components > 1
}

fn eligible(
    event: RepertoireEvent,
    goal: &BehaviorGoalFrame,
    context: &BehaviorContextFrame,
) -> Result<(), RepertoireReason> {
    let recipe = repertoire_recipe(event.id).ok_or(RepertoireReason::InvalidEvidence)?;
    let asleep = sleeping(goal, context);
    if asleep && recipe.family != 12 {
        return Err(RepertoireReason::SleepSuppressed);
    }
    if physical_danger(goal, context) && !(asleep && recipe.family == 12 || recipe.family == 18) {
        return Err(RepertoireReason::SafetySuppressed);
    }
    if recipe.requires_support && !context.support_confirmed() {
        return Err(RepertoireReason::InvalidEvidence);
    }
    if recipe.requires_object && context.orb_position.is_none() && context.edible_position.is_none()
    {
        return Err(RepertoireReason::InvalidEvidence);
    }
    Ok(())
}

fn render_recipe(
    active: ActiveRecipe,
    goal: &BehaviorGoalFrame,
    context: &BehaviorContextFrame,
    packet: &SomaticActuationPacket,
    preferred_side: f32,
) -> RepertoireOutput {
    let recipe = repertoire_recipe(active.event.id).expect("validated recipe");
    let variation = active.variation;
    let t = (active.elapsed / (recipe.duration.max(0.1) * variation.duration)).clamp(0.0, 1.0);
    let (phase, envelope) = if t < variation.prepare_end {
        (RepertoirePhase::Prepare, smooth(t / variation.prepare_end))
    } else if t < variation.recover_start {
        (RepertoirePhase::Express, 1.0)
    } else {
        (
            RepertoirePhase::Recover,
            1.0 - smooth((t - variation.recover_start) / (1.0 - variation.recover_start)),
        )
    };
    // Finite pulses belong to this event's expressive phase, never a free timer.
    let pulse = if recipe.effect.pulses > 1
        && (variation.prepare_end..variation.recover_start).contains(&t)
    {
        0.72 + 0.28
            * (std::f32::consts::TAU * (t - variation.prepare_end)
                / (variation.recover_start - variation.prepare_end)
                * f32::from(recipe.effect.pulses))
            .cos()
    } else {
        1.0
    };
    let amount = envelope * active.event.confidence.clamp(0.0, 1.0) * pulse * variation.amplitude;
    let side = if active.event.side.abs() > 0.25 {
        active.event.side.signum()
    } else {
        preferred_side
    };
    let e = recipe.effect;
    let mut effect = RepertoireEffect {
        brow_asymmetry: e.brow_asymmetry * amount * side * variation.asymmetry,
        mouth_asymmetry: e.mouth_asymmetry * amount * side * variation.asymmetry,
        mouth_curve: e.mouth_curve * amount,
        mouth_open: e.mouth_open * amount,
        eye_aperture: e.eye_aperture * amount,
        eye_scale: e.eye_scale * amount,
        squint: e.squint * amount,
        breath: e.breath * amount,
        lean: e.lean * amount * side,
        roll: e.roll * amount * side,
        soft_yield: e.soft_yield * amount,
        pulses: if active.onset { e.pulses } else { 0 },
    };
    let asleep = sleeping(goal, context);
    let object_busy = matches!(
        context.world_goal,
        MotorWorldGoal::CarryOrbHome
            | MotorWorldGoal::RetrieveOrb
            | MotorWorldGoal::ReturnOrb
            | MotorWorldGoal::OfferOrb
    );
    let body_busy = context.pet_dragged
        || object_busy
        || physical_danger(goal, context)
        || packet.fields.iter().all(Option::is_some);
    if body_busy || recipe.channels & CHANNEL_BODY == 0 {
        effect.lean = 0.0;
        effect.roll = 0.0;
        effect.soft_yield = 0.0;
    }
    if recipe.channels & CHANNEL_BREATH == 0 {
        effect.breath = 0.0;
    }
    if asleep {
        // Sleep expression cannot wake the organism or open either eye. Quiet
        // body/breath signals remain possible from actual sleep/contact phases.
        effect.eye_aperture = 0.0;
        effect.eye_scale = 0.0;
        effect.squint = 0.0;
        effect.mouth_open = 0.0;
        effect.mouth_curve = 0.0;
        effect.brow_asymmetry = 0.0;
        effect.mouth_asymmetry = 0.0;
    }
    if physical_danger(goal, context) {
        effect.mouth_curve = effect.mouth_curve.min(0.0);
    }
    let field = if !body_busy
        && recipe.channels & CHANNEL_BODY != 0
        && amount > 0.001
        && effect
            .soft_yield
            .abs()
            .max(effect.lean.abs())
            .max(effect.roll.abs())
            > 0.001
    {
        // Preserve the sign and kind of the authored action. Previously every
        // yield became Flatten, roll was discarded, and airborne accents had
        // no output at all. These remain small local forces, never root motion.
        let grounded = context.support_confirmed();
        let (kind, axis, strength) = if effect.soft_yield.abs()
            >= effect.lean.abs().max(effect.roll.abs())
            && effect.soft_yield.abs() > 0.005
        {
            if effect.soft_yield > 0.0 && grounded {
                (SomaticFieldKind::Flatten, Vec2::Y, effect.soft_yield * 0.30)
            } else if effect.soft_yield < 0.0 {
                (
                    SomaticFieldKind::Gather,
                    Vec2::Y,
                    effect.soft_yield.abs() * 0.24,
                )
            } else {
                (SomaticFieldKind::Brace, Vec2::Y, effect.soft_yield * 0.24)
            }
        } else if effect.roll.abs() >= effect.lean.abs() && effect.roll.abs() > 0.001 {
            (
                SomaticFieldKind::Curl,
                Vec2::Y * effect.roll.signum(),
                effect.roll.abs() * 0.45,
            )
        } else {
            (
                SomaticFieldKind::Shear,
                Vec2::X * effect.lean.signum(),
                effect.lean.abs() * 0.45,
            )
        };
        Some(
            LocalSomaticField {
                kind,
                space: FieldSpace::BodyLocal,
                center: Vec2::new(side * 0.18, -0.16),
                axis,
                radius: 0.35,
                strength: strength.min(0.08),
                falloff: 2.2,
                frequency_hz: 0.0,
                phase_01: t,
                target_component: None,
            }
            .bounded(),
        )
    } else {
        None
    };
    let blink_request = if active.onset
        && (!asleep || active.event.id == 116)
        && !context.pet_dragged
        && !object_busy
        && recipe.channels & CHANNEL_LIDS != 0
        && matches!(active.event.id, 21 | 22 | 23 | 28 | 29 | 30 | 116)
    {
        Some(RepertoireBlinkRequest {
            strength: if active.event.id == 116 { 0.72 } else { 0.8 },
            duration_seconds: if active.event.id == 116 {
                1.1
            } else if active.event.id == 22 {
                0.52
            } else {
                0.16
            },
            sleep_check: active.event.id == 116,
        })
    } else {
        None
    };
    RepertoireOutput {
        effect,
        active_id: Some(active.event.id),
        phase,
        reason: if body_busy {
            RepertoireReason::ResourceBusy
        } else if active.external {
            RepertoireReason::ExternalEvent
        } else {
            RepertoireReason::ContextEvent
        },
        budget: amount,
        gaze_target: if !asleep
            && !object_busy
            && !context.pet_dragged
            && !physical_danger(goal, context)
            && recipe.channels & CHANNEL_GAZE != 0
        {
            active.event.target
        } else {
            None
        },
        blink_request,
        field,
    }
}

fn smooth(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn recipe_bouts_vary_but_each_trajectory_is_deterministic_and_smooth() {
        let (goal, context, packet) = fixture();
        let mut signatures = std::collections::HashSet::new();
        for sequence in 1..65 {
            let variation = recipe_variation(42, sequence, 35, 0.3);
            assert_eq!(variation, recipe_variation(42, sequence, 35, 0.3));
            assert!((0.94..=1.11).contains(&variation.duration));
            assert!((0.79..=1.0).contains(&variation.amplitude));
            signatures.insert((variation.duration.to_bits(), variation.amplitude.to_bits()));
            let duration = repertoire_recipe(35).unwrap().duration * variation.duration;
            let mut previous = 0.0_f32;
            for frame in 0..=300 {
                let active = ActiveRecipe {
                    event: event(35),
                    elapsed: duration * frame as f32 / 300.0,
                    external: true,
                    onset: frame == 0,
                    variation,
                };
                let output = render_recipe(active, &goal, &context, &packet, 1.0);
                assert!((output.budget - previous).abs() < 0.06);
                assert_eq!(
                    output.effect,
                    render_recipe(active, &goal, &context, &packet, 1.0).effect
                );
                previous = output.budget;
            }
            assert!(previous < 0.0001);
        }
        assert_eq!(signatures.len(), 64);
        for id in [21, 22, 29, 30, 116] {
            assert_eq!(
                recipe_variation(42, 20, id, 0.8),
                RecipeVariation::default()
            );
        }
    }
    fn fixture() -> (
        BehaviorGoalFrame,
        BehaviorContextFrame,
        SomaticActuationPacket,
    ) {
        let mut life = lifecore::LifeCore::new(lifecore::Genome::from_seed(42), 42);
        let body_intent = life
            .tick(&lifecore::SensorFrame::default(), &Default::default(), 0.05)
            .body_intent;
        let mut goal = BehaviorGoalFrame {
            action: ActionId::IdleHover,
            body_intent,
            affect: Default::default(),
            drives: life.state.drives,
            felt: Default::default(),
            derived: Default::default(),
            attachment: 0.4,
            recent_outcome: None,
        };
        goal.drives.safety = 0.0;
        let mut context = BehaviorContextFrame::default();
        context.body.motion.world_position = Vec2::splat(0.5);
        context.orb_position = Some(Vec2::new(0.7, 0.5));
        context.orb_id = Some(1);
        context.somatic.supported = true;
        (goal, context, SomaticActuationPacket::default())
    }
    fn event(id: u16) -> RepertoireEvent {
        RepertoireEvent {
            id,
            confidence: 1.0,
            target: Some(Vec2::new(0.7, 0.5)),
            side: 1.0,
        }
    }
    #[test]
    fn body_recipes_preserve_signed_roll_gather_and_lean_channels() {
        let (goal, mut context, packet) = fixture();
        context.somatic.supported = false;
        let render = |id, side, context: &BehaviorContextFrame| {
            render_recipe(
                ActiveRecipe {
                    event: RepertoireEvent { side, ..event(id) },
                    elapsed: repertoire_recipe(id).unwrap().duration * 0.4,
                    external: true,
                    onset: false,
                    variation: RecipeVariation::default(),
                },
                &goal,
                context,
                &packet,
                1.0,
            )
        };
        let left = render(7, -1.0, &context).field.unwrap();
        let right = render(7, 1.0, &context).field.unwrap();
        assert_eq!(left.kind, SomaticFieldKind::Curl);
        assert_eq!(right.kind, SomaticFieldKind::Curl);
        assert_eq!(left.axis, -right.axis);
        assert!(left.strength > 0.025 && left.strength <= 0.08);
        assert_eq!(
            render(154, 1.0, &context).field.unwrap().kind,
            SomaticFieldKind::Gather
        );
        assert_eq!(
            render(151, 1.0, &context).field.unwrap().kind,
            SomaticFieldKind::Shear
        );
        let airborne_yield = render(140, 1.0, &context).field.unwrap();
        assert_ne!(airborne_yield.kind, SomaticFieldKind::Flatten);
        context.pet_dragged = true;
        assert!(render(7, 1.0, &context).field.is_none());
        context.pet_dragged = false;
        context.world_goal = MotorWorldGoal::CarryOrbHome;
        assert!(render(7, 1.0, &context).field.is_none());
    }
    #[test]
    fn internal_curiosity_generates_bounded_initiative_and_sleep_suppresses_it() {
        let (mut goal, context, packet) = fixture();
        goal.derived.curiosity = 0.8;
        goal.felt.boredom = 0.7;
        let mut adapter = RepertoireContextAdapter::default();
        let mut count = 0;
        for _ in 0..2400 {
            count += adapter.observe(&goal, &context, &packet, 0.05).len();
        }
        assert!(
            (2..=8).contains(&count),
            "bounded need-driven bouts: {count}"
        );
        goal.action = ActionId::Sleep;
        adapter.observe(&goal, &context, &packet, 0.05);
        for _ in 0..2400 {
            assert!(adapter.observe(&goal, &context, &packet, 0.05).is_empty());
        }
    }

    #[test]
    fn unchanged_scene_has_no_timer_generated_behavior() {
        let (goal, context, packet) = fixture();
        let mut runtime = RepertoireRuntime::new(42);
        for _ in 0..1800 {
            let output = runtime.tick(&goal, &context, &packet, &[], 0.05);
            assert_eq!(output.active_id, None);
            assert_eq!(output.effect, RepertoireEffect::default());
        }
    }
    #[test]
    fn observed_orb_motion_generates_an_edge_not_an_every_frame_command() {
        let (goal, mut context, packet) = fixture();
        let mut adapter = RepertoireContextAdapter::default();
        assert!(adapter.observe(&goal, &context, &packet, 0.05).is_empty());
        context.orb_position = Some(Vec2::new(0.71, 0.5));
        assert!(
            adapter
                .observe(&goal, &context, &packet, 0.05)
                .iter()
                .any(|e| e.id == 2)
        );
        let _ = adapter.observe(&goal, &context, &packet, 0.05); // Observed stop.
        assert!(adapter.observe(&goal, &context, &packet, 0.05).is_empty());
    }
    #[test]
    fn held_external_event_does_not_restart_after_cooldown() {
        let (goal, context, packet) = fixture();
        let mut runtime = RepertoireRuntime::new(42);
        let request = event(31);
        assert_eq!(
            runtime
                .tick(&goal, &context, &packet, &[request], 0.05)
                .active_id,
            Some(31)
        );
        let mut completed = false;
        for _ in 0..6000 {
            let output = runtime.tick(&goal, &context, &packet, &[request], 0.05);
            if completed {
                assert_eq!(output.active_id, None);
            }
            completed |= output.active_id.is_none();
        }
        assert!(completed);
    }
    #[test]
    fn every_catalog_recipe_accepts_an_explicit_evidence_event_and_is_bounded() {
        let (goal, context, packet) = fixture();
        for id in 1..=REPERTOIRE_COUNT as u16 {
            let mut runtime = RepertoireRuntime::new(42);
            assert_eq!(
                runtime
                    .tick(&goal, &context, &packet, &[event(id)], 0.05)
                    .active_id,
                Some(id),
                "id{id}"
            );
            for _ in 0..16 {
                let output = runtime.tick(&goal, &context, &packet, &[], 0.05);
                let e = output.effect;
                for value in [
                    e.brow_asymmetry,
                    e.mouth_asymmetry,
                    e.mouth_curve,
                    e.mouth_open,
                    e.eye_aperture,
                    e.eye_scale,
                    e.squint,
                    e.breath,
                    e.lean,
                    e.roll,
                    e.soft_yield,
                ] {
                    assert!(value.is_finite() && value.abs() <= 1.0, "id{id}: {value}");
                }
                assert!((0.0..=1.0).contains(&output.budget));
                if let Some(field) = output.field {
                    assert!(field.strength <= 0.08);
                }
            }
        }
    }
    #[test]
    fn sleep_allows_only_quiet_phase_driven_signals_and_never_opens_eyes() {
        let (mut goal, mut context, mut packet) = fixture();
        let mut runtime = RepertoireRuntime::new(42);
        let _ = runtime.tick(&goal, &context, &packet, &[], 0.05);
        goal.action = ActionId::Sleep;
        context.companion_intent = PrimaryIntent::Sleep;
        packet.phase_name = "nrem_hold".into();
        let awake_request = [event(31)];
        for tick in 0..40 {
            let output = runtime.tick(
                &goal,
                &context,
                &packet,
                if tick == 0 { &awake_request[..] } else { &[] },
                0.05,
            );
            if let Some(id) = output.active_id {
                assert_eq!(repertoire_recipe(id).unwrap().family, 12);
            }
            assert_eq!(output.effect.eye_aperture, 0.0);
            assert_eq!(output.effect.eye_scale, 0.0);
            assert!(output.gaze_target.is_none());
            assert!(output.blink_request.is_none());
        }
    }
    #[test]
    fn drag_grip_and_danger_cannot_be_overridden_by_decorative_fields() {
        let (mut goal, mut context, packet) = fixture();
        context.world_goal = MotorWorldGoal::CarryOrbHome;
        let mut runtime = RepertoireRuntime::new(42);
        let _ = runtime.tick(&goal, &context, &packet, &[event(92)], 0.05);
        for _ in 0..12 {
            let output = runtime.tick(&goal, &context, &packet, &[], 0.05);
            assert!(output.field.is_none());
            assert_eq!(output.effect.lean, 0.0);
            assert!(output.gaze_target.is_none());
        }
        context.world_goal = MotorWorldGoal::None;
        context.pet_dragged = true;
        let output = runtime.tick(&goal, &context, &packet, &[event(93)], 0.05);
        assert!(output.field.is_none());
        goal.felt.pain_like = 0.8;
        assert_eq!(
            runtime
                .tick(&goal, &context, &packet, &[event(94)], 0.05)
                .active_id,
            None
        );
    }
    #[test]
    fn deterministic_replay_and_invalid_event_rejection() {
        let (goal, context, packet) = fixture();
        let mut a = RepertoireRuntime::new(42);
        let mut b = a.clone();
        for i in 0..200 {
            let events = if i == 0 { vec![event(42)] } else { Vec::new() };
            assert_eq!(
                a.tick(&goal, &context, &packet, &events, 0.05),
                b.tick(&goal, &context, &packet, &events, 0.05)
            );
        }
        let invalid = RepertoireEvent {
            confidence: f32::NAN,
            ..event(31)
        };
        assert_eq!(
            a.tick(&goal, &context, &packet, &[invalid], f32::NAN)
                .active_id,
            None
        );
    }

    #[test]
    fn post_carry_release_requires_measured_moving_load_then_unloading() {
        for hz in [30.0, 60.0, 120.0] {
            let (goal, mut c, packet) = fixture();
            let mut adapter = RepertoireContextAdapter::default();
            c.world_goal = MotorWorldGoal::CarryOrbHome;
            c.body.motion.velocity = Vec2::new(0.04, 0.0);
            for _ in 0..(hz as usize) {
                assert!(
                    !adapter
                        .observe(&goal, &c, &packet, 1.0 / hz)
                        .iter()
                        .any(|e| e.id == 83)
                );
            }
            c.world_goal = MotorWorldGoal::None;
            assert!(
                !adapter
                    .observe(&goal, &c, &packet, 1.0 / hz)
                    .iter()
                    .any(|e| e.id == 83)
            );
            c.world_goal = MotorWorldGoal::CarryOrbHome;
            c.body.motion.object_load = 0.18;
            for _ in 0..(hz as usize) {
                let _ = adapter.observe(&goal, &c, &packet, 1.0 / hz);
            }
            c.world_goal = MotorWorldGoal::None;
            c.body.motion.object_load = 0.0;
            assert!(
                adapter
                    .observe(&goal, &c, &packet, 1.0 / hz)
                    .iter()
                    .any(|e| e.id == 83)
            );
            assert!(
                !adapter
                    .observe(&goal, &c, &packet, 1.0 / hz)
                    .iter()
                    .any(|e| e.id == 83)
            );
        }
    }

    #[test]
    fn coverage_is_explicit_and_unimplemented_learning_stays_dormant() {
        let ids = autonomous_repertoire_coverage();
        assert!(ids.len() > 90);
        for id in [191, 192, 193, 194, 195, 196, 197, 198, 199, 200] {
            assert!(
                !ids.contains(&id),
                "id{id} needs evidence this adapter does not have"
            );
        }
        let dormant: Vec<_> = (1..=200).filter(|id| !ids.contains(id)).collect();
        eprintln!(
            "autonomous expressive coverage {} / 200; external-only IDs {:?}",
            ids.len(),
            dormant
        );
    }

    #[test]
    fn repeated_real_inspection_events_choose_deterministic_compatible_variants() {
        let (goal, mut context, packet) = fixture();
        let mut adapter = RepertoireContextAdapter {
            seed: 42,
            ..Default::default()
        };
        let mut seen = std::collections::BTreeSet::new();
        let _ = adapter.observe(&goal, &context, &packet, 0.05);
        for _ in 0..40 {
            context.companion_intent = PrimaryIntent::Inspect;
            for event in adapter.observe(&goal, &context, &packet, 0.05) {
                seen.insert(event.id);
            }
            context.companion_intent = PrimaryIntent::IdleContent;
            let _ = adapter.observe(&goal, &context, &packet, 0.05);
        }
        assert!(
            seen.len() >= 4,
            "one causal state should have several coherent expressions: {seen:?}"
        );
        assert!(
            seen.iter()
                .all(|id| *id == 31 || expressive_variants(31).contains(id))
        );
    }

    #[test]
    fn unrelated_moving_surface_cannot_emit_a_riding_event() {
        let (goal, mut context, mut packet) = fixture();
        let floor = lifecore::SurfaceId("test:floor".into());
        context.surfaces = vec![
            crate::SurfaceCandidate {
                surface_id: floor.clone(),
                minimum: Vec2::ZERO,
                maximum: Vec2::ONE,
                velocity: Vec2::ZERO,
                familiarity: 1.0,
                recent_failed_landings: 0,
            },
            crate::SurfaceCandidate {
                surface_id: lifecore::SurfaceId("other:window".into()),
                minimum: Vec2::ZERO,
                maximum: Vec2::ONE,
                velocity: Vec2::ZERO,
                familiarity: 1.0,
                recent_failed_landings: 0,
            },
        ];
        packet.support = Some(crate::SurfaceAttachmentCommand {
            surface_id: floor,
            anchor_point: Vec2::new(0.5, 1.0),
            normal: Vec2::NEG_Y,
            tangent: Vec2::X,
            target_contact_fraction: 0.3,
            normal_compliance: 0.2,
            tangent_friction: 0.6,
            adhesion: 0.2,
            load_fraction: 0.4,
            break_force: 0.8,
            release_half_life: 0.3,
        });
        let mut adapter = RepertoireContextAdapter::default();
        let _ = adapter.observe(&goal, &context, &packet, 0.05);
        context.surfaces[1].velocity = Vec2::X;
        assert!(adapter.observe(&goal, &context, &packet, 0.05).is_empty());
        context.surfaces[0].velocity = Vec2::new(0.1, 0.0);
        assert!(
            adapter
                .observe(&goal, &context, &packet, 0.05)
                .iter()
                .any(|e| e.id == 189)
        );
    }

    #[test]
    fn virtual_irritation_nominates_once_until_recovered() {
        let (goal, context, packet) = fixture();
        let mut adapter = RepertoireContextAdapter::default();
        adapter.virtual_interoception.signals.irritation = 0.3;
        assert!(
            adapter
                .observe(&goal, &context, &packet, 0.0)
                .iter()
                .any(|e| e.id == 29)
        );
        for _ in 0..1_000 {
            assert!(
                !adapter
                    .observe(&goal, &context, &packet, 0.0)
                    .iter()
                    .any(|e| e.id == 29)
            );
        }
        adapter.virtual_interoception.signals.irritation = 0.0;
        adapter.observe(&goal, &context, &packet, 0.0);
        adapter.virtual_interoception.signals.irritation = 0.3;
        assert!(
            adapter
                .observe(&goal, &context, &packet, 0.0)
                .iter()
                .any(|e| e.id == 29)
        );
    }

    #[test]
    fn unchanged_phase_does_not_repeat_gesture_when_scheduler_renews_bout() {
        let (goal, context, mut packet) = fixture();
        let mut adapter = RepertoireContextAdapter::default();
        let _ = adapter.observe(&goal, &context, &packet, 0.05);
        packet.program = Some(crate::BehaviorProgramId::RestDrowsyYawn);
        packet.phase_name = "audible_yawn_peak".into();
        assert!(
            adapter
                .observe(&goal, &context, &packet, 0.05)
                .iter()
                .any(|event| event.id == 77)
        );
        // Longer than every ordinary gesture cooldown: unchanged physical
        // input must not acquire a new cause from bookkeeping alone.
        for bout in 1..=4_000 {
            packet.source_bout_id = bout;
            assert!(
                !adapter
                    .observe(&goal, &context, &packet, 0.05)
                    .iter()
                    .any(|event| event.id == 77)
            );
        }
        packet.phase_name = "blink_settle".into();
        assert!(
            adapter
                .observe(&goal, &context, &packet, 0.05)
                .iter()
                .any(|event| event.id == 26)
        );
        packet.phase_name = "audible_yawn_peak".into();
        assert!(
            adapter
                .observe(&goal, &context, &packet, 0.05)
                .iter()
                .any(|event| event.id == 77),
            "a real new phase still has an accompaniment"
        );
    }

    #[test]
    fn real_yawn_phase_is_an_event_once_and_danger_retains_gaze_ownership() {
        let (mut goal, context, mut packet) = fixture();
        let mut adapter = RepertoireContextAdapter::default();
        let _ = adapter.observe(&goal, &context, &packet, 0.05);
        packet.program = Some(crate::BehaviorProgramId::RestDrowsyYawn);
        packet.source_bout_id = 7;
        packet.phase_name = "audible_yawn_peak".into();
        assert!(
            adapter
                .observe(&goal, &context, &packet, 0.05)
                .iter()
                .any(|e| e.id == 77)
        );
        assert!(adapter.observe(&goal, &context, &packet, 0.05).is_empty());
        goal.felt.startle = 0.8;
        let mut runtime = RepertoireRuntime::new(42);
        let output = runtime.tick(&goal, &context, &packet, &[event(171)], 0.05);
        assert_eq!(output.active_id, Some(171));
        assert!(output.gaze_target.is_none());
        assert!(output.field.is_none());
    }
}
