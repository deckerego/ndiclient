//! Notices when video stops arriving, so the user can be told once per stall.

use std::time::{Duration, Instant};

pub struct StallDetector {
    threshold: Duration,
    last_frame: Instant,
    reported: bool,
}

impl StallDetector {
    /// Starts counting immediately, so a source that never sends video also stalls.
    pub fn new(threshold: Duration) -> Self {
        Self {
            threshold,
            last_frame: Instant::now(),
            reported: false,
        }
    }

    pub fn record_frame(&mut self) {
        self.last_frame = Instant::now();
        self.reported = false;
    }

    /// True exactly once per stall: the first check after `threshold` passes without a frame.
    pub fn take_new_stall(&mut self) -> bool {
        if self.reported || self.last_frame.elapsed() < self.threshold {
            return false;
        }
        self.reported = true;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_stall_before_threshold() {
        let mut detector = StallDetector::new(Duration::from_secs(60));
        assert!(!detector.take_new_stall());
    }

    #[test]
    fn stall_reported_after_threshold() {
        let mut detector = StallDetector::new(Duration::ZERO);
        assert!(detector.take_new_stall());
    }

    #[test]
    fn stall_reported_only_once() {
        let mut detector = StallDetector::new(Duration::ZERO);
        detector.take_new_stall();
        assert!(!detector.take_new_stall());
    }

    #[test]
    fn frame_rearms_stall_report() {
        let mut detector = StallDetector::new(Duration::ZERO);
        detector.take_new_stall();
        detector.record_frame();
        assert!(detector.take_new_stall());
    }
}
