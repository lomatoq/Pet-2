//! Coupling digestion to measured body contact and the desktop work area.
use super::*;

pub fn update(runtime: &mut PetRuntime, dt: f32) {
    if runtime.birth.started.is_some() {
        return;
    }
    let feedback = runtime.body.simulation.feedback.clone();
    let bounds = runtime.topology.virtual_physical_bounds;
    let extent = Vec2::new(bounds.width().max(1) as f32, bounds.height().max(1) as f32);
    let aspect = extent.x / extent.y;
    let p = virtual_normalized_to_physical(&runtime.topology, feedback.world_position);
    let monitor = runtime.topology.monitors.iter().find(|m| {
        p.x >= m.physical_bounds.minimum.x as f32 && p.x <= m.physical_bounds.maximum.x as f32
    });
    let floor = monitor.map_or(1.0, |m| {
        (m.working_area.maximum.y - bounds.minimum.y) as f32 / extent.y
    });
    let left = monitor.map_or(0.0, |m| {
        (m.working_area.minimum.x - bounds.minimum.x) as f32 / extent.x
    });
    let right = monitor.map_or(1.0, |m| {
        (m.working_area.maximum.x - bounds.minimum.x) as f32 / extent.x
    });
    let bottom = runtime
        .body
        .liquid_physical_support_pixels(Vec2::Y, extent.y);
    let mouth = runtime.body.feeding_mouth_tip_pixels(extent.y);
    let state = runtime.ecology.state();
    let active = state.waste.active();
    let wants = state.metabolism.tract.needs_to_go() || active;
    let allowed = !runtime.sensors.pet_dragged
        && runtime.ecology.feeding_navigation().is_none()
        && runtime.life.state.affect.stress < 0.75;
    if wants && allowed && runtime.digestion_site.is_none() {
        // Nearby free ground, clear of the bed and previous traces. Identity
        // breaks equal-distance ties without a new random scripted animation.
        let preference = if state.identity_seed.is_multiple_of(2) {
            1.0
        } else {
            -1.0
        };
        let den = state.den.anchor;
        let x = [0.045, -0.045, 0.09, -0.09, 0.16, -0.16]
            .into_iter()
            .map(|offset| {
                (feedback.world_position.x + offset * preference / aspect)
                    .clamp(left + 0.08 / aspect, right - 0.08 / aspect)
            })
            .min_by(|a, b| {
                let cost = |x: f32| {
                    let occupied = state
                        .waste
                        .chains
                        .iter()
                        .filter(|c| {
                            c.nodes
                                .iter()
                                .any(|n| (n.position.x - x).abs() * aspect < 0.07)
                        })
                        .count() as f32;
                    let nest = f32::from((x - den.x).abs() * aspect < 0.2) * 2.0;
                    (x - feedback.world_position.x).abs() * aspect + occupied + nest
                };
                cost(*a).total_cmp(&cost(*b))
            })
            .unwrap_or(feedback.world_position.x);
        runtime.digestion_site = Some(Vec2::new(x, floor));
    }
    if !wants {
        runtime.digestion_site = None;
    }
    let mut settled = false;
    let tolerance = if active { 2.0 } else { 1.0 };
    if let Some(site) = runtime.digestion_site
        && allowed
    {
        runtime.voice_motion = None;
        let target = Vec2::new(site.x, (site.y - bottom.y / extent.y).clamp(0.08, 0.98));
        runtime.intent.target_position = target;
        runtime.intent.target_surface = None;
        runtime.intent.locomotion = LocomotionMode::Arrive;
        runtime.intent.desired_speed = 0.22;
        settled = (feedback.world_position.x - site.x).abs() * aspect < 0.025 * tolerance
            && (feedback.world_position.y + bottom.y / extent.y - site.y).abs() < 0.014 * tolerance
            && (feedback.velocity * Vec2::new(aspect, 1.0)).length() < 0.08 * tolerance;
        let mut packet = runtime.last_motor_packet.clone();
        packet.fields.fill(None);
        packet.support = (!runtime.cradle_seat.inside
            && (feedback.world_position.y + bottom.y / extent.y - site.y).abs() < 0.07)
            .then_some(pet_motor::SurfaceAttachmentCommand {
                surface_id: lifecore::SurfaceId("digestion:floor".into()),
                anchor_point: Vec2::new(site.x, site.y),
                normal: -Vec2::Y,
                tangent: Vec2::X,
                target_contact_fraction: 0.2,
                normal_compliance: 0.25,
                tangent_friction: 0.65,
                adhesion: 0.0,
                load_fraction: 0.22,
                break_force: 0.7,
                release_half_life: 0.18,
            });
        if settled {
            runtime.intent.desired_speed = 0.0;
            runtime.intent.pose = PoseIntent::Compact;
            let effort = runtime.ecology.state().metabolism.tract.strain;
            packet.fields[0] = Some(pet_motor::LocalSomaticField {
                kind: pet_motor::SomaticFieldKind::Gather,
                space: pet_motor::FieldSpace::BodyLocal,
                center: Vec2::new(0.0, 0.12),
                axis: Vec2::Y,
                radius: 0.5,
                strength: effort * 0.10,
                falloff: 2.0,
                frequency_hz: 0.0,
                phase_01: 0.0,
                target_component: None,
            });
        }
        runtime.body.set_somatic_actuation(packet);
    }
    let contour = runtime.body.main_liquid_contact_bounds_pixels(extent.y);
    let outlet = lateral_outlet(feedback.world_position, contour.minimum, contour.maximum,
        extent, left, right, floor, runtime.ecology.state().identity_seed);
    let frame = pet_ecology::DigestionFrame {
        body_bounds: Some([feedback.world_position+contour.minimum/extent,
            feedback.world_position+contour.maximum/extent]),
        outlet,
        mouth: feedback.world_position + mouth / extent,
        gas_outlet: feedback.world_position + runtime.body.liquid_physical_support_pixels(Vec2::new(1.0,0.45).normalize(),extent.y) / extent,
        body_velocity: feedback.velocity,
        floor,
        settled: settled && allowed,
        aspect,
        cursor: runtime
            .cleanup_mode
            .then_some(runtime.sensors.cursor_position),
    };
    runtime.ecology.step_digestion(frame, dt);
    let gut = &runtime.ecology.state().metabolism.tract;
    if gut.strain > 0.03 && allowed {
        let face = &mut runtime.intent.expression;
        face.eye_aperture = (1.0 - gut.strain * 0.65).max(0.25);
        face.brow_tension = gut.strain * 0.55;
        face.brow_raise = -gut.strain * 0.3;
        face.mouth_compression = gut.strain * 0.8;
        face.mouth_open = 0.03 + gut.strain * 0.12;
        face.mouth_curve = -gut.strain * 0.18;
        face.effort = gut.strain;
        face.cheek_glow = face.cheek_glow.max(gut.strain * 0.4);
    } else if gut.burp > 0.05 && allowed {
        runtime.intent.expression.mouth_open = 0.15 + gut.burp * 0.45;
        runtime.intent.expression.mouth_compression = 0.0;
        runtime.intent.expression.eye_aperture = 1.0 + gut.burp * 0.2;
    }
}

// The chain is drawn in the world overlay. Place its first joint outside the
// measured silhouette, so it emerges beside the lower flank rather than being
// painted across the belly. Clearance includes the largest emitted radius.
#[allow(clippy::too_many_arguments)]
fn lateral_outlet(center: Vec2, minimum: Vec2, maximum: Vec2, extent: Vec2,
    left: f32, right: f32, floor: f32, seed: u64) -> Vec2 {
    let clearance = 7.0 * extent.y / 1152.0;
    let low = center.x + (minimum.x - clearance) / extent.x;
    let high = center.x + (maximum.x + clearance) / extent.x;
    let room_left = (low - left) * extent.x;
    let room_right = (right - high) * extent.x;
    let prefer_right = seed.is_multiple_of(2);
    let right_side = if room_left < clearance { true }
        else if room_right < clearance { false } else { prefer_right };
    Vec2::new(if right_side { high } else { low },
        (center.y + (maximum.y - clearance * 2.0) / extent.y).min(floor - clearance / extent.y))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn waste_exits_lower_flank_outside_body_at_all_desktop_aspects() {
        for aspect in [0.75, 1.0, 2.4, 3.5] {
            let extent=Vec2::new(1152.0*aspect,1152.0);
            let lo=Vec2::new(-90.0,-65.0);let hi=Vec2::new(80.0,48.0);
            for seed in [1,2] {
                let p=Vec2::new(0.5,0.9);
                let outlet=lateral_outlet(p,lo,hi,extent,0.0,1.0,0.95,seed);
                let local=(outlet-p)*extent;
                assert!(local.x<lo.x-6.0 || local.x>hi.x+6.0);
                assert!(local.y>0.0 && outlet.y<0.95);
            }
            let p=Vec2::new(1.0-hi.x/extent.x,0.9);
            assert!(lateral_outlet(p,lo,hi,extent,0.0,1.0,0.95,2).x<p.x);
        }
    }
}
