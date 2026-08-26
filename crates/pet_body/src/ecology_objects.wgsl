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

fn soft_edge(distance: f32, inner: f32, outer: f32) -> f32 {
    return 1.0 - smoothstep(inner, outer, distance);
}

@fragment
fn fragment_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let kind = input.material.x;
    let radial_distance = length(input.local);
    if kind > 0.5 && kind < 1.5 {
        let rim = soft_edge(abs(radial_distance - 0.88), 0.02, 0.15);
        let membrane = soft_edge(radial_distance, 0.70, 1.05) * 0.42;
        let breathing = 0.88 + 0.12 * sin(input.material.w * 1.15 + radial_distance * 5.0);
        let alpha = max(rim * 0.78, membrane) * input.color.a * breathing;
        let highlight = vec3<f32>(0.34, 0.28, 0.52) * rim;
        let rgb = input.color.rgb * (0.58 + membrane * 0.55) + highlight;
        return vec4<f32>(rgb * alpha, alpha);
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
