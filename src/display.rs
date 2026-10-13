//! An SDL2 desktop window that shows a stream of pictures, letterboxed.

use sdl2::EventPump;
use sdl2::event::Event;
use sdl2::keyboard::Keycode;
use sdl2::pixels::PixelFormatEnum;
use sdl2::render::{Canvas, Texture, TextureAccess, TextureCreator};
use sdl2::video::{Window, WindowContext};

use crate::picture::{Dimensions, Picture, PixelLayout};

const INITIAL_WINDOW_SIZE: Dimensions = Dimensions {
    width: 1280,
    height: 720,
};

/// The texture currently on the GPU and the picture shape it was made for.
struct StreamTexture {
    texture: Texture,
    layout: PixelLayout,
    dimensions: Dimensions,
}

impl StreamTexture {
    fn fits(&self, picture: &Picture) -> bool {
        self.layout == picture.layout && self.dimensions == picture.dimensions
    }
}

pub struct VideoWindow {
    canvas: Canvas<Window>,
    texture_creator: TextureCreator<WindowContext>,
    event_pump: EventPump,
    stream_texture: Option<StreamTexture>,
}

impl VideoWindow {
    pub fn open(title: &str) -> Result<Self, String> {
        let sdl = sdl2::init()?;
        sdl2::hint::set("SDL_RENDER_SCALE_QUALITY", "linear");
        let window = sdl
            .video()?
            .window(title, INITIAL_WINDOW_SIZE.width, INITIAL_WINDOW_SIZE.height)
            .resizable()
            .position_centered()
            .build()
            .map_err(|error| error.to_string())?;
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
        self.prepare_texture_for(picture)?;
        let texture = &mut self
            .stream_texture
            .as_mut()
            .expect("texture prepared above")
            .texture;
        texture
            .update(None, picture.pixels, picture.line_stride)
            .map_err(|error| error.to_string())?;
        self.canvas.clear();
        self.canvas.copy(texture, None, None)?;
        self.canvas.present();
        Ok(())
    }

    /// Recreates the texture whenever the source changes resolution or format.
    fn prepare_texture_for(&mut self, picture: &Picture) -> Result<(), String> {
        if self
            .stream_texture
            .as_ref()
            .is_some_and(|current| current.fits(picture))
        {
            return Ok(());
        }
        let Dimensions { width, height } = picture.dimensions;
        let texture = self
            .texture_creator
            .create_texture(
                sdl_pixel_format(picture.layout),
                TextureAccess::Streaming,
                width,
                height,
            )
            .map_err(|error| error.to_string())?;
        self.replace_texture(StreamTexture {
            texture,
            layout: picture.layout,
            dimensions: picture.dimensions,
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

/// SDL names packed formats by their 32-bit word layout, so B G R A in memory
/// on a little-endian CPU is ARGB8888.
fn sdl_pixel_format(layout: PixelLayout) -> PixelFormatEnum {
    match layout {
        PixelLayout::Uyvy => PixelFormatEnum::UYVY,
        PixelLayout::Bgra => PixelFormatEnum::ARGB8888,
    }
}
