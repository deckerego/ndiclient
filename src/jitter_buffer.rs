//! Smooths out uneven audio arrival between the network and the sound card.
//! Playback starts once enough audio is queued to ride out network jitter,
//! and a backlog (from a burst, or a sender clock that runs fast) is trimmed
//! so audio does not fall ever further behind the video.

use std::collections::VecDeque;
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use crate::sound::AudioFormat;

const SILENCE: f32 = 0.0;

/// How much audio the buffer holds: playback (re)starts once `start` is
/// queued, and a queue longer than `max` is cut back to `start`.
#[derive(Clone, Copy, Debug)]
pub struct AudioLatency {
    pub start: Duration,
    pub max: Duration,
}

pub struct JitterBuffer {
    latency: AudioLatency,
    format: Option<AudioFormat>,
    samples: VecDeque<f32>,
    playing: bool,
}

impl JitterBuffer {
    pub fn new(latency: AudioLatency) -> Self {
        Self {
            latency,
            format: None,
            samples: VecDeque::new(),
            playing: false,
        }
    }

    /// The format of the most recently queued audio, if any arrived yet.
    pub fn format(&self) -> Option<AudioFormat> {
        self.format
    }

    /// Queues interleaved samples. Audio still queued in an older format is
    /// discarded, since it can no longer be played.
    pub fn push(&mut self, format: AudioFormat, samples: impl IntoIterator<Item = f32>) {
        if self.format != Some(format) {
            self.format = Some(format);
            self.samples.clear();
            self.playing = false;
        }
        self.samples.extend(samples);
        self.trim_backlog(format);
    }

    /// Fills `output` with interleaved samples in `format`, or with silence
    /// while the buffer is priming or holds another format. Running dry
    /// pauses playback until the buffer has primed again.
    pub fn pull(&mut self, format: AudioFormat, output: &mut [f32]) {
        if self.format != Some(format) {
            output.fill(SILENCE);
            return;
        }
        self.playing |= self.samples.len() >= format.sample_count(self.latency.start);
        if !self.playing {
            output.fill(SILENCE);
            return;
        }
        let available = output.len().min(self.samples.len());
        for (slot, sample) in output.iter_mut().zip(self.samples.drain(..available)) {
            *slot = sample;
        }
        output[available..].fill(SILENCE);
        self.playing = available == output.len();
    }

    fn trim_backlog(&mut self, format: AudioFormat) {
        if self.samples.len() <= format.sample_count(self.latency.max) {
            return;
        }
        // Both counts are whole instants, so the cut keeps channels aligned.
        let excess = self.samples.len() - format.sample_count(self.latency.start);
        self.samples.drain(..excess);
    }
}

/// Locks a buffer shared between threads. A panic elsewhere cannot leave the
/// queue in a state that is unsafe to play, so a poisoned lock is still used.
pub fn lock(buffer: &Mutex<JitterBuffer>) -> MutexGuard<'_, JitterBuffer> {
    buffer.lock().unwrap_or_else(PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 1 kHz mono keeps sample counts readable: 1 ms is one sample.
    const MONO_1K: AudioFormat = AudioFormat {
        sample_rate: 1_000,
        channels: 1,
    };
    const STEREO_1K: AudioFormat = AudioFormat {
        sample_rate: 1_000,
        channels: 2,
    };
    const LATENCY: AudioLatency = AudioLatency {
        start: Duration::from_millis(4),
        max: Duration::from_millis(8),
    };

    fn ramp(count: usize) -> impl Iterator<Item = f32> {
        (1..=count).map(|value| value as f32)
    }

    fn pulled(buffer: &mut JitterBuffer, format: AudioFormat, count: usize) -> Vec<f32> {
        let mut output = vec![-1.0; count];
        buffer.pull(format, &mut output);
        output
    }

    #[test]
    fn new_buffer_has_no_format() {
        assert_eq!(JitterBuffer::new(LATENCY).format(), None);
    }

    #[test]
    fn push_sets_format() {
        let mut buffer = JitterBuffer::new(LATENCY);
        buffer.push(MONO_1K, ramp(1));
        assert_eq!(buffer.format(), Some(MONO_1K));
    }

    #[test]
    fn silent_while_priming() {
        let mut buffer = JitterBuffer::new(LATENCY);
        buffer.push(MONO_1K, ramp(3));
        assert_eq!(pulled(&mut buffer, MONO_1K, 2), [0.0, 0.0]);
    }

    #[test]
    fn plays_queued_samples_once_primed() {
        let mut buffer = JitterBuffer::new(LATENCY);
        buffer.push(MONO_1K, ramp(4));
        assert_eq!(pulled(&mut buffer, MONO_1K, 2), [1.0, 2.0]);
    }

    #[test]
    fn keeps_playing_below_start_level() {
        let mut buffer = JitterBuffer::new(LATENCY);
        buffer.push(MONO_1K, ramp(4));
        pulled(&mut buffer, MONO_1K, 2);
        assert_eq!(pulled(&mut buffer, MONO_1K, 2), [3.0, 4.0]);
    }

    #[test]
    fn underrun_pads_with_silence() {
        let mut buffer = JitterBuffer::new(LATENCY);
        buffer.push(MONO_1K, ramp(4));
        assert_eq!(
            pulled(&mut buffer, MONO_1K, 6),
            [1.0, 2.0, 3.0, 4.0, 0.0, 0.0]
        );
    }

    #[test]
    fn underrun_primes_again() {
        let mut buffer = JitterBuffer::new(LATENCY);
        buffer.push(MONO_1K, ramp(4));
        pulled(&mut buffer, MONO_1K, 6);
        buffer.push(MONO_1K, ramp(2));
        assert_eq!(pulled(&mut buffer, MONO_1K, 2), [0.0, 0.0]);
    }

    #[test]
    fn other_format_is_silent() {
        let mut buffer = JitterBuffer::new(LATENCY);
        buffer.push(MONO_1K, ramp(4));
        assert_eq!(pulled(&mut buffer, STEREO_1K, 2), [0.0, 0.0]);
    }

    #[test]
    fn format_change_discards_old_samples() {
        let mut buffer = JitterBuffer::new(LATENCY);
        buffer.push(MONO_1K, ramp(4));
        buffer.push(STEREO_1K, ramp(8));
        assert_eq!(pulled(&mut buffer, STEREO_1K, 2), [1.0, 2.0]);
    }

    #[test]
    fn backlog_at_max_is_kept() {
        let mut buffer = JitterBuffer::new(LATENCY);
        buffer.push(MONO_1K, ramp(8));
        assert_eq!(pulled(&mut buffer, MONO_1K, 1), [1.0]);
    }

    #[test]
    fn backlog_beyond_max_is_cut_to_newest_start_level() {
        let mut buffer = JitterBuffer::new(LATENCY);
        buffer.push(MONO_1K, ramp(9));
        assert_eq!(pulled(&mut buffer, MONO_1K, 5), [6.0, 7.0, 8.0, 9.0, 0.0]);
    }
}
