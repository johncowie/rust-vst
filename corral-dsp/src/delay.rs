//! A fixed multi-channel delay, allocated once.

pub struct DelayLine {
    /// `channels × len` samples, channel-major.
    buffer: Vec<f32>,
    len: usize,
    pos: usize,
    channels: usize,
}

impl DelayLine {
    /// Allocates. `len` may be 0 (no delay).
    pub fn new(channels: usize, len: usize) -> Self {
        Self {
            buffer: vec![0.0; channels * len],
            len,
            pos: 0,
            channels,
        }
    }

    /// Zeroes the history; doesn't allocate.
    pub fn reset(&mut self) {
        self.buffer.fill(0.0);
        self.pos = 0;
    }

    /// Delays one frame in place. `frame.len()` must be ≤ `channels`.
    /// Samples are copied, never computed, so the output is bit-identical
    /// to the input `len` frames earlier (NaN included).
    #[inline]
    pub fn process(&mut self, frame: &mut [f32]) {
        debug_assert!(frame.len() <= self.channels);
        if self.len == 0 {
            return;
        }
        for (channel, sample) in frame.iter_mut().enumerate() {
            std::mem::swap(sample, &mut self.buffer[channel * self.len + self.pos]);
        }
        self.pos += 1;
        if self.pos == self.len {
            self.pos = 0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Runs `input` (one frame per element) through a fresh delay line.
    fn run(delay: &mut DelayLine, input: &[[f32; 2]]) -> Vec<[f32; 2]> {
        input
            .iter()
            .map(|&frame| {
                let mut frame = frame;
                delay.process(&mut frame);
                frame
            })
            .collect()
    }

    #[test]
    fn an_impulse_comes_out_len_frames_later_on_each_channel() {
        for len in [1, 5, 1200] {
            let mut delay = DelayLine::new(2, len);
            let mut input = vec![[0.0; 2]; 2 * len + 3];
            input[2] = [1.0, -1.0];
            let output = run(&mut delay, &input);
            for (i, frame) in output.iter().enumerate() {
                let expected = if i == 2 + len { [1.0, -1.0] } else { [0.0; 2] };
                assert_eq!(*frame, expected, "len {len}, frame {i}");
            }
        }
    }

    #[test]
    fn channels_dont_mix() {
        let mut delay = DelayLine::new(2, 3);
        let input: Vec<[f32; 2]> = (0..10).map(|i| [i as f32, -(i as f32) - 100.0]).collect();
        let output = run(&mut delay, &input);
        for i in 3..10 {
            assert_eq!(output[i], input[i - 3], "frame {i}");
        }
    }

    #[test]
    fn zero_length_is_the_identity() {
        let mut delay = DelayLine::new(2, 0);
        let input: Vec<[f32; 2]> = (0..5).map(|i| [i as f32, 1.0 - i as f32]).collect();
        assert_eq!(run(&mut delay, &input), input);
    }

    #[test]
    fn reset_clears_the_history() {
        let mut delay = DelayLine::new(2, 4);
        run(&mut delay, &[[1.0, 2.0]; 3]);
        delay.reset();
        assert_eq!(run(&mut delay, &[[0.0; 2]; 4]), vec![[0.0; 2]; 4]);
    }

    #[test]
    fn nan_and_negative_zero_come_out_bit_for_bit() {
        let mut delay = DelayLine::new(2, 2);
        let nan = f32::from_bits(0x7fc0_1234);
        let input = [
            [nan, -0.0],
            [f32::INFINITY, f32::MIN_POSITIVE],
            [0.0; 2],
            [0.0; 2],
        ];
        let output = run(&mut delay, &input);
        let bits = |f: [f32; 2]| f.map(f32::to_bits);
        assert_eq!(bits(output[2]), bits(input[0]));
        assert_eq!(bits(output[3]), bits(input[1]));
    }

    #[test]
    fn a_mono_frame_uses_the_first_channel() {
        let mut delay = DelayLine::new(2, 1);
        let mut frame = [0.5];
        delay.process(&mut frame);
        assert_eq!(frame, [0.0]);
        let mut frame = [0.25];
        delay.process(&mut frame);
        assert_eq!(frame, [0.5]);
    }
}
