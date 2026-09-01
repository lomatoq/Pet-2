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

fn den_converging_energy_noise(point: vec2<f32>, phase: f32) -> f32 {
    let radius = length(point);
    let direction = normalize(point + vec2<f32>(0.0001, 0.0));
    // Adding phase to radial distance makes the coherent noise field travel
    // inward. There is deliberately no opposing/outward layer.
    let inward_coordinate = direction * (radius * 2.20 + phase * 0.34)
        + vec2<f32>(-phase * 0.016, phase * 0.019)
        + vec2<f32>(7.13, 3.71);
    let inward_primary = noise21(inward_coordinate * 0.92) * 2.0 - 1.0;
    let inward_detail = noise21(inward_coordinate * 1.72 + vec2<f32>(2.41, 8.17)) * 2.0 - 1.0;
    return inward_primary * 0.84 + inward_detail * 0.16;
}

fn den_surface_height(point: vec2<f32>, phase: f32, energy_amount: f32) -> f32 {
    let broad = sin(dot(point, vec2<f32>(0.82, 0.43)) * TAU + phase * 0.58);
    let cross = sin(dot(point, vec2<f32>(-0.38, 1.02)) * TAU - phase * 0.41 + 1.31);
    let eddy = sin(
        (point.x * point.y * 1.35 + point.x * 0.24 - point.y * 0.17) * TAU
            + phase * 0.27,
    );
    let traveling_energy = den_converging_energy_noise(point, phase);
    return broad * 0.43
        + cross * 0.25
        + eddy * 0.12
        + traveling_energy * energy_amount;
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
        let familiarity = saturate(input.material.z);
        let time = input.material.w;
        let activity_integral = input.color.a;

        let local = input.local;
        let r = radial_distance;
        // A low-frequency two-dimensional height field replaces the old radial
        // sine rings. Its finite-difference gradient is also the shared source
        // for silhouette displacement, fake refraction and RGB dispersion.
        let surface_phase = time * mix(0.12, 0.23, activity) + activity_integral * 0.28;
        let energy_amount = mix(0.24, 0.44, activity);
        let height = den_surface_height(local, surface_phase, energy_amount);
        let epsilon = 0.026;
        let gradient = vec2<f32>(
            den_surface_height(local + vec2<f32>(epsilon, 0.0), surface_phase, energy_amount)
                - den_surface_height(local - vec2<f32>(epsilon, 0.0), surface_phase, energy_amount),
            den_surface_height(local + vec2<f32>(0.0, epsilon), surface_phase, energy_amount)
                - den_surface_height(local - vec2<f32>(0.0, epsilon), surface_phase, energy_amount),
        ) / (epsilon * 2.0);
        let displacement_strength = mix(0.072, 0.128, activity);
        let displaced_local = local + gradient * displacement_strength * (1.0 - smoothstep(0.72, 1.12, r));
        let edge_displacement = height * mix(0.046, 0.086, activity);
        let displaced_r = length(displaced_local);
        let field_mask = 1.0 - smoothstep(0.78 + edge_displacement, 1.055 + edge_displacement, r);
        let center_density = pow(saturate(1.0 - displaced_r), 1.32);
        let normal = normalize(gradient + normalize(local + vec2<f32>(0.0001, 0.0)) * 0.28);

        // There is no desktop capture bound to this pass, so the lens samples a
        // quiet synthetic backdrop. Offsetting each channel along the same
        // surface normal creates a restrained fake dispersion instead of grey
        // contour lines, while remaining a single transparent draw call.
        let optical_shift = normal * height * mix(0.038, 0.086, activity) * field_mask;
        let chroma_split = normal * mix(0.032, 0.072, activity) * (0.35 + abs(height) * 0.65);
        let refracted_red = den_virtual_backdrop(displaced_local + optical_shift + chroma_split, surface_phase).r;
        let refracted_green = den_virtual_backdrop(displaced_local + optical_shift, surface_phase).g;
        let refracted_blue = den_virtual_backdrop(displaced_local + optical_shift - chroma_split, surface_phase).b;
        let refracted = vec3<f32>(refracted_red, refracted_green, refracted_blue);
        let fresnel = pow(saturate((displaced_r - 0.40) / 0.66), 1.65);
        let refract_alpha = field_mask
            * (0.40 + center_density * 0.60)
            * mix(0.050, 0.105, activity);

        let tint = mix(
            vec3<f32>(0.38, 0.66, 0.88),
            vec3<f32>(0.64, 0.45, 0.86),
            saturate(familiarity * 0.34 + activity * 0.18),
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
            * mix(0.009, 0.024, activity);
        let spectrum_side = 0.5 + 0.5 * sin((normal.x - normal.y) * 3.2 + height * 4.0);
        let dispersion_tint = mix(
            vec3<f32>(1.00, 0.30, 0.16),
            vec3<f32>(0.18, 0.48, 1.00),
            spectrum_side,
        );
        let dispersion_alpha = field_mask
            * (0.28 + fresnel * 0.72)
            * saturate(0.24 + length(gradient) * 0.34)
            * mix(0.014, 0.042, activity);
        let tint_alpha = field_mask
            * (0.20 + center_density * 0.80)
            * mix(0.012, 0.033, activity);

        // Stable screen-space sub-LSB dither breaks 8-bit gradient quantization
        // without temporal shimmer. Apply it to premultiplied energy and alpha
        // together so blending remains correct on light and dark backdrops.
        let dither = (hash21(floor(input.position.xy)) - 0.5) / 255.0 * field_mask;
        let dithered_tint_alpha = max(tint_alpha + dither * 0.72, 0.0);
        let dithered_caustic_alpha = max(caustic_alpha + dither * 0.28, 0.0);

        let particle_time = time + activity_integral * 0.35;

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
            let particle_distance = length(displaced_local - particle_position);
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
            particle_rgb += mix(tint, caustic_tint, seed_a * 0.36) * particle_energy;
            particle_alpha += particle_energy;
        }

        let center_glow = pow(saturate(1.0 - displaced_r / 0.38), 2.4)
            * mix(0.002, 0.010, activity);
        let light_alpha = dithered_tint_alpha
            + dithered_caustic_alpha
            + dispersion_alpha
            + particle_alpha
            + center_glow;
        let alpha = saturate(refract_alpha + light_alpha * (1.0 - refract_alpha));
        let light_rgb = tint * (dithered_tint_alpha + center_glow)
            + caustic_tint * dithered_caustic_alpha
            + dispersion_tint * dispersion_alpha
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
