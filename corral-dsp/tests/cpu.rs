//! Times the chain per 512-frame stereo block, for the README. No
//! pass/fail threshold; run it in a release build.

mod common;

use std::hint::black_box;
use std::time::Instant;

use common::note_sequence;
use corral_dsp::chain::Chain;
use corral_dsp::grid::{Grid, PITCH_CLASSES, Row};
use corral_dsp::shift::DEFAULT_GLIDE_S;

const SAMPLE_RATE: f32 = 48_000.0;
const SECONDS: usize = 10;
const BLOCK: usize = 512;

const C_MAJOR: [f64; 8] = [60.0, 62.0, 64.0, 65.0, 67.0, 69.0, 71.0, 72.0];
const NOTE_S: f64 = 0.5;

fn every_row(row: Row) -> Grid {
    let mut grid = Grid::EMPTY;
    for pc in 0..PITCH_CLASSES {
        grid.set_row(pc, row);
    }
    grid
}

#[test]
#[ignore = "a release-build measurement: cargo test --release -p corral-dsp --test cpu -- --ignored --nocapture"]
fn time_per_block_with_a_full_row() {
    if cfg!(debug_assertions) {
        println!("WARNING: built with debug assertions; these figures mean nothing");
    }
    let once = note_sequence(SAMPLE_RATE, &C_MAJOR, NOTE_S);
    let len = SECONDS * SAMPLE_RATE as usize;
    let signal: Vec<f32> = once.iter().copied().cycle().take(len).collect();
    let real_time_us = BLOCK as f64 / f64::from(SAMPLE_RATE) * 1e6;

    let grids = [
        ("empty", Grid::EMPTY),
        ("1 voice (+4)", Grid::uniform(4)),
        (
            "3 voices (-5, +4, +7)",
            every_row(Row::EMPTY.with(-5, true).with(4, true).with(7, true)),
        ),
        ("24 voices (full row)", every_row(Row::from_bits(u32::MAX))),
    ];
    println!(
        "{} s at {SAMPLE_RATE} Hz, {BLOCK}-frame stereo blocks ({real_time_us:.0} µs real time each):",
        SECONDS
    );
    for (name, grid) in grids {
        let settings = common::voices(grid, DEFAULT_GLIDE_S, 0.5);
        let mut chain = Chain::new(SAMPLE_RATE);
        let mut left = [0.0f32; BLOCK];
        let mut right = [0.0f32; BLOCK];
        let (blocks, _) = signal.as_chunks::<BLOCK>();
        let mut times: Vec<f64> = blocks
            .iter()
            .map(|block| {
                left.copy_from_slice(block);
                right.copy_from_slice(block);
                let start = Instant::now();
                for (l, r) in left.iter_mut().zip(right.iter_mut()) {
                    let mut frame = [*l, *r];
                    black_box(chain.process_frame(&mut frame, settings));
                    [*l, *r] = frame;
                }
                let elapsed = start.elapsed().as_secs_f64() * 1e6;
                black_box((&left, &right));
                elapsed
            })
            .collect();
        times.sort_by(f64::total_cmp);
        let mean = times.iter().sum::<f64>() / times.len() as f64;
        let p99 = times[(times.len() * 99 / 100).min(times.len() - 1)];
        let max = times[times.len() - 1];
        let share = |us: f64| us / real_time_us * 100.0;
        println!(
            "{name:<24} mean {mean:>7.1} µs ({:>5.1}%)  p99 {p99:>7.1} µs ({:>5.1}%)  max {max:>7.1} µs ({:>5.1}%)",
            share(mean),
            share(p99),
            share(max)
        );
    }
}
