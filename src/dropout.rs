//! Hides short dropouts in the received audio. Some senders (OBS was seen
//! doing it at about 12 a second) replace a few milliseconds of late audio
//! with digital silence. Each such hole clicks, and in music the clicks sound
//! like static. A run of exact silence in every channel that is no longer than
//! a limit and lies between two sounding instants is bridged by a straight
//! line, which keeps the waveform continuous. Longer silence passes unchanged.

use std::iter;
use std::time::Duration;

use crate::sound::AudioFormat;

const SILENCE: f32 = 0.0;

pub struct DropoutConcealer {
    max_dropout: Duration,
    format: Option<AudioFormat>,
    /// The last instant passed on, or empty if it was silent.
    last_sound: Vec<f32>,
    /// Silent instants held back until it is known whether they are a dropout.
    held_silence: usize,
}

impl DropoutConcealer {
    pub fn new(max_dropout: Duration) -> Self {
        Self {
            max_dropout,
            format: None,
            last_sound: Vec::new(),
            held_silence: 0,
        }
    }

    /// Returns the interleaved `samples` with dropouts bridged. Trailing
    /// silence that may still turn out to be a dropout is held back and
    /// passed on with the next samples.
    pub fn conceal(
        &mut self,
        format: AudioFormat,
        samples: impl IntoIterator<Item = f32>,
    ) -> Vec<f32> {
        if self.format != Some(format) {
            self.restart(format);
        }
        let samples: Vec<f32> = samples.into_iter().collect();
        let mut output = Vec::with_capacity(samples.len());
        for instant in samples.chunks_exact(usize::from(format.channels)) {
            if is_silent(instant) {
                self.hold_silence(format, &mut output);
            } else {
                self.bridge_to(instant, &mut output);
                output.extend_from_slice(instant);
                self.last_sound = instant.to_vec();
            }
        }
        output
    }

    /// Held silence belongs to the old format and can no longer be played.
    fn restart(&mut self, format: AudioFormat) {
        self.format = Some(format);
        self.last_sound.clear();
        self.held_silence = 0;
    }

    fn hold_silence(&mut self, format: AudioFormat, output: &mut Vec<f32>) {
        if self.last_sound.is_empty() {
            output.extend(iter::repeat_n(SILENCE, usize::from(format.channels)));
            return;
        }
        self.held_silence += 1;
        if self.held_silence > format.instant_count(self.max_dropout) {
            self.release_silence(format, output);
        }
    }

    /// The held silence is too long for a dropout: it is passed on as it is.
    fn release_silence(&mut self, format: AudioFormat, output: &mut Vec<f32>) {
        let held_samples = self.held_silence * usize::from(format.channels);
        output.extend(iter::repeat_n(SILENCE, held_samples));
        self.held_silence = 0;
        self.last_sound.clear();
    }

    /// Replaces the held silence by a straight line from the last sound to `next`.
    fn bridge_to(&mut self, next: &[f32], output: &mut Vec<f32>) {
        let steps = self.held_silence + 1;
        for step in 1..steps {
            let progress = step as f32 / steps as f32;
            output.extend(
                self.last_sound
                    .iter()
                    .zip(next)
                    .map(|(before, after)| before + (after - before) * progress),
            );
        }
        self.held_silence = 0;
    }
}

fn is_silent(instant: &[f32]) -> bool {
    instant.iter().all(|&sample| sample == SILENCE)
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
    const MAX_DROPOUT: Duration = Duration::from_millis(3);

    fn concealed(samples: &[f32]) -> Vec<f32> {
        DropoutConcealer::new(MAX_DROPOUT).conceal(MONO_1K, samples.iter().copied())
    }

    #[test]
    fn sound_passes_unchanged() {
        assert_eq!(concealed(&[1.0, -2.0, 3.0]), [1.0, -2.0, 3.0]);
    }

    #[test]
    fn leading_silence_passes_unchanged() {
        assert_eq!(concealed(&[0.0, 0.0, 1.0]), [0.0, 0.0, 1.0]);
    }

    #[test]
    fn dropout_is_bridged_by_a_straight_line() {
        assert_eq!(
            concealed(&[2.0, 0.0, 0.0, 0.0, 6.0]),
            [2.0, 3.0, 4.0, 5.0, 6.0]
        );
    }

    #[test]
    fn trailing_silence_is_held_back() {
        assert_eq!(concealed(&[1.0, 0.0, 0.0]), [1.0]);
    }

    #[test]
    fn held_silence_is_bridged_by_the_next_samples() {
        let mut concealer = DropoutConcealer::new(MAX_DROPOUT);
        concealer.conceal(MONO_1K, [1.0, 0.0]);
        assert_eq!(concealer.conceal(MONO_1K, [3.0]), [2.0, 3.0]);
    }

    #[test]
    fn silence_at_max_dropout_is_bridged() {
        assert_eq!(
            concealed(&[1.0, 0.0, 0.0, 0.0, 1.0]),
            [1.0, 1.0, 1.0, 1.0, 1.0]
        );
    }

    #[test]
    fn silence_beyond_max_dropout_passes_unchanged() {
        assert_eq!(
            concealed(&[1.0, 0.0, 0.0, 0.0, 0.0, 1.0]),
            [1.0, 0.0, 0.0, 0.0, 0.0, 1.0]
        );
    }

    #[test]
    fn silence_in_one_channel_is_not_a_dropout() {
        let mut concealer = DropoutConcealer::new(MAX_DROPOUT);
        let output = concealer.conceal(STEREO_1K, [1.0, 1.0, 0.0, 5.0, 3.0, 3.0]);
        assert_eq!(output, [1.0, 1.0, 0.0, 5.0, 3.0, 3.0]);
    }

    #[test]
    fn channels_are_bridged_separately() {
        let mut concealer = DropoutConcealer::new(MAX_DROPOUT);
        let output = concealer.conceal(STEREO_1K, [2.0, 4.0, 0.0, 0.0, 6.0, 8.0]);
        assert_eq!(output, [2.0, 4.0, 4.0, 6.0, 6.0, 8.0]);
    }

    #[test]
    fn format_change_discards_held_silence() {
        let mut concealer = DropoutConcealer::new(MAX_DROPOUT);
        concealer.conceal(MONO_1K, [1.0, 0.0]);
        assert_eq!(concealer.conceal(STEREO_1K, [3.0, 3.0]), [3.0, 3.0]);
    }
}
