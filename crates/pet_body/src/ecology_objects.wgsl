const TAU: f32 = 6.28318530718;

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

fn soft_edge(distance_from_center: f32, inner: f32, outer: f32) -> f32 {
    return 1.0 - smoothstep(inner, outer, distance_from_center);
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
        let field_mask = 1.0 - smoothstep(0.76, 1.03, r);
        let center_density = pow(saturate(1.0 - r), 1.40);

        let irregularity =
            sin(local.x * 4.1 + time * 0.21) * 0.010
            + sin(local.y * 3.4 - time * 0.17) * 0.007
            + sin((local.x + local.y) * 5.2 + time * 0.11) * 0.004;
        let warped_r = r + irregularity * field_mask;

        // Integrating activity on the CPU keeps both motion clocks continuous when
        // proximity changes. The shader reconstructs the requested idle/active rates.
        let ripple_phase = time * 0.20 + activity_integral * 0.35;
        let particle_time = time + activity_integral * 0.35;
        let wave_a = sin((warped_r * 3.20 + ripple_phase) * TAU);
        let wave_b = sin((warped_r * 5.05 + ripple_phase * 1.31 + 1.73) * TAU);
        let wave = wave_a * 0.72 + wave_b * 0.28;
        let ripple_ridge = smoothstep(0.48, 0.94, wave);
        let ripple_alpha = ripple_ridge
            * field_mask
            * (0.32 + center_density * 0.68)
            * mix(0.012, 0.033, activity);

        // TODO: sample the renderer's existing desktop backdrop here once its bind
        // group is exposed to the ecology overlay. Keep the local one-draw-call
        // fallback instead of duplicating the capture texture or render pipeline.
        let refracted = vec3<f32>(0.0);
        let refract_alpha = 0.0;

        let tint = vec3<f32>(0.88, 0.95, 1.00);
        let tint_alpha = field_mask
            * (0.20 + center_density * 0.80)
            * mix(0.018, 0.043, activity);

        var particle_rgb = vec3<f32>(0.0);
        var particle_alpha = 0.0;
        for (var i: u32 = 0u; i < 7u; i = i + 1u) {
            let fi = f32(i);
            let seed_a = hash11(fi + 1.17);
            let seed_b = hash11(fi + 9.41);
            let seed_c = hash11(fi + 17.83);
            let seed_d = hash11(fi + 31.29);

            let base_speed = mix(0.115, 0.195, seed_b);
            let progress = fract(particle_time * base_speed + seed_a);
            let appear = smoothstep(0.00, 0.10, progress);
            let shrink = 1.0 - smoothstep(0.70, 0.98, progress);
            let scale = appear * shrink;

            let start_radius = mix(0.34, 0.92, seed_c);
            let radius = start_radius * pow(1.0 - progress, 1.32);
            let base_angle = seed_d * TAU;
            let angular_drift = sin(progress * TAU + seed_a * 5.0) * 0.16
                + sin(progress * 3.7 + seed_c * 8.0) * 0.045;
            let angle = base_angle + angular_drift;
            let particle_position = vec2<f32>(cos(angle), sin(angle)) * radius;

            let base_size = mix(0.009, 0.016, hash11(fi + 47.2));
            let particle_size = max(base_size * scale, 0.0002);
            let particle_distance = length(local - particle_position);
            let core = (1.0 - smoothstep(
                particle_size * 0.18,
                particle_size,
                particle_distance,
            )) * 0.052 * field_mask;
            let halo_radius = particle_size * mix(4.8, 6.5, seed_b);
            let halo = pow(
                saturate(1.0 - particle_distance / max(halo_radius, 0.001)),
                2.25,
            ) * 0.016 * field_mask;
            let particle_energy = core + halo;
            particle_rgb += tint * particle_energy;
            particle_alpha += particle_energy;
        }

        let center_glow = pow(saturate(1.0 - r / 0.34), 2.4)
            * mix(0.004, 0.018, activity);
        let light_alpha = tint_alpha + ripple_alpha + particle_alpha + center_glow;
        let alpha = saturate(refract_alpha + light_alpha * (1.0 - refract_alpha));
        let light_rgb = tint * (tint_alpha + ripple_alpha + center_glow) + particle_rgb;
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
