use std::collections::VecDeque;

use lifecore::BehaviorGoalFrame;

use crate::{
    ActivePerformance, BehaviorContextFrame, BehaviorProgramId, BehaviorTarget, BoutStyle,
    CompletionReason, MotorReadabilityTuning, MotorTraceRecord, PROGRAM_COUNT, PhaseAdvance,
    PhaseId, RegimeBlend, SomaticActuationPacket, SomaticRegime, SurfaceTarget, advance_phase,
    choose_program, definition, lock_target, phase_clock_scale, phase_name, phase_progress,
};

const TRACE_CAPACITY: usize = 256;

#[derive(Debug, Clone)]
pub struct BehaviorPerformanceRuntime {
    identity_seed: u64,
    next_bout_id: u64,
    active: Option<ActivePerformance>,
    cooldowns: [f32; PROGRAM_COUNT],
    remembered_surface: Option<SurfaceTarget>,
    tuning: MotorReadabilityTuning,
    regime: RegimeBlend,
    regime_candidate: SomaticRegime,
    regime_candidate_seconds: f32,
    trace: VecDeque<MotorTraceRecord>,
    recent_motion_signatures: VecDeque<u8>,
    last_completion: CompletionReason,
    last_packet: SomaticActuationPacket,
}

impl BehaviorPerformanceRuntime {
    #[must_use]
    pub fn new(identity_seed: u64) -> Self {
        Self {
            identity_seed,
            next_bout_id: 1,
            active: None,
            cooldowns: [0.0; PROGRAM_COUNT],
            remembered_surface: None,
            tuning: MotorReadabilityTuning::default(),
            regime: RegimeBlend::default(),
            regime_candidate: SomaticRegime::CalmContent,
            regime_candidate_seconds: 0.0,
            trace: VecDeque::with_capacity(TRACE_CAPACITY),
            recent_motion_signatures: VecDeque::with_capacity(2),
            last_completion: CompletionReason::None,
            last_packet: SomaticActuationPacket::default(),
        }
    }

    pub fn set_tuning(&mut self, tuning: MotorReadabilityTuning) {
        self.tuning = tuning.bounded();
    }

    #[must_use]
    pub fn tick(
        &mut self,
        goal: &BehaviorGoalFrame,
        context: &BehaviorContextFrame,
        dt: f32,
    ) -> SomaticActuationPacket {
        self.tick_internal(goal, context, dt, true)
    }

    /// Starts one explicit Body Lab review bout. The same catalog definition,
    /// target locking, phase clock, actuation bus and PBF consumer are used as
    /// in production; only autonomous program replacement is suspended.
    pub fn begin_lab_fixture(
        &mut self,
        program: BehaviorProgramId,
        goal: &BehaviorGoalFrame,
        context: &BehaviorContextFrame,
    ) {
        if self.active.is_some() {
            self.finish_active(CompletionReason::Interrupted);
        }
        self.start(program, crate::MotorCause::LabFixture, goal, context);
    }

    /// Advances a previously started Body Lab bout without running the normal
    /// selector. Bounded phase maxima still finish the program automatically.
    #[must_use]
    pub fn tick_lab_fixture(
        &mut self,
        goal: &BehaviorGoalFrame,
        context: &BehaviorContextFrame,
        dt: f32,
    ) -> SomaticActuationPacket {
        self.tick_internal(goal, context, dt, false)
    }

    pub fn cancel_lab_fixture(&mut self) {
        if self
            .active
            .as_ref()
            .is_some_and(|active| active.cause == crate::MotorCause::LabFixture)
        {
            self.finish_active(CompletionReason::Interrupted);
        }
    }

    fn tick_internal(
        &mut self,
        goal: &BehaviorGoalFrame,
        context: &BehaviorContextFrame,
        dt: f32,
        allow_selection: bool,
    ) -> SomaticActuationPacket {
        let dt = if dt.is_finite() {
            dt.clamp(0.0, 0.25)
        } else {
            0.0
        };
        for cooldown in &mut self.cooldowns {
            *cooldown = (*cooldown - dt).max(0.0);
        }
        self.update_regime(goal, dt);

        if let Some(active) = &mut self.active {
            match advance_phase(active, context, dt) {
                PhaseAdvance::Hold | PhaseAdvance::Advanced => {}
                PhaseAdvance::Finished(reason) => self.finish_active(reason),
            }
        }

        if allow_selection {
            let current_program = self.active.as_ref().map(|active| active.program);
            let current_readable = self
                .active
                .as_ref()
                .is_none_or(|active| active.minimum_readability_reached);
            if let Some(decision) = choose_program(
                goal,
                context,
                current_program,
                current_readable,
                &self.cooldowns,
            ) {
                let should_start = match self.active.as_ref() {
                    None => true,
                    Some(active) if active.program == decision.program => false,
                    Some(active) => {
                        decision.priority.rank() > definition(active.program).priority.rank()
                            || active.minimum_readability_reached
                    }
                };
                if should_start {
                    if self.active.is_some() {
                        self.finish_active(CompletionReason::Interrupted);
                    }
                    self.start(decision.program, decision.cause, goal, context);
                }
            }
        }

        let mut packet = SomaticActuationPacket {
            frame_id: context.frame_id,
            regime: self.regime,
            ..SomaticActuationPacket::default()
        };
        super::programs::apply_regime(&mut packet, self.regime);
        if let Some(active) = &self.active {
            let progress = phase_progress(active);
            let name = phase_name(active.program, active.phase.index);
            packet.source_bout_id = active.bout_id;
            packet.program = Some(active.program);
            packet.phase = Some(active.phase);
            packet.phase_name = name.to_owned();
            packet.phase_progress = progress;
            packet.cause = active.cause;
            let phase_started =
                active.phase_time <= dt * phase_clock_scale(active.program, name) + 1.0e-6;
            super::programs::apply_program(
                &mut packet,
                active,
                name,
                progress,
                phase_started,
                goal,
                context,
                self.tuning,
            );
            // Global physical tempo is owned by `pet_body::locomotion`. Keeping
            // it out of the authored packet preserves the relative timing and
            // braking envelopes shared by all 64 programs instead of stacking
            // a second, program-dependent speed multiplier here.
            if matches!(
                active.program,
                BehaviorProgramId::RestSurfaceRoostSearch
                    | BehaviorProgramId::RestLandingSoftTouchdown
            ) && packet.locomotion.speed_multiplier > 0.001
            {
                packet.locomotion.speed_multiplier =
                    (packet.locomotion.speed_multiplier * 1.55).max(1.20);
                packet.locomotion.acceleration_limit = 1.50;
            }
            packet.sanitize();
            self.record_trace(&packet, goal, context);
        } else {
            packet.sanitize();
        }
        self.last_packet = packet.clone();
        packet
    }

    pub(crate) fn start(
        &mut self,
        program: BehaviorProgramId,
        cause: crate::MotorCause,
        goal: &BehaviorGoalFrame,
        context: &BehaviorContextFrame,
    ) {
        let bout_id = self.next_bout_id;
        self.next_bout_id = self.next_bout_id.saturating_add(1);
        let target = lock_target(program, goal, context, self.remembered_surface.as_ref());
        if let Some(BehaviorTarget::Surface(surface)) = &target {
            self.remembered_surface = Some(surface.clone());
        }
        let sampled_style = sample_style_avoiding(
            self.identity_seed,
            bout_id,
            program,
            &self.recent_motion_signatures,
        );
        if self.recent_motion_signatures.len() == 2 {
            self.recent_motion_signatures.pop_front();
        }
        self.recent_motion_signatures
            .push_back(motion_signature(sampled_style));
        self.active = Some(ActivePerformance {
            bout_id,
            program,
            phase: PhaseId { program, index: 0 },
            phase_time: 0.0,
            total_time: 0.0,
            locked_target: target,
            sampled_style,
            minimum_readability_reached: false,
            interruption_request: None,
            source_action: goal.action,
            cause,
        });
        self.last_completion = CompletionReason::None;
    }

    fn finish_active(&mut self, reason: CompletionReason) {
        if let Some(active) = self.active.take() {
            self.cooldowns[active.program.index()] = definition(active.program).cooldown_seconds;
            self.last_completion = reason;
            if reason == CompletionReason::Invalidated
                && matches!(active.locked_target, Some(BehaviorTarget::Surface(_)))
            {
                self.remembered_surface = None;
            }
        }
    }

    fn update_regime(&mut self, goal: &BehaviorGoalFrame, dt: f32) {
        let scores = regime_scores(goal);
        let challenger_index = scores
            .iter()
            .enumerate()
            .max_by(|left, right| left.1.total_cmp(right.1))
            .map_or(0, |(index, _)| index);
        let challenger = regime_from_index(challenger_index);
        let current_score = scores[regime_index(self.regime.primary)];
        let challenger_score = scores[challenger_index];
        if challenger != self.regime.primary {
            if challenger == self.regime_candidate {
                self.regime_candidate_seconds += dt;
            } else {
                self.regime_candidate = challenger;
                self.regime_candidate_seconds = dt;
            }
            if challenger_score > current_score + 0.24
                || self.regime_candidate_seconds >= self.tuning.state_hysteresis_s
            {
                self.regime.primary = challenger;
                self.regime_candidate_seconds = 0.0;
            }
        } else {
            self.regime_candidate = challenger;
            self.regime_candidate_seconds = 0.0;
        }
        let primary_index = regime_index(self.regime.primary);
        let secondary_index = scores
            .iter()
            .enumerate()
            .filter(|(index, _)| *index != primary_index)
            .filter(|(index, _)| compatible(self.regime.primary, regime_from_index(*index)))
            .max_by(|left, right| left.1.total_cmp(right.1))
            .map_or(primary_index, |(index, _)| index);
        self.regime.secondary = regime_from_index(secondary_index);
        self.regime.primary_weight = if scores[primary_index] - scores[secondary_index] > 0.42 {
            0.82
        } else {
            0.70
        };
    }

    fn record_trace(
        &mut self,
        packet: &SomaticActuationPacket,
        goal: &BehaviorGoalFrame,
        context: &BehaviorContextFrame,
    ) {
        let (Some(program), Some(phase)) = (packet.program, packet.phase) else {
            return;
        };
        if self.trace.len() == TRACE_CAPACITY {
            self.trace.pop_front();
        }
        self.trace.push_back(MotorTraceRecord {
            frame_id: packet.frame_id,
            bout_id: packet.source_bout_id,
            source_event: packet.cause,
            source_action: goal.action,
            state_regime: packet.regime,
            program,
            phase,
            phase_name: packet.phase_name.clone(),
            target_locked: packet.locomotion.target_locked,
            field_kinds: packet.fields.map(|field| field.map(|field| field.kind)),
            completion_reason: self.last_completion,
            motor_error: context.somatic.motor_error,
        });
    }

    #[must_use]
    pub fn active(&self) -> Option<&ActivePerformance> {
        self.active.as_ref()
    }

    #[must_use]
    pub fn last_packet(&self) -> &SomaticActuationPacket {
        &self.last_packet
    }

    #[must_use]
    pub fn last_completion(&self) -> CompletionReason {
        self.last_completion
    }

    pub fn traces(&self) -> impl Iterator<Item = &MotorTraceRecord> {
        self.trace.iter()
    }

    #[must_use]
    pub fn trace_tail(&self, limit: usize) -> Vec<MotorTraceRecord> {
        let skip = self.trace.len().saturating_sub(limit);
        self.trace.iter().skip(skip).cloned().collect()
    }
}

impl Default for BehaviorPerformanceRuntime {
    fn default() -> Self {
        Self::new(1)
    }
}

fn sample_style(seed: u64, bout_id: u64, program: BehaviorProgramId) -> BoutStyle {
    let base = splitmix64(seed ^ bout_id.rotate_left(17) ^ (program as u64).rotate_left(41));
    let a = unit_from_bits(base);
    let b = unit_from_bits(splitmix64(base));
    let c = unit_from_bits(splitmix64(base ^ 0xA5A5_5A5A_D3C1_7E19));
    BoutStyle {
        amplitude: 0.88 + a * 0.24,
        tempo: 0.90 + b * 0.20,
        arc_sign: if base & 1 == 0 { -1.0 } else { 1.0 },
        asymmetry: 0.72 + c * 0.28,
        seed: base,
    }
}

fn sample_style_avoiding(
    seed: u64,
    bout_id: u64,
    program: BehaviorProgramId,
    recent: &VecDeque<u8>,
) -> BoutStyle {
    for attempt in 0..8_u64 {
        let style = sample_style(
            seed ^ attempt.wrapping_mul(0xD1B5_4A32_D192_ED03),
            bout_id,
            program,
        );
        if !recent.contains(&motion_signature(style)) {
            return style;
        }
    }
    // Eight candidates cover the six coarse signatures under normal hashing.
    // Keep deterministic progress even under an adversarial/colliding seed.
    let mut style = sample_style(seed, bout_id, program);
    let forced = (0_u8..12)
        .find(|candidate| {
            let arc = candidate & 1;
            let tempo = (candidate / 2) % 3;
            let asymmetry = (candidate / 6) % 2;
            let signature = arc | (tempo << 1) | (asymmetry << 3);
            !recent.contains(&signature)
        })
        .unwrap_or(0);
    style.arc_sign = if forced & 1 == 0 { -1.0 } else { 1.0 };
    style.tempo = match (forced / 2) % 3 {
        0 => 0.94,
        1 => 1.00,
        _ => 1.07,
    };
    style.asymmetry = if (forced / 6) % 2 == 0 { 0.80 } else { 0.92 };
    style
}

fn motion_signature(style: BoutStyle) -> u8 {
    let arc = u8::from(style.arc_sign >= 0.0);
    let tempo = if style.tempo < 0.967 {
        0
    } else if style.tempo < 1.033 {
        1
    } else {
        2
    };
    let asymmetry = u8::from(style.asymmetry >= 0.86);
    arc | (tempo << 1) | (asymmetry << 3)
}

fn splitmix64(mut value: u64) -> u64 {
    value = value.wrapping_add(0x9E37_79B9_7F4A_7C15);
    value = (value ^ (value >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    value ^ (value >> 31)
}

fn unit_from_bits(value: u64) -> f32 {
    ((value >> 40) as u32) as f32 / 0x00FF_FFFF as f32
}

fn regime_scores(goal: &BehaviorGoalFrame) -> [f32; 8] {
    let valence = goal.affect.valence;
    [
        (valence.max(0.0) * 0.45
            + (1.0 - goal.affect.stress) * 0.35
            + (1.0 - (goal.affect.arousal - 0.32).abs()) * 0.20)
            .clamp(0.0, 1.0),
        (goal.drives.play * 0.52 + goal.felt.play_readiness * 0.32 + goal.affect.arousal * 0.16)
            .clamp(0.0, 1.0),
        (goal.drives.social * 0.34 + goal.attachment * 0.30 + goal.felt.social_safety * 0.36)
            .clamp(0.0, 1.0),
        (goal.drives.curiosity * 0.42
            + goal.derived.neural_novelty * 0.30
            + goal.felt.exploration_readiness * 0.28)
            .clamp(0.0, 1.0),
        (goal.drives.sleep * 0.40
            + (1.0 - goal.felt.activation) * 0.28
            + goal.felt.physical_load * 0.32)
            .clamp(0.0, 1.0),
        ((-valence).max(0.0) * 0.62 + (1.0 - goal.derived.neural_threat) * 0.18).clamp(0.0, 1.0),
        (goal.affect.stress * 0.34 + goal.drives.safety * 0.26 + goal.felt.pain_like * 0.40)
            .clamp(0.0, 1.0),
        (goal.affect.frustration * 0.62 + (1.0 - goal.felt.agency_match) * 0.38).clamp(0.0, 1.0),
    ]
}

fn regime_index(regime: SomaticRegime) -> usize {
    regime as usize
}

fn regime_from_index(index: usize) -> SomaticRegime {
    use SomaticRegime as R;
    [
        R::CalmContent,
        R::Playful,
        R::Affiliative,
        R::Curious,
        R::Fatigued,
        R::SadLowValence,
        R::Threatened,
        R::Frustrated,
    ][index.min(7)]
}

fn compatible(primary: SomaticRegime, secondary: SomaticRegime) -> bool {
    use SomaticRegime as R;
    !matches!(
        (primary, secondary),
        (R::Threatened, R::Playful | R::Affiliative | R::CalmContent)
            | (R::Playful | R::Affiliative | R::CalmContent, R::Threatened)
            | (R::SadLowValence, R::Playful)
            | (R::Playful, R::SadLowValence)
    )
}

#[cfg(test)]
mod tests {
    use glam::Vec2;
    use lifecore::{
        ActionId, AffectState, BehaviorGoalFrame, BodyIntent, DerivedNervousState, Drives,
        FeltStateV1, Genome, LocomotionMode, PoseIntent, SurfaceId,
    };

    use super::*;

    fn goal(action: ActionId, target: Vec2) -> BehaviorGoalFrame {
        let genome = Genome::from_seed(9);
        BehaviorGoalFrame {
            action,
            body_intent: BodyIntent {
                locomotion: LocomotionMode::Arrive,
                target_position: target,
                target_surface: None,
                desired_speed: 0.25,
                facing_direction: 1.0,
                gaze_target: Some(target),
                pose: PoseIntent::Curious,
                expression: Default::default(),
                interaction_target: None,
            },
            affect: AffectState::default(),
            drives: Drives::initial(&genome.temperament),
            felt: FeltStateV1 {
                activation: 0.62,
                agency_match: 0.8,
                social_safety: 0.7,
                body_integrity: 1.0,
                ..FeltStateV1::default()
            },
            derived: DerivedNervousState::default(),
            attachment: 0.2,
            recent_outcome: None,
        }
    }

    #[test]
    fn target_and_sampled_style_are_locked_for_the_whole_bout() {
        let mut runtime = BehaviorPerformanceRuntime::new(42);
        let mut context = BehaviorContextFrame {
            frame_id: 1,
            ..BehaviorContextFrame::default()
        };
        context.body.motion.world_position = Vec2::new(0.2, 0.2);
        let first = runtime.tick(
            &goal(ActionId::ExploreScreen, Vec2::new(0.8, 0.7)),
            &context,
            0.05,
        );
        let style = runtime.active().expect("active").sampled_style;
        let locked = first.locomotion.target_position;
        context.frame_id = 2;
        let second = runtime.tick(
            &goal(ActionId::ExploreScreen, Vec2::new(0.1, 0.9)),
            &context,
            0.05,
        );
        assert_eq!(second.locomotion.target_position, locked);
        assert_eq!(runtime.active().expect("active").sampled_style, style);
    }

    #[test]
    fn sixteen_repeated_bouts_avoid_the_last_two_body_signatures_deterministically() {
        fn signatures(seed: u64) -> Vec<u8> {
            let position = Vec2::new(0.42, 0.51);
            let goal = goal(ActionId::HappyDisplay, position);
            let mut context = BehaviorContextFrame::default();
            context.body.motion.world_position = position;
            let mut runtime = BehaviorPerformanceRuntime::new(seed);
            let mut signatures = Vec::new();
            for _ in 0..16 {
                runtime.start(
                    BehaviorProgramId::PlayPlayBowAnalog,
                    crate::MotorCause::LabFixture,
                    &goal,
                    &context,
                );
                signatures.push(motion_signature(
                    runtime.active().expect("active bout").sampled_style,
                ));
            }
            signatures
        }

        let first = signatures(0x5164_B0A7);
        let replay = signatures(0x5164_B0A7);
        assert_eq!(first, replay);
        for index in 1..first.len() {
            assert_ne!(
                first[index],
                first[index - 1],
                "immediate repeat at {index}"
            );
            if index >= 2 {
                assert_ne!(first[index], first[index - 2], "two-back repeat at {index}");
            }
        }
    }

    #[test]
    fn replay_is_deterministic_and_every_packet_is_bounded() {
        let mut left = BehaviorPerformanceRuntime::new(77);
        let mut right = BehaviorPerformanceRuntime::new(77);
        let mut context = BehaviorContextFrame::default();
        context.body.motion.world_position = Vec2::new(0.25, 0.3);
        for frame in 1..240 {
            context.frame_id = frame;
            let goal = goal(ActionId::ExploreScreen, Vec2::new(0.78, 0.64));
            let a = left.tick(&goal, &context, 1.0 / 20.0);
            let b = right.tick(&goal, &context, 1.0 / 20.0);
            assert_eq!(a, b);
            assert!(a.field_count() <= 4);
            assert!((0.0..=1.5).contains(&a.locomotion.speed_multiplier));
            assert!((0.60..=1.40).contains(&a.material.density_compliance_multiplier));
        }
        assert_eq!(
            left.traces().cloned().collect::<Vec<_>>(),
            right.traces().cloned().collect::<Vec<_>>()
        );
    }

    #[test]
    fn integrity_interrupt_beats_positive_content() {
        let mut runtime = BehaviorPerformanceRuntime::new(5);
        let mut context = BehaviorContextFrame::default();
        context.body.topology.connected_components = 2;
        context.body.topology.detached_mass_fraction = 0.1;
        let mut goal = goal(ActionId::HappyDisplay, Vec2::new(0.5, 0.5));
        goal.affect.valence = 0.8;
        goal.affect.stress = 0.0;
        let packet = runtime.tick(&goal, &context, 0.05);
        assert_eq!(
            packet.program,
            Some(BehaviorProgramId::DefenseFragmentTrackAndRemerge)
        );
        assert!(packet.material.surface_tension_multiplier >= 1.2);
    }

    #[test]
    fn every_catalog_program_emits_a_bounded_non_cosmetic_packet_with_a_cause() {
        let base_goal = goal(ActionId::IdleHover, Vec2::new(0.72, 0.58));
        let mut context = BehaviorContextFrame::default();
        context.body.motion.world_position = Vec2::new(0.24, 0.31);
        context.den_anchor = Some(Vec2::new(0.04, 0.78));
        context.cursor_position = Vec2::new(0.56, 0.44);
        for (frame, program) in BehaviorProgramId::ALL.into_iter().enumerate() {
            let mut runtime = BehaviorPerformanceRuntime::new(91);
            context.frame_id = frame as u64 + 1;
            runtime.start(
                program,
                crate::MotorCause::BrainAction,
                &base_goal,
                &context,
            );
            let packet = runtime.tick(&base_goal, &context, 0.01);
            assert_eq!(packet.program, Some(program), "{}", program.wire_name());
            assert!(packet.phase.is_some(), "{}", program.wire_name());
            assert!(packet.field_count() <= crate::LOCAL_FIELD_BUDGET);
            assert!(packet.support.iter().count() <= crate::SURFACE_ATTACHMENT_BUDGET);
            let non_cosmetic = packet.locomotion.pose != crate::MotorPoseIntent::Neutral
                || packet.locomotion.speed_multiplier != 1.0
                || packet.material != crate::BoundedMaterialActuation::default()
                || packet.internal != crate::InternalPhysiologyActuation::default()
                || packet.field_count() > 0
                || packet.support.is_some();
            assert!(
                non_cosmetic,
                "{} emitted only presentation",
                program.wire_name()
            );
            let trace = runtime.traces().last().expect("causal motor trace");
            assert_eq!(trace.program, program);
            assert_eq!(trace.source_event, crate::MotorCause::BrainAction);
            assert!(!trace.phase_name.is_empty());
        }
    }

    #[test]
    fn all_64_programs_finish_their_phase_grammar_under_valid_numeric_evidence() {
        let base_goal = goal(ActionId::IdleHover, Vec2::new(0.72, 0.58));
        let mut context = BehaviorContextFrame::default();
        context.body.motion.world_position = Vec2::new(0.50, 0.949);
        context.body_bottom_extent = 0.05;
        context.screen_edge_gap_px = 1.0;
        context.screen_edge_support_stable_seconds = 1.0;
        context.screen_edge_supported = true;
        context.screen_edge_normal_velocity_px_s = 0.0;
        context.somatic.contact_fraction = 0.40;
        context.somatic.support_stability = 1.0;
        context.somatic.supported = true;
        context.pointer_released = true;
        context.gesture_ended = true;
        context.world_event = crate::MotorWorldEvent::OrbStored;
        context.surfaces.push(crate::SurfaceCandidate {
            surface_id: SurfaceId("screen:bottom_edge".into()),
            minimum: Vec2::new(0.0, 0.999),
            maximum: Vec2::ONE,
            velocity: Vec2::ZERO,
            familiarity: 1.0,
            recent_failed_landings: 0,
        });

        for program in BehaviorProgramId::ALL {
            let mut runtime = BehaviorPerformanceRuntime::new(0x6400 + program.index() as u64);
            runtime.start(
                program,
                crate::MotorCause::BrainAction,
                &base_goal,
                &context,
            );
            let mut active = runtime.active().expect("started program").clone();
            let mut finished = None;
            for _ in 0..4_096 {
                if let PhaseAdvance::Finished(reason) =
                    crate::advance_phase(&mut active, &context, 0.25)
                {
                    finished = Some(reason);
                    break;
                }
            }
            assert!(finished.is_some(), "{} did not finish", program.wire_name());
            assert!(
                active.total_time.is_finite() && active.total_time > 0.0,
                "{} invalid phase clock",
                program.wire_name()
            );
        }
    }

    #[test]
    fn body_lab_fixture_runs_all_64_without_autonomous_replacement() {
        let base_goal = goal(ActionId::IdleHover, Vec2::new(0.72, 0.58));
        let mut context = BehaviorContextFrame::default();
        context.body.motion.world_position = Vec2::new(0.50, 0.949);
        context.body_bottom_extent = 0.05;
        context.screen_edge_gap_px = 1.0;
        context.screen_edge_support_stable_seconds = 1.0;
        context.screen_edge_supported = true;
        context.screen_edge_normal_velocity_px_s = 0.0;
        context.somatic.contact_fraction = 0.40;
        context.somatic.support_stability = 1.0;
        context.somatic.supported = true;
        context.pointer_released = true;
        context.gesture_ended = true;
        context.surfaces.push(crate::SurfaceCandidate {
            surface_id: SurfaceId("screen:bottom_edge".into()),
            minimum: Vec2::new(0.0, 0.999),
            maximum: Vec2::ONE,
            velocity: Vec2::ZERO,
            familiarity: 1.0,
            recent_failed_landings: 0,
        });

        for program in BehaviorProgramId::ALL {
            let mut runtime = BehaviorPerformanceRuntime::new(0x1AB0 + program.index() as u64);
            runtime.begin_lab_fixture(program, &base_goal, &context);
            let first = runtime.tick_lab_fixture(&base_goal, &context, 0.01);
            assert_eq!(first.program, Some(program), "{}", program.wire_name());
            assert_eq!(first.cause, crate::MotorCause::LabFixture);
            for _ in 0..4_096 {
                if runtime.active().is_none() {
                    break;
                }
                let _ = runtime.tick_lab_fixture(&base_goal, &context, 0.25);
            }
            assert!(
                runtime.active().is_none(),
                "{} Lab bout did not finish",
                program.wire_name()
            );
        }
    }

    #[test]
    fn sustained_measured_contact_starts_hold_without_an_injected_label() {
        let mut runtime = BehaviorPerformanceRuntime::new(0xA01D);
        let position = Vec2::new(0.42, 0.51);
        let mut goal = goal(ActionId::IdleHover, position);
        goal.body_intent.desired_speed = 0.0;
        let mut context = BehaviorContextFrame::default();
        context.body.motion.world_position = position;
        context.pointer_down = true;
        context.pet_dragged = true;
        context.body.contact.contact_count = 1;
        context.body.contact.duration = 0.60;
        context.body.contact.tangential_speed = 0.02;
        context.somatic.maximum_strain = 0.18;

        let packet = runtime.tick(&goal, &context, 1.0 / 20.0);

        assert_eq!(
            packet.program,
            Some(BehaviorProgramId::TouchSustainedHoldRelaxOrResist)
        );
    }

    #[test]
    fn short_stationary_contact_starts_soft_touch_without_an_injected_label() {
        let mut runtime = BehaviorPerformanceRuntime::new(0x50F7);
        let position = Vec2::new(0.42, 0.51);
        let mut goal = goal(ActionId::IdleHover, position);
        goal.body_intent.desired_speed = 0.0;
        let mut context = BehaviorContextFrame::default();
        context.body.motion.world_position = position;
        context.pointer_down = true;
        context.pet_dragged = true;
        context.body.contact.contact_count = 1;
        context.body.contact.duration = 0.20;
        // The soft body may slide under a stationary pointer. That reactive
        // motion is not evidence that the user authored a pull.
        context.body.contact.tangential_speed = 0.14;
        context.somatic.maximum_strain = 0.31;

        let packet = runtime.tick(&goal, &context, 1.0 / 20.0);

        assert_eq!(packet.program, Some(BehaviorProgramId::TouchSoftTouchYield));
    }

    #[test]
    fn pressured_stationary_contact_reaches_the_hold_resistance_program() {
        let mut runtime = BehaviorPerformanceRuntime::new(0xB0A7);
        let position = Vec2::new(0.42, 0.51);
        let mut goal = goal(ActionId::IdleHover, position);
        goal.body_intent.desired_speed = 0.0;
        let mut context = BehaviorContextFrame::default();
        context.body.motion.world_position = position;
        context.pointer_down = true;
        context.pet_dragged = true;
        context.body.contact.contact_count = 1;
        context.body.contact.duration = 1.8;
        context.body.contact.pressure = 0.78;
        context.body.contact.tangential_speed = 0.04;
        context.somatic.maximum_strain = 0.96;
        context.boundary_violation = 0.51;

        let packet = runtime.tick(&goal, &context, 1.0 / 20.0);

        assert_eq!(
            packet.program,
            Some(BehaviorProgramId::TouchSustainedHoldRelaxOrResist)
        );
        assert!(packet.material.density_compliance_multiplier < 1.0);
    }

    #[test]
    fn local_pain_without_measured_contact_starts_the_specific_guard_program() {
        let mut runtime = BehaviorPerformanceRuntime::new(0xDA6E);
        let position = Vec2::new(0.42, 0.51);
        let mut goal = goal(ActionId::IdleHover, position);
        goal.felt.pain_like = 0.40;
        let mut context = BehaviorContextFrame::default();
        context.body.motion.world_position = position;

        let packet = runtime.tick(&goal, &context, 1.0 / 20.0);

        assert_eq!(
            packet.program,
            Some(BehaviorProgramId::DefenseLocalPainGuard)
        );
    }

    #[test]
    fn selected_sleep_uses_the_rest_controller_instead_of_restarting_den_escort() {
        let mut runtime = BehaviorPerformanceRuntime::new(0x51EE9);
        let position = Vec2::new(0.42, 0.51);
        let mut goal = goal(ActionId::Sleep, position);
        goal.drives.sleep = 0.82;
        goal.felt.sleep_pressure = 0.74;
        let mut context = BehaviorContextFrame::default();
        context.body.motion.world_position = position;
        context.world_goal = crate::MotorWorldGoal::SleepInDen;
        context.body_bottom_extent = 0.05;
        context.screen_edge_gap_px = 420.0;
        context.surfaces.push(crate::SurfaceCandidate {
            surface_id: SurfaceId("screen:bottom_edge".into()),
            minimum: Vec2::new(0.0, 0.999),
            maximum: Vec2::ONE,
            velocity: Vec2::ZERO,
            familiarity: 0.74,
            recent_failed_landings: 0,
        });

        let packet = runtime.tick(&goal, &context, 1.0 / 20.0);

        assert_eq!(
            packet.program,
            Some(BehaviorProgramId::RestSurfaceRoostSearch)
        );
    }

    #[test]
    fn sleep_support_acquisition_has_no_cooldown_gap() {
        let mut runtime = BehaviorPerformanceRuntime::new(0x51EEA);
        let position = Vec2::new(0.42, 0.51);
        let mut goal = goal(ActionId::Sleep, position);
        goal.drives.sleep = 0.82;
        let mut context = BehaviorContextFrame::default();
        context.body.motion.world_position = position;
        context.body_bottom_extent = 0.05;
        context.screen_edge_gap_px = 420.0;
        context.surfaces.push(crate::SurfaceCandidate {
            surface_id: SurfaceId("screen:bottom_edge".into()),
            minimum: Vec2::new(0.0, 0.999),
            maximum: Vec2::ONE,
            velocity: Vec2::ZERO,
            familiarity: 0.74,
            recent_failed_landings: 0,
        });
        runtime.cooldowns[BehaviorProgramId::RestSurfaceRoostSearch.index()] = 1.0;

        let packet = runtime.tick(&goal, &context, 1.0 / 20.0);

        assert_eq!(
            packet.program,
            Some(BehaviorProgramId::RestSurfaceRoostSearch)
        );
    }

    #[test]
    fn sleep_lands_when_it_reaches_the_inside_of_a_surface() {
        let mut runtime = BehaviorPerformanceRuntime::new(0x51EEB);
        let mut goal = goal(ActionId::Sleep, Vec2::new(0.5, 0.949));
        goal.drives.sleep = 0.82;
        let mut context = BehaviorContextFrame::default();
        context.body.motion.world_position = Vec2::new(0.5, 0.949);
        context.body_bottom_extent = 0.05;
        context.screen_edge_gap_px = 2.0;
        context.surfaces.push(crate::SurfaceCandidate {
            surface_id: SurfaceId("screen:bottom_edge".into()),
            minimum: Vec2::new(0.0, 0.999),
            maximum: Vec2::ONE,
            velocity: Vec2::ZERO,
            familiarity: 0.74,
            recent_failed_landings: 0,
        });

        let packet = runtime.tick(&goal, &context, 1.0 / 20.0);

        assert_eq!(
            packet.program,
            Some(BehaviorProgramId::RestLandingSoftTouchdown)
        );
        assert_eq!(
            packet.locomotion.target_position,
            Some(Vec2::new(0.5, 0.949))
        );
    }

    #[test]
    fn sleep_reaches_nrem_only_after_independent_edge_dwell() {
        let mut goal = goal(ActionId::Sleep, Vec2::new(0.5, 0.949));
        goal.drives.sleep = 0.82;
        let mut context = BehaviorContextFrame::default();
        context.body.motion.world_position = Vec2::new(0.5, 0.949);
        context.body_bottom_extent = 0.05;
        context.screen_edge_gap_px = 1.5;
        context.somatic.supported = true;
        context.surfaces.push(crate::SurfaceCandidate {
            surface_id: SurfaceId("screen:bottom_edge".into()),
            minimum: Vec2::new(0.0, 0.999),
            maximum: Vec2::ONE,
            velocity: Vec2::ZERO,
            familiarity: 1.0,
            recent_failed_landings: 0,
        });

        let mut before_dwell = BehaviorPerformanceRuntime::new(0x51EEC);
        let packet = before_dwell.tick(&goal, &context, 1.0 / 20.0);
        assert_eq!(
            packet.program,
            Some(BehaviorProgramId::RestLandingSoftTouchdown),
            "self-generated support must not skip measured contact"
        );

        context.screen_edge_supported = true;
        context.screen_edge_support_stable_seconds = 0.45;
        let mut settling = BehaviorPerformanceRuntime::new(0x51EED);
        let packet = settling.tick(&goal, &context, 1.0 / 20.0);
        assert_eq!(packet.program, Some(BehaviorProgramId::RestSitSettle));
        assert!(packet.expression.eye_aperture_delta > -0.5);

        context.screen_edge_support_stable_seconds = 0.95;
        let mut sleeping = BehaviorPerformanceRuntime::new(0x51EEE);
        let packet = sleeping.tick(&goal, &context, 1.0 / 20.0);
        assert_eq!(packet.program, Some(BehaviorProgramId::RestNremSleep));
        assert!(packet.expression.eye_aperture_delta <= -0.7);
        assert!(packet.internal.breath_speed_multiplier < 0.5);
    }

    #[test]
    fn critical_safety_threat_preempts_even_a_measured_hold() {
        let mut runtime = BehaviorPerformanceRuntime::new(0x5AFE);
        let position = Vec2::new(0.42, 0.51);
        let mut goal = goal(ActionId::IdleHover, position);
        goal.drives.safety = 0.82;
        let mut context = BehaviorContextFrame::default();
        context.body.motion.world_position = position;
        context.pointer_down = true;
        context.body.contact.contact_count = 1;
        context.body.contact.duration = 1.0;
        context.body.contact.tangential_speed = 0.02;

        let packet = runtime.tick(&goal, &context, 1.0 / 20.0);

        assert_eq!(
            packet.program,
            Some(BehaviorProgramId::DefenseThreatHardenCompact)
        );
    }

    #[test]
    fn measured_contact_motion_starts_pull_rebound_without_an_injected_label() {
        let mut runtime = BehaviorPerformanceRuntime::new(0xA011);
        let position = Vec2::new(0.42, 0.51);
        let mut goal = goal(ActionId::IdleHover, position);
        goal.body_intent.desired_speed = 0.0;
        let mut context = BehaviorContextFrame::default();
        context.body.motion.world_position = position;
        context.pointer_down = true;
        context.pet_dragged = true;
        context.body.contact.contact_count = 1;
        context.body.contact.duration = 0.20;
        context.cursor_velocity = Vec2::new(0.16, 0.0);
        context.body.contact.tangential_speed = 0.16;
        context.somatic.maximum_strain = 0.22;

        let packet = runtime.tick(&goal, &context, 1.0 / 20.0);

        assert_eq!(
            packet.program,
            Some(BehaviorProgramId::TouchPullReleaseRebound)
        );
    }

    #[test]
    fn completed_touch_classification_cannot_restart_a_stale_bout() {
        let mut runtime = BehaviorPerformanceRuntime::new(0x70AC);
        let position = Vec2::new(0.42, 0.51);
        let mut goal = goal(ActionId::IdleHover, position);
        goal.body_intent.desired_speed = 0.0;
        let mut context = BehaviorContextFrame::default();
        context.body.motion.world_position = position;
        context.gesture = lifecore::EmbodiedGestureKind::SoftTouch;
        context.gesture_confidence = 0.95;
        context.gesture_ended = true;
        let packet = runtime.tick(&goal, &context, 1.0 / 20.0);
        assert_ne!(packet.program, Some(BehaviorProgramId::TouchSoftTouchYield));
    }
}
