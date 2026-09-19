//! Application seam for causal regulation, motor expression, and finite grip.
//!
//! Call `tick` before the motor runtime so learned surface familiarity can
//! affect target ranking. Call `decorate_packet` after repertoire/surface-care
//! merging so the causal phase and finite-grip release remain authoritative.

use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
};

use glam::Vec2;
use lifecore::{
    ActionId, BehaviorGoalFrame, CausalPhase, ExpectedOutcome, OrganicCauseCode, OrganicOutcome,
    OrganicOutcomeCode, OrganicRegulationStateV1, OrganicRegulator, PrimaryIntent, RegulationInput,
    RegulationOutput,
};
use pet_motor::{
    BehaviorContextFrame, CompletionReason, FieldSpace, InternalFlowPhase, LocalSomaticField,
    MotorPoseIntent, SomaticActuationPacket, SomaticFieldKind, VoiceSemanticIntent,
    rank_cling_surface, rank_surface,
};

const GRIP_CAPACITY_SECONDS: f32 = 6.0;
const GRIP_RELEASE_SECONDS: f32 = 0.65;
const ORGANIC_STATE_FILE: &str = "organic_regulation.json";
const MAX_ORGANIC_STATE_BYTES: u64 = 128 * 1024;

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct OrganicRuntimeInput {
    pub cause: Option<OrganicCauseCode>,
    pub expected: Option<ExpectedOutcome>,
    pub outcome: Option<OrganicOutcome>,
    pub heard_name: bool,
    /// Event-like acoustic salience above the local noise floor. This is not
    /// speech recognition and never implies that a name was heard.
    pub sound_interest: f32,
    pub quiet: bool,
    pub user_available: f32,
    pub novelty: f32,
    pub prediction_error: f32,
    pub direct_contact: f32,
    /// Zero derives a stable context from action, intent, and support.
    pub context_key: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OrganicRuntimeOutput {
    pub regulation: RegulationOutput,
    pub preferred_rest_target: Option<Vec2>,
    pub cling_available: bool,
}

#[derive(Debug, Clone)]
pub struct OrganicRuntime {
    regulator: OrganicRegulator,
    last_action: Option<ActionId>,
    was_contacting: bool,
    was_sound_salient: bool,
    was_novel: bool,
    heard_name_latched: bool,
    pending_contact_credit: bool,
    pending_contact_action: Option<ActionId>,
    pending_contact_age: f32,
    last_completion: CompletionReason,
    pending_action_change: bool,
    last_axis: Vec2,
    latest: Option<OrganicRuntimeOutput>,
    grip_remaining: f32,
    grip_release_remaining: f32,
    cling_exhausted: bool,
}

impl Default for OrganicRuntime {
    fn default() -> Self {
        Self::from_state(OrganicRegulationStateV1::default())
    }
}

impl OrganicRuntime {
    #[must_use]
    pub fn from_state(state: OrganicRegulationStateV1) -> Self {
        Self {
            regulator: OrganicRegulator::from_state(state),
            last_action: None,
            was_contacting: false,
            was_sound_salient: false,
            was_novel: false,
            heard_name_latched: false,
            pending_contact_credit: false,
            pending_contact_action: None,
            pending_contact_age: 0.0,
            last_completion: CompletionReason::None,
            pending_action_change: false,
            last_axis: Vec2::X,
            latest: None,
            grip_remaining: GRIP_CAPACITY_SECONDS,
            grip_release_remaining: 0.0,
            cling_exhausted: false,
        }
    }

    #[must_use]
    pub fn persistent_state(&self) -> OrganicRegulationStateV1 {
        self.regulator.persistent_state()
    }

    /// Loads only the bounded feature/state DTO. Missing, oversized, or
    /// malformed state falls back safely without affecting the rest of Pet 2.
    #[must_use]
    pub fn load(root: &Path) -> Self {
        let path = root.join(ORGANIC_STATE_FILE);
        for candidate in [&path, &sibling(&path, ".organic_regulation.bak")] {
            if fs::metadata(candidate).is_ok_and(|meta| meta.len() <= MAX_ORGANIC_STATE_BYTES)
                && let Ok(bytes) = fs::read(candidate)
                && let Ok(state) = serde_json::from_slice::<OrganicRegulationStateV1>(&bytes)
            {
                return Self::from_state(state);
            }
        }
        Self::default()
    }

    /// Writes at most 128 KiB through a same-directory temporary and backup.
    pub fn save(&self, root: &Path) -> io::Result<()> {
        fs::create_dir_all(root)?;
        let path = root.join(ORGANIC_STATE_FILE);
        let temporary = sibling(&path, ".organic_regulation.tmp");
        let backup = sibling(&path, ".organic_regulation.bak");
        let bytes = serde_json::to_vec_pretty(&self.persistent_state())
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        if bytes.len() as u64 > MAX_ORGANIC_STATE_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "organic regulation state exceeds 128 KiB",
            ));
        }
        let mut file = fs::File::create(&temporary)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        if path.exists() {
            if backup.exists() {
                fs::remove_file(&backup)?;
            }
            fs::rename(&path, &backup)?;
        }
        if let Err(error) = fs::rename(&temporary, &path) {
            if backup.exists() {
                let _ = fs::rename(&backup, &path);
            }
            return Err(error);
        }
        Ok(())
    }

    #[must_use]
    pub fn latest(&self) -> Option<&OrganicRuntimeOutput> {
        self.latest.as_ref()
    }

    /// Updates causal state from the previous physical feedback and enriches
    /// available surface familiarity before this frame's motor target lock.
    pub fn tick(
        &mut self,
        goal: &BehaviorGoalFrame,
        context: &mut BehaviorContextFrame,
        mut input: OrganicRuntimeInput,
        dt: f32,
    ) -> OrganicRuntimeOutput {
        let dt = finite(dt).clamp(0.0, 0.25);
        self.enrich_surface_familiarity(goal, context);
        let changed = self
            .last_action
            .is_some_and(|previous| previous != goal.action);
        self.pending_action_change |= changed;
        let pending_action_ready =
            self.pending_action_change && self.regulator.active_trace().is_none();
        let initial_urge = self.last_action.is_none()
            && goal.action != ActionId::IdleHover
            && goal.drives.strongest().1 >= 0.18;
        let contact = finite(input.direct_contact)
            .clamp(0.0, 1.0)
            .max(if context.pet_touched { 1.0 } else { 0.0 });
        let fresh_contact = contact > 0.20 && !self.was_contacting;
        let sound_interest = finite(input.sound_interest).clamp(0.0, 1.0);
        let novelty = finite(input.novelty).clamp(0.0, 1.0);
        let fresh_sound = sound_interest > 0.04 && !self.was_sound_salient;
        let fresh_novelty = novelty > 0.42 && !self.was_novel;
        let fresh_name = input.heard_name && !self.heard_name_latched;
        if changed
            || self
                .regulator
                .active_trace()
                .is_some_and(|trace| trace.phase == CausalPhase::Recover)
        {
            self.pending_contact_credit = false;
            self.pending_contact_action = None;
            self.pending_contact_age = 0.0;
        } else if self.pending_contact_credit {
            self.pending_contact_age += dt;
            if self.pending_contact_age > 1.0 {
                self.pending_contact_credit = false;
                self.pending_contact_action = None;
            }
        }
        if fresh_contact && is_social(goal.action) {
            self.pending_contact_credit = true;
            self.pending_contact_action = Some(goal.action);
            self.pending_contact_age = 0.0;
        }
        let cause = input.cause.or_else(|| {
            fresh_name
                .then_some(OrganicCauseCode::HeardName)
                .or_else(|| fresh_contact.then_some(OrganicCauseCode::DirectContact))
                .or_else(|| {
                    (fresh_sound || fresh_novelty).then_some(OrganicCauseCode::ExternalNovelty)
                })
                .or_else(|| {
                    ((pending_action_ready || initial_urge)
                        && matches!(
                            goal.action,
                            ActionId::LandOnWindow | ActionId::ClingToWindowSide
                        ))
                    .then_some(OrganicCauseCode::SurfaceOpportunity)
                })
                .or_else(|| {
                    (pending_action_ready || initial_urge)
                        .then_some(OrganicCauseCode::ActionChanged)
                })
        });

        let surface_key = selected_surface(goal, context)
            .map(|surface| support_context_key(&surface.surface_id.0, goal.action));
        let context_key = if input.context_key == 0 {
            surface_key
                .unwrap_or_else(|| behavioral_context_key(goal.action, context.companion_intent))
        } else {
            input.context_key
        };
        let (drive, drive_error) = goal.drives.strongest();
        let phase_index = context.somatic.phase.map_or(0, |phase| phase.index);
        let trace_matches = self
            .regulator
            .active_trace()
            .is_some_and(|trace| trace.selected_action == goal.action);
        let social_contact_credit = trace_matches
            && self.pending_contact_credit
            && self.pending_contact_action == Some(goal.action)
            && self.pending_contact_age <= 1.0
            && is_social(goal.action)
            && self.regulator.active_trace().is_some_and(|trace| {
                matches!(trace.phase, CausalPhase::Act | CausalPhase::AwaitOutcome)
            });
        let completion_edge = !changed
            && context.somatic.completion_reason != CompletionReason::None
            && self.last_completion == CompletionReason::None;
        input.outcome = if changed && self.regulator.active_trace().is_some() {
            Some(OrganicOutcome {
                code: OrganicOutcomeCode::Interrupted,
                confidence: 1.0,
                quality: 0.0,
            })
        } else {
            input.outcome.or_else(|| {
                inferred_outcome(
                    goal.action,
                    context,
                    trace_matches && completion_edge,
                    social_contact_credit,
                )
            })
        };
        let regulation = self.regulator.tick(RegulationInput {
            dt,
            timestamp_seconds: context.timestamp_seconds,
            selected_action: goal.action,
            primary_intent: context.companion_intent,
            expected: input
                .expected
                .unwrap_or_else(|| expected_for(goal.action, context.companion_intent)),
            strongest_drive: drive,
            drive_error,
            cause,
            external_novelty: novelty.max(sound_interest),
            prediction_error: input.prediction_error,
            user_available: input.user_available,
            quiet: input.quiet || context.focus_mode,
            direct_contact: contact,
            context_key,
            oriented: context.somatic.target_locked || phase_index >= 1,
            prepared: phase_index >= 2,
            acted: phase_index >= 3 || context.somatic.completion_reason != CompletionReason::None,
            outcome: input.outcome,
        });
        if regulation
            .trace
            .as_ref()
            .is_some_and(|trace| trace.selected_action == goal.action)
        {
            self.pending_action_change = false;
        }

        self.update_grip(goal, context, dt);
        self.last_axis = (goal.body_intent.target_position - context.body.motion.world_position)
            .normalize_or(self.last_axis);
        self.last_action = Some(goal.action);
        self.was_contacting = contact > 0.20;
        if social_contact_credit {
            self.pending_contact_credit = false;
            self.pending_contact_action = None;
            self.pending_contact_age = 0.0;
        }
        self.last_completion = context.somatic.completion_reason;
        self.was_sound_salient = if self.was_sound_salient {
            sound_interest >= 0.015
        } else {
            sound_interest > 0.04
        };
        self.was_novel = if self.was_novel {
            novelty >= 0.25
        } else {
            novelty > 0.42
        };
        self.heard_name_latched = input.heard_name;
        let preferred_rest_target = habitual_rest_target(&self.regulator, goal, context);
        let output = OrganicRuntimeOutput {
            regulation,
            preferred_rest_target,
            cling_available: !self.cling_exhausted && self.grip_remaining > 0.0,
        };
        self.latest = Some(output.clone());
        output
    }

    /// Adds phase-specific internal flow and enforces finite awake grip.
    /// This never changes the root position or selects a different action.
    pub fn decorate_packet(&self, goal: &BehaviorGoalFrame, packet: &mut SomaticActuationPacket) {
        let Some(latest) = &self.latest else {
            return;
        };
        if let Some(trace) = &latest.regulation.trace {
            let expression_phase = if trace.phase == CausalPhase::IntegrateOutcome
                && trace.observed != OrganicOutcomeCode::Success
            {
                CausalPhase::Recover
            } else {
                trace.phase
            };
            let (flow_phase, duration, gain, damping) = match expression_phase {
                CausalPhase::Notice => (InternalFlowPhase::Notice, 0.28, 1.08, 0.04),
                CausalPhase::Prepare => (InternalFlowPhase::Prepare, 0.38, 1.16, 0.06),
                CausalPhase::Act => (InternalFlowPhase::Act, 0.70, 1.22, 0.04),
                CausalPhase::AwaitOutcome => (
                    InternalFlowPhase::AwaitOutcome,
                    trace.deadline_seconds,
                    0.72,
                    0.28,
                ),
                CausalPhase::IntegrateOutcome => (InternalFlowPhase::Outcome, 0.24, 1.04, 0.10),
                CausalPhase::Recover => (InternalFlowPhase::Recover, 0.85, 0.68, 0.38),
            };
            let progress = (trace.phase_elapsed / duration.max(0.01)).clamp(0.0, 1.0);
            packet.internal.flow_phase = flow_phase;
            packet.internal.flow_phase_progress = progress;
            packet.internal.flow_strength_multiplier *= gain;
            packet.internal.flow_damping = packet.internal.flow_damping.max(damping);
            push_causal_field(
                packet,
                expression_phase,
                progress,
                self.last_axis,
                latest.regulation.activation,
            );
        }

        let social = is_social(goal.action);
        if social && latest.regulation.trace.is_none() && !latest.regulation.bid_allowed {
            packet.voice.semantic = VoiceSemanticIntent::None;
            packet.voice.emit_once = false;
            packet.voice.intensity = 0.0;
            packet.locomotion.speed_multiplier = 0.0;
            packet.expression.acknowledgement = 0.0;
        }

        if goal.action == ActionId::ClingToWindowSide {
            let release = if self.cling_exhausted {
                (self.grip_release_remaining / GRIP_RELEASE_SECONDS).clamp(0.0, 1.0)
            } else {
                (self.grip_remaining / GRIP_CAPACITY_SECONDS).clamp(0.25, 1.0)
            };
            if let Some(support) = &mut packet.support {
                support.adhesion *= release;
                support.load_fraction *= release;
                support.break_force = support.break_force.min(0.58);
                support.release_half_life = support.release_half_life.max(0.42);
            }
            if self.cling_exhausted && self.grip_release_remaining <= 0.0 {
                packet.support = None;
                packet.locomotion.pose = MotorPoseIntent::Recover;
            }
        }
        packet.sanitize();
    }

    fn enrich_surface_familiarity(
        &self,
        goal: &BehaviorGoalFrame,
        context: &mut BehaviorContextFrame,
    ) {
        for surface in &mut context.surfaces {
            let key = support_context_key(&surface.surface_id.0, goal.action);
            let learned = self.regulator.selected_habit_strength(key, goal.action);
            surface.familiarity = (surface.familiarity + learned * 0.28).clamp(0.0, 1.0);
        }
    }

    fn update_grip(&mut self, goal: &BehaviorGoalFrame, context: &BehaviorContextFrame, dt: f32) {
        if goal.action == ActionId::ClingToWindowSide {
            if !self.cling_exhausted {
                let cost = if context.somatic.supported {
                    0.72 + context.somatic.maximum_strain.clamp(0.0, 1.0) * 0.70
                } else {
                    0.20
                };
                self.grip_remaining = (self.grip_remaining - dt * cost).max(0.0);
                if self.grip_remaining <= 0.0 {
                    self.cling_exhausted = true;
                    self.grip_release_remaining = GRIP_RELEASE_SECONDS;
                }
            } else {
                self.grip_release_remaining = (self.grip_release_remaining - dt).max(0.0);
            }
        } else {
            self.grip_remaining = (self.grip_remaining + dt * 0.55).min(GRIP_CAPACITY_SECONDS);
            if self.grip_remaining >= GRIP_CAPACITY_SECONDS * 0.65 {
                self.cling_exhausted = false;
            }
            self.grip_release_remaining = 0.0;
        }
    }
}

fn selected_surface(
    goal: &BehaviorGoalFrame,
    context: &BehaviorContextFrame,
) -> Option<pet_motor::SurfaceTarget> {
    match goal.action {
        ActionId::ClingToWindowSide => rank_cling_surface(context),
        ActionId::Sleep => rank_surface(context, true, false),
        ActionId::LandOnWindow => rank_surface(context, false, false),
        _ => None,
    }
}

fn inferred_outcome(
    action: ActionId,
    context: &BehaviorContextFrame,
    completion_edge: bool,
    social_contact_credit: bool,
) -> Option<OrganicOutcome> {
    if social_contact_credit {
        return Some(OrganicOutcome::success(0.82));
    }
    if !completion_edge {
        return None;
    }
    match context.somatic.completion_reason {
        CompletionReason::GoalReached
        | CompletionReason::ContactConfirmed
        | CompletionReason::SupportConfirmed
        | CompletionReason::UserResponded => Some(OrganicOutcome::success(
            context.somatic.support_stability.max(0.68),
        )),
        CompletionReason::TimedOut if is_social(action) => Some(OrganicOutcome::no_response()),
        CompletionReason::TimedOut | CompletionReason::Invalidated => Some(OrganicOutcome {
            code: OrganicOutcomeCode::Failed,
            confidence: 1.0,
            quality: 0.0,
        }),
        CompletionReason::Interrupted => Some(OrganicOutcome {
            code: OrganicOutcomeCode::Interrupted,
            confidence: 1.0,
            quality: 0.0,
        }),
        _ => None,
    }
}

fn expected_for(action: ActionId, intent: PrimaryIntent) -> ExpectedOutcome {
    if is_social(action)
        || matches!(
            intent,
            PrimaryIntent::InviteContact | PrimaryIntent::InvitePlay
        )
    {
        ExpectedOutcome {
            continuation: 0.55,
            user_response: 0.68,
            success: 0.56,
            uncertainty: 0.28,
            ..ExpectedOutcome::default()
        }
    } else if matches!(action, ActionId::LandOnWindow | ActionId::ClingToWindowSide) {
        ExpectedOutcome {
            object_contact: 0.82,
            success: 0.70,
            uncertainty: 0.18,
            ..ExpectedOutcome::default()
        }
    } else {
        ExpectedOutcome {
            continuation: 0.36,
            success: 0.58,
            uncertainty: 0.24,
            ..ExpectedOutcome::default()
        }
    }
}

fn habitual_rest_target(
    regulator: &OrganicRegulator,
    goal: &BehaviorGoalFrame,
    context: &BehaviorContextFrame,
) -> Option<Vec2> {
    if goal.action == ActionId::Sleep {
        return None;
    }
    context
        .surfaces
        .iter()
        .filter_map(|surface| {
            let strength = regulator.selected_habit_strength(
                support_context_key(&surface.surface_id.0, goal.action),
                goal.action,
            );
            (strength >= 0.08).then_some((surface, strength))
        })
        .max_by(|left, right| left.1.total_cmp(&right.1))
        .map(|(surface, _)| (surface.minimum + surface.maximum) * 0.5)
}

fn push_causal_field(
    packet: &mut SomaticActuationPacket,
    phase: CausalPhase,
    progress: f32,
    axis: Vec2,
    activation: f32,
) {
    let pulse = (4.0 * progress * (1.0 - progress)).powi(2);
    let (kind, strength) = match phase {
        CausalPhase::Notice => (SomaticFieldKind::Pulse, 0.08 * pulse),
        CausalPhase::Prepare => (SomaticFieldKind::Gather, 0.12 * (0.4 + progress * 0.6)),
        CausalPhase::Act => (SomaticFieldKind::Wave, 0.10 * pulse),
        CausalPhase::AwaitOutcome => (SomaticFieldKind::Brace, 0.035),
        CausalPhase::IntegrateOutcome => (SomaticFieldKind::Pulse, 0.12 * pulse),
        CausalPhase::Recover => (SomaticFieldKind::Gather, 0.08 * (1.0 - progress)),
    };
    if let Some(slot) = packet.fields.iter_mut().find(|field| field.is_none()) {
        *slot = Some(LocalSomaticField {
            kind,
            space: FieldSpace::BodyLocal,
            center: axis.normalize_or(Vec2::X) * 0.10,
            axis: axis.normalize_or(Vec2::X),
            radius: 0.58,
            strength: strength * (0.45 + activation.clamp(0.0, 1.0) * 0.55),
            falloff: 2.2,
            frequency_hz: 0.0,
            phase_01: progress,
            target_component: None,
        });
    }
}

fn support_context_key(surface_id: &str, action: ActionId) -> u64 {
    stable_hash(surface_id.as_bytes()) ^ (action as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
}

fn behavioral_context_key(action: ActionId, intent: PrimaryIntent) -> u64 {
    (action as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ (intent as u64 + 1).wrapping_mul(0xD1B5_4A32_D192_ED03)
}

fn stable_hash(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01B3);
    }
    hash
}

fn is_social(action: ActionId) -> bool {
    matches!(
        action,
        ActionId::ApproachCursor
            | ActionId::InvitePetting
            | ActionId::InviteCursorChase
            | ActionId::SilentStare
            | ActionId::Chirp
            | ActionId::Purr
    )
}

fn finite(value: f32) -> f32 {
    if value.is_finite() { value } else { 0.0 }
}

fn sibling(path: &Path, file_name: &str) -> PathBuf {
    path.with_file_name(file_name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lifecore::{Drives, Genome, SurfaceId};
    use pet_motor::{BehaviorProgramId, PhaseId, SurfaceAttachmentCommand, SurfaceCandidate};

    fn goal(action: ActionId) -> BehaviorGoalFrame {
        let mut life = lifecore::LifeCore::new(Genome::from_seed(7), 7);
        let mut drives = Drives::initial(&life.state.genome.temperament);
        drives.social = 0.8;
        BehaviorGoalFrame {
            action,
            body_intent: life
                .tick(&Default::default(), &Default::default(), 0.05)
                .body_intent,
            affect: Default::default(),
            drives,
            felt: Default::default(),
            derived: Default::default(),
            attachment: 0.4,
            recent_outcome: None,
        }
    }

    #[test]
    fn corrupt_primary_recovers_the_previous_causal_state() {
        let folder = tempfile::tempdir().unwrap();
        let state = OrganicRegulationStateV1 {
            activation: 0.37,
            ..Default::default()
        };
        let runtime = OrganicRuntime::from_state(state);
        runtime.save(folder.path()).unwrap();
        runtime.save(folder.path()).unwrap();
        fs::write(folder.path().join(ORGANIC_STATE_FILE), b"interrupted write").unwrap();
        let restored = OrganicRuntime::load(folder.path()).persistent_state();
        assert!((restored.activation - 0.37).abs() < 1.0e-6);
    }

    #[test]
    fn unanswered_contact_relaxes_instead_of_using_the_success_pulse() {
        let mut runtime = OrganicRuntime::default();
        let goal = goal(ActionId::InvitePetting);
        let mut context = BehaviorContextFrame::default();
        runtime.tick(
            &goal,
            &mut context,
            OrganicRuntimeInput {
                cause: Some(OrganicCauseCode::DirectContact),
                ..Default::default()
            },
            0.05,
        );
        let trace = runtime
            .latest
            .as_mut()
            .unwrap()
            .regulation
            .trace
            .as_mut()
            .unwrap();
        trace.phase = CausalPhase::IntegrateOutcome;
        trace.observed = OrganicOutcomeCode::NoResponse;
        trace.phase_elapsed = 0.12;
        let mut packet = SomaticActuationPacket::default();
        runtime.decorate_packet(&goal, &mut packet);
        assert_eq!(packet.internal.flow_phase, InternalFlowPhase::Recover);
        assert!(
            packet
                .fields
                .iter()
                .flatten()
                .any(|field| field.kind == SomaticFieldKind::Gather)
        );
        runtime
            .latest
            .as_mut()
            .unwrap()
            .regulation
            .trace
            .as_mut()
            .unwrap()
            .observed = OrganicOutcomeCode::Success;
        let mut successful = SomaticActuationPacket::default();
        runtime.decorate_packet(&goal, &mut successful);
        assert_eq!(successful.internal.flow_phase, InternalFlowPhase::Outcome);
    }

    #[test]
    fn adapter_maps_measured_motor_progress_to_causal_flow() {
        let mut runtime = OrganicRuntime::default();
        let goal = goal(ActionId::InvitePetting);
        let mut context = BehaviorContextFrame {
            companion_intent: PrimaryIntent::InviteContact,
            ..Default::default()
        };
        context.somatic.target_locked = true;
        context.somatic.phase = Some(PhaseId {
            program: BehaviorProgramId::SocialPettingSolicitation,
            index: 2,
        });
        let output = runtime.tick(
            &goal,
            &mut context,
            OrganicRuntimeInput {
                cause: Some(OrganicCauseCode::DriveError),
                user_available: 1.0,
                ..OrganicRuntimeInput::default()
            },
            0.05,
        );
        assert!(output.regulation.trace.is_some());
        let mut packet = SomaticActuationPacket::default();
        runtime.decorate_packet(&goal, &mut packet);
        assert_ne!(packet.internal.flow_phase, InternalFlowPhase::Ambient);
        assert!(packet.field_count() > 0);
    }

    #[test]
    fn finite_cling_budget_ends_in_a_smooth_physical_release() {
        let mut runtime = OrganicRuntime::default();
        let goal = goal(ActionId::ClingToWindowSide);
        let mut context = BehaviorContextFrame::default();
        context.somatic.supported = true;
        context.somatic.maximum_strain = 0.5;
        for _ in 0..300 {
            runtime.tick(&goal, &mut context, OrganicRuntimeInput::default(), 0.05);
        }
        assert!(!runtime.latest().unwrap().cling_available);
        let mut packet = SomaticActuationPacket {
            support: Some(SurfaceAttachmentCommand {
                surface_id: SurfaceId("screen:left_edge".into()),
                anchor_point: Vec2::new(0.0, 0.5),
                normal: Vec2::X,
                tangent: Vec2::Y,
                target_contact_fraction: 0.28,
                normal_compliance: 0.22,
                tangent_friction: 0.68,
                adhesion: 0.42,
                load_fraction: 0.18,
                break_force: 0.58,
                release_half_life: 0.42,
            }),
            ..Default::default()
        };
        runtime.decorate_packet(&goal, &mut packet);
        assert!(packet.support.is_none());
        assert_eq!(packet.locomotion.pose, MotorPoseIntent::Recover);
    }

    #[test]
    fn all_four_edges_keep_sleep_on_bottom_and_cling_off_bottom() {
        let mut context = BehaviorContextFrame::default();
        context.body.motion.world_position = Vec2::new(0.02, 0.4);
        context.surfaces = [
            ("screen:top_edge", Vec2::ZERO, Vec2::new(1.0, 0.002)),
            ("screen:right_edge", Vec2::new(0.998, 0.0), Vec2::ONE),
            ("screen:bottom_edge", Vec2::new(0.0, 0.998), Vec2::ONE),
            ("screen:left_edge", Vec2::ZERO, Vec2::new(0.002, 1.0)),
        ]
        .into_iter()
        .map(|(id, minimum, maximum)| SurfaceCandidate {
            surface_id: SurfaceId(id.into()),
            minimum,
            maximum,
            velocity: Vec2::ZERO,
            familiarity: 0.5,
            recent_failed_landings: 0,
        })
        .collect();
        assert_eq!(
            rank_surface(&context, true, false).unwrap().surface_id.0,
            "screen:bottom_edge"
        );
        assert_ne!(
            rank_cling_surface(&context).unwrap().surface_id.0,
            "screen:bottom_edge"
        );
    }

    #[test]
    fn contact_credit_is_short_lived_and_scoped_to_the_current_social_action() {
        let mut runtime = OrganicRuntime::default();
        let mut context = BehaviorContextFrame::default();
        let locomotion = goal(ActionId::ExploreScreen);
        runtime.tick(
            &locomotion,
            &mut context,
            OrganicRuntimeInput {
                direct_contact: 1.0,
                ..OrganicRuntimeInput::default()
            },
            0.05,
        );
        assert!(!runtime.pending_contact_credit);

        runtime.was_contacting = false;
        let social = goal(ActionId::InvitePetting);
        runtime.tick(
            &social,
            &mut context,
            OrganicRuntimeInput {
                direct_contact: 1.0,
                ..OrganicRuntimeInput::default()
            },
            0.05,
        );
        assert_eq!(
            runtime.pending_contact_action,
            Some(ActionId::InvitePetting)
        );
        for _ in 0..21 {
            runtime.tick(&social, &mut context, OrganicRuntimeInput::default(), 0.05);
        }
        assert!(!runtime.pending_contact_credit);
        assert_eq!(runtime.pending_contact_action, None);
    }
}
