---
type: plan
id: "2026-09-25-0002-gain-plugin-loadable-in-ableton-live-10"
title: "Gain Plugin Loadable in Ableton Live 10 Implementation Plan"
date: "2026-09-25T14:23:05+00:00"
author: John Cowie Del Corral
producer: create-plan
status: draft
work_item_id: "work-item:0002"
parent: "work-item:0002"
tags: [phase-1, dsp, parameters, install, ableton]
revision: "f874ef81cbb181095e7c2fd831798cdb0c7b4275"
repository: "rust-vst"
last_updated: "2026-09-25T14:23:05+00:00"
last_updated_by: John Cowie Del Corral
schema_version: 1
---

# Gain Plugin Loadable in Ableton Live 10 Implementation Plan

## Overview

This plan completes Phase 1 of `ROADMAP.md`. It delivers:

- a smoothed, automatable `Gain` parameter on `corral` (−60 dB to +12 dB,
  where the minimum shows "-inf" and outputs true silence);
- a `mise run install` task that refuses to run while Live is open. Otherwise
  it builds the universal bundle, ad-hoc signs and verifies it, and replaces
  `~/Library/Audio/Plug-Ins/VST3/Corral.vst3`;
- README and ROADMAP updates;
- manual verification in Ableton Live 10 Standard (under Rosetta) on the
  Apple Silicon test machine.

## Current State Analysis

- `corral/src/lib.rs:10-11`: `CorralParams` is an empty `#[derive(Params, Default)]`
  struct.
- `corral/src/lib.rs:39-46`: `process` leaves the buffer untouched, so the
  plugin already passes audio through.
- `corral/src/lib.rs:21-25`: layout is stereo in / stereo out, and stays that way.
- `corral/src/lib.rs:59`: the VST3 class ID is `*b"JCowieCorral0001"`. It must
  never change.
- `mise.toml:8-10`: `build-universal` runs `cargo xtask bundle-universal corral --release`
  and writes `target/bundled/Corral.vst3` (currently reports `x86_64 arm64`
  from `lipo -archs`).
- `mise.toml:28-30`: `check` is `run = [{ task = "fmt-check" }, { task = "lint" }]`.
- There is no install task. Nothing is installed yet.
- `README.md:43-54`: tasks table. There is no Ableton section yet.
- `ROADMAP.md:24-35`: the Phase 1 items are all unticked.
- Ableton Live 10 Standard is installed at
  `/Applications/Ableton Live 10 Standard.app`. Its executable is
  `Contents/MacOS/Live`.

## Desired End State

- Corral has one parameter, "Gain" (`#[id = "gain"]`), that Live can
  automate. It is smoothed logarithmically over 50 ms and shows "-inf dB" at
  its minimum, where the output is exactly 0.0.
- `mise run install`, with Live closed, leaves a universal, validly signed
  `~/Library/Audio/Plug-Ins/VST3/Corral.vst3`. With Live running, it exits
  non-zero, tells you to quit Live, and changes nothing.
- The README documents `install` and how to load the plugin in Live 10.
- Every Phase 1 item in ROADMAP is ticked.
- Every acceptance criterion in work item 0002 has been checked.

### Key Discoveries:

- **The smoother lands exactly on its target.**
  - On the last step, `Smoother::next` returns `target_f32` itself
    (`nice-plug-core-0.4.2/src/params/smoothing.rs:303-305`).
  - A `FloatRange::Skewed` turns normalized 0 back into exactly `min`
    (`params/range.rs:64-70`).
  - So a threshold of "the minimum gain plus a tiny tolerance" reliably
    detects the floor.
- **The stock formatter only prints "-inf" below −100 dB.**
  `formatters::v2s_f32_gain_to_db` prints "-inf" only below
  `util::MINUS_INFINITY_GAIN` (1e-5, −100 dB) (`formatters.rs:88`). So we
  wrap it.
- **Typing "-inf" already works.** `formatters::s2v_f32_gain_to_db` parses
  "-inf" to 0.0 (`formatters.rs:105-115`), which the range clamps to the
  −60 dB minimum. No custom parser is needed.
- **`pgrep -f "Ableton Live"` would match the task's own shell.**
  - mise runs a task script as `sh -c <script>`, so the pattern appears in
    the parent shell's arguments.
  - `pgrep -xq Live` matches the exact process name instead and avoids this.
- **mise can mix inline scripts and task references in one `run` array.**
  - Tested on mise 2026.3.12:
    `run = ['<script>', { task = "a" }, '<script>']` runs the steps in order.
  - A non-zero exit stops the task and returns that exit code.
  - So the Live check can run *before* the build. `depends` would build first.
- **The xtask's signature can't be trusted.** It ignores `codesign`'s exit
  code, so the install task signs again and verifies.
- **Copying over a signed binary in place risks "Code Signature Invalid".**
  The old bundle is `rm -rf`'d before `cp -R`.

## What We're NOT Doing

- CLAP installation, a mono layout, or a GUI.
- Automated tests or an offline harness (Phase 2).
- A custom bundle identifier, notarisation, or Developer ID signing.
- Supporting or testing other Ableton versions or other DAWs.
- An `uninstall` task.
- Extending the gain range down to 0.0 (the −60 dB floor is treated as
  silence instead).

## Implementation Approach

1. Change the plugin code first and check that it builds and lints.
2. Add the install task.
3. Update the docs.
4. Verify by hand in Live 10.

The roadmap items that need Live are ticked only after phase 4 passes.

## Phase 1: Gain Parameter and Processing

### Overview

Add the `Gain` parameter to `CorralParams` and apply it per sample frame in
`process`.

### Changes Required:

#### 1. Plugin crate

**File**: `corral/src/lib.rs`
**Changes**:

- Replace the derived `Default` on `CorralParams` with a hand-written one
  that builds the gain parameter.
- Add a `silence_threshold` helper that the formatter and `process` both use.
- Rewrite `process` to follow nice-plug's `examples/gain` pattern.
- Update the struct doc comment.

```rust
use std::sync::Arc;

use nice_plug::prelude::*;

const MIN_GAIN_DB: f32 = -60.0;
const MAX_GAIN_DB: f32 = 12.0;

/// Stereo gain. The −60 dB floor is treated as silence ("-inf").
#[derive(Default)]
pub struct Corral {
    params: Arc<CorralParams>,
}

#[derive(Params)]
struct CorralParams {
    /// Linear gain. The ID is stored in saved sets and automation, so it must never change.
    #[id = "gain"]
    gain: FloatParam,
}

/// Gains at or below this are the floor and output silence. The smoother
/// lands exactly on its target, so the tolerance only absorbs float error.
fn silence_threshold() -> f32 {
    util::db_to_gain(MIN_GAIN_DB) * 1.0001
}

impl Default for CorralParams {
    fn default() -> Self {
        let gain_to_db = formatters::v2s_f32_gain_to_db(2);

        Self {
            gain: FloatParam::new(
                "Gain",
                util::db_to_gain(0.0),
                FloatRange::gain_range(MIN_GAIN_DB, MAX_GAIN_DB),
            )
            // Logarithmic smoothing can't reach 0.0, which is why the range stops at −60 dB.
            .with_smoother(SmoothingStyle::Logarithmic(50.0))
            .with_unit(" dB")
            .with_value_to_string(Arc::new(move |gain| {
                if gain <= silence_threshold() {
                    String::from("-inf")
                } else {
                    gain_to_db(gain)
                }
            }))
            .with_string_to_value(formatters::s2v_f32_gain_to_db()),
        }
    }
}
```

And the new `process` body:

```rust
    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        _context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        let silence_threshold = silence_threshold();

        for channel_samples in buffer.iter_samples() {
            // One smoothed value per frame, shared by both channels.
            let gain = self.params.gain.smoothed.next();
            let silent = gain <= silence_threshold;

            for sample in channel_samples {
                *sample = if silent { 0.0 } else { *sample * gain };
            }
        }

        ProcessStatus::Normal
    }
```

Notes:

- Writing `0.0` rather than multiplying keeps the floor truly silent even for
  non-finite input.
- `util::db_to_gain` is not `const`, so `process` computes the threshold once
  per block rather than once per sample.
- If clippy objects to anything, fix it without changing behaviour or the
  parameter ID.

### Success Criteria:

#### Automated Verification:

- [x] Formatting and lints pass: `mise run check`
- [x] Universal bundle builds: `mise run build-universal`
- [x] Bundle is universal: `lipo -archs target/bundled/Corral.vst3/Contents/MacOS/Corral`
      prints `x86_64 arm64`
- [x] The parameter ID and class ID are intact: `grep -n '#\[id = "gain"\]' corral/src/lib.rs`
      and `grep -n 'JCowieCorral0001' corral/src/lib.rs` each match once

#### Manual Verification:

- [x] None in this phase. Behaviour is checked in Live in Phase 4.

---

## Phase 2: `install` mise Task

### Overview

Add one task that guards against Live running, builds, signs, verifies and
installs.

### Changes Required:

#### 1. Task definition

**File**: `mise.toml`
**Changes**: Add after `build-universal`:

```toml
[tasks.install]
description = "Build the universal VST3, ad-hoc sign it, and install it to ~/Library/Audio/Plug-Ins/VST3 (quit Ableton Live first)"
run = [
  # Exact process-name match. `pgrep -f` would match this script's own `sh -c`.
  '''
if pgrep -xq Live; then
  echo "Ableton Live is running. Quit Ableton Live first, then run 'mise run install' again." >&2
  exit 1
fi
''',
  { task = "build-universal" },
  '''
set -eu
bundle=target/bundled/Corral.vst3
dest="$HOME/Library/Audio/Plug-Ins/VST3"

# The xtask signs too, but ignores codesign's exit code.
codesign --force --deep -s - "$bundle"
codesign --verify --verbose "$bundle"

mkdir -p "$dest"
# Remove, then copy: overwriting a signed binary in place can get the host
# killed with "Code Signature Invalid".
rm -rf "$dest/Corral.vst3"
cp -R "$bundle" "$dest/"
echo "Installed $dest/Corral.vst3"
''',
]
```

Notes:

- mise runs tasks from the config root, so the relative `target/bundled`
  path is correct from any subdirectory.
- If `set -eu` or the mixed array doesn't behave on the pinned mise version,
  fall back to separate hidden helper tasks (e.g. `install:check-live`,
  `install:copy`) sequenced with `{ task = ... }`. Keep the same step order.

### Success Criteria:

#### Automated Verification (with Live closed):

- [x] `mise tasks` lists `install` with its description
- [x] `mise run install` exits 0
- [x] Installed bundle exists: `test -d ~/Library/Audio/Plug-Ins/VST3/Corral.vst3`
- [x] Installed binary is universal: `lipo -archs ~/Library/Audio/Plug-Ins/VST3/Corral.vst3/Contents/MacOS/Corral`
      prints `x86_64 arm64`
- [x] Installed signature is valid: `codesign --verify --verbose ~/Library/Audio/Plug-Ins/VST3/Corral.vst3`
      exits 0
- [x] A second `mise run install` also succeeds (the replace path works)
- [x] `mise run check` still exits 0

#### Manual Verification:

- [ ] With Live open, `pgrep -xl Live` shows the Live process, confirming the
      process name under Rosetta
- [ ] With Live open:
  - record `stat -f %m ~/Library/Audio/Plug-Ins/VST3/Corral.vst3`;
  - run `mise run install`, which exits non-zero with the "Quit Ableton Live
    first" message and does not start a build;
  - run `stat` again and confirm the mtime is unchanged.

---

## Phase 3: Documentation

### Overview

Document the new task and how to load the plugin in Live 10. Tick the
roadmap items that are already proven.

### Changes Required:

#### 1. README

**File**: `README.md`
**Changes**:

- Add a row to the tasks table after `build-universal`, with the same
  description as in `mise.toml`:
  `| install | Build the universal VST3, ad-hoc sign it, and install it to ~/Library/Audio/Plug-Ins/VST3 (quit Ableton Live first) |`
- Add a section after the paragraph about bundle paths:

```markdown
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
```

- Correct step 2 during Phase 4 if Live turns out to find
  `~/Library/Audio/Plug-Ins/VST3` without the "System Folders" setting.

#### 2. Roadmap

**File**: `ROADMAP.md`
**Changes** (Phase 1 section):

- Tick "Plugin that copies input to output untouched" (done by the Phase 0
  scaffold).
- Tick "Add a `Gain` parameter…" and "Smooth the parameter…".
- Tick "Ad-hoc codesign the bundle…" and "Add a mise task that builds,
  signs, and installs in one step".
- Tick "Build the VST3 bundle and copy it…" and append: "(`mise run install`
  uses the universal build, because Live 10 runs under Rosetta and needs the
  x86_64 slice)".
- Leave the two Live-dependent items ("Rescan plugins in Ableton…" and
  "Check that the parameter can be automated…") unticked until Phase 4.

### Success Criteria:

#### Automated Verification:

- [x] README tasks table has an `install` row: `grep -n '| `install` |' README.md`
- [x] README has the section: `grep -n '## Loading in Ableton Live 10' README.md`
- [x] Six of the eight Phase 1 roadmap items are ticked:
      `sed -n '/## Phase 1/,/## Phase 2/p' ROADMAP.md | grep -c '\[x\]'` prints `6`

#### Manual Verification:

- [ ] README instructions read correctly and match the task's behaviour

---

## Phase 4: Manual Verification in Ableton Live 10

### Overview

Work through the work item's acceptance criteria in Live, answer the open
question about the VST3 folder, and tick the remaining roadmap items.

### Changes Required:

#### 1. Resolve the VST3 folder question

- Turn "Use VST3 Plug-in System Folders" off, then Option-click Rescan. Note
  whether Corral still appears. Then turn it back on and rescan.
- If Corral appears with the setting off, change README step 2 to say the
  setting isn't needed for the user folder. Otherwise leave it as written.

#### 2. Tick the remaining roadmap items

**File**: `ROADMAP.md`
**Changes**: Once the checks below pass, tick "Rescan plugins in Ableton…"
and "Check that the parameter can be automated from Ableton".

### Success Criteria:

#### Automated Verification:

- [ ] All eight Phase 1 roadmap items are ticked:
      `sed -n '/## Phase 1/,/## Phase 2/p' ROADMAP.md | grep -c '\[x\]'` prints `8`
- [ ] `mise run check` exits 0

#### Manual Verification:

- [ ] After a fresh install and rescan, Corral appears under Plug-ins and
      drops onto an audio track
- [ ] At 0 dB (the default), toggling the device on and off makes no audible
      or metered difference
- [ ] Gain at −12 dB lowers the track meter peak by about 12 dB. At +12 dB
      it raises it by about 12 dB, compared with 0 dB
- [ ] At its minimum, Gain displays "-inf dB" and the track meter shows no
      signal
- [ ] Sweeping Gain quickly by hand over a sustained tone makes no clicks or
      zipper noise
- [ ] A drawn automation envelope for Gain in Arrangement view is followed
      without clicks
- [ ] A set with a non-default Gain value and Gain automation keeps both
      after you save, quit Live and reopen the set
- [ ] The README's "System Folders" step matches what was observed

---

## Testing Strategy

### Unit Tests:

- None. Automated tests and the offline harness are Phase 2 (work item
  scope).

### Integration Tests:

- The install task's automated checks in Phase 2 (`lipo`,
  `codesign --verify`, and running the install twice) act as the integration
  test for the build → sign → install pipeline.

### Manual Testing Steps:

1. Quit Live and run `mise run install`. Confirm the files, archs and
   signature.
2. Open Live, run `mise run install`, and confirm it refuses and the mtime
   is unchanged.
3. Quit Live, reopen it, Option-click Rescan, and load Corral onto an audio
   track with a loop playing.
4. Toggle the device at 0 dB, then set −12 dB, +12 dB and the minimum,
   watching the track meter.
5. Hold a sustained tone (e.g. Operator on a MIDI track routed to the audio
   track, or a sine sample) and sweep Gain fast.
6. Draw a Gain automation envelope in Arrangement view and play it back.
7. Save the set, quit, reopen, and check the value and automation.
8. Test the "System Folders" setting off and on (see Phase 4, item 1).

## Performance Considerations

- The per-sample work is one smoother step per frame and one multiply per
  sample. This is negligible.
- The silence threshold is computed once per block.
- Universal builds compile two targets, which makes each install slower. The
  work item accepts this.

## Migration Notes

- This is the first install. There is nothing to migrate.
- The parameter ID `gain` and the VST3 class ID `JCowieCorral0001` are fixed
  from here on. Changing either breaks saved sets and automation.

## References

- Original work item: `meta/work/0002-gain-plugin-loadable-in-ableton-live-10.md`
- Previous plan: `meta/plans/2026-09-25-0001-project-setup-with-mise-and-nice-plug.md`
- Plugin source: `corral/src/lib.rs`
- Tasks: `mise.toml`
- nice-plug gain example: https://codeberg.org/RustAudio/nice-plug/raw/branch/main/examples/gain/src/lib.rs
- nice-plug-core 0.4.2: `src/formatters.rs:85-115`, `src/params/range.rs:64-70`,
  `src/params/smoothing.rs:287-311`
