const PI = 3.14159265359;
const R0 = 6371000.0;

struct Uniforms {
    projection: mat4x4f,
    normal_projection: mat4x4f,
    camera_pos: vec4f,
    sun_direction: vec3f,
    view_mode: i32,
}

struct TerrainUniforms {
    raster_point: vec2f,
    model_point: vec2f,
    pixel_scale: vec2f,
    size: vec2f,
}

@group(0) @binding(0) var<uniform> uniforms: Uniforms;
@group(1) @binding(0) var t_terrain: texture_2d<f32>;
@group(1) @binding(1) var<uniform> terrain_uniforms: TerrainUniforms;

struct VertexInput {
    @location(0) position: vec2u,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4f,
    @location(0) color: vec3f,
    @location(1) world_position: vec3f,
    @location(2) world_normal: vec3f,
}

fn to_model(
    raster: VertexInput,
    transform: TerrainUniforms,
) -> vec2f {
    let model = vec2f(
        (f32(raster.position.x) - transform.raster_point.x) * transform.pixel_scale.x + transform.model_point.x,
        (f32(raster.position.y) - transform.raster_point.y) * -transform.pixel_scale.y + transform.model_point.y,
    );

    return model;
}

@vertex
fn vs_main(
    raster: VertexInput,
) -> VertexOutput {
    var out: VertexOutput;

    let height = textureLoad(t_terrain, raster.position, 0).r;
    let model_position = to_model(raster, terrain_uniforms);
    let longitude= model_position.x * PI / 180.0;
    let latitude = model_position.y * PI / 180.0;

    let R = R0 + height;

    let position = vec3f(
        R * cos(latitude) * cos(longitude),
        R * cos(latitude) * sin(longitude),
        R * sin(latitude),
    );
    
    out.color = vec3f(1.0, 1.0, 1.0);
    out.world_position = position;
    out.world_normal = normalize(position);

    out.clip_position = uniforms.projection * vec4f(position, 1.0);
    return out;
}

fn hash12n(seed: vec2f) -> f32 {
	var p  = fract(seed * vec2f(5.3987, 5.4421));
    p += dot(p.yx, p.xy + vec2f(21.5351, 14.3137));
	return fract(p.x * p.y * 95.4307);
}

fn hash42n(p: vec2f) -> vec3f {
    return vec3f(hash12n(p), hash12n(p + 0.07), hash12n(p + 0.11));
}

fn ditherRGB(color: vec3f, p: vec2f) -> vec3f {
    return color + 1.0 * (hash42n(p) + hash42n(p + 0.13) - 1.0) / 255.0;
}

fn lin2srgb(color: vec3f) -> vec3f {
    let color_lo = 12.92 * color;
    let color_hi = 1.055 * pow(color,vec3f(0.41666)) - 0.055;
    let s = step( vec3f(0.0031308), color);
    return mix( color_lo, color_hi, s );
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4f {
    let ambient_strength = 0.01;
    let light_color = vec3f(1.0, 1.0, 1.0);
    let light_position = vec3f(100000.0, 1000000.0, 150000.0);

    let diffuse_strength = 0.7 * max(dot(normalize(in.world_normal), uniforms.sun_direction), 0.0);
    let diffuse_color = light_color * diffuse_strength;

    let ambient_color = light_color * ambient_strength;
    let result_lin = (ambient_color + diffuse_color) * in.color;
    //let result_srgb = lin2srgb(result_lin);
    let result = ditherRGB(result_lin, in.world_position.xy);

    if uniforms.view_mode == 2 {
        return vec4f(in.world_normal, 1.0);
        //return vec4f(result_lin, 1.0);
    } else if uniforms.view_mode == 1 {
        return vec4f(1.0, 1.0, 1.0, 1.0);
        //return vec4f(result_srgb, 1.0);
    } else {
        return vec4f(result, 1.0);
    }
}
