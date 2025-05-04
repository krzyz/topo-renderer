const PI = 3.14159265359;
const R0 = 6371000.0;

struct Uniforms {
    projection: mat4x4<f32>,
    normal_projection: mat4x4<f32>,
    camera_pos: vec4<f32>,
    lambda_phi_h: vec3<f32>,
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

fn transform(h: f32, lambda_deg: f32, phi_deg: f32, lambda_0_deg: f32, phi_0_deg: f32) -> vec3<f32> {
    let r = R0 + h;
    let phi = phi_deg / 180.0 * PI;
    let lambda = lambda_deg / 180.0 * PI;
    let phi_0 = phi_0_deg / 180.0 * PI;
    let lambda_0 = lambda_0_deg / 180.0 * PI;
    let dphi = phi - phi_0;
    let dlambda = lambda - lambda_0;
    // y is up
    let x = -r * (sin(dphi) * cos(dlambda) + (1.0 - cos(dlambda)) * sin(phi) * cos(phi_0));
    let y = r * (cos(dphi) * cos(dlambda) + (1.0 - cos(dlambda)) * sin(phi) * sin(phi_0)) - R0;
    let z = r * cos(phi) * sin(dlambda);

    return vec3<f32>(x, y, z);
}

@vertex
fn vs_main(
    model: VertexInput,
) -> VertexOutput {
    var out: VertexOutput;

    let lambda_0 = uniforms.lambda_phi_h.x;
    let phi_0 = uniforms.lambda_phi_h.y;
    let height = uniforms.lambda_phi_h.z;

    let lambda = model.position.x;
    let phi = model.position.z;

    let position = transform(model.position.y - height, lambda, phi, lambda_0, phi_0);

    let view_normal = uniforms.normal_projection * vec4<f32>(model.normal, 1.0);

    out.color = vec3<f32>(1.0, 1.0, 1.0);
    out.world_position = position;
    out.world_normal = 0.5 * (normalize(model.normal.xzy) + vec3<f32>(1.0));

    out.clip_position = uniforms.projection * vec4<f32>(position, 1.0);
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let ambient_strength = 0.1;
    let light_color = vec3<f32>(1.0, 1.0, 1.0);
    let light_position = vec3<f32>(100000.0, 1000000.0, 150000.0);

    let light_dir = normalize(light_position - in.world_position);

    let diffuse_strength = 0.7 * max(dot(in.world_normal, light_dir), 0.0);
    let diffuse_color = light_color * diffuse_strength;


    let ambient_color = light_color * ambient_strength;
    let result = (ambient_color + diffuse_color) * in.color;

    if uniforms.view_mode == 1 {
        return vec4<f32>(in.world_normal, 1.0);
    } else if uniforms.view_mode == 2 {
        return vec4<f32>(light_dir, 1.0);
    } else {
        return vec4<f32>(result, 1.0);
    }
}
