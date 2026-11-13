struct Uniform {
    image: vec4<u32>, flags: vec4<u32>, field_size: vec4<u32>, geometry: vec4<f32>,
    exposure: vec4<f32>, scale: vec4<f32>,
    curve: array<vec4<f32>,64>,
    displacement: array<vec4<f32>,256>,
}
@group(0) @binding(0) var<storage,read> pixels: array<vec4<f32>>;
@group(0) @binding(1) var<storage,read> field: array<vec4<f32>>;
@group(0) @binding(2) var<storage,read> levels: array<vec4<u32>>;
@group(0) @binding(3) var<uniform> u: Uniform;
@group(0) @binding(4) var<storage,read_write> output: array<vec4<f32>>;
fn texel(p:vec2<i32>,level:u32)->vec4<f32> {
    let desc=levels[level]; let size=vec2<i32>(desc.yz);
    if u.flags.x==0u && (any(p<vec2<i32>(0)) || any(p>=size)) {return vec4<f32>(0.);}
    let q=vec2<u32>(clamp(p,vec2<i32>(0),size-vec2<i32>(1)));
    return pixels[desc.x+q.y*desc.y+q.x];
}
fn cubic(t:f32)->vec4<f32> {
    return vec4<f32>((1.-t)*(1.-t)*(1.-t),3.*t*t*t-6.*t*t+4.,-3.*t*t*t+3.*t*t+3.*t+1.,t*t*t)/6.;
}
fn sample_at(pos:vec2<f32>,level:u32)->vec4<f32> {
    if u.flags.y==0u {return texel(vec2<i32>(floor(floor(pos*4096.+vec2<f32>(0.5))/4096.+vec2<f32>(0.5))),level);}
    let base=vec2<i32>(floor(pos));let t=fract(pos);
    if u.flags.y==1u {return mix(mix(texel(base,level),texel(base+vec2<i32>(1,0),level),t.x),mix(texel(base+vec2<i32>(0,1),level),texel(base+vec2<i32>(1,1),level),t.x),t.y);}
    let wx=cubic(t.x);let wy=cubic(t.y);var result=vec4<f32>(0.);
    for(var y=0;y<4;y++) {for(var x=0;x<4;x++) {result+=texel(base+vec2<i32>(x-1,y-1),level)*wx[x]*wy[y];}}
    return result;
}
fn field_texel(p:vec2<i32>)->vec4<f32> {
    let q=vec2<u32>(clamp(p,vec2<i32>(0),vec2<i32>(u.field_size.xy)-vec2<i32>(1)));
    return field[q.y*u.field_size.x+q.x];
}
fn field_at(pos:vec2<f32>)->vec4<f32> {
    let q=pos*vec2<f32>(u.field_size.xy-vec2<u32>(1))/vec2<f32>(max(u.image.xy,vec2<u32>(2))-vec2<u32>(1));
    let p=vec2<i32>(floor(q));let t=fract(q);
    return mix(mix(field_texel(p),field_texel(p+vec2<i32>(1,0)),t.x),mix(field_texel(p+vec2<i32>(0,1)),field_texel(p+vec2<i32>(1,1)),t.x),t.y);
}
fn direction(pos:vec2<f32>)->vec2<f32> {let f=field_at(pos).xy;return f/max(length(f/u.scale.xy),1e-8);}
fn advance(pos:vec2<f32>,step:f32)->vec2<f32> {let a=direction(pos);return pos+direction(pos+a*step*0.5)*step;}
fn weight(t:f32)->f32 {
    if u.flags.z==1u {return max(0.,1.-abs(2.*t));}
    if u.flags.z==2u {let v=cos(3.141592653589793*t);return v*v;}
    if u.flags.z==3u {
        let x=clamp(0.5-t,0.,1.)*255.;let a=u32(floor(x));let b=min(a+1u,255u);
        return mix(u.curve[a/4u][a%4u],u.curve[b/4u][b%4u],x-f32(a));
    }
    return 1.;
}
@compute @workgroup_size(8,8)
fn main(@builtin(global_invocation_id) id:vec3<u32>) {
    if any(id.xy>=u.image.xy) {return;}
    let index=id.y*u.image.x+id.x;let pos=vec2<f32>(id.xy);let f=field_at(pos);let amount=u.geometry.x*clamp(f.z,0.,1.);
    if amount<=1e-6 {output[index]=pixels[index];return;}
    let mode=u.image.z;
    if mode>=18u {
        var lod=clamp(log2(1.+amount*min(u.scale.x,u.scale.y)),0.,f32(u.flags.w-1u));if mode==19u {lod=floor(lod+0.5);}
        let a=u32(floor(lod));let b=min(a+1u,u.flags.w-1u);
        let pa=(pos+vec2<f32>(0.5))*vec2<f32>(levels[a].yz)/vec2<f32>(u.image.xy)-vec2<f32>(0.5);
        let pb=(pos+vec2<f32>(0.5))*vec2<f32>(levels[b].yz)/vec2<f32>(u.image.xy)-vec2<f32>(0.5);
        output[index]=mix(sample_at(pa,a),sample_at(pb,b),lod-f32(a));return;
    }
    var sum=vec4<f32>(0.);var total=0.;let n=u.image.w;
    if mode==5u {
        let half=(n+1u)/2u;let dt=amount/(2.*f32(half));var center=pos;let shift=amount*u.exposure.y;
        let steps=u32(max(1.,ceil(abs(shift)/2.)));
        for(var k=0u;k<steps;k++) {center=advance(center,shift/f32(steps));}
        for(var side=0u;side<2u;side++) {
            let sign=select(-1.,1.,side==1u);var q=advance(center,sign*dt*0.5);
            for(var j=0u;j<half;j++) {let t=sign*(f32(j)+0.5)/(2.*f32(half));let w=weight(t);sum+=sample_at(q,0u)*w;total+=w;q=advance(q,sign*dt);}
        }
    } else {
        for(var j=0u;j<n;j++) {
            let t=(f32(j)+0.5)/f32(n)-0.5;let v=t+u.exposure.y;var q=pos;
            switch mode {
                case 0u: {q+=vec2<f32>(cos(u.geometry.y),sin(u.geometry.y))*amount*v*u.scale.xy;}
                case 1u: {let theta=u.geometry.y*v;let a=(pos-u.geometry.zw)/u.scale.xy;q=u.geometry.zw+vec2<f32>(a.x*cos(theta)-a.y*sin(theta),a.x*sin(theta)+a.y*cos(theta))*u.scale.xy;}
                case 2u: {q=u.geometry.zw+(pos-u.geometry.zw)*exp(amount*0.01*v);}
                case 3u,4u: {q+=f.xy*u.exposure.x*v;}
                case 17u: {let pair=u.displacement[j/2u];let direction=select(pair.xy,pair.zw,(j%2u)==1u);q+=direction*amount*u.scale.xy;}
                default: {}
            }
            let w=select(weight(t),1.,mode==17u);sum+=sample_at(q,0u)*w;total+=w;
        }
    }
    output[index]=select(pixels[index],sum/max(total,1e-8),total>1e-8);
}
