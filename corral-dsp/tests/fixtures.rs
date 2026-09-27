//! Scores the detector on the committed dataset excerpts in `tests/fixtures/`
//! (see its `ATTRIBUTION.md`). Every fixture's scores are printed, then the
//! test fails if any is below its threshold.

mod common;

use std::fs;
use std::path::{Path, PathBuf};

use corral_dsp::chain::Chain;
use corral_dsp::eval::{Scores, parse_reference_csv, score};

// Thresholds from work item 0004's acceptance criteria. Any change must carry
// a comment with the measured score and the reason.
const NSYNTH_MIN_RPA: f64 = 0.95;
const MDB_MIN_RPA: f64 = 0.90;
const MDB_MAX_OCTAVE_GAP: f64 = 0.02;
const VOCADITO_MIN_RPA: f64 = 0.90;
const VOCADITO_MAX_OCTAVE_GAP: f64 = 0.02;
const VOCADITO_MAX_VFA: f64 = 0.20;

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
