@group(0) @binding(0) var terrain_heightmap: texture_2d<f32>;
@group(0) @binding(1) var calculated_normals: texture_storage_2d<rgba8unorm, write>;

@compute
@workgroup_size(16, 16)
fn compute_normals(
    @builtin(global_invocation_id) global_id: vec3u,
) {
    let dimensions = vec2i(textureDimensions(terrain_heightmap));
    let coords = vec2i(global_id.xy);

    if (coords.x >= dimensions.x - 1 || coords.y >= dimensions.y - 1 || coords.x < 1 || coords.y < 1) {
        return;
    }

    let x = 90.0;
    let y = 90.0;

    let center = vec3f(0, 0, textureLoad(terrain_heightmap, coords.xy, 0).r);
    let top_left = vec3f(-x, -y, textureLoad(terrain_heightmap, coords.xy + vec2i(-1, -1), 0).r);
    let top = vec3f(0, -y, textureLoad(terrain_heightmap, coords.xy + vec2i(0, -1), 0).r);
    let top_right = vec3f(x, -y, textureLoad(terrain_heightmap, coords.xy + vec2i(1, -1), 0).r);
    let left = vec3f(x, 0, textureLoad(terrain_heightmap, coords.xy + vec2i(-1, 0), 0).r);
    let right = vec3f(x, 0, textureLoad(terrain_heightmap, coords.xy + vec2i(1, 0), 0).r);
    let bottom_left = vec3f(-x, y, textureLoad(terrain_heightmap, coords.xy + vec2i(-1, 1), 0).r);
    let bottom = vec3f(0, y, textureLoad(terrain_heightmap, coords.xy + vec2i(0, 1), 0).r);
    let bottom_right = vec3f(x, y, textureLoad(terrain_heightmap, coords.xy + vec2i(1, 1), 0).r);

    let normal =
        contribution(center, left, top)
        + 0.5 * contribution(center, top, top_right)
        + 0.5 * contribution(center, top_right, right)
        + contribution(center, right, bottom_right)
        + 0.5 * contribution(center, bottom, bottom_left)
        + 0.5 * contribution(center, bottom_left, left);

    textureStore(calculated_normals, coords.xy, vec4f(normalize(normal), 0.0));
    //textureStore(calculated_normals, coords.xy, vec4f(1.0, 1.0, 0.0, 0.0));
}

fn contribution(h0: vec3f, h1: vec3f, h2: vec3f) -> vec3f {
    let side1 = h1 - h0;
    let side2 = h2 - h1;

    return cross(side1, side2);
}
