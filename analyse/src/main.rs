//! Offline harness: runs the plugin's DSP chain over a WAV file, writing the
//! processed audio (in the input's own format) and a per-hop pitch CSV.

use std::error::Error;
use std::fs::{self, File};
use std::io::{BufReader, BufWriter, Read, Seek, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use corral_dsp::chain::Chain;
use hound::{SampleFormat, WavReader, WavSpec, WavWriter};

const USAGE: &str = "usage: analyse <in.wav> [out.wav] [--gain-db <dB>]";

/// Where output goes when no `out.wav` is given.
const DEFAULT_OUTPUT_DIR: &str = "target/analyse";

#[derive(Debug, PartialEq)]
struct Args {
    input: PathBuf,
    output: PathBuf,
    gain_db: f32,
}

/// `None` for anything that doesn't match [`USAGE`].
fn parse_args(args: impl IntoIterator<Item = String>) -> Option<Args> {
    let mut paths = Vec::new();
    let mut gain_db = 0.0;
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        if arg == "--gain-db" {
            // Taken as-is, so negative values like "-6" work.
            gain_db = args
                .next()?
                .parse()
                .ok()
                .filter(|db: &f32| db.is_finite())?;
        } else if arg.starts_with("--") {
            return None;
        } else {
            paths.push(PathBuf::from(arg));
        }
    }
    let (input, output) = match <[PathBuf; 2]>::try_from(paths) {
        Ok([input, output]) => (input, output),
        Err(paths) => {
            let [input] = <[PathBuf; 1]>::try_from(paths).ok()?;
            let stem = input.file_stem()?;
            let output = Path::new(DEFAULT_OUTPUT_DIR)
                .join(stem)
                .with_extension("wav");
            (input, output)
        }
    };
    Some(Args {
        input,
        output,
        gain_db,
    })
}

struct Summary {
    hops: usize,
    voiced_hops: usize,
    frames: u64,
    sample_rate: u32,
    channels: u16,
}

/// The input formats the harness accepts.
#[derive(Clone, Copy)]
enum Format {
    /// 16- or 24-bit integer, read as `i32`. The value is 2^(bits−1).
    Int {
        full_scale: f32,
        min: i32,
        max: i32,
    },
    Float,
}

impl Format {
    fn of(spec: WavSpec) -> Result<Self, String> {
        if !(1..=2).contains(&spec.channels) {
            return Err(format!(
                "{} channels: only mono and stereo are supported",
                spec.channels
            ));
        }
        match (spec.sample_format, spec.bits_per_sample) {
            (SampleFormat::Int, bits @ (16 | 24)) => {
                let max = (1i32 << (bits - 1)) - 1;
                Ok(Self::Int {
                    full_scale: (1u32 << (bits - 1)) as f32,
                    min: -max - 1,
                    max,
                })
            }
            (SampleFormat::Float, 32) => Ok(Self::Float),
            (format, bits) => Err(format!(
                "{bits}-bit {format:?} samples: only 16/24-bit Int and 32-bit Float are supported"
            )),
        }
    }
}

/// Reads WAV from `input`, writes processed WAV (same spec) to `output` and the
/// per-hop CSV to `csv`.
fn run<R: Read, W: Write + Seek, C: Write>(
    input: R,
    output: W,
    csv: C,
    gain_db: f32,
) -> Result<Summary, Box<dyn Error>> {
    let mut reader = WavReader::new(input)?;
    let spec = reader.spec();
    let format = Format::of(spec)?;

    // Both conversions are exact for 16- and 24-bit values, so a gain of
    // exactly 1.0 gives bit-identical output.
    let samples: Vec<f32> = match format {
        Format::Int { full_scale, .. } => reader
            .samples::<i32>()
            .map(|s| s.map(|s| s as f32 / full_scale))
            .collect::<Result<_, _>>()?,
        Format::Float => reader.samples::<f32>().collect::<Result<_, _>>()?,
    };

    let mut writer = WavWriter::new(output, spec)?;
    let mut csv = BufWriter::new(csv);
    writeln!(csv, "time_s,f0_hz,confidence,voiced")?;

    // Exactly 1.0 at 0 dB.
    let gain = 10f32.powf(gain_db / 20.0);
    let sample_rate = f64::from(spec.sample_rate);
    let mut chain = Chain::new(spec.sample_rate as f32);
    let mut summary = Summary {
        hops: 0,
        voiced_hops: 0,
        frames: 0,
        sample_rate: spec.sample_rate,
        channels: spec.channels,
    };

    for frame in samples.chunks_exact(usize::from(spec.channels)) {
        let mut buffer = [0.0; 2];
        let frame_out = &mut buffer[..frame.len()];
        frame_out.copy_from_slice(frame);

        if let Some(detection) = chain.process_frame(frame_out, gain) {
            summary.hops += 1;
            summary.voiced_hops += usize::from(detection.voiced);
            writeln!(
                csv,
                "{:.6},{:.3},{:.4},{}",
                detection.window_centre as f64 / sample_rate,
                detection.f0_hz,
                detection.confidence,
                u8::from(detection.voiced)
            )?;
        }

        for &sample in frame_out.iter() {
            match format {
                Format::Int {
                    full_scale,
                    min,
                    max,
                } => writer.write_sample(((sample * full_scale).round() as i32).clamp(min, max))?,
                Format::Float => writer.write_sample(sample)?,
            }
        }
        summary.frames += 1;
    }

    writer.finalize()?;
    csv.flush()?;
    Ok(summary)
}

fn analyse(args: &Args) -> Result<(PathBuf, Summary), Box<dyn Error>> {
    let csv_path = args.output.with_extension("csv");
    if let Some(parent) = args.output.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent)?;
    }
    let input = BufReader::new(
        File::open(&args.input).map_err(|e| format!("{}: {e}", args.input.display()))?,
    );
    let output = BufWriter::new(File::create(&args.output)?);
    let csv = File::create(&csv_path)?;
    let summary = run(input, output, csv, args.gain_db)
        .map_err(|e| format!("{}: {e}", args.input.display()))?;
    Ok((csv_path, summary))
}

fn main() -> ExitCode {
    let Some(args) = parse_args(std::env::args().skip(1)) else {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    };
    match analyse(&args) {
        Ok((csv_path, summary)) => {
            let voiced_percent = if summary.hops == 0 {
                0.0
            } else {
                100.0 * summary.voiced_hops as f64 / summary.hops as f64
            };
            println!("Wrote {}", args.output.display());
            println!("Wrote {}", csv_path.display());
            println!(
                "{} frames at {} Hz, {} ch: {} hops, {voiced_percent:.0}% voiced",
                summary.frames, summary.sample_rate, summary.channels, summary.hops
            );
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use std::f32::consts::TAU;
    use std::io::Cursor;

    use super::*;

    const SAMPLE_RATE: u32 = 44_100;

    fn spec(sample_format: SampleFormat, bits_per_sample: u16, channels: u16) -> WavSpec {
        WavSpec {
            channels,
            sample_rate: SAMPLE_RATE,
            bits_per_sample,
            sample_format,
        }
    }

    /// One second of an interleaved 220 Hz tone at −6 dBFS, then full-scale
    /// minimum and maximum on every channel.
    fn test_signal(channels: u16) -> Vec<f32> {
        let tone =
            (0..SAMPLE_RATE).map(|i| 0.5 * (TAU * 220.0 * i as f32 / SAMPLE_RATE as f32).sin());
        tone.chain([-1.0, 1.0])
            .flat_map(|s| std::iter::repeat_n(s, usize::from(channels)))
            .collect()
    }

    fn int_wav(spec: WavSpec, samples: &[i32]) -> Vec<u8> {
        let mut bytes = Cursor::new(Vec::new());
        let mut writer = WavWriter::new(&mut bytes, spec).unwrap();
        for &s in samples {
            writer.write_sample(s).unwrap();
        }
        writer.finalize().unwrap();
        bytes.into_inner()
    }

    fn float_wav(spec: WavSpec, samples: &[f32]) -> Vec<u8> {
        let mut bytes = Cursor::new(Vec::new());
        let mut writer = WavWriter::new(&mut bytes, spec).unwrap();
        for &s in samples {
            writer.write_sample(s).unwrap();
        }
        writer.finalize().unwrap();
        bytes.into_inner()
    }

    /// The signal in a format's native integer values, full scale included.
    fn int_samples(bits: u16, signal: &[f32]) -> Vec<i32> {
        let full_scale = (1i64 << (bits - 1)) as f64;
        signal
            .iter()
            .map(|&s| {
                (f64::from(s) * full_scale)
                    .round()
                    .clamp(-full_scale, full_scale - 1.0) as i32
            })
            .collect()
    }

    /// Runs the harness on WAV bytes, returning the output WAV and CSV text.
    fn run_bytes(wav: &[u8], gain_db: f32) -> Result<(Vec<u8>, String), Box<dyn Error>> {
        let mut output = Cursor::new(Vec::new());
        let mut csv = Vec::new();
        run(wav, &mut output, &mut csv, gain_db)?;
        Ok((output.into_inner(), String::from_utf8(csv)?))
    }

    fn read_ints(wav: &[u8]) -> (WavSpec, Vec<i32>) {
        let mut reader = WavReader::new(wav).unwrap();
        let samples = reader.samples::<i32>().map(Result::unwrap).collect();
        (reader.spec(), samples)
    }

    #[test]
    fn int_output_is_bit_identical_at_0_db() {
        for bits in [16, 24] {
            for channels in [1, 2] {
                let spec = spec(SampleFormat::Int, bits, channels);
                let input = int_samples(bits, &test_signal(channels));
                let (output, _) = run_bytes(&int_wav(spec, &input), 0.0).unwrap();
                let (out_spec, output) = read_ints(&output);
                assert_eq!(out_spec, spec);
                assert_eq!(output, input, "{bits}-bit, {channels} ch");
            }
        }
    }

    #[test]
    fn float_output_is_bit_identical_at_0_db() {
        for channels in [1, 2] {
            let spec = spec(SampleFormat::Float, 32, channels);
            let input = test_signal(channels);
            let (output, _) = run_bytes(&float_wav(spec, &input), 0.0).unwrap();
            let mut reader = WavReader::new(output.as_slice()).unwrap();
            assert_eq!(reader.spec(), spec);
            let output: Vec<f32> = reader.samples::<f32>().map(Result::unwrap).collect();
            let bits = |v: &[f32]| v.iter().map(|s| s.to_bits()).collect::<Vec<_>>();
            assert_eq!(bits(&output), bits(&input), "{channels} ch");
        }
    }

    #[test]
    fn minus_6_db_halves_16_bit_samples() {
        let spec = spec(SampleFormat::Int, 16, 2);
        let input = int_samples(16, &test_signal(2));
        let (output, _) = run_bytes(&int_wav(spec, &input), -6.0206).unwrap();
        let (_, output) = read_ints(&output);
        for (i, o) in input.iter().zip(&output) {
            let expected = f64::from(*i) / 2.0;
            assert!((f64::from(*o) - expected).abs() <= 1.0, "{i} → {o}");
        }
    }

    #[test]
    fn csv_has_one_row_per_hop() {
        let spec = spec(SampleFormat::Int, 16, 1);
        let input = int_samples(16, &test_signal(1));
        let frames = input.len();
        let (_, csv) = run_bytes(&int_wav(spec, &input), 0.0).unwrap();

        let chain = Chain::new(SAMPLE_RATE as f32);
        let (window, hop) = (chain.window_len(), chain.hop_len());
        let mut lines = csv.lines();
        assert_eq!(lines.next(), Some("time_s,f0_hz,confidence,voiced"));
        let rows: Vec<Vec<&str>> = lines.map(|l| l.split(',').collect()).collect();
        assert_eq!(rows.len(), (frames - window) / hop + 1);

        let times: Vec<f64> = rows.iter().map(|r| r[0].parse().unwrap()).collect();
        let step = hop as f64 / f64::from(SAMPLE_RATE);
        for pair in times.windows(2) {
            assert!((pair[1] - pair[0] - step).abs() < 2e-6, "{pair:?}");
        }
        for row in &rows {
            assert_eq!(row.len(), 4);
            assert!(row[3] == "0" || row[3] == "1", "{row:?}");
        }
        // The steady 220 Hz tone is voiced and tracked.
        let voiced: Vec<f32> = rows
            .iter()
            .filter(|r| r[3] == "1")
            .map(|r| r[1].parse().unwrap())
            .collect();
        assert!(!voiced.is_empty());
        assert!(voiced.iter().all(|f| (f - 220.0).abs() < 1.0), "{voiced:?}");
    }

    #[test]
    fn unsupported_formats_are_rejected() {
        let signal = test_signal(1);
        let cases = [
            ("8-bit", spec(SampleFormat::Int, 8, 1), 8),
            ("32-bit int", spec(SampleFormat::Int, 32, 1), 32),
        ];
        for (name, spec, bits) in cases {
            let wav = int_wav(spec, &int_samples(bits, &signal));
            assert!(run_bytes(&wav, 0.0).is_err(), "{name} was accepted");
        }
        let wav = int_wav(
            spec(SampleFormat::Int, 16, 3),
            &int_samples(16, &test_signal(3)),
        );
        assert!(run_bytes(&wav, 0.0).is_err(), "3 channels were accepted");
    }

    fn args(list: &[&str]) -> Option<Args> {
        parse_args(list.iter().map(|s| s.to_string()))
    }

    #[test]
    fn output_defaults_to_target_analyse() {
        assert_eq!(
            args(&["in/voice.wav"]),
            Some(Args {
                input: PathBuf::from("in/voice.wav"),
                output: PathBuf::from("target/analyse/voice.wav"),
                gain_db: 0.0,
            })
        );
    }

    #[test]
    fn gain_accepts_negative_values_anywhere() {
        let parsed = args(&["--gain-db", "-6", "a.wav", "b.wav"]).unwrap();
        assert_eq!(parsed.output, PathBuf::from("b.wav"));
        assert_eq!(parsed.gain_db, -6.0);
    }

    #[test]
    fn bad_arguments_are_rejected() {
        for bad in [
            &[][..],
            &["a.wav", "b.wav", "c.wav"],
            &["a.wav", "--gain-db"],
            &["a.wav", "--gain-db", "loud"],
            &["a.wav", "--verbose"],
        ] {
            assert_eq!(args(bad), None, "{bad:?}");
        }
    }
}
