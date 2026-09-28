//! Signal generators and helpers shared by the integration tests.

// Each test binary compiles this module and uses a different subset of it.
#![allow(dead_code)]

use std::f64::consts::TAU;
use std::path::Path;

use corral_dsp::chain::{Chain, Settings};
use corral_dsp::eval::{CentsStats, EstFrame, Scores};
use corral_dsp::pitch::Detection;

/// A tone whose fundamental follows `freq_at(t)`, with harmonics at
/// amplitude 1/k. Each harmonic's phase is integrated per sample, so pitch
/// changes are phase-continuous. Harmonics above 0.45·sr are dropped, and
/// the result is normalised to `peak`.
pub fn harmonic_tone(
    sample_rate: f32,
    seconds: f64,
    freq_at: impl Fn(f64) -> f64,
    harmonics: usize,
    peak: f32,
) -> Vec<f32> {
    let sr = f64::from(sample_rate);
    let len = (seconds * sr).round() as usize;
    let mut phase = 0.0;
    let mut signal: Vec<f64> = (0..len)
        .map(|i| {
            let freq = freq_at(i as f64 / sr);
            let value = (1..=harmonics)
                .take_while(|&k| k as f64 * freq < 0.45 * sr)
                .map(|k| (k as f64 * phase).sin() / k as f64)
                .sum();
            phase = (phase + TAU * freq / sr) % TAU;
            value
        })
        .collect();
    let max = signal.iter().fold(0.0f64, |m, s| m.max(s.abs()));
    if max > 0.0 {
        signal.iter_mut().for_each(|s| *s *= f64::from(peak) / max);
    }
    signal.into_iter().map(|s| s as f32).collect()
}

/// Uniform white noise in ±`peak`, from a xorshift32 PRNG (no `rand` dependency).
pub fn white_noise(len: usize, peak: f32, seed: u32) -> Vec<f32> {
    let mut state = seed.max(1);
    (0..len)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            (state as f32 / u32::MAX as f32 * 2.0 - 1.0) * peak
        })
        .collect()
}

/// The frequency of a MIDI note (A4 = 440 Hz).
pub fn midi_to_hz(midi: f64) -> f64 {
    440.0 * 2f64.powf((midi - 69.0) / 12.0)
}

/// The C3 → C5 major scale (MIDI notes), 15 steps.
pub const SCALE_STEPS: [f64; 15] = [
    48.0, 50.0, 52.0, 53.0, 55.0, 57.0, 59.0, 60.0, 62.0, 64.0, 65.0, 67.0, 69.0, 71.0, 72.0,
];
/// How long each scale note lasts.
pub const SCALE_NOTE_S: f64 = 0.4;

/// The scale's length in seconds.
pub fn scale_seconds() -> f64 {
    SCALE_NOTE_S * SCALE_STEPS.len() as f64
}

/// The scale's fundamental at time `t`.
pub fn scale_freq_at(t: f64) -> f64 {
    midi_to_hz(SCALE_STEPS[((t / SCALE_NOTE_S) as usize).min(SCALE_STEPS.len() - 1)])
}

/// The scale as a 12-harmonic tone at peak 0.5.
pub fn scale(sample_rate: f32) -> Vec<f32> {
    harmonic_tone(sample_rate, scale_seconds(), scale_freq_at, 12, 0.5)
}

/// How far `t` is from the nearest scale step (including the start and end).
pub fn from_scale_step(t: f64) -> f64 {
    (t - (t / SCALE_NOTE_S).round() * SCALE_NOTE_S).abs()
}

/// Unity gain, dry only: the output is the input delayed by the latency.
pub const DRY: Settings = Settings {
    gain: 1.0,
    mix: 0.0,
    shift: 0,
};

/// Runs a `Chain` over a mono signal at unity gain and returns one estimate
/// per hop, timed at the analysis window's centre.
pub fn track(signal: &[f32], sample_rate: f32) -> Vec<EstFrame> {
    let mut chain = Chain::new(sample_rate);
    signal
        .iter()
        .filter_map(|&s| chain.process_frame(&mut [s], DRY))
        .map(|d| EstFrame {
            time_s: d.window_centre as f64 / f64::from(sample_rate),
            f0_hz: d.f0_hz,
            voiced: d.voiced,
        })
        .collect()
}

/// Runs a `Chain` over a mono signal with `settings(i)` for sample `i`.
/// Returns the output (the same length as the input, not latency
/// compensated) and every detection.
pub fn process(
    signal: &[f32],
    sample_rate: f32,
    settings: impl Fn(usize) -> Settings,
) -> (Vec<f32>, Vec<Detection>) {
    let mut chain = Chain::new(sample_rate);
    let mut detections = Vec::new();
    let output = signal
        .iter()
        .enumerate()
        .map(|(i, &s)| {
            let mut frame = [s];
            detections.extend(chain.process_frame(&mut frame, settings(i)));
            frame[0]
        })
        .collect();
    (output, detections)
}

/// Pitch estimates of a chain's output, with each time moved back by the
/// chain's latency so it lines up with the input.
pub fn output_estimates(output: &[f32], sample_rate: f32, latency: usize) -> Vec<EstFrame> {
    let shift_s = latency as f64 / f64::from(sample_rate);
    track(output, sample_rate)
        .into_iter()
        .map(|e| EstFrame {
            time_s: e.time_s - shift_s,
            ..e
        })
        .collect()
}

/// Prints one line of shifted-pitch stats in a fixed format, e.g.
/// `A3 +4 44100 Hz  median +3.2¢  within 0.987  (voiced 380)`.
pub fn print_cents(name: &str, stats: &CentsStats) {
    let median = stats
        .median_cents
        .map_or_else(|| String::from("n/a"), |m| format!("{m:+.1}¢"));
    let within = stats
        .share_within
        .map_or_else(|| String::from("  n/a"), |s| format!("{s:.3}"));
    println!(
        "{name:<28} median {median}  within {within}  (voiced {})",
        stats.voiced_frames
    );
}

/// Root mean square.
pub fn rms(signal: &[f32]) -> f64 {
    if signal.is_empty() {
        return 0.0;
    }
    let power: f64 = signal.iter().map(|&x| f64::from(x).powi(2)).sum();
    (power / signal.len() as f64).sqrt()
}

/// The largest |x[i] − x[i−1]|.
pub fn max_jump(signal: &[f32]) -> f32 {
    signal
        .windows(2)
        .map(|w| (w[1] - w[0]).abs())
        .fold(0.0, f32::max)
}

/// Prints one line of scores in a fixed format, e.g.
/// `vocadito_35_low  RPA 0.934  RCA 0.941  VR 0.902  VFA 0.121  (voiced 1377, unvoiced 211)`.
pub fn print_scores(name: &str, scores: &Scores) {
    let f = |v: Option<f64>| v.map_or_else(|| String::from("  n/a"), |v| format!("{v:.3}"));
    println!(
        "{name:<28} RPA {}  RCA {}  VR {}  VFA {}  (voiced {}, unvoiced {})",
        f(scores.raw_pitch_accuracy),
        f(scores.raw_chroma_accuracy),
        f(scores.voicing_recall),
        f(scores.voicing_false_alarm),
        scores.voiced_frames,
        scores.unvoiced_frames,
    );
}

/// Reads a 16/24-bit int or 32-bit float WAV as mono (channels averaged),
/// with the same int → float conversion as the harness. Returns the samples
/// and the sample rate.
pub fn read_wav_mono(path: &Path) -> (Vec<f32>, u32) {
    let mut reader =
        hound::WavReader::open(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let spec = reader.spec();
    let interleaved: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Int => {
            let scale = 1.0 / (1u32 << (spec.bits_per_sample - 1)) as f32;
            reader
                .samples::<i32>()
                .map(|s| s.expect("a readable sample") as f32 * scale)
                .collect()
        }
        hound::SampleFormat::Float => reader
            .samples::<f32>()
            .map(|s| s.expect("a readable sample"))
            .collect(),
    };
    let channels = usize::from(spec.channels);
    let mono = interleaved
        .chunks_exact(channels)
        .map(|frame| frame.iter().sum::<f32>() / channels as f32)
        .collect();
    (mono, spec.sample_rate)
}
