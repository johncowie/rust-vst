//! The shifter's objective tests: output pitch, identity, level and clicks,
//! at 44.1 and 48 kHz. Every test prints its numbers.

mod common;

use common::{
    assert_no_clicks, from_scale_step, harmonic_tone, midi_to_hz, output_estimates, print_cents,
    process, rms, scale, scale_freq_at, scale_seconds, white_noise,
};
use corral_dsp::chain::{Chain, Settings};
use corral_dsp::eval::{CentsStats, RefFrame, cents_stats};
use corral_dsp::grid::INTERVALS;
use corral_dsp::shift;

// Thresholds from work item 0005's acceptance criteria. Any change must carry
// a comment with the measured score and the reason.
const MEDIAN_CENTS: f64 = 10.0; // +4, tones and scale
const SHARE_25_CENTS: f64 = 0.95; // +4, tones and scale
const OTHER_SHIFTS_MEDIAN_CENTS: f64 = 15.0;
const LEVEL_DB: f64 = 1.5;
/// Reference frames within this of the signal's start or end are left out:
/// the detector needs 46 ms to fill, then 2 hops of hysteresis and a 10 ms
/// fade (the work item's 50 ms can't cover that; recorded in the plan).
const EDGE_S: f64 = 0.15;
/// …and within this of a scale step.
const STEP_EXCLUSION_S: f64 = 0.05;

const SAMPLE_RATES: [f32; 2] = [44_100.0, 48_000.0];
const TOLERANCE_CENTS: f64 = 25.0;
const REF_HOP_S: f64 = 0.01;

fn wet(shift: i8) -> Settings {
    common::fixed(shift, 1.0)
}

fn latency(sample_rate: f32) -> usize {
    Chain::new(sample_rate).latency()
}

fn hop_s(sample_rate: f32) -> f64 {
    Chain::new(sample_rate).hop_len() as f64 / f64::from(sample_rate)
}

/// Reference frames every 10 ms, leaving out `EDGE_S` at each end, with
/// `f0_at(t)` giving the pitch (or `None` to leave the frame out).
fn reference(seconds: f64, f0_at: impl Fn(f64) -> Option<f64>) -> Vec<RefFrame> {
    (0..)
        .map(|i| EDGE_S + i as f64 * REF_HOP_S)
        .take_while(|&t| t <= seconds - EDGE_S)
        .filter_map(|time_s| {
            f0_at(time_s).map(|f0| RefFrame {
                time_s,
                f0_hz: f0 as f32,
            })
        })
        .collect()
}

/// Runs the chain wet at `shift` and scores the output's pitch against the
/// reference × the shift's ratio.
fn shifted_stats(
    signal: &[f32],
    sample_rate: f32,
    shift: i8,
    reference: &[RefFrame],
) -> CentsStats {
    let (output, _) = process(signal, sample_rate, |_| wet(shift));
    let estimates = output_estimates(&output, sample_rate, latency(sample_rate));
    cents_stats(
        reference,
        &estimates,
        hop_s(sample_rate),
        f64::from(shift::ratio(f32::from(shift))),
        TOLERANCE_CENTS,
    )
}

fn assert_plus_four(name: &str, stats: &CentsStats) {
    print_cents(name, stats);
    let median = stats.median_cents.expect("estimates");
    let share = stats.share_within.expect("voiced reference frames");
    assert!(median.abs() <= MEDIAN_CENTS, "{name}: median {median:+.1}¢");
    assert!(share >= SHARE_25_CENTS, "{name}: within ±25¢ {share:.3}");
}

#[test]
fn plus_four_tracks_on_tones() {
    for sample_rate in SAMPLE_RATES {
        for (name, midi) in [("A3", 57.0), ("E2", 40.0), ("C4", 60.0)] {
            let freq = midi_to_hz(midi);
            let signal = harmonic_tone(sample_rate, 2.0, |_| freq, 8, 0.5);
            let reference = reference(2.0, |_| Some(freq));
            let stats = shifted_stats(&signal, sample_rate, 4, &reference);
            assert_plus_four(&format!("{name} +4 {sample_rate} Hz"), &stats);
        }
    }
}

#[test]
fn plus_four_tracks_on_a_scale() {
    for sample_rate in SAMPLE_RATES {
        let reference = reference(scale_seconds(), |t| {
            (from_scale_step(t) > STEP_EXCLUSION_S).then(|| scale_freq_at(t))
        });
        let stats = shifted_stats(&scale(sample_rate), sample_rate, 4, &reference);
        assert_plus_four(&format!("scale +4 {sample_rate} Hz"), &stats);
    }
}

#[test]
fn other_shifts_track() {
    let cases = [
        ("A3", 57.0, -12),
        ("C4", 60.0, -12),
        ("A3", 57.0, -5),
        ("A3", 57.0, 7),
        ("A3", 57.0, 12),
    ];
    let mut failed = Vec::new();
    for sample_rate in SAMPLE_RATES {
        for (note, midi, shift) in cases {
            let freq = midi_to_hz(midi);
            let signal = harmonic_tone(sample_rate, 2.0, |_| freq, 8, 0.5);
            let reference = reference(2.0, |_| Some(freq));
            let stats = shifted_stats(&signal, sample_rate, shift, &reference);
            let name = format!("{note} {shift:+} {sample_rate} Hz");
            print_cents(&name, &stats);
            let median = stats.median_cents.unwrap_or(f64::INFINITY);
            if median.abs() > OTHER_SHIFTS_MEDIAN_CENTS {
                failed.push(format!("{name}: median {median:+.1}¢"));
            }
        }
    }
    assert!(failed.is_empty(), "\n{}", failed.join("\n"));
}

/// Asserts the output is `signal` delayed by the latency, bit for bit.
fn assert_delayed(name: &str, signal: &[f32], sample_rate: f32, settings: Settings) {
    let latency = latency(sample_rate);
    let (output, _) = process(signal, sample_rate, |_| settings);
    for (i, &y) in output.iter().enumerate() {
        let expected = if i < latency {
            0.0
        } else {
            signal[i - latency]
        };
        assert_eq!(y.to_bits(), expected.to_bits(), "{name}, sample {i}");
    }
}

#[test]
fn silence_noise_and_mix_zero_are_the_delayed_input() {
    for sample_rate in SAMPLE_RATES {
        let len = 2 * sample_rate as usize;
        let at = |name: &str| format!("{name} {sample_rate} Hz");
        // The gate never opens, so even fully wet the output is dry.
        assert_delayed(&at("silence, mix 1"), &vec![0.0; len], sample_rate, wet(4));
        let noise = white_noise(len, 0.25, 0x5EED);
        assert_delayed(&at("noise, mix 1"), &noise, sample_rate, wet(4));

        let dry = common::fixed(4, 0.0);
        let a3 = harmonic_tone(sample_rate, 2.0, |_| midi_to_hz(57.0), 8, 0.5);
        assert_delayed(&at("A3, mix 0"), &a3, sample_rate, dry);
        assert_delayed(&at("scale, mix 0"), &scale(sample_rate), sample_rate, dry);
    }
}

/// The output's level relative to the input's, in dB, away from the edges.
fn level_db(signal: &[f32], sample_rate: f32, shift: i8) -> f64 {
    let latency = latency(sample_rate);
    let edge = (EDGE_S * f64::from(sample_rate)) as usize;
    let (output, _) = process(signal, sample_rate, |_| wet(shift));
    let out = &output[edge + latency..output.len() - edge];
    let input = &signal[edge..signal.len() - edge - latency];
    20.0 * (rms(out) / rms(input)).log10()
}

#[test]
fn level_is_preserved() {
    let mut failed = Vec::new();
    for sample_rate in SAMPLE_RATES {
        let a3 = harmonic_tone(sample_rate, 2.0, |_| midi_to_hz(57.0), 8, 0.5);
        let scale = scale(sample_rate);
        for (name, signal) in [("A3", &a3), ("scale", &scale)] {
            for shift in [-12, -5, 4, 7, 12] {
                let db = level_db(signal, sample_rate, shift);
                println!("level {name} {shift:+} {sample_rate} Hz: {db:+.2} dB");
                // Only +4 is asserted; the rest are printed for the README.
                if shift == 4 && db.abs() > LEVEL_DB {
                    failed.push(format!("{name} +4 {sample_rate} Hz: {db:+.2} dB"));
                }
            }
        }
    }
    assert!(failed.is_empty(), "\n{}", failed.join("\n"));
}

#[test]
fn no_clicks_at_voicing_transitions() {
    for sample_rate in SAMPLE_RATES {
        let sr = f64::from(sample_rate);
        let a3 = midi_to_hz(57.0);

        // Three 0.5 s bursts with 0.3 s of silence between them.
        let burst = harmonic_tone(sample_rate, 0.5, |_| a3, 8, 0.5);
        let gap = vec![0.0; (0.3 * sr) as usize];
        let mut bursts = Vec::new();
        for _ in 0..3 {
            bursts.extend(&gap);
            bursts.extend(&burst);
        }
        bursts.extend(&gap);
        let (output, _) = process(&bursts, sample_rate, |_| wet(4));
        assert_no_clicks(&format!("bursts {sample_rate} Hz"), &bursts, &output, 4.0);

        // −6 dBFS down to −66 and back over 2 s: the level gate closes the
        // wet path and reopens it while the tone carries on.
        let tone = harmonic_tone(sample_rate, 2.0, |_| a3, 8, 1.0);
        let ramp: Vec<f32> = tone
            .iter()
            .enumerate()
            .map(|(i, &x)| {
                let t = i as f64 / sr;
                let db = -6.0 - 60.0 * (1.0 - (t - 1.0).abs());
                x * 10f64.powf(db / 20.0) as f32
            })
            .collect();
        let (output, _) = process(&ramp, sample_rate, |_| wet(4));
        assert_no_clicks(&format!("level ramp {sample_rate} Hz"), &ramp, &output, 4.0);
    }
}

#[test]
fn no_clicks_during_a_shift_sweep() {
    for sample_rate in SAMPLE_RATES {
        let sr = f64::from(sample_rate);
        let a3 = midi_to_hz(57.0);

        // −12 → +12 (skipping 0), one semitone every 200 ms. Each step
        // edits the sounding row, so it is a 20 ms crossfade, not a glide.
        let signal = harmonic_tone(sample_rate, 4.8, |_| a3, 8, 0.5);
        let step_len = (0.2 * sr) as usize;
        let (output, _) = process(&signal, sample_rate, |i| {
            wet(INTERVALS[(i / step_len).min(INTERVALS.len() - 1)])
        });
        assert_no_clicks(
            &format!("shift sweep {sample_rate} Hz"),
            &signal,
            &output,
            12.0,
        );

        // Mix 0 → 1 → 0 over 2 s at +4.
        let signal = harmonic_tone(sample_rate, 2.0, |_| a3, 8, 0.5);
        let len = signal.len() as f32;
        let (output, _) = process(&signal, sample_rate, |i| {
            common::fixed(4, 1.0 - (2.0 * i as f32 / len - 1.0).abs())
        });
        assert_no_clicks(
            &format!("mix sweep {sample_rate} Hz"),
            &signal,
            &output,
            4.0,
        );
    }
}
