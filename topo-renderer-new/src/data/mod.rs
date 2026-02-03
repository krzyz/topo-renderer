pub mod application_data;
pub mod camera;

use winit::dpi::{PhysicalSize, Pixel};

pub fn pad_256(size: u32) -> u32 {
    ((size - 1) / 256 + 1) * 256
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Size<T> {
    pub width: T,
    pub height: T,
}

impl<T> From<PhysicalSize<T>> for Size<T> {
    fn from(physical_size: PhysicalSize<T>) -> Self {
        Size {
            width: physical_size.width,
            height: physical_size.height,
        }
    }
}

impl<T> From<(T, T)> for Size<T> {
    fn from(value: (T, T)) -> Self {
        Size {
            width: value.0,
            height: value.1,
        }
    }
}

impl<T> From<winit::dpi::LogicalSize<T>> for Size<T>
where
    T: Pixel,
{
    fn from(value: winit::dpi::LogicalSize<T>) -> Self {
        (value.width, value.height).into()
    }
}
