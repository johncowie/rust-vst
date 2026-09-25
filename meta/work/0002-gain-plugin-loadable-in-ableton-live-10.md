---
type: work-item
id: "0002"
title: "Gain Plugin Loadable in Ableton Live 10"
date: "2026-09-25T14:16:29+00:00"
author: John Cowie Del Corral
producer: create-work-item
status: draft
kind: story
priority: high
blocked_by: ["work-item:0001"]
tags: [phase-1, dsp, parameters, install, ableton]
last_updated: "2026-09-25T14:16:29+00:00"
last_updated_by: John Cowie Del Corral
schema_version: 1
---

# 0002: Gain Plugin Loadable in Ableton Live 10

**Kind**: Story
**Status**: Draft
**Priority**: High
**Author**: John Cowie Del Corral

## Summary

As the plugin developer, I want corral to apply a smoothed, automatable gain
and to install into Ableton Live 10 with one command, so that I can prove
audio flows through our code and set up a quick build, install and listen
loop for every later phase.

This is Phase 1 of `ROADMAP.md`.

## Context

- Phase 0 (0001) produced a `corral` crate that bundles as VST3 and CLAP.
  - Its `process` leaves the buffer untouched, so it already passes audio
    through unchanged.
  - It has no parameters, and nothing yet installs it or loads it in a DAW.
- **The test machine** is an Apple Silicon Mac (arm64) on macOS 14.6.1, with
  Ableton Live 10 Standard.
- **Live 10 is Intel-only** and runs under Rosetta 2 (officially from 10.1.35).
  - It can only load plugins that contain an x86_64 slice.
  - The first native Apple Silicon Live was 11.1.
  - So an arm64-only (host-arch) build will not show up in Live. Installing
    must use the universal build (`mise run build-universal`).
- **VST3 support** arrived in Live 10.1.
  - Live finds VST3s in the system folders when Preferences → Plug-ins →
    "Use VST3 Plug-in System Folders" is on.
  - Live 10.1 auto-assigns parameters for VST3s with fewer than 65
    parameters, so Gain can be automated without configuration.
- **Rebuilds and rescans**: Live doesn't detect plugins added or changed while
  it's running.
  - A rebuilt plugin needs Live to be quit and relaunched.
  - Option-click Rescan deletes the plugin database and does a clean scan.
- **Replacing a signed binary in place** (copying over it) can get the loading
  process killed with "Code Signature Invalid", because the kernel caches the
  old signature. Remove the old file, then copy the new one in.
- **Signing**: nice-plug's xtask already ad-hoc signs bundles (`codesign -f -s -`).
  - It only warns if `codesign` can't be *spawned*. It ignores the exit
    code, so a failed signature goes unnoticed.
  - Locally built bundles don't get the `com.apple.quarantine` attribute.
- **Smoothing near zero**: nice-plug's `SmoothingStyle::Logarithmic` can't
  smooth to or from 0.0. Its docs warn that value ranges must not include 0.

## Requirements

1. **Gain parameter** on `CorralParams`:
   - A `FloatParam` named "Gain" with the stable ID `#[id = "gain"]`.
   - Range: `FloatRange::gain_range(-60.0, 12.0)` (skewed for resolution
     near 0 dB).
   - Default: `util::db_to_gain(0.0)`.
   - Unit " dB", with gain-to-dB value/string formatters.
   - The minimum displays as "-inf".
2. **Smoothing**: `SmoothingStyle::Logarithmic(50.0)`.
3. **Processing**:
   - For each sample frame, take one `smoothed.next()` value and apply it to
     both channels.
   - When the smoothed gain is at or below the −60 dB minimum, write 0.0
     (true silence) instead of multiplying.
4. **Channel layout** stays stereo in / stereo out, unchanged.
5. **`mise run install` task**, with a description. It:
   1. Exits non-zero with a "quit Ableton Live first" message if Live is
      running, and changes nothing.
   2. Builds the universal bundle (runs the `build-universal` task).
   3. Ad-hoc signs `target/bundled/Corral.vst3`
      (`codesign --force --deep -s -`).
   4. Checks the signature with `codesign --verify`, failing on error.
   5. Removes any existing `~/Library/Audio/Plug-Ins/VST3/Corral.vst3`, then
      copies the new bundle in (creating the folder if it's missing).
6. **README**: add `install` to the tasks table and a short "Loading in
   Ableton Live 10" section:
   - Turn on "Use VST3 Plug-in System Folders".
   - Quit Live before installing.
   - Option-click Rescan if the plugin's name or parameters change.
   - Live 10 runs under Rosetta and needs the x86_64 slice.
7. **ROADMAP**: tick the Phase 1 items as they're verified, including
   pass-through (done by the Phase 0 scaffold). Note that installing uses the
   universal build because Live 10 runs under Rosetta.

## Acceptance Criteria

- [ ] Given Ableton Live is closed, when I run `mise run install`, then:
      - `~/Library/Audio/Plug-Ins/VST3/Corral.vst3` exists;
      - `lipo -archs` on its binary reports both `x86_64` and `arm64`;
      - `codesign --verify` on the installed bundle exits 0.
- [ ] Given Ableton Live is running, when I run `mise run install`, then it
      exits non-zero with a message telling me to quit Live, and the installed
      bundle's modification time is unchanged.
- [ ] Given a fresh install and "Use VST3 Plug-in System Folders" enabled,
      when I launch Live 10 and rescan, then Corral appears under Plug-ins and
      can be dropped onto an audio track.
- [ ] Given Corral on an audio track at 0 dB (the default), when audio plays,
      then toggling the device on and off makes no audible or metered
      difference.
- [ ] Given audio is playing, when I set Gain to −12 dB, then the track meter
      peak drops by about 12 dB. When I set it to +12 dB, it rises by about
      12 dB compared with 0 dB.
- [ ] Given Gain is at its minimum, then the parameter displays "-inf" and
      the track meter shows no signal.
- [ ] Given a sustained tone, when I sweep Gain quickly across its range by
      hand, then there are no audible clicks or zipper noise.
- [ ] Given an automation envelope drawn for Gain in Arrangement view, when
      I play back, then the level follows the envelope without clicks.
- [ ] Given a set with a non-default Gain value and Gain automation, when I
      save it, quit Live, and reopen the set, then the value and automation
      are restored.
- [ ] `mise run check` exits 0, and `mise tasks` lists `install` with a
      description.

## Open Questions

- Does Live 10 need "Use VST3 Plug-in System Folders" enabled to see the user
  folder `~/Library/Audio/Plug-Ins/VST3`? Research suggests yes but it isn't
  confirmed. Check during implementation and correct the README if it's
  wrong.

## Dependencies

- Blocked by: 0001 (Phase 0 project setup)
- Blocks: Phase 2 (development foundations)

## Assumptions

- Live 10 runs well enough on macOS 14.6.1 to verify the plugin, even though
  it's officially unsupported after Big Sur. If it doesn't, moving to Live
  11.1+ is a separate decision.
- Install always uses the universal build, even though that makes each
  install slower (two targets). This fits Live 10 under Rosetta and still
  works in a native host after an upgrade.

## Technical Notes

- Follow nice-plug's `examples/gain` pattern: iterate `buffer.iter_samples()`
  and call `self.params.gain.smoothed.next()` once per frame.
- `util::db_to_gain(-60.0)` is 0.001. Compare the smoothed value against
  the range minimum (with a small tolerance) to decide when to output silence.
  Use a custom value-to-string formatter, or wrap `v2s_f32_gain_to_db`, so the
  minimum shows "-inf". The stock formatter only does this below −100 dB.
- Detect whether Live is running with `pgrep -f "Ableton Live"` or similar.
  Confirm the actual process name for Live 10 Standard.
- `--deep` is unnecessary for a single-binary `.vst3` but harmless. It's kept
  to match the roadmap.
- The VST3 class ID (`JCowieCorral0001`) and the parameter ID `gain` must
  never change, or saved sets and automation break.

## Drafting Notes

- "−∞ at the bottom" is done by treating the −60 dB minimum as silence rather
  than extending the range to 0. This keeps the roadmap's Logarithmic
  smoothing, which can't cross 0. The step from −60 dB to silence is
  inaudible.
- The install task signs again even though the xtask already signs, because
  the xtask ignores codesign's exit code. It also fulfils the roadmap's
  explicit codesign item.
- The install task removes the old bundle before copying rather than copying
  over it, to avoid the macOS signature-cache crash.
- The roadmap's separate build, sign and install steps are merged into one
  `install` task.
- Stereo-only kept, as it's the easiest option ("whichever is easiest") and
  Live audio tracks are stereo.
- CLAP install, mono layout, GUI, automated tests (Phase 2), a custom bundle
  identifier, and other Ableton versions are out of scope.
- Verification is manual in Live 10, as agreed with the developer.

## References

- Source: `ROADMAP.md` (Phase 1)
- Related: 0001 (Phase 0 project setup)
- Live 10 release notes: https://www.ableton.com/en/release-notes/live-10/
- Live 11.1 Apple Silicon: https://www.ableton.com/en/blog/free-live-11-1-update-apple-silicon-support-out-now/
- Live manual, using plug-ins: https://www.ableton.com/en/manual/using-plug-ins/
- Apple forums, replacing signed binaries: https://developer.apple.com/forums/thread/130313
- nice-plug gain example: https://codeberg.org/RustAudio/nice-plug/raw/branch/main/examples/gain/src/lib.rs
- nice-plug SmoothingStyle: https://docs.rs/nice-plug/latest/nice_plug/params/smoothing/enum.SmoothingStyle.html
- nice-plug-xtask source: https://docs.rs/crate/nice-plug-xtask/latest/source/src/lib.rs
