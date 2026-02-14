const R0 = 6371000.0;

struct TerrainUniforms {
    raster_point: vec2f,
    model_point: vec2f,
    pixel_scale: vec2f,
    size: vec2f,
    normal_to_world_rotation: mat3x3f,
}

// lt means "left" or "top", rb means "right" or "bottom"
@group(0) @binding(0) var terrain_heightmap_lt: texture_2d<f32>;
@group(0) @binding(1) var terrain_heightmap_rt: texture_2d<f32>;
@group(0) @binding(2) var terrain_heightmap_lb: texture_2d<f32>;
@group(0) @binding(3) var terrain_heightmap_rb: texture_2d<f32>;
@group(0) @binding(4) var calculated_normals_lt: texture_storage_2d<rgba8unorm, write>;
@group(0) @binding(5) var calculated_normals_rt: texture_storage_2d<rgba8unorm, write>;
@group(0) @binding(6) var calculated_normals_lb: texture_storage_2d<rgba8unorm, write>;
@group(0) @binding(7) var calculated_normals_rb: texture_storage_2d<rgba8unorm, write>;
@group(0) @binding(8) var<uniform> terrain_uniforms_lt: TerrainUniforms;

fn to_latitude(
    position_y: i32,
    transform: TerrainUniforms,
) -> f32 {
    return (f32(position_y) - transform.raster_point.y) * -transform.pixel_scale.y + transform.model_point.y;
}

@compute
@workgroup_size(1)
fn compute_normals_corner() {
    let dimensions = vec2i(textureDimensions(terrain_heightmap_lt));

    // bottom-right corner of the top-left patch: x is dimensions.x - 1, y is dimensions.y - 1 
    // bottom-left corner of the top-right patch: x is 0, y is dimensions.y - 1 
    // top-right corner of the bottom-left patch: x is dimensions.x - 1, y 0 
    // top-left corner of the bottom-right patch: x is 0, y 0 

    let latitude = to_latitude(dimensions.y - 1, terrain_uniforms_lt);

    let x = radians(abs(terrain_uniforms_lt.pixel_scale.x)) * R0;
    let y = radians(abs(terrain_uniforms_lt.pixel_scale.y)) * R0 * cos(radians(latitude));

    let coords_top_left = vec2i(dimensions.x - 1, dimensions.y - 1);
    let coords_top_right = vec2i(0, dimensions.y - 1);
    let coords_bottom_left = vec2i(dimensions.x - 1, 0);
    let coords_bottom_right = vec2i(0, 0);

    let center = vec3f(0, 0, textureLoad(terrain_heightmap_lt, coords_top_left, 0).r);
    let top_left = vec3f(-x, -y, textureLoad(terrain_heightmap_lt, coords_top_left + vec2i(-1, -1), 0).r);
    let top = vec3f(0, -y, textureLoad(terrain_heightmap_lt, coords_top_left + vec2i(0, -1), 0).r);
    let top_right = vec3f(x, -y, textureLoad(terrain_heightmap_rt, coords_top_right + vec2i(1, -1), 0).r);
    let left = vec3f(x, 0, textureLoad(terrain_heightmap_lt, coords_top_left + vec2i(-1, 0), 0).r);
    let right = vec3f(x, 0, textureLoad(terrain_heightmap_rt, coords_top_right + vec2i(1, 0), 0).r);
    let bottom_left = vec3f(-x, y, textureLoad(terrain_heightmap_lb, coords_bottom_left + vec2i(-1, 1), 0).r);
    let bottom = vec3f(0, y, textureLoad(terrain_heightmap_lb, coords_bottom_left + vec2i(0, 1), 0).r);
    let bottom_right = vec3f(x, y, textureLoad(terrain_heightmap_rb, coords_bottom_right + vec2i(1, 1), 0).r);

    let normal =
        contribution(center, left, top)
        + 0.5 * contribution(center, top, top_right)
        + 0.5 * contribution(center, top_right, right)
        + contribution(center, right, bottom_right)
        + 0.5 * contribution(center, bottom, bottom_left)
        + 0.5 * contribution(center, bottom_left, left);

    textureStore(calculated_normals_lt, coords_top_left, vec4f(normalize(normal), 0.0));
    textureStore(calculated_normals_rt, coords_top_right, vec4f(normalize(normal), 0.0));
    textureStore(calculated_normals_lb, coords_bottom_left, vec4f(normalize(normal), 0.0));
    textureStore(calculated_normals_rb, coords_bottom_right, vec4f(normalize(normal), 0.0));
}

fn contribution(h0: vec3f, h1: vec3f, h2: vec3f) -> vec3f {
    let side1 = h1 - h0;
    let side2 = h2 - h1;

    return cross(side1, side2);
}
