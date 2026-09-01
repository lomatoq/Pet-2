const TAU: f32 = 6.28318530718;

struct EcologyGlobals {
    // xy = inverse viewport size, z = asynchronous capture freshness.
    viewport: vec4<f32>,
    // xy = scale and zw = offset from overlay UV to capture UV.
    capture_transform: vec4<f32>,
}

@group(0) @binding(0) var<uniform> globals: EcologyGlobals;
@group(0) @binding(1) var desktop_background: texture_2d<f32>;
@group(0) @binding(2) var desktop_sampler: sampler;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) local: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) material: vec4<f32>,
}

@vertex
fn vertex_main(
    @builtin(vertex_index) vertex_index: u32,
    @location(0) center_radius: vec4<f32>,
    @location(1) color: vec4<f32>,
    @location(2) material: vec4<f32>,
) -> VertexOutput {
    var corners = array<vec2<f32>, 6>(
        vec2<f32>(-1.35, -1.35),
        vec2<f32>( 1.35, -1.35),
        vec2<f32>(-1.35,  1.35),
        vec2<f32>(-1.35,  1.35),
        vec2<f32>( 1.35, -1.35),
        vec2<f32>( 1.35,  1.35),
    );
    let local = corners[vertex_index];
    var output: VertexOutput;
    output.position = vec4<f32>(center_radius.xy + local * center_radius.zw, 0.0, 1.0);
    output.local = local;
    output.color = color;
    output.material = material;
    return output;
}

fn saturate(value: f32) -> f32 {
    return clamp(value, 0.0, 1.0);
}

fn hash11(value: f32) -> f32 {
    return fract(sin(value * 127.1 + 311.7) * 43758.5453123);
}

fn hash21(value: vec2<f32>) -> f32 {
    return fract(sin(dot(value, vec2<f32>(127.1, 311.7))) * 43758.5453123);
}

fn soft_edge(distance_from_center: f32, inner: f32, outer: f32) -> f32 {
    return 1.0 - smoothstep(inner, outer, distance_from_center);
}

struct InwardPulse {
    energy: f32,
    slope: f32,
    progress: f32,
}

fn inward_pulse(radius: f32, clock: f32, phase: f32, speed: f32) -> InwardPulse {
    let progress = fract(clock * speed + phase);
    let eased = progress * progress * (3.0 - 2.0 * progress);
    let pulse_radius = mix(1.025, 0.025, eased);
    // A pulse is born invisibly at the outside boundary. Its energy grows as it
    // travels inward, peaks on arrival, then clears before the cycle wraps.
    let birth = smoothstep(0.0, 0.145, progress);
    let clear = 1.0 - smoothstep(0.935, 1.0, progress);
    let life = birth * clear;
    let width = mix(0.135, 0.225, progress);
    let signed_distance = (radius - pulse_radius) / width;
    let bell = exp(-0.5 * signed_distance * signed_distance);
    let inward_gain = mix(0.34, 1.0, pow(progress, 0.82));
    var result: InwardPulse;
    result.energy = bell * life * inward_gain;
    result.slope = -signed_distance * bell * life * inward_gain;
    result.progress = progress;
    return result;
}

fn luminance(color: vec3<f32>) -> f32 {
    return dot(color, vec3<f32>(0.2126, 0.7152, 0.0722));
}

@fragment
fn fragment_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let kind = input.material.x;
    let radial_distance = length(input.local);
    if kind > 0.5 && kind < 1.5 {
        let activity = saturate(input.material.y);
        let time = input.material.w;
        let activity_integral = input.color.a;

        let local = input.local;
        let r = radial_distance;
        let field_mask = 1.0 - smoothstep(0.82, 1.055, r);
        let center_density = pow(saturate(1.0 - r), 1.28);

        let irregularity =
            sin(local.x * 3.7 + time * 0.23) * 0.018
            + sin(local.y * 3.1 - time * 0.19) * 0.013
            + sin((local.x + local.y) * 4.6 + time * 0.13) * 0.007;
        let warped_r = r + irregularity * field_mask;

        // Activity changes the rates continuously; it never resets either clock.
        let ripple_clock = time + activity_integral * 0.58;
        let particle_time = time + activity_integral * 0.35;
        let pulse_a = inward_pulse(warped_r, ripple_clock, 0.08, mix(0.070, 0.092, activity));
        let pulse_b = inward_pulse(warped_r, ripple_clock, 0.57, mix(0.057, 0.078, activity));
        let pulse_energy = saturate(pulse_a.energy + pulse_b.energy * 0.78);
        let pulse_slope = pulse_a.slope + pulse_b.slope * 0.72;
        let ripple_alpha = pulse_energy
            * field_mask
            * mix(0.092, 0.152, activity);

        let screen_uv = input.position.xy * globals.viewport.xy;
        let capture_uv = screen_uv * globals.capture_transform.xy
            + globals.capture_transform.zw;
        let capture_freshness = saturate(globals.viewport.z);
        let base_background = textureSample(
            desktop_background,
            desktop_sampler,
            clamp(capture_uv, vec2<f32>(0.0), vec2<f32>(1.0)),
        ).rgb;

        let radial_direction = local / max(r, 0.035);
        let tangent = vec2<f32>(-radial_direction.y, radial_direction.x);
        let convective =
            sin(local.y * 8.7 + time * 1.31)
            * sin(local.x * 5.3 - time * 0.77);
        let distortion_energy = field_mask
            * saturate(pulse_energy * 0.82 + abs(pulse_slope) * 0.64);
        let distortion_px = radial_direction * pulse_slope * mix(2.7, 4.8, activity)
            + tangent * convective * distortion_energy * mix(1.0, 1.85, activity);
        let capture_offset = distortion_px
            * globals.viewport.xy
            * globals.capture_transform.xy
            * capture_freshness;
        // Three nearby samples make a restrained spectral split at the hot-air
        // edge without turning readable desktop content into an RGB ghost.
        let sample_r = textureSample(
            desktop_background,
            desktop_sampler,
            clamp(capture_uv + capture_offset * 1.16, vec2<f32>(0.0), vec2<f32>(1.0)),
        ).r;
        let sample_g = textureSample(
            desktop_background,
            desktop_sampler,
            clamp(capture_uv + capture_offset, vec2<f32>(0.0), vec2<f32>(1.0)),
        ).g;
        let sample_b = textureSample(
            desktop_background,
            desktop_sampler,
            clamp(capture_uv + capture_offset * 0.82, vec2<f32>(0.0), vec2<f32>(1.0)),
        ).b;
        let refracted = vec3<f32>(sample_r, sample_g, sample_b);
        let refract_alpha = capture_freshness
            * distortion_energy
            * mix(0.46, 0.68, activity);

        let background_luma = luminance(base_background);
        let purple_presence = saturate(
            (base_background.r + base_background.b) * 0.5 - base_background.g + 0.08,
        );
        let light_contrast = vec3<f32>(0.87, 0.95, 1.00);
        let dark_contrast = vec3<f32>(0.19, 0.055, 0.32);
        var adaptive_tint = mix(
            light_contrast,
            dark_contrast,
            smoothstep(0.42, 0.72, background_luma),
        );
        adaptive_tint = mix(
            adaptive_tint,
            vec3<f32>(0.54, 0.92, 1.00),
            purple_presence * 0.58,
        );
        let tint = mix(
            vec3<f32>(0.88, 0.95, 1.00),
            adaptive_tint,
            capture_freshness * 0.72,
        );
        let ripple_tint = mix(
            adaptive_tint,
            vec3<f32>(0.47, 0.21, 0.91),
            0.34 + activity * 0.16,
        );
        let tint_alpha = field_mask
            * (0.20 + center_density * 0.80)
            * mix(0.030, 0.058, activity);

        // Multiplicative sub-LSB dither preserves exact zero at pulse birth, so
        // no static contour can pre-announce a new ripple on the outer edge.
        let dither = (hash21(floor(input.position.xy)) - 0.5) / 255.0 * field_mask;
        let dithered_tint_alpha = tint_alpha * (1.0 + dither * 18.0);
        let dithered_ripple_alpha = ripple_alpha * (1.0 + dither * 10.0);

        var particle_rgb = vec3<f32>(0.0);
        var particle_alpha = 0.0;
        for (var i: u32 = 0u; i < 12u; i = i + 1u) {
            let fi = f32(i);
            let seed_a = hash11(fi + 1.17);
            let seed_b = hash11(fi + 9.41);
            let seed_c = hash11(fi + 17.83);
            let seed_d = hash11(fi + 31.29);

            let base_speed = mix(0.082, 0.172, seed_b);
            let progress = fract(particle_time * base_speed + seed_a);
            let appear = smoothstep(0.00, mix(0.08, 0.15, seed_c), progress);
            let shrink = 1.0 - smoothstep(mix(0.78, 0.88, seed_b), 0.985, progress);
            let scale = appear * shrink;

            let start_radius = mix(0.985, 1.055, seed_c);
            let inward_progress = progress * progress * (3.0 - 2.0 * progress);
            let radius = mix(start_radius, mix(0.035, 0.12, seed_b), inward_progress);
            let base_angle = seed_d * TAU;
            let angular_drift = sin(progress * TAU + seed_a * 5.0)
                    * mix(0.10, 0.28, seed_b)
                + sin(progress * mix(3.1, 5.2, seed_d) + seed_c * 8.0) * 0.065;
            let angle = base_angle + angular_drift;
            let particle_position = vec2<f32>(cos(angle), sin(angle)) * radius;

            let base_size = mix(0.0075, 0.0165, hash11(fi + 47.2));
            let particle_size = max(base_size * scale, 0.0002);
            let particle_distance = length(local - particle_position);
            let core = (1.0 - smoothstep(
                particle_size * 0.18,
                particle_size,
                particle_distance,
            )) * mix(0.042, 0.070, seed_c) * field_mask * scale;
            let halo_radius = particle_size * mix(4.8, 6.5, seed_b);
            let halo = pow(
                saturate(1.0 - particle_distance / max(halo_radius, 0.001)),
                2.25,
            ) * mix(0.010, 0.021, seed_d) * field_mask * scale;
            let particle_energy = core + halo;
            let particle_tint = mix(
                tint,
                vec3<f32>(0.62, 0.48, 1.00),
                seed_a * 0.42,
            );
            particle_rgb += particle_tint * particle_energy;
            particle_alpha += particle_energy;
        }

        let arrival_energy = max(
            pulse_a.energy * smoothstep(0.72, 0.92, pulse_a.progress),
            pulse_b.energy * smoothstep(0.72, 0.92, pulse_b.progress),
        );
        let center_glow = pow(saturate(1.0 - r / 0.38), 2.15)
            * (mix(0.007, 0.022, activity)
                + arrival_energy * mix(0.030, 0.060, activity));
        let light_alpha = dithered_tint_alpha
            + dithered_ripple_alpha
            + particle_alpha
            + center_glow;
        let alpha = saturate(refract_alpha + light_alpha * (1.0 - refract_alpha));
        let light_rgb = tint * (dithered_tint_alpha + center_glow)
            + ripple_tint * dithered_ripple_alpha
            + particle_rgb;
        let rgb = refracted * refract_alpha + light_rgb * (1.0 - refract_alpha);
        return vec4<f32>(rgb, alpha);
    }

    let wobble = sin(atan2(input.local.y, input.local.x) * 3.0 + input.material.w * 1.7) * 0.025;
    let body = soft_edge(radial_distance + wobble, 0.88, 1.02);
    let halo = soft_edge(radial_distance, 1.03, 1.31) * (1.0 - body) * input.material.y * 0.34;
    let highlight_center = vec2<f32>(-0.30, 0.32);
    let highlight = soft_edge(distance(input.local, highlight_center), 0.08, 0.34);
    let core = soft_edge(radial_distance, 0.0, 0.88);
    let alpha = clamp(body * input.color.a + halo, 0.0, 1.0);
    var rgb = input.color.rgb * (0.72 + core * 0.34);
    rgb += vec3<f32>(0.62, 0.78, 0.92) * highlight * body * 0.52;
    rgb += input.color.rgb * halo * 0.72;
    return vec4<f32>(rgb * alpha, alpha);
}
