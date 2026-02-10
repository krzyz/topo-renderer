use wgpu::util::DeviceExt;
use winit::event_loop::EventLoopProxy;

use crate::{app::ApplicationEvent, data::DepthState};

// A custom buffer container for dynamic resizing.
pub struct Buffer {
    pub raw: wgpu::Buffer,
    label: &'static str,
    size: u64,
    usage: wgpu::BufferUsages,
    pub mapped: bool,
}

impl Buffer {
    pub fn new(
        device: &wgpu::Device,
        label: &'static str,
        size: u64,
        usage: wgpu::BufferUsages,
    ) -> Self {
        Self {
            raw: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size,
                usage,
                mapped_at_creation: false,
            }),
            label,
            size,
            usage,
            mapped: false,
        }
    }

    pub fn new_init(
        device: &wgpu::Device,
        label: &'static str,
        data: &[u8],
        usage: wgpu::BufferUsages,
    ) -> Self {
        Self {
            raw: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(label),
                contents: data,
                usage,
            }),
            label,
            size: data.len() as u64,
            usage,
            mapped: false,
        }
    }

    pub fn resize(&mut self, device: &wgpu::Device, new_size: u64) {
        if new_size > self.size {
            self.unmap();
            self.raw = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(self.label),
                size: new_size,
                usage: self.usage,
                mapped_at_creation: false,
            });
        }
        self.mapped = false;
    }

    pub fn unmap(&mut self) {
        if self.mapped {
            self.raw.unmap();
            self.mapped = false;
        }
    }

    pub fn map(
        &mut self,
        sender: EventLoopProxy<ApplicationEvent>,
        new_depth_state: DepthState,
    ) -> bool {
        if !self.mapped {
            self.raw.slice(..).map_async(wgpu::MapMode::Read, move |_| {
                let _ = sender.send_event(ApplicationEvent::RenderEvent(
                    super::render_engine::RenderEvent::DepthBufferReady(new_depth_state),
                ));
            });
            self.mapped = true;
            true
        } else {
            false
        }
    }
}

impl Drop for Buffer {
    fn drop(&mut self) {
        self.unmap();
    }
}
