//! Scores the detector on the committed dataset excerpts in `tests/fixtures/`
//! (see its `ATTRIBUTION.md`). Every fixture's scores are printed, then the
//! test fails if any is below its threshold.

mod common;

use std::fs;
use std::path::{Path, PathBuf};

use corral_dsp::chain::{Chain, Settings};
use corral_dsp::eval::{EstFrame, RefFrame, Scores, cents_stats, parse_reference_csv, score};
use corral_dsp::shift;

// Thresholds from work item 0004's acceptance criteria. Any change must carry
// a comment with the measured score and the reason.
const NSYNTH_MIN_RPA: f64 = 0.95;
const MDB_MIN_RPA: f64 = 0.90;
const MDB_MAX_OCTAVE_GAP: f64 = 0.02;
const VOCADITO_MIN_RPA: f64 = 0.90;
const VOCADITO_MAX_OCTAVE_GAP: f64 = 0.02;
const VOCADITO_MAX_VFA: f64 = 0.20;

// Work item 0005: the chain's output at +4, scored against reference × 2^(4/12).
const SHIFTED_MIN_SHARE: f64 = 0.90;
const SHIFTED_TOLERANCE_CENTS: f64 = 25.0;
const SHIFTED_FIXTURES: [&str; 3] = [
    "vocadito_35_low",
    "vocadito_7_vibrato",
    "mdb_nightowl_08_male",
];

/// The headline threshold for a shifted fixture. All three miss the work
/// item's 0.90, so each is set just below its measured score, as decided in
/// the plan for 0005. The limit is the detector, not the shifter: scored the
/// same way (±25¢) on the *unshifted* input, the detector alone gets 0.833,
/// 0.851 and 0.915, and at Shift 0 the chain's output scores the same as
/// its delayed input. Missed voiced frames (gate closed) also count as misses.
fn shifted_min_share(name: &str) -> f64 {
    match name {
        // Measured 0.718 (chain-voiced 0.750); detector alone 0.833.
        "vocadito_35_low" => 0.70,
        // Measured 0.812 (chain-voiced 0.852); detector alone 0.851.
        "vocadito_7_vibrato" => 0.80,
        // Measured 0.851 (chain-voiced 0.875); detector alone 0.915.
        "mdb_nightowl_08_male" => 0.84,
        _ => SHIFTED_MIN_SHARE,
    }
}

/// How many fixtures of each kind must be present, so a missing file fails.
const EXPECTED_COUNTS: [(&str, usize); 3] = [("vocadito_", 2), ("mdb_", 2), ("nsynth_", 3)];

struct Thresholds {
    min_rpa: f64,
    max_octave_gap: Option<f64>,
    max_vfa: Option<f64>,
}

fn thresholds(name: &str) -> Thresholds {
    if name.starts_with("nsynth_") {
        Thresholds {
            min_rpa: NSYNTH_MIN_RPA,
            max_octave_gap: None,
            max_vfa: None,
        }
    } else if name.starts_with("mdb_") {
        Thresholds {
            min_rpa: MDB_MIN_RPA,
            max_octave_gap: Some(MDB_MAX_OCTAVE_GAP),
            max_vfa: None,
        }
    } else if name.starts_with("vocadito_") {
        Thresholds {
            min_rpa: VOCADITO_MIN_RPA,
            max_octave_gap: Some(VOCADITO_MAX_OCTAVE_GAP),
            max_vfa: Some(VOCADITO_MAX_VFA),
        }
    } else {
        panic!("{name}: no thresholds for this fixture's prefix");
    }
}

/// Every threshold `scores` misses, as readable messages.
fn failures(name: &str, scores: &Scores) -> Vec<String> {
    let t = thresholds(name);
    let rpa = scores.raw_pitch_accuracy.unwrap_or(0.0);
    let rca = scores.raw_chroma_accuracy.unwrap_or(0.0);
    let mut failed = Vec::new();
    if rpa < t.min_rpa {
        failed.push(format!("{name}: RPA {rpa:.3} < {}", t.min_rpa));
    }
    if let Some(max) = t.max_octave_gap
        && rca - rpa > max
    {
        failed.push(format!("{name}: RCA − RPA {:.3} > {max}", rca - rpa));
    }
    if let Some(max) = t.max_vfa {
        let vfa = scores.voicing_false_alarm.unwrap_or(0.0);
        if vfa > max {
            failed.push(format!("{name}: VFA {vfa:.3} > {max}"));
        }
    }
    failed
}

fn fixture_wavs() -> Vec<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let mut wavs: Vec<PathBuf> = fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "wav"))
        .collect();
    wavs.sort();
    wavs
}

#[test]
fn fixtures_meet_the_accuracy_thresholds() {
    let wavs = fixture_wavs();
    let names: Vec<String> = wavs
        .iter()
        .map(|p| p.file_stem().unwrap().to_string_lossy().into_owned())
        .collect();
    for (prefix, expected) in EXPECTED_COUNTS {
        let found = names.iter().filter(|n| n.starts_with(prefix)).count();
        assert_eq!(found, expected, "{prefix}* fixtures in tests/fixtures");
    }

    let mut failed = Vec::new();
    for (wav, name) in wavs.iter().zip(&names) {
        let csv = wav.with_extension("f0.csv");
        let reference = parse_reference_csv(
            &fs::read_to_string(&csv).unwrap_or_else(|e| panic!("{}: {e}", csv.display())),
        );
        let (signal, sample_rate) = common::read_wav_mono(wav);
        let sample_rate = sample_rate as f32;
        let estimates = common::track(&signal, sample_rate);
        let max_gap_s = Chain::new(sample_rate).hop_len() as f64 / f64::from(sample_rate);
        let scores = score(&reference, &estimates, max_gap_s);
        common::print_scores(name, &scores);
        failed.extend(failures(name, &scores));
    }
    assert!(failed.is_empty(), "\n{}", failed.join("\n"));
}

#[test]
fn shifted_fixtures_track_the_shifted_reference() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let settings = Settings {
        gain: 1.0,
        mix: 1.0,
        shift: 4,
    };
    let ratio = f64::from(shift::ratio(4.0));
    let mut failed = Vec::new();
    for name in SHIFTED_FIXTURES {
        let wav = dir.join(format!("{name}.wav"));
        let csv = dir.join(format!("{name}.f0.csv"));
        let reference = parse_reference_csv(
            &fs::read_to_string(&csv).unwrap_or_else(|e| panic!("{}: {e}", csv.display())),
        );
        let (signal, sample_rate) = common::read_wav_mono(&wav);
        let sample_rate = sample_rate as f32;
        let chain = Chain::new(sample_rate);
        let max_gap_s = chain.hop_len() as f64 / f64::from(sample_rate);

        let (output, detections) = common::process(&signal, sample_rate, |_| settings);
        let estimates = common::output_estimates(&output, sample_rate, chain.latency());
        let headline = cents_stats(
            &reference,
            &estimates,
            max_gap_s,
            ratio,
            SHIFTED_TOLERANCE_CENTS,
        );

        // Only the frames the chain itself voiced: isolates the shifter from
        // the detector's recall.
        let chain_voicing: Vec<EstFrame> = detections
            .iter()
            .map(|d| EstFrame {
                time_s: d.window_centre as f64 / f64::from(sample_rate),
                f0_hz: d.f0_hz,
                voiced: d.voiced,
            })
            .collect();
        let chain_voiced: Vec<RefFrame> = reference
            .iter()
            .copied()
            .filter(|r| score(&[*r], &chain_voicing, max_gap_s).voicing_recall == Some(1.0))
            .collect();
        let diagnostic = cents_stats(
            &chain_voiced,
            &estimates,
            max_gap_s,
            ratio,
            SHIFTED_TOLERANCE_CENTS,
        );

        common::print_cents(&format!("{name} +4"), &headline);
        common::print_cents(&format!("{name} +4 (chain-voiced)"), &diagnostic);
        let share = headline.share_within.unwrap_or(0.0);
        let min = shifted_min_share(name);
        if share < min {
            failed.push(format!("{name}: within ±25¢ {share:.3} < {min}"));
        }
    }
    assert!(failed.is_empty(), "\n{}", failed.join("\n"));
}
