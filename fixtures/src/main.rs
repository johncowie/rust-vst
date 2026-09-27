//! Prepares the committed pitch-detection fixtures from the datasets that
//! `mise run fixtures` downloads into `.datasets/`.
//!
//! - `fixtures scan <reference.csv> [--len <s>]` helps choose excerpts.
//! - `fixtures build` regenerates `corral-dsp/tests/fixtures/` from [`EXCERPTS`].

use std::error::Error;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use corral_dsp::eval::{RefFrame, parse_reference_csv};
use corral_dsp::pitch::{MAX_F0_HZ, MIN_F0_HZ};
use hound::{SampleFormat, WavReader, WavSpec, WavWriter};
use rubato::audioadapter_buffers::direct::InterleavedSlice;
use rubato::{Fft, FixedSync, Resampler};

const USAGE: &str = "usage: fixtures scan <reference.csv> [--len <s>] | fixtures build";

const DATASETS_DIR: &str = ".datasets";
const FIXTURES_DIR: &str = "corral-dsp/tests/fixtures";
/// The committed fixtures must stay under this many bytes in total.
const MAX_TOTAL_BYTES: u64 = 5 * 1024 * 1024;

const NSYNTH_SAMPLE_RATE: usize = 16_000;
const OUTPUT_SAMPLE_RATE: usize = 44_100;
/// NSynth notes are held for 3 s; the reference covers the sustained part
/// only, skipping the attack and the release.
const NSYNTH_SUSTAIN_S: (f64, f64) = (0.2, 2.8);
const NSYNTH_REFERENCE_HOP_S: f64 = 0.01;

#[derive(Clone, Copy, PartialEq)]
enum Dataset {
    Vocadito,
    MdbStemSynth,
    NSynth,
}

enum Source {
    /// Excerpt a dataset WAV + its f0 CSV.
    Annotated {
        wav: &'static str,
        csv: &'static str,
        start_s: f64,
        len_s: f64,
    },
    /// NSynth held note: resample to 44.1 kHz, synthesise a constant-pitch reference.
    NSynth { wav: &'static str, midi: u8 },
}

struct Excerpt {
    name: &'static str,
    dataset: Dataset,
    source: Source,
    note: &'static str,
}

/// Chosen with `fixtures scan` (every voiced reference frame in 80 Hz–1 kHz).
const EXCERPTS: &[Excerpt] = &[
    Excerpt {
        name: "vocadito_7_vibrato",
        dataset: Dataset::Vocadito,
        source: Source::Annotated {
            wav: "vocadito_7.wav",
            csv: "vocadito_7_f0.csv",
            start_s: 10.0,
            len_s: 8.0,
        },
        note: "Held notes with clear vibrato (about ±50 cents at 5–7 Hz, e.g. at 7.4 s); 214–469 Hz",
    },
    Excerpt {
        name: "vocadito_35_low",
        dataset: Dataset::Vocadito,
        source: Source::Annotated {
            wav: "vocadito_35.wav",
            csv: "vocadito_35_f0.csv",
            start_s: 1.0,
            len_s: 8.0,
        },
        note: "Track 35, joint-lowest `average_pitch` (track 28 dips below 80 Hz); 84–186 Hz",
    },
    Excerpt {
        name: "mdb_nightowl_08_male",
        dataset: Dataset::MdbStemSynth,
        source: Source::Annotated {
            wav: "AClassicEducation_NightOwl_STEM_08.RESYN.wav",
            csv: "AClassicEducation_NightOwl_STEM_08.RESYN.csv",
            start_s: 13.0,
            len_s: 8.0,
        },
        note: "Male singer; 146–308 Hz, 81% voiced",
    },
    Excerpt {
        name: "mdb_pachelbel_02_low",
        dataset: Dataset::MdbStemSynth,
        source: Source::Annotated {
            wav: "MusicDelta_Pachelbel_STEM_02.RESYN.wav",
            csv: "MusicDelta_Pachelbel_STEM_02.RESYN.csv",
            start_s: 212.0,
            len_s: 8.0,
        },
        note: "Cello; 82–148 Hz, reaching E2",
    },
    Excerpt {
        name: "nsynth_e2",
        dataset: Dataset::NSynth,
        source: Source::NSynth {
            wav: "string_acoustic_071-040-127.wav",
            midi: 40,
        },
        note: "Acoustic string, E2 (82.4 Hz)",
    },
    Excerpt {
        name: "nsynth_c4",
        dataset: Dataset::NSynth,
        source: Source::NSynth {
            wav: "keyboard_electronic_098-060-100.wav",
            midi: 60,
        },
        note: "Electronic keyboard, C4 (261.6 Hz)",
    },
    Excerpt {
        name: "nsynth_g5",
        dataset: Dataset::NSynth,
        source: Source::NSynth {
            wav: "flute_acoustic_002-079-100.wav",
            midi: 79,
        },
        note: "Acoustic flute, G5 (784.0 Hz)",
    },
];

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.iter().map(String::as_str).collect::<Vec<_>>()[..] {
        ["build"] => build(),
        ["scan", csv] => scan(Path::new(csv), 8.0),
        ["scan", csv, "--len", len] => match len.parse() {
            Ok(len) if len > 0.0 => scan(Path::new(csv), len),
            _ => usage(),
        },
        _ => usage(),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn usage() -> ! {
    eprintln!("{USAGE}");
    std::process::exit(2);
}

fn in_range(f0_hz: f32) -> bool {
    (MIN_F0_HZ..=MAX_F0_HZ).contains(&f0_hz)
}

/// Prints the hop, then for each `len_s` window (in 1 s steps) the voiced
/// share, the voiced f0 range, and `OK` if every voiced frame is in range.
fn scan(csv: &Path, len_s: f64) -> Result<(), Box<dyn Error>> {
    let frames = parse_reference_csv(&fs::read_to_string(csv)?);
    let (Some(first), Some(second), Some(last)) = (frames.first(), frames.get(1), frames.last())
    else {
        return Err(format!("{}: fewer than two frames", csv.display()).into());
    };
    let hop = second.time_s - first.time_s;
    println!(
        "{}: {} frames, hop {:.6} s ({:.1} samples at 44.1 kHz)",
        csv.display(),
        frames.len(),
        hop,
        hop * 44_100.0
    );
    println!("start_s  voiced  min_hz   max_hz");
    let mut start = 0.0;
    while start + len_s <= last.time_s + hop {
        let window: Vec<&RefFrame> = frames
            .iter()
            .filter(|f| (start..start + len_s).contains(&f.time_s))
            .collect();
        let voiced: Vec<f32> = window
            .iter()
            .map(|f| f.f0_hz)
            .filter(|&f0| f0 > 0.0)
            .collect();
        if !voiced.is_empty() {
            let min = voiced.iter().copied().fold(f32::INFINITY, f32::min);
            let max = voiced.iter().copied().fold(0.0, f32::max);
            println!(
                "{start:>7.1}  {:>5.0}%  {min:>7.1}  {max:>7.1}  {}",
                100.0 * voiced.len() as f64 / window.len() as f64,
                if voiced.iter().all(|&f0| in_range(f0)) {
                    "OK"
                } else {
                    ""
                }
            );
        }
        start += 1.0;
    }
    Ok(())
}

fn build() -> Result<(), Box<dyn Error>> {
    let out_dir = Path::new(FIXTURES_DIR);
    fs::create_dir_all(out_dir)?;
    // Start clean, so a removed excerpt doesn't linger.
    for entry in fs::read_dir(out_dir)? {
        let path = entry?.path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if name.ends_with(".wav") || name.ends_with(".f0.csv") || name == "ATTRIBUTION.md" {
            fs::remove_file(&path)?;
        }
    }

    let datasets = Path::new(DATASETS_DIR);
    for excerpt in EXCERPTS {
        let wav_out = out_dir.join(format!("{}.wav", excerpt.name));
        let csv_out = out_dir.join(format!("{}.f0.csv", excerpt.name));
        let reference = match excerpt.source {
            Source::Annotated {
                wav,
                csv,
                start_s,
                len_s,
            } => {
                cut_wav(&find(datasets, wav)?, &wav_out, start_s, len_s)?;
                let reference: Vec<RefFrame> =
                    parse_reference_csv(&fs::read_to_string(find(datasets, csv)?)?)
                        .into_iter()
                        .filter(|f| (start_s..start_s + len_s).contains(&f.time_s))
                        .map(|f| RefFrame {
                            time_s: f.time_s - start_s,
                            f0_hz: f.f0_hz,
                        })
                        .collect();
                if let Some(bad) = reference
                    .iter()
                    .find(|f| f.f0_hz > 0.0 && !in_range(f.f0_hz))
                {
                    return Err(format!(
                        "{}: voiced reference at {:.3} s is {} Hz, outside {MIN_F0_HZ}–{MAX_F0_HZ} Hz",
                        excerpt.name, bad.time_s, bad.f0_hz
                    )
                    .into());
                }
                reference
            }
            Source::NSynth { wav, midi } => {
                resample_nsynth(&find(datasets, wav)?, &wav_out)?;
                let f0_hz = (440.0 * 2f64.powf((f64::from(midi) - 69.0) / 12.0)) as f32;
                let (from, to) = NSYNTH_SUSTAIN_S;
                let steps = ((to - from) / NSYNTH_REFERENCE_HOP_S).round() as usize;
                (0..=steps)
                    .map(|i| RefFrame {
                        time_s: from + i as f64 * NSYNTH_REFERENCE_HOP_S,
                        f0_hz,
                    })
                    .collect()
            }
        };
        let mut text = String::from("time_s,f0_hz\n");
        for f in &reference {
            writeln!(text, "{:.6},{:.4}", f.time_s, f.f0_hz.max(0.0))?;
        }
        fs::write(&csv_out, text)?;
        println!("{}: {} reference frames", excerpt.name, reference.len());
    }

    fs::write(out_dir.join("ATTRIBUTION.md"), attribution())?;

    let mut total = 0;
    for entry in fs::read_dir(out_dir)? {
        total += entry?.metadata()?.len();
    }
    println!(
        "{} excerpts, {:.2} MB in {FIXTURES_DIR}",
        EXCERPTS.len(),
        total as f64 / (1024.0 * 1024.0)
    );
    if total >= MAX_TOTAL_BYTES {
        return Err(format!("fixtures total {total} bytes, over the 5 MB budget").into());
    }
    Ok(())
}

/// Finds a file by name anywhere under `root`, because the archives' top-level
/// folders aren't fixed. macOS resource forks (`._*`, `__MACOSX`) are skipped.
fn find(root: &Path, name: &str) -> Result<PathBuf, Box<dyn Error>> {
    let mut dirs = vec![root.to_path_buf()];
    while let Some(dir) = dirs.pop() {
        let mut entries: Vec<PathBuf> = fs::read_dir(&dir)?
            .map(|e| e.map(|e| e.path()))
            .collect::<Result<_, _>>()?;
        entries.sort();
        for path in entries {
            if path.file_name().is_some_and(|n| n == "__MACOSX") {
                continue;
            }
            if path.is_dir() {
                dirs.push(path);
            } else if path.file_name().is_some_and(|n| n == name) {
                return Ok(path);
            }
        }
    }
    Err(format!(
        "{name} not found under {}. Run `mise run fixtures` to download the datasets.",
        root.display()
    )
    .into())
}

/// Copies the frames in `[start_s, start_s + len_s)`, keeping the source's spec.
fn cut_wav(source: &Path, dest: &Path, start_s: f64, len_s: f64) -> Result<(), Box<dyn Error>> {
    let mut reader = WavReader::open(source)?;
    let spec = reader.spec();
    let channels = usize::from(spec.channels);
    let start = (start_s * f64::from(spec.sample_rate)).round() as usize * channels;
    let len = (len_s * f64::from(spec.sample_rate)).round() as usize * channels;
    let available = reader.len() as usize;
    if start + len > available {
        return Err(format!(
            "{}: excerpt ends past the file's {:.2} s",
            source.display(),
            available as f64 / channels as f64 / f64::from(spec.sample_rate)
        )
        .into());
    }
    let mut writer = WavWriter::create(dest, spec)?;
    match spec.sample_format {
        SampleFormat::Int => {
            for s in reader.samples::<i32>().skip(start).take(len) {
                writer.write_sample(s?)?;
            }
        }
        SampleFormat::Float => {
            for s in reader.samples::<f32>().skip(start).take(len) {
                writer.write_sample(s?)?;
            }
        }
    }
    writer.finalize()?;
    Ok(())
}

/// Resamples a 16 kHz, 16-bit mono NSynth note to 44.1 kHz, 16-bit mono.
fn resample_nsynth(source: &Path, dest: &Path) -> Result<(), Box<dyn Error>> {
    let mut reader = WavReader::open(source)?;
    let spec = reader.spec();
    if (
        spec.channels,
        spec.sample_rate,
        spec.bits_per_sample,
        spec.sample_format,
    ) != (1, NSYNTH_SAMPLE_RATE as u32, 16, SampleFormat::Int)
    {
        return Err(format!(
            "{}: expected 16 kHz 16-bit mono, found {spec:?}",
            source.display()
        )
        .into());
    }
    let input: Vec<f64> = reader
        .samples::<i16>()
        .map(|s| s.map(|s| f64::from(s) / 32_768.0))
        .collect::<Result<_, _>>()?;

    let mut resampler = Fft::<f64>::new(
        NSYNTH_SAMPLE_RATE,
        OUTPUT_SAMPLE_RATE,
        1024,
        1,
        FixedSync::Both,
    )?;
    let adapter = InterleavedSlice::new(&input, 1, input.len())?;
    let output = resampler
        .process_all(&adapter, input.len(), None)?
        .take_data();

    let mut writer = WavWriter::create(
        dest,
        WavSpec {
            channels: 1,
            sample_rate: OUTPUT_SAMPLE_RATE as u32,
            bits_per_sample: 16,
            sample_format: SampleFormat::Int,
        },
    )?;
    for s in output {
        writer.write_sample((s * 32_768.0).round().clamp(-32_768.0, 32_767.0) as i16)?;
    }
    writer.finalize()?;
    Ok(())
}

fn attribution() -> String {
    let mut text = String::from(
        "# Test fixture attribution\n\n\
         Generated by `mise run fixtures` (`fixtures/src/main.rs`). Don't edit by hand.\n\n\
         Each `<name>.wav` is an excerpt of a dataset recording, and `<name>.f0.csv`\n\
         is its reference pitch track (`time_s,f0_hz`, 0 = unvoiced), with times\n\
         relative to the excerpt's start.\n",
    );
    for (dataset, heading, body) in DATASET_NOTES {
        if EXCERPTS.iter().all(|e| e.dataset != dataset) {
            continue;
        }
        write!(text, "\n## {heading}\n\n{body}\n\n").unwrap();
        text.push_str("| Fixture | Source | Excerpt | Notes |\n|---|---|---|---|\n");
        for e in EXCERPTS.iter().filter(|e| e.dataset == dataset) {
            let (file, span) = match e.source {
                Source::Annotated {
                    wav,
                    start_s,
                    len_s,
                    ..
                } => (wav, format!("{start_s} s + {len_s} s")),
                Source::NSynth { wav, .. } => (wav, String::from("whole note, 16 → 44.1 kHz")),
            };
            writeln!(text, "| `{}` | `{file}` | {span} | {} |", e.name, e.note).unwrap();
        }
    }
    text
}

const DATASET_NOTES: [(Dataset, &str, &str); 3] = [
    (
        Dataset::Vocadito,
        "Vocadito",
        "- Source: <https://zenodo.org/records/5578807> (`vocadito.zip`, md5 \
         `dea40fd18f14d899643c4ba221b33a46`)\n\
         - Licence: CC BY 4.0\n\
         - Citation: R. M. Bittner, K. Pasalo, J. J. Bosch, G. Meseguer-Brocal and \
         D. Rubinstein, \"vocadito: A dataset of solo vocals with f0, note, and lyric \
         annotations\", 2021. arXiv:2110.05580.\n\
         - Reference: `Annotations/F0/vocadito_N_f0.csv`.",
    ),
    (
        Dataset::MdbStemSynth,
        "MDB-stem-synth",
        "- Source: <https://zenodo.org/records/1481172> (`MDB-stem-synth.tar.gz`, SHA-256 \
         `92c3349e62b1e0cfca07a286d47fe3b19c4e46da71ccd19554b6e2aab172aa1a`)\n\
         - Licence: **CC BY-NC 4.0 (non-commercial)**. This is acceptable only \
         while Corral is private and not sold. If that changes, these fixtures \
         must be replaced.\n\
         - Citation: J. Salamon, R. M. Bittner, J. Bonada, J. J. Bosch, E. Gómez \
         and J. P. Bello, \"An Analysis/Synthesis Framework for Automatic F0 \
         Annotation of Multitrack Datasets\", ISMIR 2017. Also cite the original \
         MedleyDB: R. M. Bittner, J. Salamon, M. Tierney, M. Mauch, C. Cannam and \
         J. P. Bello, \"MedleyDB: A Multitrack Dataset for Annotation-Intensive MIR \
         Research\", ISMIR 2014.\n\
         - Reference: `annotation_stems/<stem>.RESYN.csv`. Instruments are from \
         the MedleyDB metadata.",
    ),
    (
        Dataset::NSynth,
        "NSynth",
        "- Source: <https://magenta.tensorflow.org/datasets/nsynth> \
         (`nsynth-test.jsonwav.tar.gz`, SHA-256 \
         `0f9ba5d62beba9ec4612f918d19f5e87a681822f1c566124f05fe8b27a51934c`)\n\
         - Licence: CC BY 4.0\n\
         - Citation: J. Engel, C. Resnick, A. Roberts, S. Dieleman, D. Eck, \
         K. Simonyan and M. Norouzi, \"Neural Audio Synthesis of Musical Notes \
         with WaveNet Autoencoders\", 2017. arXiv:1704.01279.\n\
         - Reference: synthesised at the note's MIDI pitch (A4 = 440 Hz), every \
         10 ms from 0.2 s to 2.8 s (the sustained part).",
    ),
];
