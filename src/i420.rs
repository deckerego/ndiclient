//! Repacks packed 4:2:2 UYVY into planar 4:2:0 I420.
//!
//! SDL's OpenGL renderer cannot show UYVY textures natively; it converts them
//! to RGB on the CPU instead. I420 it converts on the GPU, and repacking into
//! it is much cheaper than SDL's conversion. Vertical chroma resolution is
//! halved by averaging each pair of rows.

use crate::picture::{Dimensions, Picture};

const UYVY_BYTES_PER_PIXEL_PAIR: usize = 4;
const UYVY_BLUE_CHROMA: usize = 0;
const UYVY_FIRST_LUMA: usize = 1;
const UYVY_RED_CHROMA: usize = 2;
const UYVY_SECOND_LUMA: usize = 3;
const PIXELS_PER_CHROMA_SAMPLE: usize = 2;

/// An I420 image whose plane buffers are reused from frame to frame.
#[derive(Default)]
pub struct I420Frame {
    dimensions: Dimensions,
    luma: Vec<u8>,
    blue_chroma: Vec<u8>,
    red_chroma: Vec<u8>,
}

impl I420Frame {
    /// An odd last row or column is dropped, because I420 needs even dimensions.
    pub fn fill_from_uyvy(&mut self, picture: &Picture) {
        self.dimensions = even_dimensions(picture.dimensions);
        self.resize_planes();
        for row_pair in 0..self.chroma_height() {
            self.fill_row_pair(picture, row_pair);
        }
    }

    pub fn dimensions(&self) -> Dimensions {
        self.dimensions
    }

    pub fn luma(&self) -> &[u8] {
        &self.luma
    }

    pub fn blue_chroma(&self) -> &[u8] {
        &self.blue_chroma
    }

    pub fn red_chroma(&self) -> &[u8] {
        &self.red_chroma
    }

    pub fn luma_pitch(&self) -> usize {
        self.dimensions.width as usize
    }

    pub fn chroma_pitch(&self) -> usize {
        self.luma_pitch() / PIXELS_PER_CHROMA_SAMPLE
    }

    fn chroma_height(&self) -> usize {
        self.dimensions.height as usize / PIXELS_PER_CHROMA_SAMPLE
    }

    fn resize_planes(&mut self) {
        let luma_size = self.luma_pitch() * self.dimensions.height as usize;
        let chroma_size = self.chroma_pitch() * self.chroma_height();
        self.luma.resize(luma_size, 0);
        self.blue_chroma.resize(chroma_size, 0);
        self.red_chroma.resize(chroma_size, 0);
    }

    fn fill_row_pair(&mut self, picture: &Picture, row_pair: usize) {
        let top_row = row_pair * PIXELS_PER_CHROMA_SAMPLE;
        let width = self.luma_pitch();
        let chroma_pitch = self.chroma_pitch();
        let top_source = uyvy_row(picture, top_row, width);
        let bottom_source = uyvy_row(picture, top_row + 1, width);

        let luma_rows =
            &mut self.luma[top_row * width..(top_row + PIXELS_PER_CHROMA_SAMPLE) * width];
        let (top_luma, bottom_luma) = luma_rows.split_at_mut(width);
        let chroma_range = row_pair * chroma_pitch..(row_pair + 1) * chroma_pitch;
        let blue = &mut self.blue_chroma[chroma_range.clone()];
        let red = &mut self.red_chroma[chroma_range];

        copy_luma(top_source, top_luma);
        copy_luma(bottom_source, bottom_luma);
        average_chroma(top_source, bottom_source, UYVY_BLUE_CHROMA, blue);
        average_chroma(top_source, bottom_source, UYVY_RED_CHROMA, red);
    }
}

fn even_dimensions(dimensions: Dimensions) -> Dimensions {
    Dimensions {
        width: dimensions.width & !1,
        height: dimensions.height & !1,
    }
}

fn uyvy_row<'pixels>(picture: &Picture<'pixels>, row: usize, width: usize) -> &'pixels [u8] {
    let start = row * picture.line_stride;
    let row_bytes = width / PIXELS_PER_CHROMA_SAMPLE * UYVY_BYTES_PER_PIXEL_PAIR;
    &picture.pixels[start..start + row_bytes]
}

fn copy_luma(source: &[u8], luma: &mut [u8]) {
    let (source_pairs, _) = source.as_chunks::<UYVY_BYTES_PER_PIXEL_PAIR>();
    let (luma_pairs, _) = luma.as_chunks_mut::<PIXELS_PER_CHROMA_SAMPLE>();
    for (source_pair, luma_pair) in source_pairs.iter().zip(luma_pairs) {
        *luma_pair = [source_pair[UYVY_FIRST_LUMA], source_pair[UYVY_SECOND_LUMA]];
    }
}

fn average_chroma(top: &[u8], bottom: &[u8], offset: usize, chroma: &mut [u8]) {
    let (top_pairs, _) = top.as_chunks::<UYVY_BYTES_PER_PIXEL_PAIR>();
    let (bottom_pairs, _) = bottom.as_chunks::<UYVY_BYTES_PER_PIXEL_PAIR>();
    for ((top_pair, bottom_pair), sample) in top_pairs.iter().zip(bottom_pairs).zip(chroma) {
        *sample = rounded_average(top_pair[offset], bottom_pair[offset]);
    }
}

fn rounded_average(first: u8, second: u8) -> u8 {
    ((u16::from(first) + u16::from(second)).div_ceil(2)) as u8
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::picture::PixelLayout;

    /// Two pixel pairs per row; values encode position so misplacement shows.
    const FOUR_BY_TWO: [u8; 16] = [
        10, 1, 20, 2, 30, 3, 40, 4, // top row: U Y V Y U Y V Y
        11, 5, 22, 6, 33, 7, 44, 8, // bottom row
    ];

    fn uyvy_picture(pixels: &[u8], width: u32, height: u32, line_stride: usize) -> Picture<'_> {
        Picture {
            layout: PixelLayout::Uyvy,
            dimensions: Dimensions { width, height },
            line_stride,
            pixels,
        }
    }

    fn converted(picture: &Picture) -> I420Frame {
        let mut frame = I420Frame::default();
        frame.fill_from_uyvy(picture);
        frame
    }

    #[test]
    fn luma_is_copied_in_pixel_order() {
        let frame = converted(&uyvy_picture(&FOUR_BY_TWO, 4, 2, 8));
        assert_eq!(frame.luma(), [1, 2, 3, 4, 5, 6, 7, 8]);
    }

    #[test]
    fn blue_chroma_averages_two_rows() {
        let frame = converted(&uyvy_picture(&FOUR_BY_TWO, 4, 2, 8));
        assert_eq!(frame.blue_chroma(), [11, 32]);
    }

    #[test]
    fn red_chroma_averages_two_rows() {
        let frame = converted(&uyvy_picture(&FOUR_BY_TWO, 4, 2, 8));
        assert_eq!(frame.red_chroma(), [21, 42]);
    }

    #[test]
    fn chroma_pitch_is_half_the_width() {
        let frame = converted(&uyvy_picture(&FOUR_BY_TWO, 4, 2, 8));
        assert_eq!(frame.chroma_pitch(), 2);
    }

    #[test]
    fn padding_after_each_row_is_skipped() {
        let padded = [
            10, 1, 20, 2, 0xEE, 0xEE, // top row plus two padding bytes
            10, 3, 20, 4, 0xEE, 0xEE,
        ];
        let frame = converted(&uyvy_picture(&padded, 2, 2, 6));
        assert_eq!(frame.luma(), [1, 2, 3, 4]);
    }

    #[test]
    fn odd_last_row_is_dropped() {
        let three_rows = [10, 1, 20, 2, 10, 3, 20, 4, 10, 5, 20, 6];
        let frame = converted(&uyvy_picture(&three_rows, 2, 3, 4));
        assert_eq!(
            frame.dimensions(),
            Dimensions {
                width: 2,
                height: 2
            }
        );
    }

    #[test]
    fn smaller_frame_shrinks_the_planes() {
        let mut frame = converted(&uyvy_picture(&FOUR_BY_TWO, 4, 2, 8));
        frame.fill_from_uyvy(&uyvy_picture(&FOUR_BY_TWO[..4], 2, 1, 4));
        assert!(frame.luma().is_empty());
    }
}
