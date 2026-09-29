---
type: work-item
id: "0005"
title: "Realtime TD-PSOLA Pitch Shifter"
date: "2026-09-28T09:14:07+00:00"
author: John Cowie Del Corral
producer: create-work-item
status: done
kind: story
priority: high
blocked_by: ["work-item:0004"]
tags: [phase-4, dsp, pitch-shifting, td-psola, latency, testing, docs]
last_updated: "2026-09-29T08:03:49+00:00"
last_updated_by: John Cowie Del Corral
schema_version: 1
---

# 0005: Realtime TD-PSOLA Pitch Shifter

**Kind**: Story
**Status**: Draft
**Priority**: High
**Author**: John Cowie Del Corral

## Summary

As the plugin developer, I want Corral to add a pitch-shifted copy of my
voice (by a chosen number of semitones) mixed with the dry signal, with its
latency reported to the host, so that Phase 5 can pick the harmony interval
from the key and have a shifter it can drive.

This is Phase 4 of `ROADMAP.md`. It uses TD-PSOLA and includes a design
doc explaining TD-PSOLA and how it compares to a phase vocoder.

## Context

- **Pitch detection:** 0004 delivered a causal YIN tracker in `corral-dsp`:
  - 2048 window and 256 hop at 44.1/48 kHz;
  - range 80 Hz–1 kHz;
  - a voicing gate;
  - onset-to-detection latency of about 34–40 ms (see README).
- **Shared chain:** `Chain` in `corral-dsp/src/chain.rs` is shared by the
  plugin and the `analyse` harness. It currently applies gain only; audio
  isn't delayed.
- **Deferred work picked up here:**
  - 0003 moved the dry/wet mix to this phase.
  - 0004 moved host latency reporting to this phase.
- **Why TD-PSOLA:** it suits monophonic voice.
  - It reuses the detected period.
  - It keeps formants because grain length follows the original period.
  - It needs only about two periods of lookahead.
  - A phase vocoder handles polyphony but adds phasiness, smears
    transients, shifts formants and needs an FFT frame of latency.
- **Rust crates:** none is a maintained PSOLA crate.
  - `pitch_shift` (MIT) and `signalsmith-stretch` (MIT) are phase-vocoder
    style.
  - Rubber Band is GPL or commercial.
  - So the shifter is written by us.
- **Latency in nice-plug 0.4.2:**
  - Latency is set with `set_latency_samples` (on `ActivateContext`, and
    also on `ProcessContext`).
  - In VST3, a changed value triggers `restartComponent(kLatencyChanged)`,
    which restarts audio in Live. So latency should be fixed for each
    sample rate.
  - Live has known quirks around latency updates (e.g. after preset load).

## Requirements

1. **TD-PSOLA shifter** in `corral-dsp` (no `nice_plug` imports):
   - It is causal and streaming.
   - Grains are Hann-windowed, about two periods long, and centred on
     analysis pitch marks.
   - Analysis marks are tracked by searching near the previous mark plus
     one period.
   - Synthesis marks are spaced at the period divided by the shift ratio.
   - The period comes from the existing YIN tracker. It is smoothed, and
     octave jumps (about ×2 or ×0.5) are rejected unless they persist
     across several hops.
   - All buffers are allocated at setup. The per-sample and per-hop path
     doesn't allocate, lock or block.
2. **Unvoiced input goes dry**:
   - When the voicing gate reports unvoiced, the wet path uses the delayed
     dry signal.
   - Switching between shifted and dry uses an equal-power crossfade of
     about 5–20 ms. The plan picks the value.
   - The crossfade has hysteresis so it doesn't chatter on borderline
     frames.
3. **Dry/wet mix**:
   - A `Mix` parameter from 0 to 100%, defaulting to 50%, smoothed.
   - It is a linear crossfade.
   - The dry path is delayed by the same latency as the wet path so the
     two stay phase-aligned.
4. **Shift parameter**:
   - `Shift` is a whole number of semitones from −12 to +12, defaulting to
     +4.
   - It can change while audio is playing without clicks and without
     changing the reported latency.
   - This is the roadmap's interim "fixed harmoniser".
5. **Stereo handling**:
   - The wet signal is made from the mono sum, which is also what the
     detector analyses.
   - It is added equally to both channels.
   - The dry path stays stereo.
6. **Latency**:
   - The plugin reports one fixed latency for each sample rate at
     activation. It doesn't depend on pitch or `Shift`.
   - The aim is the lowest latency that keeps quality acceptable. Research
     suggests about 25 ms (about two periods at 80 Hz) is realistic.
   - The latency is measured and recorded, not pass/fail.
7. **Offline harness**:
   - `mise run analyse` runs the same chain, including the shifter.
   - It takes `--shift <semitones>` and `--mix <percent>` flags.
   - It writes the shifted audio, the same length as the input, alongside
     the existing CSV.
8. **Objective tests** in `cargo test`:
   - **Output pitch:** run the existing YIN on the output and compare it
     with the input pitch × 2^(n/12).
   - **Length:** the output is the same length as the input.
   - **Reported latency:** it agrees with the actual delay, measured by
     cross-correlation.
   - **Unvoiced and Mix = 0%:** the output equals the input delayed by the
     latency.
   - **Level:** RMS is preserved.
   - **Clicks:** none at voicing transitions or during a `Shift` sweep.
   - **Allocation:** `no_alloc.rs` also covers the shifter.
   - The tests print their scores, so tuning changes can be compared.
9. **Editor**:
   - `Shift` and `Mix` controls next to the existing Gain slider and note
     readout.
10. **Design doc** at `docs/td-psola.md`:
    - A high-level overview of TD-PSOLA: pitch marks, grain windowing,
      overlap-add, how shifting changes synthesis-mark spacing, and why
      formants are preserved.
    - A comparison with a phase vocoder covering latency, formant
      handling, polyphony, phasiness and transients, and dependence on a
      pitch detector.
    - Why Corral uses TD-PSOLA, and its known artefacts and mitigations.
    - Cited sources.
11. **README**: record the reported latency, in samples and ms, at 44.1 and
    48 kHz, and document the new parameters and harness flags.
12. **ROADMAP**: tick the Phase 4 items once they're verified.

## Acceptance Criteria

- [ ] Given synthesised harmonic tones and a synthesised scale at 44.1 kHz
      and 48 kHz, with Shift = +4 and Mix = 100%, when the output is
      analysed with YIN (ignoring 50 ms at each voicing edge), then:
      - the median error against the input pitch × 2^(4/12) is within
        ±10¢;
      - ≥ 95% of voiced frames are within ±25¢.
- [ ] Given the Vocadito and MDB-stem-synth male-singer fixtures, with
      Shift = +4 and Mix = 100%, when the output is analysed, then ≥ 90% of
      voiced reference frames are within ±25¢ of the reference pitch
      × 2^(4/12).
- [ ] Given synthesised harmonic tones with Shift = −12, −5, +7 and +12,
      when the output is analysed, then the median error is within ±15¢
      for each shift.
- [ ] Given synthesised silence, white noise, or any input with Mix = 0%,
      when processed, then the output is bit-identical to the input
      delayed by the reported latency.
- [ ] Given a synthesised note onset, when dry input and wet output are
      cross-correlated, then the measured delay agrees with the reported
      latency to within ±1 hop.
- [ ] Given voiced synthesised input with Mix = 100%, when processed, then
      output RMS is within ±1.5 dB of input RMS.
- [ ] Given voiced/unvoiced transitions, and an automated `Shift` sweep
      from −12 to +12, when processed, then no sample-to-sample jump in the
      output exceeds a click threshold. The plan calibrates the threshold
      from the dry signal and records it.
- [ ] Given `corral-dsp/tests/no_alloc.rs`, when it runs, then the
      shifter's per-sample and per-hop path allocates nothing.
- [ ] Given `mise run analyse in.wav out.wav --shift 4 --mix 100`, then a
      shifted output WAV the same length as the input and the per-hop CSV
      are written.
- [ ] Given Corral on an audio track in Live 10 with Shift = +4 and
      Mix = 50%, when I sing a held A3, then I hear a C#4 harmony alongside
      my voice.
- [ ] Given Corral on a track in Live 10 alongside other tracks, when the
      set plays, then Live's delay compensation keeps the Corral track
      aligned with the others: Live shows the reported latency, and there
      is no audible flam against a reference track.
- [ ] Given Corral in Live 10, when I move or automate `Shift` and `Mix`
      while singing, then there are no audible clicks, and the reported
      latency doesn't change.
- [ ] `docs/td-psola.md` exists and covers:
      - how TD-PSOLA works (pitch marks, grains, overlap-add, formant
        preservation);
      - a comparison table with a phase vocoder (latency, formants,
        polyphony, phasiness, detector dependence);
      - why Corral uses TD-PSOLA;
      - at least three cited sources.
- [ ] The README records the reported latency, in samples and ms, at
      44.1 and 48 kHz, and documents `Shift`, `Mix` and the new harness
      flags.
- [ ] `mise run check` passes.

## Open Questions

## Dependencies

- Blocked by: 0004 (realtime pitch detection), which provides the period
  estimate, the voicing gate, the shared `Chain`, the `analyse` harness and
  the fixtures.
- Blocks: Phase 5 (key-aware harmony), which will drive `Shift` from the
  detected note's scale degree.

## Assumptions

- **Stereo handling:** building the wet signal from the mono sum and adding
  it to both channels is enough, because the input is a single voice or
  instrument.
- **Harmony for now:** until Phase 5, "the harmony" is the dry voice plus
  one shifted voice, so a 50% linear mix is a sensible default.
- **Stale period:** we accept a slightly stale period estimate at note
  onsets. This keeps latency at about two periods rather than delaying the
  audio by the detector window as well, which would give about 50 ms in
  total.
- **Doc location:** the design doc lives in a new `docs/` directory at the
  repo root, not in `meta/`, because it explains the product rather than
  the process.

## Technical Notes

- **Mark placement:** after the first mark, search ±~P/4 around the previous
  mark + P for the largest |x|, or the peak of a low-passed signal. Placing
  marks on peaks reduces phasing.
- **Grain size:** keep grains at about 2 × the analysis period. Don't vary
  them with the shift ratio, or formants move.
- **Latency estimate:** a 2P grain centred on a mark needs P samples after
  the mark, plus up to P more to confirm it. With P_max ≈ 12.5 ms (80 Hz),
  that is about 1024 samples at 44.1 kHz and about 1200 at 48 kHz.
  - A candidate formula is `ceil(2 × sr / 80)`, set in `activate()`.
  - Keep it independent of pitch and `Shift`, since each change restarts
    audio in Live.
- **Large shifts:** big downward shifts leave gaps between grains ("a
  series of pulses"). ±12 is the stress case, and +4 (ratio 1.26) is mild.
- **Delay line:** the dry delay line and the mark and grain buffers belong
  in `Chain`, so that the plugin and the harness share them.
- **Test reference:** the GPL-3.0 `maxrmorrison/psola` (a Praat wrapper) can
  be used offline to compare against by ear. It must not be linked or
  vendored.
- **Rosetta:** test the x86_64 slice in Live 10, as in 0002 to 0004.
- **Debug builds:** debug builds still abort in Live 10 (see the ROADMAP
  icebox), so realtime safety is checked by `no_alloc.rs` rather than
  `assert_process_allocs` in the DAW.

## Drafting Notes

- **Scope:** all of Phase 4 is one story, which the plan splits into
  implementation phases, as with 0004. The developer chose this.
- **Algorithm:** TD-PSOLA was chosen by the developer. A phase vocoder is
  covered only in the design doc's comparison.
- **Latency:** it is measure-and-report with a lowest-possible aim, at the
  developer's request. There is no pass/fail ceiling.
- **Unvoiced input:** passing the dry signal through was the developer's
  call. The crossfade and hysteresis are my additions, to avoid clicks.
- **Tolerances:** ±10¢, ±15¢, ±25¢, the 95% and 90% frame shares and
  ±1.5 dB are engineering judgement, not published standards. The plan may
  adjust them after the first measurements, but should record why.
- **Parameters:** a whole-semitone `Shift` of ±12 (default +4) and a
  `Mix` default of 50% are my calls.
- **Editor:** the `Shift` and `Mix` controls are inferred from "Make the
  shift amount a parameter" and the need to listen-test the mix.
- **Out of scope:**
  - key/scale intervals, stabilisation and glide (Phase 5);
  - multiple voices, formant correction, humanisation and presets
    (Phase 6);
  - latency that changes with pitch;
  - polyphonic input.

## References

- Source: `ROADMAP.md` (Phase 4)
- Related: 0004 (realtime pitch detection), 0003 (tested gain DSP and basic
  plugin GUI; it moved the dry/wet mix here)
- Moulines & Charpentier, "Pitch-synchronous waveform processing techniques
  for text-to-speech synthesis using diphones", Speech Communication 9(5–6),
  1990
- Driedger & Müller, "A Review of Time-Scale Modification of Music Signals",
  Applied Sciences 6(2), 2016: https://www.mdpi.com/2076-3417/6/2/57
- Laroche & Dolson, "Improved phase vocoder time-scale modification of
  audio", IEEE Trans. Speech and Audio Processing, 1999
- KVR thread on PSOLA grain size and marks: https://www.kvraudio.com/forum/viewtopic.php?t=511549
- Kong, Stanford EE264 project on PSOLA artefacts: https://web.stanford.edu/class/ee264/projects/EE264_w2015_final_project_kong.pdf
- Reference implementations:
  - https://github.com/angelcandas/local-mic-effects (YIN + PSOLA, JUCE)
  - https://github.com/sannawag/TD-PSOLA
  - https://github.com/maxrmorrison/psola (GPL-3.0, reference only)
- Ableton latency compensation: https://help.ableton.com/hc/en-us/articles/360001820360
- Latency reporting quirks: https://cycling74.com/forums/report-vst3-plugin-latency-to-ableton,
  https://github.com/iPlug2/iPlug2/issues/731
