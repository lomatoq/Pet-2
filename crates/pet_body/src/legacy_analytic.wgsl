// Legacy analytic SDF renderer retained as an explicit rollback path.
struct Globals {
    viewport_time: vec4<f32>,
    body_shape: vec4<f32>,
    face_shape: vec4<f32>,
    appendages: vec4<f32>,
    primary_hsv: vec4<f32>,
    secondary_hsv: vec4<f32>,
    glow_hsv: vec4<f32>,
    iris_hsv: vec4<f32>,
    gaze_pupil: vec4<f32>,
    lids_brows: vec4<f32>,
    brow_mouth: vec4<f32>,
    mouth_voice: vec4<f32>,
    motion_a: vec4<f32>,
    motion_b: vec4<f32>,
    pattern: vec4<f32>,
    occlusion: vec4<f32>,
    physiology_a: vec4<f32>,
    physiology_b: vec4<f32>,
    flow: vec4<f32>,
    eye_detail: vec4<f32>,
    visual_detail: vec4<f32>,
    droplet_meta: vec4<f32>,
    morph_a: vec4<f32>,
    morph_b: vec4<f32>,
    droplet_position_radius: array<vec4<f32>, 8>,
    droplet_motion_shape: array<vec4<f32>, 8>,
    droplet_bridge: array<vec4<f32>, 8>,
    render_mode: vec4<f32>,
    liquid_meta: vec4<f32>,
    face_frame_a: vec4<f32>,
    face_frame_b: vec4<f32>,
    liquid_motion: vec4<f32>,
    material_a: vec4<f32>,
    material_b: vec4<f32>,
    material_c: vec4<f32>,
    material_d: vec4<f32>,
    material_e: vec4<f32>,
    material_f: vec4<f32>,
    face_tuning: vec4<f32>,
    desktop_capture: vec4<f32>,
    desktop_capture_meta: vec4<f32>,
};

@group(0) @binding(0) var<uniform> globals: Globals;
@group(0) @binding(1) var desktop_background: texture_2d<f32>;
@group(0) @binding(2) var desktop_sampler: sampler;

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vertex_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    let positions = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0),
    );
    let uvs = array<vec2<f32>, 3>(
        vec2<f32>(0.0, 1.0),
        vec2<f32>(2.0, 1.0),
        vec2<f32>(0.0, -1.0),
    );
    var output: VertexOutput;
    output.clip_position = vec4<f32>(positions[vertex_index], 0.0, 1.0);
    output.uv = uvs[vertex_index];
    return output;
}

fn saturate(value: f32) -> f32 {
    return clamp(value, 0.0, 1.0);
}

fn rotate2(point: vec2<f32>, angle: f32) -> vec2<f32> {
    let cosine = cos(angle);
    let sine = sin(angle);
    return vec2<f32>(
        point.x * cosine - point.y * sine,
        point.x * sine + point.y * cosine,
    );
}

fn hsv_to_rgb(hsv: vec3<f32>) -> vec3<f32> {
    let k = vec4<f32>(1.0, 2.0 / 3.0, 1.0 / 3.0, 3.0);
    let p = abs(fract(hsv.xxx + k.xyz) * 6.0 - k.www);
    return hsv.z * mix(k.xxx, clamp(p - k.xxx, vec3<f32>(0.0), vec3<f32>(1.0)), hsv.y);
}

fn sd_ellipse(point: vec2<f32>, radii: vec2<f32>) -> f32 {
    let safe_radii = max(radii, vec2<f32>(0.002));
    return (length(point / safe_radii) - 1.0) * min(safe_radii.x, safe_radii.y);
}

fn sd_liquid_bloblet(
    point: vec2<f32>,
    center: vec2<f32>,
    radius: f32,
    stretch: f32,
    rotation: f32,
    phase: f32,
) -> f32 {
    let safe_radius = max(radius, 0.002);
    let local = rotate2(point - center, -rotation);
    let stretch_amount = saturate(stretch);
    let area_compensation = inverseSqrt(1.0 + stretch_amount * 0.55);
    let core_radius = safe_radius * area_compensation;
    let core = length(local) - core_radius;

    // Two unequal satellite kernels make a pear/shoulder silhouette rather than
    // exposing the simulation parcel as a rotated ellipse. The kernel offsets are
    // persistent in the parcel frame, so slow phase motion does not change topology.
    let tip_center = vec2<f32>(
        safe_radius * (0.18 + stretch_amount * 0.62),
        sin(phase * 0.73) * safe_radius * 0.10,
    );
    let tip_radius = core_radius * mix(0.72, 0.54, stretch_amount);
    let tip = length(local - tip_center) - tip_radius;
    let shoulder_center = vec2<f32>(
        -safe_radius * 0.12,
        cos(phase * 1.17) * safe_radius * 0.24,
    );
    let shoulder = length(local - shoulder_center) - core_radius * 0.55;
    let pear = smooth_union(core, tip, safe_radius * 0.38);
    return smooth_union(pear, shoulder, safe_radius * 0.24);
}

fn sd_capsule(point: vec2<f32>, start: vec2<f32>, end: vec2<f32>, radius: f32) -> f32 {
    let segment = end - start;
    let denominator = max(dot(segment, segment), 0.000001);
    let projection = clamp(dot(point - start, segment) / denominator, 0.0, 1.0);
    return length(point - (start + segment * projection)) - radius;
}

fn sd_tapered_segment(
    point: vec2<f32>,
    start: vec2<f32>,
    end: vec2<f32>,
    start_radius: f32,
    end_radius: f32,
) -> f32 {
    let segment = end - start;
    let denominator = max(dot(segment, segment), 0.000001);
    let projection = clamp(dot(point - start, segment) / denominator, 0.0, 1.0);
    let radius = mix(start_radius, end_radius, projection);
    return length(point - mix(start, end, projection)) - radius;
}

fn smooth_union(a: f32, b: f32, smoothing: f32) -> f32 {
    let k = max(smoothing, 0.0001);
    let h = saturate(0.5 + 0.5 * (b - a) / k);
    return mix(b, a, h) - k * h * (1.0 - h);
}

fn body_space(point: vec2<f32>) -> vec2<f32> {
    let tilted = rotate2(point, -globals.motion_a.z);
    return tilted / max(globals.motion_a.xy, vec2<f32>(0.35));
}

fn head_center() -> vec2<f32> {
    let length_value = globals.body_shape.y;
    let breath = (globals.motion_b.w - 0.5) * 0.018;
    return vec2<f32>(globals.motion_a.w * 0.65, 0.135 + length_value * 0.055 + globals.motion_b.x + breath);
}

fn body_field(point: vec2<f32>) -> f32 {
    let p = body_space(point);
    let width = globals.body_shape.x;
    let length_value = globals.body_shape.y;
    let roundness = globals.body_shape.z;
    let softness = globals.face_shape.w;
    let compression = globals.pattern.w;
    let metabolic_pulse = globals.physiology_a.x;
    let breath = (globals.motion_b.w - 0.5) * (0.006 + softness * 0.010);
    let center = vec2<f32>(0.0, -0.015 - compression * 0.025);
    let base_radii = vec2<f32>(
        0.285 + width * 0.105 + roundness * 0.018 + breath,
        0.355 + length_value * 0.090 - compression * 0.035 + breath * 0.72,
    );
    let radii = base_radii * vec2<f32>(
        1.0 - metabolic_pulse * 0.42,
        1.0 + metabolic_pulse,
    );
    let local = p - center;

    // CPU-integrated Rayleigh/Lamb-like modes retain acceleration and collision
    // history. There is no mode 1, so deformation cannot translate the pet.
    let normalized_local = local / max(radii, vec2<f32>(0.002));
    let polar_angle = atan2(normalized_local.y, normalized_local.x);
    let basis2 = vec2<f32>(cos(polar_angle * 2.0), sin(polar_angle * 2.0));
    let basis3 = vec2<f32>(cos(polar_angle * 3.0), sin(polar_angle * 3.0));
    let basis4 = vec2<f32>(cos(polar_angle * 4.0), sin(polar_angle * 4.0));
    let modal_wave = dot(globals.morph_a.xy, basis2)
        + dot(globals.morph_a.zw, basis3)
        + dot(globals.morph_b.xy, basis4);
    let modal_radius = clamp(globals.morph_b.z * (1.0 + modal_wave), 0.78, 1.22);
    let modal_local = local / modal_radius;
    let taper = 1.0 + modal_local.y * (0.10 + roundness * 0.06);
    let morphed = vec2<f32>(
        modal_local.x * taper,
        modal_local.y,
    );
    return sd_ellipse(morphed, radii);
}

fn droplet_fields(point: vec2<f32>, main_field: f32) -> vec2<f32> {
    var droplets = 1000.0;
    var liquid = main_field;
    for (var index: u32 = 0u; index < 8u; index += 1u) {
        let packed = globals.droplet_position_radius[index];
        let motion = globals.droplet_motion_shape[index];
        let bridge = globals.droplet_bridge[index];
        if (motion.w <= 0.001) {
            continue;
        }
        let visible_radius = packed.z * sqrt(saturate(motion.w));
        let distance = sd_liquid_bloblet(
            point,
            packed.xy,
            visible_radius,
            motion.x,
            motion.y,
            motion.z,
        );
        droplets = min(droplets, distance);

        // Adhesion and a filament are different topological signals. During budding
        // and coalescence a short bowed, tapered neck joins a parcel to the membrane;
        // detached parcels remain independent droplets instead of becoming one tail.
        let filament_strength = bridge.w;
        let anchor = bridge.xy;
        let span = packed.xy - anchor;
        let span_length = length(span);
        if (filament_strength > 0.08 && span_length > visible_radius * 0.32) {
            let span_normal = normalize(vec2<f32>(-span.y, span.x) + vec2<f32>(0.000001));
            let bend = mix(anchor, packed.xy, 0.52)
                + span_normal
                    * sin(motion.z * 1.31)
                    * min(span_length, 0.16)
                    * (0.07 + motion.x * 0.06);
            let root_radius = max(bridge.z, 0.001) * filament_strength;
            let middle_radius = root_radius * 0.68;
            let tip_radius = min(root_radius * 1.18, visible_radius * 0.31);
            let first_half = sd_tapered_segment(
                point,
                anchor,
                bend,
                root_radius,
                middle_radius,
            );
            let second_half = sd_tapered_segment(
                point,
                bend,
                packed.xy,
                middle_radius,
                tip_radius,
            );
            let filament = smooth_union(
                first_half,
                second_half,
                visible_radius * 0.10 * filament_strength,
            );
            liquid = smooth_union(
                liquid,
                filament,
                visible_radius * 0.13 * filament_strength,
            );
        }
        if (packed.w > 0.001) {
            // Adhesion is a separate, continuous state. Detached parcels are not
            // globally metaballed into the body, preventing permanent body lumps and
            // cohesion strands while budding/coalescing still grows a soft neck.
            liquid = smooth_union(liquid, distance, visible_radius * 0.62 * packed.w);
        } else {
            liquid = min(liquid, distance);
        }
    }
    return vec2<f32>(droplets, liquid);
}

fn body_normal(field: f32) -> vec3<f32> {
    let gradient = vec2<f32>(dpdx(field), dpdy(field));
    let planar = normalize(gradient + vec2<f32>(0.000001));
    let thickness = smoothstep(0.0, 0.24, -field);
    let edge_slope = mix(0.96, 0.14, thickness);
    let depth = mix(0.30, 1.0, thickness);
    return normalize(vec3<f32>(planar * edge_slope, depth));
}

fn interior_depth(field: f32) -> f32 {
    return saturate(-field / 0.22);
}

fn living_flow(point: vec2<f32>) -> f32 {
    var p = point * globals.visual_detail.z * 0.72;
    let phase = globals.flow.z;
    let warp = vec2<f32>(
        sin(p.y * 1.7 + phase * 1.1),
        cos(p.x * 1.4 - phase * 0.9),
    ) * globals.flow.w;
    p += warp;
    let layer_a = sin(dot(p, vec2<f32>(1.15, 0.73)) * 3.1 + phase * 5.0);
    let layer_b = sin(dot(p, vec2<f32>(-0.62, 1.32)) * 2.4 - phase * 3.2);
    let layer_c = cos(length(p * vec2<f32>(0.82, 1.18)) * 4.2 + phase * 1.7);
    return layer_a * 0.50 + layer_b * 0.31 + layer_c * 0.19;
}

fn eye_center(side: f32) -> vec2<f32> {
    let head_ratio = globals.body_shape.w;
    let spacing = 0.070 + globals.face_shape.y * 0.31;
    return head_center() + vec2<f32>(side * spacing, 0.025 + head_ratio * 0.045);
}

fn eye_radii() -> vec2<f32> {
    let size = globals.face_shape.x;
    return vec2<f32>(0.065 + size * 0.34, 0.072 + size * 0.31);
}

fn eye_mask(point: vec2<f32>, center: vec2<f32>, radii: vec2<f32>) -> f32 {
    let distance = length((point - center) / radii);
    return 1.0 - smoothstep(0.96, 1.025, distance);
}

fn eye_gaze_offset(side: f32) -> vec2<f32> {
    let inward = -side * globals.gaze_pupil.z;
    return clamp(
        vec2<f32>(globals.gaze_pupil.x * 0.34 + inward, globals.gaze_pupil.y * 0.27),
        vec2<f32>(-0.34),
        vec2<f32>(0.34),
    );
}

fn lid_aperture(local_eye: vec2<f32>, blink: f32, squint: f32) -> f32 {
    let x2 = local_eye.x * local_eye.x;
    let upper_curve = 0.78 - x2 * 0.20 - blink * 1.44 - squint * 0.28;
    let lower_curve = -0.76 + x2 * 0.11 + blink * 0.70 + squint * 0.16;
    let upper_open = 1.0 - smoothstep(upper_curve - 0.055, upper_curve + 0.055, local_eye.y);
    let lower_open = smoothstep(lower_curve - 0.055, lower_curve + 0.055, local_eye.y);
    return upper_open * lower_open;
}

fn eye_socket_shadow(point: vec2<f32>, side: f32) -> f32 {
    let local = (point - eye_center(side)) / eye_radii();
    let socket_r = length(local);
    return smoothstep(0.80, 1.03, socket_r)
        * (1.0 - smoothstep(1.02, 1.14, socket_r));
}

fn eye_layer(
    point: vec2<f32>,
    side: f32,
    body_color: vec3<f32>,
    secondary: vec3<f32>,
    glow_color: vec3<f32>,
) -> vec4<f32> {
    let center = eye_center(side);
    let radii = eye_radii();
    let local = (point - center) / radii;
    let mask = eye_mask(point, center, radii);
    let blink = select(globals.lids_brows.y, globals.lids_brows.x, side < 0.0);
    let aperture = lid_aperture(local, blink, globals.lids_brows.z);
    let visible = mask * aperture;
    let lid_presence = smoothstep(0.02, 0.18, max(blink, globals.lids_brows.z * 0.48));
    let lid = mask * (1.0 - aperture) * lid_presence;

    let iris_offset = eye_gaze_offset(side);
    let iris_radius = globals.eye_detail.x;
    let iris_local = (local - iris_offset) / vec2<f32>(iris_radius, iris_radius * 1.035);
    let iris_r = length(iris_local);
    let iris_angle = atan2(iris_local.y, iris_local.x);
    let iris_mask = visible * (1.0 - smoothstep(0.94, 1.015, iris_r));
    let limbal_ring = iris_mask
        * smoothstep(0.73, 0.90, iris_r)
        * (1.0 - smoothstep(0.93, 1.0, iris_r));
    let fiber_wave = sin(
        iris_angle * globals.eye_detail.z
        + iris_r * 11.0
        + globals.pattern.z * 2.7,
    );
    let secondary_fiber = sin(
        iris_angle * (globals.eye_detail.z * 0.47)
        - iris_r * 17.0
        + globals.viewport_time.y * 0.03,
    );
    let fiber_footprint = fwidth(iris_angle) * globals.eye_detail.z;
    let fiber_prefilter = 1.0 - smoothstep(0.42, 1.20, fiber_footprint);
    let fiber = (fiber_wave * 0.68 + secondary_fiber * 0.32)
        * globals.eye_detail.w
        * fiber_prefilter;
    let collarette = iris_mask
        * smoothstep(0.32, 0.39, iris_r)
        * (1.0 - smoothstep(0.49, 0.58, iris_r));
    let pupil_ratio = mix(0.34, 0.68, globals.gaze_pupil.w)
        * mix(0.94, 1.06, globals.face_shape.z);
    let pupil_r = iris_r / max(pupil_ratio, 0.01);
    let pupil_mask = iris_mask * (1.0 - smoothstep(0.90, 1.0, pupil_r));
    let eye_r = length(local);
    let eye_rim = smoothstep(0.84, 0.91, eye_r)
        * (1.0 - smoothstep(0.96, 1.01, eye_r))
        * mask;

    let sclera_top = vec3<f32>(0.965, 0.958, 0.930);
    let sclera_bottom = vec3<f32>(0.990, 0.975, 0.940);
    let sclera = mix(sclera_bottom, sclera_top, saturate(local.y * 0.5 + 0.5));
    var iris_color = mix(secondary * 0.70, glow_color, 0.34 + globals.physiology_b.z * 0.18);
    if (globals.iris_hsv.w > 0.5) {
        let authored_iris = hsv_to_rgb(globals.iris_hsv.xyz);
        let radial_light = 0.72 + (1.0 - smoothstep(0.16, 0.92, iris_r)) * 0.34;
        iris_color = authored_iris * radial_light;
    }
    iris_color *= 1.0
        + fiber
            * smoothstep(0.22, 0.92, iris_r)
            * (0.08 + globals.physiology_b.z * 0.04);
    iris_color = mix(iris_color, iris_color * 0.42, limbal_ring * globals.eye_detail.y);
    iris_color = mix(
        iris_color,
        glow_color * 0.52 + iris_color * 0.48,
        collarette * 0.18,
    );
    var color = sclera;
    color = mix(color, iris_color, iris_mask);
    color = mix(color, vec3<f32>(0.008, 0.014, 0.024), pupil_mask);
    color = mix(color, secondary * 0.32, eye_rim * 0.58);

    let main_highlight = visible
        * (1.0 - smoothstep(0.025, 0.092, length(local - vec2<f32>(-0.23, 0.27))));
    let micro_highlight = visible
        * (1.0 - smoothstep(0.010, 0.035, length(local - vec2<f32>(0.06, 0.13))));
    color += vec3<f32>(0.98, 1.0, 1.0)
        * (main_highlight * 0.86 + micro_highlight * 0.36)
        * globals.physiology_b.w
        * globals.visual_detail.y;
    let lid_tint = mix(body_color, secondary, 0.10 + globals.lids_brows.z * 0.08);
    color = mix(color, lid_tint, lid);
    return vec4<f32>(color, max(visible, lid));
}

fn brow_mask(point: vec2<f32>, side: f32) -> f32 {
    let center = eye_center(side);
    let radii = eye_radii();
    let raise = globals.lids_brows.w * 0.046;
    let tension = globals.brow_mouth.x;
    let asymmetry = globals.brow_mouth.y * side * 0.018;
    let inner_raise = saturate(-globals.brow_mouth.w) * (1.0 - tension * 0.55);
    let inner = center + vec2<f32>(
        -side * (radii.x * 0.70 + tension * 0.010),
        radii.y * 1.06 + raise + inner_raise * 0.042 - tension * 0.044 + asymmetry,
    );
    let inner_mid = center + vec2<f32>(
        -side * radii.x * 0.34,
        radii.y * 1.09 + raise + inner_raise * 0.034 - tension * 0.015 + asymmetry * 0.55,
    );
    let crown = center + vec2<f32>(
        0.0,
        radii.y * 1.11 + raise + inner_raise * 0.017 + tension * 0.004,
    );
    let outer_mid = center + vec2<f32>(
        side * radii.x * 0.36,
        radii.y * 1.10 + raise * 0.90 - inner_raise * 0.004 + tension * 0.008 - asymmetry * 0.45,
    );
    let outer = center + vec2<f32>(
        side * radii.x * 0.70,
        radii.y * 1.06 + raise * 0.76 - inner_raise * 0.015 + tension * 0.011 - asymmetry,
    );
    let distance = min(
        min(sd_capsule(point, inner, inner_mid, 0.0), sd_capsule(point, inner_mid, crown, 0.0)),
        min(sd_capsule(point, crown, outer_mid, 0.0), sd_capsule(point, outer_mid, outer, 0.0)),
    );
    return 1.0 - smoothstep(0.008, 0.018, distance);
}

fn expressive_mouth_y(normalized_x: f32, curve: f32, tension: f32) -> f32 {
    let x = clamp(normalized_x, -1.0, 1.0);
    let center_arc = -curve * 0.052 * (1.0 - x * x);
    let corner_lift = curve * 0.011 * smoothstep(0.56, 1.0, abs(x));
    let pressed_center = tension * 0.004 * (1.0 - abs(x));
    return center_arc + corner_lift + pressed_center;
}

fn expressive_mouth_distance(local: vec2<f32>, width: f32, curve: f32, tension: f32) -> f32 {
    let p0 = vec2<f32>(-width, expressive_mouth_y(-1.0, curve, tension));
    let p1 = vec2<f32>(-width * 0.67, expressive_mouth_y(-0.67, curve, tension));
    let p2 = vec2<f32>(-width * 0.33, expressive_mouth_y(-0.33, curve, tension));
    let p3 = vec2<f32>(0.0, expressive_mouth_y(0.0, curve, tension));
    let p4 = vec2<f32>(width * 0.33, expressive_mouth_y(0.33, curve, tension));
    let p5 = vec2<f32>(width * 0.67, expressive_mouth_y(0.67, curve, tension));
    let p6 = vec2<f32>(width, expressive_mouth_y(1.0, curve, tension));
    return min(
        min(
            min(sd_capsule(local, p0, p1, 0.0), sd_capsule(local, p1, p2, 0.0)),
            sd_capsule(local, p2, p3, 0.0),
        ),
        min(
            sd_capsule(local, p3, p4, 0.0),
            min(sd_capsule(local, p4, p5, 0.0), sd_capsule(local, p5, p6, 0.0)),
        ),
    );
}

fn mouth_layer(point: vec2<f32>, body_color: vec3<f32>, secondary: vec3<f32>, _glow_color: vec3<f32>) -> vec4<f32> {
    let center = head_center() + vec2<f32>(0.0, -0.105 - globals.body_shape.w * 0.050);
    let open = globals.brow_mouth.z;
    let curve = globals.brow_mouth.w;
    let tension = globals.mouth_voice.x;
    let voice = globals.mouth_voice.z;
    let width = 0.070 + tension * 0.040 + open * 0.026;
    let height = 0.012 + open * 0.080 + voice * 0.012;
    let local = point - center;
    let normalized_x = clamp(local.x / max(width, 0.001), -1.0, 1.0);
    let expressive_y = expressive_mouth_y(normalized_x, curve, tension);
    let deformed_local = vec2<f32>(local.x, local.y - expressive_y * 0.35);
    let open_distance = sd_ellipse(deformed_local, vec2<f32>(width, height));
    let open_mask = (1.0 - smoothstep(-0.003, 0.010, open_distance)) * smoothstep(0.025, 0.13, open);
    let line_distance = expressive_mouth_distance(local, width, curve, tension);
    let line_mask = (1.0 - smoothstep(0.006, 0.016, line_distance)) * (1.0 - open_mask);
    let corner_mask = smoothstep(0.62, 0.94, abs(normalized_x)) * line_mask;
    let crease_shadow = (1.0 - smoothstep(0.008, 0.022, line_distance)) * 0.10;
    let inner = mix(body_color, secondary, 0.10) * 0.19;
    var color = mix(secondary * (0.44 - crease_shadow), inner, open_mask);
    color = mix(color, secondary * 0.34, corner_mask * saturate(abs(curve) * 0.18 + 0.08));
    return vec4<f32>(color, max(open_mask * 0.76, line_mask));
}

fn occlusion_visibility(point: vec2<f32>) -> f32 {
    let mode = globals.occlusion.x;
    let edge = globals.occlusion.y;
    let softness = globals.occlusion.z;
    if (mode < 0.5) {
        return 1.0;
    }
    if (mode < 1.5) {
        return 1.0 - smoothstep(edge - softness, edge + softness, point.x);
    }
    if (mode < 2.5) {
        return smoothstep(edge - softness, edge + softness, point.x);
    }
    if (mode < 3.5) {
        return smoothstep(edge - softness, edge + softness, point.y);
    }
    let head_reveal = smoothstep(edge - softness, edge + softness, point.y);
    let eye_reveal = eye_mask(body_space(point), eye_center(-1.0), eye_radii())
        + eye_mask(body_space(point), eye_center(1.0), eye_radii());
    return max(head_reveal * 0.82, saturate(eye_reveal));
}

fn background_for_mode(uv: vec2<f32>, mode: f32) -> vec4<f32> {
    if (mode < 0.5) {
        return vec4<f32>(0.0);
    }
    if (mode < 1.5) {
        return vec4<f32>(0.012, 0.016, 0.022, 1.0);
    }
    if (mode < 2.5) {
        return vec4<f32>(0.96, 0.965, 0.97, 1.0);
    }
    if (mode < 3.5) {
        return vec4<f32>(0.42, 0.43, 0.45, 1.0);
    }
    if (mode < 4.5) {
        let cell = floor(uv * 14.0);
        let alternating = (i32(cell.x) + i32(cell.y)) % 2;
        let value = select(0.18, 0.72, alternating == 0);
        let accent = 0.04 * sin(uv.x * 43.0 + uv.y * 31.0);
        return vec4<f32>(vec3<f32>(value + accent), 1.0);
    }
    if (mode < 5.5) {
        return vec4<f32>(0.56, 0.40, 0.29, 1.0);
    }
    return vec4<f32>(0.22, 0.36, 0.52, 1.0);
}

fn review_background(uv: vec2<f32>) -> vec4<f32> {
    return background_for_mode(uv, globals.render_mode.y);
}

fn material_background(uv: vec2<f32>) -> vec4<f32> {
    if (globals.render_mode.y > 0.5) {
        return review_background(uv);
    }
    if (globals.render_mode.z > 1.5) {
        let desktop_uv = uv * globals.desktop_capture.xy + globals.desktop_capture.zw;
        return background_for_mode(desktop_uv, 4.0);
    }
    if (globals.render_mode.z > 0.5) {
        let capture_uv = clamp(
            uv * globals.desktop_capture.xy + globals.desktop_capture.zw,
            vec2<f32>(0.0),
            vec2<f32>(1.0),
        );
        let captured = textureSample(desktop_background, desktop_sampler, capture_uv).rgb;
        let neutral = vec3<f32>(0.040, 0.050, 0.062);
        return vec4<f32>(mix(neutral, captured, globals.desktop_capture_meta.x), 1.0);
    }
    return vec4<f32>(0.0);
}

fn sdf_antialias(field: f32) -> f32 {
    // Derivative-scaled reconstruction softens subpixel phase changes without an
    // extra multisample render target or another full-screen pass.
    let pixel_floor = globals.render_mode.w * globals.appendages.w * 1.25;
    return max(fwidth(field) * 1.28, pixel_floor);
}

@fragment
fn fragment_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let aspect = globals.viewport_time.x;
    var point = input.uv * 2.0 - vec2<f32>(1.0);
    point.x *= aspect;
    point.y = -point.y;
    point *= globals.appendages.w;
    let body_point = body_space(point);

    let main_field = body_field(point);
    let droplet_data = droplet_fields(body_point, main_field);
    let drop_field = droplet_data.x;
    let merged_field = droplet_data.y;
    let antialias = sdf_antialias(merged_field);
    let merged_coverage = 1.0 - smoothstep(-antialias, antialias, merged_field);
    if (merged_coverage <= 0.0005) {
        return vec4<f32>(0.0);
    }

    let body_antialias = sdf_antialias(main_field);
    let drop_antialias = sdf_antialias(drop_field);
    let body_coverage = 1.0 - smoothstep(-body_antialias, body_antialias, main_field);
    let droplet_coverage = 1.0 - smoothstep(-drop_antialias, drop_antialias, drop_field);
    let normal = body_normal(merged_field);
    let primary = hsv_to_rgb(globals.primary_hsv.xyz);
    let secondary = hsv_to_rgb(globals.secondary_hsv.xyz);
    let glow_color = hsv_to_rgb(globals.glow_hsv.xyz);
    let light = normalize(vec3<f32>(-0.42, 0.58, 0.70));
    let fill_light = normalize(vec3<f32>(0.72, -0.18, 0.66));
    let view = vec3<f32>(0.0, 0.0, 1.0);
    let half_vector = normalize(light + view);
    let ndotl = dot(normal, light);
    let ndotv = saturate(dot(normal, view));
    let wrapped_light = saturate((ndotl + 0.28) / 1.28);
    let key = smoothstep(0.08, 0.82, wrapped_light);
    let fill = saturate(dot(normal, fill_light)) * 0.5 + 0.5;
    let interior = interior_depth(merged_field);
    let optical_thickness = sqrt(saturate(interior * (2.0 - interior)));
    let edge_shell = 1.0 - smoothstep(0.00, 0.20, interior);
    let inner_mass = smoothstep(0.08, 0.68, interior);
    let thinness = 1.0 - interior;
    let rim = pow(1.0 - ndotv, globals.material_b.w) * globals.material_b.z;
    let fresnel = pow(1.0 - ndotv, 2.60);
    let softness = globals.face_shape.w;
    let transmission = pow(saturate((dot(normal, -light) + 0.18) / 1.18), 1.45)
        * pow(thinness, 0.58);
    let specular = pow(saturate(dot(normal, half_vector)), globals.material_c.w);

    let flow_value = living_flow(body_point);
    let flow_gradient = vec2<f32>(dpdx(flow_value), dpdy(flow_value));
    let flow_positive = smoothstep(0.42, 0.88, 0.5 + flow_value * 0.5);
    let flow_negative = smoothstep(0.44, 0.86, 0.5 - flow_value * 0.5);
    let flow_light = inner_mass * flow_positive * globals.flow.x;
    let flow_cool = inner_mass * flow_negative * globals.flow.x;
    let iridescent_hue = fract(
        globals.primary_hsv.x
        + 0.11
        + rim * 0.38
        + flow_value * globals.flow.x * 0.070,
    );
    let iridescent = hsv_to_rgb(vec3<f32>(iridescent_hue, 0.50, 1.0));
    let shadow_color = mix(primary * 0.42, secondary * 0.48, 0.36);
    let lit_color = mix(primary * 0.94, primary * 1.13 + vec3<f32>(0.035, 0.025, 0.0), key);
    var membrane_color = mix(shadow_color, lit_color, 0.28 + key * 0.72);
    membrane_color += secondary * fill * 0.055;
    membrane_color += glow_color * transmission * (0.19 + softness * 0.19);
    membrane_color = mix(membrane_color, iridescent, rim * (0.10 + softness * 0.13));
    let broad_specular = pow(saturate(dot(normal, half_vector)), globals.material_c.y);
    membrane_color += vec3<f32>(1.0, 0.97, 0.92)
        * (specular * (0.080 + softness * 0.045) * globals.material_c.z
            + broad_specular * 0.032 * globals.material_c.x);
    membrane_color += mix(glow_color, iridescent, 0.26)
        * rim
        * (0.12 + softness * 0.11 + globals.viewport_time.w * 0.05);

    // Three bilinear taps provide a small thickness-dependent transmission blur. The
    // SDF normal supplies the broad lens and the analytic flow derivative supplies a
    // moving micro-distortion, avoiding a blur texture or an extra render pass.
    let lens_offset = normal.xy
        * mix(0.0045, 0.0150, optical_thickness)
        * mix(0.82, 1.18, globals.physiology_a.w)
        * globals.material_b.x;
    let flow_offset = flow_gradient
        * inner_mass
        * globals.flow.x
        * (0.026 + globals.flow.w * 0.08);
    let refracted_uv = input.uv + lens_offset + flow_offset;
    let inverse_height = globals.render_mode.w;
    let texel = vec2<f32>(inverse_height / max(aspect, 0.001), inverse_height);
    let blur_axis = normalize(vec2<f32>(0.72, 0.69) + normal.xy * 0.18);
    let blur_offset = blur_axis
        * texel
        * mix(0.70, 1.35, optical_thickness)
        * globals.material_b.y;
    let behind_center = material_background(refracted_uv);
    let behind = behind_center * 0.50
        + material_background(refracted_uv + blur_offset) * 0.25
        + material_background(refracted_uv - blur_offset) * 0.25;
    let has_refractive_background = behind.a > 0.5;

    // Beer-Lambert-style volume absorption plus a separate wet Fresnel membrane.
    // Keeping these as two Porter-Duff layers prevents the translucent center from
    // reading like a flat alpha-tinted plastic surface.
    let thickness = pow(optical_thickness, 0.78) * globals.material_a.z;
    let optical_depth = (0.75 + globals.physiology_a.z * 1.65)
        * (0.62 + (1.0 - globals.physiology_a.w) * 0.55);
    let volume_alpha = saturate(
        1.0 - exp(-optical_depth * thickness * 0.31 * globals.material_a.x),
    );
    let membrane_mask = saturate(pow(edge_shell, 0.78) * 0.68 + fresnel * 0.78);
    let membrane_alpha = globals.physiology_a.y * membrane_mask * 0.80;
    let material_alpha = saturate(
        volume_alpha + membrane_alpha * (1.0 - volume_alpha),
    ) * globals.material_d.y;
    var volume_color = mix(
        primary * 0.30,
        glow_color * 0.72,
        0.34 + flow_positive * globals.flow.x * 0.32,
    );
    volume_color += glow_color * flow_light * 0.58 * globals.material_a.y;
    volume_color += mix(secondary * 0.74, glow_color * 0.62, 0.34)
        * flow_cool
        * 0.14;
    let material_premultiplied = volume_color * volume_alpha
        + membrane_color * membrane_alpha * (1.0 - volume_alpha);
    let fallback_volume = material_premultiplied / max(material_alpha, 0.0001);

    let pigment = clamp(mix(primary, secondary, 0.16), vec3<f32>(0.04), vec3<f32>(0.96));
    let absorption_coeff = (vec3<f32>(1.0) - pigment * 0.72)
        * (0.62 + globals.physiology_a.z * 1.42)
        * globals.material_a.x;
    let transmittance = exp(-absorption_coeff * thickness * 0.92);
    let absorbed = vec3<f32>(1.0) - transmittance;
    var refracted_volume = behind.rgb * transmittance;
    refracted_volume += primary * absorbed * (0.10 + inner_mass * 0.12);
    refracted_volume += glow_color
        * flow_light
        * (0.18 + globals.physiology_b.x * 0.26);
    refracted_volume += mix(secondary * 0.72, glow_color * 0.58, 0.42)
        * flow_cool
        * 0.065;
    let membrane_mix = membrane_alpha * (0.44 + rim * 0.28);
    let refracted_material = mix(refracted_volume, membrane_color, membrane_mix);
    var color = select(fallback_volume, refracted_material, has_refractive_background);

    let core_center = vec2<f32>(0.0, -0.06 + globals.motion_b.x * 0.2);
    let core_radii = vec2<f32>(
        globals.visual_detail.x * 0.55,
        globals.visual_detail.x * 0.72,
    );
    let core_distance = length((body_point - core_center) / core_radii);
    let core_mask = (1.0 - smoothstep(0.20, 1.0, core_distance)) * inner_mass;
    let core_pulse = 0.72 + 0.28 * sin(globals.viewport_time.y * 1.1 + globals.pattern.z);
    color += glow_color
        * core_mask
        * globals.physiology_b.x
        * core_pulse
        * 0.38
        * globals.material_d.x;
    let purr_wave = sin(globals.viewport_time.y * 26.0 + point.y * 18.0) * globals.mouth_voice.w;
    color += glow_color * purr_wave * 0.018;

    let socket = max(eye_socket_shadow(body_point, -1.0), eye_socket_shadow(body_point, 1.0));
    color = mix(
        color,
        color * 0.88 + primary * 0.12,
        socket * body_coverage * 0.030,
    );
    let cheek_y = head_center().y - eye_radii().y * 0.92;
    let cheek_distance_left = length((body_point - vec2<f32>(eye_center(-1.0).x, cheek_y)) / vec2<f32>(0.080, 0.045));
    let cheek_distance_right = length((body_point - vec2<f32>(eye_center(1.0).x, cheek_y)) / vec2<f32>(0.080, 0.045));
    let cheek = (1.0 - smoothstep(0.58, 1.0, min(cheek_distance_left, cheek_distance_right)))
        * globals.mouth_voice.y;
    let cheek_color = mix(primary * 1.04, glow_color * 1.14, 0.74);
    color = mix(color, cheek_color, cheek * 0.24);

    let left_eye = eye_layer(body_point, -1.0, color, secondary, glow_color);
    let right_eye = eye_layer(body_point, 1.0, color, secondary, glow_color);
    color = mix(color, left_eye.rgb, left_eye.a * body_coverage * globals.face_tuning.x);
    color = mix(color, right_eye.rgb, right_eye.a * body_coverage * globals.face_tuning.x);
    let brow = max(brow_mask(body_point, -1.0), brow_mask(body_point, 1.0))
        * body_coverage
        * globals.face_tuning.x;
    color = mix(color, secondary * 0.42, brow * (0.48 + globals.brow_mouth.x * 0.20));
    let mouth = mouth_layer(body_point, membrane_color, secondary, glow_color);
    color = mix(color, mouth.rgb, mouth.a * body_coverage * globals.face_tuning.x);

    let drop_gradient = normalize(vec2<f32>(dpdx(drop_field), dpdy(drop_field)) + vec2<f32>(0.000001));
    let drop_highlight = pow(saturate(dot(drop_gradient, normalize(vec2<f32>(-0.55, 0.84)))), 8.5)
        * droplet_coverage;
    color += vec3<f32>(0.94, 1.0, 1.0)
        * drop_highlight
        * (0.16 + globals.droplet_meta.y * 0.10);

    let pulse_alpha = globals.physiology_a.x * inner_mass * 0.020;
    let visibility = occlusion_visibility(point);
    let face_coverage = saturate(max(max(left_eye.a, right_eye.a), max(brow, mouth.a)))
        * globals.face_tuning.x;
    let face_alpha_floor = face_coverage * body_coverage * 0.98;
    let refractive_alpha = select(
        saturate(material_alpha + pulse_alpha),
        1.0,
        has_refractive_background,
    );
    let shape_alpha = merged_coverage
        * visibility
        * max(refractive_alpha, face_alpha_floor);
    let halo_mask = (1.0 - smoothstep(0.0, 0.075, max(merged_field, 0.0)))
        * (1.0 - merged_coverage);
    let halo_coverage = saturate(halo_mask * globals.physiology_b.y * visibility * 0.55);
    let uncovered_by_shape = 1.0 - shape_alpha;
    let final_alpha = saturate(shape_alpha + halo_coverage * uncovered_by_shape);
    let surface_color = clamp(color, vec3<f32>(0.0), vec3<f32>(1.0));
    let halo_color = clamp(glow_color, vec3<f32>(0.0), vec3<f32>(1.0));
    let premultiplied = surface_color * shape_alpha
        + halo_color * halo_coverage * uncovered_by_shape;

    // Lab-only inspection modes. Each branch remains premultiplied so the
    // compositor tests the same alpha path as the production material.
    let debug_view = globals.face_tuning.y;
    if (debug_view > 0.5) {
        var debug_color = vec3<f32>(0.0);
        var debug_alpha = final_alpha;
        if (debug_view < 1.5) {
            let signed_field = saturate(0.5 - merged_field * 5.0);
            debug_color = mix(vec3<f32>(0.03, 0.12, 0.55), vec3<f32>(1.0, 0.28, 0.04), signed_field);
        } else if (debug_view < 2.5) {
            debug_color = vec3<f32>(final_alpha);
            debug_alpha = max(final_alpha, 0.03);
        } else if (debug_view < 3.5) {
            debug_color = mix(vec3<f32>(0.05, 0.08, 0.10), vec3<f32>(1.0, 0.05, 0.72), face_coverage);
        } else {
            debug_color = mix(vec3<f32>(0.02, 0.16, 0.55), vec3<f32>(0.95, 0.92, 0.10), flow_positive);
        }
        return vec4<f32>(debug_color * debug_alpha, debug_alpha);
    }

    // Always write linear premultiplied color to the HDR intermediate. Output alpha
    // convention and review backgrounds are handled once in the native-size resolve.
    return vec4<f32>(premultiplied, final_alpha);
}
