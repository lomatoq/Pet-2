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
    cinematic_a: vec4<f32>,
    cinematic_b: vec4<f32>,
    cinematic_c: vec4<f32>,
    cinematic_d: vec4<f32>,
    cinematic_e: vec4<f32>,
    cinematic_f: vec4<f32>,
    cinematic_g: vec4<f32>,
    cinematic_h: vec4<f32>,
    cinematic_i: vec4<f32>,
    glow_orb_position_radius: array<vec4<f32>, 8>,
    glow_orb_color_intensity: array<vec4<f32>, 8>,
    soul_glow_position_radius: array<vec4<f32>, 6>,
    face_lids: array<vec4<f32>, 2>,
    face_brows: array<vec4<f32>, 2>,
    face_mouth: vec4<f32>,
    face_eye: vec4<f32>,
};

@group(0) @binding(0) var<uniform> globals: Globals;
@group(0) @binding(1) var desktop_background: texture_2d<f32>;
@group(0) @binding(2) var desktop_sampler: sampler;
@group(0) @binding(3) var density_texture: texture_2d<f32>;
@group(0) @binding(4) var density_sampler: sampler;
@group(0) @binding(5) var macro_texture: texture_2d<f32>;
@group(0) @binding(6) var material_flow_texture: texture_2d<f32>;
@group(0) @binding(7) var studio_environment: texture_2d<f32>;
@group(0) @binding(8) var studio_sampler: sampler;

// Compile the two material variants as separate, statically-specialized
// pipelines. Keeping this as a pipeline override lets Naga and the native
// backend discard the unused branch before compiling the large fragment
// program instead of asking the driver to optimize both materials at once.
override CINEMATIC: bool = true;

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

fn hsv_to_rgb(hsv: vec3<f32>) -> vec3<f32> {
    let k = vec4<f32>(1.0, 2.0 / 3.0, 1.0 / 3.0, 3.0);
    let p = abs(fract(hsv.xxx + k.xyz) * 6.0 - k.www);
    return hsv.z * mix(k.xxx, clamp(p - k.xxx, vec3<f32>(0.0), vec3<f32>(1.0)), hsv.y);
}

fn density_at(uv: vec2<f32>) -> vec4<f32> {
    return textureSample(density_texture, density_sampler, clamp(uv, vec2<f32>(0.0), vec2<f32>(1.0)));
}

fn macro_at(uv: vec2<f32>) -> vec4<f32> {
    return textureSample(macro_texture, density_sampler, clamp(uv, vec2<f32>(0.0), vec2<f32>(1.0)));
}

fn material_flow_at(uv: vec2<f32>) -> vec4<f32> {
    return textureSample(material_flow_texture, density_sampler, clamp(uv, vec2<f32>(0.0), vec2<f32>(1.0)));
}

fn liquid_core(phi: f32) -> f32 {
    let iso = globals.liquid_meta.z;
    let core_level = max(globals.material_e.x, iso + 0.05);
    return saturate((phi - iso) / (core_level - iso));
}

fn reconstructed_height(uv: vec2<f32>) -> f32 {
    let core = liquid_core(density_at(uv).r);
    return globals.material_e.z * pow(core, globals.material_e.y);
}

fn macro_height(uv: vec2<f32>) -> f32 {
    let integrated_thickness = max(macro_at(uv).g, 0.0);
    let rounded = 1.0 - exp(-integrated_thickness * 0.82);
    return globals.material_e.z * pow(max(rounded, 0.00001), globals.material_e.y);
}

fn macro_height_gradient(uv: vec2<f32>) -> vec2<f32> {
    let macro_size = vec2<f32>(textureDimensions(macro_texture));
    let texel = 1.0 / max(macro_size, vec2<f32>(1.0));
    return vec2<f32>(
        (macro_height(uv + vec2<f32>(texel.x, 0.0))
            - macro_height(uv - vec2<f32>(texel.x, 0.0))) / max(2.0 * texel.x, 0.00001),
        (macro_height(uv + vec2<f32>(0.0, texel.y))
            - macro_height(uv - vec2<f32>(0.0, texel.y))) / max(2.0 * texel.y, 0.00001),
    );
}

struct StudioLobes {
    base: vec3<f32>,
    coat: vec3<f32>,
    fill: vec3<f32>,
    base_mask: f32,
    coat_mask: f32,
};

fn rounded_box_distance(point: vec2<f32>, half_extent: vec2<f32>, radius: f32) -> f32 {
    let q = abs(point) - half_extent + vec2<f32>(radius);
    return min(max(q.x, q.y), 0.0) + length(max(q, vec2<f32>(0.0))) - radius;
}

fn rounded_softbox(
    coordinate: vec2<f32>,
    center: vec2<f32>,
    half_extent: vec2<f32>,
    corner_radius: f32,
    feather: f32,
) -> f32 {
    let distance = rounded_box_distance(coordinate - center, half_extent, corner_radius);
    return 1.0 - smoothstep(-feather, feather, distance);
}

fn studio_lobe_masks(
    reflection: vec3<f32>,
    base_roughness: f32,
    coat_roughness: f32,
) -> vec2<f32> {
    let base_width = vec2<f32>(0.13, 0.085) + vec2<f32>(0.32, 0.22) * base_roughness;
    let base_feather = 0.025 + base_roughness * 0.16;
    let base_mask = rounded_softbox(
        reflection.xy,
        vec2<f32>(-0.31, 0.27),
        base_width,
        0.075 + base_roughness * 0.11,
        base_feather,
    );
    let coat_width = vec2<f32>(0.045, 0.038) + vec2<f32>(0.19, 0.14) * coat_roughness;
    let coat_feather = 0.012 + coat_roughness * 0.12;
    let coat_mask = rounded_softbox(
        reflection.xy,
        vec2<f32>(0.14, 0.43),
        coat_width,
        0.032 + coat_roughness * 0.075,
        coat_feather,
    );
    return vec2<f32>(base_mask, coat_mask);
}

fn studio_lobes(
    reflection: vec3<f32>,
    base_roughness: f32,
    coat_roughness: f32,
) -> StudioLobes {
    // The lobe geometry is analytic. Roughness now changes the actual angular
    // support and energy, not merely the brightness of one environment texel.
    let masks = studio_lobe_masks(reflection, base_roughness, coat_roughness);
    let base_mask = masks.x;
    let base_peak = mix(1.42, 0.48, sqrt(base_roughness));
    let coat_mask = masks.y;
    let coat_peak = mix(1.85, 0.74, sqrt(coat_roughness * 2.0));

    let uv = vec2<f32>(0.5 + reflection.x * 0.5, 0.5 - reflection.y * 0.5);
    let environment = textureSample(studio_environment, studio_sampler, uv).rgb;
    let environment_luma = dot(environment, vec3<f32>(0.2126, 0.7152, 0.0722));
    let fill = mix(vec3<f32>(environment_luma), environment, 0.35) * 0.18;
    return StudioLobes(
        vec3<f32>(1.08, 1.04, 1.02) * base_mask * base_peak,
        vec3<f32>(1.16, 1.18, 1.17) * coat_mask * coat_peak,
        fill,
        base_mask,
        coat_mask,
    );
}

fn jelly_relief_material(
    body_color: vec3<f32>,
    glow_color: vec3<f32>,
    signed_distance: f32,
    raised: f32,
) -> vec3<f32> {
    let relief = globals.cinematic_h.y;
    let distance_gradient = vec2<f32>(dpdx(signed_distance), -dpdy(signed_distance));
    let micro_normal = normalize(vec3<f32>(
        -distance_gradient * raised * (18.0 + relief * 20.0),
        1.0,
    ));
    let reflection = normalize(reflect(vec3<f32>(0.0, 0.0, -1.0), micro_normal));
    let lobes = studio_lobe_masks(
        reflection,
        globals.cinematic_c.y,
        globals.cinematic_c.z,
    );
    let base = body_color * globals.cinematic_i.x;
    let wet = chroma_preserving_rim(body_color, glow_color, 1.05, 1.10)
        * (lobes.x * 0.30 + lobes.y * 0.62 * globals.cinematic_i.y)
        * globals.cinematic_c.x;
    return base + wet * relief;
}

fn shifted_sine(sine_value: f32, cosine_value: f32, offset: vec3<f32>) -> vec3<f32> {
    return sine_value * cos(offset) + cosine_value * sin(offset);
}

fn caustic_network(coordinate: vec2<f32>) -> vec3<f32> {
    let phase_a = dot(coordinate, normalize(vec2<f32>(1.0, 0.37))) * 5.3;
    let phase_b = dot(coordinate, normalize(vec2<f32>(-0.43, 1.0))) * 5.7 + 1.71;
    let phase_c = dot(coordinate, normalize(vec2<f32>(0.71, 0.70))) * 8.9 + 0.83;
    let sine_a = sin(phase_a);
    let sine_b = sin(phase_b);
    let sine_c = sin(phase_c);
    let cosine_a = cos(phase_a);
    let cosine_b = cos(phase_b);
    let cosine_c = cos(phase_c);
    // Trig identities produce genuinely shifted RGB ridges from one shared wave
    // evaluation. This is visibly dispersive without tripling the caustic graph.
    let dispersion = globals.cinematic_e.z * 0.62;
    let spectral_offset = vec3<f32>(dispersion, 0.0, -dispersion);
    let a = shifted_sine(sine_a, cosine_a, spectral_offset * 0.91);
    let b = shifted_sine(sine_b, cosine_b, spectral_offset * -0.74);
    let c = shifted_sine(sine_c, cosine_c, spectral_offset * 1.26);
    let ridges = vec3<f32>(1.0) - abs((a + b + c * 0.34) / 2.34);
    // A wide Hermite ramp reads as diffuse light inside jelly instead of thin,
    // static contour lines.
    return smoothstep(vec3<f32>(0.22), vec3<f32>(0.80), ridges);
}

fn flowing_caustics(material_coordinate: vec2<f32>, material_velocity: vec2<f32>) -> vec3<f32> {
    let time = globals.viewport_time.y * globals.cinematic_d.z;
    let scale = globals.cinematic_d.y;
    let warp = vec2<f32>(
        sin(material_coordinate.y * scale * 2.7 + time * 0.73),
        cos(material_coordinate.x * scale * 2.3 - time * 0.61),
    ) * (0.055 + globals.flow.w * 0.28);
    let drift = material_velocity * 0.11;
    let phase = fract(time * 0.70);
    let weight_a = 1.0 - abs(phase * 2.0 - 1.0);
    let phase_b = fract(phase + 0.5);
    let weight_b = 1.0 - abs(phase_b * 2.0 - 1.0);
    let base = material_coordinate * scale + warp + drift;
    let a = caustic_network(base + vec2<f32>(time * 0.72, -time * 0.49));
    let b = caustic_network(base * 1.05 + vec2<f32>(-time * 0.58, time * 0.66) + 3.17);
    return (a * weight_a + b * weight_b) / max(weight_a + weight_b, 0.0001);
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
        // Evaluate the fallback checker in physical desktop pixels. Counting a
        // fixed number of cells in normalized UV stretched every cell on an
        // ultrawide desktop and made the pattern visibly slide/resize when the
        // host geometry changed. `desktop_capture` maps the current viewport to
        // the captured/global rect, so dividing the display viewport by that
        // scale reconstructs the stable desktop pixel domain.
        let render_scale = max(globals.liquid_meta.y, 1.0);
        let display_size = vec2<f32>(
            1.0 / max(globals.liquid_meta.w, 1.0e-7),
            1.0 / max(globals.render_mode.w, 1.0e-7),
        ) / render_scale;
        let capture_size = display_size / max(abs(globals.desktop_capture.xy), vec2<f32>(1.0e-5));
        let checker_coordinate = uv * capture_size / 64.0;
        let cell = floor(checker_coordinate);
        let alternating = (i32(cell.x) + i32(cell.y)) % 2;
        let value = select(0.18, 0.72, alternating == 0);
        let accent = 0.04 * sin(checker_coordinate.x * 2.17 + checker_coordinate.y * 1.83);
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
    // The lab background must enter the material pass itself. Otherwise all
    // refraction/blur/transmission controls appear inert even though compose
    // draws the same backdrop after shading.
    if (globals.render_mode.y > 0.5) {
        return review_background(uv);
    }
    if (globals.render_mode.z > 1.5) {
        let desktop_uv = uv * globals.desktop_capture.xy + globals.desktop_capture.zw;
        return background_for_mode(desktop_uv, 4.0);
    }
    if (globals.render_mode.z > 0.5) {
        let capture_uv = uv * globals.desktop_capture.xy + globals.desktop_capture.zw;
        let captured = textureSample(
            desktop_background,
            desktop_sampler,
            clamp(capture_uv, vec2<f32>(0.0), vec2<f32>(1.0)),
        );
        let neutral = vec3<f32>(0.040, 0.050, 0.062);
        return vec4<f32>(mix(neutral, captured.rgb, globals.desktop_capture_meta.x), 1.0);
    }
    return vec4<f32>(0.0);
}

fn luminance(color: vec3<f32>) -> f32 {
    return dot(color, vec3<f32>(0.2126, 0.7152, 0.0722));
}

fn chroma_preserving_rim(
    pigment: vec3<f32>,
    glow_color: vec3<f32>,
    saturation: f32,
    peak: f32,
) -> vec3<f32> {
    let base = mix(pigment, glow_color, 0.18);
    let luma = luminance(base);
    let saturated = max(
        vec3<f32>(0.0),
        vec3<f32>(luma) + (base - vec3<f32>(luma)) * saturation,
    );
    return saturated * (peak / max(max(max(saturated.r, saturated.g), saturated.b), 0.001));
}

fn screen_light_probe(uv: vec2<f32>) -> vec3<f32> {
    // Production already supplies the desktop crop. Two wide probes turn its
    // local luminance gradient into a slow highlight-direction/intensity bias;
    // Body Lab and transparent fallback take the zero-cost fixed-studio branch.
    if (globals.render_mode.z < 0.5 || globals.render_mode.z > 1.5) {
        return vec3<f32>(0.0, 0.0, 1.0);
    }
    if (globals.desktop_capture_meta.x < 0.01) {
        return vec3<f32>(0.0, 0.0, 0.92);
    }
    let offset = vec2<f32>(0.070, -0.070 * globals.viewport_time.x);
    let low_uv = (uv - offset) * globals.desktop_capture.xy + globals.desktop_capture.zw;
    let high_uv = (uv + offset) * globals.desktop_capture.xy + globals.desktop_capture.zw;
    let low = textureSample(desktop_background, desktop_sampler, low_uv).rgb;
    let high = textureSample(desktop_background, desktop_sampler, high_uv).rgb;
    let delta = luminance(high) - luminance(low);
    let gradient = vec2<f32>(delta, -delta * 0.82);
    let average = (luminance(low) + luminance(high)) * 0.5;
    return vec3<f32>(
        gradient * 0.28 * globals.desktop_capture_meta.x,
        mix(0.92, mix(0.82, 1.18, average), globals.desktop_capture_meta.x),
    );
}

fn local_point(uv: vec2<f32>) -> vec2<f32> {
    var point = uv * 2.0 - vec2<f32>(1.0);
    point.x *= globals.viewport_time.x;
    // Match the simulation's explicit Y-up convention used by density accumulation.
    point.y = -point.y;
    return point * globals.appendages.w - globals.body_shape.zw;
}

fn face_space(point: vec2<f32>) -> vec2<f32> {
    let delta = point - globals.face_frame_a.xy;
    let axis_x = normalize(globals.face_frame_a.zw + vec2<f32>(0.000001, 0.0));
    let axis_y = normalize(globals.face_frame_b.xy + vec2<f32>(0.0, 0.000001));
    return vec2<f32>(dot(delta, axis_x), dot(delta, axis_y))
        / max(globals.face_frame_b.zw, vec2<f32>(0.01));
}

fn eye_radii() -> vec2<f32> {
    let size = globals.face_shape.x;
    // A semantic eye disk stays very slightly wider than tall. Material strain is
    // carried by the face frame's pose, never by an anisotropic eye scale.
    return vec2<f32>(0.057 + size * 0.245, 0.054 + size * 0.225);
}

fn eye_center(side: f32) -> vec2<f32> {
    let spacing = 0.058 + globals.face_shape.y * 0.25;
    return vec2<f32>(side * spacing, 0.008);
}

fn lid_aperture(local_eye: vec2<f32>, blink: f32, squint: f32, side: f32) -> f32 {
    let shape = globals.face_lids[select(1u, 0u, side < 0.0)];
    let x2 = local_eye.x * local_eye.x;
    let inner_to_outer = clamp(local_eye.x * side * 0.5 + 0.5, 0.0, 1.0);
    let upper = 1.02 - x2 * (0.12 + shape.w * 0.12)
        + mix(shape.x, shape.y, inner_to_outer) * 0.5 - blink * 1.65 - squint * 0.2 - (1.0 - globals.face_eye.x) * 1.7;
    let lower = -0.98 + x2 * 0.12 + shape.z * 0.95 + blink * 0.72 + squint * 0.16;
    let aa = max(fwidth(local_eye.y), 0.035);
    let upper_open = 1.0 - smoothstep(upper - aa, upper + aa, local_eye.y);
    let lower_open = smoothstep(lower - aa, lower + aa, local_eye.y);
    return upper_open * lower_open * (1.0 - smoothstep(0.94, 0.995, blink));
}

fn eye_layer(
    point: vec2<f32>,
    side: f32,
    body_color: vec3<f32>,
    secondary: vec3<f32>,
    glow_color: vec3<f32>,
    highlight_shift: vec2<f32>,
) -> vec4<f32> {
    let local = (point - eye_center(side)) / eye_radii();
    let eye_distance = length(local);
    let eye_edge_footprint = max(fwidth(eye_distance), 0.004);
    let eye_mask = 1.0 - smoothstep(0.95, 1.02, eye_distance);
    let gaze = clamp(
        vec2<f32>(
            globals.gaze_pupil.x * 0.32 - side * globals.gaze_pupil.z,
            globals.gaze_pupil.y * 0.26,
        ),
        vec2<f32>(-0.34),
        vec2<f32>(0.34),
    );
    let iris_scale = globals.eye_detail.x * select(1.0, 1.14, CINEMATIC);
    let iris_local = (local - gaze) / max(iris_scale, 0.01);
    let iris_distance = length(iris_local);
    let iris_angle = atan2(iris_local.y, iris_local.x);
    // Keep derivatives outside the non-uniform eye-mask branch, then skip the
    // harmonic/glint work for the overwhelming majority of body pixels.
    let fiber_count = select(globals.eye_detail.z, min(globals.eye_detail.z, 31.0), CINEMATIC);
    let fiber_footprint = fwidth(iris_angle) * fiber_count;
    // At desktop scale unresolved fibers must converge to the smooth mean instead
    // of temporally aliasing. The old threshold preserved high-frequency contrast
    // well below one cycle per pixel and made the iris visibly shimmer.
    let fiber_prefilter = 1.0 - smoothstep(0.24, 0.78, fiber_footprint);
    if (eye_mask <= 0.0001) {
        return vec4<f32>(body_color, 0.0);
    }
    let blink = select(globals.lids_brows.y, globals.lids_brows.x, side < 0.0);
    let aperture = lid_aperture(local, blink, globals.lids_brows.z, side);
    let visible = eye_mask * aperture;
    let lid_presence = smoothstep(0.02, 0.18, max(blink, globals.lids_brows.z * 0.48));
    let lid = eye_mask * (1.0 - aperture) * lid_presence;
    let iris = visible * (1.0 - smoothstep(0.92, 1.01, iris_distance));
    let autonomic_pupil = clamp(
        globals.gaze_pupil.w + side * globals.cinematic_i.w,
        0.15,
        0.95,
    );
    let inherited_pupil_scale = clamp(globals.face_shape.z / 0.52, 0.72, 1.28);
    let pupil_ratio = mix(0.33, 0.67, autonomic_pupil)
        * inherited_pupil_scale
        * select(1.0, 1.04, CINEMATIC);
    let pupil = iris * (1.0 - smoothstep(0.88, 1.0, iris_distance / pupil_ratio));
    let limbal = iris * smoothstep(0.84, 0.965, iris_distance);
    let fiber_wave = sin(
        iris_angle * fiber_count
        + iris_distance * 8.0
        + globals.pattern.z * 2.7,
    );
    let secondary_fiber = sin(
        iris_angle * (fiber_count * 0.43)
        - iris_distance * 12.0
        + globals.pattern.z * 1.3,
    );
    let fiber = (fiber_wave * 0.68 + secondary_fiber * 0.32)
        * globals.eye_detail.w
        * fiber_prefilter;
    let collarette = iris
        * smoothstep(0.41, 0.46, iris_distance)
        * (1.0 - smoothstep(0.53, 0.59, iris_distance));

    var sclera = mix(
        vec3<f32>(0.995, 0.997, 1.0),
        vec3<f32>(0.955, 0.970, 0.982),
        saturate(local.y) * 0.42,
    );
    if (CINEMATIC) {
        // The compositor tone-maps all organism radiance. Calibrate the neutral
        // dielectric white in HDR and divide by exposure so the post-tone sclera
        // stays white instead of becoming the previous mid-gray.
        let white_hdr = 2.75 / max(globals.cinematic_i.z, 0.25);
        sclera = vec3<f32>(white_hdr)
            * mix(1.0, 0.965, saturate(local.y) * 0.42);
    } else {
        sclera = mix(sclera, body_color, 0.10);
    }
    var iris_color = mix(secondary * 0.70, glow_color, 0.32 + globals.physiology_b.z * 0.18);
    if (CINEMATIC) {
        let inner_ember = 1.0 - smoothstep(0.18, 0.88, iris_distance);
        iris_color = mix(
            vec3<f32>(0.105, 0.024, 0.018),
            vec3<f32>(0.315, 0.060, 0.036),
            inner_ember * 0.40,
        );
    }
    if (globals.iris_hsv.w > 0.5) {
        let authored_iris = hsv_to_rgb(globals.iris_hsv.xyz);
        let radial_light = 0.72 + (1.0 - smoothstep(0.16, 0.92, iris_distance)) * 0.34;
        iris_color = authored_iris * radial_light;
    }
    if (CINEMATIC) {
        iris_color *= 1.0
            + fiber
                * smoothstep(0.18, 0.92, iris_distance)
                * (0.22 + globals.physiology_b.z * 0.10);
    } else {
        iris_color *= 1.0
            + sin(iris_angle * globals.eye_detail.z) * globals.eye_detail.w * 0.10;
    }
    iris_color = mix(iris_color, iris_color * 0.58, limbal * globals.eye_detail.y);
    if (CINEMATIC) {
        iris_color = mix(
            iris_color,
            iris_color * 0.78 + vec3<f32>(0.36, 0.075, 0.050) * 0.22,
            collarette * 0.28,
        );
    }
    var color = mix(sclera, iris_color, iris);
    color = mix(color, vec3<f32>(0.006, 0.012, 0.022), pupil);
    if (CINEMATIC) {
        let boundary = smoothstep(
            0.88 - eye_edge_footprint,
            0.985,
            eye_distance,
        ) * visible;
        let separation = boundary * smoothstep(-0.32, 0.86, -local.y + local.x * 0.10);
        let wet_contour = boundary * smoothstep(-0.28, 0.82, local.y - local.x * 0.16);
        color *= 1.0 - separation * globals.cinematic_h.x * 0.24;
        color += mix(vec3<f32>(1.13, 1.15, 1.17), body_color, 0.10)
            * wet_contour
            * globals.cinematic_h.x
            * 0.34;
    }
    let highlight_center = vec2<f32>(-0.25, 0.28) + highlight_shift;
    let highlight_scale = select(1.0, globals.cinematic_g.w, CINEMATIC);
    let highlight = visible * (1.0 - smoothstep(
        0.03 * highlight_scale,
        0.10 * highlight_scale,
        length(local - highlight_center),
    ));
    let micro_highlight = visible * (1.0 - smoothstep(
        0.012,
        0.042,
        length(local - (vec2<f32>(0.055, 0.12) + highlight_shift * 0.65)),
    ));
    let wet_highlight_color = select(
        vec3<f32>(1.15),
        mix(
            vec3<f32>(1.16, 1.18, 1.17),
            clamp(body_color, vec3<f32>(0.0), vec3<f32>(1.2)),
            globals.cinematic_f.x * 0.72,
        ),
        CINEMATIC,
    );
    color += wet_highlight_color
        * (highlight * select(1.0, 0.94, CINEMATIC)
            + micro_highlight * select(0.0, 0.46, CINEMATIC))
        * globals.physiology_b.w
        * globals.visual_detail.y;
    color = mix(color, body_color, lid);
    return vec4<f32>(color, max(visible, lid));
}

fn eye_socket_relief(point: vec2<f32>, side: f32) -> vec2<f32> {
    let local = (point - eye_center(side)) / eye_radii();
    let radius = length(local);
    let ring = smoothstep(0.94, 1.015, radius)
        * (1.0 - smoothstep(1.03, 1.20, radius));
    let lower_shadow = ring * smoothstep(-0.18, 0.88, -local.y + local.x * 0.12);
    let upper_wet = ring * smoothstep(-0.35, 0.82, local.y - local.x * 0.18);
    return vec2<f32>(lower_shadow, upper_wet);
}

fn segment_distance(point: vec2<f32>, a: vec2<f32>, b: vec2<f32>) -> f32 {
    let segment = b - a;
    let t = clamp(dot(point - a, segment) / max(dot(segment, segment), 0.00001), 0.0, 1.0);
    return length(point - (a + segment * t));
}

fn brow_distance(point: vec2<f32>, side: f32) -> f32 {
    let center = eye_center(side);
    let radii = eye_radii();
    let shape = globals.face_brows[select(1u, 0u, side < 0.0)];
    let raise = globals.lids_brows.w * 0.028;
    let base = center.y + radii.y * 1.12 + raise;
    var distance = 10.0;
    var previous = vec2<f32>(center.x - side * radii.x * 0.8, base + shape.x * 0.045);
    for (var i = 1u; i <= 8u; i += 1u) {
        let t = f32(i) / 8.0;
        let next = vec2<f32>(center.x + side * radii.x * mix(-0.8, 0.8, t),
            base + mix(shape.x, shape.y, t) * 0.045 + shape.z * 0.035 * 4.0 * t * (1.0 - t));
        distance = min(distance, segment_distance(point, previous, next));
        previous = next;
    }
    return distance / shape.w;
}

fn brow_mask(point: vec2<f32>, side: f32) -> f32 {
    return 1.0 - smoothstep(0.008, 0.016, brow_distance(point, side));
}

fn expressive_mouth_y(normalized_x: f32, curve: f32, tension: f32) -> f32 {
    let x = clamp(normalized_x, -1.0, 1.0);
    let center_arc = -curve * 0.048 * (1.0 - x * x);
    let corner_lift = curve * 0.010 * smoothstep(0.56, 1.0, abs(x));
    let pressed_center = tension * 0.004 * (1.0 - abs(x));
    return center_arc + corner_lift + pressed_center
        + mix(globals.face_mouth.y, globals.face_mouth.z, x * 0.5 + 0.5) * 0.035 * abs(x);
}

fn expressive_mouth_distance(local: vec2<f32>, width: f32, curve: f32, tension: f32) -> f32 {
    // Six short spans approximate a smooth expressive arc while retaining clear,
    // independently moving center, shoulder, and corner silhouettes.
    let p0 = vec2<f32>(-width, expressive_mouth_y(-1.0, curve, tension));
    let p1 = vec2<f32>(-width * 0.67, expressive_mouth_y(-0.67, curve, tension));
    let p2 = vec2<f32>(-width * 0.33, expressive_mouth_y(-0.33, curve, tension));
    let p3 = vec2<f32>(0.0, expressive_mouth_y(0.0, curve, tension));
    let p4 = vec2<f32>(width * 0.33, expressive_mouth_y(0.33, curve, tension));
    let p5 = vec2<f32>(width * 0.67, expressive_mouth_y(0.67, curve, tension));
    let p6 = vec2<f32>(width, expressive_mouth_y(1.0, curve, tension));
    return min(
        min(
            min(segment_distance(local, p0, p1), segment_distance(local, p1, p2)),
            segment_distance(local, p2, p3),
        ),
        min(
            segment_distance(local, p3, p4),
            min(segment_distance(local, p4, p5), segment_distance(local, p5, p6)),
        ),
    );
}

fn mouth_layer(
    point: vec2<f32>,
    body_color: vec3<f32>,
    secondary: vec3<f32>,
    glow_color: vec3<f32>,
) -> vec4<f32> {
    let center = vec2<f32>(0.0, -0.115);
    let open = globals.brow_mouth.z;
    let curve = globals.brow_mouth.w;
    let tension = globals.mouth_voice.x;
    let voice = globals.mouth_voice.z;
    let width = (0.065 + tension * 0.020) * globals.face_mouth.x;
    let height = (0.010 + open * 0.065 + voice * 0.010) * (1.0 - globals.face_mouth.w * 0.65);
    let local = point - center;
    let normalized_x = clamp(local.x / width, -1.0, 1.0);
    let expressive_y = expressive_mouth_y(normalized_x, curve, tension);
    let deformed_local = vec2<f32>(local.x, local.y - expressive_y);
    let ellipse = length(deformed_local / vec2<f32>(width, max(height, 0.006)));
    let open_gate = smoothstep(0.025, 0.13, open);
    let closed_gate = 1.0 - open_gate;
    let open_mask = (1.0 - smoothstep(0.88, 1.02, ellipse)) * open_gate;
    let line_distance = expressive_mouth_distance(local, width, curve, tension);
    let line_aa = max(fwidth(line_distance), 0.003);
    let line_mask = (1.0 - smoothstep(0.010 - line_aa, 0.010 + line_aa, line_distance)) * closed_gate;
    let feature_mask = max(open_mask, line_mask);
    let open_distance = (ellipse - 1.0) * min(width, max(height, 0.006));
    let closed_distance = line_distance - 0.010;
    let feature_distance = mix(
        closed_distance,
        open_distance,
        smoothstep(0.025, 0.13, open),
    );
    let relief = select(0.0, globals.cinematic_h.y, CINEMATIC);
    let crease_shadow = (1.0 - smoothstep(0.008, 0.026, line_distance))
        * smoothstep(-0.55, 0.72, -deformed_local.y / max(height, 0.006));
    let dark_jelly = mix(body_color, secondary, 0.10) * 0.19;
    var color = mix(secondary * (0.40 - crease_shadow * relief * 0.12), dark_jelly, open_mask);
    let lip_edge = open_mask * smoothstep(0.62, 0.93, ellipse);
    let upper_lip = lip_edge * smoothstep(-0.32, 0.82, deformed_local.y / max(height, 0.006));
    let ridge_distance = expressive_mouth_distance(
        local - vec2<f32>(0.0, 0.005),
        width,
        curve,
        tension,
    );
    let closed_wet_ridge = (1.0 - smoothstep(0.004, 0.012, ridge_distance)) * closed_gate;
    let wet_color = mix(vec3<f32>(1.08, 1.10, 1.09), secondary, 0.22);
    color += wet_color * (upper_lip * 0.34 + closed_wet_ridge * 0.20) * relief;
    if (CINEMATIC) {
        let groove = max(line_mask, crease_shadow * (1.0 - open_mask));
        let groove_base = body_color * globals.cinematic_i.x
            * (1.0 - groove * relief * 0.12);
        // The cavity is the organism's own jelly: optically thicker, darker and
        // partially transparent, without a separate plastic tongue material.
        let cavity = mix(body_color, secondary, 0.08) * 0.17;
        color = mix(groove_base, cavity, open_mask);
        let lip_shell = max(
            lip_edge,
            (1.0 - smoothstep(0.010, 0.023, line_distance)) * closed_gate,
        );
        let lip_body = jelly_relief_material(body_color, glow_color, feature_distance, 1.0);
        color = mix(color, lip_body, lip_shell * 0.62 * relief);
        color += chroma_preserving_rim(body_color, glow_color, 1.08, 1.12)
            * (upper_lip * 0.30 + closed_wet_ridge * 0.24)
            * relief;
        let groove_relief = jelly_relief_material(body_color, glow_color, feature_distance, -0.72);
        color = mix(color, groove_relief, line_mask * 0.46 * relief);
    }
    return vec4<f32>(color, max(open_mask * 0.76, line_mask));
}

fn internal_orb_radiance(
    point: vec2<f32>,
    density: f32,
    thickness: f32,
    absorption_coefficient: vec3<f32>,
    secondary: vec3<f32>,
    glow_color: vec3<f32>,
) -> vec3<f32> {
    var radiance = vec3<f32>(0.0);
    let body_mask = smoothstep(
        globals.liquid_meta.z * 0.96,
        globals.liquid_meta.z * 1.42,
        density,
    );
    let view = vec3<f32>(0.0, 0.0, 1.0);
    let light = normalize(vec3<f32>(-0.48, 0.62, 0.64));
    let half_vector = normalize(view + light);

    for (var index = 0u; index < 8u; index += 1u) {
        if (f32(index) >= globals.cinematic_a.z) {
            break;
        }
        let sphere = globals.glow_orb_position_radius[index];
        let orb_data = globals.glow_orb_color_intensity[index];
        let radius = max(sphere.z, 0.001);
        let delta = point - sphere.xy;
        let q2 = dot(delta, delta) / (radius * radius);
        if (q2 > 10.0) {
            continue;
        }

        // The core follows projected sphere chord length instead of a flat disk.
        let sphere_z = sqrt(max(1.0 - q2, 0.0));
        let chord_length = 2.0 * radius * sphere_z;
        let luminous_core = 1.0 - exp(-chord_length / max(radius * 0.62, 0.001));
        // A narrow and a broad Gaussian make a controlled optical halo without
        // Screen/Overlay blending or a default blur filter.
        let near_halo = exp(-q2 / (2.0 * 0.42 * 0.42));
        let far_halo = exp(-q2 / (2.0 * 1.18 * 1.18));
        let halo = near_halo * 0.72 + far_halo * 0.28;
        let front_transmission = exp(
            -absorption_coefficient * thickness * sphere.w * 0.72,
        );
        let sphere_normal = normalize(vec3<f32>(-delta / radius, max(sphere_z, 0.08)));
        let sphere_fresnel = 0.035 + 0.965 * pow(1.0 - max(sphere_normal.z, 0.0), 5.0);
        let sphere_studio = studio_lobes(
            normalize(reflect(-view, sphere_normal)),
            0.16,
            0.07,
        );
        let sphere_reflection = (sphere_studio.base + sphere_studio.coat + sphere_studio.fill)
            * globals.cinematic_c.x;
        let pearl_highlight = pow(saturate(dot(sphere_normal, half_vector)), 28.0)
            * step(q2, 1.0)
            * (1.0 - sphere.w * 0.55);
        let round_glint = 1.0 - smoothstep(
            0.08,
            0.24,
            length(delta / radius - vec2<f32>(-0.31, 0.34)),
        );
        var orb_color = mix(secondary, glow_color, orb_data.x);
        orb_color = mix(orb_color, vec3<f32>(1.0, 1.02, 1.04), orb_data.y);
        let profile = luminous_core * 0.28
            + halo * globals.cinematic_a.w * 0.08
            + pearl_highlight * 0.14;
        radiance += orb_color
            * orb_data.z
            * profile
            * front_transmission
            * body_mask;
        radiance += sphere_reflection
            * (0.16 + sphere_fresnel * 0.72)
            * step(q2, 1.0)
            * front_transmission
            * body_mask
            * orb_data.z;
        radiance += vec3<f32>(1.18, 1.16, 1.08)
            * round_glint
            * step(q2, 1.0)
            * front_transmission
            * body_mask
            * orb_data.z
            * 0.46;
    }
    return radiance;
}

fn soul_glow_radiance(
    point: vec2<f32>,
    density: f32,
    transmittance: vec3<f32>,
    pigment: vec3<f32>,
    glow_color: vec3<f32>,
) -> vec3<f32> {
    var field = 0.0;
    for (var index = 0u; index < 6u; index += 1u) {
        if (f32(index) >= globals.cinematic_f.y) {
            break;
        }
        let lobe = globals.soul_glow_position_radius[index];
        let radius = max(lobe.z, 0.001);
        let normalized = (point - lobe.xy) / radius;
        let distance_squared = dot(normalized, normalized);
        let feather = max(globals.cinematic_f.w, 0.20);
        let soft_lobe = 1.0 / (1.0 + distance_squared * 1.65 / (feather * feather));
        field += soft_lobe * soft_lobe * lobe.w;
    }
    // Rational soft union has metaball overlap without a hard per-lobe edge or
    // an extra transcendental per pixel.
    let metaball = field * 0.92 / (1.0 + field * 0.92);
    let body_mask = smoothstep(
        globals.liquid_meta.z * 0.92,
        globals.liquid_meta.z * 1.50,
        density,
    );
    let front_transmission = mix(vec3<f32>(1.0), transmittance, 0.28);
    let soul_color = mix(pigment, glow_color, 0.38);
    return soul_color
        * metaball
        * globals.cinematic_f.z
        * (0.72 + globals.physiology_b.x * 0.62)
        * front_transmission
        * body_mask;
}

fn rounded_surface_highlights(
    normal: vec3<f32>,
    screen_probe: vec3<f32>,
    pigment: vec3<f32>,
    glow_color: vec3<f32>,
) -> vec3<f32> {
    let screen_shift = screen_probe.xy * 1.65;
    let key_coordinate = (normal.xy - vec2<f32>(-0.34, 0.30) - screen_shift)
        / vec2<f32>(0.29, 0.18);
    let pearl_coordinate = (normal.xy - vec2<f32>(0.16, 0.45) - screen_shift * 0.72)
        / vec2<f32>(0.12, 0.12);
    let key_q = dot(key_coordinate, key_coordinate);
    let pearl_q = dot(pearl_coordinate, pearl_coordinate);
    let key = 1.0 / (1.0 + key_q * key_q * 3.8);
    let pearl = 1.0 / (1.0 + pearl_q * pearl_q * 5.2);
    let facing = smoothstep(0.16, 0.72, normal.z);
    let highlight_tint = mix(
        vec3<f32>(1.18, 1.20, 1.19),
        mix(pigment, glow_color, 0.16),
        globals.cinematic_f.x,
    );
    return highlight_tint
        * (key * 1.18 + pearl * 0.86)
        * globals.cinematic_e.w
        * screen_probe.z
        * facing;
}

@fragment
fn fragment_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let field = density_at(input.uv);
    let density = field.r;
    let iso = globals.liquid_meta.z;
    let antialias = max(fwidth(density) * 1.12, 0.006);
    let coverage = smoothstep(iso - antialias, iso + antialias, density);
    var halo_coverage = smoothstep(iso * 0.58, iso, density)
        * (1.0 - coverage)
        * globals.physiology_b.y
        * 0.32;
    if (CINEMATIC) {
        let cinematic_halo = smoothstep(iso * 0.34, iso * 0.90, density)
            * (1.0 - coverage)
            * (0.15 + globals.physiology_b.y * 0.85);
        halo_coverage = max(halo_coverage, cinematic_halo);
    }
    if (coverage + halo_coverage <= 0.0005) {
        return vec4<f32>(0.0);
    }

    let texel = vec2<f32>(globals.liquid_meta.w, globals.render_mode.w);
    let field_right = density_at(input.uv + vec2<f32>(texel.x, 0.0));
    let field_left = density_at(input.uv - vec2<f32>(texel.x, 0.0));
    let field_down = density_at(input.uv + vec2<f32>(0.0, texel.y));
    let field_up = density_at(input.uv - vec2<f32>(0.0, texel.y));
    let density_gradient = vec2<f32>(
        field_right.r - field_left.r,
        field_down.r - field_up.r,
    );
    let gradient_direction = normalize(density_gradient + vec2<f32>(0.000001, 0.0));
    let density_gradient_per_pixel = density_gradient * 0.5;
    let render_to_native = max(
        f32(textureDimensions(density_texture).x)
            / max(f32(textureDimensions(macro_texture).x) * 2.0, 1.0),
        1.0,
    );
    let silhouette_distance_pixels = max(density - iso, 0.0)
        / max(length(density_gradient_per_pixel), 0.0001)
        / render_to_native;
    let edge_gradient = 1.0 - smoothstep(
        0.0,
        globals.cinematic_c.w,
        silhouette_distance_pixels,
    );
    let edge_shell = 1.0 - smoothstep(0.0, 3.2, silhouette_distance_pixels);
    // Central differences must be divided by their UV baseline. Without this,
    // increasing render resolution flattened the reconstructed surface normal and
    // made every specular control look inert.
    var height_gradient = vec2<f32>(
        (reconstructed_height(input.uv + vec2<f32>(texel.x, 0.0))
            - reconstructed_height(input.uv - vec2<f32>(texel.x, 0.0)))
            / max(texel.x * 2.0, 0.00001),
        (reconstructed_height(input.uv + vec2<f32>(0.0, texel.y))
            - reconstructed_height(input.uv - vec2<f32>(0.0, texel.y)))
            / max(texel.y * 2.0, 0.00001),
    ) * globals.material_e.w * 0.32;
    if (CINEMATIC) {
        // v2 reconstructs the macro normal from filtered *integrated thickness*.
        // Unlike the rejected v1 core normal, this remains rounded through the
        // interior instead of saturating to a flat (0,0,1) plateau.
        height_gradient = macro_height_gradient(input.uv)
            * globals.material_e.w
            * 0.24;
    }
    let normal = normalize(vec3<f32>(-height_gradient, 1.0));
    let point = local_point(input.uv);
    // The particle solver owns UI interaction. A legacy screen-space half-plane
    // here produced the rectangular disappearing mask reported in review.
    let visibility = 1.0;
    let core = liquid_core(density);
    var thickness = field.g / max(density, 0.00001)
        * pow(core, globals.material_e.y)
        * globals.material_a.z;
    if (CINEMATIC) {
        // Never normalize G by density in the cinematic path: G is the optical
        // line integral, and preserving overlap is what makes the center thick.
        thickness = log(1.0 + max(field.g, 0.0) * 0.92)
            * globals.material_a.z
            * 1.34;
    }
    let emission_density = field.b / max(density, 0.00001);
    let pigment_density = field.a / max(density, 0.00001);
    let material_flow = material_flow_at(input.uv) / max(density, 0.00001);
    let material_coordinate = material_flow.xy;
    let material_velocity = material_flow.zw;

    let primary = hsv_to_rgb(globals.primary_hsv.xyz);
    let secondary = hsv_to_rgb(globals.secondary_hsv.xyz);
    let glow_color = hsv_to_rgb(globals.glow_hsv.xyz);
    let pigment = mix(primary, secondary, saturate(pigment_density) * 0.34);
    let light = normalize(vec3<f32>(-0.48, 0.62, 0.64));
    let fill = normalize(vec3<f32>(0.68, -0.22, 0.70));
    let view = vec3<f32>(0.0, 0.0, 1.0);
    let ndotv = saturate(dot(normal, view));
    let fresnel = globals.material_d.w
        + (1.0 - globals.material_d.w) * pow(1.0 - ndotv, 5.0);
    let fill_light = saturate(dot(normal, fill) * 0.5 + 0.5);
    let wrapped_key = saturate(
        (dot(normal, light) + globals.material_f.x)
            / (1.0 + globals.material_f.x),
    );

    // Density-gradient and mean particle velocity drive refraction; there is no
    // free-running sine distortion detached from the simulated material.
    let velocity_offset = globals.liquid_motion.xy * vec2<f32>(1.0, -1.0) * 0.00042;
    let refraction_offset = -gradient_direction
        * mix(0.0015, 0.0105, thickness)
        * globals.material_b.x
        + velocity_offset * thickness;
    let refracted_uv = input.uv + refraction_offset;
    let blur_axis = normalize(vec2<f32>(0.64, 0.77) + gradient_direction * 0.22);
    let blur_offset = blur_axis * texel * mix(0.85, 2.1, thickness) * globals.material_b.y;
    let background_center = material_background(refracted_uv);
    var background: vec4<f32>;
    if (CINEMATIC) {
        let rough_offset = texel
            * mix(1.1, 2.8, thickness)
            * globals.material_b.y;
        background = background_center * 0.36
            + material_background(refracted_uv + vec2<f32>(rough_offset.x, 0.0)) * 0.16
            + material_background(refracted_uv - vec2<f32>(rough_offset.x, 0.0)) * 0.16
            + material_background(refracted_uv + vec2<f32>(0.0, rough_offset.y)) * 0.16
            + material_background(refracted_uv - vec2<f32>(0.0, rough_offset.y)) * 0.16;
    } else {
        background = background_center * 0.50
            + material_background(refracted_uv + blur_offset) * 0.25
            + material_background(refracted_uv - blur_offset) * 0.25;
    }

    // Body-hue-preserving Beer-Lambert absorption. The reference transmittance
    // makes the thick center colored instead of dead gray glass.
    let reference_transmittance = mix(
        vec3<f32>(0.18),
        max(pigment, vec3<f32>(0.025)),
        globals.material_f.w,
    );
    let absorption_coefficient = -log(max(reference_transmittance, vec3<f32>(0.02)))
        * globals.material_a.x;
    let transmittance = exp(-absorption_coefficient * thickness);
    let scatter_amount = 1.0 - exp(-globals.material_a.y * thickness);
    let scattering = pigment
        * scatter_amount
        * (globals.material_f.y + wrapped_key * globals.material_f.z);
    let opaque_gel = pigment * (0.16 + core * 0.34 + fill_light * 0.06);
    let refracted_gel = background.rgb * transmittance + scattering;
    var volume_color = select(
        opaque_gel + scattering,
        mix(opaque_gel + scattering, refracted_gel, globals.material_a.w),
        background.a > 0.5,
    );

    let half_key = normalize(light + view);
    let half_fill = normalize(fill + view);
    let broad_specular = pow(saturate(dot(normal, half_key)), globals.material_c.y)
        * globals.material_c.x;
    let tight_specular = pow(saturate(dot(normal, half_fill)), globals.material_c.w)
        * globals.material_c.z;
    let rim = pow(1.0 - ndotv, globals.material_b.w) * globals.material_b.z;
    var color = volume_color;
    var cinematic_screen_probe = vec3<f32>(0.0, 0.0, 1.0);
    if (CINEMATIC) {
        color += internal_orb_radiance(
            point,
            density,
            thickness,
            absorption_coefficient,
            secondary,
            glow_color,
        );
        color += soul_glow_radiance(
            point,
            density,
            transmittance,
            pigment,
            glow_color,
        );
        let normal_variance = max(length(dpdx(normal)), length(dpdy(normal)));
        let base_roughness = clamp(globals.cinematic_c.y + normal_variance * 0.35, 0.05, 1.0);
        let coat_roughness = clamp(globals.cinematic_c.z + normal_variance * 0.18, 0.02, 0.50);
        cinematic_screen_probe = screen_light_probe(input.uv);
        let reflection = normalize(reflect(-view, normal) + vec3<f32>(cinematic_screen_probe.xy, 0.0));
        let studio = studio_lobes(reflection, base_roughness, coat_roughness);
        let reflection_strength = globals.cinematic_c.x * cinematic_screen_probe.z;
        let base_tint = mix(vec3<f32>(1.0), pigment, globals.cinematic_f.x * 0.72);
        let coat_tint = mix(vec3<f32>(1.0), mix(pigment, glow_color, 0.16), globals.cinematic_f.x);
        let base_environment = (studio.base * base_tint + studio.fill) * reflection_strength;
        let coat_environment = studio.coat * coat_tint * reflection_strength;
        let coat_fresnel = 0.04 + 0.96 * pow(1.0 - ndotv, 5.0);
        // Filament-style clearcoat energy compensation: the neutral wet coat
        // subtracts energy from the colored volume before adding its reflection.
        color *= 1.0 - coat_fresnel * 0.42;
        color += base_environment
            * (globals.material_c.x * 0.72 + fresnel * 1.28)
            * (1.0 - coat_fresnel * 0.36);
        color += coat_environment
            * coat_fresnel
            * (0.92 + globals.material_c.z * 0.62);
        let softbox_gate = smoothstep(
            0.20,
            0.72,
            luminance(coat_environment) / max(globals.cinematic_c.x, 0.001),
        );
        color += coat_environment
            * softbox_gate
            * (0.42 + globals.material_c.z * 0.34);
        color += rounded_surface_highlights(
            normal,
            cinematic_screen_probe,
            pigment,
            glow_color,
        );

        // Distance to the actual raw level set gives a broad, stable edge light
        // even when the reconstructed normal becomes frontal in the center.
        let back_lit = saturate(dot(-normal, light) * 0.5 + 0.5);
        let silhouette_light = pow(edge_gradient, max(globals.material_b.w * 0.42, 0.65))
            * globals.material_b.z
            * (0.62 + back_lit * 0.38);
        let broad_rim_tint = chroma_preserving_rim(
            pigment,
            glow_color,
            globals.cinematic_h.z,
            1.02,
        );
        color += broad_rim_tint
            * silhouette_light
            * globals.cinematic_e.y
            * 0.38;
        let narrow_rim_tint = chroma_preserving_rim(
            pigment,
            glow_color,
            globals.cinematic_h.z,
            1.34,
        );
        color += narrow_rim_tint
            * edge_shell
            * globals.cinematic_e.x
            * (0.68 + globals.material_b.z * 0.46)
            * (0.88 + back_lit * 0.12);

        let caustics = flowing_caustics(material_coordinate, material_velocity)
            * globals.cinematic_d.x
            * smoothstep(0.08, 0.70, core)
            * (0.38 + scatter_amount * 0.62);
        color += mix(glow_color, vec3<f32>(1.0, 0.94, 0.72), 0.28) * caustics * 0.21;
        color += mix(pigment, glow_color, 0.52)
            * globals.material_d.x
            * (0.025 + globals.physiology_b.x * 0.12)
            * pow(core, 0.62);
    } else {
        // Preserved Current / Safe incumbent.
        color += vec3<f32>(1.03, 1.08, 1.12)
            * (broad_specular + tight_specular)
            * (0.25 + fresnel * 0.75);
        color += mix(pigment, glow_color, 0.12)
            * rim
            * (0.32 + globals.face_shape.w * 0.12);
    }
    color += mix(pigment, glow_color, 0.20)
        * emission_density
        * (0.22 + globals.physiology_b.x * 0.42)
        * pow(core, 0.70)
        * globals.material_d.x;
    // The flow texture is carried by the particles themselves (field.b). This
    // changes both the simulated circulation and the contrast of that advected
    // material without introducing screen-space noise.
    let living_flow = saturate((emission_density - 0.18) * 1.28)
        * globals.flow.x;
    color += mix(pigment, glow_color, 0.62)
        * living_flow
        * (0.035 + 0.13 * globals.material_d.x)
        * pow(core, 0.82);
    color += pigment * globals.mouth_voice.w * sin(globals.viewport_time.y * 24.0 + point.y * 17.0) * 0.012;

    // Local contrast backing suppresses internal pattern only behind the features.
    let face_region = 1.0 - smoothstep(0.22, 0.34, length(face_space(point)));
    color = mix(color, color * 0.76, face_region * globals.face_tuning.x);
    let body_face_color = color;
    let face_point = face_space(point);
    let highlight_shift = select(
        vec2<f32>(0.0),
        cinematic_screen_probe.xy * 0.72,
        CINEMATIC,
    );
    let left_socket = eye_socket_relief(face_point, -1.0) * globals.face_tuning.x;
    let right_socket = eye_socket_relief(face_point, 1.0) * globals.face_tuning.x;
    if (CINEMATIC) {
        let socket = max(left_socket, right_socket);
        color *= 1.0 - socket.x * globals.cinematic_h.x * 0.34;
        color += mix(vec3<f32>(1.08, 1.10, 1.09), pigment, globals.cinematic_f.x)
            * socket.y
            * globals.cinematic_h.x
            * 0.30;
    }
    let cheek_y = eye_center(-1.0).y - eye_radii().y * 0.86;
    let cheek_left = length(
        (face_point - vec2<f32>(eye_center(-1.0).x, cheek_y)) / vec2<f32>(0.066, 0.038),
    );
    let cheek_right = length(
        (face_point - vec2<f32>(eye_center(1.0).x, cheek_y)) / vec2<f32>(0.066, 0.038),
    );
    let cheek = (1.0 - smoothstep(0.48, 1.0, min(cheek_left, cheek_right)))
        * globals.mouth_voice.y
        * coverage
        * globals.face_tuning.x;
    let cheek_color = mix(body_face_color * 1.06, glow_color * 1.12, 0.72);
    color = mix(color, cheek_color, cheek * select(0.20, 0.28, CINEMATIC));
    let left_eye = eye_layer(face_point, -1.0, color, secondary, glow_color, highlight_shift);
    let right_eye = eye_layer(face_point, 1.0, color, secondary, glow_color, highlight_shift);
    color = mix(color, left_eye.rgb, left_eye.a * coverage * globals.face_tuning.x);
    color = mix(color, right_eye.rgb, right_eye.a * coverage * globals.face_tuning.x);
    let left_brow_distance = brow_distance(face_point, -1.0);
    let right_brow_distance = brow_distance(face_point, 1.0);
    let brow_distance_field = min(left_brow_distance, right_brow_distance);
    let brow = (1.0 - smoothstep(0.008, 0.016, brow_distance_field))
        * coverage
        * globals.face_tuning.x;
    var brow_color = secondary * 0.38;
    if (CINEMATIC) {
        let brow_shadow = max(
            brow_mask(face_point + vec2<f32>(-0.004, 0.005), -1.0),
            brow_mask(face_point + vec2<f32>(-0.004, 0.005), 1.0),
        ) * coverage * globals.face_tuning.x;
        let brow_wet = max(
            brow_mask(face_point + vec2<f32>(0.003, -0.004), -1.0),
            brow_mask(face_point + vec2<f32>(0.003, -0.004), 1.0),
        ) * coverage * globals.face_tuning.x;
        color *= 1.0 - brow_shadow * globals.cinematic_h.y * 0.16;
        brow_color = jelly_relief_material(
            body_face_color,
            glow_color,
            brow_distance_field - 0.012,
            1.0,
        );
        // A softly lit contour remains legible on black jelly independently of halo.
        brow_color = max(brow_color, vec3<f32>(0.32, 0.34, 0.35));
        brow_color += chroma_preserving_rim(
            pigment,
            glow_color,
            globals.cinematic_h.z,
            1.10,
        ) * brow_wet * globals.cinematic_h.y * 0.32;
    }
    color = mix(
        color,
        brow_color,
        brow * select(0.52 + globals.brow_mouth.x * 0.18, 0.72, CINEMATIC),
    );
    let mouth = mouth_layer(face_point, body_face_color, secondary, glow_color);
    color = mix(color, mouth.rgb, mouth.a * coverage * globals.face_tuning.x);
    let face_coverage = saturate(max(max(left_eye.a, right_eye.a), max(brow, mouth.a)))
        * coverage
        * globals.face_tuning.x;

    let volume_alpha = 1.0 - exp(
        -(globals.material_a.x * 0.68 + globals.material_a.y * 0.32)
            * max(thickness, 0.03),
    );
    let membrane_alpha = fresnel * 0.28;
    let material_alpha = saturate(volume_alpha + membrane_alpha * (1.0 - volume_alpha))
        * globals.material_d.y;
    let refractive_alpha = select(material_alpha, 1.0, background.a > 0.5);
    let shape_alpha = coverage * visibility * max(refractive_alpha, face_coverage * 0.99);
    let halo_alpha = halo_coverage * visibility * 0.18 * (1.0 - shape_alpha);
    let final_alpha = saturate(shape_alpha + halo_alpha);
    // Deliberately HDR and linear. Compose unpremultiplies, tone-maps, then
    // premultiplies again, so bright wet highlights retain their shape.
    var halo_color = mix(pigment, glow_color, 0.30);
    if (CINEMATIC) {
        halo_color = mix(vec3<f32>(1.05), mix(pigment, glow_color, 0.24), 0.48) * 1.12;
    }
    let premultiplied = color * shape_alpha + halo_color * halo_alpha;
    let debug_view = globals.face_tuning.y;
    if (debug_view > 0.5) {
        var debug_color = vec3<f32>(0.0);
        var debug_alpha = final_alpha;
        if (debug_view < 1.5) {
            let normalized_density = saturate(density / max(iso * 3.0, 0.001));
            debug_color = mix(vec3<f32>(0.02, 0.10, 0.42), vec3<f32>(1.0, 0.22, 0.02), normalized_density);
            debug_alpha = max(smoothstep(iso * 0.18, iso * 0.42, density), 0.025);
        } else if (debug_view < 2.5) {
            debug_color = vec3<f32>(final_alpha);
            debug_alpha = max(final_alpha, 0.03);
        } else if (debug_view < 3.5) {
            debug_color = mix(vec3<f32>(0.04, 0.07, 0.10), vec3<f32>(1.0, 0.04, 0.70), face_coverage);
        } else if (debug_view < 4.5) {
            let debug_emission = saturate(field.b / max(density, 0.001));
            let debug_pigment = saturate(field.a / max(density, 0.001));
            debug_color = vec3<f32>(debug_pigment, debug_emission, 0.16);
        } else if (debug_view < 5.5) {
            let normalized_thickness = saturate(thickness / 2.2);
            debug_color = mix(vec3<f32>(0.015, 0.03, 0.08), vec3<f32>(0.42, 1.0, 0.72), normalized_thickness);
        } else if (debug_view < 6.5) {
            debug_color = mix(vec3<f32>(0.015, 0.018, 0.024), vec3<f32>(0.72, 1.0, 0.86), edge_gradient);
        } else if (debug_view < 7.5) {
            debug_color = normal * 0.5 + vec3<f32>(0.5);
        } else if (debug_view < 8.5) {
            let debug_studio = studio_lobes(
                normalize(reflect(-view, normal)),
                globals.cinematic_c.y,
                globals.cinematic_c.z,
            );
            // Red = gel/base lobe, green = wet-coat lobe, blue = ambient fill.
            debug_color = vec3<f32>(
                debug_studio.base_mask,
                debug_studio.coat_mask,
                luminance(debug_studio.fill) * 4.0,
            ) * globals.cinematic_c.x;
        } else {
            let debug_caustics = flowing_caustics(material_coordinate, material_velocity);
            debug_color = mix(vec3<f32>(0.01, 0.02, 0.05), glow_color, debug_caustics);
        }
        return vec4<f32>(debug_color * debug_alpha, debug_alpha);
    }
    return vec4<f32>(premultiplied, final_alpha);
}
