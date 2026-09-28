//! The processing chain shared by the plugin and the offline harness:
//! mono sum → pitch tracker and shifter (pre-gain); each channel → a delay
//! of the chain's fixed latency; wet gate between the delayed dry signal and
//! the shifted one; dry/wet mix; gain last.

use crate::delay::DelayLine;
use crate::gain;
use crate::pitch::{Detection, PitchTracker};
use crate::psola;

/// The most channels a frame may have.
const MAX_CHANNELS: usize = 2;

/// Per-frame controls. The plugin smooths `gain` and `mix`; `Chain` ramps
/// `shift` itself.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Settings {
    /// Linear gain, applied last.
    pub gain: f32,
    /// 0.0 = dry only, 1.0 = wet only.
    pub mix: f32,
    /// Whole semitones, clamped to `shift::MIN..=shift::MAX`.
    pub shift: i32,
}

pub struct Chain {
    tracker: PitchTracker,
    /// Delays the dry signal by `latency` frames.
    delay: DelayLine,
    latency: usize,
}

impl Chain {
    /// Allocates. Call from `initialize()`.
    pub fn new(sample_rate: f32) -> Self {
        let latency = psola::latency_samples(sample_rate);
        Self {
            tracker: PitchTracker::new(sample_rate),
            delay: DelayLine::new(MAX_CHANNELS, latency),
            latency,
        }
    }

    /// Clears the detector's and the delay's history; doesn't allocate.
    pub fn reset(&mut self) {
        self.tracker.reset();
        self.delay.reset();
    }

    pub fn window_len(&self) -> usize {
        self.tracker.window_len()
    }

    pub fn hop_len(&self) -> usize {
        self.tracker.hop_len()
    }

    /// The fixed delay, in frames, from input to output. Depends only on the
    /// sample rate.
    pub fn latency(&self) -> usize {
        self.latency
    }

    /// Processes one frame in place (one sample per channel, at most two).
    /// The output is delayed by [`Self::latency`] frames. The detector sees
    /// the mean of the channels before gain, so the Gain slider doesn't
    /// affect detection. An empty frame's mean is NaN, which the tracker
    /// treats as 0.0.
    #[inline]
    pub fn process_frame(&mut self, frame: &mut [f32], settings: Settings) -> Option<Detection> {
        debug_assert!((0.0..=1.0).contains(&settings.mix), "{}", settings.mix);
        let mono = frame.iter().sum::<f32>() / frame.len() as f32;
        let detection = self.tracker.push(mono);
        self.delay.process(frame); // frame now holds the delayed dry signal
        // Phase 3 replaces this with the wet gate and shifter.
        for sample in frame.iter_mut() {
            *sample = gain::apply_gain(*sample, settings.gain);
        }
        detection
    }
}
