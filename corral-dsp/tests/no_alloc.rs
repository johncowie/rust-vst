//! Proves the per-frame path never allocates, with nice-plug's allocation
//! checker as this test binary's global allocator. The violation counter is
//! global to the binary, so this file holds exactly one test.

// The checker (and its counter) only exists with debug assertions.
#![cfg(debug_assertions)]

mod common;

use common::{MAJOR_THIRDS, fixed, harmonic_tone, thirds_grid, white_noise};
use corral_dsp::chain::{Chain, Settings};
use corral_dsp::grid::{Grid, INTERVALS, PITCH_CLASSES, Row};
use corral_dsp::shift::{DEFAULT_GLIDE_S, MAX_GLIDE_S};
use nice_assert_no_alloc::{
    AllocDisabler, assert_no_alloc, reset_violation_count, violation_count,
};

#[global_allocator]
static A: AllocDisabler = AllocDisabler;

fn run(chain: &mut Chain, signal: &[f32], settings: Settings) {
    for &s in signal {
        let mut frame = [s, s];
        std::hint::black_box(chain.process_frame(&mut frame, settings));
    }
}

#[test]
fn per_frame_path_never_allocates() {
    for sample_rate in [44_100.0, 48_000.0] {
        // Built (and signals synthesised) outside the check: setup may allocate.
        let mut chain = Chain::new(sample_rate);
        let len = sample_rate as usize;
        let tone = harmonic_tone(sample_rate, 1.0, |_| 220.0, 8, 0.5);
        let noise = white_noise(len, 0.25, 7);
        let silence = vec![0.0; len];
        let scale = common::scale(sample_rate);

        reset_violation_count();
        assert_no_alloc(|| {
            for signal in [&tone, &noise, &silence] {
                run(&mut chain, signal, fixed(4, 0.5));
            }
            // Shift stepping through every interval every 2000 samples,
            // mix alternating.
            for (i, &s) in tone.iter().enumerate() {
                let step = i / 2000;
                let settings = fixed(INTERVALS[step % INTERVALS.len()], (step % 2) as f32);
                let mut frame = [s, s];
                std::hint::black_box(chain.process_frame(&mut frame, settings));
            }
            // C major thirds over the scale while the glide sweeps
            // 0 → 250 ms, so onsets, note changes and coefficient
            // recomputes all happen.
            let len = scale.len() as f32;
            for (i, &s) in scale.iter().enumerate() {
                let settings = common::voices(
                    thirds_grid(0, &MAJOR_THIRDS),
                    MAX_GLIDE_S * i as f32 / len,
                    1.0,
                );
                let mut frame = [s, s];
                std::hint::black_box(chain.process_frame(&mut frame, settings));
            }
            // Every row full: 24 voices on every note.
            let full = Row::from_bits(u32::MAX);
            let mut grid = Grid::EMPTY;
            for pc in 0..PITCH_CLASSES {
                grid.set_row(pc, full);
            }
            run(
                &mut chain,
                &scale,
                common::voices(grid, DEFAULT_GLIDE_S, 0.5),
            );
            // Even pitch classes full, odd ones {−12, +12}, so note changes
            // both glide and fade. Every 1000 samples the same cell is
            // toggled in every row, so each toggle edits the sounding row.
            let octaves = Row::EMPTY.with(-12, true).with(12, true);
            for (i, &s) in scale.iter().enumerate() {
                let toggled = INTERVALS[i / 1000 % INTERVALS.len()];
                let mut grid = Grid::EMPTY;
                for pc in 0..PITCH_CLASSES {
                    let row = if pc % 2 == 0 { full } else { octaves };
                    grid.set_row(pc, row.toggled(toggled));
                }
                let settings = common::voices(grid, DEFAULT_GLIDE_S, 0.5);
                let mut frame = [s, s];
                std::hint::black_box(chain.process_frame(&mut frame, settings));
            }
            chain.reset();
            run(&mut chain, &tone, fixed(-12, 1.0));
            run(&mut chain, &tone, common::DRY);
        });
        assert_eq!(violation_count(), 0, "{sample_rate} Hz");
    }
}
