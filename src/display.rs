//! An SDL2 desktop window that shows a stream of pictures, letterboxed.

use sdl2::EventPump;
use sdl2::event::Event;
use sdl2::keyboard::Keycode;
use sdl2::pixels::PixelFormatEnum;
use sdl2::render::{Canvas, Texture, TextureAccess, TextureCreator};
use sdl2::video::{Window, WindowContext};

use crate::i420::I420Frame;
use crate::picture::{Dimensions, Picture, PixelLayout};

const INITIAL_WINDOW_SIZE: Dimensions = Dimensions {
    width: 1280,
    height: 720,
};

/// SDL names packed formats by their 32-bit word layout, so B G R A in memory
/// on a little-endian CPU is ARGB8888.
const BGRA_TEXTURE_FORMAT: PixelFormatEnum = PixelFormatEnum::ARGB8888;

/// The texture currently on the GPU and the image shape it was made for.
struct StreamTexture {
    texture: Texture,
    format: PixelFormatEnum,
    dimensions: Dimensions,
}

impl StreamTexture {
    fn fits(&self, format: PixelFormatEnum, dimensions: Dimensions) -> bool {
        self.format == format && self.dimensions == dimensions
    }
}

/// Whether the video opens in a resizable window or covers the whole screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowMode {
    Windowed,
    Fullscreen,
}

pub struct VideoWindow {
    canvas: Canvas<Window>,
    texture_creator: TextureCreator<WindowContext>,
    event_pump: EventPump,
    stream_texture: Option<StreamTexture>,
    i420_frame: I420Frame,
}

impl VideoWindow {
    pub fn open(title: &str, mode: WindowMode) -> Result<Self, String> {
        let sdl = sdl2::init()?;
        sdl2::hint::set("SDL_RENDER_SCALE_QUALITY", "linear");
        let video = sdl.video()?;
        let mut builder =
            video.window(title, INITIAL_WINDOW_SIZE.width, INITIAL_WINDOW_SIZE.height);
        builder.position_centered();
        match mode {
            WindowMode::Windowed => builder.resizable(),
            // Desktop fullscreen keeps the current display mode, so no mode switch happens.
            WindowMode::Fullscreen => builder.fullscreen_desktop(),
        };
        let window = builder.build().map_err(|error| error.to_string())?;
        let mut canvas = window
            .into_canvas()
            .accelerated()
            .present_vsync()
            .build()
            .map_err(|error| error.to_string())?;
        // Wayland only maps a window once something is presented, so show
        // black until the first frame arrives.
        canvas.clear();
        canvas.present();
        let texture_creator = canvas.texture_creator();
        let event_pump = sdl.event_pump()?;
        Ok(Self {
            canvas,
            texture_creator,
            event_pump,
            stream_texture: None,
            i420_frame: I420Frame::default(),
        })
    }

    /// Drains pending window events; true once the user closes the window or presses Escape.
    pub fn is_close_requested(&mut self) -> bool {
        self.event_pump.poll_iter().any(|event| {
            matches!(
                event,
                Event::Quit { .. }
                    | Event::KeyDown {
                        keycode: Some(Keycode::Escape),
                        ..
                    }
            )
        })
    }

    pub fn show(&mut self, picture: &Picture) -> Result<(), String> {
        match picture.layout {
            PixelLayout::Uyvy => self.upload_as_i420(picture)?,
            PixelLayout::Bgra => self.upload_bgra(picture)?,
        }
        self.present()
    }

    fn upload_as_i420(&mut self, picture: &Picture) -> Result<(), String> {
        self.i420_frame.fill_from_uyvy(picture);
        self.prepare_texture(PixelFormatEnum::IYUV, self.i420_frame.dimensions())?;
        let frame = &self.i420_frame;
        prepared_texture(&mut self.stream_texture)
            .update_yuv(
                None,
                frame.luma(),
                frame.luma_pitch(),
                frame.blue_chroma(),
                frame.chroma_pitch(),
                frame.red_chroma(),
                frame.chroma_pitch(),
            )
            .map_err(|error| error.to_string())
    }

    fn upload_bgra(&mut self, picture: &Picture) -> Result<(), String> {
        self.prepare_texture(BGRA_TEXTURE_FORMAT, picture.dimensions)?;
        prepared_texture(&mut self.stream_texture)
            .update(None, picture.pixels, picture.line_stride)
            .map_err(|error| error.to_string())
    }

    fn present(&mut self) -> Result<(), String> {
        let texture = &self
            .stream_texture
            .as_ref()
            .expect("texture prepared by the upload")
            .texture;
        self.canvas.clear();
        self.canvas.copy(texture, None, None)?;
        self.canvas.present();
        Ok(())
    }

    /// Recreates the texture whenever the source changes resolution or format.
    fn prepare_texture(
        &mut self,
        format: PixelFormatEnum,
        dimensions: Dimensions,
    ) -> Result<(), String> {
        if self
            .stream_texture
            .as_ref()
            .is_some_and(|current| current.fits(format, dimensions))
        {
            return Ok(());
        }
        let Dimensions { width, height } = dimensions;
        let texture = self
            .texture_creator
            .create_texture(format, TextureAccess::Streaming, width, height)
            .map_err(|error| error.to_string())?;
        self.replace_texture(StreamTexture {
            texture,
            format,
            dimensions,
        });
        // Logical size makes SDL scale the picture to the window while keeping its aspect ratio.
        self.canvas
            .set_logical_size(width, height)
            .map_err(|error| error.to_string())
    }

    fn replace_texture(&mut self, replacement: StreamTexture) {
        if let Some(previous) = self.stream_texture.replace(replacement) {
            // SAFETY: The renderer that owns the texture is still alive.
            unsafe { previous.texture.destroy() };
        }
    }
}

fn prepared_texture(stream_texture: &mut Option<StreamTexture>) -> &mut Texture {
    &mut stream_texture
        .as_mut()
        .expect("texture prepared before upload")
        .texture
}
