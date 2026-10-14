//! Hides short dropouts in the received audio. Some senders (OBS was seen
//! doing it about 14 times a second) punch holes of a few milliseconds of
//! digital silence into their audio. Each hole clicks, and in music the
//! clicks sound like static. A run of exact silence in every channel that is
//! no longer than a limit and follows sound is filled with a bridge from the
//! sound on either side. Longer silence passes unchanged.
//!
//! All audio is passed on after a constant lookahead of twice the limit, so
//! a dropout and enough sound after it to fill it with have always arrived
//! by the time it is passed on. Being constant, the delay adds no jitter.

use std::ops::Range;
use std::time::Duration;

use crate::bridge;
use crate::sound::AudioFormat;

const SILENCE: f32 = 0.0;

pub struct DropoutConcealer {
    /// Longer silence is genuine and passes unchanged.
    max_dropout: Duration,
    format: Option<AudioFormat>,
    /// Interleaved instants: up to one dropout's worth already passed on
    /// (to fill the next dropout from), followed by those not yet passed on.
    window: Vec<f32>,
    /// Number of instants at the start of `window` already passed on.
    released: usize,
}

impl DropoutConcealer {
    pub fn new(max_dropout: Duration) -> Self {
        Self {
            max_dropout,
            format: None,
            window: Vec::new(),
            released: 0,
        }
    }

    /// Takes interleaved `samples` and returns, with dropouts filled, the
    /// samples that have been held for the lookahead.
    pub fn conceal(
        &mut self,
        format: AudioFormat,
        samples: impl IntoIterator<Item = f32>,
    ) -> Vec<f32> {
        if self.format != Some(format) {
            self.restart(format);
        }
        self.window.extend(samples);
        let first = self.released;
        let lookahead = 2 * self.max_instants(format);
        let ready_end = self.instant_count(format).saturating_sub(lookahead);
        while self.released < ready_end {
            self.release_next(format, ready_end);
        }
        let output = self.samples_of(format, first..self.released).to_vec();
        self.forget_old_sound(format);
        output
    }

    /// Held samples belong to the old format and can no longer be played.
    fn restart(&mut self, format: AudioFormat) {
        self.format = Some(format);
        self.window.clear();
        self.released = 0;
    }

    /// Releases the next stretch of sound or genuine silence, or fills and
    /// releases the next dropout, without going beyond `ready_end`.
    fn release_next(&mut self, format: AudioFormat, ready_end: usize) {
        let start = self.released;
        let silence_end = self.end_of_silence(format, start);
        let end = if silence_end == start {
            self.start_of_silence(format, start)
        } else {
            if self.is_dropout(format, start..silence_end) {
                self.fill(format, start..silence_end);
            }
            silence_end
        };
        self.released = end.min(ready_end);
    }

    /// Silence is a dropout if it is short and follows sound. Silence before
    /// `ready_end` that reaches the end of the window is longer than the
    /// lookahead, so a dropout always has sound after it.
    fn is_dropout(&self, format: AudioFormat, silence: Range<usize>) -> bool {
        self.follows_sound(format, silence.start) && silence.len() <= self.max_instants(format)
    }

    /// Fills `dropout` with a bridge from the sound before it to the sound
    /// after it, which is cut short where the next silence starts.
    fn fill(&mut self, format: AudioFormat, dropout: Range<usize>) {
        let channels = usize::from(format.channels);
        let before_start = dropout.start.saturating_sub(dropout.len());
        let after_end = self
            .start_of_silence(format, dropout.end)
            .min(dropout.end + dropout.len());
        let filled = bridge::between(
            self.samples_of(format, before_start..dropout.start),
            self.samples_of(format, dropout.end..after_end),
            channels,
            dropout.len(),
        );
        self.window[dropout.start * channels..dropout.end * channels].copy_from_slice(&filled);
    }

    /// Keeps one dropout's worth of passed-on sound to fill the next from.
    fn forget_old_sound(&mut self, format: AudioFormat) {
        let forgotten = self.released.saturating_sub(self.max_instants(format));
        self.window
            .drain(..forgotten * usize::from(format.channels));
        self.released -= forgotten;
    }

    fn follows_sound(&self, format: AudioFormat, instant: usize) -> bool {
        instant > 0 && !self.is_silent(format, instant - 1)
    }

    /// The first silent instant at or after `from`, or the end of the window.
    fn start_of_silence(&self, format: AudioFormat, from: usize) -> usize {
        (from..self.instant_count(format))
            .find(|&instant| self.is_silent(format, instant))
            .unwrap_or_else(|| self.instant_count(format))
    }

    /// The first sounding instant at or after `from`, or the end of the window.
    fn end_of_silence(&self, format: AudioFormat, from: usize) -> usize {
        (from..self.instant_count(format))
            .find(|&instant| !self.is_silent(format, instant))
            .unwrap_or_else(|| self.instant_count(format))
    }

    fn is_silent(&self, format: AudioFormat, instant: usize) -> bool {
        self.samples_of(format, instant..instant + 1)
            .iter()
            .all(|&sample| sample == SILENCE)
    }

    fn max_instants(&self, format: AudioFormat) -> usize {
        format.instant_count(self.max_dropout)
    }

    fn instant_count(&self, format: AudioFormat) -> usize {
        self.window.len() / usize::from(format.channels)
    }

    fn samples_of(&self, format: AudioFormat, instants: Range<usize>) -> &[f32] {
        let channels = usize::from(format.channels);
        &self.window[instants.start * channels..instants.end * channels]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 1 kHz keeps durations readable: 1 ms is one instant.
    const MONO_1K: AudioFormat = AudioFormat {
        sample_rate: 1_000,
        channels: 1,
    };
    const STEREO_1K: AudioFormat = AudioFormat {
        sample_rate: 1_000,
        channels: 2,
    };
    const MAX_DROPOUT: Duration = Duration::from_millis(10);
    /// Instants held back: twice the longest dropout.
    const LOOKAHEAD: usize = 20;
    const PERIOD: usize = 8;

    /// A waveform that repeats every `PERIOD` instants, so a dropout of one
    /// period is filled exactly.
    fn tone(range: Range<usize>) -> Vec<f32> {
        range
            .map(|t| [1.0, 3.0, 4.0, 2.0, -1.0, -3.0, -4.0, -2.0][t % PERIOD])
            .collect()
    }

    fn concealed(samples: &[f32]) -> Vec<f32> {
        DropoutConcealer::new(MAX_DROPOUT).conceal(MONO_1K, samples.iter().copied())
    }

    /// A tone with its instants `dropout` replaced by silence.
    fn tone_with_dropout(length: usize, dropout: Range<usize>) -> Vec<f32> {
        let mut samples = tone(0..length);
        samples[dropout].fill(SILENCE);
        samples
    }

    fn stereo(left: &[f32], right: &[f32]) -> Vec<f32> {
        left.iter().zip(right).flat_map(|(&l, &r)| [l, r]).collect()
    }

    #[test]
    fn sound_passes_unchanged_after_the_lookahead() {
        assert_eq!(concealed(&tone(0..100)), tone(0..100 - LOOKAHEAD));
    }

    #[test]
    fn lookahead_is_held_back() {
        assert!(concealed(&tone(0..LOOKAHEAD)).is_empty());
    }

    #[test]
    fn held_samples_are_released_with_later_ones() {
        let mut concealer = DropoutConcealer::new(MAX_DROPOUT);
        concealer.conceal(MONO_1K, tone(0..LOOKAHEAD));
        let output = concealer.conceal(MONO_1K, tone(LOOKAHEAD..40));
        assert_eq!(output, tone(0..40 - LOOKAHEAD));
    }

    #[test]
    fn leading_silence_passes_unchanged() {
        let samples: Vec<f32> = [0.0, 0.0].into_iter().chain(tone(0..60)).collect();
        assert_eq!(concealed(&samples)[..2], [0.0, 0.0]);
    }

    #[test]
    fn dropout_is_filled_with_the_continued_waveform() {
        let output = concealed(&tone_with_dropout(100, 48..48 + PERIOD));
        assert_eq!(output[48..48 + PERIOD], tone(48..48 + PERIOD)[..]);
    }

    #[test]
    fn sound_around_a_dropout_passes_unchanged() {
        let output = concealed(&tone_with_dropout(100, 48..48 + PERIOD));
        assert_eq!(output[..48], tone(0..48)[..]);
    }

    #[test]
    fn dropout_cut_short_by_the_next_silence_is_filled() {
        let mut samples = tone_with_dropout(100, 50..51);
        samples[53] = SILENCE;
        assert!(!concealed(&samples)[50..53].contains(&SILENCE));
    }

    #[test]
    fn silence_beyond_max_dropout_passes_unchanged() {
        let output = concealed(&tone_with_dropout(100, 50..61));
        assert_eq!(output[50..61], [SILENCE; 11]);
    }

    #[test]
    fn silence_in_one_channel_is_not_a_dropout() {
        let left = tone_with_dropout(100, 50..53);
        let samples = stereo(&left, &tone(0..100));
        let output = DropoutConcealer::new(MAX_DROPOUT).conceal(STEREO_1K, samples.iter().copied());
        assert_eq!(output, samples[..output.len()]);
    }

    #[test]
    fn channels_are_filled_separately() {
        let left = tone_with_dropout(100, 50..53);
        let right: Vec<f32> = left.iter().map(|&sample| -sample).collect();
        let output = DropoutConcealer::new(MAX_DROPOUT).conceal(STEREO_1K, stereo(&left, &right));
        assert_eq!(output[2 * 51], -output[2 * 51 + 1]);
    }

    #[test]
    fn format_change_discards_held_samples() {
        let mut concealer = DropoutConcealer::new(MAX_DROPOUT);
        concealer.conceal(MONO_1K, tone(0..LOOKAHEAD));
        let samples = stereo(&tone(0..40), &tone(0..40));
        let output = concealer.conceal(STEREO_1K, samples.iter().copied());
        assert_eq!(output, samples[..2 * (40 - LOOKAHEAD)]);
    }
}
