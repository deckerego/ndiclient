//! Keeps the frame-sync's padding out of the jitter buffer. When NDI's
//! frame-sync runs out of received audio, it fills its queue with silence.
//! Played as if it were sound, that silence would cut the audio off without a
//! fade, and the jitter buffer could neither bridge the gap nor learn from it.
//! So after the frame-sync's queue has run empty, exact silence is dropped
//! until sound returns; the buffer then runs down and handles the underrun.

const SILENCE: f32 = 0.0;

pub struct PaddingRemover {
    /// Whether silence received now may be the frame-sync's padding.
    starved: bool,
}

impl PaddingRemover {
    /// Before any audio has arrived, the frame-sync holds only padding.
    pub fn new() -> Self {
        Self { starved: true }
    }

    pub fn note_empty_queue(&mut self) {
        self.starved = true;
    }

    /// Returns interleaved `samples` of `channels` channels without the
    /// silent instants that lead them while the frame-sync is starved.
    pub fn remove(&mut self, channels: usize, samples: Vec<f32>) -> Vec<f32> {
        if !self.starved {
            return samples;
        }
        let padding = samples
            .chunks(channels)
            .take_while(|instant| is_silent(instant))
            .count();
        let sound = samples[padding * channels..].to_vec();
        self.starved = sound.is_empty();
        sound
    }
}

fn is_silent(instant: &[f32]) -> bool {
    instant.iter().all(|&sample| sample == SILENCE)
}

#[cfg(test)]
mod tests {
    use super::*;

    const MONO: usize = 1;
    const STEREO: usize = 2;

    fn after_start(samples: &[f32]) -> Vec<f32> {
        PaddingRemover::new().remove(MONO, samples.to_vec())
    }

    #[test]
    fn padding_before_the_first_sound_is_dropped() {
        assert_eq!(after_start(&[0.0, 0.0, 0.5]), [0.5]);
    }

    #[test]
    fn silence_after_sound_is_kept() {
        assert_eq!(after_start(&[0.5, 0.0, 0.0]), [0.5, 0.0, 0.0]);
    }

    #[test]
    fn padding_without_sound_is_dropped_entirely() {
        assert!(after_start(&[0.0, 0.0]).is_empty());
    }

    #[test]
    fn padding_is_dropped_until_sound_arrives_in_a_later_block() {
        let mut remover = PaddingRemover::new();
        remover.remove(MONO, vec![0.0]);
        assert_eq!(remover.remove(MONO, vec![0.0, 0.5]), [0.5]);
    }

    #[test]
    fn silence_is_kept_once_sound_has_arrived() {
        let mut remover = PaddingRemover::new();
        remover.remove(MONO, vec![0.5]);
        assert_eq!(remover.remove(MONO, vec![0.0, 0.5]), [0.0, 0.5]);
    }

    #[test]
    fn empty_queue_makes_silence_padding_again() {
        let mut remover = PaddingRemover::new();
        remover.remove(MONO, vec![0.5]);
        remover.note_empty_queue();
        assert_eq!(remover.remove(MONO, vec![0.0, 0.5]), [0.5]);
    }

    #[test]
    fn sound_in_one_channel_ends_the_padding() {
        let samples = vec![0.0, 0.0, 0.0, 0.5, 0.0, 0.0];
        let kept = PaddingRemover::new().remove(STEREO, samples);
        assert_eq!(kept, [0.0, 0.5, 0.0, 0.0]);
    }
}
