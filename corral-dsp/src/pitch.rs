//! Realtime pitch detection: YIN, with its difference function computed
//! through FFT, run once per hop over a ring buffer of mono input, plus a
//! voicing gate on level and confidence.

use std::sync::Arc;

use realfft::num_complex::Complex;
use realfft::{ComplexToReal, RealFftPlanner, RealToComplex};

/// Detection range: E2 (82.4 Hz) sits just above the floor; ~B5 (988 Hz) below the top.
pub const MIN_F0_HZ: f32 = 80.0;
pub const MAX_F0_HZ: f32 = 1000.0;
/// YIN's absolute threshold on the cumulative mean normalised difference.
const YIN_THRESHOLD: f32 = 0.15;
/// Voiced needs confidence (1 − CMND at the chosen lag) at or above this…
const MIN_CONFIDENCE: f32 = 0.8;
/// …and the window RMS at or above this.
const LEVEL_GATE_DBFS: f32 = -50.0;

/// The lag range is widened by this factor beyond the detection range, so
/// the parabolic fit has a neighbour on each side at the edges.
const LAG_MARGIN: f32 = 1.1;
/// About 50 cents: an estimate this far outside the detection range still
/// counts as voiced.
const RANGE_MARGIN: f32 = 1.03;

/// Window length at 48 kHz and below; it doubles for each further multiple.
const BASE_WINDOW_LEN: usize = 2048;
const HOPS_PER_WINDOW: usize = 8;

/// One hop's output. `f0_hz` is YIN's estimate (0.0 when the level gate
/// skipped analysis); `voiced` is the gate's decision.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Detection {
    pub f0_hz: f32,
    pub confidence: f32,
    pub voiced: bool,
    /// Index (samples since reset) of the analysis window's centre sample.
    pub window_centre: u64,
}

/// YIN on one window. Owns every buffer and both FFT plans; `analyse` never allocates.
struct Yin {
    sample_rate: f32,
    tau_min: usize,
    tau_max: usize,
    integration_len: usize,
    fft_len: usize,
    forward: Arc<dyn RealToComplex<f32>>,
    inverse: Arc<dyn ComplexToReal<f32>>,
    /// FFT input, and the inverse FFT's output. `fft_len` long.
    fft_in: Vec<f32>,
    /// Spectra of the whole window and of its first `integration_len` samples.
    spectrum_x: Vec<Complex<f32>>,
    spectrum_y: Vec<Complex<f32>>,
    scratch_forward: Vec<Complex<f32>>,
    scratch_inverse: Vec<Complex<f32>>,
    /// Unnormalised cross-correlation of the window with its first
    /// `integration_len` samples. `fft_len` long.
    correlation: Vec<f32>,
    /// `energy_prefix[i]` is the sum of the first `i` squared samples.
    energy_prefix: Vec<f64>,
    /// Cumulative mean normalised difference for lags 0..=tau_max + 1.
    cmnd: Vec<f32>,
}

impl Yin {
    fn new(sample_rate: f32, window_len: usize) -> Self {
        let integration_len = window_len / 2;
        let tau_min = ((sample_rate / (MAX_F0_HZ * LAG_MARGIN)).floor() as usize).max(2);
        let tau_max = (sample_rate / (MIN_F0_HZ / LAG_MARGIN)).ceil() as usize;
        assert!(
            tau_max + 1 + integration_len <= window_len,
            "window of {window_len} is too short for lags up to {tau_max}"
        );
        // Long enough that the correlation doesn't wrap for any lag we read.
        let fft_len = (window_len + integration_len).next_power_of_two();

        let mut planner = RealFftPlanner::new();
        let forward = planner.plan_fft_forward(fft_len);
        let inverse = planner.plan_fft_inverse(fft_len);
        Self {
            sample_rate,
            tau_min,
            tau_max,
            integration_len,
            fft_len,
            fft_in: forward.make_input_vec(),
            spectrum_x: forward.make_output_vec(),
            spectrum_y: forward.make_output_vec(),
            scratch_forward: forward.make_scratch_vec(),
            scratch_inverse: inverse.make_scratch_vec(),
            correlation: inverse.make_output_vec(),
            energy_prefix: vec![0.0; window_len + 1],
            cmnd: vec![0.0; tau_max + 2],
            forward,
            inverse,
        }
    }

    /// Returns `(f0_hz, confidence)` for one window of `window_len` finite samples.
    fn analyse(&mut self, window: &[f32]) -> (f32, f32) {
        let w = self.integration_len;

        // 1. Cross term r(τ) = Σ_{j<W} x_j·x_{j+τ}, as IFFT(X · conj(Y)).
        self.fft_in[..window.len()].copy_from_slice(window);
        self.fft_in[window.len()..].fill(0.0);
        self.forward
            .process_with_scratch(
                &mut self.fft_in,
                &mut self.spectrum_x,
                &mut self.scratch_forward,
            )
            .expect("FFT lengths are fixed at setup");
        self.fft_in[..w].copy_from_slice(&window[..w]);
        self.fft_in[w..].fill(0.0);
        self.forward
            .process_with_scratch(
                &mut self.fft_in,
                &mut self.spectrum_y,
                &mut self.scratch_forward,
            )
            .expect("FFT lengths are fixed at setup");
        for (x, y) in self.spectrum_x.iter_mut().zip(&self.spectrum_y) {
            *x *= y.conj();
        }
        // A real signal's DC and Nyquist bins are real; the inverse rejects anything else.
        let last = self.spectrum_x.len() - 1;
        self.spectrum_x[0].im = 0.0;
        self.spectrum_x[last].im = 0.0;
        self.inverse
            .process_with_scratch(
                &mut self.spectrum_x,
                &mut self.correlation,
                &mut self.scratch_inverse,
            )
            .expect("FFT lengths are fixed at setup");

        // 2. Energy prefix sums, for the two energy terms of d(τ).
        let mut sum = 0.0;
        for (prefix, &x) in self.energy_prefix[1..].iter_mut().zip(window) {
            sum += f64::from(x) * f64::from(x);
            *prefix = sum;
        }

        // 3. d(τ) = Σ x_j² + Σ x_{j+τ}² − 2·r(τ), then the cumulative mean
        // normalised difference.
        let p = &self.energy_prefix;
        let scale = 1.0 / self.fft_len as f64;
        self.cmnd[0] = 1.0;
        let mut running = 0.0;
        for tau in 1..self.cmnd.len() {
            let r = f64::from(self.correlation[tau]) * scale;
            let d = (p[w] + p[tau + w] - p[tau] - 2.0 * r).max(0.0);
            running += d;
            self.cmnd[tau] = if running > 0.0 {
                (d * tau as f64 / running) as f32
            } else {
                1.0
            };
        }

        // 4. The first dip below the threshold, walked down to its minimum;
        // otherwise the global minimum.
        let (tau_min, tau_max) = (self.tau_min, self.tau_max);
        let tau = match (tau_min..=tau_max).find(|&t| self.cmnd[t] < YIN_THRESHOLD) {
            Some(mut t) => {
                while t < tau_max && self.cmnd[t + 1] < self.cmnd[t] {
                    t += 1;
                }
                t
            }
            None => (tau_min..=tau_max)
                .min_by(|&a, &b| self.cmnd[a].total_cmp(&self.cmnd[b]))
                .unwrap_or(tau_min),
        };

        // 5. Parabolic refinement around the chosen lag (tau_min ≥ 2, and
        // cmnd has an entry past tau_max, so both neighbours exist).
        let (a, b, c) = (self.cmnd[tau - 1], self.cmnd[tau], self.cmnd[tau + 1]);
        let denominator = a - 2.0 * b + c;
        let offset = if denominator > f32::EPSILON {
            (0.5 * (a - c) / denominator).clamp(-1.0, 1.0)
        } else {
            0.0
        };
        let f0 = self.sample_rate / (tau as f32 + offset);
        let confidence = (1.0 - b).clamp(0.0, 1.0);
        if f0.is_finite() && confidence.is_finite() {
            (f0, confidence)
        } else {
            (0.0, 0.0)
        }
    }
}

/// Collects mono input in a ring buffer and runs YIN once per hop,
/// independent of the host's buffer size.
pub struct PitchTracker {
    yin: Yin,
    /// The last `window_len` samples; the oldest is at `write_pos`.
    ring: Vec<f32>,
    write_pos: usize,
    /// Samples in `ring` since reset; saturates at `window_len`.
    filled: usize,
    since_hop: usize,
    hop_len: usize,
    samples_seen: u64,
    /// Linearised copy of the ring, oldest first.
    window: Vec<f32>,
    /// 10^(LEVEL_GATE_DBFS/20).
    level_gate_rms: f32,
}

impl PitchTracker {
    /// Allocates everything. Call from `initialize()`, not the audio thread.
    pub fn new(sample_rate: f32) -> Self {
        let multiple = (sample_rate / 48_000.0).ceil().max(1.0) as usize;
        let window_len = (BASE_WINDOW_LEN * multiple).next_power_of_two();
        Self {
            yin: Yin::new(sample_rate, window_len),
            ring: vec![0.0; window_len],
            write_pos: 0,
            filled: 0,
            since_hop: 0,
            hop_len: window_len / HOPS_PER_WINDOW,
            samples_seen: 0,
            window: vec![0.0; window_len],
            level_gate_rms: 10.0f32.powf(LEVEL_GATE_DBFS / 20.0),
        }
    }

    /// Clears history; doesn't allocate.
    pub fn reset(&mut self) {
        self.ring.fill(0.0);
        self.write_pos = 0;
        self.filled = 0;
        self.since_hop = 0;
        self.samples_seen = 0;
    }

    pub fn window_len(&self) -> usize {
        self.ring.len()
    }

    pub fn hop_len(&self) -> usize {
        self.hop_len
    }

    /// Non-finite samples are treated as 0.0. Returns a detection every
    /// `hop_len` samples once the window has filled.
    pub fn push(&mut self, sample: f32) -> Option<Detection> {
        let window_len = self.ring.len();
        self.ring[self.write_pos] = if sample.is_finite() { sample } else { 0.0 };
        self.write_pos = (self.write_pos + 1) % window_len;
        self.filled = (self.filled + 1).min(window_len);
        self.samples_seen += 1;
        self.since_hop += 1;
        if self.since_hop < self.hop_len {
            return None;
        }
        self.since_hop = 0;
        if self.filled < window_len {
            return None;
        }

        let (newest, oldest) = self.ring.split_at(self.write_pos);
        self.window[..oldest.len()].copy_from_slice(oldest);
        self.window[oldest.len()..].copy_from_slice(newest);

        let energy: f64 = self
            .window
            .iter()
            .map(|&x| f64::from(x) * f64::from(x))
            .sum();
        let rms = (energy / window_len as f64).sqrt() as f32;
        let (f0_hz, confidence) = if rms >= self.level_gate_rms {
            self.yin.analyse(&self.window)
        } else {
            (0.0, 0.0)
        };
        let in_range = (MIN_F0_HZ / RANGE_MARGIN..=MAX_F0_HZ * RANGE_MARGIN).contains(&f0_hz);
        Some(Detection {
            f0_hz,
            confidence,
            voiced: confidence >= MIN_CONFIDENCE && in_range,
            window_centre: self.samples_seen - (window_len / 2) as u64,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(sample_rate: f32, freq: f32, len: usize) -> impl Iterator<Item = f32> {
        (0..len).map(move |i| {
            let t = i as f64 / f64::from(sample_rate);
            (0.5 * (std::f64::consts::TAU * f64::from(freq) * t).sin()) as f32
        })
    }

    /// Pushes every sample and returns the detections.
    fn detections(tracker: &mut PitchTracker, signal: impl Iterator<Item = f32>) -> Vec<Detection> {
        signal.filter_map(|s| tracker.push(s)).collect()
    }

    #[test]
    fn window_and_hop_scale_with_the_sample_rate() {
        for (sample_rate, window, hop) in [
            (44_100.0, 2048, 256),
            (48_000.0, 2048, 256),
            (96_000.0, 4096, 512),
        ] {
            // `new` asserts the lag range fits the window.
            let tracker = PitchTracker::new(sample_rate);
            assert_eq!(
                (tracker.window_len(), tracker.hop_len()),
                (window, hop),
                "{sample_rate} Hz"
            );
        }
    }

    #[test]
    fn detects_once_per_hop_after_the_window_fills() {
        let mut tracker = PitchTracker::new(44_100.0);
        let (window, hop) = (tracker.window_len(), tracker.hop_len());
        for i in 1..window {
            assert!(tracker.push(0.0).is_none(), "detection at sample {i}");
        }
        let mut centres = Vec::new();
        for i in 0..hop * 4 {
            let detection = tracker.push(0.0);
            assert_eq!(detection.is_some(), i % hop == 0, "sample {}", window + i);
            centres.extend(detection.map(|d| d.window_centre));
        }
        let first = (window / 2) as u64;
        let expected: Vec<u64> = (0..4).map(|k| first + (k * hop) as u64).collect();
        assert_eq!(centres, expected);
    }

    #[test]
    fn a_sine_is_detected_within_five_cents() {
        for sample_rate in [44_100.0, 48_000.0] {
            let mut tracker = PitchTracker::new(sample_rate);
            let found = detections(&mut tracker, sine(sample_rate, 220.0, 8192));
            assert!(!found.is_empty());
            for d in found {
                let cents = 1200.0 * (d.f0_hz / 220.0).log2();
                assert!(cents.abs() < 5.0, "{sample_rate} Hz: {} Hz", d.f0_hz);
                assert!(d.confidence >= 0.9, "confidence {}", d.confidence);
                assert!(d.voiced);
            }
        }
    }

    #[test]
    fn reset_restarts_the_fill() {
        let mut tracker = PitchTracker::new(48_000.0);
        let window = tracker.window_len();
        assert!(!detections(&mut tracker, sine(48_000.0, 220.0, window + 100)).is_empty());
        tracker.reset();
        for i in 1..window {
            assert!(tracker.push(0.1).is_none(), "detection at sample {i}");
        }
        let detection = tracker
            .push(0.1)
            .expect("a detection once the window refills");
        assert_eq!(detection.window_centre, (window / 2) as u64);
    }

    #[test]
    fn non_finite_input_gives_finite_output() {
        let mut tracker = PitchTracker::new(44_100.0);
        let signal = sine(44_100.0, 220.0, 8192)
            .enumerate()
            .map(|(i, s)| match i % 50 {
                0 => f32::NAN,
                1 => f32::INFINITY,
                2 => f32::NEG_INFINITY,
                _ => s,
            });
        let found = detections(&mut tracker, signal);
        assert!(!found.is_empty());
        for d in found {
            assert!(d.f0_hz.is_finite() && d.confidence.is_finite(), "{d:?}");
        }

        tracker.reset();
        let found = detections(&mut tracker, std::iter::repeat_n(f32::NAN, 4096));
        assert!(found.iter().all(|d| d.f0_hz == 0.0 && !d.voiced));
    }
}
