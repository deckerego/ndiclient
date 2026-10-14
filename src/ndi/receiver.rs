use std::ptr;
use std::time::Duration;

use super::ffi;
use super::finder::SourceDescriptor;
use super::runtime::NdiRuntime;
use crate::picture::{Dimensions, Picture, PixelLayout};
use crate::sound::{AudioFormat, Sound};

const FOURCC_UYVY: u32 = u32::from_le_bytes(*b"UYVY");
const FOURCC_BGRA: u32 = u32::from_le_bytes(*b"BGRA");
const FOURCC_BGRX: u32 = u32::from_le_bytes(*b"BGRX");

/// Outcome of a single capture attempt.
pub enum Capture<Frame> {
    Frame(Frame),
    Nothing,
    ConnectionLost,
}

/// A connection to one NDI source that receives video and audio.
pub struct Receiver<'runtime> {
    instance: ffi::RecvInstance,
    _runtime: &'runtime NdiRuntime,
}

// SAFETY: NDI documents that capture may be called simultaneously on separate
// threads, e.g. one for video and one for audio, and each frame is freed
// through the receiver it came from.
unsafe impl Sync for Receiver<'_> {}

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
    pub fn capture_video(&self, timeout: Duration) -> Capture<VideoFrame<'_>> {
        // SAFETY: VideoFrameV2 is plain old data; all-zero is a valid empty frame.
        let mut frame: ffi::VideoFrameV2 = unsafe { std::mem::zeroed() };
        match self.capture(&mut frame, ptr::null_mut(), timeout) {
            ffi::FRAME_TYPE_VIDEO => Capture::Frame(VideoFrame {
                receiver: self,
                frame,
            }),
            ffi::FRAME_TYPE_ERROR => Capture::ConnectionLost,
            _ => Capture::Nothing,
        }
    }

    /// Waits up to `timeout` for the next audio frame.
    pub fn capture_audio(&self, timeout: Duration) -> Capture<AudioFrame<'_>> {
        // SAFETY: AudioFrameV2 is plain old data; all-zero is a valid empty frame.
        let mut frame: ffi::AudioFrameV2 = unsafe { std::mem::zeroed() };
        match self.capture(ptr::null_mut(), &mut frame, timeout) {
            ffi::FRAME_TYPE_AUDIO => Capture::Frame(AudioFrame {
                receiver: self,
                frame,
            }),
            ffi::FRAME_TYPE_ERROR => Capture::ConnectionLost,
            _ => Capture::Nothing,
        }
    }

    /// Captures into whichever frame is non-null and returns NDI's frame type.
    fn capture(
        &self,
        video: *mut ffi::VideoFrameV2,
        audio: *mut ffi::AudioFrameV2,
        timeout: Duration,
    ) -> std::ffi::c_int {
        let timeout_ms = u32::try_from(timeout.as_millis()).unwrap_or(u32::MAX);
        // SAFETY: `instance` is a live receiver; each frame is null or writable.
        unsafe {
            ffi::NDIlib_recv_capture_v2(self.instance, video, audio, ptr::null_mut(), timeout_ms)
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

/// A captured audio frame whose buffer is returned to NDI on drop.
pub struct AudioFrame<'receiver> {
    receiver: &'receiver Receiver<'receiver>,
    frame: ffi::AudioFrameV2,
}

impl AudioFrame<'_> {
    /// Returns `None` for an empty or malformed frame.
    pub fn sound(&self) -> Option<Sound<'_>> {
        let format = AudioFormat {
            sample_rate: u32::try_from(self.frame.sample_rate).ok()?,
            channels: u16::try_from(self.frame.no_channels).ok()?,
        };
        let samples_per_channel = usize::try_from(self.frame.no_samples).ok()?;
        let channel_stride =
            usize::try_from(self.frame.channel_stride_in_bytes).ok()? / size_of::<f32>();
        let last_channel = usize::from(format.channels).checked_sub(1)?;
        if self.frame.data.is_null() || samples_per_channel > channel_stride {
            return None;
        }
        let sample_count = last_channel * channel_stride + samples_per_channel;
        // SAFETY: NDI's planar float buffer holds every channel's samples, one
        // channel stride apart, valid until the frame is freed in Drop.
        let samples = unsafe { std::slice::from_raw_parts(self.frame.data, sample_count) };
        Some(Sound {
            format,
            samples_per_channel,
            channel_stride,
            samples,
        })
    }
}

impl Drop for AudioFrame<'_> {
    fn drop(&mut self) {
        // SAFETY: `frame` was filled by NDIlib_recv_capture_v2 on this receiver.
        unsafe { ffi::NDIlib_recv_free_audio_v2(self.receiver.instance, &self.frame) };
    }
}
