//! An SDL2 desktop window that shows a stream of pictures, letterboxed.
//! F11 or F switches between windowed and fullscreen; fullscreen hides the
//! mouse cursor so the screen can serve as a kiosk display. While the
//! source sends no video, an optional wait image can cover the picture.

use sdl2::event::Event;
use sdl2::keyboard::Keycode;
use sdl2::mouse::MouseUtil;
use sdl2::pixels::PixelFormatEnum;
use sdl2::render::{Canvas, Texture, TextureAccess, TextureCreator};
use sdl2::video::{FullscreenType, Window, WindowContext};
use sdl2::{EventPump, Sdl};

use crate::i420::I420Frame;
use crate::picture::{Dimensions, Picture, PixelLayout};
use crate::wait_image::WaitImage;

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

impl WindowMode {
    fn toggled(self) -> Self {
        match self {
            Self::Windowed => Self::Fullscreen,
            Self::Fullscreen => Self::Windowed,
        }
    }

    /// Desktop fullscreen keeps the current display mode, so no mode switch happens.
    fn fullscreen_type(self) -> FullscreenType {
        match self {
            Self::Windowed => FullscreenType::Off,
            Self::Fullscreen => FullscreenType::Desktop,
        }
    }

    /// A kiosk display shows only the picture, so fullscreen hides the pointer.
    fn shows_cursor(self) -> bool {
        self == Self::Windowed
    }
}

/// Whether the viewer still wants to watch after the latest window events.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowStatus {
    Open,
    CloseRequested,
}

/// What the viewer asked for through a window event.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum UserRequest {
    Close,
    ToggleFullscreen,
}

fn user_request(event: &Event) -> Option<UserRequest> {
    match event {
        Event::Quit { .. } => Some(UserRequest::Close),
        Event::KeyDown {
            keycode: Some(keycode),
            repeat: false,
            ..
        } => key_request(*keycode),
        _ => None,
    }
}

fn key_request(keycode: Keycode) -> Option<UserRequest> {
    match keycode {
        Keycode::Escape => Some(UserRequest::Close),
        Keycode::F11 | Keycode::F => Some(UserRequest::ToggleFullscreen),
        _ => None,
    }
}

pub struct VideoWindow {
    canvas: Canvas<Window>,
    texture_creator: TextureCreator<WindowContext>,
    event_pump: EventPump,
    mouse: MouseUtil,
    mode: WindowMode,
    stream_texture: Option<StreamTexture>,
    i420_frame: I420Frame,
    wait_image: Option<WaitImage>,
    wait_texture: Option<Texture>,
}

impl VideoWindow {
    pub fn open(
        sdl: &Sdl,
        title: &str,
        mode: WindowMode,
        wait_image: Option<WaitImage>,
    ) -> Result<Self, String> {
        sdl2::hint::set("SDL_RENDER_SCALE_QUALITY", "linear");
        let video = sdl.video()?;
        // Resizable also in fullscreen, so the window can be resized after toggling out.
        let window = video
            .window(title, INITIAL_WINDOW_SIZE.width, INITIAL_WINDOW_SIZE.height)
            .position_centered()
            .resizable()
            .build()
            .map_err(|error| error.to_string())?;
        let canvas = window
            .into_canvas()
            .accelerated()
            .present_vsync()
            .build()
            .map_err(|error| error.to_string())?;
        let texture_creator = canvas.texture_creator();
        let mut video_window = Self {
            canvas,
            texture_creator,
            event_pump: sdl.event_pump()?,
            mouse: sdl.mouse(),
            mode: WindowMode::Windowed,
            stream_texture: None,
            i420_frame: I420Frame::default(),
            wait_image,
            wait_texture: None,
        };
        video_window.enter(mode)?;
        // Wayland only maps a window once something is presented, so show
        // black until the first frame arrives.
        video_window.present()?;
        Ok(video_window)
    }

    /// Drains pending window events and acts on them: closing the window or
    /// pressing Escape requests close, F11 or F toggles fullscreen.
    pub fn handle_events(&mut self) -> Result<WindowStatus, String> {
        let requests: Vec<UserRequest> = self
            .event_pump
            .poll_iter()
            .filter_map(|event| user_request(&event))
            .collect();
        for request in requests {
            match request {
                UserRequest::Close => return Ok(WindowStatus::CloseRequested),
                UserRequest::ToggleFullscreen => self.toggle_fullscreen()?,
            }
        }
        Ok(WindowStatus::Open)
    }

    /// Redraws right away, because a stalled source may not send a new frame for a while.
    fn toggle_fullscreen(&mut self) -> Result<(), String> {
        self.enter(self.mode.toggled())?;
        self.present()
    }

    fn enter(&mut self, mode: WindowMode) -> Result<(), String> {
        self.canvas
            .window_mut()
            .set_fullscreen(mode.fullscreen_type())?;
        self.mouse.show_cursor(mode.shows_cursor());
        self.mode = mode;
        Ok(())
    }

    pub fn show(&mut self, picture: &Picture) -> Result<(), String> {
        self.hide_wait_image()?;
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

    /// Draws the wait image if it is shown, else the latest picture, or
    /// black before the first one arrives.
    pub fn present(&mut self) -> Result<(), String> {
        self.canvas.clear();
        let stream = self.stream_texture.as_ref().map(|current| &current.texture);
        if let Some(texture) = self.wait_texture.as_ref().or(stream) {
            self.canvas.copy(texture, None, None)?;
        }
        self.canvas.present();
        Ok(())
    }

    /// Covers the video with the wait image, if one was given, until the
    /// next picture arrives. The image is only on the GPU while it is shown.
    pub fn show_wait_image(&mut self) -> Result<(), String> {
        let Some(image) = &self.wait_image else {
            return Ok(());
        };
        if self.wait_texture.is_some() {
            return Ok(());
        }
        let dimensions = image.dimensions();
        let texture = self
            .texture_creator
            .create_texture_from_surface(image.surface())
            .map_err(|error| error.to_string())?;
        self.wait_texture = Some(texture);
        self.letterbox(dimensions)
    }

    fn hide_wait_image(&mut self) -> Result<(), String> {
        let Some(texture) = self.wait_texture.take() else {
            return Ok(());
        };
        // SAFETY: The renderer that owns the texture is still alive.
        unsafe { texture.destroy() };
        match self
            .stream_texture
            .as_ref()
            .map(|current| current.dimensions)
        {
            Some(dimensions) => self.letterbox(dimensions),
            None => Ok(()),
        }
    }

    /// Logical size makes SDL scale what is drawn to the window while keeping its aspect ratio.
    fn letterbox(&mut self, dimensions: Dimensions) -> Result<(), String> {
        self.canvas
            .set_logical_size(dimensions.width, dimensions.height)
            .map_err(|error| error.to_string())
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
        self.letterbox(dimensions)
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

#[cfg(test)]
mod tests {
    use super::*;
    use sdl2::keyboard::Mod;

    fn key_down(keycode: Keycode) -> Event {
        Event::KeyDown {
            timestamp: 0,
            window_id: 0,
            keycode: Some(keycode),
            scancode: None,
            keymod: Mod::NOMOD,
            repeat: false,
        }
    }

    fn repeated_key_down(keycode: Keycode) -> Event {
        Event::KeyDown {
            timestamp: 0,
            window_id: 0,
            keycode: Some(keycode),
            scancode: None,
            keymod: Mod::NOMOD,
            repeat: true,
        }
    }

    #[test]
    fn windowed_toggles_to_fullscreen() {
        assert_eq!(WindowMode::Windowed.toggled(), WindowMode::Fullscreen);
    }

    #[test]
    fn fullscreen_toggles_to_windowed() {
        assert_eq!(WindowMode::Fullscreen.toggled(), WindowMode::Windowed);
    }

    #[test]
    fn windowed_shows_cursor() {
        assert!(WindowMode::Windowed.shows_cursor());
    }

    #[test]
    fn fullscreen_hides_cursor() {
        assert!(!WindowMode::Fullscreen.shows_cursor());
    }

    #[test]
    fn quit_event_requests_close() {
        let quit = Event::Quit { timestamp: 0 };
        assert_eq!(user_request(&quit), Some(UserRequest::Close));
    }

    #[test]
    fn escape_requests_close() {
        let request = user_request(&key_down(Keycode::Escape));
        assert_eq!(request, Some(UserRequest::Close));
    }

    #[test]
    fn f11_toggles_fullscreen() {
        let request = user_request(&key_down(Keycode::F11));
        assert_eq!(request, Some(UserRequest::ToggleFullscreen));
    }

    #[test]
    fn f_toggles_fullscreen() {
        let request = user_request(&key_down(Keycode::F));
        assert_eq!(request, Some(UserRequest::ToggleFullscreen));
    }

    #[test]
    fn held_toggle_key_is_ignored() {
        assert_eq!(user_request(&repeated_key_down(Keycode::F11)), None);
    }

    #[test]
    fn other_key_is_ignored() {
        assert_eq!(user_request(&key_down(Keycode::Space)), None);
    }
}
