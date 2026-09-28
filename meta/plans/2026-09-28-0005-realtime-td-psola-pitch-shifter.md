---
type: plan
id: "2026-09-28-0005-realtime-td-psola-pitch-shifter"
title: "Realtime TD-PSOLA Pitch Shifter Implementation Plan"
date: "2026-09-28T11:34:55+00:00"
author: John Cowie Del Corral
producer: create-plan
status: draft
work_item_id: "work-item:0005"
parent: "work-item:0005"
tags: [phase-4, dsp, pitch-shifting, td-psola, latency, testing, docs]
revision: "c08365a415ea52c610d744a0098d5e8dd0aa5860"
repository: "rust-vst"
last_updated: "2026-09-28T11:34:55+00:00"
last_updated_by: John Cowie Del Corral
schema_version: 1
---

# Realtime TD-PSOLA Pitch Shifter Implementation Plan

## Overview

This plan delivers Phase 4 of `ROADMAP.md` (work item 0005):

- a causal, streaming TD-PSOLA shifter in `corral-dsp`, driven by the
  existing YIN tracker's period and voicing gate;
- a fixed latency of `ceil(2 · sr / 80)` samples (25 ms), reported to the
  host once per activation, with the dry path delayed to match;
- `Shift` (whole semitones, −12..=+12, default +4) and `Mix` (0–100 %,
  default 50 %) parameters, in the plugin, the editor and the `analyse`
  harness;
- objective tests (output pitch, identity, latency, level, clicks,
  allocation) that print their scores;
- `docs/td-psola.md`, README and ROADMAP updates, and listening checks in
  Live 10.

Decisions made with the developer during planning:

- **Harness alignment:** `analyse` compensates its own latency. It feeds L
  zeros after the input and drops the first L output samples, so the output
  lines up with the input and is the same length. At `--mix 0` it is
  bit-identical to the input.
- **Fixture criterion:** the headline score uses every voiced reference
  frame, as the work item says. A diagnostic score over the frames the
  chain itself voiced is printed alongside. If the headline misses 0.90,
  the threshold is lowered to just below the measured value, with a comment
  recording the score and the reason.
- **Phasing:** seven phases, as below.

## Current State Analysis

- `corral-dsp/src/chain.rs:37-44`: `Chain::process_frame(frame, gain)` sends
  the channel mean (pre-gain) to `PitchTracker::push`, applies
  `gain::apply_gain` to every channel in place, and returns the hop's
  `Option<Detection>`. Nothing is delayed.
- `corral-dsp/src/pitch.rs`:
  - `:11-12`: `MIN_F0_HZ = 80.0`, `MAX_F0_HZ = 1000.0`.
  - `:221-229`: the window is 2048 at ≤ 48 kHz, doubling per further multiple
    of 48 kHz. The hop is window / 8.
  - `:286-291`: `Detection { f0_hz, confidence, voiced, window_centre }`.
    `window_centre = samples_seen − window/2`, so it is 1024 samples behind
    the newest input at 44.1/48 kHz.
- `corral/src/lib.rs`:
  - `:38-43`: `CorralParams` has only `gain` (id `"gain"`).
  - `:121-130`: `activate()` rebuilds `Chain` for the host's sample rate and
    ignores `_context`.
  - `:145-161`: `process()` copies each frame into `[f32; 2]`, calls
    `process_frame`, and copies it back.
- `corral/src/editor.rs:16`: a 300×170 window with a heading, the Gain
  slider, the gain readout and the note readout.
- `analyse/src/main.rs`:
  - `:26-60`: `parse_args` accepts `--gain-db` and one or two paths.
  - `:109-179`: `run()` converts to f32, runs `Chain` frame by frame, writes
    the WAV in the input's format and the per-hop CSV.
  - Tests at `:298-338` and `:390-428` assert bit-identical output at 0 dB,
    including every committed fixture.
- `corral-dsp/tests/`:
  - `common/mod.rs`: `harmonic_tone`, `white_noise`, `midi_to_hz`, `track`,
    `print_scores`, `read_wav_mono`. `track` calls
    `process_frame(&mut [s], 1.0)`.
  - `no_alloc.rs`: one test, `AllocDisabler` as the global allocator.
  - `latency.rs`, `synthetic.rs`, `fixtures.rs`: detector tests.
- `corral-dsp/src/eval.rs`: `RefFrame`, `EstFrame`, `score` (RPA, RCA, VR,
  VFA at 50 ¢), `parse_reference_csv`.
- nice-plug 0.4.2:
  - `ActivateContext::set_latency_samples(u32)` exists
    (`nice-plug-core-0.4.2/src/context/activate.rs:28`).
  - In VST3, `set_latency_samples` only triggers
    `restartComponent(kLatencyChanged)` when the value changes
    (`nice-plug-0.4.2/src/wrapper/vst3/inner.rs:642-648`).
  - `IntParam` (`IntRange::Linear { min, max }`), `SmoothingStyle::Linear`,
    and `formatters::v2s_f32_percentage` / `s2v_f32_percentage` are
    available.

## Desired End State

- **Latency:** `corral_dsp::psola::latency_samples(sr) = ceil(2 · sr / 80)`:
  1103 samples (25.0 ms) at 44.1 kHz, 1200 (25.0 ms) at 48 kHz. `Chain`
  delays everything by exactly this. The plugin reports it in `activate()`.
  It doesn't depend on pitch, `Shift` or `Mix`.
- **Chain:** `Chain::process_frame(frame, Settings { gain, mix, shift })`.
  Per frame:
  - the mono mean goes to the tracker (unchanged) and to the shifter's input
    ring;
  - each channel goes through an L-sample delay line;
  - a wet gate chooses between the delayed dry channel and the shifted mono
    signal, with a 10 ms equal-power crossfade and 2-hop hysteresis;
  - `Mix` crossfades linearly between delayed dry and wet;
  - gain is applied last.
- **Identity:** when the gate is fully dry, or `mix == 0.0`, each output
  sample is the delayed input sample (times the gain), bit for bit.
- **Shifter:** TD-PSOLA with:
  - analysis marks on signal peaks, tracked at previous mark + P ± P/4;
  - Hann grains of 2P centred on analysis marks;
  - synthesis marks spaced P / ratio (P here being the local mark spacing;
    see Phase 4 results);
  - overlap-add normalised by the window sum (floored at 1).
  P comes from a period smoother that rejects octave jumps unless they
  persist for 3 hops. `Shift` changes glide over 50 ms.
- **Realtime safety:** everything is allocated in `Chain::new`, and
  `no_alloc.rs` covers shifting, mixing and `Shift` changes.
- **Plugin and editor:** the plugin has `Shift` (id `"shift"`) and `Mix`
  (id `"mix"`), and the editor shows their sliders.
- **Harness:** `mise run analyse in.wav out.wav --shift 4 --mix 100` writes a
  latency-compensated WAV the same length as the input, plus the CSV.
- **Tests:** `mise run check` passes, and the new tests print their scores.
- **Docs:** `docs/td-psola.md` exists. README records latency and
  parameters, and ROADMAP Phase 4 is ticked.

Verify with:

```sh
mise run check
cargo test -p corral-dsp --test shifter --test latency --test fixtures -- --nocapture
mise run analyse corral-dsp/tests/fixtures/vocadito_7_vibrato.wav target/analyse/v.wav --shift 4 --mix 100
```

### Key Discoveries:

- **The detector is already time-aligned with the output.** The output
  sample at input time n is o = n − L. The latest detection is centred at
  n − 1024. With L = 1103 (44.1 kHz) or 1200 (48 kHz), the period and
  voicing estimate describe audio 79 or 176 samples ahead of the output
  sample, which is less than one hop. So:
  - voicing and period decisions are applied to the output as soon as they
    arrive, with no extra delay;
  - the "stale period" assumption in the work item costs almost nothing.
  At 96 kHz, L = 2400 and the centre is 2048 behind, so the same holds.
- **Why L = 2·P_max, not less.**
  - Grains are read sample by sample from a mono input ring. Grain k reads
    input `t_a + (o − t_s)`, which is at or before n as long as
    t_a − t_s < L. So reading grains never limits latency.
  - The limit is choosing the analysis mark when a grain *starts*, at
    output t_s − P. A mark is confirmed once the input reaches
    mark + 1.25P (the ±P/4 search window). So at grain start the confirmed
    marks reach at least t_s − P + L − 1.25P.
  - With L = 2·P_max, that is ≥ t_s − 0.25P at 80 Hz, and more at higher
    pitches. Lower latencies would force grains to use marks up to a whole
    period early near E2.
- **Normalisation.** Hann grains of length 2P overlap-added at spacing P
  sum to 1. At spacing P / r they sum to about r, so upward shifts get
  louder. Dividing each output sample by `max(window_sum, 1)` keeps the
  level for r ≥ 1 and never amplifies the gaps at r < 1.
- **Signed zero.** `dry + mix · (wet − dry)` turns −0.0 into +0.0, which
  breaks bit-identity. So the chain short-circuits to `dry` when
  `mix == 0.0` or the gate is fully dry.
- **Test edges.** The detector voices no hop until its 2048-sample window
  has filled (46 ms). The wet gate then needs 2 hops of hysteresis and a
  10 ms fade. So the work item's "ignore 50 ms at each voicing edge" can't
  cover the start of a signal. The tests exclude 150 ms at the start and
  end of a signal, and 50 ms either side of each scale step. This is a
  recorded adjustment, as the work item's drafting notes allow.
- **The existing harness tests assert bit-identity at 0 dB.** With latency
  compensation and `--mix 0` they keep their meaning. Only the call sites
  change.

## What We're NOT Doing

- Key and scale intervals, note stabilisation, glide between harmony notes
  (Phase 5).
- Multiple voices, formant correction, humanisation, presets (Phase 6).
- Latency that changes with pitch, or changing latency at runtime.
- Polyphonic input.
- Fractional or continuous `Shift` values (whole semitones only).
- A phase-vocoder implementation (it appears only in the design doc).
- Linking or vendoring `maxrmorrison/psola` (GPL). It may be used offline,
  by ear, as a reference.
- Allocation assertions in Live (debug builds still abort in Live 10).
  `no_alloc.rs` covers realtime safety.
- Changing the detector's window, thresholds or gate.

## Implementation Approach

Build from the outside in, so every phase ends green:

1. Plumb latency, the dry delay and `Mix` through `Chain` with a wet signal
   equal to the delayed dry. Identity and latency tests pass immediately,
   and the API is stable for the harness, plugin and tests.
2. Add the pure, unit-testable pieces: period smoothing, semitone ramp, wet
   gate.
3. Add TD-PSOLA and wire it into `Chain`.
4. Add the objective test suite, then tune constants against it and record
   the numbers.
5–6. Expose it in the harness, then in the plugin and editor.
7. Write the docs and verify in Live 10.

Constants are named at the top of each module, with a comment saying what
they were tuned against. The tuned values are:

| Constant | Value | Where |
|---|---|---|
| `LATENCY_PERIODS` | 2.0 (× P_max) | `psola.rs` |
| `MARK_SEARCH` | 0.25 (± P) | `psola.rs` |
| `MAX_GRAINS` | 8 | `psola.rs` |
| `CROSSFADE_S` | 0.010 | `wet_gate.rs` |
| `OPEN_HOPS` / `CLOSE_HOPS` | 2 / 2 | `wet_gate.rs` |
| `SHIFT_RAMP_S` | 0.050 | `shift.rs` |
| `OCTAVE_PERSIST_HOPS` | 3 | `period.rs` |
| `NOTE_CHANGE_CENTS` | 100 | `period.rs` |
| `SMOOTHING` | 0.5 per hop | `period.rs` |

---

## Phase 1: Latency, Dry Delay and Mix Plumbing

### Overview

Give `Chain` its final shape: fixed latency, a stereo dry delay, and a
`Settings` argument carrying gain, mix and shift. The wet path is the
delayed dry signal for now, so the output is the input delayed by L at any
mix. Update every caller.

### Changes Required:

#### 1. Delay line

**File**: `corral-dsp/src/delay.rs` (new)

```rust
//! A fixed multi-channel delay, allocated once.

pub struct DelayLine {
    /// `channels × len` samples, channel-major.
    buffer: Vec<f32>,
    len: usize,
    pos: usize,
    channels: usize,
}

impl DelayLine {
    /// Allocates. `len` may be 0 (no delay).
    pub fn new(channels: usize, len: usize) -> Self { … }
    /// Zeroes the history; doesn't allocate.
    pub fn reset(&mut self) { … }
    /// Delays one frame in place. `frame.len()` must be ≤ `channels`.
    /// Samples are copied, never computed, so the output is bit-identical
    /// to the input `len` frames earlier (NaN included).
    #[inline]
    pub fn process(&mut self, frame: &mut [f32]) { … }
}
```

Unit tests: an impulse comes out exactly `len` frames later on each channel;
channels don't mix; `len = 0` is the identity; `reset` clears the history;
NaN and −0.0 come out bit for bit.

#### 2. Latency

**File**: `corral-dsp/src/psola.rs` (new; the shifter joins it in Phase 3)

```rust
//! TD-PSOLA pitch shifting. See docs/td-psola.md.

use crate::pitch::MIN_F0_HZ;

/// Latency in periods of the lowest detectable pitch. A 2P grain centred on
/// a mark needs P samples after the mark, and the mark needs up to 1.25P of
/// lookahead to be confirmed; see the plan for 0005.
const LATENCY_PERIODS: f32 = 2.0;

/// The chain's fixed latency. Depends only on the sample rate.
pub fn latency_samples(sample_rate: f32) -> usize {
    (LATENCY_PERIODS * sample_rate / MIN_F0_HZ).ceil() as usize
}
```

Unit test: 1103 at 44.1 kHz, 1200 at 48 kHz, 2400 at 96 kHz.

#### 3. Chain settings and mix

**File**: `corral-dsp/src/chain.rs`

- Update the module doc: mono sum → tracker and shifter; each channel →
  delay; wet gate; mix; gain.
- Add:

```rust
/// Per-frame controls. The plugin smooths `gain` and `mix`; `Chain` ramps
/// `shift` itself.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Settings {
    /// Linear gain, applied last.
    pub gain: f32,
    /// 0.0 = dry only, 1.0 = wet only.
    pub mix: f32,
    /// Whole semitones, clamped to `shift::MIN..=shift::MAX`.
    pub shift: i32,
}
```

- `Chain` gains `delay: DelayLine` (2 channels, `latency_samples(sr)`) and
  `latency: usize`. It also gets `pub fn latency(&self) -> usize`.
  `reset()` also resets the delay.
- `process_frame(&mut self, frame: &mut [f32], settings: Settings) -> Option<Detection>`:

```rust
let mono = frame.iter().sum::<f32>() / frame.len() as f32;
let detection = self.tracker.push(mono);
self.delay.process(frame); // frame now holds the delayed dry signal
// Phase 3 replaces this with the wet gate and shifter.
for sample in frame.iter_mut() {
    *sample = gain::apply_gain(*sample, settings.gain);
}
detection
```

  At this stage `mix` and `shift` are unused apart from a debug-assert
  that `mix` is within 0.0..=1.0. The wet path equals the dry path, so any
  mix gives the delayed dry signal.
- Add `pub mod delay;` and `pub mod psola;` to `corral-dsp/src/lib.rs`.

#### 4. Callers

- `corral-dsp/tests/common/mod.rs`: `track` passes
  `Settings { gain: 1.0, mix: 0.0, shift: 0 }`. Add
  `pub fn process(signal: &[f32], sample_rate: f32, settings: impl Fn(usize) -> Settings) -> (Vec<f32>, Vec<Detection>)`.
  It returns the mono output (same length as the input, not compensated)
  and every detection, so later tests can vary settings per sample.
- `corral-dsp/tests/latency.rs`, `no_alloc.rs`: pass `Settings`.
- `analyse/src/main.rs`: pass `Settings { gain, mix: 0.0, shift: 0 }` for
  now. Phase 5 adds flags and compensation. Until then the harness output
  is delayed by L, so mark the bit-identity tests `#[ignore = "restored in
  phase 5 (latency compensation)"]` for this phase only.
- `corral/src/lib.rs`: pass `Settings { gain, mix: 0.0, shift: 0 }` for now.
  Phase 6 adds the parameters and latency reporting.

#### 5. Identity and delay tests

**File**: `corral-dsp/tests/shifter.rs` (new; grows in Phase 4)

- `mix_zero_is_the_delayed_input`: harmonic A3, white noise and silence at
  44.1 and 48 kHz, stereo frames with different L/R content. Output equals
  input delayed by `chain.latency()`, compared bit for bit
  (`f32::to_bits`), with zeros for the first L frames.

### Success Criteria:

#### Automated Verification:

- [x] `mise run check` passes (the harness bit-identity tests are ignored
      for this phase only).
- [x] `cargo test -p corral-dsp delay` and `cargo test -p corral-dsp psola`
      pass.
- [x] `cargo test -p corral-dsp --test shifter` passes.
- [x] `cargo test -p corral-dsp --test no_alloc` passes.
- [x] `grep -r nice_plug corral-dsp/` finds nothing.

#### Manual Verification:

- [x] None. The plugin is unchanged apart from an unreported 25 ms delay;
      don't install this build.

---

## Phase 2: Period Smoother, Semitone Ramp and Wet Gate

### Overview

The three small, pure state machines that sit between the detector and the
shifter, each fully unit-tested.

### Changes Required:

#### 1. Period smoother

**File**: `corral-dsp/src/period.rs` (new)

```rust
//! Turns per-hop YIN estimates into the period the shifter uses: smoothed,
//! with octave jumps rejected unless they persist.

/// A jump within this many cents of ±1200 is an octave candidate.
const OCTAVE_WINDOW_CENTS: f32 = 150.0;
/// An octave jump is accepted once this many consecutive hops agree.
const OCTAVE_PERSIST_HOPS: u32 = 3;
/// Any other jump bigger than this is a note change and is taken at once.
const NOTE_CHANGE_CENTS: f32 = 100.0;
/// One-pole coefficient per hop, in the log-period domain, for smaller moves.
const SMOOTHING: f32 = 0.5;

pub struct PeriodSmoother { sample_rate: f32, period: Option<f32>, candidate: Option<f32>, candidate_hops: u32 }

impl PeriodSmoother {
    pub fn new(sample_rate: f32) -> Self;
    pub fn reset(&mut self);
    /// Feeds one hop. Unvoiced hops leave the period unchanged.
    pub fn update(&mut self, detection: &Detection);
    /// The period in samples, or `None` before the first voiced hop.
    pub fn period(&self) -> Option<f32>;
}
```

Rules for a voiced hop with period p = sr / f0, given the current period S:

1. S is `None`: set S = p.
2. c = 1200 · log2(p / S). If |c| is within `OCTAVE_WINDOW_CENTS` of 1200,
   it is an octave candidate. Count consecutive candidate hops (with the
   candidate's own period within 100 ¢ of the previous candidate). Once the
   count reaches `OCTAVE_PERSIST_HOPS`, set S = p. Otherwise leave S alone.
3. Otherwise, clear the candidate. If |c| > `NOTE_CHANGE_CENTS`, set S = p.
   Otherwise S = exp(ln S + `SMOOTHING` · (ln p − ln S)).

Unit tests:
- a steady 220 Hz converges within 0.1 ¢;
- a 2-semitone step is taken on the first hop;
- a one-hop or two-hop octave glitch (×2 and ×0.5) is ignored;
- three hops of a new octave are accepted;
- unvoiced hops hold the period;
- `reset` clears it.

#### 2. Shift maths, ramp and display

**File**: `corral-dsp/src/shift.rs` (new)

```rust
//! The Shift parameter: semitones → ratio, a click-free ramp, and display.

pub const MIN: i32 = -12;
pub const MAX: i32 = 12;
pub const DEFAULT: i32 = 4;
/// Time for any change of Shift to complete. Avoids clicks and zipper noise;
/// not a musical glide (that is Phase 5).
const SHIFT_RAMP_S: f32 = 0.050;

/// 2^(semitones/12).
pub fn ratio(semitones: f32) -> f32;

/// Linear ramp in semitones toward the target. Once per sample.
pub struct SemitoneRamp { current: f32, target: f32, step: f32 }
impl SemitoneRamp {
    pub fn new(sample_rate: f32, initial: i32) -> Self;
    /// Jumps to `target` with no ramp (used on reset).
    pub fn reset(&mut self, target: i32);
    pub fn next(&mut self, target: i32) -> f32;
}

/// "+4", "0", "-12" (no unit; the parameter adds " st").
pub fn format_shift(semitones: i32) -> String;
/// Accepts "+4", "4", "-5", " 7 st"; rejects out-of-range and non-integers.
pub fn parse_shift(text: &str) -> Option<i32>;
```

Every change takes `SHIFT_RAMP_S`, whatever its size. When `next` sees a new
target, it sets `step = |target − current| / (SHIFT_RAMP_S · sr)`. It then
moves `current` by `step` per sample until it lands exactly on the target.

Unit tests:
- `ratio(12) == 2`, `ratio(-12) == 0.5`, `ratio(0) == 1`;
- the ramp reaches any target in `SHIFT_RAMP_S · sr` samples ± 1 and is
  monotonic;
- the format/parse round trip covers −12..=12;
- out-of-range and fractional input is rejected.

#### 3. Wet gate

**File**: `corral-dsp/src/wet_gate.rs` (new)

```rust
//! Chooses between the delayed dry signal and the shifted signal, with
//! hysteresis on the voicing decision and an equal-power crossfade.

const CROSSFADE_S: f32 = 0.010;
/// Consecutive voiced hops needed to open, and unvoiced hops to close.
const OPEN_HOPS: u32 = 2;
const CLOSE_HOPS: u32 = 2;

pub struct WetGate { open: bool, run: u32, position: f32, step: f32 }

impl WetGate {
    pub fn new(sample_rate: f32) -> Self;
    pub fn reset(&mut self);
    /// Feeds one hop's voicing decision.
    pub fn update(&mut self, voiced: bool);
    /// Whether the gate wants the shifted signal (after hysteresis).
    pub fn is_open(&self) -> bool;
    /// True while any shifted signal is audible (open or still fading out).
    pub fn is_active(&self) -> bool;
    /// Advances the fade by one sample. Returns (dry_gain, shifted_gain),
    /// with cos/sin of position · π/2. (Named `next_gains`, not `next`, so
    /// clippy doesn't mistake it for `Iterator::next`.)
    pub fn next_gains(&mut self) -> (f32, f32);
}
```

Unit tests:
- one voiced hop doesn't open it, two do;
- one unvoiced hop doesn't close it, two do;
- the fade takes `CROSSFADE_S · sr` samples ± 1;
- dry² + shifted² = 1 within 1e-6 throughout;
- at rest closed, `next_gains()` returns exactly `(1.0, 0.0)` and `is_active()` is
  false;
- alternating voiced/unvoiced hops never open it.

Add `pub mod period; pub mod shift; pub mod wet_gate;` to `lib.rs`.

### Success Criteria:

#### Automated Verification:

- [x] `cargo test -p corral-dsp period shift wet_gate` passes.
- [x] `mise run check` passes.

#### Manual Verification:

- [x] None.

---

## Phase 3: TD-PSOLA Shifter and Chain Integration

### Overview

Implement the streaming shifter and connect the Phase 2 pieces in `Chain`.
After this phase the chain shifts pitch.

### Changes Required:

#### 1. The shifter

**File**: `corral-dsp/src/psola.rs`

All time indices are `i64` input-sample indices since reset. The newest
input is `n = written − 1`, and the output being produced is `o = n − L`.

```rust
/// ± this fraction of a period around previous mark + P is searched for the next mark.
const MARK_SEARCH: f32 = 0.25;
/// Grains alive at once: 2P / (P / r_max) + 1 = 5 at r = 2; 8 leaves headroom.
const MAX_GRAINS: usize = 8;
/// Confirmed marks kept (from o − 2·P_max to n); sized for P_min at 1 kHz.
const MAX_MARKS: usize = 128;

#[derive(Clone, Copy, Default)]
struct Grain { centre: f64, source: i64, half: f32, live: bool }

pub struct Psola {
    latency: i64,
    /// Mono input, power-of-two ring ≥ L + 2·P_max + 4.
    input: Vec<f32>,
    mask: usize,
    written: i64,
    /// Confirmed analysis marks, oldest first (fixed-size ring).
    marks: [i64; MAX_MARKS], mark_head: usize, mark_len: usize,
    /// +1.0 or −1.0: marks sit on peaks of this sign.
    polarity: f32,
    grains: [Grain; MAX_GRAINS],
    /// Output time of the next synthesis mark.
    next_synthesis: f64,
    running: bool,
}

impl Psola {
    /// Allocates. Call from `Chain::new`.
    pub fn new(sample_rate: f32) -> Self;
    pub fn reset(&mut self);
    /// Stores one mono input sample (non-finite → 0.0). If running, confirms
    /// any marks now within reach: while last + P + ⌈MARK_SEARCH·P⌉ ≤ n,
    /// push argmax(polarity · x) over last + P ± MARK_SEARCH·P.
    pub fn push(&mut self, sample: f32, period: f32);
    /// Starts shifting at the current output time. The first mark is
    /// argmax |x| over [o − ⌈P⌉, o]. Its sign sets `polarity`. Then marks are
    /// confirmed up to n (a bounded catch-up burst of at most ~(L + P) / P
    /// searches). `next_synthesis` = that first mark.
    pub fn start(&mut self, period: f32);
    /// Stops and forgets marks and grains.
    pub fn stop(&mut self);
    pub fn is_running(&self) -> bool;
    /// The shifted sample for output time o.
    pub fn next(&mut self, period: f32, ratio: f32) -> f32;
}
```

`next(period P, ratio r)`:

1. **Spawn.** While `o ≥ next_synthesis − P`, spawn a grain:
   - `centre = next_synthesis`;
   - `source` = the confirmed mark nearest to `centre`;
   - `half = P`.
   Put it in a free slot. If there is none, reuse the grain with the
   smallest `centre`; that should never happen, and a debug-assert says so.
   Then `next_synthesis += P / r`.
2. **Overlap-add.** For each live grain with |o − centre| < half:
   - `w = 0.5 + 0.5·cos(π·(o − centre)/half)`;
   - read input at `source + (o − centre)` with linear interpolation, with
     the index clamped to ≤ n;
   - `acc += w·x; wsum += w`.
   Retire grains with `o ≥ centre + half`.
3. **Prune** marks older than `o − 2·⌈P_max⌉`.
4. Return `acc / wsum.max(1.0)`.

The mark ring and input ring are sized from `P_max = sr / MIN_F0_HZ` and
`P_min = sr / (MAX_F0_HZ · 1.03)`. Nothing grows after `new`.

Unit tests (in `psola.rs`):
- the latency and ring sizes at 44.1/48/96 kHz;
- on a 220 Hz harmonic tone, confirmed marks are spaced sr/220 ± 1 sample
  and sit on positive (or all negative) peaks;
- **ratio 1 reconstructs the input.** On a steady harmonic tone with P
  fixed, output vs input delayed by L has SNR ≥ 20 dB after the first 2P
  (print the value);
- at ratio 2 and P at 1 kHz, the number of live grains never exceeds 5;
- non-finite input gives finite output;
- `stop` then `start` works without stale grains.

#### 2. Chain integration

**File**: `corral-dsp/src/chain.rs`

`Chain` gains `smoother: PeriodSmoother`, `ramp: SemitoneRamp`,
`gate: WetGate` and `psola: Psola`. `reset()` resets all of them, and the
ramp resets to `shift::DEFAULT`. `process_frame`:

```rust
let mono = frame.iter().sum::<f32>() / frame.len() as f32;
let detection = self.tracker.push(mono);
if let Some(d) = &detection {
    self.smoother.update(d);
    self.gate.update(d.voiced);
}
let semitones = self.ramp.next(settings.shift.clamp(shift::MIN, shift::MAX));
let period = self.smoother.period();

// Keep the shifter's state in step with the gate.
match (self.gate.is_active(), self.psola.is_running(), period) {
    (true, false, Some(p)) => self.psola.start(p),
    (false, true, _) => self.psola.stop(),
    _ => {}
}
self.psola.push(mono, period.unwrap_or(0.0));

self.delay.process(frame); // delayed dry
if self.psola.is_running() {
    let shifted = self.psola.next(period.expect("running implies a period"), shift::ratio(semitones));
    let (dry_gain, shifted_gain) = self.gate.next_gains();
    if settings.mix != 0.0 {
        for dry in frame.iter_mut() {
            let wet = dry_gain * *dry + shifted_gain * shifted;
            *dry += settings.mix * (wet - *dry);
        }
    }
} else {
    self.gate.next_gains(); // keep the fade clock in step; returns (1, 0) at rest
}
for sample in frame.iter_mut() {
    *sample = gain::apply_gain(*sample, settings.gain);
}
detection
```

The gate opens only on a voiced hop, and the smoother has a period after
any voiced hop, so `start` always has a period. `psola.push` ignores
`period` while stopped.

### Success Criteria:

#### Automated Verification:

- [x] `cargo test -p corral-dsp psola` passes and prints the ratio-1 SNR.
- [x] `cargo test -p corral-dsp --test shifter` (the Phase 1 identity test)
      still passes, bit for bit.
- [x] `cargo test -p corral-dsp --test no_alloc` passes.
- [x] `mise run check` passes.

#### Manual Verification:

- [ ] Temporarily running `common::process` on a harmonic A3 at `shift: 4,
      mix: 1.0` and writing the result with hound shows a C#4 tone by ear.
      Optional; Phase 5's harness makes this routine.

---

## Phase 4: Objective Tests and Tuning

### Overview

Add the work item's measurable criteria as `cargo test` tests that print
their numbers. Then tune the Phase 2/3 constants against them and record
the final scores.

### Changes Required:

#### 1. Shifted-pitch metrics

**File**: `corral-dsp/src/eval.rs`

```rust
#[derive(Clone, Copy, Debug)]
pub struct CentsStats {
    /// Median signed error over voiced reference frames with an estimate (f0 > 0).
    pub median_cents: Option<f64>,
    /// Share of *all* voiced reference frames within `tolerance_cents`
    /// (a missing or zero estimate is a miss).
    pub share_within: Option<f64>,
    pub voiced_frames: usize,
}

/// Compares estimates with reference × `ratio`. Estimate times are taken
/// as-is: callers subtract the chain latency first.
pub fn cents_stats(reference: &[RefFrame], estimates: &[EstFrame], max_gap_s: f64,
                   ratio: f64, tolerance_cents: f64) -> CentsStats;
```

It reuses `nearest`. Unit tests cover:
- a perfect shifted estimate → median 0, share 1;
- a constant +20 ¢ offset → median 20, share 0 at ±10 ¢;
- missing estimates count as misses in `share_within` but not in the median;
- unvoiced reference frames are ignored;
- an empty input gives `None`.

#### 2. Test helpers

**File**: `corral-dsp/tests/common/mod.rs`

- `pub fn output_estimates(output: &[f32], sample_rate: f32, latency: usize) -> Vec<EstFrame>`:
  `track(output)` with each time shifted by −latency / sr.
- `pub fn print_cents(name: &str, stats: &CentsStats)`, in the fixed format
  `name  median +3.2¢  within 0.987  (voiced 380)`.
- `pub fn rms(signal: &[f32]) -> f64`.
- `pub fn max_jump(signal: &[f32]) -> f32`: the largest |x[i] − x[i−1]|.

#### 3. The tests

**File**: `corral-dsp/tests/shifter.rs`

Constants at the top, with the work item's values. Any change carries a
comment with the measured score and the reason.

```rust
const MEDIAN_CENTS: f64 = 10.0;          // +4, tones and scale
const SHARE_25_CENTS: f64 = 0.95;        // +4, tones and scale
const OTHER_SHIFTS_MEDIAN_CENTS: f64 = 15.0;
const LEVEL_DB: f64 = 1.5;
/// Reference frames within this of the signal's start or end are left out:
/// the detector needs 46 ms to fill, then 2 hops of hysteresis and a 10 ms
/// fade (the work item's 50 ms can't cover that; recorded in the plan).
const EDGE_S: f64 = 0.15;
/// …and within this of a scale step.
const STEP_EXCLUSION_S: f64 = 0.05;
/// A jump above CLICK_MARGIN × ratio × the dry signal's own largest jump is a click.
const CLICK_MARGIN: f32 = 1.5;
```

Tests (all at 44.1 and 48 kHz, `mix: 1.0` unless stated):

- `plus_four_tracks_on_tones`: harmonic A3, E2 and C4, each 2 s with 8
  harmonics, `shift: 4`. Compute `cents_stats` against f0 × 2^(4/12) at
  ±25 ¢. Assert |median| ≤ 10 and share ≥ 0.95.
- `plus_four_tracks_on_a_scale`: the C3–C5 scale from `synthetic.rs` (move
  its step list into `common` so both use it). Exclude
  `STEP_EXCLUSION_S` around each step and `EDGE_S` at the ends. Same
  thresholds.
- `other_shifts_track`: harmonic A3 (and C4 for −12, so the output stays
  above 80 Hz) at `shift` −12, −5, +7, +12. Assert |median| ≤ 15 each.
  Print the share at ±25 ¢ too.
- `silence_noise_and_mix_zero_are_the_delayed_input`: extends Phase 1's
  test to `shift: 4` with `mix: 1.0` for silence and white noise (the gate
  never opens), and `mix: 0.0` for the harmonic tone and the scale. Bit for
  bit.
- `level_is_preserved`: harmonic A3 and the scale at `shift: 4`. The RMS of
  the output over [EDGE_S + L/sr, end − EDGE_S] is within ±1.5 dB of the
  input's RMS over the same span shifted back by L. Print the dB difference
  for every shift in −12, −5, +4, +7, +12; only +4 is asserted.
- `no_clicks_at_voicing_transitions`: A3 at `shift: 4` in three bursts of
  0.5 s with 0.3 s of silence between them. Then an A3 whose level ramps
  from −6 dBFS to −66 dBFS and back over 2 s, so the gate closes and
  reopens while audio continues. Threshold = `CLICK_MARGIN ·
  ratio(4) · max_jump(input)`. Assert `max_jump(output)` ≤ threshold, and
  print both values.
- `no_clicks_during_a_shift_sweep`: 5 s of A3, with `shift` stepping −12 →
  +12 one semitone every 200 ms via the per-sample `settings` closure.
  Threshold uses `ratio(12)`. Also sweep `mix` 0 → 1 → 0 over 2 s at
  `shift: 4`, with the ratio(4) threshold. Print the values.

**File**: `corral-dsp/tests/latency.rs`

- `reported_latency_matches_the_measured_delay`: 0.3 s silence, 1 s A3,
  0.3 s silence, at `shift: 4`, `mix: 1.0`. Build envelopes of input and
  output (|x| through a 5 ms one-pole low-pass). Cross-correlate over lags
  0..=4096 and take the peak lag. Assert |lag − `chain.latency()`| ≤ one
  hop. Repeat at `mix: 0.0`, where the lag must be exact. Print the
  measured lags, and the latency in samples and ms, for the README.

**File**: `corral-dsp/tests/fixtures.rs`

- `shifted_fixtures_track_the_shifted_reference`: for `vocadito_*` and
  `mdb_nightowl_08_male`, run the chain at `shift: 4`, `mix: 1.0` and
  estimate the output with `output_estimates`. Compute `cents_stats`
  against reference × 2^(4/12) at ±25 ¢, with `max_gap_s` = one hop.
  - **Headline:** `share_within` over all voiced reference frames. It must
    be ≥ `SHIFTED_MIN_SHARE = 0.90`.
  - **Diagnostic (printed, not asserted):** the same share over only the
    reference frames whose nearest chain detection (in the same run) was
    voiced. This isolates the shifter from detector recall.
  - If the headline misses on a fixture, set its threshold to just below
    the measured value, with a comment giving the measured headline and
    diagnostic scores and the reason. This follows the developer's
    decision during planning and the work item's drafting notes.

**File**: `corral-dsp/tests/no_alloc.rs`

Keep the single test. Inside `assert_no_alloc`, run:
- tone, noise and silence at `mix: 0.5, shift: 4`;
- the tone with `shift` sweeping −12..=12 one step every 2000 samples and
  `mix` alternating 0.0/1.0;
- `reset()`;
- the tone again at `shift: -12`.
The shifter's `start`/`stop`, mark catch-up and grain spawning all run
inside the check.

#### 4. Tuning

Run `cargo test -p corral-dsp --test shifter --test latency --test fixtures
-- --nocapture` and adjust the named constants. Tune `MARK_SEARCH`,
`SMOOTHING`, `CROSSFADE_S`, the hop counts and the normalisation floor, but
don't change the latency formula. Start with the ratio-1 SNR, then +4
tones, then clicks, then fixtures. Every change to a constant gets a
one-line comment giving what it was tuned against and the score. Record
the final printed numbers for Phase 7's README.

If a work-item threshold can't be met, record the measured value and the
reason next to the constant, as the drafting notes allow. The fixture
threshold is handled as described above. For any other threshold, set it
just below the measurement and flag it in the Phase 7 README section.

### Success Criteria:

#### Automated Verification:

- [x] `cargo test -p corral-dsp eval` passes (new `cents_stats` tests).
- [x] `cargo test -p corral-dsp --test shifter -- --nocapture` passes and
      prints pitch, level and click numbers at both sample rates.
- [x] `cargo test -p corral-dsp --test latency -- --nocapture` passes and
      prints the measured lag within ±1 hop of 1103 / 1200.
- [x] `cargo test -p corral-dsp --test fixtures -- --nocapture` passes and
      prints headline and diagnostic shares for all three shifted fixtures.
- [x] `cargo test -p corral-dsp --test no_alloc` passes.
- [x] `mise run check` passes.

#### Manual Verification:

- [x] Every threshold that differs from the work item has a comment with
      the measured value and the reason.

#### Phase 4 results (for Phase 7's README)

**Deviation from Phase 3's design:** synthesis marks are spaced by the
*local analysis-mark spacing* around each grain's source mark (÷ ratio), and
each grain's half-length is that spacing, rather than the detector's period
P. The detector's period now only steers mark tracking. Spacing by P made
the output pitch inherit the detector's per-hop error and drift out of step
with the marks. The fixtures at +4 scored 0.36 / 0.52 / 0.63 that way, and
0.72 / 0.81 / 0.85 after the change. The plan's constants are unchanged.
A sweep of `SMOOTHING` (0.5, 1.0) and `MARK_SEARCH` (0.15, 0.25, 0.35)
moved the fixture scores by ≤ 0.02, which is noise, so the planned values
stay.

Measured (44.1 / 48 kHz):

- Ratio-1 SNR (psola unit test): 71.2 / 66.1 dB at 110 Hz, 55.7 / 60.4 at
  220 Hz, 52.7 / 57.4 at 440 Hz.
- +4 on A3, E2, C4 and the scale: |median| ≤ 0.3¢, share within ±25¢
  1.000.
- Other shifts (A3 at −12, −5, +7, +12; C4 at −12): |median| ≤ 0.6¢, share
  1.000.
- Level at +4: −0.94 dB (A3), −0.88 dB (scale). Not asserted: −12
  −3.5 dB, −5 −1.8 dB, +7 −3.0 dB, +12 −6.3 dB. This is inherent to
  TD-PSOLA: the output samples the input's spectral envelope at the new
  harmonics, and for a 1/k tone, harmonics 2, 4, 6 and 8 carry 0.233 of the
  power (−6.3 dB) at +12.
- Clicks: the max jump equals the dry signal's own (0.0748 / 0.0687) for
  the bursts, the level ramp and the mix sweep. The shift sweep's is
  0.105 / 0.069, against thresholds of 0.224 / 0.206.
- Reported vs measured delay: mix 0 exactly 1103 / 1200. Mix 1 measured
  1105 / 1202.
- Fixtures at +4, headline (chain-voiced diagnostic) against the
  detector's own ±25¢ score on the unshifted input:
  - `vocadito_35_low`: 0.718 (0.750), detector 0.833. Threshold 0.70.
  - `vocadito_7_vibrato`: 0.812 (0.852), detector 0.851. Threshold 0.80.
  - `mdb_nightowl_08_male`: 0.851 (0.875), detector 0.915. Threshold 0.84.
  The 0.90 target is above what the detector itself scores at ±25¢, so it
  can't be met by the shifter alone. The thresholds were lowered as
  decided in planning.

---

## Phase 5: Harness Flags and Latency Compensation

### Overview

`analyse` gets `--shift` and `--mix`, runs the full chain, and compensates
its latency so the output WAV lines up with the input.

### Changes Required:

**File**: `analyse/src/main.rs`

- Usage:
  `analyse <in.wav> [out.wav] [--gain-db <dB>] [--shift <semitones>] [--mix <percent>]`.
- `Args` gains `shift: i32` (default `shift::DEFAULT` = 4, parsed with
  `shift::parse_shift`, so −12..=12 integers only) and `mix_percent: f32`
  (default 50, finite, 0..=100). These match the plugin's defaults.
- `run(input, output, csv, options: Options)`, with
  `Options { gain_db, shift, mix_percent }`.
- **Compensation:** process every input frame, then L frames of zeros
  (`chain.latency()`). Skip writing the first L output frames, so exactly
  as many frames are written as were read. Only hops that complete while
  real input frames are processed write CSV rows. This keeps the row count
  and times as before.
- The summary adds a line:
  `latency 1103 samples (25.0 ms), compensated; shift +4, mix 50%`.

**File**: `mise.toml`

- `analyse` task description: add `[--shift <semitones>] [--mix <percent>]`.

**Tests** (`analyse/src/main.rs`):

- Remove the Phase 1 `#[ignore]`s. The bit-identity tests (int, float,
  committed fixtures) run with `mix_percent: 0.0` and pass unchanged.
- `minus_6_db_halves_16_bit_samples` runs at `mix_percent: 0.0`.
- New:
  - `output_is_the_same_length_as_the_input` at `--shift 4 --mix 100`, mono
    and stereo, int and float.
  - `mix_100_output_is_shifted`: a 1 s 220 Hz tone at `--shift 4 --mix
    100`. Running `Chain` detection over the output WAV's samples finds a
    median f0 within 10 ¢ of 277.2 Hz, ignoring the first and last 0.15 s.
  - `defaults_match_the_plugin`: parsing `["a.wav"]` gives shift 4 and
    mix 50.
  - `shift_and_mix_flags_parse` (for example `--shift -5 --mix 100`) and
    rejection cases: `--shift 13`, `--shift 2.5`, `--mix 101`, `--mix -1`,
    `--mix`, `--shift`.

### Success Criteria:

#### Automated Verification:

- [x] `cargo test -p analyse` passes with no ignored tests.
- [x] `mise run analyse corral-dsp/tests/fixtures/vocadito_7_vibrato.wav target/analyse/v.wav --shift 4 --mix 100`
      writes `target/analyse/v.wav` and `target/analyse/v.csv`. The WAV
      has the same frame count as the input (check with
      `afinfo target/analyse/v.wav` against the input).
- [x] `mise run check` passes.

#### Manual Verification:

- [ ] `target/analyse/v.wav` sounds like the singer a major third up, with
      the dry voice absent at `--mix 100` and aligned with the input when
      the two are played side by side.
- [ ] At `--mix 50`, the harmony sits alongside the voice without flamming.

---

## Phase 6: Plugin Parameters, Latency Reporting and Editor

### Overview

Expose `Shift` and `Mix`, report the latency to the host, and add the two
sliders.

### Changes Required:

#### 1. Parameters

**File**: `corral/src/lib.rs`

```rust
#[derive(Params)]
struct CorralParams {
    /// Linear gain. The ID is stored in saved sets and automation, so it must never change.
    #[id = "gain"]
    gain: FloatParam,
    /// Whole semitones for the shifted voice. Chain ramps changes itself. ID must never change.
    #[id = "shift"]
    shift: IntParam,
    /// Dry/wet, 0.0–1.0, shown as a percentage. ID must never change.
    #[id = "mix"]
    mix: FloatParam,
}
```

```rust
shift: IntParam::new(
    "Shift",
    shift::DEFAULT,
    IntRange::Linear { min: shift::MIN, max: shift::MAX },
)
.with_unit(" st")
.with_value_to_string(Arc::new(shift::format_shift))
.with_string_to_value(Arc::new(|s| shift::parse_shift(s))),
mix: FloatParam::new("Mix", 0.5, FloatRange::Linear { min: 0.0, max: 1.0 })
    .with_smoother(SmoothingStyle::Linear(20.0))
    .with_unit("%")
    .with_value_to_string(formatters::v2s_f32_percentage(0))
    .with_string_to_value(formatters::s2v_f32_percentage()),
```

#### 2. Latency and processing

- `activate(&mut self, _, buffer_config, context)`:

```rust
// Allocates the detector's and shifter's buffers, off the audio thread.
self.chain = Chain::new(buffer_config.sample_rate);
// Fixed per sample rate: in VST3 a change restarts audio in Live.
context.set_latency_samples(self.chain.latency() as u32);
true
```

- `process()`: per frame, build
  `Settings { gain: gain.smoothed.next(), mix: mix.smoothed.next(), shift: self.params.shift.value() }`.
  Nothing else changes.
- Update the struct doc comment: stereo gain, pitch detection and a
  TD-PSOLA harmony.

#### 3. Editor

**File**: `corral/src/editor.rs`

- Module doc: Gain, Shift and Mix sliders, the gain readout and the note.
- `WINDOW_SIZE` → `300 × 240`.
- After the Gain slider and readout, add
  `ui.add(widgets::ParamSlider::for_param(&self.params.shift, &setter));`
  and the same for `mix`, then the note readout as before.

### Success Criteria:

#### Automated Verification:

- [x] `mise run check` passes.
- [x] `mise run build` bundles VST3 and CLAP.
- [x] `mise run build-universal` succeeds (the x86_64 slice for Live 10).

#### Manual Verification:

- [x] Covered in Phase 7.

---

## Phase 7: Documentation and Verification in Live 10

### Overview

Write the design doc and update the README and ROADMAP with the measured
numbers, then run the listening and delay-compensation checks in Live 10.

### Changes Required:

#### 1. Design doc

**File**: `docs/td-psola.md` (new)

Sections:

1. **What TD-PSOLA does.**
   - Pitch marks: one per period, on waveform peaks.
   - Grains: Hann-windowed, two periods long, centred on marks.
   - Overlap-add at synthesis marks spaced P / ratio.
   - Why this changes pitch but not duration: marks are reused or skipped.
   - Why formants survive: each grain keeps the original period's spectral
     envelope, because grain length follows the *analysis* period rather
     than the shift ratio.
   - A small ASCII diagram of analysis and synthesis marks.
2. **How Corral implements it.**
   - The YIN period and the smoother (octave rejection).
   - Mark tracking at P ± P/4.
   - The nearest-mark choice.
   - Window-sum normalisation.
   - The wet gate (hysteresis, 10 ms equal-power fade).
   - The `Shift` ramp.
   - The latency derivation, L = 2·P_max, and why the detector's window
     centre (n − 1024) lands within a hop of the output.
3. **Compared with a phase vocoder.** A table with rows for latency,
   formants, polyphony, phasiness/transients, detector dependence and CPU.
4. **Why Corral uses TD-PSOLA.**
5. **Known artefacts and mitigations.**
   - Pulses and buzziness at large downward shifts (window-sum floor).
   - Mark jitter at high pitches (linear interpolation, YIN's sub-sample
     period).
   - Detector errors: octave errors are rejected, and unvoiced input passes
     dry.
   - The onset delay before the wet path opens (about 60 ms).
   - Transients in voiced audio are repeated or skipped.
6. **Sources.** Moulines & Charpentier (1990), Driedger & Müller (2016),
   Laroche & Dolson (1999), de Cheveigné & Kawahara (2002, YIN), and the
   Kong EE264 report, all with links as in the work item. That makes at
   least three.

#### 2. README

**File**: `README.md`

- The intro under "Pitch detection" says audio now passes through with the
  gain applied and a shifted harmony mixed in.
- New section **"Pitch shifting"**:
  - Algorithm: TD-PSOLA, with a link to `docs/td-psola.md`.
  - Parameters:
    - `Shift`: whole semitones −12..+12, default +4, ramped over 50 ms;
    - `Mix`: 0–100 %, default 50 %, linear, smoothed over 20 ms;
    - the wet signal is built from the mono sum and added to both
      channels, while the dry path stays stereo;
    - unvoiced input passes dry, with a 10 ms crossfade.
  - A latency table:

    | Sample rate | Latency (samples) | Latency (ms) |
    |---|---|---|
    | 44.1 kHz | 1103 | 25.0 |
    | 48 kHz | 1200 | 25.0 |

    Say that it is reported to the host at activation and doesn't change
    with `Shift` or pitch. Give the re-measure command.
  - The measured scores from Phase 4:
    - +4 on tones and the scale (median ¢, share within ±25 ¢);
    - the other shifts;
    - level (dB);
    - click thresholds and the measured maxima;
    - the fixture headline and diagnostic shares;
    - any threshold that differs from the work item, and why.
  - The print command:
    `cargo test -p corral-dsp --test shifter --test latency --test fixtures -- --nocapture`.
- **Offline harness:** the new flags, their defaults (shift +4, mix 50 %),
  latency compensation, and that `--mix 0` at 0 dB is bit-identical to the
  input. Update the example command.
- **Tasks table:** the `analyse` row gets the new flags.

#### 3. ROADMAP

**File**: `ROADMAP.md`

Tick the Phase 4 items once the Live checks below pass:

- the pitch shifter (the TD-PSOLA option, noting it was chosen);
- mixing the shifted and dry signals;
- the dry/wet parameter;
- reporting latency;
- the listening tests;
- the shift parameter.

### Success Criteria:

#### Automated Verification:

- [x] `docs/td-psola.md` exists and contains a comparison table and at
      least three cited sources: `grep -c '^- ' docs/td-psola.md` under
      "Sources" is ≥ 3.
- [x] `mise run check` passes.

Note: the onset delay was measured rather than estimated. On synthesised
tones at 82–440 Hz, the wet voice starts fading in 11–23 ms after the
note's onset at the output, not the "about 60 ms" guessed above, because
the 25 ms latency hides part of the detector's delay. The ROADMAP ticks
wait for the Live checks below.

#### Manual Verification (Live 10 under Rosetta, after `mise run install` with Live closed):

- [ ] Option-click Rescan. Corral loads, and the window shows the Gain,
      Shift and Mix sliders and the note readout.
- [ ] Shift +4, Mix 50 %: singing a held A3 gives a C#4 harmony alongside
      the voice.
- [ ] Live shows Corral's latency (for example in the device title bar's
      delay readout or in the track's delay compensation). With a click or
      drum reference track playing against a copy of it through Corral at
      Mix 0 %, there is no audible flam.
- [ ] Moving and automating `Shift` (−12 → +12) and `Mix` while singing
      gives no clicks. Audio doesn't restart, which would show that the
      latency changed.
- [ ] Listening: note artefacts at low notes (E2–A2), at ±12, and on
      consonants and transients, in the README under "Known artefacts" or
      in the design doc.
- [ ] Changing Live's sample rate between 44.1 and 48 kHz re-activates the
      plugin with the new latency and keeps it aligned.

---

## Testing Strategy

### Unit Tests:

- `delay.rs`: exact delay, per-channel independence, zero length, reset,
  bit-exact NaN/−0.0.
- `psola.rs`:
  - latency and ring sizes;
  - mark spacing and polarity;
  - ratio-1 reconstruction (SNR);
  - grain bound at r = 2;
  - finite output on non-finite input;
  - stop/start.
- `period.rs`: convergence, note-change snap, octave glitch rejection,
  persistent octave acceptance, unvoiced hold.
- `shift.rs`: ratio, ramp duration and monotonicity, format/parse round
  trip and rejection.
- `wet_gate.rs`: hysteresis both ways, fade length, equal power, exact
  (1, 0) at rest, no chatter.
- `eval.rs`: `cents_stats`.
- `analyse`: flags, defaults, length, compensation and bit-identity at mix 0.

### Integration Tests (`corral-dsp/tests/`):

- `shifter.rs`: pitch at +4 (tones, scale), the other shifts, identity
  (silence, noise, mix 0), level, clicks (voicing, shift sweep, mix sweep).
- `latency.rs`: onset-to-detection (unchanged) and reported vs measured
  delay.
- `fixtures.rs`: detector accuracy (unchanged), plus shifted-output
  accuracy with headline and diagnostic scores.
- `no_alloc.rs`: the full chain, shifting, including start/stop and `Shift`
  changes.

### Manual Testing Steps:

1. `mise run analyse` on `vocadito_7_vibrato.wav` and
   `mdb_nightowl_08_male.wav` at `--mix 100`. Listen for a clean major
   third up, with vibrato preserved.
2. The same at `--shift -12` and `--shift 12`. Listen for the known
   artefacts and note them.
3. Live 10: the Phase 7 checks.

## Performance Considerations

- **Per sample:** at most 5 live grains, each with one `cos`, one linear
  interpolation and a multiply-add, plus two delay-line copies. That is
  well under the YIN hop's FFT cost.
- **Per hop:** the smoother and gate updates are O(1).
- **Mark confirmation:** normally one search of P/2 samples per period. At
  `start`, a catch-up burst of up to (L + P) / P searches, bounded at
  ~(1200 + 44) / 44 ≈ 28 at 1 kHz × P/2 samples. That is about 600
  comparisons, once per voiced onset.
- **Memory at 44.1/48 kHz:**
  - the input ring is 4096 f32;
  - the delay line is 2 × 1200 f32;
  - the marks are 128 i64;
  - the grains are 8 small structs.
- The "Performance pass" on the roadmap (Phase 6) remains the place for
  profiling.

## Migration Notes

- New parameter IDs `"shift"` and `"mix"` are appended. Existing Live sets
  with Corral load with Shift +4 and Mix 50 %, so an old set will now play
  a harmony. This is acceptable while the project is personal. Note it in
  the README.
- After install, Option-click Rescan so Live picks up the new parameters.
- The latency is new. Live applies delay compensation once the plugin is
  re-instantiated or the set is reloaded.
- `Chain::process_frame`'s signature changes. All callers (plugin, harness,
  tests) are updated in Phase 1.

## References

- Original work item: `meta/work/0005-realtime-td-psola-pitch-shifter.md`
- Previous plan (detector, `Chain`, harness, fixtures):
  `meta/plans/2026-09-27-0004-realtime-pitch-detection-with-offline-test-harness.md`
- Code: `corral-dsp/src/chain.rs:37`, `corral-dsp/src/pitch.rs:221-291`,
  `corral/src/lib.rs:121-161`, `analyse/src/main.rs:109-179`,
  `corral-dsp/tests/common/mod.rs:64-75`
- nice-plug latency: `nice-plug-core-0.4.2/src/context/activate.rs:28`,
  `nice-plug-0.4.2/src/wrapper/vst3/inner.rs:642-648`
- Moulines & Charpentier, Speech Communication 9(5–6), 1990
- Driedger & Müller, Applied Sciences 6(2), 2016: https://www.mdpi.com/2076-3417/6/2/57
- Laroche & Dolson, IEEE Trans. Speech and Audio Processing, 1999
- Kong, Stanford EE264: https://web.stanford.edu/class/ee264/projects/EE264_w2015_final_project_kong.pdf
- Ableton latency compensation: https://help.ableton.com/hc/en-us/articles/360001820360
