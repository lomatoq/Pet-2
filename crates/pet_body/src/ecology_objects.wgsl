const TAU: f32 = 6.28318530718;
override PREMULTIPLIED_OUTPUT: f32 = 1.0;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) local: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) material: vec4<f32>,
    @location(3) screen_uv: vec2<f32>,
    @location(4) local_to_screen: vec2<f32>,
    @location(5) den_surface: vec4<f32>,
    @location(6) den_optics: vec4<f32>,
    @location(7) den_particles: vec4<f32>,
    @location(8) den_noise: vec4<f32>,
    @location(9) den_material: vec4<f32>,
    @location(10) den_mask: vec4<f32>,
    @location(11) background_uv_rect: vec4<f32>,
}

@group(0) @binding(0) var desktop_background: texture_2d<f32>;
@group(0) @binding(1) var desktop_background_sampler: sampler;

struct ReferenceBackgroundVertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) screen_uv: vec2<f32>,
}

@vertex
fn reference_background_vertex(
    @builtin(vertex_index) vertex_index: u32,
) -> ReferenceBackgroundVertexOutput {
    var positions = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>( 3.0, -1.0),
        vec2<f32>(-1.0,  3.0),
    );
    let position = positions[vertex_index];
    var output: ReferenceBackgroundVertexOutput;
    output.position = vec4<f32>(position, 0.0, 1.0);
    output.screen_uv = position * vec2<f32>(0.5, -0.5) + vec2<f32>(0.5);
    return output;
}

@fragment
fn reference_background_fragment(
    input: ReferenceBackgroundVertexOutput,
) -> @location(0) vec4<f32> {
    let color = textureSample(
        desktop_background,
        desktop_background_sampler,
        clamp(input.screen_uv, vec2<f32>(0.001), vec2<f32>(0.999)),
    );
    return vec4<f32>(color.rgb, 1.0);
}

@vertex
fn vertex_main(
    @builtin(vertex_index) vertex_index: u32,
    @location(0) center_radius: vec4<f32>,
    @location(1) color: vec4<f32>,
    @location(2) material: vec4<f32>,
    @location(3) den_surface: vec4<f32>,
    @location(4) den_optics: vec4<f32>,
    @location(5) den_particles: vec4<f32>,
    @location(6) den_noise: vec4<f32>,
    @location(7) den_material: vec4<f32>,
    @location(8) den_mask: vec4<f32>,
    @location(9) background_uv_rect: vec4<f32>,
) -> VertexOutput {
    var corners = array<vec2<f32>, 6>(
        vec2<f32>(-1.50, -1.50),
        vec2<f32>( 1.50, -1.50),
        vec2<f32>(-1.50,  1.50),
        vec2<f32>(-1.50,  1.50),
        vec2<f32>( 1.50, -1.50),
        vec2<f32>( 1.50,  1.50),
    );
    let local = corners[vertex_index];
    var output: VertexOutput;
    output.position = vec4<f32>(center_radius.xy + local * center_radius.zw, 0.0, 1.0);
    output.local = local;
    output.color = color;
    output.material = material;
    output.screen_uv = output.position.xy * vec2<f32>(0.5, -0.5) + vec2<f32>(0.5);
    output.local_to_screen = center_radius.zw * vec2<f32>(0.5, -0.5);
    output.den_surface = den_surface;
    output.den_optics = den_optics;
    output.den_particles = den_particles;
    output.den_noise = den_noise;
    output.den_material = den_material;
    output.den_mask = den_mask;
    output.background_uv_rect = background_uv_rect;
    return output;
}

fn saturate(value: f32) -> f32 {
    return clamp(value, 0.0, 1.0);
}

fn maximum_channel(value: vec3<f32>) -> f32 {
    return max(value.r, max(value.g, value.b));
}

// Finds the smallest source-over alpha that can reproduce `target` over
// `background` with a legal non-negative premultiplied source color. Unlike an
// opaque captured disc, this allocates coverage only where displaced pixels
// genuinely differ from the undistorted capture.
fn minimum_reconstruction_alpha(background: vec3<f32>, desired: vec3<f32>) -> f32 {
    let epsilon = vec3<f32>(0.0001);
    let darker = max(
        (background - desired) / max(background, epsilon),
        vec3<f32>(0.0),
    );
    let lighter = max(
        (desired - background) / max(vec3<f32>(1.0) - background, epsilon),
        vec3<f32>(0.0),
    );
    return saturate(max(maximum_channel(darker), maximum_channel(lighter)));
}

// The Lab usually draws over an opaque reference target while the desktop app
// draws into a transparent native composition surface. Encode every ecology
// fragment in the convention selected for that actual surface so the two paths
// perform the same source-over operation.
fn encode_surface_output(premultiplied: vec4<f32>) -> vec4<f32> {
    if PREMULTIPLIED_OUTPUT > 0.5 {
        return premultiplied;
    }
    if premultiplied.a <= 0.00001 {
        return vec4<f32>(0.0);
    }
    return vec4<f32>(premultiplied.rgb / premultiplied.a, premultiplied.a);
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

fn noise21(point: vec2<f32>) -> f32 {
    let cell = floor(point);
    let fraction = fract(point);
    let curve = fraction * fraction * (3.0 - 2.0 * fraction);
    let a = hash21(cell);
    let b = hash21(cell + vec2<f32>(1.0, 0.0));
    let c = hash21(cell + vec2<f32>(0.0, 1.0));
    let d = hash21(cell + vec2<f32>(1.0, 1.0));
    return mix(mix(a, b, curve.x), mix(c, d, curve.x), curve.y);
}

fn den_energy_sample(sample_point: vec2<f32>, direction: vec2<f32>, noise: vec4<f32>) -> f32 {
    let broad_warp = noise21(sample_point * 0.52 + vec2<f32>(5.31, 9.17)) * 2.0 - 1.0;
    let warped_point = sample_point + direction * broad_warp * noise.z;
    let inward_primary = noise21(warped_point + vec2<f32>(7.13, 3.71)) * 2.0 - 1.0;
    let inward_detail = noise21(warped_point * noise.x + vec2<f32>(2.41, 8.17)) * 2.0 - 1.0;
    return mix(inward_primary, inward_detail, noise.y);
}

fn den_converging_energy_noise(
    point: vec2<f32>,
    phase: f32,
    noise_size: f32,
    inward_speed: f32,
    noise: vec4<f32>,
) -> f32 {
    let radius = length(point);
    let direction = normalize(point + vec2<f32>(0.0001, 0.0));
    // Radial phase controls only inward travel. Angular scale independently
    // controls packet width, so broadening the field never averages unrelated
    // samples away or reduces its amplitude.
    let frequency = 3.25 / max(noise_size, 0.20);
    let radial_coordinate = (radius + phase * inward_speed * 3.2) * frequency;
    let width_fraction = saturate(noise.w * 0.01);
    // Log interpolation makes the whole 0..100 control useful: roughly ten
    // angular cells at zero, two at the midpoint, and one broad packet at 100.
    let angular_cells = exp2(mix(3.321928, -1.514573, width_fraction));
    let radial_axis = normalize(vec2<f32>(0.82, -0.57));
    let sample_point = direction * angular_cells + radial_axis * radial_coordinate;
    return den_energy_sample(sample_point, direction, noise);
}

fn den_inward_ring(
    point: vec2<f32>,
    phase: f32,
    lane_offset: f32,
    maximum_radius: f32,
) -> f32 {
    let radius = length(point);
    let angle = atan2(point.y, point.x);
    let irregularity = (
        sin(angle * 3.0 + 0.7) * 0.016
        + sin(angle * 5.0 + 2.1) * 0.009
    ) * maximum_radius;
    let progress = fract(phase + lane_offset);
    // Spawn and finish the opacity ramp outside the visible 1.055R material
    // mask. The already-formed ring then crosses the outer edge and travels
    // continuously inward; it never appears in place on the boundary.
    let ring_radius = mix(maximum_radius * 1.35, maximum_radius * 0.20, progress);
    let half_width = maximum_radius * 0.115;
    let distance_to_ring = abs(radius + irregularity - ring_radius);
    let band = 1.0 - smoothstep(half_width * 0.30, half_width, distance_to_ring);
    // Both sides of the sawtooth are zero before it wraps, so a new ring never
    // pops into existence and the arriving ring never flashes at the centre.
    let spawn = smoothstep(0.00, 0.10, progress);
    let absorb = 1.0 - smoothstep(0.82, 1.00, progress);
    return band * spawn * absorb;
}

fn den_concentric_ripple(point: vec2<f32>, phase: f32, maximum_radius: f32) -> f32 {
    let primary = den_inward_ring(point, phase, 0.0, maximum_radius);
    let secondary = den_inward_ring(point, phase, 0.53, maximum_radius) * 0.62;
    return max(primary, secondary);
}

fn den_surface_height(
    point: vec2<f32>,
    noise_phase: f32,
    ripple_phase: f32,
    surface: vec4<f32>,
    noise: vec4<f32>,
    noise_mix: f32,
    maximum_radius: f32,
) -> f32 {
    let inward_energy = den_converging_energy_noise(
        point,
        noise_phase,
        surface.x,
        surface.z,
        noise,
    );
    let ripple = den_concentric_ripple(point, ripple_phase, maximum_radius);
    let ripple_height = ripple * surface.w;
    let noise_height = inward_energy * surface.y;
    return mix(ripple_height, noise_height, noise_mix);
}

fn den_virtual_backdrop(point: vec2<f32>, phase: f32) -> vec3<f32> {
    let broad = 0.5 + 0.5 * sin(dot(point, vec2<f32>(0.72, -0.46)) * TAU + phase * 0.11);
    let soft = 0.5 + 0.5 * sin(dot(point, vec2<f32>(-0.34, 0.78)) * TAU - phase * 0.07 + 2.1);
    let luminance = saturate(0.22 + broad * 0.46 + soft * 0.20);
    return mix(vec3<f32>(0.18, 0.25, 0.38), vec3<f32>(0.57, 0.68, 0.79), luminance);
}

@fragment
fn fragment_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let kind = input.material.x;
    let radial_distance = length(input.local);
    if kind > 0.5 && kind < 1.5 {
        let activity = saturate(input.material.y);
        // Orb proximity is a subtle acknowledgement, never a second visual
        // mode. Bound every reactive endpoint to 22% of its authored range.
        let reactive_activity = activity * 0.22;
        let familiarity = saturate(input.material.z);
        let time = input.material.w;
        let activity_integral = input.color.a;
        let orb_speedup_fraction = saturate(input.color.b);
        let surface = input.den_surface;
        let optics = input.den_optics;
        let particles = input.den_particles;
        let noise = input.den_noise;
        let den_material = input.den_material;
        let center_mask_settings = input.den_mask;
        let displacement_radius = particles.w;

        let local = input.local;
        let r = radial_distance;
        // The concentric ripple remains the primary form; a much coarser,
        // inward-only noise field rides over it as an energy layer.
        // Never multiply absolute runtime by a changing speed. That creates a
        // phase jump proportional to app uptime when an orb crosses the activity
        // boundary, perceived as a sudden fast-forward. Integrating activity on
        // the CPU keeps phase continuous and adds only a small bounded rate lift.
        let noise_phase = time * 0.34
            + activity_integral * 0.34 * orb_speedup_fraction;
        let ripple_phase = (time * 0.42
            + activity_integral * 0.42 * orb_speedup_fraction) * input.color.g;
        let inward_energy = den_converging_energy_noise(
            local,
            noise_phase,
            surface.x,
            surface.z,
            noise,
        );
        let concentric = den_concentric_ripple(local, ripple_phase, displacement_radius);
        let height = den_surface_height(
            local,
            noise_phase,
            ripple_phase,
            surface,
            noise,
            den_material.w,
            displacement_radius,
        );
        let epsilon = center_mask_settings.w;
        let gradient = vec2<f32>(
            den_surface_height(local + vec2<f32>(epsilon, 0.0), noise_phase, ripple_phase, surface, noise, den_material.w, displacement_radius)
                - den_surface_height(local - vec2<f32>(epsilon, 0.0), noise_phase, ripple_phase, surface, noise, den_material.w, displacement_radius),
            den_surface_height(local + vec2<f32>(0.0, epsilon), noise_phase, ripple_phase, surface, noise, den_material.w, displacement_radius)
                - den_surface_height(local - vec2<f32>(0.0, epsilon), noise_phase, ripple_phase, surface, noise, den_material.w, displacement_radius),
        ) / (epsilon * 2.0);
        let displacement_strength = mix(0.125, 0.145, reactive_activity) * optics.x;
        let displaced_local = local + gradient * displacement_strength;
        let displaced_r = length(displaced_local);
        let outer_mask = 1.0 - smoothstep(
            displacement_radius * 0.78,
            displacement_radius * 1.055,
            r,
        );
        let center_reveal = smoothstep(
            center_mask_settings.x,
            center_mask_settings.x + center_mask_settings.y,
            displaced_r,
        );
        let center_mask = mix(1.0, center_reveal, center_mask_settings.z);
        let field_mask = outer_mask * center_mask;
        let center_density = pow(saturate(1.0 - displaced_r), 1.32);
        let normal = normalize(gradient + normalize(local + vec2<f32>(0.0001, 0.0)) * 0.28);

        // Sample the actual screen crop behind the transparent overlay. The
        // vertex-provided UV is screen-global, so the den bends stationary
        // desktop content instead of a synthetic color field. A quiet analytic
        // backdrop remains only as a portable/stale-capture fallback.
        // Fade the sample coordinates to the undistorted pixel with the same
        // broad envelope used by coverage. High displacement therefore cannot
        // pull alternating dark/light pixels into a spiky circular seam.
        let optical_edge_fade = smoothstep(0.0, 1.0, outer_mask);
        // Height is signed and crosses zero as the inward crest travels. Using
        // it as the carrier made the desktop displacement disappear for part
        // of every cycle, then abruptly become legible again. The gradient
        // already supplies the continuously moving direction; keep a modest
        // optical floor and let absolute height modulate only its strength.
        let optical_carrier = 0.28 + saturate(abs(height)) * 0.72;
        let optical_shift_local = normal * optical_carrier
            * mix(0.120, 0.145, reactive_activity)
            * optics.x * optical_edge_fade;
        let chroma_split_local = normal * mix(0.032, 0.044, reactive_activity) * optics.z
            * (0.35 + abs(height) * 0.65) * optical_edge_fade;
        let capture_extent = max(input.background_uv_rect.zw, vec2<f32>(0.0001));
        let capture_uv = (input.screen_uv - input.background_uv_rect.xy) / capture_extent;
        let optical_shift = optical_shift_local * input.local_to_screen / capture_extent;
        let chroma_split = chroma_split_local * input.local_to_screen / capture_extent;
        let capture_freshness = saturate(input.color.r);
        let captured_base = textureSample(
            desktop_background,
            desktop_background_sampler,
            clamp(capture_uv, vec2<f32>(0.001), vec2<f32>(0.999)),
        ).rgb;
        let capture_red = textureSample(
            desktop_background,
            desktop_background_sampler,
            clamp(capture_uv + optical_shift + chroma_split, vec2<f32>(0.001), vec2<f32>(0.999)),
        ).r;
        let capture_green = textureSample(
            desktop_background,
            desktop_background_sampler,
            clamp(capture_uv + optical_shift, vec2<f32>(0.001), vec2<f32>(0.999)),
        ).g;
        let capture_blue = textureSample(
            desktop_background,
            desktop_background_sampler,
            clamp(capture_uv + optical_shift - chroma_split, vec2<f32>(0.001), vec2<f32>(0.999)),
        ).b;
        let captured_refracted = vec3<f32>(capture_red, capture_green, capture_blue);
        let fallback_red = den_virtual_backdrop(displaced_local + optical_shift_local + chroma_split_local, noise_phase).r;
        let fallback_green = den_virtual_backdrop(displaced_local + optical_shift_local, noise_phase).g;
        let fallback_blue = den_virtual_backdrop(displaced_local + optical_shift_local - chroma_split_local, noise_phase).b;
        let fallback_refracted = vec3<f32>(fallback_red, fallback_green, fallback_blue);
        let refracted = mix(fallback_refracted, captured_refracted, capture_freshness);
        let fallback_base = den_virtual_backdrop(displaced_local, noise_phase);
        let background_reference = mix(fallback_base, captured_base, capture_freshness);
        // Captured pixels must actually cover the undistorted desktop below the
        // transparent overlay; otherwise adding a few percent of displaced
        // color reads as tint, not refraction. Keep the synthetic fallback
        // subtle, but make a fresh real capture optically legible.
        // Refraction is material presence, not proximity feedback.  Keeping
        // this independent of activity prevents the den from appearing only
        // while the creature or an orb crosses it.
        let refraction_opacity = mix(0.052, 0.58, capture_freshness)
            * optics.y * den_material.x;
        let refract_alpha = min(field_mask
            * (0.58 + center_density * 0.42)
            * refraction_opacity, 0.90);

        let tint = mix(
            vec3<f32>(0.24, 0.48, 0.70),
            vec3<f32>(0.50, 0.28, 0.68),
            saturate(familiarity * 0.34 + reactive_activity * 0.18),
        );
        let caustic = pow(saturate(0.58 + height * 0.42), 2.1)
            * (0.35 + saturate(length(gradient) * 0.18) * 0.65);
        let caustic_tint = mix(
            vec3<f32>(0.30, 0.70, 0.94),
            vec3<f32>(0.86, 0.38, 0.96),
            saturate(0.5 + normal.x * 0.5),
        );
        let caustic_alpha = caustic
            * field_mask
            * (0.30 + center_density * 0.70)
            * mix(0.006, 0.016, reactive_activity)
            * den_material.z;
        let tint_alpha = field_mask
            * (0.20 + center_density * 0.80)
            * mix(0.006, 0.015, reactive_activity)
            * den_material.y;

        // A narrow crest travels inward. The old 0.22 floor modulated almost
        // the whole den at once, which looked like rapid global flicker on a
        // dark transparent desktop even though the checkerboard Lab hid it.
        let ripple_crest = concentric;
        // The moving light/noise is absorbed by a soft sink at exactly 20% of
        // the authored maximum displacement radius instead of flashing through
        // the centre and spawning a new visible ring there.
        let inward_sink = smoothstep(
            displacement_radius * 0.20,
            displacement_radius * 0.27,
            displaced_r,
        );
        // The authored centre mask belongs to the quiet lens/material base.
        // The travelling energy must cross that base visibly on its way to the
        // 0.20R sink; otherwise only the outer rim is ever seen and it reads as
        // an alpha flash instead of inward motion.
        let travelling_mask = outer_mask * inward_sink;
        let ripple_alpha = ripple_crest
            * travelling_mask
            * (0.58 + center_density * 0.42)
            * mix(0.050, 0.095, reactive_activity)
            * particles.x;
        // Blue at the outside, violet as the crest converges. Keeping this
        // spatial rather than time-global prevents a second brightness pulse.
        let ripple_color_phase = saturate(
            1.0 - displaced_r / max(displacement_radius, 0.001),
        );
        let ripple_tint = mix(
            vec3<f32>(0.12, 0.38, 0.74),
            vec3<f32>(0.42, 0.16, 0.68),
            ripple_color_phase,
        );
        let energy_visibility = pow(saturate(0.5 + inward_energy * 0.5), 1.35);
        let energy_alpha = energy_visibility
            * travelling_mask
            * (0.24 + center_density * 0.76)
            * mix(0.030, 0.060, reactive_activity)
            * optics.w;
        let energy_tint = mix(
            vec3<f32>(0.12, 0.34, 0.58),
            vec3<f32>(0.42, 0.18, 0.58),
            saturate(0.5 + inward_energy * 0.5),
        );

        // Stable screen-space sub-LSB dither breaks 8-bit gradient quantization
        // without temporal shimmer. Apply it to premultiplied energy and alpha
        // together so blending remains correct on light and dark backdrops.
        let dither_unit = (hash21(floor(input.position.xy)) - 0.5) / 255.0;
        let dither = dither_unit * field_mask;
        let dithered_tint_alpha = max(tint_alpha + dither * 0.72, 0.0);
        let dithered_caustic_alpha = max(caustic_alpha + dither * 0.28, 0.0);
        let dithered_ripple_alpha = max(
            ripple_alpha + dither_unit * 0.18 * travelling_mask,
            0.0,
        );

        let particle_time = time + activity_integral * orb_speedup_fraction;

        var particle_rgb = vec3<f32>(0.0);
        var particle_alpha = 0.0;
        for (var i: u32 = 0u; i < 24u; i = i + 1u) {
            let fi = f32(i);
            if (fi >= particles.z) {
                continue;
            }
            let seed_a = hash11(fi + 1.17);
            let seed_b = hash11(fi + 9.41);
            let seed_c = hash11(fi + 17.83);
            let seed_d = hash11(fi + 31.29);

            let base_speed = mix(0.115, 0.195, seed_b);
            let progress = fract(particle_time * base_speed + seed_a);
            let appear = smoothstep(0.00, 0.08, progress);
            let shrink = 1.0 - smoothstep(0.88, 1.00, progress);
            let scale = appear * shrink;

            let start_radius = mix(0.46, 1.02, seed_c);
            let radius = start_radius * pow(1.0 - progress, 1.32);
            let base_angle = seed_d * TAU;
            let angular_drift = sin(progress * TAU + seed_a * 5.0) * 0.16
                + sin(progress * 3.7 + seed_c * 8.0) * 0.045;
            let angle = base_angle + angular_drift;
            let particle_position = vec2<f32>(cos(angle), sin(angle)) * radius;

            let base_size = mix(0.012, 0.022, hash11(fi + 47.2));
            let particle_size = max(base_size * scale, 0.0002);
            let particle_distance = length(displaced_local - particle_position);
            let core = (1.0 - smoothstep(
                particle_size * 0.18,
                particle_size,
                particle_distance,
            )) * 0.150 * travelling_mask;
            let halo_radius = particle_size * mix(4.8, 6.5, seed_b);
            let halo = pow(
                saturate(1.0 - particle_distance / max(halo_radius, 0.001)),
                2.25,
            ) * 0.045 * travelling_mask;
            let particle_energy = (core + halo) * particles.y;
            particle_rgb += mix(tint, caustic_tint, seed_a * 0.36) * particle_energy;
            particle_alpha += particle_energy;
        }

        let center_glow = pow(saturate(1.0 - displaced_r / 0.38), 2.4)
            * mix(0.002, 0.010, reactive_activity)
            * optics.w;
        let light_rgb = tint * (dithered_tint_alpha + center_glow)
            + caustic_tint * dithered_caustic_alpha
            + ripple_tint * dithered_ripple_alpha
            + energy_tint * energy_alpha
            + particle_rgb;

        // A transparent overlay cannot directly edit the DWM framebuffer behind
        // it. Reconstruct the displaced captured target through source-over
        // instead: choose only the minimum alpha needed at changed pixels, then
        // solve the premultiplied source color analytically. Flat regions remain
        // alpha zero, avoiding the old dark captured disc; real background edges
        // can now move and split into RGB instead of becoming additive noise.
        if PREMULTIPLIED_OUTPUT > 0.5 {
            let reconstruction_mix = refract_alpha * capture_freshness;
            let desired_background = mix(background_reference, refracted, reconstruction_mix);
            let required_alpha = minimum_reconstruction_alpha(
                background_reference,
                desired_background,
            );
            let desktop_light = min(light_rgb, vec3<f32>(0.42));
            let light_support_alpha = min(maximum_channel(desktop_light) * 0.20, 0.040);
            let output_alpha = max(required_alpha, light_support_alpha);
            let reconstructed_source = max(
                desired_background - background_reference * (1.0 - output_alpha),
                vec3<f32>(0.0),
            );
            let output_rgb = min(reconstructed_source + desktop_light, vec3<f32>(1.0));
            return vec4<f32>(output_rgb, output_alpha);
        }

        let light_alpha = dithered_tint_alpha
            + dithered_caustic_alpha
            + dithered_ripple_alpha
            + energy_alpha
            + particle_alpha
            + center_glow;
        let alpha = saturate(refract_alpha + light_alpha * (1.0 - refract_alpha));
        let optical_rgb = refracted * refract_alpha + light_rgb * (1.0 - refract_alpha);
        // A straight-alpha compositor has no additive-alpha representation.
        // Retain the best-effort captured-background floor for that uncommon
        // fallback; the production Windows surface takes the exact additive
        // branch above.
        let rgb = max(optical_rgb, background_reference * alpha);
        return encode_surface_output(vec4<f32>(rgb, alpha));
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
    return encode_surface_output(vec4<f32>(rgb * alpha, alpha));
}
