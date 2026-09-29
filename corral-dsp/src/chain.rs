//! The processing chain shared by the plugin and the offline harness:
//! mono sum → pitch tracker and the shifter's shared analysis; each
//! channel → a delay of the chain's fixed latency; wet gate between the
//! delayed dry signal and the summed harmony voices; dry/wet mix. Once per
//! hop the note stabiliser picks the stable note, and the grid's row for
//! its pitch class says which intervals sound: one voice each, at full
//! level. When the note changes the voices glide (matched by position) or
//! fade (surplus); edits of the sounding row fade their own voice; at
//! voicing onset the voices jump. See `voices`.

use crate::delay::DelayLine;
use crate::grid::{Grid, Row};
use crate::note;
use crate::period::PeriodSmoother;
use crate::pitch::{Detection, PitchTracker};
use crate::psola::{self, Analysis};
use crate::shift;
use crate::stabiliser::{NoteStabiliser, Update};
use crate::voices::{VoicePool, VoiceState};
use crate::wet_gate::WetGate;

/// The most channels a frame may have.
const MAX_CHANNELS: usize = 2;

/// Per-frame controls. The plugin smooths `mix`; `Chain` glides and
/// fades the voices itself.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Settings {
    /// 0.0 = dry only, 1.0 = wet only.
    pub mix: f32,
    /// Which intervals sound for each pitch class. The plugin copies it
    /// once per buffer.
    pub grid: Grid,
    /// Glide time constant in seconds, 0.0..=shift::MAX_GLIDE_S.
    pub glide_s: f32,
}

pub struct Chain {
    tracker: PitchTracker,
    /// Delays the dry signal by `latency` frames.
    delay: DelayLine,
    latency: usize,
    smoother: PeriodSmoother,
    stabiliser: NoteStabiliser,
    sample_rate: f32,
    /// The glide time the coefficient was computed for, so `exp` runs only
    /// when it changes.
    glide_s: f32,
    glide_coefficient: f32,
    gate: WetGate,
    analysis: Analysis,
    voices: VoicePool,
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
            sample_rate,
            glide_s: shift::DEFAULT_GLIDE_S,
            glide_coefficient: shift::glide_coefficient(shift::DEFAULT_GLIDE_S, sample_rate),
            gate: WetGate::new(sample_rate),
            analysis: Analysis::new(sample_rate),
            voices: VoicePool::new(sample_rate),
        }
    }

    /// Clears all history and closes the wet gate; doesn't allocate.
    pub fn reset(&mut self) {
        self.tracker.reset();
        self.delay.reset();
        self.smoother.reset();
        self.stabiliser.reset();
        self.gate.reset();
        self.analysis.reset();
        self.voices.reset();
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

    /// The latest stable note. Kept through unvoiced hops; callers blank
    /// it for display.
    pub fn stable_note(&self) -> Option<i32> {
        self.stabiliser.note()
    }

    /// The row the sounding voices follow, for the harness CSV.
    pub fn sounding(&self) -> Row {
        self.voices.row()
    }

    /// The live voices, for tests.
    pub fn voices(&self) -> impl Iterator<Item = VoiceState> + '_ {
        self.voices.states()
    }

    /// Processes one frame in place (one sample per channel, at most two).
    /// The output is delayed by [`Self::latency`] frames. The detector and
    /// the shifter see the mean of the channels. An empty frame's mean is
    /// NaN, which both treat as 0.0. While the wet gate is fully dry, or
    /// `mix` is 0.0, each output sample is the delayed input, bit for bit.
    #[inline]
    pub fn process_frame(&mut self, frame: &mut [f32], settings: Settings) -> Option<Detection> {
        debug_assert!((0.0..=1.0).contains(&settings.mix), "{}", settings.mix);
        let mono = frame.iter().sum::<f32>() / frame.len() as f32;
        let detection = self.tracker.push(mono);
        let mut update = None;
        if let Some(d) = &detection {
            self.smoother.update(d);
            self.gate.update(d.voiced);
            if d.voiced
                && let Some(period) = self.smoother.period()
            {
                let pitch = note::midi_from_hz(self.sample_rate / period);
                // Onset: no harmony is sounding, so the note is taken at once.
                update = Some(self.stabiliser.update(pitch, !self.gate.is_open()));
            }
        }
        if let Some(note) = self.stabiliser.note() {
            let row = settings.grid.row_for(note);
            match update {
                // The voices jump to their intervals (no glide after silence).
                Some(Update::Onset) => self.voices.onset(row, !self.gate.is_active()),
                Some(Update::Changed) => self.voices.change(row),
                _ if row != self.voices.row() => self.voices.edit(row),
                _ => {}
            }
        }
        if settings.glide_s != self.glide_s {
            self.glide_s = settings.glide_s;
            self.glide_coefficient = shift::glide_coefficient(
                settings.glide_s.min(shift::MAX_GLIDE_S),
                self.sample_rate,
            );
        }
        let period = self.smoother.period();

        // Keep the shifter's state in step with the gate. The gate opens
        // only on a voiced hop, which also gives the smoother a period.
        match (self.gate.is_active(), self.analysis.is_running(), period) {
            (true, false, Some(p)) => {
                let mark = self.analysis.start(p);
                self.voices.start_all(mark);
            }
            (false, true, _) => {
                self.analysis.stop();
                self.voices.stop_all();
            }
            _ => {}
        }
        self.analysis.push(mono, period.unwrap_or(0.0));

        self.delay.process(frame); // frame now holds the delayed dry signal
        match period {
            Some(p) if self.analysis.is_running() => {
                let shifted = self
                    .voices
                    .next(Some((&self.analysis, p)), self.glide_coefficient);
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
                // Keeps the glides, the fades and the gate's clock in step;
                // the gate returns (1, 0) at rest.
                self.voices.next(None, self.glide_coefficient);
                self.gate.next_gains();
            }
        }
        detection
    }
}
