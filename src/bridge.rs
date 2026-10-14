//! Fills a hole that a sender punched into continuous audio. Such a sender
//! inserts silence rather than replacing sound with it, so the sound after
//! the hole carries straight on from the sound before it. The fill is a
//! cross-fade from the sound after the hole into the sound before it: it
//! starts as the continuation of what came before and ends leading into
//! what follows, so neither seam clicks and the music keeps its tone.

const SILENCE: f32 = 0.0;

/// Interleaved audio of `instants` instants of `channels` channels to put
/// between `before` and `after`. Whatever part of it neither side holds
/// enough sound for stays silent.
pub fn between(before: &[f32], after: &[f32], channels: usize, instants: usize) -> Vec<f32> {
    let faded = instants
        .min(before.len() / channels)
        .min(after.len() / channels)
        * channels;
    let mut fill = cross_fade(&after[..faded], &before[before.len() - faded..], channels);
    fill.resize(instants * channels, SILENCE);
    fill
}

/// Fades from `from` into `to`, with one gain per instant.
fn cross_fade(from: &[f32], to: &[f32], channels: usize) -> Vec<f32> {
    let instants = from.len() / channels;
    from.iter()
        .zip(to)
        .enumerate()
        .map(|(index, (&a, &b))| {
            let weight = raised_cosine(index / channels, instants);
            a * (1.0 - weight) + b * weight
        })
        .collect()
}

/// Rises smoothly from near 0 at the first of `count` steps to near 1 at the last.
fn raised_cosine(index: usize, count: usize) -> f32 {
    let progress = (index as f32 + 0.5) / count as f32;
    0.5 - 0.5 * (std::f32::consts::PI * progress).cos()
}

#[cfg(test)]
mod tests {
    use super::*;

    const PERIOD: usize = 8;

    /// A waveform that repeats every `PERIOD` instants.
    fn periodic(length: usize) -> Vec<f32> {
        (0..length)
            .map(|t| [1.0, 3.0, 4.0, 2.0, -1.0, -3.0, -4.0, -2.0][t % PERIOD])
            .collect()
    }

    #[test]
    fn hole_of_a_whole_period_is_filled_with_the_waveform() {
        let signal = periodic(64);
        let fill = between(&signal[..32], &signal[32..], 1, PERIOD);
        assert_eq!(fill, periodic(PERIOD));
    }

    #[test]
    fn fill_cross_fades_from_the_sound_after_into_the_sound_before() {
        assert_eq!(between(&[1.0, 1.0], &[3.0, 3.0], 1, 1), [2.0]);
    }

    #[test]
    fn channels_fade_separately() {
        assert_eq!(between(&[1.0, -1.0], &[3.0, -3.0], 2, 1), [2.0, -2.0]);
    }

    #[test]
    fn fill_beyond_the_sound_available_stays_silent() {
        assert_eq!(between(&[1.0], &[3.0], 1, 2), [2.0, SILENCE]);
    }

    #[test]
    fn silent_sides_give_a_silent_fill() {
        assert_eq!(between(&[0.0; 4], &[0.0; 4], 1, 2), [0.0, 0.0]);
    }
}
