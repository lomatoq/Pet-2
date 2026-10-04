// V66 authored luminous ink. No texture samples or time-varying shading.
struct Uniforms {
    tint_alpha: vec4<f32>, // linear absorption tint, independent menu fade
    shape: vec4<f32>,      // logical viewport width/height, emphasis, periodic phase
    output: vec4<f32>,     // x: gamma encode for non-sRGB attachments
    quad: vec4<f32>,       // subpixel NDC center and half-size; material unchanged
};
@group(0) @binding(0) var<uniform> globals: Uniforms;
struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};
@vertex
fn vertex_main(@builtin(vertex_index) index: u32) -> VertexOutput {
    let vertices = array<vec2<f32>, 6>(vec2<f32>(-1.0,-1.0),vec2<f32>(1.0,-1.0),vec2<f32>(-1.0,1.0),
        vec2<f32>(-1.0,1.0),vec2<f32>(1.0,-1.0),vec2<f32>(1.0,1.0));
    let p=vertices[index];
    var result:VertexOutput;
    result.position=vec4<f32>(p*globals.quad.zw+globals.quad.xy,0.0,1.0);
    result.uv=vec2<f32>(p.x,-p.y)*0.5+0.5;
    return result;
}
fn gamma_encode(rgb:vec3<f32>)->vec3<f32> {
    let c=max(rgb,vec3<f32>(0.0));
    return select(1.055*pow(c,vec3<f32>(1.0/2.4))-0.055,c*12.92,c<=vec3<f32>(0.0031308));
}
// Artist-authored shading, not desktop refraction or a photoreal BSDF.
fn gaussian2(p:vec2<f32>, center:vec2<f32>, scale:vec2<f32>)->f32 {
    let d=(p-center)/scale;
    return exp(-dot(d,d));
}
fn ink_material(q:vec2<f32>,z:f32,tint:vec3<f32>,emphasis:f32)->vec3<f32> {
    let saturated=tint/max(max(tint.r,tint.g),max(tint.b,0.001));
    let radius=length(q);
    // Airy colored ink: a quiet pool leaves space for the rounded white symbol.
    let depth=vec3<f32>(0.035,0.042,0.066)+saturated*0.16;
    let shoulder=smoothstep(0.40,0.96,radius);
    let upper=exp(-pow((q.x+0.56)/0.50,2.0)-pow((q.y+0.34)/0.57,2.0));
    let lower=gaussian2(q,vec2<f32>(0.33,0.55),vec2<f32>(0.50,0.27));
    var c=depth+saturated*(shoulder*0.048+upper*0.088+lower*0.069);
    // Two scales of one authored bowed wash. Narrow crest and wide halo are
    // separated so the surface reads as luminous depth at native 56px.
    let curve=-0.65+0.24*q.x+0.34*q.x*q.x;
    let taper=exp(-pow((q.x+0.24)/0.62,4.0));
    let d=q.y-curve;
    let halo=exp(-pow(d/0.15,2.0))*taper;
    let crest=exp(-pow(d/0.035,2.0))*taper;
    let cyan=vec3<f32>(0.20,0.61,0.76);
    let violet=vec3<f32>(0.38,0.23,0.72);
    let radiance=mix(cyan,violet,smoothstep(-0.55,0.54,q.x));
    c+=mix(saturated,radiance,0.55)*halo*0.24;
    c+=radiance*crest*(0.50+0.10*emphasis);
    // An asymmetric colored shoulder glances along one side only. No closed
    // contour, radial ring, white crust, pseudo-metal or temporal iridescence.
    let facing=normalize(q+vec2<f32>(0.00001));
    let left=max(dot(facing,normalize(vec2<f32>(-0.91,-0.42))),0.0);
    c+=mix(saturated,cyan,0.35)*pow(1.0-z,2.0)*pow(left,4.0)*0.20;
    c+=mix(saturated,violet,0.45)*lower*0.055;
    return c*(1.0+0.16*emphasis);
}
@fragment
fn fragment_main(input:VertexOutput)->@location(0) vec4<f32> {
    let size=globals.shape.xy;
    let p=(input.uv*2.0-1.0)*size/min(size.x,size.y);
    let radius=length(p);
    let aa=max(fwidth(radius),0.001);
    let coverage=1.0-smoothstep(0.925-aa*0.5,0.925+aa*0.5,radius);
    if(coverage<=0.0) {return vec4<f32>(0.0);}
    let q=p/0.925;
    let z=sqrt(max(1.0-dot(q,q),0.0001));
    var color=ink_material(q,z,globals.tint_alpha.rgb,globals.shape.z);
    // High center opacity anchors glyph contrast; a soft translucent shoulder.
    let material_alpha=0.992-0.035*pow(1.0-z,2.0);
    let alpha=coverage*material_alpha*globals.tint_alpha.w;
    color=clamp(color,vec3<f32>(0.0),vec3<f32>(1.0));
    if(globals.output.x>0.5) {color=gamma_encode(color);}
    return vec4<f32>(color*alpha,alpha);
}
