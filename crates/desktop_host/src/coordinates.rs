use glam::Vec2;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PhysicalDesktopPoint {
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RectI {
    pub minimum: PhysicalDesktopPoint,
    pub maximum: PhysicalDesktopPoint,
}

impl RectI {
    #[must_use]
    pub fn width(self) -> i32 {
        self.maximum.x.saturating_sub(self.minimum.x)
    }

    #[must_use]
    pub fn height(self) -> i32 {
        self.maximum.y.saturating_sub(self.minimum.y)
    }

    #[must_use]
    pub fn is_valid(self) -> bool {
        self.width() > 0 && self.height() > 0
    }

    #[must_use]
    pub fn contains(self, point: PhysicalDesktopPoint) -> bool {
        point.x >= self.minimum.x
            && point.y >= self.minimum.y
            && point.x < self.maximum.x
            && point.y < self.maximum.y
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MonitorId(pub String);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MonitorInfo {
    pub id: MonitorId,
    pub physical_bounds: RectI,
    pub working_area: RectI,
    pub scale_factor: f64,
    pub primary: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct DisplayTopology {
    pub monitors: Vec<MonitorInfo>,
    pub virtual_physical_bounds: RectI,
    pub revision: u64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct NormalizedDesktopPoint {
    pub x: f32,
    pub y: f32,
}

impl NormalizedDesktopPoint {
    #[must_use]
    pub fn as_vec2(self) -> Vec2 {
        Vec2::new(self.x, self.y)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PersistedPetPosition {
    pub monitor: Option<MonitorId>,
    pub normalized: NormalizedDesktopPoint,
}

impl Default for PersistedPetPosition {
    fn default() -> Self {
        Self {
            monitor: None,
            normalized: NormalizedDesktopPoint { x: 0.5, y: 0.5 },
        }
    }
}

impl DisplayTopology {
    #[must_use]
    pub fn new(monitors: Vec<MonitorInfo>, revision: u64) -> Self {
        let virtual_physical_bounds = monitors.iter().fold(RectI::default(), |bounds, monitor| {
            if !bounds.is_valid() {
                monitor.physical_bounds
            } else {
                RectI {
                    minimum: PhysicalDesktopPoint {
                        x: bounds.minimum.x.min(monitor.physical_bounds.minimum.x),
                        y: bounds.minimum.y.min(monitor.physical_bounds.minimum.y),
                    },
                    maximum: PhysicalDesktopPoint {
                        x: bounds.maximum.x.max(monitor.physical_bounds.maximum.x),
                        y: bounds.maximum.y.max(monitor.physical_bounds.maximum.y),
                    },
                }
            }
        });
        Self {
            monitors,
            virtual_physical_bounds,
            revision,
        }
    }

    #[must_use]
    pub fn primary(&self) -> Option<&MonitorInfo> {
        self.monitors
            .iter()
            .find(|monitor| monitor.primary)
            .or_else(|| self.monitors.first())
    }

    #[must_use]
    pub fn monitor_at(&self, point: PhysicalDesktopPoint) -> Option<&MonitorInfo> {
        self.monitors
            .iter()
            .find(|monitor| monitor.physical_bounds.contains(point))
            .or_else(|| self.primary())
    }

    #[must_use]
    pub fn normalize(&self, point: PhysicalDesktopPoint) -> PersistedPetPosition {
        let Some(monitor) = self.monitor_at(point) else {
            return PersistedPetPosition::default();
        };
        let bounds = monitor.working_area;
        PersistedPetPosition {
            monitor: Some(monitor.id.clone()),
            normalized: NormalizedDesktopPoint {
                x: ((point.x - bounds.minimum.x) as f32 / bounds.width().max(1) as f32)
                    .clamp(0.0, 1.0),
                y: ((point.y - bounds.minimum.y) as f32 / bounds.height().max(1) as f32)
                    .clamp(0.0, 1.0),
            },
        }
    }

    #[must_use]
    pub fn remap(&self, persisted: &PersistedPetPosition) -> PhysicalDesktopPoint {
        let monitor = persisted
            .monitor
            .as_ref()
            .and_then(|id| self.monitors.iter().find(|monitor| &monitor.id == id))
            .or_else(|| self.primary());
        let Some(monitor) = monitor else {
            return PhysicalDesktopPoint::default();
        };
        let bounds = monitor.working_area;
        let x = persisted.normalized.x.clamp(0.0, 1.0);
        let y = persisted.normalized.y.clamp(0.0, 1.0);
        PhysicalDesktopPoint {
            x: bounds.minimum.x + (x * bounds.width() as f32).round() as i32,
            y: bounds.minimum.y + (y * bounds.height() as f32).round() as i32,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn topology() -> DisplayTopology {
        DisplayTopology::new(
            vec![
                MonitorInfo {
                    id: MonitorId("left-150".into()),
                    physical_bounds: RectI {
                        minimum: PhysicalDesktopPoint { x: -2560, y: -200 },
                        maximum: PhysicalDesktopPoint { x: 0, y: 1240 },
                    },
                    working_area: RectI {
                        minimum: PhysicalDesktopPoint { x: -2560, y: -200 },
                        maximum: PhysicalDesktopPoint { x: 0, y: 1200 },
                    },
                    scale_factor: 1.5,
                    primary: false,
                },
                MonitorInfo {
                    id: MonitorId("primary-100".into()),
                    physical_bounds: RectI {
                        minimum: PhysicalDesktopPoint { x: 0, y: 0 },
                        maximum: PhysicalDesktopPoint { x: 1920, y: 1080 },
                    },
                    working_area: RectI {
                        minimum: PhysicalDesktopPoint { x: 0, y: 0 },
                        maximum: PhysicalDesktopPoint { x: 1920, y: 1040 },
                    },
                    scale_factor: 1.0,
                    primary: true,
                },
            ],
            7,
        )
    }

    #[test]
    fn roundtrip_handles_negative_coordinates_and_mixed_scaling() {
        let topology = topology();
        let point = PhysicalDesktopPoint { x: -1280, y: 500 };
        let persisted = topology.normalize(point);
        let remapped = topology.remap(&persisted);
        assert_eq!(persisted.monitor, Some(MonitorId("left-150".into())));
        assert!((remapped.x - point.x).abs() <= 1);
        assert!((remapped.y - point.y).abs() <= 1);
    }

    #[test]
    fn missing_monitor_falls_back_to_primary_safe_area() {
        let point = topology().remap(&PersistedPetPosition {
            monitor: Some(MonitorId("disconnected".into())),
            normalized: NormalizedDesktopPoint { x: 2.0, y: -2.0 },
        });
        assert_eq!(point, PhysicalDesktopPoint { x: 1920, y: 0 });
    }

    #[test]
    fn normalization_is_stable_at_required_scale_factors() {
        for (index, scale_factor) in [1.0, 1.25, 1.5, 2.0].into_iter().enumerate() {
            let minimum = PhysicalDesktopPoint {
                x: -3_000 + index as i32 * 2_000,
                y: -400 + index as i32 * 100,
            };
            let bounds = RectI {
                minimum,
                maximum: PhysicalDesktopPoint {
                    x: minimum.x + 1_600,
                    y: minimum.y + 900,
                },
            };
            let topology = DisplayTopology::new(
                vec![MonitorInfo {
                    id: MonitorId(format!("scale-{scale_factor}")),
                    physical_bounds: bounds,
                    working_area: bounds,
                    scale_factor,
                    primary: true,
                }],
                index as u64,
            );
            let point = PhysicalDesktopPoint {
                x: minimum.x + 533,
                y: minimum.y + 677,
            };
            let remapped = topology.remap(&topology.normalize(point));
            assert!((remapped.x - point.x).abs() <= 1);
            assert!((remapped.y - point.y).abs() <= 1);
        }
    }

    #[test]
    fn moving_between_mixed_scale_monitors_preserves_each_local_position() {
        let topology = topology();
        let left = PhysicalDesktopPoint { x: -420, y: 640 };
        let primary = PhysicalDesktopPoint { x: 420, y: 640 };
        let left_saved = topology.normalize(left);
        let primary_saved = topology.normalize(primary);
        assert_eq!(left_saved.monitor, Some(MonitorId("left-150".into())));
        assert_eq!(primary_saved.monitor, Some(MonitorId("primary-100".into())));
        assert_eq!(topology.remap(&left_saved), left);
        assert_eq!(topology.remap(&primary_saved), primary);
    }
}
