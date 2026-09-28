---
type: plan
id: "2026-09-28-0006-key-aware-diatonic-harmony"
title: "Key-Aware Diatonic Harmony Implementation Plan"
date: "2026-09-28T14:56:54+00:00"
author: John Cowie Del Corral
producer: create-plan
status: draft
work_item_id: "work-item:0006"
parent: "work-item:0006"
tags: [phase-5, dsp, harmony, music-theory, stabilisation, glide, testing]
revision: "8eefc2e9195824437cbbb1580c67b9a149aa2b36"
repository: "rust-vst"
last_updated: "2026-09-28T14:56:54+00:00"
last_updated_by: John Cowie Del Corral
schema_version: 1
---

# Key-Aware Diatonic Harmony Implementation Plan

## Overview

This plan delivers Phase 5 of `ROADMAP.md` (work item 0006). It turns the
fixed-interval TD-PSOLA shifter from 0005 into an auto-harmoniser that
adds a diatonic third above the voice in a chosen key and scale:

- pure interval logic in `corral-dsp` (major and natural minor, and the
  "scale note a semitone below" rule for out-of-key notes);
- a per-hop note stabiliser, so vibrato and scoops don't make the
  harmony flicker;
- a one-pole glide between intervals, replacing the 50 ms anti-click
  `SemitoneRamp`;
- `Key`, `Scale` and `Glide` in the plugin and harness, with `Shift`
  removed;
- objective tests with printed scores, then the README and ROADMAP.

## Current State Analysis

- `Chain::process_frame` (`corral-dsp/src/chain.rs:86`) runs the tracker,
  then `PeriodSmoother::update` and `WetGate::update` once per hop. Every
  sample it ramps `settings.shift` through `SemitoneRamp`
  (`chain.rs:109`) and passes `shift::ratio(semitones)` to `Psola::next`.
- `Settings` (`chain.rs:20`) is `{ gain, mix, shift: i32 }`. The plugin,
  the harness (`analyse/src/main.rs:173`), `tests/common/mod.rs:91`
  (`DRY`), `tests/shifter.rs:33` (`wet`), `tests/fixtures.rs:149`,
  `tests/latency.rs:100` and `tests/no_alloc.rs` all build it.
- `Psola::next` (`psola.rs:267`) uses the ratio only to space the next
  synthesis mark (`next_synthesis += local / ratio`, `psola.rs:289`).
  Grains are Hann-windowed and overlap-added. A step in the ratio moves
  where future grains go, but can't break the waveform.
- `PeriodSmoother` (`period.rs:53`):
  - ignores unvoiced, NaN and zero estimates;
  - ignores octave jumps unless they persist for 3 hops;
  - snaps on jumps over 100¢ and smooths smaller moves (one-pole 0.5 in
    log period).

  Its `period()` is therefore already free of octave errors and
  out-of-range values.
- `WetGate` (`wet_gate.rs`) opens after 2 consecutive voiced hops and
  closes after 2 unvoiced ones, with a 10 ms equal-power fade.
- `note.rs` has `nearest_note`, `format_note` ("D4 +12¢") and
  `pack`/`unpack` for the audio → editor atomic. `NAMES` is private.
- `shift.rs` holds `ratio`, `SemitoneRamp`, `MIN`/`MAX`/`DEFAULT` and
  `format_shift`/`parse_shift`. The last four exist only for the `Shift`
  parameter and the `--shift` flag.
- The hop is 256 samples at 44.1 and 48 kHz (5.8 / 5.3 ms). The latency
  is 1103 / 1200 samples (25 ms). The ratio is chosen at input time n
  and applied to output time n − L, so the stabiliser's decisions reach
  the output 25 ms "early" relative to the audio they describe.
- The 0005 tests assert fixed shifts at −12, −5, +4, +7 and +12, a
  −12 → +12 sweep, levels and fixture scores. With no `Shift` at all,
  these would have nothing to drive them.

## Desired End State

- The plugin has `Gain`, `Key` (C…B, default C), `Scale` (Major / Natural
  minor, default Major), `Glide` (0–250 ms, default 50) and `Mix`. Singing
  a C major scale with Key C gives a diatonic third above each note.
- The editor shows "D4 → F4" (stable note → harmony note), with the
  detected note and cents in a smaller line under it.
- `mise run analyse in.wav out.wav --key C --scale major --glide 50
  --mix 100` writes the WAV and a CSV with `stable_note`, `interval` and
  `harmony_note` columns.
- `cargo test` includes the exhaustive interval test and the harmony
  tests. All 0005 tests still pass with unchanged scores (via
  `IntervalSource::Fixed`).
- The latency is unchanged: 1103 / 1200 samples.
- The README documents everything, and the ROADMAP's Phase 4 is ticked.
  Phase 5 is ticked once verified, with the out-of-key decision
  recorded.

Verify with `mise run check`, the printed scores
(`cargo test -p corral-dsp --test harmony -- --nocapture`) and the manual
Live 10 checks in Phases 6 and 7.

### Key Discoveries:

- Ratio changes only affect grain spacing (`psola.rs:289`). **Decision:
  there is no minimum ramp**, so Glide = 0 is an instant jump. The
  Glide-0 click tests in Phase 4 verify this. The contingency, only if
  they fail, is a 5 ms floor on the glide time constant, with the
  measured jump recorded here.
- The stabiliser reads the smoother's period (`period.rs:85`) on voiced
  hops. It then ignores octave errors and out-of-range estimates "in the
  same way as the period smoothing", with no extra code.
- "Voicing onset" is a voiced hop while `WetGate::is_open()` is false
  (checked after the hop's `update`). This picks the note on the first
  voiced hop, and the gate opens on the second. A single dropped hop
  mid-note doesn't close the gate, so it doesn't count as an onset.
- nice-plug's `#[derive(Enum)]` takes `#[name = "…"]` and a stable
  `#[id = "…"]` (`nice-plug-derive-0.1.2/src/lib.rs:7`), so `Key` and
  `Scale` can be `EnumParam`s with IDs that survive reordering.
- Because of the "note below" rule, each scale reduces to a 12-entry
  table indexed by pitch class relative to the key:
  - major `[4,4,3,3,3,4,4,4,4,3,3,3]`;
  - natural minor `[3,3,3,4,4,3,3,3,4,4,4,4]`.

## What We're NOT Doing

- Other scales and modes, intervals other than a third above, harmony
  below the voice, or several voices (Phase 6).
- Pitch-correcting the harmony. It stays the live input × a ratio, so
  vibrato and intonation carry through.
- Setting the key from MIDI, or detecting the key automatically.
- Migrating saved sets that used `Shift`. Hosts drop the unknown `shift`
  ID.
- Exposing `IntervalSource::Fixed` in the plugin or harness. It exists
  for the tests and for Phase 6.
- Changing the detector, the smoother, the wet gate or the shifter.

## Implementation Approach

Build bottom-up, as in 0005: first the pure functions, then the stateful
parts with unit tests on made-up per-hop pitches. Then wire them into
`Chain` while every 0005 test still passes. Then the objective harmony
tests and tuning. Finally the harness, the plugin, and the docs.

Two decisions were agreed during planning:

1. **`IntervalSource`.** `Settings` carries
   `IntervalSource::{Diatonic { key, scale }, Fixed(i32)}` instead of
   `shift`. The plugin and harness only build `Diatonic`. The 0005 tests
   use `Fixed(n)`, so their coverage and thresholds are kept, and
   Phase 6's interval choice has a place to go.
2. **A 30¢ settle check in the stabiliser.** A change is committed only
   when the hold (or the fast path) is satisfied *and* the pitch is
   within 30¢ of its nearest note. Why:
   - A linear 200¢ scoop over 150 ms spends 60 ms, exactly the hold, in
     the in-between semitone's >70¢ zone. Whether it commits that note
     depends on where the hops fall.
   - With the check, the hold is satisfied at about −50¢, between notes,
     so the stabiliser waits and commits the target instead. That leaves
     about 20¢ (about 2.6 hops) of margin.
   - For clean steps and vibrato it changes nothing: a pitch 70–130¢
     from the stable note is always within 30¢ of the neighbouring note.

**Glide shape:** `Glide` is a one-pole **time constant** τ in semitones:
after τ the shift has covered 63% of the change, and 95% after 3τ.
Antares' half-life was rejected for this reason:

- At the 50 ms default, a semitone change comes within 25¢ after
  1.39τ = 69 ms.
- Measured in input time, the semitone steps in the C major scale (E→F,
  B→C) change interval about 70 ms after the audio does. With τ, about
  40 ms of each such note falls outside the 100 ms exclusion, which
  gives roughly 96–97% within ±25¢ against the 95% limit.
- A 50 ms half-life (τ = 72 ms) needs 100 ms to come within 25¢, which
  gives roughly 94% and fails.

---

## Phase 1: Interval Logic

### Overview

Pure functions and tables for key, scale and the diatonic third, plus the
note-name helpers the readout and CSV need. No state, and no `nice_plug`.

### Changes Required:

#### 1. Harmony module

**File**: `corral-dsp/src/harmony.rs` (new; add `pub mod harmony;` to
`lib.rs`)

```rust
//! Key and scale → the diatonic third above a note, and where the
//! interval comes from.

use crate::note::NAMES;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scale {
    Major,
    NaturalMinor,
}

/// Semitones above the key of each scale degree.
const MAJOR_STEPS: [u8; 7] = [0, 2, 4, 5, 7, 9, 11];
const NATURAL_MINOR_STEPS: [u8; 7] = [0, 2, 3, 5, 7, 8, 10];

/// Semitones from each pitch class (relative to the key) up to the
/// diatonic third of its scale degree. A pitch class outside the scale
/// uses the scale note a semitone below: in both scales every chromatic
/// note sits between two scale notes, so "nearest" is always a tie.
const fn thirds(steps: [u8; 7]) -> [i32; 12] { /* const while-loops */ }

const MAJOR_THIRDS: [i32; 12] = thirds(MAJOR_STEPS);
const NATURAL_MINOR_THIRDS: [i32; 12] = thirds(NATURAL_MINOR_STEPS);

/// `key` is a pitch class, 0 = C … 11 = B. `midi` may be any note; the
/// octave is ignored.
pub fn third_above(key: u8, scale: Scale, midi: i32) -> i32 {
    let relative = (midi - i32::from(key)).rem_euclid(12) as usize;
    match scale {
        Scale::Major => MAJOR_THIRDS[relative],
        Scale::NaturalMinor => NATURAL_MINOR_THIRDS[relative],
    }
}

/// Where the harmony interval comes from. The plugin and harness use
/// `Diatonic`; `Fixed` keeps a whole-semitone shift for the shifter's
/// regression tests (and Phase 6's interval choice).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IntervalSource {
    Diatonic { key: u8, scale: Scale },
    Fixed(i32),
}

impl IntervalSource {
    /// Semitones from `midi` up to the harmony note.
    pub fn interval(self, midi: i32) -> i32 { … }
}

/// "C" … "B" (sharps), for the harness summary and CSV.
pub fn format_key(key: u8) -> &'static str;
/// Case-insensitive "C", "c#", "Db", "Bb", "E#"…; `None` otherwise.
pub fn parse_key(text: &str) -> Option<u8>;
/// "major"; "minor", "natural-minor" or "natural minor" (case-insensitive).
pub fn parse_scale(text: &str) -> Option<Scale>;
pub fn format_scale(scale: Scale) -> &'static str; // "major", "natural minor"
```

`thirds` finds the degree `d` with `steps[d] == rel`. Failing that, it
uses `steps[d] == rel − 1`, which always exists because 0 is in every
scale. It returns `(steps[(d + 2) % 7] + 12 − steps[d]) % 12`. `Fixed`
is only `debug_assert!`ed to be within ±12.

Unit tests in `harmony.rs`:

- **Exhaustive.** For all 12 keys × 2 scales × 12 pitch classes (and
  three octaves of `midi`), `third_above` equals a **hand-written**
  `REFERENCE: [[i32; 12]; 2]` (the two tables above, typed as literals)
  indexed by `(pc − key) mod 12`.
- **Named cases:**
  - C major: C→E, D→F, E→G, F→A, G→B, A→C, B→D;
  - A minor: A→C, B→D, C→E, D→F, E→G, F→A, G→B;
  - out of key in C major: C#→F, Eb→F#, F#→A#, G#→C, Bb→C#.
- **Negative `midi`** uses `rem_euclid` correctly.
- **`Fixed(n)`** returns `n` for any note.
- **Parsing:** `parse_key`/`format_key` round-trip over 0..12. Flats and
  lower case are accepted, and "H", "", "C##" and "Cb5" are rejected.
  The `parse_scale` forms are accepted and "dorian" is rejected.

#### 2. Note helpers

**File**: `corral-dsp/src/note.rs`

- Make `NAMES` `pub(crate)`.
- Add `pub fn format_name(midi: i32) -> String` ("D4"), and use it
  inside `format_note`.
- Add `pub fn midi_from_hz(f0_hz: f32) -> f32`, the fractional MIDI
  note (`69 + 12·log2(f0/440)`), for the stabiliser.
- Add `pub fn pack_pair(pair: Option<(i32, i32)>) -> u32` and
  `unpack_pair`: two `i16`s in one `u32`, with `UNVOICED` meaning none.
  They carry (stable note, harmony note) to the editor.

Tests: `format_name` on octave boundaries and sharps; `midi_from_hz`
exact on the equal-tempered notes; `pack_pair` round-trips over MIDI
40–95 (harmony up to B5 + 12).

### Success Criteria:

#### Automated Verification:

- [x] Interval and note tests pass: `cargo test -p corral-dsp --lib harmony note`
- [x] `mise run check` passes

#### Manual Verification:

- [ ] The hand-written reference table was typed from music theory, not
      copied from the implementation's output

---

## Phase 2: Note Stabiliser and Glide

### Overview

The two stateful parts, allocation-free, each unit-tested on made-up
input. `SemitoneRamp` and the `Shift`-only helpers are removed.

### Changes Required:

#### 1. Note stabiliser

**File**: `corral-dsp/src/stabiliser.rs` (new; `pub mod stabiliser;`)

```rust
//! Chooses the note the harmony interval is picked from, once per hop,
//! with hysteresis so vibrato and scoops don't make it flicker.

/// A change needs the pitch this far from the stable note's centre…
const CHANGE_CENTS: f32 = 70.0;
/// …for at least this long…
const HOLD_S: f32 = 0.060;
/// …or this far for this many hops (the fast path)…
const FAST_CENTS: f32 = 150.0;
const FAST_HOPS: u32 = 3;
/// …and only once the pitch is within this of its nearest note, so a
/// scoop can't commit to a note it is passing through (see the plan).
const SETTLE_CENTS: f32 = 30.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Update { Held, Onset, Changed }

pub struct NoteStabiliser {
    note: Option<i32>,
    /// Consecutive hops beyond CHANGE_CENTS, and beyond FAST_CENTS.
    held: u32,
    fast: u32,
    /// ⌈HOLD_S · sr / hop⌉: 11 at 44.1 kHz (63.9 ms), 12 at 48 kHz (64 ms).
    hold_hops: u32,
}

impl NoteStabiliser {
    pub fn new(sample_rate: f32, hop_len: usize) -> Self;
    pub fn reset(&mut self);
    /// The stable MIDI note, if one has been chosen since reset.
    pub fn note(&self) -> Option<i32>;
    /// Feeds one voiced hop's pitch (fractional MIDI). `onset` is true
    /// when no harmony is sounding (the wet gate is closed): the nearest
    /// note is then taken at once.
    pub fn update(&mut self, pitch: f32, onset: bool) -> Update;
}
```

`update`:

1. `nearest = pitch.round() as i32`.
2. If `onset` or there is no note yet: set `note = nearest`, clear the
   counters, and return `Onset`.
3. `deviation = 100 · |pitch − note|`. If it is ≤ `CHANGE_CENTS`, clear
   the counters and return `Held`.
4. `held += 1`. `fast = if deviation > FAST_CENTS { fast + 1 } else { 0 }`.
5. `settled = 100 · |pitch − nearest| ≤ SETTLE_CENTS`.
6. If `settled && (held ≥ hold_hops || fast ≥ FAST_HOPS)`: set
   `note = nearest`, clear the counters, and return `Changed`. Otherwise
   return `Held`.

Unit tests (per-hop pitch sequences, 48 kHz, hop 256):

- **Onset** takes the nearest note on the first update: 60.4 → 60 and
  60.6 → 61. A later `onset = true` re-picks at once.
- **Hold path:** a 100¢ step (60 → 61.0) changes on exactly the
  `hold_hops`-th hop beyond 70¢, and not one hop earlier.
- **Fast path:** a 200¢ step changes on the 3rd hop.
- **Vibrato:** a sinusoid of ±100¢ at 4.5, 5, 6 and 7 Hz, sampled per
  hop for 3 s, never changes the note. A ±69¢ one never even starts the
  counter.
- **Brief excursions:** 1–10 hops at +100¢ followed by a return reset
  the counter, and the note stays.
- **Settle check:** pitch at +150¢ held (between two notes) never
  commits. Moving it to +180¢ commits 62 as soon as it settles.
- **Linear scoop:** −200¢ → 0 over 150 ms at both hop lengths and every
  hop phase (offset the ramp by 0–255 samples). It gives at most one
  change, and ends on the target.
- `reset` forgets the note.

#### 2. Glide

**File**: `corral-dsp/src/shift.rs` (rewritten)

```rust
//! Semitones → ratio, and the glide between harmony intervals.

pub const DEFAULT_GLIDE_S: f32 = 0.050;
pub const MAX_GLIDE_S: f32 = 0.250;
/// Closer than this (0.01¢) and the glide lands exactly on its target.
const SNAP_SEMITONES: f32 = 1e-4;

pub fn ratio(semitones: f32) -> f32; // unchanged

/// Per-sample one-pole coefficient for a glide time constant (seconds).
/// 0.0 (an instant jump) for `glide_s <= 0`.
pub fn glide_coefficient(glide_s: f32, sample_rate: f32) -> f32 {
    if glide_s > 0.0 { (-1.0 / (glide_s * sample_rate)).exp() } else { 0.0 }
}

/// Exponential glide in semitones (log frequency). `Glide` is its time
/// constant: 63% of any change after `glide_s`, 95% after 3 × `glide_s`.
pub struct Glide { current: f32 }

impl Glide {
    pub fn new(initial: f32) -> Self;
    /// Lands on `target` at once (voicing onset, reset).
    pub fn jump(&mut self, target: f32);
    pub fn current(&self) -> f32;
    /// Advances one sample toward `target` and returns the value.
    #[inline]
    pub fn next(&mut self, target: f32, coefficient: f32) -> f32 {
        self.current = target + coefficient * (self.current - target);
        if (self.current - target).abs() < SNAP_SEMITONES {
            self.current = target;
        }
        self.current
    }
}
```

Delete `SemitoneRamp`, `MIN`, `MAX`, `DEFAULT`, `format_shift`,
`parse_shift` and their tests.

Tests:

- `ratio` exactness (kept).
- After `n = round(τ·sr)` samples a 1-semitone change is 63.2 ± 0.5%
  done, at 44.1/48/96 kHz.
- The glide is monotonic with no overshoot, and lands exactly on the
  target (`== target`).
- The largest per-sample step is `1 − coefficient` for a 1-semitone
  change.
- A coefficient of 0 jumps in one sample.
- `jump` lands at once.
- A target change mid-glide continues smoothly from where it is.

### Success Criteria:

#### Automated Verification:

- [x] Stabiliser and glide tests pass: `cargo test -p corral-dsp --lib stabiliser shift`
- [x] `mise run lint` passes (Phase 3 fixes the callers; until then the
      workspace won't build, so Phases 2 and 3 land in one commit)

#### Manual Verification:

- [x] The hop counts printed by a debug test (11 / 12) match the comment

#### Implementation notes (Phase 2)

- **Glide state is an offset from the target.** The sketched
  `current = target + c·(current − target)` stalls in f32: near 12
  semitones a 250 ms glide at 96 kHz moves less than half an ulp per
  sample once it is about 1¢ away, above `SNAP_SEMITONES`, so it never
  lands. `Glide` stores `offset = current − target` and decays that
  (`offset *= c`), which keeps its relative precision. Same maths and
  API; a jump to 4.0 still gives exactly 4.0.
- **`MIN`/`MAX`/`DEFAULT`/`format_shift`/`parse_shift` are kept** until
  their last callers go: the plugin's `Shift` (Phase 6) and the
  harness's `--shift` (Phase 5). Both now drive
  `IntervalSource::Fixed(n)` at the default glide, rather than a
  hard-coded `Fixed(4)`, so they keep working in between.
  `SemitoneRamp` is gone.
- Vibrato margin, per unit test: the longest run beyond 70¢ for ±100¢ at
  4.5 Hz is 10 of 11 hops (44.1 kHz) and 11 of 12 (48 kHz); 5 Hz is
  9 and 10; 7 Hz is 7 at both.

---

## Phase 3: Chain Integration

### Overview

`Chain` chooses the interval from the stable note and glides the shift
toward it. Every existing caller moves to the new `Settings`, and all
0005 tests pass with unchanged scores.

### Changes Required:

#### 1. Settings and Chain

**File**: `corral-dsp/src/chain.rs`

```rust
/// Per-frame controls. The plugin smooths `gain` and `mix`; `Chain`
/// glides the interval itself.
pub struct Settings {
    pub gain: f32,
    pub mix: f32,
    pub interval: IntervalSource,
    /// Glide time constant in seconds, 0.0..=shift::MAX_GLIDE_S.
    pub glide_s: f32,
}

/// The stable note and the interval chosen for it; the harmony note is
/// `note + interval`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Harmony { pub note: i32, pub interval: i32 }
```

New `Chain` fields:

- `stabiliser: NoteStabiliser`, built with `(sample_rate, tracker.hop_len())`;
- `glide: Glide`, replacing `ramp`;
- `sample_rate: f32`;
- `glide_s: f32` and `glide_coefficient: f32`, a cache recomputed only
  when `settings.glide_s` changes, so `exp` isn't called every sample;
- `harmony: Option<Harmony>`.

`process_frame`, per hop:

```rust
if let Some(d) = &detection {
    self.smoother.update(d);
    self.gate.update(d.voiced);
    if d.voiced && let Some(period) = self.smoother.period() {
        let pitch = note::midi_from_hz(self.sample_rate / period);
        // Onset: no harmony is sounding, so the note is taken at once
        // and the shift jumps to its interval (no glide after silence).
        let onset = !self.gate.is_open();
        if self.stabiliser.update(pitch, onset) == Update::Onset {
            let note = self.stabiliser.note().expect("just chosen");
            self.glide.jump(settings.interval.interval(note) as f32);
        }
    }
}
```

Per sample, replacing the ramp line:

```rust
self.harmony = self.stabiliser.note().map(|note| Harmony {
    note,
    interval: settings.interval.interval(note),
});
if settings.glide_s != self.glide_s { /* recompute the coefficient */ }
let semitones = match self.harmony {
    Some(h) => self.glide.next(h.interval as f32, self.glide_coefficient),
    None => self.glide.current(),
};
```

A `Key`/`Scale` change mid-note changes `interval`, so the shift glides
to the new interval at the `Glide` rate. `reset()` also resets the
stabiliser, jumps the glide to 0.0 and clears `harmony`. New accessors:

- `pub fn harmony(&self) -> Option<Harmony>`: the latest choice. It is
  kept through unvoiced hops, and callers blank it for display.
- `pub fn shift_semitones(&self) -> f32`: the glide's value, for tests.

Update the module doc to mention the stabiliser and glide.

#### 2. Existing callers

- `tests/common/mod.rs`: `DRY` becomes `interval: Fixed(0)`,
  `glide_s: DEFAULT_GLIDE_S`. Add `pub fn fixed(shift: i32, mix: f32) ->
  Settings` and `pub fn diatonic(key, scale, glide_s, mix) -> Settings`.
- `tests/shifter.rs`, `tests/fixtures.rs` and `tests/latency.rs`:
  `shift: n` → `interval: IntervalSource::Fixed(n)`,
  `glide_s: DEFAULT_GLIDE_S`. The sweep uses
  `(-12 + i / step_len).min(12)` directly instead of `shift::MIN`/`MAX`.
- `analyse/src/main.rs` and `corral/src/lib.rs`: keep them compiling
  with `Fixed(4)` for now. Phases 5 and 6 replace this.

**Expected: identical +4 output.** Before, the ramp sat at `DEFAULT = 4`
and never moved. Now the glide jumps to exactly 4.0 at onset, and
`next(4.0, c)` stays bit-exact. So every +4 score, the fixture table and
the latency figures must print exactly the same numbers as in the
README. The −12 → +12 sweep now glides exponentially instead of linearly
over 50 ms. Its click figure is re-printed, and it must stay under the
same threshold.

#### 3. No-alloc test

**File**: `corral-dsp/tests/no_alloc.rs`

- Keep the existing passes, using `Fixed`.
- Add a diatonic pass: every 2000 samples, `key` cycles 0..12 and
  `scale` alternates, while `glide_s` sweeps 0 → 0.25 over a scale
  signal (`common::scale`). This exercises onsets, changes and glide
  coefficient recomputes.

### Success Criteria:

#### Automated Verification:

- [x] All 0005 tests pass: `cargo test -p corral-dsp`
- [x] +4, fixture and latency figures are unchanged:
      `cargo test -p corral-dsp --test shifter --test fixtures --test latency -- --nocapture`
      (compare with the README tables)
- [x] No allocation: `cargo test -p corral-dsp --test no_alloc`
- [x] `mise run check` passes

#### Manual Verification:

- [x] The re-printed Shift-sweep click figure is noted for Phase 7

#### Phase 3 results (for Phase 7's README)

Compared line by line with the `--nocapture` output at `d6255b4`:

- Every +4 figure, the fixture table and the latency figures are
  identical.
- The Shift-sweep click figures are 0.1046 at 44.1 kHz (unchanged,
  threshold 0.2244) and **0.0962 at 48 kHz** (was 0.0690, threshold
  0.2062).
- Only the non-+4 fixed shifts at 48 kHz moved, and only slightly.
  Before, they started with a 50 ms ramp from the old `DEFAULT = 4`;
  now the glide jumps straight to the interval at onset:
  - A3 +7 median +0.4¢ → +0.0¢;
  - level scale +7 −2.95 → −2.94 dB, −12 −3.48 → −3.47 dB, −5 −1.78 →
    −1.79 dB.

---

## Phase 4: Objective Harmony Tests and Tuning

### Overview

A new `corral-dsp/tests/harmony.rs` covers the work item's acceptance
criteria at 44.1 and 48 kHz and prints every score. Then tune if needed,
and record the results here.

### Changes Required:

#### 1. Helpers

**File**: `corral-dsp/tests/common/mod.rs`

- `pub fn note_sequence(sample_rate, midis: &[f64], note_s: f64) ->
  Vec<f32>`: a legato, phase-continuous 12-harmonic tone at peak 0.5,
  built with `harmonic_tone`.
- `pub struct Trace { pub output: Vec<f32>, pub hops: Vec<(Detection,
  Option<Harmony>)>, pub shifts: Vec<f32> }` and `pub fn
  process_traced(signal, sample_rate, settings: impl Fn(usize) ->
  Settings) -> Trace`. These record `chain.harmony()` on each hop and
  `chain.shift_semitones()` on each sample.
- Move `CLICK_MARGIN` and `assert_no_clicks(name, signal, output,
  max_semitones: f32)` here from `shifter.rs`, unchanged: the margin is
  0005's calibration. `shifter.rs` imports them.

#### 2. The tests

**File**: `corral-dsp/tests/harmony.rs`

Constants, following 0005:

```rust
const SAMPLE_RATES: [f32; 2] = [44_100.0, 48_000.0];
const TOLERANCE_CENTS: f64 = 25.0;
const SHARE_WITHIN: f64 = 0.95;
const EDGE_S: f64 = 0.15;               // as shifter.rs
const TRANSITION_EXCLUSION_S: f64 = 0.1;
const NOTE_S: f64 = 0.5;
```

1. **`c_major_scale_gets_diatonic_thirds`**:
   - Input: C4 D4 E4 F4 G4 A4 B4 C5 (60…72), 0.5 s each, Key 0 Major,
     Mix 1.0, Glide default.
   - Reference frames every 10 ms at input f0 × 2^(third/12), with the
     thirds hand-written as `[4, 3, 3, 4, 4, 3, 3, 4]`. Frames within
     `EDGE_S` of either end or 100 ms of a note change are dropped.
   - Score with `output_estimates` + `cents_stats(ratio = 1.0)`.
   - Assert share ≥ 0.95. Print the median and share, and the same at
     Glide 0 (printed only).
2. **`a_minor_scale_gets_diatonic_thirds`**: the same for A3…A4
   (57 59 60 62 64 65 67 69) with Key 9, Natural minor, and thirds
   `[3, 3, 4, 3, 3, 4, 4, 3]`.
3. **`vibrato_never_changes_the_note`**:
   - Input: C4 for 2 s after 0.3 s of silence, with ±100¢ vibrato at
     5 Hz and at 7 Hz.
   - The vibrato's phase is centred on the onset: `sin(2πf(t − t₀ −
     12.5 ms))`. The first voiced hop hears about the first 25 ms of the
     note, so centring its phase there makes that hop hear C4 itself.
     The onset rule takes whatever note the first hop hears, so a
     vibrato caught at +60¢ would legitimately start on C#.
   - Assert: from the first hop with a harmony on, every voiced hop's
     `Harmony` is `{ note: 60, interval: 4 }`. Print the number of
     changes (0).
4. **`a_scoop_settles_once_on_the_target`**:
   - Input: E4 (64) in C major after 0.3 s of silence. It rises
     linearly from −200¢ over 50, 100 and 150 ms, then holds for 1 s.
   - Assert: the final note is 64, and there is at most one `note`
     change after the first. Print the onset note, the change count and
     the time of the change.
5. **`onset_after_silence_jumps_without_glide`**:
   - Input: 0.3 s silence, C4 for 0.6 s, 0.3 s silence, D4 for 0.6 s,
     at Glide 0.25 s (so a wrongly applied glide would be obvious).
   - Assert for each note: the first voiced hop in it has `harmony.note`
     equal to the sung note.
   - From that hop's sample on, `shifts` equals the note's interval
     exactly (4.0, then 3.0), with no intermediate values.
6. **`legato_step_changes_quickly_and_glides`**:
   - Input: C4 for 1 s → D4 for 1 s, legato, C major, Glide 0.05.
   - Let `h_d` be the first hop after the step where the raw detection
     is within 50¢ of D4, and `h_c` the hop where `harmony.note`
     becomes 62.
   - Assert `(h_c − h_d)·hop_s ≤ 0.060 + 3·hop_s`, and print the value.
     The fast path should give 3 hops.
   - After `h_c`, `shifts` falls monotonically from 4 to 3, and every
     per-sample step is ≤ `1.001 · (1 − glide_coefficient(0.05, sr))`,
     so there is no jump.
7. **`out_of_key_notes_use_the_note_below`**:
   - Input: C#4 Eb4 F#4 G#4 Bb4 (61 63 66 68 70), legato, 0.5 s each,
     C major, Mix 1.0.
   - Per note, the median of the output estimates within ±25¢ of the
     expected harmony: F4, F#4, A#4, C5, C#5 (65 66 70 72 73). Frames
     are the note's own, excluding 100 ms at each change and `EDGE_S`
     at the ends.
   - "Never drops out": every output estimate in those frames is voiced
     (print the share, 1.000). No hop has `harmony == None` after the
     first voiced hop.
8. **`no_clicks_on_interval_key_scale_and_glide_changes`**. The
   threshold is `CLICK_MARGIN · ratio(4) · max_jump(dry)` from 0005, in
   all four cases:
   - the C major scale at Glide 0.05 and at 0;
   - A3 held for 5 s, with `key` stepping through all 12 every 250 ms
     and `scale` flipping every 500 ms, at Glide 0 and 0.05;
   - the C major scale with `glide_s` swept linearly 0 → 0.25 s across
     the signal, while `key` alternates 0 ↔ 7 every 200 ms so the glide
     keeps moving.

   Print each figure.

The latency is unaffected by construction: `Chain::latency()` depends
only on the sample rate. `tests/latency.rs` keeps asserting it. Also
add one diatonic run (Key 0, Mix 1.0) to
`reported_latency_matches_the_measured_delay`.

#### 3. Tuning

If any assertion misses:

- Measure first, and print which frames fail. They should be in the
  transitions.
- The levers, in order:
  1. `SETTLE_CENTS` (30);
  2. `HOLD_S` (60 ms), kept ≥ 55 ms so 5 Hz ±100¢ vibrato (about 51 ms
     beyond 70¢) is still rejected;
  3. the transition exclusion (the work item's 100 ms).
- Every change carries a comment with the measured score and the reason
  (the work item allows this, and 0005 did the same). It is also
  recorded in the results below.
- If a Glide-0 click test fails, add the 5 ms glide floor (see Key
  Discoveries) and record the before and after figures.

### Success Criteria:

#### Automated Verification:

- [ ] Harmony tests pass and print scores:
      `cargo test -p corral-dsp --test harmony -- --nocapture`
- [ ] Shifter tests still pass after moving the click helpers:
      `cargo test -p corral-dsp --test shifter`
- [ ] `mise run check` passes

#### Manual Verification:

- [ ] Listen to the scale output (`mise run analyse` in Phase 5, or a
      test-written WAV): the thirds sound right and the glides are
      smooth
- [ ] Results recorded below

#### Phase 4 results (for Phase 7's README)

_To be filled in during implementation:_

- the scale shares at 44.1 and 48 kHz;
- the vibrato change counts;
- the scoop change counts and times;
- the legato change delay;
- the out-of-key medians;
- the click figures;
- any tuning, with its reason.

---

## Phase 5: Harness Flags and CSV

### Overview

The harness drops `--shift` and gains `--key`, `--scale` and `--glide`.
The CSV gains the harmony columns.

### Changes Required:

**File**: `analyse/src/main.rs`

- `USAGE`: `analyse <in.wav> [out.wav] [--gain-db <dB>] [--key <C..B>]
  [--scale <major|minor>] [--glide <ms>] [--mix <percent>]`.
- `Options { gain_db, key: u8, scale: Scale, glide_ms: f32, mix_percent }`.
  The defaults match the plugin: C, Major, 50 ms, 50%.
- `parse_args`:
  - `--key` uses `harmony::parse_key`;
  - `--scale` uses `harmony::parse_scale`;
  - `--glide` accepts a finite value in `0.0..=250.0`;
  - `--shift` is now an unknown flag, so it is rejected.
- `Settings { interval: Diatonic { key, scale }, glide_s: glide_ms /
  1000.0, … }`.
- CSV header: `time_s,f0_hz,confidence,voiced,stable_note,interval,harmony_note`.
  On voiced hops the new columns are filled from `chain.harmony()`,
  e.g. `D4,+3,F4` (via `note::format_name`). On unvoiced hops they are
  empty.
- The summary line reads `latency … compensated; key C major, glide 50
  ms, mix 100%`.

Tests:

- `csv_has_one_row_per_hop`: 7 columns. On the voiced 220 Hz rows the
  new columns are `A3,+3,C4` (A is vi in C major).
- `mix_100_output_is_shifted`: expect 220 × `ratio(3.0)` at the default
  key.
- `defaults_match_the_plugin`: key 0, Major, 50 ms, 50%.
- `key_scale_and_glide_flags_parse`: `--key Db` → 1, `--key a#` → 10,
  `--scale minor` and `--scale natural-minor` → `NaturalMinor`,
  `--glide 0` and `--glide 250`.
- `bad_arguments_are_rejected`: replace the `--shift` cases with
  `--shift 4` (now unknown), `--key H`, `--key`, `--scale dorian`,
  `--glide 251`, `--glide -1`, `--glide NaN` and `--glide`.
- Unchanged: the bit-identical, length and fixture pass-through tests,
  now with the new `Options`.

**File**: `mise.toml`: update the `analyse` task description to the new
usage.

### Success Criteria:

#### Automated Verification:

- [ ] Harness tests pass: `cargo test -p analyse`
- [ ] `mise run analyse corral-dsp/tests/fixtures/vocadito_7_vibrato.wav target/analyse/v.wav --key C --scale major --glide 50 --mix 100`
      writes `v.wav` and `v.csv` with the three new columns
- [ ] `mise run check` passes

#### Manual Verification:

- [ ] Scanning `v.csv`: the stable note holds through the vibrato while
      `f0_hz` swings

---

## Phase 6: Plugin Parameters and Editor

### Overview

`Key`, `Scale` and `Glide` replace `Shift` in the plugin. The editor gets
their controls and the "D4 → F4" readout.

### Changes Required:

#### 1. Parameters

**File**: `corral/src/lib.rs`

```rust
/// The IDs are stored in saved sets and automation, so they must never change.
#[derive(Enum, Clone, Copy, Debug, PartialEq, Eq)]
enum KeyParam {
    #[id = "c"] #[name = "C"] C,
    #[id = "c-sharp"] #[name = "C#"] CSharp,
    // … D, D#, E, F, F#, G, G#, A, A#, B
}

#[derive(Enum, Clone, Copy, Debug, PartialEq, Eq)]
enum ScaleParam {
    #[id = "major"] #[name = "Major"] Major,
    #[id = "natural-minor"] #[name = "Natural minor"] NaturalMinor,
}
```

- `KeyParam` → pitch class via `as u8`: the variants are declared C…B,
  so the discriminants are 0…11. A unit test in the plugin crate checks
  every variant.
- `impl From<ScaleParam> for harmony::Scale`.
- `CorralParams` fields:
  - `key: EnumParam<KeyParam>` (`#[id = "key"]`, default C);
  - `scale: EnumParam<ScaleParam>` (`#[id = "scale"]`, default Major);
  - `glide: FloatParam` (`#[id = "glide"]`): `"Glide"`, 50.0,
    `Linear { 0.0, 250.0 }`, unit `" ms"`,
    `formatters::v2s_f32_rounded(0)`. It is not smoothed; `Chain`
    caches the coefficient.
- Remove the `shift` field. `process()` builds
  `Settings { interval: Diatonic { key, scale }, glide_s: glide / 1000.0, … }`.
- Update the struct's doc to "Stereo gain, pitch detection and a
  key-aware diatonic harmony".

#### 2. Readout

- Add `harmony_readout: Arc<AtomicU32>` and `last_harmony_readout: u32`,
  alongside `note_readout`.
- At the end of each buffer, on the last hop, publish
  `note::pack_pair(chain.harmony().filter(|_| detection.voiced).map(|h|
  (h.note, h.note + h.interval)))`. Like the note, it is stored only
  when it changes, and triggers a repaint only while the editor is open.

#### 3. Editor

**File**: `corral/src/editor.rs`

- Sliders in order: Gain (plus the gain readout), Key, Scale, Glide,
  Mix.
- Then a large monospace line: `"D4 → F4"`, or `"—"`.
- Under it, a small line with the detected note (`format_note`, e.g.
  `"D4 +12¢"`), so the stabiliser can be compared with the detector.
- `WINDOW_SIZE` grows to about 300 × 340, so every row fits (checked by
  eye).
- Update the module doc.

### Success Criteria:

#### Automated Verification:

- [ ] `cargo test -p corral` passes (the key-variant mapping test)
- [ ] `mise run build-universal` succeeds
- [ ] `mise run check` passes

#### Manual Verification:

- [ ] `mise run install`, then Option-click Rescan in Live 10 (Rosetta,
      x86_64 slice). Corral shows Gain, Key, Scale, Glide and Mix, with
      no Shift
- [ ] Key C, Major, Mix 50%: singing a C major scale gives a diatonic
      third above each note, and the readout shows e.g. "D4 → F4"
- [ ] Held notes with vibrato: the harmony and the readout's arrow line
      don't flicker
- [ ] Changing Key, Scale and Glide while singing: no audible clicks,
      and the latency Live reports stays at 1103 / 1200 samples
- [ ] Out-of-key notes (e.g. sing F#) still produce a harmony (A#)

---

## Phase 7: Documentation and Roadmap

### Overview

The README, the design doc and the ROADMAP catch up.

### Changes Required:

**File**: `README.md`

- A new **Harmony** section after Pitch shifting:
  - `Key` and `Scale` (major and natural minor);
  - the interval tables;
  - the out-of-key rule, with the C major examples (C#→F, F#→A#,
    Eb→F#) and why ("nearest" is always a tie);
  - that the harmony never drops out on out-of-key notes.
- **Stabiliser:** 70¢ for 60 ms (11 / 12 hops), the fast path of 150¢
  for 3 hops, the 30¢ settle check and why it was added, and onset
  picking the note at once.
- **`Glide`:** 0–250 ms, default 50, as a time constant (63% after the
  set time, 95% after 3×). It jumps at onset. There is no minimum ramp,
  and why that doesn't click.
- **Harmony accuracy:** the Phase 4 scores and how to print them.
- **Pitch shifting:** replace the `Shift` bullet (the harmony interval
  now comes from Key and Scale, see Harmony). Change "doesn't change
  with Shift, Mix" to "Key, Scale, Glide, Mix". Replace the "Old Live
  sets" note: sets saved with Shift load with its value dropped, at
  Key C Major. Keep the shifter accuracy figures, noting that the tests
  drive the chain at fixed intervals internally. Update the Shift-sweep
  click figure from Phase 3.
- **Offline harness:** the new example command, the flags, and the
  three CSV columns.
- **Task table:** the new `analyse` description.

**File**: `docs/td-psola.md`: replace the "Shift ramp" bullet with the
glide (a one-pole in semitones, chosen by the stabiliser's note). Point
to the README's Harmony section.

**File**: `ROADMAP.md`

- Tick every Phase 4 item.
- Tick the Phase 5 items once the automated and manual checks pass.
- Annotate the out-of-key item: "Decision (0006): an out-of-key note
  uses the interval of the scale note a semitone below (C#→F in C
  major); the harmony never mutes."
- Note in Phase 5 that the only scales are major and natural minor
  ("others" stay open).

### Success Criteria:

#### Automated Verification:

- [ ] `mise run check` passes

#### Manual Verification:

- [ ] The README's figures match the latest `--nocapture` output
- [ ] The Phase 5 end-to-end item is ticked only after the Live check in
      Phase 6

---

## Testing Strategy

### Unit Tests:

- **`harmony.rs`:**
  - exhaustive 12 × 2 × 12 against a hand-written table;
  - the named cases;
  - `Fixed`;
  - parsing.
- **`stabiliser.rs`:**
  - onset;
  - hold and fast timing to the hop;
  - vibrato at 4.5–7 Hz;
  - brief excursions;
  - the settle check;
  - a linear scoop at every hop phase.
- **`shift.rs`:**
  - the time constant;
  - monotonic, exact landing;
  - maximum step;
  - jump;
  - retargeting.
- **`note.rs`:** `format_name`, `midi_from_hz`, `pack_pair`.

### Integration Tests:

- **`tests/harmony.rs`** covers every audio acceptance criterion at
  44.1 and 48 kHz, printing its scores.
- **The 0005 suites** (`shifter`, `fixtures`, `latency`, `synthetic`)
  run through `IntervalSource::Fixed` with unchanged +4 figures.
- **`no_alloc`** includes diatonic key/scale/glide sweeps.
- **The harness tests** cover the flags and the CSV columns.

### Manual Testing Steps:

1. `mise run analyse` a fixture at Key C Major, Mix 100, and listen to
   the output and read the CSV.
2. In Live 10: sing a C major scale at Key C, Mix 50%, and hear the
   thirds.
3. Hold a note with wide vibrato: no flicker.
4. Scoop into notes: the harmony settles without a double change.
5. Sweep Key, Scale and Glide while singing: no clicks, and the latency
   is unchanged.
6. Sing chromatic passing notes: the harmony follows the note below's
   interval and never drops out.

## Performance Considerations

- The stabiliser runs once per hop, with a handful of float operations
  and no allocation.
- The interval is a table lookup per sample.
- The glide is one multiply-add per sample. Its `exp` runs only when
  `Glide` changes.
- There is no change to buffer sizes or latency.

## Migration Notes

- `Shift` (ID `shift`) is removed. Hosts ignore its stored value, and
  such sets load at Key C, Major, Glide 50 ms. There are no users yet
  (work item assumption).
- The `--shift` harness flag is removed. Scripts using it get the usage
  error.

## References

- Original work item: `meta/work/0006-key-aware-diatonic-harmony.md`
- Previous plan: `meta/plans/2026-09-28-0005-realtime-td-psola-pitch-shifter.md`
- Chain: `corral-dsp/src/chain.rs:86`
- Grain spacing by ratio: `corral-dsp/src/psola.rs:289`
- Period smoother: `corral-dsp/src/period.rs:53`
- Wet gate: `corral-dsp/src/wet_gate.rs:38`
- Click calibration: `corral-dsp/tests/shifter.rs:26`, `:209`
- nice-plug `Enum` derive: `nice-plug-derive-0.1.2/src/lib.rs:7`
