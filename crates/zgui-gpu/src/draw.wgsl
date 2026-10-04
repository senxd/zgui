struct Vertex { @location(0) rect:vec4<f32>, @location(1) uv:vec4<f32>, @location(2) color:vec4<f32>, @location(3) fade:vec4<f32>, @location(4) options:vec4<f32>, @location(5) shape:vec4<f32>, @location(6) border:vec4<f32>, @location(7) mask:vec4<f32> }
// Only `uv` and `point` vary across a quad; the rest are per-quad constants.
// Flat, they cost nothing per pixel (a software rasterizer interpolates every
// smooth varying for every pixel it shades).
struct Out { @builtin(position) position:vec4<f32>, @location(0) uv:vec2<f32>, @location(1) @interpolate(flat) color:vec4<f32>, @location(2) point:vec2<f32>, @location(3) @interpolate(flat) fade:vec4<f32>, @location(4) @interpolate(flat) options:vec4<f32>, @location(5) @interpolate(flat) shape:vec4<f32>, @location(6) @interpolate(flat) border:vec4<f32>, @location(7) @interpolate(flat) mask:vec4<f32> }
// Group 1: one immutable buffer per viewport size, so layers never rewrite it.
@group(1) @binding(0) var<uniform> viewport:vec4<f32>;
@group(0) @binding(1) var atlas:texture_2d<f32>;
@group(0) @binding(2) var tex_sampler:sampler;
@vertex fn vs(v:Vertex,@builtin(vertex_index) i:u32)->Out {
 // Corners (0,0) (1,0) (0,1) (0,1) (1,0) (1,1) from bit masks: a runtime
 // index into a local array makes Metal reserve per-thread scratch memory
 // for every thread the GPU can run, hundreds of MB on a large GPU.
 let corner=vec2(f32((50u>>i)&1u),f32((44u>>i)&1u));
 let p=v.rect.xy+corner*v.rect.zw;
 var position=p;
 if v.options.z>0.5 {position=vec2(dot(v.shape.xy,p)+v.border.x,dot(v.shape.zw,p)+v.border.y);}
 var o:Out; o.position=vec4(position.x/viewport.x*2.-1.,1.-position.y/viewport.y*2.,0.,1.);o.uv=v.uv.xy+corner*v.uv.zw;o.color=v.color;o.point=p;o.fade=v.fade;o.options=v.options;o.shape=v.shape;o.border=v.border;o.mask=v.mask;return o;
}
// Gaussian coverage, rather than a Gaussian-shaped distance falloff: the edge
// of a blurred opaque shape has 50% coverage and corners integrate both axes.
fn normal_cdf(x:f32)->f32 {
 let t=1./(1.+0.2316419*abs(x));
 let tail=0.3989422804*exp(-0.5*x*x)*t*(0.319381530+t*(-0.356563782+t*(1.781477937+t*(-1.821255978+t*1.330274429))));
 return select(tail,1.-tail,x>=0.);
}
fn gaussian_shadow(point:vec2<f32>,rect:vec4<f32>,corners:vec4<f32>,sigma:f32)->f32 {
 let p=point-rect.xy;
 if max(max(corners.x,corners.y),max(corners.z,corners.w))<=0. {
   return (normal_cdf((rect.z-p.x)/sigma)-normal_cdf(-p.x/sigma))*(normal_cdf((rect.w-p.y)/sigma)-normal_cdf(-p.y/sigma));
 }
 // Integrate the horizontal Gaussian analytically, and the vertical Gaussian
 // with midpoint quadrature. The finite 4-sigma window drops <0.007% coverage.
 let lo=max(0.,p.y-4.*sigma);let hi=min(rect.w,p.y+4.*sigma);
 if hi<=lo {return 0.;}
 let step=(hi-lo)/24.;var sum=0.;
 for(var i=0;i<24;i++) {
   let y=lo+(f32(i)+0.5)*step;
   var left=0.;var right=rect.z;
   let rl=select(corners.x,corners.w,y>rect.w*0.5);
   let rr=select(corners.y,corners.z,y>rect.w*0.5);
   let edge=min(y,rect.w-y);
   if edge<rl {left=rl-sqrt(max(0.,rl*rl-(rl-edge)*(rl-edge)));}
   if edge<rr {right-=rr-sqrt(max(0.,rr*rr-(rr-edge)*(rr-edge)));}
   let horizontal=normal_cdf((right-p.x)/sigma)-normal_cdf((left-p.x)/sigma);
   let vertical=(y-p.y)/sigma;
   sum+=horizontal*exp(-0.5*vertical*vertical);
 }
 return clamp(sum*step/(sigma*2.5066282746),0.,1.);
}
fn shade(v:Out)->vec4<f32> {
 var color=v.color;
 if v.options.z<0.5 && (v.shape.w>0.5 || v.shape.z>0.) {
   var corner=v.shape.x;
   if v.options.w>0.5 {
     let right=v.point.x>v.fade.x+v.fade.z*0.5; let bottom=v.point.y>v.fade.y+v.fade.w*0.5;
     corner=select(select(v.border.x,v.border.y,right),select(v.border.w,v.border.z,right),bottom);
   }
   let radius=clamp(corner,0.,min(v.fade.z,v.fade.w)*0.5);
   let q=abs(v.point-v.fade.xy-v.fade.zw*0.5)-(v.fade.zw*0.5-vec2(radius));
   let distance=length(max(q,vec2(0.)))+min(max(q.x,q.y),0.)-radius;
   if v.shape.z>0. {
     if v.shape.z<0.01 {color.a*=clamp(0.5-distance,0.,1.);}
     else {color.a*=gaussian_shadow(v.point,v.fade,clamp(v.border,vec4(0.),vec4(min(v.fade.z,v.fade.w)*0.5)),v.shape.z);}
   }
   // Analytic SDF gradient keeps coverage independent of 2x2 fragment groups
   // after an odd-pixel retained scroll, including rounded corners.
   else {let pixel=fwidth(v.point);let outside=max(q,vec2(0.));let norm=length(outside);let axis=select(vec2(0.,1.),vec2(1.,0.),q.x>q.y);let gradient=select(axis,outside/max(norm,0.00001),norm>0.);let aa=max(dot(gradient,pixel),0.5);let outer=clamp(0.5-distance/aa,0.,1.);let inner=clamp(0.5-(distance+v.shape.y)/aa,0.,1.);let alpha=color.a*inner+v.border.a*(outer-inner);let rgb=select(vec3(0.),(color.rgb*color.a*inner+v.border.rgb*v.border.a*(outer-inner))/max(alpha,0.00001),alpha>0.);color=vec4(rgb,alpha);}
 }

 if v.options.y>0.5 {let tex=textureSample(atlas,tex_sampler,v.uv);let rgb=select(tex.rgb,tex.rgb/max(tex.a,0.00001),v.options.y>1.5);color=vec4(color.rgb*rgb,color.a*tex.a);}
 if v.options.x>0. {let distance=min(v.point-v.fade.xy,v.fade.xy+v.fade.zw-v.point); color.a*=clamp(min(distance.x,distance.y)/v.options.x,0.,1.);}
 // A `fade_edges` ancestor: a quadratic ramp over each band, as Zeron fades
 // transcript edges.
 if v.mask.z>0. {let t=clamp((v.point.y-v.mask.x)/v.mask.z,0.,1.); color.a*=t*t;}
 if v.mask.w>0. {let b=clamp((v.mask.y-v.point.y)/v.mask.w,0.,1.); color.a*=b*b;}
 return vec4(color.rgb*color.a,color.a);
}
@fragment fn fs(v:Out)->@location(0) vec4<f32> { return shade(v); }
// Single-pass presentation: the same colour to the retained target and the drawable.
struct Both { @location(0) retained:vec4<f32>, @location(1) drawable:vec4<f32> }
@fragment fn fs_both(v:Out)->Both { let c=shade(v); return Both(c,c); }
// Plain quads (no corners, borders, shadows, fades, masks or transforms):
// most text and backgrounds. A software rasterizer runs a fragment shader on
// the CPU for every pixel, and every branch of `shade` with it, so these use
// this one where drawing is split (see `push_draw`). Same results as `shade`.
// `texture`: 0 for a flat colour, 1 for straight-alpha samples (glyphs), 2
// for premultiplied ones (icons, images), as `options.y`.
struct Basic { @builtin(position) position:vec4<f32>, @location(0) uv:vec2<f32>, @location(1) @interpolate(flat) color:vec4<f32>, @location(2) @interpolate(flat) texture:f32, @location(3) point:vec2<f32>, @location(4) @interpolate(flat) fade:vec4<f32>, @location(5) @interpolate(flat) square:f32 }
@vertex fn vs_basic(@location(0) rect:vec4<f32>,@location(1) uv:vec4<f32>,@location(2) color:vec4<f32>,@location(3) fade:vec4<f32>,@location(4) options:vec4<f32>,@location(5) shape:vec4<f32>,@builtin(vertex_index) i:u32)->Basic {
 let corner=vec2(f32((50u>>i)&1u),f32((44u>>i)&1u));
 let p=rect.xy+corner*rect.zw;
 var o:Basic; o.position=vec4(p.x/viewport.x*2.-1.,1.-p.y/viewport.y*2.,0.,1.);o.uv=uv.xy+corner*uv.zw;o.color=color;o.texture=options.y;o.point=p;o.fade=fade;o.square=shape.w;return o;
}
// Straight samples: colour.rgb * tex.rgb * (colour.a * tex.a). Premultiplied
// ones already carry tex.a in tex.rgb, as `shade`'s divide-then-multiply does.
fn basic(v:Basic)->vec4<f32> {
 var color=v.color;
 if v.square>0.5 {
   let q=abs(v.point-v.fade.xy-v.fade.zw*0.5)-v.fade.zw*0.5;
   let distance=length(max(q,vec2(0.)))+min(max(q.x,q.y),0.);
   let pixel=fwidth(v.point);
   color.a*=clamp(0.5-distance/max(max(pixel.x,pixel.y),0.5),0.,1.);
 }
 if v.texture>0.5 {let tex=textureSample(atlas,tex_sampler,v.uv); let a=color.a*tex.a; return vec4(color.rgb*tex.rgb*select(a,color.a,v.texture>1.5),a);}
 return vec4(color.rgb*color.a,color.a);
}
@fragment fn fs_basic(v:Basic)->@location(0) vec4<f32> { return basic(v); }
@fragment fn fs_basic_both(v:Basic)->Both { let c=basic(v); return Both(c,c); }
