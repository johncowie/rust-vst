---
type: plan
id: "2026-09-29-0007-harmony-grid-ui-and-multi-voice-harmony"
title: "Harmony Grid UI and Multi-Voice Harmony Implementation Plan"
date: "2026-09-29T09:11:09+00:00"
author: John Cowie Del Corral
producer: create-plan
status: done
work_item_id: "work-item:0007"
parent: "work-item:0007"
tags: [phase-6, gui, dsp, harmony, multi-voice, glide, testing]
revision: "5671e9efdcbd7f298bce20bfccb33a96f27a447f"
repository: "rust-vst"
last_updated: "2026-09-29T11:52:30+00:00"
last_updated_by: John Cowie Del Corral
schema_version: 1
---

# Harmony Grid UI and Multi-Voice Harmony Implementation Plan

## Overview

This plan delivers work item 0007, the new Phase 6 of `ROADMAP.md`.
Corral's single diatonic third becomes a harmoniser you program with a
grid: for each pitch class you choose which intervals (−12…−1, +1…+12)
sound, and each one becomes a voice. In outline:

- a pure grid model in `corral-dsp`, shared lock-free between the
  editor and the audio thread;
- TD-PSOLA split into one shared analysis side and up to 24 synthesis
  voices;
- a voice pool that matches voices by position when the stable note
  changes (they glide), fades surplus voices in and out, and follows
  grid edits by interval value;
- `Gain`, `Key` and `Scale` removed; `Glide` and `Mix` kept;
- a 960×560 editor with the 12 × 25 grid;
- `--voices` in the harness, new tests with printed scores, a CPU
  measurement, then the README and ROADMAP.

## Current State Analysis

- `Chain::process_frame` (`corral-dsp/src/chain.rs:127`):
  - runs the tracker, the period smoother, the wet gate and the
    stabiliser once per hop;
  - on `Update::Onset` it jumps a single `Glide` to
    `settings.interval.interval(note)` (`chain.rs:141`);
  - every sample, it glides toward that interval and feeds
    `shift::ratio(semitones)` to one `Psola` (`chain.rs:175-176`);
  - it applies `gain::apply_gain` last (`chain.rs:192`).
- `Settings` (`chain.rs:25`) is `{ gain, mix, interval: IntervalSource,
  glide_s }`. `IntervalSource` (`harmony.rs:69`) is `Diatonic { key,
  scale }` or `Fixed(i32)`. `Fixed` exists only for the 0005 shifter
  tests.
- `Psola` (`psola.rs:43`) mixes two concerns:
  - **analysis:** the input ring, `written`, the marks ring and
    `polarity`, filled by `push` and `confirm_marks`;
  - **synthesis:** `grains` and `next_synthesis`, advanced by `next`
    (`psola.rs:267`).

  `next` only *reads* the marks. The one mutation it makes, pruning
  marks older than `o − 2·P_max` (`psola.rs:312-317`), depends only on
  the output time, not on the ratio.
- The wet gate replaces the dry signal inside the wet path
  (`chain.rs:182`): `wet = dry_gain·dry + shifted_gain·shifted`. With no
  shifted signal, a voiced passage therefore comes out at (1 − Mix) ×
  dry, and unvoiced gaps at full dry. This is the behaviour the work
  item's "Mix keeps its meaning" assumption and its empty-grid criterion
  describe.
- The plugin (`corral/src/lib.rs`):
  - has `gain`, `key`, `scale`, `glide` and `mix` params;
  - builds `IntervalSource::Diatonic` once per buffer
    (`lib.rs:251`);
  - publishes three readouts through atomics (`lib.rs:281-320`): the
    gain, the raw note and the (stable, harmony) pair via
    `note::pack_pair`.
- The editor (`corral/src/editor.rs`) is a 300×340 `CentralPanel` of
  `ParamSlider`s and labels.
- The harness (`analyse/src/main.rs`):
  - takes `--gain-db`, `--key`, `--scale`, `--glide` and `--mix`
    (`main.rs:17`);
  - writes `stable_note,interval,harmony_note` CSV columns
    (`main.rs:189`).
- Tests that build `Settings`:
  - `tests/common/mod.rs`: `DRY`, `fixed`, `diatonic` and
    `process_traced`, whose `Trace` records `chain.harmony()` per hop
    and `chain.shift_semitones()` per sample;
  - `tests/shifter.rs`, `tests/fixtures.rs:149`, `tests/latency.rs:101`,
    `tests/no_alloc.rs` and `tests/harmony.rs`.
- The click check is `common::assert_no_clicks` (`common/mod.rs:269`).
  It fails when the largest jump exceeds `CLICK_MARGIN (1.5) ·
  ratio(max_semitones) · max_jump(dry)`, the 0005 calibration.
- `note.rs` has `NAMES` (`pub(crate)`), `format_name` and
  `pack`/`unpack`. `harmony.rs:94` has `parse_key`, which accepts
  sharps, flats and any case.

## Desired End State

- The plugin has only `Glide` and `Mix` parameters. The editor is a
  fixed 960×560 window with:
  - a 12 × 25 grid of upright note-name cells under a −12…+12 ruler;
  - a display-only centre column that highlights the stable note;
  - 288 clickable toggles, drawn as "sounding" in the sounding row;
  - Glide and Mix sliders;
  - a readout such as `C4 → G3 · E4`, with the raw note under it.
- When the stable note's pitch class is *p*, each "on" interval in row
  *p* sounds as one voice at full level. Voices are summed into the wet
  signal. When the note changes, voices glide (matched by position) or
  fade (surplus). Edits fade their own voice. At onset, voices jump.
- The latency is unchanged: 1103 / 1200 samples.
- `mise run analyse in.wav out.wav --voices C:-5,+4 D:+3 --glide 50 --mix
  100` writes the WAV and a CSV with the stable note, the active
  intervals and the voice notes on each hop.
- `mise run check` passes. It includes the exhaustive grid test, the
  voice-pool unit tests, `tests/voices.rs` (the work item's acceptance
  criteria) and the ported 0005 and 0006 suites.
- The README documents the grid, voice matching, fades, `--voices`, the
  measured CPU cost of 24 voices and the clipping warning. It no longer
  mentions Gain, Key or Scale. The ROADMAP has the new Phase 6, the
  renumbered Phase 7 and the two Icebox items.

Verify with `mise run check`, the printed scores
(`cargo test -p corral-dsp --test voices --test harmony -- --nocapture`),
the CPU figure
(`cargo test --release -p corral-dsp --test cpu -- --ignored --nocapture`)
and the Live 10 checks in Phase 6.

### Key Discoveries:

- **The analysis can be shared.** Voices differ only in `grains` and
  `next_synthesis`. One `Analysis` (the ring and marks, about 16 KB)
  serves every voice. Each `psola::Voice` (8 grains, about 260 B) only
  reads it. Pruning moves to `Analysis::prune()`, called once per
  sample after every voice's `next`.
- **The 0005 figures can stay bit-exact.** The analysis must start
  exactly where `Psola::start` does, and each voice must start
  synthesising at the first mark. A voice at level exactly 1.0 summed
  from −0.0 gives the same float, and the removed gain stage was ×1.0.
  So every fixed-interval +4 figure should print unchanged. This is the
  regression gate for Phases 2 and 3.
- **A fixed shift is a uniform grid.** `Grid::uniform(n)` puts `{n}` in
  every row, which reproduces `IntervalSource::Fixed(n)` for n ≠ 0.
  Only the −12 → +12 sweep changes, for two reasons:
  - it has to skip 0;
  - changing the grid mid-note is an *edit*, which fades the old voice
    out and the new one in rather than gliding.
- **Summed voices need a scaled click threshold.** Two voices at full
  level can legitimately jump by the sum of their own largest jumps.
  So the check with several voices is
  `CLICK_MARGIN · Σ ratio(interval) · max_jump(dry)`: 0005's
  calibration applied per voice. With one voice it is exactly the old
  threshold.
- **Slots.** At most 24 voices sound at once, but a quick 24 → 0 → 24
  change (possible, since the stabiliser's fast path can change the
  note every 3 hops, about 16 ms, which is shorter than the 20 ms fade)
  needs 24 more slots for voices still fading out. So the pool has
  48 slots. If none is free it steals the quietest fading voice; this
  shouldn't happen in practice.
- **Getting the grid to the audio thread.** A `SharedGrid` of
  `[AtomicU32; 12]` in `corral-dsp` uses only `std`. The editor
  `fetch_xor`s one bit per click. `process()` copies the 12 rows into a
  `Grid` (`Copy`, 48 bytes) once per buffer. `Relaxed` ordering is
  enough: each row is independent, and a copy torn across a click only
  delays the change by one buffer.
- **The readout doesn't need a list of voices.** The editor already
  holds the grid, so the audio thread only publishes the stable MIDI
  note. The editor derives the sounding row and the `G3 · E4` list from
  it.

## What We're NOT Doing

- Saving the grid with the set or preset, or automating the toggles.
  The grid starts empty on every load (work item assumption; a
  follow-up item).
- Preset fills, such as chords in a key (Icebox).
- Per-voice level or pan, normalising the level as voices are added,
  a voice cap, or a limiter. 24 voices can clip; the README says so.
- Drag-to-paint, a clear button, keyboard control or undo in the
  editor.
- Formant correction, humanisation or MIDI control (Phase 7).
- Changing the detector, the smoother, the stabiliser, the wet gate or
  the latency.
- Optimising the synthesis (for example caching `ratio` while a glide
  is settled) before Phase 4 has measured it.

## Implementation Approach

Build bottom-up, as in 0005 and 0006:

1. the pure grid;
2. the `Psola` split, proved bit-exact;
3. the voice pool wired into `Chain`, with every caller moved and Gain
   removed. `Key` and `Scale` keep working through a temporary
   `harmony::diatonic_grid` bridge.

Then come the new objective tests and the CPU figure, then the
harness, then the plugin and editor (which deletes `harmony.rs`), and
last the docs.

Decisions agreed during planning:

1. **Fades are 20 ms and equal-power.** The gain is
   `sin(position · π/2)`, with position moving linearly, the same curve
   as the wet gate. Surplus voices often fade out while others fade in,
   and equal power keeps the loudness steady through that. 20 ms is
   twice the gate's fade and still feels immediate.
2. **Voices follow a note change by position and an edit by value.**
   - A note change matches voices ascending to ascending, so +4 glides
     to +3.
   - An edit of the sounding row only fades the voice for that cell;
     every other voice is untouched.
   - Switching a cell back on while its voice is still fading out
     revives that voice.
3. **Onset** matches by position but jumps instead of gliding. If the
   wet signal is fully silent (`!gate.is_active()`), surplus voices are
   freed at once and new ones start at full level, so the voices start
   exactly at their intervals.
4. **`--gain-db` is removed** along with Gain, so the harness keeps
   running exactly the plugin's chain.
5. **0006's scale, vibrato, scoop, onset and legato tests are kept.**
   They run through a grid filled with each scale's thirds, from
   hand-typed tables in `tests/common`. The out-of-key test and the
   Key/Scale click test go.
6. **The window stays 960×560.** The legend shares the title line, and
   the sliders and readout share one strip under the grid.

---

## Phase 1: Grid Model

### Overview

A pure module for the grid, its lock-free shared form, parsing and
display helpers. Nothing else changes yet.

### Changes Required:

#### 1. Grid module

**File**: `corral-dsp/src/grid.rs` (new; add `pub mod grid;` to
`lib.rs`)

```rust
//! The harmony grid: for each pitch class of the stable note, which
//! intervals (−12…−1 and +1…+12 semitones) sound as voices.

use std::sync::atomic::{AtomicU32, Ordering};

use crate::note::{self, NAMES};

pub const PITCH_CLASSES: usize = 12;
/// Intervals in a row, so the most voices that can sound at once.
pub const MAX_VOICES: usize = 24;

/// Bit i of a row is `INTERVALS[i]`. Ascending, so reading the bits from
/// the lowest gives the intervals in ascending order.
pub const INTERVALS: [i8; MAX_VOICES] = [
    -12, -11, -10, -9, -8, -7, -6, -5, -4, -3, -2, -1,
    1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12,
];

/// The bit for `interval`; `None` for 0 or beyond ±12.
pub fn bit(interval: i8) -> Option<u32>;

/// One row's "on" intervals as 24 bits.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Row(u32);

impl Row {
    pub const EMPTY: Self = Self(0);
    /// Bits above 23 are dropped.
    pub fn from_bits(bits: u32) -> Self;
    pub fn bits(self) -> u32;
    pub fn contains(self, interval: i8) -> bool;
    /// `self` unchanged for an interval with no bit.
    pub fn with(self, interval: i8, on: bool) -> Self;
    pub fn toggled(self, interval: i8) -> Self;
    pub fn len(self) -> usize; // count_ones
    pub fn is_empty(self) -> bool;
    /// The on intervals in ascending order, without allocating.
    pub fn intervals(self) -> Intervals;
}

/// Iterates a row's bits lowest first (`trailing_zeros`, then clear it).
#[derive(Clone)]
pub struct Intervals(u32);
impl Iterator for Intervals { type Item = i8; /* … */ }

/// All twelve rows, C first. `Copy`, so `Settings` stays `Copy`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Grid {
    rows: [Row; PITCH_CLASSES],
}

impl Grid {
    pub const EMPTY: Self;
    pub fn row(&self, pitch_class: usize) -> Row;
    /// The row for a MIDI note's pitch class; any octave, negative too.
    pub fn row_for(&self, midi: i32) -> Row;
    pub fn set_row(&mut self, pitch_class: usize, row: Row);
    pub fn toggle(&mut self, pitch_class: usize, interval: i8);
    /// `{interval}` in every row: a fixed shift, for the shifter tests.
    pub fn uniform(interval: i8) -> Self;
}

/// The grid shared by the editor (which toggles cells) and the audio
/// thread (which copies it once per buffer). Lock-free: one atomic per
/// row. `Relaxed` is enough, because rows are independent and nothing
/// else is published with them.
pub struct SharedGrid {
    rows: [AtomicU32; PITCH_CLASSES],
}

impl SharedGrid {
    /// Empty.
    pub fn new() -> Self; // and Default
    /// Twelve relaxed loads; doesn't allocate.
    pub fn snapshot(&self) -> Grid;
    pub fn row(&self, pitch_class: usize) -> Row;
    /// `fetch_xor` of the cell's bit.
    pub fn toggle(&self, pitch_class: usize, interval: i8);
}

/// The note name in row `pitch_class`, column `interval` (0 = the
/// centre column): `NAMES[(pitch_class + interval) mod 12]`.
pub fn cell_name(pitch_class: usize, interval: i8) -> &'static str;

/// The editor's readout: `"C4 → G3 · E4"`, or `"C4 → —"` for an empty row.
pub fn format_voices(stable: i32, row: Row) -> String;

/// Case-insensitive "C", "c#", "Db", "Bb", "E#"…; `None` otherwise.
/// (Moved from `harmony::parse_key`.)
pub fn parse_pitch_class(text: &str) -> Option<usize>;

/// Row specs such as `C:-5,+4` and `D:+3` (whitespace inside an item
/// separates specs too). Intervals are ±1…12 with an optional `+`. A
/// pitch class given twice, a repeated interval, 0 or beyond ±12 give
/// `None`.
pub fn parse_grid<'a>(specs: impl IntoIterator<Item = &'a str>) -> Option<Grid>;

/// `"C:-5,+4 D:+3"`, rows C…B, intervals ascending with signs; `"none"`
/// when empty. Round-trips through `parse_grid`.
pub fn format_grid(grid: &Grid) -> String;
```

`harmony.rs` keeps its own `parse_key` until Phase 6 deletes the
module. Its parsing tests are copied here now.

Unit tests in `grid.rs`:

- **Exhaustive toggling** (acceptance criterion 1). A reference model
  `[Vec<i8>; 12]` is kept independently: insert or remove the interval,
  then `sort`.
  - For each of the 288 cells: toggle it on in an empty grid, then
    check every row's `intervals().collect()` against the model. Toggle
    it off and check again.
  - Toggle all 288 cells on in a scrambled order
    (`k · 109 mod 288`, since 109 is coprime to 288), checking all
    12 rows after each toggle. Then toggle them all off in another
    order (`k · 173 mod 288`), checking the same way.
- `INTERVALS` has 24 entries, is strictly ascending and has no 0.
  `bit` round-trips, and `bit(0)`, `bit(13)` and `bit(-13)` are `None`.
- `row_for(48)`, `(60)`, `(72)` and `(-12)` are all row C, and
  `row_for(-1)` is row B.
- `Row::from_bits(u32::MAX).len() == 24`. `with` and `toggled` ignore 0
  and ±13.
- `SharedGrid`: toggles made on a spawned thread are all seen by
  `snapshot()` after `join`. Toggling a cell twice leaves it off.
- `cell_name(0, -5) == "G"`, `cell_name(11, 1) == "C"`, and
  `cell_name(p, 0) == NAMES[p]`.
- `format_voices(60, {−5, +4}) == "C4 → G3 · E4"`, and an empty row
  gives `"C4 → —"`.
- Parsing:
  - `parse_pitch_class` gets the old `parse_key` cases (round-trip,
    flats and lower case, bad input);
  - `parse_grid(["C:-5,+4", "D:+3"])` and `parse_grid(["C:-5,+4 D:+3"])`
    give the same grid;
  - `"db:3"` gives row C# = {+3};
  - rejected: `"C"`, `"C:"`, `"C:0"`, `"C:+13"`, `"C:-13"`, `"H:+1"`,
    `"C:+4,+4"`, `"C:+4 C:+3"`, `"C:four"` and an empty list;
  - `format_grid` round-trips a few grids, and gives `"none"` for
    `Grid::EMPTY`.

### Success Criteria:

#### Automated Verification:

- [x] Grid tests pass: `cargo test -p corral-dsp --lib grid`
- [x] `mise run check` passes

#### Manual Verification:

- [x] The reference model in the exhaustive test is independent of the
      bit logic (a sorted `Vec`, not `Row`)

---

## Phase 2: Split TD-PSOLA into Shared Analysis and Synthesis Voices

### Overview

`Psola` becomes `psola::Analysis` (shared) and `psola::Voice` (one
synthesis side). `Chain` still drives a single voice. Every existing
test must print exactly the same numbers as before.

### Changes Required:

#### 0. Baseline

Before any code change, record the current output:

```sh
cargo test -p corral-dsp --test shifter --test fixtures --test latency \
  --test harmony -- --nocapture --test-threads=1 2>&1 \
  | grep -v -e '^test ' -e 'finished in' > target/0007-baseline.txt
```

#### 1. Psola split

**File**: `corral-dsp/src/psola.rs`

```rust
/// The analysis side, shared by every voice: the mono input ring and
/// the confirmed pitch marks.
pub struct Analysis {
    latency: i64,
    max_period: i64,
    input: Vec<f32>,
    mask: usize,
    written: i64,
    marks: [i64; MAX_MARKS],
    mark_head: usize,
    mark_len: usize,
    polarity: f32,
    running: bool,
}

impl Analysis {
    /// Allocates. Call from `Chain::new`.
    pub fn new(sample_rate: f32) -> Self;
    pub fn reset(&mut self);
    pub fn push(&mut self, sample: f32, period: f32); // unchanged
    /// Starts marking at the current output time, exactly as
    /// `Psola::start` did. Returns the first mark: where every voice
    /// that is live now starts synthesising.
    pub fn start(&mut self, period: f32) -> i64;
    /// Forgets the marks.
    pub fn stop(&mut self);
    pub fn is_running(&self) -> bool;
    /// Where a voice added mid-note starts: the latest confirmed mark at
    /// or before the output time, else the earliest mark.
    pub fn entry_mark(&self) -> i64;
    /// Drops marks no grain can use any more, keeping the last. Call once
    /// per sample, after every voice's `next`.
    pub fn prune(&mut self);
    // private, unchanged: newest, output_time, at, read, last_mark, mark,
    // push_mark, argmax, confirm_marks, nearest_mark, local_period
}

/// One synthesis voice: its grains and synthesis clock over a shared
/// `Analysis`.
pub struct Voice {
    grains: [Grain; MAX_GRAINS],
    next_synthesis: f64,
    running: bool,
}

impl Voice {
    pub const fn new() -> Self; // and Default
    /// Clears the grains; the first synthesis mark is `mark`.
    pub fn start(&mut self, mark: i64);
    pub fn stop(&mut self);
    pub fn is_running(&self) -> bool;
    /// The shifted sample for the analysis's output time: grain spawning
    /// and overlap-add from the old `Psola::next`, reading `analysis`.
    /// Doesn't prune.
    pub fn next(&mut self, analysis: &Analysis, period: f32, ratio: f32) -> f32;
}
```

The maths doesn't change: the body of `next` moves into `Voice::next`,
and the pruning loop moves into `Analysis::prune`. Update the module
doc to describe the split.

Unit tests: port every existing test through a small test-only pairing
that calls `start`, `push`, `next` and `prune` in the old order. Add:

- **Voices don't disturb each other.** Two voices at ratios 1.5 and
  0.75 over one analysis each give output bit-identical to a lone voice
  at that ratio.
- **A voice started mid-run works.** Start a voice at `entry_mark()`
  half a second into a running analysis, at ratio 1. Its SNR after two
  periods is ≥ 20 dB.
- `entry_mark()` ≤ the output time whenever a mark at or before it
  exists.

#### 2. Chain

**File**: `corral-dsp/src/chain.rs`

Replace `psola: Psola` with `analysis: Analysis` and `voice: Voice`:

```rust
match (self.gate.is_active(), self.analysis.is_running(), period) {
    (true, false, Some(p)) => {
        let mark = self.analysis.start(p);
        self.voice.start(mark);
    }
    (false, true, _) => {
        self.analysis.stop();
        self.voice.stop();
    }
    _ => {}
}
self.analysis.push(mono, period.unwrap_or(0.0));
// …
Some(p) if self.analysis.is_running() => {
    let shifted = self.voice.next(&self.analysis, p, shift::ratio(semitones));
    self.analysis.prune();
    // mixing unchanged
}
```

### Success Criteria:

#### Automated Verification:

- [x] Psola tests pass: `cargo test -p corral-dsp --lib psola`
- [x] Every printed figure is unchanged: rerun the baseline command into
      `target/0007-phase2.txt`, and `diff target/0007-baseline.txt
      target/0007-phase2.txt` shows no differences
- [x] `mise run check` passes

#### Manual Verification:

- [x] None beyond the diff

---

## Phase 3: Voice Pool, Chain Integration and Gain Removal

### Overview

A `VoicePool` gives `Chain` up to 24 voices driven by the grid. Every
caller moves to the new `Settings`. Gain is removed everywhere.
`IntervalSource` goes, and the plugin and harness keep their Key and
Scale behaviour through a temporary `harmony::diatonic_grid`.

### Changes Required:

#### 1. Voice pool

**File**: `corral-dsp/src/voices.rs` (new; `pub mod voices;`)

```rust
//! The harmony voices over one shared analysis: which intervals sound,
//! how they follow the stable note and grid edits, and their fades.

use std::f32::consts::FRAC_PI_2;

use crate::grid::{MAX_VOICES, Row};
use crate::psola::{self, Analysis};
use crate::shift::{self, Glide};

/// Voices fade in and out over this long, equal-power (as the wet gate).
const FADE_S: f32 = 0.020;
/// A full row sounding while a full row fades out (see the plan).
const SLOTS: usize = 2 * MAX_VOICES;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Free,
    Sounding,
    FadingOut,
}

struct Slot {
    phase: Phase,
    /// Where it glides to: its interval in the row while sounding.
    interval: i8,
    glide: Glide,
    /// 0.0 = silent … 1.0 = full; moves by `step` each sample.
    position: f32,
    synth: psola::Voice,
}

/// One live voice, for the tests.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VoiceState {
    pub interval: i8,
    /// The glide's value.
    pub semitones: f32,
    /// sin(position · π/2); exactly 0.0 and 1.0 at rest.
    pub gain: f32,
    /// False while fading out.
    pub sounding: bool,
}

pub struct VoicePool {
    /// Boxed, allocated in `new` (from `Chain::new`, in `activate()`).
    slots: Box<[Slot]>,
    /// The row the sounding voices follow: exactly one sounding slot per
    /// interval in it.
    row: Row,
    step: f32,
}

impl VoicePool {
    /// Allocates.
    pub fn new(sample_rate: f32) -> Self;
    /// Frees every voice.
    pub fn reset(&mut self);
    pub fn row(&self) -> Row;

    /// Voicing onset: as `change`, but matched voices jump to their new
    /// intervals. When `silent` (no wet signal audible), fading voices
    /// are freed and every sounding voice is at full level at once.
    pub fn onset(&mut self, row: Row, silent: bool);
    /// The stable note changed. The sounding voices (ascending) take
    /// `row`'s intervals (ascending) by position and glide there. Extra
    /// intervals get new voices, which jump there and fade in. Unmatched
    /// old voices fade out.
    pub fn change(&mut self, row: Row);
    /// The sounding row was edited. Intervals no longer on fade out;
    /// newly on intervals get voices that fade in. Every other voice is
    /// untouched.
    pub fn edit(&mut self, row: Row);

    /// Every live voice starts synthesising at `mark` (the analysis has
    /// just started).
    pub fn start_all(&mut self, mark: i64);
    pub fn stop_all(&mut self);

    /// Advances every live voice's glide and fade by one sample. With
    /// `synthesis`, it also returns Σ gain · voice; a live voice that
    /// isn't synthesising yet starts at `analysis.entry_mark()`. Frees
    /// voices whose fade-out has finished.
    pub fn next(&mut self, synthesis: Option<(&Analysis, f32)>, coefficient: f32) -> f32;

    pub fn states(&self) -> impl Iterator<Item = VoiceState> + '_;
}
```

Details:

- **`claim(interval)`**, private. It picks a slot for a new voice at
  `interval`, in this order:
  1. a slot fading out at that interval, which is revived and keeps
     its position and synthesis;
  2. a free slot;
  3. the fading slot with the lowest position, which is stolen.

  For a free or stolen slot it runs `glide.jump(interval)`,
  `position = 0.0` and `synth.stop()`. The slot's phase becomes
  `Sounding`.
- **Matching by position without allocating.** Walk `self.row.intervals()`
  to collect the sounding slot indices into a `[usize; MAX_VOICES]`,
  ascending. Zip them with `row.intervals()`:
  - a matched slot gets `interval = new`, plus `glide.jump` on onset;
  - surplus new intervals are `claim`ed;
  - surplus old slots become `FadingOut`.

  Then `self.row = row`.
- **`edit`** uses bits:
  - every interval in `old & !new` has its slot set to `FadingOut`;
  - every interval in `new & !old` is `claim`ed.
- **Gain** is `if p == 0.0 { 0.0 } else if p == 1.0 { 1.0 } else {
  (p · FRAC_PI_2).sin() }`, as `WetGate::next_gains`.
- **`next`** sums from `-0.0`, so a single voice at gain 1.0 passes
  through unchanged, sign of zero included. The ratio is
  `shift::ratio(glide.next(f32::from(interval), coefficient))`.
- **The steal case** can't be reached by any realistic input. It is
  counted in a `#[cfg(test)]` field so the unit tests can check it
  doesn't happen.

Unit tests in `voices.rs` use `next(None, c)`, so they need no audio:

- **Onset while silent.** Row {−5, +4}: two sounding voices, with
  semitones exactly −5.0 and +4.0 and gain exactly 1.0 from the first
  sample.
- **Onset while the wet is still fading out.** Matched voices jump
  (the semitones equal the new interval on the next sample). Surplus
  voices fade.
- **`change`, same size.** {+4} → {+3}: the same slot now targets +3,
  and after one sample its semitones are `4 − (1 − c)`.
- **`change`, growing.** {+4} → {+3, +7}:
  - the +4 slot targets +3;
  - a new slot sits at exactly +7;
  - the new slot's gain rises monotonically from 0 to 1 in
    `round(FADE_S · sr) ± 1` samples, at 44.1, 48 and 96 kHz.
- **`change`, shrinking.** {+3, +7} → {+4}: +3's slot targets +4, and
  +7 fades out and is freed after the fade.
- **`change` matches by position, not value.** {−5, +4} → {−12, −5,
  +4, +12}:
  - the −5 voice targets −12, and the +4 voice targets −5;
  - new voices at +4 and +12 fade in.
- **`edit` works by value.** {−5, +4} plus +7: the −5 and +4 slots are
  untouched (same slots, gain 1.0 throughout), and +7 fades in.
  Removing −5 fades only −5.
- **Revival.** Remove +7, then add it back 5 ms later: it gets the same
  slot, its gain rises again from where it was without reaching 0, and
  no new slot is used.
- **Slots.** 24 → 0 → 24 within 5 ms uses 48 slots with no steal.
  Another flip at once steals, and the counter says so.
- `reset` frees everything, and `row()` is empty.

#### 2. Settings and Chain

**File**: `corral-dsp/src/chain.rs`

```rust
/// Per-frame controls. The plugin smooths `mix`; `Chain` glides and
/// fades the voices itself.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Settings {
    /// 0.0 = dry only, 1.0 = wet only.
    pub mix: f32,
    /// Which intervals sound for each pitch class. The plugin copies it
    /// once per buffer.
    pub grid: Grid,
    /// Glide time constant in seconds, 0.0..=shift::MAX_GLIDE_S.
    pub glide_s: f32,
}
```

- Remove `Harmony`, `harmony()`, `shift_semitones()`, the single
  `glide`, the gain loop and `use crate::gain`.
- Replace `voice: Voice` with `voices: VoicePool`.
- New accessors:
  - `pub fn stable_note(&self) -> Option<i32>`: kept through unvoiced
    hops; callers blank it for display;
  - `pub fn sounding(&self) -> Row`: the row the sounding voices
    follow, for the harness CSV;
  - `pub fn voices(&self) -> impl Iterator<Item = VoiceState> + '_`:
    for tests.

Per hop, then per sample:

```rust
let mut update = None;
if let Some(d) = &detection {
    self.smoother.update(d);
    self.gate.update(d.voiced);
    if d.voiced && let Some(period) = self.smoother.period() {
        let pitch = note::midi_from_hz(self.sample_rate / period);
        update = Some(self.stabiliser.update(pitch, !self.gate.is_open()));
    }
}
if let Some(note) = self.stabiliser.note() {
    let row = settings.grid.row_for(note);
    match update {
        // Onset: voices jump to their intervals (no glide after silence).
        Some(Update::Onset) => self.voices.onset(row, !self.gate.is_active()),
        Some(Update::Changed) => self.voices.change(row),
        _ if row != self.voices.row() => self.voices.edit(row),
        _ => {}
    }
}
// glide coefficient cache: unchanged
match (self.gate.is_active(), self.analysis.is_running(), period) {
    (true, false, Some(p)) => {
        let mark = self.analysis.start(p);
        self.voices.start_all(mark);
    }
    (false, true, _) => {
        self.analysis.stop();
        self.voices.stop_all();
    }
    _ => {}
}
self.analysis.push(mono, period.unwrap_or(0.0));
self.delay.process(frame);
match period {
    Some(p) if self.analysis.is_running() => {
        let shifted = self.voices.next(Some((&self.analysis, p)), self.glide_coefficient);
        self.analysis.prune();
        // gate gains and mix: unchanged
    }
    _ => {
        self.voices.next(None, self.glide_coefficient);
        self.gate.next_gains();
    }
}
```

A `Changed` note of the same pitch class (an octave leap) calls
`change` with the same row, which matches every voice to its own
interval: nothing moves. Update the module doc: there is no gain, and
the voices come from the grid. `reset()` resets the pool.

#### 3. Gain removal

- Delete `corral-dsp/src/gain.rs` and `pub mod gain;`.
- `corral/src/lib.rs`:
  - remove `MAX_GAIN_DB`, the `gain` param, `readout_gain` and
    `last_readout_gain`, and the gain readout's store;
  - the struct doc becomes "Pitch detection and a grid-driven
    multi-voice harmony".
- `corral/src/editor.rs`: remove the Gain slider, the gain label and
  `readout_gain`.
- `analyse/src/main.rs`:
  - remove `--gain-db`, `Options.gain_db` and the gain computation;
  - `--gain-db` is now an unknown flag;
  - remove the test `minus_6_db_halves_16_bit_samples`;
  - `dry(gain_db)` becomes `dry()`.

#### 4. Interval source bridge

**File**: `corral-dsp/src/harmony.rs`

- Remove `IntervalSource` and its test.
- Add `pub fn diatonic_grid(key: u8, scale: Scale) -> Grid`, with row
  `pc = {third_above(key, scale, pc)}`, documented as "a bridge for the
  plugin and harness until Phases 5 and 6 replace Key and Scale". It
  gets one test: C major row D = {+3}.
- `corral/src/lib.rs` and `analyse/src/main.rs` build
  `Settings { grid: harmony::diatonic_grid(key, scale), … }`. Their
  behaviour matches 0006, except that a Key or Scale change mid-note is
  now an edit (a crossfade) rather than a glide.

#### 5. Test callers

**File**: `corral-dsp/tests/common/mod.rs`

- `DRY = Settings { mix: 0.0, grid: Grid::EMPTY, glide_s: DEFAULT_GLIDE_S }`.
- `fixed(shift: i8, mix)` uses `Grid::uniform(shift)`.
- `pub fn voices(grid: Grid, glide_s: f32, mix: f32) -> Settings`.
- The hand-typed thirds tables move here from `harmony.rs`'s tests:
  - `MAJOR_THIRDS: [i8; 12] = [4, 4, 3, 3, 3, 4, 4, 4, 4, 3, 3, 3]`;
  - `NATURAL_MINOR_THIRDS: [i8; 12] = [3, 3, 3, 4, 4, 3, 3, 3, 4, 4, 4, 4]`;
  - `pub fn thirds_grid(key: usize, table: &[i8; 12]) -> Grid` sets row
    `pc = {table[(pc − key) mod 12]}`.
- `Hop { sample, detection, note: Option<i32>, sounding: Row }`.
- `Trace { output, hops, voices: Vec<Vec<VoiceState>> }`: the live
  voices on each sample.
- `pub fn single_voice(trace) -> Vec<Option<VoiceState>>`: panics if
  more than one voice is ever live.
- `pub fn assert_no_clicks_voices(name, signal, output, intervals: &[i8])`
  uses the threshold `CLICK_MARGIN · Σ ratio(i) · max_jump(dry)`.
  `assert_no_clicks` becomes the single-interval case of it, with its
  threshold unchanged.

**Files**: `tests/shifter.rs`, `tests/fixtures.rs`, `tests/latency.rs`

- Shifts become `i8`. Only the −12 → +12 sweep changes: it steps
  through `grid::INTERVALS` (skipping 0) every 200 ms, and each step is
  now a 20 ms crossfade. Record its new click figure.
- `latency.rs`: the "key C major" case becomes
  `voices(thirds_grid(0, &MAJOR_THIRDS), DEFAULT_GLIDE_S, 1.0)`, named
  "C major thirds grid, mix 1".

**File**: `tests/harmony.rs`

This file keeps the single-voice stabiliser and glide tests, driven by
thirds grids:

- the C major and A minor scales, using `thirds_grid` with key 0 and 9;
- vibrato (`hop.note == Some(60)` and `hop.sounding == {+4}`);
- the scoop;
- onset after silence (`single_voice` semitones exact from the first
  voiced hop);
- the legato C4 → D4 glide;
- clicks on the scale at Glide 50 and 0, and the Glide sweep without
  the Key alternation.

It drops `out_of_key_notes_use_the_note_below` and the Key/Scale click
case. Update its module doc.

**File**: `tests/no_alloc.rs`

- Keep the fixed passes.
- Replace the diatonic pass with a thirds-grid pass while the glide
  sweeps.
- Add two passes over the scale:
  - **full rows:** every row with all 24 on;
  - **mixed rows:** even pitch classes full, odd ones {−12, +12}, so
    note changes both glide and fade. Every 1000 samples the same cell
    (`INTERVALS[step % 24]`) is toggled in every row, so each toggle
    hits the sounding row. The grids are built on the stack inside the
    closure.
- Then `reset()` and a dry pass, as now.

### Success Criteria:

#### Automated Verification:

- [x] Voice-pool tests pass: `cargo test -p corral-dsp --lib voices`
- [x] No allocation: `cargo test -p corral-dsp --test no_alloc`
- [x] Every fixed-interval figure is unchanged except the shift sweep:
      rerun the baseline command into `target/0007-phase3.txt` and diff
      it. The only differences are the sweep's click lines, the
      `harmony` lines for the dropped tests, and the renamed latency
      case, which still prints 1106 / 1202.
- [x] The thirds-grid scale scores equal 0006's (0.956 / 0.966 and
      0.961 / 0.966 at Glide 50; 1.000 at Glide 0)
- [x] The shift-sweep click figure stays under its threshold
      (`cargo test -p corral-dsp --test shifter -- --nocapture`)
- [x] `mise run check` passes

#### Manual Verification:

- [x] The new shift-sweep click figure is noted for Phase 7

#### Results:

- The diff against the baseline shows only the expected lines. Every
  fixed-interval figure, both thirds-grid scale scores (0.956 / 0.966,
  0.961 / 0.966 at Glide 50; 1.000 at Glide 0), the onset, legato,
  scoop and vibrato lines, and the latency (1106 / 1202) are unchanged.
- Shift sweep (now 24 steps over 4.8 s, each a 20 ms crossfade): max
  jump 0.1046 at 44.1 kHz (threshold 0.2244, unchanged) and 0.0963 at
  48 kHz (threshold 0.2062; was 0.0962).
- The glide-sweep click case without the Key alternation: 0.2366 at
  44.1 kHz (was 0.2396); 48 kHz unchanged.
- **Slots test deviation.** Because `claim` revives a fading voice at
  the same interval, a plain 24 → 0 → 24 change reuses the fading slots
  and never needs more than 24. Fading voices outnumber a row only when
  a change matches sounding voices onto intervals whose voices are still
  fading. The test therefore runs full → {+1…+12} → empty → full with
  no time between: each round leaves 12 more voices fading, so the
  second round fills all 48 slots without stealing, and the third
  steals 12.


---

## Phase 4: Multi-Voice Tests, CPU Measurement and Tuning

### Overview

`corral-dsp/tests/voices.rs` covers the work item's audio acceptance
criteria at 44.1 and 48 kHz, printing every figure. `tests/cpu.rs`
measures a full row. Tune if needed, and record the results here.

### Changes Required:

#### 1. The tests

**File**: `corral-dsp/tests/voices.rs` (new)

The constants follow `harmony.rs`: `SAMPLE_RATES`,
`TOLERANCE_CENTS = 25.0`, `EDGE_S = 0.15` and `SILENCE_S = 0.3`. Each
test prints its figures.

1. **`an_empty_grid_is_the_dry_signal_scaled_by_one_minus_mix`**
   - Input: the C3–C5 scale (`common::scale`), with an empty grid and
     Mix 0.5.
   - Assert that no voice is ever live.
   - Assert that between `EDGE_S` from the start and the end, the
     output equals `0.5 · input[i − latency]` within 1e-6. Print the
     largest deviation.
   - If a hop mid-scale is unvoiced, the gate's fade makes the output
     rise toward the dry signal. The check then skips 30 ms around it
     and prints how many samples it skipped. The scale is voiced
     throughout in `synthetic.rs`, so none are expected.
2. **`two_voices_are_g3_and_e4_and_sum`**
   - Input: C4 held for 2 s, Mix 1.0.
   - Run it with row C set to {−5}, to {+4}, to {−5, +4}, and empty.
   - Alone, each voice's `output_estimates` median must be within ±25¢
     of G3 (55) and E4 (64). Print the share within ±25¢ too.
   - The summed run satisfies `out_both − out_−5 − out_+4 + out_empty`
     ≤ 1e-5 on every sample, which is the linearity of
     `wet = Σ gain · voice`. Print the largest difference. The
     empty-grid run cancels the dry and gate terms, which appear once
     in each run.
3. **`a_matched_voice_glides_rather_than_fading`**
   - Input: row C = {+4}, row D = {+3}, legato C4 → D4 (1 s each),
     Glide 50 ms.
   - Assert `single_voice` never panics (at most one voice is live on
     any sample).
   - Assert its gain is exactly 1.0 from the first voiced hop.
   - Assert its semitones fall monotonically from 4 to 3, never faster
     than `1.001 · (1 − c)`.
   - Print the step delay and the steepest step.
4. **`surplus_voices_fade_in_and_out_without_clicks`**
   - Input: row C = {+4}, row D = {+3, +7}.
   - C4 → D4: the +4 voice glides to +3. A +7 voice appears at exactly
     +7 with gain 0 and rises monotonically to 1 over 20 ms ± 1 sample.
   - D4 → C4: the +3 voice glides to +4, and the +7 voice falls to 0
     over 20 ms and is gone.
   - Both directions pass `assert_no_clicks_voices(…, &[3, 7])` at
     Glide 50 and 0.
5. **`toggling_the_sounding_row_does_not_click`**
   - Input: A3 held for 3 s, row A = {+4}, Glide 50 ms.
   - Every 250 ms one cell of row A changes: +7 on, −5 on, +4 off,
     +12 on, −5 off, −12 on, +7 off, and so on. Also switch +12 off and
     back on 5 ms apart (the revival path).
   - After each toggle, check that only the toggled voice's gain
     changes, and that it takes 20 ms ± 1 sample.
   - Pass `assert_no_clicks_voices` with every interval that was ever
     on together.
6. **`voices_start_at_their_intervals_after_silence`**
   - Input, at Glide 250 ms: 0.3 s of silence, C4 for 0.6 s with row
     C = {−5, +4}, 0.3 s of silence, then D4 for 0.6 s with row
     D = {+3}.
   - From the first voiced hop of C4 to the note's end: exactly two
     voices, with semitones exactly −5.0 and +4.0 and gain exactly 1.0
     on every sample.
   - D4: the old −5 voice jumps to +3 by position, and +4 is freed at
     once because the wet was silent. So there is exactly one voice at
     +3.0 with gain 1.0.
   - Print the first voiced hop's time for each note.

#### 2. CPU measurement

**File**: `corral-dsp/tests/cpu.rs` (new)

```rust
//! Times the chain per 512-frame stereo block, for the README. No
//! pass/fail threshold; run it in a release build.

#[test]
#[ignore = "a release-build measurement: cargo test --release -p corral-dsp --test cpu -- --ignored --nocapture"]
fn time_per_block_with_a_full_row() { … }
```

- 10 s at 48 kHz of the C major scale (`note_sequence`, repeated),
  Mix 0.5, Glide 50 ms.
- Four grids: empty; `uniform(4)` (1 voice); every row {−5, +4, +7}
  (3 voices); every row full (24 voices).
- Time each block with `Instant`. For each grid, print:
  - the mean, 99th-percentile and maximum µs per block;
  - that time as a share of the block's 10.67 ms real time.
- If built with debug assertions, print a warning that the figures
  mean nothing.
- Run it natively (arm64), and under Rosetta with
  `--target x86_64-apple-darwin`, since Live 10 runs the x86_64 slice.

#### 3. Tuning

If an assertion misses:

- Print which samples fail first. They should be at fades or glides.
- The levers are `FADE_S` (20 ms) and the fade curve. The gain shape
  mustn't move `a_matched_voice_glides_rather_than_fading`.
- Every change carries a comment with the measured figure and the
  reason, and is recorded in the results below.

### Success Criteria:

#### Automated Verification:

- [x] Multi-voice tests pass and print figures:
      `cargo test -p corral-dsp --test voices -- --nocapture`
- [x] The CPU test runs and prints four rows:
      `cargo test --release -p corral-dsp --test cpu -- --ignored --nocapture`
- [x] `mise run check` passes (the CPU test is ignored there)

#### Manual Verification:

- [x] Listen to a test-written or harness WAV of row C = {−5, +4} on a
      held C4: G3 and E4 are clear, and toggles don't tick
- [x] Results (figures, CPU on arm64 and under Rosetta) recorded below
      for the README

#### Results:

No tuning was needed: every assertion passed on the first run, with
`FADE_S` at 20 ms and the equal-power curve.

- **Empty grid, Mix 50%:** the output is exactly 0.5 × the delayed dry
  signal (largest deviation 0.0 at both rates).
  - **Deviation from the plan:** the scale is not voiced throughout.
    The detector calls 11 hops unvoiced at semitone steps. Two of those
    steps (4.43 s and 5.23 s) have two unvoiced hops in a row, which
    closes the gate.
  - So the test skips 30 ms from each such pair only (2 closes, 2646 /
    2880 samples). A lone unvoiced hop doesn't move the gate, so it is
    still checked.
- **Two voices on C4:**
  - G3 alone: median −0.2¢ (44.1 kHz) and +0.2¢ (48 kHz);
  - E4 alone: +0.1¢ at both rates;
  - both within ±25¢ on 100% of frames;
  - the sum's largest difference from the voices alone is 1.19e-7.
- **Matched voice, C4 {+4} → D4 {+3}, Glide 50 ms:** one voice at full
  level throughout.
  - It starts gliding 17.4 ms (44.1 kHz) and 10.7 ms (48 kHz) after D4
    is detected.
  - Steepest step: 0.000453 against a limit of 0.000454 at 44.1 kHz,
    and 0.000417 against 0.000417 at 48 kHz.
- **Surplus voices:**
  - +7 fades in over 882 / 960 samples and out over 881 / 960, i.e.
    20 ms at both rates.
  - Largest jump at Glide 50 and 0: 0.2255–0.2270 at 44.1 kHz
    (threshold 0.5828) and 0.2077–0.2089 at 48 kHz (threshold 0.5358).
- **Toggles on a held A3:**
  - 10 fades of 881–882 samples (44.1 kHz) and 960 (48 kHz); every other
    voice stays at full level.
  - Revival of +12 after 5 ms: it falls to gain 0.924, then takes 221 /
    240 samples to recover, the same time it spent falling.
  - Largest jump, against the threshold for the loudest set on together,
    {+4, +7, +12}: 0.2343 (threshold 0.7743) at 44.1 kHz and 0.2196
    (threshold 0.7117) at 48 kHz.
- **Onset after silence, Glide 250 ms:**
  - C4 {−5, +4} and D4 {+3}: 0 samples differ from the exact intervals
    at full level.
  - First voiced hop: 36.7 / 30.6 ms after the note starts at 44.1 kHz,
    and 30.6 / 32.0 ms at 48 kHz.
- **CPU:** 10 s at 48 kHz, 512-frame stereo blocks, 10 667 µs of real
  time each, on an Apple Silicon Mac. Each cell is mean / p99 / max, in
  µs, with the mean's share of real time in brackets:

  | Grid                  | arm64 (native)          | x86_64 (Rosetta)        |
  |-----------------------|-------------------------|-------------------------|
  | empty                 | 53.7 / 61.5 / 79.3 (0.5%)   | 63.3 / 76.1 / 84.0 (0.6%)   |
  | 1 voice (+4)          | 71.9 / 82.7 / 102.0 (0.7%)  | 89.8 / 107.0 / 120.6 (0.8%) |
  | 3 voices (−5, +4, +7) | 100.1 / 115.6 / 199.2 (0.9%)| 133.9 / 153.3 / 159.7 (1.3%)|
  | 24 voices (full row)  | 371.5 / 409.8 / 507.2 (3.5%)| 580.4 / 651.5 / 749.1 (5.4%)|

  Each voice costs about 13 µs per block native and 22 µs under
  Rosetta, so the optimisations deferred in Performance Considerations
  aren't needed. Run the Rosetta figure with `--target
  x86_64-apple-darwin` added.

---

## Phase 5: Harness `--voices` and CSV

### Overview

The harness drops `--key` and `--scale` and gains `--voices`. The CSV
lists the stable note, the active intervals and the voice notes.

### Changes Required:

**File**: `analyse/src/main.rs`

- `USAGE`: `analyse <in.wav> [out.wav] [--voices <row>…] [--glide <ms>]
  [--mix <percent>]`, where a row is `<C..B>:<±1..12>[,…]`, e.g.
  `--voices C:-5,+4 D:+3`.
- `Options { grid: Grid, glide_ms, mix_percent }`. The default is
  `Grid::EMPTY`, 50 ms and 50%, the plugin's defaults.
- `parse_args`, for `--voices`:
  - consume the following arguments while they contain `:` (at least
    one), and parse them with `grid::parse_grid`;
  - an argument may hold several specs separated by spaces, so both
    `--voices C:-5,+4 D:+3` and `--voices "C:-5,+4 D:+3"` work;
  - `--key` and `--scale` become unknown flags.
- `Settings { grid: options.grid, … }`. Remove the use of
  `harmony::diatonic_grid` and the `harmony` import.
- CSV header: `time_s,f0_hz,confidence,voiced,stable_note,intervals,voice_notes`.
  - On voiced hops, the columns come from `chain.stable_note()` and
    `chain.sounding()`, with space-separated lists, e.g.
    `C4,-5 +4,G3 E4`.
  - An empty row gives `C4,,`.
  - Unvoiced hops give `,,`.
- The summary line reads `latency … compensated; voices C:-5,+4 D:+3,
  glide 50 ms, mix 100%` (`voices none` when empty, via
  `grid::format_grid`).

Tests:

- `csv_has_one_row_per_hop`:
  - 7 columns;
  - with `--voices A:+3`, the voiced rows end `A3,+3,C4`;
  - by default they end `A3,,`.
- `mix_100_output_is_shifted`: with the grid `A:+3`, expect 220 ×
  `ratio(3.0)`.
- `defaults_match_the_plugin`: an empty grid, 50 ms and 50%.
- `voices_flag_parses`:
  - `["a.wav", "--voices", "C:-5,+4", "D:+3", "--glide", "0"]`;
  - `["--voices", "C:-5,+4 D:+3", "a.wav"]`;
  - `["--voices", "db:3", "a.wav"]` gives row C# = {+3}.
- `bad_arguments_are_rejected` adds:
  - `--voices` alone, and `--voices a.wav`, which has no spec;
  - `C:0`, `C:+13`, `H:+1`, `C:` and `C:+4 C:+3`;
  - `--key C`, `--scale major` and `--gain-db -6`, all now unknown.
- The bit-identical, length and fixture pass-through tests are
  unchanged.

**File**: `mise.toml`: set the `analyse` description to the new usage.

### Success Criteria:

#### Automated Verification:

- [x] Harness tests pass: `cargo test -p analyse`
- [x] `mise run analyse corral-dsp/tests/fixtures/vocadito_7_vibrato.wav target/analyse/v.wav --voices C:-5,+4 D:+3 --glide 50 --mix 100`
      writes `v.wav`, and `v.csv` with the three new columns
- [x] `mise run check` passes

#### Manual Verification:

- [x] Reading `v.csv`: hops on C list `-5 +4`, hops on D list `+3`,
      and other notes have empty interval columns

---

## Phase 6: Plugin and Harmony Grid Editor

### Overview

`Key` and `Scale` go, and `harmony.rs` is deleted. The plugin shares a
`SharedGrid` with a new 960×560 editor built around the grid.

### Changes Required:

#### 1. Plugin

**File**: `corral/src/lib.rs`

- Remove `KeyParam`, `ScaleParam`, the `From` impl, the `key` and
  `scale` params, and their tests. Only `glide` and `mix` remain, with
  IDs unchanged.
- Delete `corral-dsp/src/harmony.rs` and `pub mod harmony;`. Its last
  caller is gone.
- Add `grid: Arc<SharedGrid>`, created in `Default` and cloned into
  `CorralEditor::new`.
- `process()` reads `let grid = self.grid.snapshot();` once per buffer,
  with Glide, and builds `Settings { mix, grid, glide_s }` per frame.
- Replace `harmony_readout` / `last_harmony_readout` with
  `stable_readout` / `last_stable_readout`. On each buffer's last hop,
  publish `note::pack_midi(chain.stable_note().filter(|_|
  detection.voiced))`. As now, it is stored only when it changes, and
  triggers a repaint only while the editor is open.

**File**: `corral-dsp/src/note.rs`: add `pack_midi(Option<i32>) -> u32`
and `unpack_midi`, using the `UNVOICED` sentinel. Test that they
round-trip over MIDI 0–127. Remove `pack_pair` / `unpack_pair` and
their test, which no longer have callers.

#### 2. Editor

**File**: `corral/src/editor.rs`

- `WINDOW_SIZE = LogicalSize::new(960.0, 560.0)`.
- `CorralEditor::new(params, grid, note_readout, stable_readout)`.
- The layout, top to bottom, inside the `CentralPanel`:
  - **Title line:** "Corral" on the left; the legend on the right, with
    four swatches and labels (stable note, on, on and sounding, off).
  - **Ruler:** one centred label per column, −12 … +12. The 0 is in
    the stable colour.
  - **Grid:**
    - `CELL = 30.0` and `GAP = 4.0`, so 25 columns take 846 px,
      centred;
    - rows C (top) … B;
    - column `c` is the interval `c − 12`, and each cell shows
      `grid::cell_name(pc, interval)`, centred and upright;
    - one rounded outline spans all 12 rows around the centre column.
  - **Bottom strip:**
    - on the left, `ParamSlider`s for Glide and Mix;
    - on the right, the readout in large monospace, with the raw
      detected note (`format_note`) under it in small type.
- Cells are drawn with `ui.allocate_rect(rect, Sense::click())` and the
  painter:
  - **Off:** a dark fill with grey text. Hover brightens the stroke.
  - **On:** a blue fill with white text.
  - **On in the sounding row:** a brighter blue with a 2 px yellow
    stroke.
  - **Centre column:** a darker fill with bold white text. For the
    stable note's pitch class it has a yellow fill with dark text.
    Nothing is highlighted while unvoiced.
  - The sounding row also gets a faint background band.
- A click on a toggle cell calls `grid.toggle(pc, interval)`. The
  centre column has no click sense.
- Readout:
  - `unpack_midi(stable_readout)`;
  - `Some(n)` gives `grid::format_voices(n, grid.row(n mod 12))`;
  - `None` gives `—`.
- The colours are `const Color32`s at the top of the module. Update the
  module doc.

### Success Criteria:

#### Automated Verification:

- [x] `cargo test -p corral` passes
- [x] `grep -rn "harmony::\|IntervalSource\|gain::" corral corral-dsp analyse`
      finds nothing
- [x] `mise run build-universal` succeeds
- [x] `mise run check` passes

#### Manual Verification:

- [x] `mise run install`, then Option-click Rescan in Live 10 (Rosetta,
      x86_64 slice). Corral opens a 960×560 window with only Glide and
      Mix controls. Every cell is labelled upright, and it matches the
      mock-up's layout
- [x] Row C = {−5, +4}, Mix 50%: singing a C gives G below and E above.
      The C in the centre column lights up, and the two toggles show as
      sounding. The readout shows e.g. `C4 → G3 · E4`
- [x] Singing a note whose row is empty gives only the dry signal. Its
      centre cell still lights up, no toggle shows as sounding, and the
      readout shows `→ —`
- [x] Toggling cells while singing, and moving between programmed
      notes: no audible clicks, and the latency Live reports stays at
      1103 / 1200 samples
- [x] Reopening the set starts with an empty grid (expected; not saved)

#### Results:

- `cargo test -p corral` now has no tests: the only ones were the Key
  and Scale param tests, which went with the params.
- Two editor details the plan didn't settle:
  - egui's default fonts have no bold face, so the centre column's
    labels are drawn twice, 0.6 px apart;
  - the readout shrinks its font to fit the strip, since a full row
    lists 24 voices.

---

## Phase 7: Documentation and Roadmap

### Overview

The README, the design doc and the ROADMAP catch up.

### Changes Required:

**File**: `README.md`

- **Harmony** is rewritten as the harmony grid:
  - rows are pitch classes of the stable note, and columns are
    intervals −12…+12 from it, so the octave follows the singer;
  - each "on" cell is one voice at full level, summed into the wet
    signal;
  - note changes match voices by position (with the +4 → +3 example)
    and glide;
  - surplus voices fade in and out over 20 ms, equal-power;
  - edits fade only their own voice, and a quick off/on revives it;
  - at onset, voices jump;
  - the grid starts empty and isn't saved with the set.
- **Warning:** several voices at full level can clip, and there is no
  normalisation or limiter yet.
- **Editor:** the grid, the centre column, the colours, and the readout
  (`C4 → G3 · E4`, with the raw note under it).
- **Note stabiliser and Glide:** kept. "Changing Key or Scale mid-note"
  becomes "matched voices glide when the note changes".
- **Accuracy:**
  - the Phase 4 figures;
  - the thirds-grid scale scores from Phase 3;
  - the new click table, with the Σ-ratio threshold explained.
- **CPU:** the Phase 4 table (empty, 1, 3 and 24 voices; arm64 and
  Rosetta), and how to rerun it.
- **Pitch detection:** remove "taken before the gain, so the Gain
  slider doesn't affect detection".
- **Pitch shifting:**
  - intervals now come from the grid;
  - the latency doesn't change with the grid, the number of voices,
    Glide or Mix;
  - the shift-sweep click figure from Phase 3.
- **Old Live sets:** sets saved with Gain, Key or Scale load with those
  values dropped (no users yet).
- **Offline harness:**
  - the example command with `--voices`;
  - the flag syntax;
  - no `--gain-db`, `--key` or `--scale`;
  - the three CSV columns.
- **Task table:** the new `analyse` description.

**File**: `docs/td-psola.md`: describe the split between the shared
analysis (ring and marks) and the per-voice synthesis (grains and
clock), and why it costs little per voice.

**File**: `ROADMAP.md`

- Insert **Phase 6: Harmony grid** after Phase 5, with these items,
  ticked once verified:
  - a grid of toggles (one row per pitch class, intervals −12…+12)
    that replaces Key and Scale;
  - several voices, matched by position (glide), with surplus and
    edited voices faded;
  - Gain removed;
  - the grid editor;
  - harness `--voices`;
  - the CPU cost of 24 voices measured;
  - end-to-end in Ableton.
- Renumber the old Phase 6 to **Phase 7: Refinement and extras**:
  - strike "Choice of harmony interval" and "Multiple harmony voices",
    each with "(Phase 6)";
  - add an item for "Per-voice level and pan, and level normalisation"
    (the part not delivered).
- In Phase 5, change "other scales stay open for Phase 6" to
  "superseded by the harmony grid (Phase 6)".
- Icebox:
  - preset grid fills (for example, chords in a chosen key);
  - saving the grid with the set or preset, and host automation of the
    toggles.

### Success Criteria:

#### Automated Verification:

- [x] `mise run check` passes

#### Manual Verification:

- [x] The README's figures match the latest `--nocapture` output and
      the CPU run
- [x] Phase 6's end-to-end item is ticked only after the Live checks in
      Phase 6 of this plan

#### Results:

- The README's figures were checked against a fresh `--nocapture` run of
  the `voices`, `harmony`, `shifter`, `latency`, `fixtures` and
  `synthetic` suites. The CPU table is Phase 4's run; it wasn't rerun.
- The harmony click table loses the Key/Scale alternation row, and its
  glide-sweep row is now 0.237 / 0.210 (was 0.240 / 0.210). The
  out-of-key test went with `harmony.rs`'s interval tables, so the README
  no longer lists it.
- ROADMAP Phase 6: every item is ticked, the editor and end-to-end
  items once the Live checks in Phase 6 of this plan were done.

---

## Testing Strategy

### Unit Tests:

- **`grid.rs`:**
  - exhaustive toggling of all 288 cells against a sorted-`Vec` model;
  - bit mapping;
  - `row_for` across octaves;
  - `SharedGrid` across threads;
  - cell names and the readout;
  - parsing and formatting.
- **`psola.rs`:**
  - every existing test through `Analysis` + `Voice`;
  - independent voices over one analysis;
  - mid-run start.
- **`voices.rs`:**
  - onset (silent and fading);
  - `change` by position (same size, growing, shrinking, reordering);
  - `edit` by value;
  - revival;
  - fade timing at three sample rates;
  - slots and stealing;
  - reset.
- **`note.rs`:** `pack_midi`.

### Integration Tests:

- **`tests/voices.rs`:** every audio acceptance criterion at 44.1 and
  48 kHz, printed.
- **`tests/harmony.rs`:** the stabiliser and glide tests, driven by
  thirds grids.
- **The 0005 suites** (`shifter`, `fixtures`, `latency`, `synthetic`)
  use uniform grids. Their figures are identical except the sweep.
- **`no_alloc`:** full rows, mixed rows with note changes, and toggles.
- **`cpu`** (ignored): the time per block for 0, 1, 3 and 24 voices.
- **The harness tests:** `--voices` and the CSV.

### Manual Testing Steps:

1. `mise run analyse` a fixture with `--voices C:-5,+4 D:+3 --mix 100`,
   listen to it, and read the CSV.
2. In Live 10, set row C = {−5, +4}, sing C, and hear G3 and E4. The
   editor highlights the C in the centre column and the two toggles.
3. Sing a note whose row is empty: dry only.
4. Toggle cells mid-note, and move between programmed notes: no
   clicks.
5. Fill a whole row (24 voices) and sing: it works, and it's loud
   (expected clipping). Check Live's CPU meter.
6. Check that the latency is still 1103 / 1200.

## Performance Considerations

- The analysis (ring and marks) runs once per sample whatever the
  number of voices.
- Each voice costs a glide multiply-add, one `exp2` for its ratio, and
  overlap-add over at most 5 grains (a cosine and an interpolated read
  each). At 24 voices that is about 24 `exp2` and 100 cosines per
  sample.
- Grain spawning scans the marks once per period per voice.
- Phase 4 measures the total. If it proves too high, caching the ratio
  while a glide is settled and a cosine table are the first
  optimisations, but they are out of scope here.
- The pool is 48 slots of about 300 B, boxed once in `Chain::new`. The
  audio thread allocates nothing, and `no_alloc` checks this with 24
  voices, note changes and toggles.
- Reading the grid is 12 relaxed atomic loads per buffer, and a
  `u32` compare per sample to spot edits.

## Migration Notes

- `Gain` (`gain`), `Key` (`key`) and `Scale` (`scale`) are removed.
  Hosts ignore their stored values. There are no users yet (work item
  assumption).
- The grid isn't a parameter, so it isn't saved: every load starts with
  an empty grid, and Mix 100% is then silent while singing.
- The harness flags `--gain-db`, `--key` and `--scale` are removed.
  Scripts using them get the usage error.

## References

- Original work item: `meta/work/0007-harmony-grid-ui-and-multi-voice-harmony.md`
- Sketch and mock-up: `meta/work/assets/0007-harmony-grid-sketch.png`,
  `meta/work/assets/0007-harmony-grid-mockup.png`
- Previous plan: `meta/plans/2026-09-28-0006-key-aware-diatonic-harmony.md`
- Chain: `corral-dsp/src/chain.rs:127`
- Psola state and synthesis: `corral-dsp/src/psola.rs:43`, `:267`,
  mark pruning `:312`
- Wet gate gains: `corral-dsp/src/wet_gate.rs:65`
- Click calibration: `corral-dsp/tests/common/mod.rs:264`
- Plugin readouts: `corral/src/lib.rs:281`
- Harness usage and CSV: `analyse/src/main.rs:17`, `:187`
