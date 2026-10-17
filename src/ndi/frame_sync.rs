//! NDI's frame synchronizer turns the frames a source pushes into frames the
//! receiver pulls at its own pace. Video is pulled once per display refresh
//! and gets the frame due at that moment, repeated or skipped as the clocks
//! require. Audio is pulled as fast as the sound card plays it, and NDI
//! resamples it so the sender's clock never drifts away from the sound card's.

use std::ffi::c_int;
use std::sync::{Mutex, MutexGuard, PoisonError};

use super::ffi;
use super::receiver::Receiver;
use crate::picture::{Dimensions, Picture, PixelLayout};
use crate::sound::{AudioFormat, Sound};

const FOURCC_UYVY: u32 = u32::from_le_bytes(*b"UYVY");
const FOURCC_BGRA: u32 = u32::from_le_bytes(*b"BGRA");
const FOURCC_BGRX: u32 = u32::from_le_bytes(*b"BGRX");

/// Asks NDI for the format of the audio it receives instead of for samples.
const SOURCE_AUDIO_FORMAT: c_int = 0;

pub struct FrameSync<'receiver> {
    instance: ffi::FrameSyncInstance,
    /// NDI documents concurrent calls for receivers, not for frame-syncs, so
    /// the audio and video threads take turns calling into it.
    calls: Mutex<()>,
    _receiver: &'receiver Receiver<'receiver>,
}

// SAFETY: Every call into NDI with `instance` holds the `calls` lock, and
// each frame is freed through the frame-sync it came from.
unsafe impl Sync for FrameSync<'_> {}

impl<'receiver> FrameSync<'receiver> {
    pub fn new(receiver: &'receiver Receiver<'receiver>) -> Result<Self, String> {
        // SAFETY: The receiver is live and, by its borrow, outlives the frame-sync.
        let instance = unsafe { ffi::NDIlib_framesync_create(receiver.instance()) };
        if instance.is_null() {
            return Err("NDI failed to create a frame synchronizer".to_string());
        }
        Ok(Self {
            instance,
            calls: Mutex::new(()),
            _receiver: receiver,
        })
    }

    /// The video frame due now, which may be the one returned last time.
    /// `None` until the first frame has arrived.
    pub fn capture_video(&self) -> Option<VideoFrame<'_>> {
        // SAFETY: VideoFrameV2 is plain old data; all-zero is a valid empty frame.
        let mut frame: ffi::VideoFrameV2 = unsafe { std::mem::zeroed() };
        {
            let _turn = self.take_turn();
            // SAFETY: `instance` is a live frame-sync and `frame` is writable.
            unsafe {
                ffi::NDIlib_framesync_capture_video(
                    self.instance,
                    &mut frame,
                    ffi::FRAME_FORMAT_TYPE_PROGRESSIVE,
                );
            }
        }
        let frame = VideoFrame {
            frame_sync: self,
            frame,
        };
        // An empty frame is still freed, by dropping it.
        (!frame.frame.data.is_null()).then_some(frame)
    }

    /// The format of the audio being received; `None` before any arrived.
    pub fn audio_format(&self) -> Option<AudioFormat> {
        let query = self.capture_raw_audio(
            SOURCE_AUDIO_FORMAT,
            SOURCE_AUDIO_FORMAT,
            SOURCE_AUDIO_FORMAT,
        );
        query
            .format()
            .filter(|format| format.sample_rate > 0 && format.channels > 0)
    }

    /// How many instants of received audio wait to be captured.
    pub fn queued_audio(&self) -> usize {
        let _turn = self.take_turn();
        // SAFETY: `instance` is a live frame-sync.
        let depth = unsafe { ffi::NDIlib_framesync_audio_queue_depth(self.instance) };
        usize::try_from(depth).unwrap_or(0)
    }

    /// Takes `instants` instants of audio in `format`. NDI pads with silence
    /// what it hasn't received, so ask for no more than is queued.
    pub fn capture_audio(&self, format: AudioFormat, instants: usize) -> AudioFrame<'_> {
        self.capture_raw_audio(
            c_int::try_from(format.sample_rate).unwrap_or(c_int::MAX),
            c_int::from(format.channels),
            c_int::try_from(instants).unwrap_or(c_int::MAX),
        )
    }

    fn capture_raw_audio(
        &self,
        sample_rate: c_int,
        channels: c_int,
        instants: c_int,
    ) -> AudioFrame<'_> {
        // SAFETY: AudioFrameV2 is plain old data; all-zero is a valid empty frame.
        let mut frame: ffi::AudioFrameV2 = unsafe { std::mem::zeroed() };
        let _turn = self.take_turn();
        // SAFETY: `instance` is a live frame-sync and `frame` is writable.
        unsafe {
            ffi::NDIlib_framesync_capture_audio(
                self.instance,
                &mut frame,
                sample_rate,
                channels,
                instants,
            );
        }
        AudioFrame {
            frame_sync: self,
            frame,
        }
    }

    /// A panic elsewhere while holding the lock leaves nothing inconsistent,
    /// since the lock guards no data.
    fn take_turn(&self) -> MutexGuard<'_, ()> {
        self.calls.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl Drop for FrameSync<'_> {
    fn drop(&mut self) {
        // SAFETY: `instance` came from NDIlib_framesync_create and is
        // destroyed once; borrowed frames have already been freed.
        unsafe { ffi::NDIlib_framesync_destroy(self.instance) };
    }
}

/// Tells video frames apart: the frame-sync hands out the same frame for as
/// many display refreshes as it is due, but reuses buffers between frames.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameStamp {
    timestamp: i64,
    timecode: i64,
}

/// A video frame whose buffer is returned to NDI on drop.
pub struct VideoFrame<'frame_sync> {
    frame_sync: &'frame_sync FrameSync<'frame_sync>,
    frame: ffi::VideoFrameV2,
}

impl VideoFrame<'_> {
    pub fn stamp(&self) -> FrameStamp {
        FrameStamp {
            timestamp: self.frame.timestamp,
            timecode: self.frame.timecode,
        }
    }

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
        let _turn = self.frame_sync.take_turn();
        // SAFETY: `frame` was filled by NDIlib_framesync_capture_video on this frame-sync.
        unsafe { ffi::NDIlib_framesync_free_video(self.frame_sync.instance, &mut self.frame) };
    }
}

/// An audio frame whose buffer is returned to NDI on drop.
pub struct AudioFrame<'frame_sync> {
    frame_sync: &'frame_sync FrameSync<'frame_sync>,
    frame: ffi::AudioFrameV2,
}

impl AudioFrame<'_> {
    fn format(&self) -> Option<AudioFormat> {
        Some(AudioFormat {
            sample_rate: u32::try_from(self.frame.sample_rate).ok()?,
            channels: u16::try_from(self.frame.no_channels).ok()?,
        })
    }

    /// Returns `None` for an empty or malformed frame.
    pub fn sound(&self) -> Option<Sound<'_>> {
        let format = self.format()?;
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
        let _turn = self.frame_sync.take_turn();
        // SAFETY: `frame` was filled by NDIlib_framesync_capture_audio on this frame-sync.
        unsafe { ffi::NDIlib_framesync_free_audio(self.frame_sync.instance, &mut self.frame) };
    }
}
