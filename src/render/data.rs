pub struct Vertex {
    pub position: glam::Vec3,
    pub normal: glam::Vec3,
}

impl Vertex {
    const ATTRIBS: [wgpu::VertexAttribute; 2] = wgpu::vertex_attr_array![
        // position
        0 => Float32x3,
        // normal
        1 => Float32x3
    ];

    pub fn new(position: glam::Vec3, normal: glam::Vec3) -> Self {
        Self { position, normal }
    }

    pub fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBS,
        }
    }
}
