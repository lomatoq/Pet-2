// V67 opal energy membrane. No texture samples, extra passes or desktop capture.
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
// V67 opal membrane: broad subsurface light, no metallic rim or gloss stripe.
// State energy fills the object; it is never represented by a status dot.
fn bell(p:vec2<f32>, center:vec2<f32>, width:vec2<f32>)->f32 {
    let d=(p-center)/width;
    return exp(-dot(d,d));
}
fn opal_material(q:vec2<f32>, z:f32, tint:vec3<f32>, emphasis:f32, press:f32)->vec3<f32> {
    let tint_soft=mix(vec3<f32>(0.58,0.66,0.78), tint, 0.34);
    let phase=globals.shape.w;
    let lower_pool=bell(q,vec2<f32>(0.30,0.45),vec2<f32>(0.78,0.57));
    let edge=pow(1.0-z,1.4);
    let normal=normalize(vec3<f32>(q,z));
    let light=clamp(dot(normal,normalize(vec3<f32>(-0.42,-0.52,0.77))),0.0,1.0);
    var color=mix(tint_soft*0.76,vec3<f32>(0.91,0.95,0.98),0.30+0.48*sqrt(light));
    // Wide inner folds give an optical volume instead of a flat shaded badge.
    let bend=0.28*sin(q.x*2.0+phase*0.22)+0.20*q.x;
    let fold=exp(-pow((q.y-0.36-bend)/0.29,2.0));
    let cyan=vec3<f32>(0.14,0.69,0.77);
    let violet=vec3<f32>(0.52,0.37,0.80);
    let energy=mix(cyan,violet,smoothstep(-0.5,0.85,q.x));
    color=mix(color, mix(tint_soft,energy,0.64),0.50*lower_pool+0.29*fold);
    let glint=bell(q,vec2<f32>(-0.33,-0.43),vec2<f32>(0.29,0.22));
    color+=vec3<f32>(0.075,0.082,0.075)*glint;
    color-=edge*vec3<f32>(0.14,0.125,0.09);
    // A depression redirects the light across the inside shoulder. The center
    // dimming and the broad luminous basin move together with the scaled icon.
    let basin=bell(q,vec2<f32>(0.0,0.08),vec2<f32>(0.63,0.62));
    color*=1.0-0.24*press*basin;
    let energized=(0.27+0.52*fold+0.21*lower_pool)*emphasis;
    color=mix(color, mix(energy,vec3<f32>(0.86,0.98,1.0),0.20),energized*0.80);
    let inner_crescent=bell(q,vec2<f32>(-0.32,0.36),vec2<f32>(0.37,0.57));
    color+=vec3<f32>(0.25,0.48,0.44)*emphasis*inner_crescent*(0.55+press*0.35);
    return color;
}
@fragment
fn fragment_main(input:VertexOutput)->@location(0) vec4<f32> {
    let size=globals.shape.xy;
    let p=(input.uv*2.0-1.0)*size/min(size.x,size.y);
    let radius=length(p);
    // Keep a soft optical edge in logical pixels at every display density.
    // Derivatives alone made 2x DPI controls perceptually sharper than 1x.
    let aa=max(fwidth(radius)*1.55,2.8/min(size.x,size.y));
    let coverage=1.0-smoothstep(0.922-aa*0.5,0.922+aa*0.5,radius);
    let outer_glow=exp(-pow(max(radius-0.88,0.0)/0.12,2.0))
        *(1.0-smoothstep(0.97,1.0,radius))*(1.0-coverage)
        *(0.12+globals.shape.z*0.15);
    if(coverage+outer_glow<=0.0001) {return vec4<f32>(0.0);}
    let q=p/0.922;
    let z=sqrt(max(1.0-dot(q,q),0.0001));
    var color=opal_material(q,z,globals.tint_alpha.rgb,globals.shape.z,globals.output.y);
    let body_alpha=coverage*0.995;
    let light_color=mix(vec3<f32>(0.57,0.78,0.95),vec3<f32>(0.53,0.96,0.82),globals.shape.z);
    let optical_alpha=body_alpha+outer_glow;
    color=(color*body_alpha+light_color*outer_glow)/max(optical_alpha,0.00001);
    let alpha=optical_alpha*globals.tint_alpha.w;
    color=clamp(color,vec3<f32>(0.0),vec3<f32>(1.0));
    if(globals.output.x>0.5) {color=gamma_encode(color);}
    return vec4<f32>(color*alpha,alpha);
}
