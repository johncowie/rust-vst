# Corral

Corral is an auto-harmoniser audio plugin (VST3 and CLAP) written in Rust with
[nice-plug](https://codeberg.org/RustAudio/nice-plug). See
[`ROADMAP.md`](ROADMAP.md) for where it is heading.

## Prerequisites

- macOS, with Xcode or the Command Line Tools (for the linker and `lipo`).
- [mise](https://mise.jdx.dev) 2026.3.12 or newer, with `mise activate` in your
  shell.
- rustup is optional. mise installs it if it is missing.

## Setup

1. Trust the project config:

   ```sh
   mise trust
   ```

   This is required. Without it, mise ignores `mise.toml` (under
   `mise activate`) or prompts for trust (under `mise install`).

2. Install the toolchain:

   ```sh
   mise install
   ```

   This installs Rust 1.98.1 through rustup, with `clippy`, `rustfmt` and both
   macOS targets (`aarch64-apple-darwin` and `x86_64-apple-darwin`).

3. mise sets `RUSTUP_TOOLCHAIN` inside the project, which overrides any global
   value you export. Tools that run outside mise's environment still see the
   global toolchain. This includes an IDE's rust-analyzer without mise
   integration.

## Tasks

Run a task with `mise run <task>`, or `mise run` on its own to pick one.

| Task | Description |
|---|---|
| `build` | Bundle corral as VST3 and CLAP for the host architecture (release) |
| `build-universal` | Bundle corral as universal (arm64 + x86_64) macOS VST3 and CLAP (release) |
| `fmt` | Format all workspace code |
| `fmt-check` | Check formatting without changing files |
| `lint` | Run clippy on all targets, treating warnings as errors |
| `check` | Run fmt-check then lint (the CI entry point) |

Bundles land in `target/bundled/Corral.vst3` and `target/bundled/Corral.clap`.
`build-universal` writes to the same paths, overwriting them with universal
binaries.

## Licensing

- nice-plug is ISC-licensed.
- Its VST3 bindings come from the [`vst3`](https://crates.io/crates/vst3) crate,
  which is MIT OR Apache-2.0.
- So, unlike upstream nih-plug (whose `vst3-sys` bindings are GPLv3), there is
  no GPLv3 constraint on distributing the plugin.
- The project is for personal use for now.
