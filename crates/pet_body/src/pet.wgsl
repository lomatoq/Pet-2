struct Globals {
    view_projection: mat4x4<f32>,
    time_arousal_blink_glow: vec4<f32>,
    pattern: vec4<f32>,
};

@group(0) @binding(0) var<uniform> globals: Globals;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec4<f32>,
    @location(3) part: f32,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec4<f32>,
    @location(3) part: f32,
};

@vertex
fn vertex_main(input: VertexInput) -> VertexOutput {
    var position = input.position;
    let time = globals.time_arousal_blink_glow.x;
    let arousal = globals.time_arousal_blink_glow.y;
    if (input.part >= 2.0 && input.part < 4.0) {
        let side = select(-1.0, 1.0, input.part >= 3.0);
        position.z += sin(time * (5.0 + arousal * 8.0) + position.x * side * 7.0) * (0.025 + arousal * 0.04);
    }
    if (input.part >= 7.0 && input.part < 8.0) {
        position.x += sin(time * 1.8 - position.y * 8.0) * 0.025;
    }
    if (input.part < 2.0) {
        let breathing_scale = 1.0 + sin(time * 2.0) * 0.008;
        position.x *= breathing_scale;
        position.z *= breathing_scale;
    }
    var output: VertexOutput;
    output.clip_position = globals.view_projection * vec4<f32>(position, 1.0);
    output.world_position = position;
    output.normal = normalize(input.normal);
    output.color = input.color;
    output.part = input.part;
    return output;
}

fn hash21(point: vec2<f32>) -> f32 {
    return fract(sin(dot(point, vec2<f32>(127.1, 311.7)) + globals.pattern.z) * 43758.5453);
}

@fragment
fn fragment_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let light = normalize(vec3<f32>(-0.35, 0.65, 0.68));
    let diffuse = max(dot(normalize(input.normal), light), 0.0);
    let toon = select(select(0.56, 0.78, diffuse > 0.30), 1.05, diffuse > 0.68);
    let rim = pow(1.0 - abs(normalize(input.normal).z), 2.4);
    var warped = input.world_position.xy * globals.pattern.x;
    warped += vec2<f32>(sin(warped.y * 2.1), cos(warped.x * 1.7)) * 0.18;
    let stripe = 0.5 + 0.5 * sin((warped.x + warped.y) * 4.0 + globals.pattern.z);
    let spot = smoothstep(0.44, 0.60, hash21(floor(warped * 2.0)) * stripe);
    let pattern_mix = spot * globals.pattern.y * 0.22;
    var rgb = input.color.rgb * (toon + rim * 0.16);
    rgb = mix(rgb, rgb * vec3<f32>(0.72, 0.86, 1.16), pattern_mix);
    if (input.part >= 4.0 && input.part < 6.0) {
        let eye_glint = pow(max(dot(normalize(input.normal), normalize(vec3<f32>(-0.4, 0.5, 1.0))), 0.0), 18.0);
        rgb += vec3<f32>(eye_glint * 0.85 + globals.time_arousal_blink_glow.w * 0.12);
    }
    if (input.part >= 10.0) {
        rgb += input.color.rgb * globals.time_arousal_blink_glow.w * 0.35;
    }
    let blink = globals.time_arousal_blink_glow.z;
    let eye_alpha = select(1.0, 1.0 - blink * 0.92, input.part >= 4.0 && input.part < 6.0);
    return vec4<f32>(rgb, input.color.a * eye_alpha);
}
