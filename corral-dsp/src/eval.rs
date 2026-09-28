//! Pitch-tracking metrics after mir_eval.melody, simplified: each reference
//! frame is compared with the nearest estimate in time (no resampling).

/// A reference frame. `f0_hz <= 0` means unvoiced.
#[derive(Clone, Copy, Debug)]
pub struct RefFrame {
    pub time_s: f64,
    pub f0_hz: f32,
}

/// A detector output. `f0_hz` may be > 0 even when `voiced` is false (the
/// raw estimate); raw pitch/chroma accuracy use it regardless, as mir_eval does.
#[derive(Clone, Copy, Debug)]
pub struct EstFrame {
    pub time_s: f64,
    pub f0_hz: f32,
    pub voiced: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct Scores {
    pub raw_pitch_accuracy: Option<f64>,
    pub raw_chroma_accuracy: Option<f64>,
    pub voicing_recall: Option<f64>,
    pub voicing_false_alarm: Option<f64>,
    pub voiced_frames: usize,
    pub unvoiced_frames: usize,
}

pub const TOLERANCE_CENTS: f64 = 50.0;

/// Stands in for a reference frame with no estimate within `max_gap_s`.
const MISSING: EstFrame = EstFrame {
    time_s: f64::NAN,
    f0_hz: 0.0,
    voiced: false,
};

/// `estimates` must be sorted by time. A reference frame more than
/// `max_gap_s` from every estimate counts as an unvoiced estimate with no pitch.
pub fn score(reference: &[RefFrame], estimates: &[EstFrame], max_gap_s: f64) -> Scores {
    let mut voiced_frames = 0;
    let mut unvoiced_frames = 0;
    let mut pitch_hits = 0;
    let mut chroma_hits = 0;
    let mut voicing_hits = 0;
    let mut false_alarms = 0;

    for frame in reference {
        let estimate = nearest(estimates, frame.time_s, max_gap_s).unwrap_or(MISSING);
        if frame.f0_hz > 0.0 {
            voiced_frames += 1;
            if estimate.voiced {
                voicing_hits += 1;
            }
            if estimate.f0_hz > 0.0 {
                let error = 1200.0 * (f64::from(estimate.f0_hz) / f64::from(frame.f0_hz)).log2();
                if error.abs() <= TOLERANCE_CENTS {
                    pitch_hits += 1;
                }
                if (error - 1200.0 * (error / 1200.0).round()).abs() <= TOLERANCE_CENTS {
                    chroma_hits += 1;
                }
            }
        } else {
            unvoiced_frames += 1;
            if estimate.voiced {
                false_alarms += 1;
            }
        }
    }

    let ratio = |hits: usize, total: usize| (total > 0).then(|| hits as f64 / total as f64);
    Scores {
        raw_pitch_accuracy: ratio(pitch_hits, voiced_frames),
        raw_chroma_accuracy: ratio(chroma_hits, voiced_frames),
        voicing_recall: ratio(voicing_hits, voiced_frames),
        voicing_false_alarm: ratio(false_alarms, unvoiced_frames),
        voiced_frames,
        unvoiced_frames,
    }
}

/// How closely a shifted output tracks the reference pitch times a ratio.
#[derive(Clone, Copy, Debug)]
pub struct CentsStats {
    /// Median signed error over voiced reference frames with an estimate (f0 > 0).
    pub median_cents: Option<f64>,
    /// Share of *all* voiced reference frames within `tolerance_cents`
    /// (a missing or zero estimate is a miss).
    pub share_within: Option<f64>,
    pub voiced_frames: usize,
}

/// Compares estimates with reference × `ratio`. Estimate times are taken
/// as-is: callers subtract the chain latency first. `estimates` must be
/// sorted by time.
pub fn cents_stats(
    reference: &[RefFrame],
    estimates: &[EstFrame],
    max_gap_s: f64,
    ratio: f64,
    tolerance_cents: f64,
) -> CentsStats {
    let mut errors = Vec::new();
    let mut voiced_frames = 0;
    let mut hits = 0;
    for frame in reference.iter().filter(|f| f.f0_hz > 0.0) {
        voiced_frames += 1;
        let estimate = nearest(estimates, frame.time_s, max_gap_s).unwrap_or(MISSING);
        if estimate.f0_hz > 0.0 {
            let expected = f64::from(frame.f0_hz) * ratio;
            let error = 1200.0 * (f64::from(estimate.f0_hz) / expected).log2();
            if error.abs() <= tolerance_cents {
                hits += 1;
            }
            errors.push(error);
        }
    }
    errors.sort_by(f64::total_cmp);
    let median_cents = match errors.len() {
        0 => None,
        len if len % 2 == 1 => Some(errors[len / 2]),
        len => Some((errors[len / 2 - 1] + errors[len / 2]) / 2.0),
    };
    CentsStats {
        median_cents,
        share_within: (voiced_frames > 0).then(|| hits as f64 / voiced_frames as f64),
        voiced_frames,
    }
}

/// The estimate nearest `time_s`, if one is within `max_gap_s`.
fn nearest(estimates: &[EstFrame], time_s: f64, max_gap_s: f64) -> Option<EstFrame> {
    let after = estimates.partition_point(|e| e.time_s < time_s);
    let before = after.checked_sub(1).map(|i| estimates[i]);
    let after = estimates.get(after).copied();
    [before, after]
        .into_iter()
        .flatten()
        .filter(|e| (e.time_s - time_s).abs() <= max_gap_s)
        .min_by(|a, b| {
            (a.time_s - time_s)
                .abs()
                .total_cmp(&(b.time_s - time_s).abs())
        })
}

/// Parses "time,f0" CSV text. Lines that don't parse as two numbers (headers,
/// blanks) are skipped.
pub fn parse_reference_csv(text: &str) -> Vec<RefFrame> {
    text.lines()
        .filter_map(|line| {
            let (time, f0) = line.split_once(',')?;
            Some(RefFrame {
                time_s: time.trim().parse().ok()?,
                f0_hz: f0.trim().parse().ok()?,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOP: f64 = 0.01;

    /// Reference frames every `HOP` seconds with the given f0s.
    fn reference(f0s: &[f32]) -> Vec<RefFrame> {
        f0s.iter()
            .enumerate()
            .map(|(i, &f0_hz)| RefFrame {
                time_s: i as f64 * HOP,
                f0_hz,
            })
            .collect()
    }

    /// One estimate per reference frame, at the same time.
    fn estimates(
        reference: &[RefFrame],
        f0_hz: impl Fn(f32) -> f32,
        voiced: impl Fn(f32) -> bool,
    ) -> Vec<EstFrame> {
        reference
            .iter()
            .map(|r| EstFrame {
                time_s: r.time_s,
                f0_hz: f0_hz(r.f0_hz),
                voiced: voiced(r.f0_hz),
            })
            .collect()
    }

    fn cents(f0: f32, cents: f64) -> f32 {
        (f64::from(f0) * 2f64.powf(cents / 1200.0)) as f32
    }

    const MIXED: [f32; 6] = [220.0, 220.0, 0.0, 330.0, 0.0, 440.0];

    #[test]
    fn perfect_estimate_scores_one() {
        let r = reference(&MIXED);
        let s = score(&r, &estimates(&r, |f| f, |f| f > 0.0), HOP);
        assert_eq!(s.raw_pitch_accuracy, Some(1.0));
        assert_eq!(s.raw_chroma_accuracy, Some(1.0));
        assert_eq!(s.voicing_recall, Some(1.0));
        assert_eq!(s.voicing_false_alarm, Some(0.0));
        assert_eq!((s.voiced_frames, s.unvoiced_frames), (4, 2));
    }

    #[test]
    fn octave_errors_count_for_chroma_only() {
        let r = reference(&[220.0, 330.0, 440.0]);
        for factor in [2.0, 0.5] {
            let s = score(&r, &estimates(&r, |f| f * factor, |_| true), HOP);
            assert_eq!(s.raw_pitch_accuracy, Some(0.0));
            assert_eq!(s.raw_chroma_accuracy, Some(1.0));
        }
    }

    #[test]
    fn tolerance_is_fifty_cents() {
        let r = reference(&[220.0, 330.0]);
        for (offset, expected) in [(49.0, 1.0), (-49.0, 1.0), (51.0, 0.0), (-51.0, 0.0)] {
            let s = score(&r, &estimates(&r, |f| cents(f, offset), |_| true), HOP);
            assert_eq!(s.raw_pitch_accuracy, Some(expected), "{offset} cents");
            assert_eq!(s.raw_chroma_accuracy, Some(expected), "{offset} cents");
        }
    }

    #[test]
    fn unvoiced_estimate_with_the_right_pitch_counts_for_pitch_not_voicing() {
        let r = reference(&[220.0, 330.0]);
        let s = score(&r, &estimates(&r, |f| f, |_| false), HOP);
        assert_eq!(s.raw_pitch_accuracy, Some(1.0));
        assert_eq!(s.voicing_recall, Some(0.0));
    }

    #[test]
    fn voiced_estimates_on_silence_are_false_alarms() {
        let r = reference(&MIXED);
        let s = score(&r, &estimates(&r, |_| 220.0, |_| true), HOP);
        assert_eq!(s.voicing_false_alarm, Some(1.0));
    }

    #[test]
    fn empty_denominators_give_none() {
        let silent = reference(&[0.0, 0.0]);
        let s = score(&silent, &estimates(&silent, |f| f, |_| false), HOP);
        assert_eq!(s.raw_pitch_accuracy, None);
        assert_eq!(s.raw_chroma_accuracy, None);
        assert_eq!(s.voicing_recall, None);
        assert_eq!(s.voicing_false_alarm, Some(0.0));

        let voiced = reference(&[220.0]);
        let s = score(&voiced, &estimates(&voiced, |f| f, |_| true), HOP);
        assert_eq!(s.voicing_false_alarm, None);

        let s = score(&[], &[], HOP);
        assert_eq!((s.voiced_frames, s.unvoiced_frames), (0, 0));
        assert_eq!(s.raw_pitch_accuracy, None);
    }

    #[test]
    fn uses_the_nearest_estimate_in_time() {
        let r = [RefFrame {
            time_s: 1.0,
            f0_hz: 220.0,
        }];
        let e = [
            EstFrame {
                time_s: 0.996,
                f0_hz: 440.0,
                voiced: true,
            },
            EstFrame {
                time_s: 1.003,
                f0_hz: 220.0,
                voiced: true,
            },
        ];
        assert_eq!(score(&r, &e, HOP).raw_pitch_accuracy, Some(1.0));
    }

    #[test]
    fn reference_frames_beyond_the_gap_are_misses() {
        let r = reference(&[220.0, 220.0, 0.0]);
        let e = [EstFrame {
            time_s: 0.0,
            f0_hz: 220.0,
            voiced: true,
        }];
        // Only the first reference frame (t = 0) is within half a hop.
        let s = score(&r, &e, HOP / 2.0);
        assert_eq!(s.raw_pitch_accuracy, Some(0.5));
        assert_eq!(s.voicing_recall, Some(0.5));
        assert_eq!(s.voicing_false_alarm, Some(0.0));
    }

    #[test]
    fn cents_stats_of_a_perfect_shifted_estimate() {
        let r = reference(&MIXED);
        let ratio = 2f64.powf(4.0 / 12.0);
        let e = estimates(&r, |f| (f64::from(f) * ratio) as f32, |f| f > 0.0);
        let s = cents_stats(&r, &e, HOP, ratio, 25.0);
        assert!(s.median_cents.unwrap().abs() < 1e-3);
        assert_eq!(s.share_within, Some(1.0));
        assert_eq!(s.voiced_frames, 4);
    }

    #[test]
    fn cents_stats_of_a_constant_offset() {
        let r = reference(&[220.0, 330.0, 440.0]);
        let s = cents_stats(
            &r,
            &estimates(&r, |f| cents(f, 20.0), |_| true),
            HOP,
            1.0,
            10.0,
        );
        assert!((s.median_cents.unwrap() - 20.0).abs() < 1e-3);
        assert_eq!(s.share_within, Some(0.0));
    }

    #[test]
    fn cents_stats_counts_missing_estimates_as_misses_but_not_in_the_median() {
        let r = reference(&[220.0, 220.0, 220.0, 220.0]);
        // Only the first two frames have an estimate within the gap.
        let e = [
            EstFrame {
                time_s: 0.0,
                f0_hz: cents(220.0, 5.0),
                voiced: true,
            },
            EstFrame {
                time_s: HOP,
                f0_hz: cents(220.0, 5.0),
                voiced: true,
            },
        ];
        let s = cents_stats(&r, &e, HOP / 2.0, 1.0, 25.0);
        assert!((s.median_cents.unwrap() - 5.0).abs() < 1e-3);
        assert_eq!(s.share_within, Some(0.5));

        // A zero estimate is a miss too.
        let e = estimates(&r, |_| 0.0, |_| false);
        let s = cents_stats(&r, &e, HOP, 1.0, 25.0);
        assert_eq!(s.median_cents, None);
        assert_eq!(s.share_within, Some(0.0));
    }

    #[test]
    fn cents_stats_ignores_unvoiced_reference_frames() {
        let r = reference(&MIXED);
        // Wildly wrong wherever the reference is unvoiced.
        let e = estimates(&r, |f| if f > 0.0 { f } else { 1000.0 }, |_| true);
        let s = cents_stats(&r, &e, HOP, 1.0, 25.0);
        assert_eq!(s.share_within, Some(1.0));
        assert_eq!(s.voiced_frames, 4);
    }

    #[test]
    fn cents_stats_of_nothing_is_none() {
        let s = cents_stats(&[], &[], HOP, 1.0, 25.0);
        assert_eq!(s.median_cents, None);
        assert_eq!(s.share_within, None);
        assert_eq!(s.voiced_frames, 0);
    }

    #[test]
    fn parse_reference_csv_skips_headers_and_blank_lines() {
        let frames = parse_reference_csv("time,Hz\n0.0,220.5\n\n0.01, 0\r\n0.02,330\n");
        let parsed: Vec<_> = frames.iter().map(|f| (f.time_s, f.f0_hz)).collect();
        assert_eq!(parsed, [(0.0, 220.5), (0.01, 0.0), (0.02, 330.0)]);
    }
}
