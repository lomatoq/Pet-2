struct ShadowFilterGlobals {
    // xy: one texel in the active blur direction; z: normalized center weight;
    // w: number of positive paired taps.
    direction_meta: vec4<f32>,
    // x: native-to-mask downsample factor; y: source-to-mask footprint.
    downsample: vec4<f32>,
    // Each vec4 stores two (fractional offset, normalized combined weight) pairs.
    paired_taps: array<vec4<f32>, 8>,
};

@group(0) @binding(0) var<uniform> globals: ShadowFilterGlobals;
@group(0) @binding(1) var source_texture: texture_2d<f32>;
@group(0) @binding(2) var source_sampler: sampler;

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

fn alpha_at(uv: vec2<f32>) -> f32 {
    return textureSample(source_texture, source_sampler, uv).a;
}

@fragment
fn downsample_fragment(input: VertexOutput) -> @location(0) f32 {
    let source_texel = 1.0 / vec2<f32>(textureDimensions(source_texture));
    // A native material target needs a 2x2 or 4x4 footprint; Body Lab's 2x
    // material target needs 4x4 or 8x8. These centered bilinear gathers are the
    // exact box averages for each integer footprint, so blur never aliases the
    // particle silhouette before the Gaussian passes.
    if (globals.downsample.y < 3.0) {
        var sum = 0.0;
        for (var y: i32 = -1; y <= 1; y += 2) {
            for (var x: i32 = -1; x <= 1; x += 2) {
                sum += alpha_at(
                    input.uv + vec2<f32>(f32(x), f32(y)) * source_texel * 0.5,
                );
            }
        }
        return sum * 0.25;
    }

    if (globals.downsample.y < 6.0) {
        var sum = 0.0;
        for (var y: i32 = -1; y <= 1; y += 2) {
            for (var x: i32 = -1; x <= 1; x += 2) {
                sum += alpha_at(input.uv + vec2<f32>(f32(x), f32(y)) * source_texel);
            }
        }
        return sum * 0.25;
    }

    var sum = 0.0;
    for (var y: i32 = -3; y <= 3; y += 2) {
        for (var x: i32 = -3; x <= 3; x += 2) {
            sum += alpha_at(input.uv + vec2<f32>(f32(x), f32(y)) * source_texel);
        }
    }
    return sum * 0.0625;
}

fn paired_tap(index: u32) -> vec2<f32> {
    let packed = globals.paired_taps[index / 2u];
    return select(packed.xy, packed.zw, (index & 1u) == 1u);
}

@fragment
fn blur_fragment(input: VertexOutput) -> @location(0) f32 {
    var alpha = textureSample(source_texture, source_sampler, input.uv).r
        * globals.direction_meta.z;
    let pair_count = u32(clamp(globals.direction_meta.w, 0.0, 16.0));
    for (var index: u32 = 0u; index < 16u; index += 1u) {
        if (index >= pair_count) {
            break;
        }
        let tap = paired_tap(index);
        let offset = globals.direction_meta.xy * tap.x;
        alpha += (
            textureSample(source_texture, source_sampler, input.uv + offset).r
            + textureSample(source_texture, source_sampler, input.uv - offset).r
        ) * tap.y;
    }
    return clamp(alpha, 0.0, 1.0);
}
