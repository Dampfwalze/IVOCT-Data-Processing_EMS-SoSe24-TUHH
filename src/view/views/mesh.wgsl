struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) normal: vec3<f32>,
};

struct Constants {
    mvp: mat4x4<f32>,
};

var<push_constant> consts: Constants;


@vertex
fn vs_main(
    model: VertexInput,
) -> VertexOutput {
    var out: VertexOutput;
    let normal = consts.mvp * vec4<f32>(model.normal, 0.0);
    out.normal = normalize(normal.xyz);
    out.clip_position = consts.mvp * vec4<f32>(model.position, 1.0);
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let light = normalize(vec3<f32>(0.5, 0.5, -1.0));
    let normal = normalize(in.normal);

    let intensity = max(dot(normal, light), 0.0);

    let col = mix(vec3<f32>(0.4), vec3<f32>(0.7), intensity);

    return vec4<f32>(col, 1.0);
}