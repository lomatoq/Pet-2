//! Transient presentation geometry. No affect, reward or learned state is owned here.
use serde::{Deserialize, Serialize};

use crate::ExpressionState;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct FaceGeometry {
    /// Per eye: upper inner/outer height offsets, lower raise, upper curvature.
    pub lids: [[f32; 4]; 2],
    /// Per brow: inner/outer height offsets, arc height, thickness multiplier.
    pub brows: [[f32; 4]; 2],
    /// Width multiplier, left/right corner height, compression.
    pub mouth: [f32; 4],
}

impl Default for FaceGeometry {
    fn default() -> Self {
        Self {
            lids: [[0.0; 4]; 2],
            brows: [[0.0, 0.0, 0.25, 1.0]; 2],
            mouth: [1.0, 0.0, 0.0, 0.0],
        }
    }
}

impl FaceGeometry {
    pub fn maximum_error(self, other: Self) -> f32 {
        self.lids
            .iter()
            .flatten()
            .chain(self.brows.iter().flatten())
            .chain(self.mouth.iter())
            .zip(
                other
                    .lids
                    .iter()
                    .flatten()
                    .chain(other.brows.iter().flatten())
                    .chain(other.mouth.iter()),
            )
            .map(|(a, b)| (a - b).abs())
            .fold(0.0, f32::max)
    }

    pub fn sanitized(mut self) -> Self {
        fn bound(v: f32, lo: f32, hi: f32, fallback: f32) -> f32 {
            if v.is_finite() {
                v.clamp(lo, hi)
            } else {
                fallback
            }
        }
        for lid in &mut self.lids {
            for v in &mut lid[..2] {
                *v = bound(*v, -1.0, 1.0, 0.0);
            }
            lid[2] = bound(lid[2], 0.0, 1.0, 0.0);
            lid[3] = bound(lid[3], -1.0, 1.0, 0.0);
        }
        for brow in &mut self.brows {
            for v in &mut brow[..3] {
                *v = bound(*v, -1.0, 1.0, 0.0);
            }
            brow[3] = bound(brow[3], 0.5, 1.8, 1.0);
        }
        self.mouth[0] = bound(self.mouth[0], 0.35, 1.6, 1.0);
        self.mouth[1] = bound(self.mouth[1], -1.0, 1.0, 0.0);
        self.mouth[2] = bound(self.mouth[2], -1.0, 1.0, 0.0);
        self.mouth[3] = bound(self.mouth[3], 0.0, 1.0, 0.0);
        self
    }

    pub fn approach(&mut self, target: Self, amount: f32) {
        let target = target.sanitized();
        let amount = amount.clamp(0.0, 1.0);
        for (current, desired) in self
            .lids
            .iter_mut()
            .flatten()
            .chain(self.brows.iter_mut().flatten())
            .chain(self.mouth.iter_mut())
            .zip(
                target
                    .lids
                    .iter()
                    .flatten()
                    .chain(target.brows.iter().flatten())
                    .chain(target.mouth.iter()),
            )
        {
            *current += (*desired - *current) * amount;
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FacePose {
    #[default]
    Awake,
    Curious,
    Playful,
    Affectionate,
    Confused,
    Tired,
    Startled,
    Boundary,
}

impl FacePose {
    pub const ALL: [Self; 8] = [
        Self::Awake,
        Self::Curious,
        Self::Playful,
        Self::Affectionate,
        Self::Confused,
        Self::Tired,
        Self::Startled,
        Self::Boundary,
    ];

    /// Same canonical endpoint for autonomous composition and Lab interventions.
    #[must_use]
    pub fn expression(self) -> ExpressionState {
        let mut e = ExpressionState {
            face_pose: self,
            ..ExpressionState::default()
        };
        match self {
            Self::Awake => {}
            Self::Curious => {
                e.brow_raise = 0.65;
                e.pupil_focus = 0.9;
                e.geometry.lids = [[0.22, 0.22, 0.0, 0.3]; 2];
                e.geometry.brows = [[0.35, 0.2, 0.8, 1.0]; 2];
                e.geometry.mouth[0] = 0.65;
            }
            Self::Playful => {
                e.mouth_curve = 0.85;
                e.mouth_open = 0.38;
                e.brow_raise = 0.35;
                e.geometry.lids = [[0.15, 0.15, 0.20, 0.2]; 2];
                e.geometry.mouth = [1.2, 0.6, 0.6, 0.0];
            }
            Self::Affectionate => {
                e.mouth_curve = 0.60;
                e.cheek_glow = 0.45;
                e.geometry.lids = [[0.0, 0.0, 0.55, 0.25]; 2];
                e.geometry.brows = [[0.12, 0.05, 0.5, 0.9]; 2];
                e.geometry.mouth = [1.1, 0.35, 0.35, 0.0];
            }
            Self::Confused => {
                e.brow_asymmetry = 0.65;
                e.mouth_open = 0.30;
                e.geometry.lids = [[0.3, 0.2, 0.0, 0.3], [-0.12, -0.05, 0.05, 0.0]];
                e.geometry.brows = [[0.7, 0.5, 0.9, 1.0], [-0.15, 0.0, 0.1, 1.0]];
                e.geometry.mouth[0] = 0.40;
            }
            Self::Tired => {
                e.eye_aperture = 0.45;
                e.brow_raise = -0.35;
                e.mouth_curve = -0.25;
                e.geometry.brows = [[-0.2, -0.15, 0.0, 0.85]; 2];
                e.geometry.mouth = [0.8, -0.15, -0.15, 0.25];
            }
            Self::Startled => {
                e.brow_raise = 0.7;
                e.brow_tension = 0.65;
                e.mouth_open = 0.65;
                e.pupil_focus = 1.0;
                e.geometry.lids = [[0.35, 0.35, 0.0, 0.2]; 2];
                e.geometry.brows = [[0.55, 0.2, 0.7, 1.3]; 2];
                e.geometry.mouth[0] = 0.5;
            }
            Self::Boundary => {
                e.eye_aperture = 0.75;
                e.brow_tension = 0.85;
                e.mouth_tension = 0.85;
                e.mouth_compression = 0.7;
                e.geometry.lids = [[-0.3, 0.05, 0.08, -0.1]; 2];
                e.geometry.brows = [[-0.65, 0.25, 0.0, 1.4]; 2];
                e.geometry.mouth = [1.4, 0.0, 0.0, 0.85];
            }
        }
        e
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_expression_loads_neutral_geometry_and_unknown_geometry_is_rejected() {
        let mut value = serde_json::to_value(ExpressionState::default()).unwrap();
        value.as_object_mut().unwrap().remove("geometry");
        value.as_object_mut().unwrap().remove("face_pose");
        let old: ExpressionState = serde_json::from_value(value).unwrap();
        assert_eq!(old.geometry, FaceGeometry::default());
        assert_eq!(old.face_pose, FacePose::Awake);
        assert!(serde_json::from_str::<FaceGeometry>(r#"{"fake_lid": 1}"#).is_err());
    }

    #[test]
    fn geometry_sanitization_contains_nonfinite_and_out_of_range_inputs() {
        let shape = FaceGeometry {
            lids: [[f32::NAN, 8.0, -1.0, f32::INFINITY]; 2],
            brows: [[-8.0, 8.0, 3.0, 0.0]; 2],
            mouth: [f32::NAN, -9.0, 9.0, 9.0],
        }
        .sanitized();
        assert_eq!(shape.lids[0], [0.0, 1.0, 0.0, 0.0]);
        assert_eq!(shape.brows[0], [-1.0, 1.0, 1.0, 0.5]);
        assert_eq!(shape.mouth, [1.0, -1.0, 1.0, 1.0]);
    }
}
