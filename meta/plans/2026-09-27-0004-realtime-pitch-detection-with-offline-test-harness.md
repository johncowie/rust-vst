---
type: plan
id: "2026-09-27-0004-realtime-pitch-detection-with-offline-test-harness"
title: "Realtime Pitch Detection with Offline Test Harness Implementation Plan"
date: "2026-09-27T13:14:38+00:00"
author: John Cowie Del Corral
producer: create-plan
status: draft
work_item_id: "work-item:0004"
parent: "work-item:0004"
tags: [phase-3, dsp, pitch-detection, testing, gui]
revision: "d52105612b2eeb1c3af3ba24717b2a2926cf75c0"
repository: "rust-vst"
last_updated: "2026-09-27T13:14:38+00:00"
last_updated_by: John Cowie Del Corral
schema_version: 1
---

# Realtime Pitch Detection with Offline Test Harness Implementation Plan

## Overview

This plan delivers Phase 3 of `ROADMAP.md` (work item 0004):

- a plain `corral-dsp` crate (no nice-plug) holding the gain maths, a YIN
  pitch detector, a voicing gate, frequency → note conversion and
  `mir_eval`-style metrics;
- a `Chain` that the plugin and the offline harness both run: a mono sum of
  the input goes to the detector, then gain is applied;
- an `analyse` harness (`mise run analyse`) that reads a WAV and writes the
  processed WAV and a per-hop pitch CSV;
- a reproducible `fixtures` prep tool (`mise run fixtures`) that cuts the
  committed Vocadito, MDB-stem-synth and NSynth excerpts;
- accuracy, allocation and latency tests under `cargo test`;
- the detected note in the editor (e.g. `"D4 +12¢"`, or `"—"`);
- README and ROADMAP updates, and manual checks in Live 10.

Decisions made with the developer during planning:

- **Crates:** split them. `corral-dsp` is a lib, and `analyse` and
  `fixtures` are bins. `corral` stays `cdylib`.
- **Algorithm:** YIN, written by us, with the difference function computed
  through `realfft`. The FFT plans and buffers are allocated at setup. MPM is
  the fallback only if YIN can't meet the thresholds.
- **Tap point:** the detector sees the input *before* the Gain parameter.
- **Fixtures:** produced by a committed prep tool from downloads into a
  gitignored `.datasets/`. Only the excerpts are committed.

## Current State Analysis

- `Cargo.toml:3`: workspace members are `corral` and `xtask`. Clippy's `all`
  group is set to warn, and `mise run lint` adds `-D warnings`.
- `corral/Cargo.toml:7-8`: `crate-type = ["cdylib"]`. The dependencies are
  nice-plug 0.4, nice-plug-egui 0.5, and egui 0.36 with `default_fonts`.
- `corral/src/dsp.rs`: `MIN_GAIN_DB`, `SILENCE_THRESHOLD`, `apply_gain` and
  `format_gain`, with 10 unit tests. It has no nice-plug imports.
- `corral/src/lib.rs`:
  - `:118-146`: `process()` takes one smoothed gain per frame, applies
    `dsp::apply_gain` to both channels, then publishes the last gain once per
    buffer, only on change. It does this with `AtomicF32::store`, and
    `request_repaint()` only when `editor_state.is_open()`.
  - There is no `initialize()` or `reset()`.
- `corral/src/editor.rs`: a 300×120 window with a heading, a Gain
  `ParamSlider` and `Gain: <x> dB`, read from `Arc<AtomicF32>`.
- `mise.toml`:
  - `install` checks that Live is closed, runs `build-universal`, then signs
    and copies.
  - `check` = fmt-check, lint, then test.
- `.gitignore`: `/target`, `.DS_Store`, `.accelerator/config.local.md` and
  `mise.local.toml`.
- There is no WAV I/O, no FFT, no test audio and no `tests/` directory.

## Desired End State

- **Workspace members:** `corral`, `corral-dsp`, `analyse`, `fixtures` and
  `xtask`. `corral-dsp` has no `nice_plug` import anywhere.
- **Detector:** `corral_dsp::chain::Chain` sums the channels to mono, runs
  YIN every 256 samples on a 2048-sample window (44.1 and 48 kHz), and
  applies the gain. The per-hop path doesn't allocate, which an
  `assert_no_alloc` test proves.
- **Accuracy:** `cargo test` scores the detector on synthesised signals and
  on seven committed fixtures. It fails below the thresholds and prints each
  fixture's scores.
- **Harness:** `mise run analyse <in.wav> [out.wav]` writes `out.wav` and
  `out.csv` (`time_s,f0_hz,confidence,voiced`). At 0 dB, the output samples
  are bit-identical to the input.
- **Fixtures:** `mise run fixtures` regenerates the committed fixtures from
  the downloaded datasets.
- **Plugin:** Corral in Live 10 shows the sung note, or `"—"`. Its audio is
  unchanged from 0003.
- **Docs:** the README records the algorithm, window, hop, the latency at
  44.1 and 48 kHz, and the fixture scores. The ROADMAP Phase 3 items are
  ticked.

### Key Discoveries:

- **Allocation assertions need a debug build and a host.**
  - The allocator is installed only with
    `#[cfg(all(debug_assertions, feature = "assert_process_allocs"))]`
    (`nice-plug-0.4.2/src/wrapper/util.rs:30-32`).
  - The check wraps the wrapper's process call (`util.rs:193-203`).
  - On a violation it **aborts**, which crashes the host, because
    `nice-assert-no-alloc` 1.0 calls `handle_alloc_error`.
  - The standalone `dummy` backend feeds no audio, so it can't exercise the
    detector.
  - So we check the per-hop path with an automated test in `corral-dsp`,
    which uses `nice-assert-no-alloc` as that test binary's global
    allocator. The plugin glue is checked by hand with a debug build in Live.
- **`nice-plug-xtask` passes extra arguments to `cargo build`.**
  `bundle-universal` clones `other_args` for each target
  (`nice-plug-xtask-0.1.1/src/lib.rs:112-135`), so
  `cargo xtask bundle-universal corral --features assert_process_allocs`
  (without `--release`) builds a debug universal bundle.
- **`realfft` 3.5:**
  - `RealFftPlanner::plan_fft_forward/inverse` returns
    `Arc<dyn RealToComplex<f32>>` or `Arc<dyn ComplexToReal<f32>>`.
  - `process_with_scratch` doesn't allocate. `process` allocates scratch on
    every call, so it must not be used.
  - The input slice is used as scratch, so it has to be refilled on every
    call.
  - `num_complex::Complex` is re-exported as `realfft::num_complex`.
- **`hound` 3.5.1** reads 16- and 24-bit int (as `i32`), and 32-bit float,
  including `WAVE_FORMAT_EXTENSIBLE`.
  - The sample type must be wide enough for `bits_per_sample`, so 24-bit is
    read as `i32`.
  - int16 or int24 → f32 via ÷2¹⁵ or ÷2²³ is exact in f32, and so is the
    reverse (both fit in a 24-bit mantissa). So a gain of exactly 1.0 gives
    bit-identical output.
- **`ChannelSamples::iter_mut()`** can be called again on the same frame
  (`nice-plug-core-0.4.2/src/buffer/samples.rs:130-137`). So `process()` can
  copy the frame into `[f32; 2]`, hand it to `Chain`, and write it back.
- **YIN through FFT.** `d(τ) = r_x(0..W) + E(τ..τ+W) − 2·r(τ)`.
  - The shifted-energy term changes with τ and comes from a prefix sum of x².
  - The cross-term `r(τ) = Σ_{j<W} x_j·x_{j+τ}` is
    `IFFT(X · conj(Y)) / L`, where X is the zero-padded window and Y is the
    first W samples, zero-padded.
  - With N = 2048 and W = 1024, L = 4096 avoids circular wrap for
    τ ≤ N − W.
- **Datasets** (research, 2026-09-27):
  - **Vocadito:** `vocadito.zip`, 58.5 MB, CC BY 4.0, md5
    `dea40fd18f14d899643c4ba221b33a46`.
    - Files are `Audio/vocadito_N.wav` (44.1 kHz, 16-bit, mono) and
      `Annotations/F0/vocadito_N_f0.csv` (`time,Hz`, 0 = unvoiced; header
      and hop unverified).
    - There is one F0 annotator.
    - `vocadito_metadata.csv` has `average_pitch`. The lowest-voiced tracks
      are 35, 28, 4, 23, 36 and 26.
  - **MDB-stem-synth:** `MDB-stem-synth.tar.gz`, 1.8 GB, CC BY-NC 4.0.
    - Files are `audio_stems/<Artist>_<Song>_STEM_NN.RESYN.wav` and
      `annotation_stems/*.RESYN.csv`, with a 128-sample hop at 44.1 kHz.
    - The instrument isn't in the filename. MedleyDB's YAML metadata gives
      it:
      - male singer: `AClassicEducation_NightOwl_STEM_08`/`_13`,
        `MusicDelta_80sRock_STEM_04`, `StevenClark_Bounty_STEM_07`/`_08`;
      - cello: `MusicDelta_Beethoven_STEM_04`,
        `MatthewEntwistle_AnEveningWithOliver_STEM_01`/`_02`,
        `MusicDelta_Pachelbel_STEM_02`,
        `MusicDelta_InTheHalloftheMountainKing_STEM_04`;
      - bassoon: `MusicDelta_Beethoven_STEM_12`/`_13`,
        `MatthewEntwistle_AnEveningWithOliver_STEM_04`,
        `MusicDelta_InTheHalloftheMountainKing_STEM_08`.
    - Electric bass mostly sits below 80 Hz, so it's avoided.
  - **NSynth test:** `nsynth-test.jsonwav.tar.gz`, about 350 MB, CC BY 4.0.
    - The download is
      `http://download.magenta.tensorflow.org/datasets/nsynth/nsynth-test.jsonwav.tar.gz`.
    - Files are named `<family>_<source>_<NNN>-<pitch>-<velocity>.wav`.
    - Audio is 4 s at 16 kHz. The note is held for 3 s, then released.
- **mise tasks run in the project root**, so relative paths given to
  `mise run analyse` resolve from the root.

## What We're NOT Doing

- Pitch shifting, reporting latency to the host, and a dry/wet control
  (Phase 4).
- Note stabilisation or hysteresis (Phase 5). The display shows each hop's
  result.
- User-facing detector parameters. The thresholds are internal constants.
- pYIN or any full-sequence smoothing, and the `pyin` reference crate.
- Sample rates other than 44.1 and 48 kHz. Higher rates must still work (the
  window scales, see Phase 3), but they aren't tested or tuned.
- A runtime resampler. NSynth is resampled once, when fixtures are prepared.
- Committing raw datasets, or downloading anything during `mise run check`.
- WAV formats other than 16/24-bit int and 32-bit float, or more than 2
  channels, in the harness.
- CI outside `mise run check`.

## Implementation Approach

1. Split out the crate with no behaviour change.
2. Build the pure pieces (note maths, metrics), then the detector against
   synthesised signals, because they give exact ground truth.
3. Build the harness, then the fixtures and tests against real audio, where
   the thresholds are checked and tuning happens.
4. Wire the detector into the plugin last, so everything the plugin calls is
   already tested.
5. Record the numbers in the README, then verify in Live.

If the dataset thresholds can't be met by tuning YIN's constants (Phase 5):

- Record the scores in the work item.
- Stop and decide with the developer whether to relax a threshold (with a
  written reason) or switch to MPM.

---

## Phase 1: Split Out `corral-dsp`

### Overview

Create the plain DSP crate, move the gain code into it, and point the plugin
at it. No behaviour changes.

### Changes Required:

#### 1. Workspace

**File**: `Cargo.toml`

```toml
[workspace]
resolver = "3"
members = ["corral", "corral-dsp", "xtask"]
```

(`analyse` and `fixtures` are added in Phases 4 and 5.)

#### 2. New crate

**File**: `corral-dsp/Cargo.toml` (new)

```toml
[package]
name = "corral-dsp"
version = "0.1.0"
edition.workspace = true
publish = false

[dependencies]

[lints]
workspace = true
```

**File**: `corral-dsp/src/lib.rs` (new)

```rust
//! Plain DSP for Corral. Nothing here depends on nice-plug, so the plugin,
//! the offline harness and the tests all run the same code.

pub mod gain;
```

**File**: `corral-dsp/src/gain.rs`, moved with
`git mv corral/src/dsp.rs corral-dsp/src/gain.rs`

- The contents and tests are unchanged.
- Change the module doc to `//! Gain maths and the Gain display formatter.`

#### 3. Plugin glue

**File**: `corral/Cargo.toml`: add `corral-dsp = { path = "../corral-dsp" }`.

**Files**: `corral/src/lib.rs`, `corral/src/editor.rs`

- Remove `mod dsp;`.
- Add `use corral_dsp::gain;`.
- Replace `dsp::` with `gain::` (`MIN_GAIN_DB`, `format_gain`, `apply_gain`).

### Success Criteria:

#### Automated Verification:

- [x] `mise run check` exits 0, and the 10 gain tests now run in
      `corral-dsp`
- [x] `grep -rn nice_plug corral-dsp/` prints nothing
- [x] `test ! -e corral/src/dsp.rs`
- [x] `mise run build-universal` succeeds

#### Manual Verification:

- [x] None. The unchanged gain tests pin the behaviour.

---

## Phase 2: Note Maths and Accuracy Metrics

### Overview

Pure functions with unit tests. They convert frequency to a note and cents,
format the note for display, pack it into a `u32` for the atomic, and score
an estimate against a reference.

### Changes Required:

#### 1. Notes

**File**: `corral-dsp/src/note.rs` (new). Add `pub mod note;` to `lib.rs`.

```rust
//! Frequency → nearest MIDI note and cents offset (A4 = 440 Hz), and display.

const NAMES: [&str; 12] = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];

/// A note and a whole-cent offset in −50..=49. A frequency exactly 50 cents
/// above a note rounds up to the next note at −50, so the two edges never
/// disagree.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Note {
    pub midi: i32,
    pub cents: i32,
}

/// `None` for zero, negative or non-finite input.
pub fn nearest_note(f0_hz: f32) -> Option<Note> {
    if !(f0_hz.is_finite() && f0_hz > 0.0) {
        return None;
    }
    // Round to whole cents first, then split with integer maths. The float
    // error from an exact ±50-cent input (≪ 0.5 cent) can't change the result.
    let cents_from_a4 = (1200.0 * (f64::from(f0_hz) / 440.0).log2()).round() as i32;
    let shifted = cents_from_a4 + 69 * 100 + 50;
    Some(Note {
        midi: shifted.div_euclid(100),
        cents: shifted.rem_euclid(100) - 50,
    })
}

/// e.g. "A4 +0¢", "D4 +12¢", "C#3 -7¢". MIDI 60 is C4.
pub fn format_note(note: Note) -> String {
    let name = NAMES[note.midi.rem_euclid(12) as usize];
    let octave = note.midi.div_euclid(12) - 1;
    format!("{name}{octave} {:+}¢", note.cents)
}

/// Packed "no note", for the audio → editor atomic.
pub const UNVOICED: u32 = u32::MAX;

/// Packs a note (or unvoiced) into one `u32`, so the audio thread can
/// publish it with a single store and compare it cheaply.
pub fn pack(note: Option<Note>) -> u32 {
    match note {
        None => UNVOICED,
        Some(Note { midi, cents }) => (u32::from(midi as i16 as u16) << 16) | (cents + 50) as u32,
    }
}

pub fn unpack(bits: u32) -> Option<Note> {
    (bits != UNVOICED).then(|| Note {
        midi: i32::from((bits >> 16) as u16 as i16),
        cents: (bits & 0xFFFF) as i32 - 50,
    })
}
```

Unit tests, in `#[cfg(test)] mod tests` as in `gain.rs`:

- 440 Hz → `Note { midi: 69, cents: 0 }`, which formats as `"A4 +0¢"`.
- Every note from C1 (MIDI 24) to B7 (MIDI 107), computed with
  `440·2^((m−69)/12)`:
  - maps to itself with cents 0;
  - at ±10 and ±49 cents, gives the same note with the right offset;
  - at exactly +50 cents, gives the next note at −50;
  - at exactly −50 cents, gives the same note at −50.
- The format of octave boundaries: MIDI 59 → `"B3 …"`, 60 → `"C4 …"`, and
  sharps (`"C#3 -7¢"`).
- `nearest_note` returns `None` for 0, −1, NaN and ∞.
- `pack` and `unpack` round-trip across the 80 Hz–1 kHz notes and cents
  −50..=49. `pack(None) == UNVOICED`. No packed note equals `UNVOICED`.

`clippy::cast_*` lints aren't in `clippy::all`. If any fires anyway, add a
narrow `#[allow]` with a comment. Don't loosen the workspace lints.

#### 2. Metrics

**File**: `corral-dsp/src/eval.rs` (new). Add `pub mod eval;`.

A simplified version of `mir_eval.melody`. Scores are taken at the
reference's frame times, and each reference frame is compared with the
nearest estimate.

```rust
//! Pitch-tracking metrics after mir_eval.melody, simplified: each reference
//! frame is compared with the nearest estimate in time (no resampling).

/// A reference frame. `f0_hz <= 0` means unvoiced.
#[derive(Clone, Copy, Debug)]
pub struct RefFrame { pub time_s: f64, pub f0_hz: f32 }

/// A detector output. `f0_hz` may be > 0 even when `voiced` is false (the
/// raw estimate); raw pitch/chroma accuracy use it regardless, as mir_eval does.
#[derive(Clone, Copy, Debug)]
pub struct EstFrame { pub time_s: f64, pub f0_hz: f32, pub voiced: bool }

#[derive(Clone, Copy, Debug)]
pub struct Scores {
    pub raw_pitch_accuracy: Option<f64>,
    pub raw_chroma_accuracy: Option<f64>,
    pub voicing_recall: Option<f64>,
    pub voicing_false_alarm: Option<f64>,
    pub voiced_frames: usize,
    pub unvoiced_frames: usize,
}

pub const TOLERANCE_CENTS: f64 = 50.0;

/// `estimates` must be sorted by time. A reference frame more than
/// `max_gap_s` from every estimate counts as an unvoiced estimate with no pitch.
pub fn score(reference: &[RefFrame], estimates: &[EstFrame], max_gap_s: f64) -> Scores { … }

/// Parses "time,f0" CSV text. Lines that don't parse as two numbers (headers,
/// blanks) are skipped.
pub fn parse_reference_csv(text: &str) -> Vec<RefFrame> { … }
```

Definitions, with each denominator at 0 giving `None`:

- **voiced reference frame:** `f0_hz > 0`.
- **raw pitch accuracy:** voiced reference frames where the estimate's
  `f0_hz > 0` and `|1200·log2(est/ref)| ≤ 50`, divided by all voiced
  reference frames.
- **raw chroma accuracy:** the same, but with the cents error folded to the
  nearest octave first: `e − 1200·round(e/1200)`.
- **voicing recall:** voiced reference frames with `voiced == true`, divided
  by all voiced reference frames.
- **voicing false alarm:** unvoiced reference frames with `voiced == true`,
  divided by all unvoiced reference frames.

The nearest estimate is found with `partition_point` on `time_s`.

Unit tests:

- a perfect estimate scores 1.0 on all four;
- all estimates an octave high give RPA 0 and RCA 1;
- an error of 49 cents counts, and one of 51 cents doesn't;
- an estimate that is unvoiced but carries the right `f0` counts for RPA but
  not for voicing recall;
- voicing false alarm when estimates are voiced on silent reference frames;
- `None` for empty denominators;
- a reference frame outside `max_gap_s` counts as a miss;
- `parse_reference_csv` skips a header line and blank lines.

### Success Criteria:

#### Automated Verification:

- [x] `mise run check` exits 0
- [x] `cargo test -p corral-dsp note::` and `cargo test -p corral-dsp eval::`
      each run and pass
- [x] `grep -rn nice_plug corral-dsp/` prints nothing

#### Manual Verification:

- [x] None.

---

## Phase 3: YIN Detector, Voicing Gate and `Chain`

### Overview

The realtime detector and the chain the plugin runs, tested on synthesised
signals. This phase also adds the allocation test and the latency
measurement.

### Changes Required:

#### 1. Dependencies

**File**: `corral-dsp/Cargo.toml`

```toml
[dependencies]
realfft = "3.5"

[dev-dependencies]
# Same fork nice-plug uses; installed as the global allocator in tests/no_alloc.rs only.
nice-assert-no-alloc = { version = "1.0", features = ["warn_debug"] }
hound = "3.5"
```

`warn_debug` makes violations count (`violation_count()`) instead of
aborting, so a failure shows up as a normal failed assertion. If
`nice-assert-no-alloc` 1.0 lacks that feature or API, use `assert_no_alloc`
1.1 with the same settings.

#### 2. Detector

**File**: `corral-dsp/src/pitch.rs` (new). Add `pub mod pitch;`.

The constants, all internal:

```rust
/// Detection range: E2 (82.4 Hz) sits just above the floor; ~B5 (988 Hz) below the top.
pub const MIN_F0_HZ: f32 = 80.0;
pub const MAX_F0_HZ: f32 = 1000.0;
/// YIN's absolute threshold on the cumulative mean normalised difference.
const YIN_THRESHOLD: f32 = 0.15;
/// Voiced needs confidence (1 − CMND at the chosen lag) at or above this…
const MIN_CONFIDENCE: f32 = 0.8;
/// …and the window RMS at or above this.
const LEVEL_GATE_DBFS: f32 = -50.0;
```

**Window and hop scale with the sample rate.**

- `window_len = 2048 × ceil(sample_rate / 48000)`, rounded up to a power of
  two. `hop_len = window_len / 8`.
- At 44.1 and 48 kHz that gives 2048 and 256. At 96 kHz it gives 4096 and
  512, which keeps the timing about the same and covers the lag range.

Lags:

- `integration_len W = window_len / 2`.
- `tau_min = max(2, floor(sr / (MAX_F0_HZ · 1.1)))`.
- `tau_max = ceil(sr / (MIN_F0_HZ / 1.1))`.
  - This is 607 at 44.1 kHz and 660 at 48 kHz.
  - Assert `tau_max + 1 + W <= window_len` in `new`.
- `fft_len = (window_len + W).next_power_of_two()`, which is 4096.

```rust
/// One hop's output. `f0_hz` is YIN's estimate (0.0 when the level gate
/// skipped analysis); `voiced` is the gate's decision.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Detection {
    pub f0_hz: f32,
    pub confidence: f32,
    pub voiced: bool,
    /// Index (samples since reset) of the analysis window's centre sample.
    pub window_centre: u64,
}

/// YIN on one window. Owns every buffer and both FFT plans; `analyse` never allocates.
struct Yin {
    sample_rate: f32,
    tau_min: usize,
    tau_max: usize,
    integration_len: usize,
    forward: Arc<dyn RealToComplex<f32>>,
    inverse: Arc<dyn ComplexToReal<f32>>,
    fft_in: Vec<f32>,             // fft_len
    spectrum_x: Vec<Complex<f32>>, // fft_len / 2 + 1
    spectrum_y: Vec<Complex<f32>>,
    scratch_forward: Vec<Complex<f32>>,
    scratch_inverse: Vec<Complex<f32>>,
    correlation: Vec<f32>,        // fft_len
    energy_prefix: Vec<f64>,      // window_len + 1
    cmnd: Vec<f32>,               // tau_max + 2
}
```

`Yin::analyse(&mut self, window: &[f32]) -> (f32 /*f0*/, f32 /*confidence*/)`
works like this:

1. **Cross term.**
   - Copy `window` into `fft_in` and zero the rest, then forward-FFT into
     `spectrum_x`.
   - Copy `window[..W]` into `fft_in` and zero the rest, then forward-FFT into
     `spectrum_y`.
   - Set `spectrum_x[k] *= spectrum_y[k].conj()`, and zero the imaginary parts
     of DC and Nyquist.
   - Inverse-FFT into `correlation`. Then `r(τ) = correlation[τ] / fft_len`.
   - Every FFT call is `process_with_scratch(...).expect("FFT lengths are fixed
     at setup")`. The panic path is unreachable, and only a failing `expect`
     would allocate.
2. **Energy.** `energy_prefix[i] = Σ_{j<i} x_j²` in f64. Then
   `d(τ) = (P[W] + P[τ+W] − P[τ] − 2 r(τ)).max(0)` for τ in 1..=tau_max+1.
3. **Cumulative mean normalised difference.** `cmnd[0] = 1`, and
   `cmnd[τ] = d(τ)·τ / Σ_{j=1..τ} d(j)`. It is 1 when the running sum is 0.
4. **Pick the lag.**
   - Take the first τ in tau_min..=tau_max with `cmnd[τ] < YIN_THRESHOLD`,
     then walk forward while `cmnd[τ+1] < cmnd[τ]`.
   - If no τ is below the threshold, use the argmin over tau_min..=tau_max.
5. **Refine.**
   - Fit a parabola through `cmnd[τ−1..=τ+1]`. Skip the fit if the
     denominator is ≈ 0.
   - `f0 = sr / τ_refined`, and `confidence = (1 − cmnd[τ]).clamp(0, 1)`.

```rust
/// Collects mono input in a ring buffer and runs YIN once per hop,
/// independent of the host's buffer size.
pub struct PitchTracker {
    yin: Yin,
    ring: Vec<f32>,      // window_len
    write_pos: usize,
    filled: usize,       // saturates at window_len
    since_hop: usize,
    hop_len: usize,
    samples_seen: u64,
    window: Vec<f32>,    // linearised copy of the ring, window_len
    level_gate_rms: f32, // 10^(LEVEL_GATE_DBFS/20)
}

impl PitchTracker {
    /// Allocates everything. Call from `initialize()`, not the audio thread.
    pub fn new(sample_rate: f32) -> Self { … }
    /// Clears history; doesn't allocate.
    pub fn reset(&mut self) { … }
    pub fn window_len(&self) -> usize { … }
    pub fn hop_len(&self) -> usize { … }
    /// Non-finite samples are treated as 0.0. Returns a detection every
    /// `hop_len` samples once the window has filled.
    pub fn push(&mut self, sample: f32) -> Option<Detection> { … }
}
```

In `push`:

- After each hop, once `filled == window_len`, linearise the ring into
  `window` and compute its RMS.
- Below the gate: return `f0 0.0, confidence 0.0, voiced false`, and skip
  YIN.
- Otherwise run YIN.
  `voiced = confidence >= MIN_CONFIDENCE && (MIN_F0_HZ/1.03..=MAX_F0_HZ*1.03).contains(&f0)`,
  where 1.03 is about 50 cents.
- `window_centre = samples_seen − window_len/2`.

#### 3. Chain

**File**: `corral-dsp/src/chain.rs` (new). Add `pub mod chain;`.

```rust
//! The processing chain shared by the plugin and the offline harness:
//! mono-sum → pitch tracker (pre-gain), then gain on every channel.

pub struct Chain { tracker: PitchTracker }

impl Chain {
    /// Allocates. Call from `initialize()`.
    pub fn new(sample_rate: f32) -> Self { … }
    pub fn reset(&mut self) { self.tracker.reset() }
    pub fn window_len(&self) -> usize { … }
    pub fn hop_len(&self) -> usize { … }

    /// Processes one frame in place (one sample per channel). The detector
    /// sees the mean of the channels before gain, so the Gain slider
    /// doesn't affect detection.
    #[inline]
    pub fn process_frame(&mut self, frame: &mut [f32], gain: f32) -> Option<Detection> {
        let mono = frame.iter().sum::<f32>() / frame.len() as f32;
        let detection = self.tracker.push(mono);
        for sample in frame.iter_mut() {
            *sample = gain::apply_gain(*sample, gain);
        }
        detection
    }
}
```

An empty frame gives NaN, which `push` sanitises to 0.0.

#### 4. Unit tests in `pitch.rs`

- `new` gives window and hop of 2048/256 at 44.1 and 48 kHz, and 4096/512 at
  96 kHz. The lag assertion holds at all three.
- No detection until `window_len` samples have been pushed. After that there
  is exactly one detection every `hop_len` samples.
- `window_centre` values are spaced by `hop_len`.
- A pure 220 Hz sine gives an f0 within 5 cents, with confidence ≥ 0.9.
- `reset` restarts the fill: there is no detection for the next
  `window_len − 1` samples.
- NaN and ∞ input don't produce a NaN `f0` or `confidence`.

#### 5. Integration tests

**File**: `corral-dsp/tests/common/mod.rs` (new)

- The file starts with `#![allow(dead_code)]`, commented to say each test
  binary uses a different subset.
- `harmonic_tone(sr, seconds, freq_at: impl Fn(f64) -> f64, harmonics: usize, peak: f32) -> Vec<f32>`
  - It's phase-continuous: each harmonic's phase is integrated from
    `freq_at` per sample.
  - It has amplitude 1/k, drops harmonics above 0.45·sr, and normalises to
    `peak`.
- `white_noise(n, peak, seed)` uses a xorshift32 PRNG, so it needs no `rand`
  dependency.
- `track(signal, sr) -> Vec<EstFrame>` runs a `Chain` frame by frame on mono
  at gain 1.0. Its `time_s` is `window_centre / sr`.
- `print_scores(name, &Scores)` prints one line in a fixed format.
  - For example:
    `vocadito_35_low  RPA 0.934  RCA 0.941  VR 0.902  VFA 0.121  (voiced 1377, unvoiced 211)`.
  - `None` prints as `n/a`.
- `read_wav_mono(path) -> (Vec<f32>, u32)` uses hound, with the same
  int/float conversion as the harness, averaging stereo.

**File**: `corral-dsp/tests/synthetic.rs` (new)

Each test runs at both 44.1 and 48 kHz. The reference frames are every
10 ms, starting at 0.1 s. `max_gap_s` is `hop/sr`.

| Signal | Content | Reference excludes | Asserts |
|---|---|---|---|
| Scale | C major C3→C5 (15 notes), 0.4 s each, 12 harmonics, peak 0.5 | frames within ±30 ms of each step | RPA ≥ 0.98, RCA − RPA ≤ 0.01 |
| Vibrato | A3, `220·2^((50/1200)·sin(2π·5.5·t))`, 3 s | — | RPA ≥ 0.98, RCA − RPA ≤ 0.01 |
| Low edge | E2 (82.41 Hz), 2 s | — | RPA ≥ 0.98, RCA − RPA ≤ 0.01 |
| High edge | B5 (987.77 Hz), 2 s | — | RPA ≥ 0.98, RCA − RPA ≤ 0.01 |
| Silence | 3 s of zeros | — | ≥ 99% of hops unvoiced |
| White noise | 3 s, peak 0.25 (above the level gate) | — | ≥ 99% of hops unvoiced |

- The scale, vibrato and range-edge tests also assert voicing recall ≥ 0.95.
  That's our addition. It catches a gate so strict it would blank the
  display.
- Each test prints its scores with `print_scores`.

**File**: `corral-dsp/tests/no_alloc.rs` (new)

```rust
use nice_assert_no_alloc::{AllocDisabler, assert_no_alloc, reset_violation_count, violation_count};

#[global_allocator]
static A: AllocDisabler = AllocDisabler;

#[test]
fn per_frame_path_never_allocates() {
    for sr in [44_100.0, 48_000.0] {
        // Built (and signals synthesised) outside the check: setup may allocate.
        let mut chain = Chain::new(sr);
        let tone = …; let noise = …; let silence = vec![0.0; …];
        reset_violation_count();
        assert_no_alloc(|| {
            for signal in [&tone, &noise, &silence] {
                for &s in signal.iter() {
                    let mut frame = [s, s];
                    std::hint::black_box(chain.process_frame(&mut frame, 0.5));
                }
            }
            chain.reset();
            for &s in tone.iter() { let mut frame = [s, s]; std::hint::black_box(chain.process_frame(&mut frame, 1.0)); }
        });
        assert_eq!(violation_count(), 0);
    }
}
```

This file has exactly one `#[test]`, because the violation counter is global
to the binary.

**File**: `corral-dsp/tests/latency.rs` (new)

- For each rate (44.1 and 48 kHz), and for A3 (220 Hz) and E2 (82.41 Hz):
  - 0.5 s of silence, then the tone (8 harmonics, peak 0.5) for 1 s.
  - Onset = sample `0.5·sr`.
- Find the first detection, at hop end sample `window_centre + window_len/2`,
  that is voiced and within 50 cents of the tone. The following 4 detections
  must also be voiced and within 50 cents, which makes it "stable".
- Print a line like:
  `latency 44100 Hz A3: 52.2 ms (window 2048, hop 256)`.
  - Latency = (hop end sample − onset) / sr.
  - It excludes the host buffer.
- Assert only that a stable detection exists within 0.5 s. There is no
  pass/fail limit on the latency itself.

### Success Criteria:

#### Automated Verification:

- [x] `mise run check` exits 0
- [x] `cargo test -p corral-dsp --test synthetic -- --nocapture` passes and
      prints scores for every signal at both rates
- [x] `cargo test -p corral-dsp --test no_alloc` passes
- [x] `cargo test -p corral-dsp --test latency -- --nocapture` prints four
      latency lines
- [x] Sanity check for the allocation test:
  - [x] temporarily add `let _v = vec![0u8; 1];` inside `Yin::analyse`;
  - [x] `cargo test -p corral-dsp --test no_alloc` now fails;
  - [x] revert the line.
- [x] `grep -rn nice_plug corral-dsp/src` prints nothing
- [x] `grep -n '\.process(' corral-dsp/src/pitch.rs` prints nothing (only
      `process_with_scratch` is used)

#### Manual Verification:

- [x] None. Real audio is checked in Phase 5 and in Live in Phase 7.

---

## Phase 4: `analyse` Offline Harness

### Overview

A binary that runs `Chain` over a WAV file. It writes the processed audio in
the input's own format, plus a per-hop CSV.

### Changes Required:

#### 1. Crate

- `Cargo.toml` members gain `"analyse"`.
- **File**: `analyse/Cargo.toml` (new): package `analyse`, workspace
  edition and lints. Dependencies are
  `corral-dsp = { path = "../corral-dsp" }` and `hound = "3.5"`.

**File**: `analyse/src/main.rs` (new)

Usage: `analyse <in.wav> [out.wav] [--gain-db <dB>]`

- The default `out.wav` is `target/analyse/<input stem>.wav`. Its parent
  directory is created.
- The CSV path is `out.wav` with the extension `.csv`.
- Arguments are parsed by hand, without `clap`.
- A bad argument prints the usage line to stderr and exits with status 2.

Structure (testable without the filesystem):

```rust
struct Summary { hops: usize, voiced_hops: usize, frames: u64, sample_rate: u32, channels: u16 }

/// Reads WAV from `input`, writes processed WAV (same spec) to `output` and the
/// per-hop CSV to `csv`.
fn run<R: Read, W: Write + Seek, C: Write>(input: R, output: W, csv: C, gain_db: f32)
    -> Result<Summary, Box<dyn Error>>
```

- **Formats.** Accept `(Int, 16)`, `(Int, 24)` (read as `i32`) and
  `(Float, 32)`, with 1 or 2 channels. Anything else is an error that names
  the format.
- **Conversion.** Int → `x as f32 / 2^(bits−1)`. Back to int with
  `(y · 2^(bits−1)).round().clamp(min, max) as i32`. Float passes through.
- **Gain.** `10^(gain_db/20)`, which is exactly 1.0 at 0 dB. It's passed as
  the constant gain to `Chain::process_frame`.
- **Output.** `hound::WavWriter::new(output, reader.spec())`.
- **CSV.**
  - The header is `time_s,f0_hz,confidence,voiced`.
  - Each hop writes one row, `format!("{:.6},{:.3},{:.4},{}", window_centre / sr, f0, confidence, voiced as u8)`.
  - Write it through a `BufWriter`.
- **Result.** `main` prints the paths and a summary, for example
  `2583 hops, 61% voiced`.

Unit tests in `main.rs`:

- **Bit-identity.** For each of `(Int,16)`, `(Int,24)` and `(Float,32)`, and
  for mono and stereo:
  - build a WAV in a `Cursor<Vec<u8>>` from a tone plus extremes (full-scale
    min and max);
  - run at 0 dB;
  - read the output back and `assert_eq!` sample vectors of the native type.
- **Gain.** At −6.0206 dB, 16-bit samples halve (within ±1 LSB).
- **CSV.**
  - The header matches.
  - The row count equals `(frames − window_len) / hop_len + 1` when
    `frames ≥ window_len`.
  - Times increase by `hop/sr`.
- **Rejection.** 8-bit and 32-bit int input, and 3-channel input, return an
  error.
- **Fixture bit-identity.** This test is added in Phase 5.
  - It runs `run` on every committed fixture WAV, found through
    `env!("CARGO_MANIFEST_DIR")/../corral-dsp/tests/fixtures`.
  - It asserts that the samples are identical at 0 dB.

#### 2. mise task

**File**: `mise.toml`, after `test`:

```toml
[tasks.analyse]
description = "Run the plugin's DSP chain on a WAV: analyse <in.wav> [out.wav] [--gain-db <dB>] (writes out.wav and out.csv; paths are relative to the project root)"
run = "cargo run --release --quiet -p analyse --"
```

- mise appends the task's arguments to the `run` string.
- If mise swallows `--gain-db`, the documented form is
  `mise run analyse -- in.wav out.wav --gain-db -6`. Record whichever form
  works in the README.

### Success Criteria:

#### Automated Verification:

- [x] `mise run check` exits 0, including the `analyse` unit tests
- [x] `mise tasks` lists `analyse` with its description
- [x] Synthesised input works end to end:
  - [x] write a test WAV (a stereo 16-bit 220 Hz tone) with a small
        throwaway test or snippet, not committed;
  - [x] `mise run analyse <it> target/analyse/tone.wav` writes
        `target/analyse/tone.wav` and `target/analyse/tone.csv`;
  - [x] the CSV's voiced rows show `f0_hz` ≈ 220.
- [x] `mise run analyse` with no arguments exits non-zero and prints the
      usage line

#### Manual Verification:

- [ ] The CSV opens in a spreadsheet, and its f0 column plots as a plausible
      pitch track for a synthesised tone

---

## Phase 5: Fixtures and Accuracy Tests

### Overview

This phase has five parts:

- build the prep tool;
- download the datasets;
- choose and cut the seven excerpts;
- commit them with attribution;
- make `cargo test` score every fixture against the acceptance thresholds.

Any tuning happens here.

### Changes Required:

#### 1. Ignore the downloads

**File**: `.gitignore`: add `/.datasets`.

#### 2. Prep tool

- `Cargo.toml` members gain `"fixtures"`.
- **File**: `fixtures/Cargo.toml` (new). Dependencies are `corral-dsp`,
  `hound = "3.5"` and `rubato` (the latest release).
  - Use its fixed-ratio offline resampler, 16 000 → 44 100.
  - Check the API name against the resolved version's docs.

**File**: `fixtures/src/main.rs` (new), with two subcommands.

- **`fixtures scan <reference.csv> [--len 8]`** helps choose excerpts. It:
  - prints the hop (from the first two rows);
  - for sliding windows of `len` seconds in 1 s steps, prints the start, the
    voiced %, the min and max voiced f0, and `OK` if every voiced frame is
    within 80–1000 Hz.
- **`fixtures build`** regenerates `corral-dsp/tests/fixtures/` from a
  manifest held in code:

```rust
enum Source {
    /// Excerpt a dataset WAV + its f0 CSV.
    Annotated { wav: &'static str, csv: &'static str, start_s: f64, len_s: f64 },
    /// NSynth held note: resample to 44.1 kHz, synthesise a constant-pitch reference.
    NSynth { wav: &'static str, midi: u8 },
}
struct Excerpt { name: &'static str, dataset: Dataset, source: Source, note: &'static str }
```

For each excerpt, it:

1. Finds the source files under `.datasets/` with a recursive search by
   file name, because the archives' top-level folders aren't known. It fails
   with a clear message if a file is missing.
2. **Annotated excerpts:**
   - Copy the samples in `[start, start+len)` with the source's own spec.
   - Write `<name>.f0.csv` with the header `time_s,f0_hz` and the rows in
     that range, with times shifted by `−start`.
   - Fail if any voiced reference frame falls outside 80–1000 Hz.
3. **NSynth:**
   - Resample the whole 4 s note to 44.1 kHz, and write 16-bit mono.
   - Write `<name>.f0.csv` with rows every 10 ms from 0.2 s to 2.8 s, at
     `440·2^((midi−69)/12)`. This covers the sustained part only, skipping
     the attack and the release, which starts at 3 s.
4. Writes `ATTRIBUTION.md`: one section per dataset (source URL, licence,
   citation), then a table of excerpts (name, source file, start/length,
   notes such as "clear vibrato", "track 35 — lowest average pitch"). The
   MDB section states that it is CC BY-NC, and that the fixtures must be
   replaced if the plugin is ever sold.
5. Prints the total size and fails if it is 5 MB or more.

Citations:

- **Vocadito:** Bittner et al. 2021, arXiv:2110.05580.
- **MDB-stem-synth:** Salamon, Bittner, Bonada, Bosch, Gómez and Bello,
  ISMIR 2017.
- **NSynth:** Engel et al. 2017, arXiv:1704.01279.

#### 3. mise task

**File**: `mise.toml`

```toml
[tasks.fixtures]
description = "Download the pitch datasets into .datasets/ (~2.2 GB, once) and regenerate the committed test fixtures"
run = '''
set -eu
mkdir -p .datasets
cd .datasets
fetch() { [ -f "$1" ] || { curl --fail --location --output "$1.part" "$2" && mv "$1.part" "$1"; }; }
fetch vocadito.zip 'https://zenodo.org/records/5578807/files/vocadito.zip?download=1'
fetch MDB-stem-synth.tar.gz 'https://zenodo.org/records/1481172/files/MDB-stem-synth.tar.gz?download=1'
fetch nsynth-test.jsonwav.tar.gz 'http://download.magenta.tensorflow.org/datasets/nsynth/nsynth-test.jsonwav.tar.gz'
[ "$(md5 -q vocadito.zip)" = dea40fd18f14d899643c4ba221b33a46 ] || { echo "vocadito.zip checksum mismatch" >&2; exit 1; }
[ -d vocadito ] || unzip -q vocadito.zip -x '__MACOSX/*' -d vocadito
[ -d mdb ] || { mkdir mdb && tar -xzf MDB-stem-synth.tar.gz -C mdb; }
[ -d nsynth ] || { mkdir nsynth && tar -xzf nsynth-test.jsonwav.tar.gz -C nsynth; }
cd ..
cargo run --release --quiet -p fixtures -- build
'''
```

- Only Vocadito's checksum is published. After the first download, record
  the SHA-256 of the other two archives in `ATTRIBUTION.md`.

#### 4. Choosing the excerpts

This is a one-off step. Record the choices in the manifest.

| Fixture | Dataset | How to choose |
|---|---|---|
| `vocadito_<n>_vibrato` | Vocadito | Listen to a few tracks, and pick an 8 s phrase with clear, sustained vibrato (the f0 CSV oscillates at about 5–7 Hz). `scan` must show `OK`. |
| `vocadito_<n>_low` | Vocadito | Take the lowest-`average_pitch` track (35, then 28, 4, 23…) that has an 8 s window with `scan` `OK`. |
| `mdb_<stem>_male` | MDB-stem-synth | A male-singer stem (`AClassicEducation_NightOwl_STEM_08` first), 8 s, `OK`, and ≥ 60% voiced. |
| `mdb_<stem>_low` | MDB-stem-synth | A cello or bassoon stem from the list in Key Discoveries. It needs an 8 s window with `OK` whose minimum voiced f0 is ≤ 90 Hz, so it reaches E2. |
| `nsynth_<note>` ×3 | NSynth test | One each at MIDI 40–43 (a string, guitar or reed), MIDI 60 (keyboard), and MIDI 76–83 (vocal or flute). Prefer velocity 100 or 127, found by filename (`ls .datasets/nsynth/**/audio/*-040-*`). Listen to check that each note is clean and in tune. |

The budget:

- 4 × 8 s × 88.2 kB/s ≈ 2.8 MB at 16-bit (or ≈ 4.2 MB if MDB is 24-bit).
- 3 NSynth notes ≈ 1.1 MB.
- If this goes over, shorten the excerpts to 6 s.

#### 5. Committed fixtures

`corral-dsp/tests/fixtures/`:

- 7 × `<name>.wav`;
- 7 × `<name>.f0.csv`;
- `ATTRIBUTION.md`.

#### 6. Accuracy test

**File**: `corral-dsp/tests/fixtures.rs` (new)

- Find every `*.wav` in `tests/fixtures/`, using
  `env!("CARGO_MANIFEST_DIR")`, and pair it with its `.f0.csv`.
- Assert the counts: vocadito 2, mdb 2, nsynth 3. If fixtures go missing,
  the test fails.
- For each one:
  - `read_wav_mono`, then `track`, then `eval::score`, with `max_gap_s`
    of hop/sr;
  - `print_scores`;
  - collect failures, and assert once at the end, so that a single run
    prints every fixture's scores.

Thresholds by prefix:

| Prefix | RPA | RCA − RPA | VFA |
|---|---|---|---|
| `nsynth_` | ≥ 0.95 | — | — |
| `mdb_` | ≥ 0.90 | ≤ 0.02 | — |
| `vocadito_` | ≥ 0.90 | ≤ 0.02 | ≤ 0.20 |

The thresholds are constants at the top of the file. Each has a comment
pointing to work item 0004's acceptance criteria. Any change must carry a
comment with the measured score and the reason.

#### 7. Harness fixture test

Add the fixture bit-identity test to `analyse/src/main.rs`, as described in
Phase 4.

#### 8. Tuning

If a threshold fails:

1. Tune `YIN_THRESHOLD` (0.10–0.20), `MIN_CONFIDENCE` (0.6–0.9) and
   `LEVEL_GATE_DBFS` (−60 to −40).
2. Re-run `synthetic.rs`, `fixtures.rs` and `latency.rs`, and keep the
   synthetic tests passing.
3. If some setting passes all of them, commit it. Note the final constants
   and scores in the README (Phase 7).
4. If no setting passes everything, stop. Report the best scores, then
   discuss with the developer whether to relax a threshold with a recorded
   reason or switch to MPM.

### Success Criteria:

#### Automated Verification:

- [x] `mise run fixtures` completes, and a second run doesn't download
      again
- [x] `git status --porcelain corral-dsp/tests/fixtures` is empty after a
      second `mise run fixtures`, so the output is reproducible
- [x] `du -ck corral-dsp/tests/fixtures/* | tail -1` is under 5120
- [x] `ls corral-dsp/tests/fixtures/*.wav | wc -l` prints 7
- [x] `git check-ignore .datasets` matches
- [x] `cargo test -p corral-dsp --test fixtures -- --nocapture` passes and
      prints 7 score lines
- [x] `cargo test -p analyse` passes, including the fixture bit-identity test
- [x] `mise run check` exits 0

#### Manual Verification:

- [ ] `ATTRIBUTION.md` names the source, licence and citation for each
      dataset, and its excerpt table matches the committed files
- [ ] `mise run analyse corral-dsp/tests/fixtures/<vocadito vibrato>.wav target/analyse/v.wav`
      writes a CSV with one row per hop. Plotting `f0_hz` for voiced rows
      against the reference CSV shows the vibrato tracked with no octave
      jumps.
- [ ] The vibrato excerpt audibly has vibrato. The low-singer excerpt is
      audibly a low voice. The MDB low excerpt audibly reaches around E2.

---

## Phase 6: Plugin Integration and Editor Readout

### Overview

Run `Chain` in `process()` and publish the note through an `AtomicU32`. Show
the note in the editor. Add an opt-in debug build that has nice-plug's
allocation assertions turned on.

### Changes Required:

#### 1. Feature

**File**: `corral/Cargo.toml`

```toml
[features]
# Debug builds only: aborts the host if process() allocates. See `mise run install-debug`.
assert_process_allocs = ["nice-plug/assert_process_allocs"]
```

#### 2. Plugin

**File**: `corral/src/lib.rs`

- **Imports.** `use std::sync::atomic::{AtomicU32, Ordering};` and
  `use corral_dsp::{chain::Chain, gain, note};`.
- **New fields on `Corral`:**

```rust
    /// The DSP chain. Rebuilt for the host's sample rate in `initialize()`.
    chain: Chain,
    /// The last hop's note, packed with `note::pack`; `note::UNVOICED` when
    /// nothing is detected. Shown by the editor.
    note_readout: Arc<AtomicU32>,
    /// The last value stored in `note_readout`.
    last_note_readout: u32,
```

- **`Default`.**
  - `chain: Chain::new(48_000.0)` is a placeholder that `initialize()`
    replaces. The host always calls `initialize()` before `process()`.
  - `note_readout: Arc::new(AtomicU32::new(note::UNVOICED))`, and
    `last_note_readout: note::UNVOICED`.
  - Pass `note_readout.clone()` to `CorralEditor::new`.
- **Add to `impl Plugin`**, matching nice-plug 0.4's `initialize` signature:

  > **Implementation note:** nice-plug 0.4.2 calls this hook `activate` with an
  > `ActivateContext` (`nice-plug-core-0.4.2/src/plugin.rs:246-258`), not
  > `initialize`/`InitContext`. It has the same contract: off the audio thread,
  > always followed by `reset()`. The code uses `activate`.

```rust
    fn initialize(
        &mut self,
        _audio_io_layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        _context: &mut impl InitContext<Self>,
    ) -> bool {
        // Allocates the detector's buffers and FFT plans, off the audio thread.
        self.chain = Chain::new(buffer_config.sample_rate);
        true
    }

    fn reset(&mut self) {
        self.chain.reset();
    }
```

- **`process()` becomes:**

```rust
        let mut last_gain = None;
        let mut last_detection = None;

        for mut channel_samples in buffer.iter_samples() {
            // One smoothed value per frame, shared by both channels.
            let gain = self.params.gain.smoothed.next();

            // Stereo in and out is the only layout, so the frame always has two channels.
            let mut frame = [0.0; 2];
            for (value, sample) in frame.iter_mut().zip(channel_samples.iter_mut()) {
                *value = *sample;
            }
            if let Some(detection) = self.chain.process_frame(&mut frame, gain) {
                last_detection = Some(detection);
            }
            for (sample, value) in channel_samples.iter_mut().zip(frame) {
                *sample = value;
            }
            last_gain = Some(gain);
        }

        // Share readouts with the editor once per buffer, only when they change.
        // They're stored even while the window is closed, so it's right when it
        // opens. Each is a single atomic store.
        let mut changed = false;
        if let Some(gain) = last_gain
            && gain != self.last_readout_gain
        {
            self.last_readout_gain = gain;
            self.readout_gain.store(gain, Ordering::Relaxed);
            changed = true;
        }
        if let Some(detection) = last_detection {
            let packed = note::pack(
                detection.voiced.then(|| note::nearest_note(detection.f0_hz)).flatten(),
            );
            if packed != self.last_note_readout {
                self.last_note_readout = packed;
                self.note_readout.store(packed, Ordering::Relaxed);
                changed = true;
            }
        }
        if changed && self.editor_state.is_open() {
            self.repaint_notifier.request_repaint();
        }
```

- A buffer with no hop boundary leaves the note as it was.
- The audio output is exactly `apply_gain(input, gain)`, as in 0003.

#### 3. Editor

**File**: `corral/src/editor.rs`

- Change the module doc to "…a Gain slider, a gain readout and the detected
  note."
- `WINDOW_SIZE` becomes `300.0 × 170.0`.
- Add a `note_readout: Arc<AtomicU32>` field, documented as "The packed note
  from the last analysis hop (`note::pack`)". Add it to `new`'s parameters.
- In `ui`:

```rust
        let note = note::unpack(self.note_readout.load(Ordering::Relaxed))
            .map_or_else(|| String::from("—"), note::format_note);
        …
            ui.label(format!("Gain: {readout} dB"));
            ui.label(egui::RichText::new(note).size(28.0).monospace());
```

- The em dash `—` is U+2014. egui's default fonts include it. If it renders
  as a box in Live, use `"-"` and record the change in the work item.

#### 4. Debug install task

**File**: `mise.toml`. Split `install` into hidden steps, so the debug
variant reuses them:

```toml
[tasks.require-live-closed]
hide = true
run = '''…the existing pgrep check, with the message naming `mise run install` or `install-debug`…'''

[tasks.copy-bundle]
hide = true
run = '''…the existing sign, verify, remove and copy script…'''

[tasks.install]
description = "(unchanged)"
run = [{ task = "require-live-closed" }, { task = "build-universal" }, { task = "copy-bundle" }]

[tasks.install-debug]
description = "Build a debug universal VST3 with nice-plug's allocation assertions (aborts Live if process() allocates), sign it and install it (quit Ableton Live first)"
run = [
  { task = "require-live-closed" },
  "cargo xtask bundle-universal corral --features assert_process_allocs",
  { task = "copy-bundle" },
]
```

If mise 2026.3.12 doesn't accept `hide = true`, drop the field. The tasks
still work and are just listed.

### Success Criteria:

#### Automated Verification:

- [x] `mise run check` exits 0
- [x] `mise run build-universal` succeeds, and
      `lipo -archs target/bundled/Corral.vst3/Contents/MacOS/Corral` prints
      `x86_64 arm64`
- [x] `cargo build -p corral --features assert_process_allocs` succeeds
- [x] The IDs are intact: `grep -c '#\[id = "gain"\]' corral/src/lib.rs` and
      `grep -c JCowieCorral0001 corral/src/lib.rs` each print 1
- [x] Reading `process()` shows no allocating or locking calls. The only
      calls are `smoothed.next()`, `Chain::process_frame`, `note::pack` and
      `nearest_note` (arithmetic only), two relaxed atomic stores, `is_open()`
      and `request_repaint()`.
- [x] `mise tasks` lists `install-debug`

#### Manual Verification:

- [ ] Covered in Phase 7.

---

## Phase 7: Documentation and Verification in Live 10

### Overview

Record the measurements and new tasks in the README, verify in Live 10 with
the debug and release builds, then tick the roadmap.

### Changes Required:

#### 1. README

**File**: `README.md`

- **Tasks table.** Add `analyse`, `fixtures` and `install-debug`, with
  descriptions matching `mise.toml` exactly.
- **New section "Pitch detection"**, after "Loading in Ableton Live 10":
  - The algorithm:
    - YIN, with its difference function computed through FFT (`realfft`,
      plans made in `initialize()`);
    - a mono sum of the input, taken before the gain;
    - the range, 80 Hz–1 kHz;
    - the final values of `YIN_THRESHOLD`, `MIN_CONFIDENCE` and
      `LEVEL_GATE_DBFS`.
  - A table with the window, hop and measured latency, filled in from
    `cargo test -p corral-dsp --test latency -- --nocapture`:

    | Sample rate | Window | Hop | Latency A3 | Latency E2 |
    |---|---|---|---|---|
    | 44.1 kHz | 2048 (46.4 ms) | 256 (5.8 ms) | … ms | … ms |
    | 48 kHz | 2048 (42.7 ms) | 256 (5.3 ms) | … ms | … ms |

    - Add one line explaining the definition: the time from onset to the
      first of 5 consecutive correct detections.
    - Add that the host buffer adds to these figures.
  - The fixture scores table, from
    `cargo test -p corral-dsp --test fixtures -- --nocapture`, plus the
    synthetic results.
  - How to run the harness and regenerate the fixtures. Mention that
    `.datasets/` takes about 2.2 GB, and link
    `corral-dsp/tests/fixtures/ATTRIBUTION.md`.
- **Licensing.** Add one bullet saying the test fixtures include CC BY-NC
  material (MDB-stem-synth), which is fine only while the project isn't
  sold.

#### 2. Roadmap

**File**: `ROADMAP.md`. Once the manual checks below pass, tick all eight
Phase 3 items.

### Success Criteria:

#### Automated Verification:

- [ ] `mise run check` exits 0
- [ ] `grep -c '| `analyse` |\|| `fixtures` |\|| `install-debug` |' README.md`
      prints 3
- [ ] `grep -n 'Latency' README.md` matches, and the table has no `…`
      placeholders left
- [ ] With Live closed, `mise run install-debug` and then `mise run install`
      both exit 0
- [ ] `sed -n '/## Phase 3/,/## Phase 4/p' ROADMAP.md | grep -c '\[x\]'`
      prints 8

#### Manual Verification:

With the **debug** build from `mise run install-debug`:

- [ ] Option-click Rescan, put Corral on an audio track, and sing and play
      into it for a minute. Open and close the window, move the Gain slider,
      and stop and start transport.
- [ ] Live doesn't crash or abort. That shows `process()` never allocated
      with the detector running.

Then with the **release** build from `mise run install`:

- [ ] The window shows the heading, the Gain slider, the `Gain: 0.00 dB`
      readout and `—` when there's no input.
- [ ] Sing a held A3 into a mic on the track (monitoring on). The editor
      shows `A3`, with cents within ±20 of a reference tuner (e.g. Live's
      Tuner device on the same track).
- [ ] Stop singing. The display goes to `—` within about 100 ms.
- [ ] Sing a scale. The note follows each step, without octave jumps on
      sustained notes.
- [ ] Set Gain to −inf. The audio is silent, but the note still tracks,
      because detection happens before the gain.
- [ ] No regression from 0003:
  - the slider, the readout and automation still work;
  - a set saved with a non-default Gain restores it;
  - opening and closing the window, and deleting the device with its window
    open, doesn't crash;
  - the audio at 0 dB is unchanged.
- [ ] Check at 44.1 kHz and at 48 kHz (Preferences → Audio → Sample Rate).
      The note tracks correctly at both.

---

## Testing Strategy

### Unit Tests:

- **`gain`:** unchanged from 0003.
- **`note`:**
  - the nearest note and cents across octaves 1–7;
  - exactly ±50 cents rounding consistently;
  - `"A4 +0¢"`;
  - sharps and octave boundaries;
  - invalid input;
  - `pack` and `unpack` round-trips.
- **`eval`:** each metric on hand-built references, the 50-cent edge, octave
  folding, empty denominators, gaps, and CSV parsing.
- **`pitch`:**
  - window and hop per sample rate;
  - hop cadence and `window_centre`;
  - sine accuracy;
  - `reset`;
  - non-finite input.
- **`analyse`:**
  - bit-identity per format and channel count;
  - gain;
  - the CSV shape;
  - rejected formats;
  - fixture bit-identity.

### Integration Tests (`corral-dsp/tests/`):

- **`synthetic.rs`:** scale, vibrato, E2 and B5 at 44.1 and 48 kHz; silence
  and white noise.
- **`fixtures.rs`:** all seven committed excerpts against the acceptance
  thresholds, with scores printed.
- **`no_alloc.rs`:** `Chain::process_frame` on voiced, noise and silence
  input, plus `reset`, under `assert_no_alloc` with the counting allocator.
- **`latency.rs`:** onset-to-stable-detection times, printed for the README.

### Manual Testing Steps:

1. Run `mise run fixtures` once, and listen to each excerpt.
2. Run `mise run analyse` on a fixture and plot the CSV against the
   reference.
3. Run `mise run install-debug`, then use the plugin in Live for a minute
   (the allocation check).
4. Run `mise run install`, then work through the Live checks in Phase 7.

## Performance Considerations

- **Per hop:** 2 forward and 1 inverse real FFTs of 4096, O(N) energy and
  CMND passes over ≤ 661 lags, and a 2048-sample copy.
  - That's about 172 hops/s at 44.1 kHz. It's negligible next to the host
    budget, even in the opt-level 1 debug build.
- **Below the level gate,** YIN is skipped. That lowers the average cost, but
  the worst case (every hop voiced) is what the host sees.
- **Per sample:** a sum, a ring-buffer write and a counter. The gain path is
  unchanged.
- **Editor:** it repaints only when the packed note or the gain changes. That
  is at most once per buffer, and only while the window is open.
- **Test time:** the dev profile has `opt-level = 1`. The synthetic and
  fixture audio totals about 2 minutes, which is about 20k hops, so the
  added test time should be seconds.

## Migration Notes

- The parameter ID `gain` and the class ID `JCowieCorral0001` are unchanged,
  so saved sets still load.
- The editor window grows to 300×170. The size isn't saved.
- Install the debug build only when checking allocations. `mise run install`
  replaces it with the release build.
- Contributors need `mise run fixtures`, and about 2.2 GB, only to
  *regenerate* fixtures. `mise run check` uses the committed excerpts.

## References

- Work item: `meta/work/0004-realtime-pitch-detection-with-offline-test-harness.md`
- Previous plan: `meta/plans/2026-09-26-0003-tested-gain-dsp-and-basic-plugin-gui.md`
- Plugin: `corral/src/lib.rs:118-146` (process and readout pattern),
  `corral/src/editor.rs`
- nice-plug 0.4.2:
  - `src/wrapper/util.rs:30-32, 193-203` (`assert_process_allocs`)
  - nice-plug-core `src/buffer/samples.rs:119-165` (`ChannelSamples`)
- nice-plug-xtask 0.1.1: `src/lib.rs:112-135` (extra args are passed to
  cargo)
- YIN: de Cheveigné & Kawahara (2002), JASA 111(4). FFT formulation:
  https://www.hyuncat.com/blog/yin/
- realfft: https://docs.rs/realfft (`RealToComplex::process_with_scratch`)
- hound: https://github.com/ruuda/hound
- nice-assert-no-alloc: https://github.com/BillyDM/nice-assert-no-alloc
- Vocadito: https://zenodo.org/records/5578807
- MDB-stem-synth: https://zenodo.org/records/1481172
- MedleyDB metadata, which gives each stem's instrument:
  https://github.com/marl/medleydb/tree/master/medleydb/data/Metadata
- NSynth: https://magenta.tensorflow.org/datasets/nsynth
- mir_eval melody: https://mir-eval.readthedocs.io/latest/api/melody.html
