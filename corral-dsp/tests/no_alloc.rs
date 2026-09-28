//! Proves the per-frame path never allocates, with nice-plug's allocation
//! checker as this test binary's global allocator. The violation counter is
//! global to the binary, so this file holds exactly one test.

// The checker (and its counter) only exists with debug assertions.
#![cfg(debug_assertions)]

mod common;

use common::{harmonic_tone, white_noise};
use corral_dsp::chain::{Chain, Settings};
use corral_dsp::shift;
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

        reset_violation_count();
        assert_no_alloc(|| {
            for signal in [&tone, &noise, &silence] {
                run(
                    &mut chain,
                    signal,
                    Settings {
                        gain: 0.5,
                        mix: 0.5,
                        shift: 4,
                    },
                );
            }
            // Shift sweeping −12..=12 every 2000 samples, mix alternating.
            for (i, &s) in tone.iter().enumerate() {
                let step = i / 2000;
                let settings = Settings {
                    gain: 1.0,
                    mix: if step % 2 == 0 { 0.0 } else { 1.0 },
                    shift: shift::MIN + (step % 25) as i32,
                };
                let mut frame = [s, s];
                std::hint::black_box(chain.process_frame(&mut frame, settings));
            }
            chain.reset();
            run(
                &mut chain,
                &tone,
                Settings {
                    gain: 1.0,
                    mix: 1.0,
                    shift: -12,
                },
            );
            run(&mut chain, &tone, common::DRY);
        });
        assert_eq!(violation_count(), 0, "{sample_rate} Hz");
    }
}
