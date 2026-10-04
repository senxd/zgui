struct PaintGeometry {
 matrix:array<vec4<f32>,2>, inverse:array<vec4<f32>,2>, fade_inverse:array<vec4<f32>,2>,
 bounds:vec4<f32>, metadata:vec4<u32>
}
struct PaintClip { inverse:array<vec4<f32>,2>, bounds:vec4<f32>, axes:vec4<u32> }
struct PaintClips { values:array<PaintClip> }
@group(GEOMETRY_GROUP) @binding(0) var<uniform> geometry:PaintGeometry;
@group(GEOMETRY_GROUP) @binding(1) var<storage,read> paint_clips:PaintClips;
fn paint_point(rows:array<vec4<f32>,2>, p:vec2<f32>)->vec2<f32>{
 let q=vec3(p,1.);return vec2(dot(rows[0].xyz,q),dot(rows[1].xyz,q));
}
fn paint_visible(p:vec2<f32>)->bool{
 for(var i=0u;i<geometry.metadata.y;i++){
  let clip=paint_clips.values[geometry.metadata.x+i];let q=paint_point(clip.inverse,p);
  if clip.axes.x!=0u && (q.x<clip.bounds.x || q.x>=clip.bounds.x+clip.bounds.z){return false;}
  if clip.axes.y!=0u && (q.y<clip.bounds.y || q.y>=clip.bounds.y+clip.bounds.w){return false;}
 }
 return true;
}
