---
type: plan
id: "2026-09-25-0001-project-setup-with-mise-and-nice-plug"
title: "Project Setup with mise and nice-plug Implementation Plan"
date: "2026-09-25T13:56:14+00:00"
author: John Cowie Del Corral
producer: create-plan
status: draft
work_item_id: "work-item:0001"
parent: "work-item:0001"
tags: [setup, tooling, phase-0]
revision: "8848dfcbe2e82dafdb18600da3920b0b0dd18165"
repository: "rust-vst"
last_updated: "2026-09-25T13:56:14+00:00"
last_updated_by: John Cowie Del Corral
schema_version: 1
---

# Project Setup with mise and nice-plug Implementation Plan

## Overview

This plan completes Phase 0 of `ROADMAP.md`. It delivers:

- a toolchain pinned by mise (Rust 1.98.1, which overrides the shell's global
  `RUSTUP_TOOLCHAIN=1.79.0`);
- a Cargo workspace with a `corral` stereo pass-through plugin that exports
  VST3 and CLAP, plus the nice-plug `xtask` bundler;
- six mise tasks for building, formatting and linting;
- a README and an updated roadmap.

Every later phase depends on this.

## Current State Analysis

- The repository holds only `ROADMAP.md`, `.gitignore` (`/target`, `.DS_Store`,
  `.accelerator/config.local.md`), the Accelerator scaffold under
  `.accelerator/` and `meta/`, and work item 0001. There is no Rust code, no
  `Cargo.toml` and no `mise.toml`.
- Local environment, checked on 2026-09-25:
  - mise 2026.3.12 at `/opt/homebrew/bin/mise`, with `eval "$(mise activate zsh)"`
    at `~/.zshrc:57`.
  - The global mise config (`~/.config/mise/config.toml`) sets
    `[settings] lockfile = true`.
  - rustup is at `~/.cargo/bin/rustup`, and only `1.79.0-aarch64-apple-darwin`
    is installed. `RUSTUP_TOOLCHAIN=1.79.0` is exported in the shell.
  - Xcode is installed (`/Applications/Xcode.app/Contents/Developer`), and
    `lipo` is at `/usr/bin/lipo`.
  - There is no git remote.

## Desired End State

- `mise trust && mise install` in a fresh clone installs Rust 1.98.1 through
  rustup, with `clippy`, `rustfmt` and both macOS targets.
- Inside the repository, cargo reports 1.98.1 under `mise exec`, `mise run`
  and `mise activate`. Outside it, cargo reports 1.79.0.
- `mise run build` writes `target/bundled/Corral.vst3` and
  `target/bundled/Corral.clap`.
- `mise run build-universal` writes the same bundles, and their binaries
  contain both `x86_64` and `arm64`.
- `mise run check` exits 0 on the scaffold, and exits non-zero on a clippy
  violation or a formatting error.
- `mise tasks` lists `build`, `build-universal`, `fmt`, `fmt-check`, `lint`
  and `check`, each with a description.
- The README documents setup and licensing, and `ROADMAP.md` names nice-plug
  and has every Phase 0 item ticked.

### Key Discoveries:

**nice-plug**
- `nice-plug` 0.4.2 (imported as `nice_plug`, edition 2024, MSRV 1.88, ISC) and
  `nice-plug-xtask` 0.1.1 (imported as `nice_plug_xtask`, ISC) are both on
  crates.io.
- nice-plug has no git dependencies. Its VST3 bindings are the `vst3` 0.3
  crate, licensed `MIT OR Apache-2.0`.
- API differences from nih-plug:
  - The export macros are `nice_export_clap!` and `nice_export_vst3!`.
  - `type Editor = ();` is now required.
  - `initialize` is renamed `activate`.
- The xtask supports `cargo xtask bundle-universal <pkg> [--release]`
  (macOS only). It builds `--target x86_64-apple-darwin` and
  `--target aarch64-apple-darwin`, then merges the two with `lipo` into the
  same `target/bundled/<Name>.{vst3,clap}` paths, with no "universal" suffix.
  **This answers the work item's open question.** No hand-written `lipo` step
  is needed.
- The xtask does **not** install targets. mise provides them.
- A `bundler.toml` next to the root `Cargo.toml` sets the display name
  (`[corral] name = "Corral"`). Without it, bundles take the package name
  (`corral.vst3`).
- On macOS, the binary inside a bundle is at `<Name>.vst3/Contents/MacOS/<Name>`
  and `<Name>.clap/Contents/MacOS/<Name>`.
- `crate-type = ["cdylib"]` is enough. `"lib"` is only needed for standalone
  builds.

**mise 2026.3.12**
- The rust plugin delegates to rustup and always exports `RUSTUP_TOOLCHAIN`,
  `RUSTUP_HOME` and `CARGO_HOME`. By default these point at the existing
  `~/.rustup` and `~/.cargo`.
- **In 2026.3.12, `components` and `targets` must be comma-separated strings
  with no spaces.** The plugin splits on `,` and does not trim. TOML arrays
  only work in newer mise. (Decision: use strings and set
  `min_version = "2026.3.12"`.)
- In 2026.3.12, every project `mise.toml` must be trusted:
  - `mise install` prompts for trust.
  - `mise activate` silently skips an untrusted config and prints a warning.
  - The README must tell users to run `mise trust` first.
- In tasks, `run = [{ task = "a" }, { task = "b" }]` runs the tasks one after
  another. `depends` runs them in parallel. (Decision: `check` runs them one
  after another.)
- Tasks run under `sh -c` with `set -e`.

## What We're NOT Doing

- Codesigning, installing into `~/Library/Audio/Plug-Ins/`, or loading the
  plugin in Ableton. These are Phase 1.
- A gain parameter, or any parameters at all. The params struct stays empty.
- A GUI or editor.
- CI. The `check` task is designed so CI can call it later.
- `clippy::pedantic` or any other stricter lint groups, and any rustfmt options
  beyond `style_edition`.
- A `rust-toolchain.toml`. It would conflict with the `RUSTUP_TOOLCHAIN` that
  mise sets.
- Fixing IDE rust-analyzer when it runs outside mise's environment. The README
  documents this limitation.
- Removing the developer's global `RUSTUP_TOOLCHAIN` export.
- Ticking the Phase 6 "Universal binary and CLAP build" item. It only gets a
  note that Phase 0 delivered it.

## Implementation Approach

We build up in layers and verify each one before the next. The toolchain comes
first, because everything else needs it. Next comes the workspace, checked
directly with `mise exec -- cargo …`. Then the tasks wrap those same commands.
The docs come last, so they describe what was actually built.

Plugin identity (these values are baked into saved Ableton sessions, so do not
change them later):

| Field | Value |
|---|---|
| `NAME` | `"Corral"` |
| `VENDOR` | `"John Cowie Del Corral"` |
| `URL` | `"https://github.com/johncowie/rust-vst"` (placeholder; there is no remote yet, and changing it later is harmless) |
| `EMAIL` | `"john.a.cowie@gmail.com"` |
| `CLAP_ID` | `"com.johncowie.corral"` |
| `VST3_CLASS_ID` | `*b"JCowieCorral0001"` (exactly 16 bytes) |

---

## Phase 1: Toolchain via mise

### Overview

Pin Rust 1.98.1 with its components and targets, and prove that it overrides
the global `RUSTUP_TOOLCHAIN=1.79.0`.

### Changes Required:

#### 1. mise configuration

**File**: `mise.toml` (new)

```toml
min_version = "2026.3.12"

[tools]
# mise 2026.3.x only parses comma-separated strings (no spaces) for these options.
rust = { version = "1.98.1", components = "clippy,rustfmt", targets = "aarch64-apple-darwin,x86_64-apple-darwin" }
```

#### 2. Trust and install

Run `mise trust` and then `mise install` in the repo root. The global
`lockfile = true` setting makes `mise install` write a `mise.lock`. Commit it.

#### 3. Ignore local mise overrides

**File**: `.gitignore`
**Changes**: add `mise.local.toml`, so that personal overrides are not
committed.

### Success Criteria:

#### Automated Verification:

- [x] `mise install` exits 0.
- [x] `rustup toolchain list` includes `1.98.1-aarch64-apple-darwin`.
- [x] `rustup +1.98.1 target list --installed` lists both
      `aarch64-apple-darwin` and `x86_64-apple-darwin`.
- [x] `rustup +1.98.1 component list --installed` includes `clippy` and
      `rustfmt`.
- [x] `RUSTUP_TOOLCHAIN=1.79.0 mise exec -- cargo --version` reports
      `cargo 1.98.1`.
- [x] `mise env | grep RUSTUP_TOOLCHAIN` shows `1.98.1`.
- [x] `mise.lock` exists.

#### Manual Verification:

- [x] In a new interactive zsh (where `mise activate` runs and
      `RUSTUP_TOOLCHAIN=1.79.0` is exported):
  - [x] `cd` into the repo, and `cargo --version` reports 1.98.1.
  - [x] `cd ..`, and `cargo --version` reports 1.79.0.

---

## Phase 2: Workspace Scaffold

### Overview

Create the Cargo workspace, the `corral` pass-through plugin, the `xtask`
bundler and the lint and format configuration. Then check that both the
single-architecture and the universal bundles build.

### Changes Required:

#### 1. Root workspace manifest

**File**: `Cargo.toml` (new)

```toml
[workspace]
resolver = "3"
members = ["corral", "xtask"]

[workspace.package]
edition = "2024"

[workspace.lints.clippy]
all = { level = "warn", priority = -1 }

# Recommended by nice-plug's GETTING_STARTED.md.
[profile.dev]
opt-level = 1

[profile.release]
codegen-units = 1
lto = "thin"
```

#### 2. Plugin crate

**File**: `corral/Cargo.toml` (new)

```toml
[package]
name = "corral"
version = "0.1.0"
edition.workspace = true
publish = false

[lib]
crate-type = ["cdylib"]

[dependencies]
nice-plug = "0.4"

[lints]
workspace = true
```

**File**: `corral/src/lib.rs` (new)

```rust
use std::sync::Arc;

use nice_plug::prelude::*;

/// Stereo pass-through. Buffers are processed in place, so leaving them
/// untouched passes audio straight through.
#[derive(Default)]
pub struct Corral {
    params: Arc<CorralParams>,
}

#[derive(Params, Default)]
struct CorralParams {}

impl Plugin for Corral {
    const NAME: &'static str = "Corral";
    const VENDOR: &'static str = "John Cowie Del Corral";
    const URL: &'static str = "https://github.com/johncowie/rust-vst";
    const EMAIL: &'static str = "john.a.cowie@gmail.com";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[AudioIOLayout {
        main_input_channels: NonZeroU32::new(2),
        main_output_channels: NonZeroU32::new(2),
        ..AudioIOLayout::const_default()
    }];

    const MIDI_INPUT: MidiConfig = MidiConfig::None;

    type Editor = ();
    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn process(
        &mut self,
        _buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        _context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        ProcessStatus::Normal
    }
}

impl ClapPlugin for Corral {
    const CLAP_ID: &'static str = "com.johncowie.corral";
    const CLAP_DESCRIPTION: Option<&'static str> = Some("Auto-harmoniser");
    const CLAP_MANUAL_URL: Option<&'static str> = None;
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] =
        &[ClapFeature::AudioEffect, ClapFeature::Stereo];
}

impl Vst3Plugin for Corral {
    const VST3_CLASS_ID: [u8; 16] = *b"JCowieCorral0001";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Fx, Vst3SubCategory::Tools];
}

nice_export_clap!(Corral);
nice_export_vst3!(Corral);
```

Compile-time adjustments you may need:
- If the prelude does not re-export `NonZeroU32`, add
  `use std::num::NonZeroU32;`.
- Run `cargo fmt` after writing the code, so the final layout follows rustfmt
  rather than this plan.

#### 3. Bundler xtask

**File**: `xtask/Cargo.toml` (new)

```toml
[package]
name = "xtask"
version = "0.1.0"
edition.workspace = true
publish = false

[dependencies]
nice-plug-xtask = "0.1.1"

[lints]
workspace = true
```

**File**: `xtask/src/main.rs` (new)

```rust
fn main() -> nice_plug_xtask::Result<()> {
    nice_plug_xtask::main()
}
```

**File**: `.cargo/config.toml` (new)

```toml
[alias]
xtask = "run --package xtask --release --"
```

**File**: `bundler.toml` (new)

```toml
[corral]
name = "Corral"
```

#### 4. Formatting config

**File**: `rustfmt.toml` (new)

```toml
style_edition = "2024"
```

#### 5. Lockfile

Commit the generated `Cargo.lock`. This workspace builds a product, not a
library.

### Success Criteria:

#### Automated Verification:

- [x] `mise exec -- cargo build --workspace` exits 0.
- [x] `mise exec -- cargo xtask bundle corral --release` exits 0.
- [x] `target/bundled/Corral.vst3/Contents/MacOS/Corral` and
      `target/bundled/Corral.clap/Contents/MacOS/Corral` exist.
- [x] `mise exec -- cargo xtask bundle-universal corral --release` exits 0.
- [x] `lipo -archs target/bundled/Corral.vst3/Contents/MacOS/Corral` reports
      both `x86_64` and `arm64`. The same holds for the `.clap` binary.
- [x] `mise exec -- cargo fmt --all --check` exits 0.
- [x] `mise exec -- cargo clippy --workspace --all-targets -- -D warnings`
      exits 0.

#### Manual Verification:

- [x] `plutil -p target/bundled/Corral.vst3/Contents/Info.plist` shows
      `CFBundleExecutable` set to `Corral`.

---

## Phase 3: mise Tasks

### Overview

Wrap the commands verified in Phase 2 as mise tasks, and prove that the lint
and format tasks fail when they should.

### Changes Required:

#### 1. Task definitions

**File**: `mise.toml`
**Changes**: append the following.

```toml
[tasks.build]
description = "Bundle corral as VST3 and CLAP for the host architecture (release)"
run = "cargo xtask bundle corral --release"

[tasks.build-universal]
description = "Bundle corral as universal (arm64 + x86_64) macOS VST3 and CLAP (release)"
run = "cargo xtask bundle-universal corral --release"

[tasks.fmt]
description = "Format all workspace code"
run = "cargo fmt --all"

[tasks.fmt-check]
description = "Check formatting without changing files"
run = "cargo fmt --all --check"

[tasks.lint]
description = "Run clippy on all targets, treating warnings as errors"
run = "cargo clippy --workspace --all-targets -- -D warnings"

[tasks.check]
description = "Run fmt-check then lint (the CI entry point)"
run = [{ task = "fmt-check" }, { task = "lint" }]
```

Fallback: if mise 2026.3.12 rejects the `{ task = … }` run entries, use
`run = ["mise run fmt-check", "mise run lint"]` instead. This keeps the tasks
running one after the other and failing fast.

### Success Criteria:

#### Automated Verification:

- [x] `mise tasks` lists all six tasks, each with a non-empty description.
- [x] `rm -rf target/bundled && mise run build` exits 0, and `Corral.vst3`
      and `Corral.clap` appear in `target/bundled/`.
- [x] After `mise run build-universal`, `lipo -archs` on the bundled VST3
      binary reports both `x86_64` and `arm64`.
- [x] `mise run check` exits 0.
- [x] Negative lint test:
  - Temporarily add `let mut unused = 0; let _ = unused;` to `process` in
    `corral/src/lib.rs`.
  - `mise run lint` exits non-zero (`unused_mut` is denied by `-D warnings`).
  - Revert the change.
- [x] Negative format test:
  - Temporarily mis-format `xtask/src/main.rs`, for example by collapsing
    `main` onto one line with extra spaces.
  - `mise run fmt-check` exits non-zero.
  - Run `mise run fmt`. `mise run fmt-check` then exits 0.
  - Confirm that `git diff xtask/src/main.rs` is empty.
- [x] `mise run check` stops at `fmt-check` and does not run clippy when
      formatting fails. Check this with the mis-formatted file.

#### Manual Verification:

- [ ] `mise run` with no arguments shows the task picker with descriptions.

---

## Phase 4: Documentation

### Overview

Write the README and bring `ROADMAP.md` in line with the decisions and the
delivered work.

### Changes Required:

#### 1. README

**File**: `README.md` (new)

**Sections**:

- **Title and summary**: Corral, an auto-harmoniser audio plugin (VST3 and
  CLAP) written in Rust with nice-plug. Link to `ROADMAP.md`.
- **Prerequisites**:
  - macOS, with Xcode or the Command Line Tools (for the linker and `lipo`).
  - [mise](https://mise.jdx.dev) 2026.3.12 or newer, with `mise activate`
    in the shell.
  - rustup is optional, because mise installs it if it is missing.
- **Setup**:
  1. `mise trust`. This is required. Otherwise mise ignores `mise.toml` or
     prompts for trust.
  2. `mise install`. This installs Rust 1.98.1 through rustup, with clippy,
     rustfmt and both macOS targets.
  3. Note that mise sets `RUSTUP_TOOLCHAIN`, which overrides any global value.
     Tools that run outside mise's environment still see the global toolchain.
     This includes an IDE's rust-analyzer without mise integration.
- **Tasks**: a table of the six tasks with their descriptions. Say that bundles
  land in `target/bundled/Corral.{vst3,clap}`, and that `build-universal`
  overwrites them with universal binaries.
- **Licensing**:
  - nice-plug is ISC-licensed.
  - Its VST3 bindings come from the `vst3` crate, which is MIT OR Apache-2.0.
  - So, unlike upstream nih-plug (whose `vst3-sys` bindings are GPLv3), there
    is no GPLv3 constraint on distributing the plugin.
  - The project is for personal use for now.

#### 2. Roadmap

**File**: `ROADMAP.md`
**Changes**:

- **Phase 0**:
  - Toolchain item: tick it, and reword it to "Install the Rust toolchain via
    mise (Rust 1.98.1 through rustup), plus both macOS targets".
  - Framework item: tick it, and reword it to name
    [nice-plug](https://codeberg.org/RustAudio/nice-plug), the maintained
    community fork of nih-plug (which is in maintenance mode).
  - Scaffold item: tick it.
  - Licensing item: tick it, and reword it to "nice-plug is ISC and its VST3
    bindings are MIT/Apache, so there is no GPLv3 constraint (see README)".
  - Add a ticked item for the mise tasks and the lint and format config.
- **Phase 1**:
  - Change `nih-plug SmoothingStyle::Logarithmic` to name nice-plug.
  - Reword the "script or `just`/`make` target" item to a mise task. Leave it
    unticked.
- **Phase 2**: change `nih_plug_egui` to "nice-plug's egui editor integration".
- **Phase 6**: append a note to the "Universal binary (arm64 + x86_64) and a
  CLAP build" item: "(delivered in Phase 0 via `mise run build-universal`)".
  Leave it unticked.

### Success Criteria:

#### Automated Verification:

- [x] `grep -n 'nih' ROADMAP.md` shows only the phrase that explains
      nice-plug is a fork of nih-plug.
- [x] `grep -c '\- \[ \]' ROADMAP.md` finds no unticked items between
      `## Phase 0` and `## Phase 1`.
- [x] `grep -n 'mise trust' README.md` and `grep -n 'ISC' README.md` both
      match.

#### Manual Verification:

- [ ] The README reads correctly from top to bottom, and following its setup
      steps in a fresh clone works.
- [ ] The roadmap wording is accurate and consistent.

---

## Testing Strategy

### Unit Tests:

- None. The plugin has no logic yet. DSP unit tests start in Phase 2 of the
  roadmap.

### Integration Tests:

- The bundling commands (`build` and `build-universal`) act as the integration
  check that the plugin compiles and exports both formats.

### Manual Testing Steps:

1. Open a new zsh terminal where `RUSTUP_TOOLCHAIN=1.79.0` is exported.
   `cd` into the repo and run `cargo --version`. Expect 1.98.1.
2. `cd ..` and run `cargo --version`. Expect 1.79.0.
3. Run `mise run build-universal` followed by
   `lipo -archs target/bundled/Corral.clap/Contents/MacOS/Corral`. Expect
   `x86_64 arm64`.
4. Run `mise tasks` and check that all six tasks have descriptions.

## Performance Considerations

- `lto = "thin"` and `codegen-units = 1` slow release builds, including the
  `--release` build of the xtask itself. In return they give better runtime
  DSP performance, which matters for the later phases.
- `opt-level = 1` for dev builds keeps debug builds usable for real-time audio.

## Migration Notes

None. This is a greenfield setup.

## References

- Original work item: `meta/work/0001-project-setup-with-mise-and-nice-plug.md`
- Roadmap: `ROADMAP.md` (Phase 0)
- nice-plug getting-started guide:
  https://codeberg.org/RustAudio/nice-plug/raw/branch/main/GETTING_STARTED.md
- nice-plug xtask (`bundle-universal`, `bundler.toml`):
  https://codeberg.org/RustAudio/nice-plug/raw/branch/main/crates/nice-plug-xtask/src/lib.rs
- nice-plug changelog (API renames):
  https://codeberg.org/RustAudio/nice-plug/raw/branch/main/CHANGELOG.md
- mise rust plugin at v2026.3.12 (comma-string options, `RUSTUP_TOOLCHAIN`):
  https://raw.githubusercontent.com/jdx/mise/v2026.3.12/src/plugins/core/rust.rs
- mise trust check at v2026.3.12:
  https://raw.githubusercontent.com/jdx/mise/v2026.3.12/src/config/config_file/mod.rs
- mise TOML tasks: https://mise.jdx.dev/tasks/toml-tasks.html
