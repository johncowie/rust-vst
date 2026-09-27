//! Signal generators and helpers shared by the integration tests.

// Each test binary compiles this module and uses a different subset of it.
#![allow(dead_code)]

use std::f64::consts::TAU;
use std::path::Path;

use corral_dsp::chain::Chain;
use corral_dsp::eval::{EstFrame, Scores};

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

/// Runs a `Chain` over a mono signal at unity gain and returns one estimate
/// per hop, timed at the analysis window's centre.
pub fn track(signal: &[f32], sample_rate: f32) -> Vec<EstFrame> {
    let mut chain = Chain::new(sample_rate);
    signal
        .iter()
        .filter_map(|&s| chain.process_frame(&mut [s], 1.0))
        .map(|d| EstFrame {
            time_s: d.window_centre as f64 / f64::from(sample_rate),
            f0_hz: d.f0_hz,
            voiced: d.voiced,
        })
        .collect()
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
