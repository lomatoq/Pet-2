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
};

@group(0) @binding(0) var<uniform> globals: Globals;
@group(0) @binding(1) var density_texture: texture_2d<f32>;
@group(0) @binding(2) var density_sampler: sampler;

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

fn field_at(uv: vec2<f32>) -> vec4<f32> {
    return textureSample(
        density_texture,
        density_sampler,
        clamp(uv, vec2<f32>(0.0), vec2<f32>(1.0)),
    );
}

@fragment
fn fragment_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let source_size = vec2<f32>(textureDimensions(density_texture));
    let texel = 1.0 / max(source_size, vec2<f32>(1.0));
    let radius = mix(1.25, 4.25, clamp(globals.cinematic_a.y, 0.0, 1.0));
    // One wide cross is intentionally cheaper than the old 13-tap Poisson
    // kernel. The source is already a smooth particle field, so the extra ring
    // only spent bandwidth without changing the macro silhouette.
    return field_at(input.uv) * 0.46
        + field_at(input.uv + vec2<f32>(radius * texel.x, 0.0)) * 0.135
        + field_at(input.uv - vec2<f32>(radius * texel.x, 0.0)) * 0.135
        + field_at(input.uv + vec2<f32>(0.0, radius * texel.y)) * 0.135
        + field_at(input.uv - vec2<f32>(0.0, radius * texel.y)) * 0.135;
}
