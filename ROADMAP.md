# Roadmap

The end goal is an **auto-harmoniser** plugin. You pick a key, then sing or play a
monophonic instrument into it. The plugin detects the pitch you're playing and adds
a pitch-shifted copy at an interval that fits the key. For example, in C major a
sung C gets a major third (E) and a sung D gets a minor third (F).

We get there in small steps. Each phase should end with something that loads and
works in Ableton.

## Phase 0: Project setup

- [x] `git init`
- [x] Install the Rust toolchain via mise (Rust 1.98.1 through rustup), plus both macOS targets (`aarch64-apple-darwin`, `x86_64-apple-darwin`)
- [x] Pick the plugin framework: [nice-plug](https://codeberg.org/RustAudio/nice-plug), the maintained community fork of nih-plug (which is in maintenance mode). It exports VST3 and CLAP from one codebase
- [x] Scaffold the crate (`cdylib`) and the `xtask` bundler (`cargo xtask bundle <name> --release`)
- [x] Add `.gitignore` (`/target`)
- [x] Note the licensing: nice-plug is ISC and its VST3 bindings are MIT/Apache, so there is no GPLv3 constraint (see README)
- [x] Add mise tasks (`build`, `build-universal`, `fmt`, `fmt-check`, `lint`, `check`) and the lint and format config

## Phase 1: Minimal pass-through plugin (volume control)

The goal is to prove that audio flows through our code and that we can change it.

- [ ] Plugin that copies input to output untouched
- [ ] Add a `Gain` parameter (dB, e.g. -60 to +12) and apply it per sample
- [ ] Smooth the parameter so it doesn't click (nice-plug `SmoothingStyle::Logarithmic`)
- [ ] Build the VST3 bundle and copy it to `~/Library/Audio/Plug-Ins/VST3/`
- [ ] Ad-hoc codesign the bundle so macOS / Ableton will load it (`codesign --force --deep -s - <bundle>`)
- [ ] Rescan plugins in Ableton, put the plugin on an audio track, and check that the gain audibly changes the signal
- [ ] Check that the parameter can be automated from Ableton
- [ ] Add a mise task that builds, signs, and installs in one step

## Phase 2: Development foundations

- [ ] Offline test harness: run the plugin's DSP on a `.wav` file and write the result to a `.wav` (using `hound`), so DSP can be tested without a DAW
- [ ] Unit tests for the DSP pieces, kept separate from the plugin glue code
- [ ] Basic GUI (e.g. nice-plug's egui editor integration) with a gain knob and a readout area for debug info
- [ ] Dry/wet mix parameter (the harmoniser will need it)
- [ ] Collect some test audio: a sung scale, a held note, vibrato, and a monophonic instrument line

## Phase 3: Pitch detection

- [ ] Implement a monophonic pitch detector. Start with YIN, and consider pYIN or McLeod (MPM) if YIN proves noisy
- [ ] Voicing / confidence gate so that silence, breaths, and noise don't produce a pitch
- [ ] Convert frequency to MIDI note number plus cents offset
- [ ] Show the detected note in the GUI (e.g. "D4 +12¢") and confirm it tracks your voice in Ableton
- [ ] Test the detector against the test audio from Phase 2 (accuracy, octave errors)
- [ ] Measure the detection latency and tune the window size (trade latency against low-note accuracy)

## Phase 4: Pitch shifting

- [ ] Implement a pitch shifter that shifts by a fixed amount (start with +4 semitones)
  - Option A: **TD-PSOLA**, which uses the detected pitch period, suits monophonic voice, and keeps formants
  - Option B: **phase vocoder**, which is more general but can sound "phasey" and moves formants
- [ ] Mix the shifted signal with the dry signal
- [ ] Report plugin latency to the host so Ableton can compensate for the delay
- [ ] Listening tests in Ableton: artefacts, transients, low notes
- [ ] Make the shift amount a parameter (semitones) as an interim "fixed harmoniser"

## Phase 5: Key-aware harmony (the auto-harmoniser)

- [ ] `Key` parameter (C to B) and `Scale` parameter (major, then natural minor, then others)
- [ ] Map the detected note to a scale degree
- [ ] Compute the **diatonic third above** within the scale (C→E major 3rd, D→F minor 3rd, E→G minor 3rd, etc.)
- [ ] Choose how to handle out-of-key input notes: snap to the nearest scale note, or use a fixed fallback interval
- [ ] Hysteresis / note stabilisation so the harmony doesn't flicker between intervals on vibrato or scoops
- [ ] Glide between harmony notes rather than jumping instantly
- [ ] Unit tests for the key/scale/interval logic (pure functions, easy to test exhaustively)
- [ ] End-to-end test in Ableton: sing a scale in C and hear correct diatonic thirds

## Phase 6: Refinement and extras

- [ ] Choice of harmony interval (3rd above, 3rd below, 5th, 6th, octave)
- [ ] Multiple harmony voices (e.g. 3rd + 5th for a triad) with per-voice level and pan
- [ ] Formant preservation / correction for more natural harmonies
- [ ] Humanisation: small detune, timing offsets, and pitch drift per voice
- [ ] Set the key from MIDI (sidechain MIDI notes or chords to pick the harmony)
- [ ] Presets
- [ ] Performance pass: profile CPU use and remove allocations from the audio thread
- [ ] Universal binary (arm64 + x86_64) and a CLAP build for other DAWs (delivered in Phase 0 via `mise run build-universal`)
