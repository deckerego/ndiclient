//! The picture shown instead of the video while the source sends none, so an
//! idle screen isn't mistaken for one that is switched off. It is a BMP file,
//! the one image format SDL2 reads without an extra library.

use std::path::Path;

use sdl2::surface::Surface;

use crate::picture::Dimensions;

pub struct WaitImage {
    surface: Surface<'static>,
}

impl WaitImage {
    /// Reads the whole file now, so a file that isn't a usable BMP fails at
    /// start-up rather than at the first stall.
    pub fn load(path: &Path) -> Result<Self, String> {
        let surface = Surface::load_bmp(path)
            .map_err(|error| format!("cannot use \"{}\" as wait image: {error}", path.display()))?;
        Ok(Self { surface })
    }

    pub fn surface(&self) -> &Surface<'static> {
        &self.surface
    }

    pub fn dimensions(&self) -> Dimensions {
        Dimensions {
            width: self.surface.width(),
            height: self.surface.height(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sdl2::pixels::PixelFormatEnum;
    use std::path::PathBuf;

    const IMAGE_SIZE: Dimensions = Dimensions {
        width: 4,
        height: 2,
    };

    fn scratch_file(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("ndiclient-{}-{name}", std::process::id()))
    }

    fn saved_bitmap(name: &str) -> PathBuf {
        let path = scratch_file(name);
        Surface::new(IMAGE_SIZE.width, IMAGE_SIZE.height, PixelFormatEnum::RGB24)
            .and_then(|surface| surface.save_bmp(&path))
            .expect("bitmap saved");
        path
    }

    #[test]
    fn bitmap_keeps_its_dimensions() {
        let path = saved_bitmap("valid.bmp");
        let image = WaitImage::load(&path).map(|image| image.dimensions());
        std::fs::remove_file(&path).ok();
        assert_eq!(image, Ok(IMAGE_SIZE));
    }

    #[test]
    fn file_that_is_not_a_bitmap_is_rejected() {
        let path = scratch_file("not-a-bitmap.png");
        std::fs::write(&path, b"\x89PNG\r\n\x1a\n").expect("file written");
        let image = WaitImage::load(&path);
        std::fs::remove_file(&path).ok();
        assert!(image.is_err());
    }

    #[test]
    fn missing_file_is_rejected() {
        assert!(WaitImage::load(&scratch_file("missing.bmp")).is_err());
    }
}
