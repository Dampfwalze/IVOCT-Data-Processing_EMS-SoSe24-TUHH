@group(0) @binding(0)
var m_scan_texture_array: binding_array<texture_2d<u32>>;

@group(1) @binding(0)
var<storage, read> b_scan_segments: array<u32>;

struct PolarConstants {
    rect: vec4<f32>,
    texture_count: u32,
    view_rotation: f32,
};

struct CartesianConstants {
    b_scan_start: u32,
    b_scan_end: u32,
    texture_count: u32,
};

var<push_constant> polar_constants: PolarConstants;
var<push_constant> cartesian_constants: CartesianConstants;

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
fn polar_vs_main(@builtin(vertex_index) idx: u32) -> VertexOut {
    let lookup = position_lookup[idx];

    var out: VertexOut;

    out.position = vec4<f32>(polar_constants.rect[lookup.x], polar_constants.rect[lookup.y], 0.0, 1.0);
    out.uv = uvs[idx];

    return out;
}

@fragment
fn polar_fs_main(in: VertexOut)  -> @location(0) vec4<f32>{
    return vec4<f32>(vec3<f32>(sample_m_scan(in.uv, polar_constants.texture_count)), 1.0);
}

@vertex
fn cartesian_vs_main(@builtin(vertex_index) idx: u32) -> VertexOut {
    var out: VertexOut;

    out.position = vec4<f32>(uvs[idx] * 2.0 - 1.0, 0.0, 1.0);
    out.uv = uvs[idx];

    return out;
}

@fragment
fn cartesian_fs_main(in: VertexOut)  -> @location(0) vec4<f32>{
    let pi = radians(180.0);
    let two_pi = radians(360.0);

    if (cartesian_constants.texture_count == 0) {
        discard;
    }

    let pos = in.uv * 2.0 - 1.0;

    let distance = distance(pos, vec2<f32>(0.0));

    if (distance > 1.0) {
        discard;
    }

    let alpha = (atan2(pos.x, pos.y) + pi) / two_pi;

    let a_scan_idx = cartesian_constants.b_scan_start + u32(alpha * f32(cartesian_constants.b_scan_end - cartesian_constants.b_scan_start));

    let tex_dim = textureDimensions(m_scan_texture_array[0]);

    let tex_idx = a_scan_idx / tex_dim.y;

    if (tex_idx >= cartesian_constants.texture_count) {
        discard;
    }

    let tex_column = a_scan_idx % tex_dim.y;

    let tex_row = u32(distance * f32(tex_dim.x));

    let pixel = textureLoad(m_scan_texture_array[tex_idx], vec2<u32>(tex_row, tex_column), 0);

    return vec4<f32>(vec3<f32>(f32(pixel.r) / 65535.0), 1.0);
}

@fragment
fn side_fs_main(in: VertexOut)  -> @location(0) vec4<f32>{
    if (polar_constants.texture_count == 0) {
        discard;
    }
    let b_scan_idx = u32(floor(in.uv.x * f32(arrayLength(&b_scan_segments) - 1)));
    let b_scan_start = b_scan_segments[b_scan_idx];
    let b_scan_end = b_scan_segments[b_scan_idx + 1];
    let b_scan_len = b_scan_end - b_scan_start;

    let rot = (polar_constants.view_rotation + step(0.5, in.uv.y) * 0.5) % 1.0;

    let a_scan_idx = u32(floor(rot * f32(b_scan_len - 1)));

    let tex_dims = textureDimensions(m_scan_texture_array[0]);

    let tex_row = u32(floor(f32(tex_dims.x) * abs(in.uv.y - 0.5) * 2.0));

    let global_a_scan_idx = b_scan_start + a_scan_idx;
    let tex_idx = global_a_scan_idx / tex_dims.y;
    let tex_column = global_a_scan_idx % tex_dims.y;

    if (tex_idx >= polar_constants.texture_count) {
        discard;
    }

    let pixel = textureLoad(m_scan_texture_array[tex_idx], vec2<u32>(tex_row, tex_column), 0);

    return vec4<f32>(vec3<f32>(f32(pixel.r) / 65535.0), 1.0);
}

fn sample_m_scan(uv: vec2<f32>, texture_count: u32) -> f32 {
    let global_x = uv.x * f32(texture_count);

    var tex_idx = u32(floor(global_x));
    tex_idx = min(tex_idx, texture_count - 1);

    let tex_x = fract(global_x);

    let pixel_uv = vec2<u32>(vec2<f32>(uv.y, tex_x) * vec2<f32>(textureDimensions(m_scan_texture_array[tex_idx])));

    let pixel = textureLoad(m_scan_texture_array[tex_idx], pixel_uv, 0);

    return f32(pixel.r) / 65535.0;
}