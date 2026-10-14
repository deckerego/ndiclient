//! A device-agnostic description of one block of decoded audio.

use std::fmt;

/// Sample rate and channel count, which together fix how samples are played.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct AudioFormat {
    pub sample_rate: u32,
    pub channels: u16,
}

impl AudioFormat {
    /// Number of instants (one sample of every channel) that play for `duration`.
    pub fn instant_count(self, duration: std::time::Duration) -> usize {
        (duration.as_secs_f64() * f64::from(self.sample_rate)) as usize
    }

    /// Number of interleaved samples that play for `duration`.
    pub fn sample_count(self, duration: std::time::Duration) -> usize {
        self.instant_count(duration) * usize::from(self.channels)
    }
}

impl fmt::Display for AudioFormat {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{} Hz, {} channels",
            self.sample_rate, self.channels
        )
    }
}

/// Borrowed planar 32-bit float samples: each channel's samples are contiguous
/// and channels start `channel_stride` samples apart.
pub struct Sound<'samples> {
    pub format: AudioFormat,
    pub samples_per_channel: usize,
    pub channel_stride: usize,
    pub samples: &'samples [f32],
}

impl Sound<'_> {
    /// Keeps at most the first `count` channels, which in NDI's order are the
    /// front left and right ones.
    pub fn first_channels(self, count: u16) -> Self {
        let channels = self.format.channels.min(count);
        Self {
            format: AudioFormat {
                channels,
                ..self.format
            },
            ..self
        }
    }

    /// The samples in playback order: one sample of every channel per instant.
    pub fn interleaved(&self) -> impl Iterator<Item = f32> + '_ {
        let channels = usize::from(self.format.channels);
        (0..self.samples_per_channel).flat_map(move |instant| {
            (0..channels).map(move |channel| self.samples[channel * self.channel_stride + instant])
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    const STEREO_48K: AudioFormat = AudioFormat {
        sample_rate: 48_000,
        channels: 2,
    };

    /// Two instants of three channels, stored with one padding sample per channel.
    fn three_channels() -> Sound<'static> {
        Sound {
            format: AudioFormat {
                sample_rate: 48_000,
                channels: 3,
            },
            samples_per_channel: 2,
            channel_stride: 3,
            samples: &[1.0, 2.0, 0.0, 10.0, 20.0, 0.0, 100.0, 200.0, 0.0],
        }
    }

    #[test]
    fn instant_count_counts_one_per_sampling_period() {
        assert_eq!(STEREO_48K.instant_count(Duration::from_millis(10)), 480);
    }

    #[test]
    fn sample_count_covers_every_channel() {
        assert_eq!(STEREO_48K.sample_count(Duration::from_millis(10)), 960);
    }

    #[test]
    fn interleaving_alternates_channels() {
        let samples: Vec<f32> = three_channels().interleaved().collect();
        assert_eq!(samples, [1.0, 10.0, 100.0, 2.0, 20.0, 200.0]);
    }

    #[test]
    fn first_channels_drops_later_channels() {
        let samples: Vec<f32> = three_channels().first_channels(2).interleaved().collect();
        assert_eq!(samples, [1.0, 10.0, 2.0, 20.0]);
    }

    #[test]
    fn first_channels_keeps_fewer_channels_as_they_are() {
        let sound = three_channels().first_channels(8);
        assert_eq!(sound.format.channels, 3);
    }
}
