//! A display-agnostic description of one decoded video image.

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PixelLayout {
    /// Packed 4:2:2 YUV, byte order U Y V Y.
    Uyvy,
    /// Packed 8-bit BGRA, byte order B G R A.
    Bgra,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Dimensions {
    pub width: u32,
    pub height: u32,
}

/// Borrowed pixels plus everything needed to interpret them.
pub struct Picture<'pixels> {
    pub layout: PixelLayout,
    pub dimensions: Dimensions,
    pub line_stride: usize,
    pub pixels: &'pixels [u8],
}
