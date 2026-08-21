struct Globals {
    viewport_time: vec4<f32>,
    body_shape: vec4<f32>,
    face_shape: vec4<f32>,
    appendages: vec4<f32>,
    primary_hsv: vec4<f32>,
    secondary_hsv: vec4<f32>,
    glow_hsv: vec4<f32>,
    gaze_pupil: vec4<f32>,
    lids_brows: vec4<f32>,
    brow_mouth: vec4<f32>,
    mouth_voice: vec4<f32>,
    motion_a: vec4<f32>,
    motion_b: vec4<f32>,
    pattern: vec4<f32>,
    occlusion: vec4<f32>,
};

@group(0) @binding(0) var<uniform> globals: Globals;

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
        vec2<f32>(0.0, 0.0),
        vec2<f32>(2.0, 0.0),
        vec2<f32>(0.0, 2.0),
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

fn sd_capsule(point: vec2<f32>, start: vec2<f32>, end: vec2<f32>, radius: f32) -> f32 {
    let segment = end - start;
    let denominator = max(dot(segment, segment), 0.000001);
    let projection = clamp(dot(point - start, segment) / denominator, 0.0, 1.0);
    return length(point - (start + segment * projection)) - radius;
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
    let length = globals.body_shape.y;
    let breath = (globals.motion_b.w - 0.5) * 0.018;
    return vec2<f32>(globals.motion_a.w, 0.235 + length * 0.105 + globals.motion_b.x + breath);
}

fn body_field(point: vec2<f32>) -> f32 {
    let p = body_space(point);
    let width = globals.body_shape.x;
    let length = globals.body_shape.y;
    let roundness = globals.body_shape.z;
    let head_ratio = globals.body_shape.w;
    let softness = globals.face_shape.w;
    let compression = globals.pattern.w;
    let breath = (globals.motion_b.w - 0.5) * (0.008 + softness * 0.014);

    let torso_center = vec2<f32>(0.0, -0.105 - compression * 0.035);
    let torso_radii = vec2<f32>(
        0.205 + width * 0.135 + roundness * 0.020 + breath,
        0.280 + length * 0.135 - compression * 0.045 + breath * 0.55,
    );
    let head_radii = vec2<f32>(
        0.205 + head_ratio * 0.145,
        0.180 + head_ratio * 0.135,
    );
    let head = sd_ellipse(p - head_center(), head_radii);
    var field = smooth_union(
        sd_ellipse(p - torso_center, torso_radii),
        head,
        0.075 + softness * 0.080,
    );

    let cheek_y = head_center().y - head_radii.y * 0.34;
    let cheek_x = head_radii.x * 0.62;
    let cheek_radius = vec2<f32>(head_radii.x * 0.43, head_radii.y * 0.40);
    field = smooth_union(field, sd_ellipse(p - vec2<f32>(-cheek_x, cheek_y), cheek_radius), 0.055);
    field = smooth_union(field, sd_ellipse(p - vec2<f32>(cheek_x, cheek_y), cheek_radius), 0.055);

    let tail_lag = globals.motion_b.yz;
    let tail_start = vec2<f32>(0.0, torso_center.y - torso_radii.y * 0.72);
    let tail_end = tail_start
        + vec2<f32>(
            tail_lag.x + sin(globals.viewport_time.y * 1.35) * 0.035,
            -0.18 - globals.appendages.x * 0.16 + tail_lag.y,
        );
    let tail_radius = 0.026 + globals.appendages.y * 0.22;
    field = smooth_union(
        field,
        sd_capsule(p, tail_start, tail_end, tail_radius),
        0.035 + softness * 0.025,
    );
    field = smooth_union(
        field,
        sd_ellipse(p - tail_end, vec2<f32>(tail_radius * 1.15, tail_radius * 0.88)),
        0.020,
    );

    let arm_y = torso_center.y - torso_radii.y * 0.12;
    let arm_reach = 0.045 + globals.appendages.z * 0.24;
    let arm_radius = 0.022 + softness * 0.012;
    field = smooth_union(
        field,
        sd_capsule(
            p,
            vec2<f32>(-torso_radii.x * 0.72, arm_y),
            vec2<f32>(-torso_radii.x - arm_reach, arm_y - 0.055),
            arm_radius,
        ),
        0.028,
    );
    field = smooth_union(
        field,
        sd_capsule(
            p,
            vec2<f32>(torso_radii.x * 0.72, arm_y),
            vec2<f32>(torso_radii.x + arm_reach, arm_y - 0.055),
            arm_radius,
        ),
        0.028,
    );

    let fin_size = globals.appendages.z;
    if (fin_size > 0.025) {
        let fin_y = head_center().y + head_radii.y * 0.28;
        let fin_radii = vec2<f32>(0.030 + fin_size * 0.20, 0.050 + fin_size * 0.15);
        field = smooth_union(
            field,
            sd_ellipse(
                rotate2(p - vec2<f32>(-head_radii.x * 0.90, fin_y), -0.55),
                fin_radii,
            ),
            0.028,
        );
        field = smooth_union(
            field,
            sd_ellipse(
                rotate2(p - vec2<f32>(head_radii.x * 0.90, fin_y), 0.55),
                fin_radii,
            ),
            0.028,
        );
    }
    return field;
}

fn body_normal(point: vec2<f32>) -> vec3<f32> {
    let epsilon = 0.0025;
    let gradient = vec2<f32>(
        body_field(point + vec2<f32>(epsilon, 0.0)) - body_field(point - vec2<f32>(epsilon, 0.0)),
        body_field(point + vec2<f32>(0.0, epsilon)) - body_field(point - vec2<f32>(0.0, epsilon)),
    );
    let planar = normalize(gradient + vec2<f32>(0.00001));
    return normalize(vec3<f32>(planar * 0.58, 0.82));
}

fn hash21(point: vec2<f32>) -> f32 {
    return fract(sin(dot(point, vec2<f32>(127.1, 311.7)) + globals.pattern.z) * 43758.5453);
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

fn lid_aperture(local_eye: vec2<f32>, blink: f32, squint: f32) -> f32 {
    let upper = 0.91 - blink * 1.58 - squint * 0.34;
    let lower = -0.91 + blink * 0.73 + squint * 0.18;
    let upper_open = 1.0 - smoothstep(upper - 0.055, upper + 0.055, local_eye.y);
    let lower_open = smoothstep(lower - 0.055, lower + 0.055, local_eye.y);
    return upper_open * lower_open;
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
    let lid = mask * (1.0 - aperture);

    let gaze = globals.gaze_pupil.xy;
    let convergence = globals.gaze_pupil.z;
    let inward = -side * convergence;
    let iris_offset = vec2<f32>(gaze.x * 0.30 + inward, gaze.y * 0.23);
    let iris_distance = length((local - iris_offset) / vec2<f32>(0.50, 0.54));
    let iris = visible * (1.0 - smoothstep(0.88, 1.0, iris_distance));
    let pupil_radius = mix(0.18, 0.36, globals.gaze_pupil.w);
    let pupil_distance = length((local - iris_offset) / vec2<f32>(pupil_radius, pupil_radius * 1.08));
    let pupil = visible * (1.0 - smoothstep(0.88, 1.0, pupil_distance));
    let outline = mask * (1.0 - smoothstep(0.82, 0.98, length(local)));

    let sclera = vec3<f32>(0.985, 0.955, 0.875);
    let iris_color = mix(secondary * 0.78, glow_color, 0.28 + globals.viewport_time.w * 0.18);
    var color = sclera;
    color = mix(color, iris_color, iris);
    color = mix(color, vec3<f32>(0.025, 0.035, 0.050), pupil);
    color = mix(color, secondary * 0.45, outline * 0.16);

    let highlight_center = vec2<f32>(-0.18, 0.22) + iris_offset * 0.25;
    let highlight = visible
        * (1.0 - smoothstep(0.035, 0.105, length(local - highlight_center)))
        * (1.0 - pupil * 0.28);
    color += vec3<f32>(0.95, 1.0, 1.0) * highlight * 0.92;

    let lid_tint = mix(body_color, secondary, 0.10 + globals.lids_brows.z * 0.08);
    color = mix(color, lid_tint, lid);
    return vec4<f32>(color, max(visible, lid));
}

fn brow_mask(point: vec2<f32>, side: f32) -> f32 {
    let center = eye_center(side);
    let radii = eye_radii();
    let raise = globals.lids_brows.w * 0.032;
    let tension = globals.brow_mouth.x;
    let asymmetry = globals.brow_mouth.y * side * 0.018;
    let inner_raise = tension * 0.040;
    let outer_raise = globals.lids_brows.w * 0.018;
    let start = center + vec2<f32>(-side * radii.x * 0.68, radii.y * 1.06 + raise + inner_raise + asymmetry);
    let end = center + vec2<f32>(side * radii.x * 0.66, radii.y * 1.12 + raise + outer_raise - asymmetry);
    let distance = sd_capsule(point, start, end, 0.010 + tension * 0.004);
    return 1.0 - smoothstep(0.0, 0.012, distance);
}

fn mouth_layer(point: vec2<f32>, secondary: vec3<f32>, glow_color: vec3<f32>) -> vec4<f32> {
    let center = head_center() + vec2<f32>(0.0, -0.105 - globals.body_shape.w * 0.050);
    let open = globals.brow_mouth.z;
    let curve = globals.brow_mouth.w;
    let tension = globals.mouth_voice.x;
    let voice = globals.mouth_voice.z;
    let width = 0.070 + tension * 0.040 + open * 0.026;
    let height = 0.012 + open * 0.080 + voice * 0.012;
    let local = point - center;
    let open_distance = sd_ellipse(local, vec2<f32>(width, height));
    let open_mask = (1.0 - smoothstep(-0.003, 0.010, open_distance)) * smoothstep(0.025, 0.13, open);

    let normalized_x = clamp(local.x / max(width, 0.001), -1.0, 1.0);
    let curve_y = curve * 0.034 * (1.0 - normalized_x * normalized_x);
    let line_distance = abs(local.y - curve_y) + max(abs(local.x) - width, 0.0) * 0.7;
    let line_mask = (1.0 - smoothstep(0.006, 0.016, line_distance)) * (1.0 - open_mask);

    let inner = vec3<f32>(0.070, 0.025, 0.055);
    var color = mix(secondary * 0.48, inner, open_mask);
    let tongue_center = center + vec2<f32>(0.0, -height * 0.34);
    let tongue = open_mask
        * (1.0 - smoothstep(0.82, 1.0, length((point - tongue_center) / vec2<f32>(width * 0.62, height * 0.43))));
    color = mix(color, mix(vec3<f32>(0.92, 0.25, 0.42), glow_color, 0.15), tongue * 0.78);
    return vec4<f32>(color, max(open_mask, line_mask));
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
    let eye_reveal = eye_mask(point, eye_center(-1.0), eye_radii())
        + eye_mask(point, eye_center(1.0), eye_radii());
    return max(head_reveal * 0.82, saturate(eye_reveal));
}

@fragment
fn fragment_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let aspect = globals.viewport_time.x;
    var point = input.uv * 2.0 - vec2<f32>(1.0);
    point.x *= aspect;
    point *= globals.appendages.w;

    let field = body_field(point);
    let antialias = max(fwidth(field), 0.0015);
    let body_alpha = 1.0 - smoothstep(-antialias, antialias, field);
    if (body_alpha <= 0.0005) {
        return vec4<f32>(0.0);
    }

    let primary = hsv_to_rgb(globals.primary_hsv.xyz);
    let secondary = hsv_to_rgb(globals.secondary_hsv.xyz);
    let glow_color = hsv_to_rgb(globals.glow_hsv.xyz);
    let normal = body_normal(point);
    let light = normalize(vec3<f32>(-0.38, 0.64, 0.68));
    let diffuse = max(dot(normal, light), 0.0);
    let toon = select(select(0.58, 0.80, diffuse > 0.28), 1.04, diffuse > 0.68);
    let rim = pow(1.0 - max(normal.z, 0.0), 2.2);
    let softness = globals.face_shape.w;
    let subsurface = pow(max(dot(normal, -light), 0.0), 2.0) * (0.05 + softness * 0.10);

    let body_point = body_space(point);
    var warped = body_point * globals.pattern.x;
    warped += vec2<f32>(sin(warped.y * 2.1), cos(warped.x * 1.7)) * 0.14;
    let bands = 0.5 + 0.5 * sin((warped.x + warped.y) * 3.5 + globals.pattern.z);
    let cells = hash21(floor(warped * 2.0));
    let pattern_mask = smoothstep(0.46, 0.68, cells * bands) * globals.pattern.y;
    var color = primary * toon;
    color = mix(color, secondary * (0.88 + diffuse * 0.18), pattern_mask * 0.26);
    color += glow_color * (globals.viewport_time.w * 0.075 + subsurface);
    color += mix(secondary, glow_color, 0.35) * rim * (0.09 + softness * 0.08);

    let purr_wave = sin(globals.viewport_time.y * 26.0 + point.y * 18.0) * globals.mouth_voice.w;
    color += glow_color * purr_wave * 0.018;

    let cheek_y = head_center().y - eye_radii().y * 0.92;
    let cheek_distance_left = length((point - vec2<f32>(eye_center(-1.0).x, cheek_y)) / vec2<f32>(0.080, 0.045));
    let cheek_distance_right = length((point - vec2<f32>(eye_center(1.0).x, cheek_y)) / vec2<f32>(0.080, 0.045));
    let cheek = (1.0 - smoothstep(0.58, 1.0, min(cheek_distance_left, cheek_distance_right)))
        * globals.mouth_voice.y;
    color = mix(color, glow_color * 1.12, cheek * 0.28);

    let left_eye = eye_layer(point, -1.0, color, secondary, glow_color);
    let right_eye = eye_layer(point, 1.0, color, secondary, glow_color);
    color = mix(color, left_eye.rgb, left_eye.a);
    color = mix(color, right_eye.rgb, right_eye.a);

    let brow = max(brow_mask(point, -1.0), brow_mask(point, 1.0));
    color = mix(color, secondary * 0.42, brow * (0.48 + globals.brow_mouth.x * 0.20));

    let mouth = mouth_layer(point, secondary, glow_color);
    color = mix(color, mouth.rgb, mouth.a);

    let visibility = occlusion_visibility(point);
    let alpha = body_alpha * visibility;
    if (alpha <= 0.0005) {
        return vec4<f32>(0.0);
    }
    return vec4<f32>(color, alpha);
}
