//! Smooths out uneven audio arrival between the network and the sound card.
//! Playback starts once enough audio is queued to ride out network jitter,
//! and a backlog (from a burst, or a sender clock that runs fast) is trimmed
//! so audio does not fall ever further behind the video. Uninterrupted audio
//! matters more than keeping it close to the video, so every underrun raises
//! the queued amount. Starts, stops and cuts are faded so they don't click.

use std::collections::VecDeque;
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use crate::sound::AudioFormat;

const SILENCE: f32 = 0.0;

/// How much audio the buffer holds. Playback (re)starts once the prime level
/// is queued. It begins at `start` and doubles after every underrun, up to
/// `limit`. A queue longer than the prime level plus `headroom` is cut back
/// to the prime level.
#[derive(Clone, Copy, Debug)]
pub struct AudioLatency {
    pub start: Duration,
    pub limit: Duration,
    pub headroom: Duration,
}

pub struct JitterBuffer {
    latency: AudioLatency,
    /// Length of the fades at playback starts, underruns and cuts.
    fade: Duration,
    prime_level: Duration,
    format: Option<AudioFormat>,
    samples: VecDeque<f32>,
    playing: bool,
}

impl JitterBuffer {
    pub fn new(latency: AudioLatency, fade: Duration) -> Self {
        Self {
            latency,
            fade,
            prime_level: latency.start,
            format: None,
            samples: VecDeque::new(),
            playing: false,
        }
    }

    /// The format of the most recently queued audio, if any arrived yet.
    pub fn format(&self) -> Option<AudioFormat> {
        self.format
    }

    /// How much audio is queued before playback (re)starts.
    pub fn prime_level(&self) -> Duration {
        self.prime_level
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
    /// fades out, raises the prime level and pauses playback until the
    /// buffer has primed again.
    pub fn pull(&mut self, format: AudioFormat, output: &mut [f32]) {
        if self.format != Some(format) || !self.is_ready_to_play(format) {
            output.fill(SILENCE);
            return;
        }
        let available = output.len().min(self.samples.len());
        for (slot, sample) in output.iter_mut().zip(self.samples.drain(..available)) {
            *slot = sample;
        }
        output[available..].fill(SILENCE);
        if available < output.len() {
            self.underrun(format, &mut output[..available]);
        }
    }

    /// Starts playback with a fade-in once the buffer has primed.
    fn is_ready_to_play(&mut self, format: AudioFormat) -> bool {
        if !self.playing && self.samples.len() >= format.sample_count(self.prime_level) {
            self.playing = true;
            let fade = self.fade_samples(format).min(self.samples.len());
            apply_fade(format, self.samples.range_mut(..fade), FadeDirection::In);
        }
        self.playing
    }

    fn underrun(&mut self, format: AudioFormat, played: &mut [f32]) {
        let fade = self.fade_samples(format).min(played.len());
        let tail = played.len() - fade;
        apply_fade(format, played[tail..].iter_mut(), FadeDirection::Out);
        self.playing = false;
        self.prime_level = (self.prime_level * 2).min(self.latency.limit);
    }

    /// Cuts the queue back to the prime level, cross-fading from the audio
    /// that would have played next into the audio that plays after the cut.
    fn trim_backlog(&mut self, format: AudioFormat) {
        let max = format.sample_count(self.prime_level + self.latency.headroom);
        if self.samples.len() <= max {
            return;
        }
        // Both counts are whole instants, so the cut keeps channels aligned.
        let excess = self.samples.len() - format.sample_count(self.prime_level);
        let fade = self.fade_samples(format).min(self.samples.len() - excess);
        let instants = fade / usize::from(format.channels);
        for index in 0..fade {
            let weight = fade_weight(index / usize::from(format.channels), instants);
            let cut_from = self.samples[index];
            let cut_to = &mut self.samples[excess + index];
            *cut_to = cut_from * (1.0 - weight) + *cut_to * weight;
        }
        self.samples.drain(..excess);
    }

    fn fade_samples(&self, format: AudioFormat) -> usize {
        format.sample_count(self.fade)
    }
}

#[derive(Clone, Copy)]
enum FadeDirection {
    In,
    Out,
}

/// Fades interleaved samples in or out, with one gain per instant.
fn apply_fade<'a>(
    format: AudioFormat,
    samples: impl ExactSizeIterator<Item = &'a mut f32>,
    direction: FadeDirection,
) {
    let channels = usize::from(format.channels);
    let instants = samples.len() / channels;
    for (index, sample) in samples.enumerate() {
        let rising = fade_weight(index / channels, instants);
        *sample *= match direction {
            FadeDirection::In => rising,
            FadeDirection::Out => 1.0 - rising,
        };
    }
}

/// Rises in equal steps from above 0 at the first of `instants` to below 1 at the last.
fn fade_weight(instant: usize, instants: usize) -> f32 {
    (instant + 1) as f32 / (instants + 1) as f32
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
        limit: Duration::from_millis(12),
        headroom: Duration::from_millis(4),
    };
    const NO_FADE: Duration = Duration::ZERO;
    /// One instant: its gain is 1/2, which keeps faded values exact.
    const SHORT_FADE: Duration = Duration::from_millis(1);

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
        assert_eq!(JitterBuffer::new(LATENCY, NO_FADE).format(), None);
    }

    #[test]
    fn push_sets_format() {
        let mut buffer = JitterBuffer::new(LATENCY, NO_FADE);
        buffer.push(MONO_1K, ramp(1));
        assert_eq!(buffer.format(), Some(MONO_1K));
    }

    #[test]
    fn silent_while_priming() {
        let mut buffer = JitterBuffer::new(LATENCY, NO_FADE);
        buffer.push(MONO_1K, ramp(3));
        assert_eq!(pulled(&mut buffer, MONO_1K, 2), [0.0, 0.0]);
    }

    #[test]
    fn plays_queued_samples_once_primed() {
        let mut buffer = JitterBuffer::new(LATENCY, NO_FADE);
        buffer.push(MONO_1K, ramp(4));
        assert_eq!(pulled(&mut buffer, MONO_1K, 2), [1.0, 2.0]);
    }

    #[test]
    fn keeps_playing_below_start_level() {
        let mut buffer = JitterBuffer::new(LATENCY, NO_FADE);
        buffer.push(MONO_1K, ramp(4));
        pulled(&mut buffer, MONO_1K, 2);
        assert_eq!(pulled(&mut buffer, MONO_1K, 2), [3.0, 4.0]);
    }

    #[test]
    fn underrun_pads_with_silence() {
        let mut buffer = JitterBuffer::new(LATENCY, NO_FADE);
        buffer.push(MONO_1K, ramp(4));
        assert_eq!(
            pulled(&mut buffer, MONO_1K, 6),
            [1.0, 2.0, 3.0, 4.0, 0.0, 0.0]
        );
    }

    #[test]
    fn underrun_primes_again() {
        let mut buffer = JitterBuffer::new(LATENCY, NO_FADE);
        buffer.push(MONO_1K, ramp(4));
        pulled(&mut buffer, MONO_1K, 6);
        buffer.push(MONO_1K, ramp(2));
        assert_eq!(pulled(&mut buffer, MONO_1K, 2), [0.0, 0.0]);
    }

    #[test]
    fn other_format_is_silent() {
        let mut buffer = JitterBuffer::new(LATENCY, NO_FADE);
        buffer.push(MONO_1K, ramp(4));
        assert_eq!(pulled(&mut buffer, STEREO_1K, 2), [0.0, 0.0]);
    }

    #[test]
    fn format_change_discards_old_samples() {
        let mut buffer = JitterBuffer::new(LATENCY, NO_FADE);
        buffer.push(MONO_1K, ramp(4));
        buffer.push(STEREO_1K, ramp(8));
        assert_eq!(pulled(&mut buffer, STEREO_1K, 2), [1.0, 2.0]);
    }

    #[test]
    fn backlog_at_max_is_kept() {
        let mut buffer = JitterBuffer::new(LATENCY, NO_FADE);
        buffer.push(MONO_1K, ramp(8));
        assert_eq!(pulled(&mut buffer, MONO_1K, 1), [1.0]);
    }

    #[test]
    fn backlog_beyond_max_is_cut_to_newest_start_level() {
        let mut buffer = JitterBuffer::new(LATENCY, NO_FADE);
        buffer.push(MONO_1K, ramp(9));
        assert_eq!(pulled(&mut buffer, MONO_1K, 5), [6.0, 7.0, 8.0, 9.0, 0.0]);
    }

    #[test]
    fn starts_with_prime_level_at_start() {
        let buffer = JitterBuffer::new(LATENCY, NO_FADE);
        assert_eq!(buffer.prime_level(), Duration::from_millis(4));
    }

    #[test]
    fn underrun_doubles_prime_level() {
        let mut buffer = JitterBuffer::new(LATENCY, NO_FADE);
        buffer.push(MONO_1K, ramp(4));
        pulled(&mut buffer, MONO_1K, 6);
        assert_eq!(buffer.prime_level(), Duration::from_millis(8));
    }

    #[test]
    fn prime_level_stops_growing_at_limit() {
        let mut buffer = JitterBuffer::new(LATENCY, NO_FADE);
        buffer.push(MONO_1K, ramp(4));
        pulled(&mut buffer, MONO_1K, 6);
        buffer.push(MONO_1K, ramp(8));
        pulled(&mut buffer, MONO_1K, 10);
        assert_eq!(buffer.prime_level(), Duration::from_millis(12));
    }

    #[test]
    fn backlog_limit_follows_prime_level() {
        let mut buffer = JitterBuffer::new(LATENCY, NO_FADE);
        buffer.push(MONO_1K, ramp(4));
        pulled(&mut buffer, MONO_1K, 6);
        buffer.push(MONO_1K, ramp(12));
        assert_eq!(pulled(&mut buffer, MONO_1K, 1), [1.0]);
    }

    #[test]
    fn playback_fades_in() {
        let mut buffer = JitterBuffer::new(LATENCY, SHORT_FADE);
        buffer.push(MONO_1K, ramp(4));
        assert_eq!(pulled(&mut buffer, MONO_1K, 4), [0.5, 2.0, 3.0, 4.0]);
    }

    #[test]
    fn underrun_fades_out() {
        let mut buffer = JitterBuffer::new(LATENCY, SHORT_FADE);
        buffer.push(MONO_1K, ramp(4));
        pulled(&mut buffer, MONO_1K, 2);
        assert_eq!(pulled(&mut buffer, MONO_1K, 4), [3.0, 2.0, 0.0, 0.0]);
    }

    #[test]
    fn backlog_cut_cross_fades() {
        let mut buffer = JitterBuffer::new(LATENCY, SHORT_FADE);
        buffer.push(MONO_1K, ramp(4));
        pulled(&mut buffer, MONO_1K, 4);
        buffer.push(MONO_1K, ramp(9));
        assert_eq!(pulled(&mut buffer, MONO_1K, 4), [3.5, 7.0, 8.0, 9.0]);
    }

    #[test]
    fn stereo_fade_applies_one_gain_per_instant() {
        let mut buffer = JitterBuffer::new(LATENCY, SHORT_FADE);
        buffer.push(STEREO_1K, ramp(8));
        assert_eq!(pulled(&mut buffer, STEREO_1K, 2), [0.5, 1.0]);
    }
}
