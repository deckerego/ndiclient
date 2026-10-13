//! Periodic frames-per-second reporting, used to judge playback performance.

use std::time::{Duration, Instant};

pub struct FrameRateMeter {
    report_interval: Duration,
    interval_start: Instant,
    frames_in_interval: u32,
}

impl FrameRateMeter {
    pub fn new(report_interval: Duration) -> Self {
        Self {
            report_interval,
            interval_start: Instant::now(),
            frames_in_interval: 0,
        }
    }

    /// Counts one shown frame; returns the average rate whenever an interval completes.
    pub fn record_frame(&mut self) -> Option<f64> {
        self.frames_in_interval += 1;
        let elapsed = self.interval_start.elapsed();
        if elapsed < self.report_interval {
            return None;
        }
        let frames_per_second = f64::from(self.frames_in_interval) / elapsed.as_secs_f64();
        self.interval_start = Instant::now();
        self.frames_in_interval = 0;
        Some(frames_per_second)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_report_before_interval_elapses() {
        let mut meter = FrameRateMeter::new(Duration::from_secs(60));
        assert_eq!(meter.record_frame(), None);
    }

    #[test]
    fn report_once_interval_elapses() {
        let mut meter = FrameRateMeter::new(Duration::ZERO);
        assert!(meter.record_frame().is_some());
    }
}
