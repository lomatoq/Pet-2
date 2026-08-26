use glam::Vec2;

pub const VISUAL_GRID_WIDTH: usize = 16;
pub const VISUAL_GRID_HEIGHT: usize = 9;
pub const VISUAL_GRID_CELLS: usize = VISUAL_GRID_WIDTH * VISUAL_GRID_HEIGHT;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SpatialVisualCell {
    pub luminance: f32,
    pub contrast: f32,
    pub colorfulness: f32,
    pub warmth: f32,
    pub hue: f32,
    pub motion: f32,
    pub edge_density: f32,
    pub sudden_change: f32,
}

impl SpatialVisualCell {
    #[must_use]
    pub fn bounded(mut self) -> Self {
        for value in [
            &mut self.luminance,
            &mut self.contrast,
            &mut self.colorfulness,
            &mut self.warmth,
            &mut self.hue,
            &mut self.motion,
            &mut self.edge_density,
            &mut self.sudden_change,
        ] {
            *value = if value.is_finite() {
                value.clamp(0.0, 1.0)
            } else {
                0.0
            };
        }
        self
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SpatialVisualFrame {
    pub cells: [SpatialVisualCell; VISUAL_GRID_CELLS],
    pub sequence: u64,
    pub timestamp: f64,
}

impl Default for SpatialVisualFrame {
    fn default() -> Self {
        Self {
            cells: [SpatialVisualCell::default(); VISUAL_GRID_CELLS],
            sequence: 0,
            timestamp: 0.0,
        }
    }
}

impl SpatialVisualFrame {
    #[must_use]
    pub fn bounded(mut self) -> Self {
        for cell in &mut self.cells {
            *cell = cell.bounded();
        }
        self.timestamp = if self.timestamp.is_finite() {
            self.timestamp.max(0.0)
        } else {
            0.0
        };
        self
    }

    #[must_use]
    pub fn cell_at(&self, position: Vec2) -> SpatialVisualCell {
        self.cells[cell_index(position)]
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum VisualRegionKind {
    #[default]
    Quiet,
    Motion,
    SuddenChange,
    Bright,
    Dark,
    UnusualColor,
    SharedCue,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct VisualAttentionTarget {
    pub position: Vec2,
    pub score: f32,
    pub kind: VisualRegionKind,
    pub explicit: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SharedAttentionCue {
    pub position: Vec2,
    pub remaining_seconds: f32,
    pub strength: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SpatialAttentionRuntime {
    habituation: [f32; VISUAL_GRID_CELLS],
    target: Option<VisualAttentionTarget>,
    hold_remaining: f32,
    shared_cue: Option<SharedAttentionCue>,
}

impl Default for SpatialAttentionRuntime {
    fn default() -> Self {
        Self {
            habituation: [0.0; VISUAL_GRID_CELLS],
            target: None,
            hold_remaining: 0.0,
            shared_cue: None,
        }
    }
}

impl SpatialAttentionRuntime {
    pub fn cue(&mut self, position: Vec2, duration_seconds: f32) {
        if position.is_finite() {
            self.shared_cue = Some(SharedAttentionCue {
                position: cell_center(cell_index(position)),
                remaining_seconds: duration_seconds.clamp(2.0, 4.0),
                strength: 1.0,
            });
        }
    }

    #[must_use]
    pub const fn target(&self) -> Option<VisualAttentionTarget> {
        self.target
    }

    pub fn update(&mut self, frame: Option<&SpatialVisualFrame>, dt: f32) {
        let dt = if dt.is_finite() {
            dt.clamp(0.0, 0.25)
        } else {
            0.0
        };
        self.hold_remaining = (self.hold_remaining - dt).max(0.0);
        if let Some(cue) = &mut self.shared_cue {
            cue.remaining_seconds = (cue.remaining_seconds - dt).max(0.0);
            cue.strength = (cue.remaining_seconds / 2.0).clamp(0.0, 1.0);
            if cue.remaining_seconds > 0.0 {
                self.target = Some(VisualAttentionTarget {
                    position: cue.position,
                    score: 1.0,
                    kind: VisualRegionKind::SharedCue,
                    explicit: true,
                });
                self.hold_remaining = self.hold_remaining.max(0.35);
                return;
            }
            self.shared_cue = None;
        }
        let Some(frame) = frame else {
            self.target = None;
            return;
        };
        let mut best = None;
        for (index, cell) in frame.cells.iter().copied().enumerate() {
            let active_motion = cell.motion.max(cell.sudden_change);
            self.habituation[index] = if active_motion > 0.12 {
                (self.habituation[index] + dt * active_motion * 0.32).clamp(0.0, 0.92)
            } else {
                (self.habituation[index] - dt * 0.08).max(0.0)
            };
            let motion_score = cell.motion * (1.0 - self.habituation[index]);
            let luminance_score = ((cell.luminance - 0.5).abs() * 2.0 - 0.35).max(0.0);
            let color_score = (cell.colorfulness - 0.42).max(0.0);
            let score = (cell.sudden_change * 0.46
                + motion_score * 0.28
                + luminance_score * 0.14
                + color_score * 0.12)
                .clamp(0.0, 1.0);
            let kind = if cell.sudden_change >= motion_score.max(luminance_score) {
                VisualRegionKind::SuddenChange
            } else if motion_score >= luminance_score.max(color_score) {
                VisualRegionKind::Motion
            } else if luminance_score >= color_score {
                if cell.luminance >= 0.5 {
                    VisualRegionKind::Bright
                } else {
                    VisualRegionKind::Dark
                }
            } else {
                VisualRegionKind::UnusualColor
            };
            let candidate = VisualAttentionTarget {
                position: cell_center(index),
                score,
                kind,
                explicit: false,
            };
            if best.is_none_or(|current: VisualAttentionTarget| candidate.score > current.score) {
                best = Some(candidate);
            }
        }
        if self.hold_remaining > 0.0
            && self.target.is_some_and(|current| {
                best.is_none_or(|candidate| candidate.score < current.score + 0.16)
            })
        {
            return;
        }
        self.target = best.filter(|target| target.score >= 0.12);
        self.hold_remaining = if self.target.is_some() { 0.65 } else { 0.0 };
    }
}

#[must_use]
pub fn cell_index(position: Vec2) -> usize {
    let column = (position.x.clamp(0.0, 0.999_999) * VISUAL_GRID_WIDTH as f32) as usize;
    let row = (position.y.clamp(0.0, 0.999_999) * VISUAL_GRID_HEIGHT as f32) as usize;
    row * VISUAL_GRID_WIDTH + column
}

#[must_use]
pub fn cell_center(index: usize) -> Vec2 {
    let index = index.min(VISUAL_GRID_CELLS - 1);
    let row = index / VISUAL_GRID_WIDTH;
    let column = index % VISUAL_GRID_WIDTH;
    Vec2::new(
        (column as f32 + 0.5) / VISUAL_GRID_WIDTH as f32,
        (row as f32 + 0.5) / VISUAL_GRID_HEIGHT as f32,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_cue_targets_the_requested_cell_without_pixels() {
        let mut runtime = SpatialAttentionRuntime::default();
        runtime.cue(Vec2::new(0.73, 0.22), 3.0);
        runtime.update(None, 0.05);
        let target = runtime.target().unwrap();
        assert!(target.explicit);
        assert_eq!(target.kind, VisualRegionKind::SharedCue);
        assert_eq!(
            cell_index(target.position),
            cell_index(Vec2::new(0.73, 0.22))
        );
    }

    #[test]
    fn continuous_motion_habituates_instead_of_forcing_gaze_chatter() {
        let mut runtime = SpatialAttentionRuntime::default();
        let mut frame = SpatialVisualFrame::default();
        frame.cells[17].motion = 0.8;
        for _ in 0..120 {
            runtime.update(Some(&frame), 0.05);
        }
        assert!(runtime.habituation[17] > 0.7);
        assert!(runtime.target().is_none_or(|target| target.score < 0.3));
    }
}
