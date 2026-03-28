#import bevy_pbr::mesh_functions::{get_world_from_local, mesh_position_local_to_clip}
#import bevy_pbr::mesh_view_bindings::view

struct Vertex {
    @builtin(instance_index) instance_index: u32,
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) color: vec4<f32>,
    @location(4) texture_layer: u32,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_normal: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) color: vec4<f32>,
    @location(3) @interpolate(flat) texture_layer: u32,
};

@group(2) @binding(0) var texture_array: texture_2d_array<f32>;
@group(2) @binding(1) var texture_sampler: sampler;

@vertex
fn vertex(in: Vertex) -> VertexOutput {
    var out: VertexOutput;
    let world_from_local = get_world_from_local(in.instance_index);
    out.clip_position = mesh_position_local_to_clip(
        world_from_local,
        vec4<f32>(in.position, 1.0)
    );
    out.world_normal = (world_from_local * vec4<f32>(in.normal, 0.0)).xyz;
    out.uv = in.uv;
    out.color = in.color;
    out.texture_layer = in.texture_layer;
    return out;
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let tex = textureSample(texture_array, texture_sampler, in.uv, i32(in.texture_layer));
    // in.color.rgb contains the AO-modulated linear color baked by the mesher.
    // When ChunkMaterial is active the vertex color RGB holds the AO factor
    // (1.0 = fully lit, 0.4 = fully occluded). Multiply texture by AO factor.
    let ao = in.color.rgb;
    // Simple diffuse lighting with a fixed sun direction.
    let light_dir = normalize(vec3<f32>(0.4, 1.0, 0.3));
    let n_dot_l = max(dot(normalize(in.world_normal), light_dir), 0.0);
    let ambient = 0.25;
    let light = ambient + (1.0 - ambient) * n_dot_l;
    return vec4<f32>(tex.rgb * ao * light, tex.a);
}
