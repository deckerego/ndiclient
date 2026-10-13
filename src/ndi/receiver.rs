use std::ptr;
use std::time::Duration;

use super::ffi;
use super::finder::SourceDescriptor;
use super::runtime::NdiRuntime;
use crate::picture::{Dimensions, Picture, PixelLayout};

const FOURCC_UYVY: u32 = u32::from_le_bytes(*b"UYVY");
const FOURCC_BGRA: u32 = u32::from_le_bytes(*b"BGRA");
const FOURCC_BGRX: u32 = u32::from_le_bytes(*b"BGRX");

/// Outcome of a single capture attempt.
pub enum Capture<'receiver> {
    Video(VideoFrame<'receiver>),
    Nothing,
    ConnectionLost,
}

/// A connection to one NDI source that receives video only.
pub struct Receiver<'runtime> {
    instance: ffi::RecvInstance,
    _runtime: &'runtime NdiRuntime,
}

impl<'runtime> Receiver<'runtime> {
    pub fn connect(
        runtime: &'runtime NdiRuntime,
        source: &SourceDescriptor,
    ) -> Result<Self, String> {
        let settings = ffi::RecvCreateV3 {
            source_to_connect_to: source.as_raw(),
            // UYVY is what NDI decodes to natively and what SDL can upload
            // without conversion; BGRA only appears for sources with alpha.
            color_format: ffi::RECV_COLOR_FORMAT_UYVY_BGRA,
            bandwidth: ffi::RECV_BANDWIDTH_HIGHEST,
            allow_video_fields: false,
            ndi_recv_name: ptr::null(),
        };
        // SAFETY: `settings` and the source strings outlive the call; NDI
        // copies them during creation.
        let instance = unsafe { ffi::NDIlib_recv_create_v3(&settings) };
        if instance.is_null() {
            return Err(format!(
                "NDI failed to create a receiver for {}",
                source.name()
            ));
        }
        Ok(Self {
            instance,
            _runtime: runtime,
        })
    }

    /// Waits up to `timeout` for the next video frame.
    pub fn capture_video(&self, timeout: Duration) -> Capture<'_> {
        let timeout_ms = u32::try_from(timeout.as_millis()).unwrap_or(u32::MAX);
        // SAFETY: VideoFrameV2 is plain old data; all-zero is a valid empty frame.
        let mut frame: ffi::VideoFrameV2 = unsafe { std::mem::zeroed() };
        // SAFETY: `instance` is a live receiver and `frame` is writable.
        let frame_type = unsafe {
            ffi::NDIlib_recv_capture_v2(
                self.instance,
                &mut frame,
                ptr::null_mut(),
                ptr::null_mut(),
                timeout_ms,
            )
        };
        match frame_type {
            ffi::FRAME_TYPE_VIDEO => Capture::Video(VideoFrame {
                receiver: self,
                frame,
            }),
            ffi::FRAME_TYPE_ERROR => Capture::ConnectionLost,
            _ => Capture::Nothing,
        }
    }
}

impl Drop for Receiver<'_> {
    fn drop(&mut self) {
        // SAFETY: `instance` came from NDIlib_recv_create_v3 and is destroyed
        // once; borrowed frames have already been released by the borrow checker.
        unsafe { ffi::NDIlib_recv_destroy(self.instance) };
    }
}

/// A captured video frame whose buffer is returned to NDI on drop.
pub struct VideoFrame<'receiver> {
    receiver: &'receiver Receiver<'receiver>,
    frame: ffi::VideoFrameV2,
}

impl VideoFrame<'_> {
    /// Returns `None` for pixel formats this utility does not display.
    pub fn picture(&self) -> Option<Picture<'_>> {
        let layout = self.pixel_layout()?;
        let line_stride = usize::try_from(self.frame.line_stride_in_bytes).ok()?;
        let dimensions = Dimensions {
            width: u32::try_from(self.frame.xres).ok()?,
            height: u32::try_from(self.frame.yres).ok()?,
        };
        let byte_count = line_stride * dimensions.height as usize;
        // SAFETY: For packed formats NDI allocates `line_stride * yres` bytes,
        // valid until the frame is freed in Drop.
        let pixels = unsafe { std::slice::from_raw_parts(self.frame.data, byte_count) };
        Some(Picture {
            layout,
            dimensions,
            line_stride,
            pixels,
        })
    }

    fn pixel_layout(&self) -> Option<PixelLayout> {
        match self.frame.four_cc {
            FOURCC_UYVY => Some(PixelLayout::Uyvy),
            FOURCC_BGRA | FOURCC_BGRX => Some(PixelLayout::Bgra),
            _ => None,
        }
    }
}

impl Drop for VideoFrame<'_> {
    fn drop(&mut self) {
        // SAFETY: `frame` was filled by NDIlib_recv_capture_v2 on this receiver.
        unsafe { ffi::NDIlib_recv_free_video_v2(self.receiver.instance, &self.frame) };
    }
}
