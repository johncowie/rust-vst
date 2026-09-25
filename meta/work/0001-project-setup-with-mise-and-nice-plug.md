---
type: work-item
id: "0001"
title: "Project Setup with mise and nice-plug"
date: "2026-09-25T13:45:00+00:00"
author: John Cowie Del Corral
producer: create-work-item
status: draft
kind: task
priority: high
tags: [setup, tooling, phase-0]
last_updated: "2026-09-25T13:45:00+00:00"
last_updated_by: John Cowie Del Corral
schema_version: 1
---

# 0001: Project Setup with mise and nice-plug

**Kind**: Task
**Status**: Draft
**Priority**: High
**Author**: John Cowie Del Corral

## Summary

Complete Phase 0 of the roadmap: get a reproducible toolchain, a scaffolded
`corral` plugin crate that bundles as VST3 and CLAP, and one-command build and
lint tasks. Every later phase depends on this.

## Context

The project is an auto-harmoniser audio plugin (see `ROADMAP.md`). Phase 0 has
`git init` and `.gitignore` done; toolchain, framework choice, scaffolding and
the licensing note remain.

- The developer's shell exports `RUSTUP_TOOLCHAIN=1.79.0` globally (active
  toolchain 1.79.0, only `aarch64-apple-darwin` installed). `RUSTUP_TOOLCHAIN`
  takes precedence over `rust-toolchain.toml`, so the project needs a tool that
  sets it explicitly. mise does this: it delegates to rustup and sets
  `RUSTUP_TOOLCHAIN` in its own environment.
- Current stable Rust is 1.98.1 (2026-09-03).
- nih-plug, which the roadmap suggested, is in maintenance mode. Its README
  points to the community fork **nice-plug** (codeberg.org/RustAudio/nice-plug),
  which is maintained, published on crates.io (`nice-plug` 0.4.x, edition
  2024, MSRV 1.88), ISC-licensed, and uses the MIT/Apache `vst3` crate rather
  than the GPLv3 `vst3-sys`. It describes itself as experimental.
- Use is personal only for now.

## Requirements

1. **Toolchain via mise**: add a `mise.toml` pinning `rust = "1.98.1"` with
   components `clippy` and `rustfmt` and targets `aarch64-apple-darwin` and
   `x86_64-apple-darwin`.
2. **Override the global pin**: inside the project (under `mise activate`,
   `mise exec`, or `mise run`), the toolchain must be the mise-pinned version
   despite the shell's `RUSTUP_TOOLCHAIN=1.79.0`.
3. **Cargo workspace** containing:
   - `corral`: a `cdylib` plugin crate depending on `nice-plug = "0.4"`, as a
     minimal stereo pass-through plugin exporting both VST3 and CLAP.
   - `xtask`: the nice-plug bundler xtask, with the
     `.cargo/config.toml` alias `xtask = "run --package xtask --release --"`.
4. **Universal build**: confirm whether nice-plug's bundler supports a
   universal (arm64 + x86_64) macOS bundle. If not, build both targets and
   merge them with `lipo`.
5. **mise tasks**, each with a `description`:
   - `build`: `cargo xtask bundle corral --release`
   - `build-universal`: universal macOS bundle (per requirement 4)
   - `fmt`: `cargo fmt --all`
   - `fmt-check`: `cargo fmt --all --check`
   - `lint`: `cargo clippy --workspace --all-targets -- -D warnings`
   - `check`: runs `fmt-check` and `lint`
6. **Lint configuration**:
   - `[workspace.lints]` in the root `Cargo.toml` with `[lints] workspace = true`
     in each member (standard clippy warnings, no pedantic group).
   - A `rustfmt.toml` setting only `style_edition = "2024"`.
7. **README** with setup instructions (`mise install`, the tasks), plus a
   licensing note: nice-plug is ISC and its VST3 bindings are MIT/Apache, so
   there's no GPLv3 constraint (unlike upstream nih-plug).
8. **Roadmap update**: replace nih-plug references with nice-plug, reword the
   licensing item, and tick the completed Phase 0 items.

## Acceptance Criteria

- [ ] Given a fresh clone with mise installed, when I run `mise install`, then
      Rust 1.98.1 is installed with `clippy`, `rustfmt`, and both
      `aarch64-apple-darwin` and `x86_64-apple-darwin` targets.
- [ ] Given the shell exports `RUSTUP_TOOLCHAIN=1.79.0`, when I run
      `mise exec -- cargo --version` in the repo, then it reports 1.98.1.
- [ ] Given the shell exports `RUSTUP_TOOLCHAIN=1.79.0` and runs
      `mise activate`, when I `cd` into the repo and run plain `cargo --version`,
      then it reports 1.98.1; and after `cd`-ing out, it reports 1.79.0 again.
- [ ] Given the toolchain is installed, when I run `mise run build`, then
      `target/bundled/` contains a `.vst3` and a `.clap` bundle for corral.
- [ ] Given the toolchain is installed, when I run `mise run build-universal`,
      then `lipo -archs` on the bundled binary reports both `x86_64` and
      `arm64`.
- [ ] Given the scaffolded code, when I run `mise run check`, then it exits 0.
- [ ] Given a deliberately introduced clippy violation (e.g. an unnecessary
      `mut`), when I run `mise run lint`, then it exits non-zero.
- [ ] Given a deliberately mis-formatted file, when I run `mise run fmt-check`,
      then it exits non-zero; after `mise run fmt` it exits 0.
- [ ] `mise tasks` lists all six tasks, each with a description.
- [ ] The README contains setup instructions and the licensing note, and
      `ROADMAP.md` shows all Phase 0 items ticked with nice-plug named as the
      framework.

## Open Questions

- Does nice-plug's xtask support `bundle-universal` and the `bundler.toml`
  format the same way nih-plug's did? (Resolved during implementation;
  requirement 4 covers both outcomes.)

## Dependencies

- Blocked by: none
- Blocks: Phase 1 (minimal pass-through plugin with gain control)

## Assumptions

- The developer's global `RUSTUP_TOOLCHAIN=1.79.0` export stays in place, and
  the project relies on mise overriding it. Tools that run outside mise's
  environment (e.g. an IDE's rust-analyzer without mise integration) may still
  see 1.79.0. Removing the global export would be more robust.
- The universal (arm64 + x86_64) build belongs in Phase 0 because the roadmap
  lists both macOS targets there, even though Phase 6 mentions it as a
  refinement.
- Both VST3 and CLAP are exported. Ableton is the primary host (VST3), and CLAP
  costs nothing extra.

## Technical Notes

- mise Rust config: `rust = { version = "1.98.1", components = ["clippy", "rustfmt"], targets = ["aarch64-apple-darwin", "x86_64-apple-darwin"] }`.
- Check the override with `mise env | grep RUSTUP_TOOLCHAIN`.
- Bundles are written to `target/bundled/<Name>.vst3` and `<Name>.clap`.
- nice-plug's getting-started guide suggests `opt-level = 1` for dev builds and
  `lto = "thin"` with `codegen-units = 1` for release. Worth adopting now.
- Codesigning, installing to `~/Library/Audio/Plug-Ins/`, and loading in
  Ableton are Phase 1, not this task.

## Drafting Notes

- Framework switched from the roadmap's nih-plug to nice-plug after research
  showed nih-plug is in maintenance mode. The developer approved this.
- Rust pinned to an exact version (1.98.1) rather than `"stable"` so builds are
  reproducible and upgrades are deliberate.
- The crate is scaffolded by hand rather than with the cookiecutter template,
  to avoid a Python dependency for one-off setup.
- Lint and format settings are kept minimal (default rustfmt, standard clippy
  with warnings as errors). Stricter groups such as `clippy::pedantic` can be
  added later.
- CI is out of scope. The `check` task is written so CI can call it later.
- "Done" for Phase 0 means it compiles and bundles; loading in a DAW is Phase 1.
- A `mise activate` acceptance criterion was added because that is the normal
  interactive workflow. `mise exec` alone would not prove that plain `cargo` in
  the shell uses the right toolchain.

## References

- Source: `ROADMAP.md` (Phase 0)
- nice-plug: https://codeberg.org/RustAudio/nice-plug
- nih-plug (maintenance mode notice): https://github.com/robbert-vdh/nih-plug
- mise Rust docs: https://mise.jdx.dev/lang/rust.html
- mise TOML tasks: https://mise.jdx.dev/tasks/toml-tasks.html
- rustup override precedence: https://rust-lang.github.io/rustup/overrides.html
