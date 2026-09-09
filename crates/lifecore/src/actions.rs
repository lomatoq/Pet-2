use std::array;

use glam::Vec2;
use rand::{RngCore, SeedableRng};
use rand_chacha::ChaCha8Rng;
use serde::{Deserialize, Serialize};

use crate::{
    AffectState, DriveVector, EmbodiedInteractionFrame, InteractionBodyActuation, VoiceGenome,
};

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

    /// Actions that remain semantically honest during automatically detected
    /// work. Unlike explicit Focus Mode this still permits quiet self-directed
    /// activity, but never turns a work cue into a social bid.
    #[must_use]
    pub const fn is_desktop_work_allowed(self) -> bool {
        matches!(
            self,
            Self::Sleep
                | Self::WakeUp
                | Self::IdleHover
                | Self::ObserveCursor
                | Self::ObserveUserActivity
                | Self::RetreatFromCursor
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
    /// Aggregate protection pressure inferred from typing/scroll rhythm. This
    /// contains no content and is transient rather than a saved user setting.
    pub desktop_focus_pressure: f32,
    #[serde(default)]
    pub embodied_interaction: EmbodiedInteractionFrame,
    #[serde(default)]
    pub interaction_actuation: InteractionBodyActuation,
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
            desktop_focus_pressure: 0.0,
            embodied_interaction: EmbodiedInteractionFrame::default(),
            interaction_actuation: InteractionBodyActuation::default(),
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
    #[serde(default)]
    pub geometry: crate::FaceGeometry,
    #[serde(default)]
    pub face_pose: crate::FacePose,
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
    #[serde(default = "default_eye_aperture")]
    pub eye_aperture: f32,
    #[serde(default = "default_eye_scale")]
    pub eye_scale: f32,
    #[serde(default)]
    pub brow_asymmetry: f32,
    #[serde(default)]
    pub mouth_compression: f32,
    #[serde(default)]
    pub mouth_asymmetry: f32,
    #[serde(default)]
    pub effort: f32,
    #[serde(default)]
    pub relief: f32,
}

impl Default for ExpressionState {
    fn default() -> Self {
        Self {
            geometry: crate::FaceGeometry::default(),
            face_pose: crate::FacePose::Awake,
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
            eye_aperture: 1.0,
            eye_scale: 1.0,
            brow_asymmetry: 0.0,
            mouth_compression: 0.0,
            mouth_asymmetry: 0.0,
            effort: 0.0,
            relief: 0.0,
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
            ..Self::default()
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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VocalStyle {
    #[default]
    SocialContact,
    TouchResponse,
    PlayInvite,
    AttentionCall,
    ContentMurmur,
    Purr,
    RhythmMimic,
    Startle,
    Frustrated,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VocalFamily {
    #[default]
    SoftContact,
    QuestionWhine,
    PlayYip,
    AttentionCall,
    ContentMurmur,
    Purr,
    StartleSqueak,
    FrustratedGrunt,
    RhythmMimic,
    PlayfulTrill,
}

impl VocalFamily {
    pub const ALL: [Self; 10] = [
        Self::SoftContact,
        Self::QuestionWhine,
        Self::PlayYip,
        Self::AttentionCall,
        Self::ContentMurmur,
        Self::Purr,
        Self::StartleSqueak,
        Self::FrustratedGrunt,
        Self::RhythmMimic,
        Self::PlayfulTrill,
    ];
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct VocalGesture {
    pub pressure_peak: f32,
    pub adduction: f32,
    pub open_quotient: f32,
    pub closure_sharpness: f32,
    pub frontness: f32,
    pub tongue_height: f32,
    pub lip_rounding: f32,
    pub constriction: f32,
    pub nasality: f32,
    pub instability: f32,
    pub body_excitation: f32,
}

impl Default for VocalGesture {
    fn default() -> Self {
        Self {
            pressure_peak: 0.55,
            adduction: 0.52,
            open_quotient: 0.58,
            closure_sharpness: 0.46,
            frontness: 0.0,
            tongue_height: 0.0,
            lip_rounding: 0.30,
            constriction: 0.20,
            nasality: 0.16,
            instability: 0.08,
            body_excitation: 0.42,
        }
    }
}

impl VocalGesture {
    pub fn sanitize(&mut self) {
        self.pressure_peak = unit(self.pressure_peak);
        self.adduction = unit(self.adduction);
        self.open_quotient = self.open_quotient.clamp(0.30, 0.82);
        self.closure_sharpness = unit(self.closure_sharpness);
        self.frontness = self.frontness.clamp(-1.0, 1.0);
        self.tongue_height = self.tongue_height.clamp(-1.0, 1.0);
        self.lip_rounding = unit(self.lip_rounding);
        self.constriction = unit(self.constriction);
        self.nasality = unit(self.nasality);
        self.instability = unit(self.instability);
        self.body_excitation = unit(self.body_excitation);
    }

    #[must_use]
    pub fn is_valid(self) -> bool {
        let mut sanitized = self;
        sanitized.sanitize();
        sanitized == self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct BodyVoiceFrame {
    pub main_mass_ratio: f32,
    pub detached_mass_ratio: f32,
    pub component_count: u8,
    pub shape_aspect_ratio: f32,
    pub stretch: f32,
    pub compression: f32,
    pub bond_strain: f32,
    pub material_stress: f32,
    pub contact_area: f32,
    pub slosh_energy: f32,
    pub internal_speed: f32,
    pub collision_impulse: f32,
    pub contact_impulse: f32,
    pub release_impulse: f32,
    pub detach_impulse: f32,
    pub remerge_impulse: f32,
}

impl Default for BodyVoiceFrame {
    fn default() -> Self {
        Self {
            main_mass_ratio: 1.0,
            detached_mass_ratio: 0.0,
            component_count: 1,
            shape_aspect_ratio: 1.0,
            stretch: 0.0,
            compression: 0.0,
            bond_strain: 0.0,
            material_stress: 0.0,
            contact_area: 0.0,
            slosh_energy: 0.0,
            internal_speed: 0.0,
            collision_impulse: 0.0,
            contact_impulse: 0.0,
            release_impulse: 0.0,
            detach_impulse: 0.0,
            remerge_impulse: 0.0,
        }
    }
}

impl BodyVoiceFrame {
    #[must_use]
    pub fn sanitized(mut self) -> Self {
        let finite_unit = |value: f32| {
            if value.is_finite() {
                value.clamp(0.0, 1.0)
            } else {
                0.0
            }
        };
        self.main_mass_ratio = finite_unit(self.main_mass_ratio);
        self.detached_mass_ratio = finite_unit(self.detached_mass_ratio);
        self.component_count = self.component_count.clamp(1, 4);
        self.shape_aspect_ratio = if self.shape_aspect_ratio.is_finite() {
            self.shape_aspect_ratio.clamp(1.0, 4.0)
        } else {
            1.0
        };
        self.stretch = if self.stretch.is_finite() {
            self.stretch.clamp(-1.0, 1.0)
        } else {
            0.0
        };
        self.compression = finite_unit(self.compression);
        self.bond_strain = finite_unit(self.bond_strain);
        self.material_stress = finite_unit(self.material_stress);
        self.contact_area = finite_unit(self.contact_area);
        self.slosh_energy = finite_unit(self.slosh_energy);
        self.internal_speed = finite_unit(self.internal_speed);
        self.collision_impulse = finite_unit(self.collision_impulse);
        self.contact_impulse = finite_unit(self.contact_impulse);
        self.release_impulse = finite_unit(self.release_impulse);
        self.detach_impulse = finite_unit(self.detach_impulse);
        self.remerge_impulse = finite_unit(self.remerge_impulse);
        self
    }
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
    #[serde(default)]
    pub gesture: crate::VoiceGesture,
    #[serde(default)]
    pub priority: u8,
    #[serde(default)]
    pub style: VocalStyle,
    #[serde(default)]
    pub valence: f32,
    #[serde(default)]
    pub arousal: f32,
    #[serde(default)]
    pub fatigue: f32,
    #[serde(default)]
    pub confidence: f32,
    #[serde(default)]
    pub attachment: f32,
    /// Optional normalized inter-onset intervals for a grounded rhythm echo.
    /// Zero entries mean no authored interval; waveform ownership stays in
    /// `pet_audio` and no raw input timing history is retained.
    #[serde(default)]
    pub rhythm_intervals: [f32; 8],
    /// Ephemeral, bounded prosody deltas from the nervous-system phenotype.
    /// Identity and hard loudness limits remain owned by `VoiceGenome`.
    #[serde(default)]
    pub phenotype: crate::VoicePhenotypeActuation,
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
    SoftTouch,
    PhysicalStartle,
    PlayfulRelease,
    CalmBoundary,
    ComponentDetached,
    ComponentRemerged,
    FragmentHelped,
}

impl VocalTrigger {
    #[must_use]
    pub const fn is_supported(self) -> bool {
        match self {
            Self::Action(action) => action.is_vocal(),
            Self::ToyOffer
            | Self::CatchSuccess
            | Self::MissAndRetry
            | Self::NeedHelp
            | Self::FoodInspect
            | Self::FoodAccepted
            | Self::FoodRefused
            | Self::HomeReturn
            | Self::SkillMastered
            | Self::RhythmEcho
            | Self::VisualNotice
            | Self::SoftTouch
            | Self::PhysicalStartle
            | Self::PlayfulRelease
            | Self::CalmBoundary
            | Self::ComponentDetached
            | Self::ComponentRemerged
            | Self::FragmentHelped => true,
        }
    }

    #[must_use]
    pub const fn expects_response(self) -> bool {
        matches!(
            self,
            Self::Action(ActionId::Chirp | ActionId::MimicClickRhythm) | Self::ToyOffer
        )
    }

    #[must_use]
    pub const fn priority(self) -> u8 {
        match self {
            Self::PhysicalStartle | Self::ComponentDetached => 255,
            Self::CalmBoundary => 240,
            Self::NeedHelp | Self::FragmentHelped => 220,
            Self::ComponentRemerged | Self::FoodAccepted => 190,
            Self::Action(ActionId::Purr) | Self::HomeReturn | Self::SoftTouch => 150,
            Self::Action(_)
            | Self::ToyOffer
            | Self::CatchSuccess
            | Self::MissAndRetry
            | Self::FoodInspect
            | Self::FoodRefused
            | Self::SkillMastered
            | Self::RhythmEcho
            | Self::VisualNotice
            | Self::PlayfulRelease => 170,
        }
    }
}

const fn default_eye_aperture() -> f32 {
    1.0
}

const fn default_eye_scale() -> f32 {
    1.0
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VocalMotif {
    pub id: u64,
    pub parent_id: Option<u64>,
    pub generation: u32,
    pub seed: u64,
    #[serde(default)]
    pub family: VocalFamily,
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
    #[serde(default)]
    pub gesture: VocalGesture,
}

#[must_use]
pub fn generate_initial_motifs(voice: &VoiceGenome) -> Vec<VocalMotif> {
    let mut rng = ChaCha8Rng::seed_from_u64(voice.voice_seed);
    (0..VocalFamily::ALL.len())
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
        syllable.gesture.pressure_peak =
            unit(syllable.gesture.pressure_peak * (1.0 + signed_random(&mut rng) * 0.055));
        syllable.gesture.adduction =
            unit(syllable.gesture.adduction + signed_random(&mut rng) * 0.035);
        syllable.gesture.open_quotient =
            (syllable.gesture.open_quotient + signed_random(&mut rng) * 0.025).clamp(0.30, 0.82);
        syllable.gesture.closure_sharpness =
            unit(syllable.gesture.closure_sharpness + signed_random(&mut rng) * 0.04);
        syllable.gesture.frontness =
            (syllable.gesture.frontness + signed_random(&mut rng) * 0.05).clamp(-1.0, 1.0);
        syllable.gesture.constriction =
            unit(syllable.gesture.constriction + signed_random(&mut rng) * 0.04);
        syllable.gesture.nasality =
            unit(syllable.gesture.nasality + signed_random(&mut rng) * 0.035);
        syllable.gesture.instability =
            unit(syllable.gesture.instability + signed_random(&mut rng) * 0.035);
        syllable.gesture.body_excitation =
            unit(syllable.gesture.body_excitation + signed_random(&mut rng) * 0.035);
        clamp_syllable(syllable);
    }
    let (minimum_count, maximum_count) = family_syllable_bounds(child.family);
    if random_unit(&mut rng) < 0.08 && child.syllables.len() < maximum_count {
        let source = child.syllables[(rng.next_u32() as usize) % child.syllables.len()].clone();
        child.syllables.push(source);
    } else if random_unit(&mut rng) < 0.06 && child.syllables.len() > minimum_count {
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
    let family = VocalFamily::ALL[index % VocalFamily::ALL.len()];
    let (minimum_count, maximum_count) = family_syllable_bounds(family);
    let syllable_count = if minimum_count == maximum_count {
        minimum_count
    } else {
        minimum_count + (rng.next_u32() as usize % (maximum_count - minimum_count + 1))
    };
    let mut syllables = Vec::with_capacity(syllable_count);
    for syllable_index in 0..syllable_count {
        let mut syllable = canonical_syllable(family, syllable_index, voice);
        syllable.duration_ms *= 1.0 + signed_random(rng) * 0.07;
        syllable.gap_after_ms *= 1.0 + signed_random(rng) * 0.10;
        let identity_pitch =
            2.0_f32.powf(signed_random(rng) * voice.pitch_range_octaves.clamp(0.3, 2.0) * 0.045);
        syllable.pitch_start *= identity_pitch;
        syllable.pitch_peak *= identity_pitch * (1.0 + signed_random(rng) * 0.025);
        syllable.pitch_end *= identity_pitch * (1.0 + signed_random(rng) * 0.025);
        syllable.gesture.frontness =
            (syllable.gesture.frontness + signed_random(rng) * 0.06).clamp(-1.0, 1.0);
        syllable.gesture.nasality = unit(syllable.gesture.nasality + signed_random(rng) * 0.04);
        syllable.gesture.body_excitation =
            unit(syllable.gesture.body_excitation + signed_random(rng) * 0.04);
        clamp_syllable(&mut syllable);
        syllables.push(syllable);
    }
    let id = rng.next_u64() ^ index as u64;
    VocalMotif {
        id,
        parent_id,
        generation,
        seed: rng.next_u64(),
        family,
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
    syllable.gesture.sanitize();
}

/// Repairs additive fields from legacy motifs without changing their learned IDs.
pub fn repair_vocal_motifs(motifs: &mut [VocalMotif]) {
    for motif in motifs {
        let legacy_gestures = motif
            .syllables
            .iter()
            .all(|syllable| syllable.gesture == VocalGesture::default());
        if legacy_gestures {
            motif.family =
                VocalFamily::ALL[(splitmix64(motif.seed) as usize) % VocalFamily::ALL.len()];
        }
        for (index, syllable) in motif.syllables.iter_mut().enumerate() {
            if legacy_gestures || !syllable.gesture.is_valid() {
                syllable.gesture = legacy_gesture(motif.family, motif.seed, index, syllable);
            }
            clamp_syllable(syllable);
        }
    }
}

#[must_use]
pub fn gesture_distance(left: VocalGesture, right: VocalGesture) -> f32 {
    0.18 * (left.pressure_peak - right.pressure_peak).abs()
        + 0.14 * (left.adduction - right.adduction).abs()
        + 0.10 * (left.open_quotient - right.open_quotient).abs()
        + 0.12 * (left.frontness - right.frontness).abs()
        + 0.12 * (left.constriction - right.constriction).abs()
        + 0.10 * (left.nasality - right.nasality).abs()
        + 0.12 * (left.instability - right.instability).abs()
        + 0.12 * (left.body_excitation - right.body_excitation).abs()
}

fn family_syllable_bounds(family: VocalFamily) -> (usize, usize) {
    match family {
        VocalFamily::PlayYip | VocalFamily::RhythmMimic => (2, 3),
        VocalFamily::AttentionCall | VocalFamily::PlayfulTrill => (2, 2),
        _ => (1, 1),
    }
}

fn canonical_syllable(family: VocalFamily, index: usize, voice: &VoiceGenome) -> Syllable {
    let (
        duration_ms,
        gap_after_ms,
        pitch_start,
        pitch_peak,
        pitch_end,
        amplitude,
        noisiness,
        click,
        mouth_open,
        trill_amount,
        gesture,
    ) = match family {
        VocalFamily::SoftContact => (
            225.0,
            0.0,
            0.88,
            1.00,
            1.07,
            0.56,
            0.16,
            0.04,
            0.58,
            0.04,
            VocalGesture {
                pressure_peak: 0.46,
                adduction: 0.46,
                open_quotient: 0.64,
                closure_sharpness: 0.38,
                frontness: 0.12,
                tongue_height: 0.06,
                lip_rounding: 0.24,
                constriction: 0.14,
                nasality: 0.18,
                instability: 0.05,
                body_excitation: 0.42,
            },
        ),
        VocalFamily::QuestionWhine => (
            455.0,
            0.0,
            0.78,
            1.24,
            1.06,
            0.62,
            0.12,
            0.02,
            0.72,
            0.05,
            VocalGesture {
                pressure_peak: 0.58,
                adduction: 0.52,
                open_quotient: 0.67,
                closure_sharpness: 0.42,
                frontness: 0.34,
                tongue_height: 0.10,
                lip_rounding: 0.18,
                constriction: 0.10,
                nasality: 0.20,
                instability: 0.08,
                body_excitation: 0.38,
            },
        ),
        VocalFamily::PlayYip => (
            105.0 + index as f32 * 12.0,
            62.0,
            0.96,
            1.28 - index as f32 * 0.05,
            1.08,
            0.70 - index as f32 * 0.05,
            0.14,
            0.12,
            0.68,
            0.10,
            VocalGesture {
                pressure_peak: 0.72 - index as f32 * 0.07,
                adduction: 0.62,
                open_quotient: 0.52,
                closure_sharpness: 0.68,
                frontness: 0.26,
                tongue_height: 0.05,
                lip_rounding: 0.12,
                constriction: 0.22,
                nasality: 0.08,
                instability: 0.14,
                body_excitation: 0.56,
            },
        ),
        VocalFamily::AttentionCall => (
            if index == 0 { 245.0 } else { 190.0 },
            if index == 0 { 92.0 } else { 0.0 },
            if index == 0 { 0.88 } else { 0.94 },
            if index == 0 { 1.18 } else { 1.08 },
            if index == 0 { 0.98 } else { 1.02 },
            if index == 0 { 0.72 } else { 0.58 },
            0.15,
            0.08,
            0.66,
            0.05,
            VocalGesture {
                pressure_peak: if index == 0 { 0.72 } else { 0.54 },
                adduction: 0.58,
                open_quotient: 0.58,
                closure_sharpness: 0.54,
                frontness: 0.18,
                tongue_height: 0.04,
                lip_rounding: 0.20,
                constriction: 0.18,
                nasality: 0.14,
                instability: 0.08,
                body_excitation: 0.48,
            },
        ),
        VocalFamily::ContentMurmur => (
            430.0,
            0.0,
            0.62,
            0.67,
            0.59,
            0.52,
            0.18,
            0.01,
            0.18,
            0.02,
            VocalGesture {
                pressure_peak: 0.38,
                adduction: 0.44,
                open_quotient: 0.70,
                closure_sharpness: 0.30,
                frontness: -0.28,
                tongue_height: -0.12,
                lip_rounding: 0.58,
                constriction: 0.30,
                nasality: 0.68,
                instability: 0.04,
                body_excitation: 0.82,
            },
        ),
        VocalFamily::Purr => (
            500.0,
            0.0,
            0.54,
            0.57,
            0.53,
            0.48,
            0.20,
            0.0,
            0.10,
            0.0,
            VocalGesture {
                pressure_peak: 0.30,
                adduction: 0.50,
                open_quotient: 0.72,
                closure_sharpness: 0.34,
                frontness: -0.36,
                tongue_height: -0.18,
                lip_rounding: 0.62,
                constriction: 0.28,
                nasality: 0.62,
                instability: 0.10,
                body_excitation: 0.88,
            },
        ),
        VocalFamily::StartleSqueak => (
            92.0,
            0.0,
            0.94,
            1.48,
            1.08,
            0.66,
            0.22,
            0.20,
            0.80,
            0.04,
            VocalGesture {
                pressure_peak: 0.92,
                adduction: 0.76,
                open_quotient: 0.42,
                closure_sharpness: 0.84,
                frontness: 0.42,
                tongue_height: 0.20,
                lip_rounding: 0.06,
                constriction: 0.30,
                nasality: 0.04,
                instability: 0.58,
                body_excitation: 0.62,
            },
        ),
        VocalFamily::FrustratedGrunt => (
            175.0,
            0.0,
            0.72,
            0.68,
            0.57,
            0.66,
            0.34,
            0.10,
            0.30,
            0.03,
            VocalGesture {
                pressure_peak: 0.74,
                adduction: 0.78,
                open_quotient: 0.40,
                closure_sharpness: 0.72,
                frontness: -0.22,
                tongue_height: 0.28,
                lip_rounding: 0.38,
                constriction: 0.66,
                nasality: 0.22,
                instability: 0.36,
                body_excitation: 0.66,
            },
        ),
        VocalFamily::RhythmMimic => (
            82.0,
            105.0,
            0.92,
            1.10,
            0.96,
            0.60,
            0.16,
            0.28,
            0.55,
            0.04,
            VocalGesture {
                pressure_peak: 0.62,
                adduction: 0.64,
                open_quotient: 0.50,
                closure_sharpness: 0.72,
                frontness: 0.18,
                tongue_height: 0.12,
                lip_rounding: 0.16,
                constriction: 0.32,
                nasality: 0.10,
                instability: 0.12,
                body_excitation: 0.48,
            },
        ),
        VocalFamily::PlayfulTrill => (
            205.0,
            if index == 0 { 70.0 } else { 0.0 },
            0.90,
            1.15,
            1.03,
            0.64,
            0.16,
            0.08,
            0.62,
            0.48,
            VocalGesture {
                pressure_peak: 0.64,
                adduction: 0.58,
                open_quotient: 0.56,
                closure_sharpness: 0.56,
                frontness: 0.24,
                tongue_height: 0.06,
                lip_rounding: 0.14,
                constriction: 0.38,
                nasality: 0.12,
                instability: 0.24,
                body_excitation: 0.54,
            },
        ),
    };
    Syllable {
        duration_ms: duration_ms / voice.phrase_speed.clamp(0.5, 1.8),
        gap_after_ms: gap_after_ms / voice.phrase_speed.clamp(0.5, 1.8),
        pitch_start,
        pitch_peak,
        pitch_end,
        amplitude,
        noisiness: unit(noisiness + voice.breathiness * 0.20),
        click: unit(click + voice.click_amount * 0.15),
        mouth_open,
        trill_amount,
        vibrato_amount: 0.0,
        gesture,
    }
}

fn legacy_gesture(
    family: VocalFamily,
    motif_seed: u64,
    index: usize,
    syllable: &Syllable,
) -> VocalGesture {
    let mut gesture = canonical_syllable(family, index, &legacy_voice_identity()).gesture;
    let variation = seeded_signed(motif_seed ^ index as u64 ^ 0x76ab_31d2);
    gesture.pressure_peak = unit(0.34 + syllable.amplitude * 0.56 + variation * 0.03);
    gesture.adduction = unit(0.40 + syllable.click * 0.22 + syllable.trill_amount * 0.08);
    gesture.open_quotient =
        (0.68 - syllable.click * 0.16 - syllable.noisiness * 0.08).clamp(0.30, 0.82);
    gesture.closure_sharpness = unit(0.30 + syllable.click * 0.52);
    gesture.frontness = (syllable.pitch_end - syllable.pitch_start).clamp(-1.0, 1.0);
    gesture.constriction = unit(0.12 + syllable.click * 0.28 + syllable.trill_amount * 0.18);
    gesture.nasality = unit(0.10 + (1.0 - syllable.mouth_open) * 0.48);
    gesture.instability = unit(0.04 + syllable.trill_amount * 0.30 + syllable.noisiness * 0.16);
    gesture.body_excitation = unit(0.26 + syllable.amplitude * 0.48);
    gesture.sanitize();
    gesture
}

fn legacy_voice_identity() -> VoiceGenome {
    let mut voice = crate::Genome::from_seed(0x564f_4943_455f_5631).voice;
    voice.phrase_speed = 1.0;
    voice
}

fn splitmix64(mut value: u64) -> u64 {
    value = value.wrapping_add(0x9e37_79b9_7f4a_7c15);
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

fn seeded_signed(seed: u64) -> f32 {
    let bits = (splitmix64(seed) >> 40) as u32;
    bits as f32 / 0x00ff_ffff as f32 * 2.0 - 1.0
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
