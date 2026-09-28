//! TD-PSOLA pitch shifting. See docs/td-psola.md.

use crate::pitch::MIN_F0_HZ;

/// Latency in periods of the lowest detectable pitch. A 2P grain centred on
/// a mark needs P samples after the mark, and the mark needs up to 1.25P of
/// lookahead to be confirmed; see the plan for 0005.
const LATENCY_PERIODS: f32 = 2.0;

/// The chain's fixed latency. Depends only on the sample rate.
pub fn latency_samples(sample_rate: f32) -> usize {
    (LATENCY_PERIODS * sample_rate / MIN_F0_HZ).ceil() as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latency_is_two_periods_of_the_lowest_pitch() {
        assert_eq!(latency_samples(44_100.0), 1103);
        assert_eq!(latency_samples(48_000.0), 1200);
        assert_eq!(latency_samples(96_000.0), 2400);
    }
}
