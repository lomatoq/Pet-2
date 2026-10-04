// V62 smoky glass lens: analytic optics and studio lights, no desktop sampling.
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
fn environment(direction:vec3<f32>)->vec3<f32> {
    let sky=smoothstep(-0.32,0.62,direction.y);
    let horizon=1.0-smoothstep(0.06,0.24,abs(direction.y+0.15));
    return mix(vec3<f32>(0.035,0.050,0.075),vec3<f32>(0.32,0.40,0.46),sky)
        +vec3<f32>(0.10,0.08,0.12)*horizon;
}
fn softbox(ray:vec3<f32>,phase:f32)->f32 {
    // Angular area light behind the sphere: grazing reflections have negative
    // ray.z. Planar front-only projection incorrectly reduces these to eyebrows.
    let light=normalize(vec3<f32>(-0.60+0.025*sin(phase),0.65+0.02*cos(phase),-0.42));
    return smoothstep(0.74,0.975,dot(ray,light));
}
fn secondary_softbox(ray:vec3<f32>,phase:f32)->f32 {
    let light=normalize(vec3<f32>(0.65+0.02*cos(phase),-0.60+0.025*sin(phase),-0.48));
    let across=dot(ray,normalize(vec3<f32>(0.60,0.65,0.0)));
    return smoothstep(0.77,0.975,dot(ray,light)-across*across*0.38);
}
fn band(radius:f32,center:f32,width:f32,aa:f32)->f32 {
    return 1.0-smoothstep(width,width+aa,abs(radius-center));
}
@fragment
fn fragment_main(input:VertexOutput)->@location(0) vec4<f32> {
    let size=globals.shape.xy;
    let p=(input.uv*2.0-1.0)*size/min(size.x,size.y);
    let radius=length(p);
    let aa=max(fwidth(radius),0.001);
    let coverage=1.0-smoothstep(0.925-aa*0.5,0.925+aa*0.5,radius);
    if(coverage<=0.0) { return vec4<f32>(0.0); }
    let q=p/0.925;
    let r2=min(dot(q,q),0.9999);
    let z=sqrt(max(1.0-r2,0.0001));
    let point=normalize(vec3<f32>(q.x,-q.y,z));
    let normal=point;
    let view=vec3<f32>(0.0,0.0,1.0);
    let reflection=reflect(-view,normal);
    // Two analytic Snell interfaces through a sphere. Only the procedural
    // studio is sampled: this is an environment approximation, not live glass.
    let inside=refract(-view,normal,1.0/1.42);
    let chord=max(-2.0*dot(point,inside),0.0);
    let exit_point=normalize(point+inside*chord);
    let transmitted=refract(inside,-exit_point,1.42);
    let refracted_environment=environment(transmitted);
    let fresnel=0.030+0.970*pow(1.0-z,5.0);
    let phase=globals.shape.w;
    let emphasis=globals.shape.z;
    let facing=p/max(radius,0.001);
    let upper=max(dot(facing,normalize(vec2<f32>(-0.62,-0.78))),0.0);
    let lower=max(dot(facing,normalize(vec2<f32>(0.46,0.89))),0.0);
    // Tinted transmission replaces the V61 pale diffuse sphere. No matte paint
    // lobe: a calm absorptive center supports white glyphs across desktops.
    let tint=globals.tint_alpha.rgb;
    let absorption=tint*(0.084+0.012*z)+vec3<f32>(0.001,0.002,0.003);
    // Quiet dark center, colored refractive depth only on the clear shoulder.
    var color=absorption+refracted_environment*smoothstep(0.45,0.80,radius)*0.12;
    let inner_light=0.030*smoothstep(0.35,0.85,radius)*lower;
    color+=mix(tint,vec3<f32>(0.22,0.34,0.39),0.40)*inner_light;
    color=mix(color,environment(reflection),fresnel*(0.38+0.25*upper));
    // Broad colored environment transmission through the shoulder volume,
    // bounded away from the calm glyph center. No colored contour stroke.
    let volume=smoothstep(0.32,0.78,radius)*(1.0-smoothstep(0.86,0.925,radius));
    color+=volume*(vec3<f32>(0.024,0.035,0.080)*upper+vec3<f32>(0.070,0.030,0.012)*lower);
    // Curved broad window reflection on the shoulder, leaving glyph centers
    // clear. Different surface depths distinguish glass from a flat stroke.
    let window=softbox(reflection,phase)*smoothstep(0.40,0.58,radius);
    color=mix(color,vec3<f32>(0.90,0.97,1.0),window*(0.92+0.04*emphasis));
    let secondary=secondary_softbox(reflection,phase)*smoothstep(0.43,0.64,radius);
    color=mix(color,vec3<f32>(0.95,0.90,0.84),secondary*0.82);
    let meniscus=band(radius,0.865,0.015,aa*0.70);
    color*=1.0-meniscus*(0.31+0.12*lower);
    let caustic=band(radius,0.815,0.015,aa*1.2)*pow(lower,1.5);
    let refracted_tint=mix(tint,vec3<f32>(0.43,0.64,0.70),0.38);
    color+=refracted_tint*caustic*(0.32+0.08*emphasis);
    let rim=band(radius,0.908,0.004,aa*0.65);
    let optical_phase=(1.0-z)*11.0+q.x*1.1+q.y*0.6+0.18*sin(phase);
    let film=0.5+0.5*cos(vec3<f32>(optical_phase,optical_phase*1.17+2.1,optical_phase*1.38+4.2));
    let rim_color=mix(vec3<f32>(0.78,0.94,1.0),film,0.10);
    // Interrupted grazing highlight; no continuous white coin outline.
    color=mix(color,rim_color,rim*0.83*pow(upper,3.0));
    let catch_direction=max(dot(facing,normalize(vec2<f32>(-0.70,0.71))),0.0);
    let catchlight=band(radius,0.894,0.007,aa*0.8)*pow(catch_direction,9.0);
    color=mix(color,vec3<f32>(0.92,0.98,1.0),catchlight*0.26);
    let material_alpha=clamp(0.90-0.11*r2+0.11*fresnel,0.79,0.94);
    let alpha=coverage*material_alpha*globals.tint_alpha.w;
    color=clamp(color,vec3<f32>(0.0),vec3<f32>(1.0));
    if(globals.output.x>0.5) { color=gamma_encode(color); }
    return vec4<f32>(color*alpha,alpha);
}
