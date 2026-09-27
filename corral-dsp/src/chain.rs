//! The processing chain shared by the plugin and the offline harness:
//! mono-sum → pitch tracker (pre-gain), then gain on every channel.

use crate::gain;
use crate::pitch::{Detection, PitchTracker};

pub struct Chain {
    tracker: PitchTracker,
}

impl Chain {
    /// Allocates. Call from `initialize()`.
    pub fn new(sample_rate: f32) -> Self {
        Self {
            tracker: PitchTracker::new(sample_rate),
        }
    }

    /// Clears the detector's history; doesn't allocate.
    pub fn reset(&mut self) {
        self.tracker.reset();
    }

    pub fn window_len(&self) -> usize {
        self.tracker.window_len()
    }

    pub fn hop_len(&self) -> usize {
        self.tracker.hop_len()
    }

    /// Processes one frame in place (one sample per channel). The detector
    /// sees the mean of the channels before gain, so the Gain slider
    /// doesn't affect detection. An empty frame's mean is NaN, which the
    /// tracker treats as 0.0.
    #[inline]
    pub fn process_frame(&mut self, frame: &mut [f32], gain: f32) -> Option<Detection> {
        let mono = frame.iter().sum::<f32>() / frame.len() as f32;
        let detection = self.tracker.push(mono);
        for sample in frame.iter_mut() {
            *sample = gain::apply_gain(*sample, gain);
        }
        detection
    }
}
