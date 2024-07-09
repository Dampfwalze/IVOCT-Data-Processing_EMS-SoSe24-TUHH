@group(0) @binding(0)
var m_scan_texture_array: binding_array<texture_2d<u32>>;

@group(1) @binding(0)
var color_maps: texture_storage_2d<rgba8unorm, read>;

@group(2) @binding(0)
var<storage, read> b_scan_segments: array<u32>;

struct VertexConstants {
    rect: vec4<f32>,
}

struct PolarConstants {
    _padding: vec4<f32>,
    tex_count: u32,
    map_idx: u32,
    a_scan_count: u32,
};

struct CartesianConstants {
    _padding: vec4<f32>,
    tex_count: u32,
    map_idx: u32,
    b_scan_start: u32,
    b_scan_end: u32,
};

struct SideConstants {
    _padding: vec4<f32>,
    tex_count: u32,
    map_idx: u32,
    view_rot: f32,
};

var<push_constant> vert_consts: VertexConstants;
var<push_constant> polar_consts: PolarConstants;
var<push_constant> cart_consts: CartesianConstants;
var<push_constant> side_consts: SideConstants;

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

    out.position = vec4<f32>(vert_consts.rect[lookup.x], vert_consts.rect[lookup.y], 0.0, 1.0);
    out.uv = uvs[idx];

    return out;
}

@fragment
fn polar_fs_main(in: VertexOut) -> @location(0) vec4<f32>{
    if (polar_consts.tex_count == 0) {
        discard;
    }
    let tex_dim = textureDimensions(m_scan_texture_array[0]);

    let pixel = load_m_scan(
        u32(in.uv.x * f32(polar_consts.a_scan_count)),
        u32(in.uv.y * f32(tex_dim.x)),
        polar_consts.tex_count,
        tex_dim
    );

    return sample_color_map(pixel, polar_consts.map_idx);
}

// Concept:
// let pos = vector from center to fragment
//
// let distance: 0..1 = norm(pos)
// let alpha: 0..1 = ((axes angle of pos) + pi) / 2pi
//
// let a_scan_idx = b_scan_start + alpha * (b_scan_end - b_scan_start)
// let a_scan_sample_idx = distance * a_scan_length
@fragment
fn cartesian_fs_main(in: VertexOut) -> @location(0) vec4<f32>{
    let pi = radians(180.0);
    let two_pi = radians(360.0);

    if (cart_consts.tex_count == 0) {
        discard;
    }
    let tex_dim = textureDimensions(m_scan_texture_array[0]);

    let pos = in.uv * 2.0 - 1.0;

    let distance = length(pos);
    if (distance >= 1.0) {
        discard;
    }

    let alpha = (atan2(pos.x, pos.y) + pi) / two_pi;

    let pixel = load_m_scan(
        cart_consts.b_scan_start + u32(alpha * f32(cart_consts.b_scan_end - cart_consts.b_scan_start)),
        u32(distance * f32(tex_dim.x)),
        cart_consts.tex_count,
        tex_dim
    );

    return sample_color_map(pixel, cart_consts.map_idx);
}

@fragment
fn side_fs_main(in: VertexOut) -> @location(0) vec4<f32>{
    if (side_consts.tex_count == 0) {
        discard;
    }
    let tex_dim = textureDimensions(m_scan_texture_array[0]);

    let b_scan_idx = u32(floor(in.uv.x * f32(arrayLength(&b_scan_segments) - 1)));
    let b_scan_start = b_scan_segments[b_scan_idx];
    let b_scan_end = b_scan_segments[b_scan_idx + 1];
    let b_scan_len = b_scan_end - b_scan_start;

    let rot = (side_consts.view_rot + step(0.5, in.uv.y) * 0.5) % 1.0;

    let a_scan_idx = u32(floor(rot * f32(b_scan_len - 1)));

    let tex_row = u32(floor(f32(tex_dim.x) * abs(in.uv.y - 0.5) * 2.0));

    let pixel = load_m_scan(
        b_scan_start + a_scan_idx,
        tex_row,
        side_consts.tex_count,
        tex_dim
    );

    return sample_color_map(pixel, side_consts.map_idx);
}

/// Load a sample from the m-scan texture array.
fn load_m_scan(a_scan_idx: u32, sample_idx: u32, tex_count: u32, tex_dim: vec2<u32>) -> f32 {
    let tex_idx = a_scan_idx / tex_dim.y;
    let tex_column = a_scan_idx % tex_dim.y;

    if (tex_idx >= tex_count) {
        discard;
    }

    let pixel = textureLoad(m_scan_texture_array[tex_idx], vec2<u32>(sample_idx, tex_column), 0);

    return f32(pixel.r) / 65535.0;
}

fn sample_color_map(value: f32, map_idx: u32) -> vec4<f32> {
    let dims = textureDimensions(color_maps);

    if (map_idx >= dims.y) {
        return vec4<f32>(1.0, 0.0, 1.0, 1.0);
    }

    let col_idx = clamp(value, 0.0, 1.0) * f32(dims.x - 1);

    let lower = textureLoad(color_maps, vec2<u32>(u32(floor(col_idx)), map_idx));
    let upper = textureLoad(color_maps, vec2<u32>(u32(ceil(col_idx)), map_idx));

    let pixel = mix(lower, upper, fract(col_idx));

    return vec4<f32>(pixel.rgb, 1.0);
}