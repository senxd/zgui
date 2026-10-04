struct Params { bounds:vec4<f32>, effect:vec4<f32>, mask:vec4<f32> }
@group(0) @binding(0) var filtered:texture_2d<f32>;
@group(0) @binding(1) var original:texture_2d<f32>;
@group(0) @binding(2) var<uniform> params:Params;
@vertex fn vs(@builtin(vertex_index) i:u32)->@builtin(position) vec4<f32>{
 let p=vec2(f32((i<<1u)&2u),f32(i&2u))*2.-1.;return vec4(p,0.,1.);
}
fn composite(p:vec4<f32>)->vec4<f32>{
 let local=vec2<i32>(p.xy-params.effect.zw);var amount=params.effect.x;
 if params.effect.y>0.{let d=min(p.y-params.bounds.y,params.bounds.y+params.bounds.w-p.y);amount*=clamp(d/params.effect.y,0.,1.);}
 if params.mask.z>0.{let t=clamp((p.y-params.mask.x)/params.mask.z,0.,1.);amount*=t*t;}
 if params.mask.w>0.{let b=clamp((params.mask.y-p.y)/params.mask.w,0.,1.);amount*=b*b;}
 return mix(textureLoad(original,local,0),textureLoad(filtered,local,0),amount);
}
@fragment fn fs(@builtin(position) p:vec4<f32>)->@location(0) vec4<f32>{return composite(p);}
struct Both { @location(0) retained:vec4<f32>, @location(1) drawable:vec4<f32> }
@fragment fn fs_both(@builtin(position) p:vec4<f32>)->Both{let c=composite(p);return Both(c,c);}
