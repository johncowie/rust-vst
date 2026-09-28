---
type: work-item
id: "0006"
title: "Key-Aware Diatonic Harmony"
date: "2026-09-28T14:36:44+00:00"
author: John Cowie Del Corral
producer: create-work-item
status: draft
kind: story
priority: high
blocked_by: ["work-item:0005"]
tags: [phase-5, dsp, harmony, music-theory, stabilisation, glide, testing]
last_updated: "2026-09-28T14:36:44+00:00"
last_updated_by: John Cowie Del Corral
schema_version: 1
---

# 0006: Key-Aware Diatonic Harmony

**Kind**: Story
**Status**: Draft
**Priority**: High
**Author**: John Cowie Del Corral

## Summary

As the plugin developer, I want Corral to pick the harmony interval from
a Key and Scale I choose, so that singing into it produces a diatonic
third above my voice. For example, in C major a sung C gets an E and a
sung D gets an F. This turns the fixed shifter into the auto-harmoniser
the project is aiming for.

This is Phase 5 of `ROADMAP.md`. It covers major and natural-minor scales,
an out-of-key rule, note stabilisation so vibrato doesn't make the
harmony flicker, and a glide between harmony notes.

## Context

- **What 0005 delivered:**
  - a causal TD-PSOLA shifter driven by a whole-semitone `Shift`
    parameter (−12 to +12);
  - a 50 ms anti-click `SemitoneRamp` (`corral-dsp/src/shift.rs`), whose
    comment says a musical glide is Phase 5;
  - an unvoiced wet gate with a crossfade;
  - a dry/wet `Mix`;
  - a fixed latency for each sample rate.
- **What 0004 delivered:**
  - the YIN tracker (256-sample hop, 34–40 ms from note onset to
    detection) and its voicing gate;
  - `note.rs`, which converts a frequency to a MIDI note plus cents.
- **Phase 4 is treated as done:** the developer considers it complete. Its
  ROADMAP boxes are ticked as part of this work.
- **Vibrato carries through:** the harmony is the live input multiplied by
  a ratio, so the singer's vibrato and intonation reach the harmony in
  parallel. This matches TC-Helicon's "Smoothing" at 100% and Antares'
  Naturalize in fixed-interval mode. Only the choice of interval is
  quantised.
- **Research on stabilisation:**
  - Commercial harmonisers don't publish their note-change hysteresis.
  - Waves Tune Real-Time changes note only when pitch passes 50¢ plus a
    user-set 0–40¢ margin, optionally held for 0–300 ms.
  - Tony/pYIN treats changes of at least ⅔ semitone as note changes and
    prunes notes shorter than about 100 ms.
- **Research on glide:**
  - Antares Harmony Engine glides exponentially (0–1000 ms half-life,
    default 125 ms), only between notes that touch; after any silence it
    jumps.
  - TC-Helicon has a portamento time for each voice.
- **Research on out-of-key notes:**
  - None of the products researched mutes the harmony.
  - TC-Helicon holds the previous harmony note ("nc").
  - Eventide and Waves snap the input to the scale.

## Requirements

1. **Parameters:**
   - Add `Key` (C, C#, …, B; default C) and `Scale` (Major, Natural minor;
     default Major).
   - Remove `Shift`, along with its editor control and harness flag.
   - `Gain` and `Mix` are unchanged.
2. **Interval logic** as pure functions in `corral-dsp` (no `nice_plug`
   imports):
   - Map the stable note's pitch class (octave ignored) to a scale degree,
     then to the number of semitones up to the diatonic third above:
     - major: I +4, ii +3, iii +3, IV +4, V +4, vi +3, vii +3;
     - natural minor: i +3, ii +3, III +4, iv +3, v +3, VI +4, VII +4.
   - **Out-of-key rule:** a note that is not in the scale uses the interval
     of the scale note a semitone below. For example, in C major C# uses
     C's +4 (giving F), F# uses F's +4 (giving A#), and Eb uses D's +3
     (giving F#).
   - The harmony never drops out because a note is out of key.
3. **Note stabiliser** (per hop, no allocation):
   - The stable note changes only when the detected pitch is more than
     70¢ from the stable note's centre and stays there for at least
     60 ms.
   - Fast path: a deviation of more than 150¢ that lasts 3 hops switches
     straight away.
   - At voicing onset (the first voiced hop after unvoiced), the nearest
     note is taken immediately, without the hold.
   - Out-of-range and octave-error detections are ignored in the same way
     as the period smoothing from 0005.
4. **Glide:**
   - A `Glide` parameter from 0 to 250 ms, default 50 ms.
   - The shift in semitones moves exponentially toward the new interval.
   - At voicing onset the shift jumps to the interval with no glide.
   - It replaces `SemitoneRamp`.
   - Glide = 0 and mid-note changes to `Key` or `Scale` must still be
     click-free. The plan decides whether a minimum ramp is needed.
5. **Latency** is unchanged from 0005. It doesn't depend on `Key`, `Scale`
   or `Glide`.
6. **Harness:**
   - `mise run analyse` gains `--key`, `--scale` and `--glide` and drops
     `--shift`.
   - The per-hop CSV gains columns for the stable note, the interval in
     semitones and the harmony note.
7. **Editor:**
   - `Key`, `Scale` and `Glide` controls.
   - The readout shows the input and harmony notes, e.g. "D4 → F4".
8. **Objective tests** in `cargo test`, with scores printed so tuning
   changes can be compared (see Acceptance Criteria).
9. **README:** document `Key`, `Scale`, `Glide`, the out-of-key rule, the
   stabiliser and glide values, the new harness flags and the measured
   scores.
10. **ROADMAP:**
    - Tick the Phase 4 items.
    - Tick the Phase 5 items once verified.
    - Record the out-of-key decision against the Phase 5 item that asks
      for it.

## Acceptance Criteria

- [ ] Given every combination of 12 keys, 2 scales and 12 input pitch
      classes, when the interval function runs, then it matches a
      hand-written reference table (an exhaustive unit test).
- [ ] Given a synthesised C major scale (C4 to C5, harmonic tones, 500 ms
      notes) with Key = C, Scale = Major and Mix = 100%, when the output
      is analysed with YIN at 44.1 and 48 kHz, then ≥ 95% of voiced frames
      are within ±25¢ of the expected diatonic third. Frames within 100 ms
      of a note change are excluded.
- [ ] Given a synthesised A natural-minor scale (A3 to A4) with Key = A
      and Scale = Natural minor, when analysed as above, then ≥ 95% of
      voiced frames are within ±25¢ of the expected diatonic third.
- [ ] Given a held synthesised note with ±100¢ sinusoidal vibrato at 5 Hz
      and at 7 Hz, when processed, then the stable note and the interval
      never change after onset.
- [ ] Given a synthesised scoop that rises from 200¢ below the target over
      up to 150 ms, when processed, then the stable note settles on the
      target and changes at most once after onset.
- [ ] Given a synthesised note that starts after silence, when processed,
      then the stable note is chosen on the first voiced hop and the shift
      reaches the interval with no glide.
- [ ] Given a synthesised step from C4 to D4 (legato) in C major with
      Glide = 50 ms, when processed, then the stable note changes within
      60 ms plus 3 hops of the detector reporting D4, and the shift moves
      from +4 to +3 without jumping.
- [ ] Given out-of-key synthesised notes in C major (C#4, Eb4, F#4, G#4,
      Bb4), when processed, then the harmony pitches are F4, F#4, A#4, C5
      and C#5 (±25¢ median), and the harmony never drops out.
- [ ] Given interval changes, mid-note `Key` and `Scale` changes, and a
      `Glide` sweep from 0 to 250 ms, when processed, then no
      sample-to-sample jump exceeds the click threshold calibrated in
      0005.
- [ ] Given `corral-dsp/tests/no_alloc.rs`, when it runs, then the
      stabiliser and glide allocate nothing.
- [ ] Given `mise run analyse in.wav out.wav --key C --scale major
      --glide 50 --mix 100`, then the output WAV and a CSV with the
      stable-note, interval and harmony-note columns are written.
- [ ] Given Corral on an audio track in Live 10 with Key = C, Scale =
      Major and Mix = 50%, when I sing a C major scale, then I hear a
      diatonic third above each note. On held notes with vibrato the
      harmony does not flicker.
- [ ] Given Corral in Live 10, when I change `Key`, `Scale` or `Glide`
      while singing, then there are no audible clicks and the reported
      latency doesn't change.
- [ ] The README documents `Key`, `Scale`, `Glide`, the out-of-key rule,
      the stabiliser values and the new harness flags.
- [ ] `mise run check` passes.

## Open Questions

## Dependencies

- Blocked by: 0005 (TD-PSOLA pitch shifter), which provides the shifter,
  the wet gate, `Mix`, the latency and the click threshold. The developer
  considers it done.
- Blocks: Phase 6 (interval choice, multiple voices, MIDI key selection).

## Assumptions

- **Removing `Shift`:** this breaks saved Live sets and presets that used
  it. There are no users yet, so no migration is needed.
- **Harmony direction:** the harmony is only a third above. Other
  intervals and directions are Phase 6.
- **Pitch correction:** the harmony follows the singer's intonation and
  vibrato in parallel (ratio times live input). It is not corrected to
  exact scale pitches.

## Technical Notes

- **Vibrato arithmetic:**
  - A ±100¢ sinusoid spends about 25% of each cycle beyond 70¢, which is
    about 42 ms per swing at 6 Hz and about 51 ms at 5 Hz. A 60 ms hold
    rejects it.
  - Vibrato of ±70¢ or less never crosses the threshold.
- **Median filter:** a 3–5-hop running median before the threshold test
  could also remove outlier and octave errors. The plan can decide.
- **Timing:** the interval choice lags the detector by about 60 ms on held
  changes (about 95–100 ms after the audio onset). The harmony pitch still
  follows the live input immediately.
- **Chromatic output:** the harmony is the input times a ratio, so some
  out-of-key inputs always give a chromatic harmony (F# → A# in C major),
  whichever rule is used. This is the parallel passing tone other
  harmonisers also produce.
- **Why "the note below":** every chromatic note in a major or
  natural-minor scale sits exactly between two scale notes, so "nearest
  scale note" is always a tie. "The note below" breaks the tie with a
  stateless 12-entry table.
- **Glide shape:** the glide works in semitones (log frequency), not Hz.
  `Glide` is a time constant or half-life; the plan picks one and
  documents it.
- **Signals the stabiliser needs:** the voicing-onset signal comes from the
  existing voicing gate, and the "unvoiced → jump" rule matches Antares
  Harmony Engine.
- **Rosetta:** test the x86_64 slice in Live 10 (Rosetta), as in 0002 to
  0005.

## Drafting Notes

- **Scope:** all of Phase 5 is one story, split into implementation phases
  by the plan. The developer chose this.
- **Scales:** only major and natural minor, as the developer decided. Other
  scales and modes are out of scope.
- **`Shift`:** removing it was the developer's decision.
- **Out-of-key rule:** "use the interval of the scale note below" was my
  recommendation, and the developer accepted it over their first instinct
  to skip the shift. No product I found mutes the harmony. Muting would
  drop the harmony on passing and blue notes, and near note boundaries.
- **Stabiliser values:** 70¢, 60 ms and 150¢ over 3 hops are my inference
  from Waves' and Tony's published figures. The plan may tune them after
  measurement, but should record why.
- **`Glide`:** making it a user parameter, with a 0–250 ms range and a
  50 ms default, is my call. It uses a narrower range than Antares
  (0–1000 ms), because third-to-third changes are mostly one semitone.
- **Scale-test tolerances:** ±25¢, 95% and the 100 ms transition exclusion
  follow 0005's tolerances and are engineering judgement.
- **ROADMAP:** ticking Phase 4 here follows from the developer treating it
  as done.
- **Out of scope:**
  - other scales and modes;
  - interval choice, harmony below the input, and multiple voices
    (Phase 6);
  - pitch-correcting the harmony;
  - setting the key from MIDI;
  - automatic key detection.

## References

- Source: `ROADMAP.md` (Phase 5)
- Related: 0005 (TD-PSOLA pitch shifter), 0004 (realtime pitch detection)
- Waves Tune Real-Time user guide (Note Transition Cents and Time,
  note states): https://assets.wavescdn.com/pdf/plugins/tune-real-time.pdf
- Mauch et al., "Computer-aided Melody Note Transcription Using the Tony
  Software", TENOR 2015:
  https://www.tenor-conference.org/proceedings/2015/04-Mauch-Tony.pdf
- Antares Harmony Engine Evo manual (Transition Rate, Naturalize, Scale
  Interval mode): https://www.strumentimusicali.net/manuali/ANTARES_HE_ENG.pdf
- TC-Helicon VoiceWorks (Scalic mode, "nc", Smoothing):
  https://www.manualslib.com/manual/685721/Tc-Helicon-Voiceworks-Harmony.html?page=25
- TC-Helicon VoiceLive 2 (portamento, Smoothing):
  https://www.manualslib.com/manual/817814/Tc-Helicon-Voicelive-2.html?page=66
- Eventide H90 Harmonizer (Quantization):
  https://cdn.eventideaudio.com/manuals/h90/1.10.7/content/algorithms/harmonizer.html
