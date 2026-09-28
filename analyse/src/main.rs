//! Offline harness: runs the plugin's DSP chain over a WAV file, writing the
//! processed audio (in the input's own format) and a per-hop pitch CSV. The
//! chain's latency is compensated, so the output lines up with the input and
//! is the same length.

use std::error::Error;
use std::fs::{self, File};
use std::io::{BufReader, BufWriter, Read, Seek, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use corral_dsp::chain::{Chain, Settings};
use corral_dsp::harmony::IntervalSource;
use corral_dsp::shift;
use hound::{SampleFormat, WavReader, WavSpec, WavWriter};

const USAGE: &str =
    "usage: analyse <in.wav> [out.wav] [--gain-db <dB>] [--shift <semitones>] [--mix <percent>]";

/// Where output goes when no `out.wav` is given.
const DEFAULT_OUTPUT_DIR: &str = "target/analyse";

/// Matches the plugin's Mix default.
const DEFAULT_MIX_PERCENT: f32 = 50.0;

/// The processing controls, as given on the command line.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Options {
    gain_db: f32,
    /// Whole semitones, `shift::MIN..=shift::MAX`.
    shift: i32,
    /// 0.0–100.0.
    mix_percent: f32,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            gain_db: 0.0,
            shift: shift::DEFAULT,
            mix_percent: DEFAULT_MIX_PERCENT,
        }
    }
}

#[derive(Debug, PartialEq)]
struct Args {
    input: PathBuf,
    output: PathBuf,
    options: Options,
}

/// `None` for anything that doesn't match [`USAGE`].
fn parse_args(args: impl IntoIterator<Item = String>) -> Option<Args> {
    let mut paths = Vec::new();
    let mut options = Options::default();
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        // Values are taken as-is, so negative ones like "-6" work.
        match arg.as_str() {
            "--gain-db" => {
                options.gain_db = args
                    .next()?
                    .parse()
                    .ok()
                    .filter(|db: &f32| db.is_finite())?;
            }
            "--shift" => options.shift = shift::parse_shift(&args.next()?)?,
            "--mix" => {
                options.mix_percent = args
                    .next()?
                    .parse()
                    .ok()
                    .filter(|percent| (0.0..=100.0).contains(percent))?;
            }
            _ if arg.starts_with("--") => return None,
            _ => paths.push(PathBuf::from(arg)),
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
        options,
    })
}

struct Summary {
    hops: usize,
    voiced_hops: usize,
    frames: u64,
    sample_rate: u32,
    channels: u16,
    /// The chain's latency in frames, compensated in the output.
    latency: usize,
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
/// per-hop CSV to `csv`. The chain's latency is compensated: `latency` frames
/// of silence follow the input, and the first `latency` output frames are
/// dropped, so the output has as many frames as the input and lines up with
/// it. At 0 dB and 0 % mix the output is bit-identical to the input.
fn run<R: Read, W: Write + Seek, C: Write>(
    input: R,
    output: W,
    csv: C,
    options: Options,
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

    let settings = Settings {
        // Exactly 1.0 at 0 dB.
        gain: 10f32.powf(options.gain_db / 20.0),
        // Exactly 0.0 and 1.0 at 0 % and 100 %.
        mix: options.mix_percent / 100.0,
        // --shift stays a fixed interval until --key/--scale/--glide replace
        // it (plan 0006, Phase 5).
        interval: IntervalSource::Fixed(options.shift),
        glide_s: shift::DEFAULT_GLIDE_S,
    };
    let sample_rate = f64::from(spec.sample_rate);
    let mut chain = Chain::new(spec.sample_rate as f32);
    let channels = usize::from(spec.channels);
    let latency = chain.latency();
    let mut summary = Summary {
        hops: 0,
        voiced_hops: 0,
        frames: 0,
        sample_rate: spec.sample_rate,
        channels: spec.channels,
        latency,
    };

    let input_frames = samples.chunks_exact(channels);
    let frames = input_frames.len();
    let padding = std::iter::repeat_n(&[0.0; 2][..channels], latency);
    for (index, frame) in input_frames.chain(padding).enumerate() {
        let mut buffer = [0.0; 2];
        let frame_out = &mut buffer[..frame.len()];
        frame_out.copy_from_slice(frame);

        let detection = chain.process_frame(frame_out, settings);
        // Only hops completed by real input are reported, as without
        // compensation.
        if let Some(detection) = detection.filter(|_| index < frames) {
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

        // The first `latency` output frames precede the input.
        if index < latency {
            continue;
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
    let summary = run(input, output, csv, args.options)
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
            println!(
                "latency {} samples ({:.1} ms), compensated; shift {}, mix {}%",
                summary.latency,
                1000.0 * summary.latency as f64 / f64::from(summary.sample_rate),
                shift::format_shift(args.options.shift),
                args.options.mix_percent
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

    fn spec_float(channels: u16) -> WavSpec {
        spec(SampleFormat::Float, 32, channels)
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

    /// Dry only, so the output is the input times the gain.
    fn dry(gain_db: f32) -> Options {
        Options {
            gain_db,
            mix_percent: 0.0,
            ..Options::default()
        }
    }

    /// Fully shifted, at the default shift.
    fn wet() -> Options {
        Options {
            mix_percent: 100.0,
            ..Options::default()
        }
    }

    /// Runs the harness on WAV bytes, returning the output WAV and CSV text.
    fn run_bytes(wav: &[u8], options: Options) -> Result<(Vec<u8>, String), Box<dyn Error>> {
        let mut output = Cursor::new(Vec::new());
        let mut csv = Vec::new();
        run(wav, &mut output, &mut csv, options)?;
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
                let (output, _) = run_bytes(&int_wav(spec, &input), dry(0.0)).unwrap();
                let (out_spec, output) = read_ints(&output);
                assert_eq!(out_spec, spec);
                assert_eq!(output, input, "{bits}-bit, {channels} ch");
            }
        }
    }

    #[test]
    fn float_output_is_bit_identical_at_0_db() {
        for channels in [1, 2] {
            let spec = spec_float(channels);
            let input = test_signal(channels);
            let (output, _) = run_bytes(&float_wav(spec, &input), dry(0.0)).unwrap();
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
        let (output, _) = run_bytes(&int_wav(spec, &input), dry(-6.0206)).unwrap();
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
        let (_, csv) = run_bytes(&int_wav(spec, &input), dry(0.0)).unwrap();

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
            assert!(run_bytes(&wav, dry(0.0)).is_err(), "{name} was accepted");
        }
        let wav = int_wav(
            spec(SampleFormat::Int, 16, 3),
            &int_samples(16, &test_signal(3)),
        );
        assert!(
            run_bytes(&wav, dry(0.0)).is_err(),
            "3 channels were accepted"
        );
    }

    #[test]
    fn committed_fixtures_pass_through_bit_identical_at_0_db() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../corral-dsp/tests/fixtures");
        let mut checked = 0;
        for entry in fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_none_or(|e| e != "wav") {
                continue;
            }
            let input = fs::read(&path).unwrap();
            let (output, _) = run_bytes(&input, dry(0.0)).unwrap();
            let (mut before, mut after) = (
                WavReader::new(input.as_slice()).unwrap(),
                WavReader::new(output.as_slice()).unwrap(),
            );
            assert_eq!(after.spec(), before.spec(), "{}", path.display());
            match before.spec().sample_format {
                SampleFormat::Int => {
                    let samples = |r: &mut WavReader<&[u8]>| {
                        r.samples::<i32>().map(Result::unwrap).collect::<Vec<_>>()
                    };
                    assert!(
                        samples(&mut before) == samples(&mut after),
                        "{}",
                        path.display()
                    );
                }
                SampleFormat::Float => {
                    let bits = |r: &mut WavReader<&[u8]>| {
                        r.samples::<f32>()
                            .map(|s| s.unwrap().to_bits())
                            .collect::<Vec<_>>()
                    };
                    assert!(bits(&mut before) == bits(&mut after), "{}", path.display());
                }
            }
            checked += 1;
        }
        assert_eq!(checked, 7, "fixture WAVs in {}", dir.display());
    }

    #[test]
    fn output_is_the_same_length_as_the_input() {
        for channels in [1, 2] {
            let signal = test_signal(channels);
            let spec = spec(SampleFormat::Int, 16, channels);
            let input = int_samples(16, &signal);
            let (output, _) = run_bytes(&int_wav(spec, &input), wet()).unwrap();
            let (out_spec, output) = read_ints(&output);
            assert_eq!(out_spec, spec);
            assert_eq!(output.len(), input.len(), "int, {channels} ch");

            let spec = spec_float(channels);
            let (output, _) = run_bytes(&float_wav(spec, &signal), wet()).unwrap();
            let reader = WavReader::new(output.as_slice()).unwrap();
            assert_eq!(reader.spec(), spec);
            assert_eq!(reader.len() as usize, signal.len(), "float, {channels} ch");
        }
    }

    #[test]
    fn mix_100_output_is_shifted() {
        let tone: Vec<f32> = (0..SAMPLE_RATE)
            .map(|i| 0.5 * (TAU * 220.0 * i as f32 / SAMPLE_RATE as f32).sin())
            .collect();
        let (output, _) = run_bytes(&float_wav(spec_float(1), &tone), wet()).unwrap();
        let output: Vec<f32> = WavReader::new(output.as_slice())
            .unwrap()
            .samples::<f32>()
            .map(Result::unwrap)
            .collect();

        // Detect the output's pitch with a dry chain.
        let mut chain = Chain::new(SAMPLE_RATE as f32);
        let settings = Settings {
            gain: 1.0,
            mix: 0.0,
            interval: IntervalSource::Fixed(0),
            glide_s: shift::DEFAULT_GLIDE_S,
        };
        let edge = 0.15;
        let duration = output.len() as f64 / f64::from(SAMPLE_RATE);
        let mut f0s: Vec<f32> = output
            .iter()
            .filter_map(|&s| chain.process_frame(&mut [s], settings))
            .filter(|d| {
                let time = d.window_centre as f64 / f64::from(SAMPLE_RATE);
                d.voiced && time >= edge && time <= duration - edge
            })
            .map(|d| d.f0_hz)
            .collect();
        assert!(!f0s.is_empty(), "no voiced hops in the output");
        f0s.sort_by(f32::total_cmp);
        let median = f0s[f0s.len() / 2];
        let expected = 220.0 * shift::ratio(4.0);
        let cents = 1200.0 * (median / expected).log2();
        assert!(
            cents.abs() <= 10.0,
            "median {median:.2} Hz, expected {expected:.2} Hz ({cents:+.1}¢)"
        );
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
                options: Options::default(),
            })
        );
    }

    #[test]
    fn defaults_match_the_plugin() {
        let options = args(&["a.wav"]).unwrap().options;
        assert_eq!(options.shift, 4);
        assert_eq!(options.mix_percent, 50.0);
        assert_eq!(options.gain_db, 0.0);
    }

    #[test]
    fn shift_and_mix_flags_parse() {
        let parsed = args(&["--shift", "-5", "a.wav", "--mix", "100"]).unwrap();
        assert_eq!(parsed.options.shift, -5);
        assert_eq!(parsed.options.mix_percent, 100.0);
        let parsed = args(&["a.wav", "--shift", "+12", "--mix", "0"]).unwrap();
        assert_eq!(parsed.options.shift, 12);
        assert_eq!(parsed.options.mix_percent, 0.0);
    }

    #[test]
    fn gain_accepts_negative_values_anywhere() {
        let parsed = args(&["--gain-db", "-6", "a.wav", "b.wav"]).unwrap();
        assert_eq!(parsed.output, PathBuf::from("b.wav"));
        assert_eq!(parsed.options.gain_db, -6.0);
    }

    #[test]
    fn bad_arguments_are_rejected() {
        for bad in [
            &[][..],
            &["a.wav", "b.wav", "c.wav"],
            &["a.wav", "--gain-db"],
            &["a.wav", "--gain-db", "loud"],
            &["a.wav", "--verbose"],
            &["a.wav", "--shift", "13"],
            &["a.wav", "--shift", "-13"],
            &["a.wav", "--shift", "2.5"],
            &["a.wav", "--shift"],
            &["a.wav", "--mix", "101"],
            &["a.wav", "--mix", "-1"],
            &["a.wav", "--mix", "NaN"],
            &["a.wav", "--mix"],
        ] {
            assert_eq!(args(bad), None, "{bad:?}");
        }
    }
}
