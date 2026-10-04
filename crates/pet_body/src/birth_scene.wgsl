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
// Moonlight cradle: trace the generated dark hollow/front lip interface.
// The runtime slices one source in complementary alpha, never two drifting images.
fn nest_front_rim_y(u:f32)->f32 {
    let points=array<vec2<f32>,19>(
        vec2<f32>(147,500),vec2<f32>(232,390),vec2<f32>(332,385),
        vec2<f32>(382,457),vec2<f32>(482,511),vec2<f32>(582,540),
        vec2<f32>(732,560),vec2<f32>(882,565),vec2<f32>(990,566),
        vec2<f32>(1132,564),vec2<f32>(1282,556),vec2<f32>(1432,531),
        vec2<f32>(1532,499),vec2<f32>(1582,473),vec2<f32>(1632,418),
        vec2<f32>(1682,390),vec2<f32>(1742,380),vec2<f32>(1802,460),
        vec2<f32>(1837,530));
    let x=clamp(u*1980.0,points[0].x,points[18].x);
    for (var i=0u;i<18u;i+=1u) {
        let a=points[i];let b=points[i+1u];
        if x<=b.x {return mix(a.y,b.y,(x-a.x)/(b.x-a.x))/866.0;}
    }
    return points[18].y/866.0;
}
// Each new generated capsule half shares its complete canvas and transform
// between rear and front. A depth split follows the authored rim, not a tint.
fn capsule_front_coverage(uv:vec2<f32>,upper:bool)->f32 {
    let x=uv.x*2.0-1.0;
    var edge=0.20+0.35*sqrt(max(0.0,1.0-x*x));
    if upper { edge=0.90-0.20*sin(abs(x)*3.14159265)+0.080*x*x; }
    let front=smoothstep(edge-0.002,edge+0.002,uv.y);
    return select(front,1.0-front,upper);
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
        return vec4<f32>(sample.rgb * alpha, alpha);
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
    var source_alpha=sample.a;
    if input.effect.z>0.5 {
        let upper=input.effect.z==2.0 || input.effect.z==4.0;
        let coverage=capsule_front_coverage(input.uv,upper);
        let front_alpha=sample.a*coverage;
        let back_alpha=(sample.a-front_alpha)/max(1.0-front_alpha,0.000001);
        source_alpha=select(back_alpha,front_alpha,input.effect.z>2.5);
    }
    let alpha=source_alpha*input.opacity;
    return vec4<f32>(sample.rgb*alpha,alpha);
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
    // New authored luminous volume, slowly sheared in radius so inner energy
    // flows while the physical silhouette and opening aperture stay unchanged.
    let angle=time*0.095*(1.0-radius*radius)+0.10*sin(time*0.31+radius*3.2);
    let q=rotate2(p,angle);
    let ink=textureSample(picture,picture_sampler,q*0.489+0.5);
    let color=ink.rgb*(1.0+0.055*charge);
    let theta=atan2(p.y,p.x);
    let hole=radius*0.5+release*0.034*(sin(theta*3.0-open*4.4)+0.45*sin(theta*5.0+open*3.2));
    let clear=mix(1.0,smoothstep(open*0.83-0.13,open*0.83+0.065,hole),smoothstep(0.02,0.27,open));
    let coverage=alpha*opacity*clear*ink.a;
    return vec4<f32>(color*coverage,coverage);
}
