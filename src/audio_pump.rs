//! The one place with threading code: a second thread moves the receiver's
//! audio into the jitter buffer, so audio keeps flowing while the main thread
//! waits for video, vsync or window events.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

use crate::dropout::DropoutConcealer;
use crate::jitter_buffer::{self, JitterBuffer};
use crate::ndi::{Capture, Receiver};

/// Bounds how long the pump takes to notice that it should stop.
const CAPTURE_TIMEOUT: Duration = Duration::from_millis(100);

/// SDL only plays 1, 2, 4, 6 or 8 channels; front left and right carry the
/// usual NDI programme mix.
const PLAYED_CHANNELS: u16 = 2;

/// Longer than the dropouts seen from OBS (up to 5 ms), short enough that
/// holding back silence until it ends adds little delay.
const MAX_DROPOUT: Duration = Duration::from_millis(10);

/// Runs `body` while the receiver's audio is pumped into `buffer` on a
/// second thread, which stops when `body` returns.
pub fn run_alongside<Output>(
    receiver: &Receiver,
    buffer: &Mutex<JitterBuffer>,
    body: impl FnOnce() -> Output,
) -> Output {
    let stop = AtomicBool::new(false);
    thread::scope(|scope| {
        scope.spawn(|| pump(receiver, buffer, &stop));
        // Also stops the pump if `body` panics, so the scope can join it.
        let _stop_on_exit = StopOnDrop(&stop);
        body()
    })
}

fn pump(receiver: &Receiver, buffer: &Mutex<JitterBuffer>, stop: &AtomicBool) {
    let mut concealer = DropoutConcealer::new(MAX_DROPOUT);
    while !stop.load(Ordering::Relaxed) {
        // A lost connection is reported by the video loop, which ends the program.
        if let Capture::Frame(frame) = receiver.capture_audio(CAPTURE_TIMEOUT)
            && let Some(sound) = frame.sound()
        {
            let sound = sound.first_channels(PLAYED_CHANNELS);
            let samples = concealer.conceal(sound.format, sound.interleaved());
            jitter_buffer::lock(buffer).push(sound.format, samples);
        }
    }
}

struct StopOnDrop<'flag>(&'flag AtomicBool);

impl Drop for StopOnDrop<'_> {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Relaxed);
    }
}
