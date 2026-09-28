//! The Shift parameter: semitones → ratio, a click-free ramp, and display.

pub const MIN: i32 = -12;
pub const MAX: i32 = 12;
pub const DEFAULT: i32 = 4;
/// Time for any change of Shift to complete. Avoids clicks and zipper noise;
/// not a musical glide (that is Phase 5).
const SHIFT_RAMP_S: f32 = 0.050;

/// 2^(semitones/12).
pub fn ratio(semitones: f32) -> f32 {
    (semitones / 12.0).exp2()
}

/// Linear ramp in semitones toward the target. Once per sample.
pub struct SemitoneRamp {
    current: f32,
    target: f32,
    /// Signed semitones per sample for the change in progress.
    step: f32,
    /// Samples left in the change in progress. Counting them, rather than
    /// comparing `current` with `target`, keeps the duration exact.
    remaining: u32,
    ramp_samples: u32,
}

impl SemitoneRamp {
    pub fn new(sample_rate: f32, initial: i32) -> Self {
        Self {
            current: initial as f32,
            target: initial as f32,
            step: 0.0,
            remaining: 0,
            ramp_samples: ((SHIFT_RAMP_S * sample_rate).round() as u32).max(1),
        }
    }

    /// Jumps to `target` with no ramp (used on reset).
    pub fn reset(&mut self, target: i32) {
        self.current = target as f32;
        self.target = target as f32;
        self.step = 0.0;
        self.remaining = 0;
    }

    /// Advances one sample toward `target` and returns the current value.
    /// Every change takes `SHIFT_RAMP_S`, whatever its size; a new target
    /// mid-ramp starts a fresh ramp from where it is.
    #[inline]
    pub fn next(&mut self, target: i32) -> f32 {
        let target = target as f32;
        if target != self.target {
            self.target = target;
            self.remaining = self.ramp_samples;
            self.step = (target - self.current) / self.ramp_samples as f32;
        }
        if self.remaining > 0 {
            self.remaining -= 1;
            // Measured back from the target, so it never overshoots or drifts.
            self.current = self.target - self.step * self.remaining as f32;
        }
        self.current
    }
}

/// "+4", "0", "-12" (no unit; the parameter adds " st").
pub fn format_shift(semitones: i32) -> String {
    if semitones == 0 {
        "0".to_string()
    } else {
        format!("{semitones:+}")
    }
}

/// Accepts "+4", "4", "-5", " 7 st"; rejects out-of-range and non-integers.
pub fn parse_shift(text: &str) -> Option<i32> {
    let text = text.trim();
    let number = text.strip_suffix("st").unwrap_or(text).trim_end();
    number
        .parse::<i32>()
        .ok()
        .filter(|semitones| (MIN..=MAX).contains(semitones))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ratios_of_whole_octaves_and_unison_are_exact() {
        assert_eq!(ratio(12.0), 2.0);
        assert_eq!(ratio(-12.0), 0.5);
        assert_eq!(ratio(0.0), 1.0);
        assert!((ratio(7.0) - 1.498_307).abs() < 1e-6);
    }

    /// Runs the ramp toward `target` until it lands; returns the samples taken
    /// and every value along the way.
    fn run_to(ramp: &mut SemitoneRamp, target: i32) -> (usize, Vec<f32>) {
        let mut values = Vec::new();
        for i in 1..100_000 {
            let value = ramp.next(target);
            values.push(value);
            if value == target as f32 {
                return (i, values);
            }
        }
        panic!("the ramp never reached {target}");
    }

    #[test]
    fn the_ramp_reaches_any_target_in_the_ramp_time_and_is_monotonic() {
        for sample_rate in [44_100.0, 48_000.0, 96_000.0] {
            let expected = SHIFT_RAMP_S * sample_rate;
            for (from, to) in [(0, 1), (4, -12), (-12, 12), (12, 11), (DEFAULT, 7)] {
                let mut ramp = SemitoneRamp::new(sample_rate, from);
                let (samples, values) = run_to(&mut ramp, to);
                assert!(
                    (samples as f32 - expected).abs() <= 1.0,
                    "{from} → {to} at {sample_rate}: {samples} samples, expected {expected}"
                );
                let rising = to > from;
                for pair in values.windows(2) {
                    assert!(
                        if rising {
                            pair[1] >= pair[0]
                        } else {
                            pair[1] <= pair[0]
                        },
                        "{from} → {to}: {pair:?}"
                    );
                }
                // And it stays there.
                assert_eq!(ramp.next(to), to as f32);
            }
        }
    }

    #[test]
    fn a_new_target_mid_ramp_takes_a_full_ramp_from_where_it_is() {
        let sample_rate = 48_000.0;
        let mut ramp = SemitoneRamp::new(sample_rate, 0);
        for _ in 0..1000 {
            ramp.next(12);
        }
        let (samples, _) = run_to(&mut ramp, -12);
        assert!((samples as f32 - SHIFT_RAMP_S * sample_rate).abs() <= 1.0);
    }

    #[test]
    fn reset_jumps_without_a_ramp() {
        let mut ramp = SemitoneRamp::new(48_000.0, 0);
        ramp.next(12);
        ramp.reset(DEFAULT);
        assert_eq!(ramp.next(DEFAULT), DEFAULT as f32);
    }

    #[test]
    fn format_and_parse_round_trip_over_the_range() {
        for semitones in MIN..=MAX {
            let text = format_shift(semitones);
            assert_eq!(parse_shift(&text), Some(semitones), "{text}");
            assert_eq!(
                parse_shift(&format!("{text} st")),
                Some(semitones),
                "{text}"
            );
        }
        assert_eq!(format_shift(4), "+4");
        assert_eq!(format_shift(0), "0");
        assert_eq!(format_shift(-12), "-12");
    }

    #[test]
    fn parse_accepts_the_documented_forms() {
        assert_eq!(parse_shift("+4"), Some(4));
        assert_eq!(parse_shift("4"), Some(4));
        assert_eq!(parse_shift("-5"), Some(-5));
        assert_eq!(parse_shift(" 7 st"), Some(7));
        assert_eq!(parse_shift("7st"), Some(7));
    }

    #[test]
    fn parse_rejects_out_of_range_and_non_integers() {
        for text in ["13", "-13", "2.5", "4.0", "", "st", "four", "+", "1e1"] {
            assert_eq!(parse_shift(text), None, "{text:?}");
        }
    }
}
