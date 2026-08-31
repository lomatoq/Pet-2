use std::array;

use glam::Vec2;
use rand::{RngCore, SeedableRng};
use rand_chacha::ChaCha8Rng;
use serde::{Deserialize, Serialize};

use crate::{AffectState, DriveVector, VoiceGenome};

pub const ACTION_COUNT: usize = 24;
pub const EXPRESSION_READOUT_COUNT: usize = 12;

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionId {
    IdleHover,
    ObserveCursor,
    ObserveUserActivity,
    ApproachCursor,
    RetreatFromCursor,
    LandOnWindow,
    ClingToWindowSide,
    Sleep,
    WakeUp,
    ExploreScreen,
    PeekFromEdge,
    InvitePetting,
    InviteCursorChase,
    PlayCursorChase,
    BringProceduralOrb,
    HideAndSeek,
    MimicClickRhythm,
    SilentStare,
    HappyDisplay,
    FrustratedRetreat,
    Chirp,
    Purr,
    SelfPlay,
    Metamorphosis,
}

impl ActionId {
    pub const ALL: [Self; ACTION_COUNT] = [
        Self::IdleHover,
        Self::ObserveCursor,
        Self::ObserveUserActivity,
        Self::ApproachCursor,
        Self::RetreatFromCursor,
        Self::LandOnWindow,
        Self::ClingToWindowSide,
        Self::Sleep,
        Self::WakeUp,
        Self::ExploreScreen,
        Self::PeekFromEdge,
        Self::InvitePetting,
        Self::InviteCursorChase,
        Self::PlayCursorChase,
        Self::BringProceduralOrb,
        Self::HideAndSeek,
        Self::MimicClickRhythm,
        Self::SilentStare,
        Self::HappyDisplay,
        Self::FrustratedRetreat,
        Self::Chirp,
        Self::Purr,
        Self::SelfPlay,
        Self::Metamorphosis,
    ];

    #[must_use]
    pub const fn index(self) -> usize {
        self as usize
    }

    #[must_use]
    pub fn definition(self) -> ActionDefinition {
        use ActionId as A;
        let quiet = ActionConditions::quiet();
        match self {
            A::IdleHover => definition(
                self,
                1.0,
                5.0,
                0.0,
                0.95,
                quiet,
                relief(0.0, 0.0, 0.0, 0.02, 0.04, 0.02, 0.01, 0.0),
                0.0,
            ),
            A::ObserveCursor => definition(
                self,
                1.0,
                5.5,
                1.0,
                0.90,
                ActionConditions::cursor(true),
                relief(0.0, 0.03, 0.0, 0.12, 0.0, 0.0, 0.0, 0.04),
                0.01,
            ),
            A::ObserveUserActivity => definition(
                self,
                1.5,
                7.0,
                2.0,
                0.85,
                quiet,
                relief(0.0, 0.04, 0.0, 0.08, 0.01, 0.0, 0.0, 0.02),
                0.01,
            ),
            A::ApproachCursor => definition(
                self,
                1.0,
                5.0,
                2.0,
                0.70,
                ActionConditions::cursor_near(false),
                relief(0.0, 0.12, 0.04, 0.08, 0.0, 0.0, 0.0, 0.03),
                0.05,
            ),
            A::RetreatFromCursor => definition(
                self,
                0.8,
                3.5,
                0.5,
                0.95,
                ActionConditions::threat(),
                relief(0.0, 0.0, 0.0, 0.01, 0.03, 0.30, 0.05, 0.0),
                0.0,
            ),
            A::LandOnWindow => definition(
                self,
                1.2,
                6.0,
                5.0,
                0.60,
                ActionConditions::surface(),
                relief(0.02, 0.0, 0.02, 0.09, 0.09, 0.01, 0.05, 0.10),
                0.03,
            ),
            A::ClingToWindowSide => definition(
                self,
                1.3,
                6.5,
                5.0,
                0.60,
                ActionConditions::surface(),
                relief(0.0, 0.0, 0.03, 0.10, 0.03, 0.01, 0.06, 0.10),
                0.04,
            ),
            A::Sleep => definition(
                self,
                5.0,
                90.0,
                0.0,
                0.20,
                ActionConditions::sleep(),
                relief(0.34, 0.0, 0.0, 0.0, 0.18, 0.04, 0.03, 0.0),
                0.0,
            ),
            A::WakeUp => definition(
                self,
                0.7,
                2.0,
                1.0,
                0.95,
                quiet,
                relief(0.0, 0.0, 0.0, 0.02, 0.0, 0.0, 0.0, 0.02),
                0.0,
            ),
            A::ExploreScreen => definition(
                self,
                2.0,
                9.0,
                3.0,
                0.55,
                quiet,
                relief(0.0, 0.0, 0.04, 0.16, 0.0, 0.0, 0.10, 0.22),
                0.02,
            ),
            A::PeekFromEdge => definition(
                self,
                1.8,
                7.0,
                8.0,
                0.55,
                ActionConditions::attention(false),
                relief(0.0, 0.10, 0.02, 0.11, 0.0, 0.0, 0.03, 0.12),
                0.06,
            ),
            A::InvitePetting => definition(
                self,
                1.8,
                8.0,
                12.0,
                0.45,
                ActionConditions::attention(false),
                relief(0.0, 0.24, 0.0, 0.03, 0.08, 0.0, 0.0, 0.0),
                0.12,
            ),
            A::InviteCursorChase => definition(
                self,
                1.4,
                6.0,
                12.0,
                0.55,
                ActionConditions::attention(false),
                relief(0.0, 0.10, 0.20, 0.08, 0.0, 0.0, 0.0, 0.03),
                0.14,
            ),
            A::PlayCursorChase => definition(
                self,
                2.0,
                12.0,
                4.0,
                0.70,
                ActionConditions::cursor_chase(),
                relief(0.0, 0.08, 0.30, 0.12, 0.0, 0.0, 0.05, 0.10),
                0.0,
            ),
            A::BringProceduralOrb => definition(
                self,
                2.5,
                9.0,
                20.0,
                0.40,
                ActionConditions::attention(false),
                relief(0.0, 0.13, 0.25, 0.08, 0.0, 0.0, 0.04, 0.12),
                0.25,
            ),
            A::HideAndSeek => definition(
                self,
                2.0,
                10.0,
                18.0,
                0.50,
                ActionConditions::attention(false),
                relief(0.0, 0.08, 0.22, 0.14, 0.0, 0.02, 0.10, 0.15),
                0.16,
            ),
            A::MimicClickRhythm => definition(
                self,
                1.0,
                5.0,
                25.0,
                0.50,
                ActionConditions::attention(false),
                relief(0.0, 0.12, 0.08, 0.10, 0.0, 0.0, 0.0, 0.12),
                0.30,
            ),
            A::SilentStare => definition(
                self,
                1.6,
                8.0,
                5.0,
                0.65,
                ActionConditions::attention(true),
                relief(0.0, 0.09, 0.0, 0.06, 0.0, 0.0, 0.0, 0.03),
                0.015,
            ),
            A::HappyDisplay => definition(
                self,
                1.0,
                4.0,
                4.0,
                0.70,
                quiet,
                relief(0.0, 0.05, 0.04, 0.0, 0.08, 0.0, 0.0, 0.0),
                0.02,
            ),
            A::FrustratedRetreat => definition(
                self,
                1.2,
                5.0,
                6.0,
                0.75,
                quiet,
                relief(0.0, 0.0, 0.0, 0.0, 0.06, 0.10, 0.10, 0.02),
                0.0,
            ),
            A::Chirp => definition(
                self,
                0.4,
                2.2,
                10.0,
                0.80,
                ActionConditions::attention(false),
                relief(0.0, 0.10, 0.02, 0.04, 0.0, 0.0, 0.0, 0.04),
                0.34,
            ),
            A::Purr => definition(
                self,
                1.5,
                8.0,
                8.0,
                0.65,
                ActionConditions::attention(false),
                relief(0.0, 0.14, 0.0, 0.0, 0.15, 0.0, 0.0, 0.0),
                0.20,
            ),
            A::SelfPlay => definition(
                self,
                2.0,
                10.0,
                3.0,
                0.55,
                quiet,
                relief(0.0, 0.0, 0.22, 0.06, 0.0, 0.0, 0.22, 0.10),
                0.0,
            ),
            A::Metamorphosis => definition(
                self,
                6.0,
                14.0,
                3600.0,
                0.0,
                ActionConditions::metamorphosis(),
                relief(0.0, 0.0, 0.0, 0.18, 0.08, 0.12, 0.12, 0.32),
                0.0,
            ),
        }
    }

    #[must_use]
    pub const fn is_attention_strategy(self) -> bool {
        matches!(
            self,
            Self::SilentStare
                | Self::ApproachCursor
                | Self::LandOnWindow
                | Self::PeekFromEdge
                | Self::BringProceduralOrb
                | Self::Chirp
                | Self::MimicClickRhythm
                | Self::HideAndSeek
                | Self::FrustratedRetreat
        )
    }

    #[must_use]
    pub const fn is_focus_allowed(self) -> bool {
        matches!(
            self,
            Self::Sleep
                | Self::WakeUp
                | Self::IdleHover
                | Self::ObserveCursor
                | Self::ObserveUserActivity
                | Self::SilentStare
                | Self::SelfPlay
                | Self::RetreatFromCursor
                | Self::FrustratedRetreat
        )
    }

    #[must_use]
    pub const fn is_vocal(self) -> bool {
        matches!(self, Self::Chirp | Self::Purr | Self::MimicClickRhythm)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ActionDefinition {
    pub id: ActionId,
    pub minimum_duration: f32,
    pub maximum_duration: f32,
    pub cooldown: f32,
    pub interruptibility: f32,
    pub required_conditions: ActionConditions,
    pub drive_relief: DriveVector,
    pub interruption_cost: f32,
    pub unsolicited_attention_cost: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ActionConditions {
    pub needs_cursor: bool,
    #[serde(default)]
    pub needs_cursor_proximity: bool,
    #[serde(default)]
    pub needs_cursor_engagement: bool,
    pub needs_user_available: bool,
    pub needs_surface: bool,
    pub threat_only: bool,
    pub sleep_drive_minimum: f32,
    pub generation_minimum: u32,
    pub allowed_in_focus_mode: bool,
}

impl ActionConditions {
    const fn quiet() -> Self {
        Self {
            needs_cursor: false,
            needs_cursor_proximity: false,
            needs_cursor_engagement: false,
            needs_user_available: false,
            needs_surface: false,
            threat_only: false,
            sleep_drive_minimum: 0.0,
            generation_minimum: 0,
            allowed_in_focus_mode: true,
        }
    }

    const fn cursor(focus_allowed: bool) -> Self {
        Self {
            needs_cursor: true,
            allowed_in_focus_mode: focus_allowed,
            ..Self::quiet()
        }
    }

    const fn cursor_chase() -> Self {
        Self {
            needs_cursor_engagement: true,
            ..Self::cursor(false)
        }
    }

    const fn cursor_near(focus_allowed: bool) -> Self {
        Self {
            needs_cursor_proximity: true,
            ..Self::cursor(focus_allowed)
        }
    }

    const fn surface() -> Self {
        Self {
            needs_surface: true,
            ..Self::quiet()
        }
    }

    const fn threat() -> Self {
        Self {
            threat_only: true,
            ..Self::quiet()
        }
    }

    const fn attention(focus_allowed: bool) -> Self {
        Self {
            needs_user_available: true,
            allowed_in_focus_mode: focus_allowed,
            ..Self::quiet()
        }
    }

    const fn sleep() -> Self {
        Self {
            sleep_drive_minimum: 0.46,
            ..Self::quiet()
        }
    }

    const fn metamorphosis() -> Self {
        Self {
            generation_minimum: 0,
            allowed_in_focus_mode: false,
            ..Self::quiet()
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AppCategory {
    Creative,
    Communication,
    Entertainment,
    FocusedWork,
    System,
    Unknown,
}

impl AppCategory {
    #[must_use]
    pub const fn index(self) -> usize {
        match self {
            Self::Creative => 0,
            Self::Communication => 1,
            Self::Entertainment => 2,
            Self::FocusedWork => 3,
            Self::System => 4,
            Self::Unknown => 5,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DayPhase {
    Night,
    Morning,
    Day,
    Evening,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    pub minimum: Vec2,
    pub maximum: Vec2,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SurfaceId(pub String);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SurfaceRect {
    pub id: SurfaceId,
    pub rect: Rect,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SensorFrame {
    pub timestamp: f64,
    pub screen_size: Vec2,
    pub cursor_position: Vec2,
    pub cursor_velocity: Vec2,
    pub cursor_acceleration: Vec2,
    pub cursor_distance_to_pet: f32,
    pub cursor_approach_speed: f32,
    pub pointer_down: bool,
    pub pointer_pressed: bool,
    pub pointer_released: bool,
    pub pet_hovered: bool,
    pub pet_touched: bool,
    pub pet_dragged: bool,
    pub user_idle_seconds: f32,
    pub user_activity_rate: f32,
    pub recent_click_rhythm: [f32; 8],
    pub active_app_category: AppCategory,
    pub active_window_rect: Option<Rect>,
    pub visible_surfaces: Vec<SurfaceRect>,
    pub time_of_day_01: f32,
    pub day_phase: DayPhase,
    pub audio_rms: Option<f32>,
    pub voice_activity: Option<f32>,
    pub mean_luminance: Option<f32>,
    pub local_luminance: Option<f32>,
    pub user_presence: Option<f32>,
    pub user_availability: Option<f32>,
}

impl Default for SensorFrame {
    fn default() -> Self {
        Self {
            timestamp: 0.0,
            screen_size: Vec2::ONE,
            cursor_position: Vec2::splat(0.5),
            cursor_velocity: Vec2::ZERO,
            cursor_acceleration: Vec2::ZERO,
            cursor_distance_to_pet: 1.0,
            cursor_approach_speed: 0.0,
            pointer_down: false,
            pointer_pressed: false,
            pointer_released: false,
            pet_hovered: false,
            pet_touched: false,
            pet_dragged: false,
            user_idle_seconds: 0.0,
            user_activity_rate: 0.0,
            recent_click_rhythm: [0.0; 8],
            active_app_category: AppCategory::Unknown,
            active_window_rect: None,
            visible_surfaces: Vec::new(),
            time_of_day_01: 0.5,
            day_phase: DayPhase::Day,
            audio_rms: None,
            voice_activity: None,
            mean_luminance: None,
            local_luminance: None,
            user_presence: None,
            user_availability: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CollisionEvent {
    pub normal: Vec2,
    pub intensity: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BodyFeedback {
    pub world_position: Vec2,
    pub velocity: Vec2,
    pub acceleration: Vec2,
    pub grounded: bool,
    pub clinging: bool,
    pub current_surface: Option<SurfaceId>,
    pub cursor_contact: bool,
    pub collision: Option<CollisionEvent>,
    pub pose_error: f32,
    pub locomotion_completed: bool,
}

impl Default for BodyFeedback {
    fn default() -> Self {
        Self {
            world_position: Vec2::splat(0.5),
            velocity: Vec2::ZERO,
            acceleration: Vec2::ZERO,
            grounded: false,
            clinging: false,
            current_surface: None,
            cursor_contact: false,
            collision: None,
            pose_error: 0.0,
            locomotion_completed: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LocomotionMode {
    Hover,
    Seek,
    Arrive,
    Flee,
    Orbit,
    Wander,
    SurfaceApproach,
    Landing,
    EdgeCling,
    Sleep,
    Cocoon,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PoseIntent {
    Neutral,
    Curious,
    Playful,
    Compact,
    Landing,
    Clinging,
    Sleeping,
    Display,
    Cocoon,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum InteractionTarget {
    Cursor,
    Surface(SurfaceId),
    ProceduralOrb,
    User,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ExpressionState {
    pub blink_left: f32,
    pub blink_right: f32,
    pub squint: f32,
    pub pupil_size: f32,
    pub pupil_focus: f32,
    pub brow_raise: f32,
    pub brow_tension: f32,
    pub mouth_open: f32,
    pub mouth_curve: f32,
    pub mouth_tension: f32,
    pub cheek_glow: f32,
    pub body_glow: f32,
}

impl Default for ExpressionState {
    fn default() -> Self {
        Self {
            blink_left: 0.0,
            blink_right: 0.0,
            squint: 0.0,
            pupil_size: 0.5,
            pupil_focus: 0.5,
            brow_raise: 0.0,
            brow_tension: 0.0,
            mouth_open: 0.0,
            mouth_curve: 0.0,
            mouth_tension: 0.0,
            cheek_glow: 0.0,
            body_glow: 0.2,
        }
    }
}

impl ExpressionState {
    #[must_use]
    pub fn from_readouts(readouts: [f32; EXPRESSION_READOUT_COUNT], affect: AffectState) -> Self {
        Self {
            blink_left: positive_readout(readouts[0], 0.72),
            blink_right: positive_readout(readouts[1], 0.72),
            squint: positive_readout(readouts[2], 0.30),
            // Pupils are an arousal/intensity channel, not a valence meter.
            pupil_size: unit(0.46 + affect.arousal * 0.42),
            pupil_focus: unit(readouts[4] * 0.5 + 0.5),
            brow_raise: signed(readouts[5] + affect.arousal * 0.2),
            brow_tension: positive_readout(readouts[6], 0.20),
            mouth_open: unit(positive_readout(readouts[7], 0.30) + affect.arousal * 0.08),
            mouth_curve: signed(readouts[8] + affect.valence * 0.55),
            mouth_tension: positive_readout(readouts[9], 0.20),
            cheek_glow: unit(readouts[10] * 0.35 + affect.attachment * 0.65),
            body_glow: unit(0.15 + readouts[11] * 0.25 + affect.arousal * 0.35),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BodyIntent {
    pub locomotion: LocomotionMode,
    pub target_position: Vec2,
    pub target_surface: Option<SurfaceId>,
    pub desired_speed: f32,
    pub facing_direction: f32,
    pub gaze_target: Option<Vec2>,
    pub pose: PoseIntent,
    pub expression: ExpressionState,
    pub interaction_target: Option<InteractionTarget>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VocalRequest {
    pub motif_id: u64,
    /// Ephemeral rendition identity. It varies timbre/noise/room details while
    /// reward credit remains attached only to the stable learned motif.
    #[serde(default)]
    pub performance_seed: u64,
    pub gain: f32,
    pub pan: f32,
    pub pitch_scale: f32,
    pub tempo_scale: f32,
    pub stress: f32,
    pub purr: bool,
    /// Optional normalized inter-onset intervals for a grounded rhythm echo.
    /// Zero entries mean no authored interval; waveform ownership stays in
    /// `pet_audio` and no raw input timing history is retained.
    #[serde(default)]
    pub rhythm_intervals: [f32; 8],
}

/// The semantic reason the mind wants to vocalize.
///
/// The desktop host only reports the interaction. `LifeCore` remains the sole
/// owner of repertoire choice, anti-repetition, and outcome credit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VocalTrigger {
    /// A vocal action selected by the normal action arbitrator.
    Action(ActionId),
    /// A short, responsive sound after direct material contact.
    Touch,
    ToyOffer,
    CatchSuccess,
    MissAndRetry,
    NeedHelp,
    FoodInspect,
    FoodAccepted,
    FoodRefused,
    HomeReturn,
    SkillMastered,
    RhythmEcho,
    VisualNotice,
}

impl VocalTrigger {
    #[must_use]
    pub const fn is_supported(self) -> bool {
        match self {
            Self::Action(action) => action.is_vocal(),
            Self::Touch
            | Self::ToyOffer
            | Self::CatchSuccess
            | Self::MissAndRetry
            | Self::NeedHelp
            | Self::FoodInspect
            | Self::FoodAccepted
            | Self::FoodRefused
            | Self::HomeReturn
            | Self::SkillMastered
            | Self::RhythmEcho
            | Self::VisualNotice => true,
        }
    }

    #[must_use]
    pub const fn expects_response(self) -> bool {
        matches!(
            self,
            Self::Action(ActionId::Chirp | ActionId::MimicClickRhythm) | Self::ToyOffer
        )
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VocalMotif {
    pub id: u64,
    pub parent_id: Option<u64>,
    pub generation: u32,
    pub seed: u64,
    pub syllables: Vec<Syllable>,
    pub context_weights: [f32; 16],
    pub expected_reward: f32,
    pub use_count: u32,
    pub success_count: u32,
    pub novelty: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Syllable {
    pub duration_ms: f32,
    pub gap_after_ms: f32,
    pub pitch_start: f32,
    pub pitch_peak: f32,
    pub pitch_end: f32,
    pub amplitude: f32,
    pub noisiness: f32,
    pub click: f32,
    pub mouth_open: f32,
    pub trill_amount: f32,
    pub vibrato_amount: f32,
}

#[must_use]
pub fn generate_initial_motifs(voice: &VoiceGenome) -> Vec<VocalMotif> {
    let mut rng = ChaCha8Rng::seed_from_u64(voice.voice_seed);
    (0..8)
        .map(|index| generate_motif(&mut rng, voice, index, None, 0))
        .collect()
}

#[must_use]
pub fn mutate_motif(parent: &VocalMotif, mutation_seed: u64) -> VocalMotif {
    let mut rng = ChaCha8Rng::seed_from_u64(mutation_seed ^ parent.seed);
    let mut child = parent.clone();
    child.id = rng.next_u64();
    child.parent_id = Some(parent.id);
    child.generation = parent.generation.saturating_add(1);
    child.seed = mutation_seed;
    child.use_count = 0;
    child.success_count = 0;
    child.expected_reward = parent.expected_reward * 0.65;
    child.novelty = 1.0;
    for syllable in &mut child.syllables {
        syllable.duration_ms *= 1.0 + signed_random(&mut rng) * 0.10;
        syllable.gap_after_ms *= 1.0 + signed_random(&mut rng) * 0.15;
        syllable.pitch_start *= 1.0 + signed_random(&mut rng) * 0.05;
        syllable.pitch_peak *= 1.0 + signed_random(&mut rng) * 0.05;
        syllable.pitch_end *= 1.0 + signed_random(&mut rng) * 0.05;
        syllable.amplitude = unit(syllable.amplitude * (1.0 + signed_random(&mut rng) * 0.05));
        syllable.noisiness = unit(syllable.noisiness + signed_random(&mut rng) * 0.05);
        syllable.trill_amount = unit(syllable.trill_amount + signed_random(&mut rng) * 0.08);
        clamp_syllable(syllable);
    }
    if random_unit(&mut rng) < 0.08 && child.syllables.len() < 6 {
        let source = child.syllables[(rng.next_u32() as usize) % child.syllables.len()].clone();
        child.syllables.push(source);
    } else if random_unit(&mut rng) < 0.06 && child.syllables.len() > 1 {
        child.syllables.pop();
    }
    child
}

fn generate_motif(
    rng: &mut impl RngCore,
    voice: &VoiceGenome,
    index: usize,
    parent_id: Option<u64>,
    generation: u32,
) -> VocalMotif {
    let syllable_count = 2 + (rng.next_u32() % 4) as usize;
    let mut syllables = Vec::with_capacity(syllable_count);
    let range = voice.pitch_range_octaves * 0.5;
    for _ in 0..syllable_count {
        let start = 2.0_f32.powf(signed_random(rng) * range);
        let peak = start * 2.0_f32.powf(signed_random(rng) * range * 0.6);
        let end = peak * 2.0_f32.powf(signed_random(rng) * range * 0.6);
        syllables.push(Syllable {
            duration_ms: (70.0 + random_unit(rng) * 190.0) / voice.phrase_speed,
            gap_after_ms: random_unit(rng) * 110.0 / voice.phrase_speed,
            pitch_start: start,
            pitch_peak: peak,
            pitch_end: end,
            amplitude: 0.45 + random_unit(rng) * 0.45,
            noisiness: unit(voice.breathiness + signed_random(rng) * 0.12),
            click: unit(voice.click_amount + signed_random(rng) * 0.10),
            mouth_open: 0.35 + random_unit(rng) * 0.60,
            trill_amount: random_unit(rng) * 0.55,
            vibrato_amount: random_unit(rng) * 0.75,
        });
    }
    let id = rng.next_u64() ^ index as u64;
    VocalMotif {
        id,
        parent_id,
        generation,
        seed: rng.next_u64(),
        syllables,
        context_weights: array::from_fn(|_| signed_random(rng) * 0.12),
        expected_reward: 0.0,
        use_count: 0,
        success_count: 0,
        novelty: 1.0,
    }
}

fn clamp_syllable(syllable: &mut Syllable) {
    syllable.duration_ms = syllable.duration_ms.clamp(35.0, 520.0);
    syllable.gap_after_ms = syllable.gap_after_ms.clamp(0.0, 280.0);
    syllable.pitch_start = syllable.pitch_start.clamp(0.45, 2.2);
    syllable.pitch_peak = syllable.pitch_peak.clamp(0.45, 2.2);
    syllable.pitch_end = syllable.pitch_end.clamp(0.45, 2.2);
    syllable.amplitude = unit(syllable.amplitude);
    syllable.noisiness = unit(syllable.noisiness);
    syllable.click = unit(syllable.click);
    syllable.mouth_open = unit(syllable.mouth_open);
    syllable.trill_amount = unit(syllable.trill_amount);
    syllable.vibrato_amount = unit(syllable.vibrato_amount);
}

#[allow(clippy::too_many_arguments)]
fn definition(
    id: ActionId,
    minimum_duration: f32,
    maximum_duration: f32,
    cooldown: f32,
    interruptibility: f32,
    required_conditions: ActionConditions,
    drive_relief: DriveVector,
    unsolicited_attention_cost: f32,
) -> ActionDefinition {
    ActionDefinition {
        id,
        minimum_duration,
        maximum_duration,
        cooldown,
        interruptibility,
        required_conditions,
        drive_relief,
        interruption_cost: (1.0 - interruptibility) * 0.25,
        unsolicited_attention_cost,
    }
}

#[allow(clippy::too_many_arguments)]
const fn relief(
    sleep: f32,
    social: f32,
    play: f32,
    curiosity: f32,
    comfort: f32,
    safety: f32,
    autonomy: f32,
    novelty: f32,
) -> DriveVector {
    DriveVector {
        sleep,
        social,
        play,
        curiosity,
        comfort,
        safety,
        autonomy,
        novelty,
    }
}

fn random_unit(rng: &mut impl RngCore) -> f32 {
    (f64::from(rng.next_u32()) / f64::from(u32::MAX)) as f32
}

fn signed_random(rng: &mut impl RngCore) -> f32 {
    random_unit(rng) * 2.0 - 1.0
}

fn unit(value: f32) -> f32 {
    value.clamp(0.0, 1.0)
}

fn positive_readout(value: f32, threshold: f32) -> f32 {
    unit((value - threshold) / (1.0 - threshold).max(0.001))
}

fn signed(value: f32) -> f32 {
    value.clamp(-1.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn neutral_readouts_do_not_half_activate_face_intensities() {
        let expression =
            ExpressionState::from_readouts([0.0; EXPRESSION_READOUT_COUNT], AffectState::default());
        assert_eq!(expression.blink_left, 0.0);
        assert_eq!(expression.blink_right, 0.0);
        assert_eq!(expression.squint, 0.0);
        assert_eq!(expression.brow_tension, 0.0);
        assert_eq!(expression.mouth_tension, 0.0);
    }

    #[test]
    fn strong_positive_readouts_still_drive_face_intensities() {
        let expression =
            ExpressionState::from_readouts([1.0; EXPRESSION_READOUT_COUNT], AffectState::default());
        assert_eq!(expression.blink_left, 1.0);
        assert_eq!(expression.blink_right, 1.0);
        assert_eq!(expression.squint, 1.0);
        assert_eq!(expression.brow_tension, 1.0);
        assert_eq!(expression.mouth_tension, 1.0);
    }

    #[test]
    fn pupil_readout_tracks_arousal_instead_of_emotional_valence() {
        let pleasant = ExpressionState::from_readouts(
            [0.0; EXPRESSION_READOUT_COUNT],
            AffectState {
                valence: 1.0,
                arousal: 0.8,
                ..AffectState::default()
            },
        );
        let unpleasant = ExpressionState::from_readouts(
            [0.0; EXPRESSION_READOUT_COUNT],
            AffectState {
                valence: -1.0,
                arousal: 0.8,
                stress: 1.0,
                ..AffectState::default()
            },
        );
        assert_eq!(pleasant.pupil_size, unpleasant.pupil_size);
        assert!(pleasant.pupil_size > ExpressionState::default().pupil_size);
    }

    #[test]
    fn solitary_play_is_not_an_attention_bid() {
        assert!(!ActionId::SelfPlay.is_attention_strategy());
    }
}
