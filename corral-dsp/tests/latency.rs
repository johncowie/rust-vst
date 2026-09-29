//! Measures the delay from a note onset to the first stable, correct
//! detection, for the README (no pass/fail limit on it), and checks the
//! chain's reported latency against the delay measured on its output.

mod common;

use common::{DRY, MAJOR_THIRDS, harmonic_tone, midi_to_hz, process, thirds_grid};
use corral_dsp::chain::Chain;
use corral_dsp::shift::DEFAULT_GLIDE_S;

/// A detection is "stable" when it and the next few are all correct.
const STABLE_RUN: usize = 5;
const TOLERANCE_CENTS: f32 = 50.0;
const SILENCE_S: f64 = 0.5;
const TONE_S: f64 = 1.0;

#[test]
fn onset_to_stable_detection() {
    for sample_rate in [44_100.0, 48_000.0] {
        for (name, midi) in [("A3", 57.0), ("E2", 40.0)] {
            let freq = midi_to_hz(midi);
            let onset = (SILENCE_S * f64::from(sample_rate)) as u64;
            let mut signal = vec![0.0; onset as usize];
            signal.extend(harmonic_tone(sample_rate, TONE_S, |_| freq, 8, 0.5));

            let mut chain = Chain::new(sample_rate);
            let (window_len, hop_len) = (chain.window_len(), chain.hop_len());
            let correct: Vec<(u64, bool)> = signal
                .iter()
                .filter_map(|&s| chain.process_frame(&mut [s], DRY))
                .map(|d| {
                    let hop_end = d.window_centre + (window_len / 2) as u64;
                    let cents = 1200.0 * (d.f0_hz / freq as f32).log2();
                    (hop_end, d.voiced && cents.abs() <= TOLERANCE_CENTS)
                })
                .collect();

            let first_stable = correct
                .windows(STABLE_RUN)
                .find(|run| run[0].0 > onset && run.iter().all(|&(_, ok)| ok))
                .map(|run| run[0].0)
                .unwrap_or_else(|| panic!("{sample_rate} Hz {name}: never stable"));
            let latency_s = (first_stable - onset) as f64 / f64::from(sample_rate);
            println!(
                "latency {sample_rate} Hz {name}: {:.1} ms (window {window_len}, hop {hop_len})",
                latency_s * 1000.0
            );
            assert!(latency_s <= 0.5, "{sample_rate} Hz {name}: {latency_s} s");
        }
    }
}

/// |x| through a one-pole low-pass with this time constant.
const ENVELOPE_S: f32 = 0.005;
const MAX_LAG: usize = 4096;

fn envelope(signal: &[f32], sample_rate: f32) -> Vec<f32> {
    let coeff = (-1.0 / (ENVELOPE_S * sample_rate)).exp();
    let mut state = 0.0;
    signal
        .iter()
        .map(|x| {
            state = x.abs() + coeff * (state - x.abs());
            state
        })
        .collect()
}

/// The lag in 0..=MAX_LAG at which `output`'s envelope best matches `input`'s.
fn measured_delay(input: &[f32], output: &[f32], sample_rate: f32) -> usize {
    let (input, output) = (envelope(input, sample_rate), envelope(output, sample_rate));
    let correlation = |lag: usize| -> f64 {
        input
            .iter()
            .zip(&output[lag..])
            .map(|(&a, &b)| f64::from(a) * f64::from(b))
            .sum()
    };
    (0..=MAX_LAG)
        .max_by(|&a, &b| correlation(a).total_cmp(&correlation(b)))
        .expect("lags")
}

#[test]
fn reported_latency_matches_the_measured_delay() {
    for sample_rate in [44_100.0, 48_000.0] {
        let chain = Chain::new(sample_rate);
        let (latency, hop) = (chain.latency(), chain.hop_len());
        let silence = vec![0.0; (0.3 * sample_rate) as usize];
        let mut signal = silence.clone();
        signal.extend(harmonic_tone(
            sample_rate,
            1.0,
            |_| midi_to_hz(57.0),
            8,
            0.5,
        ));
        signal.extend(&silence);

        for (name, settings) in [
            ("mix 1", common::fixed(4, 1.0)),
            ("mix 0", common::fixed(4, 0.0)),
            (
                "C major thirds grid, mix 1",
                common::voices(thirds_grid(0, &MAJOR_THIRDS), DEFAULT_GLIDE_S, 1.0),
            ),
        ] {
            let (output, _) = process(&signal, sample_rate, |_| settings);
            let lag = measured_delay(&signal, &output, sample_rate);
            println!(
                "delay {sample_rate} Hz, {name}: measured {lag}, reported {latency} samples ({:.1} ms)",
                latency as f32 / sample_rate * 1000.0
            );
            if settings.mix == 0.0 {
                assert_eq!(lag, latency, "{sample_rate} Hz, {name}");
            } else {
                assert!(
                    lag.abs_diff(latency) <= hop,
                    "{sample_rate} Hz, {name}: {lag} vs {latency}"
                );
            }
        }
    }
}
