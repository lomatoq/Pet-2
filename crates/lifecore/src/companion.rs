use glam::Vec2;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrimaryIntent {
    #[default]
    IdleContent,
    Rest,
    Sleep,
    Wake,
    Orient,
    Inspect,
    Approach,
    Follow,
    InviteContact,
    AcceptContact,
    Nuzzle,
    GroomSelf,
    InvitePlay,
    Chase,
    Intercept,
    Catch,
    Carry,
    OfferObject,
    SearchObject,
    Explore,
    Hide,
    Peek,
    Mimic,
    Celebrate,
    RecoverFromMiss,
    Avoid,
    StartleFreeze,
    GuardPain,
    EscapePressure,
    RejectContact,
    SettleAfterStress,
    ReturnHome,
    Nest,
    EatInspect,
    EatAccept,
    EatReject,
    SocialCheckIn,
    QuietCompanionship,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SocialMode {
    #[default]
    Autonomous,
    SocialOrienting,
    Affiliative,
    Playful,
    ContactSeeking,
    ContactSatiated,
    Uncertain,
    Protective,
    Resting,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IntentTargetKind {
    #[default]
    None,
    UserProxy,
    Cursor,
    ContactPoint,
    ScreenRegion,
    Surface,
    Orb,
    Den,
    BodyComponent,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct IntentTarget {
    pub kind: IntentTargetKind,
    pub position: Option<Vec2>,
    pub confidence: f32,
    pub component_id: Option<u8>,
}

impl IntentTarget {
    #[must_use]
    pub fn sanitized(mut self) -> Self {
        self.position = self
            .position
            .filter(|position| position.is_finite())
            .map(|position| position.clamp(Vec2::ZERO, Vec2::ONE));
        self.confidence = unit(self.confidence);
        self
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct ExpectedOutcome {
    pub continuation: f32,
    pub user_response: f32,
    pub object_contact: f32,
    pub success: f32,
    pub uncertainty: f32,
}

impl ExpectedOutcome {
    #[must_use]
    pub fn sanitized(mut self) -> Self {
        self.continuation = unit(self.continuation);
        self.user_response = unit(self.user_response);
        self.object_contact = unit(self.object_contact);
        self.success = unit(self.success);
        self.uncertainty = unit(self.uncertainty);
        self
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct CompanionIntentFrame {
    pub episode_id: u64,
    pub primary: PrimaryIntent,
    pub target: IntentTarget,
    pub confidence: f32,
    pub urgency: f32,
    pub valence: f32,
    pub arousal: f32,
    pub social_safety: f32,
    pub trust: f32,
    pub attachment: f32,
    pub play_readiness: f32,
    pub curiosity: f32,
    pub fatigue: f32,
    pub discomfort: f32,
    pub surprise: f32,
    pub frustration: f32,
    pub anticipation: f32,
    pub contact_pleasantness: f32,
    pub agency_match: f32,
    pub expected_outcome: ExpectedOutcome,
    pub social_mode: SocialMode,
    pub persistence: f32,
}

impl CompanionIntentFrame {
    #[must_use]
    pub fn sanitized(mut self) -> Self {
        self.target = self.target.sanitized();
        self.confidence = unit(self.confidence);
        self.urgency = unit(self.urgency);
        self.valence = signed(self.valence);
        self.arousal = unit(self.arousal);
        self.social_safety = unit(self.social_safety);
        self.trust = unit(self.trust);
        self.attachment = unit(self.attachment);
        self.play_readiness = unit(self.play_readiness);
        self.curiosity = unit(self.curiosity);
        self.fatigue = unit(self.fatigue);
        self.discomfort = unit(self.discomfort);
        self.surprise = unit(self.surprise);
        self.frustration = unit(self.frustration);
        self.anticipation = unit(self.anticipation);
        self.contact_pleasantness = signed(self.contact_pleasantness);
        self.agency_match = unit(self.agency_match);
        self.expected_outcome = self.expected_outcome.sanitized();
        self.persistence = unit(self.persistence);
        self
    }

    #[must_use]
    pub fn protective(self) -> bool {
        matches!(
            self.primary,
            PrimaryIntent::Avoid
                | PrimaryIntent::StartleFreeze
                | PrimaryIntent::GuardPain
                | PrimaryIntent::EscapePressure
                | PrimaryIntent::RejectContact
                | PrimaryIntent::SettleAfterStress
        ) || self.discomfort > 0.55
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompanionEventSource {
    #[default]
    Unknown,
    UserPointer,
    DirectContact,
    WindowEnvironment,
    DesktopActivity,
    VisualField,
    ProceduralObject,
    PetBody,
    UiAutomation,
    ApplicationAdapter,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompanionEventKind {
    #[default]
    None,
    PointerApproach,
    PointerWithdraw,
    PointerHover,
    SoftTouch,
    Stroke,
    Hold,
    Pull,
    Release,
    Tickle,
    Flick,
    RhythmicTap,
    PlayInvitation,
    ObjectMoved,
    ObjectThrown,
    UserActive,
    UserIdleShort,
    UserIdleLong,
    UserReturned,
    WindowAppeared,
    WindowDisappeared,
    WindowMoved,
    WindowResized,
    SurfaceNearPass,
    SurfaceCollision,
    Trapped,
    Freed,
    VisualNovelty,
    UiFocusChanged,
    UiAction,
    ProgressStarted,
    ProgressCompleted,
    AppTaskStarted,
    AppTaskCompleted,
    BodyPain,
    BodyRecovery,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct AppraisedEvent {
    pub source: CompanionEventSource,
    pub kind: CompanionEventKind,
    pub target: IntentTarget,
    pub confidence: f32,
    pub novelty: f32,
    pub controllability: f32,
    pub directness: f32,
    pub social_likelihood: f32,
    pub threat_likelihood: f32,
    pub play_likelihood: f32,
    pub contact_quality: f32,
    pub prediction_error: f32,
    pub expectedness: f32,
    pub timestamp: f64,
}

impl AppraisedEvent {
    #[must_use]
    pub fn sanitized(mut self) -> Self {
        self.target = self.target.sanitized();
        self.confidence = unit(self.confidence);
        self.novelty = unit(self.novelty);
        self.controllability = unit(self.controllability);
        self.directness = unit(self.directness);
        self.social_likelihood = unit(self.social_likelihood);
        self.threat_likelihood = unit(self.threat_likelihood);
        self.play_likelihood = unit(self.play_likelihood);
        self.contact_quality = signed(self.contact_quality);
        self.prediction_error = unit(self.prediction_error);
        self.expectedness = unit(self.expectedness);
        self.timestamp = if self.timestamp.is_finite() {
            self.timestamp.max(0.0)
        } else {
            0.0
        };
        self
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct SocialMotivationState {
    pub orienting: f32,
    pub reward_expectancy: f32,
    pub maintaining: f32,
    pub reunion_interest: f32,
    pub bid_persistence: f32,
    pub contact_satiation: f32,
}

impl SocialMotivationState {
    pub fn tick(&mut self, interacting: bool, dt: f32) {
        let dt = finite_dt(dt);
        if interacting {
            self.contact_satiation = (self.contact_satiation + dt * 0.055).clamp(0.0, 1.0);
            self.maintaining = (self.maintaining + dt * 0.035).clamp(0.0, 1.0);
        } else {
            self.contact_satiation = (self.contact_satiation - dt * 0.018).clamp(0.0, 1.0);
            self.maintaining = (self.maintaining - dt * 0.008).clamp(0.0, 1.0);
        }
    }

    #[must_use]
    pub fn sanitized(mut self) -> Self {
        self.orienting = unit(self.orienting);
        self.reward_expectancy = unit(self.reward_expectancy);
        self.maintaining = unit(self.maintaining);
        self.reunion_interest = unit(self.reunion_interest);
        self.bid_persistence = unit(self.bid_persistence);
        self.contact_satiation = unit(self.contact_satiation);
        self
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InteractionRegion {
    #[default]
    Unknown,
    Front,
    Side,
    Back,
    Top,
    Lower,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InteractionSpeedBin {
    #[default]
    Still,
    Slow,
    Medium,
    Fast,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InteractionRhythmBin {
    #[default]
    None,
    Slow,
    Medium,
    Fast,
    Irregular,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PreferenceKey {
    pub kind: CompanionEventKind,
    pub region: InteractionRegion,
    pub speed: InteractionSpeedBin,
    pub rhythm: InteractionRhythmBin,
    pub context_tag: u8,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct PreferenceValue {
    pub expected_pleasantness: f32,
    pub confidence: f32,
    pub exposure_count: u16,
    pub recency: f32,
}

impl PreferenceValue {
    pub fn update(&mut self, outcome: f32, recency: f32) {
        let outcome = signed(outcome);
        let rate = 0.04 * (1.0 - self.confidence * 0.65);
        self.expected_pleasantness = (self.expected_pleasantness
            + (outcome - self.expected_pleasantness) * rate)
            .clamp(-1.0, 1.0);
        self.confidence = (self.confidence + 0.025).clamp(0.0, 1.0);
        self.exposure_count = self.exposure_count.saturating_add(1);
        self.recency = unit(recency);
    }
}

pub const MAX_COMPANION_PREFERENCES: usize = 32;
pub const MAX_RITUAL_STEPS: usize = 8;
pub const MAX_RITUALS: usize = 12;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RitualStep {
    pub kind: CompanionEventKind,
    pub coarse_direction: i8,
    pub coarse_duration: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct RitualPrototype {
    pub id: u32,
    pub steps: [RitualStep; MAX_RITUAL_STEPS],
    pub step_count: u8,
    pub meaning: PrimaryIntent,
    pub confidence: f32,
    pub successful_uses: u16,
    pub failed_uses: u16,
}

impl Default for RitualPrototype {
    fn default() -> Self {
        Self {
            id: 0,
            steps: [RitualStep::default(); MAX_RITUAL_STEPS],
            step_count: 0,
            meaning: PrimaryIntent::IdleContent,
            confidence: 0.0,
            successful_uses: 0,
            failed_uses: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompanionSocialMemory {
    pub schema_version: u32,
    pub trust: f32,
    pub familiarity: f32,
    pub attachment: f32,
    pub social_safety: f32,
    pub motivation: SocialMotivationState,
    pub preferences: Vec<(PreferenceKey, PreferenceValue)>,
    pub rituals: Vec<RitualPrototype>,
}

impl Default for CompanionSocialMemory {
    fn default() -> Self {
        Self {
            schema_version: 1,
            trust: 0.55,
            familiarity: 0.15,
            attachment: 0.12,
            social_safety: 0.70,
            motivation: SocialMotivationState::default(),
            preferences: Vec::new(),
            rituals: Vec::new(),
        }
    }
}

impl CompanionSocialMemory {
    pub fn update_safe_social_outcome(&mut self, quality: f32, dt: f32) {
        let dt = finite_dt(dt);
        let positive = unit(quality.max(0.0));
        self.familiarity = (self.familiarity + dt * 0.004 + positive * 0.0015).clamp(0.0, 1.0);
        self.social_safety = (self.social_safety + positive * 0.004).clamp(0.0, 1.0);
        // Attachment intentionally moves very slowly.
        self.attachment = (self.attachment + positive * 0.0012).clamp(0.0, 1.0);
        self.trust = (self.trust + positive * 0.0018).clamp(0.0, 1.0);
    }

    pub fn update_clear_direct_harm(&mut self, intensity: f32) {
        let intensity = unit(intensity);
        if intensity < 0.30 {
            return;
        }
        self.social_safety = (self.social_safety - intensity * 0.018).clamp(0.0, 1.0);
        self.trust = (self.trust - intensity * 0.010).clamp(0.0, 1.0);
        self.attachment = (self.attachment - intensity * 0.0025).clamp(0.0, 1.0);
    }

    pub fn upsert_preference(&mut self, key: PreferenceKey, outcome: f32, recency: f32) {
        if let Some((_, value)) = self
            .preferences
            .iter_mut()
            .find(|(candidate, _)| *candidate == key)
        {
            value.update(outcome, recency);
            return;
        }
        if self.preferences.len() >= MAX_COMPANION_PREFERENCES
            && let Some((index, _)) = self
                .preferences
                .iter()
                .enumerate()
                .min_by(|left, right| left.1.1.confidence.total_cmp(&right.1.1.confidence))
        {
            self.preferences.remove(index);
        }
        let mut value = PreferenceValue::default();
        value.update(outcome, recency);
        self.preferences.push((key, value));
    }

    #[must_use]
    pub fn sanitized(mut self) -> Self {
        self.schema_version = 1;
        self.trust = unit(self.trust);
        self.familiarity = unit(self.familiarity);
        self.attachment = unit(self.attachment);
        self.social_safety = unit(self.social_safety);
        self.motivation = self.motivation.sanitized();
        self.preferences.truncate(MAX_COMPANION_PREFERENCES);
        self.rituals.truncate(MAX_RITUALS);
        for (_, value) in &mut self.preferences {
            value.expected_pleasantness = signed(value.expected_pleasantness);
            value.confidence = unit(value.confidence);
            value.recency = unit(value.recency);
        }
        for ritual in &mut self.rituals {
            ritual.step_count = ritual.step_count.min(MAX_RITUAL_STEPS as u8);
            ritual.confidence = unit(ritual.confidence);
        }
        self
    }
}

fn finite_dt(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

fn unit(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

fn signed(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(-1.0, 1.0)
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordinary_absence_is_not_encoded_as_attachment_loss() {
        let mut memory = CompanionSocialMemory::default();
        let before = memory.attachment;
        memory.motivation.tick(false, 600.0);
        assert_eq!(memory.attachment, before);
    }

    #[test]
    fn direct_harm_changes_trust_more_than_attachment() {
        let mut memory = CompanionSocialMemory::default();
        let trust_before = memory.trust;
        let attachment_before = memory.attachment;
        memory.update_clear_direct_harm(1.0);
        assert!(trust_before - memory.trust > attachment_before - memory.attachment);
    }

    #[test]
    fn preferences_require_repeated_evidence_to_become_confident() {
        let mut memory = CompanionSocialMemory::default();
        let key = PreferenceKey {
            kind: CompanionEventKind::Stroke,
            region: InteractionRegion::Side,
            speed: InteractionSpeedBin::Slow,
            rhythm: InteractionRhythmBin::None,
            context_tag: 0,
        };
        memory.upsert_preference(key, 1.0, 1.0);
        assert!(memory.preferences[0].1.confidence < 0.1);
        for _ in 0..24 {
            memory.upsert_preference(key, 1.0, 1.0);
        }
        assert!(memory.preferences[0].1.confidence > 0.5);
    }
}
