//! Plain DSP for corral. Nothing here depends on nice-plug, so it can be
//! unit-tested without a host.

/// The bottom of the Gain range. The floor is treated as silence ("-inf").
pub const MIN_GAIN_DB: f32 = -60.0;

/// Linear gain at [`MIN_GAIN_DB`]: 10^(−60/20).
const FLOOR_GAIN: f32 = 0.001;

/// Gains at or below this are the floor and output silence. The smoother
/// lands exactly on its target, so the tolerance only absorbs float error.
pub const SILENCE_THRESHOLD: f32 = FLOOR_GAIN * 1.0001;

/// Applies a linear gain to one sample. At or below the floor the result is
/// exactly 0.0, even for non-finite input.
#[inline]
pub fn apply_gain(sample: f32, gain: f32) -> f32 {
    if gain <= SILENCE_THRESHOLD {
        0.0
    } else {
        sample * gain
    }
}

/// Formats a linear gain as dB to two decimal places, without the unit.
/// The floor shows "-inf". Matches nice-plug's `v2s_f32_gain_to_db(2)`,
/// including never showing "-0.00".
pub fn format_gain(gain: f32) -> String {
    if gain <= SILENCE_THRESHOLD {
        return String::from("-inf");
    }

    let db = gain.log10() * 20.0;
    if (db * 100.0).round() / 100.0 == 0.0 {
        format!("{:.2}", 0.0)
    } else {
        format!("{db:.2}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The same formula as nice-plug's `util::db_to_gain`, which builds the
    /// parameter's range.
    fn db_to_gain(db: f32) -> f32 {
        10.0f32.powf(db * 0.05)
    }

    fn assert_close(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() <= expected.abs() * 1e-5,
            "{actual} is not close to {expected}"
        );
    }

    #[test]
    fn threshold_sits_just_above_the_floor() {
        let floor = db_to_gain(MIN_GAIN_DB);
        assert!(floor <= SILENCE_THRESHOLD);
        assert!(SILENCE_THRESHOLD < floor * 1.001);
    }

    #[test]
    fn unity_gain_leaves_sample_unchanged() {
        assert_eq!(apply_gain(0.5, db_to_gain(0.0)), 0.5);
    }

    #[test]
    fn gain_scales_by_ten_to_the_db_over_twenty() {
        for (db, factor) in [(12.0, 3.981_072), (-6.0, 0.501_187_2), (-12.0, 0.251_188_6)] {
            assert_close(apply_gain(0.5, db_to_gain(db)), 0.5 * factor);
        }
    }

    #[test]
    fn floor_is_silent() {
        assert_eq!(apply_gain(1.0, db_to_gain(MIN_GAIN_DB)), 0.0);
    }

    #[test]
    fn below_floor_is_silent() {
        for gain in [db_to_gain(-80.0), 0.0] {
            assert_eq!(apply_gain(1.0, gain), 0.0);
        }
    }

    #[test]
    fn floor_is_silent_for_non_finite_input() {
        let floor = db_to_gain(MIN_GAIN_DB);
        assert_eq!(apply_gain(f32::NAN, floor), 0.0);
        assert_eq!(apply_gain(f32::INFINITY, floor), 0.0);
    }

    #[test]
    fn just_above_floor_is_not_silent() {
        let gain = db_to_gain(-59.9);
        assert_close(apply_gain(1.0, gain), gain);
    }

    #[test]
    fn floor_and_below_display_as_minus_inf() {
        assert_eq!(format_gain(db_to_gain(MIN_GAIN_DB)), "-inf");
        assert_eq!(format_gain(0.0), "-inf");
    }

    #[test]
    fn displays_db_to_two_decimal_places() {
        assert_eq!(format_gain(db_to_gain(0.0)), "0.00");
        assert_eq!(format_gain(db_to_gain(12.0)), "12.00");
        assert_eq!(format_gain(db_to_gain(-12.0)), "-12.00");
        assert_eq!(format_gain(db_to_gain(-59.9)), "-59.90");
    }

    #[test]
    fn just_below_unity_displays_as_zero_not_negative_zero() {
        assert_eq!(format_gain(db_to_gain(-0.001)), "0.00");
    }
}
