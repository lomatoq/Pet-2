use glam::Vec2;
use lifecore::{BodyFeedbackV2, EmbodiedGestureKind, PrimaryIntent, SurfaceId};
use serde::{Deserialize, Serialize};

pub const LOCAL_FIELD_BUDGET: usize = 4;
pub const SURFACE_ATTACHMENT_BUDGET: usize = 1;
pub const PROGRAM_COUNT: usize = 64;
pub const P0_PROGRAM_COUNT: usize = 30;

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BehaviorProgramId {
    MoveOrientReflex,
    MovePunctuatedTravel,
    MoveBrakeSquashRecover,
    RestSurfaceRoostSearch,
    RestLandingSoftTouchdown,
    RestSitSettle,
    RestDrowsyYawn,
    RestNremSleep,
    RestRemDreamWake,
    SocialPettingSolicitation,
    SocialRubNuzzleCursor,
    TouchSoftTouchYield,
    TouchSustainedHoldRelaxOrResist,
    TouchPullReleaseRebound,
    DefenseThreatHardenCompact,
    DefenseFragmentTrackAndRemerge,
    StateContentedOpenDrift,
    StateRespiratorySighReset,
    HomeLowEnergyRecharge,
    HomeDenReturnEscort,
    RestLeanRest,
    RestNestAdjust,
    MoveCuriosityArcApproach,
    MoveCautiousApproach,
    MoveExcitedDashOvershoot,
    MoveInspectPauseScan,
    MoveCheckBackSocialReference,
    SocialGreetingApproach,
    SocialMutualGazePulse,
    SocialSlowBlinkAffiliation,
    SocialPresentTouchSide,
    SocialQuietCompanionship,
    SocialAttentionBidWait,
    TouchStrokeFollow,
    TouchLeanIntoStroke,
    TouchTickleWriggle,
    TouchRhythmicTouchSync,
    TouchCircularStirCooperate,
    PlayPlayBowAnalog,
    PlayChaseInviteFeint,
    PlayCursorChaseBout,
    PlayFakeMissRetry,
    PlayOrbCatchEnvelop,
    PlayOrbCarryOffer,
    PlayHidePeekReveal,
    PlaySelfPlayDroplet,
    HomeHungerSearchBid,
    HomeFoodInspectSample,
    HomeFoodAcceptTransport,
    HomeFoodRefusePushAway,
    HomeDigestionSatiation,
    HomeDenNestRest,
    DefenseStartleOrientFreeze,
    DefenseOverpressureBoundary,
    DefenseLocalPainGuard,
    DefenseStrainBraceAndRelease,
    DefenseSafeFragmentDetach,
    DefensePostStressShakeOff,
    StateSadHeavySag,
    StateCuriousProbeBud,
    StateBoredFidgetSelfStim,
    StateSocialPurrCoregulation,
    StateSelfGroomRealign,
    StateEmotionTransitionSettle,
}

impl BehaviorProgramId {
    pub const ALL: [Self; PROGRAM_COUNT] = [
        Self::MoveOrientReflex,
        Self::MovePunctuatedTravel,
        Self::MoveBrakeSquashRecover,
        Self::RestSurfaceRoostSearch,
        Self::RestLandingSoftTouchdown,
        Self::RestSitSettle,
        Self::RestDrowsyYawn,
        Self::RestNremSleep,
        Self::RestRemDreamWake,
        Self::SocialPettingSolicitation,
        Self::SocialRubNuzzleCursor,
        Self::TouchSoftTouchYield,
        Self::TouchSustainedHoldRelaxOrResist,
        Self::TouchPullReleaseRebound,
        Self::DefenseThreatHardenCompact,
        Self::DefenseFragmentTrackAndRemerge,
        Self::StateContentedOpenDrift,
        Self::StateRespiratorySighReset,
        Self::HomeLowEnergyRecharge,
        Self::HomeDenReturnEscort,
        Self::RestLeanRest,
        Self::RestNestAdjust,
        Self::MoveCuriosityArcApproach,
        Self::MoveCautiousApproach,
        Self::MoveExcitedDashOvershoot,
        Self::MoveInspectPauseScan,
        Self::MoveCheckBackSocialReference,
        Self::SocialGreetingApproach,
        Self::SocialMutualGazePulse,
        Self::SocialSlowBlinkAffiliation,
        Self::SocialPresentTouchSide,
        Self::SocialQuietCompanionship,
        Self::SocialAttentionBidWait,
        Self::TouchStrokeFollow,
        Self::TouchLeanIntoStroke,
        Self::TouchTickleWriggle,
        Self::TouchRhythmicTouchSync,
        Self::TouchCircularStirCooperate,
        Self::PlayPlayBowAnalog,
        Self::PlayChaseInviteFeint,
        Self::PlayCursorChaseBout,
        Self::PlayFakeMissRetry,
        Self::PlayOrbCatchEnvelop,
        Self::PlayOrbCarryOffer,
        Self::PlayHidePeekReveal,
        Self::PlaySelfPlayDroplet,
        Self::HomeHungerSearchBid,
        Self::HomeFoodInspectSample,
        Self::HomeFoodAcceptTransport,
        Self::HomeFoodRefusePushAway,
        Self::HomeDigestionSatiation,
        Self::HomeDenNestRest,
        Self::DefenseStartleOrientFreeze,
        Self::DefenseOverpressureBoundary,
        Self::DefenseLocalPainGuard,
        Self::DefenseStrainBraceAndRelease,
        Self::DefenseSafeFragmentDetach,
        Self::DefensePostStressShakeOff,
        Self::StateSadHeavySag,
        Self::StateCuriousProbeBud,
        Self::StateBoredFidgetSelfStim,
        Self::StateSocialPurrCoregulation,
        Self::StateSelfGroomRealign,
        Self::StateEmotionTransitionSettle,
    ];

    pub const ALL_P0: [Self; P0_PROGRAM_COUNT] = [
        Self::MoveOrientReflex,
        Self::MovePunctuatedTravel,
        Self::MoveBrakeSquashRecover,
        Self::RestSurfaceRoostSearch,
        Self::RestLandingSoftTouchdown,
        Self::RestSitSettle,
        Self::RestDrowsyYawn,
        Self::RestNremSleep,
        Self::RestRemDreamWake,
        Self::SocialPettingSolicitation,
        Self::SocialRubNuzzleCursor,
        Self::TouchSoftTouchYield,
        Self::TouchSustainedHoldRelaxOrResist,
        Self::TouchPullReleaseRebound,
        Self::DefenseThreatHardenCompact,
        Self::DefenseFragmentTrackAndRemerge,
        Self::StateContentedOpenDrift,
        Self::StateRespiratorySighReset,
        Self::HomeLowEnergyRecharge,
        Self::HomeDenReturnEscort,
        Self::SocialAttentionBidWait,
        Self::TouchStrokeFollow,
        Self::PlayPlayBowAnalog,
        Self::HomeDenNestRest,
        Self::DefenseStartleOrientFreeze,
        Self::DefenseOverpressureBoundary,
        Self::DefenseStrainBraceAndRelease,
        Self::DefenseSafeFragmentDetach,
        Self::StateSocialPurrCoregulation,
        Self::StateEmotionTransitionSettle,
    ];

    #[must_use]
    pub const fn index(self) -> usize {
        self as usize
    }

    #[must_use]
    pub const fn wire_name(self) -> &'static str {
        match self {
            Self::MoveOrientReflex => "move.orient_reflex",
            Self::MovePunctuatedTravel => "move.punctuated_travel",
            Self::MoveBrakeSquashRecover => "move.brake_squash_recover",
            Self::RestSurfaceRoostSearch => "rest.surface_roost_search",
            Self::RestLandingSoftTouchdown => "rest.landing_soft_touchdown",
            Self::RestSitSettle => "rest.sit_settle",
            Self::RestDrowsyYawn => "rest.drowsy_yawn",
            Self::RestNremSleep => "rest.nrem_sleep",
            Self::RestRemDreamWake => "rest.rem_dream_wake",
            Self::SocialPettingSolicitation => "social.petting_solicitation",
            Self::SocialRubNuzzleCursor => "social.rub_nuzzle_cursor",
            Self::TouchSoftTouchYield => "touch.soft_touch_yield",
            Self::TouchSustainedHoldRelaxOrResist => "touch.sustained_hold_relax_or_resist",
            Self::TouchPullReleaseRebound => "touch.pull_release_rebound",
            Self::DefenseThreatHardenCompact => "defense.threat_harden_compact",
            Self::DefenseFragmentTrackAndRemerge => "defense.fragment_track_and_remerge",
            Self::StateContentedOpenDrift => "state.contented_open_drift",
            Self::StateRespiratorySighReset => "state.respiratory_sigh_reset",
            Self::HomeLowEnergyRecharge => "home.low_energy_recharge",
            Self::HomeDenReturnEscort => "home.den_return_escort",
            Self::RestLeanRest => "rest.lean_rest",
            Self::RestNestAdjust => "rest.nest_adjust",
            Self::MoveCuriosityArcApproach => "move.curiosity_arc_approach",
            Self::MoveCautiousApproach => "move.cautious_approach",
            Self::MoveExcitedDashOvershoot => "move.excited_dash_overshoot",
            Self::MoveInspectPauseScan => "move.inspect_pause_scan",
            Self::MoveCheckBackSocialReference => "move.check_back_social_reference",
            Self::SocialGreetingApproach => "social.greeting_approach",
            Self::SocialMutualGazePulse => "social.mutual_gaze_pulse",
            Self::SocialSlowBlinkAffiliation => "social.slow_blink_affiliation",
            Self::SocialPresentTouchSide => "social.present_touch_side",
            Self::SocialQuietCompanionship => "social.quiet_companionship",
            Self::SocialAttentionBidWait => "social.attention_bid_wait",
            Self::TouchStrokeFollow => "touch.stroke_follow",
            Self::TouchLeanIntoStroke => "touch.lean_into_stroke",
            Self::TouchTickleWriggle => "touch.tickle_wriggle",
            Self::TouchRhythmicTouchSync => "touch.rhythmic_touch_sync",
            Self::TouchCircularStirCooperate => "touch.circular_stir_cooperate",
            Self::PlayPlayBowAnalog => "play.play_bow_analog",
            Self::PlayChaseInviteFeint => "play.chase_invite_feint",
            Self::PlayCursorChaseBout => "play.cursor_chase_bout",
            Self::PlayFakeMissRetry => "play.fake_miss_retry",
            Self::PlayOrbCatchEnvelop => "play.orb_catch_envelop",
            Self::PlayOrbCarryOffer => "play.orb_carry_offer",
            Self::PlayHidePeekReveal => "play.hide_peek_reveal",
            Self::PlaySelfPlayDroplet => "play.self_play_droplet",
            Self::HomeHungerSearchBid => "home.hunger_search_bid",
            Self::HomeFoodInspectSample => "home.food_inspect_sample",
            Self::HomeFoodAcceptTransport => "home.food_accept_transport",
            Self::HomeFoodRefusePushAway => "home.food_refuse_push_away",
            Self::HomeDigestionSatiation => "home.digestion_satiation",
            Self::HomeDenNestRest => "home.den_nest_rest",
            Self::DefenseStartleOrientFreeze => "defense.startle_orient_freeze",
            Self::DefenseOverpressureBoundary => "defense.overpressure_boundary",
            Self::DefenseLocalPainGuard => "defense.local_pain_guard",
            Self::DefenseStrainBraceAndRelease => "defense.strain_brace_and_release",
            Self::DefenseSafeFragmentDetach => "defense.safe_fragment_detach",
            Self::DefensePostStressShakeOff => "defense.post_stress_shake_off",
            Self::StateSadHeavySag => "state.sad_heavy_sag",
            Self::StateCuriousProbeBud => "state.curious_probe_bud",
            Self::StateBoredFidgetSelfStim => "state.bored_fidget_self_stim",
            Self::StateSocialPurrCoregulation => "state.social_purr_coregulation",
            Self::StateSelfGroomRealign => "state.self_groom_realign",
            Self::StateEmotionTransitionSettle => "state.emotion_transition_settle",
        }
    }

    #[must_use]
    pub const fn family(self) -> ProgramFamily {
        use BehaviorProgramId as P;
        match self {
            P::RestSurfaceRoostSearch
            | P::RestLandingSoftTouchdown
            | P::RestSitSettle
            | P::RestDrowsyYawn
            | P::RestNremSleep
            | P::RestRemDreamWake
            | P::RestLeanRest
            | P::RestNestAdjust => ProgramFamily::RestSleep,
            P::MoveOrientReflex
            | P::MovePunctuatedTravel
            | P::MoveBrakeSquashRecover
            | P::MoveCuriosityArcApproach
            | P::MoveCautiousApproach
            | P::MoveExcitedDashOvershoot
            | P::MoveInspectPauseScan
            | P::MoveCheckBackSocialReference => ProgramFamily::LocomotionAttention,
            P::SocialPettingSolicitation
            | P::SocialRubNuzzleCursor
            | P::SocialGreetingApproach
            | P::SocialMutualGazePulse
            | P::SocialSlowBlinkAffiliation
            | P::SocialPresentTouchSide
            | P::SocialQuietCompanionship
            | P::SocialAttentionBidWait => ProgramFamily::AffiliationSocial,
            P::TouchSoftTouchYield
            | P::TouchSustainedHoldRelaxOrResist
            | P::TouchPullReleaseRebound
            | P::TouchStrokeFollow
            | P::TouchLeanIntoStroke
            | P::TouchTickleWriggle
            | P::TouchRhythmicTouchSync
            | P::TouchCircularStirCooperate => ProgramFamily::TouchManipulation,
            P::PlayPlayBowAnalog
            | P::PlayChaseInviteFeint
            | P::PlayCursorChaseBout
            | P::PlayFakeMissRetry
            | P::PlayOrbCatchEnvelop
            | P::PlayOrbCarryOffer
            | P::PlayHidePeekReveal
            | P::PlaySelfPlayDroplet => ProgramFamily::PlayObject,
            P::HomeLowEnergyRecharge
            | P::HomeDenReturnEscort
            | P::HomeHungerSearchBid
            | P::HomeFoodInspectSample
            | P::HomeFoodAcceptTransport
            | P::HomeFoodRefusePushAway
            | P::HomeDigestionSatiation
            | P::HomeDenNestRest => ProgramFamily::MetabolismHome,
            P::DefenseThreatHardenCompact
            | P::DefenseFragmentTrackAndRemerge
            | P::DefenseStartleOrientFreeze
            | P::DefenseOverpressureBoundary
            | P::DefenseLocalPainGuard
            | P::DefenseStrainBraceAndRelease
            | P::DefenseSafeFragmentDetach
            | P::DefensePostStressShakeOff => ProgramFamily::DefenseIntegrity,
            P::StateContentedOpenDrift
            | P::StateRespiratorySighReset
            | P::StateSadHeavySag
            | P::StateCuriousProbeBud
            | P::StateBoredFidgetSelfStim
            | P::StateSocialPurrCoregulation
            | P::StateSelfGroomRealign
            | P::StateEmotionTransitionSettle => ProgramFamily::PhysiologyMaterial,
        }
    }
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProgramFamily {
    RestSleep,
    LocomotionAttention,
    AffiliationSocial,
    TouchManipulation,
    PlayObject,
    MetabolismHome,
    DefenseIntegrity,
    PhysiologyMaterial,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PhaseId {
    pub program: BehaviorProgramId,
    pub index: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MotorPriority {
    Background,
    Voluntary,
    Reactive,
    Integrity,
    Emergency,
}

impl MotorPriority {
    #[must_use]
    pub const fn rank(self) -> u8 {
        match self {
            Self::Background => 0,
            Self::Voluntary => 1,
            Self::Reactive => 2,
            Self::Integrity => 3,
            Self::Emergency => 4,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InterruptPolicy {
    YieldToExplicitGoal,
    FinishReadablePhase,
    BlendWithGoalWhenSafe,
    Immediate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MotorCause {
    BrainAction,
    /// Explicit, bounded developer review through Body Lab. This is never
    /// selected by the organism and never persists as learning evidence.
    LabFixture,
    SalientStimulus,
    UserGesture,
    BodyIntegrity,
    SurfaceContact,
    PhysiologicalTransition,
    WorldEvent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MotorWorldGoal {
    None,
    ReturnHome,
    SleepInDen,
    CarryOrbHome,
    ReturnOrb,
    RetrieveOrb,
    InspectWindow,
    RideWindow,
    SharedAttention,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MotorWorldEvent {
    None,
    DenFieldEntered,
    OrbCaptureStarted,
    OrbCaptureAcceleration,
    OrbStored,
    CaptureFailed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompletionReason {
    None,
    PhaseComplete,
    GoalReached,
    ContactConfirmed,
    SupportConfirmed,
    IntegrityRestored,
    UserResponded,
    GracefulWithdrawal,
    TimedOut,
    Interrupted,
    Invalidated,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct BoutStyle {
    pub amplitude: f32,
    pub tempo: f32,
    pub arc_sign: f32,
    pub asymmetry: f32,
    pub seed: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SurfaceTarget {
    pub surface_id: SurfaceId,
    pub anchor_point: Vec2,
    pub normal: Vec2,
    pub tangent: Vec2,
    /// Distance from the navigation centre to the real liquid silhouette along
    /// the support normal. Glow, shadow, and presentation bubbles are excluded.
    #[serde(default = "default_surface_clearance")]
    pub center_clearance: f32,
    pub score: f32,
}

fn default_surface_clearance() -> f32 {
    0.038
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BehaviorTarget {
    Point(Vec2),
    Cursor(Vec2),
    Surface(SurfaceTarget),
    Den(Vec2),
    Orb(Vec2),
    Component(u8),
}

impl BehaviorTarget {
    #[must_use]
    pub fn world_position(&self) -> Option<Vec2> {
        match self {
            Self::Point(point) | Self::Cursor(point) | Self::Den(point) | Self::Orb(point) => {
                Some(*point)
            }
            Self::Surface(surface) => {
                Some(surface.anchor_point + surface.normal * surface.center_clearance)
            }
            Self::Component(_) => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActivePerformance {
    pub bout_id: u64,
    pub program: BehaviorProgramId,
    pub phase: PhaseId,
    pub phase_time: f32,
    pub total_time: f32,
    pub locked_target: Option<BehaviorTarget>,
    pub sampled_style: BoutStyle,
    pub minimum_readability_reached: bool,
    pub interruption_request: Option<CompletionReason>,
    pub source_action: lifecore::ActionId,
    pub cause: MotorCause,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SurfaceCandidate {
    pub surface_id: SurfaceId,
    pub minimum: Vec2,
    pub maximum: Vec2,
    pub velocity: Vec2,
    pub familiarity: f32,
    pub recent_failed_landings: u8,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BehaviorContextFrame {
    pub frame_id: u64,
    pub timestamp_seconds: f64,
    pub body: BodyFeedbackV2,
    pub somatic: SomaticPerformanceFeedback,
    pub cursor_position: Vec2,
    pub cursor_velocity: Vec2,
    pub cursor_acceleration: Vec2,
    pub pointer_down: bool,
    pub pointer_pressed: bool,
    pub pointer_released: bool,
    pub pet_touched: bool,
    pub pet_dragged: bool,
    pub selected_salience: f32,
    #[serde(default)]
    pub companion_intent: PrimaryIntent,
    #[serde(default)]
    pub companion_confidence: f32,
    pub gesture: EmbodiedGestureKind,
    pub gesture_confidence: f32,
    pub gesture_ended: bool,
    pub boundary_violation: f32,
    pub locomotion_completed: bool,
    /// Bottom edge of the real metaball silhouette below the navigation centre,
    /// expressed in normalized desktop coordinates.
    pub body_bottom_extent: f32,
    /// Signed physical-pixel gap between the real main liquid silhouette and
    /// the desktop bottom edge. Positive means airborne; glow, shadow and
    /// presentation bubbles are deliberately excluded.
    #[serde(default = "default_screen_edge_gap_px")]
    pub screen_edge_gap_px: f32,
    /// Velocity along the bottom-edge normal in physical pixels per second.
    #[serde(default)]
    pub screen_edge_normal_velocity_px_s: f32,
    /// Independent dwell accumulated from the measured pixel gap and velocity.
    /// This must not be derived from the commanded surface constraint.
    #[serde(default)]
    pub screen_edge_support_stable_seconds: f32,
    /// True only after the measured bottom-edge contact has remained within
    /// tolerance for the required dwell.
    #[serde(default)]
    pub screen_edge_supported: bool,
    pub surfaces: Vec<SurfaceCandidate>,
    pub den_anchor: Option<Vec2>,
    pub den_familiarity: f32,
    pub orb_position: Option<Vec2>,
    pub orb_stored: bool,
    pub world_goal: MotorWorldGoal,
    pub world_event: MotorWorldEvent,
}

impl Default for BehaviorContextFrame {
    fn default() -> Self {
        Self {
            frame_id: 0,
            timestamp_seconds: 0.0,
            body: BodyFeedbackV2::default(),
            somatic: SomaticPerformanceFeedback::default(),
            cursor_position: Vec2::splat(0.5),
            cursor_velocity: Vec2::ZERO,
            cursor_acceleration: Vec2::ZERO,
            pointer_down: false,
            pointer_pressed: false,
            pointer_released: false,
            pet_touched: false,
            pet_dragged: false,
            selected_salience: 0.0,
            companion_intent: PrimaryIntent::IdleContent,
            companion_confidence: 0.0,
            gesture: EmbodiedGestureKind::Unknown,
            gesture_confidence: 0.0,
            gesture_ended: false,
            boundary_violation: 0.0,
            locomotion_completed: true,
            body_bottom_extent: 0.038,
            screen_edge_gap_px: default_screen_edge_gap_px(),
            screen_edge_normal_velocity_px_s: 0.0,
            screen_edge_support_stable_seconds: 0.0,
            screen_edge_supported: false,
            surfaces: Vec::new(),
            den_anchor: None,
            den_familiarity: 0.0,
            orb_position: None,
            orb_stored: false,
            world_goal: MotorWorldGoal::None,
            world_event: MotorWorldEvent::None,
        }
    }
}

fn default_screen_edge_gap_px() -> f32 {
    10_000.0
}

impl BehaviorContextFrame {
    #[must_use]
    pub fn has_bottom_screen_edge(&self) -> bool {
        self.surfaces
            .iter()
            .any(|surface| surface.surface_id.0 == "screen:bottom_edge")
    }

    /// Sleep uses independent physical screen-edge evidence. Other surface
    /// performances retain the PBF contact/support feedback path.
    #[must_use]
    pub fn support_confirmed(&self) -> bool {
        if self.has_bottom_screen_edge() {
            self.screen_edge_supported
        } else {
            self.somatic.supported
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldSpace {
    World,
    BodyLocal,
    SurfaceTangentNormal,
    ComponentLocal,
    PointerContact,
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SomaticFieldKind {
    Attract,
    Repel,
    Anchor,
    Flatten,
    Shear,
    Curl,
    Orbit,
    Pulse,
    Wave,
    Brace,
    Gather,
    Bud,
    MassShift,
    Grip,
    GravityBias,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LocalSomaticField {
    pub kind: SomaticFieldKind,
    pub space: FieldSpace,
    pub center: Vec2,
    pub axis: Vec2,
    pub radius: f32,
    pub strength: f32,
    pub falloff: f32,
    pub frequency_hz: f32,
    pub phase_01: f32,
    pub target_component: Option<u8>,
}

impl LocalSomaticField {
    #[must_use]
    pub fn bounded(mut self) -> Self {
        self.center = finite_vec(self.center);
        self.axis = finite_vec(self.axis).normalize_or_zero();
        self.radius = finite(self.radius, 0.20).clamp(0.04, 1.25);
        self.strength = finite(self.strength, 0.0).clamp(-1.0, 1.0);
        self.falloff = finite(self.falloff, 2.0).clamp(0.5, 6.0);
        self.frequency_hz = finite(self.frequency_hz, 0.0).clamp(0.0, 12.0);
        self.phase_01 = finite(self.phase_01, 0.0).rem_euclid(1.0);
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LocomotionEnvelope {
    pub target_position: Option<Vec2>,
    pub target_locked: bool,
    pub speed_multiplier: f32,
    pub acceleration_limit: f32,
    pub braking: f32,
    pub arrival_pause: f32,
    pub lift_fraction: f32,
    pub approach_arc: f32,
    pub gaze_lead: f32,
    pub pose: MotorPoseIntent,
}

impl Default for LocomotionEnvelope {
    fn default() -> Self {
        Self {
            target_position: None,
            target_locked: false,
            speed_multiplier: 1.0,
            acceleration_limit: 1.0,
            braking: 0.0,
            arrival_pause: 0.0,
            lift_fraction: 1.0,
            approach_arc: 0.0,
            gaze_lead: 0.0,
            pose: MotorPoseIntent::Neutral,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MotorPoseIntent {
    #[default]
    Neutral,
    Orient,
    Travel,
    Brake,
    Landing,
    SupportedRest,
    SupportedSleep,
    Touch,
    OfferContact,
    Threat,
    Content,
    Recover,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct BoundedMaterialActuation {
    pub density_compliance_multiplier: f32,
    pub viscosity_multiplier: f32,
    pub surface_tension_multiplier: f32,
    pub flight_damping_multiplier: f32,
    pub flight_stretch_multiplier: f32,
    pub motor_gain_multiplier: f32,
    pub angular_damping_multiplier: f32,
    pub shape_recovery_delta: f32,
    pub rest_spacing_multiplier: f32,
}

impl Default for BoundedMaterialActuation {
    fn default() -> Self {
        Self {
            density_compliance_multiplier: 1.0,
            viscosity_multiplier: 1.0,
            surface_tension_multiplier: 1.0,
            flight_damping_multiplier: 1.0,
            flight_stretch_multiplier: 1.0,
            motor_gain_multiplier: 1.0,
            angular_damping_multiplier: 1.0,
            shape_recovery_delta: 0.0,
            rest_spacing_multiplier: 1.0,
        }
    }
}

impl BoundedMaterialActuation {
    pub fn sanitize(&mut self) {
        self.density_compliance_multiplier =
            finite(self.density_compliance_multiplier, 1.0).clamp(0.60, 1.40);
        self.viscosity_multiplier = finite(self.viscosity_multiplier, 1.0).clamp(0.65, 1.50);
        self.surface_tension_multiplier =
            finite(self.surface_tension_multiplier, 1.0).clamp(0.75, 1.35);
        self.flight_damping_multiplier =
            finite(self.flight_damping_multiplier, 1.0).clamp(0.70, 1.45);
        self.flight_stretch_multiplier =
            finite(self.flight_stretch_multiplier, 1.0).clamp(0.75, 1.40);
        self.motor_gain_multiplier = finite(self.motor_gain_multiplier, 1.0).clamp(0.50, 1.35);
        self.angular_damping_multiplier =
            finite(self.angular_damping_multiplier, 1.0).clamp(0.70, 1.45);
        self.shape_recovery_delta = finite(self.shape_recovery_delta, 0.0).clamp(-0.20, 0.45);
        self.rest_spacing_multiplier = finite(self.rest_spacing_multiplier, 1.0).clamp(0.90, 1.12);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct InternalPhysiologyActuation {
    pub flow_strength_multiplier: f32,
    pub flow_speed_multiplier: f32,
    pub pulse_amplitude: f32,
    pub breath_amplitude_multiplier: f32,
    pub breath_speed_multiplier: f32,
    pub flow_damping: f32,
}

impl Default for InternalPhysiologyActuation {
    fn default() -> Self {
        Self {
            flow_strength_multiplier: 1.0,
            flow_speed_multiplier: 1.0,
            pulse_amplitude: 0.0,
            breath_amplitude_multiplier: 1.0,
            breath_speed_multiplier: 1.0,
            flow_damping: 0.0,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct ExpressionIntent {
    pub gaze_target: Option<Vec2>,
    pub eye_aperture_delta: f32,
    pub squint_delta: f32,
    pub mouth_open: f32,
    pub blink: f32,
    pub relief: f32,
    pub effort: f32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VoiceSemanticIntent {
    #[default]
    None,
    Notice,
    Contact,
    Ack,
    Invite,
    Query,
    Effort,
    Boundary,
    Alarm,
    Relief,
    PhysiologicalBreath,
    Yawn,
    Purr,
    DreamMurmur,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct VoiceMotorIntent {
    pub semantic: VoiceSemanticIntent,
    pub emit_once: bool,
    pub intensity: f32,
}

impl Default for VoiceMotorIntent {
    fn default() -> Self {
        Self {
            semantic: VoiceSemanticIntent::None,
            emit_once: false,
            intensity: 0.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SurfaceAttachmentCommand {
    pub surface_id: SurfaceId,
    pub anchor_point: Vec2,
    pub normal: Vec2,
    pub tangent: Vec2,
    pub target_contact_fraction: f32,
    pub normal_compliance: f32,
    pub tangent_friction: f32,
    pub adhesion: f32,
    pub load_fraction: f32,
    pub break_force: f32,
    pub release_half_life: f32,
}

impl SurfaceAttachmentCommand {
    pub fn sanitize(&mut self) {
        self.anchor_point = finite_vec(self.anchor_point).clamp(Vec2::ZERO, Vec2::ONE);
        self.normal = finite_vec(self.normal).normalize_or_zero();
        self.tangent = finite_vec(self.tangent).normalize_or_zero();
        if self.tangent.length_squared() <= 1.0e-6 {
            self.tangent = Vec2::new(-self.normal.y, self.normal.x);
        }
        self.target_contact_fraction = finite(self.target_contact_fraction, 0.32).clamp(0.15, 0.50);
        self.normal_compliance = finite(self.normal_compliance, 0.25).clamp(0.02, 1.0);
        self.tangent_friction = finite(self.tangent_friction, 0.60).clamp(0.0, 1.0);
        self.adhesion = finite(self.adhesion, 0.20).clamp(0.0, 0.70);
        self.load_fraction = finite(self.load_fraction, 0.30).clamp(0.0, 0.65);
        self.break_force = finite(self.break_force, 0.75).clamp(0.10, 1.0);
        self.release_half_life = finite(self.release_half_life, 0.30).clamp(0.05, 1.5);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct RegimeBlend {
    pub primary: SomaticRegime,
    pub secondary: SomaticRegime,
    pub primary_weight: f32,
}

impl Default for RegimeBlend {
    fn default() -> Self {
        Self {
            primary: SomaticRegime::CalmContent,
            secondary: SomaticRegime::CalmContent,
            primary_weight: 1.0,
        }
    }
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SomaticRegime {
    CalmContent,
    Playful,
    Affiliative,
    Curious,
    Fatigued,
    SadLowValence,
    Threatened,
    Frustrated,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SomaticActuationPacket {
    pub frame_id: u64,
    pub source_bout_id: u64,
    pub program: Option<BehaviorProgramId>,
    pub phase: Option<PhaseId>,
    pub phase_name: String,
    pub phase_progress: f32,
    pub cause: MotorCause,
    pub regime: RegimeBlend,
    pub locomotion: LocomotionEnvelope,
    pub material: BoundedMaterialActuation,
    pub internal: InternalPhysiologyActuation,
    pub fields: [Option<LocalSomaticField>; LOCAL_FIELD_BUDGET],
    pub support: Option<SurfaceAttachmentCommand>,
    pub expression: ExpressionIntent,
    pub voice: VoiceMotorIntent,
}

impl Default for SomaticActuationPacket {
    fn default() -> Self {
        Self {
            frame_id: 0,
            source_bout_id: 0,
            program: None,
            phase: None,
            phase_name: String::new(),
            phase_progress: 0.0,
            cause: MotorCause::BrainAction,
            regime: RegimeBlend::default(),
            locomotion: LocomotionEnvelope::default(),
            material: BoundedMaterialActuation::default(),
            internal: InternalPhysiologyActuation::default(),
            fields: [None; LOCAL_FIELD_BUDGET],
            support: None,
            expression: ExpressionIntent::default(),
            voice: VoiceMotorIntent::default(),
        }
    }
}

impl SomaticActuationPacket {
    pub fn sanitize(&mut self) {
        self.locomotion.speed_multiplier =
            finite(self.locomotion.speed_multiplier, 1.0).clamp(0.0, 1.50);
        self.phase_progress = unit(self.phase_progress);
        self.locomotion.acceleration_limit =
            finite(self.locomotion.acceleration_limit, 1.0).clamp(0.05, 1.50);
        self.locomotion.braking = unit(self.locomotion.braking);
        self.locomotion.arrival_pause = unit(self.locomotion.arrival_pause);
        self.locomotion.lift_fraction = unit(self.locomotion.lift_fraction);
        self.locomotion.approach_arc = finite(self.locomotion.approach_arc, 0.0).clamp(-0.35, 0.35);
        self.locomotion.gaze_lead = unit(self.locomotion.gaze_lead);
        self.material.sanitize();
        self.internal.flow_strength_multiplier =
            finite(self.internal.flow_strength_multiplier, 1.0).clamp(0.20, 1.60);
        self.internal.flow_speed_multiplier =
            finite(self.internal.flow_speed_multiplier, 1.0).clamp(0.20, 1.60);
        self.internal.pulse_amplitude = unit(self.internal.pulse_amplitude);
        self.internal.breath_amplitude_multiplier =
            finite(self.internal.breath_amplitude_multiplier, 1.0).clamp(0.35, 1.80);
        self.internal.breath_speed_multiplier =
            finite(self.internal.breath_speed_multiplier, 1.0).clamp(0.30, 1.80);
        self.internal.flow_damping = unit(self.internal.flow_damping);
        for field in self.fields.iter_mut().flatten() {
            *field = field.bounded();
        }
        if let Some(support) = &mut self.support {
            support.sanitize();
        }
        self.expression.eye_aperture_delta =
            finite(self.expression.eye_aperture_delta, 0.0).clamp(-0.75, 0.50);
        self.expression.squint_delta = finite(self.expression.squint_delta, 0.0).clamp(-0.50, 0.75);
        self.expression.mouth_open = unit(self.expression.mouth_open);
        self.expression.blink = unit(self.expression.blink);
        self.expression.relief = unit(self.expression.relief);
        self.expression.effort = unit(self.expression.effort);
        self.voice.intensity = unit(self.voice.intensity);
    }

    #[must_use]
    pub fn field_count(&self) -> usize {
        self.fields.iter().flatten().count()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SomaticPerformanceFeedback {
    pub frame_id: u64,
    pub program_id: Option<BehaviorProgramId>,
    pub phase: Option<PhaseId>,
    pub phase_progress: f32,
    pub target_locked: bool,
    pub contact_fraction: f32,
    pub support_stability: f32,
    pub supported: bool,
    pub supported_seconds: f32,
    pub local_deformation_energy: f32,
    pub locality_fraction: f32,
    pub maximum_strain: f32,
    pub mass_conservation_error: f32,
    pub completion_reason: CompletionReason,
    pub motor_error: f32,
    pub user_response_credit: f32,
}

impl Default for SomaticPerformanceFeedback {
    fn default() -> Self {
        Self {
            frame_id: 0,
            program_id: None,
            phase: None,
            phase_progress: 0.0,
            target_locked: false,
            contact_fraction: 0.0,
            support_stability: 0.0,
            supported: false,
            supported_seconds: 0.0,
            local_deformation_energy: 0.0,
            locality_fraction: 1.0,
            maximum_strain: 0.0,
            mass_conservation_error: 0.0,
            completion_reason: CompletionReason::None,
            motor_error: 0.0,
            user_response_credit: 0.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MotorTraceRecord {
    pub frame_id: u64,
    pub bout_id: u64,
    pub source_event: MotorCause,
    pub source_action: lifecore::ActionId,
    pub state_regime: RegimeBlend,
    pub program: BehaviorProgramId,
    pub phase: PhaseId,
    pub phase_name: String,
    pub target_locked: bool,
    pub field_kinds: [Option<SomaticFieldKind>; LOCAL_FIELD_BUDGET],
    pub completion_reason: CompletionReason,
    pub motor_error: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct MotorReadabilityTuning {
    pub phase_contrast: f32,
    pub local_field_gain: f32,
    pub support_gain: f32,
    pub preparation_gain: f32,
    pub follow_through_gain: f32,
    pub state_hysteresis_s: f32,
    pub baseline_adaptation_s: f32,
}

impl Default for MotorReadabilityTuning {
    fn default() -> Self {
        Self {
            phase_contrast: 1.0,
            local_field_gain: 1.0,
            support_gain: 1.0,
            preparation_gain: 1.0,
            follow_through_gain: 1.0,
            state_hysteresis_s: 0.45,
            baseline_adaptation_s: 9.0,
        }
    }
}

impl MotorReadabilityTuning {
    #[must_use]
    pub fn bounded(mut self) -> Self {
        self.phase_contrast = finite(self.phase_contrast, 1.0).clamp(0.55, 1.60);
        self.local_field_gain = finite(self.local_field_gain, 1.0).clamp(0.50, 1.50);
        self.support_gain = finite(self.support_gain, 1.0).clamp(0.50, 1.50);
        self.preparation_gain = finite(self.preparation_gain, 1.0).clamp(0.50, 1.60);
        self.follow_through_gain = finite(self.follow_through_gain, 1.0).clamp(0.50, 1.60);
        self.state_hysteresis_s = finite(self.state_hysteresis_s, 0.45).clamp(0.25, 1.0);
        self.baseline_adaptation_s = finite(self.baseline_adaptation_s, 9.0).clamp(2.0, 60.0);
        self
    }
}

pub(crate) fn unit(value: f32) -> f32 {
    finite(value, 0.0).clamp(0.0, 1.0)
}

pub(crate) fn finite(value: f32, fallback: f32) -> f32 {
    if value.is_finite() { value } else { fallback }
}

pub(crate) fn finite_vec(value: Vec2) -> Vec2 {
    if value.is_finite() { value } else { Vec2::ZERO }
}
