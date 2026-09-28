//! The processing chain shared by the plugin and the offline harness:
//! mono sum → pitch tracker and shifter (pre-gain); each channel → a delay
//! of the chain's fixed latency; wet gate between the delayed dry signal and
//! the shifted one; dry/wet mix; gain last.

use crate::delay::DelayLine;
use crate::gain;
use crate::period::PeriodSmoother;
use crate::pitch::{Detection, PitchTracker};
use crate::psola::{self, Psola};
use crate::shift::{self, SemitoneRamp};
use crate::wet_gate::WetGate;

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
    smoother: PeriodSmoother,
    ramp: SemitoneRamp,
    gate: WetGate,
    psola: Psola,
}

impl Chain {
    /// Allocates. Call from `initialize()`.
    pub fn new(sample_rate: f32) -> Self {
        let latency = psola::latency_samples(sample_rate);
        Self {
            tracker: PitchTracker::new(sample_rate),
            delay: DelayLine::new(MAX_CHANNELS, latency),
            latency,
            smoother: PeriodSmoother::new(sample_rate),
            ramp: SemitoneRamp::new(sample_rate, shift::DEFAULT),
            gate: WetGate::new(sample_rate),
            psola: Psola::new(sample_rate),
        }
    }

    /// Clears all history and closes the wet gate; doesn't allocate.
    pub fn reset(&mut self) {
        self.tracker.reset();
        self.delay.reset();
        self.smoother.reset();
        self.ramp.reset(shift::DEFAULT);
        self.gate.reset();
        self.psola.reset();
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
    /// The output is delayed by [`Self::latency`] frames. The detector and
    /// the shifter see the mean of the channels before gain, so the Gain
    /// slider doesn't affect detection. An empty frame's mean is NaN, which
    /// both treat as 0.0. While the wet gate is fully dry, or `mix` is 0.0,
    /// each output sample is the delayed input times the gain, bit for bit.
    #[inline]
    pub fn process_frame(&mut self, frame: &mut [f32], settings: Settings) -> Option<Detection> {
        debug_assert!((0.0..=1.0).contains(&settings.mix), "{}", settings.mix);
        let mono = frame.iter().sum::<f32>() / frame.len() as f32;
        let detection = self.tracker.push(mono);
        if let Some(d) = &detection {
            self.smoother.update(d);
            self.gate.update(d.voiced);
        }
        let semitones = self.ramp.next(settings.shift.clamp(shift::MIN, shift::MAX));
        let period = self.smoother.period();

        // Keep the shifter's state in step with the gate. The gate opens
        // only on a voiced hop, which also gives the smoother a period.
        match (self.gate.is_active(), self.psola.is_running(), period) {
            (true, false, Some(p)) => self.psola.start(p),
            (false, true, _) => self.psola.stop(),
            _ => {}
        }
        self.psola.push(mono, period.unwrap_or(0.0));

        self.delay.process(frame); // frame now holds the delayed dry signal
        match period {
            Some(p) if self.psola.is_running() => {
                let shifted = self.psola.next(p, shift::ratio(semitones));
                let (dry_gain, shifted_gain) = self.gate.next_gains();
                // Skipping the arithmetic keeps the dry path bit-exact (−0.0
                // included) when nothing shifted is audible.
                if settings.mix != 0.0 && shifted_gain != 0.0 {
                    for dry in frame.iter_mut() {
                        let wet = dry_gain * *dry + shifted_gain * shifted;
                        *dry += settings.mix * (wet - *dry);
                    }
                }
            }
            _ => {
                // Keeps the fade clock in step; returns (1, 0) at rest.
                self.gate.next_gains();
            }
        }
        for sample in frame.iter_mut() {
            *sample = gain::apply_gain(*sample, settings.gain);
        }
        detection
    }
}
