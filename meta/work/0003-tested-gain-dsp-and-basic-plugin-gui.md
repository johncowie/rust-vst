---
type: work-item
id: "0003"
title: "Tested Gain DSP and Basic Plugin GUI"
date: "2026-09-26T08:55:05+00:00"
author: John Cowie Del Corral
producer: create-work-item
status: draft
kind: story
priority: medium
blocked_by: ["work-item:0002"]
tags: [phase-2, dsp, testing, gui, egui]
last_updated: "2026-09-26T08:55:05+00:00"
last_updated_by: John Cowie Del Corral
schema_version: 1
---

# 0003: Tested Gain DSP and Basic Plugin GUI

**Kind**: Story
**Status**: Draft
**Priority**: Medium
**Author**: John Cowie Del Corral

## Summary

As the plugin developer, I want corral's DSP to live in a plain, unit-tested
module and the plugin to have a basic GUI showing the gain it is applying, so
that I can change DSP safely without a DAW round trip for every check, and see
what the plugin is doing when I test it in Ableton.

This is Phase 2 of `ROADMAP.md`, reduced to unit tests and a GUI. The offline
`.wav` harness, test audio and the dry/wet parameter move to later phases.

## Context

- 0002 gave corral a smoothed, automatable Gain parameter and a
  `mise run install` loop into Ableton Live 10.
  - The gain maths currently lives inline in `Plugin::process` and in the
    parameter's formatter closure in `corral/src/lib.rs`. None of it is tested.
  - The plugin has no editor (`type Editor = ()`), so Live shows its generic
    parameter panel.
- Later phases (pitch detection, pitch shifting, key/scale logic) are mostly
  pure DSP and logic that the roadmap wants unit-tested. This item sets up the
  pattern they'll follow: DSP in plain Rust, with `process()` as thin glue.
- **Testing a cdylib**: in-crate `#[cfg(test)]` modules work with the current
  `crate-type = ["cdylib"]`, because `cargo test` builds its own harness from
  the lib source. Only tests under `tests/`, benches or extra binaries would
  need a `lib`/`rlib` crate-type.
- **GUI options in nice-plug**:
  - `nice-plug-egui` (0.5.x on crates.io) works with nice-plug 0.4.2 and has
    a `gain_egui` example that is close to what we need.
  - `nice-plug-iced` pulls in far more dependencies.
  - Vizia is only available through the third-party, git-only `vizia-plug`.
- **nice-plug-egui's API differs from nih-plug's**:
  - The editor is a struct that implements `NiceEguiApp`, handed to
    `create_egui_editor`.
  - Editor state (`Arc<EguiEditorState>`) lives on the plugin struct. It
    isn't serialisable, so the window size isn't saved with the set.
  - Live values reach the GUI through an `Arc<AtomicF32>` and a repaint
    notifier.
- **macOS risk**: nice-plug-egui is "tested on Linux and Windows, with limited
  testing on macOS". baseview has had macOS click, keyboard and crash-on-open
  issues, all fixed now. Its default backend is OpenGL, which is deprecated on
  macOS but still works on 14.x. No reports were found either way for Rosetta
  or Live 10.

## Requirements

1. **DSP module**: create `corral/src/dsp.rs` with no `nice_plug` imports,
   containing:
   - applying a linear gain to a sample, with gains at or below the −60 dB
     floor giving exactly 0.0;
   - the silence threshold (currently `silence_threshold()`);
   - formatting a linear gain for display: "-inf" at or below the floor,
     otherwise dB to two decimal places, matching today's output.
2. **Glue**: `process()` and the Gain parameter's value-to-string formatter
   call into `dsp`. Behaviour in Live is unchanged from 0002, and the Gain
   parameter ID stays `gain`.
3. **Unit tests** in `dsp.rs` (`#[cfg(test)]`), covering at least:
   - 0 dB leaves a sample unchanged;
   - +12 dB, −6 dB and −12 dB scale by `10^(dB/20)`, within a small tolerance;
   - a gain at the floor, or below it, outputs exactly 0.0;
   - a gain just above the floor (e.g. −59.9 dB) is not silent;
   - display: the floor shows "-inf", unity shows "0.00 dB" (or whatever
     today's formatter produces), and +12 dB shows "12.00 dB".
4. **`mise run test` task** (`cargo test --workspace`), with a description.
   `mise run check` runs it after fmt-check and lint.
5. **Editor**:
   - Add `nice-plug-egui` (0.5.x, default `opengl` feature) and implement an
     editor following nice-plug's `gain_egui` example.
   - The editor has a `widgets::ParamSlider` for Gain, so edits go through the
     host's parameter setter and can be recorded as automation.
   - The editor has a text readout of the current **smoothed** gain, formatted
     by the `dsp` display function (so "-inf" at the floor).
   - The audio thread shares the readout value through an `Arc<AtomicF32>`
     (relaxed store). It requests a repaint only when the value has changed
     and the editor is open. Nothing on the audio thread allocates, locks or
     blocks.
   - Use a small fixed default window size. It doesn't need to be resizable
     or saved.
6. **README**: add `test` to the tasks table.
7. **ROADMAP**:
   - Tick "Unit tests for the DSP pieces" and "Basic GUI" once they're
     verified.
   - Move "Dry/wet mix parameter" to Phase 4.
   - Move the offline `.wav` harness and "Collect some test audio" to
     Phase 3, where the pitch detector needs them.

## Acceptance Criteria

- [x] Given the DSP module, when I run `mise run test`, then all gain and
      display tests pass. `mise run check` runs them and exits 0.
- [x] Given `corral/src/dsp.rs`, then it contains no `nice_plug` import.
- [x] `mise tasks` lists `test` with a description.
- [x] Given Corral is installed with `mise run install` and on an audio track
      in Live 10, when I open the plugin window, then an editor appears with
      a Gain slider and a gain readout. The window isn't blank.
- [x] Given the editor is open, when I drag the slider to −12 dB, then:
      - the readout glides to "-12.00 dB";
      - Live's own display of the Gain parameter matches;
      - the track meter peak drops about 12 dB, as in 0002.
- [x] Given Gain automation is playing back, when the editor is open, then
      the slider and the readout both follow the envelope.
- [x] Given the slider is at its minimum, then the readout shows "-inf" and
      the track meter shows no signal.
- [x] Given the editor is open, when I close and reopen the plugin window
      several times, and delete the device while its window is open, then
      Live doesn't crash.
- [x] Given a set with a non-default Gain value, when I save it, quit Live
      and reopen it, then the value is restored (no regression from 0002).

## Open Questions

## Dependencies

- Blocked by: 0002 (gain plugin loadable in Ableton Live 10)
- Blocks: Phase 3 (pitch detection), which reuses the DSP/test pattern and
  the GUI readout area

## Assumptions

- **Readout value:** "current gain" in the readout means the smoothed gain
  the audio thread is actually applying, not the parameter's target value.
  That's why the readout glides.
- **Window size:** it's acceptable that the editor's window size isn't saved
  with the set.
- **Deferring the harness:** it's acceptable to defer the offline `.wav`
  harness and test audio. Testing by ear in Ableton plus unit tests is enough
  for now.

## Technical Notes

- Follow `examples/gain_egui` in the nice-plug repo:
  - store `editor_state`, a repaint notifier and `initial_editor:
    Option<Editor>` on the plugin struct;
  - `editor()` calls `create_egui_editor(..., self.initial_editor.take()...)`.
- Set `type Editor` to the nice-plug-egui editor type. Take the exact type
  from the example.
- In `process()`, keep the last value stored in the `AtomicF32` in a local
  field, and only store and request a repaint when the new value differs
  (e.g. once per buffer, using the last frame's gain). This avoids a repaint
  request every sample.
- Test the x86_64 slice specifically, because Live 10 loads that one under
  Rosetta.
- If the editor fails in Live 10 (blank window, crash on open or close),
  record the symptom and stop. The next step is decided then (options include
  the `wgpu` feature, `nice-plug-iced`, or dropping the GUI from this item).

## Drafting Notes

- **Scope cut:** Phase 2 is reduced to unit tests and a GUI because the
  developer isn't attached to the offline harness or test audio, and is happy
  to test by ear in Ableton for now. Dry/wet waits until Phase 4, when there
  is a real wet signal.
- **Why egui:** nice-plug-egui was chosen because it's the simplest official
  option with a matching example. The developer had no preference.
- **Size of the DSP split:** it's deliberately small (gain only). Its value
  is setting up the "pure DSP + thin glue" pattern for later phases.
- **Tests in `check`:** tests are added to `mise run check` because they are
  fast and pure, so the CI entry point covers them.
- **Integration tests:** tests under `tests/` are out of scope, which avoids
  changing `crate-type`.
- **GUI risk in Live 10:** the developer chose to try the default egui
  backend and deal with any Live 10 / Rosetta problems if they come up,
  rather than planning a fallback up front.
- **Priority:** set to medium rather than high because this item is
  foundations, not user-facing harmoniser behaviour.

## References

- Source: `ROADMAP.md` (Phase 2)
- Related: 0002 (gain plugin loadable in Ableton Live 10)
- nice-plug repo and gain_egui example: https://codeberg.org/RustAudio/nice-plug/src/branch/main/examples/gain_egui/src/lib.rs
- nice-plug-egui on crates.io: https://crates.io/crates/nice-plug-egui
- EguiEditorState docs: https://docs.rs/nice-plug-egui/latest/nice_plug_egui/struct.EguiEditorState.html
- baseview issues (macOS): https://github.com/RustAudio/baseview/issues
