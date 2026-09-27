---
type: work-item
id: "0004"
title: "Realtime Pitch Detection with Offline Test Harness"
date: "2026-09-27T11:14:38+00:00"
author: John Cowie Del Corral
producer: create-work-item
status: draft
kind: story
priority: high
blocked_by: ["work-item:0003"]
tags: [phase-3, dsp, pitch-detection, testing, gui]
last_updated: "2026-09-27T11:14:38+00:00"
last_updated_by: John Cowie Del Corral
schema_version: 1
---

# 0004: Realtime Pitch Detection with Offline Test Harness

**Kind**: Story
**Status**: Draft
**Priority**: High
**Author**: John Cowie Del Corral

## Summary

As the plugin developer, I want Corral to detect the note I'm singing or
playing, show it in the GUI, and prove its accuracy offline against
annotated audio, so that the harmoniser's later phases (pitch shifting,
key-aware intervals) rest on a detector I can measure and trust.

This is Phase 3 of `ROADMAP.md`, including the offline `.wav` harness and
test audio that 0003 moved here.

## Context

- 0003 set up the pattern of plain, unit-tested DSP in `corral/src/dsp.rs`
  with `process()` as thin glue. It also added an egui editor with a readout
  fed from the audio thread by atomics, repainting only on change.
- `corral` is `crate-type = ["cdylib"]` only, so a separate harness binary
  can't link against it as it stands.
- **Audio:** the plugin is stereo in and out. Live 10 runs it under Rosetta
  (x86_64 slice) at the set's sample rate, usually 44.1 or 48 kHz.
- **Window size:** the lowest target note is E2 (82.41 Hz, period 12.14 ms).
  - YIN-family detectors need about two periods: ~1071 samples at 44.1 kHz,
    ~1165 at 48 kHz.
  - A 1024 window is marginal at E2. 2048 is safe: 46.4 ms at 44.1 kHz,
    42.7 ms at 48 kHz.
  - The algorithmic delay is about half the window plus a hop, plus the host
    buffer.
- **Algorithms:**
  - **YIN** is simple and gives an aperiodicity value that can act as
    confidence. Its main weakness is octave errors.
  - **pYIN** adds probabilistic candidates and HMM smoothing. The smoothing
    runs over the whole sequence, so realtime use needs a causal or
    fixed-lag version.
  - **McLeod/MPM** has a natural 0–1 "clarity" value and is said to handle
    vibrato well.
- **Rust crates:** none is realtime-safe as published.
  - `pitch-detection` 0.3.0 (MIT/Apache; MPM, YIN, autocorrelation; returns
    `clarity`) creates FFT plans on every call, which allocates.
  - `pyin` (MIT) works offline on whole signals only. It's usable as a
    reference implementation in tests.
  - `aubio` bindings are GPL-3.0, so they're excluded.
- **Datasets:**
  - **Vocadito** (CC BY 4.0): solo singing with human-corrected pitch
    annotations. 44.1 kHz; a `time,Hz` CSV where 0 means unvoiced.
  - **NSynth** (CC BY 4.0): isolated held notes with a known MIDI pitch. It
    has no frame-level pitch track, and the audio is 16 kHz.
  - **MDB-stem-synth** (CC BY-NC 4.0): resynthesised stems with exact pitch
    labels (~2.9 ms hop), including male singer, bass, cello and bassoon. It
    is non-commercial only, which is acceptable because this repo is private
    and not for sale.
  - No dataset has a clean sung scale.
- **Metrics:** the standard ones (`mir_eval.melody`) are:
  - **raw pitch accuracy:** the share of voiced frames within 50 cents;
  - **raw chroma accuracy:** the same, ignoring octave;
  - **voicing recall** and **voicing false alarm**.

  The gap between the two accuracy numbers stands in for the octave-error
  rate. For reference, the published figures are:
  - pYIN: raw pitch accuracy ~0.92 on MDB-stem-synth and ~0.99 on RWC-synth;
  - CREPE: raw pitch accuracy 0.98 on Vocadito, with voicing false
    alarm 0.31.

## Requirements

1. **Crate structure**: the DSP must be usable from a non-plugin binary and
   from integration tests. Either add `rlib` next to `cdylib`, or split out a
   plain `corral-dsp` crate. The plan decides. The DSP code has no
   `nice_plug` imports.
2. **Offline harness**:
   - It is a command (e.g. `mise run analyse <in.wav> [out.wav]`) that runs
     the same DSP chain the plugin runs, reading and writing WAV with
     `hound`.
   - It accepts 16/24-bit integer and 32-bit float mono or stereo WAV input.
   - It writes the processed audio and a per-hop CSV:
     `time_s,f0_hz,confidence,voiced`.
3. **Test fixtures**:
   - **Committed excerpts** go in a fixtures directory (under ~5 MB in
     total), each with its ground-truth pitch track and an `ATTRIBUTION.md`
     giving source, licence and citation:
     - Vocadito: one phrase with clear vibrato, and one phrase from a
       lower-voiced singer;
     - MDB-stem-synth: one male-singer excerpt, and one low monophonic
       instrument line (bass, cello or bassoon) reaching E2;
     - NSynth: two or three held notes, one near E2 and one in the soprano
       range, resampled to 44.1 kHz.
   - **Synthesised signals** are generated in the tests, not committed:
     - a harmonic-rich stepwise C major scale;
     - a held note with 5.5 Hz ±50-cent vibrato;
     - silence;
     - white noise.
4. **Pitch detector** in the plain DSP code:
   - The algorithm (YIN, MPM or a causal pYIN variant) and whether to write
     it ourselves or fork a crate are decided in the plan.
   - It detects 80 Hz–1 kHz (E2 to about B5) at 44.1 kHz and 48 kHz.
   - It outputs a frequency and a confidence value for each hop.
   - All buffers and FFT plans are allocated at setup (`initialize()`). The
     per-hop path doesn't allocate, lock or block.
5. **Voicing gate**:
   - Silence, breaths and noise report "no pitch".
   - It combines a level threshold with a confidence threshold. Both are
     internal constants for now.
6. **Frequency → note**:
   - A pure function returns the nearest MIDI note and the cents offset
     (A4 = 440 Hz).
   - A formatter produces text such as `"D4 +12¢"`.
7. **Plugin integration**:
   - The plugin analyses a mono sum of the stereo input.
   - Audio output is unchanged from 0003 (gain only).
   - The detected note (or unvoiced) reaches the editor through atomics,
     and the editor repaints only when the value changes, following 0003's
     readout pattern.
   - The editor shows the note, e.g. `"D4 +12¢"`, or `"—"` when unvoiced,
     alongside the existing Gain slider and readout.
8. **Accuracy tests**:
   - Rust implementations of raw pitch accuracy, raw chroma accuracy,
     voicing recall and voicing false alarm (50-cent tolerance, evaluated
     on the reference's frame times).
   - `cargo test` runs them against every fixture and fails below the
     acceptance thresholds.
   - The tests print each fixture's scores, so tuning changes can be
     compared.
9. **Latency measurement**:
   - Measure the delay from a synthesised note onset to the first stable,
     correct detection, at 44.1 and 48 kHz.
   - Record it in the README, together with the chosen window and hop.
   - There is no pass/fail limit in this phase.
10. **ROADMAP**: tick the Phase 3 items once they're verified.

## Acceptance Criteria

- [ ] Given the synthesised scale and vibrato signals at 44.1 kHz and
      48 kHz, when the accuracy tests run, then raw pitch accuracy ≥ 0.98,
      and raw chroma accuracy minus raw pitch accuracy ≤ 0.01.
- [ ] Given the NSynth held notes, when the accuracy tests run, then raw
      pitch accuracy ≥ 0.95 over the sustained part of each note.
- [ ] Given the MDB-stem-synth excerpts, when the accuracy tests run, then
      raw pitch accuracy ≥ 0.90 and raw chroma accuracy minus raw pitch
      accuracy ≤ 0.02.
- [ ] Given the Vocadito excerpts, when the accuracy tests run, then:
      - raw pitch accuracy ≥ 0.90;
      - raw chroma accuracy minus raw pitch accuracy ≤ 0.02;
      - voicing false alarm ≤ 0.20.
- [ ] Given synthesised silence or white noise, when the detector runs,
      then ≥ 99% of hops are reported unvoiced.
- [ ] Given frequencies across octaves 1–7, when converted to notes, then
      the unit tests confirm:
      - the nearest note and the cents offset are correct;
      - frequencies at exactly ±50 cents round consistently;
      - 440 Hz formats as `"A4 +0¢"`.
- [ ] Given a debug build with nice-plug's allocation assertions enabled
      (e.g. the `assert_process_allocs` feature), when the plugin processes
      audio with the detector running, then nothing panics.
- [ ] Given `mise run analyse <vocadito excerpt>.wav out.wav`, then a CSV
      with one row per hop and an output WAV are written. At 0 dB gain, the
      output WAV is bit-identical to the input.
- [ ] Given Corral on an audio track in Live 10, when I sing a held A3,
      then the editor shows `A3`, with a cents value within ±20 of a
      reference tuner.
- [ ] Given Corral on an audio track in Live 10, when I stop singing, then
      the editor shows `"—"` within about 100 ms.
- [ ] Given Corral on an audio track in Live 10, when I sing a scale, then
      the displayed note follows each step without octave jumps on
      sustained notes.
- [ ] The measured onset-to-detection latency, window size and hop size are
      recorded in the README for 44.1 kHz and 48 kHz.
- [ ] `mise run check` passes, and `mise tasks` lists `analyse` with a
      description.

## Open Questions

## Dependencies

- Blocked by: 0003 (tested gain DSP and basic plugin GUI), which provides
  the DSP/test pattern and the GUI readout mechanism.
- Blocks: Phase 4 (pitch shifting), which uses the detected pitch period
  and the offline harness. It also blocks Phase 5 (key-aware harmony), which
  maps the detected note to a scale degree.

## Assumptions

- **Synthesised signals:** these are used alongside the dataset clips,
  because no public dataset has a clean sung scale and synthesised signals
  give exact ground truth.
- **Non-commercial data:** CC BY-NC data (MDB-stem-synth) is acceptable,
  because the repo is private and the plugin won't be sold. If that
  changes, the MDB fixtures must be replaced.
- **Mono analysis:** analysing a mono sum of the stereo input is enough,
  since the input is a single voice or instrument.

## Technical Notes

- **Realtime-safe FFT:** if we use `rustfft`, create the `Arc<dyn Fft>`
  plans and scratch buffers at setup, and call only `process_with_scratch`
  per hop. `pitch-detection`'s `internals.rs` shows the algorithms, but
  calls `FftPlanner::new()` on every call.
- **Analysis buffering:**
  - A ring buffer collects the mono input.
  - Analysis runs once per hop (e.g. 256 samples), independent of the host
    buffer size.
  - Analysis cost is spread over buffers, not tied to buffer boundaries.
- **Reference implementation:** the `pyin` crate could be a dev-dependency
  that serves as an offline reference, to sanity-check fixtures or compare
  scores. It is not required.
- **Fixture resampling:** do NSynth's 16 kHz → 44.1 kHz resampling once,
  when preparing fixtures, and commit the result. Don't add a runtime
  resampler.
- **Vocadito hop size:** check it from the CSV. It isn't documented; it is
  probably Tony's default of 256 samples.
- **Rosetta:** test the x86_64 slice in Live 10, as in 0002 and 0003.

## Drafting Notes

- **Scope:** the whole of Phase 3 is one story, matching how 0002 and 0003
  each took one phase. The developer chose this over an epic.
- **Thresholds:** the accuracy thresholds are judgement calls, based on
  published pYIN and CREPE figures. They aren't a standard. The plan may
  adjust them after the first measurements, but it should record why.
- **Latency:** it is measure-and-record only in this phase, at the
  developer's request. Reporting latency to the host waits until Phase 4,
  when the audio is actually delayed.
- **Algorithm choice:** it is deliberately left to planning. The
  requirements are algorithm-agnostic: range, confidence output and
  realtime safety.
- **"Monophonic instrument line":** the roadmap item is covered by an
  MDB-stem-synth low instrument excerpt plus NSynth held notes.
- **Metrics:** they are written in Rust, so the toolchain has no Python or
  `mir_eval` dependency. They are simplified: they compare on the
  reference's frame times rather than `mir_eval`'s resampling.
- **Out of scope:**
  - pitch shifting and host latency reporting (Phase 4);
  - note stabilisation / hysteresis (Phase 5);
  - user-facing detector parameters;
  - non-causal full-sequence pYIN smoothing.
- **Priority:** set to high because pitch detection is the core of the
  harmoniser. 0003 was medium because it was foundations.

## References

- Source: `ROADMAP.md` (Phase 3)
- Related: 0003 (tested gain DSP and basic plugin GUI), 0002 (gain plugin
  loadable in Ableton Live 10)
- Vocadito: https://zenodo.org/records/5578807 (paper: https://arxiv.org/abs/2110.05580)
- MDB-stem-synth: https://zenodo.org/records/1481172
- NSynth: https://magenta.tensorflow.org/datasets/nsynth
- pYIN (Mauch & Dixon 2014): https://webspace.eecs.qmul.ac.uk/s.e.dixon/pub/2014/MauchDixon-PYIN-ICASSP2014.pdf
- McLeod & Wyvill, "A smarter way to find pitch" (ICMC 2005): https://quod.lib.umich.edu/i/icmc/bbp2372.2005.107?rgn=main&view=fulltext
- CREPE paper (baseline figures): https://arxiv.org/abs/1802.06182
- mir_eval melody metrics: https://mir-eval.readthedocs.io/latest/api/melody.html
- pitch-detection crate: https://github.com/alesgenova/pitch-detection
- pyin crate: https://github.com/Sytronik/pyin-rs
- hound: https://github.com/ruuda/hound
