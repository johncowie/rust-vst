//! The shifter's objective tests: identity and delay.

mod common;

use common::{harmonic_tone, midi_to_hz, white_noise};
use corral_dsp::chain::{Chain, Settings};

#[test]
fn mix_zero_is_the_delayed_input() {
    for sample_rate in [44_100.0, 48_000.0] {
        let len = sample_rate as usize;
        let a3 = harmonic_tone(sample_rate, 1.0, |_| midi_to_hz(57.0), 8, 0.5);
        let noise = white_noise(len, 0.25, 7);
        let silence = vec![0.0; len];
        // Different content on each channel.
        let cases = [
            ("tone / noise", &a3, &noise),
            ("noise / tone", &noise, &a3),
            ("silence / tone", &silence, &a3),
            ("silence", &silence, &silence),
        ];
        for (name, left, right) in cases {
            let mut chain = Chain::new(sample_rate);
            let latency = chain.latency();
            let settings = Settings {
                gain: 1.0,
                mix: 0.0,
                shift: 4,
            };
            let output: Vec<[f32; 2]> = left
                .iter()
                .zip(right.iter())
                .map(|(&l, &r)| {
                    let mut frame = [l, r];
                    chain.process_frame(&mut frame, settings);
                    frame
                })
                .collect();

            for (i, frame) in output.iter().enumerate() {
                let expected = if i < latency {
                    [0.0; 2]
                } else {
                    [left[i - latency], right[i - latency]]
                };
                assert_eq!(
                    frame.map(f32::to_bits),
                    expected.map(f32::to_bits),
                    "{sample_rate} Hz, {name}, frame {i}"
                );
            }
        }
    }
}
