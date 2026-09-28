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
| `analyse` | Run the plugin's DSP chain on a WAV: analyse <in.wav> [out.wav] [--gain-db <dB>] [--key <C..B>] [--scale <major\|minor>] [--glide <ms>] [--mix <percent>] (writes out.wav and out.csv; paths are relative to the project root) |
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
[Harmony](#harmony)). Audio passes through with the gain applied and a
pitch-shifted harmony mixed in (see [Pitch shifting](#pitch-shifting)).

- **Algorithm:** YIN, with its difference function computed through FFT
  (`realfft`). The FFT plans and buffers are made when the host activates the
  plugin, so the audio thread never allocates.
- **Input:** a mono sum of the stereo input, taken *before* the gain, so the
  Gain slider doesn't affect detection.
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
mise run analyse corral-dsp/tests/fixtures/vocadito_7_vibrato.wav target/analyse/v.wav --key C --scale major --glide 50 --mix 100
```

This runs the same chain the plugin runs, harmony included. It writes the
processed audio (in the input's format) and a per-hop CSV next to it
(`target/analyse/v.csv`). The output path is optional; it defaults to
`target/analyse/<input name>.wav`. Input can be 16- or 24-bit integer or
32-bit float WAV, mono or stereo.

- `--gain-db <dB>`: gain, default 0.
- `--key <C..B>`: the key, default C. Sharps or flats, any case (`F#`,
  `bb`).
- `--scale <major|minor>`: default major. `natural-minor` also works.
- `--glide <ms>`: 0 to 250, default 50.
- `--mix <percent>`: 0 to 100, default 50.

The CSV's columns are
`time_s,f0_hz,confidence,voiced,stable_note,interval,harmony_note`. On
voiced hops the last three give the stabiliser's note, the interval chosen
for it and the harmony note, e.g. `D4,+3,F4`. On unvoiced hops they are
empty.

The defaults match the plugin's. The harness compensates the chain's
latency: it feeds that many zeros after the input and drops that many from
the start of the output. So the output lines up with the input and has the
same length. At `--mix 0` and 0 dB it is bit-identical to the input.

### Test fixtures

The excerpts in `corral-dsp/tests/fixtures/` come from Vocadito,
MDB-stem-synth and NSynth. See
[`ATTRIBUTION.md`](corral-dsp/tests/fixtures/ATTRIBUTION.md) for sources,
licences and citations. `mise run fixtures` regenerates them: it downloads
the datasets into the gitignored `.datasets/` (about 2.2 GB, once) and cuts
the excerpts. `mise run check` only needs the committed excerpts.

## Pitch shifting

Corral adds a pitch-shifted copy of the input, a diatonic third above it
(see [Harmony](#harmony)). The algorithm is TD-PSOLA, driven by the detector's period. See
[`docs/td-psola.md`](docs/td-psola.md) for how it works, how it compares
with a phase vocoder, and its known artefacts.

- **Interval:** chosen from `Key` and `Scale` for each note, and glided
  between at the `Glide` rate. See [Harmony](#harmony).
- **`Mix`:** 0–100 %, default 50 %. A linear crossfade between the dry
  signal and the harmony, smoothed over 20 ms.
- **Stereo:** the harmony is made from the mono sum (the signal the
  detector hears) and added equally to both channels. The dry path stays
  stereo.
- **Unvoiced input** (silence, breaths, most consonants) passes dry. The
  harmony fades in and out over 10 ms, 2 hops after the voicing changes.
  After a note's onset it starts fading in 11–23 ms later at the output.
- **Old Live sets:** `Shift` has been removed. A set saved with it loads
  with its value dropped, at Key C, Major, Glide 50 ms. A set saved before
  Mix existed loads it at 50 %.

### Latency

| Sample rate | Latency (samples) | Latency (ms) |
|---|---|---|
| 44.1 kHz | 1103 | 25.0 |
| 48 kHz | 1200 | 25.0 |

The latency is ⌈2 · sr / 80⌉, two periods of the lowest detectable pitch.
The whole signal, dry path included, is delayed by it, so dry and shifted
stay aligned. It is reported to the host when the plugin is activated and
doesn't change with `Key`, `Scale`, `Glide`, `Mix` or the pitch sung (in
VST3 a change restarts audio in Live). Cross-correlating the input with
the output measures exactly 1103 / 1200 samples at Mix 0 %, 1105 / 1202 at
a fixed +4 and Mix 100 %, and 1106 / 1202 in Key C Major at Mix 100 %. Re-measure with
`cargo test -p corral-dsp --test latency -- --nocapture`.

### Accuracy

These tests check the shifter on its own. They drive the chain at fixed
intervals internally (the plugin has no fixed-interval setting), run its
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
  interval from −12 to +12, glided at the default 50 ms, it is
  0.105 / 0.096, against thresholds of 0.224 / 0.206.
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

Corral harmonises a diatonic third above the voice, in the key and scale you
choose. Sung C in C major gets E (a major third) and sung D gets F (a minor
third). The harmony is the live input times a ratio, so it is not
pitch-corrected: vibrato and intonation carry through to it.

- **`Key`:** C to B, default C.
- **`Scale`:** Major or Natural minor, default Major.
- **`Glide`:** 0–250 ms, default 50 ms. See [Glide](#glide).
- **Readout:** the editor shows the stable note and the harmony note in
  large type (`D4 → F4`, or `—` when nothing is voiced). The note the
  detector hears is in a small line under it (`D4 +12¢`), so you can compare
  the two.

The code is in `corral-dsp/src/harmony.rs` (the intervals),
`stabiliser.rs` (the note) and `shift.rs` (the glide), wired together in
`chain.rs`.

### Intervals

The interval depends only on the sung note's pitch class relative to the
key, so each scale is a 12-entry table (semitones up to the harmony):

| Above the key | 0 | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9 | 10 | 11 |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| In C major | C | C# | D | D# | E | F | F# | G | G# | A | A# | B |
| Major | +4 | +4 | +3 | +3 | +3 | +4 | +4 | +4 | +4 | +3 | +3 | +3 |
| In A minor | A | A# | B | C | C# | D | D# | E | F | F# | G | G# |
| Natural minor | +3 | +3 | +3 | +4 | +4 | +3 | +3 | +3 | +4 | +4 | +4 | +4 |

**Out-of-key notes** use the interval of the scale note a semitone below.
In C major, C# → F (C's +4), Eb → F# (D's +3) and F# → A# (F's +4). In
major and natural minor every chromatic note sits exactly between two scale
notes, so "the nearest scale note" is always a tie; "the note below" breaks
it without any state. The harmony never drops out on an out-of-key note.
Some out-of-key notes give a chromatic harmony (F# → A#), whichever rule is
used, because the harmony moves in parallel with the voice. Other
harmonisers produce the same passing tone.

### Note stabiliser

The interval is chosen from a *stable* note, not the raw detection, so
vibrato and scoops don't make the harmony flicker between a major and a
minor third. Once per hop, the stabiliser takes the smoothed period (so it
already ignores octave errors and out-of-range estimates) and:

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

A held semitone change therefore reaches the interval about 95–100 ms after
the audio does. The harmony's pitch still follows the voice at once; only
the choice of interval waits.

### Glide

`Glide` moves the shift toward a new interval exponentially, in semitones
(log frequency). It is a time constant: after the set time the shift has
covered 63 % of the change, and 95 % after three times it. A time constant
was chosen over a half-life because, at the 50 ms default, a semitone
change comes within 25 ¢ after 69 ms, where a 50 ms half-life would need
100 ms.

- At onset the shift jumps straight to the note's interval: there is no
  glide after silence.
- Changing `Key` or `Scale` mid-note glides to the new interval at the same
  rate.
- **There is no minimum ramp.** At `Glide` 0 the interval changes in one
  sample, and that doesn't click: the shifter uses the ratio only to space
  its *next* grain, and grains are Hann-windowed and overlap-added, so a
  step moves where future grains go but can't break the waveform. The click
  tests measure the same largest jump at Glide 0 as at 50 ms.

### Accuracy

`tests/harmony.rs` scores the harmony on synthesised signals at 44.1 and
48 kHz, Mix 100 %.

- **Scales.** C major (C4–C5, Key C) and A minor (A3–A4, Key A, Natural
  minor), 0.5 s a note, against the input × the hand-written third. The
  share of voiced 10 ms frames within ±25 ¢ must be at least 95 %,
  ignoring 150 ms at each end and 120 ms after each note change (205 voiced
  frames; medians −0.4 to +0.5 ¢):

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
- **Vibrato:** C4 with ±100 ¢ vibrato at 5 and 7 Hz for 2 s: 0 note changes.
- **Scoops** from −200 ¢ into E4 over 50, 100 and 150 ms: the onset takes
  D4 and there is exactly one change, to E4, 84–170 ms in.
- **Onset after silence** at Glide 250 ms: the first voiced hop (31–37 ms
  into the note) takes the sung note, and the shift is exactly +4 (C4) or
  +3 (D4) from that sample on.
- **Legato C4 → D4:** the note changes 3 hops (17.4 ms) at 44.1 kHz and
  2 hops (10.7 ms) at 48 kHz after D4 is first detected. The glide then
  falls monotonically from +4 to +3, never faster than the time constant
  allows.
- **Out-of-key notes** in C major (C#, Eb, F#, G#, Bb): medians within
  ±3.4 ¢ of F, F#, A#, C and C#; 100 % of frames within ±25 ¢ and voiced,
  and no hop without a harmony.
- **Clicks** (the largest sample-to-sample jump, below 1.5 × ratio(+4) ×
  the dry signal's; 44.1 / 48 kHz):

  | Case | Max jump | Threshold |
  |---|---|---|
  | C major scale, Glide 50 ms | 0.245 / 0.211 | 0.483 / 0.444 |
  | C major scale, Glide 0 | 0.237 / 0.211 | 0.483 / 0.444 |
  | A3, Key and Scale changing every 250 / 500 ms, Glide 0 or 50 ms | 0.109 / 0.100 | 0.205 / 0.188 |
  | C major scale, Glide swept 0–250 ms, Key C ↔ G | 0.240 / 0.210 | 0.483 / 0.444 |

The interval logic also has exhaustive unit tests: every key, scale and
pitch class against a table typed by hand from music theory.

Print the scores with:

```sh
cargo test -p corral-dsp --test harmony -- --nocapture
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
