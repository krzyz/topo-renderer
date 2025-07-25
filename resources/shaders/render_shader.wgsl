const PI = 3.14159265359;
const R0 = 6371000.0;

struct Uniforms {
    projection: mat4x4<f32>,
    normal_projection: mat4x4<f32>,
    camera_pos: vec4<f32>,
    sun_direction: vec3<f32>,
    view_mode: i32,
}

@group(0) @binding(0) var<uniform> uniforms: Uniforms;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec3<f32>,
    @location(1) world_position: vec3<f32>,
    @location(2) world_normal: vec3<f32>,
}

@vertex
fn vs_main(
    model: VertexInput,
) -> VertexOutput {
    var out: VertexOutput;

    let position = model.position;

    //let view_normal = uniforms.normal_projection * vec4<f32>(model.normal, 1.0);

    out.color = vec3<f32>(1.0, 1.0, 1.0);
    out.world_position = position;
    out.world_normal = normalize(model.normal.xzy);

    out.clip_position = uniforms.projection * vec4<f32>(position, 1.0);
    return out;
}

fn hash12n(seed: vec2<f32>) -> f32 {
	var p  = fract(seed * vec2<f32>(5.3987, 5.4421));
    p += dot(p.yx, p.xy + vec2<f32>(21.5351, 14.3137));
	return fract(p.x * p.y * 95.4307);
}

fn hash42n(p: vec2<f32>) -> vec3<f32> {
    return vec3<f32>(hash12n(p), hash12n(p + 0.07), hash12n(p + 0.11));
}

fn ditherRGB(color: vec3<f32>, p: vec2<f32>) -> vec3<f32> {
    return color + 1.0 * (hash42n(p) + hash42n(p + 0.13) - 1.0) / 255.0;
}

fn lin2srgb(color: vec3<f32>) -> vec3<f32> {
    let color_lo = 12.92 * color;
    let color_hi = 1.055 * pow(color,vec3<f32>(0.41666)) - 0.055;
    let s = step( vec3<f32>(0.0031308), color);
    return mix( color_lo, color_hi, s );
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let ambient_strength = 0.01;
    let light_color = vec3<f32>(1.0, 1.0, 1.0);
    let light_position = vec3<f32>(100000.0, 1000000.0, 150000.0);

    let diffuse_strength = 0.7 * max(dot(normalize(in.world_normal), uniforms.sun_direction), 0.0);
    let diffuse_color = light_color * diffuse_strength;

    let ambient_color = light_color * ambient_strength;
    let result_lin = (ambient_color + diffuse_color) * in.color;
    //let result_srgb = lin2srgb(result_lin);
    let result = ditherRGB(result_lin, in.world_position.xy);

    if uniforms.view_mode == 2 {
        return vec4<f32>(in.world_normal, 1.0);
        //return vec4<f32>(result_lin, 1.0);
    } else if uniforms.view_mode == 1 {
        return vec4<f32>(result_lin, 1.0);
        //return vec4<f32>(result_srgb, 1.0);
    } else {
        return vec4<f32>(result, 1.0);
    }
}
