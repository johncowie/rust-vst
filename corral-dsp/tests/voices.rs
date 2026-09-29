//! The multi-voice tests: an empty grid, two voices summed, a matched voice
//! gliding, surplus voices fading, edits of the sounding row and onset
//! after silence, at 44.1 and 48 kHz. Every test prints its numbers.

mod common;

use common::{
    Trace, assert_no_clicks_voices, harmonic_tone, midi_to_hz, note_sequence, output_estimates,
    print_cents, process_traced, scale, single_voice,
};
use corral_dsp::chain::Chain;
use corral_dsp::eval::{RefFrame, cents_stats};
use corral_dsp::grid::{Grid, Row};
use corral_dsp::shift::{self, DEFAULT_GLIDE_S};
use corral_dsp::voices::VoiceState;

// Thresholds from work item 0007's acceptance criteria. Any change must carry
// a comment with the measured score and the reason.
const SAMPLE_RATES: [f32; 2] = [44_100.0, 48_000.0];
const TOLERANCE_CENTS: f64 = 25.0;
/// As shifter.rs: the detector fills, then the gate's hysteresis and fade.
const EDGE_S: f64 = 0.15;
/// Leading silence before a note.
const SILENCE_S: f64 = 0.3;
/// The voice pool's fade time (`voices::FADE_S`).
const FADE_S: f64 = 0.020;
const REF_HOP_S: f64 = 0.01;

const C: usize = 0;
const D: usize = 2;
const A: usize = 9;

fn row(intervals: &[i8]) -> Row {
    intervals
        .iter()
        .fold(Row::EMPTY, |row, &i| row.with(i, true))
}

/// A grid with the given rows set and the rest empty.
fn grid(rows: &[(usize, &[i8])]) -> Grid {
    let mut grid = Grid::EMPTY;
    for &(pitch_class, intervals) in rows {
        grid.set_row(pitch_class, row(intervals));
    }
    grid
}

fn latency(sample_rate: f32) -> usize {
    Chain::new(sample_rate).latency()
}

fn hop_s(sample_rate: f32) -> f64 {
    Chain::new(sample_rate).hop_len() as f64 / f64::from(sample_rate)
}

fn samples(seconds: f64, sample_rate: f32) -> usize {
    (seconds * f64::from(sample_rate)).round() as usize
}

fn fade_samples(sample_rate: f32) -> usize {
    samples(FADE_S, sample_rate)
}

/// The live voice at `interval` on sample `i`, if any. At most one exists.
fn voice_at(trace: &Trace, i: usize, interval: i8) -> Option<VoiceState> {
    let mut found = trace.voices[i].iter().filter(|v| v.interval == interval);
    let voice = found.next().copied();
    assert!(
        found.next().is_none(),
        "sample {i}: two voices at {interval:+}"
    );
    voice
}

/// The first hop at or after `from` whose detection is voiced.
fn first_voiced_hop(trace: &Trace, from: usize) -> usize {
    trace
        .hops
        .iter()
        .find(|h| h.sample >= from && h.detection.voiced)
        .expect("a voiced hop")
        .sample
}

/// The first hop at or after `from` whose stable note is `note`.
fn note_chosen(trace: &Trace, from: usize, note: i32) -> usize {
    trace
        .hops
        .iter()
        .find(|h| h.sample >= from && h.note == Some(note))
        .unwrap_or_else(|| panic!("note {note} chosen"))
        .sample
}

#[test]
fn an_empty_grid_is_the_dry_signal_scaled_by_one_minus_mix() {
    let mix = 0.5;
    for sample_rate in SAMPLE_RATES {
        let signal = scale(sample_rate);
        let settings = common::voices(Grid::EMPTY, DEFAULT_GLIDE_S, mix);
        let trace = process_traced(&signal, sample_rate, |_| settings);
        assert!(
            trace.voices.iter().all(Vec::is_empty),
            "{sample_rate} Hz: a voice was live"
        );

        // The detector calls a hop unvoiced at some scale steps. Two in a
        // row close the gate, which fades toward the dry signal and back:
        // 10 ms out, two voiced hops, 10 ms in. Skip 30 ms from each such
        // pair; a lone unvoiced hop changes nothing, so it isn't skipped.
        let first = first_voiced_hop(&trace, 0);
        let skip = samples(0.030, sample_rate);
        let lone = trace
            .hops
            .iter()
            .filter(|h| h.sample > first && !h.detection.voiced)
            .count();
        let closes: Vec<usize> = trace
            .hops
            .windows(2)
            .filter(|w| w[0].sample > first && !w[0].detection.voiced && !w[1].detection.voiced)
            .map(|w| w[1].sample)
            .collect();
        let skipped = |i: usize| closes.iter().any(|&s| (s..s + skip).contains(&i));

        let latency = latency(sample_rate);
        let edge = samples(EDGE_S, sample_rate);
        let (mut largest, mut skipped_samples) = (0.0f32, 0);
        for i in latency + edge..signal.len() - edge {
            if skipped(i) {
                skipped_samples += 1;
                continue;
            }
            let expected = (1.0 - mix) * signal[i - latency];
            largest = largest.max((trace.output[i] - expected).abs());
        }
        println!(
            "empty grid, mix 50% {sample_rate} Hz: largest deviation from 0.5 × dry {largest:.2e} ({lone} unvoiced hops, {} gate closes, {skipped_samples} samples skipped)",
            closes.len()
        );
        assert!(largest <= 1e-6, "{sample_rate} Hz: {largest}");
    }
}

#[test]
fn two_voices_are_g3_and_e4_and_sum() {
    let c4 = midi_to_hz(60.0);
    let seconds = 2.0;
    let reference: Vec<RefFrame> = (0..)
        .map(|i| EDGE_S + i as f64 * REF_HOP_S)
        .take_while(|&t| t <= seconds - EDGE_S)
        .map(|time_s| RefFrame {
            time_s,
            f0_hz: c4 as f32,
        })
        .collect();
    let mut failed = Vec::new();
    for sample_rate in SAMPLE_RATES {
        let signal = harmonic_tone(sample_rate, seconds, |_| c4, 12, 0.5);
        let run = |intervals: &[i8]| {
            let settings = common::voices(grid(&[(C, intervals)]), DEFAULT_GLIDE_S, 1.0);
            process_traced(&signal, sample_rate, |_| settings).output
        };
        let low = run(&[-5]);
        let high = run(&[4]);
        let both = run(&[-5, 4]);
        let empty = run(&[]);

        for (name, interval, output) in [("G3", -5i8, &low), ("E4", 4, &high)] {
            let estimates = output_estimates(output, sample_rate, latency(sample_rate));
            let stats = cents_stats(
                &reference,
                &estimates,
                hop_s(sample_rate),
                f64::from(shift::ratio(f32::from(interval))),
                TOLERANCE_CENTS,
            );
            let label = format!("C4 {interval:+} alone = {name} {sample_rate} Hz");
            print_cents(&label, &stats);
            let median = stats.median_cents.expect("estimates");
            if median.abs() > TOLERANCE_CENTS {
                failed.push(format!("{label}: median {median:+.1}¢"));
            }
        }

        // wet = dry_gain · dry + shifted_gain · Σ gain · voice, so the
        // empty run cancels the dry term the other three each carry.
        let largest = (0..signal.len())
            .map(|i| (both[i] - low[i] - high[i] + empty[i]).abs())
            .fold(0.0f32, f32::max);
        println!(
            "C4 {{−5, +4}} {sample_rate} Hz: largest |both − (−5) − (+4) + empty| {largest:.2e}"
        );
        if largest > 1e-5 {
            failed.push(format!("{sample_rate} Hz: the sum differs by {largest}"));
        }
    }
    assert!(failed.is_empty(), "\n{}", failed.join("\n"));
}

#[test]
fn a_matched_voice_glides_rather_than_fading() {
    let glide_s = DEFAULT_GLIDE_S;
    let settings = common::voices(grid(&[(C, &[4]), (D, &[3])]), glide_s, 1.0);
    let d4 = midi_to_hz(62.0) as f32;
    for sample_rate in SAMPLE_RATES {
        let signal = note_sequence(sample_rate, &[60.0, 62.0], 1.0);
        let trace = process_traced(&signal, sample_rate, |_| settings);
        let voice = single_voice(&trace);

        let first = first_voiced_hop(&trace, 0);
        let voice: Vec<VoiceState> = voice[first..]
            .iter()
            .map(|v| {
                let v = v.expect("a voice from the first voiced hop");
                assert_eq!(v.gain, 1.0, "{sample_rate} Hz: the voice glides, not fades");
                v
            })
            .collect();

        let step = samples(1.0, sample_rate);
        let detected = trace
            .hops
            .iter()
            .find(|h| {
                h.sample >= step
                    && h.detection.f0_hz > 0.0
                    && (1200.0 * (h.detection.f0_hz / d4).log2()).abs() <= 50.0
            })
            .expect("D4 detected")
            .sample;
        let changed = note_chosen(&trace, step, 62);
        let glide: Vec<f32> = voice[changed - 1 - first..]
            .iter()
            .map(|v| v.semitones)
            .collect();
        assert_eq!(glide[0], 4.0, "{sample_rate} Hz: before the change");
        assert_eq!(*glide.last().expect("samples"), 3.0, "{sample_rate} Hz");
        let max_step = 1.001 * (1.0 - shift::glide_coefficient(glide_s, sample_rate));
        let steepest = glide
            .windows(2)
            .map(|w| w[0] - w[1])
            .inspect(|&fall| assert!(fall >= 0.0, "{sample_rate} Hz: rises by {}", -fall))
            .fold(0.0, f32::max);
        println!(
            "C4 {{+4}} → D4 {{+3}} {sample_rate} Hz: one voice at full level, glides {:.1} ms after D4 is detected, steepest step {steepest:.6} semitones (limit {max_step:.6})",
            (changed - detected) as f64 / f64::from(sample_rate) * 1000.0
        );
        assert!(steepest <= max_step, "{sample_rate} Hz: {steepest}");
    }
}

/// Asserts the voice at `interval` fades from sample `from` (the change),
/// monotonically, over `FADE_S` ± 1 sample: in to 1.0, holding exactly
/// `interval` semitones, or out until it is gone. Returns the fade's
/// length in samples.
fn assert_fades(
    trace: &Trace,
    sample_rate: f32,
    from: usize,
    interval: i8,
    fading_in: bool,
    label: &str,
) -> usize {
    let before = voice_at(trace, from - 1, interval);
    let mut gains = Vec::new();
    for i in from.. {
        let Some(v) = voice_at(trace, i, interval) else {
            assert!(!fading_in, "{label}: the voice vanished");
            break;
        };
        assert_eq!(v.sounding, fading_in, "{label}: sample {i}");
        gains.push(v.gain);
        if fading_in {
            assert_eq!(v.semitones, f32::from(interval), "{label}: sample {i}");
            if v.gain == 1.0 {
                break;
            }
        }
    }
    if fading_in {
        assert!(before.is_none(), "{label}: already live");
        assert!(gains.windows(2).all(|w| w[1] >= w[0]), "{label}");
    } else {
        assert_eq!(before.map(|v| v.gain), Some(1.0), "{label}: before");
        assert!(gains.windows(2).all(|w| w[1] <= w[0]), "{label}");
    }
    let expected = fade_samples(sample_rate);
    assert!(
        gains.len().abs_diff(expected) <= 1,
        "{label}: {} samples vs {expected}",
        gains.len()
    );
    gains.len()
}

#[test]
fn surplus_voices_fade_in_and_out_without_clicks() {
    let grid = grid(&[(C, &[4]), (D, &[3, 7])]);
    for sample_rate in SAMPLE_RATES {
        let step = samples(1.0, sample_rate);
        for (name, notes) in [("C4 → D4", [60.0, 62.0]), ("D4 → C4", [62.0, 60.0])] {
            let signal = note_sequence(sample_rate, &notes, 1.0);
            for glide_s in [DEFAULT_GLIDE_S, 0.0] {
                let label = format!("{name} glide {:.0} ms {sample_rate} Hz", glide_s * 1000.0);
                let settings = common::voices(grid, glide_s, 1.0);
                let trace = process_traced(&signal, sample_rate, |_| settings);
                let changed = note_chosen(&trace, step, notes[1] as i32);
                let into_d = notes[1] == 62.0;

                // The matched voice glides (full level throughout) between
                // +4 and +3.
                let (from, to) = if into_d { (4, 3) } else { (3, 4) };
                let before = voice_at(&trace, changed - 1, from).expect("the matched voice");
                assert_eq!((before.semitones, before.gain), (f32::from(from), 1.0));
                for i in changed..signal.len() {
                    let v = voice_at(&trace, i, to).expect("the matched voice");
                    assert_eq!(v.gain, 1.0, "{label}: sample {i}");
                }
                assert!(voice_at(&trace, changed, from).is_none(), "{label}");

                let fade = assert_fades(&trace, sample_rate, changed, 7, into_d, &label);
                if !into_d {
                    assert!(
                        (changed + fade..signal.len()).all(|i| voice_at(&trace, i, 7).is_none()),
                        "{label}: +7 is gone"
                    );
                }
                println!(
                    "{label}: +7 fades {} over {fade} samples ({:.2} ms)",
                    if into_d { "in" } else { "out" },
                    fade as f64 / f64::from(sample_rate) * 1000.0
                );
                assert_no_clicks_voices(&label, &signal, &trace.output, &[3, 7]);
            }
        }
    }
}

#[test]
fn toggling_the_sounding_row_does_not_click() {
    // (time in seconds, the cell toggled in row A), on an A3 held for 3 s
    // that starts with row A = {+4}. +12 goes off and back on 5 ms apart:
    // the revival path.
    const TOGGLES: [(f64, i8); 12] = [
        (0.5, 7),
        (0.75, -5),
        (1.0, 4),
        (1.25, 12),
        (1.5, -5),
        (1.75, -12),
        (2.0, 7),
        (2.25, 12),
        (2.255, 12),
        (2.5, 4),
        (2.75, -12),
        (2.8, 7),
    ];
    let start = row(&[4]);
    for sample_rate in SAMPLE_RATES {
        let signal = harmonic_tone(sample_rate, 3.0, |_| midi_to_hz(57.0), 12, 0.5);
        let at: Vec<(usize, i8)> = TOGGLES
            .iter()
            .map(|&(t, interval)| (samples(t, sample_rate), interval))
            .collect();
        let row_at = |i: usize| {
            at.iter()
                .take_while(|&&(s, _)| s <= i)
                .fold(start, |row, &(_, interval)| row.toggled(interval))
        };
        let trace = process_traced(&signal, sample_rate, |i| {
            let mut grid = Grid::EMPTY;
            grid.set_row(A, row_at(i));
            common::voices(grid, DEFAULT_GLIDE_S, 1.0)
        });
        assert!(
            trace
                .hops
                .iter()
                .filter(|h| h.note.is_some())
                .all(|h| h.note == Some(57)),
            "{sample_rate} Hz: the note stays A3"
        );

        let mut fades = Vec::new();
        for (k, &(s, interval)) in at.iter().enumerate() {
            let label = format!("toggle {interval:+} at {} {sample_rate} Hz", TOGGLES[k].0);
            let on = row_at(s).contains(interval);
            let until = at.get(k + 1).map_or(signal.len(), |&(next, _)| next);
            // Every other voice keeps full level and its interval.
            for i in s..until {
                for v in trace.voices[i].iter().filter(|v| v.interval != interval) {
                    assert_eq!(v.gain, 1.0, "{label}: {v:?} at sample {i}");
                    assert_eq!(v.semitones, f32::from(v.interval), "{label}");
                }
            }
            if until - s < fade_samples(sample_rate) {
                // Cut short by the revival: checked there.
                continue;
            }
            let revived =
                k > 0 && at[k - 1].1 == interval && s - at[k - 1].0 < fade_samples(sample_rate);
            if revived {
                // It rises from where it was, never reaching 0, and takes as
                // long as it spent falling.
                let fell = s - at[k - 1].0;
                let gains: Vec<f32> = (s..until)
                    .map(|i| {
                        voice_at(&trace, i, interval)
                            .expect("revived, not freed")
                            .gain
                    })
                    .collect();
                let lowest = voice_at(&trace, s - 1, interval).expect("fading").gain;
                let rise = gains.iter().position(|&g| g == 1.0).expect("back to full") + 1;
                println!(
                    "{label}: revived from gain {lowest:.3}, back to full in {rise} samples (fell for {fell})"
                );
                assert!(lowest > 0.0 && gains[0] > lowest, "{label}");
                assert!(gains.windows(2).all(|w| w[1] >= w[0]), "{label}");
                assert!(rise.abs_diff(fell) <= 1, "{label}: {rise} vs {fell}");
            } else {
                let fade = assert_fades(&trace, sample_rate, s, interval, on, &label);
                fades.push(fade);
            }
        }
        println!(
            "toggles {sample_rate} Hz: {} fades of {}–{} samples (expected {})",
            fades.len(),
            fades.iter().min().expect("fades"),
            fades.iter().max().expect("fades"),
            fade_samples(sample_rate)
        );

        // The threshold of the loudest set of voices ever on together.
        let loudest = (0..signal.len())
            .step_by(samples(0.05, sample_rate))
            .map(row_at)
            .max_by(|a, b| ratio_sum(*a).total_cmp(&ratio_sum(*b)))
            .expect("rows");
        let intervals: Vec<i8> = loudest.intervals().collect();
        assert_no_clicks_voices(
            &format!("toggles, loudest {intervals:?} {sample_rate} Hz"),
            &signal,
            &trace.output,
            &intervals,
        );
    }
}

fn ratio_sum(row: Row) -> f32 {
    row.intervals().map(|i| shift::ratio(f32::from(i))).sum()
}

#[test]
fn voices_start_at_their_intervals_after_silence() {
    // (MIDI, pitch class, the row's intervals), each after silence.
    let notes: [(f64, usize, &[i8]); 2] = [(60.0, C, &[-5, 4]), (62.0, D, &[3])];
    let grid = grid(&[(C, notes[0].2), (D, notes[1].2)]);
    // A long glide, so a wrongly applied one would be obvious.
    let settings = common::voices(grid, shift::MAX_GLIDE_S, 1.0);
    for sample_rate in SAMPLE_RATES {
        let gap = vec![0.0; samples(SILENCE_S, sample_rate)];
        let mut signal = Vec::new();
        let mut spans = Vec::new();
        for (midi, _, _) in notes {
            signal.extend(&gap);
            let start = signal.len();
            let f0 = midi_to_hz(midi);
            signal.extend(harmonic_tone(sample_rate, 0.6, |_| f0, 12, 0.5));
            spans.push(start..signal.len());
        }
        signal.extend(&gap);

        let trace = process_traced(&signal, sample_rate, |_| settings);
        for ((midi, _, intervals), span) in notes.into_iter().zip(&spans) {
            let first = first_voiced_hop(&trace, span.start);
            let label = format!("{midi} {sample_rate} Hz");
            let hop = trace
                .hops
                .iter()
                .find(|h| h.sample == first)
                .expect("the hop");
            assert_eq!(hop.note, Some(midi as i32), "{label}");
            let wrong = (first..span.end)
                .filter(|&i| {
                    let mut voices = trace.voices[i].clone();
                    voices.sort_by_key(|v| v.interval);
                    voices.len() != intervals.len()
                        || voices.iter().zip(intervals).any(|(v, &interval)| {
                            v.interval != interval
                                || v.semitones != f32::from(interval)
                                || v.gain != 1.0
                                || !v.sounding
                        })
                })
                .count();
            println!(
                "onset {label}: first voiced hop {:.1} ms in, voices {intervals:?} exactly at full level ({wrong} samples differ)",
                (first - span.start) as f64 / f64::from(sample_rate) * 1000.0
            );
            assert_eq!(wrong, 0, "{label}");
        }
    }
}
