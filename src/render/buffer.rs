use std::sync::mpsc::Sender;

use crate::{common::data::Size, render::state::Message};

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
        sender: Sender<Message>,
        current_width: u32,
        current_height: u32,
    ) -> bool {
        if !self.mapped {
            self.raw.slice(..).map_async(wgpu::MapMode::Read, move |_| {
                sender
                    .send(Message::DepthBufferReady(Size {
                        width: current_width,
                        height: current_height,
                    }))
                    .expect("Unable to send depth buffer ready message");
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
