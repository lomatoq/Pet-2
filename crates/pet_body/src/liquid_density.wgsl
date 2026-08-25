struct DensityGlobals {
    viewport_time: vec4<f32>,
    body_shape: vec4<f32>,
    face_shape: vec4<f32>,
    appendages: vec4<f32>,
};

@group(0) @binding(0) var<uniform> globals: DensityGlobals;

struct DensityOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) kernel_coordinate: vec2<f32>,
    @location(1) material: vec4<f32>,
    @location(2) material_flow: vec4<f32>,
};

struct DensityTargets {
    @location(0) field: vec4<f32>,
    @location(1) flow: vec4<f32>,
};

fn quad_corner(vertex_index: u32) -> vec2<f32> {
    let corners = array<vec2<f32>, 6>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(1.0, -1.0),
        vec2<f32>(-1.0, 1.0),
        vec2<f32>(-1.0, 1.0),
        vec2<f32>(1.0, -1.0),
        vec2<f32>(1.0, 1.0),
    );
    return corners[vertex_index];
}

fn local_to_clip(point: vec2<f32>) -> vec2<f32> {
    let organism_scale = max(globals.appendages.w, 0.001);
    let presented_point = point + globals.body_shape.zw;
    return vec2<f32>(
        presented_point.x / (organism_scale * max(globals.viewport_time.x, 0.001)),
        // Simulation and NDC are both Y-up; framebuffer/texture conversion happens
        // once through the standard top-down UVs in the full-screen passes.
        presented_point.y / organism_scale,
    );
}

@vertex
fn particle_vertex(
    @builtin(vertex_index) vertex_index: u32,
    @location(0) geometry_a: vec4<f32>,
    @location(1) geometry_b: vec4<f32>,
    @location(2) particle_material: vec4<f32>,
    @location(3) material_flow: vec4<f32>,
) -> DensityOutput {
    let corner = quad_corner(vertex_index);
    var axis = geometry_b.xy;
    if (dot(axis, axis) < 0.25) {
        axis = vec2<f32>(1.0, 0.0);
    } else {
        axis = normalize(axis);
    }
    let perpendicular = vec2<f32>(-axis.y, axis.x);
    let point = geometry_a.xy
        + axis * corner.x * geometry_a.z
        + perpendicular * corner.y * geometry_a.w;
    var output: DensityOutput;
    output.clip_position = vec4<f32>(local_to_clip(point), 0.0, 1.0);
    output.kernel_coordinate = corner;
    output.material = vec4<f32>(
        geometry_b.w,
        particle_material.x,
        particle_material.y,
        geometry_b.z,
    );
    output.material_flow = material_flow;
    return output;
}

@fragment
fn particle_fragment(input: DensityOutput) -> DensityTargets {
    let radius_squared = dot(input.kernel_coordinate, input.kernel_coordinate);
    if (radius_squared >= 1.0) {
        discard;
    }
    // Compact smooth kernel. Additive overlap is the implicit liquid field.
    let weight = pow(1.0 - radius_squared, 3.0);
    let density = weight * input.material.w;
    // Material field contract: R density, G optical thickness, B emission,
    // A pigment. None of these channels is final alpha.
    var output: DensityTargets;
    output.field = vec4<f32>(
        density,
        density * input.material.x,
        density * input.material.y,
        density * input.material.z,
    );
    output.flow = density * input.material_flow;
    return output;
}
