struct Primitive {
    width: f32,
    res_width: f32,
    res_height: f32,
};

@group(0) @binding(0) var<uniform> primitive: Primitive;

struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) normal: vec2<f32>,
    @location(2) color: vec3<f32>,
    @location(3) z_index: i32,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec3<f32>,
}

@vertex
fn vs_main(
    in: VertexInput,
) -> VertexOutput {
    var out: VertexOutput;

    var z = f32(in.z_index) / 4096.0;

    var invert_y = vec2<f32>(1.0, -1.0);

    var position = (in.position + in.normal * primitive.width) * invert_y;
    out.clip_position = vec4<f32>(2.0 * position.x / primitive.res_width - 1.0, 2.0 * position.y / primitive.res_height + 1.0, z, 1.0);
    out.color = in.color;

    return out;
}  

 
@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return vec4<f32>(in.color, 1.0);
}
 
         
         
         
         
         
         
         
         
         
         
         
