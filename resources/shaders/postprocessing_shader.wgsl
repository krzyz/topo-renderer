var<private> positions: array<vec2<f32>, 6> = array<vec2<f32>, 6>(
    vec2<f32>(-1.0, 1.0),
    vec2<f32>(-1.0, -1.0),
    vec2<f32>(1.0, -1.0),
    vec2<f32>(-1.0, 1.0),
    vec2<f32>(1.0, 1.0),
    vec2<f32>(1.0, -1.0)
);

var<private> uvs: array<vec2<f32>, 6> = array<vec2<f32>, 6>(
    vec2<f32>(0.0, 0.0),
    vec2<f32>(0.0, 1.0),
    vec2<f32>(1.0, 1.0),
    vec2<f32>(0.0, 0.0),
    vec2<f32>(1.0, 0.0),
    vec2<f32>(1.0, 1.0)
);

struct PostprocessingUniforms {
    viewport: vec2<f32>,
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
    @location(0) position: vec3<f32>,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

fn scale_depth(depth: f32) -> f32 {
    let x = 1.0 / depth - 1.0;
    return 1.0 / (5000 * x + 1);
}

fn to_uv(pixel_pos: vec2<f32>) -> vec2<f32> {
    return pixel_pos / uniforms.viewport;
}

@vertex
fn vs_main(
    @builtin(vertex_index) v_index: u32
) -> VertexOutput {
    var out: VertexOutput;

    out.clip_position = vec4<f32>(positions[v_index], 0.0, 1.0);
    out.uv = uvs[v_index];

    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    var uv = to_uv(in.clip_position.xy);
    if uniforms.pixelize_n < 99.99999 {
        uv = floor(uv * uniforms.pixelize_n) / uniforms.pixelize_n;
    }
    let render_color = textureSample(t_render, s_render, uv);
    var contour = 0f;

    for (var i = -1; i <= 1; i++) {
        for (var j = -1; j <= 1; j++) {
            let sample_uv = to_uv(in.clip_position.xy + vec2<f32>(f32(i), f32(j)));
            let depth = textureSample(t_depth, s_depth, sample_uv).r;

            let c = scale_depth(depth);

            if i == 0 && j == 0 {
                contour += 8.0 * c;
            } else {
                contour += -1.0 * c;
            }
        }
    }

    let contour_color = vec4<f32>(0.0, 0.0, 0.0, 1.0);

    return mix(render_color, contour_color, smoothstep(0.05, 0.1, 5.0 * contour));
}
