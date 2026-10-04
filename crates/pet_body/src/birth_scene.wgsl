@group(0) @binding(0) var picture: texture_2d<f32>;
@group(0) @binding(1) var picture_sampler: sampler;
struct Out { @builtin(position) position:vec4<f32>, @location(0) uv:vec2<f32>, @location(1) opacity:f32, @location(2) effect:vec4<f32>, @location(3) color:vec4<f32>, @location(5) sprite_area:f32 }
@vertex fn vertex_main(@builtin(vertex_index) index:u32,@location(0) rect:vec4<f32>,@location(1) effect:vec4<f32>,@location(2) style:vec4<f32>,@location(3) color:vec4<f32>,@location(4) ribbon_edges:vec4<f32>)->Out {
    let corners=array<vec2<f32>,6>(vec2<f32>(-1,-1),vec2<f32>(1,-1),vec2<f32>(-1,1),vec2<f32>(-1,1),vec2<f32>(1,-1),vec2<f32>(1,1));
    let q=corners[index]*rect.zw;
    var p=rect.xy+vec2<f32>(q.x*cos(style.x)-q.y*sin(style.x),q.x*sin(style.x)+q.y*cos(style.x));
    if effect.y > 3.5 && effect.y < 4.5 {
        let along=corners[index].y*0.5+0.5;
        p=mix(rect.xy,rect.zw,along)+corners[index].x*mix(ribbon_edges.xy,ribbon_edges.zw,along);
    }
    var out:Out; out.position=vec4<f32>(p.x*style.z*2-1,1-p.y*style.w*2,0,1);out.uv=corners[index]*0.5+0.5;out.opacity=style.y;out.effect=effect;out.color=color;out.sprite_area=4.0*rect.z*rect.w;return out;
}
// V29 authored near-rim trace. Central knots follow the visible cushion/shell
// interface; the shoulders return to the real outer silhouette instead of
// extending a parabola diagonally across an unbroken wall. Same knots and cubic
// interpolation are used by assets/nest/export_layers.py for editable layers.
fn nest_front_rim_y(u: f32) -> f32 {
    let knots = array<vec3<f32>, 21>(
        vec3<f32>(65.000000, 510.000000, -1.000000000),
        vec3<f32>(100.000000, 475.000000, -0.680000000),
        vec3<f32>(150.000000, 450.000000, -0.289473684),
        vec3<f32>(210.000000, 438.000000, 0.000000000),
        vec3<f32>(240.000000, 479.000000, 1.164847512),
        vec3<f32>(300.000000, 538.000000, 0.627937916),
        vec3<f32>(400.000000, 582.000000, 0.370526316),
        vec3<f32>(500.000000, 614.000000, 0.298666667),
        vec3<f32>(600.000000, 642.000000, 0.195348837),
        vec3<f32>(700.000000, 657.000000, 0.065899582),
        vec3<f32>(768.000000, 660.000000, 0.000000000),
        vec3<f32>(836.000000, 657.000000, -0.070515212),
        vec3<f32>(936.000000, 635.000000, -0.246400000),
        vec3<f32>(1036.000000, 607.000000, -0.294237288),
        vec3<f32>(1136.000000, 576.000000, -0.353055556),
        vec3<f32>(1236.000000, 535.000000, -0.585207612),
        vec3<f32>(1296.000000, 480.000000, -1.090333919),
        vec3<f32>(1330.000000, 436.000000, 0.000000000),
        vec3<f32>(1386.000000, 455.000000, 0.460057434),
        vec3<f32>(1436.000000, 490.000000, 0.830188679),
        vec3<f32>(1474.000000, 528.000000, 1.000000000)
    );
    let x = clamp(u * 1536.0, knots[0].x, knots[20].x);
    for (var i = 0u; i < 20u; i += 1u) {
        let a = knots[i];
        let b = knots[i + 1u];
        if x <= b.x {
            let h = b.x - a.x;
            let t = clamp((x - a.x) / h, 0.0, 1.0);
            let t2 = t * t;
            let t3 = t2 * t;
            return ((2.0 * t3 - 3.0 * t2 + 1.0) * a.y
                + (t3 - 2.0 * t2 + t) * h * a.z
                + (-2.0 * t3 + 3.0 * t2) * b.y
                + (t3 - t2) * h * b.z) / 1024.0;
        }
    }
    return knots[20].y / 1024.0;
}

// V66 living ink: texture alpha owns all silhouettes and the exact layer
// boundaries. Authored value only supplies bounded depth cues; broad analytic
// paint replaces the baked gold, rainbow reflection and granular metal finish.
// Same UV/function on the two nest layers avoids a new foreground paint seam.
fn illustrated_prop(sample:vec3<f32>,uv:vec2<f32>,nest:bool)->vec3<f32> {
    let value=dot(display_color(sample),vec3<f32>(0.2126,0.7152,0.0722));
    if nest {
        let mid_mask=smoothstep(0.22,0.40,value);
        let light_mask=smoothstep(0.64,0.83,value);
        var paint=mix(vec3<f32>(0.008,0.010,0.026),vec3<f32>(0.032,0.038,0.084),mid_mask);
        paint=mix(paint,vec3<f32>(0.13,0.15,0.23),light_mask);
        let cushion=smoothstep(0.72,0.87,value);
        return mix(paint,vec3<f32>(0.095,0.18,0.17),cushion*0.70);
    }
    // Rounded, quiet ink planes belong to the capsule rather than the source
    // image's metal reflections. Bounded value modulation preserves seams
    // without amplifying tiny authored grain into mottled highlight patches.
    let x=uv.x*2.0-1.0;
    let rounded=sqrt(max(0.0,1.0-x*x));
    let rise=1.0-smoothstep(0.12,0.83,uv.y);
    let plane=rounded*(0.30+0.70*rise);
    var paint=mix(vec3<f32>(0.012,0.016,0.036),vec3<f32>(0.068,0.096,0.14),plane);
    paint*=0.76+0.24*value;
    let shoulder=1.0-smoothstep(0.12,1.0,length((uv-vec2<f32>(0.27,0.24))*vec2<f32>(2.4,4.2)));
    paint+=vec3<f32>(0.015,0.036,0.038)*shoulder;
    // The existing bright central latch reads as one luminous insert. Do not
    // recolor every bright texture edge into a glowing outer crust.
    let latch=smoothstep(0.93,0.995,value)*(1.0-smoothstep(0.035,0.085,abs(uv.x-0.5)));
    return mix(paint,vec3<f32>(0.45,0.64,0.58),latch*0.72);
}

@fragment fn fragment_main(input:Out)->@location(0) vec4<f32> {
    if input.effect.y < -0.5 {
        if input.effect.z > 0.0 && input.position.y > input.effect.z { discard; }
        // Aligned cushion/rim layers. Follow the physical near-rim contour.
        // Solve back alpha so source-over
        // recomposition restores the original alpha even through the feather.
        let sample = textureSample(picture, picture_sampler, input.uv);
        let edge = nest_front_rim_y(input.uv.x);
        let coverage = smoothstep(edge - 0.0018, edge + 0.0018, input.uv.y);
        let front_alpha = sample.a * coverage;
        let back_alpha = (sample.a - front_alpha) / max(1.0 - front_alpha, 0.000001);
        let alpha = select(front_alpha, back_alpha, input.effect.y < -1.5) * input.opacity;
        return vec4<f32>(illustrated_prop(sample.rgb,input.uv,true) * alpha, alpha);
    }
    if input.effect.y>5.5 {
        let p=(input.uv-0.5)*2.0;let radius=length(p);let a=(1.0-smoothstep(0.80,1.0,radius))*input.opacity;
        let z=sqrt(max(0.0,1.0-dot(p,p)));let n=vec3<f32>(p.x,-p.y,z);
        let spec=pow(max(0.0,dot(n,normalize(vec3<f32>(-0.45,0.6,1.0)))),18.0);
        return vec4<f32>(mix(input.color.rgb*0.60,vec3<f32>(1.0),spec*0.75)*a,a);
    }
    if input.effect.y>4.5 {
        let p=(input.uv-0.5)*2.0;
        // Five overlapping large auras previously turned the opening aperture
        // into an additive white veil. Bound broad haze by footprint, while
        // leaving small particle glows and the original choreography intact.
        let broad=smoothstep(48.0*48.0,130.0*130.0,input.sprite_area);
        let channels=input.color.rgb;
        let white_fraction=min(channels.x,min(channels.y,channels.z))/max(max(channels.x,max(channels.y,channels.z)),0.00001);
        let neutral=smoothstep(0.72,0.90,white_fraction);
        // Neutral mist is a faint accent, not an opaque white particle.
        // Preserve its path/lifetime and keep colored motes unchanged.
        let energy=mix(1.0,0.12,broad)*mix(1.0,0.06,neutral);
        let a=exp(-dot(p,p)*4.6)*(1.0-smoothstep(0.72,1.0,length(p)))*input.opacity*energy;
        return vec4<f32>(input.color.rgb*a,a);
    }
    if input.effect.y>3.5 {
        let cross=input.uv.x*2.0-1.0;let profile=exp(-cross*cross*3.2)*0.55+exp(-cross*cross*62.0)*0.16;
        let q=mix(input.effect.z,input.effect.w,input.uv.y);let taper=pow(max(0.0,1.0-q),1.55);
        let a=profile*taper*input.opacity*(1.0-smoothstep(0.86,1.0,abs(cross)));
        return vec4<f32>(input.color.rgb*a,a);
    }
    if input.effect.y > 2.5 {
        let p=(input.uv-0.5)*vec2<f32>(220.0,400.0);let half=vec2<f32>(19.0,input.effect.z*0.5);let radius=18.0;
        let q=abs(p)-half+radius;let d=length(max(q,vec2<f32>(0.0)))+min(max(q.x,q.y),0.0)-radius;
        let core=1.0-smoothstep(-1.0,1.05,d);let outside=max(d,0.0);let blur=12.0;
        let local=exp(-outside*outside/(blur*blur));let wide=exp(-outside*outside/(blur*blur*8.0));
        let column=exp(-pow(p.x/(half.x*0.62),2.0))*exp(-pow(p.y/(half.y*1.95),2.0));
        let edge=exp(-pow(d/(blur*0.22),2.0));let sparkle=exp(-pow(p.x/(half.x*0.22),2.0))*exp(-pow(p.y/(half.y*0.40),2.0));
        let boundary=1.0-smoothstep(0.79,1.0,max(abs(input.uv.x*2.0-1.0),abs(input.uv.y*2.0-1.0)));
        let light=(vec3<f32>(2.55,2.35,2.05)*(core*0.95+column*0.62+sparkle*0.38)+input.color.rgb*(local*0.62+wide*0.16+edge*0.34))*input.opacity*boundary;
        let alpha=clamp(max(light.x,max(light.y,light.z)),0.0,1.0);let color=light/(1.0+light)*1.25;
        return vec4<f32>(clamp(color,vec3<f32>(0),vec3<f32>(1))*alpha,alpha);
    }
    if input.effect.y > 1.5 {
        let p=(input.uv-0.5)*2.0;let radius=length(p);
        let a=exp(-dot(p,p)*3.8)*(1.0-smoothstep(0.72,1.0,radius))*input.opacity;
        return vec4<f32>(input.color.rgb*a,a);
    }
    if input.effect.y > 0.5 { return capsule_orb(input.uv,input.effect.x,input.opacity); }
    let sample=textureSample(picture,picture_sampler,input.uv);
    let alpha=sample.a*input.opacity;
    return vec4<f32>(illustrated_prop(sample.rgb,input.uv,false)*alpha,alpha);
}


// First Light V66 seed: V20 moving geometry/charge/aperture with ink material.
fn rotate2(p:vec2<f32>,a:f32)->vec2<f32> {
    return vec2<f32>(cos(a)*p.x+sin(a)*p.y,-sin(a)*p.x+cos(a)*p.y);
}
fn display_color(x:vec3<f32>)->vec3<f32> {
    return select(12.92*x,1.055*pow(max(x,vec3<f32>(0.0)),vec3<f32>(1.0/2.4))-0.055,x>vec3<f32>(0.0031308));
}
fn linear_color(x:vec3<f32>)->vec3<f32> {
    return select(x/12.92,pow((x+0.055)/1.055,vec3<f32>(2.4)),x>vec3<f32>(0.04045));
}
fn capsule_orb(uv:vec2<f32>,time:f32,opacity:f32)->vec4<f32> {
    let open=clamp((time-7.79)/0.92,0.0,1.0);
    let charge=smoothstep(5.55,7.45,time)*(1.0-smoothstep(7.68,8.25,time));
    let release=smoothstep(0.012,0.47,open);
    let screen=(uv-0.5)*2.0*1.36;
    let polar=atan2(screen.y,screen.x);
    let wave=release*(0.074*sin(3.0*polar-open*6.3)+0.049*sin(5.0*polar+open*4.6)+0.025*sin(7.0*polar-open*8.0));
    var p=screen/(1.0+wave);
    p+=release*0.033*vec2<f32>(sin(screen.y*3.3-open*4.0),cos(screen.x*3.6+open*4.5));
    let stretch=1.0+0.055*release*sin(open*5.2);p*=vec2<f32>(stretch,1.0/stretch);
    let radius=length(p);
    let alpha=1.0-smoothstep(0.994,1.006,radius);
    if alpha<0.0001 {return vec4<f32>(0.0);}
    // Preserve V20 release deformation and aperture exactly; replace only the
    // interior material. Calm ink, a broad cool plane and an orbiting inlay
    // explain an energetic seed without raymarched candy clouds or neon crust.
    let normal_xy=p/max(1.0,radius);
    let z=sqrt(max(0.0,1.0-dot(normal_xy,normal_xy)));
    let n=vec3<f32>(normal_xy.x,-normal_xy.y,z);
    let light=dot(n,normalize(vec3<f32>(-0.56,0.64,0.54)));
    let plane=smoothstep(-0.12,0.10,light);
    let shoulder=smoothstep(0.59,0.80,light);
    var color=mix(vec3<f32>(0.004,0.006,0.017),vec3<f32>(0.025,0.033,0.069),plane);
    color=mix(color,vec3<f32>(0.080,0.17,0.18),shoulder*0.65);
    let q=rotate2(p,time*0.15+0.20*sin(time*0.32));
    let orbit_outer=length(q-vec2<f32>(-0.25,-0.09));
    let orbit_inner=length(q-vec2<f32>(-0.06,-0.02));
    let ink_inlay=(1.0-smoothstep(0.52,0.545,orbit_outer))*smoothstep(0.495,0.52,orbit_inner);
    let pulse=0.65+0.35*sin(time*2.15-radius*3.6);
    color=mix(color,vec3<f32>(0.23,0.40,0.39),ink_inlay*(0.20+0.32*charge)*pulse);
    let arc=exp(-pow((radius-0.958)/0.021,2.0));
    let angular=smoothstep(0.14,0.88,dot(normal_xy,normalize(vec2<f32>(-0.64,-0.77))));
    color+=vec3<f32>(0.28,0.36,0.37)*arc*angular*0.58;
    let violet_side=smoothstep(0.40,0.94,normal_xy.x)*smoothstep(0.30,0.90,radius);
    color+=vec3<f32>(0.039,0.017,0.075)*violet_side;
    let theta=atan2(p.y,p.x);
    let hole=radius*0.5+release*0.034*(sin(theta*3.0-open*4.4)+0.45*sin(theta*5.0+open*3.2));
    let clear=mix(1.0,smoothstep(open*0.83-0.13,open*0.83+0.065,hole),smoothstep(0.02,0.27,open));
    let coverage=alpha*opacity*clear;
    return vec4<f32>(color*coverage,coverage);
}
