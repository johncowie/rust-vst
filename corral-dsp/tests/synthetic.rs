//! Accuracy on synthesised signals with exact ground truth, at 44.1 and 48 kHz.

mod common;

use common::{harmonic_tone, midi_to_hz, print_scores, track, white_noise};
use corral_dsp::chain::Chain;
use corral_dsp::eval::{self, RefFrame, Scores};

const SAMPLE_RATES: [f32; 2] = [44_100.0, 48_000.0];
/// Reference frames are every 10 ms, from 0.1 s until 0.1 s before the end.
const REF_HOP_S: f64 = 0.01;
const REF_MARGIN_S: f64 = 0.1;

const MIN_RAW_PITCH_ACCURACY: f64 = 0.98;
const MAX_OCTAVE_GAP: f64 = 0.01;
/// Our addition to the work item's criteria: catches a gate so strict it
/// would blank the display.
const MIN_VOICING_RECALL: f64 = 0.95;
const MIN_UNVOICED_SHARE: f64 = 0.99;

/// Reference frames for a signal of `seconds`, with `f0_at(t)` giving the
/// pitch (or `None` to leave the frame out).
fn reference(seconds: f64, f0_at: impl Fn(f64) -> Option<f64>) -> Vec<RefFrame> {
    (0..)
        .map(|i| REF_MARGIN_S + i as f64 * REF_HOP_S)
        .take_while(|&t| t <= seconds - REF_MARGIN_S)
        .filter_map(|time_s| {
            f0_at(time_s).map(|f0| RefFrame {
                time_s,
                f0_hz: f0 as f32,
            })
        })
        .collect()
}

fn score(signal: &[f32], sample_rate: f32, reference: &[RefFrame]) -> Scores {
    let estimates = track(signal, sample_rate);
    let hop_s = Chain::new(sample_rate).hop_len() as f64 / f64::from(sample_rate);
    eval::score(reference, &estimates, hop_s)
}

fn assert_tracks(name: &str, scores: &Scores) {
    print_scores(name, scores);
    let rpa = scores.raw_pitch_accuracy.expect("voiced reference frames");
    let rca = scores.raw_chroma_accuracy.expect("voiced reference frames");
    let recall = scores.voicing_recall.expect("voiced reference frames");
    assert!(rpa >= MIN_RAW_PITCH_ACCURACY, "{name}: RPA {rpa:.3}");
    assert!(
        rca - rpa <= MAX_OCTAVE_GAP,
        "{name}: RCA − RPA {:.3}",
        rca - rpa
    );
    assert!(recall >= MIN_VOICING_RECALL, "{name}: VR {recall:.3}");
}

/// A held tone at `freq_at(t)` for `seconds`, scored against the same function.
fn assert_tone(name: &str, seconds: f64, harmonics: usize, freq_at: impl Fn(f64) -> f64 + Copy) {
    for sample_rate in SAMPLE_RATES {
        let signal = harmonic_tone(sample_rate, seconds, freq_at, harmonics, 0.5);
        let reference = reference(seconds, |t| Some(freq_at(t)));
        let scores = score(&signal, sample_rate, &reference);
        assert_tracks(&format!("{name} {sample_rate} Hz"), &scores);
    }
}

#[test]
fn c_major_scale() {
    // C3 → C5, 15 notes.
    const STEPS: [f64; 15] = [
        48.0, 50.0, 52.0, 53.0, 55.0, 57.0, 59.0, 60.0, 62.0, 64.0, 65.0, 67.0, 69.0, 71.0, 72.0,
    ];
    const NOTE_S: f64 = 0.4;
    /// Frames this close to a step hear both notes, so they're left out.
    const STEP_EXCLUSION_S: f64 = 0.03;
    let seconds = NOTE_S * STEPS.len() as f64;
    let freq_at = |t: f64| midi_to_hz(STEPS[((t / NOTE_S) as usize).min(STEPS.len() - 1)]);

    for sample_rate in SAMPLE_RATES {
        let signal = harmonic_tone(sample_rate, seconds, freq_at, 12, 0.5);
        let reference = reference(seconds, |t| {
            let from_step = t - (t / NOTE_S).round() * NOTE_S;
            (from_step.abs() > STEP_EXCLUSION_S).then(|| freq_at(t))
        });
        let scores = score(&signal, sample_rate, &reference);
        assert_tracks(&format!("scale {sample_rate} Hz"), &scores);
    }
}

#[test]
fn vibrato() {
    // A3 with 5.5 Hz ±50-cent vibrato.
    assert_tone("vibrato A3", 3.0, 8, |t| {
        220.0 * 2f64.powf(50.0 / 1200.0 * (std::f64::consts::TAU * 5.5 * t).sin())
    });
}

#[test]
fn low_edge_e2() {
    assert_tone("E2", 2.0, 8, |_| midi_to_hz(40.0));
}

#[test]
fn high_edge_b5() {
    assert_tone("B5", 2.0, 8, |_| midi_to_hz(83.0));
}

/// Asserts that at least 99% of hops are unvoiced.
fn assert_unvoiced(name: &str, signal: &[f32], sample_rate: f32) {
    let estimates = track(signal, sample_rate);
    let seconds = signal.len() as f64 / f64::from(sample_rate);
    let silent = reference(seconds, |_| Some(0.0));
    print_scores(name, &eval::score(&silent, &estimates, 1.0));
    let unvoiced = estimates.iter().filter(|e| !e.voiced).count();
    let share = unvoiced as f64 / estimates.len() as f64;
    assert!(
        share >= MIN_UNVOICED_SHARE,
        "{name}: only {unvoiced} of {} hops unvoiced",
        estimates.len()
    );
}

#[test]
fn silence_is_unvoiced() {
    for sample_rate in SAMPLE_RATES {
        let signal = vec![0.0; (3.0 * sample_rate) as usize];
        assert_unvoiced(&format!("silence {sample_rate} Hz"), &signal, sample_rate);
    }
}

#[test]
fn white_noise_is_unvoiced() {
    for sample_rate in SAMPLE_RATES {
        // Peak 0.25 is well above the level gate, so only confidence can reject it.
        let signal = white_noise((3.0 * sample_rate) as usize, 0.25, 0x5EED);
        assert_unvoiced(
            &format!("white noise {sample_rate} Hz"),
            &signal,
            sample_rate,
        );
    }
}
