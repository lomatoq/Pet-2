use desktop_host::{DisplayTopology, MonitorId, MonitorInfo, PhysicalDesktopPoint, RectI};
use glam::Vec2;
use pet_ecology::EcologyState;

fn physical_from_virtual(topology: &DisplayTopology, position: Vec2) -> PhysicalDesktopPoint {
    let bounds = topology.virtual_physical_bounds;
    PhysicalDesktopPoint {
        x: bounds.minimum.x + (position.x * bounds.width() as f32).round() as i32,
        y: bounds.minimum.y + (position.y * bounds.height() as f32).round() as i32,
    }
}

#[test]
fn monitor_unplug_and_dpi_change_preserve_orb_and_den_normalized_places() {
    let old = DisplayTopology::new(
        vec![
            MonitorInfo {
                id: MonitorId("left".into()),
                physical_bounds: RectI {
                    minimum: PhysicalDesktopPoint { x: -2_560, y: 0 },
                    maximum: PhysicalDesktopPoint { x: 0, y: 1_440 },
                },
                working_area: RectI {
                    minimum: PhysicalDesktopPoint { x: -2_560, y: 0 },
                    maximum: PhysicalDesktopPoint { x: 0, y: 1_400 },
                },
                scale_factor: 1.5,
                primary: false,
            },
            MonitorInfo {
                id: MonitorId("primary".into()),
                physical_bounds: RectI {
                    minimum: PhysicalDesktopPoint { x: 0, y: 0 },
                    maximum: PhysicalDesktopPoint { x: 1_920, y: 1_080 },
                },
                working_area: RectI {
                    minimum: PhysicalDesktopPoint { x: 0, y: 0 },
                    maximum: PhysicalDesktopPoint { x: 1_920, y: 1_040 },
                },
                scale_factor: 1.0,
                primary: true,
            },
        ],
        1,
    );
    let new = DisplayTopology::new(
        vec![MonitorInfo {
            id: MonitorId("replacement".into()),
            physical_bounds: RectI {
                minimum: PhysicalDesktopPoint { x: 120, y: 80 },
                maximum: PhysicalDesktopPoint { x: 2_680, y: 1_520 },
            },
            working_area: RectI {
                minimum: PhysicalDesktopPoint { x: 120, y: 80 },
                maximum: PhysicalDesktopPoint { x: 2_680, y: 1_480 },
            },
            scale_factor: 2.0,
            primary: true,
        }],
        2,
    );
    let mut state = EcologyState::new(8_181);
    state.objects[0].position = Vec2::new(0.72, 0.42);
    let before_orb = state.objects[0].position;
    let before_den = state.den.anchor;
    assert!(
        old.virtual_physical_bounds
            .contains(physical_from_virtual(&old, before_orb))
    );

    let restored = EcologyState::restore(
        serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap(),
    )
    .unwrap();
    assert_eq!(restored.objects[0].position, before_orb);
    assert_eq!(restored.den.anchor, before_den);
    assert!(
        new.virtual_physical_bounds
            .contains(physical_from_virtual(&new, before_orb))
    );
    assert!(
        new.virtual_physical_bounds
            .contains(physical_from_virtual(&new, before_den))
    );
}
