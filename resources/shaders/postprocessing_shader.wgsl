var<private> positions: array<vec2f, 6> = array<vec2f, 6>(
    vec2f(-1.0, 1.0),
    vec2f(-1.0, -1.0),
    vec2f(1.0, -1.0),
    vec2f(-1.0, 1.0),
    vec2f(1.0, 1.0),
    vec2f(1.0, -1.0)
);

var<private> uvs: array<vec2f, 6> = array<vec2f, 6>(
    vec2f(0.0, 0.0),
    vec2f(0.0, 1.0),
    vec2f(1.0, 1.0),
    vec2f(0.0, 0.0),
    vec2f(1.0, 0.0),
    vec2f(1.0, 1.0)
);

const NEAR = 50.0;
const FAR = 500000.0;

struct PostprocessingUniforms {
    viewport: vec2f,
    pixelize_n: f32,
    _padding: f32,
}

@group(0) @binding(0)
var t_render: texture_2d<f32>;
@group(0) @binding(1)
var s_render: sampler;
@group(0) @binding(2)
var t_depth: texture_2d<f32>;
@group(0) @binding(3)
var s_depth: sampler;
@group(1) @binding(0)
var<uniform> uniforms: PostprocessingUniforms;

struct VertexInput {
    @location(0) position: vec3f,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4f,
    @location(0) uv: vec2f,
}

fn to_uv(pixel_pos: vec2f) -> vec2f {
    return pixel_pos / uniforms.viewport;
}

fn dist_from_depth(depth: f32) -> f32 {
    return FAR * NEAR / (FAR - depth * (FAR - NEAR));
}

@vertex
fn vs_main(
    @builtin(vertex_index) v_index: u32
) -> VertexOutput {
    var out: VertexOutput;

    out.clip_position = vec4f(positions[v_index], 1.0/4096, 1.0);
    out.uv = uvs[v_index];

    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4f {
    var uv = to_uv(in.clip_position.xy);
    if uniforms.pixelize_n < 99.99999 {
        uv = floor(uv * uniforms.pixelize_n) / uniforms.pixelize_n;
    }
    let render_color = textureSample(t_render, s_render, uv);

    let center_sample_uv = to_uv(in.clip_position.xy);
    let center_depth = textureSample(t_depth, s_depth, center_sample_uv).r;
    let center_linear = dist_from_depth(center_depth);

    var contour = 8.0 * center_linear;

    for (var i = -1; i <= 1; i++) {
        for (var j = -1; j <= 1; j++) {
            if !(i == 0 && j == 0) {
                let sample_uv = to_uv(in.clip_position.xy + vec2f(f32(i), f32(j)));
                let depth = textureSample(t_depth, s_depth, sample_uv).r;

                contour -= dist_from_depth(depth);
            }
        }
    }

    let contour_color = vec4f(0.0, 0.0, 0.0, 1.0);

    return mix(render_color, contour_color, smoothstep(0.05, 0.15, contour/center_linear));
}
