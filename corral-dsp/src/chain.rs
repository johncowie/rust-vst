//! The processing chain shared by the plugin and the offline harness:
//! mono sum → pitch tracker and shifter (pre-gain); each channel → a delay
//! of the chain's fixed latency; wet gate between the delayed dry signal and
//! the shifted one; dry/wet mix; gain last. Once per hop the note stabiliser
//! picks the stable note, and the harmony interval comes from it; the shift
//! glides toward that interval, and jumps to it at voicing onset.

use crate::delay::DelayLine;
use crate::gain;
use crate::harmony::IntervalSource;
use crate::note;
use crate::period::PeriodSmoother;
use crate::pitch::{Detection, PitchTracker};
use crate::psola::{self, Analysis, Voice};
use crate::shift::{self, Glide};
use crate::stabiliser::{NoteStabiliser, Update};
use crate::wet_gate::WetGate;

/// The most channels a frame may have.
const MAX_CHANNELS: usize = 2;

/// Per-frame controls. The plugin smooths `gain` and `mix`; `Chain`
/// glides the interval itself.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Settings {
    /// Linear gain, applied last.
    pub gain: f32,
    /// 0.0 = dry only, 1.0 = wet only.
    pub mix: f32,
    pub interval: IntervalSource,
    /// Glide time constant in seconds, 0.0..=shift::MAX_GLIDE_S.
    pub glide_s: f32,
}

/// The stable note and the interval chosen for it; the harmony note is
/// `note + interval`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Harmony {
    pub note: i32,
    pub interval: i32,
}

pub struct Chain {
    tracker: PitchTracker,
    /// Delays the dry signal by `latency` frames.
    delay: DelayLine,
    latency: usize,
    smoother: PeriodSmoother,
    stabiliser: NoteStabiliser,
    glide: Glide,
    sample_rate: f32,
    /// The glide time the coefficient was computed for, so `exp` runs only
    /// when it changes.
    glide_s: f32,
    glide_coefficient: f32,
    harmony: Option<Harmony>,
    gate: WetGate,
    analysis: Analysis,
    voice: Voice,
}

impl Chain {
    /// Allocates. Call from `initialize()`.
    pub fn new(sample_rate: f32) -> Self {
        let latency = psola::latency_samples(sample_rate);
        let tracker = PitchTracker::new(sample_rate);
        let hop_len = tracker.hop_len();
        Self {
            tracker,
            delay: DelayLine::new(MAX_CHANNELS, latency),
            latency,
            smoother: PeriodSmoother::new(sample_rate),
            stabiliser: NoteStabiliser::new(sample_rate, hop_len),
            glide: Glide::new(0.0),
            sample_rate,
            glide_s: shift::DEFAULT_GLIDE_S,
            glide_coefficient: shift::glide_coefficient(shift::DEFAULT_GLIDE_S, sample_rate),
            harmony: None,
            gate: WetGate::new(sample_rate),
            analysis: Analysis::new(sample_rate),
            voice: Voice::new(),
        }
    }

    /// Clears all history and closes the wet gate; doesn't allocate.
    pub fn reset(&mut self) {
        self.tracker.reset();
        self.delay.reset();
        self.smoother.reset();
        self.stabiliser.reset();
        self.glide.jump(0.0);
        self.harmony = None;
        self.gate.reset();
        self.analysis.reset();
        self.voice.stop();
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

    /// The latest stable note and its interval. Kept through unvoiced
    /// hops; callers blank it for display.
    pub fn harmony(&self) -> Option<Harmony> {
        self.harmony
    }

    /// The glide's current value in semitones, for tests.
    pub fn shift_semitones(&self) -> f32 {
        self.glide.current()
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
            if d.voiced
                && let Some(period) = self.smoother.period()
            {
                let pitch = note::midi_from_hz(self.sample_rate / period);
                // Onset: no harmony is sounding, so the note is taken at once
                // and the shift jumps to its interval (no glide after silence).
                let onset = !self.gate.is_open();
                if self.stabiliser.update(pitch, onset) == Update::Onset {
                    let note = self.stabiliser.note().expect("just chosen");
                    self.glide.jump(settings.interval.interval(note) as f32);
                }
            }
        }
        self.harmony = self.stabiliser.note().map(|note| Harmony {
            note,
            interval: settings.interval.interval(note),
        });
        if settings.glide_s != self.glide_s {
            self.glide_s = settings.glide_s;
            self.glide_coefficient = shift::glide_coefficient(
                settings.glide_s.min(shift::MAX_GLIDE_S),
                self.sample_rate,
            );
        }
        let semitones = match self.harmony {
            Some(h) => self.glide.next(h.interval as f32, self.glide_coefficient),
            None => self.glide.current(),
        };
        let period = self.smoother.period();

        // Keep the shifter's state in step with the gate. The gate opens
        // only on a voiced hop, which also gives the smoother a period.
        match (self.gate.is_active(), self.analysis.is_running(), period) {
            (true, false, Some(p)) => {
                let mark = self.analysis.start(p);
                self.voice.start(mark);
            }
            (false, true, _) => {
                self.analysis.stop();
                self.voice.stop();
            }
            _ => {}
        }
        self.analysis.push(mono, period.unwrap_or(0.0));

        self.delay.process(frame); // frame now holds the delayed dry signal
        match period {
            Some(p) if self.analysis.is_running() => {
                let shifted = self.voice.next(&self.analysis, p, shift::ratio(semitones));
                self.analysis.prune();
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
