---
type: work-item
id: "0007"
title: "Harmony Grid UI and Multi-Voice Harmony"
date: "2026-09-29T08:57:34+00:00"
author: John Cowie Del Corral
producer: create-work-item
status: draft
kind: story
priority: high
relates_to: ["work-item:0006"]
tags: [phase-6, gui, dsp, harmony, multi-voice, glide]
last_updated: "2026-09-29T08:57:34+00:00"
last_updated_by: John Cowie Del Corral
schema_version: 1
---

# 0007: Harmony Grid UI and Multi-Voice Harmony

**Kind**: Story
**Status**: Draft
**Priority**: High
**Author**: John Cowie Del Corral

## Summary

As the plugin developer, I want to choose, separately for each detected
pitch, which extra pitches sound, using a grid of toggles in the plugin
window. For example, "on C, add the E above and the G below; on D, add the
F above". This turns Corral's single diatonic third into a harmoniser I
can program, with any number of voices.

The grid replaces `Key` and `Scale` completely. `Gain` goes. `Glide` and
`Mix` stay. This becomes the new Phase 6 of `ROADMAP.md`.

## Context

- **What 0006 delivered:**
  - `Key` and `Scale` choose a single diatonic third above
    (`corral-dsp/src/harmony.rs`);
  - a note stabiliser (`stabiliser.rs`);
  - an exponential `Glide` in semitones (`shift.rs`);
  - one TD-PSOLA voice (`psola.rs`), driven by `Chain`
    (`chain.rs`).
- **The editor today** is a 300×340 egui window
  (`corral/src/editor.rs`). It has sliders for Gain, Key, Scale, Glide
  and Mix, the gain readout, the harmony readout ("D4 → F4") and the raw
  detected note.
- **The developer's sketch**
  (`meta/work/assets/0007-harmony-grid-sketch.png`) shows:
  - a bordered centre column of the 12 pitch classes;
  - 12 toggle cells to its left and 12 to its right in each row.
  In the sketch, the top row reads C…B, **C**, C#…C, and the right-hand
  column repeats each row's own name. So a column is an **interval
  relative to the row's pitch** (−12…+12), not a fixed note.
- **The agreed mock-up** is at
  `meta/work/assets/0007-harmony-grid-mockup.png`.

## Requirements

1. **Parameters:**
   - Remove `Key` and `Scale`.
   - Remove `Gain` completely: the parameter, the editor slider and the
     gain readout.
   - `Glide` and `Mix` are unchanged.
2. **Grid model** as pure code in `corral-dsp` (no `nice_plug` imports):
   - 12 rows, one for each pitch class C…B;
   - 24 intervals in each row: −12…−1 and +1…+12;
   - each cell is either on or off;
   - the grid starts empty;
   - a row returns its "on" intervals in ascending order, with no
     allocation.
3. **Several voices:**
   - When the stable note's pitch class is *p*, each "on" interval in row
     *p* becomes one voice.
   - A voice is the live input shifted by that interval (the stable note
     plus the interval, so the octave follows the singer).
   - Every voice plays at full level, and the voices are summed into the
     wet signal. The level is not scaled down as voices are added.
   - There is no cap on voices below 24 for each row. All 24 voice slots
     are allocated in `initialize()`, and the audio thread allocates
     nothing.
4. **Voice changes when the stable note changes:**
   - The old row's voices are matched to the new row's by position, both
     in ascending order: the lowest old voice goes to the lowest new
     voice, and so on.
   - Matched voices glide using `Glide`.
   - Extra new voices fade in, and old voices with no match fade out,
     without clicks.
   - At voicing onset, voices jump straight to their intervals with no
     glide, as in 0006.
5. **Editing while singing:** switching a toggle in the row that's
   sounding adds or removes a voice with the same fade, without clicks.
   The change comes from the GUI thread and has to reach the audio thread
   without locks.
6. **Unchanged:** the note stabiliser, the wet gate and the latency. The
   latency doesn't depend on the grid or the number of voices.
7. **Editor:**
   - The window grows to about 960×560 (fixed size).
   - A 12 × 25 grid, as in the mock-up:
     - every cell is labelled upright with its note name (the row's pitch
       plus the column's interval);
     - an interval ruler (−12…+12) runs along the top.
   - The bordered centre column is display only:
     - it highlights the **stable** note's pitch class;
     - when unvoiced, nothing is highlighted.
   - The 288 toggle cells switch between on and off when clicked.
   - "On" cells in the row that's sounding get an extra "sounding"
     highlight.
   - Sliders for Glide and Mix.
   - A readout of the input note and its voices, e.g. "C4 → G3 · E4", with
     the raw detected note underneath.
8. **Harness:**
   - `mise run analyse` drops `--key` and `--scale`.
   - It gains `--voices`, taking a grid spec such as `C:-5,+4 D:+3`.
   - The per-hop CSV lists the stable note and the active intervals.
9. **Tests:** replace 0006's key/scale tests with grid and multi-voice
   tests (see Acceptance Criteria), with scores printed as in 0006.
10. **README:**
    - Document the grid, the voice-matching and fade behaviour, and the
      `--voices` flag.
    - Record the measured CPU cost of a full row of 24 voices.
    - Warn that several voices at full level can clip.
    - Remove the docs for `Gain`, `Key` and `Scale`.
11. **ROADMAP:**
    - Insert a new "Phase 6: Harmony grid" for this work.
    - Renumber the current Phase 6 to Phase 7, and remove or strike its
      "choice of harmony interval" and "multiple harmony voices" items,
      since this phase covers them.
    - Add two Icebox items:
      - preset grid fills (for example, chords in a chosen key);
      - saving the grid with the set or preset, and host automation of
        the toggles.

## Acceptance Criteria

- [ ] Given the grid model, when any of the 288 cells is toggled, then
      each row's ascending interval list matches a reference (exhaustive
      unit test).
- [ ] Given an empty grid and Mix = 50%, when a synthesised scale is
      processed, then the wet signal is silent and the output equals the
      delayed dry signal scaled by (1 − Mix).
- [ ] Given row C = {−5, +4} and a synthesised held C4 at 44.1 and
      48 kHz, when each voice is rendered alone and analysed with YIN,
      then the voices are G3 and E4, each within ±25¢ (median). The
      summed output equals the sum of the voices rendered alone.
- [ ] Given row C = {+4}, row D = {+3} and a synthesised legato C4 → D4
      with Glide = 50 ms, when processed, then the single voice moves
      from +4 to +3 without jumping (a glide, not a fade).
- [ ] Given row C = {+4}, row D = {+3, +7} and a legato C4 → D4 (and the
      reverse, D4 → C4), when processed, then the added voice fades in
      (or the removed one fades out). No sample-to-sample jump exceeds
      the click threshold calibrated in 0005.
- [ ] Given toggles switched on and off mid-note in the row that's
      sounding, when processed, then no jump exceeds the click threshold.
- [ ] Given a synthesised note that starts after silence with row C =
      {−5, +4}, when processed, then both voices start at their intervals
      with no glide.
- [ ] Given `corral-dsp/tests/no_alloc.rs` with all 24 intervals of the
      sounding row on, when it runs (including note changes and toggle
      edits), then nothing is allocated.
- [ ] Given all 24 voices on, when 10 s of audio is processed at 48 kHz
      in a release build, then the time per block is measured and
      recorded in the README. This records a figure; there is no pass or
      fail threshold.
- [ ] Given `mise run analyse in.wav out.wav --voices C:-5,+4 D:+3
      --glide 50 --mix 100`, then the output WAV is written, along with a
      CSV that lists the stable note and the active intervals for each
      hop.
- [ ] Given Corral on an audio track in Live 10 with row C = {−5, +4} and
      Mix = 50%, when I sing a C, then I hear G below and E above. The C
      in the centre column lights up, and the two toggles show as
      sounding.
- [ ] Given Corral in Live 10, when I sing a note whose row is empty,
      then I hear only the dry signal.
- [ ] Given Corral in Live 10, when I switch toggles or move from one
      programmed note to another while singing, then there are no audible
      clicks and the reported latency doesn't change.
- [ ] The editor has no Gain, Key or Scale controls, and every grid cell
      is labelled upright.
- [ ] `mise run check` passes.

## Open Questions

## Dependencies

- Blocked by: none. 0006 (key-aware harmony) is done and provides the
  stabiliser, glide and single-voice chain that this generalises.
- Blocks: the follow-up work item for saving and automating the grid,
  and the Icebox preset fills.

## Assumptions

- **Gain:** it is removed completely, as a parameter as well as from the
  UI. This breaks saved sets that automate it. There are no users yet, so
  no migration is needed.
- **Saving the grid:** out of scope. The grid resets to empty whenever the
  plugin is reloaded or a set is reopened. Saving and automation come in a
  separate work item.
- **Mix:** it keeps its current meaning. With Mix = 100% and no voices on
  in the row that's sounding, the output is silent.

## Technical Notes

- **Pitch classes only:** rows match the stable note's pitch class, so C3
  and C5 both use row C. Intervals apply to the actual stable note, so
  voices follow the singer's octave.
- **Voice matching:**
  - Matching by position (ascending order) lets a +4 voice glide to +3.
    Matching by column would fade it out and a new voice in. This is the
    same idea as voice leading.
  - Surplus voices fade in or out over a short fixed ramp (for example
    the 0005 anti-click length). The plan picks the fade time and shape.
- **Sharing the work between voices:**
  - `Psola` today holds its own input buffer and pitch marks, and
    `next(period, ratio)` gives one output.
  - The plan should decide whether to share the analysis side (input
    buffer, period, marks) and run up to 24 synthesis sides, or use 24
    full `Psola` instances. Sharing uses less memory and CPU.
- **Getting the grid to the audio thread:** 288 cells fit in 12 × `u32`
  bitmasks (24 bits each), or a single `[AtomicU32; 12]` shared between
  the editor and the audio thread. That is lock-free and doesn't
  allocate.
- **Clipping:** 24 voices at full level can clip a lot. This is accepted
  for now, and the README says so.
- **egui layout:** about 30 px square cells with a 4 px gap. The mock-up
  is drawn at that scale.
- **Rosetta:** test the x86_64 slice in Live 10 (Rosetta), as in 0002 to
  0006.

## Drafting Notes

- **Column meaning:** columns are intervals relative to the row's pitch.
  I inferred this from the sketch's labelling (the top row reads C…B,
  **C**, C#…C, and the right-hand column repeats the row name), and the
  developer confirmed it with the mock-up.
- **Grid replaces Key and Scale:** the developer's decision.
- **Starting state:** the developer chose an empty grid.
- **Voice count:** no cap, as the developer decided ("see how it goes").
  So the CPU measurement is recorded rather than tested against a limit.
- **Voice levels:** full level, with no normalisation, as the developer
  decided.
- **Saving and automation:** the developer put them out of scope. I've
  read this as covering both saving and automation, so the grid is not
  saved at all.
- **Glide across notes:** the developer accepted that voices keep the
  existing glide, and that surplus voices fade. Matching by position is
  my call.
- **Additions I made:** the interval ruler and the voice readout
  ("C4 → G3 · E4") weren't in the sketch. The developer saw them in the
  mock-up and didn't object.
- **Stable note, not raw detection:** the centre-column highlight follows
  the stable note rather than the raw detection, so it matches what the
  harmony is doing. The raw note is still shown in the readout.
- **Scope and roadmap:** keeping this as one story and inserting it as a
  new Phase 6 (with the current Phase 6 moving to Phase 7) are the
  developer's choices.
- **Out of scope:**
  - saving the grid and automating toggles;
  - preset fills (chords in a key);
  - per-voice level and pan, and normalising levels;
  - a voice cap;
  - formant correction and humanisation (Phase 7).

## References

- Source: developer's sketch, `meta/work/assets/0007-harmony-grid-sketch.png`
- Mock-up: `meta/work/assets/0007-harmony-grid-mockup.png`
- Related: 0006 (key-aware diatonic harmony), 0005 (TD-PSOLA pitch
  shifter)
- `ROADMAP.md` (Phase 6 and Icebox)
