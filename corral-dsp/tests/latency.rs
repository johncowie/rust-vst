//! Measures the delay from a note onset to the first stable, correct
//! detection, for the README. There is no pass/fail limit on the latency.

mod common;

use common::{harmonic_tone, midi_to_hz};
use corral_dsp::chain::Chain;

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
                .filter_map(|&s| chain.process_frame(&mut [s], 1.0))
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
