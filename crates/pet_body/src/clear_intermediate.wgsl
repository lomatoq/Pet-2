struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
};

@vertex
fn vertex_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    let positions = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0),
    );
    var output: VertexOutput;
    output.clip_position = vec4<f32>(positions[vertex_index], 0.0, 1.0);
    return output;
}

struct DualClearOutput {
    @location(0) first: vec4<f32>,
    @location(1) second: vec4<f32>,
};

@fragment
fn clear_dual() -> DualClearOutput {
    var output: DualClearOutput;
    output.first = vec4<f32>(0.0);
    output.second = vec4<f32>(0.0);
    return output;
}

@fragment
fn clear_single() -> @location(0) vec4<f32> {
    return vec4<f32>(0.0);
}
