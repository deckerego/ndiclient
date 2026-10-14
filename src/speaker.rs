//! Plays the audio queued in a jitter buffer through SDL's default output
//! device. SDL pulls samples on its own audio thread and converts them to
//! whatever the hardware needs.

use std::sync::{Arc, Mutex};

use sdl2::audio::{AudioCallback, AudioDevice, AudioSpecDesired};
use sdl2::{AudioSubsystem, Sdl};

use crate::jitter_buffer::{self, JitterBuffer};
use crate::sound::AudioFormat;

/// Samples per channel SDL asks for at a time; about 21 ms at 48 kHz, small
/// enough to keep audio close to the video and large enough not to crackle.
const DEVICE_BUFFER_SAMPLES: u16 = 1024;

/// The SDL callback: hands the device whatever the jitter buffer holds.
struct BufferPlayback {
    buffer: Arc<Mutex<JitterBuffer>>,
    format: AudioFormat,
}

impl AudioCallback for BufferPlayback {
    type Channel = f32;

    fn callback(&mut self, output: &mut [f32]) {
        jitter_buffer::lock(&self.buffer).pull(self.format, output);
    }
}

pub struct Speaker {
    audio: AudioSubsystem,
    buffer: Arc<Mutex<JitterBuffer>>,
    /// The format the device was last (re)opened for, even if opening failed,
    /// so a failure is reported once per format rather than on every check.
    format: Option<AudioFormat>,
    device: Option<AudioDevice<BufferPlayback>>,
}

impl Speaker {
    pub fn new(sdl: &Sdl, buffer: Arc<Mutex<JitterBuffer>>) -> Result<Self, String> {
        Ok(Self {
            audio: sdl.audio()?,
            buffer,
            format: None,
            device: None,
        })
    }

    /// Opens the output device for the format of the queued audio once it is
    /// known, and again whenever the source changes format. Returns the new
    /// format when the device was opened.
    pub fn follow_source(&mut self) -> Result<Option<AudioFormat>, String> {
        let source_format = jitter_buffer::lock(&self.buffer).format();
        match source_format {
            Some(format) if self.format != source_format => {
                self.format = source_format;
                self.device = None;
                self.device = Some(self.open_device(format)?);
                Ok(source_format)
            }
            _ => Ok(None),
        }
    }

    fn open_device(&self, format: AudioFormat) -> Result<AudioDevice<BufferPlayback>, String> {
        let desired = AudioSpecDesired {
            freq: Some(i32::try_from(format.sample_rate).map_err(|error| error.to_string())?),
            channels: Some(u8::try_from(format.channels).map_err(|error| error.to_string())?),
            samples: Some(DEVICE_BUFFER_SAMPLES),
        };
        let buffer = Arc::clone(&self.buffer);
        let device = self
            .audio
            .open_playback(None, &desired, |_| BufferPlayback { buffer, format })?;
        device.resume();
        Ok(device)
    }
}
