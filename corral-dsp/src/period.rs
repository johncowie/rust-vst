//! Turns per-hop YIN estimates into the period the shifter uses: smoothed,
//! with octave jumps rejected unless they persist.

use crate::pitch::Detection;

/// A jump within this many cents of ±1200 is an octave candidate.
const OCTAVE_WINDOW_CENTS: f32 = 150.0;
/// An octave jump is accepted once this many consecutive hops agree.
const OCTAVE_PERSIST_HOPS: u32 = 3;
/// Consecutive octave candidates agree when they are within this of each other.
const CANDIDATE_AGREE_CENTS: f32 = 100.0;
/// Any other jump bigger than this is a note change and is taken at once.
const NOTE_CHANGE_CENTS: f32 = 100.0;
/// One-pole coefficient per hop, in the log-period domain, for smaller moves.
const SMOOTHING: f32 = 0.5;

pub struct PeriodSmoother {
    sample_rate: f32,
    /// The smoothed period in samples.
    period: Option<f32>,
    /// The latest octave candidate's period, and how many consecutive hops
    /// have agreed with it.
    candidate: Option<f32>,
    candidate_hops: u32,
}

fn cents(to: f32, from: f32) -> f32 {
    1200.0 * (to / from).log2()
}

impl PeriodSmoother {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            sample_rate,
            period: None,
            candidate: None,
            candidate_hops: 0,
        }
    }

    pub fn reset(&mut self) {
        self.period = None;
        self.clear_candidate();
    }

    fn clear_candidate(&mut self) {
        self.candidate = None;
        self.candidate_hops = 0;
    }

    /// Feeds one hop. Unvoiced hops leave the period unchanged, and break
    /// any run of octave candidates.
    pub fn update(&mut self, detection: &Detection) {
        if !(detection.voiced && detection.f0_hz.is_finite() && detection.f0_hz > 0.0) {
            self.clear_candidate();
            return;
        }
        let p = self.sample_rate / detection.f0_hz;
        let Some(s) = self.period else {
            self.period = Some(p);
            return;
        };
        let jump = cents(p, s);
        if (jump.abs() - 1200.0).abs() <= OCTAVE_WINDOW_CENTS {
            self.candidate_hops = match self.candidate {
                Some(q) if cents(p, q).abs() <= CANDIDATE_AGREE_CENTS => self.candidate_hops + 1,
                _ => 1,
            };
            self.candidate = Some(p);
            if self.candidate_hops >= OCTAVE_PERSIST_HOPS {
                self.period = Some(p);
                self.clear_candidate();
            }
            return;
        }
        self.clear_candidate();
        self.period = Some(if jump.abs() > NOTE_CHANGE_CENTS {
            p
        } else {
            (s.ln() + SMOOTHING * (p.ln() - s.ln())).exp()
        });
    }

    /// The period in samples, or `None` before the first voiced hop.
    pub fn period(&self) -> Option<f32> {
        self.period
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f32 = 48_000.0;

    fn voiced(f0_hz: f32) -> Detection {
        Detection {
            f0_hz,
            confidence: 0.95,
            voiced: true,
            window_centre: 0,
        }
    }

    fn unvoiced() -> Detection {
        Detection {
            f0_hz: 0.0,
            confidence: 0.0,
            voiced: false,
            window_centre: 0,
        }
    }

    /// The smoothed pitch in Hz.
    fn f0(smoother: &PeriodSmoother) -> f32 {
        SR / smoother.period().unwrap()
    }

    fn feed(smoother: &mut PeriodSmoother, f0_hz: f32, hops: usize) {
        for _ in 0..hops {
            smoother.update(&voiced(f0_hz));
        }
    }

    #[test]
    fn has_no_period_before_the_first_voiced_hop() {
        let mut smoother = PeriodSmoother::new(SR);
        assert_eq!(smoother.period(), None);
        smoother.update(&unvoiced());
        assert_eq!(smoother.period(), None);
        smoother.update(&voiced(220.0));
        assert_eq!(smoother.period(), Some(SR / 220.0));
    }

    #[test]
    fn a_steady_pitch_converges_within_a_tenth_of_a_cent() {
        let mut smoother = PeriodSmoother::new(SR);
        // Start 40 cents sharp, below the note-change threshold, so the
        // smoother has to glide rather than snap.
        feed(&mut smoother, 220.0 * 2f32.powf(40.0 / 1200.0), 1);
        feed(&mut smoother, 220.0, 12);
        let error = cents(f0(&smoother), 220.0);
        assert!(error.abs() < 0.1, "{error} cents");
    }

    #[test]
    fn a_two_semitone_step_is_taken_on_the_first_hop() {
        let mut smoother = PeriodSmoother::new(SR);
        feed(&mut smoother, 220.0, 5);
        let step = 220.0 * 2f32.powf(2.0 / 12.0);
        feed(&mut smoother, step, 1);
        assert_eq!(smoother.period(), Some(SR / step));
    }

    #[test]
    fn one_or_two_hop_octave_glitches_are_ignored() {
        for factor in [2.0, 0.5] {
            for glitch_hops in [1, 2] {
                let mut smoother = PeriodSmoother::new(SR);
                feed(&mut smoother, 220.0, 5);
                feed(&mut smoother, 220.0 * factor, glitch_hops);
                assert_eq!(f0(&smoother), 220.0, "×{factor}, {glitch_hops} hops");
                feed(&mut smoother, 220.0, 1);
                // The run was broken, so another two glitch hops are ignored too.
                feed(&mut smoother, 220.0 * factor, 2);
                assert_eq!(f0(&smoother), 220.0, "×{factor}, {glitch_hops} hops");
            }
        }
    }

    #[test]
    fn three_hops_of_a_new_octave_are_accepted() {
        for factor in [2.0, 0.5] {
            let mut smoother = PeriodSmoother::new(SR);
            feed(&mut smoother, 220.0, 5);
            feed(&mut smoother, 220.0 * factor, 3);
            assert_eq!(f0(&smoother), 220.0 * factor, "×{factor}");
        }
    }

    #[test]
    fn an_unvoiced_hop_breaks_a_run_of_octave_candidates() {
        let mut smoother = PeriodSmoother::new(SR);
        feed(&mut smoother, 220.0, 5);
        feed(&mut smoother, 440.0, 2);
        smoother.update(&unvoiced());
        feed(&mut smoother, 440.0, 1);
        assert_eq!(f0(&smoother), 220.0);
    }

    #[test]
    fn unvoiced_hops_hold_the_period() {
        let mut smoother = PeriodSmoother::new(SR);
        feed(&mut smoother, 220.0, 5);
        let held = smoother.period();
        for _ in 0..10 {
            smoother.update(&unvoiced());
        }
        // A voiced flag with a nonsense estimate is ignored too.
        smoother.update(&voiced(f32::NAN));
        smoother.update(&voiced(0.0));
        assert_eq!(smoother.period(), held);
    }

    #[test]
    fn reset_clears_the_period() {
        let mut smoother = PeriodSmoother::new(SR);
        feed(&mut smoother, 220.0, 5);
        feed(&mut smoother, 440.0, 2);
        smoother.reset();
        assert_eq!(smoother.period(), None);
        // The candidate run is gone too: 440 is simply the first period.
        feed(&mut smoother, 440.0, 1);
        assert_eq!(f0(&smoother), 440.0);
    }
}
