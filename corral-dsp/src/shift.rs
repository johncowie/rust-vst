//! Semitones → ratio, and the glide between harmony intervals.

pub const DEFAULT_GLIDE_S: f32 = 0.050;
pub const MAX_GLIDE_S: f32 = 0.250;
/// Closer than this (0.01¢) and the glide lands exactly on its target.
const SNAP_SEMITONES: f32 = 1e-4;

// The Shift parameter's range and display. Only the plugin's Shift still
// uses them; they go when Key/Scale/Glide replace it (plan 0006, Phase 6).
pub const MIN: i32 = -12;
pub const MAX: i32 = 12;
pub const DEFAULT: i32 = 4;

/// 2^(semitones/12).
pub fn ratio(semitones: f32) -> f32 {
    (semitones / 12.0).exp2()
}

/// Per-sample one-pole coefficient for a glide time constant (seconds).
/// 0.0 (an instant jump) for `glide_s <= 0`.
pub fn glide_coefficient(glide_s: f32, sample_rate: f32) -> f32 {
    if glide_s > 0.0 {
        (-1.0 / (glide_s * sample_rate)).exp()
    } else {
        0.0
    }
}

/// Exponential glide in semitones (log frequency). `Glide` is its time
/// constant: 63% of any change after `glide_s`, 95% after 3 × `glide_s`.
pub struct Glide {
    target: f32,
    /// The value minus `target`. Decaying the offset itself, rather than
    /// computing `target + c · (current − target)`, keeps its relative
    /// precision: near 12 semitones an f32 can't move by the last steps of
    /// a slow glide (250 ms at 96 kHz stalls about 1¢ short), but the
    /// offset can always shrink.
    offset: f32,
}

impl Glide {
    pub fn new(initial: f32) -> Self {
        Self {
            target: initial,
            offset: 0.0,
        }
    }

    /// Lands on `target` at once (voicing onset, reset).
    pub fn jump(&mut self, target: f32) {
        self.target = target;
        self.offset = 0.0;
    }

    pub fn current(&self) -> f32 {
        self.target + self.offset
    }

    /// Advances one sample toward `target` and returns the value. A new
    /// target mid-glide continues from where it is.
    #[inline]
    pub fn next(&mut self, target: f32, coefficient: f32) -> f32 {
        if target != self.target {
            self.offset += self.target - target;
            self.target = target;
        }
        self.offset *= coefficient;
        if self.offset.abs() < SNAP_SEMITONES {
            self.offset = 0.0;
        }
        self.current()
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

    const SAMPLE_RATES: [f32; 3] = [44_100.0, 48_000.0, 96_000.0];

    /// Runs the glide toward `target` until it lands exactly; returns every
    /// value along the way.
    fn run_to(glide: &mut Glide, target: f32, coefficient: f32) -> Vec<f32> {
        let mut values = Vec::new();
        for _ in 0..1_000_000 {
            let value = glide.next(target, coefficient);
            values.push(value);
            if value == target {
                return values;
            }
        }
        panic!("the glide never landed on {target}");
    }

    #[test]
    fn after_one_time_constant_a_change_is_63_percent_done() {
        for sample_rate in SAMPLE_RATES {
            let coefficient = glide_coefficient(DEFAULT_GLIDE_S, sample_rate);
            let mut glide = Glide::new(4.0);
            let n = (DEFAULT_GLIDE_S * sample_rate).round() as usize;
            let mut value = 4.0;
            for _ in 0..n {
                value = glide.next(3.0, coefficient);
            }
            let done = 4.0 - value;
            assert!((done - 0.632).abs() <= 0.005, "{sample_rate}: {done}");
        }
    }

    #[test]
    fn the_glide_is_monotonic_and_lands_exactly_on_the_target() {
        for sample_rate in SAMPLE_RATES {
            for glide_s in [0.001, DEFAULT_GLIDE_S, MAX_GLIDE_S] {
                let coefficient = glide_coefficient(glide_s, sample_rate);
                for (from, to) in [
                    (0.0, 1.0),
                    (4.0, -12.0),
                    (-12.0, 12.0),
                    (12.0, 11.0),
                    (3.0, 4.0),
                ] {
                    let mut glide = Glide::new(from);
                    let values = run_to(&mut glide, to, coefficient);
                    let rising = to > from;
                    for pair in values.windows(2) {
                        let ordered = if rising {
                            pair[1] >= pair[0]
                        } else {
                            pair[1] <= pair[0]
                        };
                        assert!(ordered, "{from} → {to} at {sample_rate}: {pair:?}");
                    }
                    let overshoot = values.iter().any(|&v| if rising { v > to } else { v < to });
                    assert!(!overshoot, "{from} → {to} at {sample_rate}");
                    // And it stays there.
                    assert_eq!(glide.next(to, coefficient), to);
                }
            }
        }
    }

    #[test]
    fn the_largest_step_is_the_first() {
        for sample_rate in SAMPLE_RATES {
            let coefficient = glide_coefficient(DEFAULT_GLIDE_S, sample_rate);
            let mut glide = Glide::new(4.0);
            let mut previous = 4.0;
            let values = run_to(&mut glide, 3.0, coefficient);
            let largest = values.iter().fold(0.0f32, |m, &v| {
                let step = (previous - v).abs();
                previous = v;
                m.max(step)
            });
            let expected = 1.0 - coefficient;
            assert!(
                (largest - expected).abs() <= 1e-6,
                "{largest} vs {expected}"
            );
        }
    }

    #[test]
    fn a_zero_coefficient_jumps_in_one_sample() {
        assert_eq!(glide_coefficient(0.0, 48_000.0), 0.0);
        assert_eq!(glide_coefficient(-1.0, 48_000.0), 0.0);
        let mut glide = Glide::new(4.0);
        assert_eq!(glide.next(-12.0, 0.0), -12.0);
    }

    #[test]
    fn jump_lands_at_once() {
        let coefficient = glide_coefficient(MAX_GLIDE_S, 48_000.0);
        let mut glide = Glide::new(0.0);
        glide.next(12.0, coefficient);
        glide.jump(3.0);
        assert_eq!(glide.current(), 3.0);
        assert_eq!(glide.next(3.0, coefficient), 3.0);
    }

    #[test]
    fn a_new_target_mid_glide_continues_from_where_it_is() {
        let coefficient = glide_coefficient(DEFAULT_GLIDE_S, 48_000.0);
        let mut glide = Glide::new(0.0);
        for _ in 0..1000 {
            glide.next(4.0, coefficient);
        }
        let before = glide.current();
        let after = glide.next(-3.0, coefficient);
        let expected = (1.0 - coefficient) * (-3.0 - before);
        assert!(
            ((after - before) - expected).abs() <= 1e-5,
            "{before} → {after}"
        );
        run_to(&mut glide, -3.0, coefficient);
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
