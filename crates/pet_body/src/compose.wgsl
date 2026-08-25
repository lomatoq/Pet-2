struct ComposeGlobals {
    // x: premultiplied output, y: review background, zw: native inverse size.
    output_mode: vec4<f32>,
    // xy: full-silhouette drop-shadow offset in pixels, z: opacity, w: exposure.
    shadow: vec4<f32>,
    // x: feather in output pixels, yzw: linear shadow color.
    shadow_style: vec4<f32>,
    // x: cinematic bloom strength; y: intermediate render scale; zw reserved.
    post: vec4<f32>,
};

@group(0) @binding(0) var<uniform> globals: ComposeGlobals;
@group(0) @binding(1) var organism_texture: texture_2d<f32>;
@group(0) @binding(2) var organism_sampler: sampler;
@group(0) @binding(3) var shadow_texture: texture_2d<f32>;
@group(0) @binding(4) var shadow_sampler: sampler;

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

fn review_background(uv: vec2<f32>) -> vec4<f32> {
    let mode = globals.output_mode.y;
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

fn resolved_organism(uv: vec2<f32>) -> vec4<f32> {
    // Production renders the intermediate at native resolution. Sampling it at
    // four sub-texel phases would apply an unintended 3x3 tent blur, so preserve
    // the exact texel-centered sample in that path.
    if (globals.post.y < 1.5) {
        return textureSample(organism_texture, organism_sampler, uv);
    }

    // Four high-resolution phase samples form a compact 2x box/tent resolve. This
    // filters shader-defined highlights and flow, which ordinary geometry MSAA misses.
    let high_texel = globals.output_mode.zw * 0.5;
    let offset = high_texel * 0.5;
    return (
        textureSample(organism_texture, organism_sampler, uv + vec2<f32>(-offset.x, -offset.y))
        + textureSample(organism_texture, organism_sampler, uv + vec2<f32>(offset.x, -offset.y))
        + textureSample(organism_texture, organism_sampler, uv + vec2<f32>(-offset.x, offset.y))
        + textureSample(organism_texture, organism_sampler, uv + vec2<f32>(offset.x, offset.y))
    ) * 0.25;
}

fn tone_map_reinhard_white(color: vec3<f32>, white_point: f32) -> vec3<f32> {
    let value = max(color, vec3<f32>(0.0));
    let luminance = max(dot(value, vec3<f32>(0.2126, 0.7152, 0.0722)), 0.00001);
    let white_squared = white_point * white_point;
    let mapped = luminance * (1.0 + luminance / white_squared) / (1.0 + luminance);
    return value * (mapped / luminance);
}

fn tone_map_premultiplied(hdr: vec4<f32>) -> vec4<f32> {
    // Tone mapping premultiplied RGB directly creates dark translucent fringes.
    // Restore straight HDR color, map it, then reapply the original coverage.
    if (hdr.a <= 0.00001) {
        return vec4<f32>(0.0);
    }
    let straight_hdr = hdr.rgb / hdr.a * max(globals.shadow.w, 0.01);
    return vec4<f32>(tone_map_reinhard_white(straight_hdr, 4.0) * hdr.a, hdr.a);
}

fn bright_premultiplied(sample_value: vec4<f32>) -> vec3<f32> {
    // The threshold is expressed in premultiplied linear HDR, so transparent
    // edges never invent gray bloom.
    return max(sample_value.rgb - vec3<f32>(sample_value.a * 0.72), vec3<f32>(0.0));
}

fn cinematic_bloom(uv: vec2<f32>, organism: vec4<f32>) -> vec4<f32> {
    let strength = globals.post.x;
    if (strength <= 0.0001) {
        return organism;
    }
    let offsets = array<vec2<f32>, 6>(
        vec2<f32>(-0.92, -0.22), vec2<f32>(-0.42, 0.78),
        vec2<f32>(0.24, -0.86), vec2<f32>(0.86, 0.42),
        vec2<f32>(-0.58, -0.82), vec2<f32>(0.66, 0.75),
    );
    let pixel = globals.output_mode.zw;
    var bloom = bright_premultiplied(organism) * 0.24;
    for (var index: u32 = 0u; index < 6u; index += 1u) {
        let radius = select(4.5, 9.0, index >= 4u);
        let sample_value = textureSample(
            organism_texture,
            organism_sampler,
            uv + offsets[index] * pixel * radius,
        );
        bloom += bright_premultiplied(sample_value) * select(0.11, 0.07, index >= 4u);
    }
    let bloom_rgb = bloom * strength * 0.72;
    let bloom_luminance = dot(bloom_rgb, vec3<f32>(0.2126, 0.7152, 0.0722));
    let bloom_alpha = clamp(bloom_luminance * 0.12, 0.0, 0.16) * (1.0 - organism.a);
    let alpha = clamp(organism.a + bloom_alpha, 0.0, 1.0);
    return vec4<f32>(organism.rgb + bloom_rgb, alpha);
}

fn footprint_shadow(uv: vec2<f32>) -> f32 {
    // The alpha was area-prefiltered and convolved by horizontal and vertical
    // Gaussian passes. Compose performs one lookup so a wide shadow can never
    // expose a finite set of translated organism silhouettes.
    let pixel = globals.output_mode.zw;
    let source_uv = uv - globals.shadow.xy * pixel;
    return textureSample(shadow_texture, shadow_sampler, source_uv).r;
}

@fragment
fn fragment_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let organism = tone_map_premultiplied(
        cinematic_bloom(input.uv, resolved_organism(input.uv)),
    );
    let blurred_alpha = footprint_shadow(input.uv);
    let shadow_alpha = blurred_alpha * globals.shadow.z * (1.0 - organism.a);
    let shadow_premultiplied = globals.shadow_style.yzw * shadow_alpha;
    let combined_rgb = organism.rgb + shadow_premultiplied;
    let combined_alpha = clamp(organism.a + shadow_alpha, 0.0, 1.0);
    let background = review_background(input.uv);
    if (background.a > 0.5) {
        return vec4<f32>(combined_rgb + background.rgb * (1.0 - combined_alpha), 1.0);
    }
    if (globals.output_mode.x > 0.5) {
        return vec4<f32>(combined_rgb, combined_alpha);
    }
    return vec4<f32>(combined_rgb / max(combined_alpha, 0.0001), combined_alpha);
}
