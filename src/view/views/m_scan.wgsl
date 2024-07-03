@group(0) @binding(0)
var m_scan_texture_array: binding_array<texture_2d<u32>>;

@group(0) @binding(1)
var m_scan_sampler: sampler;

struct Constants {
    rect: vec4<f32>,
    texture_count: u32,
};

var<push_constant> constants: Constants;

var<private> uvs: array<vec2<f32>, 6> = array<vec2<f32>, 6>(
    vec2<f32>(0.0, 1.0),
    vec2<f32>(1.0, 0.0),
    vec2<f32>(0.0, 0.0),
    vec2<f32>(0.0, 1.0),
    vec2<f32>(1.0, 1.0),
    vec2<f32>(1.0, 0.0),
);

var<private> position_lookup: array<vec2<u32>, 6> = array<vec2<u32>, 6>(
    vec2<u32>(0, 1),
    vec2<u32>(2, 3),
    vec2<u32>(0, 3),
    vec2<u32>(0, 1),
    vec2<u32>(2, 1),
    vec2<u32>(2, 3),
);

struct VertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) idx: u32) -> VertexOut {
    let lookup = position_lookup[idx];

    var out: VertexOut;

    out.position = vec4<f32>(constants.rect[lookup.x], constants.rect[lookup.y], 0.0, 1.0);
    out.uv = uvs[idx];

    return  out;
}

@fragment
fn fs_main(in: VertexOut)  -> @location(0) vec4<f32>{
    let global_x = in.uv.x * f32(constants.texture_count);

    var tex_idx = u32(floor(global_x));
    tex_idx = min(tex_idx, constants.texture_count - 1);

    let tex_x = fract(global_x);

    let pixel_uv = vec2<u32>(vec2<f32>(in.uv.y, tex_x) * vec2<f32>(textureDimensions(m_scan_texture_array[tex_idx])));

    let pixel = textureLoad(m_scan_texture_array[tex_idx], pixel_uv, 0);

    return vec4<f32>(vec3<f32>(pixel.r) / 65535.0, 1.0);
}