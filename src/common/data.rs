use winit::dpi::PhysicalSize;

#[derive(Copy, Clone, Debug)]
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
