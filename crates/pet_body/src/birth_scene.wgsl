@group(0) @binding(0) var picture: texture_2d<f32>;
@group(0) @binding(1) var picture_sampler: sampler;
struct Out { @builtin(position) position:vec4<f32>, @location(0) uv:vec2<f32>, @location(1) opacity:f32, @location(2) effect:vec4<f32>, @location(3) color:vec4<f32> }
@vertex fn vertex_main(@builtin(vertex_index) index:u32,@location(0) rect:vec4<f32>,@location(1) effect:vec4<f32>,@location(2) style:vec4<f32>,@location(3) color:vec4<f32>)->Out {
    let corners=array<vec2<f32>,6>(vec2<f32>(-1,-1),vec2<f32>(1,-1),vec2<f32>(-1,1),vec2<f32>(-1,1),vec2<f32>(1,-1),vec2<f32>(1,1));
    let q=corners[index]*rect.zw;
    let p=rect.xy+vec2<f32>(q.x*cos(style.x)-q.y*sin(style.x),q.x*sin(style.x)+q.y*cos(style.x));
    var out:Out; out.position=vec4<f32>(p.x*style.z*2-1,1-p.y*style.w*2,0,1);out.uv=corners[index]*0.5+0.5;out.opacity=style.y;out.effect=effect;out.color=color;return out;
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

@fragment fn fragment_main(input:Out)->@location(0) vec4<f32> {
    if input.effect.y < -0.5 {
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
        let p=(input.uv-0.5)*2.0;let a=exp(-dot(p,p)*4.6)*(1.0-smoothstep(0.72,1.0,length(p)))*input.opacity;
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
    return vec4<f32>(sample.rgb*alpha,alpha);
}


// First Light v20 orb: the reference's moving volume, vortex, charge, glass
// highlights, inner motes and dissolving membrane, in native WGSL.
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
    let normal_xy=p/max(1.0,radius);
    let z=sqrt(max(0.0,1.0-dot(normal_xy,normal_xy)));
    let n=normalize(vec3<f32>(normal_xy.x,-normal_xy.y,max(z,0.001)));
    let pre=smoothstep(0.56,0.992,charge)*(1.0-smoothstep(0.02,0.22,open));
    let surge=0.5+0.5*sin(time*10.5+radius*11.0-polar*1.8);
    let spin=(0.42*sin(time*0.52+0.62*radius)+0.09*sin(time*0.96+radius*2.7))*(1.0+pre*0.85);
    var q=rotate2(p*(0.91+0.045*radius*radius),spin);
    let swirl=q;let swlen=max(length(swirl),0.001);let swdir=swirl/swlen;
    q+=vec2<f32>(-swirl.y,swirl.x)/swlen*(0.050*(1.0-radius*0.42))*sin(time*(1.10+0.95*pre)+swlen*(5.4+2.8*pre));
    q+=0.022*(1.0+0.65*pre)*vec2<f32>(sin(q.y*(4.1+0.9*pre)+time*(1.05+0.85*pre)),cos(q.x*(3.9+0.7*pre)-time*(0.98+0.82*pre)));
    q+=0.014*p*(sin(time*(0.86+0.75*pre)+radius*(3.6+2.8*pre))*0.5+0.5);
    q+=swdir*(0.020+0.030*surge)*pre*(1.0-radius*0.24);
    q+=0.060*sin(open*3.14159265)*vec2<f32>(sin(q.y*4.9+time*1.4),cos(q.x*4.7-time*1.3));
    let colors=array<vec3<f32>,5>(vec3<f32>(1.0,0.055,0.48),vec3<f32>(0.025,0.78,1),vec3<f32>(0.40,0.075,1),vec3<f32>(1,0.73,0.035),vec3<f32>(0.025,0.94,0.48));
    var inner=vec3<f32>(0.0);var trans=1.0;var emitted=vec3<f32>(0.0);
    let zmax=sqrt(max(0.015,1.0-min(dot(q,q),0.985)));let step_len=2.0*zmax/6.0;
    for(var slice=0;slice<6;slice++) {
        let depth=zmax-(f32(slice)+0.5)*step_len;
        var xy=rotate2(q,depth*0.28*sin(time*0.52));
        xy+=0.062*vec2<f32>(sin(xy.y*3.1+time*0.81+depth),cos(xy.x*2.7-time*0.70+depth));
        let pos=vec3<f32>(xy,depth);
        var density=0.0;var weight=0.0;var sum=vec3<f32>(0.0);var emission=vec3<f32>(0.0);var hot=vec3<f32>(0.0);
        for(var i=0;i<5;i++) {
            let fi=f32(i);let angle=fi*2.399963+time*(0.48+0.09*f32(i%3))+0.21*sin(time*0.63+fi);
            let orbit=0.53+0.095*sin(time*0.70+fi*1.37);
            let center=vec3<f32>(orbit*cos(angle),orbit*sin(angle)*0.97,0.24*sin(time*0.69+fi*1.6));
            let d=(pos-center)*vec3<f32>(0.96,0.91,0.8);let rad=0.78+0.075*sin(time*0.75+fi*1.7);
            let w=exp(-2.4*dot(d,d)/(rad*rad));
            let branch=d-vec3<f32>(0.14*cos(time*0.61+fi),0.18*sin(time*0.73+fi*1.4),0.05);
            let child=exp(-6.4*dot(branch,branch)/(rad*rad));let soft=exp(-1.20*dot(d,d)/(rad*rad));
            let energy=0.86+0.21*sin(time*0.82+fi*0.97)+0.09*sin(time*1.57+fi*2.2);
            hot+=colors[i]*(2.40*(w*w*w*0.86+child*0.26)+0.025*soft)*energy;
            density+=w;let cw=w*w*w*w;sum+=colors[i]*cw;weight+=cw;
            emission+=colors[i]*(1.22*w*w+0.050*soft)*(0.94+0.06*sin(time*0.92+fi));
        }
        let hue=sum/max(weight,0.00001);emission=emission*0.24+hue*density*0.60;
        let absorption=1.0-exp(-density*step_len*0.40);
        inner+=trans*(hue*absorption*0.30+emission*step_len);emitted+=trans*hot*step_len;trans*=1.0-absorption*0.65;
    }
    inner+=mix(vec3<f32>(0.027,0.08,0.135),vec3<f32>(0.10,0.027,0.11),0.5+0.5*sin(time*0.42+q.y*1.6))*(0.38+0.60*zmax);
    inner+=vec3<f32>(0.03,0.09,0.18)*(1.0-smoothstep(0.35,1.0,length(q)))*0.35;
    let frost=vec3<f32>(0.16,0.19,0.23)+inner*0.34+vec3<f32>(0.05,0.07,0.10)*pow(1.0-z,1.7);
    inner=mix(inner,frost,0.06);
    let theta=atan2(p.y,p.x);inner*=0.98+0.08*sin(time*1.05+theta*2.0-z*3.1);
    let fresnel=0.055+0.945*pow(1.0-z,4.6);
    var reflection=reflect(vec3<f32>(0,0,-1),n);
    let rxz=rotate2(reflection.xz,0.09*sin(time*0.31));reflection=vec3<f32>(rxz.x,reflection.y,rxz.y);
    let key=max(0.0,dot(reflection,normalize(vec3<f32>(-0.68,0.66,0.15))));
    let fill=max(0.0,dot(reflection,normalize(vec3<f32>(0.77,-0.10,-0.25))));
    let back=max(0.0,dot(reflection,normalize(vec3<f32>(-0.26,-0.67,-0.45))));
    let studio=vec3<f32>(0.07,0.10,0.15)*(0.40+0.60*max(0.0,reflection.y))+vec3<f32>(1,0.97,0.96)*(15.0*pow(key,105.0)+0.8*pow(key,22.0))+vec3<f32>(0.59,0.85,1)*(11.0*pow(fill,86.0)+0.55*pow(fill,18.0))+vec3<f32>(1,0.49,0.79)*6.0*pow(back,72.0);
    var light=inner*(0.96-0.05*fresnel)+studio*(0.22+0.96*fresnel);
    let edge=exp(-pow((radius-0.986)/0.009,2.0));let hue=0.73+0.25*cos(theta+vec3<f32>(0.0,1.8,3.8)+time*0.09);
    light+=edge*hue*(0.46+0.24*pow(max(0.0,-p.x),2.0));
    let sweep=max(0.0,dot(n,normalize(vec3<f32>(0.55*sin(time*0.42),0.22+0.10*cos(time*0.3),0.95))));
    let sweep2=max(0.0,dot(n,normalize(vec3<f32>(-0.45*cos(time*0.33+0.8),-0.18,0.95))));
    light+=vec3<f32>(1,0.99,0.97)*pow(sweep,44.0)*0.60+vec3<f32>(0.82,0.92,1)*pow(sweep2,28.0)*0.40;
    let shell=exp(-pow((radius-0.974)/0.040,2.0));
    light+=vec3<f32>(0.92,0.96,1)*shell*0.1356*pow(max(0.0,dot(n,normalize(vec3<f32>(-0.78,-0.18,0.60)))),2.8);
    light+=vec3<f32>(1,0.96,0.93)*shell*0.0914*pow(max(0.0,dot(n,normalize(vec3<f32>(0.36,0.58,0.73)))),3.6);
    light+=vec3<f32>(0.94,0.98,1)*shell*0.0696*pow(max(0.0,dot(n,normalize(vec3<f32>(-0.12,-0.92,0.38)))),5.5);
    light+=exp(-pow((radius-0.962)/0.026,2.0))*(pow(max(0.0,cos(theta-2.65)),6.0)+0.65*pow(max(0.0,cos(theta+0.55)),8.0))*vec3<f32>(0.31,0.48,0.59);
    light+=exp(-pow((radius-0.944)/0.034,2.0))*pow(max(0.0,dot(normalize(p+vec2<f32>(0.00001)),normalize(vec2<f32>(-0.63,-0.77)))),12.0)*vec3<f32>(0.71,0.87,1)*1.18;
    light+=vec3<f32>(0.08,0.15,0.24)*release+vec3<f32>(0.10,0.18,0.28)*exp(-dot(q,q)*1.30)*(1.10+0.42*charge);
    light+=vec3<f32>(0.08,0.16,0.30)*(0.5+0.5*sin(theta*2.0+radius*8.0-time*1.25))*(1.0-smoothstep(0.10,0.90,radius))*0.22;
    light+=vec3<f32>(0.16,0.27,0.46)*pre*(0.55+0.45*sin(time*13.0-radius*13.0+theta*1.6))*(1.0-smoothstep(0.12,0.98,radius))*0.36;
    let birth_pulse=0.5+0.5*sin(time*2.15-radius*3.6);let womb=smoothstep(0.18,0.96,charge)*(1.0-smoothstep(0.03,0.76,open));
    let contraction=0.5+0.5*sin(time*3.05-radius*6.4-theta*0.55);
    light+=vec3<f32>(0.14,0.22,0.38)*birth_pulse*womb*exp(-radius*radius*1.15)*0.55;
    light+=vec3<f32>(1,0.61,0.76)*exp(-dot(q-vec2<f32>(0,0.03),q-vec2<f32>(0,0.03))*0.92)*womb*(0.12+0.16*birth_pulse);
    light+=vec3<f32>(1,0.72,0.86)*exp(-pow(radius-0.34,2.0)/0.020)*womb*contraction*0.15;
    light+=vec3<f32>(0.98,0.53,0.70)*(1.0-smoothstep(0.16,0.88,radius))*womb*pre*(0.10+0.12*contraction);
    for(var i=0;i<16;i++) {
        let fi=f32(i);let speed=0.12+0.035*f32(i%4);let angle=fi*2.399963+time*(speed+0.02*sin(fi));
        let rr=0.16+0.58*fract(fi*0.6180339+0.17);
        let at=rr*vec2<f32>(cos(angle),sin(angle*0.93+0.23))+vec2<f32>(0.10*sin(time*(0.55+0.03*fi)+fi),0.08*cos(time*(0.50+0.04*fi)+fi*1.7));
        let d=q-at;let soft=0.060+0.042*fract(fi*0.712);
        let mote=exp(-dot(d,d)/(soft*soft));let halo=exp(-dot(d,d)/pow(soft*2.9,2.0));let bloom=exp(-dot(d,d)/pow(soft*4.1,2.0));
        let col=max(vec3<f32>(0.7+0.3*sin(fi*1.7),0.55+0.45*sin(fi*2.1+1.0),0.6+0.4*sin(fi*1.3+2.1)),vec3<f32>(0.0));
        light+=col*(mote*0.08+halo*0.17+bloom*0.10)*(0.85+0.15*pow(sin(time*1.2+fi),2.0))*1.6;
    }
    let rgb=display_color(max(light,vec3<f32>(0))/(1.0+max(light,vec3<f32>(0))*0.72));
    let hole=radius*0.5+release*0.034*(sin(theta*3.0-open*4.4)+0.45*sin(theta*5.0+open*3.2));
    let clear=mix(1.0,smoothstep(open*0.83-0.13,open*0.83+0.065,hole),smoothstep(0.02,0.27,open));
    let coverage=alpha*opacity*clear;
    let ignition=1.0+0.34*charge+0.42*pre+select(0.0,0.30*exp(-pow((open-0.12)/0.15,2.0)),open>=0.001);
    let radiance=clamp(rgb,vec3<f32>(0),vec3<f32>(1))*0.912*0.95+emitted*(0.32*1.6*ignition);
    // The reference's hue-preserving shoulder, followed by the native sRGB conversion.
    let peak=max(radiance.x,max(radiance.y,radiance.z));
    let mapped=select(peak,0.93+0.07*(1.0-exp(-(peak-0.93)/0.07)),peak>0.93);
    let color=linear_color(radiance*(mapped/max(peak,0.00001)));
    return vec4<f32>(color*coverage,coverage);
}
