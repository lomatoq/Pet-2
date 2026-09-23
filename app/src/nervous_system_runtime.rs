//! Single-threaded orchestration for the R12 brain/body nervous system.
//!
//! The app owns the cadence and double buffering. Physics publishes frame N;
//! cognition consumes it once, produces bounded actuation N+1, and the body
//! applies that immutable packet on its following fixed step.

use lifecore::{
    BodyFeedbackV2, BodyIntent, BodyInteroceptionDirector, BodyPhenotypeDirector,
    EmbodiedGestureEvent, EmbodiedGestureKind, EmbodimentSourceFrame, EpisodeContextV1,
    FastPhenotypeActuation, GestureBoundaryEvent, GestureFrameV1, InteractionBodyActuation,
    InteroceptionSnapshot, LifeCore, PerceptionSelectionV1, SensorFrame, VitaSomaticFrame,
    VoiceFeedbackV1, VoiceGenome,
};
use morph_brain::{MorphBrain, nervous_system_frame};
use pet_audio::{AudioCallbackLevels, AudioVisualFeedback};
use pet_body::{
    CompanionExpressionDirector, ExpressionEvidence, NervousReadabilityTuning, ProceduralBody,
};
use pet_motor::{SomaticActuationBus, SomaticActuationPacket};

use crate::vita_runtime::VitaRuntime;

pub struct MotorActuationFrame<'a> {
    pub packet: &'a SomaticActuationPacket,
    pub context: &'a pet_motor::BehaviorContextFrame,
    pub scene_pose: Option<lifecore::FacePose>,
}

#[derive(Debug, Clone)]
pub struct NervousSystemRuntime {
    interoception: BodyInteroceptionDirector,
    phenotype: BodyPhenotypeDirector,
    body_feedback: BodyFeedbackV2,
    snapshot: InteroceptionSnapshot,
    actuation: FastPhenotypeActuation,
    voice_feedback: VoiceFeedbackV1,
    previous_voice_energy: f32,
    voice_mouth_open: f32,
    startle_face: f32,
    companion_expression: Option<CompanionExpressionDirector>,
    blink_owner: pet_body::BlinkOwner,
    blink_reason: pet_body::BlinkReason,
    ordinary_blink_active: bool,
    pub observed_ordinary_blinks: u64,
    final_gaze: pet_body::GazeController,
    gesture: GestureFrameV1,
    gesture_age: f32,
    episode: EpisodeContextV1,
    perception: PerceptionSelectionV1,
    next_body_frame_id: u64,
    consumed_body_frame: u64,
    body_time: f64,
    executed_locomotion: lifecore::LocomotionMode,
    last_failed_attempt: Option<(u64, u8)>,
    motor_outcome: EpisodeContextV1,
    offline_motor: Option<pet_motor::BehaviorPerformanceRuntime>,
    rendered_response_id: Option<u64>,
}

impl Default for NervousSystemRuntime {
    fn default() -> Self {
        Self {
            interoception: BodyInteroceptionDirector::default(),
            phenotype: BodyPhenotypeDirector::default(),
            body_feedback: BodyFeedbackV2::default(),
            snapshot: InteroceptionSnapshot::default(),
            actuation: FastPhenotypeActuation::default(),
            voice_feedback: VoiceFeedbackV1::default(),
            previous_voice_energy: 0.0,
            voice_mouth_open: 0.0,
            startle_face: 0.0,
            companion_expression: None,
            blink_owner: pet_body::BlinkOwner::Physiological,
            blink_reason: pet_body::BlinkReason::None,
            ordinary_blink_active: false,
            observed_ordinary_blinks: 0,
            final_gaze: pet_body::GazeController::default(),
            gesture: GestureFrameV1::default(),
            gesture_age: 0.0,
            episode: EpisodeContextV1::default(),
            perception: PerceptionSelectionV1::default(),
            next_body_frame_id: 1,
            consumed_body_frame: 0,
            body_time: 0.0,
            executed_locomotion: lifecore::LocomotionMode::Hover,
            last_failed_attempt: None,
            motor_outcome: EpisodeContextV1::default(),
            offline_motor: None,
            rendered_response_id: None,
        }
    }
}

impl NervousSystemRuntime {
    /// Recipes nominate a blink; they never own or reset eyelid animation.
    pub fn request_repertoire_blink(&mut self, request: pet_motor::RepertoireBlinkRequest) {
        if let Some(director) = &mut self.companion_expression {
            director.request_blink(pet_body::BlinkRequest {
                owner: if request.sleep_check {
                    pet_body::BlinkOwner::SleepCheck
                } else if request.duration_seconds >= 0.45 {
                    // The repertoire's long blink is emitted on the concrete
                    // social signal/response onset. Short release, irritation,
                    // and ordinary events remain physiological accents.
                    pet_body::BlinkOwner::Social
                } else {
                    pet_body::BlinkOwner::Physiological
                },
                strength: request.strength,
                duration: request.duration_seconds,
            });
        }
    }

    /// Samples the already-selected eyelid event at body cadence. Call once
    /// immediately before `ProceduralBody::embodied_update`; cognition owns
    /// event causes, while this method owns only the visible motor envelope.
    pub fn advance_eye_presentation(&mut self, intent: &mut BodyIntent, dt: f32) {
        let Some(director) = &mut self.companion_expression else {
            return;
        };
        let blink = director.present_blink(dt);
        self.blink_owner = blink.owner;
        self.blink_reason = blink.reason;
        let ordinary_active =
            blink.owner == pet_body::BlinkOwner::Physiological && blink.left > 0.05;
        if ordinary_active && !self.ordinary_blink_active {
            self.observed_ordinary_blinks = self.observed_ordinary_blinks.saturating_add(1);
        }
        self.ordinary_blink_active = ordinary_active;
        intent.expression.blink_left = blink.left;
        intent.expression.blink_right = blink.right;
        self.actuation.expression.blink_left = blink.left;
        self.actuation.expression.blink_right = blink.right;
    }

    #[must_use]
    pub const fn blink_reason(&self) -> pet_body::BlinkReason {
        self.blink_reason
    }

    /// Publishes the completed authoritative body state. The previous packet is
    /// passed back only to derive temporal quantities such as jerk.
    pub fn observe_body(
        &mut self,
        body: &ProceduralBody,
        intent: &BodyIntent,
        sensors: &SensorFrame,
    ) {
        self.body_time = sensors.timestamp;
        self.executed_locomotion = intent.locomotion;
        let previous = self.body_feedback;
        self.body_feedback =
            body.body_feedback_v2(intent, sensors, Some(&previous), self.next_body_frame_id);
        self.next_body_frame_id = self.next_body_frame_id.saturating_add(1);
    }

    /// Reduces real callback output to the voice's self-monitoring contract.
    /// No requested gain or planned mouth pose is treated as heard sound.
    pub fn observe_voice(
        &mut self,
        visual: AudioVisualFeedback,
        levels: AudioCallbackLevels,
        genome: &VoiceGenome,
    ) {
        let emitted =
            (0.55 * visual.envelope + 0.30 * levels.rms + 0.15 * visual.noisiness).clamp(0.0, 1.0);
        let spectral_flux = (emitted - self.previous_voice_energy).abs().clamp(0.0, 1.0);
        self.previous_voice_energy = emitted;
        self.voice_mouth_open = visual.mouth_open.clamp(0.0, 1.0);
        let octave_offset =
            (visual.pitch_normalized.clamp(0.0, 1.0) - 0.5) * genome.pitch_range_octaves;
        self.voice_feedback = VoiceFeedbackV1 {
            envelope: visual.envelope,
            phonating: visual.active && visual.envelope > 0.01,
            exhale_phase: if visual.active {
                (1.0 - visual.breath_pressure).clamp(0.0, 1.0)
            } else {
                0.0
            },
            spectral_flux,
            actual_pitch_hz: if visual.active {
                genome.base_pitch_hz * 2.0_f32.powf(octave_offset)
            } else {
                0.0
            },
            actual_loudness: levels.peak,
        };
        self.voice_feedback.sanitize(genome.maximum_loudness);
    }

    pub fn observe_gesture(&mut self, event: &EmbodiedGestureEvent) {
        self.gesture_age = 0.0;
        let classification = event.classification;
        let boundary_violation = match event.boundary {
            GestureBoundaryEvent::Overstrain
            | GestureBoundaryEvent::ExcessivePressure
            | GestureBoundaryEvent::TopologyBudgetExhausted => classification.intensity,
            GestureBoundaryEvent::None | GestureBoundaryEvent::QuietOrSleep => 0.0,
        };
        let positive_social = matches!(
            classification.kind,
            EmbodiedGestureKind::SoftTouch
                | EmbodiedGestureKind::Tickle
                | EmbodiedGestureKind::RhythmicTouch
        );
        self.gesture = GestureFrameV1 {
            episode_id: classification.episode_id,
            confidence: classification.confidence,
            ambiguity_margin: (classification.confidence - classification.second_best_confidence)
                .clamp(0.0, 1.0),
            novelty: classification.prediction_error,
            repetition_similarity: (1.0 - classification.prediction_error).clamp(0.0, 1.0),
            rhythm_phase: None,
            rhythm_strength: if classification.kind == EmbodiedGestureKind::RhythmicTouch {
                classification.confidence
            } else {
                0.0
            },
            boundary_violation,
            target_component: event
                .frame
                .detached_event
                .or(event.frame.remerge_event)
                .or(event.frame.recovery_event),
        };
        self.episode = EpisodeContextV1 {
            episode_id: classification.episode_id,
            open: !classification.ended,
            closed: classification.ended,
            reward_positive: if positive_social && classification.ended {
                classification.confidence * (1.0 - boundary_violation)
            } else {
                0.0
            },
            reward_negative: boundary_violation,
            // A recognized invitation is not evidence of successful play or execution.
            successful_play: 0.0,
            goal_congruent_motor_success: 0.0,
            rhythmic_synchrony: self.gesture.rhythm_strength,
            safe_social_exchange: if positive_social && classification.ended {
                classification.confidence * (1.0 - boundary_violation)
            } else {
                0.0
            },
            safe_predictable_episode: classification.prediction_confidence
                * (1.0 - boundary_violation),
            ignored_social_bid: 0.0,
            boundary_violation,
            repeated_intentional_failure: 0.0,
            novel_goal_congruent_episode: classification.prediction_error
                * classification.confidence
                * (1.0 - boundary_violation),
            sleeping_or_deep_rest: f32::from(event.boundary == GestureBoundaryEvent::QuietOrSleep),
            rest_quality: f32::from(event.boundary == GestureBoundaryEvent::QuietOrSleep),
            ..EpisodeContextV1::default()
        };
    }

    pub fn has_fresh_body(&self) -> bool {
        self.body_feedback.frame_id > self.consumed_body_frame
    }

    /// Shared live/headless boundary: body evidence always precedes cognition.
    pub fn prepare(
        &mut self,
        life: &mut LifeCore,
        vita: &mut VitaRuntime,
        morph: &mut MorphBrain,
        body: &ProceduralBody,
        dt: f32,
    ) {
        let tuning = body.tuning_profile();
        self.prepare_cognition_tick(
            life,
            vita,
            morph,
            tuning.interaction.soft_touch_pressure_max,
            tuning.nervous.for_live_runtime(),
            dt,
        );
    }

    #[allow(clippy::too_many_arguments)]
    pub fn apply_actuation(
        &mut self,
        life: &mut LifeCore,
        vita: &VitaRuntime,
        morph: &MorphBrain,
        body: &mut ProceduralBody,
        sensors: &mut SensorFrame,
        intent: &mut BodyIntent,
        dt: f32,
    ) {
        self.apply_offline_actuation(life, vita, morph, body, sensors, intent, None, dt);
    }

    #[allow(clippy::too_many_arguments)]
    pub fn apply_offline_actuation(
        &mut self,
        life: &mut LifeCore,
        vita: &VitaRuntime,
        morph: &MorphBrain,
        body: &mut ProceduralBody,
        sensors: &mut SensorFrame,
        intent: &mut BodyIntent,
        ecology: Option<&mut crate::EcologyRuntime>,
        dt: f32,
    ) {
        let pose = ecology
            .as_ref()
            .and_then(|e| crate::motor_scene_pose(e.active_episode()));
        let context =
            crate::motor_context::from_frames(self, vita, body, life, sensors, intent, ecology);
        let goal = lifecore::BehaviorGoalFrame {
            action: life.state.current_action,
            body_intent: intent.clone(),
            affect: life.state.affect,
            drives: life.state.drives,
            felt: self.snapshot.felt,
            derived: self.snapshot.derived,
            attachment: life.state.affect.attachment,
            recent_outcome: None,
        };
        let packet = self
            .offline_motor
            .get_or_insert_with(|| {
                pet_motor::BehaviorPerformanceRuntime::new(life.state.genome.identity_seed)
            })
            .tick(&goal, &context, dt);
        self.set_attention(
            packet.expression.gaze_target.or(intent.gaze_target),
            lifecore::AttentionTargetKind::ObjectGoal,
            context.orb_id,
            1.0,
        );
        self.apply_motor_actuation(
            life,
            vita,
            morph,
            body,
            sensors,
            intent,
            Some(MotorActuationFrame {
                packet: &packet,
                context: &context,
                scene_pose: pose,
            }),
            dt,
        );
    }

    #[allow(clippy::too_many_arguments)]
    pub fn apply_motor_actuation(
        &mut self,
        life: &mut LifeCore,
        vita: &VitaRuntime,
        morph: &MorphBrain,
        body: &mut ProceduralBody,
        sensors: &mut SensorFrame,
        intent: &mut BodyIntent,
        motor: Option<MotorActuationFrame<'_>>,
        dt: f32,
    ) {
        body.embodiment.managed_blink = true;
        let tuning = body.tuning_profile();
        let calibration = tuning.nervous.for_live_runtime();
        let mut phenotype = self.resolve_actuation(
            life,
            vita,
            morph,
            vita.interaction_actuation(),
            motor
                .as_ref()
                .map_or(&SomaticActuationPacket::default(), |m| m.packet),
            tuning.interaction.soft_touch_pressure_max,
            calibration,
            dt,
        );
        // resolve_actuation has already arbitrated companion physiology and
        // authored motor closures. Legacy phenotype/scene/VITA projection below
        // preserves or replaces old intent blinks, so retain the managed owner
        // separately and restore it at the final face boundary.
        let managed_blinks = [
            phenotype.expression.blink_left,
            phenotype.expression.blink_right,
        ];
        let managed_asymmetry = [
            phenotype.expression.brow_asymmetry,
            phenotype.expression.mouth_asymmetry,
        ];
        let protective = motor.as_ref().is_some_and(|m| {
            m.packet.regime.primary == pet_motor::SomaticRegime::Threatened
                || m.packet
                    .program
                    .is_some_and(|p| p.family() == pet_motor::ProgramFamily::DefenseIntegrity)
        });
        // Capture causal sleep BEFORE phenotype/VITA/scene writers change the
        // render pose. A transient Sleeping pose must not reset fixation state.
        let final_gaze_mode =
            causal_gaze_mode(motor.as_ref(), life.state.current_action, protective);
        body.embodiment.suppress_microsaccades = matches!(
            final_gaze_mode,
            pet_body::FixationGazeMode::PredictiveIntercept
                | pet_body::FixationGazeMode::AvoidantCheck
                | pet_body::FixationGazeMode::Sleep
        );
        calibration.apply(&mut phenotype);
        if let Some(motor) = &motor
            && let Some(pose) = motor.scene_pose
            && self.snapshot.felt.pain_like < 0.2
            && self.snapshot.felt.restraint < 0.18
            && self.snapshot.felt.startle < 0.35
            && !life.state.focus_mode
            && !protective
        {
            phenotype.expression = pose.expression();
        }
        phenotype.apply_to_intent(intent, sensors, body.simulation.feedback.world_position);
        if let Some(motor) = motor {
            SomaticActuationBus::apply_to_intent(motor.packet, motor.context, intent);
            body.set_somatic_actuation(motor.packet.clone());
        }
        life.learning
            .body
            .adapt_intent(intent, body.simulation.feedback.cursor_contact);
        if !protective {
            vita.apply_interaction_expression(intent, sensors, &body.simulation.feedback);
        }
        self.rendered_response_id = vita
            .active_interaction_plan()
            .filter(|plan| {
                !protective
                    && !life.state.focus_mode
                    && vita.interaction_turn().state == lifecore::InteractionTurnState::Responding
                    && vita.interaction_turn().elapsed_seconds >= plan.onset_seconds
            })
            .map(|plan| plan.response_id);
        restore_managed_blinks(&mut intent.expression, managed_blinks);
        if self.blink_owner == pet_body::BlinkOwner::SleepCheck && !protective {
            // Only the actual sleep owner may briefly inspect a contact. Keep
            // locomotion asleep; avoid scene aperture masking its one-eye check.
            intent.expression.eye_aperture = 1.0;
        }
        restore_managed_asymmetry(&mut intent.expression, managed_asymmetry);
        apply_startle_face(&mut intent.expression, self.startle_face);
        // Motor/VITA may overwrite the director's already-filtered gaze above.
        // Reconcile the selected attention AFTER every writer, so no raw cursor
        // or body-center target can bypass fixation continuity on presentation.
        let selected_gaze = self
            .perception
            .attention_target_position
            .or(intent.gaze_target)
            .or(Some(body.simulation.feedback.world_position));
        let gaze = self.final_gaze.tick(
            pet_body::GazePlan {
                primary_target: selected_gaze,
                mode: final_gaze_mode,
                acquire_tau: 0.12,
                confidence: 1.0,
                ..pet_body::GazePlan::default()
            },
            dt,
        );
        intent.gaze_target = gaze.target;
        phenotype.expression = intent.expression;
        phenotype.face.gaze_target = intent.gaze_target;
        sensors.interaction_actuation = phenotype.interaction;
        self.actuation = phenotype.clone();
        body.set_fast_phenotype_actuation(phenotype);
    }

    /// Called after the platform's final target/velocity constraints.
    pub fn commit_intent(
        &mut self,
        life: &mut LifeCore,
        body: &mut ProceduralBody,
        sensors: &SensorFrame,
        intent: &BodyIntent,
    ) {
        self.actuation.expression = intent.expression;
        self.actuation.face.gaze_target = intent.gaze_target;
        body.set_fast_phenotype_actuation(self.actuation.clone());
        let command = body.simulation.preview_motor_velocity(
            &life.state.genome.body,
            intent,
            sensors,
            1.0 / 120.0,
        );
        life.learning
            .body
            .begin(self.body_feedback, command, self.body_time);
    }

    /// Called after two measured failed impulses against the same object goal.
    pub fn observe_repeated_motor_failure(&mut self, goal_id: u64, attempts: u8) {
        if attempts >= 2 && self.last_failed_attempt != Some((goal_id, attempts)) {
            self.last_failed_attempt = Some((goal_id, attempts));
            self.motor_outcome.repeated_intentional_failure = 0.5;
        }
    }

    pub fn observe_outcomes(&mut self, outcomes: &[pet_ecology::EcologyOutcome]) {
        for outcome in outcomes {
            // Generic completion can mean only a timeout or a queued command.
            // OfferOrb is completed by same-object user displacement; contact is measured.
            match outcome {
                pet_ecology::EcologyOutcome::EpisodeCompleted(
                    pet_ecology::EpisodeGoal::OfferOrb,
                ) => {
                    self.motor_outcome.successful_play = 1.0;
                    self.motor_outcome.goal_congruent_motor_success = 1.0;
                }
                pet_ecology::EcologyOutcome::ObjectContact(_) => {
                    self.motor_outcome.goal_congruent_motor_success = 1.0;
                }
                _ => {}
            }
        }
    }

    pub fn set_attention(
        &mut self,
        position: Option<glam::Vec2>,
        kind: lifecore::AttentionTargetKind,
        id: Option<u64>,
        confidence: f32,
    ) {
        self.perception.attention_target_position = position.filter(|p| p.is_finite());
        self.perception.attention_target_kind = kind;
        self.perception.attention_target_id = id;
        self.perception.attention_confidence = confidence.clamp(0.0, 1.0);
    }

    pub fn set_selected_salience(&mut self, salience: f32) {
        self.perception.selected_salience = if salience.is_finite() {
            salience.clamp(0.0, 1.0)
        } else {
            0.0
        };
    }

    /// Computes and integrates the body evidence that cognition may consume on
    /// this tick. Brain values in the source are from the preceding completed
    /// cognition tick, so the graph has no algebraic same-tick cycle.
    pub fn prepare_cognition_tick(
        &mut self,
        life: &mut LifeCore,
        vita: &mut VitaRuntime,
        morph: &mut MorphBrain,
        soft_touch_pressure_max: f32,
        calibration: NervousReadabilityTuning,
        dt: f32,
    ) {
        if !self.has_fresh_body() {
            return;
        }
        self.consumed_body_frame = self.body_feedback.frame_id;
        life.learning
            .body
            .complete(self.body_feedback, self.body_time);
        let motion = self.body_feedback.motion.velocity;
        let commanded = self.body_feedback.efference_copy.intended_velocity;
        let command = if motion.length() > 0.005 && motion.dot(commanded) > 0.0 {
            match self.executed_locomotion {
                lifecore::LocomotionMode::Flee => morph_brain::MorphCommand::Flee,
                lifecore::LocomotionMode::Seek | lifecore::LocomotionMode::Arrive => {
                    morph_brain::MorphCommand::Approach
                }
                lifecore::LocomotionMode::Orbit => morph_brain::MorphCommand::Play,
                _ => morph_brain::MorphCommand::Idle,
            }
        } else {
            morph_brain::MorphCommand::Idle
        };
        morph.acknowledge_execution(command);
        if let Some(response_id) = self.rendered_response_id.take()
            && vita
                .active_interaction_plan()
                .is_some_and(|plan| plan.response_id == response_id)
        {
            life.acknowledge_interaction_execution(response_id);
        }
        self.gesture_age += dt.clamp(0.0, 0.25);
        if self.gesture_age > 1.0 {
            self.gesture = GestureFrameV1::default();
            self.episode = EpisodeContextV1::default();
        }
        let motor_outcome = std::mem::take(&mut self.motor_outcome);
        self.episode.successful_play = motor_outcome.successful_play;
        self.episode.goal_congruent_motor_success = motor_outcome.goal_congruent_motor_success;
        self.episode.repeated_intentional_failure = motor_outcome.repeated_intentional_failure;
        let source = self.source(life, vita, morph, soft_touch_pressure_max, calibration);
        self.snapshot = self.interoception.tick(&source, dt);
        let mut episode = self.episode;
        episode.user_absent = f32::from(!self.body_feedback.environment.user_present);
        let f = self.snapshot.felt;
        let positive = (0.28 * f.contact_pleasantness
            + 0.30 * episode.successful_play
            + 0.24 * episode.goal_congruent_motor_success
            + 0.18 * episode.rhythmic_synchrony)
            .clamp(0.0, 1.0);
        let negative = (0.34 * f.pain_like
            + 0.24 * episode.boundary_violation
            + 0.20 * f.restraint
            + 0.22 * episode.repeated_intentional_failure)
            .clamp(0.0, 1.0);
        // A painful or boundary-violating action is never reinforced because it
        // happened to be salient or produced a large response.
        episode.reward_positive = if f.pain_like > 0.20 || episode.boundary_violation > 0.20 {
            0.0
        } else {
            positive
        };
        episode.reward_negative = negative;
        life.integrate_felt_state(self.snapshot, episode, dt);
        vita.integrate_felt_state(self.snapshot.felt, episode, dt);
        morph.set_somatic_input(self.snapshot.morph_sensors);
        self.episode = episode;
        // Outcome impulses are consumed once; LifeCore owns their inertia.
        self.episode.goal_congruent_motor_success = 0.0;
        self.episode.successful_play = 0.0;
        self.episode.repeated_intentional_failure = 0.0;
        if episode.closed {
            self.gesture = GestureFrameV1::default();
            self.episode = EpisodeContextV1 {
                episode_id: episode.episode_id,
                ..EpisodeContextV1::default()
            };
        }
    }

    /// Compiles the newly completed cognition state to one bounded actuation
    /// packet and merges the existing VITA gesture response at the same owner
    /// boundary.
    #[must_use]
    #[allow(clippy::too_many_arguments)] // Explicit owners plus immutable calibration and cadence.
    pub fn resolve_actuation(
        &mut self,
        life: &LifeCore,
        vita: &VitaRuntime,
        morph: &MorphBrain,
        vita_interaction: InteractionBodyActuation,
        motor_actuation: &SomaticActuationPacket,
        soft_touch_pressure_max: f32,
        calibration: NervousReadabilityTuning,
        dt: f32,
    ) -> FastPhenotypeActuation {
        let startle_target = if motor_actuation.program
            == Some(pet_motor::BehaviorProgramId::DefenseStartleOrientFreeze)
        {
            if motor_actuation.phase_name == "startle_recover" {
                1.0 - motor_actuation.phase_progress
            } else {
                1.0
            }
        } else {
            0.0
        };
        let rate = if startle_target > self.startle_face {
            24.0
        } else {
            5.0
        };
        self.startle_face +=
            (startle_target - self.startle_face) * (1.0 - (-rate * dt.clamp(0.0, 0.1)).exp());
        let source = self.source(life, vita, morph, soft_touch_pressure_max, calibration);
        let mut actuation = self.phenotype.tick(&source, self.snapshot, dt);
        merge_interaction(&mut actuation.interaction, vita_interaction);

        let director = self.companion_expression.get_or_insert_with(|| {
            CompanionExpressionDirector::new(life.state.genome.identity_seed)
        });
        let actual = self.body_feedback.efference_copy.actual_velocity;
        let intended = self.body_feedback.efference_copy.intended_velocity;
        let contact_point = (self.body_feedback.contact.contact_count > 0)
            .then_some(self.body_feedback.contact.point_world);
        let companion = director.tick(
            vita.companion_intent(),
            ExpressionEvidence {
                body_position: self.body_feedback.motion.world_position,
                contact_point,
                contact_pressure: self.body_feedback.contact.pressure,
                contact_strain: self.body_feedback.shape.maximum_strain,
                body_speed: self.body_feedback.motion.velocity.length(),
                motor_error: (actual - intended).length().clamp(0.0, 1.0),
                audio_mouth_open: self.voice_mouth_open,
                audio_active: self.voice_feedback.phonating,
                target_velocity: self.body_feedback.motion.velocity,
                protective_reflex: self.startle_face > 0.25
                    || self.snapshot.felt.startle > 0.55
                    || self.snapshot.felt.restraint > 0.72,
                pain_like: self.snapshot.felt.pain_like,
            },
            dt,
        );
        apply_companion_expression(&mut actuation, companion);
        self.blink_owner = companion.blink_owner;
        self.blink_reason = companion.blink_reason;
        // Motor physiology and defensive ownership must survive R14's face layer.
        SomaticActuationBus::compose(&mut actuation, motor_actuation);
        if companion.blink_owner == pet_body::BlinkOwner::SleepCheck {
            actuation.expression.blink_left = companion.blink_left;
            actuation.expression.blink_right = companion.blink_right;
        }
        self.actuation = actuation.clone();
        actuation
    }

    #[must_use]
    pub const fn snapshot(&self) -> InteroceptionSnapshot {
        self.snapshot
    }

    #[must_use]
    pub fn actuation(&self) -> &FastPhenotypeActuation {
        &self.actuation
    }

    #[must_use]
    pub const fn body_feedback(&self) -> BodyFeedbackV2 {
        self.body_feedback
    }

    fn source(
        &self,
        life: &LifeCore,
        vita: &VitaRuntime,
        morph: &MorphBrain,
        soft_touch_pressure_max: f32,
        calibration: NervousReadabilityTuning,
    ) -> EmbodimentSourceFrame {
        let vita_state = vita.state();
        let self_model = &vita_state.self_model;
        let mut episode = self.episode;
        episode.user_absent = f32::from(!self.body_feedback.environment.user_present);
        let mut source = EmbodimentSourceFrame {
            frame_id: self.body_feedback.frame_id,
            affect: life.state.affect,
            drives: life.state.drives,
            temperament: life.state.genome.temperament.clone(),
            voice_seed: life.state.genome.voice.voice_seed,
            vita: VitaSomaticFrame {
                appraisal: vita_state.appraisal,
                mood: vita_state.mood,
                prediction_error: self_model.prediction_error,
                agency: self_model.agency,
                uncertainty: self_model.uncertainty,
                body_schema_confidence: self_model.body_schema_confidence,
                calibration_urge: self_model.calibration_urge,
                external_force_likelihood: self_model.external_force_likelihood,
                attention_confidence: vita_state.attention.confidence,
                attention_commitment_remaining: vita_state.attention.commitment_remaining,
            },
            morph: nervous_system_frame(morph.last_output(), morph.diagnostics()),
            body: self.body_feedback,
            voice_feedback: self.voice_feedback,
            gesture: self.gesture,
            episode,
            perception: self.perception,
            soft_touch_pressure_max: soft_touch_pressure_max.clamp(0.03, 0.50),
        };
        apply_input_sensitivity(&mut source, calibration);
        source
    }
}

fn causal_gaze_mode(
    motor: Option<&MotorActuationFrame<'_>>,
    action: lifecore::ActionId,
    protective: bool,
) -> pet_body::FixationGazeMode {
    let sleeping = motor.map_or(action == lifecore::ActionId::Sleep, |motor| {
        // Support validity is already owned by motor selection; do not reset
        // the eyes on individual noisy physical-contact samples here.
        motor.packet.locomotion.pose == pet_motor::MotorPoseIntent::SupportedSleep
            && (motor.packet.program == Some(pet_motor::BehaviorProgramId::RestNremSleep)
                || motor.context.companion_intent == lifecore::PrimaryIntent::Sleep
                || action == lifecore::ActionId::Sleep)
    });
    if protective {
        pet_body::FixationGazeMode::AvoidantCheck
    } else if sleeping {
        pet_body::FixationGazeMode::Sleep
    } else if motor.is_some_and(|motor| {
        matches!(
            motor.context.companion_intent,
            lifecore::PrimaryIntent::Intercept
                | lifecore::PrimaryIntent::Chase
                | lifecore::PrimaryIntent::Catch
        )
    }) {
        pet_body::FixationGazeMode::PredictiveIntercept
    } else {
        pet_body::FixationGazeMode::Track
    }
}

fn restore_managed_blinks(expression: &mut lifecore::ExpressionState, blinks: [f32; 2]) {
    expression.blink_left = if blinks[0].is_finite() {
        blinks[0].clamp(0.0, 1.0)
    } else {
        0.0
    };
    expression.blink_right = if blinks[1].is_finite() {
        blinks[1].clamp(0.0, 1.0)
    } else {
        0.0
    };
}

fn restore_managed_asymmetry(expression: &mut lifecore::ExpressionState, asymmetry: [f32; 2]) {
    expression.brow_asymmetry = if asymmetry[0].is_finite() {
        asymmetry[0].clamp(-1.0, 1.0)
    } else {
        0.0
    };
    expression.mouth_asymmetry = if asymmetry[1].is_finite() {
        asymmetry[1].clamp(-1.0, 1.0)
    } else {
        0.0
    };
}

// The defensive motor bout is the shared cause of the recoil and the face.
// Acoustic onsets already reject the pet's own voice in HearingBridge.
fn apply_startle_face(expression: &mut lifecore::ExpressionState, strength: f32) {
    let weight = strength.clamp(0.0, 1.0);
    let startled = lifecore::FacePose::Startled.expression();
    let blend = |from: f32, to: f32| from + (to - from) * weight;
    expression.eye_aperture = blend(expression.eye_aperture, 1.0);
    expression.squint *= 1.0 - weight;
    expression.brow_raise = blend(expression.brow_raise, startled.brow_raise.max(0.8));
    expression.brow_tension = blend(expression.brow_tension, 0.12);
    expression.mouth_curve = blend(expression.mouth_curve, 0.0);
    expression.mouth_open = blend(expression.mouth_open, expression.mouth_open.max(0.48));
    for eye in 0..2 {
        for channel in 0..4 {
            expression.geometry.lids[eye][channel] = blend(
                expression.geometry.lids[eye][channel],
                startled.geometry.lids[eye][channel],
            );
            expression.geometry.brows[eye][channel] = blend(
                expression.geometry.brows[eye][channel],
                startled.geometry.brows[eye][channel],
            );
        }
    }
}

#[test]
fn motor_startle_recruits_eyes_brows_and_mouth_without_erasing_blinks() {
    let original = lifecore::FacePose::Boundary.expression();
    let mut expression = original;
    apply_startle_face(&mut expression, 0.0);
    assert_eq!(expression, original);
    expression.blink_left = 0.7;
    apply_startle_face(&mut expression, 1.0);
    assert!(expression.brow_raise >= 0.8 && expression.squint < 0.01);
    assert!(expression.geometry.lids[0][0] > 0.1);
    assert!(expression.mouth_open >= 0.48);
    assert_eq!(expression.blink_left, 0.7);
}

fn apply_companion_expression(
    actuation: &mut FastPhenotypeActuation,
    target: pet_body::CompanionExpressionTarget,
) {
    let face = target.face;
    actuation.expression.eye_aperture = face.eye_aperture;
    actuation.expression.squint = face.squint;
    actuation.expression.pupil_size = face.pupil_size;
    actuation.expression.pupil_focus = face.pupil_focus;
    actuation.expression.brow_raise = face.brow_raise;
    actuation.expression.brow_tension = face.brow_tension;
    actuation.expression.brow_asymmetry = face.brow_asymmetry;
    actuation.expression.mouth_curve = face.mouth_curve;
    actuation.expression.mouth_open = face.mouth_open;
    actuation.expression.mouth_tension = face.mouth_tension;
    actuation.expression.mouth_compression = face.mouth_compression;
    actuation.expression.mouth_asymmetry = face.mouth_asymmetry;
    actuation.expression.blink_left = target.blink_left;
    actuation.expression.blink_right = target.blink_right;
    actuation.expression.body_glow = target.body.glow;
    actuation.expression.cheek_glow = (target.body.glow * 0.65).clamp(0.0, 1.0);
    actuation.face.gaze_target = target.gaze;
    actuation.pbf.viscosity_multiplier =
        (actuation.pbf.viscosity_multiplier * target.body.viscosity_multiplier).clamp(0.65, 1.50);
    actuation.pbf.surface_tension_multiplier = (actuation.pbf.surface_tension_multiplier
        * target.body.surface_tension_multiplier)
        .clamp(0.75, 1.35);
    actuation.pbf.motor_gain_multiplier = (actuation.pbf.motor_gain_multiplier
        * (0.82 + target.body.contact_yield * 0.30))
        .clamp(0.50, 1.35);
    actuation.visual_physiology.pulse_amplitude = actuation
        .visual_physiology
        .pulse_amplitude
        .max(target.body.internal_pulse);
    actuation.visual_physiology.core_glow_multiplier =
        (actuation.visual_physiology.core_glow_multiplier * (0.82 + target.body.glow * 0.55))
            .clamp(0.55, 1.45);
}

fn apply_input_sensitivity(
    source: &mut EmbodimentSourceFrame,
    calibration: NervousReadabilityTuning,
) {
    let scaled = |value: f32, gain: f32| (value * gain).clamp(0.0, 1.0);
    let around_half = |value: f32, gain: f32| (0.5 + (value - 0.5) * gain).clamp(0.0, 1.0);

    source.affect.stress = scaled(source.affect.stress, calibration.threat_sensitivity);
    source.drives.safety = scaled(source.drives.safety, calibration.threat_sensitivity);
    source.vita.appraisal.threat =
        scaled(source.vita.appraisal.threat, calibration.threat_sensitivity);
    source.episode.reward_negative = scaled(
        source.episode.reward_negative,
        calibration.threat_sensitivity,
    );

    source.body.contact.pressure =
        scaled(source.body.contact.pressure, calibration.pain_sensitivity);
    source.body.shape.maximum_strain = scaled(
        source.body.shape.maximum_strain,
        calibration.pain_sensitivity,
    );
    source.body.shape.neck_tension =
        scaled(source.body.shape.neck_tension, calibration.pain_sensitivity);
    source.body.shape.deformation_energy = scaled(
        source.body.shape.deformation_energy,
        calibration.pain_sensitivity,
    );

    source.body.contact.area = scaled(source.body.contact.area, calibration.contact_sensitivity);
    source.body.contact.tangential_speed = scaled(
        source.body.contact.tangential_speed,
        calibration.contact_sensitivity,
    );

    source.episode.safe_social_exchange = scaled(
        source.episode.safe_social_exchange,
        calibration.safety_sensitivity,
    );
    source.episode.safe_predictable_episode = scaled(
        source.episode.safe_predictable_episode,
        calibration.safety_sensitivity,
    );
    source.episode.reward_positive = scaled(
        source.episode.reward_positive,
        calibration.safety_sensitivity,
    );

    source.body.environment.clipped_fraction = scaled(
        source.body.environment.clipped_fraction,
        calibration.restraint_sensitivity,
    );
    source.body.environment.available_motion_radius = (1.0
        - (1.0 - source.body.environment.available_motion_radius)
            * calibration.restraint_sensitivity)
        .clamp(0.0, 1.0);
    source.episode.boundary_violation = scaled(
        source.episode.boundary_violation,
        calibration.restraint_sensitivity,
    );

    source.drives.sleep = scaled(source.drives.sleep, calibration.fatigue_sensitivity);
    source.vita.mood.fatigue = scaled(source.vita.mood.fatigue, calibration.fatigue_sensitivity);

    source.vita.appraisal.novelty = scaled(
        source.vita.appraisal.novelty,
        calibration.novelty_sensitivity,
    );
    source.gesture.novelty = scaled(source.gesture.novelty, calibration.novelty_sensitivity);
    source.perception.selected_salience = scaled(
        source.perception.selected_salience,
        calibration.novelty_sensitivity,
    );

    source.body.motion.collision_impulse = scaled(
        source.body.motion.collision_impulse,
        calibration.startle_sensitivity,
    );
    source.body.environment.cursor_loom_rate = scaled(
        source.body.environment.cursor_loom_rate,
        calibration.startle_sensitivity,
    );

    source.vita.agency = around_half(source.vita.agency, calibration.agency_sensitivity);
    source.vita.body_schema_confidence = around_half(
        source.vita.body_schema_confidence,
        calibration.agency_sensitivity,
    );
    source.body.sanitize();
}

fn merge_interaction(phenotype: &mut InteractionBodyActuation, vita: InteractionBodyActuation) {
    phenotype.target_component = vita.target_component.or(phenotype.target_component);
    phenotype.compliance_delta =
        (phenotype.compliance_delta + vita.compliance_delta).clamp(-0.25, 0.25);
    phenotype.cohesion_delta = (phenotype.cohesion_delta + vita.cohesion_delta).clamp(-0.25, 0.25);
    phenotype.local_pulse = phenotype.local_pulse.max(vita.local_pulse);
    phenotype.lean = (phenotype.lean + vita.lean).clamp(-0.08, 0.08);
    phenotype.recoil = phenotype.recoil.max(vita.recoil);
    phenotype.resistance = phenotype.resistance.max(vita.resistance);
    phenotype.cooperation = phenotype.cooperation.max(vita.cooperation);
    phenotype.allow_intentional_bud |= vita.allow_intentional_bud;
    phenotype.sanitize();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn managed_asymmetry_survives_scene_without_replacing_geometry() {
        let mut expression = lifecore::FacePose::Curious.expression();
        let geometry = expression.geometry;
        restore_managed_asymmetry(&mut expression, [-0.18, 0.12]);
        assert_eq!(expression.brow_asymmetry, -0.18);
        assert_eq!(expression.mouth_asymmetry, 0.12);
        assert_eq!(expression.geometry, geometry);
    }

    #[test]
    fn managed_physiological_blinks_survive_legacy_and_scene_projection() {
        let mut scheduler = pet_body::BlinkController::new(913);
        let mut smoothed = pet_body::ExpressionRuntime::default();
        smoothed.managed_actions = true;
        let mut intent = LifeCore::new(lifecore::Genome::from_seed(7), 11)
            .tick(
                &SensorFrame::default(),
                &lifecore::BodyFeedback::default(),
                1.0 / 60.0,
            )
            .body_intent;
        let mut peaks = 0;
        let mut closed = false;
        for _ in 0..60 * 30 {
            let blink = scheduler.tick(1.0 / 60.0, false, false, 0.2);
            let mut phenotype = FastPhenotypeActuation::default();
            phenotype.expression.blink_left = blink.left;
            phenotype.expression.blink_right = blink.right;
            let managed = [
                phenotype.expression.blink_left,
                phenotype.expression.blink_right,
            ];
            // This is the actual scene replacement + legacy projection which
            // previously discarded every managed physiological blink.
            phenotype.expression = lifecore::FacePose::Awake.expression();
            intent.expression.blink_left = 0.0;
            intent.expression.blink_right = 0.0;
            phenotype.apply_to_intent(&mut intent, &SensorFrame::default(), glam::Vec2::splat(0.5));
            assert_eq!(intent.expression.blink_left, 0.0);
            restore_managed_blinks(&mut intent.expression, managed);
            smoothed.update(intent.expression, 0.0, 1.0 / 60.0);
            let now_closed =
                smoothed.current.blink_left > 0.5 && smoothed.current.blink_right > 0.5;
            if now_closed && !closed {
                peaks += 1;
            }
            closed = now_closed;
        }
        assert!((1..=8).contains(&peaks), "managed blink peaks={peaks}");
        restore_managed_blinks(&mut intent.expression, [0.0, 0.0]);
        assert_eq!(
            intent.expression.blink_left, 0.0,
            "opening must not latch prior closure"
        );
    }

    #[test]
    fn body_rate_hook_delivers_unfiltered_asymmetric_blink_to_render_pose() {
        for hz in [30, 60, 120] {
            let genome = lifecore::Genome::from_seed(0xE1E5);
            let mut body = ProceduralBody::generate(&genome).unwrap();
            body.embodiment.managed_blink = true;
            let mut nervous = NervousSystemRuntime {
                companion_expression: Some(CompanionExpressionDirector::new(genome.identity_seed)),
                ..Default::default()
            };
            nervous.request_repertoire_blink(pet_motor::RepertoireBlinkRequest {
                strength: 1.0,
                duration_seconds: 0.29,
                sleep_check: false,
            });
            let mut intent = LifeCore::new(genome.clone(), 11)
                .tick(
                    &SensorFrame::default(),
                    &lifecore::BodyFeedback::default(),
                    1.0 / hz as f32,
                )
                .body_intent;
            intent.expression.eye_aperture = 1.0;
            let mut visible = Vec::new();
            for _ in 0..hz {
                nervous.advance_eye_presentation(&mut intent, 1.0 / hz as f32);
                body.embodied_update(
                    &intent,
                    &SensorFrame::default(),
                    lifecore::AffectState::default(),
                    pet_body::VisualMindInput::default(),
                    pet_body::VoiceVisualState::default(),
                    1.0 / hz as f32,
                );
                visible.push(body.embodiment.pose.blink_left);
            }
            let peak = visible
                .iter()
                .position(|sample| *sample >= 0.98)
                .expect("visible closure peak");
            let reopened = visible[peak..]
                .iter()
                .position(|sample| *sample <= 0.01)
                .map(|offset| peak + offset)
                .expect("visible opening tail completes");
            assert!(peak < reopened - peak, "hz={hz} visible={visible:?}");
            assert!(visible.len() >= 7);
        }
    }

    #[test]
    fn causal_gaze_sleep_cannot_be_inferred_from_rest_or_scene_pose() {
        let context = pet_motor::BehaviorContextFrame::default();
        let mut packet = SomaticActuationPacket {
            program: Some(pet_motor::BehaviorProgramId::RestSitSettle),
            ..Default::default()
        };
        packet.locomotion.pose = pet_motor::MotorPoseIntent::SupportedRest;
        let target = glam::Vec2::new(0.3523256, 0.7131944);
        let mut gaze = pet_body::GazeController::default();
        for tick in 0..100 {
            // Both scene expression and a changing high-level sleep nomination
            // must leave an awake supported-rest motor performance tracking.
            let motor = MotorActuationFrame {
                packet: &packet,
                context: &context,
                scene_pose: if tick % 2 == 0 {
                    Some(lifecore::FacePose::Tired)
                } else {
                    None
                },
            };
            let mode = causal_gaze_mode(
                Some(&motor),
                if tick % 2 == 0 {
                    lifecore::ActionId::Sleep
                } else {
                    lifecore::ActionId::IdleHover
                },
                false,
            );
            assert_eq!(mode, pet_body::FixationGazeMode::Track);
            let output = gaze.tick(
                pet_body::GazePlan {
                    primary_target: Some(target),
                    mode,
                    acquire_tau: 0.12,
                    confidence: 1.0,
                    ..Default::default()
                },
                0.05,
            );
            assert!(output.target.is_some(), "rest must never reset fixation");
            if tick > 30 {
                assert!(output.target.unwrap().distance(target) < 0.001);
            }
        }
        packet.program = Some(pet_motor::BehaviorProgramId::RestNremSleep);
        packet.locomotion.pose = pet_motor::MotorPoseIntent::SupportedSleep;
        let motor = MotorActuationFrame {
            packet: &packet,
            context: &context,
            scene_pose: None,
        };
        assert_eq!(
            causal_gaze_mode(Some(&motor), lifecore::ActionId::Sleep, false),
            pet_body::FixationGazeMode::Sleep
        );
        assert_eq!(
            causal_gaze_mode(Some(&motor), lifecore::ActionId::Sleep, true),
            pet_body::FixationGazeMode::AvoidantCheck
        );
    }

    #[test]
    fn final_gaze_owner_survives_motor_writes_through_rendered_pupils() {
        let mut nervous = NervousSystemRuntime::default();
        let mut life = LifeCore::new(lifecore::Genome::from_seed(7), 11);
        let vita = VitaRuntime::new(7, None);
        let morph = MorphBrain::new(7, None).unwrap();
        let mut body = ProceduralBody::generate(&life.state.genome).unwrap();
        let mut sensors = SensorFrame::default();
        let mut intent = life
            .tick(&sensors, &lifecore::BodyFeedback::default(), 0.05)
            .body_intent;
        let context = pet_motor::BehaviorContextFrame::default();
        let mut packet = SomaticActuationPacket::default();
        for tick in 0..180 {
            let target = if tick < 20 {
                glam::Vec2::splat(0.5)
            } else if tick < 100 {
                glam::Vec2::new(if tick % 2 == 0 { 0.08 } else { 0.92 }, 0.5)
            } else {
                glam::Vec2::new(0.7, 0.5)
            };
            packet.expression.gaze_target = Some(target);
            nervous.set_attention(
                Some(target),
                lifecore::AttentionTargetKind::ObjectGoal,
                None,
                1.0,
            );
            nervous.apply_motor_actuation(
                &mut life,
                &vita,
                &morph,
                &mut body,
                &mut sensors,
                &mut intent,
                Some(MotorActuationFrame {
                    packet: &packet,
                    context: &context,
                    scene_pose: None,
                }),
                0.05,
            );
            // Simulate a legacy scene/VITA render-pose writer after the causal
            // final gaze owner. The body must not reinterpret this as sleep.
            if tick >= 100 {
                intent.pose = if tick % 2 == 0 {
                    lifecore::PoseIntent::Sleeping
                } else {
                    lifecore::PoseIntent::Compact
                };
            }
            nervous.commit_intent(&mut life, &mut body, &sensors, &intent);
            body.embodied_update(
                &intent,
                &sensors,
                life.state.affect,
                pet_body::VisualMindInput::default(),
                pet_body::VoiceVisualState::default(),
                0.05,
            );
            body.presentation_update(0.05);
            if (20..100).contains(&tick) {
                assert!(
                    body.embodiment.semantic_gaze_target().length() < 0.005,
                    "raw motor gaze bypassed final owner: tick={tick} semantic={:?}",
                    body.embodiment.semantic_gaze_target()
                );
                assert!(
                    body.embodiment.microsaccade_offset().length() <= 0.042_001,
                    "eye-local offset exceeded its bound: tick={tick} offset={:?}",
                    body.embodiment.microsaccade_offset()
                );
            }
            if tick > 140 {
                assert!(
                    body.embodiment.pose.gaze.x > 0.35,
                    "render-pose flicker reset causal gaze at {tick}: {:?}",
                    body.embodiment.pose.gaze
                );
            }
        }
        assert!(
            body.embodiment.pose.gaze.x > 0.35,
            "stable target must still be followed"
        );
    }

    #[test]
    fn only_measured_ecology_outcomes_supply_motor_credit_and_survive_stale_gestures() {
        use pet_ecology::{EcologyOutcome as O, EpisodeGoal as G};
        let mut nervous = NervousSystemRuntime::default();
        nervous.observe_outcomes(&[
            O::EpisodeCompleted(G::SharedAttention),
            O::EpisodeCompleted(G::StoreMorsel),
        ]);
        assert_eq!(nervous.motor_outcome.goal_congruent_motor_success, 0.0);
        nervous.gesture_age = 5.0;
        nervous.observe_outcomes(&[O::EpisodeCompleted(G::OfferOrb)]);
        let mut life = LifeCore::new(lifecore::Genome::from_seed(7), 11);
        let mut vita = VitaRuntime::new(7, None);
        let mut morph = MorphBrain::new(7, None).unwrap();
        nervous.body_feedback.frame_id = 1;
        nervous.prepare_cognition_tick(
            &mut life,
            &mut vita,
            &mut morph,
            0.15,
            NervousReadabilityTuning::default().for_live_runtime(),
            0.05,
        );
        assert!(nervous.episode.reward_positive > 0.0);
        assert_eq!(nervous.motor_outcome, EpisodeContextV1::default());
        assert_eq!(nervous.episode.goal_congruent_motor_success, 0.0);
    }

    #[test]
    fn transient_gesture_and_pain_recover_without_another_gesture() {
        let mut nervous = NervousSystemRuntime::default();
        let mut life = LifeCore::new(lifecore::Genome::from_seed(7), 11);
        let mut vita = VitaRuntime::new(7, None);
        let mut morph = MorphBrain::new(7, None).unwrap();
        let event = EmbodiedGestureEvent {
            classification: lifecore::GestureClassification {
                episode_id: 1,
                kind: EmbodiedGestureKind::SoftTouch,
                committed: true,
                confidence: 1.0,
                prediction_error: 1.0,
                ..Default::default()
            },
            ..Default::default()
        };
        nervous.observe_gesture(&event);
        for tick in 1..=1400 {
            nervous.body_feedback.frame_id = tick;
            nervous.body_feedback.contact.pressure = if tick < 20 { 1.0 } else { 0.0 };
            nervous.body_feedback.shape.maximum_strain = if tick < 20 { 1.0 } else { 0.0 };
            nervous.prepare_cognition_tick(
                &mut life,
                &mut vita,
                &mut morph,
                0.15,
                NervousReadabilityTuning::default().for_live_runtime(),
                0.05,
            );
        }
        assert_eq!(nervous.gesture, GestureFrameV1::default());
        assert!(nervous.episode.reward_negative < 0.001);
        let before = nervous.snapshot;
        nervous.prepare_cognition_tick(
            &mut life,
            &mut vita,
            &mut morph,
            0.15,
            NervousReadabilityTuning::default().for_live_runtime(),
            0.05,
        );
        assert_eq!(
            nervous.snapshot, before,
            "a repeated physical frame must not update interoception"
        );
    }

    #[test]
    fn voice_feedback_uses_callback_output_and_respects_genome_cap() {
        let genome = lifecore::Genome::from_seed(7).voice;
        let mut runtime = NervousSystemRuntime::default();
        runtime.observe_voice(
            AudioVisualFeedback {
                active: true,
                envelope: 0.6,
                breath_pressure: 0.25,
                pitch_normalized: 0.75,
                noisiness: 0.2,
                ..AudioVisualFeedback::default()
            },
            AudioCallbackLevels {
                rms: 0.4,
                peak: 1.0,
                ..AudioCallbackLevels::default()
            },
            &genome,
        );
        assert!(runtime.voice_feedback.phonating);
        assert!(runtime.voice_feedback.actual_pitch_hz > genome.base_pitch_hz);
        assert_eq!(
            runtime.voice_feedback.actual_loudness,
            genome.maximum_loudness
        );
    }

    #[test]
    fn adverse_gesture_becomes_negative_episode_evidence() {
        let mut runtime = NervousSystemRuntime::default();
        runtime.observe_gesture(&EmbodiedGestureEvent {
            classification: lifecore::GestureClassification {
                episode_id: 9,
                kind: EmbodiedGestureKind::SlowStretch,
                confidence: 0.9,
                intensity: 0.8,
                committed: true,
                ..lifecore::GestureClassification::default()
            },
            boundary: GestureBoundaryEvent::Overstrain,
            ..EmbodiedGestureEvent::default()
        });
        assert_eq!(runtime.episode.episode_id, 9);
        assert!(runtime.episode.reward_negative > 0.7);
        assert!(runtime.episode.reward_positive <= 0.01);
    }

    #[test]
    fn profile_sensitivity_scales_real_source_evidence_and_default_is_no_op() {
        let life = LifeCore::new(lifecore::Genome::from_seed(17), 19);
        let vita = VitaRuntime::new(life.state.genome.identity_seed, None);
        let morph = MorphBrain::new(life.state.genome.identity_seed, None).unwrap();
        let mut runtime = NervousSystemRuntime::default();
        runtime.body_feedback.contact.pressure = 0.32;
        runtime.body_feedback.contact.area = 0.40;
        runtime.body_feedback.shape.maximum_strain = 0.24;
        runtime.body_feedback.motion.collision_impulse = 0.30;
        runtime.body_feedback.environment.cursor_loom_rate = 0.20;
        runtime.perception.selected_salience = 0.35;

        let neutral = runtime.source(
            &life,
            &vita,
            &morph,
            0.24,
            NervousReadabilityTuning::default(),
        );
        assert_eq!(neutral.body.contact.pressure, 0.32);
        assert_eq!(neutral.body.contact.area, 0.40);
        assert_eq!(neutral.body.shape.maximum_strain, 0.24);
        assert_eq!(neutral.body.motion.collision_impulse, 0.30);
        assert_eq!(neutral.perception.selected_salience, 0.35);

        let amplified = runtime.source(
            &life,
            &vita,
            &morph,
            0.24,
            NervousReadabilityTuning {
                pain_sensitivity: 2.0,
                contact_sensitivity: 1.5,
                novelty_sensitivity: 2.0,
                startle_sensitivity: 2.0,
                ..NervousReadabilityTuning::default()
            },
        );
        assert_eq!(amplified.body.contact.pressure, 0.64);
        assert_eq!(amplified.body.contact.area, 0.60);
        assert_eq!(amplified.body.shape.maximum_strain, 0.48);
        assert_eq!(amplified.body.motion.collision_impulse, 0.60);
        assert_eq!(amplified.body.environment.cursor_loom_rate, 0.40);
        assert_eq!(amplified.perception.selected_salience, 0.70);
    }
}
