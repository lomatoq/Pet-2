use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Wire-format version for the bounded Body Lab -> Pet command slot.
pub const LAB_CONTROL_SCHEMA_VERSION: u32 = 2;
pub const LAB_CONTROL_MIN_EXPIRY_MS: u32 = 100;
pub const LAB_CONTROL_MAX_EXPIRY_MS: u32 = 30_000;

const MIN_ATTENTION_DURATION_SECONDS: f32 = 0.1;
const MAX_ATTENTION_DURATION_SECONDS: f32 = 10.0;
const MIN_DRIVE_PULSE_DURATION_SECONDS: f32 = 0.1;
const MAX_DRIVE_PULSE_DURATION_SECONDS: f32 = 30.0;
const MIN_GESTURE_DURATION_SECONDS: f32 = 0.1;
const MAX_GESTURE_DURATION_SECONDS: f32 = 10.0;

/// A single, short-lived Lab command. The command id is chosen by the writer
/// and lets the runtime de-duplicate repeated reads of the same command slot.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LabControlEnvelope {
    pub schema_version: u32,
    pub command_id: u64,
    pub issued_unix_ms: u64,
    pub expires_after_ms: u32,
    pub command: LabControlCommand,
}

impl LabControlEnvelope {
    pub fn validate(&self) -> Result<(), LabControlValidationError> {
        if self.schema_version != LAB_CONTROL_SCHEMA_VERSION {
            return Err(LabControlValidationError::UnsupportedSchema);
        }
        if self.command_id == 0 {
            return Err(LabControlValidationError::InvalidCommandId);
        }
        if !(LAB_CONTROL_MIN_EXPIRY_MS..=LAB_CONTROL_MAX_EXPIRY_MS).contains(&self.expires_after_ms)
        {
            return Err(LabControlValidationError::InvalidExpiry);
        }
        self.command.validate()
    }

    #[must_use]
    pub fn expires_unix_ms(&self) -> u64 {
        self.issued_unix_ms
            .saturating_add(u64::from(self.expires_after_ms))
    }

    /// Commands expire at the boundary rather than one millisecond after it.
    #[must_use]
    pub fn is_expired(&self, now_unix_ms: u64) -> bool {
        now_unix_ms >= self.expires_unix_ms()
    }
}

/// Deliberately closed command set: no arbitrary text, paths, shell input, or
/// extensible JSON payloads cross this boundary.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum LabControlCommand {
    CueAttention {
        position: [f32; 2],
        duration_seconds: f32,
    },
    DrivePulse {
        drive: LabDrive,
        delta: f32,
        duration_seconds: f32,
    },
    Reward {
        value: f32,
    },
    FocusMode {
        enabled: bool,
    },
    ClearDrivePulses,
    StimulatePointerGesture {
        gesture: LabGesture,
        intensity: f32,
        duration_seconds: f32,
    },
    DeleteGestureConvention {
        convention_id: u64,
    },
    RollbackGestureConventions {
        version: u32,
    },
    ClearGestureConventions,
}

impl LabControlCommand {
    fn validate(&self) -> Result<(), LabControlValidationError> {
        match self {
            Self::CueAttention {
                position,
                duration_seconds,
            } => {
                if position
                    .iter()
                    .any(|coordinate| !coordinate.is_finite() || !(0.0..=1.0).contains(coordinate))
                {
                    return Err(LabControlValidationError::InvalidAttentionPosition);
                }
                validate_duration(
                    *duration_seconds,
                    MIN_ATTENTION_DURATION_SECONDS,
                    MAX_ATTENTION_DURATION_SECONDS,
                    LabControlValidationError::InvalidAttentionDuration,
                )
            }
            Self::DrivePulse {
                delta,
                duration_seconds,
                ..
            } => {
                if !delta.is_finite() || !(-1.0..=1.0).contains(delta) {
                    return Err(LabControlValidationError::InvalidDriveDelta);
                }
                validate_duration(
                    *duration_seconds,
                    MIN_DRIVE_PULSE_DURATION_SECONDS,
                    MAX_DRIVE_PULSE_DURATION_SECONDS,
                    LabControlValidationError::InvalidDriveDuration,
                )
            }
            Self::Reward { value } => {
                if !value.is_finite() || *value == 0.0 || !(-1.0..=1.0).contains(value) {
                    return Err(LabControlValidationError::InvalidReward);
                }
                Ok(())
            }
            Self::StimulatePointerGesture {
                intensity,
                duration_seconds,
                ..
            } => {
                if !intensity.is_finite() || !(0.0..=1.0).contains(intensity) {
                    return Err(LabControlValidationError::InvalidGestureIntensity);
                }
                validate_duration(
                    *duration_seconds,
                    MIN_GESTURE_DURATION_SECONDS,
                    MAX_GESTURE_DURATION_SECONDS,
                    LabControlValidationError::InvalidGestureDuration,
                )
            }
            Self::DeleteGestureConvention { convention_id } if *convention_id == 0 => {
                Err(LabControlValidationError::InvalidConventionId)
            }
            Self::RollbackGestureConventions { version } if *version == 0 => {
                Err(LabControlValidationError::InvalidConventionVersion)
            }
            Self::FocusMode { .. }
            | Self::ClearDrivePulses
            | Self::DeleteGestureConvention { .. }
            | Self::RollbackGestureConventions { .. }
            | Self::ClearGestureConventions => Ok(()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LabGesture {
    SoftTouch,
    SlowStretch,
    Tickle,
    ThreeBeatRhythm,
    CircularTwist,
    SharpFlick,
    Hold,
    PullRelease,
    RealSplitRemerge,
    FragmentHelp,
    OverstrainBoundary,
    SleepQuietInteraction,
}

fn validate_duration(
    duration_seconds: f32,
    minimum: f32,
    maximum: f32,
    error: LabControlValidationError,
) -> Result<(), LabControlValidationError> {
    if duration_seconds.is_finite() && (minimum..=maximum).contains(&duration_seconds) {
        Ok(())
    } else {
        Err(error)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LabDrive {
    Safety,
    Play,
    Curiosity,
    Autonomy,
    Sleep,
    Social,
    Comfort,
    Novelty,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum LabControlValidationError {
    #[error("unsupported Lab control schema")]
    UnsupportedSchema,
    #[error("Lab control command_id must be non-zero")]
    InvalidCommandId,
    #[error("Lab control expiry is outside the bounded lifetime")]
    InvalidExpiry,
    #[error("attention position must contain two finite normalized coordinates")]
    InvalidAttentionPosition,
    #[error("attention cue duration is outside its safe range")]
    InvalidAttentionDuration,
    #[error("drive pulse delta must be finite and between -1 and 1")]
    InvalidDriveDelta,
    #[error("drive pulse duration is outside its safe range")]
    InvalidDriveDuration,
    #[error("reward must be finite, non-zero, and between -1 and 1")]
    InvalidReward,
    #[error("gesture intensity must be finite and between 0 and 1")]
    InvalidGestureIntensity,
    #[error("gesture fixture duration is outside its safe range")]
    InvalidGestureDuration,
    #[error("gesture convention id must be non-zero")]
    InvalidConventionId,
    #[error("gesture convention rollback version must be non-zero")]
    InvalidConventionVersion,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn envelope(command: LabControlCommand) -> LabControlEnvelope {
        LabControlEnvelope {
            schema_version: LAB_CONTROL_SCHEMA_VERSION,
            command_id: 7,
            issued_unix_ms: 1_000,
            expires_after_ms: 2_000,
            command,
        }
    }

    #[test]
    fn serde_round_trip_uses_a_closed_tagged_contract() {
        let control = envelope(LabControlCommand::DrivePulse {
            drive: LabDrive::Curiosity,
            delta: 0.4,
            duration_seconds: 3.5,
        });
        control.validate().unwrap();

        let json = serde_json::to_string(&control).unwrap();
        assert!(json.contains("\"type\":\"drive_pulse\""));
        assert_eq!(
            serde_json::from_str::<LabControlEnvelope>(&json).unwrap(),
            control
        );
    }

    #[test]
    fn rejects_unknown_fields_and_command_types() {
        let unknown_field = r#"{
            "schema_version":1,"command_id":1,"issued_unix_ms":1,
            "expires_after_ms":1000,"command":{"type":"focus_mode","enabled":true},
            "path":"private.txt"
        }"#;
        assert!(serde_json::from_str::<LabControlEnvelope>(unknown_field).is_err());

        let unknown_command = r#"{
            "schema_version":1,"command_id":1,"issued_unix_ms":1,
            "expires_after_ms":1000,"command":{"type":"run_program","path":"x"}
        }"#;
        assert!(serde_json::from_str::<LabControlEnvelope>(unknown_command).is_err());
    }

    #[test]
    fn validates_schema_identity_expiry_and_numeric_bounds() {
        let mut control = envelope(LabControlCommand::CueAttention {
            position: [0.25, 1.0],
            duration_seconds: 0.1,
        });
        control.validate().unwrap();

        control.schema_version = 1;
        assert_eq!(
            control.validate(),
            Err(LabControlValidationError::UnsupportedSchema)
        );
        control.schema_version = LAB_CONTROL_SCHEMA_VERSION;
        control.command_id = 0;
        assert_eq!(
            control.validate(),
            Err(LabControlValidationError::InvalidCommandId)
        );
        control.command_id = 1;
        control.expires_after_ms = LAB_CONTROL_MIN_EXPIRY_MS - 1;
        assert_eq!(
            control.validate(),
            Err(LabControlValidationError::InvalidExpiry)
        );

        for invalid in [f32::NAN, f32::INFINITY, -0.01, 1.01] {
            let control = envelope(LabControlCommand::CueAttention {
                position: [invalid, 0.5],
                duration_seconds: 1.0,
            });
            assert_eq!(
                control.validate(),
                Err(LabControlValidationError::InvalidAttentionPosition)
            );
        }
        for invalid in [f32::NAN, f32::NEG_INFINITY, -1.01, 1.01] {
            let control = envelope(LabControlCommand::DrivePulse {
                drive: LabDrive::Safety,
                delta: invalid,
                duration_seconds: 1.0,
            });
            assert_eq!(
                control.validate(),
                Err(LabControlValidationError::InvalidDriveDelta)
            );
        }
        for invalid in [f32::NAN, f32::INFINITY, -1.01, 0.0, -0.0, 1.01] {
            let control = envelope(LabControlCommand::Reward { value: invalid });
            assert_eq!(
                control.validate(),
                Err(LabControlValidationError::InvalidReward)
            );
        }
    }

    #[test]
    fn validates_duration_ranges_inclusively() {
        for duration_seconds in [0.1, 10.0] {
            envelope(LabControlCommand::CueAttention {
                position: [0.0, 1.0],
                duration_seconds,
            })
            .validate()
            .unwrap();
        }
        for duration_seconds in [f32::NAN, 0.099, 10.001] {
            assert_eq!(
                envelope(LabControlCommand::CueAttention {
                    position: [0.5, 0.5],
                    duration_seconds,
                })
                .validate(),
                Err(LabControlValidationError::InvalidAttentionDuration)
            );
        }
        for duration_seconds in [0.1, 30.0] {
            envelope(LabControlCommand::DrivePulse {
                drive: LabDrive::Novelty,
                delta: 0.0,
                duration_seconds,
            })
            .validate()
            .unwrap();
        }
        for duration_seconds in [f32::NEG_INFINITY, 0.099, 30.001] {
            assert_eq!(
                envelope(LabControlCommand::DrivePulse {
                    drive: LabDrive::Novelty,
                    delta: 0.0,
                    duration_seconds,
                })
                .validate(),
                Err(LabControlValidationError::InvalidDriveDuration)
            );
        }
    }

    #[test]
    fn expiry_is_saturating_and_expires_at_the_boundary() {
        let control = envelope(LabControlCommand::ClearDrivePulses);
        assert!(!control.is_expired(2_999));
        assert!(control.is_expired(3_000));

        let mut near_overflow = control;
        near_overflow.issued_unix_ms = u64::MAX - 5;
        assert_eq!(near_overflow.expires_unix_ms(), u64::MAX);
        assert!(!near_overflow.is_expired(u64::MAX - 1));
        assert!(near_overflow.is_expired(u64::MAX));
    }
}
