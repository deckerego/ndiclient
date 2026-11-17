//! Notices when video stops arriving, so the user can be told once per
//! stall, and decides when a stall has gone on long enough to reconnect.

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

    /// True once `patience` has passed since the stall began, which is
    /// `threshold` after the last frame.
    pub fn has_waited(&self, patience: Duration) -> bool {
        self.last_frame.elapsed() >= self.threshold + patience
    }
}

/// What to do while a stall goes on: wait for the video as long as it takes,
/// or give up on the connection after waiting a while and connect anew.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RetryPolicy {
    KeepWaiting,
    ReconnectAfter(Duration),
}

impl RetryPolicy {
    pub fn gives_up_on(self, stall: &StallDetector) -> bool {
        match self {
            Self::KeepWaiting => false,
            Self::ReconnectAfter(patience) => stall.has_waited(patience),
        }
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

    const LONG: Duration = Duration::from_secs(60);

    #[test]
    fn keep_waiting_never_gives_up() {
        let stall = StallDetector::new(Duration::ZERO);
        assert!(!RetryPolicy::KeepWaiting.gives_up_on(&stall));
    }

    #[test]
    fn reconnect_gives_up_after_patience() {
        let stall = StallDetector::new(Duration::ZERO);
        assert!(RetryPolicy::ReconnectAfter(Duration::ZERO).gives_up_on(&stall));
    }

    #[test]
    fn reconnect_waits_out_patience() {
        let stall = StallDetector::new(Duration::ZERO);
        assert!(!RetryPolicy::ReconnectAfter(LONG).gives_up_on(&stall));
    }

    #[test]
    fn patience_starts_when_stall_begins() {
        let stall = StallDetector::new(LONG);
        assert!(!RetryPolicy::ReconnectAfter(Duration::ZERO).gives_up_on(&stall));
    }
}
