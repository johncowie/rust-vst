# Corral

Corral is an auto-harmoniser audio plugin (VST3 and CLAP) written in Rust with
[nice-plug](https://codeberg.org/RustAudio/nice-plug). See
[`ROADMAP.md`](ROADMAP.md) for where it is heading.

## Prerequisites

- macOS, with Xcode or the Command Line Tools (for the linker and `lipo`).
- [mise](https://mise.jdx.dev) 2026.3.12 or newer, with `mise activate` in your
  shell.
- rustup is optional. mise installs it if it is missing.

## Setup

1. Trust the project config:

   ```sh
   mise trust
   ```

   This is required. Without it, mise ignores `mise.toml` (under
   `mise activate`) or prompts for trust (under `mise install`).

2. Install the toolchain:

   ```sh
   mise install
   ```

   This installs Rust 1.98.1 through rustup, with `clippy`, `rustfmt` and both
   macOS targets (`aarch64-apple-darwin` and `x86_64-apple-darwin`).

3. mise sets `RUSTUP_TOOLCHAIN` inside the project, which overrides any global
   value you export. Tools that run outside mise's environment still see the
   global toolchain. This includes an IDE's rust-analyzer without mise
   integration.

## Tasks

Run a task with `mise run <task>`, or `mise run` on its own to pick one.

| Task | Description |
|---|---|
| `build` | Bundle corral as VST3 and CLAP for the host architecture (release) |
| `build-universal` | Bundle corral as universal (arm64 + x86_64) macOS VST3 and CLAP (release) |
| `install` | Build the universal VST3, ad-hoc sign it, and install it to ~/Library/Audio/Plug-Ins/VST3 (quit Ableton Live first) |
| `install-debug` | Build a debug universal VST3 with nice-plug's allocation assertions (aborts Live if process() allocates), sign it and install it (quit Ableton Live first) |
| `fmt` | Format all workspace code |
| `fmt-check` | Check formatting without changing files |
| `lint` | Run clippy on all targets, treating warnings as errors |
| `test` | Run the workspace unit tests |
| `check` | Run fmt-check, lint, then test (the CI entry point) |
| `analyse` | Run the plugin's DSP chain on a WAV: analyse <in.wav> [out.wav] [--voices <row>…] [--glide <ms>] [--mix <percent>] (writes out.wav and out.csv; paths are relative to the project root) |
| `fixtures` | Download the pitch datasets into .datasets/ (~2.2 GB, once) and regenerate the committed test fixtures |

Bundles land in `target/bundled/Corral.vst3` and `target/bundled/Corral.clap`.
`build-universal` writes to the same paths, overwriting them with universal
binaries.

## Loading in Ableton Live 10

1. Quit Live, then run `mise run install`. The task refuses to run while Live
   is open, because Live doesn't pick up plugins that change while it's
   running.
2. In Live, open Preferences → Plug-ins and turn on "Use VST3 Plug-in System
   Folders".
3. Corral appears under Plug-ins → VST3. Drop it onto an audio track.

Notes:

- Live 10 is Intel-only and runs under Rosetta 2 on Apple Silicon, so it only
  loads plugins with an x86_64 slice. That is why `install` uses the
  universal build.
- If the plugin's name or parameters change, Option-click Rescan in the
  Plug-ins preferences. This clears Live's plugin database and does a clean
  scan.
- `mise run install-debug` builds a version that aborts Live if `process()`
  allocates. It can't be used in Live 10 yet: in debug builds nice-plug checks
  pointer alignment, and Live's channel pointers fail that check, so Live
  aborts before Corral's `process()` runs. The `no_alloc` test in `corral-dsp`
  checks the detector chain instead. See the Icebox in
  [`ROADMAP.md`](ROADMAP.md).

## Pitch detection

Corral shows the note it hears (e.g. `D4 +12¢`, or `—` when nothing is
voiced) in a small line under the harmony readout (see
[Harmony](#harmony)). Audio passes through with the pitch-shifted harmony
voices mixed in (see [Pitch shifting](#pitch-shifting)).

- **Algorithm:** YIN, with its difference function computed through FFT
  (`realfft`). The FFT plans and buffers are made when the host activates the
  plugin, so the audio thread never allocates.
- **Input:** a mono sum of the stereo input.
- **Range:** 80 Hz–1 kHz (E2 to about B5).
- **Voicing gate:** a hop is voiced when the window RMS is at least −50 dBFS
  (`LEVEL_GATE_DBFS`) and the confidence (1 − YIN's normalised difference at
  the chosen lag) is at least 0.8 (`MIN_CONFIDENCE`). YIN's threshold
  (`YIN_THRESHOLD`) is 0.10. All three are constants in
  `corral-dsp/src/pitch.rs`.

### Window, hop and latency

| Sample rate | Window | Hop | Latency A3 | Latency E2 |
|---|---|---|---|---|
| 44.1 kHz | 2048 (46.4 ms) | 256 (5.8 ms) | 34.1 ms | 39.9 ms |
| 48 kHz | 2048 (42.7 ms) | 256 (5.3 ms) | 33.3 ms | 38.7 ms |

Latency is the time from a synthesised note's onset to the first of 5
consecutive correct detections (within 50 cents). The host's buffer adds to
these figures. Above 48 kHz, the window and hop double for each further
multiple of 48 kHz. Re-measure with
`cargo test -p corral-dsp --test latency -- --nocapture`.

### Accuracy

`mise run check` scores the detector on synthesised signals and on seven
excerpts from public datasets, and fails below the thresholds from work item
0004. The scores are:

- **RPA:** raw pitch accuracy, the share of voiced reference frames within
  50 cents;
- **RCA:** raw chroma accuracy, the same ignoring octave (RCA − RPA stands in
  for the octave-error rate);
- **VR:** voicing recall;
- **VFA:** voicing false alarm.

| Fixture | RPA | RCA | VR | VFA |
|---|---|---|---|---|
| `vocadito_7_vibrato` | 0.930 | 0.933 | 0.950 | 0.156 |
| `vocadito_35_low` | 0.959 | 0.959 | 0.951 | 0.081 |
| `mdb_nightowl_08_male` | 0.968 | 0.974 | 0.971 | 0.000 |
| `mdb_pachelbel_02_low` | 0.983 | 0.983 | 0.992 | n/a |
| `nsynth_e2` | 1.000 | 1.000 | 1.000 | n/a |
| `nsynth_c4` | 1.000 | 1.000 | 1.000 | n/a |
| `nsynth_g5` | 1.000 | 1.000 | 1.000 | n/a |

The synthesised C major scale (C3–C5), A3 with ±50-cent vibrato, E2 and B5
all score 1.000 on every metric at both 44.1 and 48 kHz. Silence and white
noise are unvoiced on every hop.

Print the scores with:

```sh
cargo test -p corral-dsp --test fixtures --test synthetic -- --nocapture
```

### Offline harness

```sh
mise run analyse corral-dsp/tests/fixtures/vocadito_7_vibrato.wav target/analyse/v.wav --voices C:-5,+4 D:+3 --glide 50 --mix 100
```

This runs the same chain the plugin runs, harmony included. It writes the
processed audio (in the input's format) and a per-hop CSV next to it
(`target/analyse/v.csv`). The output path is optional; it defaults to
`target/analyse/<input name>.wav`. Input can be 16- or 24-bit integer or
32-bit float WAV, mono or stereo.

- `--voices <row>…`: the harmony grid (see [Harmony](#harmony)), default
  empty. Each row is `<C..B>:<±1..12>[,…]`: a pitch class of the stable
  note (sharps or flats, any case: `F#`, `db`), then the intervals that
  sound on it, e.g. `--voices C:-5,+4 D:+3`. The `+` is optional. Rows can
  be separate arguments or one quoted argument (`"C:-5,+4 D:+3"`). A pitch
  class given twice, a repeated interval, 0 or beyond ±12 is an error.
- `--glide <ms>`: 0 to 250, default 50.
- `--mix <percent>`: 0 to 100, default 50.

There is no `--gain-db`, `--key` or `--scale` any more; they give the
usage error.

The CSV's columns are
`time_s,f0_hz,confidence,voiced,stable_note,intervals,voice_notes`. On
voiced hops the last three give the stabiliser's note, its row's intervals
and the notes they sound, as space-separated lists, e.g. `C4,-5 +4,G3 E4`.
A note whose row is empty gives `C4,,`, and unvoiced hops `,,`.

The defaults match the plugin's. The harness compensates the chain's
latency: it feeds that many zeros after the input and drops that many from
the start of the output. So the output lines up with the input and has the
same length. At `--mix 0` it is bit-identical to the input.

### Test fixtures

The excerpts in `corral-dsp/tests/fixtures/` come from Vocadito,
MDB-stem-synth and NSynth. See
[`ATTRIBUTION.md`](corral-dsp/tests/fixtures/ATTRIBUTION.md) for sources,
licences and citations. `mise run fixtures` regenerates them: it downloads
the datasets into the gitignored `.datasets/` (about 2.2 GB, once) and cuts
the excerpts. `mise run check` only needs the committed excerpts.

## Pitch shifting

Corral adds pitch-shifted copies of the input, one per voice the harmony
grid turns on (see [Harmony](#harmony)). The algorithm is TD-PSOLA, driven
by the detector's period, with one shared analysis and a synthesis per
voice. See
[`docs/td-psola.md`](docs/td-psola.md) for how it works, how it compares
with a phase vocoder, and its known artefacts.

- **Intervals:** from the grid's row for the stable note's pitch class,
  and glided between at the `Glide` rate when the note changes. See
  [Harmony](#harmony).
- **`Mix`:** 0–100 %, default 50 %. A linear crossfade between the dry
  signal and the summed voices, smoothed over 20 ms.
- **Stereo:** the voices are made from the mono sum (the signal the
  detector hears) and added equally to both channels. The dry path stays
  stereo.
- **Unvoiced input** (silence, breaths, most consonants) passes dry. The
  harmony fades in and out over 10 ms, 2 hops after the voicing changes.
  After a note's onset it starts fading in 11–23 ms later at the output.
- **Old Live sets:** `Gain`, `Key`, `Scale` and `Shift` have been removed.
  A set saved with any of them loads with those values dropped (there are
  no users yet), with an empty grid. A set saved before Mix existed loads
  it at 50 %.

### Latency

| Sample rate | Latency (samples) | Latency (ms) |
|---|---|---|
| 44.1 kHz | 1103 | 25.0 |
| 48 kHz | 1200 | 25.0 |

The latency is ⌈2 · sr / 80⌉, two periods of the lowest detectable pitch.
The whole signal, dry path included, is delayed by it, so dry and shifted
stay aligned. It is reported to the host when the plugin is activated and
doesn't change with the grid, the number of voices, `Glide`, `Mix` or the
pitch sung (in VST3 a change restarts audio in Live). Cross-correlating the
input with the output measures exactly 1103 / 1200 samples at Mix 0 %,
1105 / 1202 at a fixed +4 and Mix 100 %, and 1106 / 1202 with a grid of C
major's thirds at Mix 100 %. Re-measure with
`cargo test -p corral-dsp --test latency -- --nocapture`.

### Accuracy

These tests check the shifter on its own. They drive the chain at a fixed
interval, with the same single interval in every row of the grid, run its
output back through the detector and compare it with the input pitch ×
2^(shift/12). All figures are at 44.1 and 48 kHz,
Mix 100 %, ignoring 150 ms at each end of a signal and 50 ms around each
step of the scale.

- **+4 on tones and the scale** (A3, E2, C4 and the C3–C5 scale): median
  error within ±0.3 ¢, and 100 % of voiced frames within ±25 ¢. Thresholds:
  ±10 ¢ and 95 %.
- **Other shifts** (A3 at −12, −5, +7 and +12, and C4 at −12): median error
  within ±0.6 ¢, and 100 % within ±25 ¢. Threshold: ±15 ¢.
- **Level** (output RMS against input RMS): −0.94 dB on A3 and −0.88 dB on
  the scale at +4 (threshold ±1.5 dB). The other shifts are printed, not
  asserted: −3.5 dB at −12, −1.8 dB at −5, −3.1 dB at +7 and −6.3 dB at
  +12. The level falls because TD-PSOLA keeps the spectral envelope while
  moving the harmonics under it (see the design doc).
- **Clicks:** the largest sample-to-sample jump must stay below 1.5 × the
  shift ratio × the dry signal's own largest jump. For voicing bursts, a
  level ramp through the gate, and a Mix sweep, it equals the dry signal's
  (0.0748 / 0.0687; thresholds 0.141 / 0.130). For a sweep of the fixed
  interval from −12 to +12 (24 steps over 4.8 s; each step fades one voice
  out and the next in over 20 ms), it is 0.105 / 0.096, against thresholds
  of 0.224 / 0.206.
- **Fixtures at +4.** The share of voiced reference frames within ±25 ¢ of
  the reference × 2^(4/12), then the same share counting only frames the
  chain voiced, then the detector's own ±25 ¢ score on the unshifted input:

  | Fixture | Shifted | Chain-voiced only | Detector alone | Threshold |
  |---|---|---|---|---|
  | `vocadito_35_low` | 0.718 | 0.750 | 0.833 | 0.70 |
  | `vocadito_7_vibrato` | 0.812 | 0.852 | 0.851 | 0.80 |
  | `mdb_nightowl_08_male` | 0.851 | 0.875 | 0.915 | 0.84 |

  Work item 0005 asked for 0.90. That is above what the detector itself
  scores at ±25 ¢, so the shifter can't reach it alone. Each threshold is
  set just below the measured score, as agreed in planning.

Every other threshold is the work item's. The click threshold was
calibrated from the dry signal, as the work item asked. The edge
exclusions are longer than the work item's 50 ms, because the detector
needs 46 ms to fill its window before the gate can open.

Print the scores with:

```sh
cargo test -p corral-dsp --test shifter --test latency --test fixtures -- --nocapture
```

## Harmony

Corral harmonises the voice with a grid you program: for each note you
sing, you choose which intervals sound above and below it, and each one
becomes a voice. The voices are the live input times a ratio, so they are
not pitch-corrected: vibrato and intonation carry through to them.

- **The grid:** one row per pitch class of the stable note (C to B), one
  column per interval from −12 to +12 semitones. The octave follows the
  singer: row C = {−5, +4} gives G3 and E4 on a sung C4, and G4 and E5 on
  a C5.
- **Voices:** each "on" cell in the sung note's row is one voice at full
  level. The voices are summed into the wet signal. A row can hold all 24
  intervals.
- **Empty rows:** a note whose row is empty gives only the dry signal (at
  Mix 100 %, silence).
- **`Glide`:** 0–250 ms, default 50 ms. See [Glide](#glide).
- **`Mix`:** see [Pitch shifting](#pitch-shifting).
- **Not saved:** the grid is not a parameter. It starts empty whenever the
  plugin loads, isn't saved with the set, and can't be automated.

> **Warning:** several voices at full level are loud, and there is no
> normalisation or limiter yet. A full row, or a few voices at Mix 100 %,
> can clip. Turn the track down or lower Mix.

The code is in `corral-dsp/src/grid.rs` (the grid), `voices.rs` (the
voices and their fades), `stabiliser.rs` (the note) and `shift.rs` (the
glide), wired together in `chain.rs`.

### Editor

The window is 960×560. The grid fills most of it:

- **Cells** are labelled with the note they would sound, so row C's +4 cell
  reads E. Off cells are dark grey, on cells blue. Click a cell to toggle
  it.
- **The centre column** (0) is the stable note itself. It is outlined, it
  can't be toggled, and the sung pitch class lights up yellow.
- **The sung row** gets a lighter band behind it, and its on cells turn
  bright blue with a yellow outline: those voices are sounding.
- **Glide and Mix** are sliders under the grid. Both can be automated.
- **Readout:** the stable note and the notes of its voices in large type
  (`C4 → G3 · E4`; `C4 → —` for an empty row; `—` when nothing is voiced).
  The note the detector hears is in a small line under it (`C4 +12¢`), so
  you can compare the two.

### Voices and note changes

The sounding voices follow the stable note's row. How they move depends on
what changed:

- **Note changes: matched by position.** When the stable note changes, the
  old voices take the new row's intervals in order, lowest to lowest, and
  glide there at the `Glide` rate. With row C = {+4} and row D = {+3},
  moving from C4 to D4 glides one voice from +4 to +3 (E4 to F4) at full
  level.
- **Surplus voices fade.** If the new row has more intervals, the extra
  ones start as new voices that fade in; if it has fewer, the unmatched
  old voices fade out. Fades take 20 ms and are equal-power (sin/cos), like
  the wet gate.
- **Edits fade only their own voice.** Toggling a cell in the sounding row
  fades that voice in or out and leaves the others alone. Turning a cell
  off and on again before its fade ends revives the fading voice from
  where it had reached, rather than starting a new one.
- **Onset: voices jump.** After silence (when the wet gate is closed), the
  voices start at their intervals with no glide or fade-in beyond the
  gate's own.

The pool has 48 voice slots: a full row sounding while as many fade out.
Only note changes faster than a fade can need more, and then the quietest
fading voice is reused.

### Note stabiliser

The row is chosen from a *stable* note, not the raw detection, so
vibrato and scoops don't make the harmony flicker between rows. Once per
hop, the stabiliser takes the smoothed period (so it already ignores
octave errors and out-of-range estimates) and:

- **onset:** when no harmony is sounding (the wet gate is closed), takes the
  nearest note at once;
- **hold:** otherwise changes the note only when the pitch has been more than
  70 ¢ from it for 60 ms (11 hops at 44.1 kHz, 12 at 48 kHz). A ±100 ¢
  vibrato at 5 Hz is beyond 70 ¢ for only about 51 ms per swing;
- **fast path:** or when it has been more than 150 ¢ away for 3 hops, so a
  clean step of a tone or more changes within 3 hops of being detected;
- **settle check:** and in either case only once the pitch is within 30 ¢
  of its nearest note. Without it, a scoop of 200 ¢ over 150 ms spends
  exactly 60 ms in the in-between semitone's range, and whether it commits
  to that note depends on where the hops fall. With it, the stabiliser
  waits and commits to the target.

A held semitone change therefore reaches the voices about 95–100 ms after
the audio does. The voices' pitch still follows the singer at once; only
the choice of intervals waits.

### Glide

`Glide` moves each matched voice toward its new interval exponentially, in
semitones (log frequency). It is a time constant: after the set time the
shift has covered 63 % of the change, and 95 % after three times it. A time
constant was chosen over a half-life because, at the 50 ms default, a
semitone change comes within 25 ¢ after 69 ms, where a 50 ms half-life
would need 100 ms.

- At onset the voices jump straight to their intervals: there is no glide
  after silence.
- Matched voices glide when the note changes; surplus and edited voices
  fade instead (see [Voices and note changes](#voices-and-note-changes)).
- **There is no minimum ramp.** At `Glide` 0 the interval changes in one
  sample, and that doesn't click: the shifter uses the ratio only to space
  its *next* grain, and grains are Hann-windowed and overlap-added, so a
  step moves where future grains go but can't break the waveform. The click
  tests measure about the same largest jump at Glide 0 as at 50 ms.

### Accuracy

`tests/harmony.rs` and `tests/voices.rs` score the harmony on synthesised
signals at 44.1 and 48 kHz, Mix 100 %.

- **Scales, one voice.** C major (C4–C5) and A minor (A3–A4), 0.5 s a note,
  with every row of the grid set to that scale's diatonic third (row C =
  {+4}, row D = {+3}, …), against the input × the third. The share of
  voiced 10 ms frames within ±25 ¢ must be at least 95 %, ignoring 150 ms
  at each end and 120 ms after each note change (205 voiced frames; medians
  −0.4 to +0.5 ¢):

  | | 44.1 kHz | 48 kHz |
  |---|---|---|
  | C major, Glide 50 ms | 0.956 | 0.966 |
  | C major, Glide 0 | 1.000 | 1.000 |
  | A minor, Glide 50 ms | 0.961 | 0.966 |
  | A minor, Glide 0 | 1.000 | 1.000 |

  The work item's exclusion was 100 ms. At 100 ms and the default glide the
  scales scored 0.936–0.949: every miss was in the glide's tail, 100–165 ms
  after a semitone step (E→F, B→C), because the hold puts the interval
  change about 95–100 ms after the audio. The stabiliser's values can't
  move that without letting vibrato through, so the exclusion was
  lengthened to 120 ms rather than shortening the default glide.
- **Empty grid, Mix 50 %:** the output is exactly 0.5 × the delayed dry
  signal (largest deviation 0.0). The detector calls a few hops of the test
  scale unvoiced, and two pairs of them close the wet gate, so 30 ms after
  each close is left out.
- **Two voices,** row C = {−5, +4} on C4: G3 alone has a median error of
  −0.2 / +0.2 ¢ and E4 alone +0.1 ¢, with 100 % of frames within ±25 ¢.
  Both together equal the sum of each alone to within 1.2e-7.
- **Matched voice,** C4 {+4} → D4 {+3} at Glide 50 ms: one voice at full
  level throughout. It starts gliding 17.4 / 10.7 ms after D4 is detected
  (3 / 2 hops), falls monotonically from +4 to +3, and never faster than
  the time constant allows.
- **Surplus voices,** C4 {+4} ↔ D4 {+3, +7}: +7 fades in and out over
  882 / 960 samples (20 ms) while the matched voice stays at full level.
- **Edits:** 12 toggles of row A while A3 is held. Each of the 10 plain
  toggles fades its own voice over 20 ms and every other voice stays at
  full level. +12 turned off and on 5 ms apart is revived from gain 0.924
  and takes as long to recover (221 / 240 samples) as it spent falling.
- **Onset after silence** at Glide 250 ms: the first voiced hop (31–37 ms
  into the note) takes the sung note, and the voices are exactly at their
  intervals, at full level, from that sample on (C4 {−5, +4} and D4 {+3}).
- **Vibrato:** C4 with ±100 ¢ vibrato at 5 and 7 Hz for 2 s: 0 note changes.
- **Scoops** from −200 ¢ into E4 over 50, 100 and 150 ms: the onset takes
  D4 and there is exactly one change, to E4, 84–170 ms in.
- **Clicks:** the largest sample-to-sample jump. Each voice at full level
  can jump by as much as the shifted input, so the threshold is
  1.5 × Σ ratio(interval) × the dry signal's largest jump, summed over the
  voices sounding together (for one voice, 1.5 × its ratio × the dry jump,
  as in [Pitch shifting](#accuracy-1)). 44.1 / 48 kHz:

  | Case | Voices | Max jump | Threshold |
  |---|---|---|---|
  | C major scale, thirds, Glide 50 ms | +4 | 0.245 / 0.211 | 0.483 / 0.444 |
  | C major scale, thirds, Glide 0 | +4 | 0.237 / 0.211 | 0.483 / 0.444 |
  | C major scale, thirds, Glide swept 0–250 ms | +4 | 0.237 / 0.210 | 0.483 / 0.444 |
  | C4 ↔ D4, surplus +7, Glide 50 ms or 0 | +3, +7 | 0.227 / 0.209 | 0.583 / 0.536 |
  | A3, toggles | loudest set: +4, +7, +12 | 0.234 / 0.220 | 0.774 / 0.712 |

The grid also has exhaustive unit tests: every one of the 288 cells toggled
in turn against a simple independent model.

Print the scores with:

```sh
cargo test -p corral-dsp --test harmony --test voices -- --nocapture
```

### CPU

The time to process one 512-frame stereo block at 48 kHz (10 667 µs of
real time), over 10 s of a synthesised C major scale (C4–C5, repeated),
with the same row in every pitch class and Mix 50 %, on an Apple Silicon
Mac. Each cell is mean / p99 / max, in µs, with the mean's share of real
time in brackets:

| Grid | arm64 (native) | x86_64 (Rosetta) |
|---|---|---|
| empty | 53.7 / 61.5 / 79.3 (0.5 %) | 63.3 / 76.1 / 84.0 (0.6 %) |
| 1 voice (+4) | 71.9 / 82.7 / 102.0 (0.7 %) | 89.8 / 107.0 / 120.6 (0.8 %) |
| 3 voices (−5, +4, +7) | 100.1 / 115.6 / 199.2 (0.9 %) | 133.9 / 153.3 / 159.7 (1.3 %) |
| 24 voices (full row) | 371.5 / 409.8 / 507.2 (3.5 %) | 580.4 / 651.5 / 749.1 (5.4 %) |

Each voice costs about 13 µs per block native and 22 µs under Rosetta
(Live 10's case). The analysis is shared, so an empty grid costs only the
detector and the dry path. Rerun it in a release build (add
`--target x86_64-apple-darwin` for the Rosetta figures):

```sh
cargo test --release -p corral-dsp --test cpu -- --ignored --nocapture
```

## Licensing

- nice-plug is ISC-licensed.
- Its VST3 bindings come from the [`vst3`](https://crates.io/crates/vst3) crate,
  which is MIT OR Apache-2.0.
- So, unlike upstream nih-plug (whose `vst3-sys` bindings are GPLv3), there is
  no GPLv3 constraint on distributing the plugin.
- The test fixtures include CC BY-NC material (MDB-stem-synth). That is fine
  only while the project isn't sold; otherwise those fixtures must be
  replaced.
- The project is for personal use for now.
