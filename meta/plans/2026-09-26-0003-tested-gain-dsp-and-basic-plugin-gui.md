---
type: plan
id: "2026-09-26-0003-tested-gain-dsp-and-basic-plugin-gui"
title: "Tested Gain DSP and Basic Plugin GUI Implementation Plan"
date: "2026-09-26T09:09:32+00:00"
author: John Cowie Del Corral
producer: create-plan
status: draft
work_item_id: "work-item:0003"
parent: "work-item:0003"
tags: [phase-2, dsp, testing, gui, egui]
revision: "1dee02d1668f785ea0ae9621d289220609c67efa"
repository: "rust-vst"
last_updated: "2026-09-26T09:09:32+00:00"
last_updated_by: John Cowie Del Corral
schema_version: 1
---

# Tested Gain DSP and Basic Plugin GUI Implementation Plan

## Overview

This plan delivers the reduced Phase 2 of `ROADMAP.md`:

- a plain `corral/src/dsp.rs` module (no nice-plug imports) that holds the gain
  maths and the gain display formatter, with `#[cfg(test)]` unit tests;
- `process()` and the Gain formatter reduced to thin calls into `dsp`;
- a `mise run test` task, which `mise run check` now also runs;
- a small, fixed-size egui editor with a Gain `ParamSlider` and a readout of
  the smoothed gain the audio thread is applying;
- README and ROADMAP updates;
- manual verification in Ableton Live 10 (x86_64 under Rosetta).

## Current State Analysis

- `corral/src/lib.rs:5-6`: `MIN_GAIN_DB` (−60) and `MAX_GAIN_DB` (+12).
- `corral/src/lib.rs:9-12`: `Corral` derives `Default` and holds only
  `params`.
- `corral/src/lib.rs:21-25`: `silence_threshold()` is
  `util::db_to_gain(MIN_GAIN_DB) * 1.0001`.
- `corral/src/lib.rs:27-50`: the Gain formatter wraps
  `formatters::v2s_f32_gain_to_db(2)` and returns `"-inf"` at or below the
  threshold. The `" dB"` unit is appended by nice-plug, not by the formatter.
- `corral/src/lib.rs:67`: `type Editor = ();`.
- `corral/src/lib.rs:75-93`: `process()` computes the threshold once per
  block, takes one smoothed value per frame, and writes `0.0` at the floor,
  otherwise `sample * gain`.
- `corral/Cargo.toml`: `crate-type = ["cdylib"]`, one dependency
  (`nice-plug = "0.4"`, resolved to 0.4.2).
- `mise.toml`: `check` runs `fmt-check` then `lint`. There is no `test`
  task. There are no tests anywhere.
- `README.md:43-54`: the tasks table.
- `ROADMAP.md`, Phase 2: five unticked items (harness, unit tests, GUI,
  dry/wet, test audio).

## Desired End State

- `corral/src/dsp.rs` exists, has no `nice_plug` import, and holds
  `MIN_GAIN_DB`, `SILENCE_THRESHOLD`, `apply_gain` and `format_gain`, with
  unit tests.
- `mise run test` runs `cargo test --workspace` and passes. `mise run check`
  runs fmt-check, lint and test, and exits 0.
- Corral opens a 300×120 window in Live 10 with a Gain slider and a
  `Gain: <value> dB` readout that glides with the smoother and follows
  automation.
- Behaviour, parameter ID (`gain`) and VST3 class ID (`JCowieCorral0001`)
  are unchanged from 0002. Saved sets still load.
- README and ROADMAP are updated, and every acceptance criterion in work item
  0003 has been checked.

### Key Discoveries:

- **egui must be a direct dependency with `default_fonts`.**
  `nice-plug-egui` 0.5.1 depends on `egui` with `default-features = false`,
  so no fonts are compiled in and every label would render blank. The
  `gain_egui` example adds `egui = { version = "0.36.1", features =
  ["default_fonts"] }` itself. We do the same.
- **`NiceEguiApp::editor_closed()` is never called** by nice-plug-egui 0.5.1
  (it's declared in `src/lib.rs:94` and invoked nowhere). So the editor
  keeps only an `Option<GuiContext>`, overwritten in `build()` each time the
  window opens. It doesn't hold the `egui::Context` or rely on close-time
  cleanup.
- **The audio-thread side is realtime-safe.**
  - `RepaintNotifier::request_repaint()` is a relaxed `AtomicBool` store
    (`egui-baseview-0.7.2/src/window.rs:40-42`).
  - `EguiEditorState::is_open()` is an atomic load
    (`nice-plug-egui-0.5.1/src/lib.rs:163-165`).
  - `AtomicF32` is in `nice_plug::prelude` (`nice-plug-0.4.2/src/prelude.rs:36`),
    so the `atomic_float` crate isn't needed.
- **The egui root `Ui` has no background.** egui-baseview calls
  `egui_ctx.run_ui(input, |ui| app.ui(ui, frame))`
  (`egui-baseview-0.7.2/src/window.rs:591`). nice-plug-egui's own
  `ResizableWindow` wraps its content in `CentralPanel::default().show(ui, …)`
  (`src/resizable_window.rs:35`). We do the same.
- **`Plugin::editor` is called once** and takes `&mut self`
  (`nice-plug-core-0.4.2/src/plugin.rs:204-209`), so the editor app is held
  in `initial_editor: Option<CorralEditor>` and moved out with `take()`.
- **`EguiNiceSettings` defaults to non-resizable** (`resize_hint`
  default). Its title setter is spelled `with_tile`
  (`nice-plug-egui-0.5.1/src/editor.rs:48-51`).
- **Today's formatter output.** `v2s_f32_gain_to_db(2)` returns the number
  only (`"0.00"`, `"12.00"`), never `"-0.00"` (it rounds and checks for
  zero), and nice-plug appends `" dB"` when showing the value
  (`nice-plug-core-0.4.2/src/formatters.rs:85-101`,
  `src/params/float.rs:143-147`). `dsp::format_gain` reproduces this
  exactly. Its `"-inf"` branch (below 1e-5) is unreachable for us, because
  our threshold is higher.
- **`util::db_to_gain` is `10^(dB × 0.05)`** above −100 dB
  (`nice-plug-core-0.4.2/src/util.rs:46-52`). The tests use the same formula.
  `10f32.powf(-3.0)` is within an ulp of `0.001`, far inside the `1.0001`
  tolerance, so a constant threshold matches the old function.
- **`ParamSlider` goes through the host.** `ParamSlider::for_param(&param,
  &setter)` uses `ParamSetter`'s begin/set/end calls, so edits are recorded
  as automation. Its text comes from the parameter's own formatter, so it
  shows `"-inf dB"` at the floor.

## What We're NOT Doing

- The offline `.wav` harness, test audio, and the dry/wet parameter (moved to
  Phases 3 and 4 of the roadmap).
- Integration tests under `tests/`, benches, or changing `crate-type`.
- A resizable window, saving the window size, or a zoom control.
- Message channels or triple buffers between GUI and audio thread.
- nice-plug's `assert_process_allocs` feature (it only acts in debug builds,
  and we install release builds).
- Any fallback GUI backend (`wgpu`, iced). If the editor fails in Live 10,
  we stop and decide then.
- Any change to the gain range, smoothing, parameter ID or class ID.

## Implementation Approach

1. Extract the DSP and add tests first, with no behaviour change. The tests
   pin today's behaviour before the GUI work touches `lib.rs` again.
2. Add the editor.
3. Update the docs.
4. Verify by hand in Live 10, then tick the roadmap.

---

## Phase 1: DSP Module, Tests and `test` Task

### Overview

Move the gain maths and display formatting into `dsp.rs`, cover them with
unit tests, rewire `lib.rs` to call them, and add `mise run test` to
`check`.

### Changes Required:

#### 1. New DSP module

**File**: `corral/src/dsp.rs` (new)

```rust
//! Plain DSP for corral. Nothing here depends on nice-plug, so it can be
//! unit-tested without a host.

/// The bottom of the Gain range. The floor is treated as silence ("-inf").
pub const MIN_GAIN_DB: f32 = -60.0;

/// Linear gain at [`MIN_GAIN_DB`]: 10^(−60/20).
const FLOOR_GAIN: f32 = 0.001;

/// Gains at or below this are the floor and output silence. The smoother
/// lands exactly on its target, so the tolerance only absorbs float error.
pub const SILENCE_THRESHOLD: f32 = FLOOR_GAIN * 1.0001;

/// Applies a linear gain to one sample. At or below the floor the result is
/// exactly 0.0, even for non-finite input.
#[inline]
pub fn apply_gain(sample: f32, gain: f32) -> f32 {
    if gain <= SILENCE_THRESHOLD {
        0.0
    } else {
        sample * gain
    }
}

/// Formats a linear gain as dB to two decimal places, without the unit.
/// The floor shows "-inf". Matches nice-plug's `v2s_f32_gain_to_db(2)`,
/// including never showing "-0.00".
pub fn format_gain(gain: f32) -> String {
    if gain <= SILENCE_THRESHOLD {
        return String::from("-inf");
    }

    let db = gain.log10() * 20.0;
    if (db * 100.0).round() / 100.0 == 0.0 {
        format!("{:.2}", 0.0)
    } else {
        format!("{db:.2}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The same formula as nice-plug's `util::db_to_gain`, which builds the
    /// parameter's range.
    fn db_to_gain(db: f32) -> f32 {
        10.0f32.powf(db * 0.05)
    }

    fn assert_close(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() <= expected.abs() * 1e-5,
            "{actual} is not close to {expected}"
        );
    }

    #[test]
    fn threshold_sits_just_above_the_floor() {
        let floor = db_to_gain(MIN_GAIN_DB);
        assert!(floor <= SILENCE_THRESHOLD);
        assert!(SILENCE_THRESHOLD < floor * 1.001);
    }

    #[test]
    fn unity_gain_leaves_sample_unchanged() {
        assert_eq!(apply_gain(0.5, db_to_gain(0.0)), 0.5);
    }

    #[test]
    fn gain_scales_by_ten_to_the_db_over_twenty() {
        for (db, factor) in [(12.0, 3.981_072), (-6.0, 0.501_187_2), (-12.0, 0.251_188_6)] {
            assert_close(apply_gain(0.5, db_to_gain(db)), 0.5 * factor);
        }
    }

    #[test]
    fn floor_is_silent() {
        assert_eq!(apply_gain(1.0, db_to_gain(MIN_GAIN_DB)), 0.0);
    }

    #[test]
    fn below_floor_is_silent() {
        for gain in [db_to_gain(-80.0), 0.0] {
            assert_eq!(apply_gain(1.0, gain), 0.0);
        }
    }

    #[test]
    fn floor_is_silent_for_non_finite_input() {
        let floor = db_to_gain(MIN_GAIN_DB);
        assert_eq!(apply_gain(f32::NAN, floor), 0.0);
        assert_eq!(apply_gain(f32::INFINITY, floor), 0.0);
    }

    #[test]
    fn just_above_floor_is_not_silent() {
        let gain = db_to_gain(-59.9);
        assert_close(apply_gain(1.0, gain), gain);
    }

    #[test]
    fn floor_and_below_display_as_minus_inf() {
        assert_eq!(format_gain(db_to_gain(MIN_GAIN_DB)), "-inf");
        assert_eq!(format_gain(0.0), "-inf");
    }

    #[test]
    fn displays_db_to_two_decimal_places() {
        assert_eq!(format_gain(db_to_gain(0.0)), "0.00");
        assert_eq!(format_gain(db_to_gain(12.0)), "12.00");
        assert_eq!(format_gain(db_to_gain(-12.0)), "-12.00");
        assert_eq!(format_gain(db_to_gain(-59.9)), "-59.90");
    }

    #[test]
    fn just_below_unity_displays_as_zero_not_negative_zero() {
        assert_eq!(format_gain(db_to_gain(-0.001)), "0.00");
    }
}
```

Notes:

- `db_to_gain` lives in the tests module only. Nothing in the plugin needs
  it, and an unused `pub fn` would fail `clippy -D warnings`.
- If `clippy` flags a float literal as over-precise, trim a digit. Keep the
  1e-5 relative tolerance.

#### 2. Glue in the plugin

**File**: `corral/src/lib.rs`
**Changes**:

- Add `mod dsp;`.
- Remove `MIN_GAIN_DB` and `silence_threshold()`. Keep `MAX_GAIN_DB` (it's a
  parameter-range setting, not DSP).
- The range becomes `FloatRange::gain_range(dsp::MIN_GAIN_DB, MAX_GAIN_DB)`.
- The formatter becomes `.with_value_to_string(Arc::new(dsp::format_gain))`,
  dropping the `gain_to_db` local.
- `process()` becomes:

```rust
        for channel_samples in buffer.iter_samples() {
            // One smoothed value per frame, shared by both channels.
            let gain = self.params.gain.smoothed.next();

            for sample in channel_samples {
                *sample = dsp::apply_gain(*sample, gain);
            }
        }
```

- Keep the comment about logarithmic smoothing and the parameter-ID comment.
  Update the `Corral` doc comment if it still mentions the floor
  specifically.

#### 3. `test` task

**File**: `mise.toml`
**Changes**: add after `lint`, and extend `check`:

```toml
[tasks.test]
description = "Run the workspace unit tests"
run = "cargo test --workspace"

[tasks.check]
description = "Run fmt-check, lint, then test (the CI entry point)"
run = [{ task = "fmt-check" }, { task = "lint" }, { task = "test" }]
```

### Success Criteria:

#### Automated Verification:

- [x] Tests pass: `mise run test` (10 tests in `corral`, 0 failed)
- [x] `mise run check` exits 0
- [x] `dsp.rs` has no nice-plug import: `grep -n nice_plug corral/src/dsp.rs`
      prints nothing
- [x] `mise tasks` lists `test` with its description
- [x] IDs are intact: `grep -n '#\[id = "gain"\]' corral/src/lib.rs` and
      `grep -n 'JCowieCorral0001' corral/src/lib.rs` each match once
- [x] `mise run build-universal` succeeds

#### Manual Verification:

- [x] None in this phase. The tests pin today's gain and display behaviour.

---

## Phase 2: egui Editor

### Overview

Add nice-plug-egui, an editor module with a Gain slider and readout, and the
plugin wiring that shares the smoothed gain with the editor.

### Changes Required:

#### 1. Dependencies

**File**: `corral/Cargo.toml`

```toml
[dependencies]
nice-plug = "0.4"
nice-plug-egui = "0.5"
# nice-plug-egui turns off egui's default features, which include the fonts.
# Without them every label renders blank.
egui = { version = "0.36", features = ["default_fonts"] }
```

`Cargo.lock` gains nice-plug-egui 0.5.1, egui 0.36.x, egui-baseview 0.7.x
and baseview 0.3.x. Commit it.

#### 2. Editor module

**File**: `corral/src/editor.rs` (new)

```rust
//! The plugin window: a Gain slider and a readout of the gain being applied.

use std::sync::Arc;
use std::sync::atomic::Ordering;

use nice_plug::context::gui::GuiContext;
use nice_plug::editor::dpi::LogicalSize;
use nice_plug::prelude::*;
use nice_plug_egui::{EguiEditorState, NiceEguiApp, widgets};

use crate::{CorralParams, dsp};

/// Fixed and not saved with the set.
const WINDOW_SIZE: LogicalSize<f32> = LogicalSize::new(300.0, 120.0);

pub(crate) fn default_state() -> Arc<EguiEditorState> {
    EguiEditorState::from_size(WINDOW_SIZE, 1.0)
}

pub struct CorralEditor {
    params: Arc<CorralParams>,
    /// The smoothed gain the audio thread applied to its last frame.
    readout_gain: Arc<AtomicF32>,
    /// Set each time the window opens. nice-plug-egui 0.5.1 never calls
    /// `editor_closed`, so nothing clears it, and nothing needs to.
    gui_context: Option<GuiContext>,
}

impl CorralEditor {
    pub(crate) fn new(params: Arc<CorralParams>, readout_gain: Arc<AtomicF32>) -> Self {
        Self {
            params,
            readout_gain,
            gui_context: None,
        }
    }
}

impl NiceEguiApp for CorralEditor {
    fn build(
        &mut self,
        _egui_ctx: egui::Context,
        nice_gui_ctx: GuiContext,
        _frame: &mut nice_plug_egui::Frame,
    ) -> Result<(), nice_plug_egui::baseview::HandlerError> {
        self.gui_context = Some(nice_gui_ctx);
        Ok(())
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut nice_plug_egui::Frame) {
        let Some(gui_context) = &self.gui_context else {
            return;
        };
        let setter = gui_context.param_setter();
        let readout = dsp::format_gain(self.readout_gain.load(Ordering::Relaxed));

        // The root Ui has no background of its own.
        egui::CentralPanel::default().show(ui, |ui| {
            ui.heading("Corral");
            // Goes through the host's parameter setter, so edits are recorded as automation.
            ui.add(widgets::ParamSlider::for_param(&self.params.gain, &setter));
            ui.label(format!("Gain: {readout} dB"));
        });
    }
}
```

Notes:

- `CorralEditor` is `pub` inside a private module because it appears in
  `Plugin::Editor`. Its constructor is `pub(crate)` because it takes the
  crate-private `CorralParams`. If rustc still warns about a private type in
  a public interface, make `CorralParams` `pub(crate)`. Don't silence the
  lint.
- If `CentralPanel::show` doesn't accept a `&mut Ui` in the resolved egui
  version, use `show_inside`, or copy whatever
  `nice-plug-egui/src/resizable_window.rs` does.
- Imports are illustrative. Adjust them to whatever compiles cleanly under
  clippy.

#### 3. Plugin wiring

**File**: `corral/src/lib.rs`
**Changes**:

- Add `mod editor;` and imports:
  `use nice_plug_egui::{EguiEditor, EguiEditorState, EguiNiceSettings, RepaintNotifier, create_egui_editor};`
  and `use crate::editor::CorralEditor;`.
- Replace `#[derive(Default)]` on `Corral` with the fields and a hand-written
  `Default`:

```rust
pub struct Corral {
    params: Arc<CorralParams>,
    editor_state: Arc<EguiEditorState>,
    repaint_notifier: RepaintNotifier,
    /// The smoothed gain applied to the last frame, shown by the editor's readout.
    readout_gain: Arc<AtomicF32>,
    /// The last value stored in `readout_gain`, so the audio thread only
    /// stores and repaints when it changes.
    last_readout_gain: f32,
    /// Held until the host asks for the editor, which it does once.
    initial_editor: Option<CorralEditor>,
}

impl Default for Corral {
    fn default() -> Self {
        let params = Arc::new(CorralParams::default());
        let unity = util::db_to_gain(0.0);
        let readout_gain = Arc::new(AtomicF32::new(unity));

        Self {
            initial_editor: Some(CorralEditor::new(params.clone(), readout_gain.clone())),
            params,
            editor_state: editor::default_state(),
            repaint_notifier: RepaintNotifier::new(),
            readout_gain,
            last_readout_gain: unity,
        }
    }
}
```

- `type Editor = EguiEditor<CorralEditor>;`
- Add:

```rust
    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Self::Editor> {
        create_egui_editor(
            self.editor_state.clone(),
            self.repaint_notifier.clone(),
            EguiNiceSettings::new().with_tile(Self::NAME),
            self.initial_editor.take()?,
        )
    }
```

- Extend `process()` to publish the last frame's gain:

```rust
        let mut last_gain = None;

        for channel_samples in buffer.iter_samples() {
            // One smoothed value per frame, shared by both channels.
            let gain = self.params.gain.smoothed.next();

            for sample in channel_samples {
                *sample = dsp::apply_gain(*sample, gain);
            }
            last_gain = Some(gain);
        }

        // Share the gain with the readout once per buffer, only when it changes.
        // It's stored even while the window is closed, so the readout is right
        // when it opens. Both calls are single atomic stores.
        if let Some(gain) = last_gain
            && gain != self.last_readout_gain
        {
            self.last_readout_gain = gain;
            self.readout_gain.store(gain, Ordering::Relaxed);
            if self.editor_state.is_open() {
                self.repaint_notifier.request_repaint();
            }
        }
```

  (`Ordering` from `std::sync::atomic`.) An empty buffer leaves
  `last_gain` as `None` and publishes nothing.

### Success Criteria:

#### Automated Verification:

- [x] `mise run check` exits 0 (fmt, clippy `-D warnings`, tests)
- [x] `mise run build-universal` succeeds
- [x] Bundle is universal: `lipo -archs target/bundled/Corral.vst3/Contents/MacOS/Corral`
      prints `x86_64 arm64`
- [x] `grep -n 'default_fonts' corral/Cargo.toml` matches once
- [x] `grep -n 'nice_plug' corral/src/dsp.rs` still prints nothing
- [x] Nothing that allocates or locks in `process()`: reading it shows only
      `smoothed.next()`, `dsp::apply_gain`, one `AtomicF32::store`, one
      `is_open()` and one `request_repaint()`

#### Manual Verification:

- [x] None in this phase. The editor is checked in Live in Phase 4.

---

## Phase 3: Documentation

### Overview

Document the new task and reorganise the roadmap. The Phase 2 roadmap items
are ticked only after Phase 4 passes.

### Changes Required:

#### 1. README

**File**: `README.md`
**Changes**: in the tasks table,

- add after `lint`: `| test | Run the workspace unit tests |`;
- change the `check` row to
  `| check | Run fmt-check, lint, then test (the CI entry point) |`.

Descriptions must match `mise.toml` exactly.

#### 2. Roadmap

**File**: `ROADMAP.md`
**Changes**:

- Phase 2 keeps only "Unit tests for the DSP pieces, kept separate from the
  plugin glue code" and "Basic GUI (…)", both still unticked.
- Phase 3 gains, at the top, the offline test harness item and "Collect some
  test audio: …", moved verbatim from Phase 2.
- In Phase 3, "Test the detector against the test audio from Phase 2"
  becomes "Test the detector against the test audio".
- Phase 4 gains "Dry/wet mix parameter (the harmoniser will need it)",
  placed after "Mix the shifted signal with the dry signal".

### Success Criteria:

#### Automated Verification:

- [x] `grep -n '| `test` |' README.md` matches once
- [x] Phase 2 has two items: `sed -n '/## Phase 2/,/## Phase 3/p' ROADMAP.md | grep -c '^- \['`
      prints `2`
- [x] The harness and test audio are in Phase 3:
      `sed -n '/## Phase 3/,/## Phase 4/p' ROADMAP.md | grep -c 'hound\|Collect some test audio'`
      prints `2`
- [x] Dry/wet is in Phase 4:
      `sed -n '/## Phase 4/,/## Phase 5/p' ROADMAP.md | grep -c 'Dry/wet'` prints `1`

#### Manual Verification:

- [x] README and ROADMAP read correctly

---

## Phase 4: Manual Verification in Ableton Live 10

### Overview

Install the universal build and work through the work item's acceptance
criteria in Live 10, which loads the x86_64 slice under Rosetta. If the
editor fails (blank window, crash on open or close), record the symptom in
the work item and stop. The next step is decided then.

### Changes Required:

#### 1. Tick the roadmap

**File**: `ROADMAP.md`
**Changes**: once the checks below pass, tick both Phase 2 items.

### Success Criteria:

#### Automated Verification:

- [x] With Live closed, `mise run install` exits 0
- [x] Both Phase 2 items are ticked:
      `sed -n '/## Phase 2/,/## Phase 3/p' ROADMAP.md | grep -c '\[x\]'` prints `2`
- [x] `mise run check` exits 0

#### Manual Verification:

- [x] Option-click Rescan, put Corral on an audio track and open the plugin
      window: the editor shows the "Corral" heading, a Gain slider and a
      `Gain: 0.00 dB` readout. Text is visible, and the window isn't blank.
- [x] Dragging the slider to −12 dB:
  - the readout glides to `Gain: -12.00 dB`;
  - Live's own display of the Gain parameter matches;
  - the track meter peak drops about 12 dB, as in 0002.
- [x] With a Gain automation envelope playing and the window open, the
      slider and readout both follow it. Automation recorded from slider
      drags plays back.
- [x] At the slider's minimum, the slider shows "-inf dB", the readout shows
      `Gain: -inf dB`, and the track meter shows no signal.
- [x] Opening and closing the plugin window several times, then deleting the
      device while its window is open, doesn't crash Live.
- [x] A set saved with a non-default Gain value restores it after quitting
      and reopening Live.
- [x] Sweeping the slider over a sustained tone makes no clicks (no
      regression from 0002).

---

## Testing Strategy

### Unit Tests:

- `dsp::apply_gain`: unity, +12/−6/−12 dB scaling within 1e-5 relative, the
  floor and below giving exactly 0.0, non-finite input at the floor, and
  −59.9 dB not silent.
- `dsp::format_gain`: "-inf" at the floor and at 0.0, "0.00", "12.00",
  "-12.00", "-59.90", and "0.00" (not "-0.00") just below unity.
- `SILENCE_THRESHOLD` against the floor computed with nice-plug's formula.

### Integration Tests:

- None (`tests/` is out of scope). `build-universal`, `lipo` and
  `mise run install` cover the build and install pipeline.

### Manual Testing Steps:

1. Quit Live, run `mise run install`, open Live and Option-click Rescan.
2. Load Corral on an audio track with a loop playing and open its window.
3. Drag the slider to −12 dB, to the minimum, and back to 0 dB. Watch the
   readout, Live's parameter display and the track meter.
4. Record slider moves as automation in Arrangement view and play them back
   with the window open.
5. Close and reopen the window several times, then delete the device with
   its window open.
6. Save the set with a non-default Gain, quit, reopen and check the value.

## Performance Considerations

- `apply_gain` compares against a constant, so the per-sample cost is the
  same as before (a compare and a multiply).
- The readout adds one comparison per buffer and, when the gain changes,
  one atomic store and one atomic load. A repaint is requested at most once
  per buffer, and only while the window is open.
- egui only repaints on request or input, so an idle window costs little.

## Migration Notes

- The parameter ID `gain` and the class ID `JCowieCorral0001` are unchanged,
  so sets saved with 0002 still load.
- The editor's window size isn't saved with the set (accepted by the work
  item).
- Live caches plugin info, so Option-click Rescan after installing, so Live
  sees that Corral now has an editor.

## References

- Original work item: `meta/work/0003-tested-gain-dsp-and-basic-plugin-gui.md`
- Previous plan: `meta/plans/2026-09-25-0002-gain-plugin-loadable-in-ableton-live-10.md`
- Plugin source: `corral/src/lib.rs`
- Tasks: `mise.toml`
- nice-plug gain_egui example: https://codeberg.org/RustAudio/nice-plug/src/branch/main/examples/gain_egui/src/lib.rs
- nice-plug-egui 0.5.1: `src/lib.rs` (`create_egui_editor`, `NiceEguiApp`,
  `EguiEditorState`), `src/editor.rs` (`EguiNiceSettings`, `EguiEditor`),
  `src/resizable_window.rs:35`
- egui-baseview 0.7.2: `src/window.rs:24-53` (`RepaintNotifier`), `:591`
- nice-plug-core 0.4.2: `src/formatters.rs:85-101`, `src/util.rs:46-59`,
  `src/params/float.rs:143-147`, `src/plugin.rs:204-209`
