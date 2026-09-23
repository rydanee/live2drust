struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) opacity: f32,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) opacity: f32,
};

struct Live2DUniforms {
    projection: mat4x4<f32>,
    base_color: vec4<f32>,
};

@group(0) @binding(0)
var<uniform> uniforms: Live2DUniforms;

@vertex
fn vs_main(model: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = uniforms.projection * vec4<f32>(model.position, 0.0, 1.0);
    out.uv = model.uv;
    out.opacity = model.opacity;
    return out;
}

@group(1) @binding(0) var t_texture: texture_2d<f32>;
@group(1) @binding(1) var s_sampler: sampler;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
  let tex_color = textureSample(t_texture, s_sampler, in.uv);
    
  if (tex_color.a < 0.1) {
    discard;
  }
    
  return tex_color * uniforms.base_color * in.opacity;
}
