//! The one place with threading code: a second thread moves audio from NDI's
//! frame-sync into the jitter buffer, so audio keeps flowing while the main
//! thread waits for vsync or window events.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

use crate::jitter_buffer::{self, JitterBuffer};
use crate::ndi::FrameSync;
use crate::padding::PaddingRemover;
use crate::sound::AudioFormat;

/// Tops the buffer up several times per sound card period (~21 ms), and
/// bounds how long the pump takes to notice that it should stop.
const PUMP_INTERVAL: Duration = Duration::from_millis(5);

/// Left in the frame-sync's queue. Run (nearly) empty, it pads its queue
/// with silence, and senders deliver audio in frames of up to about 33 ms.
const FRAME_SYNC_RESERVE: Duration = Duration::from_millis(40);

/// SDL only plays 1, 2, 4, 6 or 8 channels; front left and right carry the
/// usual NDI programme mix.
const PLAYED_CHANNELS: u16 = 2;

/// Runs `body` while the frame-sync's audio is pumped into `buffer` on a
/// second thread, which stops when `body` returns.
pub fn run_alongside<Output>(
    frame_sync: &FrameSync,
    buffer: &Mutex<JitterBuffer>,
    body: impl FnOnce() -> Output,
) -> Output {
    let stop = AtomicBool::new(false);
    thread::scope(|scope| {
        let pump = Pump {
            frame_sync,
            buffer,
            padding: PaddingRemover::new(),
        };
        scope.spawn(|| pump.run(&stop));
        // Also stops the pump if `body` panics, so the scope can join it.
        let _stop_on_exit = StopOnDrop(&stop);
        body()
    })
}

/// Moves audio from the frame-sync into the jitter buffer.
struct Pump<'pump> {
    frame_sync: &'pump FrameSync<'pump>,
    buffer: &'pump Mutex<JitterBuffer>,
    padding: PaddingRemover,
}

impl Pump<'_> {
    fn run(mut self, stop: &AtomicBool) {
        while !stop.load(Ordering::Relaxed) {
            if let Some(format) = self.frame_sync.audio_format() {
                self.top_up(format);
            }
            thread::sleep(PUMP_INTERVAL);
        }
    }

    /// Takes as much audio as the buffer lacks, so the frame-sync is drained
    /// exactly as fast as the sound card plays, which is the clock it
    /// resamples to. Only takes received audio: in a stall the buffer runs
    /// down and handles the underrun.
    fn top_up(&mut self, source: AudioFormat) {
        let queued = self.frame_sync.queued_audio();
        let reserve = source.instant_count(FRAME_SYNC_RESERVE);
        if queued < reserve {
            self.splice();
        }
        let missing =
            jitter_buffer::lock(self.buffer).shortfall(source.narrowed_to(PLAYED_CHANNELS));
        let wanted = missing.min(queued.saturating_sub(reserve));
        if wanted > 0 {
            self.take(source, wanted);
        }
    }

    /// The pump never takes the frame-sync's queue below the reserve, so the
    /// frame-sync has dropped or re-timed audio itself, and what comes next
    /// doesn't carry on from what the buffer holds.
    fn splice(&mut self) {
        self.padding.note_empty_queue();
        jitter_buffer::lock(self.buffer).splice();
    }

    fn take(&mut self, source: AudioFormat, instants: usize) {
        let frame = self.frame_sync.capture_audio(source, instants);
        if let Some(sound) = frame.sound() {
            let sound = sound.first_channels(PLAYED_CHANNELS);
            let channels = usize::from(sound.format.channels);
            let samples = self.padding.remove(channels, sound.interleaved().collect());
            jitter_buffer::lock(self.buffer).push(sound.format, samples);
        }
    }
}

struct StopOnDrop<'flag>(&'flag AtomicBool);

impl Drop for StopOnDrop<'_> {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Relaxed);
    }
}
